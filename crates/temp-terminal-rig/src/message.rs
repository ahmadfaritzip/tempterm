use serde::{Deserialize, Serialize};

use crate::db::RigDatabase;
use crate::error::RigError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub ticket: Option<String>,
    pub frm: String,
    pub too: String,
    pub body: String,
    pub seen: bool,
    pub at: String,
    pub story: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendMessageParams<'a> {
    pub ticket: Option<&'a str>,
    pub frm: &'a str,
    pub too: &'a str,
    pub body: &'a str,
    pub story: Option<&'a str>,
}

impl RigDatabase {
    pub fn send_message(
        &self,
        ticket: Option<&str>,
        frm: &str,
        too: &str,
        body: &str,
        story: Option<&str>,
    ) -> Result<i64, RigError> {
        self.send_message_params(&SendMessageParams {
            ticket,
            frm,
            too,
            body,
            story,
        })
    }

    pub fn send_message_params(&self, params: &SendMessageParams<'_>) -> Result<i64, RigError> {
        let at = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO messages (ticket, frm, too, body, seen, at, story)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
            rusqlite::params![
                params.ticket,
                params.frm,
                params.too,
                params.body,
                at,
                params.story
            ],
        )?;

        let id = self.conn.last_insert_rowid();
        Ok(id)
    }

    pub fn read_inbox(
        &self,
        recipient: &str,
        mark_seen: bool,
        story: Option<&str>,
    ) -> Result<Vec<Message>, RigError> {
        let mut messages = if let Some(story_filter) = story {
            let mut stmt = self.conn.prepare(
                "SELECT id, ticket, frm, too, body, seen, at, story
                 FROM messages
                 WHERE too = ?1 AND seen = 0 AND (story = ?2 OR story IS NULL)
                 ORDER BY id ASC",
            )?;
            let rows = stmt.query_map(rusqlite::params![recipient, story_filter], row_to_message)?;
            let mut list = Vec::new();
            for item in rows {
                list.push(item?);
            }
            list
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, ticket, frm, too, body, seen, at, story
                 FROM messages
                 WHERE too = ?1 AND seen = 0
                 ORDER BY id ASC",
            )?;
            let rows = stmt.query_map(rusqlite::params![recipient], row_to_message)?;
            let mut list = Vec::new();
            for item in rows {
                list.push(item?);
            }
            list
        };

        if mark_seen && !messages.is_empty() {
            let ids: Vec<i64> = messages.iter().map(|m| m.id).collect();
            self.mark_messages_seen(&ids)?;
            for msg in &mut messages {
                msg.seen = true;
            }
        }

        Ok(messages)
    }

    pub fn mark_messages_seen(&self, ids: &[i64]) -> Result<(), RigError> {
        if ids.is_empty() {
            return Ok(());
        }

        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "UPDATE messages SET seen = 1 WHERE id IN ({})",
            placeholders.join(", ")
        );
        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();

        self.conn.execute(&sql, rusqlite::params_from_iter(params))?;
        Ok(())
    }
}

fn row_to_message(row: &rusqlite::Row<'_>) -> Result<Message, rusqlite::Error> {
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
}
