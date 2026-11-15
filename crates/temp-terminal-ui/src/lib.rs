//! Decoupled graphical user interface crate for temp-terminal.
//!
//! Evaluated and selected in Phase 5. Connects to `temp-terminal-core` and
//! `temp-terminal-daemon` via the attach binary stream and JSON-RPC control socket.
//! Provides split-tree pane management, virtual terminal screen rendering,
//! scrollback buffer, OSC event dispatching, and daemon connection lifecycle.

pub mod editor;
pub mod host;
pub mod icons;
pub mod renderer;
pub mod rig_board;
pub mod settings;
pub mod workspace;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::renderer::{xterm_256_to_rgb, TerminalColor};
use serde::{Deserialize, Serialize};
use temp_terminal_core::registry::{Pane, Registry, StartPaneOptions};
use temp_terminal_proto::proto::{Hello, Message, Request, Welcome, PROTOCOL_VERSION};
use temp_terminal_term::osc::{parse_status, Report, Scanner};
use temp_terminal_term::records::Records;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex, RwLock};

#[derive(Error, Debug)]
pub enum UiError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Not connected: {0}")]
    NotConnected(String),
    #[error("Pane not found: {0}")]
    PaneNotFound(String),
    #[error("Shell resolution error: {0}")]
    ShellNotFound(String),
    #[error("Operation failed: {0}")]
    Other(String),
}

/// Resolves an executable on the system PATH or returns absolute path if found.
fn find_in_path(exe_name: &str) -> Option<PathBuf> {
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let full_path = dir.join(exe_name);
            if full_path.is_file() {
                return Some(full_path);
            }
        }
    }
    None
}

/// Discovers the default interactive shell on the host platform.
/// On Windows:
/// 1. Prefers `pwsh.exe` (PowerShell 7+) on PATH or standard install dirs.
/// 2. Falls back to `powershell.exe` (Windows PowerShell 5.1).
/// 3. Returns an error if neither can be resolved (does NOT silently fallback to cmd.exe).
pub fn resolve_default_shell() -> Result<(PathBuf, Vec<String>), UiError> {
    #[cfg(windows)]
    {
        // 1. Try pwsh.exe on PATH
        if let Some(path) = find_in_path("pwsh.exe") {
            return Ok((path, vec!["-NoLogo".to_string()]));
        }

        // Check common PowerShell 7 install locations
        if let Ok(prog_files) = std::env::var("ProgramFiles") {
            let p7 = PathBuf::from(&prog_files)
                .join("PowerShell")
                .join("7")
                .join("pwsh.exe");
            if p7.is_file() {
                return Ok((p7, vec!["-NoLogo".to_string()]));
            }
        }

        // 2. Try powershell.exe on PATH
        if let Some(path) = find_in_path("powershell.exe") {
            return Ok((path, vec!["-NoLogo".to_string()]));
        }

        // Check standard Windows System32 WindowsPowerShell location
        if let Ok(sys_root) = std::env::var("SystemRoot") {
            let win_ps = PathBuf::from(&sys_root)
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe");
            if win_ps.is_file() {
                return Ok((win_ps, vec!["-NoLogo".to_string()]));
            }
        }

        Err(UiError::ShellNotFound(
            "Neither PowerShell 7 (pwsh.exe) nor Windows PowerShell (powershell.exe) could be found on PATH or in standard Windows installation directories. Please install PowerShell.".to_string(),
        ))
    }
    #[cfg(not(windows))]
    {
        if let Some(path) = find_in_path("bash") {
            Ok((path, vec![]))
        } else if let Some(path) = find_in_path("sh") {
            Ok((path, vec![]))
        } else {
            Ok((PathBuf::from("/bin/sh"), vec![]))
        }
    }
}

/// Converts the resolved default shell into a command line vector suitable for ConPTY.
pub fn default_shell_command() -> Result<Vec<String>, UiError> {
    let (exe, args) = resolve_default_shell()?;
    let mut cmd = vec![exe.to_string_lossy().to_string()];
    cmd.extend(args);
    Ok(cmd)
}

/// Split tree layout direction.
/// Corresponds to Go `tempterm/terminal.go:21-26`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitDir {
    Vertical,   // Left | Right
    Horizontal, // Top / Bottom
}

/// Split tree node for terminal panes.
/// Corresponds to Go `splitNode` (`tempterm/terminal.go:28-36`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitNode {
    pub term_id: String,
    pub dir: SplitDir,
    pub ratio: f32, // percentage first takes (e.g., 50.0)
    pub first: Option<Box<SplitNode>>,
    pub second: Option<Box<SplitNode>>,
}

impl SplitNode {
    pub fn new_leaf(term_id: impl Into<String>) -> Self {
        Self {
            term_id: term_id.into(),
            dir: SplitDir::Vertical,
            ratio: 50.0,
            first: None,
            second: None,
        }
    }

    pub fn is_leaf(&self) -> bool {
        !self.term_id.is_empty()
    }

    pub fn leaf_count(&self) -> usize {
        if self.is_leaf() {
            1
        } else {
            let left = self.first.as_ref().map_or(0, |n| n.leaf_count());
            let right = self.second.as_ref().map_or(0, |n| n.leaf_count());
            left + right
        }
    }

    pub fn term_ids(&self) -> Vec<String> {
        if self.is_leaf() {
            vec![self.term_id.clone()]
        } else {
            let mut ids = Vec::new();
            if let Some(ref f) = self.first {
                ids.extend(f.term_ids());
            }
            if let Some(ref s) = self.second {
                ids.extend(s.term_ids());
            }
            ids
        }
    }

    /// Checks if this node or any child node contains the given terminal pane ID.
    pub fn contains(&self, target_id: &str) -> bool {
        if self.is_leaf() {
            self.term_id == target_id
        } else {
            self.first.as_ref().map_or(false, |f| f.contains(target_id))
                || self
                    .second
                    .as_ref()
                    .map_or(false, |s| s.contains(target_id))
        }
    }

    /// Clamps ratio according to min 120px pane size and Go baseline bounds (12% - 88%).
    pub fn clamp_ratio(ratio: f32, available_size: f32) -> f32 {
        if available_size < 240.0 {
            return 50.0;
        }
        let min_ratio = (120.0 / available_size * 100.0).max(12.0);
        let max_ratio = ((available_size - 120.0) / available_size * 100.0).min(88.0);
        if min_ratio >= max_ratio {
            50.0
        } else {
            ratio.clamp(min_ratio, max_ratio)
        }
    }

    /// Sets the ratio of the split node at the given binary path ("" for root, "a" for left/top, etc.).
    pub fn set_ratio_at_path(&mut self, path: &str, ratio: f32) -> bool {
        if path.is_empty() {
            if !self.is_leaf() {
                self.ratio = ratio;
                return true;
            }
            return false;
        }

        let (first_char, rest) = path.split_at(1);
        if first_char == "a" {
            if let Some(ref mut f) = self.first {
                return f.set_ratio_at_path(rest, ratio);
            }
        } else if first_char == "b" {
            if let Some(ref mut s) = self.second {
                return s.set_ratio_at_path(rest, ratio);
            }
        }
        false
    }

    /// Gets the ratio of the split node at the given binary path.
    pub fn get_ratio_at_path(&self, path: &str) -> Option<f32> {
        if path.is_empty() {
            if !self.is_leaf() {
                return Some(self.ratio);
            }
            return None;
        }

        let (first_char, rest) = path.split_at(1);
        if first_char == "a" {
            self.first.as_ref().and_then(|f| f.get_ratio_at_path(rest))
        } else if first_char == "b" {
            self.second.as_ref().and_then(|s| s.get_ratio_at_path(rest))
        } else {
            None
        }
    }

    pub fn split_leaf(&mut self, target_id: &str, dir: SplitDir, new_id: &str) -> bool {
        if self.is_leaf() {
            if self.term_id == target_id {
                let current_leaf = SplitNode::new_leaf(&self.term_id);
                let new_leaf = SplitNode::new_leaf(new_id);
                self.term_id = String::new();
                self.dir = dir;
                self.ratio = 50.0;
                self.first = Some(Box::new(current_leaf));
                self.second = Some(Box::new(new_leaf));
                return true;
            }
            return false;
        }

        if let Some(ref mut f) = self.first {
            if f.split_leaf(target_id, dir, new_id) {
                return true;
            }
        }
        if let Some(ref mut s) = self.second {
            if s.split_leaf(target_id, dir, new_id) {
                return true;
            }
        }
        false
    }

