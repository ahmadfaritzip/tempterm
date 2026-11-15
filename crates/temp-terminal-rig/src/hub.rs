use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredRepo {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Story {
    pub id: String,
    pub title: String,
    pub body: String,
    pub status: String,
    pub created: String,
}

pub fn resolve_hub_dir() -> PathBuf {
    let base = if let Ok(val) = std::env::var("RIG_HOME") {
        if val.trim().is_empty() {
            return PathBuf::from("./.rig");
        }
        PathBuf::from(val.trim())
    } else {
        #[cfg(windows)]
        let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"));
        #[cfg(not(windows))]
        let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"));

        match home {
            Ok(h) if !h.trim().is_empty() => PathBuf::from(h.trim()),
            _ => return PathBuf::from("./.rig"),
        }
    };

    if base.file_name() == Some(std::ffi::OsStr::new(".rig")) {
        base
    } else {
        base.join(".rig")
    }
}
