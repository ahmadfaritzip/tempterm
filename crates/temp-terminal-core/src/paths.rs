//! Standard path resolution matching Go `tempterm/internal/paths/paths.go`.
//!
//! Provides canonical locations for:
//! - Config dir (`state.json`, `settings.json`, `ui.json`)
//! - State dir (binary links, histories, logs)
//! - Runtime dir (IPC control socket, daemon socket)

use std::env;
use std::path::PathBuf;

/// Application directory name.
/// `TEMPTERM_DEV=1`, `TEMP_TERMINAL_DEV=1`, or `OMATERM_DEV=1` uses `tempterm-dev`.
pub fn app_name() -> &'static str {
    if env::var("TEMPTERM_DEV").as_deref() == Ok("1")
        || env::var("TEMP_TERMINAL_DEV").as_deref() == Ok("1")
        || env::var("OMATERM_DEV").as_deref() == Ok("1")
    {
        "tempterm-dev"
    } else {
        "tempterm"
    }
}

/// User's home directory.
pub fn home_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(profile) = env::var("USERPROFILE") {
            return PathBuf::from(profile);
        }
        if let (Ok(drive), Ok(path)) = (env::var("HOMEDRIVE"), env::var("HOMEPATH")) {
            return PathBuf::from(format!("{}{}", drive, path));
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home);
        }
    }
    PathBuf::from("/")
}

/// Directory holding configuration files (`state.json`, `settings.json`, `ui.json`).
/// Overridden by `TEMPTERM_CONFIG_HOME`, `TEMP_TERMINAL_CONFIG_HOME`, or `OMATERM_CONFIG_HOME`.
pub fn config_dir() -> PathBuf {
    if let Ok(d) = env::var("TEMPTERM_CONFIG_HOME")
        .or_else(|_| env::var("TEMP_TERMINAL_CONFIG_HOME"))
        .or_else(|_| env::var("OMATERM_CONFIG_HOME"))
    {
        return PathBuf::from(d).join(app_name());
    }

    #[cfg(windows)]
    {
        if let Ok(app_data) = env::var("APPDATA") {
            return PathBuf::from(app_data).join(app_name());
        }
    }

    #[cfg(not(windows))]
    {
        if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join(app_name());
        }
    }

    home_dir().join(".config").join(app_name())
}

/// Canonical path to `state.json`.
pub fn state_file() -> PathBuf {
    config_dir().join("state.json")
}

/// Canonical path to `settings.json`.
pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

/// Canonical path to `ui.json`.
pub fn ui_file() -> PathBuf {
    config_dir().join("ui.json")
}

/// Directory holding runtime state, logs, and binaries (`~/.local/state/temp-terminal` on Unix, `%LOCALAPPDATA%/temp-terminal` on Windows).
/// Overridden by `XDG_STATE_HOME`.
pub fn state_dir() -> PathBuf {
    if let Ok(d) = env::var("XDG_STATE_HOME") {
        return PathBuf::from(d).join(app_name());
    }

    #[cfg(windows)]
    {
        if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
            return PathBuf::from(local_app_data).join(app_name());
        }
    }

    home_dir().join(".local").join("state").join(app_name())
}

/// Directory holding binaries and tool links (`bin/`).
pub fn bin_dir() -> PathBuf {
    state_dir().join("bin")
}

/// Directory holding log files (`logs/`).
pub fn logs_dir() -> PathBuf {
    state_dir().join("logs")
}

/// Directory holding session histories (`sessions/`).
pub fn sessions_dir() -> PathBuf {
    state_dir().join("sessions")
}

/// Runtime directory for sockets and IPC endpoints.
pub fn runtime_dir() -> PathBuf {
    if let Ok(d) = env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(d).join(app_name());
    }
    config_dir()
}

/// Default TCP address or pipe for daemon IPC server.
pub fn default_daemon_addr() -> String {
    env::var("TEMPTERM_ADDR")
        .or_else(|_| env::var("TEMP_TERMINAL_ADDR"))
        .or_else(|_| env::var("OMATERM_ADDR"))
        .unwrap_or_else(|_| "127.0.0.1:9099".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_resolution() {
        assert_eq!(app_name(), "tempterm");
        assert!(config_dir().to_string_lossy().contains("tempterm"));
        assert!(state_file().to_string_lossy().ends_with("state.json"));
        assert!(settings_file().to_string_lossy().ends_with("settings.json"));
        assert!(ui_file().to_string_lossy().ends_with("ui.json"));
        assert!(bin_dir().to_string_lossy().ends_with("bin"));
        assert!(logs_dir().to_string_lossy().ends_with("logs"));
        assert_eq!(default_daemon_addr(), "127.0.0.1:9099");
    }

    #[test]
    fn test_env_precedence_and_dev_mode() {
        // Test custom config home override
        let orig_config_home = env::var("TEMP_TERMINAL_CONFIG_HOME").ok();
        env::set_var("TEMP_TERMINAL_CONFIG_HOME", "D:\\custom_test_config");
        let cfg = config_dir();
        assert!(cfg.starts_with("D:\\custom_test_config"));
        assert!(cfg.ends_with("tempterm"));

        // Clean up or restore
        if let Some(val) = orig_config_home {
            env::set_var("TEMP_TERMINAL_CONFIG_HOME", val);
        } else {
            env::remove_var("TEMP_TERMINAL_CONFIG_HOME");
        }

        // Test custom daemon address override
        let orig_addr = env::var("TEMP_TERMINAL_ADDR").ok();
        env::set_var("TEMP_TERMINAL_ADDR", "127.0.0.1:19099");
        assert_eq!(default_daemon_addr(), "127.0.0.1:19099");
        if let Some(val) = orig_addr {
            env::set_var("TEMP_TERMINAL_ADDR", val);
        } else {
            env::remove_var("TEMP_TERMINAL_ADDR");
        }
    }
}
