use std::fmt;
use std::str::FromStr;
use serde::{Deserialize, Serialize};

use crate::db::RigDatabase;
use crate::error::RigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunRole {
    Worker,
    Reviewer,
}

impl RunRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Worker => "worker",
            Self::Reviewer => "reviewer",
        }
    }
}

impl fmt::Display for RunRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for RunRole {
    type Err = RigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "worker" => Ok(Self::Worker),
            "reviewer" => Ok(Self::Reviewer),
            other => Err(RigError::InvalidParameter(format!(
                "unknown run role: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    Pane,
    Tab,
    Headless,
    Space,
    Overflow(u32),
}

impl fmt::Display for RunMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pane => write!(f, "pane"),
            Self::Tab => write!(f, "tab"),
            Self::Headless => write!(f, "headless"),
            Self::Space => write!(f, "space"),
            Self::Overflow(n) => write!(f, "overflow:{}", n),
        }
    }
}

impl FromStr for RunMode {
    type Err = RigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s_trim = s.trim().to_ascii_lowercase();
        match s_trim.as_str() {
            "pane" => Ok(Self::Pane),
            "tab" => Ok(Self::Tab),
            "headless" => Ok(Self::Headless),
            "space" => Ok(Self::Space),
            _ => {
                let stripped = s_trim
                    .strip_prefix("overflow:")
                    .or_else(|| s_trim.strip_prefix("overflow_"))
                    .or_else(|| s_trim.strip_prefix("overflow-"))
                    .or_else(|| s_trim.strip_prefix("overflow/"))
                    .or_else(|| s_trim.strip_prefix("overflow "));

                if let Some(rest) = stripped {
                    if let Ok(n) = rest.trim().parse::<u32>() {
                        return Ok(Self::Overflow(n));
                    }
                }

                if let Ok(mode) = serde_json::from_str::<RunMode>(s) {
                    return Ok(mode);
                }

                Err(RigError::InvalidParameter(format!(
                    "unknown run mode: {s}"
                )))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: i64,
    pub ticket: String,
    pub role: RunRole,
    pub harness: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub mode: RunMode,
    pub pane: Option<String>,
    pub sid: String,
    pub pid: Option<u32>,
    pub cmd: String,
    pub status: String,
    pub cwd: String,
    pub started: String,
}

pub struct CreateRunParams<'a> {
    pub ticket: &'a str,
    pub role: RunRole,
    pub harness: &'a str,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub mode: RunMode,
    pub pane: Option<&'a str>,
    pub sid: &'a str,
    pub pid: Option<u32>,
    pub cmd: &'a str,
    pub status: &'a str,
    pub cwd: &'a str,
}

fn row_to_run(row: &rusqlite::Row) -> Result<Run, rusqlite::Error> {
    let role_str: String = row.get(2)?;
    let role = role_str.parse::<RunRole>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let mode_str: String = row.get(6)?;
    let mode = mode_str.parse::<RunMode>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let pid_i64: Option<i64> = row.get(9)?;
    let pid = pid_i64.map(|p| p as u32);

    Ok(Run {
        id: row.get(0)?,
        ticket: row.get(1)?,
        role,
        harness: row.get(3)?,
        model: row.get(4)?,
        effort: row.get(5)?,
        mode,
        pane: row.get(7)?,
        sid: row.get(8)?,
        pid,
        cmd: row.get(10)?,
        status: row.get(11)?,
        cwd: row.get(12)?,
        started: row.get(13)?,
    })
}

impl RigDatabase {
    pub fn create_run(&self, params: &CreateRunParams) -> Result<Run, RigError> {
        let started = chrono::Utc::now().to_rfc3339();
        let mode_str = params.mode.to_string();

        self.conn.execute(
            "INSERT INTO runs (ticket, role, harness, model, effort, mode, pane, sid, pid, cmd, status, cwd, started)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13);",
            rusqlite::params![
                params.ticket,
                params.role.as_str(),
                params.harness,
                params.model,
                params.effort,
                mode_str,
                params.pane,
                params.sid,
                params.pid.map(|p| p as i64),
                params.cmd,
                params.status,
                params.cwd,
                started,
            ],
        )?;

        let id = self.conn.last_insert_rowid();

        Ok(Run {
            id,
            ticket: params.ticket.to_string(),
            role: params.role,
            harness: params.harness.to_string(),
            model: params.model.map(|s| s.to_string()),
            effort: params.effort.map(|s| s.to_string()),
            mode: params.mode.clone(),
            pane: params.pane.map(|s| s.to_string()),
            sid: params.sid.to_string(),
            pid: params.pid,
            cmd: params.cmd.to_string(),
            status: params.status.to_string(),
            cwd: params.cwd.to_string(),
            started,
        })
    }

    pub fn get_run(&self, id: i64) -> Result<Run, RigError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, ticket, role, harness, model, effort, mode, pane, sid, pid, cmd, status, cwd, started
             FROM runs WHERE id = ?1;",
        )?;
        match stmt.query_row([id], row_to_run) {
            Ok(run) => Ok(run),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Err(RigError::RunNotFound(format!("{}", id)))
            }
            Err(e) => Err(RigError::Database(e)),
        }
    }

    pub fn update_run_status(&self, id: i64, status: &str, pid: Option<u32>) -> Result<(), RigError> {
        let count = match pid {
            Some(p) => self.conn.execute(
                "UPDATE runs SET status = ?1, pid = ?2 WHERE id = ?3;",
                rusqlite::params![status, p as i64, id],
            )?,
            None => self.conn.execute(
                "UPDATE runs SET status = ?1 WHERE id = ?2;",
                rusqlite::params![status, id],
            )?,
        };
        if count == 0 {
            return Err(RigError::RunNotFound(format!("{}", id)));
        }
        Ok(())
    }

    pub fn list_runs_for_ticket(&self, ticket_id: &str) -> Result<Vec<Run>, RigError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, ticket, role, harness, model, effort, mode, pane, sid, pid, cmd, status, cwd, started
             FROM runs WHERE ticket = ?1 ORDER BY id ASC;",
        )?;
        let rows = stmt.query_map([ticket_id], row_to_run)?;
        let mut runs = Vec::new();
        for row in rows {
            runs.push(row?);
        }
        Ok(runs)
    }

    pub fn get_active_run_for_ticket(&self, ticket_id: &str) -> Result<Option<Run>, RigError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, ticket, role, harness, model, effort, mode, pane, sid, pid, cmd, status, cwd, started
             FROM runs
             WHERE ticket = ?1 AND LOWER(status) NOT IN ('done', 'failed', 'stopped', 'killed', 'exited', 'completed', 'canceled', 'cancelled')
             ORDER BY id DESC
             LIMIT 1;",
        )?;
        match stmt.query_row([ticket_id], row_to_run) {
            Ok(run) => Ok(Some(run)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(RigError::Database(e)),
        }
    }
}
