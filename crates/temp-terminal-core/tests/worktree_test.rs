//! Integration tests for Git and Worktree operations in `temp-terminal-core`.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use temp_terminal_core::{add_worktree, git_command_path, merge_worktree, Repo};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tt_test_{}_{}", prefix, nanos));
        let _ = fs::create_dir_all(&path);
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn init_test_git_repo(prefix: &str) -> Option<TempDir> {
    if git_command_path().is_err() {
        return None;
    }
    let temp = TempDir::new(prefix);
    let root = &temp.path;

    // Configure user and initialize git repo
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };

    if !run(&["init", "-b", "main"]) {
        return None;
    }
    let _ = run(&["config", "user.name", "Tester"]);
    let _ = run(&["config", "user.email", "tester@example.com"]);

    // Initial commit with .gitignore ignoring .env.local
    fs::write(root.join(".gitignore"), ".env*\n.worktrees/\n").ok()?;
    fs::write(root.join("README.md"), "# Test Repo\n").ok()?;
    fs::write(root.join(".env.local"), "SECRET=42\n").ok()?;
    if !run(&["add", "README.md", ".gitignore"]) {
        return None;
    }
    if !run(&["commit", "-m", "initial commit"]) {
        return None;
    }

    Some(temp)
}

#[test]
fn test_git_repo_open_and_default_branch() {
    let Some(temp) = init_test_git_repo("repo_open") else {
        eprintln!("skipping git test: git not installed");
        return;
    };

    let repo = Repo::open(&temp.path).expect("open repo");
    assert_eq!(repo.default_branch(), "main");
}

#[test]
fn test_worktree_lifecycle_add_and_merge() {
    let Some(temp) = init_test_git_repo("wt_lifecycle") else {
        eprintln!("skipping git test: git not installed");
        return;
    };

    // 1. Add worktree
    let wt = add_worktree(&temp.path, "feature-a", "main", true).expect("add worktree");
    assert_eq!(wt.branch, "feature-a");
    assert_eq!(wt.base, "main");
    assert!(wt.dir.exists());
    assert!(wt.dir.join(".env.local").exists());

    // 2. Commit a change in the worktree
    fs::write(wt.dir.join("feature.txt"), "new feature\n").expect("write feature file");

    // 3. Merge worktree
    let commits = merge_worktree(&wt, "implement feature a", true, false).expect("merge worktree");
    assert_eq!(commits.len(), 1);
    assert!(
        !wt.dir.exists(),
        "worktree should be cleaned up after merge when keep=false"
    );
}
