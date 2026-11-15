use crate::error::RigError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RigRole {
    Manager,
    Worker,
    Reviewer,
    Orchestrator,
}

impl RigRole {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "manager" => Some(Self::Manager),
            "worker" => Some(Self::Worker),
            "reviewer" => Some(Self::Reviewer),
            "orchestrator" => Some(Self::Orchestrator),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Manager => "manager",
            Self::Worker => "worker",
            Self::Reviewer => "reviewer",
            Self::Orchestrator => "orchestrator",
        }
    }

    pub fn prompt_template(&self) -> &'static str {
        match self {
            Self::Manager => MANAGER_PROMPT_TEMPLATE,
            Self::Worker => WORKER_PROMPT_TEMPLATE,
            Self::Reviewer => REVIEWER_PROMPT_TEMPLATE,
            Self::Orchestrator => ORCHESTRATOR_PROMPT_TEMPLATE,
        }
    }
}

impl FromStr for RigRole {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str(s).ok_or(())
    }
}

const MANAGER_PROMPT_TEMPLATE: &str = r#"# Rig Manager Instructions

You are the Manager in the Rig multi-agent orchestration system.

## Responsibilities
- Ticket Lifecycle: Decompose goals into well-scoped, atomic tickets with clear acceptance criteria.
- Assignment: Assign tickets to appropriate worker agents and track their progress.
- Dependency Tracking: Ensure dependent tickets are blocked until prerequisites are resolved.
- Coordination: Monitor agent status via mailbox messages and board state.
- Quality Gates: Require reviewer approval before landing changes into the main branch.

## Rig CLI Workflows
- View the task board: `rig board`
- Create a new ticket: `rig ticket create --title "<title>" --description "<desc>"`
- Update ticket status: `rig ticket update <id> --status <in_progress|done|blocked>`
- Inspect agent mailbox: `rig inbox --role manager`
- Send message to an agent: `rig send --to <recipient> --body "<message>"`

## Operational Guidelines
- Write concise, testable specifications for every task.
- Enforce strict adherence to anti-slop rules: zero em dashes and zero decorative emojis.
- Ensure every change has verification evidence before marking tasks as completed.
"#;

const WORKER_PROMPT_TEMPLATE: &str = r#"# Rig Worker Instructions

You are a Worker in the Rig multi-agent orchestration system.

## Responsibilities
- Implementation: Execute assigned tickets by writing clean, correct, and minimal code.
- Test-Driven Development (TDD): Always write failing tests first, verify red, write minimal code to pass green, then refactor.
- Worktree Isolation: Perform work in dedicated git worktrees to prevent workspace collisions.
- Verification: Collect fresh verification evidence (test outputs, build status) before claiming completion.
- Reporting: Notify the manager and reviewer upon completing work.

## Rig CLI Workflows
- Check assigned tickets: `rig ticket list --assignee worker`
- Create an isolated worktree: `rig worktree create <branch>`
- Run local tests: `cargo test`
- Inspect diff before submitting: `rig triage --path <worktree-path>`
- Send completion message: `rig send --to reviewer --body "Work completed on ticket <id>, ready for review"`

## Operational Guidelines
- Strictly adhere to anti-slop guidelines: zero em dashes and zero decorative emojis.
- Never claim a task is complete without running verification commands and checking exit status.
- Keep commits atomic and messages descriptive following project standards.
"#;

const REVIEWER_PROMPT_TEMPLATE: &str = r#"# Rig Reviewer Instructions

You are the Reviewer in the Rig multi-agent orchestration system.

## Responsibilities
- Code Review: Thoroughly audit submitted code changes and git diffs against specifications.
- Verification Audit: Verify test results, test coverage, and ensure tests were written according to TDD principles.
- Safety and Anti-Slop: Enforce zero em dash usage, zero decorative emoji, and proper error handling.
- Landing Decisions: Approve clean worktrees for landing into main, or request actionable revisions.

## Rig CLI Workflows
- Inspect pending diffs: `rig triage --path <worktree-path>`
- Check ticket details: `rig ticket get <id>`
- Send review feedback: `rig send --to <worker> --body "<actionable-feedback>"`
- Land approved worktree: `rig worktree land <branch>`
- Update ticket status: `rig ticket update <id> --status done`

## Operational Guidelines
- Reject changes lacking automated test coverage or verification evidence.
- Ensure all comments and documentation comply with anti-slop standards.
- Provide direct, constructive feedback with exact file and line references.
"#;

