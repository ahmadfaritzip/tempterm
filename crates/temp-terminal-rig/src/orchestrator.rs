use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

use crate::db::RigDatabase;
use crate::error::RigError;
use crate::harness::{expand_argv, get_harness_spec, HarnessSpec};
use crate::skills::RigRole;
use crate::ticket::{Ticket, TicketFilter};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiRepoBoardItem {
    pub repo_name: String,
    pub repo_path: String,
    pub story_id: Option<String>,
    pub tickets: Vec<Ticket>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchPlan {
    pub repo_path: PathBuf,
    pub brief_path: PathBuf,
    pub role: String,
    pub harness: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub prompt: String,
    pub argv: Vec<String>,
}

pub struct HarnessRegistry;

impl HarnessRegistry {
    pub fn get_spec(harness: &str) -> Option<HarnessSpec> {
        get_harness_spec(harness)
    }

    pub fn resolve_argv(
        harness: &str,
        kind: &str,
        model: Option<&str>,
        effort: Option<&str>,
        sid: &str,
        prompt: Option<&str>,
    ) -> Result<Vec<String>, RigError> {
        expand_argv(harness, kind, model, effort, sid, prompt)
    }
}

pub struct OrchestratorEngine;

impl OrchestratorEngine {
    pub fn aggregate_board(
        hub_db: &RigDatabase,
        story_filter: Option<&str>,
        include_done: bool,
    ) -> Result<Vec<MultiRepoBoardItem>, RigError> {
        let repos = hub_db.list_repos()?;
        let mut board_items = Vec::new();

        for repo in repos {
            let repo_path = PathBuf::from(&repo.path);
            if !repo_path.exists() {
                continue;
            }

            let repo_db = match RigDatabase::open_repo(&repo_path) {
                Ok(db) => db,
                Err(_) => continue,
            };

            let filter = TicketFilter {
                manager: None,
                include_done,
                story: story_filter,
            };

            let tickets = repo_db.list_tickets(&filter)?;

            board_items.push(MultiRepoBoardItem {
                repo_name: repo.name,
                repo_path: repo.path,
                story_id: story_filter.map(|s| s.to_string()),
                tickets,
            });
        }

        Ok(board_items)
    }

    pub fn render_orchestrator_board(
        hub_db: &RigDatabase,
        story_filter: Option<&str>,
        include_done: bool,
    ) -> Result<String, RigError> {
        let items = Self::aggregate_board(hub_db, story_filter, include_done)?;
        let mut out = String::new();

        out.push_str("=== RIG ORCHESTRATOR MULTI-REPO BOARD ===\n");

        if let Some(story_id) = story_filter {
            if let Ok(Some(story)) = hub_db.get_story(story_id) {
                out.push_str(&format!("Story: [{}] {}\n", story.id, story.title));
            } else {
                out.push_str(&format!("Story: {}\n", story_id));
            }
        }

        out.push('\n');

        if items.is_empty() {
            out.push_str("(no registered repositories)\n");
            return Ok(out);
        }

        for item in &items {
            out.push_str(&format!("[Repo: {}] ({})\n", item.repo_name, item.repo_path));
            if item.tickets.is_empty() {
                out.push_str("  (no tickets)\n");
            } else {
                for t in &item.tickets {
                    let branch = t.branch.as_deref().unwrap_or("-");
                    out.push_str(&format!(
                        "  [{}] {} | branch: {} | status: {}\n",
                        t.id, t.title, branch, t.status
                    ));
                }
            }
        }

        Ok(out)
    }

    pub fn prepare_launch_manager(
        repo_path: &Path,
        brief_path: &Path,
        harness: Option<&str>,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Result<LaunchPlan, RigError> {
        let brief_content = fs::read_to_string(brief_path)?;

        let harness_name = if let Some(h) = harness {
            h.trim().to_string()
        } else if let Ok(db) = RigDatabase::open_repo(repo_path) {
            db.get_setting("manager.harness")?
                .unwrap_or_else(|| "claude".to_string())
        } else {
            "claude".to_string()
        };

        let base_template = RigRole::Manager.prompt_template();
        let prompt = format!(
            "{base_template}\n\n## Project Brief\n{brief_content}\n\n## Cross-Repo Orchestration\nReport cross-repo updates, questions, and status to the orchestrator using:\n`rig say orchestrator \"<message>\"`\n"
        );

        let argv = HarnessRegistry::resolve_argv(
            &harness_name,
            "headless",
            model,
            effort,
            "manager",
            Some(&prompt),
        )?;

        Ok(LaunchPlan {
            repo_path: repo_path.to_path_buf(),
            brief_path: brief_path.to_path_buf(),
            role: "manager".to_string(),
            harness: harness_name,
            model: model.map(|s| s.to_string()),
            effort: effort.map(|s| s.to_string()),
            prompt,
            argv,
        })
    }
}
