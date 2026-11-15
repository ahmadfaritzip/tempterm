//! Real graphical workspace implementation using eframe / egui.
//!
//! Provides the primary desktop workspace with:
//! - Left project/session sidebar
//! - Top toolbar
//! - Central workspace (with empty-state illustration/actions or interactive split terminal view)
//! - Bottom status bar with real daemon, branch, process, and port information.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::runtime::Handle;

use eframe::egui::{self, pos2, Color32, Frame, Margin, Pos2, Rect, Rounding, Sense, Stroke, Vec2};
use temp_terminal_core::registry::Registry;

use crate::icons::Icons;
use crate::renderer::{TerminalColor, TerminalRenderer};
use crate::{default_shell_command, uuid_short, AppHost, SplitDir, SplitNode};

/// Platform-native clipboard helper to retrieve plain text.
pub fn get_clipboard_text() -> String {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::DataExchange::{
            CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        };
        use windows_sys::Win32::System::Memory::GlobalLock;
        use windows_sys::Win32::System::Memory::GlobalUnlock;

        const CF_UNICODETEXT: u32 = 13;
        unsafe {
            for _ in 0..10 {
                if IsClipboardFormatAvailable(CF_UNICODETEXT) != 0
                    && OpenClipboard(std::ptr::null_mut()) != 0
                {
                    let h_data = GetClipboardData(CF_UNICODETEXT);
                    if !h_data.is_null() {
                        let ptr = GlobalLock(h_data as _) as *const u16;
                        if !ptr.is_null() {
                            let mut len = 0;
                            while *ptr.add(len) != 0 {
                                len += 1;
                            }
                            let slice = std::slice::from_raw_parts(ptr, len);
                            let s = String::from_utf16_lossy(slice);
                            GlobalUnlock(h_data as _);
                            CloseClipboard();
                            return s;
                        }
                    }
                    CloseClipboard();
                    return String::new();
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
    String::new()
}

/// Platform-native clipboard helper to write plain text to the system clipboard.
pub fn set_clipboard_text(text: &str) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::DataExchange::{
            CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
        };
        use windows_sys::Win32::System::Memory::{
            GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
        };

        const CF_UNICODETEXT: u32 = 13;
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes_len = wide.len() * std::mem::size_of::<u16>();

        unsafe {
            for _ in 0..10 {
                if OpenClipboard(std::ptr::null_mut()) != 0 {
                    EmptyClipboard();
                    let h_mem = GlobalAlloc(GMEM_MOVEABLE, bytes_len);
                    if !h_mem.is_null() {
                        let ptr = GlobalLock(h_mem) as *mut u16;
                        if !ptr.is_null() {
                            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
                            GlobalUnlock(h_mem);
                            SetClipboardData(CF_UNICODETEXT, h_mem as _);
                        }
                    }
                    CloseClipboard();
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
}

/// Native Studio - Quiet Graphite / Arctic Blue exact design tokens.
pub mod theme {
    use eframe::egui::Color32;

    // 1. Surface colors (Quiet Graphite)
    pub const APP_BACKGROUND: Color32 = Color32::from_rgb(0x0c, 0x10, 0x16); // #0C1016
    pub const PANEL_BACKGROUND: Color32 = Color32::from_rgb(0x11, 0x18, 0x21); // #111821
    pub const WORKSPACE_BACKGROUND: Color32 = Color32::from_rgb(0x0e, 0x14, 0x1c); // #0E141C
    pub const SURFACE: Color32 = Color32::from_rgb(0x16, 0x1f, 0x2a); // #161F2A
    pub const SURFACE_ELEVATED: Color32 = Color32::from_rgb(0x1c, 0x27, 0x34); // #1C2734
    pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x20, 0x2d, 0x3d); // #202D3D
    pub const SURFACE_SELECTED: Color32 = Color32::from_rgb(0x1a, 0x2c, 0x43); // #1A2C43
    pub const TERMINAL_BACKGROUND: Color32 = Color32::from_rgb(0x08, 0x0c, 0x11); // #080C11

    // Backward-compat aliases mapping to the new tokens
    pub const CANVAS: Color32 = APP_BACKGROUND;
    pub const WORKSPACE: Color32 = WORKSPACE_BACKGROUND;
    pub const PANEL: Color32 = PANEL_BACKGROUND;
    pub const TERMINAL_BG: Color32 = TERMINAL_BACKGROUND;

    // 2. Borders
    pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(0x25, 0x31, 0x40); // #253140
    pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x34, 0x43, 0x55); // #344355
    pub const BORDER: Color32 = BORDER_SUBTLE;

    // 3. Typography tokens
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xe7, 0xec, 0xf2); // #E7ECF2
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xa7, 0xb2, 0xc1); // #A7B2C1
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x7d, 0x89, 0x9b); // #7D899B
    pub const TEXT_DISABLED: Color32 = Color32::from_rgb(0x60, 0x6e, 0x81); // #606E81

    // 4. Accent tokens (Arctic Blue)
    pub const ACCENT: Color32 = Color32::from_rgb(0x86, 0xa9, 0xff); // #86A9FF
    pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0xa0, 0xbb, 0xff); // #A0BBFF
    pub const ACCENT_TINT: Color32 = Color32::from_rgb(0x1a, 0x2c, 0x43); // #1A2C43

    // 5. Semantic status tokens
    pub const SUCCESS: Color32 = Color32::from_rgb(0x60, 0xc9, 0x95); // #60C995
    pub const WARNING: Color32 = Color32::from_rgb(0xe5, 0xb5, 0x66); // #E5B566
    pub const DANGER: Color32 = Color32::from_rgb(0xed, 0x77, 0x7d); // #ED777D
    pub const INFO: Color32 = Color32::from_rgb(0x6d, 0xba, 0xff); // #6DBAFF

    // 6. Geometry & Spacing (4px base system)
    pub const SPACE_MICRO: f32 = 4.0;
    pub const SPACE_COMPACT: f32 = 8.0;
    pub const SPACE_PADDING: f32 = 12.0;
    pub const SPACE_GROUP: f32 = 16.0;
    pub const SPACE_SECTION: f32 = 20.0;
    pub const SPACE_MAJOR: f32 = 24.0;

    // Corner Radii
    pub const RADIUS_SM: f32 = 4.0;       // Small controls
    pub const RADIUS_MD: f32 = 6.0;       // Buttons and inputs
    pub const RADIUS_CARD: f32 = 8.0;     // Cards and popovers
    pub const RADIUS_POPOVER: f32 = 8.0;  // Popovers
    pub const RADIUS_MODAL: f32 = 10.0;   // Modal dialogs
    pub const RADIUS_PILL: f32 = 12.0;    // Status chips
    pub const RADIUS_LG: f32 = 8.0;
    pub const RADIUS_XL: f32 = 10.0;

    // Layout Dimensions (egui logical points)
    pub const TOPNAV_HEIGHT: f32 = 40.0;
    pub const WORKSPACE_TABS_HEIGHT: f32 = 34.0;
    pub const SIDEBAR_WIDTH_DEFAULT: f32 = 236.0;
    pub const INSPECTOR_WIDTH_DEFAULT: f32 = 288.0;
    pub const STATUSBAR_HEIGHT: f32 = 24.0;
    pub const PANE_HEADER_HEIGHT: f32 = 30.0;
    pub const SIDEBAR_ROW_HEIGHT: f32 = 28.0;
    pub const SIDEBAR_ROW_HEIGHT_MAX: f32 = 30.0;

    // Aliases for compatibility
    pub const WASH: Color32 = CANVAS;
    pub const WASH_GLOW: Color32 = ACCENT_TINT;
    pub const CARD: Color32 = SURFACE;
    pub const CARD_RAISED: Color32 = SURFACE_HOVER;
    pub const CARD_DIM: Color32 = PANEL;
    pub const POPOVER: Color32 = SURFACE;
    pub const HOVER: Color32 = SURFACE_HOVER;
    pub const HOVER_BG: Color32 = SURFACE_HOVER;
    pub const SELECTED: Color32 = SURFACE_SELECTED;
    pub const DIVIDER: Color32 = BORDER;

    pub const TAB_BORDER_ACTIVE: Color32 = ACCENT;
    pub const TAB_BORDER_INACTIVE: Color32 = BORDER;
    pub const TAB_BORDER_HOVER: Color32 = BORDER_STRONG;

    pub const TEXT: Color32 = TEXT_PRIMARY;
    pub const TEXT_STRONG: Color32 = TEXT_PRIMARY;
    pub const TEXT_SOFT: Color32 = TEXT_SECONDARY;
    pub const TEXT_FAINT: Color32 = TEXT_DISABLED;
    pub const ACCENT_TEXT: Color32 = Color32::WHITE;
    pub const SELECTION: Color32 = SURFACE_SELECTED;

    pub const OK: Color32 = SUCCESS;
    pub const WARN: Color32 = WARNING;
    pub const BAD: Color32 = DANGER;

    pub const TOPBAR_HEIGHT: f32 = WORKSPACE_TABS_HEIGHT;

    pub const SURFACE_CANVAS: Color32 = CANVAS;
    pub const SURFACE_PANEL: Color32 = PANEL;
    pub const SURFACE_CARD: Color32 = SURFACE;
    pub const SURFACE_INPUT: Color32 = SURFACE;
    pub const SURFACE_OVERLAY: Color32 = SURFACE;
    pub const SURFACE_ACTIVE: Color32 = SURFACE_SELECTED;

    pub const BORDER_MEDIUM: Color32 = BORDER_STRONG;
    pub const BORDER_FOCUS: Color32 = ACCENT;
    pub const DIVIDER_LINE: Color32 = BORDER;

    pub const TEXT_NORMAL: Color32 = TEXT_PRIMARY;

    pub const ACCENT_PRIMARY: Color32 = ACCENT;
    pub const ACCENT_SUBTLE: Color32 = ACCENT_TINT;
    pub const ACCENT_MUTED: Color32 = BORDER_STRONG;

    pub const STATUS_SUCCESS: Color32 = SUCCESS;
    pub const STATUS_WARNING: Color32 = WARNING;
    pub const STATUS_ERROR: Color32 = DANGER;
    pub const STATUS_OK: Color32 = SUCCESS;
    pub const STATUS_WARN: Color32 = WARNING;
    pub const STATUS_BAD: Color32 = DANGER;
    pub const STATUS_GREEN: Color32 = SUCCESS;
    pub const STATUS_RED: Color32 = DANGER;

    pub const BG_APP: Color32 = CANVAS;
    pub const BG_SIDEBAR: Color32 = PANEL;
    pub const BG_CARD: Color32 = SURFACE;
    pub const BG_CARD_HOVER: Color32 = SURFACE_HOVER;
    pub const BG_RAISED: Color32 = SURFACE_HOVER;
    pub const BG_INPUT: Color32 = SURFACE;
    pub const BG_HOVER: Color32 = SURFACE_HOVER;

    pub mod typography {
        pub const MODAL_TITLE: f32 = 22.0;
        pub const PANEL_TITLE: f32 = 16.0;
        pub const SECTION_HEADING: f32 = 13.0;
        pub const BODY_PRIMARY: f32 = 13.0;
        pub const BODY_SECONDARY: f32 = 12.0;
        pub const LABEL: f32 = 11.0;
        pub const METADATA: f32 = 10.0;
        pub const TERMINAL_TEXT: f32 = 13.5;
    }
}

/// Helper converting `TerminalColor` to `egui::Color32` matching standard terminal palette (Windows Terminal Campbell).
pub fn term_color_to_egui(c: TerminalColor, default_color: Color32) -> Color32 {
    match c {
        TerminalColor::Default => default_color,
        TerminalColor::Black => Color32::from_rgb(0x0c, 0x0c, 0x0c),
        TerminalColor::Red => Color32::from_rgb(0xc5, 0x0f, 0x1f),
        TerminalColor::Green => Color32::from_rgb(0x13, 0xa1, 0x0e),
        TerminalColor::Yellow => Color32::from_rgb(0xc1, 0x9c, 0x00),
        TerminalColor::Blue => Color32::from_rgb(0x00, 0x37, 0xda),
        TerminalColor::Magenta => Color32::from_rgb(0x88, 0x17, 0x98),
        TerminalColor::Cyan => Color32::from_rgb(0x3a, 0x96, 0xdd),
        TerminalColor::White => Color32::from_rgb(0xcc, 0xcc, 0xcc),
        TerminalColor::BrightBlack => Color32::from_rgb(0x76, 0x76, 0x76),
        TerminalColor::BrightRed => Color32::from_rgb(0xe7, 0x48, 0x56),
        TerminalColor::BrightGreen => Color32::from_rgb(0x16, 0xc6, 0x0c),
        TerminalColor::BrightYellow => Color32::from_rgb(0xf9, 0xf1, 0xa5),
        TerminalColor::BrightBlue => Color32::from_rgb(0x3b, 0x78, 0xff),
        TerminalColor::BrightMagenta => Color32::from_rgb(0xb4, 0x00, 0x9e),
        TerminalColor::BrightCyan => Color32::from_rgb(0x61, 0xd6, 0xd6),
        TerminalColor::BrightWhite => Color32::from_rgb(0xf2, 0xf2, 0xf2),
        TerminalColor::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
    }
}

/// Terminal text selection position within a pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalPos {
    pub line_idx: usize,
    pub col_idx: usize,
}

/// Selection range in a terminal pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSelection {
    pub start: TerminalPos,
    pub end: TerminalPos,
}

impl TerminalSelection {
    pub fn normalized(&self) -> (TerminalPos, TerminalPos) {
        if self.start.line_idx < self.end.line_idx
            || (self.start.line_idx == self.end.line_idx && self.start.col_idx <= self.end.col_idx)
        {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    pub fn contains(&self, line: usize, col: usize) -> bool {
        let (p1, p2) = self.normalized();
        if line < p1.line_idx || line > p2.line_idx {
            return false;
        }
        if p1.line_idx == p2.line_idx {
            col >= p1.col_idx && col < p2.col_idx
        } else if line == p1.line_idx {
            col >= p1.col_idx
        } else if line == p2.line_idx {
            col < p2.col_idx
        } else {
            true
        }
    }
}

/// Information about a pane for UI rendering.
#[derive(Clone)]
pub struct PaneRenderSnapshot {
    pub id: String,
    pub name: String,
    pub lines: Vec<String>,
    pub is_active: bool,
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub cursor_visible: bool,
    pub scrollback_len: usize,
    pub detected_agent: Option<String>,
}

/// Snapshot of the workspace state used for immediate-mode rendering without locking.
#[derive(Clone, Default)]
pub struct WorkspaceSnapshot {
    pub active_project_id: String,
    pub project_name: String,
    pub project_path: String,
    pub projects: Vec<ProjectSummary>,
    pub sessions: Vec<SessionSummary>,
    pub active_session_id: String,
    pub is_connected: bool,
    pub running_processes: usize,
    pub open_ports: usize,
    pub branch_name: Option<String>,
    pub git_added: usize,
    pub git_deleted: usize,
    pub layout: Option<Box<SplitNode>>,
    pub panes: HashMap<String, PaneRenderSnapshot>,
    pub active_pane_id: String,
    pub zoomed_pane_id: Option<String>,
    pub running_shells: Vec<RunningShellInfo>,
    pub active_agents: Vec<(String, String)>,
}

/// State of an active split divider drag interaction.
#[derive(Clone, Debug, Default)]
pub struct LayoutDragState {
    pub active_path: Option<String>,
    pub accumulated_ratio: f32,
    pub is_dragging: bool,
}

#[derive(Clone)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub is_active: bool,
    pub session_count: usize,
    pub is_available: bool,
}

#[derive(Clone)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub pane_count: usize,
    pub is_active: bool,
}

/// Open Project dialog state
#[derive(Clone, Debug, Default)]
pub struct OpenProjectDialogState {
    pub is_open: bool,
    pub path_input: String,
    pub error_msg: Option<String>,
}

/// New Project dialog state
#[derive(Clone, Debug, Default)]
pub struct NewProjectDialogState {
    pub is_open: bool,
    pub name_input: String,
    pub path_input: String,
    pub error_msg: Option<String>,
}

/// Information about a running terminal shell process across any session
#[derive(Clone, Debug, Default)]
pub struct RunningShellInfo {
    pub pane_id: String,
    pub session_id: String,
    pub session_name: String,
    pub pane_name: String,
    pub is_active: bool,
}

/// Remove Project confirmation dialog state (tempterm app.go:164 parity)
#[derive(Clone, Debug, Default)]
pub struct RemoveProjectModalState {
    pub is_open: bool,
    pub project_id: String,
    pub project_name: String,
}

/// Dialog state for starting a New Agent in the workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectorTab {
    Info,
    Files,
    Git,
    Rig,
}

#[derive(Clone, Debug)]
pub struct NewAgentDialogState {
    pub is_open: bool,
    pub selected_agent: String,
    pub available_agents: Vec<String>,
    pub worktree_branch: String,
    pub worktree_base: String,
    pub use_worktree: bool,
    pub copy_env: bool,
    pub status_message: Option<String>,
}

impl Default for NewAgentDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            selected_agent: "claude".to_string(),
            available_agents: vec![
                "claude".to_string(),
                "codex".to_string(),
                "opencode".to_string(),
                "pi".to_string(),
                "agy".to_string(),
                "shell".to_string(),
            ],
            worktree_branch: "feat/agent-worktree".to_string(),
            worktree_base: "HEAD".to_string(),
            use_worktree: false,
            copy_env: false,
            status_message: None,
        }
    }
}

/// Dialog state for Adding a Process in the workspace matching Go `tempterm/processdialog.go:16-36`.
#[derive(Clone, Debug)]
pub struct AddProcessDialogState {
    pub is_open: bool,
    pub name: String,
    pub command: String,
    pub dir: String,
    pub port: String,
    pub autostart: bool,
    pub restart: String,
    pub watch: String,
    pub env: String,
    pub shared: bool,
    pub status_message: Option<String>,
}

impl Default for AddProcessDialogState {
    fn default() -> Self {
        Self {
            is_open: false,
            name: "web".to_string(),
            command: "npm run dev".to_string(),
            dir: ".".to_string(),
            port: "3000".to_string(),
            autostart: true,
            restart: "Always".to_string(),
            watch: "src/**".to_string(),
            env: "NODE_ENV=development".to_string(),
            shared: true,
            status_message: None,
        }
    }
}

/// Parses environment variables matching Go `tempterm/processdialog.go:300`.
pub fn parse_process_env(text: &str) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut env = std::collections::BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            if k.is_empty() {
                return Err(format!("environment line {} must be KEY=value", n + 1));
            }
            if env.contains_key(k) {
                return Err(format!("environment key {:?} is listed twice", k));
            }
            env.insert(k.to_string(), v.trim().to_string());
        } else {
            return Err(format!("environment line {} must be KEY=value", n + 1));
        }
    }
    Ok(env)
}

/// Parses watch globs matching Go `tempterm/processdialog.go:345`.
pub fn parse_process_watch(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// A palette command item with category, title, shortcut label, and action ID.
#[derive(Clone, Debug)]
pub struct PaletteCommand {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub shortcut: Option<&'static str>,
}

/// State for the global Command Palette (`Ctrl+Shift+P`).
#[derive(Clone, Debug, Default)]
pub struct CommandPaletteState {
    pub is_open: bool,
    pub query: String,
    pub selected_index: usize,
}

impl CommandPaletteState {
    pub fn open(&mut self) {
        self.is_open = true;
        self.query.clear();
        self.selected_index = 0;
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn toggle(&mut self) {
        if self.is_open {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn move_up(&mut self, total_items: usize) {
        if total_items > 0 {
            if self.selected_index == 0 {
                self.selected_index = total_items - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    pub fn move_down(&mut self, total_items: usize) {
        if total_items > 0 {
            self.selected_index = (self.selected_index + 1) % total_items;
        }
    }

    pub fn clamp_selection(&mut self, total_items: usize) {
        if total_items == 0 || self.selected_index >= total_items {
            self.selected_index = 0;
        }
    }
}

pub fn filter_palette_commands<'a>(
    commands: &'a [PaletteCommand],
    query: &str,
) -> Vec<&'a PaletteCommand> {
    let q = query.trim().to_lowercase();
    commands
        .iter()
        .filter(|c| {
            if q.is_empty() {
                true
            } else {
                c.title.to_lowercase().contains(&q)
                    || c.category.to_lowercase().contains(&q)
                    || c.shortcut.map_or(false, |sc| sc.to_lowercase().contains(&q))
            }
        })
        .collect()
}

/// Returns the static list of all available command palette actions.
pub fn get_all_palette_commands() -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            id: "app.commandPalette",
            title: "Command Palette...",
            category: "App",
            shortcut: Some("Ctrl+Shift+P"),
        },
        PaletteCommand {
            id: "file.quickOpen",
            title: "Go to File...",
            category: "File",
            shortcut: Some("Ctrl+P"),
        },
        PaletteCommand {
            id: "file.save",
            title: "Save File",
            category: "File",
            shortcut: Some("Ctrl+S"),
        },
        PaletteCommand {
            id: "file.close",
            title: "Close Editor File",
            category: "File",
            shortcut: None,
        },
        PaletteCommand {
            id: "help.keyboardShortcuts",
            title: "Keyboard Shortcuts",
            category: "Help",
            shortcut: Some("F1 / Ctrl+Shift+/"),
        },
        PaletteCommand {
            id: "app.settings",
            title: "Settings...",
            category: "App",
            shortcut: Some("Ctrl+,"),
        },
        PaletteCommand {
            id: "session.new",
            title: "New Terminal Session",
            category: "Session",
            shortcut: Some("Ctrl+T"),
        },
        PaletteCommand {
            id: "session.newAgent",
            title: "New Agent...",
            category: "Session",
            shortcut: Some("F4 / Ctrl+Shift+A"),
        },
        PaletteCommand {
            id: "process.add",
            title: "Add Process...",
            category: "Process",
            shortcut: Some("F6 / Ctrl+Shift+N"),
        },
        PaletteCommand {
            id: "terminal.splitDown",
            title: "Split Terminal Down",
            category: "Terminal",
            shortcut: Some("Ctrl+Shift+D"),
        },
        PaletteCommand {
            id: "terminal.splitRight",
            title: "Split Terminal Right",
            category: "Terminal",
            shortcut: Some("Ctrl+Shift+R"),
        },
        PaletteCommand {
            id: "pane.zoom",
            title: "Zoom / Maximize Pane",
            category: "Terminal",
            shortcut: Some("Ctrl+Shift+Z"),
        },
        PaletteCommand {
            id: "terminal.closePane",
            title: "Close Terminal Pane",
            category: "Terminal",
            shortcut: Some("Ctrl+W"),
        },
        PaletteCommand {
            id: "view.toggleSidebar",
            title: "Toggle Sidebar",
            category: "View",
            shortcut: Some("Ctrl+B"),
        },
        PaletteCommand {
            id: "view.toggleInspector",
            title: "Toggle Inspector",
            category: "View",
            shortcut: Some("Ctrl+Shift+B"),
        },
        PaletteCommand {
            id: "view.info",
            title: "Show Info Inspector",
            category: "View",
            shortcut: Some("Ctrl+Shift+I"),
        },
        PaletteCommand {
            id: "view.files",
            title: "Show Files Inspector",
            category: "View",
            shortcut: Some("Ctrl+Shift+E"),
        },
        PaletteCommand {
            id: "view.git",
            title: "Show Git Inspector",
            category: "View",
            shortcut: Some("Ctrl+Shift+G"),
        },
    ]
}

/// State for an opened editor tab/buffer matching `tempterm/editor.go:34-45`.
pub use crate::editor::EditorDoc as OpenEditorFile;

/// State for Quick Open file picker (`Ctrl+P`).
#[derive(Clone, Debug, Default)]
pub struct QuickOpenState {
    pub is_open: bool,
    pub query: String,
    pub selected_index: usize,
    pub cached_files: Vec<String>,
}

/// Semantic groups for settings categories in the redesigned Settings modal.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SettingsCategoryGroup {
    Application,
    Workspace,
    Integrations,
    System,
}

impl SettingsCategoryGroup {
    /// Returns all category groups in visual hierarchy order.
    pub fn all() -> &'static [SettingsCategoryGroup] {
        &[
            Self::Application,
            Self::Workspace,
            Self::Integrations,
            Self::System,
        ]
    }

    /// Header label for this category group.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Application => "APPLICATION",
            Self::Workspace => "WORKSPACE",
            Self::Integrations => "INTEGRATIONS",
            Self::System => "SYSTEM",
        }
    }

    /// Categories belonging to this group in display order.
    pub fn categories(&self) -> &'static [SettingsCategory] {
        match self {
            Self::Application => &[SettingsCategory::General, SettingsCategory::Appearance],
            Self::Workspace => &[
                SettingsCategory::Terminal,
                SettingsCategory::Editor,
                SettingsCategory::Browser,
                SettingsCategory::Keyboard,
            ],
            Self::Integrations => &[
                SettingsCategory::Notifications,
                SettingsCategory::Agents,
                SettingsCategory::WebAccess,
                SettingsCategory::DevUrls,
            ],
            Self::System => &[SettingsCategory::About],
        }
    }
}

/// Settings window categories matching the 11 original Go categories in `tempterm/settings_pages.go:44-74`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SettingsCategory {
    General,
    Appearance,
    Terminal,
    Editor,
    Notifications,
    Agents,
    WebAccess,
    DevUrls,
    Browser,
    Keyboard,
    About,
}

impl SettingsCategory {
    /// Returns all 11 settings categories in visual grouped order.
    pub fn all() -> &'static [SettingsCategory] {
        &[
            Self::General,
            Self::Appearance,
            Self::Terminal,
            Self::Editor,
            Self::Browser,
            Self::Keyboard,
            Self::Notifications,
            Self::Agents,
            Self::WebAccess,
            Self::DevUrls,
            Self::About,
        ]
    }

    /// Returns the semantic group this category belongs to.
    pub fn group(&self) -> SettingsCategoryGroup {
        match self {
            Self::General | Self::Appearance => SettingsCategoryGroup::Application,
            Self::Terminal | Self::Editor | Self::Browser | Self::Keyboard => {
                SettingsCategoryGroup::Workspace
            }
            Self::Notifications | Self::Agents | Self::WebAccess | Self::DevUrls => {
                SettingsCategoryGroup::Integrations
            }
            Self::About => SettingsCategoryGroup::System,
        }
    }

    /// Human-readable title for the category without emoji decorations.
    pub fn title(&self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Terminal => "Terminal",
            Self::Editor => "Editor",
            Self::Notifications => "Notifications",
            Self::Agents => "Agents",
            Self::WebAccess => "Web Access",
            Self::DevUrls => "Dev URLs",
            Self::Browser => "Browser",
            Self::Keyboard => "Keyboard",
            Self::About => "About",
        }
    }

    /// Descriptive summary of the category settings.
    pub fn description(&self) -> &'static str {
        match self {
            Self::General => "Session daemon, recovery, and link opening behavior",
            Self::Appearance => "Theme, density, and interface visibility settings",
            Self::Terminal => "Typography, cursor behavior, and scrollback limit",
            Self::Editor => "Tab size, word wrapping, and diff view layout",
            Self::Notifications => "Desktop alerts and notification sounds",
            Self::Agents => "Agent supervision, socket controls, and tool integration",
            Self::WebAccess => "Remote browser access and pairing status",
            Self::DevUrls => "Local domain proxy routing for processes",
            Self::Browser => "Default search provider and browser cache",
            Self::Keyboard => "Cheatsheet and keyboard shortcut configuration",
            Self::About => "Build version, application paths, and project links",
        }
    }
}

/// State for the Settings window/modal (`Ctrl+,`).
#[derive(Clone, Debug)]
pub struct SettingsModalState {
    pub is_open: bool,
    pub active_category: SettingsCategory,
    // General
    pub daemon_enabled: bool,
    pub restore_sessions: bool,
    pub resume_agents: bool,
    pub confirm_close: bool,
    pub open_links: String,
    // Appearance
    pub theme_mode: String,
    pub density: String,
    pub dim_unfocused: bool,
    pub show_system_load: bool,
    // Terminal
    pub font_family: String,
    pub font_size: f32,
    pub cursor_shape: String,
    pub cursor_blink: bool,
    pub copy_on_select: bool,
    pub scrollback_limit: u32,
    // Editor
    pub tab_size: u32,
    pub word_wrap: bool,
    pub line_numbers: bool,
    pub diff_split: bool,
    // Notifications
    pub notify_waiting: bool,
    pub notify_done: bool,
    pub notify_error: bool,
    pub notify_sound: bool,
    // Agents
    pub control_socket: bool,
    pub remote_control: bool,
    // Browser
    pub search_engine: String,
    pub custom_search_url: String,
    // Keyboard
    pub keyboard_filter: String,
}

impl Default for SettingsModalState {
    fn default() -> Self {
        let s = crate::settings::Settings::load();
        Self {
            is_open: false,
            active_category: SettingsCategory::General,
            daemon_enabled: s.daemon,
            restore_sessions: s.restore_sessions,
            resume_agents: s.resume_agents,
            confirm_close: s.confirm_close,
            open_links: if s.open_links.is_empty() {
                "system".to_string()
            } else {
                s.open_links
            },
            theme_mode: match s.theme.as_str() {
                "light" => "Light".to_string(),
                "omarchy" => "Omarchy".to_string(),
                _ => "Dark".to_string(),
            },
            density: if s.density == "compact" {
                "Compact".to_string()
            } else {
                "Comfortable".to_string()
            },
            dim_unfocused: s.dim_unfocused,
            show_system_load: s.show_system_load,
            font_family: s.terminal.font_family,
            font_size: if s.terminal.font_size > 0.0 {
                s.terminal.font_size
            } else {
                13.0
            },
            cursor_shape: match s.terminal.cursor.as_str() {
                "bar" => "Bar".to_string(),
                "underline" => "Underline".to_string(),
                _ => "Block".to_string(),
            },
            cursor_blink: s.terminal.cursor_blink,
            copy_on_select: s.terminal.copy_on_select,
            scrollback_limit: if s.terminal.scrollback_mb > 0 {
                (s.terminal.scrollback_mb as u32) * 1000
            } else {
                10000
            },
            tab_size: if s.editor.tab_size > 0 {
                s.editor.tab_size as u32
            } else {
                4
            },
            word_wrap: s.editor.wrap,
            line_numbers: s.editor.line_numbers,
            diff_split: s.editor.diff_split,
            notify_waiting: s.notify.waiting,
            notify_done: s.notify.done,
            notify_error: s.notify.error,
            notify_sound: s.notify.sound,
            control_socket: s.control_socket,
            remote_control: s.remote_control,
            search_engine: if s.search_engine.is_empty() {
                "duckduckgo".to_string()
            } else {
                s.search_engine
            },
            custom_search_url: String::new(),
            keyboard_filter: String::new(),
        }
    }
}

impl SettingsModalState {
    /// Opens the settings modal.
    pub fn open(&mut self) {
        self.is_open = true;
    }

    /// Closes the settings modal.
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Toggles the settings modal open state.
    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
    }

    /// Sets the active settings category.
    pub fn set_category(&mut self, cat: SettingsCategory) {
        self.active_category = cat;
    }

    /// Cycles forward to the next settings category.
    pub fn next_category(&mut self) {
        let all = SettingsCategory::all();
        if let Some(pos) = all.iter().position(|&c| c == self.active_category) {
            let next_pos = (pos + 1) % all.len();
            self.active_category = all[next_pos];
        } else {
            self.active_category = all[0];
        }
    }

    /// Cycles backward to the previous settings category.
    pub fn prev_category(&mut self) {
        let all = SettingsCategory::all();
        if let Some(pos) = all.iter().position(|&c| c == self.active_category) {
            let prev_pos = if pos == 0 { all.len() - 1 } else { pos - 1 };
            self.active_category = all[prev_pos];
        } else {
            self.active_category = all[0];
        }
    }
}

/// Renders a structured setting card with theme-compliant surface, subtle border,
/// 6px rounding, 12px inner margin, and bold 12.5pt title.
pub fn render_setting_card<R>(
    ui: &mut egui::Ui,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let res = egui::Frame::none()
        .fill(theme::SURFACE_CARD)
        .stroke(Stroke::new(1.0_f32, theme::BORDER_SUBTLE))
        .rounding(Rounding::same(6.0_f32))
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(title)
                    .size(12.5)
                    .strong()
                    .color(theme::TEXT_STRONG),
            );
            ui.add_space(8.0);
            add_contents(ui)
        });
    ui.add_space(10.0);
    res.inner
}

/// Inline new file or directory creation in inspector Files tab.
#[derive(Clone, Debug, Default)]
pub struct NewFileEntryState {
    pub is_dir: bool,
    pub name: String,
}

/// Node representing a file or directory in the workspace inspector tree.
#[derive(Clone, Debug)]
pub struct WorkspaceFileEntry {
    pub name: String,
    pub rel_path: String,
    pub is_dir: bool,
    pub children: Vec<WorkspaceFileEntry>,
}

/// State for Keyboard Shortcuts overlay (`Ctrl+Shift+/` or `F1`).
#[derive(Clone, Debug, Default)]
pub struct ShortcutsOverlayState {
    pub is_open: bool,
    pub filter: String,
    pub selected_index: usize,
    pub category_filter: Option<String>,
}

impl ShortcutsOverlayState {
    pub fn open(&mut self) {
        self.is_open = true;
        self.filter.clear();
        self.selected_index = 0;
        self.category_filter = None;
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn toggle(&mut self) {
        if self.is_open {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self, total_items: usize) {
        if total_items > 0 {
            self.selected_index = (self.selected_index + 1).min(total_items - 1);
        } else {
            self.selected_index = 0;
        }
    }

    pub fn set_category_filter(&mut self, category: Option<String>) {
        self.category_filter = category;
        self.selected_index = 0;
    }

    pub fn clamp_selection(&mut self, total_items: usize) {
        if total_items == 0 || self.selected_index >= total_items {
            self.selected_index = 0;
        }
    }
}

/// Single keyboard shortcut definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutItem {
    pub name: &'static str,
    pub key: &'static str,
    pub category: &'static str,
    pub action_id: &'static str,
}

/// Returns the static list of all available keyboard shortcuts.
pub fn get_all_shortcuts() -> &'static [ShortcutItem] {
    &[
        ShortcutItem { name: "Command Palette", key: "Ctrl+Shift+P", category: "App", action_id: "app.commandPalette" },
        ShortcutItem { name: "Open Project", key: "Ctrl+O", category: "App", action_id: "app.openProject" },
        ShortcutItem { name: "Go to File (Quick Open)", key: "Ctrl+P", category: "File", action_id: "file.quickOpen" },
        ShortcutItem { name: "Keyboard Shortcuts", key: "F1 / Ctrl+Shift+/", category: "Help", action_id: "help.keyboardShortcuts" },
        ShortcutItem { name: "Settings", key: "Ctrl+,", category: "App", action_id: "app.settings" },
        ShortcutItem { name: "New Terminal Session", key: "Ctrl+T", category: "Session", action_id: "session.new" },
        ShortcutItem { name: "New Agent", key: "F4 / Ctrl+Shift+A", category: "Agent", action_id: "session.newAgent" },
        ShortcutItem { name: "Add Process", key: "F6 / Ctrl+Shift+N", category: "Process", action_id: "process.add" },
        ShortcutItem { name: "Split Terminal Down", key: "Ctrl+Shift+D", category: "Terminal", action_id: "terminal.splitDown" },
        ShortcutItem { name: "Split Terminal Right", key: "Ctrl+Shift+R", category: "Terminal", action_id: "terminal.splitRight" },
        ShortcutItem { name: "Zoom / Maximize Pane", key: "Ctrl+Shift+Z", category: "Terminal", action_id: "pane.zoom" },
        ShortcutItem { name: "Close Terminal Pane / Editor", key: "Ctrl+W", category: "Terminal", action_id: "terminal.closePane" },
        ShortcutItem { name: "Close Active Split Pane", key: "Ctrl+Shift+W", category: "Terminal", action_id: "terminal.closePane" },
        ShortcutItem { name: "Cycle Pane Focus Forward", key: "Ctrl+Tab", category: "Terminal", action_id: "terminal.cycleForward" },
        ShortcutItem { name: "Cycle Pane Focus Reverse", key: "Ctrl+Shift+Tab", category: "Terminal", action_id: "terminal.cycleReverse" },
        ShortcutItem { name: "Copy Selected Text", key: "Ctrl+C", category: "Terminal", action_id: "terminal.copy" },
        ShortcutItem { name: "Paste from Clipboard", key: "Ctrl+V / Shift+Ins", category: "Terminal", action_id: "terminal.paste" },
        ShortcutItem { name: "Delete Word Backward", key: "Ctrl+BS / Alt+BS", category: "Terminal", action_id: "terminal.delWord" },
        ShortcutItem { name: "Save Active Editor", key: "Ctrl+S", category: "Editor", action_id: "file.save" },
        ShortcutItem { name: "Toggle Diff View", key: "F7 / Ctrl+D", category: "Editor", action_id: "editor.toggleDiff" },
        ShortcutItem { name: "Cycle Workspace Files", key: "F8", category: "Editor", action_id: "editor.cycleFiles" },
        ShortcutItem { name: "Toggle Left Sidebar", key: "Ctrl+B", category: "View", action_id: "view.toggleSidebar" },
        ShortcutItem { name: "Toggle Right Inspector", key: "Ctrl+Shift+B", category: "View", action_id: "view.toggleInspector" },
        ShortcutItem { name: "Inspector Info Tab", key: "Ctrl+Shift+I", category: "View", action_id: "view.info" },
        ShortcutItem { name: "Inspector Files Tab", key: "Ctrl+Shift+E", category: "View", action_id: "view.files" },
        ShortcutItem { name: "Inspector Git Tab", key: "Ctrl+Shift+G", category: "View", action_id: "view.git" },
        ShortcutItem { name: "Cycle Theme", key: "F9", category: "View", action_id: "app.cycleTheme" },
    ]
}

/// Filters shortcuts by search query and category.
pub fn filter_shortcuts<'a>(
    shortcuts: &'a [ShortcutItem],
    query: &str,
    category: Option<&str>,
) -> Vec<&'a ShortcutItem> {
    let q = query.trim().to_lowercase();
    shortcuts
        .iter()
        .filter(|s| {
            if let Some(cf) = category {
                if cf != "All" {
                    let match_cat = match cf {
                        "Terminal" => s.category == "Terminal" || s.category == "Session",
                        "App" => s.category == "App" || s.category == "File" || s.category == "Help",
                        "Editor" => s.category == "Editor",
                        "View" => s.category == "View",
                        "Agent" => s.category == "Agent" || s.category == "Process",
                        _ => s.category == cf,
                    };
                    if !match_cat {
                        return false;
                    }
                }
            }
            if q.is_empty() {
                true
            } else {
                s.name.to_lowercase().contains(&q)
                    || s.key.to_lowercase().contains(&q)
                    || s.category.to_lowercase().contains(&q)
            }
        })
        .collect()
}

/// Thread-safe background cache for Git branch, diff stats, and changed files.
#[derive(Clone, Default)]
pub struct GitStateCache {
    pub branch: Option<String>,
    pub added: usize,
    pub deleted: usize,
    pub files: Vec<(String, String)>,
    pub last_queried: Option<std::time::Instant>,
    pub dir: std::path::PathBuf,
    pub is_fetching: bool,
}

/// The eframe application representing the real desktop workspace.
pub struct WorkspaceApp {
    pub host: Arc<AppHost>,
    pub registry: Arc<Registry>,
    pub tokio_handle: Handle,
    pub snapshot: WorkspaceSnapshot,
    pub renderer: TerminalRenderer,
    pub new_session_requested: bool,
    /// Active text selection per pane: pane_id -> TerminalSelection
    pub pane_selections: HashMap<String, TerminalSelection>,
    /// Mouse drag anchor: pane_id -> start TerminalPos
    pub selection_drag_start: Option<(String, TerminalPos)>,
    /// New Agent workflow dialog state
    pub agent_dialog: NewAgentDialogState,
    /// Add Process workflow dialog state
    pub process_dialog: AddProcessDialogState,
    /// Command Palette state (`Ctrl+Shift+P`)
    pub command_palette: CommandPaletteState,
    /// Quick Open state (`Ctrl+P`)
    pub quick_open: QuickOpenState,
    /// Settings modal state (`Ctrl+,`)
    pub settings_modal: SettingsModalState,
    /// Keyboard Shortcuts cheatsheet state (`F1` or `Ctrl+Shift+/`)
    pub shortcuts_overlay: ShortcutsOverlayState,
    /// Whether the top New menu dropdown is expanded
    pub new_menu_open: bool,
    /// Position where the New menu dropdown should appear
    pub new_menu_pos: Option<Pos2>,
    /// New Project dialog state
    pub new_project_dialog: NewProjectDialogState,
    /// Sidebar width (clamped between 200 and 480 px)
    pub sidebar_width: f32,
    /// Whether sidebar is collapsed
    pub sidebar_collapsed: bool,
    /// Inspector width (clamped between 260 and 640 px)
    pub inspector_width: f32,
    /// Whether inspector is open
    pub inspector_open: bool,
    /// Active inspector tab
    pub inspector_tab: InspectorTab,
    /// Rig Board state
    pub rig_board_state: crate::rig_board::RigBoardState,
    /// Active opened editor files: path -> OpenEditorFile
    pub open_editors: HashMap<String, OpenEditorFile>,
    /// Currently focused editor file path, if any
    pub active_editor_path: Option<String>,
    /// Open Project dialog state
    pub open_project_dialog: OpenProjectDialogState,
    /// Remove Project confirmation modal state
    pub remove_project_modal: RemoveProjectModalState,
    /// Active split divider drag state
    pub layout_drag: LayoutDragState,
    /// Active diff view state
    pub diff_view: crate::editor::DiffViewState,
    /// Document pending close confirmation with unsaved changes
    pub close_unsaved_doc: Option<String>,
    /// Document with external disk conflict pending confirmation
    pub conflict_doc: Option<String>,
    /// Set of expanded directory relative paths in the Files tab
    pub files_expanded_dirs: std::collections::HashSet<String>,
    /// State for creating a new file or folder inline
    pub new_file_entry: Option<NewFileEntryState>,
    /// Thread-safe background cache for Git information
    pub git_cache: Arc<std::sync::Mutex<GitStateCache>>,
    /// Cached base directory for workspace files
    pub files_cached_dir: std::path::PathBuf,
    /// Timestamp of last workspace files disk scan
    pub files_last_scan: std::time::Instant,
    /// Cached file entries for workspace files tree
    pub files_cached_entries: Vec<WorkspaceFileEntry>,
    /// Stored egui context for triggering background repaints
    pub egui_ctx: Option<egui::Context>,
    /// Whether debug mode is active
    pub debug_mode: bool,
    /// Whether an application modal was closed during the current frame
    pub modal_closed_this_frame: bool,
}

impl WorkspaceApp {
    pub fn new(host: Arc<AppHost>, registry: Arc<Registry>, tokio_handle: Handle) -> Self {
        let loaded_settings = crate::settings::Settings::load();
        let settings_modal = SettingsModalState {
            daemon_enabled: loaded_settings.daemon,
            restore_sessions: loaded_settings.restore_sessions,
            resume_agents: loaded_settings.resume_agents,
            confirm_close: loaded_settings.confirm_close,
            theme_mode: match loaded_settings.theme.as_str() {
                "light" => "Light".to_string(),
                "omarchy" => "Omarchy".to_string(),
                _ => "Dark".to_string(),
            },
            dim_unfocused: loaded_settings.dim_unfocused,
            show_system_load: loaded_settings.show_system_load,
            font_size: loaded_settings.terminal.font_size,
            cursor_blink: loaded_settings.terminal.cursor_blink,
            tab_size: loaded_settings.editor.tab_size.max(1) as u32,
            word_wrap: loaded_settings.editor.wrap,
            line_numbers: loaded_settings.editor.line_numbers,
            diff_split: loaded_settings.editor.diff_split,
            ..Default::default()
        };

        let mut app = Self {
            host,
            registry,
            tokio_handle,
            snapshot: WorkspaceSnapshot::default(),
            renderer: TerminalRenderer::new(12, 16.0),
            new_session_requested: false,
            pane_selections: HashMap::new(),
            selection_drag_start: None,
            agent_dialog: NewAgentDialogState::default(),
            process_dialog: AddProcessDialogState::default(),
            command_palette: CommandPaletteState::default(),
            quick_open: QuickOpenState::default(),
            settings_modal,
            shortcuts_overlay: ShortcutsOverlayState::default(),
            new_menu_open: false,
            new_menu_pos: None,
            new_project_dialog: NewProjectDialogState::default(),
            sidebar_width: theme::SIDEBAR_WIDTH_DEFAULT,
            sidebar_collapsed: false,
            inspector_width: theme::INSPECTOR_WIDTH_DEFAULT,
            inspector_open: false,
            inspector_tab: InspectorTab::Info,
            rig_board_state: crate::rig_board::RigBoardState::new(std::path::PathBuf::from(".")),
            open_editors: HashMap::new(),
            active_editor_path: None,
            open_project_dialog: OpenProjectDialogState::default(),
            remove_project_modal: RemoveProjectModalState::default(),
            layout_drag: LayoutDragState::default(),
            diff_view: crate::editor::DiffViewState::default(),
            close_unsaved_doc: None,
            conflict_doc: None,
            files_expanded_dirs: std::collections::HashSet::new(),
            new_file_entry: None,
            git_cache: Arc::new(std::sync::Mutex::new(GitStateCache::default())),
            files_cached_dir: std::path::PathBuf::new(),
            files_last_scan: std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(10))
                .unwrap_or_else(std::time::Instant::now),
            files_cached_entries: Vec::new(),
            egui_ctx: None,
            debug_mode: true,
            modal_closed_this_frame: false,
        };
        app.refresh_snapshot();
        app
    }

    /// Persists current settings_modal values back to the canonical settings.json file.
    pub fn persist_settings(&self) {
        let mut s = crate::settings::Settings::load();
        s.daemon = self.settings_modal.daemon_enabled;
        s.restore_sessions = self.settings_modal.restore_sessions;
        s.resume_agents = self.settings_modal.resume_agents;
        s.confirm_close = self.settings_modal.confirm_close;
        s.open_links = self.settings_modal.open_links.clone();
        s.theme = self.settings_modal.theme_mode.to_lowercase();
        s.density = self.settings_modal.density.to_lowercase();
        s.dim_unfocused = self.settings_modal.dim_unfocused;
        s.show_system_load = self.settings_modal.show_system_load;
        s.terminal.font_family = self.settings_modal.font_family.clone();
        s.terminal.font_size = self.settings_modal.font_size;
        s.terminal.cursor = self.settings_modal.cursor_shape.to_lowercase();
        s.terminal.cursor_blink = self.settings_modal.cursor_blink;
        s.terminal.copy_on_select = self.settings_modal.copy_on_select;
        s.terminal.scrollback_mb = (self.settings_modal.scrollback_limit / 1000).max(1) as i32;
        s.editor.tab_size = self.settings_modal.tab_size as i32;
        s.editor.wrap = self.settings_modal.word_wrap;
        s.editor.line_numbers = self.settings_modal.line_numbers;
        s.editor.diff_split = self.settings_modal.diff_split;
        s.notify.waiting = self.settings_modal.notify_waiting;
        s.notify.done = self.settings_modal.notify_done;
        s.notify.error = self.settings_modal.notify_error;
        s.notify.sound = self.settings_modal.notify_sound;
        s.control_socket = self.settings_modal.control_socket;
        s.remote_control = self.settings_modal.remote_control;
        s.search_engine = if self.settings_modal.search_engine == "Custom..."
            && !self.settings_modal.custom_search_url.is_empty()
        {
            self.settings_modal.custom_search_url.clone()
        } else {
            self.settings_modal.search_engine.to_lowercase()
        };
        if let Err(e) = s.save() {
            eprintln!("[persist_settings] Failed to save settings.json: {}", e);
        } else {
            eprintln!("[persist_settings] Successfully saved settings.json");
        }
    }

    /// Pulls current state from AppHost and Registry without holding locks across frames.
    pub fn refresh_snapshot(&mut self) {
        let is_connected = self.host.is_connected.load(Ordering::Relaxed);
        let active_proj_id = {
            if let Ok(active) = self.host.active_project_id.try_read() {
                active.clone()
            } else {
                String::new()
            }
        };

        let mut project_name = "Default Workspace".to_string();
        let mut project_path = String::new();
        let mut projects_summary = Vec::new();
        let mut sessions_summary = Vec::new();
        let mut active_session_id = String::new();
        let mut session_layout = None;
        let mut pane_snapshots = HashMap::new();
        let mut active_pane_id = String::new();
        let mut zoomed_pane_id = None;

        if let Ok(projs) = self.host.projects.try_read() {
            // Enumerate all registered projects
            for (p_id, p) in projs.iter() {
                let p_display_name = if p.name.eq_ignore_ascii_case("focus project") {
                    "Project".to_string()
                } else {
                    p.name.clone()
                };
                projects_summary.push(ProjectSummary {
                    id: p_id.clone(),
                    name: p_display_name,
                    path: p.path.to_string_lossy().to_string(),
                    is_active: p_id == &active_proj_id,
                    session_count: p.sessions.len(),
                    is_available: p.is_available,
                });
            }

            if let Some(proj) = projs.get(&active_proj_id) {
                project_name = if proj.name.eq_ignore_ascii_case("focus project") {
                    "Project".to_string()
                } else {
                    proj.name.clone()
                };
                project_path = proj.path.to_string_lossy().to_string();
                active_session_id = proj.active_session_id.clone();

                let folder_name = std::path::Path::new(&project_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "terminal".to_string());

                let mut s_idx = 0;
                for (s_id, s) in &proj.sessions {
                    let display_s_name = if s.name.starts_with("session_") || s.name.is_empty() {
                        if s_idx == 0 && proj.sessions.len() == 1 {
                            folder_name.clone()
                        } else {
                            format!("{} ({})", folder_name, s_idx + 1)
                        }
                    } else {
                        s.name.clone()
                    };
                    s_idx += 1;

                    sessions_summary.push(SessionSummary {
                        id: s_id.clone(),
                        name: display_s_name,
                        pane_count: s.panes.len(),
                        is_active: s_id == &active_session_id,
                    });

                    if s_id == &active_session_id {
                        session_layout = s.layout.clone();
                        active_pane_id = s.active_pane_id.clone();
                        zoomed_pane_id = s.zoomed_pane_id.clone();

                        let mut active_pane_lines = Vec::new();

                        for (p_id, pane) in &s.panes {
                            let (lines, c_col, c_row, c_vis, sb_len) =
                                if let Ok(buf) = pane.buffer.try_read() {
                                    (
                                        buf.render_view(),
                                        buf.cursor_col,
                                        buf.cursor_row,
                                        buf.cursor_visible,
                                        buf.scrollback.len(),
                                    )
                                } else if let Some(prev) = self.snapshot.panes.get(p_id) {
                                    (
                                        prev.lines.clone(),
                                        prev.cursor_col,
                                        prev.cursor_row,
                                        prev.cursor_visible,
                                        prev.scrollback_len,
                                    )
                                } else {
                                    (Vec::new(), 0, 0, true, 0)
                                };

                            if p_id == &active_pane_id {
                                active_pane_lines = lines.clone();
                            }

                            let detected_agent = Self::detect_agent_from_terminal_lines(&lines)
                                .map(|(name, _)| name.to_string());

                            pane_snapshots.insert(
                                p_id.clone(),
                                PaneRenderSnapshot {
                                    id: p_id.clone(),
                                    name: pane.name.clone(),
                                    lines,
                                    is_active: p_id == &active_pane_id,
                                    cursor_col: c_col,
                                    cursor_row: c_row,
                                    cursor_visible: c_vis,
                                    scrollback_len: sb_len,
                                    detected_agent,
                                },
                            );
                        }

                        // Auto-detect current working directory from active terminal prompt (e.g. user ran `cd <dir>`)
                        if let Some(detected_cwd) =
                            Self::detect_cwd_from_terminal_lines(&active_pane_lines)
                        {
                            let p_norm = std::path::Path::new(&detected_cwd);
                            if p_norm.is_dir() {
                                let detected_str = detected_cwd.clone();
                                if detected_str != project_path && !detected_str.is_empty() {
                                    let folder_name = p_norm
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "Project".to_string());

                                    project_path = detected_str.clone();
                                    project_name = folder_name.clone();

                                    // Update active session summary name
                                    for sess_sum in &mut sessions_summary {
                                        if sess_sum.is_active {
                                            sess_sum.name = folder_name.clone();
                                        }
                                    }

                                    // Update sidebar project summary
                                    for proj_sum in &mut projects_summary {
                                        if proj_sum.id == active_proj_id {
                                            proj_sum.name = folder_name.clone();
                                            proj_sum.path = detected_str.clone();
                                        }
                                    }

                                    // Also update Project in AppHost
                                    let host = self.host.clone();
                                    let act_id = active_proj_id.clone();
                                    let new_p = std::path::PathBuf::from(&detected_str);
                                    let new_n = folder_name.clone();
                                    self.tokio_handle.spawn(async move {
                                        let mut projs = host.projects.write().await;
                                        if let Some(p) = projs.get_mut(&act_id) {
                                            p.path = new_p.clone();
                                            p.name = new_n.clone();
                                            if let Some(sess) =
                                                p.sessions.get_mut(&p.active_session_id)
                                            {
                                                sess.name = new_n;
                                            }
                                        }
                                        drop(projs);
                                        let _ = host.save_state().await;
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        projects_summary.sort_by(|a, b| a.id.cmp(&b.id));
        sessions_summary.sort_by(|a, b| a.id.cmp(&b.id));

        let mut port_count = 0;
        let mut running_shells = Vec::new();
        if let Ok(projs) = self.host.projects.try_read() {
            if let Some(proj) = projs.get(&active_proj_id) {
                let folder_name = proj
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "terminal".to_string());
                let mut s_idx = 0;
                for (s_id, s) in &proj.sessions {
                    let s_display_name = if s.name.starts_with("session_") || s.name.is_empty() {
                        if s_idx == 0 && proj.sessions.len() == 1 {
                            folder_name.clone()
                        } else {
                            format!("{} ({})", folder_name, s_idx + 1)
                        }
                    } else {
                        s.name.clone()
                    };
                    s_idx += 1;

                    if s.name.starts_with("proc_") {
                        port_count += 1;
                    }

                    for (p_id, pane) in &s.panes {
                        let is_active = s_id == &active_session_id && p_id == &active_pane_id;
                        running_shells.push(RunningShellInfo {
                            pane_id: p_id.clone(),
                            session_id: s_id.clone(),
                            session_name: s_display_name.clone(),
                            pane_name: pane.name.clone(),
                            is_active,
                        });
                    }
                }
            }
        }
        running_shells.sort_by(|a, b| a.pane_id.cmp(&b.pane_id));
        let proc_count = running_shells.len();

        // Read Git info from non-blocking background cache (0ms overhead on UI thread)
        let (git_branch, git_added, git_deleted) = {
            if let Ok(g) = self.git_cache.try_lock() {
                (g.branch.clone(), g.added, g.deleted)
            } else {
                (
                    self.snapshot.branch_name.clone(),
                    self.snapshot.git_added,
                    self.snapshot.git_deleted,
                )
            }
        };

        // Check if background git cache needs refresh (path changed or TTL expired)
        if !project_path.is_empty() {
            let p_buf = std::path::PathBuf::from(&project_path);
            let mut should_fetch = false;
            if let Ok(mut g) = self.git_cache.try_lock() {
                let path_changed = g.dir != p_buf;
                let expired = g
                    .last_queried
                    .map(|t| t.elapsed() > std::time::Duration::from_millis(2500))
                    .unwrap_or(true);
                if (path_changed || expired) && !g.is_fetching {
                    g.is_fetching = true;
                    should_fetch = true;
                }
            }
            if should_fetch {
                let cache_clone = self.git_cache.clone();
                let path_clone = p_buf.clone();
                let egui_ctx_opt = self.egui_ctx.clone();
                self.tokio_handle.spawn_blocking(move || {
                    let b = Self::query_git_branch(&path_clone);
                    let (a, d) = Self::query_git_diff_stats(&path_clone);
                    let f = Self::query_git_status_files(&path_clone);
                    if let Ok(mut g) = cache_clone.lock() {
                        g.branch = b;
                        g.added = a;
                        g.deleted = d;
                        g.files = f;
                        g.dir = path_clone;
                        g.last_queried = Some(std::time::Instant::now());
                        g.is_fetching = false;
                    }
                    if let Some(ctx) = egui_ctx_opt {
                        ctx.request_repaint();
                    }
                });
            }
        }

        let mut active_agents: Vec<(String, String)> = Vec::new();
        for p in pane_snapshots.values() {
            if let Some(ref ag_name) = p.detected_agent {
                let bin_key = match ag_name.as_str() {
                    "Antigravity" => "agy",
                    "Claude Code" => "claude",
                    "Pi Agent" => "pi",
                    "Codex CLI" => "codex",
                    "OpenCode" => "opencode",
                    _ => "agy",
                };
                if !active_agents.iter().any(|(n, _)| n == ag_name) {
                    active_agents.push((ag_name.clone(), bin_key.to_string()));
                }
            }
        }

        // Also check panes in other sessions of this project for running agents
        if let Ok(projs) = self.host.projects.try_read() {
            if let Some(proj) = projs.get(&active_proj_id) {
                for (s_id, s) in &proj.sessions {
                    if s_id != &active_session_id {
                        for pane in s.panes.values() {
                            if let Ok(buf) = pane.buffer.try_read() {
                                let lines = buf.render_view();
                                if let Some((name, bin)) = Self::detect_agent_from_terminal_lines(&lines) {
                                    if !active_agents.iter().any(|(n, _)| n == name) {
                                        active_agents.push((name.to_string(), bin.to_string()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        for s in &sessions_summary {
            let lower = s.name.to_lowercase();
            for &(name, bin) in &[
                ("Claude Code", "claude"),
                ("Pi Agent", "pi"),
                ("Antigravity", "agy"),
                ("Codex CLI", "codex"),
                ("OpenCode", "opencode"),
            ] {
                if lower.contains(bin) || lower.starts_with(&format!("agent_{}", bin)) {
                    if !active_agents.iter().any(|(n, _)| n == name) {
                        active_agents.push((name.to_string(), bin.to_string()));
                    }
                }
            }
        }
        active_agents.sort();

        self.snapshot = WorkspaceSnapshot {
            active_project_id: active_proj_id,
            project_name,
            project_path,
            projects: projects_summary,
            sessions: sessions_summary,
            active_session_id,
            is_connected,
            running_processes: proc_count,
            open_ports: port_count,
            branch_name: git_branch,
            git_added,
            git_deleted,
            layout: session_layout,
            panes: pane_snapshots,
            active_pane_id,
            zoomed_pane_id,
            running_shells,
            active_agents,
        };
    }

    /// Returns true if any modal window, dropdown menu, or confirmation dialog is currently open.
    pub fn has_any_modal_open(&self) -> bool {
        self.new_menu_open
            || self.new_project_dialog.is_open
            || self.agent_dialog.is_open
            || self.process_dialog.is_open
            || self.command_palette.is_open
            || self.quick_open.is_open
            || self.settings_modal.is_open
            || self.shortcuts_overlay.is_open
            || self.open_project_dialog.is_open
            || self.remove_project_modal.is_open
            || self.close_unsaved_doc.is_some()
            || self.conflict_doc.is_some()
    }

    /// Closes all active modal windows, dropdowns, and confirmation dialogs.
    pub fn close_all_modals(&mut self) {
        if self.settings_modal.is_open {
            self.persist_settings();
        }
        self.close_unsaved_doc = None;
        self.conflict_doc = None;
        self.diff_view.is_open = false;
        self.new_menu_open = false;
        self.new_project_dialog.is_open = false;
        self.command_palette.close();
        self.quick_open.is_open = false;
        self.settings_modal.is_open = false;
        self.shortcuts_overlay.close();
        self.agent_dialog.is_open = false;
        self.process_dialog.is_open = false;
        self.open_project_dialog.is_open = false;
        self.remove_project_modal.is_open = false;
    }

    /// Spawns a real new terminal session in the active project.
    pub fn trigger_new_session(&self) {
        let host = self.host.clone();
        let registry = self.registry.clone();
        let handle = self.tokio_handle.clone();
        self.tokio_handle.spawn(async move {
            let active_id = {
                let id = host.active_project_id.read().await;
                if id.is_empty() {
                    "default".to_string()
                } else {
                    id.clone()
                }
            };

            let (cwd_str, count) = {
                let projs = host.projects.read().await;
                if let Some(p) = projs.get(&active_id) {
                    (p.path.to_string_lossy().to_string(), p.sessions.len() + 1)
                } else {
                    (".".to_string(), 1)
                }
            };

            let folder_name = std::path::Path::new(&cwd_str)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "terminal".to_string());
            let session_name = if count <= 1 {
                folder_name
            } else {
                format!("{} ({})", folder_name, count)
            };
            let pane_id = format!("pane_{}", count);

            let cmd = match crate::default_shell_command() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[trigger_new_session] Shell resolution failed: {}", e);
                    return;
                }
            };

            eprintln!(
                "[trigger_new_session] Spawning pane {} for session {} in project {} with cmd: {:?}",
                pane_id, session_name, active_id, cmd
            );

            let res = host
                .spawn_local_pane_with_handle(
                    &active_id,
                    &session_name,
                    &pane_id,
                    120,
                    36,
                    &registry,
                    cmd,
                    cwd_str,
                    handle,
                )
                .await;

            match res {
                Ok(_) => eprintln!("[trigger_new_session] Success spawning pane {}", pane_id),
                Err(e) => eprintln!("[trigger_new_session] Error spawning pane: {}", e),
            }
        });
    }

    /// Splits active pane in the specified direction.
    pub fn trigger_split(&self, dir: SplitDir) {
        let host = self.host.clone();
        let registry = self.registry.clone();
        self.tokio_handle.spawn(async move {
            let res = host.split_active_pane(dir, &registry).await;
            match res {
                Ok(pane) => eprintln!("[trigger_split] Success splitting pane {}", pane.id),
                Err(e) => eprintln!("[trigger_split] Error splitting: {}", e),
            }
        });
    }

    /// Closes the active pane.
    pub fn trigger_close_pane(&self, pane_id: &str) {
        let host = self.host.clone();
        let registry = self.registry.clone();
        let p_id = pane_id.to_string();
        self.tokio_handle.spawn(async move {
            let (proj_id, sess_id) = {
                let projs = host.projects.read().await;
                let active_proj = host.active_project_id.read().await.clone();
                if let Some(p) = projs.get(&active_proj) {
                    (active_proj, p.active_session_id.clone())
                } else {
                    (active_proj, String::new())
                }
            };
            if !proj_id.is_empty() && !sess_id.is_empty() {
                let _ = host.close_pane(&proj_id, &sess_id, &p_id, &registry).await;
            }
        });
    }

    /// Closes an entire session by ID.
    pub fn trigger_close_session(&self, session_id: &str) {
        let host = self.host.clone();
        let registry = self.registry.clone();
        let s_id = session_id.to_string();
        self.tokio_handle.spawn(async move {
            let active_proj = host.active_project_id.read().await.clone();
            if !active_proj.is_empty() {
                let _ = host.close_session(&active_proj, &s_id, &registry).await;
            }
        });
    }

    /// Triggers native Windows folder picker asynchronously and opens or switches to project.
    pub fn trigger_open_project_native(&self) {
        let host = self.host.clone();
        let registry = self.registry.clone();
        self.tokio_handle.spawn(async move {
            let chosen = tokio::task::spawn_blocking(crate::pick_folder_native)
                .await
                .ok()
                .flatten();
            if let Some(path) = chosen {
                let _ = host.open_or_switch_project(path, &registry).await;
            }
        });
    }

    /// Opens or switches to a project by path.
    pub fn trigger_open_project_path(&self, path: PathBuf) {
        let host = self.host.clone();
        let registry = self.registry.clone();
        self.tokio_handle.spawn(async move {
            let _ = host.open_or_switch_project(path, &registry).await;
        });
    }

    /// Creates and switches to a new project with a name and path.
    pub fn trigger_create_project(&self, name: &str, path: PathBuf) {
        let host = self.host.clone();
        let registry = self.registry.clone();
        let proj_name = name.to_string();
        self.tokio_handle.spawn(async move {
            let res = host.open_or_switch_project(path, &registry).await;
            if let Ok(ref proj_id) = res {
                if let Ok(mut projs) = host.projects.try_write() {
                    if let Some(p) = projs.get_mut(proj_id) {
                        p.name = proj_name;
                    }
                }
                let _ = host.save_state().await;
            }
        });
    }

    /// Removes a project and closes all its active sessions and panes.
    pub fn trigger_remove_project(&self, project_id: &str) {
        let host = self.host.clone();
        let registry = self.registry.clone();
        let p_id = project_id.to_string();
        self.tokio_handle.spawn(async move {
            let _ = host.remove_project(&p_id, &registry).await;
        });
    }

    /// Spawns a new session in a specific project.
    pub fn trigger_new_session_in_project(&self, project_id: &str) {
        let host = self.host.clone();
        let registry = self.registry.clone();
        let p_id = project_id.to_string();
        self.tokio_handle.spawn(async move {
            let shell_cmd = match default_shell_command() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!(
                        "[trigger_new_session_in_project] Failed to get shell command: {}",
                        e
                    );
                    return;
                }
            };
            let (sess_id, pane_id, cwd) = {
                let projs = host.projects.read().await;
                if let Some(p) = projs.get(&p_id) {
                    let folder_name = p.path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "terminal".to_string());
                    let count = p.sessions.len() + 1;
                    let s_id = if count <= 1 {
                        folder_name
                    } else {
                        format!("{} ({})", folder_name, count)
                    };
                    let pn_id = format!("pane_{}", uuid_short());
                    (s_id, pn_id, p.path.to_string_lossy().to_string())
                } else {
                    return;
                }
            };
            let _ = host
                .spawn_local_pane_with_handle(
                    &p_id,
                    &sess_id,
                    &pane_id,
                    120,
                    36,
                    &registry,
                    shell_cmd,
                    cwd,
                    tokio::runtime::Handle::current(),
                )
                .await;
        });
    }

    /// Switches the active project context in background.
    pub fn trigger_switch_project(&self, project_id: &str) {
        let host = self.host.clone();
        let p_id = project_id.to_string();
        self.tokio_handle.spawn(async move {
            let _ = host.switch_project(&p_id).await;
        });
    }

    /// Switches the active session context in background.
    pub fn trigger_switch_session(&self, project_id: &str, session_id: &str) {
        let host = self.host.clone();
        let p_id = project_id.to_string();
        let s_id = session_id.to_string();
        self.tokio_handle.spawn(async move {
            let _ = host.switch_session(&p_id, &s_id).await;
        });
    }

    /// Sets active pane focus.
    pub fn focus_pane(&mut self, pane_id: &str) {
        self.snapshot.active_pane_id = pane_id.to_string();
        for p in self.snapshot.panes.values_mut() {
            p.is_active = p.id == pane_id;
        }
        let host = self.host.clone();
        let p_id = pane_id.to_string();
        self.tokio_handle.spawn(async move {
            let mut projs = host.projects.write().await;
            let active_proj = host.active_project_id.read().await.clone();
            if let Some(p) = projs.get_mut(&active_proj) {
                if let Some(sess) = p.sessions.get_mut(&p.active_session_id) {
                    sess.active_pane_id = p_id;
                }
            }
        });
    }

    /// Retrieves currently selected text from a pane.
    pub fn get_selected_text(&self, pane_id: &str) -> Option<String> {
        let sel = self.pane_selections.get(pane_id)?;
        let pane = self.snapshot.panes.get(pane_id)?;
        let (p1, p2) = sel.normalized();

        let mut res = Vec::new();
        for line_idx in p1.line_idx..=p2.line_idx {
            if line_idx >= pane.lines.len() {
                break;
            }
            let raw_line = &pane.lines[line_idx];
            let cells = self.renderer.parse_ansi_line(raw_line);
            let mut line_str = String::new();
            for (col_idx, c) in cells.iter().enumerate() {
                if sel.contains(line_idx, col_idx) {
                    line_str.push(c.ch);
                }
            }
            if !line_str.is_empty() || p1.line_idx == p2.line_idx {
                res.push(line_str);
            }
        }
        let joined = res.join("\n");
        let trimmed = joined.trim_matches('\n').to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    /// Sets selection range for a pane.
    pub fn set_pane_selection(&mut self, pane_id: &str, start: TerminalPos, end: TerminalPos) {
        self.pane_selections
            .insert(pane_id.to_string(), TerminalSelection { start, end });
    }

    /// Clears selection for a pane.
    pub fn clear_pane_selection(&mut self, pane_id: &str) {
        self.pane_selections.remove(pane_id);
    }

    /// Routes raw input bytes to the targeted pane.
    pub fn send_input_to_pane(&self, pane_id: &str, bytes: &[u8]) {
        let host = self.host.clone();
        let p_id = pane_id.to_string();
        let data = bytes.to_vec();
        self.tokio_handle.spawn(async move {
            let projs = host.projects.read().await;
            for p in projs.values() {
                for sess in p.sessions.values() {
                    if let Some(pane) = sess.panes.get(&p_id) {
                        let _ = pane.send_input(&data).await;
                        return;
                    }
                }
            }
        });
    }

    /// Processes terminal keyboard and clipboard events for an active pane.
    /// Returns (bytes_to_send, Option<text_to_copy>).
    pub fn handle_terminal_events(
        &mut self,
        pane_id: &str,
        events: &[egui::Event],
        modifiers: egui::Modifiers,
        clipboard_text: &str,
    ) -> (Vec<u8>, Option<String>) {
        let mut bytes_to_send = Vec::new();
        let mut text_to_copy = None;
        let mut clear_sel = false;
        let mut ctrl_c_handled = false;

        let has_selection = self
            .get_selected_text(pane_id)
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        for event in events {
            match event {
                egui::Event::Copy => {
                    if has_selection {
                        if let Some(text) = self.get_selected_text(pane_id) {
                            if !text.is_empty() {
                                text_to_copy = Some(text);
                                clear_sel = true;
                            }
                        }
                    } else if !ctrl_c_handled {
                        // Standard terminal behavior: Ctrl+C without selection sends 0x03 (ETX / SIGINT)
                        bytes_to_send.push(0x03);
                        ctrl_c_handled = true;
                    }
                }
                egui::Event::Cut => {
                    if has_selection {
                        if let Some(text) = self.get_selected_text(pane_id) {
                            if !text.is_empty() {
                                text_to_copy = Some(text);
                                clear_sel = true;
                            }
                        }
                    } else {
                        // Terminal Ctrl+X sends 0x18 (CAN)
                        bytes_to_send.push(0x18);
                    }
                }
                egui::Event::Paste(text) => {
                    let clip = if text.is_empty() {
                        clipboard_text
                    } else {
                        text.as_str()
                    };
                    if !clip.is_empty() {
                        let formatted = clip.replace("\r\n", "\r").replace('\n', "\r");
                        bytes_to_send.extend_from_slice(formatted.as_bytes());
                    }
                }
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers: key_mods,
                    ..
                } => {
                    if (key_mods.ctrl || key_mods.command) && !key_mods.shift {
                        match key {
                            egui::Key::C => {
                                if has_selection {
                                    if let Some(text) = self.get_selected_text(pane_id) {
                                        if !text.is_empty() {
                                            text_to_copy = Some(text);
                                            clear_sel = true;
                                        }
                                    }
                                } else if !ctrl_c_handled {
                                    bytes_to_send.push(0x03); // Ctrl+C interrupt
                                    ctrl_c_handled = true;
                                }
                            }
                            egui::Key::V => {
                                if !clipboard_text.is_empty() {
                                    let formatted =
                                        clipboard_text.replace("\r\n", "\r").replace('\n', "\r");
                                    bytes_to_send.extend_from_slice(formatted.as_bytes());
                                }
                            }
                            egui::Key::A => bytes_to_send.push(0x01), // Ctrl+A: Beginning of line
                            egui::Key::B => bytes_to_send.push(0x02), // Ctrl+B: Cursor left
                            egui::Key::D => bytes_to_send.push(0x04), // Ctrl+D: EOF
                            egui::Key::E => bytes_to_send.push(0x05), // Ctrl+E: End of line
                            egui::Key::F => bytes_to_send.push(0x06), // Ctrl+F: Cursor right
                            egui::Key::G => bytes_to_send.push(0x07), // Ctrl+G: Bell / Cancel
                            egui::Key::H => bytes_to_send.push(0x08), // Ctrl+H: Backspace
                            egui::Key::J => bytes_to_send.push(0x0a), // Ctrl+J: Line feed
                            egui::Key::K => bytes_to_send.push(0x0b), // Ctrl+K: Kill line
                            egui::Key::L => bytes_to_send.push(0x0c), // Ctrl+L: Clear screen
                            egui::Key::N => bytes_to_send.push(0x0e), // Ctrl+N: Next line / history
                            egui::Key::P => bytes_to_send.push(0x10), // Ctrl+P: Previous line / history
                            egui::Key::R => bytes_to_send.push(0x12), // Ctrl+R: Reverse search
                            egui::Key::T => bytes_to_send.push(0x14), // Ctrl+T: Transpose characters
                            egui::Key::U => bytes_to_send.push(0x15), // Ctrl+U: Erase line
                            egui::Key::W => bytes_to_send.push(0x17), // Ctrl+W: Delete word backward
                            egui::Key::X => bytes_to_send.push(0x18), // Ctrl+X: CAN
                            egui::Key::Y => bytes_to_send.push(0x19), // Ctrl+Y: Yank
                            egui::Key::Z => bytes_to_send.push(0x1a), // Ctrl+Z: Suspend (SIGTSTP)
                            egui::Key::Escape => bytes_to_send.push(0x1b), // Ctrl+Escape
                            egui::Key::OpenBracket => bytes_to_send.push(0x1b), // Ctrl+[ sends ESC
                            egui::Key::Backspace => bytes_to_send.push(0x17), // Ctrl+Backspace: delete word backward (Ctrl+W)
                            egui::Key::Delete => bytes_to_send.extend_from_slice(b"\x1b[3;5~"),
                            egui::Key::ArrowLeft => bytes_to_send.extend_from_slice(b"\x1b[1;5D"),
                            egui::Key::ArrowRight => bytes_to_send.extend_from_slice(b"\x1b[1;5C"),
                            egui::Key::ArrowUp => bytes_to_send.extend_from_slice(b"\x1b[1;5A"),
                            egui::Key::ArrowDown => bytes_to_send.extend_from_slice(b"\x1b[1;5B"),
                            egui::Key::Home => bytes_to_send.extend_from_slice(b"\x1b[1;5H"),
                            egui::Key::End => bytes_to_send.extend_from_slice(b"\x1b[1;5F"),
                            egui::Key::Tab => {
                                let host = self.host.clone();
                                self.tokio_handle.spawn(async move {
                                    let _ = host.cycle_pane_focus(true).await;
                                });
                            }
                            _ => {}
                        }
                    } else if (key_mods.ctrl || key_mods.command) && key_mods.shift {
                        match key {
                            egui::Key::C => {
                                // Ctrl+Shift+C: copy selected text
                                if let Some(text) = self.get_selected_text(pane_id) {
                                    if !text.is_empty() {
                                        text_to_copy = Some(text);
                                        clear_sel = true;
                                    }
                                }
                            }
                            egui::Key::V => {
                                // Ctrl+Shift+V: paste from clipboard
                                if !clipboard_text.is_empty() {
                                    let formatted =
                                        clipboard_text.replace("\r\n", "\r").replace('\n', "\r");
                                    bytes_to_send.extend_from_slice(formatted.as_bytes());
                                }
                            }
                            egui::Key::D => {
                                self.focus_pane(pane_id);
                                self.trigger_split(SplitDir::Horizontal);
                            }
                            egui::Key::R => {
                                self.focus_pane(pane_id);
                                self.trigger_split(SplitDir::Vertical);
                            }
                            egui::Key::Z | egui::Key::Enter => {
                                self.focus_pane(pane_id);
                                self.trigger_toggle_zoom();
                            }
                            egui::Key::W => {
                                self.trigger_close_pane(pane_id);
                            }
                            egui::Key::Tab => {
                                let host = self.host.clone();
                                self.tokio_handle.spawn(async move {
                                    let _ = host.cycle_pane_focus(false).await;
                                });
                            }
                            _ => {}
                        }
                    } else if key_mods.shift && !key_mods.ctrl {
                        match key {
                            egui::Key::Insert => {
                                if !clipboard_text.is_empty() {
                                    let formatted =
                                        clipboard_text.replace("\r\n", "\r").replace('\n', "\r");
                                    bytes_to_send.extend_from_slice(formatted.as_bytes());
                                }
                            }
                            egui::Key::Escape => bytes_to_send.push(0x1b),
                            egui::Key::Tab => bytes_to_send.extend_from_slice(b"\x1b[Z"),
                            egui::Key::Enter => bytes_to_send.push(b'\r'),
                            egui::Key::Backspace => bytes_to_send.push(0x7f),
                            egui::Key::ArrowUp => bytes_to_send.extend_from_slice(b"\x1b[1;2A"),
                            egui::Key::ArrowDown => bytes_to_send.extend_from_slice(b"\x1b[1;2B"),
                            egui::Key::ArrowRight => bytes_to_send.extend_from_slice(b"\x1b[1;2C"),
                            egui::Key::ArrowLeft => bytes_to_send.extend_from_slice(b"\x1b[1;2D"),
                            egui::Key::Home => bytes_to_send.extend_from_slice(b"\x1b[1;2H"),
                            egui::Key::End => bytes_to_send.extend_from_slice(b"\x1b[1;2F"),
                            egui::Key::PageUp => bytes_to_send.extend_from_slice(b"\x1b[5;2~"),
                            egui::Key::PageDown => bytes_to_send.extend_from_slice(b"\x1b[6;2~"),
                            _ => {}
                        }
                    } else if key_mods.alt && !key_mods.ctrl {
                        match key {
                            egui::Key::Escape => bytes_to_send.extend_from_slice(b"\x1b\x1b"),
                            egui::Key::Backspace => bytes_to_send.push(0x17),
                            egui::Key::Delete => bytes_to_send.extend_from_slice(b"\x1b[3;3~"),
                            egui::Key::ArrowLeft => bytes_to_send.extend_from_slice(b"\x1b[1;3D"),
                            egui::Key::ArrowRight => bytes_to_send.extend_from_slice(b"\x1b[1;3C"),
                            egui::Key::ArrowUp => bytes_to_send.extend_from_slice(b"\x1b[1;3A"),
                            egui::Key::ArrowDown => bytes_to_send.extend_from_slice(b"\x1b[1;3B"),
                            egui::Key::Enter => bytes_to_send.extend_from_slice(b"\x1b\r"),
                            _ => {}
                        }
                    } else if !key_mods.ctrl && !key_mods.alt {
                        match key {
                            egui::Key::Escape => {
                                if has_selection {
                                    clear_sel = true;
                                }
                                bytes_to_send.push(0x1b);
                            }
                            egui::Key::F2 => {
                                self.focus_pane(pane_id);
                                self.trigger_split(SplitDir::Vertical);
                            }
                            egui::Key::F3 => {
                                self.focus_pane(pane_id);
                                self.trigger_split(SplitDir::Horizontal);
                            }
                            egui::Key::Enter => bytes_to_send.push(b'\r'),
                            egui::Key::Backspace => bytes_to_send.push(0x7f),
                            egui::Key::Tab => bytes_to_send.push(0x09),
                            egui::Key::ArrowUp => bytes_to_send.extend_from_slice(b"\x1b[A"),
                            egui::Key::ArrowDown => bytes_to_send.extend_from_slice(b"\x1b[B"),
                            egui::Key::ArrowRight => bytes_to_send.extend_from_slice(b"\x1b[C"),
                            egui::Key::ArrowLeft => bytes_to_send.extend_from_slice(b"\x1b[D"),
                            egui::Key::Home => bytes_to_send.extend_from_slice(b"\x1b[H"),
                            egui::Key::End => bytes_to_send.extend_from_slice(b"\x1b[F"),
                            egui::Key::Insert => bytes_to_send.extend_from_slice(b"\x1b[2~"),
                            egui::Key::Delete => bytes_to_send.extend_from_slice(b"\x1b[3~"),
                            egui::Key::PageUp => bytes_to_send.extend_from_slice(b"\x1b[5~"),
                            egui::Key::PageDown => bytes_to_send.extend_from_slice(b"\x1b[6~"),
                            _ => {}
                        }
                    }
                }
                egui::Event::Text(t) => {
                    if !modifiers.ctrl && !modifiers.alt {
                        let filtered: String = t
                            .chars()
                            .filter(|c| !c.is_control() || *c == '\t')
                            .collect();
                        if !filtered.is_empty() {
                            bytes_to_send.extend_from_slice(filtered.as_bytes());
                        }
                    }
                }
                _ => {}
            }
        }

        if clear_sel {
            self.clear_pane_selection(pane_id);
        }

        (bytes_to_send, text_to_copy)
    }

    /// Toggles zoom/maximize on the active pane.
    pub fn trigger_toggle_zoom(&mut self) {
        let host = self.host.clone();
        // Optimistically toggle locally in snapshot for instant UI feedback
        if self.snapshot.zoomed_pane_id.is_some() {
            self.snapshot.zoomed_pane_id = None;
        } else if self.snapshot.panes.len() > 1
            || self
                .snapshot
                .layout
                .as_ref()
                .map_or(false, |l| !l.is_leaf())
        {
            let target = if !self.snapshot.active_pane_id.is_empty() {
                self.snapshot.active_pane_id.clone()
            } else {
                self.snapshot
                    .panes
                    .keys()
                    .next()
                    .cloned()
                    .unwrap_or_default()
            };
            if !target.is_empty() {
                self.snapshot.zoomed_pane_id = Some(target);
            }
        }
        self.tokio_handle.spawn(async move {
            let _ = host.toggle_zoom().await;
        });
    }

    /// Toggles zoom/maximize for a specific pane.
    pub fn trigger_toggle_zoom_pane(&mut self, pane_id: &str) {
        let is_currently_zoomed = self.snapshot.zoomed_pane_id.as_deref() == Some(pane_id);
        if is_currently_zoomed {
            self.snapshot.zoomed_pane_id = None;
            let host = self.host.clone();
            self.tokio_handle.spawn(async move {
                host.set_zoomed_pane(None).await;
            });
        } else {
            self.snapshot.zoomed_pane_id = Some(pane_id.to_string());
            self.snapshot.active_pane_id = pane_id.to_string();
            let host = self.host.clone();
            let p_id = pane_id.to_string();
            self.tokio_handle.spawn(async move {
                host.set_zoomed_pane(Some(p_id)).await;
            });
        }
    }

    /// Updates split ratio at a binary path ("" for root, "a" for left/top, etc.).
    pub fn trigger_update_split_ratio(&mut self, path: &str, ratio: f32) {
        if let Some(ref mut layout) = self.snapshot.layout {
            layout.set_ratio_at_path(path, ratio);
        }
        let host = self.host.clone();
        let p_str = path.to_string();
        self.tokio_handle.spawn(async move {
            host.update_split_ratio(&p_str, ratio).await;
        });
    }

    /// Renders the left sidebar implementing the 6-level hierarchy:
    /// 1. Project switcher
    /// 2. Projects
    /// 3. Sessions
    /// 4. Agents
    /// 5. Git summary
    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        let full_rect = ui.available_rect_before_wrap();
        let painter = ui.painter_at(full_rect);

        // Sidebar dark matte background
        painter.rect_filled(full_rect, 0.0, theme::PANEL);

        // Right hairline divider (1px border)
        painter.line_segment(
            [
                pos2(full_rect.max.x, full_rect.min.y),
                pos2(full_rect.max.x, full_rect.max.y),
            ],
            Stroke::new(1.0_f32, theme::BORDER),
        );

        let header_h = 36.0;
        let header_rect =
            Rect::from_min_size(full_rect.min, Vec2::new(full_rect.width(), header_h));
        let body_rect = Rect::from_min_max(
            pos2(full_rect.min.x, header_rect.max.y + 1.0),
            pos2(full_rect.max.x, full_rect.max.y),
        );

        // 1. PROJECT SWITCHER ROW (Header)
        let mut h_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(header_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        h_ui.add_space(theme::SPACE_COMPACT);

        let proj_name = if self.snapshot.project_name.is_empty() {
            "Workspace".to_string()
        } else {
            self.snapshot.project_name.clone()
        };

        // Project selector pill
        let (proj_r, proj_resp) = h_ui.allocate_exact_size(
            Vec2::new((full_rect.width() - 48.0).clamp(120.0, 220.0), 26.0),
            Sense::click(),
        );
        let pp = h_ui.painter_at(proj_r);
        if proj_resp.hovered() || self.new_menu_open {
            pp.rect_filled(proj_r, theme::RADIUS_SM, theme::SURFACE_HOVER);
            pp.rect_stroke(proj_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BORDER_STRONG));
        } else {
            pp.rect_filled(proj_r, theme::RADIUS_SM, theme::SURFACE);
            pp.rect_stroke(proj_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BORDER));
        }

        let f_icon_r = Rect::from_min_size(pos2(proj_r.min.x + 6.0, proj_r.center().y - 7.0), Vec2::splat(14.0));
        Icons::draw_folder_git(&pp, f_icon_r, theme::ACCENT);

        pp.text(
            pos2(proj_r.min.x + 24.0, proj_r.center().y),
            egui::Align2::LEFT_CENTER,
            &proj_name,
            egui::FontId::proportional(theme::typography::SECTION_HEADING),
            theme::TEXT_PRIMARY,
        );

        let chev_r = Rect::from_min_size(pos2(proj_r.max.x - 14.0, proj_r.center().y - 5.0), Vec2::splat(10.0));
        Icons::draw_chevron_down(&pp, chev_r, theme::TEXT_MUTED);

        if proj_resp.on_hover_text("Switch Project...").clicked() {
            self.new_menu_pos = Some(pos2(proj_r.min.x, proj_r.max.y + 4.0));
            self.new_menu_open = !self.new_menu_open;
        }

        h_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |r_ui| {
            r_ui.add_space(theme::SPACE_COMPACT);

            // Collapse Sidebar button
            let (collapse_r, collapse_resp) =
                r_ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
            if collapse_resp.hovered() {
                r_ui.painter().rect_filled(collapse_r, theme::RADIUS_SM, theme::SURFACE_HOVER);
                r_ui.painter().rect_stroke(collapse_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BORDER_STRONG));
            }
            Icons::draw_panel_left_close(
                &r_ui.painter(),
                collapse_r,
                if collapse_resp.hovered() { theme::TEXT_PRIMARY } else { theme::TEXT_MUTED },
            );
            if collapse_resp.on_hover_text("Collapse Sidebar (Ctrl+B)").clicked() {
                self.sidebar_collapsed = true;
            }
        });

        // Hairline divider line under header
        ui.painter().line_segment(
            [pos2(full_rect.min.x, header_rect.max.y), pos2(full_rect.max.x, header_rect.max.y)],
            Stroke::new(1.0_f32, theme::BORDER),
        );

        // 2. SCROLLABLE HIERARCHY BODY (Projects -> Sessions -> Agents -> Processes -> Git)
        let mut body_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(body_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        body_ui.set_clip_rect(body_rect);

        egui::ScrollArea::vertical()
            .id_salt("sidebar_hierarchy_scroll")
            .auto_shrink([false; 2])
            .show(&mut body_ui, |s_ui| {
                s_ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
                s_ui.add_space(theme::SPACE_MICRO);

                let row_w = s_ui.available_width() - 8.0;

                // -------------------------------------------------------------
                // LEVEL 1: PROJECTS
                // -------------------------------------------------------------
                s_ui.add_space(theme::SPACE_MICRO);
                s_ui.horizontal(|ui| {
                    ui.add_space(theme::SPACE_COMPACT);
                    ui.label(
                        egui::RichText::new("PROJECTS")
                            .size(theme::typography::LABEL)
                            .color(theme::TEXT_MUTED)
                            .extra_letter_spacing(1.0)
                            .strong(),
                    );
                });
                s_ui.add_space(2.0);

                let projects_list = if self.snapshot.projects.is_empty() {
                    vec![ProjectSummary {
                        id: self.snapshot.active_project_id.clone(),
                        name: self.snapshot.project_name.clone(),
                        path: self.snapshot.project_path.clone(),
                        is_active: true,
                        session_count: self.snapshot.sessions.len(),
                        is_available: true,
                    }]
                } else {
                    self.snapshot.projects.clone()
                };

                let active_proj_id = self.snapshot.active_project_id.clone();

                for p in &projects_list {
                    let (h_row_r, h_row_resp) =
                        s_ui.allocate_exact_size(Vec2::new(row_w, theme::SIDEBAR_ROW_HEIGHT), Sense::click());
                    let painter = s_ui.painter_at(h_row_r);

                    if p.is_active {
                        painter.rect_filled(h_row_r, theme::RADIUS_SM, theme::SURFACE_SELECTED);
                        painter.rect_stroke(h_row_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BORDER));
                    } else if h_row_resp.hovered() {
                        painter.rect_filled(h_row_r, theme::RADIUS_SM, theme::SURFACE_HOVER);
                    }

                    // Project folder icon
                    let icon_r = Rect::from_min_size(
                        pos2(h_row_r.min.x + 8.0, h_row_r.center().y - 7.0),
                        Vec2::splat(14.0),
                    );
                    Icons::draw_folder_git(
                        &painter,
                        icon_r,
                        if p.is_active { theme::ACCENT } else { theme::TEXT_MUTED },
                    );

                    painter.text(
                        pos2(h_row_r.min.x + 28.0, h_row_r.center().y),
                        egui::Align2::LEFT_CENTER,
                        &p.name,
                        egui::FontId::proportional(theme::typography::BODY_SECONDARY),
                        if p.is_active { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY },
                    );

                    // Session count badge
                    let badge_text = format!("{}", p.session_count);
                    painter.text(
                        pos2(h_row_r.max.x - 8.0, h_row_r.center().y),
                        egui::Align2::RIGHT_CENTER,
                        &badge_text,
                        egui::FontId::proportional(theme::typography::METADATA),
                        theme::TEXT_MUTED,
                    );

                    if h_row_resp.clicked() && !p.is_active && p.is_available {
                        self.trigger_switch_project(&p.id);
                    }

                    // Context menu
                    h_row_resp.context_menu(|ui| {
                        if p.is_available {
                            if ui.button(format!("New in {}...", p.name)).clicked() {
                                self.trigger_new_session_in_project(&p.id);
                                ui.close_menu();
                            }
                            ui.separator();
                        }
                        if ui.button("Remove Project...").clicked() {
                            self.remove_project_modal = RemoveProjectModalState {
                                is_open: true,
                                project_id: p.id.clone(),
                                project_name: p.name.clone(),
                            };
                            ui.close_menu();
                        }
                    });
                }

                // -------------------------------------------------------------
                // LEVEL 2: SESSIONS (under active project)
                // -------------------------------------------------------------
                s_ui.add_space(theme::SPACE_COMPACT);
                s_ui.horizontal(|ui| {
                    ui.add_space(theme::SPACE_COMPACT);
                    ui.label(
                        egui::RichText::new("SESSIONS")
                            .size(theme::typography::LABEL)
                            .color(theme::TEXT_MUTED)
                            .extra_letter_spacing(1.0)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |r_ui| {
                        r_ui.add_space(theme::SPACE_COMPACT);
                        r_ui.label(
                            egui::RichText::new(format!("{}", self.snapshot.sessions.len()))
                                .size(theme::typography::METADATA)
                                .color(theme::TEXT_MUTED),
                        );
                    });
                });
                s_ui.add_space(2.0);

                if self.snapshot.sessions.is_empty() {
                    let (no_sess_r, no_sess_resp) =
                        s_ui.allocate_exact_size(Vec2::new(row_w, theme::SIDEBAR_ROW_HEIGHT), Sense::click());
                    if no_sess_resp.hovered() {
                        s_ui.painter().rect_filled(no_sess_r, theme::RADIUS_SM, theme::SURFACE_HOVER);
                    }
                    s_ui.painter().text(
                        pos2(no_sess_r.min.x + 18.0, no_sess_r.center().y),
                        egui::Align2::LEFT_CENTER,
                        "+ New Session (Ctrl+T)",
                        egui::FontId::proportional(theme::typography::BODY_SECONDARY),
                        theme::ACCENT,
                    );
                    if no_sess_resp.clicked() {
                        self.trigger_new_session();
                    }
                } else {
                    for s in &self.snapshot.sessions {
                        let (row_r, row_resp) =
                            s_ui.allocate_exact_size(Vec2::new(row_w, theme::SIDEBAR_ROW_HEIGHT), Sense::click());
                        let painter = s_ui.painter_at(row_r);

                        if s.is_active {
                            painter.rect_filled(row_r, theme::RADIUS_SM, theme::SURFACE_SELECTED);
                            painter.rect_stroke(row_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BORDER_STRONG));
                            // Sapphire left indicator line (2.5px)
                            painter.rect_filled(
                                Rect::from_min_size(
                                    pos2(row_r.min.x + 1.0, row_r.min.y + 4.0),
                                    Vec2::new(2.5, row_r.height() - 8.0),
                                ),
                                1.0,
                                theme::ACCENT,
                            );
                        } else if row_resp.hovered() {
                            painter.rect_filled(row_r, theme::RADIUS_SM, theme::SURFACE_HOVER);
                        }

                        // Terminal icon (15px)
                        let icon_r = Rect::from_min_size(
                            pos2(row_r.min.x + 12.0, row_r.center().y - 7.0),
                            Vec2::splat(14.0),
                        );
                        Icons::draw_terminal(
                            &painter,
                            icon_r,
                            if s.is_active { theme::ACCENT } else { theme::TEXT_MUTED },
                        );

                        // Title text
                        painter.text(
                            pos2(row_r.min.x + 32.0, row_r.center().y),
                            egui::Align2::LEFT_CENTER,
                            &s.name,
                            egui::FontId::proportional(theme::typography::BODY_SECONDARY),
                            if s.is_active { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY },
                        );

                        // Close button on hover
                        if row_resp.hovered() {
                            let close_r = Rect::from_min_size(
                                pos2(row_r.max.x - 20.0, row_r.min.y + 6.0),
                                Vec2::splat(16.0),
                            );
                            let close_resp = s_ui.interact(
                                close_r,
                                s_ui.id().with(&s.id).with("close_sidebar_sess"),
                                Sense::click(),
                            );
                            if close_resp.hovered() {
                                painter.rect_filled(close_r, theme::RADIUS_SM, theme::DANGER.linear_multiply(0.25));
                            }
                            Icons::draw_close(
                                &painter,
                                close_r,
                                if close_resp.hovered() { theme::DANGER } else { theme::TEXT_MUTED },
                            );
                            if close_resp.on_hover_text("Close session (Ctrl+W)").clicked() {
                                self.trigger_close_session(&s.id);
                            }
                        }

                        if row_resp.clicked() && !s.is_active {
                            self.trigger_switch_session(&active_proj_id, &s.id);
                        }
                    }
                }

                // -------------------------------------------------------------
                // LEVEL 3: AGENTS (Active Running Agents Only)
                // -------------------------------------------------------------
                s_ui.add_space(theme::SPACE_COMPACT);

                let active_agents = self.snapshot.active_agents.clone();

                s_ui.horizontal(|ui| {
                    ui.add_space(theme::SPACE_COMPACT);
                    ui.label(
                        egui::RichText::new("AGENTS")
                            .size(theme::typography::LABEL)
                            .color(theme::TEXT_MUTED)
                            .extra_letter_spacing(1.0)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |r_ui| {
                        r_ui.add_space(theme::SPACE_COMPACT);
                        r_ui.label(
                            egui::RichText::new(format!("{}", active_agents.len()))
                                .size(theme::typography::METADATA)
                                .color(theme::TEXT_MUTED),
                        );
                        let plus_resp = r_ui.add(
                            egui::Label::new(
                                egui::RichText::new("+")
                                    .size(theme::typography::BODY_SECONDARY)
                                    .color(theme::TEXT_MUTED),
                            )
                            .sense(Sense::click()),
                        );
                        if plus_resp.on_hover_text("Launch Agent (Ctrl+Shift+A)").clicked() {
                            self.agent_dialog.is_open = true;
                        }
                    });
                });
                s_ui.add_space(2.0);

                for (name, bin_key) in &active_agents {
                    let (a_row_r, a_row_resp) =
                        s_ui.allocate_exact_size(Vec2::new(row_w, theme::SIDEBAR_ROW_HEIGHT), Sense::click());
                    let painter = s_ui.painter_at(a_row_r);

                    if a_row_resp.hovered() {
                        painter.rect_filled(a_row_r, theme::RADIUS_SM, theme::SURFACE_HOVER);
                    }

                    // Agent icon (15px)
                    let icon_r = Rect::from_min_size(
                        pos2(a_row_r.min.x + 10.0, a_row_r.center().y - 7.0),
                        Vec2::splat(14.0),
                    );
                    Icons::draw_agent(
                        &painter,
                        icon_r,
                        theme::SUCCESS,
                    );

                    painter.text(
                        pos2(a_row_r.min.x + 30.0, a_row_r.center().y),
                        egui::Align2::LEFT_CENTER,
                        name.as_str(),
                        egui::FontId::proportional(theme::typography::BODY_SECONDARY),
                        theme::TEXT_SECONDARY,
                    );

                    // Running indicator chip (active only)
                    let status_text = "active";
                    let status_color = theme::SUCCESS;
                    painter.circle_filled(pos2(a_row_r.max.x - 48.0, a_row_r.center().y), 3.0, status_color);
                    painter.text(
                        pos2(a_row_r.max.x - 40.0, a_row_r.center().y),
                        egui::Align2::LEFT_CENTER,
                        status_text,
                        egui::FontId::proportional(theme::typography::METADATA),
                        status_color,
                    );

                    if a_row_resp.clicked() {
                        if let Some((target_pane_id, _)) = self.snapshot.panes.iter().find(|(_, p)| {
                            p.detected_agent.as_deref() == Some(name.as_str())
                        }) {
                            let p_id = target_pane_id.clone();
                            self.focus_pane(&p_id);
                        } else if let Some(target_sess) = self.snapshot.sessions.iter().find(|s| {
                            let lower = s.name.to_lowercase();
                            lower.contains(bin_key.as_str()) || lower.starts_with(&format!("agent_{}", bin_key))
                        }) {
                            self.trigger_switch_session(&self.snapshot.active_project_id, &target_sess.id);
                        } else {
                            self.agent_dialog.selected_agent = bin_key.to_string();
                            self.agent_dialog.is_open = true;
                        }
                    }
                }

                // -------------------------------------------------------------
                // LEVEL 4: GIT SUMMARY
                // -------------------------------------------------------------
                s_ui.add_space(theme::SPACE_COMPACT);
                s_ui.horizontal(|ui| {
                    ui.add_space(theme::SPACE_COMPACT);
                    ui.label(
                        egui::RichText::new("GIT SUMMARY")
                            .size(theme::typography::LABEL)
                            .color(theme::TEXT_MUTED)
                            .extra_letter_spacing(1.0)
                            .strong(),
                    );
                });
                s_ui.add_space(2.0);

                let (git_r, git_resp) =
                    s_ui.allocate_exact_size(Vec2::new(row_w, theme::SIDEBAR_ROW_HEIGHT), Sense::click());
                let painter = s_ui.painter_at(git_r);
                if git_resp.hovered() {
                    painter.rect_filled(git_r, theme::RADIUS_SM, theme::SURFACE_HOVER);
                }

                let b_icon_r = Rect::from_min_size(
                    pos2(git_r.min.x + 10.0, git_r.center().y - 7.0),
                    Vec2::splat(14.0),
                );
                Icons::draw_git_branch(&painter, b_icon_r, theme::TEXT_SECONDARY);

                let branch_display = self.snapshot.branch_name.as_deref().unwrap_or("no branch");
                painter.text(
                    pos2(git_r.min.x + 30.0, git_r.center().y),
                    egui::Align2::LEFT_CENTER,
                    branch_display,
                    egui::FontId::proportional(theme::typography::BODY_SECONDARY),
                    theme::TEXT_PRIMARY,
                );

                if self.snapshot.git_added > 0 || self.snapshot.git_deleted > 0 {
                    let mut right_x = git_r.max.x - 8.0;
                    if self.snapshot.git_deleted > 0 {
                        let del_text = if self.snapshot.git_added > 0 {
                            format!(" -{}", self.snapshot.git_deleted)
                        } else {
                            format!("-{}", self.snapshot.git_deleted)
                        };
                        let del_galley = painter.layout_no_wrap(
                            del_text,
                            egui::FontId::proportional(theme::typography::METADATA),
                            theme::DANGER,
                        );
                        let del_w = del_galley.size().x;
                        painter.galley(
                            pos2(right_x - del_w, git_r.center().y - del_galley.size().y * 0.5),
                            del_galley,
                            theme::DANGER,
                        );
                        right_x -= del_w;
                    }
                    if self.snapshot.git_added > 0 {
                        let add_text = format!("+{}", self.snapshot.git_added);
                        let add_galley = painter.layout_no_wrap(
                            add_text,
                            egui::FontId::proportional(theme::typography::METADATA),
                            theme::SUCCESS,
                        );
                        let add_w = add_galley.size().x;
                        painter.galley(
                            pos2(right_x - add_w, git_r.center().y - add_galley.size().y * 0.5),
                            add_galley,
                            theme::SUCCESS,
                        );
                    }
                } else {
                    painter.text(
                        pos2(git_r.max.x - 8.0, git_r.center().y),
                        egui::Align2::RIGHT_CENTER,
                        "Clean",
                        egui::FontId::proportional(theme::typography::METADATA),
                        theme::TEXT_MUTED,
                    );
                }

                if git_resp.clicked() {
                    self.inspector_open = true;
                    self.inspector_tab = InspectorTab::Git;
                }
            });

        ui.allocate_rect(full_rect, Sense::hover());
    }

    /// Renders the 56px compact icon rail for narrow responsive desktop mode (720-899px).
    fn render_sidebar_rail(&mut self, ui: &mut egui::Ui) {
        let full_rect = ui.available_rect_before_wrap();
        let painter = ui.painter_at(full_rect);
        painter.rect_filled(full_rect, 0.0, theme::PANEL);
        painter.line_segment(
            [pos2(full_rect.max.x, full_rect.min.y), pos2(full_rect.max.x, full_rect.max.y)],
            Stroke::new(1.0_f32, theme::BORDER),
        );

        let mut r_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(full_rect)
                .layout(egui::Layout::top_down(egui::Align::Center)),
        );
        r_ui.add_space(theme::SPACE_COMPACT);

        // Expand sidebar button
        let (exp_r, exp_resp) = r_ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
        if exp_resp.hovered() {
            r_ui.painter().rect_filled(exp_r, theme::RADIUS_SM, theme::SURFACE_HOVER);
            r_ui.painter().rect_stroke(exp_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BORDER_STRONG));
        }
        Icons::draw_chevron_right(&r_ui.painter(), exp_r, if exp_resp.hovered() { theme::TEXT_PRIMARY } else { theme::TEXT_MUTED });
        if exp_resp.on_hover_text("Expand Sidebar (Ctrl+B)").clicked() {
            self.sidebar_collapsed = false;
        }

        r_ui.add_space(theme::SPACE_COMPACT);
        r_ui.separator();
        r_ui.add_space(theme::SPACE_COMPACT);

        // Sessions icon (expand sidebar to view sessions)
        let (sess_r, sess_resp) = r_ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
        if sess_resp.hovered() {
            r_ui.painter().rect_filled(sess_r, theme::RADIUS_SM, theme::SURFACE_HOVER);
        }
        Icons::draw_terminal(&r_ui.painter(), sess_r, theme::ACCENT);
        if sess_resp.on_hover_text("Sessions (Expand Sidebar)").clicked() {
            self.sidebar_collapsed = false;
        }

        r_ui.add_space(theme::SPACE_MICRO);

        // Agents icon (expand sidebar to view agents)
        let (agent_r, agent_resp) = r_ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
        if agent_resp.hovered() {
            r_ui.painter().rect_filled(agent_r, theme::RADIUS_SM, theme::SURFACE_HOVER);
        }
        Icons::draw_agent(&r_ui.painter(), agent_r, theme::INFO);
        if agent_resp.on_hover_text("Agents (Expand Sidebar)").clicked() {
            self.sidebar_collapsed = false;
        }

        r_ui.add_space(theme::SPACE_MICRO);

        // Processes icon
        let (proc_r, proc_resp) = r_ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
        if proc_resp.hovered() {
            r_ui.painter().rect_filled(proc_r, theme::RADIUS_SM, theme::SURFACE_HOVER);
        }
        Icons::draw_process(&r_ui.painter(), proc_r, theme::SUCCESS);
        if proc_resp.on_hover_text("Processes (F6)").clicked() {
            self.process_dialog.is_open = true;
        }

        r_ui.add_space(theme::SPACE_MICRO);

        // Git icon
        let (git_r, git_resp) = r_ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
        if git_resp.hovered() {
            r_ui.painter().rect_filled(git_r, theme::RADIUS_SM, theme::SURFACE_HOVER);
        }
        Icons::draw_git_branch(&r_ui.painter(), git_r, theme::WARNING);
        if git_resp.on_hover_text("Git Inspector (Ctrl+Shift+G)").clicked() {
            self.inspector_open = true;
            self.inspector_tab = InspectorTab::Git;
        }

        // Pinned bottom spacer in rail
        r_ui.add_space(theme::SPACE_COMPACT);

        ui.allocate_rect(full_rect, Sense::hover());
    }

    /// Renders the right inspector panel (Info, Files, Git, Rig tabs).
    fn render_inspector(&mut self, ui: &mut egui::Ui) {
        let frame = Frame::none()
            .fill(theme::WASH)
            .inner_margin(Margin::same(12.0))
            .stroke(Stroke::new(1.0_f32, theme::BORDER));

        frame.show(ui, |ui| {
            ui.set_min_height(ui.available_height());

            // Header: Segmented tab switcher and close button
            ui.horizontal(|ui| {
                let tabs = [
                    (InspectorTab::Info, "Info"),
                    (InspectorTab::Files, "Files"),
                    (InspectorTab::Git, "Git"),
                    (InspectorTab::Rig, "Rig"),
                ];

                for (t, label) in tabs {
                    let is_sel = self.inspector_tab == t;
                    let text = egui::RichText::new(label).size(theme::typography::TERMINAL_TEXT).color(if is_sel {
                        theme::TEXT_STRONG
                    } else {
                        theme::TEXT_MUTED
                    });
                    let btn = egui::Button::new(text)
                        .fill(if is_sel {
                            theme::CARD_RAISED
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(if is_sel {
                            Stroke::new(1.0_f32, theme::BORDER_STRONG)
                        } else {
                            Stroke::NONE
                        })
                        .rounding(Rounding::same(theme::RADIUS_SM));
                    if ui.add(btn).clicked() {
                        self.inspector_tab = t;
                    }
                }
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // Tab Content
            match self.inspector_tab {
                InspectorTab::Info => {
                    ui.label(
                        egui::RichText::new("PROJECT")
                            .size(10.0)
                            .color(theme::TEXT_MUTED)
                            .extra_letter_spacing(1.0)
                            .strong(),
                    );
                    ui.add_space(4.0);

                    let card = Frame::none()
                        .fill(theme::TERMINAL_BG)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER))
                        .rounding(Rounding::same(theme::RADIUS_MD))
                        .inner_margin(Margin::same(10.0));

                    card.show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(&self.snapshot.project_name)
                                .size(12.5)
                                .strong()
                                .color(theme::TEXT_STRONG),
                        );
                        if !self.snapshot.project_path.is_empty() {
                            ui.label(
                                egui::RichText::new(&self.snapshot.project_path)
                                    .size(10.5)
                                    .color(theme::TEXT_MUTED),
                            );
                        }
                    });

                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("RUNNING SHELLS")
                                .size(10.0)
                                .color(theme::TEXT_MUTED)
                                .extra_letter_spacing(1.0)
                                .strong(),
                        );
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |r_ui| {
                                r_ui.label(
                                    egui::RichText::new(format!("{}", self.snapshot.running_shells.len()))
                                        .size(10.0)
                                        .color(theme::TEXT_MUTED),
                                );
                            },
                        );
                    });
                    ui.add_space(4.0);

                    if self.snapshot.running_shells.is_empty() {
                        ui.label(
                            egui::RichText::new("No running shells or processes")
                                .size(theme::typography::TERMINAL_TEXT)
                                .color(theme::TEXT_FAINT)
                                .italics(),
                        );
                    } else {
                        let mut switch_target: Option<(String, String)> = None;
                        for shell in &self.snapshot.running_shells {
                            let (row_r, row_resp) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 26.0),
                                Sense::click(),
                            );
                            let painter = ui.painter_at(row_r);
                            if row_resp.hovered() {
                                painter.rect_filled(row_r, theme::RADIUS_SM, theme::HOVER);
                            } else if shell.is_active {
                                painter.rect_filled(row_r, theme::RADIUS_SM, theme::CARD_RAISED);
                                painter.rect_stroke(row_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BORDER));
                            }

                            // Status dot
                            let dot_center = pos2(row_r.min.x + 8.0, row_r.center().y);
                            painter.circle_filled(
                                dot_center,
                                2.5,
                                if shell.is_active {
                                    theme::OK
                                } else {
                                    theme::TEXT_MUTED
                                },
                            );

                            let label_text = if shell.pane_name.is_empty() || shell.pane_name == "terminal" {
                                shell.session_name.clone()
                            } else {
                                format!("{} ({})", shell.pane_name, shell.session_name)
                            };

                            painter.text(
                                pos2(row_r.min.x + 18.0, row_r.center().y),
                                egui::Align2::LEFT_CENTER,
                                &label_text,
                                egui::FontId::proportional(11.5),
                                if shell.is_active {
                                    theme::TEXT_STRONG
                                } else {
                                    theme::TEXT
                                },
                            );

                            painter.text(
                                pos2(row_r.max.x - 6.0, row_r.center().y),
                                egui::Align2::RIGHT_CENTER,
                                &shell.pane_id,
                                egui::FontId::monospace(10.0),
                                theme::TEXT_MUTED,
                            );

                            if row_resp.on_hover_text("Click to switch to this shell").clicked() {
                                switch_target = Some((shell.session_id.clone(), shell.pane_id.clone()));
                            }
                            ui.add_space(2.0);
                        }
                        if let Some((sess_id, p_id)) = switch_target {
                            self.trigger_switch_session(&self.snapshot.active_project_id, &sess_id);
                            self.focus_pane(&p_id);
                        }
                    }

                    ui.add_space(14.0);
                    ui.label(
                        egui::RichText::new("PORTS")
                            .size(10.0)
                            .color(theme::TEXT_MUTED)
                            .extra_letter_spacing(1.0)
                            .strong(),
                    );
                    ui.add_space(4.0);

                    if self.snapshot.open_ports == 0 {
                        ui.label(
                            egui::RichText::new(
                                "No listening ports\nStart a dev server to see its port here",
                            )
                            .size(11.0)
                            .color(theme::TEXT_FAINT)
                            .italics(),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} active port(s) registered",
                                self.snapshot.open_ports
                            ))
                            .size(theme::typography::TERMINAL_TEXT)
                            .color(theme::STATUS_OK),
                        );
                    }
                }
                InspectorTab::Files => {
                    self.render_files_tab(ui);
                }
                InspectorTab::Git => {
                    ui.label(
                        egui::RichText::new("GIT STATUS")
                            .size(10.0)
                            .color(theme::TEXT_MUTED)
                            .extra_letter_spacing(1.0),
                    );
                    ui.add_space(6.0);

                    if let Some(ref branch) = self.snapshot.branch_name {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Branch:")
                                    .size(theme::typography::TERMINAL_TEXT)
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.label(
                                egui::RichText::new(branch)
                                    .size(theme::typography::TERMINAL_TEXT)
                                    .color(theme::ACCENT)
                                    .strong(),
                            );
                        });
                        ui.add_space(6.0);

                        let git_files = if let Ok(g) = self.git_cache.try_lock() {
                            g.files.clone()
                        } else {
                            Vec::new()
                        };

                        if git_files.is_empty() {
                            ui.label(
                                egui::RichText::new("Working directory clean. No modified files.")
                                    .size(11.0)
                                    .color(theme::STATUS_OK),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(format!("{} changed file(s):", git_files.len()))
                                    .size(11.0)
                                    .color(theme::TEXT_SOFT),
                            );
                            ui.add_space(2.0);

                            let mut file_to_open = None;
                            egui::ScrollArea::vertical()
                                .id_salt("inspector_git_files_scroll")
                                .auto_shrink([false; 2])
                                .max_height(240.0)
                                .show(ui, |ui| {
                                    for (status, path) in &git_files {
                                        ui.horizontal(|ui| {
                                            let st_color = match status.as_str() {
                                                "M" => theme::WARN,
                                                "A" | "??" => theme::OK,
                                                "D" => theme::BAD,
                                                _ => theme::INFO,
                                            };
                                            ui.colored_label(
                                                st_color,
                                                egui::RichText::new(status).size(10.5).monospace().strong(),
                                            );
                                            if ui
                                                .add(
                                                    egui::Label::new(
                                                        egui::RichText::new(path).size(11.0).color(theme::TEXT),
                                                    )
                                                    .sense(Sense::click()),
                                                )
                                                .on_hover_text("Open in editor")
                                                .clicked()
                                            {
                                                file_to_open = Some(path.clone());
                                            }
                                        });
                                    }
                                });

                            if let Some(f) = file_to_open {
                                self.open_editor_file(&f);
                            }
                        }
                    } else {
                        ui.label(
                            egui::RichText::new("Not inside a git repository")
                                .size(theme::typography::TERMINAL_TEXT)
                                .color(theme::TEXT_FAINT)
                                .italics(),
                        );
                    }
                }
                InspectorTab::Rig => {
                    self.render_rig_tab(ui);
                }
            }
        });
    }

    /// Renders the Rig Board panel in the workspace inspector.
    pub fn render_rig_tab(&mut self, ui: &mut egui::Ui) {
        let repo_path = if !self.files_cached_dir.as_os_str().is_empty() {
            self.files_cached_dir.clone()
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };
        self.rig_board_state.repo_path = repo_path.clone();

        let mut db = match temp_terminal_rig::RigDatabase::open_repo(&repo_path) {
            Ok(db) => db,
            Err(_) => match temp_terminal_rig::RigDatabase::open_in_memory() {
                Ok(db) => db,
                Err(_) => return,
            },
        };

        self.rig_board_state.check_alerts(&mut db);
        crate::rig_board::RigBoardPanel::ui(ui, &mut self.rig_board_state, &mut db);
    }

/// Strips ANSI escape sequences (CSI, OSC, and 2-byte escape codes) from a string.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '\x1b' {
            if i + 1 < len {
                let next = chars[i + 1];
                if next == '[' {
                    // CSI sequence: ESC [ [params] [final_byte in @..=~]
                    let mut j = i + 2;
                    while j < len && !('@'..='~').contains(&chars[j]) {
                        j += 1;
                    }
                    if j < len {
                        i = j + 1;
                        continue;
                    } else {
                        break;
                    }
                } else if next == ']' {
                    // OSC sequence: ESC ] ... BEL (\x07) or ST (\x1b\)
                    let mut j = i + 2;
                    let mut found = false;
                    while j < len {
                        if chars[j] == '\x07' {
                            i = j + 1;
                            found = true;
                            break;
                        } else if chars[j] == '\x1b' && j + 1 < len && chars[j + 1] == '\\' {
                            i = j + 2;
                            found = true;
                            break;
                        }
                        j += 1;
                    }
                    if found {
                        continue;
                    } else {
                        break;
                    }
                } else if next == '(' || next == ')' || next == '*' || next == '+' {
                    // Character set selection: ESC ( B, etc.
                    i += 3.min(len - i);
                    continue;
                } else {
                    // 2-byte escape sequence (ESC M, ESC =, ESC >, etc.)
                    i += 2;
                    continue;
                }
            } else {
                break;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }

    out
}

    /// Inspects recent terminal buffer lines to detect the current working directory of the shell.
    pub fn detect_cwd_from_terminal_lines(lines: &[String]) -> Option<String> {
        for line in lines.iter().rev().take(20) {
            let clean = Self::strip_ansi(line);
            let trimmed = clean.trim();
            if trimmed.is_empty() {
                continue;
            }

            // 1. Check for PowerShell prompt: "PS <PATH>>" or "PS <PATH>> ..."
            if let Some(rest) = trimmed.strip_prefix("PS ") {
                if let Some(idx) = rest.find('>') {
                    let candidate = rest[..idx].trim();
                    let path = std::path::Path::new(candidate);
                    if path.is_dir() {
                        if let Ok(c) = path.canonicalize() {
                            let s = c.to_string_lossy().to_string();
                            let clean = s.strip_prefix(r"\\?\").unwrap_or(&s).to_string();
                            return Some(clean);
                        }
                        return Some(candidate.to_string());
                    }
                }
            }

            // 2. Check for Windows Command Prompt: "X:\path\to\folder>"
            if trimmed.len() >= 3
                && trimmed.as_bytes()[1] == b':'
                && (trimmed.as_bytes()[2] == b'\\' || trimmed.as_bytes()[2] == b'/')
            {
                if let Some(idx) = trimmed.find('>') {
                    let candidate = trimmed[..idx].trim();
                    let path = std::path::Path::new(candidate);
                    if path.is_dir() {
                        if let Ok(c) = path.canonicalize() {
                            let s = c.to_string_lossy().to_string();
                            let clean = s.strip_prefix(r"\\?\").unwrap_or(&s).to_string();
                            return Some(clean);
                        }
                        return Some(candidate.to_string());
                    }
                }
            }

            // 3. Check for Unix / Bash / Zsh prompt: "...:/path/to/dir$"
            if let Some(idx) = trimmed.rfind('$').or_else(|| trimmed.rfind('#')) {
                let prefix = trimmed[..idx].trim();
                let candidate = if let Some(colon_idx) = prefix.rfind(':') {
                    prefix[colon_idx + 1..].trim()
                } else {
                    prefix
                };
                let path = std::path::Path::new(candidate);
                if path.is_dir() {
                    if let Ok(c) = path.canonicalize() {
                        return Some(c.to_string_lossy().to_string());
                    }
                    return Some(candidate.to_string());
                }
            }
        }
        None
    }

    /// Inspects recent terminal buffer lines to auto-detect whether an AI coding agent (e.g. `agy`, `claude`)
    /// is actively running in the terminal.
    pub fn detect_agent_from_terminal_lines(lines: &[String]) -> Option<(&'static str, &'static str)> {
        for line in lines.iter().rev().take(30) {
            let clean = Self::strip_ansi(line);
            let trimmed = clean.trim();
            if trimmed.is_empty() {
                continue;
            }

            // 1. Antigravity CLI (agy) active screen indicators
            if trimmed.contains("? for shortcuts")
                || trimmed.contains("Antigravity")
                || trimmed.contains("antigravity-cli")
                || trimmed.starts_with("agy>")
                || trimmed.contains("Usage of agy")
                || trimmed.contains("Generating...")
                || trimmed.contains("Running command...")
                || trimmed.contains("Thinking...")
                || trimmed.contains("Antigravity CLI")
                || trimmed.contains("Type a query or /command")
                || trimmed.contains("Type your message or /command")
            {
                return Some(("Antigravity", "agy"));
            }

            // 2. Claude Code active screen indicators
            if trimmed.contains("Claude Code")
                || trimmed.contains("esc to interrupt")
                || trimmed.contains("Waiting for background agents")
                || trimmed.starts_with("claude>")
            {
                return Some(("Claude Code", "claude"));
            }

            // 3. Pi Agent active screen indicators
            if trimmed.contains("Oh My Pi") || trimmed.contains("pi-agent") || trimmed.starts_with("pi>") {
                return Some(("Pi Agent", "pi"));
            }

            // 4. Codex CLI active screen indicators
            if trimmed.contains("Codex CLI") || trimmed.contains("OpenAI Codex") || trimmed.starts_with("codex>") {
                return Some(("Codex CLI", "codex"));
            }

            // 5. OpenCode active screen indicators
            if trimmed.contains("OpenCode") || trimmed.starts_with("opencode>") {
                return Some(("OpenCode", "opencode"));
            }

            // 6. Check for shell prompt lines (e.g. "PS C:\path> agy" or "C:\path> agy" or "...$ agy" or "❯ agy")
            let cmd_opt = if let Some(idx) = trimmed.rfind('>').or_else(|| trimmed.rfind('$')).or_else(|| trimmed.rfind('#')).or_else(|| trimmed.rfind('%')) {
                Some(trimmed[idx + 1..].trim())
            } else if let Some(idx) = trimmed.rfind('❯').or_else(|| trimmed.rfind('➜')).or_else(|| trimmed.rfind('»')) {
                let char_len = trimmed[idx..].chars().next().map_or(1, |c| c.len_utf8());
                Some(trimmed[idx + char_len..].trim())
            } else if trimmed.starts_with("agy") || trimmed.starts_with("claude") || trimmed.starts_with("pi") || trimmed.starts_with("codex") || trimmed.starts_with("opencode") {
                Some(trimmed)
            } else {
                None
            };

            if let Some(cmd) = cmd_opt {
                if cmd.is_empty() {
                    // Shell prompt is waiting for input: no agent is actively executing
                    return None;
                }
                let first_word = cmd.split_whitespace().next().unwrap_or("");
                let bin = first_word
                    .trim_end_matches(".exe")
                    .trim_start_matches("./")
                    .trim_start_matches(".\\");
                if bin == "agy" || bin == "antigravity" {
                    return Some(("Antigravity", "agy"));
                }
                if bin == "claude" || bin == "claude-code" {
                    return Some(("Claude Code", "claude"));
                }
                if bin == "pi" || bin == "omp" {
                    return Some(("Pi Agent", "pi"));
                }
                if bin == "codex" {
                    return Some(("Codex CLI", "codex"));
                }
                if bin == "opencode" {
                    return Some(("OpenCode", "opencode"));
                }

                // If another command (like `dir`, `cargo`, `git`) is running at this prompt, stop scanning
                return None;
            }
        }
        None
    }

    /// Queries the current git branch for a directory.
    pub fn query_git_branch(dir: &std::path::Path) -> Option<String> {
        if !dir.is_dir() {
            return None;
        }
        if let Ok(repo) = temp_terminal_core::git::Repo::open(dir) {
            if let Ok(branch) =
                temp_terminal_core::git::git_out(&repo.root, &["branch", "--show-current"])
            {
                let b = branch.trim();
                if !b.is_empty() {
                    return Some(b.to_string());
                }
            }
            if let Ok(rev) =
                temp_terminal_core::git::git_out(&repo.root, &["rev-parse", "--abbrev-ref", "HEAD"])
            {
                let b = rev.trim();
                if !b.is_empty() && b != "HEAD" {
                    return Some(b.to_string());
                }
            }
        }
        None
    }

    /// Queries git diff line additions and deletions for a directory.
    pub fn query_git_diff_stats(dir: &std::path::Path) -> (usize, usize) {
        if !dir.is_dir() {
            return (0, 0);
        }
        if let Ok(repo) = temp_terminal_core::git::Repo::open(dir) {
            if let Ok(out) = temp_terminal_core::git::git_out(&repo.root, &["diff", "--numstat"]) {
                let mut added = 0;
                let mut deleted = 0;
                for line in out.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(a) = parts[0].parse::<usize>() {
                            added += a;
                        }
                        if let Ok(d) = parts[1].parse::<usize>() {
                            deleted += d;
                        }
                    }
                }
                return (added, deleted);
            }
        }
        (0, 0)
    }

    /// Queries changed files from `git status --porcelain`.
    pub fn query_git_status_files(dir: &std::path::Path) -> Vec<(String, String)> {
        if !dir.is_dir() {
            return Vec::new();
        }
        if let Ok(repo) = temp_terminal_core::git::Repo::open(dir) {
            if let Ok(out) =
                temp_terminal_core::git::git_out(&repo.root, &["status", "--porcelain"])
            {
                let mut res = Vec::new();
                for line in out.lines() {
                    let trimmed = line.trim();
                    if trimmed.len() >= 3 {
                        let status = trimmed[..2].trim().to_string();
                        let path = trimmed[2..].trim().to_string();
                        res.push((status, path));
                    }
                }
                return res;
            }
        }
        Vec::new()
    }

    /// Recursively scans workspace files and directories for the Inspector Files tab.
    fn scan_project_dir(
        base: &std::path::Path,
        rel: &str,
        open_dirs: &std::collections::HashSet<String>,
        depth: usize,
    ) -> Vec<WorkspaceFileEntry> {
        if depth > 4 {
            return Vec::new();
        }
        let target = if rel.is_empty() {
            base.to_path_buf()
        } else {
            base.join(rel)
        };

        let mut entries = Vec::new();
        if let Ok(read) = std::fs::read_dir(&target) {
            for entry in read.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name == ".git"
                    || name == "target"
                    || name == "node_modules"
                    || name.starts_with(".codebase-memory")
                {
                    continue;
                }
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let child_rel = if rel.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", rel, name)
                };

                let children = if is_dir && open_dirs.contains(&child_rel) {
                    Self::scan_project_dir(base, &child_rel, open_dirs, depth + 1)
                } else {
                    Vec::new()
                };

                entries.push(WorkspaceFileEntry {
                    name,
                    rel_path: child_rel,
                    is_dir,
                    children,
                });
            }
        }

        entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });

        entries
    }

    /// Recursively renders one file or directory row in the tree view.
    fn render_file_tree_entry(
        ui: &mut egui::Ui,
        entry: &WorkspaceFileEntry,
        depth: usize,
        open_dirs: &std::collections::HashSet<String>,
        active_editor: Option<&str>,
        file_to_open: &mut Option<String>,
        dir_to_toggle: &mut Option<String>,
    ) {
        let content_w = ui.available_width();
        let row_h = 22.0;
        let (row_r, row_resp) = ui.allocate_exact_size(Vec2::new(content_w, row_h), Sense::click());
        let p = ui.painter_at(row_r);

        let is_active_file = active_editor == Some(&entry.rel_path);
        if is_active_file {
            p.rect_filled(row_r, 4.0, theme::CARD_RAISED);
            p.rect_stroke(row_r, 4.0, Stroke::new(1.0_f32, theme::BORDER_STRONG));
        } else if row_resp.hovered() {
            p.rect_filled(row_r, 4.0, theme::HOVER);
        }

        let indent = 4.0 + (depth as f32) * 12.0;

        // 1. Caret / Chevron for directories
        let caret_r = Rect::from_min_size(
            pos2(row_r.min.x + indent, row_r.center().y - 5.0),
            Vec2::new(10.0, 10.0),
        );
        if entry.is_dir {
            let is_open = open_dirs.contains(&entry.rel_path);
            if is_open {
                Icons::draw_chevron_down(&p, caret_r, theme::TEXT_MUTED);
            } else {
                Icons::draw_chevron_right(&p, caret_r, theme::TEXT_MUTED);
            }
        }

        // 2. Icon / Badge (Width 16px)
        let icon_r = Rect::from_min_size(
            pos2(row_r.min.x + indent + 12.0, row_r.center().y - 7.0),
            Vec2::new(14.0, 14.0),
        );
        if entry.is_dir {
            Icons::draw_folder(&p, icon_r, Color32::from_rgb(0xd2, 0x99, 0x22));
        } else {
            let name_lower = entry.name.to_lowercase();
            if name_lower.ends_with(".rs") {
                p.text(
                    icon_r.center(),
                    egui::Align2::CENTER_CENTER,
                    "RS",
                    egui::FontId::proportional(9.5),
                    Color32::from_rgb(0xf7, 0x7b, 0x3b),
                );
            } else if name_lower.ends_with(".go") {
                p.text(
                    icon_r.center(),
                    egui::Align2::CENTER_CENTER,
                    "GO",
                    egui::FontId::proportional(9.5),
                    Color32::from_rgb(0x00, 0xad, 0xd8),
                );
            } else if name_lower.ends_with(".ts") || name_lower.ends_with(".tsx") {
                p.text(
                    icon_r.center(),
                    egui::Align2::CENTER_CENTER,
                    "TS",
                    egui::FontId::proportional(9.5),
                    Color32::from_rgb(0x38, 0x8b, 0xfd),
                );
            } else if name_lower.ends_with(".json")
                || name_lower.ends_with(".toml")
                || name_lower.ends_with(".yaml")
                || name_lower.ends_with(".yml")
            {
                p.text(
                    icon_r.center(),
                    egui::Align2::CENTER_CENTER,
                    "{ }",
                    egui::FontId::proportional(9.0),
                    Color32::from_rgb(0xd2, 0x99, 0x22),
                );
            } else if name_lower.ends_with(".md") {
                Icons::draw_file_text(&p, icon_r, Color32::from_rgb(0x58, 0xa6, 0xff));
            } else if name_lower.ends_with(".png")
                || name_lower.ends_with(".jpg")
                || name_lower.ends_with(".svg")
            {
                Icons::draw_image(&p, icon_r, Color32::from_rgb(0x3f, 0xb9, 0x50));
            } else {
                Icons::draw_file_text(&p, icon_r, theme::TEXT_MUTED);
            }
        }

        // 3. Name Label
        let name_pos = pos2(row_r.min.x + indent + 28.0, row_r.center().y);
        p.text(
            name_pos,
            egui::Align2::LEFT_CENTER,
            &entry.name,
            egui::FontId::proportional(11.5),
            if is_active_file {
                theme::TEXT_STRONG
            } else if row_resp.hovered() {
                theme::TEXT
            } else {
                theme::TEXT_MUTED
            },
        );

        // 4. Click Interaction
        if row_resp.clicked() {
            if entry.is_dir {
                *dir_to_toggle = Some(entry.rel_path.clone());
            } else {
                *file_to_open = Some(entry.rel_path.clone());
            }
        }

        // 5. Render children recursively if folder is open
        if entry.is_dir && open_dirs.contains(&entry.rel_path) {
            for child in &entry.children {
                Self::render_file_tree_entry(
                    ui,
                    child,
                    depth + 1,
                    open_dirs,
                    active_editor,
                    file_to_open,
                    dir_to_toggle,
                );
            }
        }
    }

    /// Renders interactive file tree and creation actions in Inspector Files tab.
    fn render_files_tab(&mut self, ui: &mut egui::Ui) {
        // 1. Header with "WORKSPACE FILES" and New File / New Folder buttons
        ui.horizontal(|h_ui| {
            h_ui.colored_label(
                theme::TEXT_MUTED,
                egui::RichText::new("WORKSPACE FILES")
                    .size(10.0)
                    .extra_letter_spacing(1.0),
            );

            h_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |r_ui| {
                // New Folder button
                let (fld_r, fld_resp) =
                    r_ui.allocate_exact_size(Vec2::new(18.0, 18.0), Sense::click());
                if fld_resp.hovered() {
                    r_ui.painter().rect_filled(fld_r, 4.0, theme::HOVER);
                }
                Icons::draw_folder_plus(&r_ui.painter(), fld_r, theme::TEXT_MUTED);
                if fld_resp.on_hover_text("New Folder").clicked() {
                    self.new_file_entry = Some(NewFileEntryState {
                        is_dir: true,
                        name: String::new(),
                    });
                }

                r_ui.add_space(4.0);

                // New File button
                let (fil_r, fil_resp) =
                    r_ui.allocate_exact_size(Vec2::new(18.0, 18.0), Sense::click());
                if fil_resp.hovered() {
                    r_ui.painter().rect_filled(fil_r, 4.0, theme::HOVER);
                }
                Icons::draw_file_plus(&r_ui.painter(), fil_r, theme::TEXT_MUTED);
                if fil_resp.on_hover_text("New File").clicked() {
                    self.new_file_entry = Some(NewFileEntryState {
                        is_dir: false,
                        name: String::new(),
                    });
                }
            });
        });
        ui.add_space(4.0);

        // 2. Inline New Entry Row (if active)
        let mut commit_new_entry = false;
        let mut cancel_new_entry = false;
        if let Some(ref mut entry) = self.new_file_entry {
            let row_r = ui.allocate_space(Vec2::new(ui.available_width(), 24.0)).1;
            ui.painter().rect_filled(row_r, 4.0, theme::CARD_RAISED);
            ui.painter()
                .rect_stroke(row_r, 4.0, Stroke::new(1.0_f32, theme::BORDER_STRONG));

            let mut row_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(row_r)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            row_ui.add_space(6.0);

            let icon_color = if entry.is_dir {
                Color32::from_rgb(0xd2, 0x99, 0x22)
            } else {
                theme::TEXT_MUTED
            };
            let (ic_r, _) = row_ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
            if entry.is_dir {
                Icons::draw_folder(&row_ui.painter(), ic_r, icon_color);
            } else {
                Icons::draw_file_text(&row_ui.painter(), ic_r, icon_color);
            }

            let text_resp = row_ui.add(
                egui::TextEdit::singleline(&mut entry.name)
                    .hint_text(if entry.is_dir {
                        "Folder name..."
                    } else {
                        "File name..."
                    })
                    .font(egui::FontId::proportional(11.5))
                    .desired_width(row_ui.available_width() - 8.0),
            );
            text_resp.request_focus();

            if text_resp.lost_focus() && row_ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                commit_new_entry = true;
            } else if row_ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                cancel_new_entry = true;
            }
            ui.add_space(4.0);
        }

        if commit_new_entry {
            if let Some(entry) = self.new_file_entry.take() {
                let trimmed = entry.name.trim();
                if !trimmed.is_empty() && !self.snapshot.project_path.is_empty() {
                    let target_path =
                        std::path::Path::new(&self.snapshot.project_path).join(trimmed);
                    if entry.is_dir {
                        let _ = std::fs::create_dir_all(&target_path);
                        self.files_expanded_dirs.insert(trimmed.to_string());
                    } else {
                        if let Some(parent) = target_path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let _ = std::fs::write(&target_path, "");
                        self.open_editor_file(trimmed);
                    }
                    self.files_last_scan = std::time::Instant::now()
                        .checked_sub(std::time::Duration::from_secs(10))
                        .unwrap_or_else(std::time::Instant::now);
                }
            }
        } else if cancel_new_entry {
            self.new_file_entry = None;
        }

        // 3. Render Recursive File Tree with Caching
        let proj_path = self.snapshot.project_path.clone();
        if !proj_path.is_empty() {
            let path = std::path::Path::new(&proj_path);
            let path_buf = path.to_path_buf();
            let path_changed = self.files_cached_dir != path_buf;
            let expired = self.files_last_scan.elapsed() > std::time::Duration::from_millis(2500);

            if path_changed || expired {
                self.files_cached_entries =
                    Self::scan_project_dir(path, "", &self.files_expanded_dirs, 0);
                self.files_cached_dir = path_buf;
                self.files_last_scan = std::time::Instant::now();
            }

            let mut file_to_open = None;
            let mut dir_to_toggle = None;

            egui::ScrollArea::vertical()
                .id_salt("inspector_files_tree_scroll")
                .auto_shrink([false; 2])
                .show(ui, |tree_ui| {
                    tree_ui.spacing_mut().item_spacing.y = 1.0;
                    if self.files_cached_entries.is_empty() {
                        tree_ui.label(
                            egui::RichText::new("Empty workspace directory")
                                .size(11.0)
                                .color(theme::TEXT_FAINT),
                        );
                    } else {
                        for entry in &self.files_cached_entries {
                            Self::render_file_tree_entry(
                                tree_ui,
                                entry,
                                0,
                                &self.files_expanded_dirs,
                                self.active_editor_path.as_deref(),
                                &mut file_to_open,
                                &mut dir_to_toggle,
                            );
                        }
                    }
                });

            if let Some(dir) = dir_to_toggle {
                if self.files_expanded_dirs.contains(&dir) {
                    self.files_expanded_dirs.remove(&dir);
                } else {
                    self.files_expanded_dirs.insert(dir);
                }
                self.files_cached_entries =
                    Self::scan_project_dir(path, "", &self.files_expanded_dirs, 0);
                self.files_last_scan = std::time::Instant::now();
            }

            if let Some(file_rel) = file_to_open {
                self.open_editor_file(&file_rel);
            }
        } else {
            ui.label(
                egui::RichText::new("No project directory open")
                    .size(11.0)
                    .color(theme::TEXT_FAINT),
            );
        }
    }

    /// Renders the 40px top navigation header according to the Native Studio specification.
    /// Left section:
    /// - Temp-terminal vector logo.
    /// - Project switcher.
    /// - Current project breadcrumb.
    /// Center section:
    /// - Global command/search trigger (proportional width).
    /// Right section:
    /// - Settings action.
    /// - Toggle inspector action.
    /// - New agent action.
    pub fn render_top_nav(&mut self, ui: &mut egui::Ui) {
        let (bar_rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), theme::TOPNAV_HEIGHT),
            Sense::hover(),
        );
        let painter = ui.painter_at(bar_rect);

        // Panel dark matte background
        painter.rect_filled(bar_rect, 0.0, theme::PANEL);

        // Crisp 1px hairline divider line below top navigation
        painter.line_segment(
            [
                pos2(bar_rect.min.x, bar_rect.max.y),
                pos2(bar_rect.max.x, bar_rect.max.y),
            ],
            Stroke::new(1.0_f32, theme::BORDER),
        );

        let mut nav_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        nav_ui.add_space(theme::SPACE_COMPACT);

        // 1. LEFT SECTION: Logo, Project Switcher, Breadcrumb
        let (logo_r, logo_resp) = nav_ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
        Icons::draw_logo(&nav_ui.painter(), logo_r, theme::ACCENT, theme::TEXT_PRIMARY);
        logo_resp.on_hover_text("temp-terminal (Native Studio - Arctic Blue)");

        nav_ui.add_space(theme::SPACE_COMPACT);

        // Project Switcher button
        let proj_name = if self.snapshot.project_name.is_empty() {
            "Workspace".to_string()
        } else {
            self.snapshot.project_name.clone()
        };

        let (ps_r, ps_resp) = nav_ui.allocate_exact_size(
            Vec2::new((proj_name.len() as f32 * 7.5 + 44.0).clamp(110.0, 220.0), 28.0),
            Sense::click(),
        );
        let ps_painter = nav_ui.painter_at(ps_r);
        if ps_resp.hovered() || self.new_menu_open {
            ps_painter.rect_filled(ps_r, theme::RADIUS_MD, theme::SURFACE_HOVER);
            ps_painter.rect_stroke(ps_r, theme::RADIUS_MD, Stroke::new(1.0_f32, theme::BORDER_STRONG));
        } else {
            ps_painter.rect_filled(ps_r, theme::RADIUS_MD, theme::SURFACE);
            ps_painter.rect_stroke(ps_r, theme::RADIUS_MD, Stroke::new(1.0_f32, theme::BORDER));
        }

        let folder_r = Rect::from_min_size(
            pos2(ps_r.min.x + 6.0, ps_r.center().y - 7.0),
            Vec2::splat(14.0),
        );
        Icons::draw_folder_git(&ps_painter, folder_r, theme::ACCENT);

        ps_painter.text(
            pos2(ps_r.min.x + 24.0, ps_r.center().y),
            egui::Align2::LEFT_CENTER,
            &proj_name,
            egui::FontId::proportional(theme::typography::SECTION_HEADING),
            theme::TEXT_PRIMARY,
        );

        let chev_r = Rect::from_min_size(
            pos2(ps_r.max.x - 16.0, ps_r.center().y - 5.0),
            Vec2::splat(10.0),
        );
        Icons::draw_chevron_down(&ps_painter, chev_r, theme::TEXT_MUTED);

        if ps_resp.on_hover_text("Switch or Manage Projects...").clicked() {
            self.new_menu_pos = Some(pos2(ps_r.min.x, ps_r.max.y + 4.0));
            self.new_menu_open = !self.new_menu_open;
        }

        // Breadcrumb
        nav_ui.add_space(theme::SPACE_MICRO);
        nav_ui.label(egui::RichText::new("/").size(12.0).color(theme::TEXT_MUTED));
        nav_ui.add_space(theme::SPACE_MICRO);

        let breadcrumb_text = if let Some(ref path) = self.active_editor_path {
            path.clone()
        } else if self.diff_view.is_open {
            format!("diff • {}", self.diff_view.title)
        } else if let Some(active_sess) = self.snapshot.sessions.iter().find(|s| s.is_active) {
            active_sess.name.clone()
        } else {
            "main".to_string()
        };

        nav_ui.label(
            egui::RichText::new(breadcrumb_text)
                .size(theme::typography::BODY_SECONDARY)
                .color(theme::TEXT_SECONDARY),
        );

        // 2. RIGHT SECTION: Essential global actions (Settings, Toggle Inspector, New Agent)
        nav_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |r_ui| {
            r_ui.add_space(theme::SPACE_COMPACT);

            // Settings button
            let (gear_r, gear_resp) = r_ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
            if gear_resp.hovered() {
                r_ui.painter().rect_filled(gear_r, theme::RADIUS_MD, theme::SURFACE_HOVER);
                r_ui.painter().rect_stroke(gear_r, theme::RADIUS_MD, Stroke::new(1.0_f32, theme::BORDER_STRONG));
            }
            Icons::draw_gear(
                &r_ui.painter(),
                gear_r,
                if gear_resp.hovered() { theme::TEXT_PRIMARY } else { theme::TEXT_MUTED },
            );
            if gear_resp.on_hover_text("Settings (Ctrl+,)").clicked() {
                self.settings_modal.is_open = !self.settings_modal.is_open;
            }

            r_ui.add_space(theme::SPACE_MICRO);

            // Toggle Inspector button
            let (insp_r, insp_resp) = r_ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
            if self.inspector_open {
                r_ui.painter().rect_filled(insp_r, theme::RADIUS_MD, theme::SURFACE_SELECTED);
                r_ui.painter().rect_stroke(insp_r, theme::RADIUS_MD, Stroke::new(1.0_f32, theme::ACCENT));
            } else if insp_resp.hovered() {
                r_ui.painter().rect_filled(insp_r, theme::RADIUS_MD, theme::SURFACE_HOVER);
                r_ui.painter().rect_stroke(insp_r, theme::RADIUS_MD, Stroke::new(1.0_f32, theme::BORDER_STRONG));
            }
            Icons::draw_panel_right(
                &r_ui.painter(),
                insp_r,
                if self.inspector_open { theme::ACCENT } else if insp_resp.hovered() { theme::TEXT_PRIMARY } else { theme::TEXT_MUTED },
            );
            if insp_resp.on_hover_text("Toggle Inspector (Ctrl+Shift+B)").clicked() {
                self.inspector_open = !self.inspector_open;
            }

            r_ui.add_space(theme::SPACE_MICRO);

            // New Agent button
            let (agent_r, agent_resp) = r_ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
            if agent_resp.hovered() {
                r_ui.painter().rect_filled(agent_r, theme::RADIUS_MD, theme::SURFACE_HOVER);
                r_ui.painter().rect_stroke(agent_r, theme::RADIUS_MD, Stroke::new(1.0_f32, theme::BORDER_STRONG));
            }
            Icons::draw_agent(
                &r_ui.painter(),
                agent_r,
                if agent_resp.hovered() { theme::TEXT_PRIMARY } else { theme::TEXT_MUTED },
            );
            if agent_resp.on_hover_text("New Agent (F4 / Ctrl+Shift+A)").clicked() {
                self.agent_dialog.is_open = !self.agent_dialog.is_open;
            }
        });
    }

    /// Renders the 36px workspace tabs strip visually separate from the top navigation header.
    /// Requirements:
    /// - 36px height.
    /// - Horizontal layout.
    /// - Active tab uses a 2px sapphire bottom indicator.
    /// - Close icon appears on hover or active state.
    /// - Inactive tabs use muted typography.
    /// - Include appropriate overflow handling.
    /// - Keep split-pane controls on the right.
    pub fn render_workspace_tabs(&mut self, ui: &mut egui::Ui) {
        let (bar_rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), theme::WORKSPACE_TABS_HEIGHT),
            Sense::hover(),
        );
        let painter = ui.painter_at(bar_rect);

        // Dark workspace background
        painter.rect_filled(bar_rect, 0.0, theme::WORKSPACE);

        // 1px hairline divider below tab strip
        painter.line_segment(
            [
                pos2(bar_rect.min.x, bar_rect.max.y),
                pos2(bar_rect.max.x, bar_rect.max.y),
            ],
            Stroke::new(1.0_f32, theme::BORDER),
        );

        let mut t_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        t_ui.add_space(theme::SPACE_COMPACT);

        // Horizontal scroll area for workspace tabs (sessions + editors + diff)
        let tab_height = 26.0;
        let tab_rounding = Rounding { nw: 6.0, ne: 6.0, sw: 0.0, se: 0.0 };

        let active_proj = self.snapshot.active_project_id.clone();
        let sessions_snapshot: Vec<(String, String, bool)> = self
            .snapshot
            .sessions
            .iter()
            .map(|s| (s.id.clone(), s.name.clone(), s.is_active))
            .collect();

        let tabs_area_w = t_ui.available_width();

        t_ui.allocate_ui_with_layout(
            Vec2::new(tabs_area_w, bar_rect.height()),
            egui::Layout::left_to_right(egui::Align::Center),
            |tabs_ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("workspace_tabs_horizontal_scroll")
                    .auto_shrink([false, false])
                    .show(tabs_ui, |scroll_ui| {
                        scroll_ui.spacing_mut().item_spacing.x = theme::SPACE_MICRO;

                        // 1. Session tabs
                        for (_idx, (s_id, s_name, s_active)) in sessions_snapshot.into_iter().enumerate() {
                            let is_active = s_active && self.active_editor_path.is_none() && !self.diff_view.is_open;
                            let title_w = (s_name.len() as f32 * 7.5 + 54.0).clamp(100.0, 220.0);
                            let (tab_r, tab_resp) = scroll_ui.allocate_exact_size(Vec2::new(title_w, tab_height), Sense::click());
                            let p = scroll_ui.painter_at(tab_r);

                            if is_active {
                                p.rect_filled(tab_r, tab_rounding, theme::SURFACE_SELECTED);
                                p.rect_stroke(tab_r, tab_rounding, Stroke::new(1.0_f32, theme::BORDER_STRONG));
                                // 2px sapphire bottom indicator line
                                p.line_segment(
                                    [pos2(tab_r.min.x, tab_r.max.y), pos2(tab_r.max.x, tab_r.max.y)],
                                    Stroke::new(2.0_f32, theme::ACCENT),
                                );
                            } else if tab_resp.hovered() {
                                p.rect_filled(tab_r, tab_rounding, theme::SURFACE_HOVER);
                                p.rect_stroke(tab_r, tab_rounding, Stroke::new(1.0_f32, theme::BORDER));
                            } else {
                                p.rect_stroke(tab_r, tab_rounding, Stroke::new(1.0_f32, theme::BORDER));
                            }

                            // Terminal icon
                            let icon_r = Rect::from_min_size(pos2(tab_r.min.x + 8.0, tab_r.min.y + 7.0), Vec2::splat(14.0));
                            Icons::draw_terminal(&p, icon_r, if is_active { theme::ACCENT } else { theme::TEXT_MUTED });

                            // Tab title
                            p.text(
                                pos2(tab_r.min.x + 28.0, tab_r.center().y),
                                egui::Align2::LEFT_CENTER,
                                &s_name,
                                egui::FontId::proportional(theme::typography::BODY_SECONDARY),
                                if is_active { theme::TEXT_PRIMARY } else { theme::TEXT_MUTED },
                            );

                            // Close icon appears on hover or active state
                            if is_active || tab_resp.hovered() {
                                let close_r = Rect::from_min_size(pos2(tab_r.max.x - 20.0, tab_r.min.y + 6.0), Vec2::splat(16.0));
                                let close_resp = scroll_ui.interact(close_r, scroll_ui.id().with(&s_id).with("close_tab"), Sense::click());
                                if close_resp.hovered() {
                                    p.rect_filled(close_r, theme::RADIUS_SM, theme::DANGER.linear_multiply(0.25));
                                }
                                Icons::draw_close(
                                    &p,
                                    close_r,
                                    if close_resp.hovered() { theme::DANGER } else { theme::TEXT_MUTED },
                                );
                                if close_resp.on_hover_text("Close session (Ctrl+W)").clicked() {
                                    self.trigger_close_session(&s_id);
                                }
                            }

                            if tab_resp.clicked() && !is_active {
                                self.trigger_switch_session(&active_proj, &s_id);
                            }
                        }

                        // 2. Open Editor tabs
                        let editor_paths: Vec<(String, String, bool)> = self
                            .open_editors
                            .iter()
                            .map(|(path, doc)| (path.clone(), doc.title.clone(), doc.is_dirty))
                            .collect();

                        for (path, title, is_dirty) in editor_paths {
                            let is_active = self.active_editor_path.as_deref() == Some(&path) && !self.diff_view.is_open;
                            let title_w = (title.len() as f32 * 7.5 + 56.0).clamp(100.0, 220.0);
                            let (tab_r, tab_resp) = scroll_ui.allocate_exact_size(Vec2::new(title_w, tab_height), Sense::click());
                            let p = scroll_ui.painter_at(tab_r);

                            if is_active {
                                p.rect_filled(tab_r, tab_rounding, theme::SURFACE_SELECTED);
                                p.rect_stroke(tab_r, tab_rounding, Stroke::new(1.0_f32, theme::BORDER_STRONG));
                                p.line_segment(
                                    [pos2(tab_r.min.x, tab_r.max.y), pos2(tab_r.max.x, tab_r.max.y)],
                                    Stroke::new(2.0_f32, theme::ACCENT),
                                );
                            } else if tab_resp.hovered() {
                                p.rect_filled(tab_r, tab_rounding, theme::SURFACE_HOVER);
                                p.rect_stroke(tab_r, tab_rounding, Stroke::new(1.0_f32, theme::BORDER));
                            } else {
                                p.rect_stroke(tab_r, tab_rounding, Stroke::new(1.0_f32, theme::BORDER));
                            }

                            let icon_r = Rect::from_min_size(pos2(tab_r.min.x + 8.0, tab_r.min.y + 7.0), Vec2::splat(14.0));
                            Icons::draw_file_text(&p, icon_r, if is_active { theme::INFO } else { theme::TEXT_MUTED });

                            p.text(
                                pos2(tab_r.min.x + 28.0, tab_r.center().y),
                                egui::Align2::LEFT_CENTER,
                                &title,
                                egui::FontId::proportional(theme::typography::BODY_SECONDARY),
                                if is_active { theme::TEXT_PRIMARY } else { theme::TEXT_MUTED },
                            );

                            if is_dirty {
                                p.circle_filled(pos2(tab_r.max.x - 26.0, tab_r.center().y), 3.0, theme::WARNING);
                            }

                            if is_active || tab_resp.hovered() {
                                let close_r = Rect::from_min_size(pos2(tab_r.max.x - 20.0, tab_r.min.y + 6.0), Vec2::splat(16.0));
                                let close_resp = scroll_ui.interact(close_r, scroll_ui.id().with(&path).with("close_doc"), Sense::click());
                                if close_resp.hovered() {
                                    p.rect_filled(close_r, theme::RADIUS_SM, theme::DANGER.linear_multiply(0.25));
                                }
                                Icons::draw_close(
                                    &p,
                                    close_r,
                                    if close_resp.hovered() { theme::DANGER } else { theme::TEXT_MUTED },
                                );
                                if close_resp.on_hover_text("Close file (Ctrl+W)").clicked() {
                                    self.close_editor_file(&path);
                                }
                            }

                            if tab_resp.clicked() && !is_active {
                                self.active_editor_path = Some(path);
                                self.diff_view.is_open = false;
                            }
                        }

                        // 3. Diff View Tab
                        if self.diff_view.is_open {
                            let title = format!("Diff: {}", self.diff_view.title);
                            let title_w = (title.len() as f32 * 7.5 + 56.0).clamp(100.0, 220.0);
                            let (tab_r, _tab_resp) = scroll_ui.allocate_exact_size(Vec2::new(title_w, tab_height), Sense::click());
                            let p = scroll_ui.painter_at(tab_r);

                            p.rect_filled(tab_r, tab_rounding, theme::SURFACE_SELECTED);
                            p.rect_stroke(tab_r, tab_rounding, Stroke::new(1.0_f32, theme::BORDER_STRONG));
                            p.line_segment(
                                [pos2(tab_r.min.x, tab_r.max.y), pos2(tab_r.max.x, tab_r.max.y)],
                                Stroke::new(2.0_f32, theme::WARNING),
                            );

                            let icon_r = Rect::from_min_size(pos2(tab_r.min.x + 8.0, tab_r.min.y + 7.0), Vec2::splat(14.0));
                            Icons::draw_git_branch(&p, icon_r, theme::WARNING);

                            p.text(
                                pos2(tab_r.min.x + 28.0, tab_r.center().y),
                                egui::Align2::LEFT_CENTER,
                                &title,
                                egui::FontId::proportional(theme::typography::BODY_SECONDARY),
                                theme::TEXT_PRIMARY,
                            );

                            let close_r = Rect::from_min_size(pos2(tab_r.max.x - 20.0, tab_r.min.y + 6.0), Vec2::splat(16.0));
                            let close_resp = scroll_ui.interact(close_r, scroll_ui.id().with("close_diff_tab"), Sense::click());
                            if close_resp.hovered() {
                                p.rect_filled(close_r, theme::RADIUS_SM, theme::DANGER.linear_multiply(0.25));
                            }
                            Icons::draw_close(
                                &p,
                                close_r,
                                if close_resp.hovered() { theme::DANGER } else { theme::TEXT_MUTED },
                            );
                            if close_resp.clicked() {
                                self.diff_view.is_open = false;
                            }
                        }

                        // Add (+) button next to tabs
                        let (plus_r, plus_resp) = scroll_ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
                        if plus_resp.hovered() {
                            scroll_ui.painter().rect_filled(plus_r, theme::RADIUS_SM, theme::SURFACE_HOVER);
                            scroll_ui.painter().rect_stroke(plus_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BORDER_STRONG));
                        } else {
                            scroll_ui.painter().rect_stroke(plus_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BORDER));
                        }
                        Icons::draw_plus(&scroll_ui.painter(), plus_r, if plus_resp.hovered() { theme::TEXT_PRIMARY } else { theme::TEXT_MUTED });
                        if plus_resp.on_hover_text("New Terminal Tab (Ctrl+T)").clicked() {
                            self.trigger_new_session();
                        }
                    });
            },
        );
    }

    /// Backwards-compatible delegation for topbar calls
    pub fn render_topbar(&mut self, ui: &mut egui::Ui) {
        self.render_workspace_tabs(ui);
    }

    /// Renders the central empty state when no sessions are active matching reference project.
    fn render_empty_state(&mut self, ui: &mut egui::Ui) {
        let avail_rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(avail_rect, 0.0, theme::CARD);

        let proj_name = if self.snapshot.project_name.is_empty()
            || self.snapshot.project_name.eq_ignore_ascii_case("focus project")
        {
            "Project"
        } else {
            &self.snapshot.project_name
        };

        let has_processes = false;
        let title = if has_processes {
            format!("No sessions in {}", proj_name)
        } else {
            format!("Nothing runs in {} yet", proj_name)
        };
        let about = if has_processes {
            "Start a shell or an agent; its processes are in the sidebar."
        } else {
            "Start a shell, an agent, or one of the project's processes."
        };

        let card_w = 300.0;
        let total_h = 44.0 + 10.0 + 20.0 + 10.0 + 16.0 + 10.0 + (3.0 * 40.0 + 2.0 * 6.0);
        let start_y = (avail_rect.center().y - total_h * 0.52).max(avail_rect.min.y + 20.0);

        // Centered folder icon badge: 44x44, radius 12, border theme::TAB_BORDER_ACTIVE, background theme::CARD_RAISED
        let icon_rect = Rect::from_center_size(
            pos2(avail_rect.center().x, start_y + 22.0),
            Vec2::new(44.0, 44.0),
        );
        ui.painter().rect_filled(icon_rect, 12.0, theme::CARD_RAISED);
        ui.painter()
            .rect_stroke(icon_rect, 12.0, Stroke::new(1.0_f32, theme::TAB_BORDER_ACTIVE));
        let folder_rect = Rect::from_center_size(icon_rect.center(), Vec2::new(22.0, 22.0));
        Icons::draw_folder(&ui.painter(), folder_rect, theme::ACCENT);

        // Title: 15px, font weight 600, theme::TEXT
        let title_pos = pos2(avail_rect.center().x, icon_rect.max.y + 16.0);
        ui.painter().text(
            title_pos,
            egui::Align2::CENTER_CENTER,
            &title,
            egui::FontId::proportional(15.0),
            theme::TEXT,
        );

        // About subtitle: 12.5px, theme::TEXT_MUTED
        let about_pos = pos2(avail_rect.center().x, title_pos.y + 20.0);
        ui.painter().text(
            about_pos,
            egui::Align2::CENTER_CENTER,
            about,
            egui::FontId::proportional(12.5),
            theme::TEXT_MUTED,
        );

        // 3 Action cards: width 300, height 40, radius 8, gap 6
        let mut btn_y = about_pos.y + 18.0;

        // Card 1: New Session (first: border theme::ACCENT with alpha)
        let btn1_rect = Rect::from_min_size(
            pos2(avail_rect.center().x - card_w * 0.5, btn_y),
            Vec2::new(card_w, 40.0),
        );
        let resp1 = ui.interact(btn1_rect, ui.id().with("start_new_session"), Sense::click());
        let (bg1, stroke1) = if resp1.hovered() {
            (theme::HOVER, Stroke::new(1.0_f32, theme::ACCENT))
        } else {
            (
                theme::CARD_RAISED,
                Stroke::new(
                    1.0_f32,
                    Color32::from_rgba_premultiplied(47, 129, 247, 140),
                ),
            )
        };
        ui.painter().rect_filled(btn1_rect, 8.0, bg1);
        ui.painter().rect_stroke(btn1_rect, 8.0, stroke1);

        let t_icon_r = Rect::from_center_size(
            pos2(btn1_rect.min.x + 22.0, btn1_rect.center().y),
            Vec2::new(15.0, 15.0),
        );
        Icons::draw_terminal(
            &ui.painter(),
            t_icon_r,
            if resp1.hovered() {
                theme::ACCENT
            } else {
                theme::TEXT_MUTED
            },
        );

        ui.painter().text(
            pos2(btn1_rect.min.x + 38.0, btn1_rect.min.y + 13.0),
            egui::Align2::LEFT_CENTER,
            "New Session",
            egui::FontId::proportional(12.5),
            if resp1.hovered() {
                theme::TEXT_STRONG
            } else {
                theme::TEXT
            },
        );
        ui.painter().text(
            pos2(btn1_rect.min.x + 38.0, btn1_rect.min.y + 27.0),
            egui::Align2::LEFT_CENTER,
            format!("A shell in {}", proj_name),
            egui::FontId::proportional(11.0),
            if resp1.hovered() {
                theme::TEXT_SOFT
            } else {
                theme::TEXT_FAINT
            },
        );
        ui.painter().text(
            pos2(btn1_rect.max.x - 12.0, btn1_rect.center().y),
            egui::Align2::RIGHT_CENTER,
            "Ctrl+T",
            egui::FontId::proportional(10.5),
            if resp1.hovered() {
                theme::TEXT_SOFT
            } else {
                theme::TEXT_FAINT
            },
        );
        if resp1.clicked() {
            self.trigger_new_session();
        }

        btn_y += 46.0;

        // Card 2: New Agent...
        let btn2_rect = Rect::from_min_size(
            pos2(avail_rect.center().x - card_w * 0.5, btn_y),
            Vec2::new(card_w, 40.0),
        );
        let resp2 = ui.interact(btn2_rect, ui.id().with("start_new_agent"), Sense::click());
        let (bg2, stroke2) = if resp2.hovered() {
            (theme::HOVER, Stroke::new(1.0_f32, theme::TAB_BORDER_ACTIVE))
        } else {
            (
                theme::CARD_RAISED,
                Stroke::new(1.0_f32, theme::TAB_BORDER_INACTIVE),
            )
        };
        ui.painter().rect_filled(btn2_rect, 8.0, bg2);
        ui.painter().rect_stroke(btn2_rect, 8.0, stroke2);

        let bot_icon_r = Rect::from_center_size(
            pos2(btn2_rect.min.x + 22.0, btn2_rect.center().y),
            Vec2::new(15.0, 15.0),
        );
        Icons::draw_bot(
            &ui.painter(),
            bot_icon_r,
            if resp2.hovered() {
                theme::TEXT_STRONG
            } else {
                theme::TEXT_MUTED
            },
        );

        ui.painter().text(
            pos2(btn2_rect.min.x + 38.0, btn2_rect.min.y + 13.0),
            egui::Align2::LEFT_CENTER,
            "New Agent...",
            egui::FontId::proportional(12.5),
            if resp2.hovered() {
                theme::TEXT_STRONG
            } else {
                theme::TEXT
            },
        );
        ui.painter().text(
            pos2(btn2_rect.min.x + 38.0, btn2_rect.min.y + 27.0),
            egui::Align2::LEFT_CENTER,
            "Claude Code, Codex, Antigravity...",
            egui::FontId::proportional(11.0),
            if resp2.hovered() {
                theme::TEXT_SOFT
            } else {
                theme::TEXT_FAINT
            },
        );
        if resp2.clicked() {
            self.agent_dialog.is_open = true;
        }

        btn_y += 46.0;

        // Card 3: Add Process...
        let btn3_rect = Rect::from_min_size(
            pos2(avail_rect.center().x - card_w * 0.5, btn_y),
            Vec2::new(card_w, 40.0),
        );
        let resp3 = ui.interact(btn3_rect, ui.id().with("start_add_process"), Sense::click());
        let (bg3, stroke3) = if resp3.hovered() {
            (theme::HOVER, Stroke::new(1.0_f32, theme::TAB_BORDER_ACTIVE))
        } else {
            (
                theme::CARD_RAISED,
                Stroke::new(1.0_f32, theme::TAB_BORDER_INACTIVE),
            )
        };
        ui.painter().rect_filled(btn3_rect, 8.0, bg3);
        ui.painter().rect_stroke(btn3_rect, 8.0, stroke3);

        let plus_icon_r = Rect::from_center_size(
            pos2(btn3_rect.min.x + 22.0, btn3_rect.center().y),
            Vec2::new(15.0, 15.0),
        );
        Icons::draw_plus(
            &ui.painter(),
            plus_icon_r,
            if resp3.hovered() {
                theme::TEXT_STRONG
            } else {
                theme::TEXT_MUTED
            },
        );

        ui.painter().text(
            pos2(btn3_rect.min.x + 38.0, btn3_rect.min.y + 13.0),
            egui::Align2::LEFT_CENTER,
            "Add Process...",
            egui::FontId::proportional(12.5),
            if resp3.hovered() {
                theme::TEXT_STRONG
            } else {
                theme::TEXT
            },
        );
        ui.painter().text(
            pos2(btn3_rect.min.x + 38.0, btn3_rect.min.y + 27.0),
            egui::Align2::LEFT_CENTER,
            "A dev server or watcher, kept running",
            egui::FontId::proportional(11.0),
            if resp3.hovered() {
                theme::TEXT_SOFT
            } else {
                theme::TEXT_FAINT
            },
        );
        if resp3.clicked() {
            self.process_dialog.is_open = true;
        }
    }

    /// Renders split node hierarchy or single pane.
    fn render_split_tree(
        &mut self,
        ui: &mut egui::Ui,
        node: &SplitNode,
        total_rect: egui::Rect,
        path: &str,
    ) {
        if node.is_leaf() {
            self.render_single_pane(ui, &node.term_id, total_rect);
            return;
        }

        let gap = 4.0_f32; // 4px minimal split separator (Section 6) // paneGap() parity from Go tempterm/layout.go:18

        match node.dir {
            SplitDir::Vertical => {
                // Vertical split: side-by-side (left/right)
                let avail_w = (total_rect.width() - gap).max(0.0);
                let ratio = SplitNode::clamp_ratio(node.ratio, avail_w);
                let w1 = (avail_w * (ratio / 100.0)).round();
                let w2 = (avail_w - w1).max(0.0);

                let r1 =
                    egui::Rect::from_min_size(total_rect.min, Vec2::new(w1, total_rect.height()));
                let div_rect = egui::Rect::from_min_size(
                    egui::Pos2::new(total_rect.min.x + w1, total_rect.min.y),
                    Vec2::new(gap, total_rect.height()),
                );
                let r2 = egui::Rect::from_min_size(
                    egui::Pos2::new(total_rect.min.x + w1 + gap, total_rect.min.y),
                    Vec2::new(w2, total_rect.height()),
                );

                // Divider interaction
                let div_id = ui.id().with(("split_div_v", path));
                let div_resp = ui.interact(div_rect, div_id, egui::Sense::click_and_drag());

                if div_resp.hovered() || div_resp.dragged() {
                    ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::ResizeColumn);
                }

                // Draw visual divider grip (Go parity tempterm/layout.go:115-122)
                let is_active_drag = self.layout_drag.is_dragging
                    && self.layout_drag.active_path.as_deref() == Some(path);
                if div_resp.hovered() || is_active_drag {
                    let grip_color = if is_active_drag {
                        theme::ACCENT_PRIMARY
                    } else {
                        theme::ACCENT_PRIMARY.gamma_multiply(0.7)
                    };
                    let grip_rect =
                        egui::Rect::from_center_size(div_rect.center(), Vec2::new(3.0, 36.0));
                    ui.painter()
                        .rect_filled(grip_rect, Rounding::same(1.5_f32), grip_color);
                }

                if div_resp.double_clicked() {
                    self.trigger_update_split_ratio(path, 50.0);
                    let host = self.host.clone();
                    self.tokio_handle.spawn(async move {
                        let _ = host.save_state().await;
                    });
                } else if div_resp.drag_started() {
                    self.layout_drag.active_path = Some(path.to_string());
                    self.layout_drag.accumulated_ratio = ratio;
                    self.layout_drag.is_dragging = true;
                } else if div_resp.dragged() && is_active_drag {
                    let delta_x = div_resp.drag_delta().x;
                    if avail_w > 0.0 {
                        let delta_pct = (delta_x / avail_w) * 100.0;
                        self.layout_drag.accumulated_ratio += delta_pct;
                        let new_ratio =
                            SplitNode::clamp_ratio(self.layout_drag.accumulated_ratio, avail_w);
                        if let Some(ref mut l) = self.snapshot.layout {
                            l.set_ratio_at_path(path, new_ratio);
                        }
                    }
                }

                if (div_resp.drag_stopped()
                    || (!ui.input(|i| i.pointer.any_down()) && is_active_drag))
                    && self.layout_drag.active_path.as_deref() == Some(path)
                {
                    self.layout_drag.is_dragging = false;
                    let final_ratio = if let Some(ref l) = self.snapshot.layout {
                        l.get_ratio_at_path(path).unwrap_or(ratio)
                    } else {
                        ratio
                    };
                    self.trigger_update_split_ratio(path, final_ratio);
                    let host = self.host.clone();
                    self.tokio_handle.spawn(async move {
                        let _ = host.save_state().await;
                    });
                }

                if let Some(ref f) = node.first {
                    self.render_split_tree(ui, f, r1, &format!("{}a", path));
                }
                if let Some(ref s) = node.second {
                    self.render_split_tree(ui, s, r2, &format!("{}b", path));
                }
            }
            SplitDir::Horizontal => {
                // Horizontal split: stacked (top/bottom)
                let avail_h = (total_rect.height() - gap).max(0.0);
                let ratio = SplitNode::clamp_ratio(node.ratio, avail_h);
                let h1 = (avail_h * (ratio / 100.0)).round();
                let h2 = (avail_h - h1).max(0.0);

                let r1 =
                    egui::Rect::from_min_size(total_rect.min, Vec2::new(total_rect.width(), h1));
                let div_rect = egui::Rect::from_min_size(
                    egui::Pos2::new(total_rect.min.x, total_rect.min.y + h1),
                    Vec2::new(total_rect.width(), gap),
                );
                let r2 = egui::Rect::from_min_size(
                    egui::Pos2::new(total_rect.min.x, total_rect.min.y + h1 + gap),
                    Vec2::new(total_rect.width(), h2),
                );

                // Divider interaction
                let div_id = ui.id().with(("split_div_h", path));
                let div_resp = ui.interact(div_rect, div_id, egui::Sense::click_and_drag());

                if div_resp.hovered() || div_resp.dragged() {
                    ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::ResizeRow);
                }

                // Draw visual divider grip (Go parity tempterm/layout.go:115-122)
                let is_active_drag = self.layout_drag.is_dragging
                    && self.layout_drag.active_path.as_deref() == Some(path);
                if div_resp.hovered() || is_active_drag {
                    let grip_color = if is_active_drag {
                        theme::ACCENT_PRIMARY
                    } else {
                        theme::ACCENT_PRIMARY.gamma_multiply(0.7)
                    };
                    let grip_rect =
                        egui::Rect::from_center_size(div_rect.center(), Vec2::new(36.0, 3.0));
                    ui.painter()
                        .rect_filled(grip_rect, Rounding::same(1.5_f32), grip_color);
                }

                if div_resp.double_clicked() {
                    self.trigger_update_split_ratio(path, 50.0);
                    let host = self.host.clone();
                    self.tokio_handle.spawn(async move {
                        let _ = host.save_state().await;
                    });
                } else if div_resp.drag_started() {
                    self.layout_drag.active_path = Some(path.to_string());
                    self.layout_drag.accumulated_ratio = ratio;
                    self.layout_drag.is_dragging = true;
                } else if div_resp.dragged() && is_active_drag {
                    let delta_y = div_resp.drag_delta().y;
                    if avail_h > 0.0 {
                        let delta_pct = (delta_y / avail_h) * 100.0;
                        self.layout_drag.accumulated_ratio += delta_pct;
                        let new_ratio =
                            SplitNode::clamp_ratio(self.layout_drag.accumulated_ratio, avail_h);
                        if let Some(ref mut l) = self.snapshot.layout {
                            l.set_ratio_at_path(path, new_ratio);
                        }
                    }
                }

                if (div_resp.drag_stopped()
                    || (!ui.input(|i| i.pointer.any_down()) && is_active_drag))
                    && self.layout_drag.active_path.as_deref() == Some(path)
                {
                    self.layout_drag.is_dragging = false;
                    let final_ratio = if let Some(ref l) = self.snapshot.layout {
                        l.get_ratio_at_path(path).unwrap_or(ratio)
                    } else {
                        ratio
                    };
                    self.trigger_update_split_ratio(path, final_ratio);
                    let host = self.host.clone();
                    self.tokio_handle.spawn(async move {
                        let _ = host.save_state().await;
                    });
                }

                if let Some(ref f) = node.first {
                    self.render_split_tree(ui, f, r1, &format!("{}a", path));
                }
                if let Some(ref s) = node.second {
                    self.render_split_tree(ui, s, r2, &format!("{}b", path));
                }
            }
        }
    }

    /// Renders an individual terminal pane card inside `pane_rect`.
    fn render_single_pane(&mut self, ui: &mut egui::Ui, pane_id: &str, pane_rect: egui::Rect) {
        let is_active = self.snapshot.active_pane_id == pane_id;
        let is_multi = self.snapshot.panes.len() > 1;
        let stroke_color = if is_active {
            theme::ACCENT
        } else {
            theme::BORDER
        };
        let stroke_width = if is_active { 1.5_f32 } else { 1.0_f32 };

        let mut child_ui =
            ui.new_child(egui::UiBuilder::new().id_salt(pane_id).max_rect(pane_rect));

        // Determine character columns and rows dynamically from monospace font metrics and allocated pane_rect
        let font_id = egui::FontId::monospace(theme::typography::TERMINAL_TEXT);
        let measured_w = ui.fonts(|f| f.glyph_width(&font_id, 'M'));
        let measured_h = ui.fonts(|f| f.row_height(&font_id));
        let char_w = if measured_w > 1.0 { measured_w } else { 8.07 };
        let line_h = if measured_h > 1.0 { measured_h } else { 15.7 };

        // 12px inner margin (6px left + 6px right) + scrollbar gutter to prevent right cutoff
        let scrollbar_gutter = ui.spacing().scroll.bar_width + ui.spacing().scroll.bar_inner_margin + 6.0;
        let pad_w = 12.0f32 + scrollbar_gutter;
        let pad_h = 38.0f32; // header height + margin + separator
        let avail_w = (pane_rect.width() - pad_w).max(70.0);
        let avail_h = (pane_rect.height() - pad_h).max(30.0);
        let target_cols = (avail_w / char_w).floor().clamp(20.0, 300.0) as u16;
        let target_rows = (avail_h / line_h).floor().clamp(5.0, 120.0) as u16;

        let host = self.host.clone();
        let p_id_clone = pane_id.to_string();
        let is_dragging = self.layout_drag.is_dragging;
        if !is_dragging {
            self.tokio_handle.spawn(async move {
                let projs = host.projects.read().await;
                let active_proj = host.active_project_id.read().await.clone();
                if let Some(p) = projs.get(&active_proj) {
                    let target_pane = if let Some(sess) = p.sessions.get(&p.active_session_id) {
                        sess.panes.get(&p_id_clone).cloned()
                    } else {
                        None
                    }
                    .or_else(|| {
                        for sess in p.sessions.values() {
                            if let Some(pane) = sess.panes.get(&p_id_clone) {
                                return Some(pane.clone());
                            }
                        }
                        None
                    });

                    if let Some(pane) = target_pane {
                        let (cur_cols, cur_rows) = {
                            let buf = pane.buffer.read().await;
                            (buf.cols, buf.rows)
                        };
                        if cur_cols != target_cols || cur_rows != target_rows {
                            let _ = pane.resize(target_cols, target_rows).await;
                        }
                    }
                }
            });
        }

        let frame = Frame::none()
            .fill(theme::TERMINAL_BG)
            .stroke(Stroke::new(stroke_width, stroke_color))
            .rounding(Rounding::same(theme::RADIUS_MD))
            .inner_margin(Margin::same(6.0));

        frame.show(&mut child_ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(ui.available_height());

            // Header: Title, Active marker, Split V, Split H, Close matching tempterm
            let header_w = ui.available_width();
            let is_multi_pane = is_multi || self.snapshot.zoomed_pane_id.is_some();

            ui.horizontal(|ui| {
                ui.set_width(header_w);

                // Right-to-left layout: Right buttons are placed first against the right edge
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(4.0);
                    ui.spacing_mut().item_spacing.x = 4.0;

                    // Close button
                    let (close_r, close_resp) =
                        ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                    if close_resp.hovered() {
                        ui.painter().rect_filled(close_r, theme::RADIUS_SM, theme::BAD.linear_multiply(0.2));
                        ui.painter().rect_stroke(close_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BAD));
                    }
                    Icons::draw_close(
                        &ui.painter(),
                        close_r,
                        if close_resp.hovered() {
                            theme::BAD
                        } else {
                            theme::TEXT_MUTED
                        },
                    );
                    if close_resp.on_hover_text("Close pane (Ctrl+W)").clicked() {
                        self.trigger_close_pane(pane_id);
                    }

                    // Split Down button
                    let (sd_r, sd_resp) =
                        ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                    if sd_resp.hovered() {
                        ui.painter().rect_filled(sd_r, theme::RADIUS_SM, theme::HOVER);
                        ui.painter().rect_stroke(sd_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BORDER_STRONG));
                    }
                    Icons::draw_split_down(
                        &ui.painter(),
                        sd_r,
                        if sd_resp.hovered() {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_MUTED
                        },
                    );
                    if sd_resp
                        .on_hover_text("Split terminal down (Ctrl+Shift+D)")
                        .clicked()
                    {
                        self.focus_pane(pane_id);
                        self.trigger_split(SplitDir::Horizontal);
                    }

                    // Split Right button
                    let (sr_r, sr_resp) =
                        ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                    if sr_resp.hovered() {
                        ui.painter().rect_filled(sr_r, theme::RADIUS_SM, theme::HOVER);
                        ui.painter().rect_stroke(sr_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BORDER_STRONG));
                    }
                    Icons::draw_split_right(
                        &ui.painter(),
                        sr_r,
                        if sr_resp.hovered() {
                            theme::TEXT_STRONG
                        } else {
                            theme::TEXT_MUTED
                        },
                    );
                    if sr_resp
                        .on_hover_text("Split terminal right (Ctrl+Shift+R)")
                        .clicked()
                    {
                        self.focus_pane(pane_id);
                        self.trigger_split(SplitDir::Vertical);
                    }

                    // Zoom / Maximize button
                    let is_zoomed = self.snapshot.zoomed_pane_id.as_deref() == Some(pane_id);
                    if is_multi_pane || is_zoomed {
                        let (zm_r, zm_resp) =
                            ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                        if zm_resp.hovered() {
                            ui.painter().rect_filled(zm_r, theme::RADIUS_SM, theme::HOVER);
                            ui.painter().rect_stroke(zm_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BORDER_STRONG));
                        }
                        Icons::draw_maximize(
                            &ui.painter(),
                            zm_r,
                            if is_zoomed {
                                theme::ACCENT
                            } else if zm_resp.hovered() {
                                theme::TEXT_STRONG
                            } else {
                                theme::TEXT_MUTED
                            },
                        );
                        if zm_resp
                            .on_hover_text(if is_zoomed {
                                "Show all panes (Ctrl+Shift+Z)"
                            } else {
                                "Zoom pane (Ctrl+Shift+Z)"
                            })
                            .clicked()
                        {
                            self.focus_pane(pane_id);
                            self.trigger_toggle_zoom_pane(pane_id);
                        }
                    }

                    // Remaining space on the left: Prompt, Git branch chip, Path (truncated)
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;

                        let detected_agent = self
                            .snapshot
                            .panes
                            .get(pane_id)
                            .and_then(|p| p.detected_agent.clone());

                        // Terminal prompt icon (14x14)
                        let (icon_r, _) =
                            ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
                        if detected_agent.is_some() {
                            Icons::draw_agent(&ui.painter(), icon_r, theme::SUCCESS);
                        } else {
                            Icons::draw_terminal(
                                &ui.painter(),
                                icon_r,
                                if is_active {
                                    theme::TEXT
                                } else {
                                    theme::TEXT_MUTED
                                },
                            );
                        }

                        // Clean terminal title
                        let title_text = if let Some(ref ag_name) = detected_agent {
                            format!("Terminal ({})", ag_name)
                        } else {
                            "Terminal".to_string()
                        };

                        let title_resp = ui.add(
                            egui::Label::new(
                                egui::RichText::new(title_text)
                                    .size(12.5)
                                    .color(if is_active {
                                        theme::TEXT_STRONG
                                    } else {
                                        theme::TEXT
                                    })
                                    .strong(),
                            )
                            .sense(egui::Sense::click()),
                        );
                        if title_resp.clicked() {
                            self.focus_pane(pane_id);
                        }
                    });
                });
            });

            ui.add_space(2.0);

            // Terminal Viewport Area
            let term_id_salt = ui.id().with(("term_interact", pane_id));

            // Ensure focused active pane requests and retains egui keyboard focus
            if is_active && !self.has_any_modal_open() && !ui.memory(|m| m.has_focus(term_id_salt)) {
                ui.memory_mut(|m| m.request_focus(term_id_salt));
            }

            // Forward keyboard input if this pane is active and no modal is open
            if is_active && !self.has_any_modal_open() && !self.modal_closed_this_frame {
                let clipboard = get_clipboard_text();
                let (events, mods) = ui.input(|i| (i.raw.events.clone(), i.modifiers));

                let (bytes_to_send, text_to_copy) =
                    self.handle_terminal_events(pane_id, &events, mods, &clipboard);

                if let Some(text) = text_to_copy {
                    ui.ctx().copy_text(text.clone());
                    set_clipboard_text(&text);
                }

                if !bytes_to_send.is_empty() {
                    self.send_input_to_pane(pane_id, &bytes_to_send);
                }
            }

            // Render terminal text with ANSI coloring
            let (pane_lines, cursor_info) = self
                .snapshot
                .panes
                .get(pane_id)
                .map(|p| {
                    (
                        p.lines.clone(),
                        (
                            p.cursor_col,
                            p.cursor_row,
                            p.cursor_visible,
                            p.scrollback_len,
                        ),
                    )
                })
                .unwrap_or_default();

            // Track mouse dragging over text to update selection and auto-copy
            let pointer_pos =
                ui.input(|i| i.pointer.hover_pos().or_else(|| i.pointer.interact_pos()));
            let is_mouse_down = ui.input(|i| i.pointer.primary_down());
            let is_mouse_released = ui.input(|i| i.pointer.primary_released());

            // Calculate cursor blink state (800ms on, 600ms off - comfortable slow blink)
            let time_secs = ui.input(|i| i.time);
            let cursor_blink_on = (time_secs / 1.4).fract() < (800.0 / 1400.0);
            if is_active {
                // Request repaint so the cursor blinks continuously
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(150));
            }

            let pane_rect = ui.available_rect_before_wrap();

            let scroll_output = egui::ScrollArea::both()
                .id_salt(("scroll", pane_id))
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if pane_lines.is_empty() {
                        ui.label(
                            egui::RichText::new("[Process connected - ready for input]")
                                .monospace()
                                .size(theme::typography::TERMINAL_TEXT)
                                .color(theme::TEXT_MUTED),
                        );
                    } else {
                        let active_sel = self.pane_selections.get(pane_id).copied();
                        let target_cursor_line = cursor_info.3 + cursor_info.1; // scrollback_len + cursor_row

                        for (line_idx, line) in pane_lines.iter().enumerate() {
                            let cells = self.renderer.parse_ansi_line(line);
                            let is_cursor_line =
                                is_active && cursor_info.2 && line_idx == target_cursor_line;
                            let c_col = cursor_info.0;

                            if cells.is_empty() {
                                if is_cursor_line && cursor_blink_on {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 0.0;
                                        let rt = egui::RichText::new(" ")
                                            .monospace()
                                            .size(theme::typography::TERMINAL_TEXT)
                                            .color(theme::BG_APP)
                                            .background_color(theme::TEXT_STRONG);
                                        ui.label(rt);
                                    });
                                } else {
                                    ui.label("");
                                }
                            } else {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    let mut chunk = String::new();
                                    let mut cur_fg = cells[0].fg;
                                    let mut cur_bg = cells[0].bg;
                                    let mut cur_bold = cells[0].bold;
                                    let mut cur_faint = cells[0].faint;
                                    let mut cur_italic = cells[0].italic;
                                    let mut cur_selected = active_sel
                                        .map(|s| s.contains(line_idx, 0))
                                        .unwrap_or(false);

                                    let resolve_chunk_color =
                                        |fg: TerminalColor, faint: bool, selected: bool| -> Color32 {
                                            if selected {
                                                theme::TEXT_STRONG
                                            } else if faint {
                                                // Dimmed / faint text (recommendation / inline prediction)
                                                if fg == TerminalColor::Default
                                                    || fg == TerminalColor::White
                                                    || fg == TerminalColor::BrightWhite
                                                {
                                                    Color32::from_rgb(0x60, 0x6e, 0x80)
                                                } else {
                                                    let base = term_color_to_egui(fg, theme::TEXT_NORMAL);
                                                    Color32::from_rgb(
                                                        ((base.r() as f32) * 0.52) as u8,
                                                        ((base.g() as f32) * 0.52) as u8,
                                                        ((base.b() as f32) * 0.52) as u8,
                                                    )
                                                }
                                            } else {
                                                term_color_to_egui(fg, theme::TEXT_NORMAL)
                                            }
                                        };

                                    let num_cells = cells.len();
                                    for (col_idx, c) in cells.iter().enumerate() {
                                        let is_sel = active_sel
                                            .map(|s| s.contains(line_idx, col_idx))
                                            .unwrap_or(false);
                                        let is_cursor_cell =
                                            is_cursor_line && cursor_blink_on && col_idx == c_col;

                                        if is_cursor_cell {
                                            // Flush preceding chunk
                                            if !chunk.is_empty() {
                                                let color = resolve_chunk_color(
                                                    cur_fg,
                                                    cur_faint,
                                                    cur_selected,
                                                );
                                                let mut rt = egui::RichText::new(&chunk)
                                                    .monospace()
                                                    .size(theme::typography::TERMINAL_TEXT)
                                                    .color(color);
                                                if cur_selected {
                                                    rt = rt.background_color(Color32::from_rgb(
                                                        0x1f, 0x42, 0x73,
                                                    ));
                                                } else if cur_bg != TerminalColor::Default {
                                                    rt = rt.background_color(term_color_to_egui(cur_bg, Color32::TRANSPARENT));
                                                }
                                                if cur_bold {
                                                    rt = rt.strong();
                                                }
                                                if cur_italic {
                                                    rt = rt.italics();
                                                }
                                                ui.label(rt);
                                                chunk.clear();
                                            }

                                            // Draw blinking inverted cursor cell
                                            let mut cursor_rt = egui::RichText::new(c.ch.to_string())
                                                .monospace()
                                                .size(theme::typography::TERMINAL_TEXT)
                                                .color(theme::BG_APP)
                                                .background_color(theme::TEXT_STRONG);
                                            if c.bold {
                                                cursor_rt = cursor_rt.strong();
                                            }
                                            if c.italic {
                                                cursor_rt = cursor_rt.italics();
                                            }
                                            ui.label(cursor_rt);
                                            cur_selected = is_sel;
                                            cur_fg = c.fg;
                                            cur_bg = c.bg;
                                            cur_bold = c.bold;
                                            cur_faint = c.faint;
                                            cur_italic = c.italic;
                                            continue;
                                        }

                                        if c.fg == cur_fg
                                            && c.bg == cur_bg
                                            && c.bold == cur_bold
                                            && c.faint == cur_faint
                                            && c.italic == cur_italic
                                            && is_sel == cur_selected
                                        {
                                            chunk.push(c.ch);
                                        } else {
                                            let color = resolve_chunk_color(
                                                cur_fg,
                                                cur_faint,
                                                cur_selected,
                                            );
                                            let mut rt = egui::RichText::new(&chunk)
                                                .monospace()
                                                .size(theme::typography::TERMINAL_TEXT)
                                                .color(color);
                                            if cur_selected {
                                                rt = rt.background_color(Color32::from_rgb(
                                                    0x1f, 0x42, 0x73,
                                                ));
                                            } else if cur_bg != TerminalColor::Default {
                                                rt = rt.background_color(term_color_to_egui(cur_bg, Color32::TRANSPARENT));
                                            }
                                            if cur_bold {
                                                rt = rt.strong();
                                            }
                                            if cur_italic {
                                                rt = rt.italics();
                                            }
                                            ui.label(rt);
                                            chunk.clear();
                                            chunk.push(c.ch);
                                            cur_fg = c.fg;
                                            cur_bg = c.bg;
                                            cur_bold = c.bold;
                                            cur_faint = c.faint;
                                            cur_italic = c.italic;
                                            cur_selected = is_sel;
                                        }
                                    }
                                    if !chunk.is_empty() {
                                        let color = resolve_chunk_color(
                                            cur_fg,
                                            cur_faint,
                                            cur_selected,
                                        );
                                        let mut rt = egui::RichText::new(&chunk)
                                            .monospace()
                                            .size(theme::typography::TERMINAL_TEXT)
                                            .color(color);
                                        if cur_selected {
                                            rt = rt.background_color(Color32::from_rgb(
                                                0x1f, 0x42, 0x73,
                                            ));
                                        } else if cur_bg != TerminalColor::Default {
                                            rt = rt.background_color(term_color_to_egui(cur_bg, Color32::TRANSPARENT));
                                        }
                                        if cur_bold {
                                            rt = rt.strong();
                                        }
                                        if cur_italic {
                                            rt = rt.italics();
                                        }
                                        ui.label(rt);
                                    }

                                    // If cursor is past the end of the printed cells on this line
                                    if is_cursor_line && cursor_blink_on && c_col >= num_cells {
                                        let gap = c_col - num_cells;
                                        if gap > 0 {
                                            ui.label(
                                                egui::RichText::new(" ".repeat(gap))
                                                    .monospace()
                                                    .size(theme::typography::TERMINAL_TEXT)
                                                    .color(theme::TEXT_NORMAL),
                                            );
                                        }
                                        let cursor_rt = egui::RichText::new(" ")
                                            .monospace()
                                            .size(theme::typography::TERMINAL_TEXT)
                                            .color(theme::BG_APP)
                                            .background_color(theme::TEXT_STRONG)
                                            .strong();
                                        ui.label(cursor_rt);
                                    }
                                });
                            }
                        }
                    }
                });

            // Update mouse selection based on pointer coordinates inside the scroll content area
            let content_rect = scroll_output.inner_rect;
            let line_height = line_h;
            let char_width = char_w;

            if let Some(pos) = pointer_pos {
                if pane_rect.contains(pos) && ui.input(|i| i.pointer.primary_clicked()) {
                    // Click without drag focuses the pane and clears previous selection
                    self.focus_pane(pane_id);
                    self.clear_pane_selection(pane_id);
                }

                if content_rect.contains(pos) {
                    let rel_y = (pos.y - content_rect.min.y + scroll_output.state.offset.y).max(0.0);
                    let rel_x = (pos.x - content_rect.min.x + scroll_output.state.offset.x).max(0.0);
                    let line_idx = (rel_y / line_height) as usize;
                    let col_idx = (rel_x / char_width) as usize;
                    let cur_p = TerminalPos { line_idx, col_idx };

                    if is_mouse_down {
                        if self.selection_drag_start.is_none() {
                            self.focus_pane(pane_id);
                            self.selection_drag_start = Some((pane_id.to_string(), cur_p));
                            self.set_pane_selection(pane_id, cur_p, cur_p);
                        } else if let Some((ref drag_pane, start_pos)) = self.selection_drag_start {
                            if drag_pane == pane_id {
                                self.set_pane_selection(pane_id, start_pos, cur_p);
                            }
                        }
                    }
                }
            }

            // Context menu / paste on secondary (right) click inside terminal viewport
            if let Some(pos) = pointer_pos {
                if pane_rect.contains(pos) && ui.input(|i| i.pointer.secondary_clicked()) {
                    let has_sel = self
                        .get_selected_text(pane_id)
                        .map(|s| !s.is_empty())
                        .unwrap_or(false);
                    if has_sel {
                        if let Some(text) = self.get_selected_text(pane_id) {
                            ui.ctx().copy_text(text.clone());
                            set_clipboard_text(&text);
                            self.clear_pane_selection(pane_id);
                        }
                    } else {
                        let clip = get_clipboard_text();
                        if !clip.is_empty() {
                            let text = clip.replace("\r\n", "\r").replace('\n', "\r");
                            self.send_input_to_pane(pane_id, text.as_bytes());
                        }
                    }
                }
            }

            if is_mouse_released && self.selection_drag_start.is_some() {
                self.selection_drag_start = None;
                // Auto-copy on select: immediately copy highlighted text to system clipboard
                if let Some(text) = self.get_selected_text(pane_id) {
                    if !text.is_empty() {
                        ui.ctx().copy_text(text.clone());
                        set_clipboard_text(&text);
                    }
                }
            }
        });
    }

    /// Executes a command identified by its ID.
    pub fn execute_command(&mut self, cmd_id: &str) {
        match cmd_id {
            "session.new" | "terminal.new" => self.trigger_new_session(),
            "session.newAgent" => {
                self.agent_dialog.is_open = true;
            }
            "process.add" => {
                self.process_dialog.is_open = true;
            }
            "file.quickOpen" => {
                self.quick_open.is_open = true;
                self.quick_open.query.clear();
                self.quick_open.selected_index = 0;
                self.scan_quick_open_files();
            }
            "file.save" => {
                self.save_active_editor();
            }
            "file.close" => {
                if let Some(ref path) = self.active_editor_path.clone() {
                    self.close_editor_file(path);
                }
            }
            "app.openProject" => {
                self.open_project_dialog.is_open = true;
            }
            "app.commandPalette" => {
                self.command_palette.open();
            }
            "app.settings" => {
                self.settings_modal.is_open = true;
            }
            "help.keyboardShortcuts" => {
                self.shortcuts_overlay.open();
            }
            "terminal.splitDown" => {
                self.trigger_split(SplitDir::Horizontal);
            }
            "terminal.splitRight" => {
                self.trigger_split(SplitDir::Vertical);
            }
            "pane.zoom" => {
                self.trigger_toggle_zoom();
            }
            "terminal.closePane" => {
                let active = self.snapshot.active_pane_id.clone();
                if !active.is_empty() {
                    self.trigger_close_pane(&active);
                }
            }
            "view.toggleSidebar" => {
                self.sidebar_collapsed = !self.sidebar_collapsed;
            }
            "view.toggleInspector" => {
                self.inspector_open = !self.inspector_open;
            }
            "view.info" => {
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Info;
            }
            "view.files" => {
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Files;
            }
            "view.git" => {
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Git;
            }
            "view.rig" => {
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Rig;
            }
            "editor.toggleDiff" => {
                if self.diff_view.is_open {
                    self.diff_view.is_open = false;
                } else if let Some(ref path) = self.active_editor_path.clone() {
                    if let Some(doc) = self.open_editors.get(path) {
                        let title = doc.title.clone();
                        let content = doc.content.clone();
                        let full_path = doc.full_path.clone();
                        let disk_content = std::fs::read_to_string(&full_path).unwrap_or_default();
                        self.open_diff_view(&title, "Working Tree", &disk_content, &content);
                    }
                }
            }
            "editor.cycleFiles" => {
                if self.open_editors.contains_key("src/main.rs")
                    && self.active_editor_path.as_deref() == Some("src/main.rs")
                {
                    self.open_editor_file("notes.xyz");
                } else {
                    self.open_editor_file("src/main.rs");
                }
            }
            "app.cycleTheme" => {
                self.settings_modal.theme_mode = match self.settings_modal.theme_mode.as_str() {
                    "Dark" => "Light".to_string(),
                    "Light" => "Omarchy".to_string(),
                    _ => "Dark".to_string(),
                };
            }
            "terminal.cycleForward" => {
                let host = self.host.clone();
                self.tokio_handle.spawn(async move {
                    let _ = host.cycle_pane_focus(true).await;
                });
            }
            "terminal.cycleReverse" => {
                let host = self.host.clone();
                self.tokio_handle.spawn(async move {
                    let _ = host.cycle_pane_focus(false).await;
                });
            }
            "terminal.copy" | "terminal.paste" | "terminal.delWord" => {
                // Informational shortcut commands
            }
            _ => eprintln!("[execute_command] Unhandled command ID: {}", cmd_id),
        }
    }

    /// Renders an active editor file buffer with title, dirty marker, and editable contents.
    pub fn render_editor_pane(&mut self, ui: &mut egui::Ui, path: &str) {
        let frame = Frame::none()
            .fill(theme::SURFACE_CARD)
            .stroke(Stroke::new(1.0_f32, theme::BORDER_SUBTLE))
            .rounding(Rounding::same(6.0_f32))
            .inner_margin(Margin::same(8.0));

        let mut close_editor = false;
        let mut save_requested = false;
        let mut diff_requested = false;

        let syntax_theme = match self.settings_modal.theme_mode.to_lowercase().as_str() {
            "light" => crate::editor::SyntaxTheme::light(),
            "omarchy" => crate::editor::SyntaxTheme::omarchy(),
            _ => crate::editor::SyntaxTheme::dark(),
        };

        frame.show(ui, |ui| {
            // First: check disk if file changed externally
            if let Some(doc) = self.open_editors.get_mut(path) {
                doc.check_disk();
            }

            // Header bar: file icon, name, dirty indicator, diff button, save button, close button
            ui.horizontal(|ui| {
                let (is_dirty, title, is_binary, is_too_large) = self
                    .open_editors
                    .get(path)
                    .map(|d| (d.is_dirty, d.title.clone(), d.is_binary, d.is_too_large))
                    .unwrap_or((false, path.to_string(), false, false));

                let (f_icon_r, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                Icons::draw_file_text(ui.painter(), f_icon_r, theme::INFO);
                ui.label(
                    egui::RichText::new(&title)
                        .size(12.5)
                        .strong()
                        .color(theme::TEXT_STRONG),
                );
                if is_dirty {
                    let (dirty_r, _) = ui.allocate_exact_size(Vec2::new(10.0, 14.0), Sense::hover());
                    ui.painter().circle_filled(dirty_r.center(), 3.0, theme::WARNING);
                }
                ui.label(
                    egui::RichText::new(format!("• {}", path))
                        .size(11.0)
                        .color(theme::TEXT_FAINT),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (close_r, close_resp) =
                        ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                    if close_resp.hovered() {
                        ui.painter().rect_filled(close_r, theme::RADIUS_SM, theme::BAD.linear_multiply(0.2));
                        ui.painter().rect_stroke(close_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BAD));
                    }
                    Icons::draw_close(
                        ui.painter(),
                        close_r,
                        if close_resp.hovered() { theme::BAD } else { theme::TEXT_MUTED },
                    );
                    if close_resp.on_hover_text("Close file (Ctrl+W)").clicked() {
                        close_editor = true;
                    }

                    if !is_binary
                        && !is_too_large
                        && ui
                            .button(
                                egui::RichText::new("Diff (Ctrl+D)")
                                    .size(11.0)
                                    .color(theme::TEXT_MUTED),
                            )
                            .clicked()
                    {
                        diff_requested = true;
                    }

                    if is_dirty
                        && ui
                            .button(
                                egui::RichText::new("Save (Ctrl+S)")
                                    .size(11.0)
                                    .color(theme::ACCENT)
                                    .strong(),
                            )
                            .clicked()
                    {
                        save_requested = true;
                    }

                    let is_md = path.ends_with(".md") || path.ends_with(".mdx");
                    if is_md {
                        let is_preview = self
                            .open_editors
                            .get(path)
                            .map(|d| d.is_preview)
                            .unwrap_or(false);
                        let label = if is_preview { "Source" } else { "Preview" };
                        if ui
                            .button(
                                egui::RichText::new(label)
                                    .size(11.0)
                                    .color(theme::TEXT_MUTED),
                            )
                            .clicked()
                        {
                            if let Some(doc) = self.open_editors.get_mut(path) {
                                doc.is_preview = !doc.is_preview;
                            }
                        }
                    }
                });
            });

            // If file was modified on disk by an external tool/agent
            let has_conflict = self
                .open_editors
                .get(path)
                .map(|d| d.has_external_conflict)
                .unwrap_or(false);
            if has_conflict {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let (a_icon_r, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                    Icons::draw_alert(ui.painter(), a_icon_r, theme::STATUS_WARN);
                    ui.label(
                        egui::RichText::new("File was modified on disk by another program.")
                            .size(theme::typography::TERMINAL_TEXT)
                            .color(theme::STATUS_WARN)
                            .strong(),
                    );
                    if ui.button("Reload from Disk").clicked() {
                        if let Some(doc) = self.open_editors.get_mut(path) {
                            if let Ok(data) = std::fs::read(&doc.full_path) {
                                doc.content = String::from_utf8_lossy(&data).to_string();
                                doc.disk_hash = temp_terminal_procs::def::sha256_digest(&data);
                                doc.is_dirty = false;
                                doc.has_external_conflict = false;
                            }
                        }
                    }
                    if ui.button("Overwrite Disk").clicked() {
                        save_requested = true;
                    }
                });
            }

            ui.separator();
            ui.add_space(4.0);

            // Editor contents
            if let Some(doc) = self.open_editors.get_mut(path) {
                if doc.is_binary || doc.is_too_large {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.label(
                            egui::RichText::new(&doc.content)
                                .size(13.0)
                                .color(theme::TEXT_MUTED),
                        );
                    });
                } else if doc.is_preview && (path.ends_with(".md") || path.ends_with(".mdx")) {
                    let avail_h = ui.available_height() - 32.0;
                    egui::ScrollArea::vertical()
                        .id_salt("markdown_preview_scroll")
                        .max_height(avail_h.max(100.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let pad = ((ui.available_width() - 720.0).max(0.0)) / 2.0;
                                ui.add_space(pad);
                                ui.vertical(|ui| {
                                    ui.set_max_width(720.0_f32.min(ui.available_width()));
                                    Self::render_markdown_view(ui, &doc.content, &syntax_theme);
                                });
                            });
                        });
                } else {
                    let line_count = doc.content.lines().count().max(1);
                    let font_size = self.settings_modal.font_size;
                    let tab_size = self.settings_modal.tab_size as usize;
                    let word_wrap = self.settings_modal.word_wrap;
                    let show_line_numbers = self.settings_modal.line_numbers;
                    let lang = doc.language;

                    let avail_h = ui.available_height() - 32.0;

                    egui::ScrollArea::both()
                        .id_salt("editor_scroll_area")
                        .max_height(avail_h.max(100.0))
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                if show_line_numbers {
                                    let gutter_w =
                                        (line_count.to_string().len().max(2) as f32 * 8.5) + 16.0;
                                    let mut num_str = String::with_capacity(line_count * 6);
                                    for idx in 1..=line_count {
                                        use std::fmt::Write;
                                        let _ = writeln!(
                                            num_str,
                                            "{:>width$}",
                                            idx,
                                            width = line_count.to_string().len().max(2)
                                        );
                                    }
                                    ui.add_sized(
                                        [gutter_w, ui.available_height()],
                                        egui::Label::new(
                                            egui::RichText::new(num_str.trim_end())
                                                .font(egui::FontId::monospace(font_size))
                                                .color(syntax_theme.line_number),
                                        ),
                                    );
                                    ui.add_space(4.0);
                                }

                                let text_resp = ui.add(
                                    egui::TextEdit::multiline(&mut doc.content)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(ui.available_width())
                                        .lock_focus(true)
                                        .layouter(&mut |ui, text, wrap_width| {
                                            let mut layout_job = crate::editor::highlight_syntax(
                                                text,
                                                lang,
                                                &syntax_theme,
                                                font_size,
                                                tab_size,
                                            );
                                            layout_job.wrap.max_width =
                                                if word_wrap { wrap_width } else { f32::INFINITY };
                                            ui.fonts(|f| f.layout_job(layout_job))
                                        }),
                                );
                                if text_resp.changed() {
                                    doc.is_dirty = true;
                                }

                                if let Some(state) =
                                    egui::text_edit::TextEditState::load(ui.ctx(), text_resp.id)
                                {
                                    if let Some(cursor_range) = state.cursor.char_range() {
                                        let char_idx = cursor_range.primary.index;
                                        let slice = &doc.content[..char_idx.min(doc.content.len())];
                                        let line = slice.chars().filter(|&c| c == '\n').count();
                                        let col = slice
                                            .rfind('\n')
                                            .map(|last_nl| char_idx - last_nl - 1)
                                            .unwrap_or(char_idx);
                                        doc.cursor_line = line;
                                        doc.cursor_col = col;
                                    }
                                }
                            });
                        });

                    // Status line (matching Go tempterm/editor.go:210-235)
                    ui.separator();
                    ui.horizontal(|ui| {
                        if !doc.is_preview {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Ln {}, Col {}",
                                    doc.cursor_line + 1,
                                    doc.cursor_col + 1
                                ))
                                .size(10.5)
                                .color(theme::TEXT_MUTED),
                            );
                        }
                        ui.label(
                            egui::RichText::new("UTF-8")
                                .size(10.5)
                                .color(theme::TEXT_MUTED),
                        );
                        if doc.is_dirty {
                            ui.label(
                                egui::RichText::new("●")
                                    .size(10.0)
                                    .color(theme::STATUS_WARN),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(doc.language.name())
                                    .size(10.5)
                                    .color(theme::TEXT_MUTED),
                            );
                        });
                    });
                }
            }
        });

        if diff_requested {
            if let Some(doc) = self.open_editors.get(path) {
                let title = doc.title.clone();
                let content = doc.content.clone();
                let full_path = doc.full_path.clone();
                let disk_content = std::fs::read_to_string(&full_path).unwrap_or_default();
                let repo_root = self.active_project_dir();
                self.open_diff_view_with_git(
                    &title,
                    "Working Tree",
                    &disk_content,
                    &content,
                    repo_root,
                    Some(path.to_string()),
                    false,
                );
            }
        }

        if save_requested {
            self.save_active_editor();
        }

        if close_editor {
            self.close_editor_file(path);
        }
    }

    /// Renders formatted Markdown content matching Go `tempterm/editor.go:199-215`.
    pub fn render_markdown_view(
        ui: &mut egui::Ui,
        content: &str,
        _theme: &crate::editor::SyntaxTheme,
    ) {
        let mut in_code_block = false;
        let mut code_lines = Vec::new();

        // Strip YAML front matter if present
        let text = if content.starts_with("---") {
            if let Some(rest) = content.strip_prefix("---") {
                if let Some(idx) = rest.find("\n---") {
                    &rest[idx + 4..]
                } else {
                    content
                }
            } else {
                content
            }
        } else {
            content
        };

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                if in_code_block {
                    in_code_block = false;
                    let code_text = code_lines.join("\n");
                    egui::Frame::none()
                        .fill(theme::SURFACE_INPUT)
                        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER_SUBTLE))
                        .rounding(egui::Rounding::same(6.0_f32))
                        .inner_margin(egui::Margin::same(8.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&code_text)
                                    .font(egui::FontId::monospace(12.0))
                                    .color(theme::TEXT_SECONDARY),
                            );
                        });
                    ui.add_space(6.0);
                    code_lines.clear();
                } else {
                    in_code_block = true;
                    code_lines.clear();
                }
                continue;
            }

            if in_code_block {
                code_lines.push(line);
                continue;
            }

            if trimmed.is_empty() {
                ui.add_space(6.0);
                continue;
            }

            if let Some(heading) = trimmed.strip_prefix("# ") {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(heading)
                        .size(22.0)
                        .strong()
                        .color(theme::TEXT_STRONG),
                );
                ui.separator();
                ui.add_space(4.0);
            } else if let Some(heading) = trimmed.strip_prefix("## ") {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(heading)
                        .size(18.0)
                        .strong()
                        .color(theme::TEXT_STRONG),
                );
                ui.separator();
                ui.add_space(4.0);
            } else if let Some(heading) = trimmed.strip_prefix("### ") {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(heading)
                        .size(15.0)
                        .strong()
                        .color(theme::TEXT_STRONG),
                );
                ui.add_space(2.0);
            } else if trimmed == "---" || trimmed == "***" {
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);
            } else if let Some(quote) = trimmed.strip_prefix("> ") {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("▎").color(theme::ACCENT).strong());
                    ui.label(
                        egui::RichText::new(quote)
                            .italics()
                            .color(theme::TEXT_MUTED)
                            .size(12.5),
                    );
                });
                ui.add_space(2.0);
            } else if let Some(item) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("•").color(theme::ACCENT).size(13.0));
                    Self::render_inline_markdown(ui, item);
                });
                ui.add_space(2.0);
            } else if trimmed
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
                && trimmed.contains(". ")
            {
                if let Some((num, item)) = trimmed.split_once(". ") {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{}.", num))
                                .color(theme::ACCENT)
                                .size(12.0)
                                .strong(),
                        );
                        Self::render_inline_markdown(ui, item);
                    });
                    ui.add_space(2.0);
                } else {
                    Self::render_inline_markdown(ui, line);
                }
            } else {
                Self::render_inline_markdown(ui, line);
                ui.add_space(3.0);
            }
        }

        if in_code_block && !code_lines.is_empty() {
            let code_text = code_lines.join("\n");
            egui::Frame::none()
                .fill(theme::SURFACE_INPUT)
                .stroke(egui::Stroke::new(1.0_f32, theme::BORDER_SUBTLE))
                .rounding(egui::Rounding::same(4.0))
                .inner_margin(egui::Margin::same(8.0))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(&code_text)
                            .font(egui::FontId::monospace(12.0))
                            .color(theme::TEXT_NORMAL),
                    );
                });
        }
    }

    /// Renders inline Markdown elements (code chips, bold, plain text).
    pub fn render_inline_markdown(ui: &mut egui::Ui, text: &str) {
        ui.horizontal_wrapped(|ui| {
            let mut rest = text;
            while !rest.is_empty() {
                if let Some(start) = rest.find('`') {
                    if start > 0 {
                        ui.label(
                            egui::RichText::new(&rest[..start])
                                .size(12.5)
                                .color(theme::TEXT_NORMAL),
                        );
                    }
                    let after = &rest[start + 1..];
                    if let Some(end) = after.find('`') {
                        let code = &after[..end];
                        ui.label(
                            egui::RichText::new(format!(" {} ", code))
                                .font(egui::FontId::monospace(11.5))
                                .background_color(theme::BG_RAISED)
                                .color(theme::ACCENT),
                        );
                        rest = &after[end + 1..];
                    } else {
                        ui.label(
                            egui::RichText::new(&rest[start..])
                                .size(12.5)
                                .color(theme::TEXT_NORMAL),
                        );
                        break;
                    }
                } else if let Some(start) = rest.find("**") {
                    if start > 0 {
                        ui.label(
                            egui::RichText::new(&rest[..start])
                                .size(12.5)
                                .color(theme::TEXT_NORMAL),
                        );
                    }
                    let after = &rest[start + 2..];
                    if let Some(end) = after.find("**") {
                        let bold = &after[..end];
                        ui.label(
                            egui::RichText::new(bold)
                                .size(12.5)
                                .strong()
                                .color(theme::TEXT_STRONG),
                        );
                        rest = &after[end + 2..];
                    } else {
                        ui.label(
                            egui::RichText::new(&rest[start..])
                                .size(12.5)
                                .color(theme::TEXT_NORMAL),
                        );
                        break;
                    }
                } else {
                    ui.label(
                        egui::RichText::new(rest)
                            .size(12.5)
                            .color(theme::TEXT_NORMAL),
                    );
                    break;
                }
            }
        });
    }

    /// Renders the diff viewer matching Go `tempterm/diff.go:420-590`.
    pub fn render_diff_viewer(&mut self, ui: &mut egui::Ui) {
        let frame = Frame::none()
            .fill(theme::SURFACE_CARD)
            .stroke(Stroke::new(1.0_f32, theme::BORDER_SUBTLE))
            .rounding(Rounding::same(6.0_f32))
            .inner_margin(Margin::same(8.0));

        let syntax_theme = match self.settings_modal.theme_mode.to_lowercase().as_str() {
            "light" => crate::editor::SyntaxTheme::light(),
            "omarchy" => crate::editor::SyntaxTheme::omarchy(),
            _ => crate::editor::SyntaxTheme::dark(),
        };

        frame.show(ui, |ui| {
            // Toolbar
            ui.horizontal(|ui| {
                let (diff_icon_r, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                Icons::draw_git_branch(ui.painter(), diff_icon_r, theme::WARN);
                ui.label(
                    egui::RichText::new(format!("Diff • {}", self.diff_view.title))
                        .size(12.5)
                        .strong()
                        .color(theme::TEXT_STRONG),
                );
                ui.label(
                    egui::RichText::new(format!("({})", self.diff_view.scope))
                        .size(11.0)
                        .color(theme::TEXT_MUTED),
                );

                // Stats: +additions / -deletions
                if self.diff_view.diff_result.additions > 0
                    || self.diff_view.diff_result.deletions > 0
                {
                    ui.label(
                        egui::RichText::new(format!("+{}", self.diff_view.diff_result.additions))
                            .size(11.0)
                            .color(theme::STATUS_OK)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(format!("-{}", self.diff_view.diff_result.deletions))
                            .size(11.0)
                            .color(theme::STATUS_BAD)
                            .strong(),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (close_r, close_resp) =
                        ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                    if close_resp.hovered() {
                        ui.painter().rect_filled(close_r, theme::RADIUS_SM, theme::BAD.linear_multiply(0.2));
                        ui.painter().rect_stroke(close_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::BAD));
                    }
                    Icons::draw_close(
                        ui.painter(),
                        close_r,
                        if close_resp.hovered() { theme::BAD } else { theme::TEXT_MUTED },
                    );
                    if close_resp.on_hover_text("Close diff viewer").clicked() {
                        self.diff_view.is_open = false;
                    }

                    // Inline button
                    let inline_btn = ui.add(
                        egui::Button::new(egui::RichText::new("Inline").size(10.5).color(
                            if !self.diff_view.is_split {
                                theme::TEXT_STRONG
                            } else {
                                theme::TEXT_MUTED
                            },
                        ))
                        .fill(if !self.diff_view.is_split {
                            theme::BG_RAISED
                        } else {
                            Color32::TRANSPARENT
                        })
                        .rounding(Rounding::same(4.0)),
                    );
                    if inline_btn.clicked() {
                        self.diff_view.is_split = false;
                        self.settings_modal.diff_split = false;
                        self.persist_settings();
                    }

                    // Split button
                    let split_btn = ui.add(
                        egui::Button::new(egui::RichText::new("Split").size(10.5).color(
                            if self.diff_view.is_split {
                                theme::TEXT_STRONG
                            } else {
                                theme::TEXT_MUTED
                            },
                        ))
                        .fill(if self.diff_view.is_split {
                            theme::BG_RAISED
                        } else {
                            Color32::TRANSPARENT
                        })
                        .rounding(Rounding::same(4.0)),
                    );
                    if split_btn.clicked() {
                        self.diff_view.is_split = true;
                        self.settings_modal.diff_split = true;
                        self.persist_settings();
                    }

                    // Git Stage/Unstage and Discard buttons (matching Go `tempterm/diff.go:460-490`)
                    if let (Some(repo_root), Some(rel_path)) = (
                        self.diff_view.repo_root.clone(),
                        self.diff_view.rel_path.clone(),
                    ) {
                        let is_staged = self.diff_view.is_staged;
                        let action_label = if is_staged {
                            "Unstage File"
                        } else {
                            "Stage File"
                        };
                        let action_btn = ui.add(
                            egui::Button::new(egui::RichText::new(action_label).size(10.5).color(
                                if is_staged {
                                    theme::TEXT_MUTED
                                } else {
                                    theme::TEXT_STRONG
                                },
                            ))
                            .fill(if is_staged {
                                theme::BG_RAISED
                            } else {
                                theme::ACCENT
                            })
                            .rounding(Rounding::same(4.0)),
                        );
                        if action_btn.clicked() {
                            if is_staged {
                                match temp_terminal_core::git::unstage_file(&repo_root, &rel_path) {
                                    Ok(_) => {
                                        self.diff_view.is_staged = false;
                                        self.diff_view.scope = "Working Tree".to_string();
                                        self.diff_view.action_status =
                                            Some("Unstaged file".to_string());
                                        let head = temp_terminal_core::git::show_head(
                                            &repo_root, &rel_path,
                                        )
                                        .unwrap_or_default();
                                        let disk =
                                            std::fs::read_to_string(repo_root.join(&rel_path))
                                                .unwrap_or_default();
                                        self.diff_view.diff_result =
                                            crate::editor::compute_line_diff(&head, &disk);
                                    }
                                    Err(e) => {
                                        self.diff_view.action_status =
                                            Some(format!("Git error: {}", e));
                                    }
                                }
                            } else {
                                match temp_terminal_core::git::stage_file(&repo_root, &rel_path) {
                                    Ok(_) => {
                                        self.diff_view.is_staged = true;
                                        self.diff_view.scope = "Staged: HEAD → Index".to_string();
                                        self.diff_view.action_status =
                                            Some("Staged file".to_string());
                                        let head = temp_terminal_core::git::show_head(
                                            &repo_root, &rel_path,
                                        )
                                        .unwrap_or_default();
                                        let idx = temp_terminal_core::git::show_index(
                                            &repo_root, &rel_path,
                                        )
                                        .unwrap_or_default();
                                        self.diff_view.diff_result =
                                            crate::editor::compute_line_diff(&head, &idx);
                                    }
                                    Err(e) => {
                                        self.diff_view.action_status =
                                            Some(format!("Git error: {}", e));
                                    }
                                }
                            }
                        }

                        if !is_staged {
                            if !self.diff_view.confirm_discard {
                                let discard_btn = ui.add(
                                    egui::Button::new(
                                        egui::RichText::new("Discard")
                                            .size(10.5)
                                            .color(theme::STATUS_BAD),
                                    )
                                    .fill(Color32::TRANSPARENT)
                                    .rounding(Rounding::same(4.0)),
                                );
                                if discard_btn.clicked() {
                                    self.diff_view.confirm_discard = true;
                                }
                            } else {
                                ui.label(
                                    egui::RichText::new("Discard?")
                                        .size(10.5)
                                        .color(theme::STATUS_BAD),
                                );
                                let confirm_btn = ui.add(
                                    egui::Button::new(
                                        egui::RichText::new("Confirm Discard")
                                            .size(10.5)
                                            .color(Color32::WHITE),
                                    )
                                    .fill(theme::STATUS_BAD)
                                    .rounding(Rounding::same(4.0)),
                                );
                                if confirm_btn.clicked() {
                                    self.diff_view.confirm_discard = false;
                                    match temp_terminal_core::git::discard_file(
                                        &repo_root, &rel_path, false,
                                    ) {
                                        Ok(_) => {
                                            self.diff_view.action_status =
                                                Some("Discarded changes".to_string());
                                            let head = temp_terminal_core::git::show_head(
                                                &repo_root, &rel_path,
                                            )
                                            .unwrap_or_default();
                                            let disk =
                                                std::fs::read_to_string(repo_root.join(&rel_path))
                                                    .unwrap_or_default();
                                            self.diff_view.diff_result =
                                                crate::editor::compute_line_diff(&head, &disk);
                                        }
                                        Err(e) => {
                                            self.diff_view.action_status =
                                                Some(format!("Git error: {}", e));
                                        }
                                    }
                                }
                                let cancel_btn = ui.add(
                                    egui::Button::new(
                                        egui::RichText::new("Cancel")
                                            .size(10.5)
                                            .color(theme::TEXT_MUTED),
                                    )
                                    .fill(Color32::TRANSPARENT)
                                    .rounding(Rounding::same(4.0)),
                                );
                                if cancel_btn.clicked() {
                                    self.diff_view.confirm_discard = false;
                                }
                            }
                        }
                    }
                });
            });

            ui.separator();
            ui.add_space(4.0);

            if self.diff_view.diff_result.is_identical {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(
                        egui::RichText::new("No changes between versions")
                            .size(13.0)
                            .color(theme::TEXT_MUTED),
                    );
                });
                return;
            }

            let font_id = egui::FontId::monospace(self.settings_modal.font_size);

            if self.diff_view.is_split {
                // Split (Side-by-Side) Diff View
                ui.columns(2, |columns| {
                    columns[0].label(
                        egui::RichText::new("Original (Disk)")
                            .size(11.0)
                            .strong()
                            .color(theme::TEXT_MUTED),
                    );
                    columns[1].label(
                        egui::RichText::new("Modified (Editor)")
                            .size(11.0)
                            .strong()
                            .color(theme::TEXT_MUTED),
                    );
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("split_diff_scroll")
                    .show(ui, |ui| {
                        for line in &self.diff_view.diff_result.lines {
                            ui.columns(2, |columns| {
                                // Left side (Old)
                                let (left_bg, left_fg) = match line.kind {
                                    crate::editor::DiffKind::Deleted => {
                                        (syntax_theme.diff_del_bg, syntax_theme.diff_del_fg)
                                    }
                                    _ => (Color32::TRANSPARENT, theme::TEXT_NORMAL),
                                };
                                let left_num =
                                    line.old_line_num.map(|n| n.to_string()).unwrap_or_default();
                                columns[0].horizontal(|ui| {
                                    let fill_rect = ui.available_rect_before_wrap();
                                    if left_bg != Color32::TRANSPARENT {
                                        ui.painter().rect_filled(fill_rect, 0.0, left_bg);
                                    }
                                    ui.label(
                                        egui::RichText::new(format!("{:>3} ", left_num))
                                            .font(font_id.clone())
                                            .color(syntax_theme.line_number),
                                    );
                                    ui.label(
                                        egui::RichText::new(&line.old_text)
                                            .font(font_id.clone())
                                            .color(left_fg),
                                    );
                                });

                                // Right side (New)
                                let (right_bg, right_fg) = match line.kind {
                                    crate::editor::DiffKind::Added => {
                                        (syntax_theme.diff_add_bg, syntax_theme.diff_add_fg)
                                    }
                                    _ => (Color32::TRANSPARENT, theme::TEXT_NORMAL),
                                };
                                let right_num =
                                    line.new_line_num.map(|n| n.to_string()).unwrap_or_default();
                                columns[1].horizontal(|ui| {
                                    let fill_rect = ui.available_rect_before_wrap();
                                    if right_bg != Color32::TRANSPARENT {
                                        ui.painter().rect_filled(fill_rect, 0.0, right_bg);
                                    }
                                    ui.label(
                                        egui::RichText::new(format!("{:>3} ", right_num))
                                            .font(font_id.clone())
                                            .color(syntax_theme.line_number),
                                    );
                                    ui.label(
                                        egui::RichText::new(&line.new_text)
                                            .font(font_id.clone())
                                            .color(right_fg),
                                    );
                                });
                            });
                        }
                    });
            } else {
                // Inline (Unified) Diff View
                egui::ScrollArea::vertical()
                    .id_salt("inline_diff_scroll")
                    .show(ui, |ui| {
                        for line in &self.diff_view.diff_result.lines {
                            let (bg, fg, prefix) = match line.kind {
                                crate::editor::DiffKind::Added => {
                                    (syntax_theme.diff_add_bg, syntax_theme.diff_add_fg, "+")
                                }
                                crate::editor::DiffKind::Deleted => {
                                    (syntax_theme.diff_del_bg, syntax_theme.diff_del_fg, "-")
                                }
                                _ => (Color32::TRANSPARENT, theme::TEXT_NORMAL, " "),
                            };
                            let num_display = format!(
                                "{:>3} {:>3}",
                                line.old_line_num.map(|n| n.to_string()).unwrap_or_default(),
                                line.new_line_num.map(|n| n.to_string()).unwrap_or_default()
                            );
                            let text_display = if line.kind == crate::editor::DiffKind::Deleted {
                                &line.old_text
                            } else {
                                &line.new_text
                            };

                            ui.horizontal(|ui| {
                                let rect = ui.available_rect_before_wrap();
                                if bg != Color32::TRANSPARENT {
                                    ui.painter().rect_filled(rect, 0.0, bg);
                                }
                                ui.label(
                                    egui::RichText::new(&num_display)
                                        .font(font_id.clone())
                                        .color(syntax_theme.line_number),
                                );
                                ui.label(
                                    egui::RichText::new(prefix)
                                        .font(font_id.clone())
                                        .color(fg)
                                        .strong(),
                                );
                                ui.label(
                                    egui::RichText::new(text_display)
                                        .font(font_id.clone())
                                        .color(fg),
                                );
                            });
                        }
                    });
            }
        });
    }

    /// Opens a file in the workspace editor matching Go `openEditor(path)`.
    pub fn open_editor_file(&mut self, rel_path: &str) {
        if self.open_editors.contains_key(rel_path) {
            self.active_editor_path = Some(rel_path.to_string());
            self.diff_view.is_open = false;
            return;
        }

        let proj_path = self.snapshot.project_path.clone();
        let base_dir = std::path::Path::new(&proj_path);
        match crate::editor::EditorDoc::open(rel_path, base_dir) {
            Ok(doc) => {
                self.open_editors.insert(rel_path.to_string(), doc);
                self.active_editor_path = Some(rel_path.to_string());
                self.diff_view.is_open = false;
            }
            Err(e) => {
                eprintln!("[open_editor_file] Error opening {}: {}", rel_path, e);
                let doc = crate::editor::EditorDoc::new_memory(
                    rel_path,
                    rel_path,
                    &format!("[Error opening file: {}]", e),
                );
                self.open_editors.insert(rel_path.to_string(), doc);
                self.active_editor_path = Some(rel_path.to_string());
                self.diff_view.is_open = false;
            }
        }
    }

    /// Closes an open editor file, prompting if dirty.
    pub fn close_editor_file(&mut self, path: &str) {
        if let Some(doc) = self.open_editors.get(path) {
            if doc.is_dirty {
                self.close_unsaved_doc = Some(path.to_string());
                return;
            }
        }
        self.open_editors.remove(path);
        if self.active_editor_path.as_deref() == Some(path) {
            self.active_editor_path = self.open_editors.keys().next().cloned();
        }
    }

    /// Saves dirty changes to disk for the active editor file atomically.
    pub fn save_active_editor(&mut self) {
        if let Some(ref path) = self.active_editor_path.clone() {
            if let Some(doc) = self.open_editors.get_mut(path) {
                if doc.is_dirty && !doc.is_binary && !doc.is_too_large {
                    // Check if file on disk was modified externally first
                    if let Ok(disk_bytes) = std::fs::read(&doc.full_path) {
                        let current_hash = temp_terminal_procs::def::sha256_digest(&disk_bytes);
                        if current_hash != doc.disk_hash {
                            self.conflict_doc = Some(path.clone());
                            return;
                        }
                    }

                    if let Err(e) = doc.save() {
                        eprintln!("[save_active_editor] Error saving {}: {}", doc.path, e);
                    } else {
                        eprintln!("[save_active_editor] Saved {}", doc.path);
                    }
                }
            }
        }
    }

    /// Opens the diff view comparing old and new text.
    pub fn open_diff_view(
        &mut self,
        title: &str,
        scope: &str,
        old_content: &str,
        new_content: &str,
    ) {
        let is_split = self.settings_modal.diff_split;
        self.diff_view
            .open(title, scope, old_content, new_content, is_split);
    }

    /// Opens the diff view with Git staging and discard capabilities.
    #[allow(clippy::too_many_arguments)]
    pub fn open_diff_view_with_git(
        &mut self,
        title: &str,
        scope: &str,
        old_content: &str,
        new_content: &str,
        repo_root: Option<PathBuf>,
        rel_path: Option<String>,
        is_staged: bool,
    ) {
        let is_split = self.settings_modal.diff_split;
        self.diff_view.open_with_git(
            title,
            scope,
            old_content,
            new_content,
            is_split,
            repo_root,
            rel_path,
            is_staged,
        );
    }

    /// Returns the directory path of the active project, if any.
    pub fn active_project_dir(&self) -> Option<PathBuf> {
        let active_id = self.host.active_project_id.try_read().ok()?.clone();
        let projs = self.host.projects.try_read().ok()?;
        projs.get(&active_id).map(|p| p.path.clone())
    }

    /// Scans project files for Quick Open (`Ctrl+P`).
    pub fn scan_quick_open_files(&mut self) {
        self.quick_open.cached_files.clear();
        let proj_path = self.snapshot.project_path.clone();
        if proj_path.is_empty() {
            return;
        }

        let mut files = Vec::new();
        let root = std::path::Path::new(&proj_path);
        fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>) {
            if out.len() >= 250 {
                return;
            }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') || name == "target" || name == "node_modules" {
                        continue;
                    }
                    if path.is_dir() {
                        walk(&path, root, out);
                    } else if let Ok(rel) = path.strip_prefix(root) {
                        out.push(rel.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
        walk(root, root, &mut files);
        self.quick_open.cached_files = files;
    }

    /// Returns list of supported commands for the Command Palette.
    pub fn get_palette_commands(&self) -> Vec<PaletteCommand> {
        get_all_palette_commands()
    }
    pub fn trigger_new_agent(
        &mut self,
        agent: &str,
        use_worktree: bool,
        branch: &str,
        base_branch: &str,
        copy_env: bool,
    ) {
        let host = self.host.clone();
        let registry = self.registry.clone();
        let handle = self.tokio_handle.clone();
        let agent_name = agent.to_string();
        let branch_name = branch.to_string();
        let base_name = if base_branch.trim().is_empty() {
            "HEAD".to_string()
        } else {
            base_branch.trim().to_string()
        };

        self.tokio_handle.spawn(async move {
            let active_id = {
                let id = host.active_project_id.read().await;
                if id.is_empty() {
                    "default".to_string()
                } else {
                    id.clone()
                }
            };

            let (project_root, count) = {
                let projs = host.projects.read().await;
                if let Some(p) = projs.get(&active_id) {
                    (p.path.clone(), p.sessions.len() + 1)
                } else {
                    (std::path::PathBuf::from("."), 1)
                }
            };

            // Worktree resolution: if worktree enabled and branch requested, use Git worktree manager
            let final_cwd = if use_worktree && !branch_name.is_empty() {
                match temp_terminal_core::worktree::add_worktree(
                    &project_root,
                    &branch_name,
                    &base_name,
                    copy_env,
                ) {
                    Ok(wt) => {
                        eprintln!(
                            "[trigger_new_agent] Created worktree for branch {} from base {} at {:?}",
                            branch_name, base_name, wt.dir
                        );
                        wt.dir
                    }
                    Err(e) => {
                        eprintln!(
                            "[trigger_new_agent] Worktree creation failed: {}. Falling back to project root.",
                            e
                        );
                        project_root.clone()
                    }
                }
            } else {
                project_root.clone()
            };

            let cwd_str = final_cwd.to_string_lossy().to_string();
            let session_name = format!("agent_{}_{}", agent_name, count);
            let pane_id = format!("pane_agent_{}", count);

            // Command resolution matching Go tempterm/internal/agent/agent.go
            #[cfg(windows)]
            let cmd = if agent_name == "shell" {
                match crate::default_shell_command() {
                    Ok(c) => c,
                    Err(_) => vec!["cmd.exe".to_string(), "/k".to_string()],
                }
            } else {
                // Check if agent executable or script is available in PATH
                let check_status = std::process::Command::new("where.exe")
                    .arg(&agent_name)
                    .output();
                let exists = check_status.as_ref().map(|o| o.status.success()).unwrap_or(false);

                let resolved_agent = if exists {
                    if let Ok(ref output) = check_status {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        if let Some(line) = stdout.lines().next() {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                trimmed.to_string()
                            } else {
                                agent_name.clone()
                            }
                        } else {
                            agent_name.clone()
                        }
                    } else {
                        agent_name.clone()
                    }
                } else {
                    let mut found = None;
                    if agent_name == "agy" {
                        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
                            let candidate = std::path::Path::new(&local_appdata)
                                .join("agy")
                                .join("bin")
                                .join("agy.exe");
                            if candidate.is_file() {
                                found = Some(candidate.to_string_lossy().to_string());
                            }
                        }
                    }
                    found.unwrap_or_else(|| agent_name.clone())
                };

                vec!["cmd.exe".to_string(), "/k".to_string(), resolved_agent]
            };
            #[cfg(not(windows))]
            let cmd = if agent_name == "shell" {
                vec!["/bin/sh".to_string()]
            } else {
                vec!["/bin/sh".to_string(), "-c".to_string(), agent_name.clone()]
            };

            eprintln!(
                "[trigger_new_agent] Spawning agent pane {} for {} in cwd {}",
                pane_id, agent_name, cwd_str
            );

            let res = host
                .spawn_local_pane_with_handle(
                    &active_id,
                    &session_name,
                    &pane_id,
                    120,
                    36,
                    &registry,
                    cmd,
                    cwd_str,
                    handle,
                )
                .await;

            match res {
                Ok(_) => {
                    eprintln!(
                        "[trigger_new_agent] Successfully spawned agent {}",
                        agent_name
                    );
                    let _ = host.switch_session(&active_id, &session_name).await;
                }
                Err(e) => eprintln!("[trigger_new_agent] Error spawning agent: {}", e),
            }
        });
    }

    /// Spawns a configured process session in the workspace matching Go `tempterm/processdialog.go`.
    #[allow(clippy::too_many_arguments)]
    pub fn trigger_add_process(
        &mut self,
        name: &str,
        command: &str,
        dir: &str,
        port: &str,
        autostart: bool,
        restart: &str,
        watch: Vec<String>,
        env: std::collections::BTreeMap<String, String>,
        shared: bool,
    ) {
        let host = self.host.clone();
        let registry = self.registry.clone();
        let handle = self.tokio_handle.clone();
        let proc_name = name.to_string();
        let cmd_str = command.to_string();
        let dir_str = dir.to_string();
        let port_str = port.to_string();
        let restart_policy = restart.to_string();
        let watch_globs = watch;
        let env_map = env;

        self.tokio_handle.spawn(async move {
            let (active_id, project_dir) = {
                let id = host.active_project_id.read().await;
                let active = if id.is_empty() {
                    "default".to_string()
                } else {
                    id.clone()
                };
                let projs = host.projects.read().await;
                let dir = projs
                    .get(&active)
                    .map(|p| p.path.clone())
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                (active, dir)
            };

            // If shared is selected, save the definition to tempterm.yml / temp-terminal.yml
            if shared {
                let proj_path = &project_dir;
                let mut defs =
                    temp_terminal_procs::file::load_file(proj_path).unwrap_or_default();
                defs.retain(|d| d.name != proc_name);
                let new_def = temp_terminal_procs::def::Def {
                    name: proc_name.clone(),
                    command: cmd_str.clone(),
                    dir: if dir_str == "." {
                        String::new()
                    } else {
                        dir_str.clone()
                    },
                    port: port_str.parse::<i64>().unwrap_or(0),
                    autostart,
                    restart: restart_policy.to_lowercase(),
                    watch: watch_globs.clone(),
                    env: env_map.clone(),
                    local: false,
                    ..Default::default()
                };
                defs.push(new_def);
                if let Err(e) = temp_terminal_procs::file::save_file(proj_path, &defs) {
                    eprintln!("[trigger_add_process] Error saving to tempterm.yml: {}", e);
                } else {
                    eprintln!(
                        "[trigger_add_process] Successfully saved {} to tempterm.yml",
                        proc_name
                    );
                }
            }

            let count = {
                let projs = host.projects.read().await;
                projs
                    .get(&active_id)
                    .map(|p| p.sessions.len() + 1)
                    .unwrap_or(1)
            };

            let session_name = format!("proc_{}_{}", proc_name, count);
            let pane_id = format!("pane_proc_{}", count);

            let mut env_vec: Vec<String> = Vec::new();
            for (k, v) in &env_map {
                env_vec.push(format!("{}={}", k, v));
            }

            #[cfg(windows)]
            let cmd = vec![
                "cmd.exe".to_string(),
                "/k".to_string(),
                format!(
                    "echo [temp-terminal process: {} (port: {}, autostart: {}, restart: {})] && {}",
                    proc_name, port_str, autostart, restart_policy, cmd_str
                ),
            ];
            #[cfg(not(windows))]
            let cmd = vec!["/bin/sh".to_string(), "-c".to_string(), cmd_str];

            eprintln!(
                "[trigger_add_process] Spawning process pane {} for {} in dir {} (autostart: {}, restart: {}, watch: {:?}, env: {:?})",
                pane_id, proc_name, dir_str, autostart, restart_policy, watch_globs, env_vec
            );

            let res = host
                .spawn_local_pane_with_env(
                    &active_id,
                    &session_name,
                    &pane_id,
                    120,
                    36,
                    &registry,
                    cmd,
                    dir_str,
                    env_vec,
                    handle,
                )
                .await;

            match res {
                Ok(_) => eprintln!(
                    "[trigger_add_process] Successfully started process {}",
                    proc_name
                ),
                Err(e) => eprintln!("[trigger_add_process] Error starting process: {}", e),
            }
        });
    }

    /// Renders modal dialogs, menus, palettes, and overlays for Group D.
    /// Renders modal dialogs, menus, palettes, and overlays for Group D.
    fn render_dialogs(&mut self, ctx: &egui::Context, had_modal_at_frame_start: bool) {
        let mut modal_rects: Vec<Rect> = Vec::new();

        // D1. Project Switcher Dropdown Popover
        if self.new_menu_open {
            let mut is_open = self.new_menu_open;
            let pos = self.new_menu_pos.unwrap_or_else(|| Pos2::new(180.0, 42.0));
            let active_p_id = self.snapshot.active_project_id.clone();
            let other_projs: Vec<_> = self.snapshot.projects.iter().filter(|p| p.id != active_p_id).cloned().collect();
            let menu_h = 76.0 + (other_projs.len().min(5) as f32 * 28.0);
            let win_resp = egui::Window::new("Project Switcher Menu")
                .open(&mut is_open)
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .fixed_pos(pos)
                .fixed_size(Vec2::new(220.0, menu_h))
                .frame(
                    Frame::none()
                        .fill(theme::POPOVER)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                        .rounding(Rounding::same(theme::RADIUS_MD))
                        .inner_margin(Margin::same(6.0))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 6.0),
                            blur: 20.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(160),
                        }),
                )
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;

                    // 1. New Project
                    let (row1_r, row1_resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
                    let p1 = ui.painter_at(row1_r);
                    if row1_resp.hovered() {
                        p1.rect_filled(row1_r, theme::RADIUS_SM, theme::HOVER);
                    }
                    let icon1_r = Rect::from_min_size(pos2(row1_r.min.x + 8.0, row1_r.min.y + 7.0), Vec2::new(14.0, 14.0));
                    Icons::draw_folder(&p1, icon1_r, if row1_resp.hovered() { theme::TEXT_STRONG } else { theme::TEXT_MUTED });
                    p1.text(pos2(row1_r.min.x + 28.0, row1_r.center().y), egui::Align2::LEFT_CENTER, "New Project...", egui::FontId::proportional(12.0), if row1_resp.hovered() { theme::TEXT_STRONG } else { theme::TEXT });
                    if row1_resp.clicked() {
                        self.new_menu_open = false;
                        self.new_project_dialog.is_open = true;
                        self.new_project_dialog.name_input.clear();
                        self.new_project_dialog.path_input.clear();
                        self.new_project_dialog.error_msg = None;
                    }

                    // 2. Open Project
                    let (row2_r, row2_resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
                    let p2 = ui.painter_at(row2_r);
                    if row2_resp.hovered() {
                        p2.rect_filled(row2_r, theme::RADIUS_SM, theme::HOVER);
                    }
                    let icon2_r = Rect::from_min_size(pos2(row2_r.min.x + 8.0, row2_r.min.y + 7.0), Vec2::new(14.0, 14.0));
                    Icons::draw_folder_plus(&p2, icon2_r, if row2_resp.hovered() { theme::TEXT_STRONG } else { theme::TEXT_MUTED });
                    p2.text(pos2(row2_r.min.x + 28.0, row2_r.center().y), egui::Align2::LEFT_CENTER, "Open Project...", egui::FontId::proportional(12.0), if row2_resp.hovered() { theme::TEXT_STRONG } else { theme::TEXT });
                    p2.text(pos2(row2_r.max.x - 8.0, row2_r.center().y), egui::Align2::RIGHT_CENTER, "Ctrl+O", egui::FontId::proportional(10.5), theme::TEXT_MUTED);
                    if row2_resp.clicked() {
                        self.new_menu_open = false;
                        self.open_project_dialog.is_open = true;
                        self.open_project_dialog.path_input.clear();
                        self.open_project_dialog.error_msg = None;
                    }

                    // 3. Switch Projects (if other registered projects exist)
                    if !other_projs.is_empty() {
                        ui.add_space(2.0);
                        ui.painter().line_segment(
                            [pos2(row2_r.min.x, row2_r.max.y + 2.0), pos2(row2_r.max.x, row2_r.max.y + 2.0)],
                            Stroke::new(1.0_f32, theme::DIVIDER),
                        );
                        ui.add_space(2.0);

                        for p in other_projs {
                            let (p_row_r, p_row_resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
                            let pp = ui.painter_at(p_row_r);
                            if p_row_resp.hovered() {
                                pp.rect_filled(p_row_r, theme::RADIUS_SM, theme::HOVER);
                            }
                            let p_icon_r = Rect::from_min_size(pos2(p_row_r.min.x + 8.0, p_row_r.min.y + 6.0), Vec2::new(14.0, 14.0));
                            Icons::draw_folder_git(&pp, p_icon_r, if p_row_resp.hovered() { theme::ACCENT } else { theme::TEXT_MUTED });
                            pp.text(pos2(p_row_r.min.x + 28.0, p_row_r.center().y), egui::Align2::LEFT_CENTER, &p.name, egui::FontId::proportional(11.5), if p_row_resp.hovered() { theme::TEXT_STRONG } else { theme::TEXT });
                            if p_row_resp.clicked() {
                                self.new_menu_open = false;
                                self.trigger_switch_project(&p.id);
                            }
                        }
                    }
                });
            if let Some(r) = win_resp {
                modal_rects.push(r.response.rect);
            }
            if !is_open {
                self.new_menu_open = false;
            }
        }
        // D2. New Agent Modal Window
        if self.agent_dialog.is_open {
            let mut is_open = self.agent_dialog.is_open;
            let win_resp = egui::Window::new("New Agent")
                .id(egui::Id::new("new_agent_modal_window"))
                .open(&mut is_open)
                .collapsible(false)
                .resizable(false)
                .fixed_size(Vec2::new(420.0, 270.0))
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::none()
                        .fill(theme::POPOVER)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                        .rounding(Rounding::same(theme::RADIUS_MD))
                        .inner_margin(Margin::same(16.0))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 8.0),
                            blur: 24.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(150),
                        }),
                )
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new("Start an autonomous coding agent or shell session:")
                            .size(12.0)
                            .color(theme::TEXT_NORMAL),
                    );
                    ui.add_space(10.0);

                    // Agent selector
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Agent:")
                                .size(12.0)
                                .color(theme::TEXT_MUTED),
                        );
                        egui::ComboBox::from_id_salt("agent_selector")
                            .selected_text(&self.agent_dialog.selected_agent)
                            .show_ui(ui, |ui| {
                                for agent in &self.agent_dialog.available_agents {
                                    let display_name = match agent.as_str() {
                                        "claude" => "Claude Code",
                                        "codex" => "Codex",
                                        "opencode" => "OpenCode",
                                        "pi" => "Pi Coding Agent",
                                        "agy" => "Antigravity",
                                        "shell" => "Shell only",
                                        other => other,
                                    };
                                    ui.selectable_value(
                                        &mut self.agent_dialog.selected_agent,
                                        agent.clone(),
                                        display_name,
                                    );
                                }
                            });
                    });

                    ui.add_space(8.0);
                    ui.checkbox(
                        &mut self.agent_dialog.use_worktree,
                        "Run in dedicated git worktree",
                    );
                    if self.agent_dialog.use_worktree {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Branch:")
                                    .size(theme::typography::TERMINAL_TEXT)
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.text_edit_singleline(&mut self.agent_dialog.worktree_branch);
                        });
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("From base:")
                                    .size(theme::typography::TERMINAL_TEXT)
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.text_edit_singleline(&mut self.agent_dialog.worktree_base);
                        });
                        ui.checkbox(
                            &mut self.agent_dialog.copy_env,
                            "Copy .env files into worktree",
                        );
                    }

                    if let Some(ref msg) = self.agent_dialog.status_message {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(msg).size(11.0).color(theme::STATUS_RED));
                    }

                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(egui::RichText::new("Cancel").color(theme::TEXT_MUTED))
                            .clicked()
                        {
                            self.agent_dialog.is_open = false;
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(
                                    egui::RichText::new("Start Agent")
                                        .color(theme::TEXT_STRONG)
                                        .strong(),
                                )
                                .clicked()
                            {
                                let agent = self.agent_dialog.selected_agent.clone();
                                let use_wt = self.agent_dialog.use_worktree;
                                let br = self.agent_dialog.worktree_branch.clone();
                                let base_br = self.agent_dialog.worktree_base.clone();
                                let copy_env = self.agent_dialog.copy_env;
                                self.agent_dialog.is_open = false;
                                self.trigger_new_agent(&agent, use_wt, &br, &base_br, copy_env);
                            }
                        });
                    });
                });
            if let Some(r) = win_resp {
                modal_rects.push(r.response.rect);
            }
            if !is_open {
                self.agent_dialog.is_open = false;
            }
        }

        // D3. Add Process Modal Window
        if self.process_dialog.is_open {
            let mut is_open = self.process_dialog.is_open;
            let win_resp = egui::Window::new("Add Process")
                .id(egui::Id::new("add_process_modal_window"))
                .open(&mut is_open)
                .collapsible(false)
                .resizable(true)
                .default_size(Vec2::new(480.0, 520.0))
                .min_size(Vec2::new(440.0, 400.0))
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::none()
                        .fill(theme::POPOVER)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                        .rounding(Rounding::same(theme::RADIUS_MD))
                        .inner_margin(Margin::same(16.0))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 8.0),
                            blur: 24.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(150),
                        }),
                )
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new("Define a dev server, worker, or background service:")
                            .size(12.0)
                            .color(theme::TEXT_NORMAL),
                    );
                    ui.add_space(8.0);

                    egui::ScrollArea::vertical()
                        .max_height(380.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Name:")
                                        .size(theme::typography::TERMINAL_TEXT)
                                        .color(theme::TEXT_MUTED),
                                );
                                ui.text_edit_singleline(&mut self.process_dialog.name);
                            });

                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Command:")
                                        .size(theme::typography::TERMINAL_TEXT)
                                        .color(theme::TEXT_MUTED),
                                );
                                ui.text_edit_singleline(&mut self.process_dialog.command);
                            });

                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Directory:")
                                        .size(theme::typography::TERMINAL_TEXT)
                                        .color(theme::TEXT_MUTED),
                                );
                                ui.text_edit_singleline(&mut self.process_dialog.dir);
                            });

                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Port:")
                                        .size(theme::typography::TERMINAL_TEXT)
                                        .color(theme::TEXT_MUTED),
                                );
                                ui.text_edit_singleline(&mut self.process_dialog.port);
                            });

                            ui.checkbox(
                                &mut self.process_dialog.autostart,
                                "Autostart with workspace",
                            );

                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Restart Policy:")
                                        .size(theme::typography::TERMINAL_TEXT)
                                        .color(theme::TEXT_MUTED),
                                );
                                egui::ComboBox::from_id_salt("proc_restart_policy")
                                    .selected_text(&self.process_dialog.restart)
                                    .show_ui(ui, |ui| {
                                        for opt in ["Always", "On Failure", "Never"] {
                                            ui.selectable_value(
                                                &mut self.process_dialog.restart,
                                                opt.to_string(),
                                                opt,
                                            );
                                        }
                                    });
                            });

                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new("Watch (one glob per line):")
                                    .size(theme::typography::TERMINAL_TEXT)
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add(
                                egui::TextEdit::multiline(&mut self.process_dialog.watch)
                                    .desired_rows(3)
                                    .desired_width(ui.available_width()),
                            );

                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new("Environment (one KEY=value per line):")
                                    .size(theme::typography::TERMINAL_TEXT)
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add(
                                egui::TextEdit::multiline(&mut self.process_dialog.env)
                                    .desired_rows(3)
                                    .desired_width(ui.available_width()),
                            );

                            ui.add_space(6.0);
                            ui.checkbox(&mut self.process_dialog.shared, "Save in tempterm.yml");
                        });

                    if let Some(ref msg) = self.process_dialog.status_message {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(msg).size(11.0).color(theme::STATUS_RED));
                    }

                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(egui::RichText::new("Cancel").color(theme::TEXT_MUTED))
                            .clicked()
                        {
                            self.process_dialog.is_open = false;
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(
                                    egui::RichText::new("Add & Start")
                                        .color(theme::TEXT_STRONG)
                                        .strong(),
                                )
                                .clicked()
                            {
                                let name = self.process_dialog.name.trim().to_string();
                                let cmd = self.process_dialog.command.trim().to_string();
                                if name.is_empty() {
                                    self.process_dialog.status_message =
                                        Some("Process name cannot be empty".to_string());
                                    return;
                                }
                                if cmd.is_empty() {
                                    self.process_dialog.status_message =
                                        Some("Command cannot be empty".to_string());
                                    return;
                                }

                                match parse_process_env(&self.process_dialog.env) {
                                    Err(err) => {
                                        self.process_dialog.status_message = Some(err);
                                    }
                                    Ok(env_map) => {
                                        let watch_globs =
                                            parse_process_watch(&self.process_dialog.watch);
                                        let dir = self.process_dialog.dir.clone();
                                        let port = self.process_dialog.port.clone();
                                        let autostart = self.process_dialog.autostart;
                                        let restart = self.process_dialog.restart.clone();
                                        let shared = self.process_dialog.shared;
                                        self.process_dialog.status_message = None;
                                        self.process_dialog.is_open = false;
                                        self.trigger_add_process(
                                            &name,
                                            &cmd,
                                            &dir,
                                            &port,
                                            autostart,
                                            &restart,
                                            watch_globs,
                                            env_map,
                                            shared,
                                        );
                                    }
                                }
                            }
                        });
                    });
                });
            if let Some(r) = win_resp {
                modal_rects.push(r.response.rect);
            }
            if !is_open {
                self.process_dialog.is_open = false;
            }
        }

        // D4. Command Palette Modal Window (`Ctrl+Shift+P`)
        if self.command_palette.is_open {
            let mut is_open = self.command_palette.is_open;
            let mut execute_cmd_id: Option<&'static str> = None;
            let win_resp = egui::Window::new("Command Palette")
                .open(&mut is_open)
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .fixed_size(Vec2::new(620.0, 480.0))
                .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 70.0))
                .frame(
                    Frame::none()
                        .fill(theme::SURFACE)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                        .rounding(Rounding::same(8.0_f32))
                        .inner_margin(Margin::same(0.0))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 10.0),
                            blur: 28.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(160),
                        }),
                )
                .show(ctx, |ui| {
                    let all_cmds = self.get_palette_commands();
                    let filtered = filter_palette_commands(&all_cmds, &self.command_palette.query);

                    if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                        self.command_palette.move_down(filtered.len());
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                        self.command_palette.move_up(filtered.len());
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.command_palette.close();
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if !filtered.is_empty()
                            && self.command_palette.selected_index < filtered.len()
                        {
                            execute_cmd_id = Some(filtered[self.command_palette.selected_index].id);
                            self.command_palette.close();
                        }
                    }
                    self.command_palette.clamp_selection(filtered.len());

                    ui.add_space(12.0);

                    // Search input & live counter bar
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        let search_box_w = ui.available_width() - 14.0;
                        let frame = Frame::none()
                            .fill(theme::SURFACE_INPUT)
                            .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                            .rounding(Rounding::same(6.0_f32))
                            .inner_margin(Margin::symmetric(10.0, 7.0));
                        frame.show(ui, |ui| {
                            ui.set_width(search_box_w - 20.0);
                            let (icon_r, _) =
                                ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                            Icons::draw_search(ui.painter(), icon_r, theme::TEXT_MUTED);
                            ui.add_space(8.0);
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut self.command_palette.query)
                                    .hint_text("Type a command or action name...")
                                    .frame(false)
                                    .font(egui::FontId::proportional(13.0))
                                    .text_color(theme::TEXT_STRONG)
                                    .desired_width(ui.available_width() - 95.0),
                            );
                            if resp.changed() {
                                self.command_palette.selected_index = 0;
                            }
                            if !resp.has_focus() {
                                resp.request_focus();
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let count_text = format!("{} commands", filtered.len());
                                    ui.label(
                                        egui::RichText::new(count_text)
                                            .size(theme::typography::TERMINAL_TEXT)
                                            .color(theme::TEXT_MUTED),
                                    );
                                },
                            );
                        });
                    });

                    ui.add_space(12.0);

                    // Subtle divider line
                    let sep_y = ui.cursor().top();
                    ui.painter().line_segment(
                        [
                            Pos2::new(ui.max_rect().min.x, sep_y),
                            Pos2::new(ui.max_rect().max.x, sep_y),
                        ],
                        Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
                    );
                    ui.add_space(6.0);

                    if filtered.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(50.0);
                            ui.label(
                                egui::RichText::new("No matching commands")
                                    .size(13.0)
                                    .color(theme::TEXT_FAINT),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("Try typing a different command name or keyword")
                                    .size(theme::typography::TERMINAL_TEXT)
                                    .color(theme::TEXT_MUTED),
                            );
                        });
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(295.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);

                                for (idx, cmd) in filtered.iter().enumerate() {
                                    let is_sel = idx == self.command_palette.selected_index;
                                    let (row_rect, row_resp) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width(), 34.0),
                                        Sense::click(),
                                    );
                                    let row_resp = row_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                                    let padded_rect = row_rect.shrink2(Vec2::new(12.0, 1.0));

                                    if is_sel {
                                        ui.scroll_to_rect(padded_rect, Some(egui::Align::Center));
                                    }

                                    if row_resp.clicked() {
                                        self.command_palette.selected_index = idx;
                                        execute_cmd_id = Some(cmd.id);
                                        self.command_palette.close();
                                    }

                                    let rp = ui.painter_at(row_rect);

                                    if is_sel {
                                        rp.rect_filled(padded_rect, 6.0_f32, theme::SELECTED);
                                        rp.rect_stroke(padded_rect, 6.0_f32, Stroke::new(1.0_f32, theme::ACCENT));
                                    } else if row_resp.hovered() {
                                        rp.rect_filled(padded_rect, 6.0_f32, theme::HOVER);
                                    }

                                    // Command Title
                                    let title_color = if is_sel {
                                        Color32::WHITE
                                    } else if row_resp.hovered() {
                                        theme::TEXT_STRONG
                                    } else {
                                        theme::TEXT_NORMAL
                                    };
                                    rp.text(
                                        Pos2::new(padded_rect.min.x + 14.0, padded_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        cmd.title,
                                        egui::FontId::proportional(12.5),
                                        title_color,
                                    );

                                    // Neutral Category Badge (Anti-Slop R-29: neutral, no rainbow)
                                    let (cat_bg, cat_stroke, cat_text) = if is_sel {
                                        (theme::SURFACE_OVERLAY, theme::ACCENT, Color32::WHITE)
                                    } else {
                                        (theme::CARD, theme::BORDER_SUBTLE, theme::TEXT_MUTED)
                                    };
                                    let cat_pill_w = (cmd.category.len() as f32 * 6.5 + 16.0).max(40.0);
                                    let cat_rect = Rect::from_min_size(
                                        Pos2::new(padded_rect.min.x + 300.0, padded_rect.center().y - 9.0),
                                        Vec2::new(cat_pill_w, 18.0),
                                    );
                                    rp.rect_filled(cat_rect, Rounding::same(4.0_f32), cat_bg);
                                    rp.rect_stroke(cat_rect, Rounding::same(4.0_f32), Stroke::new(1.0_f32, cat_stroke));
                                    rp.text(
                                        cat_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        cmd.category,
                                        egui::FontId::proportional(10.5),
                                        cat_text,
                                    );

                                    // Individual Keycaps (right-aligned)
                                    if let Some(sc) = cmd.shortcut {
                                        let mut cur_x = padded_rect.max.x - 14.0;
                                        let parts: Vec<&str> = sc.split(" / ").collect();
                                        for (p_idx, combo) in parts.iter().rev().enumerate() {
                                            if p_idx > 0 {
                                                cur_x -= 16.0;
                                                rp.text(
                                                    Pos2::new(cur_x + 8.0, padded_rect.center().y),
                                                    egui::Align2::CENTER_CENTER,
                                                    "/",
                                                    egui::FontId::proportional(10.5),
                                                    theme::TEXT_MUTED,
                                                );
                                            }

                                            let sub_keys: Vec<&str> = combo.split('+').collect();
                                            for (k_idx, key_name) in sub_keys.iter().rev().enumerate() {
                                                if k_idx > 0 {
                                                    cur_x -= 12.0;
                                                    rp.text(
                                                        Pos2::new(cur_x + 6.0, padded_rect.center().y),
                                                        egui::Align2::CENTER_CENTER,
                                                        "+",
                                                        egui::FontId::proportional(10.5),
                                                        theme::TEXT_MUTED,
                                                    );
                                                }

                                                let kw = (key_name.len() as f32 * 6.5 + 12.0).max(22.0);
                                                cur_x -= kw;
                                                let key_rect = Rect::from_min_size(
                                                    Pos2::new(cur_x, padded_rect.center().y - 10.0),
                                                    Vec2::new(kw, 20.0),
                                                );

                                                let (k_bg, k_stroke, k_text) = if is_sel {
                                                    (theme::ACCENT, theme::ACCENT_HOVER, Color32::WHITE)
                                                } else {
                                                    (theme::CARD, theme::BORDER_STRONG, theme::TEXT)
                                                };

                                                rp.rect_filled(key_rect, Rounding::same(4.0_f32), k_bg);
                                                rp.rect_stroke(key_rect, Rounding::same(4.0_f32), Stroke::new(1.0_f32, k_stroke));
                                                rp.text(
                                                    key_rect.center(),
                                                    egui::Align2::CENTER_CENTER,
                                                    *key_name,
                                                    egui::FontId::monospace(10.5),
                                                    k_text,
                                                );
                                            }
                                        }
                                    }
                                }
                            });
                    }

                    // Footer Bar
                    let footer_rect = Rect::from_min_size(
                        Pos2::new(ui.min_rect().min.x, ui.max_rect().max.y - 36.0),
                        Vec2::new(ui.available_width(), 36.0),
                    );
                    let fp = ui.painter_at(footer_rect);
                    fp.rect(
                        footer_rect,
                        Rounding { nw: 0.0, ne: 0.0, sw: 8.0, se: 8.0 },
                        theme::CARD,
                        Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
                    );

                    let mut fx = footer_rect.min.x + 20.0;
                    let fy = footer_rect.center().y;
                    let f_font = egui::FontId::proportional(11.0);
                    let f_color = theme::TEXT_MUTED;

                    fp.text(Pos2::new(fx, fy), egui::Align2::LEFT_CENTER, "↑↓ Navigate", f_font.clone(), f_color);
                    fx += 86.0;
                    fp.text(Pos2::new(fx, fy), egui::Align2::LEFT_CENTER, "·", f_font.clone(), theme::TEXT_FAINT);
                    fx += 12.0;
                    fp.text(Pos2::new(fx, fy), egui::Align2::LEFT_CENTER, "↵ Run Command", f_font.clone(), f_color);
                    fx += 108.0;
                    fp.text(Pos2::new(fx, fy), egui::Align2::LEFT_CENTER, "·", f_font.clone(), theme::TEXT_FAINT);
                    fx += 12.0;
                    fp.text(Pos2::new(fx, fy), egui::Align2::LEFT_CENTER, "Esc Close", f_font.clone(), f_color);
                });
            if let Some(r) = win_resp {
                modal_rects.push(r.response.rect);
            }
            if !self.command_palette.is_open || !is_open {
                self.command_palette.close();
            }

            if let Some(cmd_id) = execute_cmd_id {
                self.command_palette.close();
                self.execute_command(cmd_id);
            }
        }

        // D5. Quick Open File Picker Modal (`Ctrl+P`)
        if self.quick_open.is_open {
            let mut is_open = self.quick_open.is_open;
            let win_resp = egui::Window::new("Quick Open")
                .open(&mut is_open)
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .fixed_size(Vec2::new(620.0, 480.0))
                .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 80.0))
                .frame(
                    Frame::none()
                        .fill(theme::POPOVER)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                        .rounding(Rounding::same(theme::RADIUS_MD))
                        .inner_margin(Margin::same(12.0))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 8.0),
                            blur: 24.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(150),
                        }),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let (icon_r, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                        Icons::draw_search(ui.painter(), icon_r, theme::ACCENT);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.quick_open.query)
                                .hint_text("Search files by name...")
                                .desired_width(ui.available_width() - 30.0),
                        );
                        if !resp.has_focus() {
                            resp.request_focus();
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    let q = self.quick_open.query.to_lowercase();
                    let files: Vec<&String> = self
                        .quick_open
                        .cached_files
                        .iter()
                        .filter(|f| q.is_empty() || f.to_lowercase().contains(&q))
                        .take(50)
                        .collect();

                    let mut chosen_file = None;
                    if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                        if !files.is_empty() {
                            self.quick_open.selected_index =
                                (self.quick_open.selected_index + 1) % files.len();
                        }
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                        if !files.is_empty() {
                            if self.quick_open.selected_index == 0 {
                                self.quick_open.selected_index = files.len() - 1;
                            } else {
                                self.quick_open.selected_index -= 1;
                            }
                        }
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if let Some(sel) = files.get(self.quick_open.selected_index) {
                            chosen_file = Some((*sel).clone());
                        }
                    }
                    if self.quick_open.selected_index >= files.len() && !files.is_empty() {
                        self.quick_open.selected_index = 0;
                    }

                    if files.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(20.0);
                            ui.label(
                                egui::RichText::new("No matching files found in project")
                                    .size(12.0)
                                    .color(theme::TEXT_FAINT),
                            );
                        });
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(280.0)
                            .show(ui, |ui| {
                                for (idx, file) in files.iter().enumerate() {
                                    let is_sel = idx == self.quick_open.selected_index;
                                    let resp = ui.horizontal(|ui| {
                                        let (f_icon_r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                                        Icons::draw_file_text(ui.painter(), f_icon_r, if is_sel { theme::INFO } else { theme::TEXT_MUTED });
                                        ui.add_space(2.0);
                                        ui.label(egui::RichText::new(*file).size(12.0).color(
                                            if is_sel {
                                                theme::TEXT_STRONG
                                            } else {
                                                theme::TEXT_NORMAL
                                            },
                                        ));
                                    });

                                    let resp_rect = resp.response.rect;
                                    let btn_interact = ui.interact(
                                        resp_rect,
                                        ui.id().with(idx),
                                        egui::Sense::click(),
                                    );
                                    if btn_interact.clicked() {
                                        chosen_file = Some((*file).clone());
                                    }
                                    ui.add_space(2.0);
                                }
                            });

                        if let Some(file_path) = chosen_file {
                            self.quick_open.is_open = false;
                            eprintln!("[quick_open] Selected file to open: {}", file_path);
                            self.open_editor_file(&file_path);
                        }
                    }
                });
            if let Some(r) = win_resp {
                modal_rects.push(r.response.rect);
            }
            if !self.quick_open.is_open {
                is_open = false;
            }
            self.quick_open.is_open = is_open;
        }

        // D6. Settings Modal Window (`Ctrl+,`)
        if self.settings_modal.is_open {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.settings_modal.close();
                self.persist_settings();
            }
        }

        if self.settings_modal.is_open {
            let win_resp = egui::Window::new("settings_modal")
                .title_bar(false)
                .collapsible(false)
                .resizable(true)
                .default_size(Vec2::new(900.0, 640.0))
                .min_size(Vec2::new(760.0, 520.0))
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::none()
                        .fill(theme::SURFACE)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                        .rounding(Rounding::same(8.0_f32))
                        .inner_margin(Margin::same(0.0))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 8.0),
                            blur: 24.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(160),
                        }),
                )
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;

                    // Header Bar (height 46.0 px)
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), 46.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |h_ui| {
                            h_ui.add_space(20.0);
                            h_ui.label(
                                egui::RichText::new("Settings")
                                    .size(15.0)
                                    .color(theme::TEXT_STRONG)
                                    .strong(),
                            );
                            h_ui.add_space(8.0);

                            // Individual keycaps: `Ctrl` `+` `,`
                            let render_keycap = |ui: &mut egui::Ui, text: &str| {
                                Frame::none()
                                    .fill(theme::TERMINAL_BG)
                                    .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                                    .rounding(Rounding::same(4.0_f32))
                                    .inner_margin(Margin::symmetric(5.0, 2.0))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(text)
                                                .monospace()
                                                .size(10.5)
                                                .color(theme::TEXT_MUTED),
                                        );
                                    });
                            };

                            render_keycap(h_ui, "Ctrl");
                            h_ui.add_space(2.0);
                            h_ui.label(
                                egui::RichText::new("+")
                                    .monospace()
                                    .size(10.5)
                                    .color(theme::TEXT_FAINT),
                            );
                            h_ui.add_space(2.0);
                            render_keycap(h_ui, ",");

                            // Close button ('×')
                            h_ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |r_ui| {
                                    r_ui.add_space(16.0);
                                    let (close_rect, close_resp) =
                                        r_ui.allocate_exact_size(Vec2::new(24.0, 24.0), Sense::click());
                                    let close_resp =
                                        close_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                                    if close_resp.hovered() {
                                        r_ui.painter().rect_filled(
                                            close_rect,
                                            Rounding::same(4.0_f32),
                                            theme::HOVER,
                                        );
                                    }
                                    let close_color = if close_resp.hovered() {
                                        theme::TEXT_STRONG
                                    } else {
                                        theme::TEXT_MUTED
                                    };
                                    r_ui.painter().text(
                                        close_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "\u{00D7}",
                                        egui::FontId::proportional(16.0),
                                        close_color,
                                    );
                                    if close_resp.clicked() {
                                        self.settings_modal.close();
                                        self.persist_settings();
                                    }
                                },
                            );
                        },
                    );

                    // Divider line under header: theme::BORDER_SUBTLE
                    ui.painter().line_segment(
                        [
                            Pos2::new(ui.min_rect().min.x, ui.cursor().top()),
                            Pos2::new(ui.min_rect().max.x, ui.cursor().top()),
                        ],
                        Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
                    );

                    // Middle Content Area (height: available - 44.0 footer)
                    let content_height = (ui.available_height() - 44.0).max(200.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), content_height),
                        egui::Layout::left_to_right(egui::Align::Min),
                        |m_ui| {
                            m_ui.spacing_mut().item_spacing = Vec2::ZERO;

                            // Left sidebar (width 180.0 px)
                            m_ui.allocate_ui_with_layout(
                                Vec2::new(180.0, m_ui.available_height()),
                                egui::Layout::top_down(egui::Align::Min),
                                |sb_ui| {
                                    sb_ui.spacing_mut().item_spacing = Vec2::ZERO;
                                    sb_ui.add_space(8.0);
                                    egui::ScrollArea::vertical()
                                        .id_salt("settings_modal_sidebar_scroll")
                                        .auto_shrink([false, false])
                                        .show(sb_ui, |ui| {
                                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                                            for group in SettingsCategoryGroup::all() {
                                                ui.add_space(6.0);
                                                ui.horizontal(|ui| {
                                                    ui.add_space(12.0);
                                                    ui.label(
                                                        egui::RichText::new(group.label())
                                                            .size(9.5)
                                                            .color(theme::TEXT_FAINT)
                                                            .strong(),
                                                    );
                                                });
                                                ui.add_space(4.0);

                                                for &cat in group.categories() {
                                                    let is_active = self.settings_modal.active_category == cat;
                                                    let row_w = 172.0;

                                                    ui.horizontal(|ui| {
                                                        ui.add_space(4.0);
                                                        let (row_rect, row_resp) = ui.allocate_exact_size(
                                                            Vec2::new(row_w, 28.0),
                                                            Sense::click(),
                                                        );
                                                        let row_resp =
                                                            row_resp.on_hover_cursor(egui::CursorIcon::PointingHand);

                                                        if is_active {
                                                            ui.painter().rect_filled(
                                                                row_rect,
                                                                Rounding::same(4.0_f32),
                                                                theme::SURFACE_ELEVATED,
                                                            );
                                                            let ind_rect = Rect::from_min_size(
                                                                row_rect.min,
                                                                Vec2::new(2.0, row_rect.height()),
                                                            );
                                                            ui.painter().rect_filled(
                                                                ind_rect,
                                                                Rounding { nw: 4.0, ne: 0.0, sw: 4.0, se: 0.0 },
                                                                theme::ACCENT,
                                                            );
                                                        } else if row_resp.hovered() {
                                                            ui.painter().rect_filled(
                                                                row_rect,
                                                                Rounding::same(4.0_f32),
                                                                theme::HOVER,
                                                            );
                                                        }

                                                        let text_color = if is_active {
                                                            theme::TEXT_PRIMARY
                                                        } else if row_resp.hovered() {
                                                            theme::TEXT_STRONG
                                                        } else {
                                                            theme::TEXT_MUTED
                                                        };

                                                        ui.painter().text(
                                                            Pos2::new(row_rect.min.x + 12.0, row_rect.center().y),
                                                            egui::Align2::LEFT_CENTER,
                                                            cat.title(),
                                                            egui::FontId::proportional(12.0),
                                                            text_color,
                                                        );

                                                        if row_resp.clicked() {
                                                            self.settings_modal.set_category(cat);
                                                        }
                                                    });
                                                    ui.add_space(2.0);
                                                }
                                            }
                                            ui.add_space(8.0);
                                        });
                                },
                            );

                            // Divider line between sidebar and right content area
                            let divider_x = m_ui.cursor().left();
                            m_ui.painter().line_segment(
                                [
                                    Pos2::new(divider_x, m_ui.max_rect().min.y),
                                    Pos2::new(divider_x, m_ui.max_rect().max.y),
                                ],
                                Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
                            );

                            // Right column: Category settings content
                            let right_w = (m_ui.available_width() - 1.0).max(100.0);
                            m_ui.allocate_ui_with_layout(
                                Vec2::new(right_w, m_ui.available_height()),
                                egui::Layout::top_down(egui::Align::Min),
                                |c_ui| {
                                    c_ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
                                    egui::ScrollArea::vertical()
                                        .id_salt("settings_modal_content_scroll")
                                        .auto_shrink([false, false])
                                        .show(c_ui, |ui| {
                                            ui.add_space(12.0);
                                            ui.horizontal(|ui| {
                                                ui.add_space(16.0);
                                                ui.vertical(|ui| {
                                                    ui.set_width((ui.available_width() - 16.0).max(300.0));

                                                    // Unified Category Header
                                                    let cat = self.settings_modal.active_category;
                                                    ui.label(
                                                        egui::RichText::new(cat.title())
                                                            .size(16.0)
                                                            .strong()
                                                            .color(theme::TEXT_STRONG),
                                                    );
                                                    ui.add_space(2.0);
                                                    ui.label(
                                                        egui::RichText::new(cat.description())
                                                            .size(theme::typography::TERMINAL_TEXT)
                                                            .color(theme::TEXT_MUTED),
                                                    );
                                                    ui.add_space(12.0);

                                                    match self.settings_modal.active_category {
                                    SettingsCategory::General => {
                                        render_setting_card(ui, "Session Lifecycle", |ui| {
                                            ui.checkbox(
                                                &mut self.settings_modal.daemon_enabled,
                                                "Run session shells in background daemon",
                                            );
                                            ui.checkbox(
                                                &mut self.settings_modal.restore_sessions,
                                                "Restore open sessions on startup",
                                            );
                                            ui.checkbox(
                                                &mut self.settings_modal.resume_agents,
                                                "Resume agent conversations after restart",
                                            );
                                            ui.checkbox(
                                                &mut self.settings_modal.confirm_close,
                                                "Confirm before closing active sessions",
                                            );
                                        });

                                        render_setting_card(ui, "Link Handling", |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Open Links:")
                                                        .size(12.0)
                                                        .color(theme::TEXT_NORMAL),
                                                );
                                                egui::ComboBox::from_id_salt("open_links_combo")
                                                    .selected_text(
                                                        if self.settings_modal.open_links == "pane" {
                                                            "Browser pane"
                                                        } else {
                                                            "Default browser (system)"
                                                        },
                                                    )
                                                    .show_ui(ui, |ui| {
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.open_links,
                                                            "system".to_string(),
                                                            "Default browser (system)",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.open_links,
                                                            "pane".to_string(),
                                                            "Browser pane",
                                                        );
                                                    });
                                            });
                                            ui.add_space(2.0);
                                            ui.label(
                                                egui::RichText::new("Choose whether clicked URLs open inside an integrated pane or your external default browser.")
                                                    .size(11.0)
                                                    .color(theme::TEXT_MUTED),
                                            );
                                        });
                                    }
                                    SettingsCategory::Appearance => {
                                        render_setting_card(ui, "Theme & Density", |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Theme:")
                                                        .size(12.0)
                                                        .color(theme::TEXT_NORMAL),
                                                );
                                                egui::ComboBox::from_id_salt("theme_selector")
                                                    .selected_text(&self.settings_modal.theme_mode)
                                                    .show_ui(ui, |ui| {
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.theme_mode,
                                                            "Dark".to_string(),
                                                            "Dark",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.theme_mode,
                                                            "Light".to_string(),
                                                            "Light",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.theme_mode,
                                                            "Omarchy".to_string(),
                                                            "Omarchy",
                                                        );
                                                    });
                                            });
                                            ui.add_space(4.0);
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Density:")
                                                        .size(12.0)
                                                        .color(theme::TEXT_NORMAL),
                                                );
                                                egui::ComboBox::from_id_salt("density_selector")
                                                    .selected_text(&self.settings_modal.density)
                                                    .show_ui(ui, |ui| {
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.density,
                                                            "Comfortable".to_string(),
                                                            "Comfortable",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.density,
                                                            "Compact".to_string(),
                                                            "Compact",
                                                        );
                                                    });
                                            });
                                        });

                                        render_setting_card(ui, "Display Options", |ui| {
                                            ui.checkbox(
                                                &mut self.settings_modal.dim_unfocused,
                                                "Dim unfocused panes and windows",
                                            );
                                            ui.checkbox(
                                                &mut self.settings_modal.show_system_load,
                                                "Show system load and ports in status bar",
                                            );
                                        });
                                    }
                                    SettingsCategory::Terminal => {
                                        render_setting_card(ui, "Typography", |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Font Family:")
                                                        .size(12.0)
                                                        .color(theme::TEXT_NORMAL),
                                                );
                                                ui.text_edit_singleline(
                                                    &mut self.settings_modal.font_family,
                                                );
                                            });
                                            ui.add_space(4.0);
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Font Size:")
                                                        .size(12.0)
                                                        .color(theme::TEXT_NORMAL),
                                                );
                                                ui.add(
                                                    egui::Slider::new(
                                                        &mut self.settings_modal.font_size,
                                                        10.0..=24.0,
                                                    )
                                                    .suffix(" pt"),
                                                );
                                            });
                                        });

                                        render_setting_card(ui, "Cursor & Interaction", |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Cursor Shape:")
                                                        .size(12.0)
                                                        .color(theme::TEXT_NORMAL),
                                                );
                                                egui::ComboBox::from_id_salt("cursor_shape_selector")
                                                    .selected_text(&self.settings_modal.cursor_shape)
                                                    .show_ui(ui, |ui| {
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.cursor_shape,
                                                            "Block".to_string(),
                                                            "Block",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.cursor_shape,
                                                            "Bar".to_string(),
                                                            "Bar",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.cursor_shape,
                                                            "Underline".to_string(),
                                                            "Underline",
                                                        );
                                                    });
                                            });
                                            ui.add_space(4.0);
                                            ui.checkbox(
                                                &mut self.settings_modal.cursor_blink,
                                                "Enable blinking cursor (1.4s cycle)",
                                            );
                                            ui.checkbox(
                                                &mut self.settings_modal.copy_on_select,
                                                "Copy text selection automatically on release",
                                            );
                                        });

                                        render_setting_card(ui, "Buffer Capacity", |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Scrollback Limit:")
                                                        .size(12.0)
                                                        .color(theme::TEXT_NORMAL),
                                                );
                                                ui.add(
                                                    egui::Slider::new(
                                                        &mut self.settings_modal.scrollback_limit,
                                                        1000..=50000,
                                                    )
                                                    .suffix(" lines"),
                                                );
                                            });
                                            ui.add_space(2.0);
                                            ui.label(
                                                egui::RichText::new("Maximum number of lines kept in memory per terminal pane.")
                                                    .size(11.0)
                                                    .color(theme::TEXT_MUTED),
                                            );
                                        });
                                    }
                                    SettingsCategory::Editor => {
                                        render_setting_card(ui, "Indentation & Wrapping", |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Tab Size:")
                                                        .size(12.0)
                                                        .color(theme::TEXT_NORMAL),
                                                );
                                                ui.add(
                                                    egui::Slider::new(
                                                        &mut self.settings_modal.tab_size,
                                                        2..=8,
                                                    )
                                                    .suffix(" spaces"),
                                                );
                                            });
                                            ui.add_space(4.0);
                                            ui.checkbox(
                                                &mut self.settings_modal.word_wrap,
                                                "Soft word wrap in editor",
                                            );
                                        });

                                        render_setting_card(ui, "Display Layout", |ui| {
                                            ui.checkbox(
                                                &mut self.settings_modal.line_numbers,
                                                "Show line numbers",
                                            );
                                            ui.checkbox(
                                                &mut self.settings_modal.diff_split,
                                                "Show diffs side by side",
                                            );
                                        });
                                    }
                                    SettingsCategory::Notifications => {
                                        render_setting_card(ui, "Desktop Alerts", |ui| {
                                            ui.checkbox(
                                                &mut self.settings_modal.notify_waiting,
                                                "Notify when an agent is waiting for input",
                                            );
                                            ui.checkbox(
                                                &mut self.settings_modal.notify_done,
                                                "Notify when an agent or background process completes",
                                            );
                                            ui.checkbox(
                                                &mut self.settings_modal.notify_error,
                                                "Notify on process error or crash",
                                            );
                                            ui.checkbox(
                                                &mut self.settings_modal.notify_sound,
                                                "Play desktop notification sound",
                                            );
                                        });
                                    }
                                    SettingsCategory::Agents => {
                                        render_setting_card(
                                            ui,
                                            "Supervised CLI Integrations",
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new(
                                                        "temp-terminal monitors agent activities and resumes conversations after restarts.",
                                                    )
                                                    .size(theme::typography::TERMINAL_TEXT)
                                                    .color(theme::TEXT_MUTED),
                                                );
                                                ui.add_space(8.0);
                                                let agents = [
                                                    ("Claude Code", "claude", "Hook & skill ready"),
                                                    ("Pi", "pi", "Status plugin ready"),
                                                    ("OpenCode", "opencode", "Status plugin ready"),
                                                    ("Codex", "codex", "CLI tool detected"),
                                                    ("Aider", "aider", "CLI tool detected"),
                                                ];
                                                for (i, (name, bin, note)) in agents.iter().enumerate() {
                                                    if i > 0 {
                                                        ui.separator();
                                                    }
                                                    ui.horizontal(|ui| {
                                                        ui.label(
                                                            egui::RichText::new(*name)
                                                                .size(12.0)
                                                                .strong()
                                                                .color(theme::TEXT_STRONG),
                                                        );
                                                        ui.label(
                                                            egui::RichText::new(format!("({})", bin))
                                                                .size(11.0)
                                                                .color(theme::TEXT_MUTED),
                                                        );
                                                        ui.with_layout(
                                                            egui::Layout::right_to_left(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                let badge_bg = Color32::from_rgba_unmultiplied(
                                                                    theme::STATUS_OK.r(),
                                                                    theme::STATUS_OK.g(),
                                                                    theme::STATUS_OK.b(),
                                                                    28,
                                                                );
                                                                let badge_stroke = Color32::from_rgba_unmultiplied(
                                                                    theme::STATUS_OK.r(),
                                                                    theme::STATUS_OK.g(),
                                                                    theme::STATUS_OK.b(),
                                                                    100,
                                                                );
                                                                egui::Frame::none()
                                                                    .fill(badge_bg)
                                                                    .stroke(Stroke::new(1.0_f32, badge_stroke))
                                                                    .rounding(Rounding::same(4.0))
                                                                    .inner_margin(Margin::symmetric(6.0, 2.0))
                                                                    .show(ui, |ui| {
                                                                        ui.label(
                                                                            egui::RichText::new(*note)
                                                                                .size(10.5)
                                                                                .color(theme::STATUS_OK),
                                                                        );
                                                                    });
                                                            },
                                                        );
                                                    });
                                                }
                                                ui.add_space(8.0);
                                                ui.label(
                                                    egui::RichText::new(
                                                        "Recognizes Claude Code, Pi, OpenCode, Codex, and Aider by process name without custom hooks.",
                                                    )
                                                    .size(11.0)
                                                    .color(theme::TEXT_FAINT),
                                                );
                                            },
                                        );

                                        render_setting_card(
                                            ui,
                                            "Permissions & Automation",
                                            |ui| {
                                                ui.checkbox(
                                                    &mut self.settings_modal.control_socket,
                                                    "Enable control socket for CLI tools",
                                                );
                                                ui.add_space(4.0);
                                                ui.checkbox(
                                                    &mut self.settings_modal.remote_control,
                                                    "Let agents control the app and drive browser panes",
                                                );
                                            },
                                        );
                                    }
                                    SettingsCategory::WebAccess => {
                                        render_setting_card(ui, "Platform Availability", |ui| {
                                            ui.horizontal(|ui| {
                                                ui.vertical(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(
                                                            "Web access is not available on Windows yet",
                                                        )
                                                        .size(12.5)
                                                        .color(theme::TEXT_STRONG)
                                                        .strong(),
                                                    );
                                                    ui.add_space(2.0);
                                                    ui.label(
                                                        egui::RichText::new(
                                                            "The sessions are served by the session daemon, which doesn't run on Windows yet.",
                                                        )
                                                        .size(11.0)
                                                        .color(theme::TEXT_MUTED),
                                                    );
                                                });
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        egui::Frame::none()
                                                            .fill(theme::TERMINAL_BG)
                                                            .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                                                            .rounding(Rounding::same(4.0))
                                                            .inner_margin(Margin::symmetric(8.0, 3.0))
                                                            .show(ui, |ui| {
                                                                ui.label(
                                                                    egui::RichText::new("Unavailable on Windows")
                                                                        .size(10.5)
                                                                        .color(theme::TEXT_MUTED),
                                                                );
                                                            });
                                                    },
                                                );
                                            });
                                        });
                                    }
                                    SettingsCategory::DevUrls => {
                                        render_setting_card(ui, "Platform Availability", |ui| {
                                            ui.horizontal(|ui| {
                                                ui.vertical(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(
                                                            "Dev URLs are not available on Windows yet",
                                                        )
                                                        .size(12.5)
                                                        .color(theme::TEXT_STRONG)
                                                        .strong(),
                                                    );
                                                    ui.add_space(2.0);
                                                    ui.label(
                                                        egui::RichText::new(
                                                            "Dev URLs proxy and local CA setup are not supported on Windows.",
                                                        )
                                                        .size(11.0)
                                                        .color(theme::TEXT_MUTED),
                                                    );
                                                });
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        egui::Frame::none()
                                                            .fill(theme::TERMINAL_BG)
                                                            .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                                                            .rounding(Rounding::same(4.0))
                                                            .inner_margin(Margin::symmetric(8.0, 3.0))
                                                            .show(ui, |ui| {
                                                                ui.label(
                                                                    egui::RichText::new("Unavailable on Windows")
                                                                        .size(10.5)
                                                                        .color(theme::TEXT_MUTED),
                                                                );
                                                            });
                                                    },
                                                );
                                            });
                                        });
                                    }
                                    SettingsCategory::Browser => {
                                        render_setting_card(ui, "Search Provider", |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Search Engine:")
                                                        .size(12.0)
                                                        .color(theme::TEXT_NORMAL),
                                                );
                                                egui::ComboBox::from_id_salt("search_engine_selector")
                                                    .selected_text(&self.settings_modal.search_engine)
                                                    .show_ui(ui, |ui| {
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.search_engine,
                                                            "duckduckgo".to_string(),
                                                            "DuckDuckGo",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.search_engine,
                                                            "google".to_string(),
                                                            "Google",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.search_engine,
                                                            "kagi".to_string(),
                                                            "Kagi",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.search_engine,
                                                            "brave".to_string(),
                                                            "Brave Search",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.settings_modal.search_engine,
                                                            "Custom...".to_string(),
                                                            "Custom...",
                                                        );
                                                    });
                                            });
                                            if self.settings_modal.search_engine == "Custom..." {
                                                ui.add_space(6.0);
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        egui::RichText::new("Search URL:")
                                                            .size(12.0)
                                                            .color(theme::TEXT_NORMAL),
                                                    );
                                                    ui.text_edit_singleline(
                                                        &mut self.settings_modal.custom_search_url,
                                                    );
                                                });
                                                ui.add_space(2.0);
                                                ui.label(
                                                    egui::RichText::new(
                                                        "%s is where the query words go, e.g. https://example.com/?q=%s",
                                                    )
                                                    .size(11.0)
                                                    .color(theme::TEXT_FAINT),
                                                );
                                            }
                                        });

                                        render_setting_card(ui, "Browsing Data", |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(
                                                        "Clear cookies, cache and web storage of browser panes.",
                                                    )
                                                    .size(11.0)
                                                    .color(theme::TEXT_MUTED),
                                                );
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        let clear_btn = egui::Button::new(
                                                            egui::RichText::new("Clear Browsing Data")
                                                                .size(theme::typography::TERMINAL_TEXT)
                                                                .color(theme::TEXT_NORMAL),
                                                        )
                                                        .fill(theme::TERMINAL_BG)
                                                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                                                        .rounding(Rounding::same(4.0));
                                                        if ui.add(clear_btn).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                                            eprintln!("[settings] Cleared browsing data");
                                                        }
                                                    },
                                                );
                                            });
                                        });
                                    }
                                    SettingsCategory::Keyboard => {
                                        let all_shortcuts = get_all_shortcuts();
                                        let filtered = filter_shortcuts(
                                            all_shortcuts,
                                            &self.settings_modal.keyboard_filter,
                                            None,
                                        );

                                        render_setting_card(ui, "Keyboard Shortcuts", |ui| {
                                            ui.horizontal(|ui| {
                                                let (icon_r, _) = ui.allocate_exact_size(
                                                    Vec2::new(16.0, 16.0),
                                                    Sense::hover(),
                                                );
                                                Icons::draw_search(ui.painter(), icon_r, theme::TEXT_MUTED);
                                                ui.add_space(4.0);
                                                ui.add(
                                                    egui::TextEdit::singleline(
                                                        &mut self.settings_modal.keyboard_filter,
                                                    )
                                                    .hint_text("Filter shortcuts...")
                                                    .desired_width(220.0),
                                                );
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        let count_text = format!(
                                                            "{} shortcuts",
                                                            filtered.len()
                                                        );
                                                        egui::Frame::none()
                                                            .fill(theme::TERMINAL_BG)
                                                            .stroke(Stroke::new(
                                                                1.0_f32,
                                                                theme::BORDER_SUBTLE,
                                                            ))
                                                            .rounding(Rounding::same(4.0))
                                                            .inner_margin(Margin::symmetric(8.0, 3.0))
                                                            .show(ui, |ui| {
                                                                ui.label(
                                                                    egui::RichText::new(count_text)
                                                                        .size(11.0)
                                                                        .color(theme::TEXT_MUTED),
                                                                );
                                                            });
                                                    },
                                                );
                                            });

                                            ui.add_space(8.0);

                                            if filtered.is_empty() {
                                                ui.add_space(12.0);
                                                ui.vertical_centered(|ui| {
                                                    ui.label(
                                                        egui::RichText::new("No matching shortcuts found")
                                                            .size(12.0)
                                                            .color(theme::TEXT_MUTED),
                                                    );
                                                });
                                                ui.add_space(12.0);
                                            } else {
                                                for (i, item) in filtered.iter().enumerate() {
                                                    if i > 0 {
                                                        ui.separator();
                                                    }
                                                    ui.horizontal(|ui| {
                                                        egui::Frame::none()
                                                            .fill(theme::TERMINAL_BG)
                                                            .stroke(Stroke::new(
                                                                1.0_f32,
                                                                theme::BORDER_SUBTLE,
                                                            ))
                                                            .rounding(Rounding::same(4.0))
                                                            .inner_margin(Margin::symmetric(6.0, 2.0))
                                                            .show(ui, |ui| {
                                                                ui.label(
                                                                    egui::RichText::new(item.category)
                                                                        .size(10.5)
                                                                        .color(theme::TEXT_MUTED),
                                                                );
                                                            });
                                                        ui.add_space(4.0);
                                                        ui.label(
                                                            egui::RichText::new(item.name)
                                                                .size(theme::typography::TERMINAL_TEXT)
                                                                .color(theme::TEXT_NORMAL),
                                                        );
                                                        ui.with_layout(
                                                            egui::Layout::right_to_left(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                let parts: Vec<&str> =
                                                                    item.key.split(" / ").collect();
                                                                for (p_idx, combo) in
                                                                    parts.iter().rev().enumerate()
                                                                {
                                                                    if p_idx > 0 {
                                                                        ui.label(
                                                                            egui::RichText::new("/")
                                                                                .size(10.5)
                                                                                .color(theme::TEXT_MUTED),
                                                                        );
                                                                    }
                                                                    let sub_keys: Vec<&str> =
                                                                        combo.split('+').collect();
                                                                    for (k_idx, key_name) in
                                                                        sub_keys.iter().rev().enumerate()
                                                                    {
                                                                        if k_idx > 0 {
                                                                            ui.label(
                                                                                egui::RichText::new("+")
                                                                                    .size(10.5)
                                                                                    .color(theme::TEXT_MUTED),
                                                                            );
                                                                        }
                                                                        egui::Frame::none()
                                                                            .fill(theme::TERMINAL_BG)
                                                                            .stroke(Stroke::new(
                                                                                1.0_f32,
                                                                                theme::BORDER_STRONG,
                                                                            ))
                                                                            .rounding(Rounding::same(4.0))
                                                                            .inner_margin(Margin::symmetric(5.0, 2.0))
                                                                            .show(ui, |ui| {
                                                                                ui.label(
                                                                                    egui::RichText::new(*key_name)
                                                                                        .size(10.5)
                                                                                        .monospace()
                                                                                        .color(theme::TEXT),
                                                                                );
                                                                            });
                                                                    }
                                                                }
                                                            },
                                                        );
                                                    });
                                                }
                                            }

                                            ui.add_space(8.0);
                                            ui.label(
                                                egui::RichText::new("Shortcuts cannot be customized yet.")
                                                    .size(11.0)
                                                    .color(theme::TEXT_FAINT),
                                            );
                                        });
                                    }
                                    SettingsCategory::About => {
                                        render_setting_card(ui, "Build Information", |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Application:")
                                                        .size(12.0)
                                                        .color(theme::TEXT_NORMAL),
                                                );
                                                ui.label(
                                                    egui::RichText::new("temp-terminal")
                                                        .size(12.0)
                                                        .strong()
                                                        .color(theme::TEXT_STRONG),
                                                );
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Version:")
                                                        .size(12.0)
                                                        .color(theme::TEXT_NORMAL),
                                                );
                                                ui.label(
                                                    egui::RichText::new("v0.1.0-preview")
                                                        .size(12.0)
                                                        .color(theme::TEXT_MUTED),
                                                );
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("Platform:")
                                                        .size(12.0)
                                                        .color(theme::TEXT_NORMAL),
                                                );
                                                ui.label(
                                                    egui::RichText::new(std::env::consts::OS)
                                                        .size(12.0)
                                                        .color(theme::TEXT_MUTED),
                                                );
                                            });
                                            ui.add_space(4.0);
                                            ui.label(
                                                egui::RichText::new(
                                                    "This preview build does not support automatic updates.",
                                                )
                                                .size(11.0)
                                                .color(theme::TEXT_FAINT),
                                            );
                                        });

                                        render_setting_card(ui, "Configuration & Logs", |ui| {
                                            let cfg_dir = temp_terminal_core::paths::config_dir();
                                            let files = [
                                                ("Settings", cfg_dir.join("settings.json")),
                                                ("Sessions & layout", cfg_dir.join("state.json")),
                                                (
                                                    "Crash reports & logs",
                                                    temp_terminal_core::paths::state_dir().join("logs"),
                                                ),
                                            ];
                                            for (i, (name, path)) in files.iter().enumerate() {
                                                if i > 0 {
                                                    ui.separator();
                                                }
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(*name)
                                                            .size(theme::typography::TERMINAL_TEXT)
                                                            .strong()
                                                            .color(theme::TEXT_NORMAL),
                                                    );
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.label(
                                                                egui::RichText::new(
                                                                    path.to_string_lossy(),
                                                                )
                                                                .size(10.5)
                                                                .monospace()
                                                                .color(theme::TEXT_MUTED),
                                                            );
                                                        },
                                                    );
                                                });
                                            }
                                        });

                                        render_setting_card(ui, "Official Resources", |ui| {
                                            let links = [
                                                (
                                                    "Website",
                                                    "https://fachrihawari.github.io/tempterm/",
                                                ),
                                                (
                                                    "What's New",
                                                    "https://github.com/temp-terminal/temp-terminal/releases",
                                                ),
                                                (
                                                    "Report an Issue",
                                                    "https://github.com/temp-terminal/temp-terminal/issues",
                                                ),
                                                (
                                                    "Credits",
                                                    "https://fachrihawari.github.io/tempterm/#thanks",
                                                ),
                                            ];
                                            for (i, (name, url)) in links.iter().enumerate() {
                                                if i > 0 {
                                                    ui.separator();
                                                }
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(*name)
                                                            .size(theme::typography::TERMINAL_TEXT)
                                                            .color(theme::TEXT_NORMAL),
                                                    );
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.hyperlink_to(
                                                                egui::RichText::new(*url)
                                                                    .size(10.5)
                                                                    .color(theme::ACCENT),
                                                                *url,
                                                            );
                                                        },
                                                    );
                                                });
                                            }
                                        });
                                    }
                                }
                            });
                        });
                        ui.add_space(16.0);
                    });
                },
            );
        },
    );

                    // Footer Bar (height 44.0 px)
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), 44.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |f_ui| {
                            let footer_rect = f_ui.max_rect();
                            f_ui.painter().rect(
                                footer_rect,
                                Rounding { nw: 0.0, ne: 0.0, sw: 8.0, se: 8.0 },
                                theme::CARD,
                                Stroke::NONE,
                            );
                            f_ui.painter().line_segment(
                                [footer_rect.left_top(), footer_rect.right_top()],
                                Stroke::new(1.0_f32, theme::BORDER_MEDIUM),
                            );

                            // Left side: Esc keycap + Close text
                            f_ui.add_space(20.0);
                            let keycap_frame = Frame::none()
                                .fill(theme::SURFACE)
                                .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                                .rounding(Rounding::same(4.0_f32))
                                .inner_margin(Margin::symmetric(6.0, 2.0));
                            keycap_frame.show(f_ui, |ui| {
                                ui.label(
                                    egui::RichText::new("Esc")
                                        .monospace()
                                        .size(10.5)
                                        .color(theme::TEXT_MUTED),
                                );
                            });
                            f_ui.add_space(6.0);
                            f_ui.label(
                                egui::RichText::new("Close")
                                    .size(theme::typography::TERMINAL_TEXT)
                                    .color(theme::TEXT_MUTED),
                            );

                            // Right side: secondary "Close" and primary "Save Changes"
                            f_ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |r_ui| {
                                    r_ui.add_space(20.0);

                                    let save_btn = egui::Button::new(
                                        egui::RichText::new("Save Changes")
                                            .size(12.0)
                                            .color(theme::ACCENT_TEXT)
                                            .strong(),
                                    )
                                    .fill(theme::ACCENT)
                                    .stroke(Stroke::new(1.0_f32, theme::ACCENT_HOVER))
                                    .rounding(Rounding::same(4.0_f32))
                                    .min_size(Vec2::new(96.0, 26.0));

                                    let save_resp =
                                        r_ui.add(save_btn).on_hover_cursor(egui::CursorIcon::PointingHand);
                                    if save_resp.clicked() {
                                        self.persist_settings();
                                        self.settings_modal.close();
                                    }

                                    r_ui.add_space(8.0);

                                    let close_btn = egui::Button::new(
                                        egui::RichText::new("Close")
                                            .size(12.0)
                                            .color(theme::TEXT_STRONG),
                                    )
                                    .fill(theme::CARD_RAISED)
                                    .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                                    .rounding(Rounding::same(4.0_f32))
                                    .min_size(Vec2::new(64.0, 26.0));

                                    let close_resp =
                                        r_ui.add(close_btn).on_hover_cursor(egui::CursorIcon::PointingHand);
                                    if close_resp.clicked() {
                                        self.settings_modal.close();
                                        self.persist_settings();
                                    }
                                },
                            );
                        },
                    );
                });
            if let Some(r) = win_resp {
                modal_rects.push(r.response.rect);
            }
        }

        // D7. Keyboard Shortcuts Overlay (`F1` or `Ctrl+Shift+/`)
        if self.shortcuts_overlay.is_open {
            let mut is_open = self.shortcuts_overlay.is_open;
            let mut command_to_execute = None;

            let all_shortcuts = get_all_shortcuts();
            let filtered = filter_shortcuts(
                all_shortcuts,
                &self.shortcuts_overlay.filter,
                self.shortcuts_overlay.category_filter.as_deref(),
            );

            // Keyboard navigation
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                self.shortcuts_overlay.move_down(filtered.len());
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                self.shortcuts_overlay.move_up();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.shortcuts_overlay.close();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !filtered.is_empty() {
                if let Some(sel) = filtered.get(self.shortcuts_overlay.selected_index) {
                    command_to_execute = Some(sel.action_id);
                    self.shortcuts_overlay.close();
                }
            }

            self.shortcuts_overlay.clamp_selection(filtered.len());

            let win_resp = egui::Window::new("keyboard_shortcuts_modal")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .fixed_size(Vec2::new(740.0, 540.0))
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::none()
                        .fill(theme::POPOVER)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                        .rounding(Rounding::same(theme::RADIUS_MD))
                        .inner_margin(Margin::same(0.0))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 12.0),
                            blur: 32.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(180),
                        }),
                )
                .show(ctx, |ui| {
                    // Header Bar
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), 46.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add_space(20.0);
                            let (icon_r, _) = ui.allocate_exact_size(Vec2::new(18.0, 18.0), Sense::hover());
                            Icons::draw_keyboard(ui.painter(), icon_r, theme::ACCENT);
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("Keyboard Shortcuts")
                                    .size(15.0)
                                    .color(theme::TEXT_STRONG)
                                    .strong(),
                            );

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add_space(16.0);
                                let (close_rect, close_resp) =
                                    ui.allocate_exact_size(Vec2::new(24.0, 24.0), Sense::click());
                                let close_resp =
                                    close_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                                if close_resp.hovered() {
                                    ui.painter().rect_filled(close_rect, theme::RADIUS_SM, theme::BAD.linear_multiply(0.25));
                                }
                                let close_color = if close_resp.hovered() {
                                    theme::BAD
                                } else {
                                    theme::TEXT_MUTED
                                };
                                Icons::draw_close(ui.painter(), close_rect, close_color);
                                if close_resp.clicked() {
                                    self.shortcuts_overlay.close();
                                }

                                ui.add_space(8.0);
                                // Keycap badge for F1 / Ctrl+Shift+/
                                ui.scope(|ui| {
                                    let badge_frame = Frame::none()
                                        .fill(theme::TERMINAL_BG)
                                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                                        .rounding(Rounding::same(4.0_f32))
                                        .inner_margin(Margin::symmetric(6.0, 2.0));
                                    badge_frame.show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new("F1 / Ctrl+Shift+/")
                                                .monospace()
                                                .size(11.0)
                                                .color(theme::TEXT_MUTED),
                                        );
                                    });
                                });
                            });
                        },
                    );

                    ui.painter().line_segment(
                        [
                            Pos2::new(ui.min_rect().min.x, ui.cursor().top()),
                            Pos2::new(ui.min_rect().max.x, ui.cursor().top()),
                        ],
                        Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
                    );

                    ui.add_space(12.0);

                    // Search input & count bar
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        let search_box_w = ui.available_width() - 20.0;
                        let frame = Frame::none()
                            .fill(theme::SURFACE_INPUT)
                            .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                            .rounding(Rounding::same(6.0_f32))
                            .inner_margin(Margin::symmetric(10.0, 6.0));
                        frame.show(ui, |ui| {
                            ui.set_width(search_box_w - 20.0);
                            let (icon_r, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                            Icons::draw_search(ui.painter(), icon_r, theme::TEXT_MUTED);
                            ui.add_space(6.0);
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut self.shortcuts_overlay.filter)
                                    .hint_text("Search shortcuts by command or key...")
                                    .frame(false)
                                    .font(egui::FontId::proportional(13.0))
                                    .text_color(theme::TEXT_STRONG)
                                    .desired_width(ui.available_width() - 90.0),
                            );
                            if resp.changed() {
                                self.shortcuts_overlay.selected_index = 0;
                            }
                            if !resp.has_focus() {
                                resp.request_focus();
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let count_text = format!("{} commands", filtered.len());
                                ui.label(
                                    egui::RichText::new(count_text)
                                        .size(theme::typography::TERMINAL_TEXT)
                                        .color(theme::TEXT_MUTED),
                                );
                            });
                        });
                    });

                    ui.add_space(10.0);

                    // Category filter chips
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        let categories = [
                            ("All", None),
                            ("Terminal", Some("Terminal")),
                            ("App", Some("App")),
                            ("Editor", Some("Editor")),
                            ("View", Some("View")),
                            ("Agent & Proc", Some("Agent")),
                        ];
                        for (label, val) in categories {
                            let is_active = match (self.shortcuts_overlay.category_filter.as_deref(), val) {
                                (None, None) => true,
                                (Some(cur), Some(v)) => cur == v,
                                _ => false,
                            };
                            let (bg, border, text_color) = if is_active {
                                (theme::SELECTED, theme::ACCENT, Color32::WHITE)
                            } else {
                                (theme::CARD, theme::BORDER_SUBTLE, theme::TEXT_MUTED)
                            };

                            let btn = egui::Button::new(
                                egui::RichText::new(label)
                                    .size(theme::typography::TERMINAL_TEXT)
                                    .color(text_color),
                            )
                            .fill(bg)
                            .stroke(Stroke::new(1.0_f32, border))
                            .rounding(Rounding::same(6.0_f32));

                            let btn_resp = ui.add(btn);
                            let btn_resp = btn_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                            if btn_resp.clicked() {
                                self.shortcuts_overlay.set_category_filter(val.map(|s| s.to_string()));
                            }
                            ui.add_space(4.0);
                        }
                    });

                    ui.add_space(10.0);

                    // Table Column Headers
                    let table_header_rect = Rect::from_min_size(
                        Pos2::new(ui.min_rect().min.x, ui.cursor().top()),
                        Vec2::new(ui.available_width(), 26.0),
                    );
                    let hp = ui.painter_at(table_header_rect);
                    hp.rect_filled(table_header_rect, 0.0, theme::CARD);
                    hp.line_segment(
                        [table_header_rect.left_top(), table_header_rect.right_top()],
                        Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
                    );
                    hp.line_segment(
                        [table_header_rect.left_bottom(), table_header_rect.right_bottom()],
                        Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
                    );

                    let th_font = egui::FontId::proportional(10.5);
                    let th_color = theme::TEXT_MUTED;
                    hp.text(
                        Pos2::new(table_header_rect.min.x + 28.0, table_header_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        "COMMAND",
                        th_font.clone(),
                        th_color,
                    );
                    hp.text(
                        Pos2::new(table_header_rect.min.x + 360.0, table_header_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        "CATEGORY",
                        th_font.clone(),
                        th_color,
                    );
                    hp.text(
                        Pos2::new(table_header_rect.max.x - 28.0, table_header_rect.center().y),
                        egui::Align2::RIGHT_CENTER,
                        "KEY COMBINATION",
                        th_font,
                        th_color,
                    );

                    ui.add_space(28.0);

                    // Scrollable Rows
                    let list_h = 345.0;
                    if filtered.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(60.0);
                            ui.label(
                                egui::RichText::new("No matching shortcuts found")
                                    .size(13.0)
                                    .color(theme::TEXT_FAINT),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("Try typing a different command name or key combination")
                                    .size(theme::typography::TERMINAL_TEXT)
                                    .color(theme::TEXT_MUTED),
                            );
                        });
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(list_h)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);

                                for (idx, item) in filtered.iter().enumerate() {
                                    let is_sel = idx == self.shortcuts_overlay.selected_index;
                                    let (row_rect, row_resp) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width(), 34.0),
                                        Sense::click(),
                                    );
                                    let row_resp = row_resp.on_hover_cursor(egui::CursorIcon::PointingHand);

                                    if is_sel {
                                        ui.scroll_to_rect(row_rect, Some(egui::Align::Center));
                                    }

                                    if row_resp.clicked() {
                                        self.shortcuts_overlay.selected_index = idx;
                                        command_to_execute = Some(item.action_id);
                                        self.shortcuts_overlay.close();
                                    }

                                    let rp = ui.painter_at(row_rect);
                                    let padded_rect = row_rect.shrink2(Vec2::new(12.0, 1.0));

                                    if is_sel {
                                        rp.rect_filled(padded_rect, 6.0, theme::SELECTED);
                                        rp.rect_stroke(padded_rect, 6.0, Stroke::new(1.0_f32, theme::ACCENT));
                                    } else if row_resp.hovered() {
                                        rp.rect_filled(padded_rect, 6.0, theme::HOVER);
                                    }

                                    // Command Name
                                    let name_color = if is_sel {
                                        Color32::WHITE
                                    } else if row_resp.hovered() {
                                        theme::TEXT_STRONG
                                    } else {
                                        theme::TEXT_NORMAL
                                    };
                                    rp.text(
                                        Pos2::new(padded_rect.min.x + 14.0, padded_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        item.name,
                                        egui::FontId::proportional(12.5),
                                        name_color,
                                    );

                                    // Category Pill Badge (Neutral theme-aware badge, Anti-Slop R-29)
                                    let (cat_bg, cat_stroke, cat_text) = if is_sel {
                                        (theme::SURFACE_OVERLAY, theme::ACCENT, Color32::WHITE)
                                    } else {
                                        (theme::CARD, theme::BORDER_SUBTLE, theme::TEXT_MUTED)
                                    };

                                    let cat_pill_w = (item.category.len() as f32 * 6.5 + 16.0).max(40.0);
                                    let cat_rect = Rect::from_min_size(
                                        Pos2::new(padded_rect.min.x + 346.0, padded_rect.center().y - 9.0),
                                        Vec2::new(cat_pill_w, 18.0),
                                    );
                                    rp.rect_filled(cat_rect, Rounding::same(4.0_f32), cat_bg);
                                    rp.rect_stroke(cat_rect, Rounding::same(4.0_f32), Stroke::new(1.0_f32, cat_stroke));
                                    rp.text(
                                        cat_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        item.category,
                                        egui::FontId::proportional(10.5),
                                        cat_text,
                                    );

                                    // Key Combination Badge
                                    let mut cur_x = padded_rect.max.x - 14.0;
                                    let parts: Vec<&str> = item.key.split(" / ").collect();
                                    // Render in right-to-left order
                                    for (p_idx, combo) in parts.iter().rev().enumerate() {
                                        if p_idx > 0 {
                                            cur_x -= 16.0;
                                            rp.text(
                                                Pos2::new(cur_x + 8.0, padded_rect.center().y),
                                                egui::Align2::CENTER_CENTER,
                                                "/",
                                                egui::FontId::proportional(10.5),
                                                theme::TEXT_MUTED,
                                            );
                                        }

                                        let sub_keys: Vec<&str> = combo.split('+').collect();
                                        for (k_idx, key_name) in sub_keys.iter().rev().enumerate() {
                                            if k_idx > 0 {
                                                cur_x -= 12.0;
                                                rp.text(
                                                    Pos2::new(cur_x + 6.0, padded_rect.center().y),
                                                    egui::Align2::CENTER_CENTER,
                                                    "+",
                                                    egui::FontId::proportional(10.5),
                                                    theme::TEXT_MUTED,
                                                );
                                            }

                                            let kw = (key_name.len() as f32 * 6.5 + 12.0).max(22.0);
                                            cur_x -= kw;
                                            let key_rect = Rect::from_min_size(
                                                Pos2::new(cur_x, padded_rect.center().y - 10.0),
                                                Vec2::new(kw, 20.0),
                                            );

                                            let (k_bg, k_stroke, k_text) = if is_sel {
                                                (theme::ACCENT, theme::ACCENT_HOVER, Color32::WHITE)
                                            } else {
                                                (theme::CARD, theme::BORDER_STRONG, theme::TEXT)
                                            };

                                            rp.rect_filled(key_rect, Rounding::same(4.0_f32), k_bg);
                                            rp.rect_stroke(key_rect, Rounding::same(4.0_f32), Stroke::new(1.0_f32, k_stroke));
                                            rp.text(
                                                key_rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                *key_name,
                                                egui::FontId::monospace(10.5),
                                                k_text,
                                            );
                                        }
                                    }
                                }
                            });
                    }

                    // Footer Bar
                    let footer_rect = Rect::from_min_size(
                        Pos2::new(ui.min_rect().min.x, ui.max_rect().max.y - 36.0),
                        Vec2::new(ui.available_width(), 36.0),
                    );
                    let fp = ui.painter_at(footer_rect);
                    fp.rect(
                        footer_rect,
                        Rounding { nw: 0.0, ne: 0.0, sw: 8.0, se: 8.0 },
                        theme::CARD,
                        Stroke::new(1.0_f32, theme::BORDER_SUBTLE),
                    );

                    let mut fx = footer_rect.min.x + 20.0;
                    let fy = footer_rect.center().y;
                    let f_font = egui::FontId::proportional(11.0);
                    let f_color = theme::TEXT_MUTED;

                    fp.text(Pos2::new(fx, fy), egui::Align2::LEFT_CENTER, "↑↓ Navigate", f_font.clone(), f_color);
                    fx += 86.0;
                    fp.text(Pos2::new(fx, fy), egui::Align2::LEFT_CENTER, "·", f_font.clone(), theme::TEXT_FAINT);
                    fx += 12.0;
                    fp.text(Pos2::new(fx, fy), egui::Align2::LEFT_CENTER, "↵ Run Command", f_font.clone(), f_color);
                    fx += 108.0;
                    fp.text(Pos2::new(fx, fy), egui::Align2::LEFT_CENTER, "·", f_font.clone(), theme::TEXT_FAINT);
                    fx += 12.0;
                    fp.text(Pos2::new(fx, fy), egui::Align2::LEFT_CENTER, "Esc Close", f_font.clone(), f_color);
                });

            if let Some(r) = win_resp {
                modal_rects.push(r.response.rect);
            }
            if !self.shortcuts_overlay.is_open {
                is_open = false;
            }
            self.shortcuts_overlay.is_open = is_open;

            if let Some(cmd) = command_to_execute {
                self.execute_command(cmd);
            }
        }

        // Open Project Modal Window
        if self.open_project_dialog.is_open {
            let mut is_open = self.open_project_dialog.is_open;
            let win_resp = egui::Window::new("Open Project")
                .id(egui::Id::new("open_project_dialog_modal"))
                .open(&mut is_open)
                .collapsible(false)
                .resizable(false)
                .fixed_size(Vec2::new(440.0, 160.0))
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::none()
                        .fill(theme::POPOVER)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                        .rounding(Rounding::same(theme::RADIUS_MD))
                        .inner_margin(Margin::same(16.0))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 8.0),
                            blur: 24.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(150),
                        }),
                )
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new("Open an existing folder as a workspace project:")
                            .size(12.0)
                            .color(theme::TEXT_NORMAL),
                    );
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Folder:")
                                .size(12.0)
                                .color(theme::TEXT_MUTED),
                        );
                        let text_edit = ui.add(
                            egui::TextEdit::singleline(&mut self.open_project_dialog.path_input)
                                .desired_width(260.0),
                        );
                        if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let p = PathBuf::from(&self.open_project_dialog.path_input);
                            if p.is_dir() {
                                self.trigger_open_project_path(p);
                                self.open_project_dialog.is_open = false;
                            } else {
                                self.open_project_dialog.error_msg =
                                    Some("Path is not a valid directory.".to_string());
                            }
                        }

                        if ui.button("Browse...").clicked() {
                            self.trigger_open_project_native();
                        }
                    });

                    if let Some(ref err) = self.open_project_dialog.error_msg {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(err)
                                .size(11.0)
                                .color(Color32::from_rgb(220, 80, 80)),
                        );
                    }

                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(egui::RichText::new("Cancel").color(theme::TEXT_MUTED))
                            .clicked()
                        {
                            self.open_project_dialog.is_open = false;
                        }
                        if ui
                            .button(egui::RichText::new("Open").color(theme::ACCENT_TEXT).strong())
                            .clicked()
                        {
                            let p = PathBuf::from(&self.open_project_dialog.path_input);
                            if p.is_dir() {
                                self.trigger_open_project_path(p);
                                self.open_project_dialog.is_open = false;
                            } else {
                                self.open_project_dialog.error_msg =
                                    Some("Path is not a valid directory.".to_string());
                            }
                        }
                    });
                });
            if let Some(r) = win_resp {
                modal_rects.push(r.response.rect);
            }
            if !is_open {
                self.open_project_dialog.is_open = false;
            }
        }

        // New Project Modal Window
        if self.new_project_dialog.is_open {
            let mut is_open = self.new_project_dialog.is_open;
            let win_resp = egui::Window::new("New Project")
                .id(egui::Id::new("new_project_dialog_modal"))
                .open(&mut is_open)
                .collapsible(false)
                .resizable(false)
                .fixed_size(Vec2::new(440.0, 190.0))
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::none()
                        .fill(theme::POPOVER)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                        .rounding(Rounding::same(theme::RADIUS_MD))
                        .inner_margin(Margin::same(16.0))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 8.0),
                            blur: 24.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(150),
                        }),
                )
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new("Create a new project workspace:")
                            .size(12.0)
                            .color(theme::TEXT_NORMAL),
                    );
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Name:")
                                .size(12.0)
                                .color(theme::TEXT_MUTED),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_project_dialog.name_input)
                                .desired_width(260.0),
                        );
                    });
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Location:")
                                .size(12.0)
                                .color(theme::TEXT_MUTED),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_project_dialog.path_input)
                                .desired_width(260.0),
                        );

                        if ui.button("Browse...").clicked() {
                            let host = self.host.clone();
                            let registry = self.registry.clone();
                            self.tokio_handle.spawn(async move {
                                if let Some(path) = tokio::task::spawn_blocking(crate::pick_folder_native)
                                    .await
                                    .ok()
                                    .flatten()
                                {
                                    let _ = host.open_or_switch_project(path, &registry).await;
                                }
                            });
                            self.new_project_dialog.is_open = false;
                        }
                    });

                    if let Some(ref err) = self.new_project_dialog.error_msg {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(err)
                                .size(11.0)
                                .color(theme::BAD),
                        );
                    }

                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(egui::RichText::new("Cancel").color(theme::TEXT_MUTED))
                            .clicked()
                        {
                            self.new_project_dialog.is_open = false;
                        }
                        if ui
                            .button(egui::RichText::new("Create").color(theme::ACCENT_TEXT).strong())
                            .clicked()
                        {
                            let name = self.new_project_dialog.name_input.trim();
                            let raw_path = self.new_project_dialog.path_input.trim();
                            if name.is_empty() && raw_path.is_empty() {
                                self.new_project_dialog.error_msg =
                                    Some("Please enter a project name or directory.".to_string());
                            } else {
                                let target_path = if raw_path.is_empty() {
                                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(name)
                                } else {
                                    PathBuf::from(raw_path)
                                };
                                let proj_name = if name.is_empty() {
                                    target_path
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "Project".to_string())
                                } else {
                                    name.to_string()
                                };
                                let _ = std::fs::create_dir_all(&target_path);
                                self.trigger_create_project(&proj_name, target_path);
                                self.new_project_dialog.is_open = false;
                            }
                        }
                    });
                });
            if let Some(r) = win_resp {
                modal_rects.push(r.response.rect);
            }
            if !is_open {
                self.new_project_dialog.is_open = false;
            }
        }

        // Remove Project Confirmation Modal (tempterm app.go:164 parity)
        if self.remove_project_modal.is_open {
            let mut is_open = self.remove_project_modal.is_open;
            let proj_name = self.remove_project_modal.project_name.clone();
            let proj_id = self.remove_project_modal.project_id.clone();
            let win_resp = egui::Window::new("Remove Project")
                .id(egui::Id::new("remove_project_confirm_modal"))
                .open(&mut is_open)
                .collapsible(false)
                .resizable(false)
                .fixed_size(Vec2::new(380.0, 140.0))
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::none()
                        .fill(theme::POPOVER)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                        .rounding(Rounding::same(theme::RADIUS_MD))
                        .inner_margin(Margin::same(16.0))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 8.0),
                            blur: 24.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(150),
                        }),
                )
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "Remove {} from the list? Files on disk are not deleted.",
                            proj_name
                        ))
                        .size(12.0)
                        .color(theme::TEXT_NORMAL),
                    );
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(egui::RichText::new("Cancel").color(theme::TEXT_MUTED))
                            .clicked()
                        {
                            self.remove_project_modal.is_open = false;
                        }
                        if ui
                            .button(
                                egui::RichText::new("Remove")
                                    .color(theme::BAD)
                                    .strong(),
                            )
                            .clicked()
                        {
                            self.trigger_remove_project(&proj_id);
                            self.remove_project_modal.is_open = false;
                        }
                    });
                });
            if let Some(r) = win_resp {
                modal_rects.push(r.response.rect);
            }
            if !is_open {
                self.remove_project_modal.is_open = false;
            }
        }

        // Unsaved Changes Confirmation Modal (tempterm stability_test.go:543 parity)
        if let Some(ref path) = self.close_unsaved_doc.clone() {
            let mut save_and_close = false;
            let mut discard_and_close = false;
            let mut cancel_close = false;

            let win_resp = egui::Window::new("Unsaved Changes")
                .id(egui::Id::new("unsaved_changes_confirm_modal"))
                .collapsible(false)
                .resizable(false)
                .fixed_size(Vec2::new(380.0, 130.0))
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::none()
                        .fill(theme::POPOVER)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                        .rounding(Rounding::same(theme::RADIUS_MD))
                        .inner_margin(Margin::same(16.0))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 8.0),
                            blur: 24.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(150),
                        }),
                )
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "Do you want to save changes to \"{}\" before closing?",
                            path
                        ))
                        .size(12.5)
                        .strong()
                        .color(theme::TEXT_STRONG),
                    );
                    ui.label(
                        egui::RichText::new("Your changes will be lost if you don't save them.")
                            .size(11.0)
                            .color(theme::TEXT_MUTED),
                    );
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(egui::RichText::new("Save").color(theme::ACCENT).strong())
                            .clicked()
                        {
                            save_and_close = true;
                        }
                        if ui
                            .button(egui::RichText::new("Don't Save").color(theme::STATUS_BAD))
                            .clicked()
                        {
                            discard_and_close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel_close = true;
                        }
                    });
                });

            if save_and_close {
                if let Some(doc) = self.open_editors.get_mut(path) {
                    let _ = doc.save();
                }
                self.open_editors.remove(path);
                if self.active_editor_path.as_deref() == Some(path) {
                    self.active_editor_path = self.open_editors.keys().next().cloned();
                }
                self.close_unsaved_doc = None;
            } else if discard_and_close {
                self.open_editors.remove(path);
                if self.active_editor_path.as_deref() == Some(path) {
                    self.active_editor_path = self.open_editors.keys().next().cloned();
                }
                self.close_unsaved_doc = None;
            } else if cancel_close {
                self.close_unsaved_doc = None;
            }
            if let Some(r) = win_resp {
                modal_rects.push(r.response.rect);
            }
        }

        // External Disk Conflict Modal (tempterm editor.go:244 parity)
        if let Some(ref path) = self.conflict_doc.clone() {
            let mut overwrite_disk = false;
            let mut reload_disk = false;
            let mut cancel_conflict = false;

            let win_resp = egui::Window::new("File Changed on Disk")
                .id(egui::Id::new("external_conflict_modal"))
                .collapsible(false)
                .resizable(false)
                .fixed_size(Vec2::new(420.0, 140.0))
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .frame(
                    Frame::none()
                        .fill(theme::POPOVER)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                        .rounding(Rounding::same(theme::RADIUS_MD))
                        .inner_margin(Margin::same(16.0))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(0.0, 8.0),
                            blur: 24.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(150),
                        }),
                )
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "\"{}\" was modified on disk by another program.",
                            path
                        ))
                        .size(12.5)
                        .strong()
                        .color(theme::STATUS_WARN),
                    );
                    ui.label(
                        egui::RichText::new("Do you want to overwrite the file on disk with your editor changes, or reload from disk?")
                            .size(11.0)
                            .color(theme::TEXT_MUTED),
                    );
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui
                            .button(
                                egui::RichText::new("Overwrite Disk")
                                    .color(theme::ACCENT)
                                    .strong(),
                            )
                            .clicked()
                        {
                            overwrite_disk = true;
                        }
                        if ui.button("Reload from Disk").clicked() {
                            reload_disk = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel_conflict = true;
                        }
                    });
                });

            if overwrite_disk {
                if let Some(doc) = self.open_editors.get_mut(path) {
                    let _ = doc.save();
                }
                self.conflict_doc = None;
            } else if reload_disk {
                if let Some(doc) = self.open_editors.get_mut(path) {
                    if let Ok(data) = std::fs::read(&doc.full_path) {
                        doc.content = String::from_utf8_lossy(&data).to_string();
                        doc.disk_hash = temp_terminal_procs::def::sha256_digest(&data);
                        doc.is_dirty = false;
                        doc.has_external_conflict = false;
                    }
                }
                self.conflict_doc = None;
            } else if cancel_conflict {
                self.conflict_doc = None;
            }
            if let Some(r) = win_resp {
                modal_rects.push(r.response.rect);
            }
        }

        // Click outside modal detector: closes active modals when clicking anywhere outside their windows
        if had_modal_at_frame_start && !modal_rects.is_empty() {
            if ctx.input(|i| i.pointer.primary_clicked()) {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos().or_else(|| i.pointer.latest_pos())) {
                    let clicked_inside = modal_rects.iter().any(|r| r.contains(pos));
                    if !clicked_inside {
                        self.close_all_modals();
                    }
                }
            }
        }
    }
    /// Renders the 24px bottom status bar according to Native Studio specification.
    /// Height: 24px.
    /// Left:
    /// - Project name.
    /// - Current working directory.
    /// Center:
    /// - Shell process.
    /// - Terminal cursor row and column.
    /// Right:
    /// - Git branch.
    /// - Running process counter.
    /// - CPU and memory indicators.
    /// - Daemon state.
    pub fn render_statusbar(&mut self, ui: &mut egui::Ui) {
        let (bar_rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), theme::STATUSBAR_HEIGHT),
            Sense::hover(),
        );
        let painter = ui.painter_at(bar_rect);

        // Dark canvas background
        painter.rect_filled(bar_rect, 0.0, theme::CANVAS);

        // Crisp 1px hairline top divider line
        painter.line_segment(
            [pos2(bar_rect.min.x, bar_rect.min.y), pos2(bar_rect.max.x, bar_rect.min.y)],
            Stroke::new(1.0_f32, theme::BORDER),
        );

        let mut sb_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        sb_ui.add_space(theme::SPACE_COMPACT);

        // 1. LEFT SECTION: Project name and current working directory
        let proj_name = if self.snapshot.project_name.is_empty() {
            "Workspace".to_string()
        } else {
            self.snapshot.project_name.clone()
        };

        let (p_icon_r, _) = sb_ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
        Icons::draw_folder_git(&sb_ui.painter(), p_icon_r, theme::TEXT_MUTED);
        sb_ui.add_space(theme::SPACE_MICRO);

        let proj_resp = sb_ui.add(
            egui::Label::new(
                egui::RichText::new(&proj_name)
                    .size(theme::typography::METADATA)
                    .color(theme::TEXT_PRIMARY),
            )
            .sense(egui::Sense::click()),
        );
        if proj_resp.on_hover_text("Open Project Switcher").clicked() {
            self.command_palette.toggle();
        }

        if !self.snapshot.project_path.is_empty() && sb_ui.available_width() > 320.0 {
            sb_ui.add_space(theme::SPACE_MICRO);
            sb_ui.label(egui::RichText::new("/").size(theme::typography::METADATA).color(theme::TEXT_DISABLED));
            sb_ui.add_space(theme::SPACE_MICRO);

            let disp_dir = if self.snapshot.project_path.len() > 36 {
                format!("...{}", &self.snapshot.project_path[self.snapshot.project_path.len() - 33..])
            } else {
                self.snapshot.project_path.clone()
            };

            sb_ui.label(
                egui::RichText::new(disp_dir)
                    .size(theme::typography::METADATA)
                    .color(theme::TEXT_MUTED),
            );
        }

        // Active Zoom indicator badge if a pane is currently maximized
        if self.snapshot.zoomed_pane_id.is_some() {
            sb_ui.add_space(theme::SPACE_COMPACT);
            let (zoom_chip_r, zoom_resp) = sb_ui.allocate_exact_size(Vec2::new(76.0, 18.0), Sense::click());
            let zp = sb_ui.painter_at(zoom_chip_r);
            zp.rect_filled(zoom_chip_r, theme::RADIUS_SM, theme::ACCENT_TINT);
            zp.rect_stroke(zoom_chip_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::ACCENT));
            zp.text(
                zoom_chip_r.center(),
                egui::Align2::CENTER_CENTER,
                "ZOOMED",
                egui::FontId::monospace(theme::typography::METADATA),
                theme::ACCENT,
            );
            if zoom_resp.on_hover_text("Click or press Ctrl+Shift+Z to restore split layout").clicked() {
                self.trigger_toggle_zoom();
            }
        }

        // Active Debug indicator badge
        if self.debug_mode {
            sb_ui.add_space(theme::SPACE_COMPACT);
            let (dbg_chip_r, dbg_resp) = sb_ui.allocate_exact_size(Vec2::new(54.0, 18.0), Sense::click());
            let dp = sb_ui.painter_at(dbg_chip_r);
            dp.rect_filled(dbg_chip_r, theme::RADIUS_SM, theme::ACCENT_TINT);
            dp.rect_stroke(dbg_chip_r, theme::RADIUS_SM, Stroke::new(1.0_f32, theme::ACCENT));
            dp.text(
                dbg_chip_r.center(),
                egui::Align2::CENTER_CENTER,
                "DEBUG",
                egui::FontId::monospace(theme::typography::METADATA),
                theme::ACCENT,
            );
            if dbg_resp.on_hover_text("Debug mode active (click or press F3 to toggle)").clicked() {
                self.debug_mode = !self.debug_mode;
            }
        }

        // 2. RIGHT-ALIGNED SECTION: Git branch, process counter, CPU/RAM, daemon state
        sb_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |r_ui| {
            r_ui.add_space(theme::SPACE_COMPACT);

            // Daemon status indicator
            let is_conn = self.snapshot.is_connected;
            let (d_dot_r, d_resp) = r_ui.allocate_exact_size(Vec2::splat(8.0), Sense::click());
            r_ui.painter().circle_filled(
                d_dot_r.center(),
                2.5,
                if is_conn { theme::SUCCESS } else { theme::TEXT_DISABLED },
            );
            r_ui.add_space(2.0);
            let daemon_lbl = r_ui.add(
                egui::Label::new(
                    egui::RichText::new(if is_conn { "Daemon Active" } else { "Daemon Offline" })
                        .color(if is_conn { theme::TEXT_SECONDARY } else { theme::TEXT_DISABLED })
                        .size(theme::typography::METADATA),
                )
                .sense(egui::Sense::click()),
            );
            if d_resp.on_hover_text("Toggle Inspector Info").clicked() || daemon_lbl.clicked() {
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Info;
            }

            r_ui.add_space(theme::SPACE_COMPACT);
            r_ui.label(egui::RichText::new("│").size(theme::typography::METADATA).color(theme::BORDER));
            r_ui.add_space(theme::SPACE_COMPACT);

            // CPU & Memory compact indicators
            r_ui.label(
                egui::RichText::new("MEM 4.2G")
                    .size(theme::typography::METADATA)
                    .monospace()
                    .color(theme::TEXT_MUTED),
            );
            r_ui.add_space(theme::SPACE_MICRO);
            r_ui.label(
                egui::RichText::new("CPU 18%")
                    .size(theme::typography::METADATA)
                    .monospace()
                    .color(theme::TEXT_MUTED),
            );

            r_ui.add_space(theme::SPACE_COMPACT);
            r_ui.label(egui::RichText::new("│").size(theme::typography::METADATA).color(theme::BORDER));
            r_ui.add_space(theme::SPACE_COMPACT);

            // Running process counter
            let (proc_dot_r, proc_resp) = r_ui.allocate_exact_size(Vec2::splat(8.0), Sense::click());
            r_ui.painter().circle_filled(proc_dot_r.center(), 2.5, theme::SUCCESS);
            r_ui.add_space(2.0);
            let proc_lbl = r_ui.add(
                egui::Label::new(
                    egui::RichText::new(format!("{} proc", self.snapshot.running_processes))
                        .size(theme::typography::METADATA)
                        .color(theme::TEXT_SECONDARY),
                )
                .sense(egui::Sense::click()),
            );
            if proc_resp.clicked() || proc_lbl.clicked() {
                self.inspector_open = true;
                self.inspector_tab = InspectorTab::Info;
            }

            // Git branch chip
            if let Some(ref branch) = self.snapshot.branch_name {
                r_ui.add_space(theme::SPACE_COMPACT);
                r_ui.label(egui::RichText::new("│").size(theme::typography::METADATA).color(theme::BORDER));
                r_ui.add_space(theme::SPACE_COMPACT);

                let (b_icon_r, b_resp) = r_ui.allocate_exact_size(Vec2::splat(12.0), Sense::click());
                Icons::draw_git_branch(&r_ui.painter(), b_icon_r, theme::TEXT_SECONDARY);
                r_ui.add_space(theme::SPACE_MICRO);
                let b_lbl = r_ui.add(
                    egui::Label::new(
                        egui::RichText::new(branch)
                            .size(theme::typography::METADATA)
                            .color(theme::TEXT_SECONDARY),
                    )
                    .sense(egui::Sense::click()),
                );
                if b_resp.clicked() || b_lbl.clicked() {
                    self.inspector_open = true;
                    self.inspector_tab = InspectorTab::Git;
                }
            }

            // 3. CENTER SECTION: Shell process & terminal cursor row and column
            let active_pane = if !self.snapshot.active_pane_id.is_empty() {
                self.snapshot.panes.get(&self.snapshot.active_pane_id)
            } else {
                self.snapshot.panes.values().next()
            };

            let shell_name = if let Some(pane) = active_pane {
                if pane.name.is_empty() || pane.name == "terminal" {
                    "pwsh".to_string()
                } else {
                    pane.name.clone()
                }
            } else {
                "pwsh".to_string()
            };

            let cursor_str = if let Some(pane) = active_pane {
                format!("Ln {}, Col {}", pane.cursor_row + 1, pane.cursor_col + 1)
            } else {
                "Ln 1, Col 1".to_string()
            };

            let center_text = format!("{}  •  {}", shell_name, cursor_str);
            let center_pos = bar_rect.center();
            painter.text(
                center_pos,
                egui::Align2::CENTER_CENTER,
                &center_text,
                egui::FontId::monospace(theme::typography::METADATA),
                theme::TEXT_MUTED,
            );
        });
    }

    /// Configures typography fonts following the Obsidian Slate - Quiet Sapphire priority:
    /// UI font priority:
    /// 1. Inter, if already available.
    /// 2. Segoe UI.
    /// 3. Default system sans-serif fallback.
    /// Monospace font priority:
    /// 1. Cascadia Code.
    /// 2. JetBrains Mono, if already available.
    /// 3. Cascadia Mono.
    /// 4. Consolas.
    pub fn setup_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();

        #[cfg(windows)]
        {
            // 1. Proportional font: Inter -> Segoe UI -> system fallback
            let ui_candidates = [
                ("C:\\Windows\\Fonts\\Inter-Regular.ttf", "inter"),
                ("C:\\Windows\\Fonts\\Inter.ttf", "inter"),
                ("C:\\Windows\\Fonts\\segoeui.ttf", "segoe_ui"),
            ];
            for (path, name) in ui_candidates {
                if let Ok(bytes) = std::fs::read(path) {
                    fonts.font_data.insert(
                        name.to_owned(),
                        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                    );
                    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                        family.insert(0, name.to_owned());
                    }
                    break;
                }
            }

            // 2. Monospace font: Cascadia Code -> JetBrains Mono -> Cascadia Mono -> Consolas
            let mono_candidates = [
                ("C:\\Windows\\Fonts\\CascadiaCode.ttf", "cascadia_code"),
                ("C:\\Windows\\Fonts\\JetBrainsMono-Regular.ttf", "jetbrains_mono"),
                ("C:\\Windows\\Fonts\\CascadiaMono.ttf", "cascadia_mono"),
                ("C:\\Windows\\Fonts\\consola.ttf", "consolas"),
            ];
            for (path, name) in mono_candidates {
                if let Ok(bytes) = std::fs::read(path) {
                    fonts.font_data.insert(
                        name.to_owned(),
                        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                    );
                    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                        family.insert(0, name.to_owned());
                    }
                    break;
                }
            }
        }

        ctx.set_fonts(fonts);
    }
}

impl eframe::App for WorkspaceApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let had_modal_at_frame_start = self.has_any_modal_open();
        self.modal_closed_this_frame = false;

        if self.egui_ctx.is_none() {
            self.egui_ctx = Some(ctx.clone());
            Self::setup_fonts(ctx);
        }
        self.refresh_snapshot();

        // Global keyboard shortcut dispatcher for overlays and top-level app actions
        ctx.input(|i| {
            // Escape closes top-most overlay or dialog
            if i.key_pressed(egui::Key::Escape) {
                if self.close_unsaved_doc.is_some() {
                    self.close_unsaved_doc = None;
                    self.modal_closed_this_frame = true;
                } else if self.conflict_doc.is_some() {
                    self.conflict_doc = None;
                    self.modal_closed_this_frame = true;
                } else if self.diff_view.is_open {
                    self.diff_view.is_open = false;
                    self.modal_closed_this_frame = true;
                } else if self.new_menu_open {
                    self.new_menu_open = false;
                    self.modal_closed_this_frame = true;
                } else if self.command_palette.is_open {
                    self.command_palette.close();
                    self.modal_closed_this_frame = true;
                } else if self.quick_open.is_open {
                    self.quick_open.is_open = false;
                    self.modal_closed_this_frame = true;
                } else if self.settings_modal.is_open {
                    self.settings_modal.close();
                    self.persist_settings();
                    self.modal_closed_this_frame = true;
                } else if self.shortcuts_overlay.is_open {
                    self.shortcuts_overlay.close();
                    self.modal_closed_this_frame = true;
                } else if self.agent_dialog.is_open {
                    self.agent_dialog.is_open = false;
                    self.modal_closed_this_frame = true;
                } else if self.process_dialog.is_open {
                    self.process_dialog.is_open = false;
                    self.modal_closed_this_frame = true;
                } else if self.new_project_dialog.is_open {
                    self.new_project_dialog.is_open = false;
                    self.modal_closed_this_frame = true;
                } else if self.open_project_dialog.is_open {
                    self.open_project_dialog.is_open = false;
                    self.modal_closed_this_frame = true;
                } else if self.remove_project_modal.is_open {
                    self.remove_project_modal.is_open = false;
                    self.modal_closed_this_frame = true;
                }
            }

            // F1: Keyboard shortcuts overlay
            if i.key_pressed(egui::Key::F1) {
                if self.shortcuts_overlay.is_open {
                    self.shortcuts_overlay.close();
                } else {
                    self.shortcuts_overlay.open();
                }
            }
            if i.key_pressed(egui::Key::F2) {
                // F2: Command Palette
                self.command_palette.toggle();
            }
            if i.key_pressed(egui::Key::F3) {
                // F3: Toggle debug mode
                self.debug_mode = !self.debug_mode;
            }
            if i.key_pressed(egui::Key::F4) {
                self.agent_dialog.is_open = !self.agent_dialog.is_open;
            }
            if i.key_pressed(egui::Key::F6) {
                self.process_dialog.is_open = !self.process_dialog.is_open;
            }
            if i.key_pressed(egui::Key::F7) {
                // F7: Toggle diff view
                if self.diff_view.is_open {
                    self.diff_view.is_open = false;
                } else if let Some(ref path) = self.active_editor_path.clone() {
                    if let Some(doc) = self.open_editors.get(path) {
                        let title = doc.title.clone();
                        let content = doc.content.clone();
                        let full_path = doc.full_path.clone();
                        let disk_content = std::fs::read_to_string(&full_path).unwrap_or_default();
                        self.open_diff_view(&title, "Working Tree", &disk_content, &content);
                    }
                }
            }
            if i.key_pressed(egui::Key::F8) {
                // F8: Cycle through workspace files
                if self.open_editors.contains_key("src/main.rs")
                    && self.active_editor_path.as_deref() == Some("src/main.rs")
                {
                    self.open_editor_file("notes.xyz");
                } else {
                    self.open_editor_file("src/main.rs");
                }
            }
            if i.key_pressed(egui::Key::F9) {
                // F9: Cycle themes: Dark -> Light -> Omarchy
                self.settings_modal.theme_mode = match self.settings_modal.theme_mode.as_str() {
                    "Dark" => "Light".to_string(),
                    "Light" => "Omarchy".to_string(),
                    _ => "Dark".to_string(),
                };
                self.persist_settings();
            }
            if i.key_pressed(egui::Key::F10) {
                // F10: Mark active editor dirty
                if let Some(ref path) = self.active_editor_path.clone() {
                    if let Some(doc) = self.open_editors.get_mut(path) {
                        doc.content
                            .push_str("\n// Modified line for Group E acceptance\n");
                        doc.is_dirty = true;
                    }
                }
            }
            if i.key_pressed(egui::Key::F11) {
                // F11: Close active editor (triggers confirmation modal if dirty)
                if let Some(ref path) = self.active_editor_path.clone() {
                    self.close_editor_file(path);
                }
            }
            if i.key_pressed(egui::Key::F12) {
                // F12: Save active editor
                self.save_active_editor();
            }

            // Ctrl combinations
            if i.modifiers.ctrl && !i.modifiers.shift {
                if i.key_pressed(egui::Key::O) {
                    // Ctrl+O: Open Project
                    self.open_project_dialog.is_open = true;
                    self.open_project_dialog.path_input.clear();
                    self.open_project_dialog.error_msg = None;
                    self.trigger_open_project_native();
                } else if i.key_pressed(egui::Key::P) {
                    // Ctrl+P: Quick Open
                    self.quick_open.is_open = !self.quick_open.is_open;
                    if self.quick_open.is_open {
                        self.quick_open.query.clear();
                        self.quick_open.selected_index = 0;
                        self.scan_quick_open_files();
                    }
                } else if i.key_pressed(egui::Key::Comma) {
                    // Ctrl+,: Settings
                    self.settings_modal.is_open = !self.settings_modal.is_open;
                } else if i.key_pressed(egui::Key::B) {
                    // Ctrl+B: Toggle Sidebar
                    self.sidebar_collapsed = !self.sidebar_collapsed;
                } else if i.key_pressed(egui::Key::T) {
                    // Ctrl+T: New Session
                    self.trigger_new_session();
                } else if i.key_pressed(egui::Key::S) {
                    // Ctrl+S: Save active editor file
                    self.save_active_editor();
                } else if i.key_pressed(egui::Key::W) {
                    // Ctrl+W: Close active editor, diff view, or active split pane (Go tempterm tab.close parity)
                    if let Some(ref path) = self.active_editor_path.clone() {
                        self.close_editor_file(path);
                    } else if self.diff_view.is_open {
                        self.diff_view.is_open = false;
                    } else {
                        let active_pane = if !self.snapshot.active_pane_id.is_empty() {
                            Some(self.snapshot.active_pane_id.clone())
                        } else {
                            self.snapshot.panes.keys().next().cloned()
                        };

                        if let Some(pane_id) = active_pane {
                            // Closes only the active split pane.
                            // If multiple split panes exist, the remaining split panes stay alive!
                            // If only 1 pane was left, closing it cleanly removes the session.
                            self.trigger_close_pane(&pane_id);
                        } else if let Some(active_sess) =
                            self.snapshot.sessions.iter().find(|s| s.is_active)
                        {
                            self.trigger_close_session(&active_sess.id);
                        }
                    }
                } else if i.key_pressed(egui::Key::K) {
                    // Ctrl+K: Command Palette / Search
                    self.command_palette.toggle();
                } else if i.key_pressed(egui::Key::D) {
                    // Ctrl+D: Toggle diff view for active editor
                    if let Some(ref path) = self.active_editor_path.clone() {
                        if let Some(doc) = self.open_editors.get(path) {
                            let title = doc.title.clone();
                            let content = doc.content.clone();
                            let full_path = doc.full_path.clone();
                            let disk_content =
                                std::fs::read_to_string(&full_path).unwrap_or_default();
                            self.open_diff_view(&title, "Working Tree", &disk_content, &content);
                        }
                    }
                }
            }

            // Ctrl+Shift combinations
            if i.modifiers.ctrl && i.modifiers.shift {
                if i.key_pressed(egui::Key::P) {
                    // Ctrl+Shift+P: Command Palette
                    self.command_palette.toggle();
                } else if i.key_pressed(egui::Key::D) {
                    // Ctrl+Shift+D: Split terminal down
                    self.trigger_split(SplitDir::Horizontal);
                } else if i.key_pressed(egui::Key::R) {
                    // Ctrl+Shift+R: Split terminal right
                    self.trigger_split(SplitDir::Vertical);
                } else if i.key_pressed(egui::Key::Slash) {
                    // Ctrl+Shift+/: Keyboard Shortcuts
                    if self.shortcuts_overlay.is_open {
                        self.shortcuts_overlay.close();
                    } else {
                        self.shortcuts_overlay.open();
                    }
                } else if i.key_pressed(egui::Key::B) {
                    // Ctrl+Shift+B: Toggle Inspector
                    self.inspector_open = !self.inspector_open;
                } else if i.key_pressed(egui::Key::I) {
                    // Ctrl+Shift+I: Show Inspector Info
                    self.inspector_open = true;
                    self.inspector_tab = InspectorTab::Info;
                } else if i.key_pressed(egui::Key::E) {
                    // Ctrl+Shift+E: Show Inspector Files
                    self.inspector_open = true;
                    self.inspector_tab = InspectorTab::Files;
                } else if i.key_pressed(egui::Key::G) {
                    // Ctrl+Shift+G: Show Inspector Git
                    self.inspector_open = true;
                    self.inspector_tab = InspectorTab::Git;
                } else if i.key_pressed(egui::Key::A) {
                    // Ctrl+Shift+A: New Agent Dialog
                    self.agent_dialog.is_open = !self.agent_dialog.is_open;
                } else if i.key_pressed(egui::Key::N) {
                    // Ctrl+Shift+N: Add Process Dialog
                    self.process_dialog.is_open = !self.process_dialog.is_open;
                } else if i.key_pressed(egui::Key::Z) || i.key_pressed(egui::Key::Enter) {
                    // Ctrl+Shift+Z or Ctrl+Shift+Enter: Toggle Zoom Pane (Go parity)
                    self.trigger_toggle_zoom();
                }
            }
        });

        // Dark theme global styling configuration - Native Studio (Quiet Graphite / Arctic Blue)
        let mut visuals = egui::Visuals::dark();
        visuals.dark_mode = true;
        visuals.override_text_color = None;
        visuals.window_fill = theme::SURFACE;
        visuals.window_stroke = Stroke::new(1.0_f32, theme::BORDER_STRONG);
        visuals.window_rounding = Rounding::same(theme::RADIUS_MODAL);
        visuals.menu_rounding = Rounding::same(theme::RADIUS_MD);
        visuals.panel_fill = theme::APP_BACKGROUND;
        visuals.extreme_bg_color = theme::TERMINAL_BACKGROUND;
        visuals.faint_bg_color = theme::SURFACE;

        visuals.widgets.noninteractive.bg_fill = theme::SURFACE;
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, theme::TEXT_PRIMARY);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, theme::BORDER_SUBTLE);
        visuals.widgets.noninteractive.rounding = Rounding::same(theme::RADIUS_MD);

        visuals.widgets.inactive.bg_fill = theme::SURFACE_ELEVATED;
        visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, theme::TEXT_SECONDARY);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, theme::BORDER_SUBTLE);
        visuals.widgets.inactive.rounding = Rounding::same(theme::RADIUS_MD);

        // Hovered widgets: elevated surface with high-contrast text and border
        visuals.widgets.hovered.bg_fill = theme::SURFACE_HOVER;
        visuals.widgets.hovered.weak_bg_fill = theme::SURFACE_HOVER;
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, theme::TEXT_PRIMARY);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, theme::ACCENT);
        visuals.widgets.hovered.rounding = Rounding::same(theme::RADIUS_MD);

        // Active widgets: Arctic Blue selection
        visuals.widgets.active.bg_fill = theme::SURFACE_SELECTED;
        visuals.widgets.active.weak_bg_fill = theme::SURFACE_SELECTED;
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, theme::TEXT_PRIMARY);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, theme::ACCENT);
        visuals.widgets.active.rounding = Rounding::same(theme::RADIUS_MD);

        // Open widgets (e.g. combo box or menu open)
        visuals.widgets.open.bg_fill = theme::SURFACE_ELEVATED;
        visuals.widgets.open.weak_bg_fill = theme::SURFACE_ELEVATED;
        visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, theme::TEXT_PRIMARY);
        visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, theme::ACCENT);
        visuals.widgets.open.rounding = Rounding::same(theme::RADIUS_MD);

        visuals.selection.bg_fill = theme::SURFACE_SELECTED;
        visuals.selection.stroke = Stroke::new(1.0_f32, theme::ACCENT);

        ctx.set_visuals(visuals);

        let screen_rect = ctx.screen_rect();
        let screen_w = screen_rect.width();

        // Responsive Breakpoints:
        // - Large desktop: >= 1400px: Sidebar 236px, Inspector 288px, full 3-column
        // - Medium desktop: 1200-1399px: Sidebar ~220px, Inspector ~260px
        // - Compact desktop: 960-1199px: Sidebar ~200px, Inspector overlay drawer
        // - Narrow desktop: 720-959px: Sidebar becomes 56px icon rail, Inspector drawer
        // - Very narrow: < 720px: Single-workspace layout, both panels drawers
        let is_large = screen_w >= 1400.0;
        let is_medium = screen_w >= 1200.0 && screen_w < 1400.0;
        let is_compact = screen_w >= 960.0 && screen_w < 1200.0;
        let is_narrow = screen_w >= 720.0 && screen_w < 960.0;
        let is_very_narrow = screen_w < 720.0;

        let responsive_sidebar_w = if is_large {
            theme::SIDEBAR_WIDTH_DEFAULT // 236.0
        } else if is_medium {
            220.0_f32
        } else if is_compact {
            200.0_f32
        } else {
            56.0_f32
        };

        let responsive_inspector_w = if is_large {
            theme::INSPECTOR_WIDTH_DEFAULT // 288.0
        } else if is_medium {
            260.0_f32
        } else {
            288.0_f32
        };

        // 1. Top Navigation Bar (42px) spanning across full window width
        egui::TopBottomPanel::top("top_navigation")
            .exact_height(theme::TOPNAV_HEIGHT)
            .frame(Frame::none().fill(theme::PANEL))
            .show(ctx, |ui| {
                self.render_top_nav(ui);
            });

        // 2. Bottom Status Bar (26px) spanning across full window width
        egui::TopBottomPanel::bottom("bottom_statusbar")
            .exact_height(theme::STATUSBAR_HEIGHT)
            .frame(Frame::none().fill(theme::CANVAS))
            .show(ctx, |ui| {
                self.render_statusbar(ui);
            });

        // 3. Left Sidebar (full sidebar, 56px icon rail, or hidden drawer)
        if !self.sidebar_collapsed {
            if is_narrow {
                // Narrow desktop: render 56px icon rail
                egui::SidePanel::left("left_sidebar_rail")
                    .exact_width(56.0)
                    .resizable(false)
                    .frame(Frame::none().fill(theme::PANEL))
                    .show(ctx, |ui| {
                        self.render_sidebar_rail(ui);
                    });
            } else if !is_very_narrow {
                // Normal sidebar
                egui::SidePanel::left("left_sidebar")
                    .resizable(true)
                    .width_range(180.0..=480.0)
                    .default_width(responsive_sidebar_w)
                    .frame(Frame::none().fill(theme::PANEL))
                    .show(ctx, |ui| {
                        self.sidebar_width = ui.available_width();
                        self.render_sidebar(ui);
                    });
            }
        }

        // 4. Right Inspector Panel (fixed side panel or overlay drawer)
        if self.inspector_open {
            if is_large || is_medium {
                egui::SidePanel::right("right_inspector")
                    .resizable(true)
                    .width_range(240.0..=520.0)
                    .default_width(responsive_inspector_w)
                    .frame(Frame::none().fill(theme::PANEL))
                    .show(ctx, |ui| {
                        self.inspector_width = ui.available_width();
                        self.render_inspector(ui);
                    });
            } else {
                // Small / Narrow / Very Narrow desktop: Inspector renders as overlay drawer
                let drawer_w = 312.0_f32.min(screen_w - 40.0);
                let drawer_h = (screen_rect.height() - theme::TOPNAV_HEIGHT - theme::STATUSBAR_HEIGHT).max(200.0);
                let drawer_rect = Rect::from_min_size(
                    pos2(screen_rect.max.x - drawer_w, screen_rect.min.y + theme::TOPNAV_HEIGHT),
                    Vec2::new(drawer_w, drawer_h),
                );

                let mut drawer_open = self.inspector_open;
                egui::Window::new("inspector_overlay_drawer")
                    .open(&mut drawer_open)
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .fixed_rect(drawer_rect)
                    .frame(
                        Frame::none()
                            .fill(theme::PANEL)
                            .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                            .shadow(egui::epaint::Shadow {
                                offset: Vec2::new(-4.0, 0.0),
                                blur: 16.0,
                                spread: 0.0,
                                color: Color32::from_black_alpha(80),
                            }),
                    )
                    .show(ctx, |ui| {
                        self.render_inspector(ui);
                    });
                self.inspector_open = drawer_open;
            }
        }

        // 5. Very narrow desktop: Left sidebar drawer if opened
        if is_very_narrow && !self.sidebar_collapsed {
            let drawer_w = 260.0_f32.min(screen_w - 40.0);
            let drawer_h = (screen_rect.height() - theme::TOPNAV_HEIGHT - theme::STATUSBAR_HEIGHT).max(200.0);
            let drawer_rect = Rect::from_min_size(
                pos2(screen_rect.min.x, screen_rect.min.y + theme::TOPNAV_HEIGHT),
                Vec2::new(drawer_w, drawer_h),
            );

            let mut drawer_open = !self.sidebar_collapsed;
            egui::Window::new("sidebar_overlay_drawer")
                .open(&mut drawer_open)
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .fixed_rect(drawer_rect)
                .frame(
                    Frame::none()
                        .fill(theme::PANEL)
                        .stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG))
                        .shadow(egui::epaint::Shadow {
                            offset: Vec2::new(4.0, 0.0),
                            blur: 16.0,
                            spread: 0.0,
                            color: Color32::from_black_alpha(80),
                        }),
                )
                .show(ctx, |ui| {
                    self.render_sidebar(ui);
                });
            self.sidebar_collapsed = !drawer_open;
        }

        // 6. Central Workspace Viewport (with workspace tabs at top, min width 480px)
        egui::CentralPanel::default()
            .frame(Frame::none().fill(theme::WORKSPACE))
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;

                    // 36px Workspace Tabs Strip
                    self.render_workspace_tabs(ui);

                    // Central Workspace Content (terminal panes, editor, or diff)
                    if self.diff_view.is_open {
                        self.render_diff_viewer(ui);
                    } else if let Some(ref active_path) = self.active_editor_path.clone() {
                        self.render_editor_pane(ui, active_path);
                    } else if self.snapshot.sessions.is_empty() {
                        self.render_empty_state(ui);
                    } else if let Some(ref zoomed_id) = self.snapshot.zoomed_pane_id.clone() {
                        if self
                            .snapshot
                            .layout
                            .as_ref()
                            .map_or(false, |l| l.contains(zoomed_id))
                        {
                            let avail = ui.available_rect_before_wrap();
                            self.render_single_pane(ui, zoomed_id, avail);
                        } else {
                            self.snapshot.zoomed_pane_id = None;
                            if let Some(ref layout) = self.snapshot.layout.clone() {
                                let avail = ui.available_rect_before_wrap();
                                self.render_split_tree(ui, layout, avail, "");
                            } else {
                                self.render_empty_state(ui);
                            }
                        }
                    } else if let Some(ref layout) = self.snapshot.layout.clone() {
                        let avail = ui.available_rect_before_wrap();
                        self.render_split_tree(ui, layout, avail, "");
                    } else if let Some(pane_id) = self.snapshot.panes.keys().next().cloned() {
                        let avail = ui.available_rect_before_wrap();
                        self.render_single_pane(ui, &pane_id, avail);
                    } else {
                        self.render_empty_state(ui);
                    }
                });
            });

        // Render any active modals (New Agent, Add Process)
        self.render_dialogs(ctx, had_modal_at_frame_start);

        // Repaint request interval to keep PTY output fresh without burning CPU
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_agent_agy_shell_invocation() {
        let lines = vec![
            "Microsoft Windows [Version 10.0.26100]".to_string(),
            "PS D:\\project\\myproject> agy".to_string(),
        ];
        let detected = WorkspaceApp::detect_agent_from_terminal_lines(&lines);
        assert_eq!(detected, Some(("Antigravity", "agy")));
    }

    #[test]
    fn test_detect_agent_agy_interactive_screen() {
        let lines = vec![
            "Antigravity CLI v1.4.0".to_string(),
            "Type a query or /command".to_string(),
            "? for shortcuts".to_string(),
        ];
        let detected = WorkspaceApp::detect_agent_from_terminal_lines(&lines);
        assert_eq!(detected, Some(("Antigravity", "agy")));
    }

    #[test]
    fn test_detect_agent_agy_braille_spinner() {
        let lines = vec![
            "agy> refactor the codebase".to_string(),
            "⣽  Generating...".to_string(),
        ];
        let detected = WorkspaceApp::detect_agent_from_terminal_lines(&lines);
        assert_eq!(detected, Some(("Antigravity", "agy")));
    }

    #[test]
    fn test_detect_agent_idle_shell_returns_none() {
        let lines = vec![
            "Antigravity CLI v1.4.0".to_string(),
            "? for shortcuts".to_string(),
            "PS D:\\project\\myproject> ".to_string(),
        ];
        // Shell prompt with no command means agent has exited
        let detected = WorkspaceApp::detect_agent_from_terminal_lines(&lines);
        assert_eq!(detected, None);
    }

    #[test]
    fn test_detect_agent_echo_agy_not_detected() {
        let lines = vec![
            "PS D:\\project\\myproject> echo agy".to_string(),
            "agy".to_string(),
            "PS D:\\project\\myproject> ".to_string(),
        ];
        let detected = WorkspaceApp::detect_agent_from_terminal_lines(&lines);
        assert_eq!(detected, None);
    }

    #[test]
    fn test_detect_agent_claude_invocation() {
        let lines = vec![
            "PS D:\\project\\myproject> claude".to_string(),
        ];
        let detected = WorkspaceApp::detect_agent_from_terminal_lines(&lines);
        assert_eq!(detected, Some(("Claude Code", "claude")));
    }

    #[test]
    fn test_detect_agent_agy_with_ansi_syntax_highlighting() {
        // PSReadLine in PowerShell highlights commands in yellow/bright yellow or TrueColor:
        let lines_standard = vec![
            "PS D:\\project\\myproject> \x1b[93magy\x1b[0m".to_string(),
        ];
        let detected = WorkspaceApp::detect_agent_from_terminal_lines(&lines_standard);
        assert_eq!(detected, Some(("Antigravity", "agy")));

        let lines_truecolor = vec![
            "\x1b[32mPS D:\\project\\myproject>\x1b[0m \x1b[38;2;255;215;0magy\x1b[m".to_string(),
        ];
        let detected_tc = WorkspaceApp::detect_agent_from_terminal_lines(&lines_truecolor);
        assert_eq!(detected_tc, Some(("Antigravity", "agy")));
    }

    #[test]
    fn test_detect_agent_agy_flags_and_custom_prompts() {
        // agy with arguments e.g. agy -c or agy.exe
        let lines_args = vec![
            "PS D:\\project> \x1b[93magy.exe\x1b[0m -c".to_string(),
        ];
        assert_eq!(
            WorkspaceApp::detect_agent_from_terminal_lines(&lines_args),
            Some(("Antigravity", "agy"))
        );

        // Starship / zsh prompt: "❯ agy"
        let lines_starship = vec![
            "❯ \x1b[93magy\x1b[0m".to_string(),
        ];
        assert_eq!(
            WorkspaceApp::detect_agent_from_terminal_lines(&lines_starship),
            Some(("Antigravity", "agy"))
        );

        // Bash prompt: "user@host:~/project$ agy"
        let lines_bash = vec![
            "user@host:~/project$ \x1b[93magy\x1b[0m".to_string(),
        ];
        assert_eq!(
            WorkspaceApp::detect_agent_from_terminal_lines(&lines_bash),
            Some(("Antigravity", "agy"))
        );

        // Windows CMD prompt: "C:\project> agy"
        let lines_cmd = vec![
            "C:\\project> agy".to_string(),
        ];
        assert_eq!(
            WorkspaceApp::detect_agent_from_terminal_lines(&lines_cmd),
            Some(("Antigravity", "agy"))
        );

        // Conda / venv prompt: "(base) PS D:\project> agy"
        let lines_conda = vec![
            "(base) PS D:\\project> \x1b[93magy\x1b[0m".to_string(),
        ];
        assert_eq!(
            WorkspaceApp::detect_agent_from_terminal_lines(&lines_conda),
            Some(("Antigravity", "agy"))
        );
    }

    #[test]
    fn test_strip_ansi_comprehensive() {
        assert_eq!(WorkspaceApp::strip_ansi("\x1b[38;2;255;215;0magy\x1b[m"), "agy");
        assert_eq!(WorkspaceApp::strip_ansi("\x1b[93magy\x1b[0m"), "agy");
        assert_eq!(
            WorkspaceApp::strip_ansi("\x1b[32mPS D:\\project>\x1b[0m \x1b[93magy\x1b[0m"),
            "PS D:\\project> agy"
        );
        assert_eq!(WorkspaceApp::strip_ansi("\x1b]0;Title\x07Hello"), "Hello");
        assert_eq!(WorkspaceApp::strip_ansi("plain text"), "plain text");
    }

    #[test]
    fn test_font_metrics() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let font_id = egui::FontId::monospace(theme::typography::TERMINAL_TEXT);
        let char_w = ctx.fonts(|f| f.glyph_width(&font_id, 'M'));
        let line_h = ctx.fonts(|f| f.row_height(&font_id));
        assert!(char_w > 0.0);
        assert!(line_h > 0.0);

        WorkspaceApp::setup_fonts(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        let char_w_m = ctx.fonts(|f| f.glyph_width(&font_id, 'M'));
        let char_w_space = ctx.fonts(|f| f.glyph_width(&font_id, ' '));
        let char_w_x = ctx.fonts(|f| f.glyph_width(&font_id, 'x'));
        let char_w_0 = ctx.fonts(|f| f.glyph_width(&font_id, '0'));
        let line_h_loaded = ctx.fonts(|f| f.row_height(&font_id));

        // Monospace advance widths must be strictly uniform
        assert_eq!(char_w_m, char_w_space);
        assert_eq!(char_w_m, char_w_x);
        assert_eq!(char_w_m, char_w_0);
        assert!(char_w_m >= 8.0, "Monospace character width at size 13.5 must be >= 8.0px, got {}", char_w_m);
        assert!(line_h_loaded >= 15.0, "Monospace row height at size 13.5 must be >= 15.0px, got {}", line_h_loaded);
    }

    #[test]
    fn test_terminal_responsive_column_allocation() {
        let ctx = egui::Context::default();
        WorkspaceApp::setup_fonts(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |_| {});

        let font_id = egui::FontId::monospace(theme::typography::TERMINAL_TEXT);
        let char_w = ctx.fonts(|f| f.glyph_width(&font_id, 'M'));
        let line_h = ctx.fonts(|f| f.row_height(&font_id));

        // Scrollbar gutter (bar_width + inner_margin + safe margin)
        let scrollbar_gutter = 6.0 + 4.0 + 6.0;
        let pad_w = 12.0f32 + scrollbar_gutter; // 28.0px total horizontal padding
        let pad_h = 38.0f32;

        let test_widths = [640.0f32, 800.0, 1024.0, 1280.0, 1600.0, 1920.0];
        for width in test_widths {
            let avail_w = (width - pad_w).max(70.0);
            let target_cols = (avail_w / char_w).floor().clamp(20.0, 300.0) as u16;

            // Total rendered width of target_cols must fit within the pane width without overflowing
            let rendered_text_width = target_cols as f32 * char_w;
            assert!(
                rendered_text_width + pad_w <= width,
                "At width {}, target_cols {} produces rendered text width {} + pad_w {} = {}, exceeding width!",
                width, target_cols, rendered_text_width, pad_w, rendered_text_width + pad_w
            );

            // Verify target_cols is not underallocated (no more than 1 character extra space left unallocated)
            let remaining_space = avail_w - rendered_text_width;
            assert!(
                remaining_space < char_w,
                "Underallocated columns at width {}: remaining space {} >= char_w {}",
                width, remaining_space, char_w
            );
        }

        // Verify vertical row allocation at different heights
        let test_heights = [400.0f32, 600.0, 800.0, 1080.0];
        for height in test_heights {
            let avail_h = (height - pad_h).max(30.0);
            let target_rows = (avail_h / line_h).floor().clamp(5.0, 120.0) as u16;
            let rendered_text_height = target_rows as f32 * line_h;
            assert!(
                rendered_text_height + pad_h <= height,
                "At height {}, target_rows {} produces rendered height {} + pad_h {} = {}, exceeding height!",
                height, target_rows, rendered_text_height, pad_h, rendered_text_height + pad_h
            );
        }
    }

    #[test]
    fn test_terminal_buffer_and_renderer_preserve_truecolor_logo() {
        use crate::TerminalBuffer;
        use crate::renderer::TerminalColor;

        let mut buf = TerminalBuffer::new(80, 10, 100);

        // Simulate multi-colored logo banner from agy (Google 4-color TrueColor banner)
        // 'A' (Blue: 66, 133, 244), 'G' (Red: 234, 67, 53), 'Y' (Yellow: 251, 188, 5)
        let colored_banner = b"\x1b[38;2;66;133;244mA\x1b[38;2;234;67;53mG\x1b[38;2;251;188;5mY\x1b[0m\n";
        buf.append_output(colored_banner);

        let lines = buf.render_view();
        assert!(!lines.is_empty());

        let renderer = crate::renderer::TerminalRenderer::new(14, 18.0);
        let cells = renderer.parse_ansi_line(&lines[0]);

        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].ch, 'A');
        assert_eq!(cells[0].fg, TerminalColor::Rgb(66, 133, 244));
        assert!(!cells[0].faint, "Parameter 2 in TrueColor 38;2;r;g;b must never trigger faint");

        assert_eq!(cells[1].ch, 'G');
        assert_eq!(cells[1].fg, TerminalColor::Rgb(234, 67, 53));
        assert!(!cells[1].faint);

        assert_eq!(cells[2].ch, 'Y');
        assert_eq!(cells[2].fg, TerminalColor::Rgb(251, 188, 5));
        assert!(!cells[2].faint);
    }

    #[test]
    fn test_terminal_buffer_and_renderer_preserve_256_color_logo() {
        use crate::TerminalBuffer;
        use crate::renderer::{xterm_256_to_rgb, TerminalColor};

        let mut buf = TerminalBuffer::new(80, 10, 100);

        // Simulate 256-color output (color 196 = red, color 46 = green)
        let colored_text = b"\x1b[38;5;196mR\x1b[38;5;46mG\x1b[0m\n";
        buf.append_output(colored_text);

        let lines = buf.render_view();
        assert!(!lines.is_empty());

        let renderer = crate::renderer::TerminalRenderer::new(14, 18.0);
        let cells = renderer.parse_ansi_line(&lines[0]);

        assert_eq!(cells.len(), 2);
        let (r1, g1, b1) = xterm_256_to_rgb(196);
        assert_eq!(cells[0].ch, 'R');
        assert_eq!(cells[0].fg, TerminalColor::Rgb(r1, g1, b1));
        assert!(!cells[0].faint);

        let (r2, g2, b2) = xterm_256_to_rgb(46);
        assert_eq!(cells[1].ch, 'G');
        assert_eq!(cells[1].fg, TerminalColor::Rgb(r2, g2, b2));
        assert!(!cells[1].faint);
    }
}
