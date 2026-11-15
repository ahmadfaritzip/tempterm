use std::path::Path;
use rusqlite::Connection;
use crate::error::RigError;
use crate::hub::{RegisteredRepo, Story};
use crate::message::Message;

pub struct RigDatabase {
    pub(crate) conn: Connection,
}

impl RigDatabase {
    pub fn open_repo(repo_root: &Path) -> Result<Self, RigError> {
        let rig_dir = if repo_root.file_name() == Some(std::ffi::OsStr::new(".rig")) {
            repo_root.to_path_buf()
        } else {
            repo_root.join(".rig")
        };
        std::fs::create_dir_all(&rig_dir)?;

        let db_path = rig_dir.join("rig.db");
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;",
        )?;
        init_repo_schema(&conn)?;

        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self, RigError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA busy_timeout = 5000;")?;
        init_repo_schema(&conn)?;

        Ok(Self { conn })
    }

    pub fn open_hub(hub_home: &Path) -> Result<Self, RigError> {
        let rig_dir = if hub_home.file_name() == Some(std::ffi::OsStr::new(".rig")) {
            hub_home.to_path_buf()
        } else {
            hub_home.join(".rig")
        };
        std::fs::create_dir_all(&rig_dir)?;

        let db_path = rig_dir.join("hub.db");
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;",
        )?;
        init_hub_schema(&conn)?;

        Ok(Self { conn })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn register_repo(&self, name: &str, path: &Path) -> Result<(), RigError> {
        let norm_path = if let Ok(canon) = std::fs::canonicalize(path) {
            let s = canon.to_string_lossy().to_string();
            let s = s.strip_prefix(r"\\?\").unwrap_or(&s).to_string();
            s.replace('\\', "/")
        } else {
            path.to_string_lossy().to_string().replace('\\', "/")
        };

        self.conn.execute(
            "INSERT OR REPLACE INTO repos (name, path) VALUES (?1, ?2)",
            rusqlite::params![name, norm_path],
        )?;
        Ok(())
    }

    pub fn list_repos(&self) -> Result<Vec<RegisteredRepo>, RigError> {
        let mut stmt = self.conn.prepare("SELECT name, path FROM repos ORDER BY name ASC")?;
        let rows = stmt.query_map([], |row| {
            Ok(RegisteredRepo {
                name: row.get(0)?,
                path: row.get(1)?,
            })
        })?;
        let mut repos = Vec::new();
        for r in rows {
            repos.push(r?);
        }
        Ok(repos)
    }

    pub fn remove_repo(&self, name_or_path: &str) -> Result<bool, RigError> {
        let norm = name_or_path.replace('\\', "/");
        let changes = self.conn.execute(
            "DELETE FROM repos WHERE name = ?1 OR path = ?1 OR path = ?2",
            rusqlite::params![name_or_path, norm],
        )?;
        Ok(changes > 0)
    }

    pub fn upsert_story(&self, id: &str, title: &str, body: &str) -> Result<Story, RigError> {
        if let Some(existing) = self.get_story(id)? {
            let new_title = if !title.is_empty() {
                title.to_string()
            } else {
                existing.title
            };
            let new_body = if !body.is_empty() {
                body.to_string()
            } else {
                existing.body
            };
            self.conn.execute(
                "UPDATE stories SET title = ?1, body = ?2 WHERE id = ?3",
                rusqlite::params![new_title, new_body, id],
            )?;
            Ok(Story {
                id: existing.id,
                title: new_title,
                body: new_body,
                status: existing.status,
                created: existing.created,
            })
        } else {
            let created = chrono::Utc::now().to_rfc3339();
            let status = "open".to_string();
            self.conn.execute(
                "INSERT INTO stories (id, title, body, status, created) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, title, body, status, created],
            )?;
            Ok(Story {
                id: id.to_string(),
                title: title.to_string(),
                body: body.to_string(),
                status,
                created,
            })
        }
    }

    pub fn get_story(&self, id: &str) -> Result<Option<Story>, RigError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, body, status, created FROM stories WHERE id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Story {
                id: row.get(0)?,
                title: row.get(1)?,
                body: row.get(2)?,
                status: row.get(3)?,
                created: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_stories(&self) -> Result<Vec<Story>, RigError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, body, status, created FROM stories ORDER BY created DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Story {
                id: row.get(0)?,
                title: row.get(1)?,
                body: row.get(2)?,
                status: row.get(3)?,
                created: row.get(4)?,
            })
        })?;
        let mut stories = Vec::new();
        for s in rows {
            stories.push(s?);
        }
        Ok(stories)
    }

    pub fn link_story_repo(&self, story_id: &str, repo_name: &str) -> Result<(), RigError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO story_repos (story, repo) VALUES (?1, ?2)",
            rusqlite::params![story_id, repo_name],
        )?;
        Ok(())
    }

    pub fn get_story_repos(&self, story_id: &str) -> Result<Vec<String>, RigError> {
        let mut stmt = self.conn.prepare(
            "SELECT repo FROM story_repos WHERE story = ?1 ORDER BY repo ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![story_id], |row| row.get(0))?;
        let mut repos = Vec::new();
        for r in rows {
            repos.push(r?);
        }
        Ok(repos)
    }

    pub fn send_hub_message(
        &self,
        ticket: Option<&str>,
        frm: &str,
        too: &str,
        body: &str,
        story: Option<&str>,
    ) -> Result<Message, RigError> {
        let at = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO messages (ticket, frm, too, body, seen, at, story)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
            rusqlite::params![ticket, frm, too, body, at, story],
        )?;
        let id = self.conn.last_insert_rowid();
        Ok(Message {
            id,
            ticket: ticket.map(|s| s.to_string()),
            frm: frm.to_string(),
            too: too.to_string(),
            body: body.to_string(),
            seen: false,
            at,
            story: story.map(|s| s.to_string()),
        })
    }

    pub fn read_hub_inbox(
        &self,
        too: &str,
        mark_seen: bool,
        story: Option<&str>,
    ) -> Result<Vec<Message>, RigError> {
        let mut messages = if let Some(story_filter) = story {
            let mut stmt = self.conn.prepare(
                "SELECT id, ticket, frm, too, body, seen, at, story
                 FROM messages
                 WHERE too = ?1 AND story = ?2
                 ORDER BY id ASC",
            )?;
            let rows = stmt.query_map(rusqlite::params![too, story_filter], |row| {
                let seen_int: i64 = row.get(5)?;
                Ok(Message {
                    id: row.get(0)?,
                    ticket: row.get(1)?,
                    frm: row.get(2)?,
                    too: row.get(3)?,
                    body: row.get(4)?,
                    seen: seen_int != 0,
                    at: row.get(6)?,
                    story: row.get(7)?,
                })
            })?;
            let mut list = Vec::new();
            for item in rows {
                list.push(item?);
            }
            list
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, ticket, frm, too, body, seen, at, story
                 FROM messages
                 WHERE too = ?1
                 ORDER BY id ASC",
            )?;
            let rows = stmt.query_map(rusqlite::params![too], |row| {
                let seen_int: i64 = row.get(5)?;
                Ok(Message {
                    id: row.get(0)?,
                    ticket: row.get(1)?,
                    frm: row.get(2)?,
                    too: row.get(3)?,
                    body: row.get(4)?,
                    seen: seen_int != 0,
                    at: row.get(6)?,
                    story: row.get(7)?,
                })
            })?;
            let mut list = Vec::new();
            for item in rows {
                list.push(item?);
            }
            list
        };

        if mark_seen {
            let unread_ids: Vec<i64> = messages.iter().filter(|m| !m.seen).map(|m| m.id).collect();
            if !unread_ids.is_empty() {
                self.mark_messages_seen(&unread_ids)?;
                for m in &mut messages {
                    m.seen = true;
                }
            }
        }

        Ok(messages)
    }
}

