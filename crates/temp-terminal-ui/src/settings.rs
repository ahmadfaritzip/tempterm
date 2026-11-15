//! Persistent application settings and preferences matching Go `tempterm/settings.go`.
//!
//! Provides schema serialization and deserialization, atomic safe-saving to disk (`settings.json`),
//! and application of defaults when configuration is missing.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use temp_terminal_core::paths;

/// Settings structure mirroring Go `tempterm/settings.go:18-62`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: String,
    pub density: String,
    pub background: String,
    pub dim_unfocused: bool,
    pub focus_follows_mouse: bool,
    pub minimum_contrast: f32,
    pub reduce_motion: bool,
    pub show_system_load: bool,
    pub notify: NotifySettings,
    pub control_socket: bool,
    pub remote_control: bool,
    pub daemon: bool,
    pub restore_sessions: bool,
    pub resume_agents: bool,
    pub git_status: bool,
    pub confirm_close: bool,
    pub open_links: String,
    pub search_engine: String,
    pub terminal: TerminalSettings,
    pub editor: EditorSettings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifySettings {
    pub waiting: bool,
    pub done: bool,
    pub error: bool,
    pub sound: bool,
    pub programs: bool,
}

impl Default for NotifySettings {
    fn default() -> Self {
        Self {
            waiting: true,
            done: true,
            error: true,
            sound: true,
            programs: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSettings {
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    pub ligatures: bool,
    pub cursor: String,
    pub cursor_blink: bool,
    pub copy_on_select: bool,
    pub right_click: String,
    pub clipboard_read: String,
    pub scrollback_mb: i32,
    pub shell: String,
    pub bell: String,
    pub option_as_meta: bool,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            font_family: String::new(),
            font_size: 13.0,
            line_height: 1.0,
            ligatures: true,
            cursor: "block".to_string(),
            cursor_blink: true,
            copy_on_select: true,
            right_click: "menu".to_string(),
            clipboard_read: "never".to_string(),
            scrollback_mb: 10,
            shell: String::new(),
            bell: "off".to_string(),
            option_as_meta: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorSettings {
    pub terminal_font: bool,
    pub tab_size: i32,
    pub wrap: bool,
    pub line_numbers: bool,
    pub markdown_preview: bool,
    pub diff_split: bool,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            terminal_font: true,
            tab_size: 4,
            wrap: true,
            line_numbers: true,
            markdown_preview: true,
            diff_split: true,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            density: "comfortable".to_string(),
            background: "wash".to_string(),
            dim_unfocused: true,
            focus_follows_mouse: true,
            minimum_contrast: 0.0,
            reduce_motion: false,
            show_system_load: true,
            notify: NotifySettings::default(),
            control_socket: true,
            remote_control: false,
            daemon: false,
            restore_sessions: true,
            resume_agents: true,
            git_status: true,
            confirm_close: false,
            open_links: "system".to_string(),
            search_engine: "duckduckgo".to_string(),
            terminal: TerminalSettings::default(),
            editor: EditorSettings::default(),
        }
    }
}

impl Settings {
    /// Loads settings from the canonical `settings.json` file.
    /// Returns default settings if the file does not exist or fails to parse.
    pub fn load() -> Self {
        let path = paths::settings_file();
        Self::load_from_path(&path)
    }

    /// Loads settings from an explicit path.
    pub fn load_from_path(path: &Path) -> Self {
        if let Ok(bytes) = fs::read(path) {
            if let Ok(s) = serde_json::from_slice::<Settings>(&bytes) {
                return s;
            }
        }
        Self::default()
    }

    /// Saves settings atomically to the canonical `settings.json` file.
    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = paths::settings_file();
        self.save_to_path(&path)
    }

    /// Saves settings atomically to an explicit path using a temp file.
    pub fn save_to_path(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let tmp_path = path.with_extension("tmp");
        fs::write(&tmp_path, json.as_bytes())?;
        fs::rename(&tmp_path, path)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default_and_roundtrip() {
        let dir = std::env::temp_dir().join(format!("tt_settings_test_{}", std::process::id()));
        let file = dir.join("settings.json");
        let _ = fs::remove_dir_all(&dir);

        // Test default loading when not present
        let s = Settings::load_from_path(&file);
        assert_eq!(s.theme, "dark");
        assert_eq!(s.terminal.font_size, 13.0);
        assert!(s.terminal.cursor_blink);

        // Test mutating and saving
        let mut modified = s;
        modified.theme = "light".to_string();
        modified.terminal.font_size = 16.0;
        modified.editor.tab_size = 2;
        modified.save_to_path(&file).expect("save should succeed");

        // Verify file exists and reload
        let reloaded = Settings::load_from_path(&file);
        assert_eq!(reloaded.theme, "light");
        assert_eq!(reloaded.terminal.font_size, 16.0);
        assert_eq!(reloaded.editor.tab_size, 2);

        let _ = fs::remove_dir_all(&dir);
    }
}
