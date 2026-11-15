//! Git worktree lifecycle management for isolated agent sessions.
//! Ported with exact parity from `tempterm/internal/worktree`.

use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::git::{git_cmd, git_out, git_test, GitError, Repo};

/// Standard folder where repository worktrees are located (`.worktrees`).
pub const WORKTREE_FOLDER: &str = ".worktrees";

#[derive(Debug, Error)]
pub enum WorktreeError {
    #[error("branch name invalid")]
    InvalidBranch,
    #[error("choose the branch to start from")]
    ChooseBaseBranch,
    #[error("the repository has no commits yet")]
    NoCommits,
    #[error("there is no branch {0}")]
    NoBranch(String),
    #[error("conflicts with {base} in {files:?}")]
    Conflict { base: String, files: Vec<String> },
    #[error("{0}")]
    Git(#[from] GitError),
    #[error("{0}")]
    Other(String),
}

/// Metadata describing a session's worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub main: PathBuf,
    pub dir: PathBuf,
    pub branch: String,
    pub base: String,
    pub rel: String,
    pub reused: bool,
}

impl WorktreeInfo {
    /// Creates a WorktreeInfo referencing an existing directory.
    pub fn of<P: Into<PathBuf>>(dir: P, branch: String, base: String) -> Self {
        let dir = dir.into();
        let main = dir
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(&dir)
            .to_path_buf();
        Self {
            main,
            dir,
            branch,
            base,
            rel: String::new(),
            reused: false,
        }
    }

    /// Starting folder for a session within this worktree.
    pub fn start(&self) -> PathBuf {
        if !self.rel.is_empty() {
            let candidate = self.dir.join(&self.rel);
            if candidate.is_dir() {
                return candidate;
            }
        }
        self.dir.clone()
    }
}

/// Worktree entry from `git worktree list`.
#[derive(Debug, Clone)]
struct WorktreeEntry {
    path: PathBuf,
    branch: String,
    bare: bool,
}

fn list_worktrees<P: AsRef<Path>>(dir: P) -> Result<Vec<WorktreeEntry>, WorktreeError> {
    let out = git_out(dir.as_ref(), &["worktree", "list", "--porcelain", "-z"])
        .map_err(|e| WorktreeError::Other(e.to_string()))?;

    let mut entries = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch = String::new();
    let mut current_bare = false;

    for field in out.split('\0') {
        if field.is_empty() {
            if let Some(path) = current_path.take() {
                entries.push(WorktreeEntry {
                    path,
                    branch: current_branch.clone(),
                    bare: current_bare,
                });
                current_branch.clear();
                current_bare = false;
            }
            continue;
        }

        if let Some(p) = field.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(p));
        } else if let Some(b) = field.strip_prefix("branch refs/heads/") {
            current_branch = b.to_string();
        } else if field == "bare" {
            current_bare = true;
        }
    }

    if let Some(path) = current_path {
        entries.push(WorktreeEntry {
            path,
            branch: current_branch,
            bare: current_bare,
        });
    }

    Ok(entries)
}

/// Checks if a branch name is valid according to git rules.
pub fn is_valid_branch(name: &str) -> bool {
    if name.is_empty() || name.starts_with('-') || name.ends_with(".lock") {
        return false;
    }
    git_test(".", &["check-ref-format", "--branch", name])
}