    pub fn remove_leaf(root: Option<Box<SplitNode>>, target_id: &str) -> Option<Box<SplitNode>> {
        let mut node = root?;
        if node.is_leaf() {
            if node.term_id == target_id {
                return None;
            }
            return Some(node);
        }

        let new_first = Self::remove_leaf(node.first.take(), target_id);
        let new_second = Self::remove_leaf(node.second.take(), target_id);

        match (new_first, new_second) {
            (None, None) => None,
            (Some(f), None) => Some(f),
            (None, Some(s)) => Some(s),
            (Some(f), Some(s)) => {
                node.first = Some(f);
                node.second = Some(s);
                Some(node)
            }
        }
    }
}

/// Terminal cell in the screen grid with character and ANSI styling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenCell {
    pub ch: char,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub bold: bool,
    pub faint: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Default for ScreenCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: TerminalColor::Default,
            bg: TerminalColor::Default,
            bold: false,
            faint: false,
            italic: false,
            underline: false,
        }
    }
}

/// In-memory terminal screen and scrollback buffer for a pane.
#[derive(Debug, Clone)]
pub struct TerminalBuffer {
    pub cols: u16,
    pub rows: u16,
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub scrollback: std::collections::VecDeque<String>,
    pub screen: Vec<Vec<ScreenCell>>,
    pub max_scrollback: usize,
    pub max_bytes: usize,
    current_bytes: usize,
    utf8_remainder: Vec<u8>,
    escape_remainder: Vec<u8>,
    current_fg: TerminalColor,
    current_bg: TerminalColor,
    current_bold: bool,
    current_faint: bool,
    current_italic: bool,
    current_underline: bool,
    pub cursor_visible: bool,
}

pub const DEFAULT_MAX_TERMINAL_BYTES: usize = 16 * 1024 * 1024; // 16 MB max retained text per pane
pub const MAX_LINE_BYTES: usize = 64 * 1024; // 64 KB max single line length

impl TerminalBuffer {
    pub fn new(cols: u16, rows: u16, max_scrollback: usize) -> Self {
        let cols_usize = cols.max(1) as usize;
        let rows_usize = rows.max(1) as usize;
        Self {
            cols,
            rows,
            cursor_col: 0,
            cursor_row: 0,
            scrollback: std::collections::VecDeque::new(),
            screen: vec![vec![ScreenCell::default(); cols_usize]; rows_usize],
            max_scrollback: max_scrollback.clamp(100, 20_000),
            max_bytes: DEFAULT_MAX_TERMINAL_BYTES,
            current_bytes: 0,
            utf8_remainder: Vec::new(),
            escape_remainder: Vec::new(),
            current_fg: TerminalColor::Default,
            current_bg: TerminalColor::Default,
            current_bold: false,
            current_faint: false,
            current_italic: false,
            current_underline: false,
            cursor_visible: true,
        }
    }

    /// Appends incoming raw text/output to the buffer, advancing lines on newline,
    /// handling incremental UTF-8 chunks and escape sequences across boundaries,
    /// ANSI cursor movement, erase line/screen, erase characters, carriage return,
    /// backspace, and tabs.
    pub fn append_output(&mut self, bytes: &[u8]) {
        // Prepend remainder from previous partial UTF-8 sequence if any (capped at 4 bytes)
        let mut data = std::mem::take(&mut self.utf8_remainder);
        if data.len() > 4 {
            data.clear();
        }
        data.extend_from_slice(bytes);

        // Extract valid UTF-8 and save trailing incomplete bytes
        let (valid_text, trailing) = match std::str::from_utf8(&data) {
            Ok(s) => (s, &[][..]),
            Err(e) => {
                let valid_up_to = e.valid_up_to();
                let valid = unsafe { std::str::from_utf8_unchecked(&data[..valid_up_to]) };
                let rem = &data[valid_up_to..];
                // Only buffer if remainder looks like an incomplete UTF-8 boundary (up to 4 bytes)
                if rem.len() < 4 && e.error_len().is_none() {
                    (valid, rem)
                } else {
                    // Invalid sequence or too long; lossy convert everything
                    let lossy = String::from_utf8_lossy(&data).to_string();
                    let full_text = if !self.escape_remainder.is_empty() {
                        let prev = String::from_utf8_lossy(&self.escape_remainder).to_string();
                        self.escape_remainder.clear();
                        prev + &lossy
                    } else {
                        lossy
                    };
                    self.process_text(&full_text);
                    return;
                }
            }
        };

        if trailing.len() < 4 {
            self.utf8_remainder.extend_from_slice(trailing);
        }

        let full_text = if !self.escape_remainder.is_empty() {
            let prev = String::from_utf8_lossy(&self.escape_remainder).to_string();
            self.escape_remainder.clear();
            prev + valid_text
        } else {
            valid_text.to_string()
        };

        self.process_text(&full_text);
    }

    fn line_to_string(cells: &[ScreenCell]) -> String {
        let last_non_blank = cells.iter().rposition(|c| c.ch != ' ');
        let len = match last_non_blank {
            Some(idx) => idx + 1,
            None => {
                // If the entire row is blank spaces, check if it contains any spaces
                return String::new();
            }
        };

        let mut res = String::new();
        let mut cur_fg = TerminalColor::Default;
        let mut cur_bg = TerminalColor::Default;
        let mut cur_bold = false;
        let mut cur_faint = false;
        let mut cur_italic = false;
        let mut cur_underline = false;

        for cell in &cells[..len] {
            if cell.fg != cur_fg
                || cell.bg != cur_bg
                || cell.bold != cur_bold
                || cell.faint != cur_faint
                || cell.italic != cur_italic
                || cell.underline != cur_underline
            {
                let mut parts = Vec::new();
                if cell.fg == TerminalColor::Default
                    && cell.bg == TerminalColor::Default
                    && !cell.bold
                    && !cell.faint
                    && !cell.italic
                    && !cell.underline
                {
                    parts.push("0".to_string());
                } else {
                    if cell.bold != cur_bold {
                        parts.push(if cell.bold { "1" } else { "22" }.to_string());
                    }
                    if cell.faint != cur_faint {
                        parts.push(if cell.faint { "2" } else { "22" }.to_string());
                    }
                    if cell.italic != cur_italic {
                        parts.push(if cell.italic { "3" } else { "23" }.to_string());
                    }
                    if cell.underline != cur_underline {
                        parts.push(if cell.underline { "4" } else { "24" }.to_string());
                    }
                    if cell.fg != cur_fg {
                        match cell.fg {
                            TerminalColor::Default => parts.push("39".to_string()),
                            TerminalColor::Black => parts.push("30".to_string()),
                            TerminalColor::Red => parts.push("31".to_string()),
                            TerminalColor::Green => parts.push("32".to_string()),
                            TerminalColor::Yellow => parts.push("33".to_string()),
                            TerminalColor::Blue => parts.push("34".to_string()),
                            TerminalColor::Magenta => parts.push("35".to_string()),
                            TerminalColor::Cyan => parts.push("36".to_string()),
                            TerminalColor::White => parts.push("37".to_string()),
                            TerminalColor::BrightBlack => parts.push("90".to_string()),
                            TerminalColor::BrightRed => parts.push("91".to_string()),
                            TerminalColor::BrightGreen => parts.push("92".to_string()),
                            TerminalColor::BrightYellow => parts.push("93".to_string()),
                            TerminalColor::BrightBlue => parts.push("94".to_string()),
                            TerminalColor::BrightMagenta => parts.push("95".to_string()),
                            TerminalColor::BrightCyan => parts.push("96".to_string()),
                            TerminalColor::BrightWhite => parts.push("97".to_string()),
                            TerminalColor::Rgb(r, g, b) => {
                                parts.push(format!("38;2;{};{};{}", r, g, b));
                            }
                        }
                    }
                    if cell.bg != cur_bg {
                        match cell.bg {
                            TerminalColor::Default => parts.push("49".to_string()),
                            TerminalColor::Black => parts.push("40".to_string()),
                            TerminalColor::Red => parts.push("41".to_string()),
                            TerminalColor::Green => parts.push("42".to_string()),
                            TerminalColor::Yellow => parts.push("43".to_string()),
                            TerminalColor::Blue => parts.push("44".to_string()),
                            TerminalColor::Magenta => parts.push("45".to_string()),
                            TerminalColor::Cyan => parts.push("46".to_string()),
                            TerminalColor::White => parts.push("47".to_string()),
                            TerminalColor::BrightBlack => parts.push("100".to_string()),
                            TerminalColor::BrightRed => parts.push("101".to_string()),
                            TerminalColor::BrightGreen => parts.push("102".to_string()),
                            TerminalColor::BrightYellow => parts.push("103".to_string()),
                            TerminalColor::BrightBlue => parts.push("104".to_string()),
                            TerminalColor::BrightMagenta => parts.push("105".to_string()),
                            TerminalColor::BrightCyan => parts.push("106".to_string()),
                            TerminalColor::BrightWhite => parts.push("107".to_string()),
                            TerminalColor::Rgb(r, g, b) => {
                                parts.push(format!("48;2;{};{};{}", r, g, b));
                            }
                        }
                    }
                }
                res.push_str(&format!("\x1b[{}m", parts.join(";")));
                cur_fg = cell.fg;
                cur_bg = cell.bg;
                cur_bold = cell.bold;
                cur_faint = cell.faint;
                cur_italic = cell.italic;
                cur_underline = cell.underline;
            }
            res.push(cell.ch);
        }

        if cur_fg != TerminalColor::Default
            || cur_bg != TerminalColor::Default
            || cur_bold
            || cur_faint
            || cur_italic
            || cur_underline
        {
            res.push_str("\x1b[m");
        }

        // Cap rendered line to MAX_LINE_BYTES
        if res.len() > MAX_LINE_BYTES {
            res.truncate(MAX_LINE_BYTES);
        }

        res
    }

