use std::collections::HashMap;

use crate::db::RigDatabase;
use crate::error::RigError;

pub fn default_setting(key: &str) -> &'static str {
    match key {
        "worker.harness" => "claude",
        "reviewer.harness" => "codex",
        "mode" => "pane",
        "worktree" => "on",
        "autonomy" => "review",
        "merge" => "local",
        _ => "",
    }
}

impl RigDatabase {
    pub fn get_setting(&self, key: &str) -> Result<Option<String>, RigError> {
        let mut stmt = self.conn.prepare("SELECT val FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(rusqlite::params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_setting_or_default(&self, key: &str) -> Result<String, RigError> {
        if let Some(val) = self.get_setting(key)? {
            return Ok(val);
        }
        Ok(default_setting(key).to_string())
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), RigError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, val) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub fn list_settings(&self) -> Result<HashMap<String, String>, RigError> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, val FROM settings ORDER BY key ASC")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let mut map = HashMap::new();
        for item in rows {
            let (k, v) = item?;
            map.insert(k, v);
        }
        Ok(map)
    }
}