/// Creates or finds an isolated git worktree for `branch` under `.worktrees`.
pub fn add_worktree<P: AsRef<Path>>(
    project_dir: P,
    branch: &str,
    base: &str,
    copy_env: bool,
) -> Result<WorktreeInfo, WorktreeError> {
    if !is_valid_branch(branch) {
        return Err(WorktreeError::InvalidBranch);
    }

    let repo = Repo::open(project_dir.as_ref())?;
    let list = list_worktrees(&repo.root)?;
    if list.is_empty() || list[0].bare {
        return Err(WorktreeError::Other(
            "the repository has no checkout to work beside".to_string(),
        ));
    }

    let main = list[0].path.clone();
    let mut info = WorktreeInfo {
        main: main.clone(),
        dir: PathBuf::new(),
        branch: branch.to_string(),
        base: String::new(),
        rel: String::new(),
        reused: false,
    };

    if let Ok(rel) = project_dir.as_ref().strip_prefix(&repo.root) {
        let rel_str = rel.to_string_lossy();
        if !rel_str.is_empty() && rel_str != "." {
            info.rel = rel_str.to_string();
        }
    }

    if !git_test(&main, &["rev-parse", "--verify", "-q", "HEAD"]) {
        return Err(WorktreeError::NoCommits);
    }

    let base_str: String;
    if base.is_empty() {
        base_str = repo.default_branch();
        if base_str.is_empty() {
            return Err(WorktreeError::ChooseBaseBranch);
        }
    } else {
        base_str = base.to_string();
    }
    let base = base_str.as_str();

    let ref_base = format!("refs/heads/{}", base);
    if !git_test(&main, &["rev-parse", "--verify", "-q", &ref_base]) {
        return Err(WorktreeError::NoBranch(base.to_string()));
    }
    info.base = base.to_string();

    let _ = git_out(&main, &["worktree", "prune"]);
    let refreshed_list = list_worktrees(&main)?;
    let target_parent = main.join(WORKTREE_FOLDER);

    for w in refreshed_list {
        if w.branch == branch {
            if w.path.parent() != Some(&target_parent) {
                return Err(WorktreeError::Other(format!(
                    "{} is checked out elsewhere in {}",
                    branch,
                    w.path.display()
                )));
            }
            info.dir = w.path;
            info.reused = true;
            return Ok(info);
        }
    }

    let safe_name = branch.replace('/', "-");
    let mut candidate_dir = target_parent.join(&safe_name);
    let mut idx = 2;
    while candidate_dir.exists() {
        candidate_dir = target_parent.join(format!("{}-{}", safe_name, idx));
        idx += 1;
    }

    let ref_branch = format!("refs/heads/{}", branch);
    if git_test(&main, &["rev-parse", "--verify", "-q", &ref_branch]) {
        git_out(
            &main,
            &["worktree", "add", &candidate_dir.to_string_lossy(), branch],
        )?;
    } else {
        git_out(
            &main,
            &[
                "worktree",
                "add",
                "-b",
                branch,
                &candidate_dir.to_string_lossy(),
                base,
            ],
        )?;
    }

    info.dir = candidate_dir;

    if copy_env {
        copy_env_files(&repo.root, &info.dir);
    }

    Ok(info)
}

/// Copies git-ignored .env files from `src` to `dst`, matching Go `worktree.CopyEnv`.
pub fn copy_env_files<P: AsRef<Path>>(src: P, dst: P) -> usize {
    let src_ref = src.as_ref();
    let dst_ref = dst.as_ref();
    let Ok(out) = git_out(
        src_ref,
        &[
            "ls-files",
            "-z",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
        ],
    ) else {
        return 0;
    };

    let mut copied = 0;
    for rel in out.split('\0') {
        if rel.is_empty()
            || rel.ends_with('/')
            || rel.starts_with(WORKTREE_FOLDER)
            || !Path::new(rel)
                .file_name()
                .map(|f| f.to_string_lossy().starts_with(".env"))
                .unwrap_or(false)
        {
            continue;
        }

        let from = src_ref.join(rel);
        let to = dst_ref.join(rel);
        if !from.is_file() || to.exists() {
            continue;
        }

        if let Some(parent) = to.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::copy(&from, &to).is_ok() {
            copied += 1;
        }
    }
    copied
}

/// Commits any pending changes in the worktree with `msg`.
pub fn commit_pending<P: AsRef<Path>>(dir: P, msg: &str) -> Result<(), WorktreeError> {
    let status = git_out(dir.as_ref(), &["status", "--porcelain"])?;
    if status.is_empty() {
        return Ok(());
    }
    git_out(dir.as_ref(), &["add", "--all"])?;
    let _ = git_cmd(dir.as_ref(), &["commit", "-q", "--no-verify", "-m", msg])?.output();
    Ok(())
}

/// Merges worktree into its base branch.
pub fn merge_worktree(
    w: &WorktreeInfo,
    msg: &str,
    squash: bool,
    keep: bool,
) -> Result<Vec<String>, WorktreeError> {
    commit_pending(&w.dir, msg)?;

    // Perform merge inside base checkout or main
    let main_ref = &w.main;
    if squash {
        git_out(main_ref, &["merge", "--squash", &w.branch])?;
        git_out(main_ref, &["commit", "-q", "-m", msg])?;
    } else {
        git_out(main_ref, &["merge", "--no-ff", "--no-edit", &w.branch])?;
    }

    if !keep && w.dir.exists() {
        let _ = remove_worktree(w);
    }

    Ok(vec![msg.to_string()])
}

/// Removes a worktree and prunes its git references.
pub fn remove_worktree(w: &WorktreeInfo) -> Result<(), WorktreeError> {
    if w.dir.exists() {
        let _ = git_cmd(
            &w.main,
            &["worktree", "remove", "--force", &w.dir.to_string_lossy()],
        )?
        .output();
    }
    let _ = git_cmd(&w.main, &["branch", "-D", &w.branch])?.output();
    let _ = git_cmd(&w.main, &["worktree", "prune"])?.output();
    Ok(())
}