    fn push_scrollback_line(&mut self, cells: Vec<ScreenCell>) {
        let line = Self::line_to_string(&cells);
        let line_len = line.len().max(cells.len());
        self.current_bytes = self.current_bytes.saturating_add(line_len);
        self.scrollback.push_back(line);
        if self.scrollback.len() > self.max_scrollback {
            if let Some(evicted) = self.scrollback.pop_front() {
                let ev_len = evicted.len().max(self.cols as usize);
                self.current_bytes = self.current_bytes.saturating_sub(ev_len);
            }
        }
        while self.current_bytes > self.max_bytes && !self.scrollback.is_empty() {
            if let Some(evicted) = self.scrollback.pop_front() {
                let ev_len = evicted.len().max(self.cols as usize);
                self.current_bytes = self.current_bytes.saturating_sub(ev_len);
            }
        }
    }

    fn process_text(&mut self, text: &str) {
        let chars_vec: Vec<char> = text.chars().collect();
        let len = chars_vec.len();
        let mut i = 0;
        let cols = self.cols.max(1) as usize;

        while i < len {
            let ch = chars_vec[i];

            if ch == '\x1b' {
                // Check if this ESC sequence might be incomplete at the end of the chunk
                if i + 1 >= len {
                    self.escape_remainder.push(b'\x1b');
                    break;
                }

                if chars_vec[i + 1] == '[' {
                    // CSI sequence: ESC [ ... final_byte
                    let start = i;
                    let mut j = i + 2;
                    let mut complete = false;
                    while j < len {
                        let c = chars_vec[j];
                        if c.is_ascii_digit() || c == ';' || c == '?' || c == ' ' {
                            j += 1;
                        } else if ('@'..='~').contains(&c) {
                            complete = true;
                            break;
                        } else {
                            break;
                        }
                    }

                    if !complete {
                        let remaining_str: String = chars_vec[start..].iter().collect();
                        if remaining_str.len() <= 64 {
                            self.escape_remainder = remaining_str.into_bytes();
                            break;
                        }
                    }

                    let params_str: String = chars_vec[i + 2..j].iter().collect();
                    let cmd = chars_vec[j];
                    i = j + 1; // advance past final byte

                    match cmd {
                        'H' | 'f' => {
                            let parts: Vec<&str> = params_str.split(';').collect();
                            let r = parts
                                .first()
                                .and_then(|s| s.parse::<usize>().ok())
                                .unwrap_or(1);
                            let c = parts
                                .get(1)
                                .and_then(|s| s.parse::<usize>().ok())
                                .unwrap_or(1);
                            self.cursor_row = (r.saturating_sub(1)).min(self.rows as usize - 1);
                            self.cursor_col = (c.saturating_sub(1)).min(cols - 1);
                        }
                        'A' => {
                            let count = params_str.parse::<usize>().unwrap_or(1);
                            self.cursor_row = self.cursor_row.saturating_sub(count);
                        }
                        'B' => {
                            let count = params_str.parse::<usize>().unwrap_or(1);
                            self.cursor_row = (self.cursor_row + count).min(self.rows as usize - 1);
                        }
                        'C' => {
                            let count = params_str.parse::<usize>().unwrap_or(1);
                            self.cursor_col = (self.cursor_col + count).min(cols - 1);
                        }
                        'D' => {
                            let count = params_str.parse::<usize>().unwrap_or(1);
                            self.cursor_col = self.cursor_col.saturating_sub(count);
                        }
                        'G' => {
                            let col = params_str.parse::<usize>().unwrap_or(1);
                            self.cursor_col = (col.saturating_sub(1)).min(cols - 1);
                        }
                        'd' => {
                            let row = params_str.parse::<usize>().unwrap_or(1);
                            self.cursor_row = (row.saturating_sub(1)).min(self.rows as usize - 1);
                        }
                        'J' => {
                            let code = params_str
                                .trim_start_matches('?')
                                .parse::<u32>()
                                .unwrap_or(0);
                            if code == 2 {
                                self.screen =
                                    vec![vec![ScreenCell::default(); cols]; self.rows as usize];
                                self.cursor_row = 0;
                                self.cursor_col = 0;
                            } else if code == 3 {
                                self.scrollback.clear();
                                self.current_bytes = 0;
                            }
                        }
                        'K' => {
                            let code = params_str.parse::<u32>().unwrap_or(0);
                            if self.cursor_row < self.screen.len() {
                                let line = &mut self.screen[self.cursor_row];
                                if code == 2 {
                                    for cell in line.iter_mut() {
                                        *cell = ScreenCell::default();
                                    }
                                } else if code == 0 {
                                    for cell in line.iter_mut().skip(self.cursor_col) {
                                        *cell = ScreenCell::default();
                                    }
                                } else if code == 1 {
                                    for cell in line.iter_mut().take(self.cursor_col + 1) {
                                        *cell = ScreenCell::default();
                                    }
                                }
                            }
                        }
                        'X' => {
                            let count = params_str.parse::<usize>().unwrap_or(1).max(1);
                            if self.cursor_row < self.screen.len() {
                                let line = &mut self.screen[self.cursor_row];
                                for cell in line.iter_mut().skip(self.cursor_col).take(count) {
                                    *cell = ScreenCell::default();
                                }
                            }
                        }
                        'h' => {
                            // DEC Private Mode Set: ESC [ ? 25 h (Show Cursor)
                            if params_str.contains("?25") {
                                self.cursor_visible = true;
                            }
                        }
                        'l' => {
                            // DEC Private Mode Reset: ESC [ ? 25 l (Hide Cursor)
                            if params_str.contains("?25") {
                                self.cursor_visible = false;
                            }
                        }
                        'm' => {
                            // SGR Color / style parser
                            if params_str.is_empty() {
                                self.current_fg = TerminalColor::Default;
                                self.current_bg = TerminalColor::Default;
                                self.current_bold = false;
                                self.current_faint = false;
                                self.current_italic = false;
                                self.current_underline = false;
                            } else {
                                let nums: Vec<u32> = params_str
                                    .split(';')
                                    .filter_map(|s| s.parse::<u32>().ok())
                                    .collect();
                                let mut p_idx = 0;
                                while p_idx < nums.len() {
                                    match nums[p_idx] {
                                        0 => {
                                            self.current_fg = TerminalColor::Default;
                                            self.current_bg = TerminalColor::Default;
                                            self.current_bold = false;
                                            self.current_faint = false;
                                            self.current_italic = false;
                                            self.current_underline = false;
                                        }
                                        1 => self.current_bold = true,
                                        2 => self.current_faint = true,
                                        3 => self.current_italic = true,
                                        4 => self.current_underline = true,
                                        22 => {
                                            self.current_bold = false;
                                            self.current_faint = false;
                                        }
                                        23 => self.current_italic = false,
                                        24 => self.current_underline = false,
                                        30 => self.current_fg = TerminalColor::Black,
                                        31 => self.current_fg = TerminalColor::Red,
                                        32 => self.current_fg = TerminalColor::Green,
                                        33 => self.current_fg = TerminalColor::Yellow,
                                        34 => self.current_fg = TerminalColor::Blue,
                                        35 => self.current_fg = TerminalColor::Magenta,
                                        36 => self.current_fg = TerminalColor::Cyan,
                                        37 => self.current_fg = TerminalColor::White,
                                        38 => {
                                            if p_idx + 4 < nums.len() && nums[p_idx + 1] == 2 {
                                                // 38;2;r;g;b Truecolor 24-bit
                                                let r = nums[p_idx + 2].min(255) as u8;
                                                let g = nums[p_idx + 3].min(255) as u8;
                                                let b = nums[p_idx + 4].min(255) as u8;
                                                self.current_fg = TerminalColor::Rgb(r, g, b);
                                                p_idx += 4;
                                            } else if p_idx + 2 < nums.len() && nums[p_idx + 1] == 5 {
                                                // 38;5;n 256-color palette
                                                let n = nums[p_idx + 2].min(255) as u8;
                                                let (r, g, b) = xterm_256_to_rgb(n);
                                                self.current_fg = TerminalColor::Rgb(r, g, b);
                                                p_idx += 2;
                                            }
                                        }
                                        39 => self.current_fg = TerminalColor::Default,
                                        40 => self.current_bg = TerminalColor::Black,
                                        41 => self.current_bg = TerminalColor::Red,
                                        42 => self.current_bg = TerminalColor::Green,
                                        43 => self.current_bg = TerminalColor::Yellow,
                                        44 => self.current_bg = TerminalColor::Blue,
                                        45 => self.current_bg = TerminalColor::Magenta,
                                        46 => self.current_bg = TerminalColor::Cyan,
                                        47 => self.current_bg = TerminalColor::White,
                                        48 => {
                                            if p_idx + 4 < nums.len() && nums[p_idx + 1] == 2 {
                                                // 48;2;r;g;b Truecolor 24-bit
                                                let r = nums[p_idx + 2].min(255) as u8;
                                                let g = nums[p_idx + 3].min(255) as u8;
                                                let b = nums[p_idx + 4].min(255) as u8;
                                                self.current_bg = TerminalColor::Rgb(r, g, b);
                                                p_idx += 4;
                                            } else if p_idx + 2 < nums.len() && nums[p_idx + 1] == 5 {
                                                // 48;5;n 256-color palette
                                                let n = nums[p_idx + 2].min(255) as u8;
                                                let (r, g, b) = xterm_256_to_rgb(n);
                                                self.current_bg = TerminalColor::Rgb(r, g, b);
                                                p_idx += 2;
                                            }
                                        }
                                        49 => self.current_bg = TerminalColor::Default,
                                        90 => self.current_fg = TerminalColor::BrightBlack,
                                        91 => self.current_fg = TerminalColor::BrightRed,
                                        92 => self.current_fg = TerminalColor::BrightGreen,
                                        93 => self.current_fg = TerminalColor::BrightYellow,
                                        94 => self.current_fg = TerminalColor::BrightBlue,
                                        95 => self.current_fg = TerminalColor::BrightMagenta,
                                        96 => self.current_fg = TerminalColor::BrightCyan,
                                        97 => self.current_fg = TerminalColor::BrightWhite,
                                        100 => self.current_bg = TerminalColor::BrightBlack,
                                        101 => self.current_bg = TerminalColor::BrightRed,
                                        102 => self.current_bg = TerminalColor::BrightGreen,
                                        103 => self.current_bg = TerminalColor::BrightYellow,
                                        104 => self.current_bg = TerminalColor::BrightBlue,
                                        105 => self.current_bg = TerminalColor::BrightMagenta,
                                        106 => self.current_bg = TerminalColor::BrightCyan,
                                        107 => self.current_bg = TerminalColor::BrightWhite,
                                        _ => {}
                                    }
                                    p_idx += 1;
                                }
                            }
                        }
                        _ => {}
                    }
                    continue;
                } else if chars_vec[i + 1] == ']' {
                    let start = i;
                    let mut j = i + 2;
                    let mut complete = false;
                    while j < len {
                        if chars_vec[j] == '\x07' {
                            complete = true;
                            j += 1;
                            break;
                        } else if chars_vec[j] == '\x1b' && j + 1 < len && chars_vec[j + 1] == '\\'
                        {
                            complete = true;
                            j += 2;
                            break;
                        } else {
                            j += 1;
                        }
                    }
                    if complete {
                        i = j;
                        continue;
                    } else {
                        let remaining_str: String = chars_vec[start..].iter().collect();
                        if remaining_str.len() <= 512 {
                            self.escape_remainder = remaining_str.into_bytes();
                            break;
                        }
                    }
                } else {
                    let next_ch = chars_vec[i + 1];
                    i += 2;
                    if next_ch == 'M' {
                        self.cursor_row = self.cursor_row.saturating_sub(1);
                    }
                    continue;
                }
            }

            if ch == '\n' {
                if self.cursor_row + 1 < self.rows as usize {
                    self.cursor_row += 1;
                } else {
                    let first = self.screen.remove(0);
                    self.push_scrollback_line(first);
                    self.screen.push(vec![ScreenCell::default(); cols]);
                }
                self.cursor_col = 0;
            } else if ch == '\r' {
                self.cursor_col = 0;
            } else if ch == '\x08' {
                self.cursor_col = self.cursor_col.saturating_sub(1);
            } else if ch == '\t' {
                let next_tab = (self.cursor_col + 8) & !7;
                self.cursor_col = next_tab.min(cols - 1);
            } else if !ch.is_control() && self.cursor_row < self.screen.len() {
                // Auto-wrap to next row if cursor reaches or exceeds column boundary
                if self.cursor_col >= cols {
                    self.cursor_col = 0;
                    if self.cursor_row + 1 < self.rows as usize {
                        self.cursor_row += 1;
                    } else {
                        let first = self.screen.remove(0);
                        self.push_scrollback_line(first);
                        self.screen.push(vec![ScreenCell::default(); cols]);
                    }
                }
                let line = &mut self.screen[self.cursor_row];
                line[self.cursor_col] = ScreenCell {
                    ch,
                    fg: self.current_fg,
                    bg: self.current_bg,
                    bold: self.current_bold,
                    faint: self.current_faint,
                    italic: self.current_italic,
                    underline: self.current_underline,
                };
                self.cursor_col += 1;
            }
            i += 1;
        }
    }

