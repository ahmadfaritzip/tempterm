use serde::{Serialize, Deserialize};
use std::process::Command;
use crate::error::RigError;
use crate::db::RigDatabase;
use crate::watcher::StreamWatcher;
use crate::run::{Run, RunRole, RunMode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessLiveness {
    Running(u32),
    Exited(i32),
    Terminated,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervisedProcessInfo {
    pub run_id: u32,
    pub ticket_id: String,
    pub pid: u32,
    pub role: RunRole,
    pub harness: String,
    pub liveness: ProcessLiveness,
    pub started_at: String,
}

pub struct ProcessSupervisor;

impl ProcessSupervisor {
    pub fn check_pid_liveness(pid: u32) -> ProcessLiveness {
        // Use tasklist or Windows task query. Sometimes tasklist output takes a moment or needs different query flags.
        let output = match Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
        {
            Ok(out) => out,
            Err(_) => return ProcessLiveness::NotFound,
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains("INFO: No tasks") || stdout.trim().is_empty() {
            return ProcessLiveness::NotFound;
        }

        if stdout.contains(&pid.to_string()) {
            ProcessLiveness::Running(pid)
        } else {
            ProcessLiveness::NotFound
        }
    }

    pub fn terminate_pid(pid: u32, force: bool) -> Result<(), RigError> {
        let mut cmd = Command::new("taskkill");
        cmd.arg("/PID").arg(pid.to_string());
        if force {
            cmd.arg("/F");
        }
        let output = cmd.output().map_err(|e| RigError::Io(e))?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not found") || stderr.contains("tidak ditemukan") || stderr.contains("tidak ada") {
                Ok(())
            } else {
                Err(RigError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("taskkill failed: {}", stderr),
                )))
            }
        }
    }

    pub fn poll_active_runs(db: &mut RigDatabase) -> Result<Vec<SupervisedProcessInfo>, RigError> {
        let runs: Vec<Run> = {
            let mut stmt = db.connection_mut().prepare(
                "SELECT id, ticket, role, harness, model, effort, mode, pane, sid, pid, cmd, status, cwd, started
                 FROM runs;"
            )?;

            let run_iter = stmt.query_map([], |row| {
                let role_str: String = row.get(2)?;
                let role = role_str.parse::<RunRole>().unwrap_or(RunRole::Worker);
                let mode_str: String = row.get(6)?;
                let mode = mode_str.parse().unwrap_or(RunMode::Pane);
                let pid_i64: Option<i64> = row.get(9)?;
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
                    pid: pid_i64.map(|p| p as u32),
                    cmd: row.get(10)?,
                    status: row.get(11)?,
                    cwd: row.get(12)?,
                    started: row.get(13)?,
                })
            })?;

            let mut collected = Vec::new();
            for r in run_iter {
                collected.push(r.map_err(|e| RigError::Database(e))?);
            }
            collected
        };

        let mut results = Vec::new();

        for run in runs {
            let pid = match run.pid {
                Some(p) => p,
                None => continue,
            };

            let liveness = Self::check_pid_liveness(pid);
            if !matches!(liveness, ProcessLiveness::Running(_)) {
                db.update_run_status(run.id, "exited", run.pid)?;
            }

            results.push(SupervisedProcessInfo {
                run_id: run.id as u32,
                ticket_id: run.ticket,
                pid,
                role: run.role,
                harness: run.harness,
                liveness,
                started_at: run.started,
            });
        }

        Ok(results)
    }

    pub fn handle_unprompted_stop(
        db: &mut RigDatabase,
        run_id: u32,
        ticket_id: &str,
        role: &str,
        tail_lines: &[String],
    ) -> Result<(), RigError> {
        let notice = StreamWatcher::format_unprompted_stop_notice(run_id, role, tail_lines);
        db.send_message(Some(ticket_id), role, "manager", &notice, None)?;
        Ok(())
    }
}
