use std::fmt;
use std::str::FromStr;
use serde::{Deserialize, Serialize};

use crate::db::RigDatabase;
use crate::error::RigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    Open,
    Draft,
    Working,
    Reviewing,
    Review,
    NeedsYou,
    Blocked,
    Done,
}

impl TicketStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Draft => "draft",
            Self::Working => "working",
            Self::Reviewing => "reviewing",
            Self::Review => "review",
            Self::NeedsYou => "needs_you",
            Self::Blocked => "blocked",
            Self::Done => "done",
        }
    }
}

impl fmt::Display for TicketStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for TicketStatus {
    type Err = RigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "open" => Ok(Self::Open),
            "draft" => Ok(Self::Draft),
            "working" => Ok(Self::Working),
            "reviewing" => Ok(Self::Reviewing),
            "review" => Ok(Self::Review),
            "needs_you" | "needs-you" | "needs you" => Ok(Self::NeedsYou),
            "blocked" => Ok(Self::Blocked),
            "done" => Ok(Self::Done),
            other => Err(RigError::InvalidParameter(format!(
                "unknown ticket status: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticket {
    pub id: String,
    pub title: String,
    pub body: String,
    pub status: TicketStatus,
    pub manager: String,
    pub review: bool,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub created: String,
    pub after: Option<String>,
    pub base: Option<String>,
    pub story: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TicketFilter<'a> {
    pub manager: Option<&'a str>,
    pub include_done: bool,
    pub story: Option<&'a str>,
}

fn row_to_ticket(row: &rusqlite::Row) -> Result<Ticket, rusqlite::Error> {
    let status_str: String = row.get(3)?;
    let status = status_str.parse::<TicketStatus>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let review_int: i64 = row.get(5)?;
    Ok(Ticket {
        id: row.get(0)?,
        title: row.get(1)?,
        body: row.get(2)?,
        status,
        manager: row.get(4)?,
        review: review_int != 0,
        branch: row.get(6)?,
        worktree: row.get(7)?,
        created: row.get(8)?,
        after: row.get(9)?,
        base: row.get(10)?,
        story: row.get(11)?,
    })
}

impl RigDatabase {
    pub fn create_ticket(
        &self,
        title: &str,
        body: &str,
        manager: &str,
        review: bool,
        after: Option<&str>,
        base: Option<&str>,
        story: Option<&str>,
    ) -> Result<Ticket, RigError> {
        let next_num: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(CAST(SUBSTR(id, 3) AS INTEGER)), 0) + 1 FROM tickets WHERE id LIKE 'T-%';",
            [],
            |row| row.get(0),
        )?;
        let id = format!("T-{}", next_num);
        let created = chrono::Utc::now().to_rfc3339();
        let status = TicketStatus::Open;

        self.conn.execute(
            "INSERT INTO tickets (id, title, body, status, manager, review, branch, worktree, created, after, base, story)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12);",
            rusqlite::params![
                id,
                title,
                body,
                status.as_str(),
                manager,
                if review { 1 } else { 0 },
                None::<String>,
                None::<String>,
                created,
                after,
                base,
                story,
            ],
        )?;

        Ok(Ticket {
            id,
            title: title.to_string(),
            body: body.to_string(),
            status,
            manager: manager.to_string(),
            review,
            branch: None,
            worktree: None,
            created,
            after: after.map(|s| s.to_string()),
            base: base.map(|s| s.to_string()),
            story: story.map(|s| s.to_string()),
        })
    }

    pub fn get_ticket(&self, id: &str) -> Result<Ticket, RigError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, body, status, manager, review, branch, worktree, created, after, base, story
             FROM tickets WHERE id = ?1;",
        )?;
        match stmt.query_row([id], row_to_ticket) {
            Ok(ticket) => Ok(ticket),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Err(RigError::TicketNotFound(id.to_string()))
            }
            Err(e) => Err(RigError::Database(e)),
        }
    }

    pub fn update_ticket_status(&self, id: &str, status: TicketStatus) -> Result<(), RigError> {
        let count = self.conn.execute(
            "UPDATE tickets SET status = ?1 WHERE id = ?2;",
            rusqlite::params![status.as_str(), id],
        )?;
        if count == 0 {
            return Err(RigError::TicketNotFound(id.to_string()));
        }
        Ok(())
    }

    pub fn update_ticket(&self, ticket: &Ticket) -> Result<(), RigError> {
        let count = self.conn.execute(
            "UPDATE tickets SET
                title = ?1,
                body = ?2,
                status = ?3,
                manager = ?4,
                review = ?5,
                branch = ?6,
                worktree = ?7,
                created = ?8,
                after = ?9,
                base = ?10,
                story = ?11
             WHERE id = ?12;",
            rusqlite::params![
                ticket.title,
                ticket.body,
                ticket.status.as_str(),
                ticket.manager,
                if ticket.review { 1 } else { 0 },
                ticket.branch,
                ticket.worktree,
                ticket.created,
                ticket.after,
                ticket.base,
                ticket.story,
                ticket.id,
            ],
        )?;
        if count == 0 {
            return Err(RigError::TicketNotFound(ticket.id.clone()));
        }
        Ok(())
    }

    pub fn list_tickets(&self, filter: &TicketFilter) -> Result<Vec<Ticket>, RigError> {
        let mut conditions = Vec::new();
        let mut params: Vec<rusqlite::types::Value> = Vec::new();

        if !filter.include_done {
            conditions.push("status != 'done'");
        }
        if let Some(m) = filter.manager {
            conditions.push("manager = ?");
            params.push(rusqlite::types::Value::Text(m.to_string()));
        }
        if let Some(s) = filter.story {
            conditions.push("story = ?");
            params.push(rusqlite::types::Value::Text(s.to_string()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };

        let query = format!(
            "SELECT id, title, body, status, manager, review, branch, worktree, created, after, base, story
             FROM tickets{}
             ORDER BY CAST(SUBSTR(id, 3) AS INTEGER) ASC, id ASC;",
            where_clause
        );

        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), row_to_ticket)?;

        let mut tickets = Vec::new();
        for r in rows {
            tickets.push(r?);
        }
        Ok(tickets)
    }

    pub fn waiting_in(&self, ticket: &Ticket) -> Result<Vec<String>, RigError> {
        let after_str = match &ticket.after {
            Some(s) if !s.trim().is_empty() => s,
            _ => return Ok(Vec::new()),
        };

        let mut waiting = Vec::new();
        for part in after_str.split(|c: char| c == ',' || c.is_whitespace()) {
            let dep_id = part.trim();
            if dep_id.is_empty() {
                continue;
            }
            if waiting.iter().any(|w: &String| w == dep_id) {
                continue;
            }
            match self.get_ticket(dep_id) {
                Ok(dep_ticket) => {
                    if dep_ticket.status != TicketStatus::Done {
                        waiting.push(dep_id.to_string());
                    }
                }
                Err(RigError::TicketNotFound(_)) => {
                    waiting.push(dep_id.to_string());
                }
                Err(e) => return Err(e),
            }
        }

        Ok(waiting)
    }

    pub fn group_of(&self, ticket: &Ticket) -> Result<&'static str, RigError> {
        match ticket.status {
            TicketStatus::NeedsYou | TicketStatus::Blocked => Ok("Needs you"),
            TicketStatus::Working | TicketStatus::Reviewing => Ok("Working"),
            TicketStatus::Review => Ok("Ready"),
            TicketStatus::Done => Ok("Done"),
            TicketStatus::Open | TicketStatus::Draft => {
                let waiting = self.waiting_in(ticket)?;
                if waiting.is_empty() {
                    Ok("To do")
                } else {
                    Ok("Queued")
                }
            }
        }
    }
}
