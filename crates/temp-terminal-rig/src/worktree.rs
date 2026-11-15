use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::RigError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeInfo {
    pub ticket_id: String,
    pub branch: String,
    pub path: PathBuf,
    pub is_clean: bool,
}

pub struct WorktreeManager;

fn run_git_cmd(dir: &Path, args: &[&str]) -> Result<std::process::Output, RigError> {
    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(dir);
    cmd.args(args);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd.output()?;
    Ok(output)
}

impl WorktreeManager {
    /// Creates an isolated worktree under `.rig/wt/<ticket_id>` on branch `rig/<ticket_id>`.
    pub fn create_worktree(
        repo_root: &Path,
        ticket_id: &str,
        base_branch: &str,
    ) -> Result<PathBuf, RigError> {
        let wt_dir = repo_root.join(".rig").join("wt").join(ticket_id);
        let branch = format!("rig/{}", ticket_id);

        if let Some(parent) = wt_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let wt_path_str = wt_dir.to_string_lossy().to_string();
        let out = run_git_cmd(
            repo_root,
            &["worktree", "add", &wt_path_str, "-b", &branch, base_branch],
        )?;

        if !out.status.success() {
            let err_msg = String::from_utf8_lossy(&out.stderr);
            return Err(RigError::GitError(format!(
                "git worktree add failed: {}",
                err_msg.trim()
            )));
        }

        Ok(wt_dir)
    }

    /// Verifies if changes on `ticket_branch` have been landed into `base_branch`.
    /// 1. Finds `merge-base <base> <branch>`.
    /// 2. Finds changed files via `git diff --name-only <merge-base> <branch>`.
    /// 3. If no files changed, returns true.
    /// 4. Checks if `git diff --quiet <branch> <base> -- <files>` returns exit status 0.
    pub fn has_landed(repo_root: &Path, base_branch: &str, ticket_branch: &str) -> bool {
        let mb_out = match run_git_cmd(repo_root, &["merge-base", base_branch, ticket_branch]) {
            Ok(o) if o.status.success() => o,
            _ => return false,
        };

        let merge_base = String::from_utf8_lossy(&mb_out.stdout).trim().to_string();
        if merge_base.is_empty() {
            return false;
        }

        let diff_out = match run_git_cmd(
            repo_root,
            &["diff", "--name-only", &merge_base, ticket_branch],
        ) {
            Ok(o) if o.status.success() => o,
            _ => return false,
        };

        let diff_str = String::from_utf8_lossy(&diff_out.stdout);
        let files: Vec<&str> = diff_str
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        if files.is_empty() {
            return true;
        }

        let mut args = vec!["diff", "--quiet", ticket_branch, base_branch, "--"];
        args.extend(files);

        match run_git_cmd(repo_root, &args) {
            Ok(out) => out.status.code() == Some(0),
            Err(_) => false,
        }
    }