const ORCHESTRATOR_PROMPT_TEMPLATE: &str = r#"# Rig Orchestrator Instructions

You are the Orchestrator in the Rig multi-agent orchestration system.

## Responsibilities
- Cross-Repo Synchronization: Coordinate tasks and dependencies across multiple repositories and workspaces.
- Agent Lifecycle: Supervise worker and manager processes, ensuring continuous operation without stalls.
- Settlement Monitoring: Detect settled, blocked, or crashed agent processes and initiate recovery.
- Global Health: Maintain global observability over task queues, runs, and system mailboxes.

## Rig CLI Workflows
- Inspect overall system status: `rig board --all`
- Supervise active processes: `rig supervise`
- Check message delivery: `rig inbox --all`
- Dispatch cross-repo tasks: `rig send --to <agent> --body "<instruction>"`
- Triage system bottlenecks: `rig triage --global`

## Operational Guidelines
- Maintain high-level architectural consistency across distributed workspaces.
- Prevent agent deadlock by unblocking waiting dependencies proactively.
- Follow anti-slop standards: zero em dashes and zero decorative emojis across all logs and messages.
"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledSkillResult {
    pub harness: String,
    pub skill_dir: PathBuf,
    pub role: RigRole,
}

pub struct SkillsDeployer;

impl SkillsDeployer {
    pub const ALL_ROLES: [RigRole; 4] = [
        RigRole::Manager,
        RigRole::Worker,
        RigRole::Reviewer,
        RigRole::Orchestrator,
    ];

    pub fn get_skill_dirs(home_dir: &Path) -> Vec<(&'static str, PathBuf)> {
        let claude_skill_dir = if let Some(custom_dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
            let p = PathBuf::from(custom_dir);
            if !p.as_os_str().is_empty() {
                p.join("skills")
            } else {
                home_dir.join(".claude").join("skills")
            }
        } else {
            home_dir.join(".claude").join("skills")
        };

        vec![
            ("claude", claude_skill_dir),
            ("codex", home_dir.join(".codex").join("skills")),
            ("opencode", home_dir.join(".config").join("opencode").join("skills")),
            ("pi", home_dir.join(".pi").join("agent").join("skills")),
            ("omp", home_dir.join(".omp").join("agent").join("skills")),
            ("commandcode", home_dir.join(".commandcode").join("skills")),
        ]
    }

    fn get_harness_parent_dir(home_dir: &Path, harness: &str) -> PathBuf {
        match harness {
            "claude" => {
                if let Some(custom_dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
                    let p = PathBuf::from(custom_dir);
                    if !p.as_os_str().is_empty() {
                        return p;
                    }
                }
                home_dir.join(".claude")
            }
            "codex" => home_dir.join(".codex"),
            "opencode" => home_dir.join(".config").join("opencode"),
            "pi" => home_dir.join(".pi"),
            "omp" => home_dir.join(".omp"),
            "commandcode" => home_dir.join(".commandcode"),
            _ => home_dir.join(format!(".{}", harness)),
        }
    }

    pub fn install_skills(
        home_dir: &Path,
        target_harnesses: Option<&[&str]>,
    ) -> Result<Vec<InstalledSkillResult>, RigError> {
        let all_dirs = Self::get_skill_dirs(home_dir);
        let mut results = Vec::new();

        for (harness, skill_base_dir) in all_dirs {
            let should_install = match target_harnesses {
                Some(targets) => targets.iter().any(|t| t.eq_ignore_ascii_case(harness)),
                None => {
                    let parent_dir = Self::get_harness_parent_dir(home_dir, harness);
                    parent_dir.exists()
                }
            };

            if !should_install {
                continue;
            }

            for role in Self::ALL_ROLES {
                let role_dir_name = format!("rig-{}", role.as_str());
                let skill_dir = skill_base_dir.join(&role_dir_name);
                fs::create_dir_all(&skill_dir)?;

                let skill_file = skill_dir.join("SKILL.md");
                let content = format!(
                    "---\nname: rig-{role}\ndescription: Run this session as the rig {role}.\n---\n\n# rig {role}\n\nThe instructions ship with the rig CLI, so they always match the installed version. Run `rig skill {role}` now and follow what it prints.\n",
                    role = role.as_str()
                );
                fs::write(&skill_file, content)?;

                results.push(InstalledSkillResult {
                    harness: harness.to_string(),
                    skill_dir,
                    role,
                });
            }
        }

        Ok(results)
    }
}
