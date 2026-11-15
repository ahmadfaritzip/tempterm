//! Graphical window host and native message loop for temp-terminal.
//! Implements a native Windows window (using Win32 API from `windows-sys`) with GDI terminal rendering,
//! split-pane layout display, scrollback view, status bar, and keyboard input forwarding.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use temp_terminal_core::Registry;
use tokio::sync::mpsc;

#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(windows)]
use windows_sys::Win32::Foundation::*;
#[cfg(windows)]
use windows_sys::Win32::Graphics::Gdi::*;
#[cfg(windows)]
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::renderer::{TerminalCell, TerminalColor, TerminalRenderer};
use crate::{AppHost, SplitDir, SplitNode, TerminalPane};

/// Configuration options for creating a native window host.
#[derive(Debug, Clone)]
pub struct WindowOptions {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub min_height: u32,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "tempterm".to_string(),
            width: 1200,
            height: 800,
            min_width: 600,
            min_height: 400,
        }
    }
}

/// A native graphical application window running an event loop and rendering terminal panes.
pub struct GuiWindow {
    pub options: WindowOptions,
    pub host: Arc<AppHost>,
    pub registry: Arc<Registry>,
    pub renderer: Arc<TerminalRenderer>,
    pub is_running: Arc<AtomicBool>,
    pub hwnd: Option<isize>,
}

#[cfg(windows)]
static CURRENT_GUI_WINDOW: std::sync::RwLock<Option<Arc<GuiWindowRef>>> =
    std::sync::RwLock::new(None);

#[cfg(windows)]
struct GuiWindowRef {
    host: Arc<AppHost>,
    registry: Arc<Registry>,
    renderer: Arc<TerminalRenderer>,
    _is_running: Arc<AtomicBool>,
}

impl GuiWindow {
    pub fn new(options: WindowOptions, host: Arc<AppHost>, registry: Arc<Registry>) -> Self {
        Self {
            options,
            host,
            registry,
            renderer: Arc::new(TerminalRenderer::new(14, 1.2)),
            is_running: Arc::new(AtomicBool::new(false)),
            hwnd: None,
        }
    }

    /// Renders the complete terminal workspace layout into terminal cells for display.
    pub async fn render_workspace_cells(
        &self,
        width_chars: usize,
        height_chars: usize,
    ) -> Vec<Vec<TerminalCell>> {
        let mut screen = vec![vec![TerminalCell::default(); width_chars]; height_chars];
        let projs = self.host.projects.read().await;
        let active_proj_id = self.host.active_project_id.read().await;

        let proj = match projs.get(&*active_proj_id) {
            Some(p) => p,
            None => {
                // Empty / Start screen
                self.render_empty_screen(&mut screen, "tempterm - No Project Open");
                return screen;
            }
        };

        let sess = match proj.sessions.get(&proj.active_session_id) {
            Some(s) => s,
            None => {
                self.render_empty_screen(
                    &mut screen,
                    &format!("Project: {} (No active session)", proj.name),
                );
                return screen;
            }
        };

        // Render top status bar (Row 0)
        let status_text = format!(
            " [Project: {}] [Session: {}] [Panes: {}] [Connected: {}] ",
            proj.name,
            sess.name,
            sess.panes.len(),
            self.host.is_connected.load(Ordering::SeqCst)
        );
        for (col, ch) in status_text.chars().enumerate() {
            if col < width_chars {
                screen[0][col] = TerminalCell {
                    ch,
                    fg: TerminalColor::White,
                    bg: TerminalColor::Blue,
                    bold: true,
                    underline: false,
                    ..Default::default()
                };
            }
        }
        for col in status_text.len()..width_chars {
            screen[0][col] = TerminalCell {
                ch: ' ',
                fg: TerminalColor::White,
                bg: TerminalColor::Blue,
                bold: false,
                underline: false,
                ..Default::default()
            };
        }

        // Render split tree panes in rows 1..height_chars - 1
        if let Some(ref root_layout) = sess.layout {
            let pane_area_height = if height_chars > 2 {
                height_chars - 2
            } else {
                1
            };
            self.render_split_layout(
                &mut screen,
                root_layout,
                &sess.panes,
                &sess.active_pane_id,
                0,
                1,
                width_chars,
                pane_area_height,
            )
            .await;
        }

        // Render bottom helper bar (Row height_chars - 1)
        let help_row = height_chars.saturating_sub(1);
        let help_text =
            " [F2] Split V  [F3] Split H  [Ctrl+W] Close Pane  [Esc] Back  [Ctrl+Q] Quit ";
        for (col, ch) in help_text.chars().enumerate() {
            if col < width_chars {
                screen[help_row][col] = TerminalCell {
                    ch,
                    fg: TerminalColor::Black,
                    bg: TerminalColor::White,
                    bold: false,
                    underline: false,
                    ..Default::default()
                };
            }
        }
        for col in help_text.len()..width_chars {
            screen[help_row][col] = TerminalCell {
                ch: ' ',
                fg: TerminalColor::Black,
                bg: TerminalColor::White,
                bold: false,
                underline: false,
                ..Default::default()
            };
        }

        screen
    }