    /// Resizes screen rows and cols.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
        let cols_usize = cols.max(1) as usize;
        let rows_usize = rows.max(1) as usize;

        for line in self.screen.iter_mut() {
            line.resize(cols_usize, ScreenCell::default());
        }
        self.screen
            .resize(rows_usize, vec![ScreenCell::default(); cols_usize]);

        if self.cursor_row >= rows_usize {
            self.cursor_row = rows_usize.saturating_sub(1);
        }
        if self.cursor_col >= cols_usize {
            self.cursor_col = cols_usize.saturating_sub(1);
        }
    }

    /// Returns rendered lines for viewing (scrollback + screen).
    /// If there is no scrollback yet, trims trailing empty lines on the screen so the
    /// terminal prompt and initial text starts neatly at the top of the viewport
    /// rather than creating artificial blank space before the bottom of the pane.
    pub fn render_view(&self) -> Vec<String> {
        let mut lines = Vec::with_capacity(self.scrollback.len() + self.screen.len());
        lines.extend(self.scrollback.iter().cloned());

        let mut screen_lines: Vec<String> = self
            .screen
            .iter()
            .map(|row| Self::line_to_string(row))
            .collect();
        if self.scrollback.is_empty() {
            // Keep up to cursor_row + 1, or the last non-empty line, whichever is greater
            let last_non_empty = screen_lines
                .iter()
                .rposition(|l| !l.is_empty())
                .unwrap_or(0);
            let keep_len = (last_non_empty + 1)
                .max(self.cursor_row + 1)
                .min(screen_lines.len());
            screen_lines.truncate(keep_len);
        }

        lines.extend(screen_lines);
        lines
    }
}

/// State of an active terminal pane in the frontend.
pub struct TerminalPane {
    pub id: String,
    pub name: String,
    pub buffer: RwLock<TerminalBuffer>,
    pub osc_scanner: Mutex<Scanner>,
    pub records: Mutex<Records>,
    pub pane_ref: Option<Arc<Pane>>,
    pub input_tx: mpsc::Sender<Vec<u8>>,
}

