use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::db::RigDatabase;
use crate::error::RigError;
use crate::harness::expand_argv;
use crate::layout::{LayoutAllocator, LayoutConfig, RectBounds, SlotRole, TabSlotLocation};
use crate::run::{CreateRunParams, RunMode, RunRole};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchRequest {
    pub ticket_id: String,
    pub role: SlotRole,
    pub harness: Option<String>,
    pub mode: Option<RunMode>,
    pub resume: bool,
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedLaunch {
    pub ticket_id: String,
    pub run_id: i64,
    pub role: SlotRole,
    pub harness: String,
    pub mode: RunMode,
    pub working_dir: PathBuf,
    pub argv: Vec<String>,
    pub slot_location: TabSlotLocation,
    pub bounds: RectBounds,
}

pub struct ProcessLauncher;

impl ProcessLauncher {
    pub fn prepare_launch(
        db: &mut RigDatabase,
        repo_root: &Path,
        request: &LaunchRequest,
        layout_config: &LayoutConfig,
    ) -> Result<PreparedLaunch, RigError> {
        let ticket = db.get_ticket(&request.ticket_id)?;

        let run_role = match request.role {
            SlotRole::Manager => RunRole::Reviewer,
            SlotRole::Board => RunRole::Reviewer,
            SlotRole::Worker(_) => {
                if ticket.review {
                    RunRole::Reviewer
                } else {
                    RunRole::Worker
                }
            }
        };

        let harness = if let Some(h) = &request.harness {
            h.clone()
        } else {
            let setting_key = match request.role {
                SlotRole::Manager => "manager.harness",
                SlotRole::Board => "board.harness",
                SlotRole::Worker(_) => {
                    if ticket.review {
                        "reviewer.harness"
                    } else {
                        "worker.harness"
                    }
                }
            };
            db.get_setting(setting_key)?
                .unwrap_or_else(|| "claude".to_string())
        };

        let mode = if let Some(m) = &request.mode {
            m.clone()
        } else {
            let mode_str = db.get_setting("mode")?.unwrap_or_else(|| "pane".to_string());
            mode_str.parse::<RunMode>().unwrap_or(RunMode::Pane)
        };

        let working_dir = if let Some(wt) = &ticket.worktree {
            if !wt.trim().is_empty() {
                let p = Path::new(wt);
                if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    repo_root.join(p)
                }
            } else {
                repo_root.to_path_buf()
            }
        } else {
            repo_root.to_path_buf()
        };

        let launch_kind = if request.resume {
            "resume"
        } else {
            "headless"
        };
        let sid = &ticket.id;
        let model = db.get_setting(&format!("{}.model", harness))?;
        let effort = db.get_setting(&format!("{}.effort", harness))?;
        let prompt_str = request.prompt.as_deref();

        let argv = expand_argv(
            &harness,
            launch_kind,
            model.as_deref(),
            effort.as_deref(),
            sid,
            prompt_str,
        )?;

        let slot_location = LayoutAllocator::allocate_slot(&request.role, layout_config);
        let bounds = LayoutAllocator::compute_rect_bounds(&slot_location);
        let cmd_str = argv.join(" ");

        let run_params = CreateRunParams {
            ticket: &ticket.id,
            role: run_role,
            harness: &harness,
            model: model.as_deref(),
            effort: effort.as_deref(),
            mode: mode.clone(),
            pane: None,
            sid: &ticket.id,
            pid: None,
            cmd: &cmd_str,
            status: "running",
            cwd: &working_dir.to_string_lossy(),
        };
        let run = db.create_run(&run_params)?;

        Ok(PreparedLaunch {
            ticket_id: ticket.id,
            run_id: run.id,
            role: request.role.clone(),
            harness,
            mode,
            working_dir,
            argv,
            slot_location,
            bounds,
        })
    }

    pub fn spawn_headless(
        prepared: &PreparedLaunch,
        log_path: Option<&Path>,
    ) -> Result<u32, RigError> {
        if prepared.argv.is_empty() {
            return Err(RigError::InvalidParameter("empty argv for spawn".to_string()));
        }

        let program = &prepared.argv[0];
        let args = &prepared.argv[1..];

        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(&prepared.working_dir)
            .stdin(Stdio::null());

        if let Some(p) = log_path {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = File::create(p)?;
            let file_err = File::create(p)?;
            cmd.stdout(Stdio::from(file)).stderr(Stdio::from(file_err));
        } else {
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let child = cmd.spawn().map_err(|e| {
            RigError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to spawn headless process '{}': {}", program, e),
            ))
        })?;

        Ok(child.id())
    }
}