fn init_repo_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS tickets (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            body TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'open',
            manager TEXT NOT NULL DEFAULT '',
            review INTEGER NOT NULL DEFAULT 0,
            branch TEXT,
            worktree TEXT,
            created TEXT NOT NULL,
            after TEXT,
            base TEXT,
            story TEXT
        );

        CREATE TABLE IF NOT EXISTS runs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ticket TEXT NOT NULL,
            role TEXT NOT NULL,
            harness TEXT NOT NULL,
            model TEXT,
            effort TEXT,
            mode TEXT NOT NULL DEFAULT 'pane',
            pane TEXT,
            sid TEXT NOT NULL,
            pid INTEGER,
            cmd TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'running',
            cwd TEXT NOT NULL,
            started TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ticket TEXT,
            frm TEXT NOT NULL,
            too TEXT NOT NULL,
            body TEXT NOT NULL,
            seen INTEGER NOT NULL DEFAULT 0,
            at TEXT NOT NULL,
            story TEXT
        );

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            val TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_messages_too_seen ON messages (too, seen);",
    )?;
    Ok(())
}

fn init_hub_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS repos (
            name TEXT PRIMARY KEY,
            path TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS stories (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            body TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'open',
            created TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS story_repos (
            story TEXT NOT NULL,
            repo TEXT NOT NULL,
            PRIMARY KEY (story, repo)
        );

        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ticket TEXT,
            frm TEXT NOT NULL,
            too TEXT NOT NULL,
            body TEXT NOT NULL,
            seen INTEGER NOT NULL DEFAULT 0,
            at TEXT NOT NULL,
            story TEXT
        );

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            val TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_messages_too_seen ON messages (too, seen);",
    )?;
    Ok(())
}