    fn render_empty_screen(&self, screen: &mut [Vec<TerminalCell>], title: &str) {
        let width = screen[0].len();
        let height = screen.len();
        let mid_y = height / 2;
        let start_x = width.saturating_sub(title.len()) / 2;

        for (i, ch) in title.chars().enumerate() {
            if start_x + i < width && mid_y < height {
                screen[mid_y][start_x + i] = TerminalCell {
                    ch,
                    fg: TerminalColor::Cyan,
                    bg: TerminalColor::Default,
                    bold: true,
                    underline: false,
                    ..Default::default()
                };
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn render_split_layout(
        &self,
        screen: &mut [Vec<TerminalCell>],
        node: &SplitNode,
        panes: &std::collections::HashMap<String, Arc<TerminalPane>>,
        active_pane_id: &str,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
    ) {
        if node.is_leaf() {
            if let Some(pane) = panes.get(&node.term_id) {
                let is_active = pane.id == active_pane_id;
                self.render_single_pane(screen, pane, is_active, x, y, w, h)
                    .await;
            }
            return;
        }

        // Internal split
        match node.dir {
            SplitDir::Vertical => {
                let left_w = (w as f32 * (node.ratio / 100.0)).round() as usize;
                let right_w = w.saturating_sub(left_w);
                if let Some(ref left) = node.first {
                    Box::pin(self.render_split_layout(
                        screen,
                        left,
                        panes,
                        active_pane_id,
                        x,
                        y,
                        left_w,
                        h,
                    ))
                    .await;
                }
                if let Some(ref right) = node.second {
                    Box::pin(self.render_split_layout(
                        screen,
                        right,
                        panes,
                        active_pane_id,
                        x + left_w,
                        y,
                        right_w,
                        h,
                    ))
                    .await;
                }
            }
            SplitDir::Horizontal => {
                let top_h = (h as f32 * (node.ratio / 100.0)).round() as usize;
                let bottom_h = h.saturating_sub(top_h);
                if let Some(ref top) = node.first {
                    Box::pin(self.render_split_layout(
                        screen,
                        top,
                        panes,
                        active_pane_id,
                        x,
                        y,
                        w,
                        top_h,
                    ))
                    .await;
                }
                if let Some(ref bottom) = node.second {
                    Box::pin(self.render_split_layout(
                        screen,
                        bottom,
                        panes,
                        active_pane_id,
                        x,
                        y + top_h,
                        w,
                        bottom_h,
                    ))
                    .await;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn render_single_pane(
        &self,
        screen: &mut [Vec<TerminalCell>],
        pane: &Arc<TerminalPane>,
        is_active: bool,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
    ) {
        let buf = pane.buffer.read().await;
        let rendered_lines = buf.render_view();
        let total_lines = rendered_lines.len();

        // Draw pane header border
        let title = format!("[{}{}]", if is_active { "* " } else { "" }, pane.name);
        for i in 0..w {
            let col = x + i;
            if col < screen[0].len() && y < screen.len() {
                let ch = if i < title.len() {
                    title.chars().nth(i).unwrap_or('-')
                } else {
                    '-'
                };
                screen[y][col] = TerminalCell {
                    ch,
                    fg: if is_active {
                        TerminalColor::BrightYellow
                    } else {
                        TerminalColor::White
                    },
                    bg: TerminalColor::Default,
                    bold: is_active,
                    underline: false,
                    ..Default::default()
                };
            }
        }

        // Draw pane terminal content
        let content_h = h.saturating_sub(1);
        let start_line = total_lines.saturating_sub(content_h);

        for row_idx in 0..content_h {
            let screen_y = y + 1 + row_idx;
            if screen_y >= screen.len() {
                break;
            }
            let line_idx = start_line + row_idx;
            let line_str = if line_idx < total_lines {
                &rendered_lines[line_idx]
            } else {
                ""
            };

            let cells = self.renderer.parse_ansi_line(line_str);
            for col_idx in 0..w {
                let screen_x = x + col_idx;
                if screen_x >= screen[0].len() {
                    break;
                }
                if col_idx < cells.len() {
                    screen[screen_y][screen_x] = cells[col_idx].clone();
                } else {
                    screen[screen_y][screen_x] = TerminalCell::default();
                }
            }
        }
    }

    /// Runs the native message loop. If headless / non-interactive, completes immediately.
    #[cfg(windows)]
    pub fn run_native_loop(&mut self) -> Result<(), String> {
        let (tx, mut rx) = mpsc::channel::<()>(1);
        self.is_running.store(true, Ordering::SeqCst);

        // Store reference for wndproc message dispatch
        let win_ref = Arc::new(GuiWindowRef {
            host: self.host.clone(),
            registry: self.registry.clone(),
            renderer: self.renderer.clone(),
            _is_running: self.is_running.clone(),
        });
        if let Ok(mut lock) = CURRENT_GUI_WINDOW.write() {
            *lock = Some(win_ref);
        }

        unsafe {
            let class_name: Vec<u16> = OsStr::new("TempTerminalMainWindow")
                .encode_wide()
                .chain(Some(0))
                .collect();
            let title: Vec<u16> = OsStr::new(&self.options.title)
                .encode_wide()
                .chain(Some(0))
                .collect();

            let hinstance = GetModuleHandleW(std::ptr::null());

            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance,
                hIcon: std::ptr::null_mut(),
                hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
                hbrBackground: GetStockObject(BLACK_BRUSH),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_name.as_ptr(),
            };

            RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                self.options.width as i32,
                self.options.height as i32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinstance,
                std::ptr::null_mut(),
            );

            if hwnd.is_null() {
                return Err("Failed to create Win32 native window".to_string());
            }

            self.hwnd = Some(hwnd as isize);
            ShowWindow(hwnd, SW_SHOW);
            UpdateWindow(hwnd);

            // Message pump
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);

                if !self.is_running.load(Ordering::SeqCst) {
                    break;
                }
            }
        }

        if let Ok(mut lock) = CURRENT_GUI_WINDOW.write() {
            *lock = None;
        }

        self.is_running.store(false, Ordering::SeqCst);
        let _ = tx.try_send(());
        let _ = rx.try_recv();
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn run_native_loop(&mut self) -> Result<(), String> {
        self.is_running.store(true, Ordering::SeqCst);
        // Headless stub on non-Windows platforms
        self.is_running.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Dispatches a key event to the active pane or triggers application shortcuts.
    pub async fn dispatch_key_input(
        host: &Arc<AppHost>,
        registry: &Arc<Registry>,
        key_code: u32,
        char_ch: Option<char>,
        ctrl_pressed: bool,
        shift_pressed: bool,
    ) {
        // 1. Check for application-wide shortcuts
        if key_code == VK_F2 as u32 {
            // F2: Split Vertical
            let _ = host.split_active_pane(SplitDir::Vertical, registry).await;
            return;
        }
        if key_code == VK_F3 as u32 {
            // F3: Split Horizontal
            let _ = host.split_active_pane(SplitDir::Horizontal, registry).await;
            return;
        }
        if ctrl_pressed && (key_code == 'W' as u32 || key_code == 0x57) {
            // Ctrl+W: Close active pane
            let _ = host.close_active_pane(registry).await;
            return;
        }
        if key_code == VK_TAB as u32 && ctrl_pressed {
            // Ctrl+Tab / Ctrl+Shift+Tab: cycle pane focus
            let _ = host.cycle_pane_focus(!shift_pressed).await;
            return;
        }

        // 2. Control characters (Ctrl+C, Ctrl+D, Ctrl+L, etc.)
        let input_bytes: Option<Vec<u8>> = if ctrl_pressed {
            match key_code as u8 {
                b'C' | b'c' => Some(vec![0x03]), // Ctrl+C
                b'D' | b'd' => Some(vec![0x04]), // Ctrl+D
                b'L' | b'l' => Some(vec![0x0c]), // Ctrl+L
                b'Z' | b'z' => Some(vec![0x1a]), // Ctrl+Z
                _ => None,
            }
        } else if let Some(ch) = char_ch {
            // Normal character input
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            Some(s.as_bytes().to_vec())
        } else {
            // Special navigation keys
            match key_code as u16 {
                VK_RETURN => Some(vec![b'\r']),
                VK_BACK => Some(vec![0x08]),
                VK_TAB => Some(vec![b'\t']),
                VK_ESCAPE => Some(vec![0x1b]),
                VK_UP => Some(b"\x1b[A".to_vec()),
                VK_DOWN => Some(b"\x1b[B".to_vec()),
                VK_RIGHT => Some(b"\x1b[C".to_vec()),
                VK_LEFT => Some(b"\x1b[D".to_vec()),
                _ => None,
            }
        };

        if let Some(bytes) = input_bytes {
            let projs = host.projects.read().await;
            let active_proj_id = host.active_project_id.read().await;
            if let Some(proj) = projs.get(&*active_proj_id) {
                if let Some(sess) = proj.sessions.get(&proj.active_session_id) {
                    if let Some(pane) = sess.panes.get(&sess.active_pane_id) {
                        let _ = pane.send_input(&bytes).await;
                    }
                }
            }
        }
    }
    pub fn close(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        #[cfg(windows)]
        if let Some(hwnd) = self.hwnd {
            unsafe {
                PostMessageW(hwnd as HWND, WM_CLOSE, 0, 0);
            }
        }
    }
}

#[cfg(windows)]
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let vk = wparam as u32;
            let ctrl = (GetKeyState(VK_CONTROL as i32) as u16 & 0x8000) != 0;
            let shift = (GetKeyState(VK_SHIFT as i32) as u16 & 0x8000) != 0;

            if let Ok(guard) = CURRENT_GUI_WINDOW.read() {
                if let Some(ref win_ref) = *guard {
                    let host = win_ref.host.clone();
                    let reg = win_ref.registry.clone();
                    tokio::spawn(async move {
                        GuiWindow::dispatch_key_input(&host, &reg, vk, None, ctrl, shift).await;
                    });
                }
            }

            // Invalidate window to redraw on shortcut or key
            InvalidateRect(hwnd, std::ptr::null(), 0);
            0
        }
        WM_CHAR => {
            let ch = char::from_u32(wparam as u32);
            let ctrl = (GetKeyState(VK_CONTROL as i32) as u16 & 0x8000) != 0;
            let shift = (GetKeyState(VK_SHIFT as i32) as u16 & 0x8000) != 0;

            // Only dispatch printable characters not consumed by Ctrl shortcuts
            if let Some(c) = ch {
                if !c.is_control() && !ctrl {
                    if let Ok(guard) = CURRENT_GUI_WINDOW.read() {
                        if let Some(ref win_ref) = *guard {
                            let host = win_ref.host.clone();
                            let reg = win_ref.registry.clone();
                            tokio::spawn(async move {
                                GuiWindow::dispatch_key_input(&host, &reg, 0, Some(c), ctrl, shift)
                                    .await;
                            });
                        }
                    }
                }
            }
            InvalidateRect(hwnd, std::ptr::null(), 0);
            0
        }
        WM_SIZE => {
            let new_width = (lparam & 0xffff) as u32;
            let new_height = ((lparam >> 16) & 0xffff) as u32;
            if let Ok(guard) = CURRENT_GUI_WINDOW.read() {
                if let Some(ref win_ref) = *guard {
                    let host = win_ref.host.clone();
                    // Approximate characters assuming 8x16 font
                    let cols = (new_width / 8).max(20) as u16;
                    let rows = (new_height / 16).max(10) as u16;
                    tokio::spawn(async move {
                        let projs = host.projects.read().await;
                        let active_proj = host.active_project_id.read().await;
                        if let Some(proj) = projs.get(&*active_proj) {
                            if let Some(sess) = proj.sessions.get(&proj.active_session_id) {
                                if let Some(pane) = sess.panes.get(&sess.active_pane_id) {
                                    let _ = pane.resize(cols, rows).await;
                                }
                            }
                        }
                    });
                }
            }
            InvalidateRect(hwnd, std::ptr::null(), 0);
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let width = (rect.right - rect.left) as usize;
            let height = (rect.bottom - rect.top) as usize;

            let char_w = 9usize;
            let char_h = 16usize;
            let cols = (width / char_w).max(20);
            let rows = (height / char_h).max(10);

            // Render workspace cells
            if let Ok(guard) = CURRENT_GUI_WINDOW.read() {
                if let Some(ref win_ref) = *guard {
                    let host = win_ref.host.clone();
                    let renderer = win_ref.renderer.clone();

                    // Synchronously wait on tokio runtime block_in_place for layout render
                    let cells = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            let win_dummy = GuiWindow {
                                options: WindowOptions::default(),
                                host,
                                registry: win_ref.registry.clone(),
                                renderer,
                                is_running: Arc::new(AtomicBool::new(false)),
                                hwnd: None,
                            };
                            win_dummy.render_workspace_cells(cols, rows).await
                        })
                    });

                    // Draw cells using GDI TextOutW & SetTextColor / SetBkColor
                    SetBkMode(hdc, OPAQUE as i32);
                    for (r_idx, row) in cells.iter().enumerate() {
                        let y_pos = (r_idx * char_h) as i32;
                        for (c_idx, cell) in row.iter().enumerate() {
                            let x_pos = (c_idx * char_w) as i32;

                            // Color map
                            let fg_color = match cell.fg {
                                TerminalColor::Default | TerminalColor::White => 0x00FFFFFF,
                                TerminalColor::Black => 0x00000000,
                                TerminalColor::Red | TerminalColor::BrightRed => 0x005555FF, // BGR
                                TerminalColor::Green | TerminalColor::BrightGreen => 0x0055FF55,
                                TerminalColor::Yellow | TerminalColor::BrightYellow => 0x0055FFFF,
                                TerminalColor::Blue | TerminalColor::BrightBlue => 0x00FF5555,
                                TerminalColor::Magenta | TerminalColor::BrightMagenta => 0x00FF55FF,
                                TerminalColor::Cyan | TerminalColor::BrightCyan => 0x00FFFF55,
                                TerminalColor::BrightBlack => 0x00888888,
                                TerminalColor::Rgb(r, g, b) => {
                                    ((b as u32) << 16) | ((g as u32) << 8) | (r as u32)
                                }
                                _ => 0x00CCCCCC,
                            };

                            let bg_color = match cell.bg {
                                TerminalColor::Default | TerminalColor::Black => 0x001E1E1E,
                                TerminalColor::Blue => 0x00800000,
                                TerminalColor::White => 0x00FFFFFF,
                                TerminalColor::Rgb(r, g, b) => {
                                    ((b as u32) << 16) | ((g as u32) << 8) | (r as u32)
                                }
                                _ => 0x001E1E1E,
                            };

                            SetTextColor(hdc, fg_color);
                            SetBkColor(hdc, bg_color);

                            let ch_u16 = [cell.ch as u16];
                            TextOutW(hdc, x_pos, y_pos, ch_u16.as_ptr(), 1);
                        }
                    }
                }
            }

            EndPaint(hwnd, &ps);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