impl TerminalPane {
    pub fn new(
        id: String,
        name: String,
        cols: u16,
        rows: u16,
        pane_ref: Option<Arc<Pane>>,
        input_tx: mpsc::Sender<Vec<u8>>,
    ) -> Self {
        Self {
            id,
            name,
            buffer: RwLock::new(TerminalBuffer::new(cols, rows, 10_000)),
            osc_scanner: Mutex::new(Scanner::new()),
            records: Mutex::new(Records::new()),
            pane_ref,
            input_tx,
        }
    }

    /// Feeds incoming terminal output to the pane's buffer and OSC scanner.
    pub async fn feed_output(&self, data: &[u8]) -> Vec<Report> {
        {
            let mut buf = self.buffer.write().await;
            buf.append_output(data);
        }

        let mut reports = Vec::new();
        let mut did_reset = false;
        {
            let mut scanner = self.osc_scanner.lock().await;

            scanner.scan(
                data,
                |body| {
                    let s = String::from_utf8_lossy(body);
                    let payload = s.strip_prefix("7501;").unwrap_or(&s);
                    if let Some(r) = parse_status(payload) {
                        reports.push(r);
                    }
                },
                || {
                    did_reset = true;
                },
            );
        }

        if did_reset || !reports.is_empty() {
            let mut recs = self.records.lock().await;
            if did_reset {
                recs.reset();
            }
            for r in &reports {
                recs.apply(r.record.clone());
            }
        }

        reports
    }

    /// Sends user input (keystrokes) to the underlying process.
    pub async fn send_input(&self, input: &[u8]) -> Result<(), UiError> {
        if let Some(ref p) = self.pane_ref {
            p.write_input(input).map_err(UiError::Io)?;
            return Ok(());
        }
        self.input_tx
            .send(input.to_vec())
            .await
            .map_err(|e| UiError::Other(e.to_string()))
    }

    /// Resizes the pane.
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<(), UiError> {
        {
            let mut buf = self.buffer.write().await;
            buf.resize(cols, rows);
        }
        if let Some(ref p) = self.pane_ref {
            p.resize(cols, rows)
                .map_err(|e| UiError::Other(e.to_string()))?;
        }
        Ok(())
    }
}

/// Represents an application session containing split tabs and terminal panes.
pub struct Session {
    pub id: String,
    pub name: String,
    pub worktree: Option<String>,
    pub layout: Option<Box<SplitNode>>,
    pub panes: HashMap<String, Arc<TerminalPane>>,
    pub active_pane_id: String,
    pub zoomed_pane_id: Option<String>,
}

impl Session {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            worktree: None,
            layout: None,
            panes: HashMap::new(),
            active_pane_id: String::new(),
            zoomed_pane_id: None,
        }
    }

    pub fn add_pane(&mut self, pane: Arc<TerminalPane>) {
        let pane_id = pane.id.clone();
        if self.layout.is_none() {
            self.layout = Some(Box::new(SplitNode::new_leaf(&pane_id)));
        } else if let Some(ref mut l) = self.layout {
            l.split_leaf(&self.active_pane_id, SplitDir::Vertical, &pane_id);
        }
        self.panes.insert(pane_id.clone(), pane);
        self.active_pane_id = pane_id;
    }

    pub fn remove_pane(&mut self, pane_id: &str) -> Option<Arc<TerminalPane>> {
        self.layout = SplitNode::remove_leaf(self.layout.take(), pane_id);
        let removed = self.panes.remove(pane_id);
        if self.zoomed_pane_id.as_deref() == Some(pane_id) {
            self.zoomed_pane_id = None;
        }
        if self.active_pane_id == pane_id {
            self.active_pane_id = self.panes.keys().next().cloned().unwrap_or_default();
        }
        removed
    }
}

/// Canonicalizes a path, stripping Windows `\\?\` UNC prefix if present.
pub fn canonicalize_path(p: &Path) -> PathBuf {
    if let Ok(c) = std::fs::canonicalize(p) {
        let s = c.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            PathBuf::from(stripped)
        } else {
            c
        }
    } else {
        p.to_path_buf()
    }
}

/// Reports whether two paths name the exact same directory (case-insensitive on Windows, matching Go samePath).
pub fn same_path(a: &Path, b: &Path) -> bool {
    let ca = canonicalize_path(a);
    let cb = canonicalize_path(b);
    #[cfg(windows)]
    {
        ca.to_string_lossy()
            .eq_ignore_ascii_case(&cb.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        ca == cb
    }
}

/// Shows native Windows folder picker dialog using PowerShell (with CREATE_NO_WINDOW).
#[cfg(windows)]
pub fn pick_folder_native() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$f = New-Object System.Windows.Forms.FolderBrowserDialog
$f.Description = 'Open Project'
if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
    Write-Output $f.SelectedPath
}
"#;
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;

    if output.status.success() {
        let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path_str.is_empty() {
            let p = PathBuf::from(path_str);
            if p.is_dir() {
                return Some(canonicalize_path(&p));
            }
        }
    }
    None
}

#[cfg(not(windows))]
pub fn pick_folder_native() -> Option<PathBuf> {
    None
}

/// Represents an active project in the workspace.
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub is_available: bool,
    pub sessions: HashMap<String, Session>,
    pub active_session_id: String,
}

impl Project {
    pub fn new(id: impl Into<String>, name: impl Into<String>, path: PathBuf) -> Self {
        let is_available = path.is_dir();
        Self {
            id: id.into(),
            name: name.into(),
            path,
            is_available,
            sessions: HashMap::new(),
            active_session_id: String::new(),
        }
    }
}

/// GUI / Terminal frontend application host.
/// Manages window lifecycle, project/session state, terminal panes, and IPC connection.
pub struct AppHost {
    pub projects: RwLock<HashMap<String, Project>>,
    pub active_project_id: RwLock<String>,
    pub daemon_addr: Option<SocketAddr>,
    pub is_connected: AtomicBool,
    pub is_closed: AtomicBool,
    pub req_id: std::sync::atomic::AtomicI64,
}

impl AppHost {
    pub fn new(daemon_addr: Option<SocketAddr>) -> Arc<Self> {
        Arc::new(Self {
            projects: RwLock::new(HashMap::new()),
            active_project_id: RwLock::new(String::new()),
            daemon_addr,
            is_connected: AtomicBool::new(false),
            is_closed: AtomicBool::new(false),
            req_id: std::sync::atomic::AtomicI64::new(1),
        })
    }

    /// Adds a project to the workspace.
    pub async fn add_project(&self, id: &str, name: &str, path: PathBuf) {
        let proj = Project::new(id, name, path);
        let mut projs = self.projects.write().await;
        projs.insert(id.to_string(), proj);
        let mut active = self.active_project_id.write().await;
        if active.is_empty() {
            *active = id.to_string();
        }
    }

    /// Switches the active project context by project id.
    pub async fn switch_project(&self, id: &str) -> bool {
        let projs = self.projects.read().await;
        if projs.contains_key(id) {
            let mut active = self.active_project_id.write().await;
            *active = id.to_string();
            drop(active);
            drop(projs);
            let _ = self.save_state().await;
            true
        } else {
            false
        }
    }

    /// Switches active session within the given or active project.
    pub async fn switch_session(&self, project_id: &str, session_id: &str) -> bool {
        let mut projs = self.projects.write().await;
        if let Some(proj) = projs.get_mut(project_id) {
            if proj.sessions.contains_key(session_id) {
                proj.active_session_id = session_id.to_string();
                drop(projs);
                let _ = self.save_state().await;
                return true;
            }
        }
        false
    }