    /// Squash-merges `rig/<ticket_id>` into `base_branch` and verifies landing.
    pub fn land_worktree(
        repo_root: &Path,
        ticket_id: &str,
        base_branch: &str,
        commit_message: Option<&str>,
    ) -> Result<String, RigError> {
        let branch = format!("rig/{}", ticket_id);

        let status_out = run_git_cmd(repo_root, &["status", "--porcelain"])?;
        if !status_out.status.success() {
            let err = String::from_utf8_lossy(&status_out.stderr);
            return Err(RigError::GitError(format!(
                "git status failed: {}",
                err.trim()
            )));
        }

        let status_str = String::from_utf8_lossy(&status_out.stdout);
        let has_dirty_files = status_str.lines().any(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return false;
            }
            let path_part = if trimmed.len() > 3 {
                trimmed[3..].trim()
            } else {
                trimmed
            };
            !path_part.starts_with(".rig") && !path_part.starts_with(".rig/")
        });

        if has_dirty_files {
            return Err(RigError::GitError(
                "working directory has uncommitted changes".to_string(),
            ));
        }

        let checkout_out = run_git_cmd(repo_root, &["checkout", base_branch])?;
        if !checkout_out.status.success() {
            let err = String::from_utf8_lossy(&checkout_out.stderr);
            return Err(RigError::GitError(format!(
                "git checkout {} failed: {}",
                base_branch,
                err.trim()
            )));
        }

        let merge_out = run_git_cmd(repo_root, &["merge", "--squash", &branch])?;
        if !merge_out.status.success() {
            let _ = run_git_cmd(repo_root, &["reset", "--hard", "HEAD"]);
            let err = String::from_utf8_lossy(&merge_out.stderr);
            return Err(RigError::GitError(format!(
                "git merge --squash {} failed: {}",
                branch,
                err.trim()
            )));
        }

        // Commit if there are staged changes
        let diff_cached = run_git_cmd(repo_root, &["diff", "--cached", "--quiet"])?;
        if diff_cached.status.code() != Some(0) {
            let default_msg = format!("land({}): merge worktree", ticket_id);
            let msg = commit_message.unwrap_or(&default_msg);
            let commit_out = run_git_cmd(repo_root, &["commit", "-m", msg])?;
            if !commit_out.status.success() {
                let err = String::from_utf8_lossy(&commit_out.stderr);
                return Err(RigError::GitError(format!(
                    "git commit failed: {}",
                    err.trim()
                )));
            }
        }

        if !Self::has_landed(repo_root, base_branch, &branch) {
            return Err(RigError::GitError(format!(
                "landing verification failed for ticket {} into {}",
                ticket_id, base_branch
            )));
        }

        let head_out = run_git_cmd(repo_root, &["rev-parse", "HEAD"])?;
        if !head_out.status.success() {
            return Err(RigError::GitError(
                "failed to get HEAD commit hash".to_string(),
            ));
        }

        let hash = String::from_utf8_lossy(&head_out.stdout).trim().to_string();
        Ok(hash)
    }

    /// Safely cleans up the worktree and deletes the branch.
    /// If `force` is false and changes have not landed, returns `RigError::UnmergedWorktree`.
    /// On Windows, directory removal retries up to 10 times with 500ms sleep on error.
    pub fn clean_worktree(
        repo_root: &Path,
        ticket_id: &str,
        base_branch: &str,
        force: bool,
    ) -> Result<(), RigError> {
        let branch = format!("rig/{}", ticket_id);
        let path = repo_root.join(".rig").join("wt").join(ticket_id);

        if !force && !Self::has_landed(repo_root, base_branch, &branch) {
            return Err(RigError::UnmergedWorktree(format!(
                "worktree for ticket {} has unmerged changes into {}",
                ticket_id, base_branch
            )));
        }

        let path_str = path.to_string_lossy().to_string();
        let _ = run_git_cmd(repo_root, &["worktree", "remove", "--force", &path_str]);

        // Windows directory locking fallback: retry up to 10 times with 500ms delay
        if path.exists() {
            let mut removed = false;
            let mut last_err = None;
            for _ in 0..10 {
                match std::fs::remove_dir_all(&path) {
                    Ok(_) => {
                        removed = true;
                        break;
                    }
                    Err(e) => {
                        last_err = Some(e);
                        sleep(Duration::from_millis(500));
                    }
                }
            }

            if !removed && path.exists() {
                if let Some(err) = last_err {
                    return Err(RigError::Io(err));
                }
            }
        }

        let _ = run_git_cmd(repo_root, &["worktree", "prune"]);
        let _ = run_git_cmd(repo_root, &["branch", "-D", &branch]);

        Ok(())
    }

    /// Lists active rig worktrees.
    pub fn list_worktrees(repo_root: &Path) -> Result<Vec<WorktreeInfo>, RigError> {
        let out = run_git_cmd(repo_root, &["worktree", "list", "--porcelain"])?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(RigError::GitError(format!(
                "git worktree list failed: {}",
                err.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut worktrees = Vec::new();

        let mut cur_path: Option<PathBuf> = None;
        let mut cur_branch = String::new();

        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                if let Some(path) = cur_path.take() {
                    if let Some(info) = parse_rig_worktree(path, &cur_branch) {
                        worktrees.push(info);
                    }
                    cur_branch.clear();
                }
                continue;
            }

            if let Some(p) = line.strip_prefix("worktree ") {
                cur_path = Some(PathBuf::from(p));
            } else if let Some(b) = line.strip_prefix("branch ") {
                cur_branch = b.to_string();
            }
        }

        if let Some(path) = cur_path {
            if let Some(info) = parse_rig_worktree(path, &cur_branch) {
                worktrees.push(info);
            }
        }

        Ok(worktrees)
    }
}

fn parse_rig_worktree(path: PathBuf, branch_ref: &str) -> Option<WorktreeInfo> {
    let short_branch = branch_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(branch_ref);

    let ticket_id = if let Some(id) = short_branch.strip_prefix("rig/") {
        id.to_string()
    } else {
        // Fallback: check if path has .rig/wt/<ticket_id>
        let path_str = path.to_string_lossy().replace('\\', "/");
        if let Some(idx) = path_str.rfind("/.rig/wt/") {
            let rest = &path_str[idx + "/.rig/wt/".len()..];
            rest.split('/').next().unwrap_or("").to_string()
        } else {
            return None;
        }
    };

    if ticket_id.is_empty() {
        return None;
    }

    let status_out = run_git_cmd(&path, &["status", "--porcelain"]);
    let is_clean = match status_out {
        Ok(out) => out.status.success() && out.stdout.is_empty(),
        Err(_) => false,
    };

    let branch = if short_branch.is_empty() {
        format!("rig/{}", ticket_id)
    } else {
        short_branch.to_string()
    };

    Some(WorktreeInfo {
        ticket_id,
        branch,
        path,
        is_clean,
    })
}
