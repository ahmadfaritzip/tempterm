use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use crate::{RigDatabase, RigError, Ticket};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffStat {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriageAction {
    ApproveLocal,
    AssignReviewer {
        harness: String,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriageResult {
    pub ticket_id: String,
    pub branch: String,
    pub base: String,
    pub diff_stat: DiffStat,
    pub action: TriageAction,
}

pub struct TriageEngine;

impl TriageEngine {
    pub fn inspect_diff_stat(repo_root: &Path, ticket: &Ticket) -> Result<DiffStat, RigError> {
        let branch = ticket.branch.as_deref().ok_or_else(|| {
            RigError::InvalidParameter(format!("Ticket {} has no branch set", ticket.id))
        })?;
        let base = ticket.base.as_deref().unwrap_or("main");

        // 1. Find merge-base between base and branch
        let mut mb_cmd = Command::new("git");
        mb_cmd.args(["merge-base", base, branch]).current_dir(repo_root);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            mb_cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mb_output = mb_cmd.output().map_err(|e| RigError::Io(e))?;
        if !mb_output.status.success() {
            let stderr = String::from_utf8_lossy(&mb_output.stderr);
            return Err(RigError::InvalidParameter(format!(
                "Failed to find merge-base between {} and {}: {}",
                base, branch, stderr.trim()
            )));
        }
        let merge_base = String::from_utf8_lossy(&mb_output.stdout).trim().to_string();
        if merge_base.is_empty() {
            return Err(RigError::InvalidParameter(format!(
                "Empty merge-base between {} and {}",
                base, branch
            )));
        }

        // 2. Run git diff --shortstat <merge_base> <branch>
        let mut diff_cmd = Command::new("git");
        diff_cmd.args(["diff", "--shortstat", &merge_base, branch]).current_dir(repo_root);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            diff_cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let diff_output = diff_cmd.output().map_err(|e| RigError::Io(e))?;
        if !diff_output.status.success() {
            let stderr = String::from_utf8_lossy(&diff_output.stderr);
            return Err(RigError::InvalidParameter(format!(
                "Failed to run git diff --shortstat: {}",
                stderr.trim()
            )));
        }

        let stat_str = String::from_utf8_lossy(&diff_output.stdout).trim().to_string();
        Self::parse_shortstat(&stat_str)
    }

    fn parse_shortstat(stat_str: &str) -> Result<DiffStat, RigError> {
        if stat_str.is_empty() {
            return Ok(DiffStat {
                files_changed: 0,
                insertions: 0,
                deletions: 0,
            });
        }

        // Example output: " 3 files changed, 10 insertions(+), 5 deletions(-)"
        // or " 1 file changed, 1 insertion(+)"
        let mut files_changed = 0;
        let mut insertions = 0;
        let mut deletions = 0;

        for part in stat_str.split(',') {
            let part = part.trim();
            if part.contains("file") {
                if let Some(num_str) = part.split_whitespace().next() {
                    files_changed = num_str.parse().unwrap_or(0);
                }
            } else if part.contains("insertion") {
                if let Some(num_str) = part.split_whitespace().next() {
                    insertions = num_str.parse().unwrap_or(0);
                }
            } else if part.contains("deletion") {
                if let Some(num_str) = part.split_whitespace().next() {
                    deletions = num_str.parse().unwrap_or(0);
                }
            }
        }

        Ok(DiffStat {
            files_changed,
            insertions,
            deletions,
        })
    }

    pub fn evaluate_triage(
        repo_root: &Path,
        db: &mut RigDatabase,
        ticket_id: &str,
        line_threshold: usize,
        reviewer_harness: Option<&str>,
    ) -> Result<TriageResult, RigError> {
        let ticket = db.get_ticket(ticket_id)?;
        let branch = ticket.branch.clone().unwrap_or_else(|| "HEAD".to_string());
        let base = ticket.base.clone().unwrap_or_else(|| "main".to_string());

        let diff_stat = Self::inspect_diff_stat(repo_root, &ticket)?;
        let total_lines = diff_stat.insertions + diff_stat.deletions;

        let action = if total_lines > line_threshold || diff_stat.files_changed > 3 {
            let harness = if let Some(h) = reviewer_harness {
                h.to_string()
            } else {
                db.get_setting("reviewer.harness")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "codex".to_string())
            };
            let reason = format!(
                "Diff statistics ({} files changed, {} insertions, {} deletions) exceed threshold ({} lines / 3 files).",
                diff_stat.files_changed, diff_stat.insertions, diff_stat.deletions, line_threshold
            );
            TriageAction::AssignReviewer { harness, reason }
        } else {
            TriageAction::ApproveLocal
        };

        Ok(TriageResult {
            ticket_id: ticket.id,
            branch,
            base,
            diff_stat,
            action,
        })
    }
}