    /// Creates a terminal pane wired directly to an attached PTY via Registry or Daemon.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_local_pane(
        self: &Arc<Self>,
        project_id: &str,
        session_id: &str,
        pane_id: &str,
        cols: u16,
        rows: u16,
        registry: &Arc<Registry>,
        command: Vec<String>,
        dir: String,
    ) -> Result<Arc<TerminalPane>, UiError> {
        let handle = tokio::runtime::Handle::current();
        self.spawn_local_pane_with_handle(
            project_id, session_id, pane_id, cols, rows, registry, command, dir, handle,
        )
        .await
    }

    /// Creates a terminal pane with an explicit tokio handle for background output dispatch.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_local_pane_with_handle(
        self: &Arc<Self>,
        project_id: &str,
        session_id: &str,
        pane_id: &str,
        cols: u16,
        rows: u16,
        registry: &Arc<Registry>,
        command: Vec<String>,
        dir: String,
        handle: tokio::runtime::Handle,
    ) -> Result<Arc<TerminalPane>, UiError> {
        self.spawn_local_pane_with_env(
            project_id,
            session_id,
            pane_id,
            cols,
            rows,
            registry,
            command,
            dir,
            Vec::new(),
            handle,
        )
        .await
    }

    /// Creates a terminal pane with custom environment variables and a tokio handle.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_local_pane_with_env(
        self: &Arc<Self>,
        project_id: &str,
        session_id: &str,
        pane_id: &str,
        cols: u16,
        rows: u16,
        registry: &Arc<Registry>,
        command: Vec<String>,
        dir: String,
        env: Vec<String>,
        handle: tokio::runtime::Handle,
    ) -> Result<Arc<TerminalPane>, UiError> {
        let (input_tx, mut input_rx) = mpsc::channel::<Vec<u8>>(128);
        // Start PTY in core Registry with custom environment
        let pane_handle = registry
            .start(StartPaneOptions {
                id: pane_id.to_string(),
                session: session_id.to_string(),
                project: project_id.to_string(),
                role: "main".to_string(),
                dir,
                command,
                env,
                cols,
                rows,
            })
            .map_err(|e| UiError::Other(e.to_string()))?;

        let pane_handle_in = pane_handle.clone();
        handle.spawn(async move {
            while let Some(bytes) = input_rx.recv().await {
                let _ = pane_handle_in.write_input(&bytes);
            }
        });

        let pane = Arc::new(TerminalPane::new(
            pane_id.to_string(),
            pane_id.to_string(),
            cols,
            rows,
            Some(pane_handle.clone()),
            input_tx,
        ));

        // Connect output callback
        let pane_output = pane.clone();
        let handle_out = handle.clone();
        registry.add_on_output(move |p_id, bytes| {
            if p_id == pane_output.id {
                let pane_async = pane_output.clone();
                let b = bytes.to_vec();
                handle_out.spawn(async move {
                    pane_async.feed_output(&b).await;
                });
            }
        });

        // Connect process exit callback to auto-close pane and session when shell exits
        let host_exit = Arc::clone(self);
        let reg_exit = Arc::clone(registry);
        let p_proj = project_id.to_string();
        let p_sess = session_id.to_string();
        let p_pane = pane_id.to_string();
        let handle_exit = handle.clone();
        registry.add_on_exit(move |exited_pane_id| {
            if exited_pane_id == p_pane {
                let h = host_exit.clone();
                let r = reg_exit.clone();
                let proj = p_proj.clone();
                let sess = p_sess.clone();
                let pane = p_pane.clone();
                handle_exit.spawn(async move {
                    let _ = h.close_pane(&proj, &sess, &pane, &r).await;
                });
            }
        });

        // Also check if history already captured initial bytes before set_on_output
        {
            let hist = pane_handle.history.lock().unwrap().clone();
            if !hist.is_empty() {
                pane.feed_output(&hist).await;
            }
        }

        // Add pane to project & session
        let mut projs = self.projects.write().await;
        let proj = projs
            .get_mut(project_id)
            .ok_or_else(|| UiError::Other(format!("project {} not found", project_id)))?;
        let folder_name = proj
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "terminal".to_string());
        let s_name = if session_id.starts_with("session_") || session_id.is_empty() {
            if proj.sessions.is_empty() {
                folder_name
            } else {
                format!("{} ({})", folder_name, proj.sessions.len() + 1)
            }
        } else {
            session_id.to_string()
        };
        let sess = proj
            .sessions
            .entry(session_id.to_string())
            .or_insert_with(|| Session::new(session_id, s_name));
        sess.add_pane(pane.clone());
        if proj.active_session_id.is_empty() || proj.active_session_id != session_id {
            proj.active_session_id = session_id.to_string();
        }

        Ok(pane)
    }

    /// Connects to the daemon socket and executes a handshake.
    pub async fn connect_daemon(&self) -> Result<(TcpStream, Welcome), UiError> {
        let addr = self
            .daemon_addr
            .ok_or_else(|| UiError::NotConnected("no daemon address provided".to_string()))?;

        let stream = TcpStream::connect(addr).await?;
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        let hello = Hello {
            kind: "control".to_string(),
            version: PROTOCOL_VERSION,
            client: "temp-terminal-ui".to_string(),
            pane: String::new(),
        };
        let mut hello_bytes = serde_json::to_vec(&hello)?;
        hello_bytes.push(b'\n');
        write_half.write_all(&hello_bytes).await?;

        let mut welcome_line = String::new();
        reader.read_line(&mut welcome_line).await?;
        let welcome: Welcome = serde_json::from_str(&welcome_line)?;

        if welcome.version != PROTOCOL_VERSION {
            return Err(UiError::Protocol(format!(
                "protocol version mismatch: daemon {}, client {}",
                welcome.version, PROTOCOL_VERSION
            )));
        }

        self.is_connected.store(true, Ordering::SeqCst);
        let stream = reader.into_inner().reunite(write_half).map_err(|e| {
            UiError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;

        Ok((stream, welcome))
    }

    /// Invokes a remote JSON-RPC method on the connected daemon.
    pub async fn call_daemon(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, UiError> {
        let (stream, _) = self.connect_daemon().await?;
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        let id = self.req_id.fetch_add(1, Ordering::SeqCst);
        let req = Request {
            id,
            method: method.to_string(),
            params,
        };

        let mut req_bytes = serde_json::to_vec(&req)?;
        req_bytes.push(b'\n');
        write_half.write_all(&req_bytes).await?;

        let mut resp_line = String::new();
        reader.read_line(&mut resp_line).await?;
        let msg: Message = serde_json::from_str(&resp_line)?;

        if !msg.error.is_empty() {
            return Err(UiError::Other(format!("{}: {}", msg.code, msg.error)));
        }

        Ok(msg.result.unwrap_or(serde_json::Value::Null))
    }

    /// Shuts down the host.
    pub fn close(&self) {
        self.is_closed.store(true, Ordering::SeqCst);
        self.is_connected.store(false, Ordering::SeqCst);
    }

    /// Handles keyboard actions and user commands in the active pane.
    pub async fn handle_action(&self, action: &str) -> Result<(), UiError> {
        let projs = self.projects.read().await;
        let active_proj = self.active_project_id.read().await;
        let proj = projs
            .get(&*active_proj)
            .ok_or_else(|| UiError::Other("no active project".to_string()))?;
        let sess = proj
            .sessions
            .get(&proj.active_session_id)
            .ok_or_else(|| UiError::Other("no active session".to_string()))?;
        let pane = sess
            .panes
            .get(&sess.active_pane_id)
            .ok_or_else(|| UiError::PaneNotFound("no active pane".to_string()))?;

        match action {
            "interrupt" => pane.send_input(b"\x03").await?,
            "eof" => pane.send_input(b"\x04").await?,
            "clear" => pane.send_input(b"\x0c").await?,
            _ => return Err(UiError::Other(format!("unknown action: {}", action))),
        }
        Ok(())
    }

    /// Splits the currently active pane in the specified direction.
    pub async fn split_active_pane(
        self: &Arc<Self>,
        dir: SplitDir,
        registry: &Arc<Registry>,
    ) -> Result<Arc<TerminalPane>, UiError> {
        let mut projs = self.projects.write().await;
        let active_proj_id = self.active_project_id.read().await.clone();
        let proj = projs
            .get_mut(&active_proj_id)
            .ok_or_else(|| UiError::Other("no active project".to_string()))?;
        let sess = proj
            .sessions
            .get_mut(&proj.active_session_id)
            .ok_or_else(|| UiError::Other("no active session".to_string()))?;

        let current_pane_id = sess.active_pane_id.clone();
        if current_pane_id.is_empty() {
            return Err(UiError::PaneNotFound("no active pane to split".to_string()));
        }

        let new_pane_id = format!("pane_{}", uuid_short());
        let new_pane_name = format!("Terminal {}", sess.panes.len() + 1);

        // Spawn PTY process for new pane
        let shell_cmd = default_shell_command()?;

        let cwd = proj.path.to_string_lossy().to_string();
        let pane_handle = registry
            .start(StartPaneOptions {
                id: new_pane_id.clone(),
                session: proj.active_session_id.clone(),
                project: active_proj_id.clone(),
                role: "split".to_string(),
                dir: cwd,
                command: shell_cmd,
                env: Vec::new(),
                cols: 80,
                rows: 24,
            })
            .map_err(|e| UiError::Other(e.to_string()))?;

        let (input_tx, mut input_rx) = mpsc::channel::<Vec<u8>>(128);
        let pane_handle_in = pane_handle.clone();
        tokio::spawn(async move {
            while let Some(bytes) = input_rx.recv().await {
                let _ = pane_handle_in.write_input(&bytes);
            }
        });

        let new_pane = Arc::new(TerminalPane::new(
            new_pane_id.clone(),
            new_pane_name,
            80,
            24,
            Some(pane_handle.clone()),
            input_tx,
        ));

        // Connect output subscriber
        let pane_output = new_pane.clone();
        let handle = tokio::runtime::Handle::current();
        let handle_out = handle.clone();
        registry.add_on_output(move |p_id, bytes| {
            if p_id == pane_output.id {
                let pane_async = pane_output.clone();
                let b = bytes.to_vec();
                handle_out.spawn(async move {
                    pane_async.feed_output(&b).await;
                });
            }
        });

        // Connect process exit callback to auto-close pane and session when shell exits
        let host_exit = Arc::clone(self);
        let reg_exit = Arc::clone(registry);
        let p_proj = active_proj_id.clone();
        let p_sess = proj.active_session_id.clone();
        let p_pane = new_pane_id.clone();
        let handle_exit = handle.clone();
        registry.add_on_exit(move |exited_pane_id| {
            if exited_pane_id == p_pane {
                let h = host_exit.clone();
                let r = reg_exit.clone();
                let proj = p_proj.clone();
                let sess = p_sess.clone();
                let pane = p_pane.clone();
                handle_exit.spawn(async move {
                    let _ = h.close_pane(&proj, &sess, &pane, &r).await;
                });
            }
        });

        // Check if history already captured initial bytes before set_on_output
        {
            let hist = pane_handle.history.lock().unwrap().clone();
            if !hist.is_empty() {
                new_pane.feed_output(&hist).await;
            }
        }

        sess.panes.insert(new_pane_id.clone(), new_pane.clone());
        if let Some(ref mut layout) = sess.layout {
            layout.split_leaf(&current_pane_id, dir, &new_pane_id);
        } else {
            sess.layout = Some(Box::new(SplitNode::new_leaf(&new_pane_id)));
        }
        sess.active_pane_id = new_pane_id;

        Ok(new_pane)
    }

    /// Closes the currently active pane in the active session.
    pub async fn close_active_pane(&self, registry: &Registry) -> Result<(), UiError> {
        let active_proj_id = self.active_project_id.read().await.clone();
        let projs = self.projects.read().await;
        let proj = projs
            .get(&active_proj_id)
            .ok_or_else(|| UiError::Other("no active project".to_string()))?;
        let sess = proj
            .sessions
            .get(&proj.active_session_id)
            .ok_or_else(|| UiError::Other("no active session".to_string()))?;
        let active_pane_id = sess.active_pane_id.clone();
        let sess_id = proj.active_session_id.clone();
        drop(projs);

        if !active_pane_id.is_empty() {
            self.close_pane(&active_proj_id, &sess_id, &active_pane_id, registry)
                .await?;
        }
        Ok(())
    }

    /// Cycles active pane focus forward (+1) or backward (-1).
    pub async fn cycle_pane_focus(&self, forward: bool) -> Result<String, UiError> {
        let mut projs = self.projects.write().await;
        let active_proj = self.active_project_id.read().await;
        let proj = projs
            .get_mut(&*active_proj)
            .ok_or_else(|| UiError::Other("no active project".to_string()))?;
        let sess = proj
            .sessions
            .get_mut(&proj.active_session_id)
            .ok_or_else(|| UiError::Other("no active session".to_string()))?;

        if let Some(ref layout) = sess.layout {
            let ids = layout.term_ids();
            if ids.is_empty() {
                return Ok(String::new());
            }
            let cur_idx = ids
                .iter()
                .position(|id| id == &sess.active_pane_id)
                .unwrap_or(0);
            let next_idx = if forward {
                (cur_idx + 1) % ids.len()
            } else if cur_idx == 0 {
                ids.len() - 1
            } else {
                cur_idx - 1
            };
            let next_id = ids[next_idx].clone();
            sess.active_pane_id = next_id.clone();
            Ok(next_id)
        } else {
            Ok(String::new())
        }
    }

    /// Updates the split ratio of the split node at the given binary path in the active session.
    pub async fn update_split_ratio(&self, path: &str, ratio: f32) -> bool {
        let active_proj_id = self.active_project_id.read().await.clone();
        let mut projs = self.projects.write().await;
        if let Some(proj) = projs.get_mut(&active_proj_id) {
            if let Some(sess) = proj.sessions.get_mut(&proj.active_session_id) {
                if let Some(ref mut layout) = sess.layout {
                    let updated = layout.set_ratio_at_path(path, ratio);
                    return updated;
                }
            }
        }
        false
    }

    /// Toggles zoom/maximize on the active pane in the active session (Ctrl+Shift+Z / Go parity).
    pub async fn toggle_zoom(&self) -> Option<String> {
        let active_proj_id = self.active_project_id.read().await.clone();
        let mut projs = self.projects.write().await;
        if let Some(proj) = projs.get_mut(&active_proj_id) {
            if let Some(sess) = proj.sessions.get_mut(&proj.active_session_id) {
                if sess.zoomed_pane_id.is_some() {
                    sess.zoomed_pane_id = None;
                    return None;
                } else if sess.panes.len() > 1
                    || sess.layout.as_ref().map_or(false, |l| !l.is_leaf())
                {
                    let target = if !sess.active_pane_id.is_empty()
                        && sess.panes.contains_key(&sess.active_pane_id)
                    {
                        sess.active_pane_id.clone()
                    } else if let Some(first_id) = sess.panes.keys().next() {
                        first_id.clone()
                    } else {
                        return None;
                    };
                    sess.zoomed_pane_id = Some(target.clone());
                    return Some(target);
                }
            }
        }
        None
    }

    /// Sets or clears the zoomed pane in the active session.
    pub async fn set_zoomed_pane(&self, pane_id: Option<String>) {
        let active_proj_id = self.active_project_id.read().await.clone();
        let mut projs = self.projects.write().await;
        if let Some(proj) = projs.get_mut(&active_proj_id) {
            if let Some(sess) = proj.sessions.get_mut(&proj.active_session_id) {
                if let Some(ref pid) = pane_id {
                    if sess.panes.contains_key(pid) {
                        sess.zoomed_pane_id = Some(pid.clone());
                        sess.active_pane_id = pid.clone();
                    } else {
                        sess.zoomed_pane_id = None;
                    }
                } else {
                    sess.zoomed_pane_id = None;
                }
            }
        }
    }

    /// Closes and removes a pane by ID, terminating its associated PTY.
    /// If the session has no remaining panes, the session is removed from the project.
    pub async fn close_pane(
        &self,
        project_id: &str,
        session_id: &str,
        pane_id: &str,
        registry: &Registry,
    ) -> Result<(), UiError> {
        let mut projs = self.projects.write().await;
        let proj = projs
            .get_mut(project_id)
            .ok_or_else(|| UiError::Other("project not found".to_string()))?;
        let sess = proj
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| UiError::Other("session not found".to_string()))?;

        if let Some(pane) = sess.remove_pane(pane_id) {
            if let Some(ref p) = pane.pane_ref {
                p.close();
            }
            registry.remove(pane_id);
        }

        // If active pane was removed, select another pane in the layout
        if sess.active_pane_id == pane_id {
            if let Some(ref layout) = sess.layout {
                let ids = layout.term_ids();
                sess.active_pane_id = ids.first().cloned().unwrap_or_default();
            } else {
                sess.active_pane_id = String::new();
            }
        }

        // If session has no remaining panes, clean up the session from the project
        if sess.panes.is_empty() {
            proj.sessions.remove(session_id);
            if proj.active_session_id == session_id {
                proj.active_session_id = proj.sessions.keys().next().cloned().unwrap_or_default();
            }
        }
        drop(projs);
        let _ = self.save_state().await;

        Ok(())
    }

    /// Closes an entire session in a project, terminating all its active terminal panes.
    pub async fn close_session(
        &self,
        project_id: &str,
        session_id: &str,
        registry: &Registry,
    ) -> Result<(), UiError> {
        let mut projs = self.projects.write().await;
        if let Some(proj) = projs.get_mut(project_id) {
            if let Some(mut sess) = proj.sessions.remove(session_id) {
                for (pane_id, pane) in sess.panes.drain() {
                    if let Some(ref p) = pane.pane_ref {
                        p.close();
                    }
                    registry.remove(&pane_id);
                }
                if proj.active_session_id == session_id {
                    proj.active_session_id =
                        proj.sessions.keys().next().cloned().unwrap_or_default();
                }
            }
        }
        drop(projs);
        let _ = self.save_state().await;
        Ok(())
    }

    /// Opens an existing project folder or switches to it if already registered (canonical path parity).
    /// If newly registered, initializes an initial terminal session and saves state.
    pub async fn open_or_switch_project(
        self: &Arc<Self>,
        dir: PathBuf,
        registry: &Arc<Registry>,
    ) -> Result<String, UiError> {
        if !dir.is_dir() {
            return Err(UiError::Other(format!(
                "path is not a valid directory: {:?}",
                dir
            )));
        }

        let norm_dir = canonicalize_path(&dir);

        // 1. Check if already registered
        let projs = self.projects.read().await;
        for (id, p) in projs.iter() {
            if same_path(&p.path, &norm_dir) {
                let existing_id = id.clone();
                drop(projs);
                self.switch_project(&existing_id).await;
                return Ok(existing_id);
            }
        }
        drop(projs);

        // 2. Add as new project
        let id = format!("proj_{}", uuid_short());
        let name = norm_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Workspace".to_string());

        let mut proj = Project::new(&id, &name, norm_dir.clone());
        proj.is_available = true;

        let mut projs = self.projects.write().await;
        projs.insert(id.clone(), proj);
        let mut active = self.active_project_id.write().await;
        *active = id.clone();
        drop(projs);
        drop(active);

        // 3. Spawns initial terminal session in the newly registered project
        let sess_id = format!("session_{}", uuid_short());
        let pane_id = format!("pane_{}", uuid_short());
        let shell_cmd = default_shell_command()?;
        let cwd_str = norm_dir.to_string_lossy().to_string();

        let _ = self
            .spawn_local_pane_with_handle(
                &id,
                &sess_id,
                &pane_id,
                120,
                36,
                registry,
                shell_cmd,
                cwd_str,
                tokio::runtime::Handle::current(),
            )
            .await?;

        let _ = self.save_state().await;
        Ok(id)
    }

    /// Removes/detaches a project, terminating all its active terminal sessions and PTYs.
    /// Selects another remaining project if the removed project was active.
    pub async fn remove_project(
        &self,
        project_id: &str,
        registry: &Registry,
    ) -> Result<(), UiError> {
        let mut projs = self.projects.write().await;
        if let Some(mut proj) = projs.remove(project_id) {
            for (_, mut sess) in proj.sessions.drain() {
                for (pane_id, pane) in sess.panes.drain() {
                    if let Some(ref p) = pane.pane_ref {
                        p.close();
                    }
                    registry.remove(&pane_id);
                }
            }
        }

        let mut active = self.active_project_id.write().await;
        if *active == project_id {
            if let Some(next_id) = projs.keys().next() {
                *active = next_id.clone();
            } else {
                *active = String::new();
            }
        }
        drop(projs);
        drop(active);

        let _ = self.save_state().await;
        Ok(())
    }

    /// Atomically persists workspace projects and sessions to a specific state file.
    pub async fn save_state_to_path(&self, state_path: &Path) -> Result<(), UiError> {
        let projs = self.projects.read().await;
        let active_id = self.active_project_id.read().await.clone();

        let mut proto_projects = Vec::new();
        let mut proto_sessions = Vec::new();
        let mut active_workspace = String::new();

        for (p_id, p) in projs.iter() {
            if p_id == &active_id {
                active_workspace = p.path.to_string_lossy().to_string();
            }
            proto_projects.push(temp_terminal_proto::proto::Project {
                id: p.id.clone(),
                name: p.name.clone(),
                dir: p.path.to_string_lossy().to_string(),
            });

            for (s_id, s) in &p.sessions {
                proto_sessions.push(temp_terminal_proto::proto::Session {
                    id: s_id.clone(),
                    project: p.id.clone(),
                    name: s.name.clone(),
                    focus: s.active_pane_id.clone(),
                    ..Default::default()
                });
            }
        }

        let state = temp_terminal_proto::proto::State {
            projects: Some(proto_projects),
            sessions: Some(proto_sessions),
            viewed: active_id,
            started: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
            workspace: active_workspace,
            ports: None,
        };

        if let Some(parent) = state_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let data =
            serde_json::to_string_pretty(&state).map_err(|e| UiError::Other(e.to_string()))?;

        let tmp_path = state_path.with_extension("tmp");
        std::fs::write(&tmp_path, data).map_err(UiError::Io)?;
        std::fs::rename(&tmp_path, state_path).map_err(UiError::Io)?;

        Ok(())
    }

    /// Atomically persists workspace projects and sessions to state.json.
    pub async fn save_state(&self) -> Result<(), UiError> {
        self.save_state_to_path(&temp_terminal_core::paths::state_file())
            .await
    }

    /// Loads persisted projects and sessions from a specific state file.
    pub async fn load_state_from_path(
        self: &Arc<Self>,
        state_path: &Path,
        registry: &Arc<Registry>,
    ) -> Result<usize, UiError> {
        if !state_path.is_file() {
            return Ok(0);
        }

        let content = std::fs::read_to_string(state_path).map_err(UiError::Io)?;
        let st: temp_terminal_proto::proto::State =
            serde_json::from_str(&content).map_err(|e| UiError::Other(e.to_string()))?;

        let mut loaded_count = 0;
        let mut projs_guard = self.projects.write().await;
        let mut first_active = st.viewed.clone();

        if let Some(projects) = st.projects {
            for p in projects {
                if p.dir.is_empty() {
                    continue;
                }
                let p_path = PathBuf::from(&p.dir);
                let is_available = p_path.is_dir();
                let id = if p.id.is_empty() {
                    format!("proj_{}", uuid_short())
                } else {
                    p.id.clone()
                };
                let name = if p.name.is_empty() {
                    p_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Workspace".to_string())
                } else {
                    p.name.clone()
                };

                let mut proj = Project::new(&id, &name, p_path);
                proj.is_available = is_available;
                projs_guard.insert(id.clone(), proj);
                loaded_count += 1;

                if first_active.is_empty() && is_available {
                    first_active = id;
                }
            }
        }

        if !first_active.is_empty() && projs_guard.contains_key(&first_active) {
            let mut active = self.active_project_id.write().await;
            *active = first_active.clone();
        } else if let Some((first_id, _)) = projs_guard.iter().find(|(_, p)| p.is_available) {
            let mut active = self.active_project_id.write().await;
            *active = first_id.clone();
        }
        drop(projs_guard);

        // If active project has no sessions, spawn initial terminal
        let active_id = self.active_project_id.read().await.clone();
        if !active_id.is_empty() {
            let projs = self.projects.read().await;
            if let Some(p) = projs.get(&active_id) {
                if p.is_available && p.sessions.is_empty() {
                    let p_path = p.path.clone();
                    drop(projs);
                    let sess_id = format!("session_{}", uuid_short());
                    let pane_id = format!("pane_{}", uuid_short());
                    if let Ok(shell_cmd) = default_shell_command() {
                        let _ = self
                            .spawn_local_pane_with_handle(
                                &active_id,
                                &sess_id,
                                &pane_id,
                                120,
                                36,
                                registry,
                                shell_cmd,
                                p_path.to_string_lossy().to_string(),
                                tokio::runtime::Handle::current(),
                            )
                            .await;
                    }
                }
            }
        }

        Ok(loaded_count)
    }

    /// Loads persisted projects and sessions from state.json, restoring workspace state.
    pub async fn load_state(self: &Arc<Self>, registry: &Arc<Registry>) -> Result<usize, UiError> {
        self.load_state_from_path(&temp_terminal_core::paths::state_file(), registry)
            .await
    }
}

/// Generates a short random ID for sessions and panes.
pub fn uuid_short() -> String {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{:x}", nanos)
}
