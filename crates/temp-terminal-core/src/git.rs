//! Git operations, snapshots, and command execution for `temp-terminal-core`.
//! Ported with exact parity from `tempterm/internal/gitsnap`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("git is not installed")]
    NoGit,
    #[error("{0} is not in a git repository")]
    NotARepo(String),
    #[error("{0}")]
    Other(String),
}

static GIT_PATH: Mutex<Option<String>> = Mutex::new(None);

/// Finds git executable in PATH.
pub fn git_command_path() -> Result<String, GitError> {
    let mut lock = GIT_PATH.lock().unwrap();
    if let Some(ref p) = *lock {
        return Ok(p.clone());
    }

    // Try looking up git
    #[cfg(windows)]
    let check = Command::new("where").arg("git").output();
    #[cfg(not(windows))]
    let check = Command::new("which").arg("git").output();

    match check {
        Ok(out) if out.status.success() => {
            let path_str = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("git")
                .trim()
                .to_string();
            *lock = Some(path_str.clone());
            Ok(path_str)
        }
        _ => Err(GitError::NoGit),
    }
}

/// Builds a `Command` targeting git with `CREATE_NO_WINDOW` on Windows.
pub fn git_cmd<P: AsRef<Path>>(dir: P, args: &[&str]) -> Result<Command, GitError> {
    let git = git_command_path()?;
    let mut cmd = Command::new(git);
    cmd.current_dir(dir);
    cmd.args(args);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    Ok(cmd)
}

/// Executes git command and returns standard output on success.
pub fn git_out<P: AsRef<Path>>(dir: P, args: &[&str]) -> Result<String, GitError> {
    let mut cmd = git_cmd(dir, args)?;
    let out = cmd.output().map_err(|e| GitError::Other(e.to_string()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(GitError::Other(if err.is_empty() {
            format!("git {:?} failed", args)
        } else {
            err
        }));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Tests whether a git command exits with code 0.
pub fn git_test<P: AsRef<Path>>(dir: P, args: &[&str]) -> bool {
    if let Ok(mut cmd) = git_cmd(dir, args) {
        if let Ok(out) = cmd.output() {
            return out.status.success();
        }
    }
    false
}

/// Information about a Git repository.
#[derive(Debug, Clone)]
pub struct Repo {
    pub root: PathBuf,
    pub git_dir: PathBuf,
}

impl Repo {
    /// Opens the Git repository enclosing `dir`.
    pub fn open<P: AsRef<Path>>(dir: P) -> Result<Self, GitError> {
        let dir_ref = dir.as_ref();
        let root_str = git_out(dir_ref, &["rev-parse", "--show-toplevel"])
            .map_err(|_| GitError::NotARepo(dir_ref.display().to_string()))?;
        let git_dir_str = git_out(dir_ref, &["rev-parse", "--git-dir"])
            .map_err(|_| GitError::NotARepo(dir_ref.display().to_string()))?;

        let root = PathBuf::from(root_str);
        let git_dir = if Path::new(&git_dir_str).is_absolute() {
            PathBuf::from(git_dir_str)
        } else {
            root.join(git_dir_str)
        };

        Ok(Repo { root, git_dir })
    }

    /// Determines the repository default branch (`main`, `master`, etc.).
    pub fn default_branch(&self) -> String {
        if let Ok(ref_name) = git_out(&self.root, &["symbolic-ref", "refs/remotes/origin/HEAD"]) {
            if let Some(branch) = ref_name.strip_prefix("refs/remotes/origin/") {
                return branch.to_string();
            }
        }
        if git_test(
            &self.root,
            &["rev-parse", "--verify", "-q", "refs/heads/main"],
        ) {
            return "main".to_string();
        }
        if git_test(
            &self.root,
            &["rev-parse", "--verify", "-q", "refs/heads/master"],
        ) {
            return "master".to_string();
        }
        String::new()
    }

    /// Stages a file: `git add -- <path>` (matching Go `service.Stage`).
    pub fn stage_file(&self, rel_path: &str) -> Result<(), GitError> {
        stage_file(&self.root, rel_path)
    }

    /// Unstages a file: `git reset -q -- <path>` (matching Go `service.Unstage`).
    pub fn unstage_file(&self, rel_path: &str) -> Result<(), GitError> {
        unstage_file(&self.root, rel_path)
    }

    /// Discards changes in a file (matching Go `service.Discard`).
    pub fn discard_file(&self, rel_path: &str, untracked: bool) -> Result<(), GitError> {
        discard_file(&self.root, rel_path, untracked)
    }

    /// Reads content from git index (`git show :<path>`).
    pub fn show_index(&self, rel_path: &str) -> Result<String, GitError> {
        show_index(&self.root, rel_path)
    }

    /// Reads content from git HEAD (`git show HEAD:<path>`).
    pub fn show_head(&self, rel_path: &str) -> Result<String, GitError> {
        show_head(&self.root, rel_path)
    }
}

/// Validates that a relative path does not escape the repository via path traversal.
pub fn validate_git_path<P: AsRef<Path>>(dir: P, rel_path: &str) -> Result<String, GitError> {
    let raw = rel_path.trim().replace('\\', "/");
    let trimmed = raw.trim_start_matches('/');
    if trimmed.is_empty() {
        return Err(GitError::Other("path cannot be empty".to_string()));
    }
    let p = Path::new(trimmed);
    if p.is_absolute() {
        return Err(GitError::Other(format!(
            "absolute path not allowed in git operations: {}",
            rel_path
        )));
    }
    for comp in p.components() {
        if matches!(comp, std::path::Component::ParentDir) {
            return Err(GitError::Other(format!(
                "path traversal ('..') not allowed in git operations: {}",
                rel_path
            )));
        }
    }
    let full = dir.as_ref().join(trimmed);
    if let (Ok(dir_canon), Ok(full_canon)) = (dir.as_ref().canonicalize(), full.canonicalize()) {
        if !full_canon.starts_with(&dir_canon) {
            return Err(GitError::Other(format!(
                "path escapes repository boundary: {}",
                rel_path
            )));
        }
    }
    Ok(trimmed.to_string())
}

/// Stages a file: `git add -- <path>` (matching Go `service.Stage`).
pub fn stage_file<P: AsRef<Path>>(dir: P, rel_path: &str) -> Result<(), GitError> {
    let clean = validate_git_path(&dir, rel_path)?;
    git_out(dir, &["add", "--", &clean])?;
    Ok(())
}

/// Unstages a file: `git reset -q -- <path>` (matching Go `service.Unstage`).
pub fn unstage_file<P: AsRef<Path>>(dir: P, rel_path: &str) -> Result<(), GitError> {
    let clean = validate_git_path(&dir, rel_path)?;
    git_out(dir, &["reset", "-q", "--", &clean])?;
    Ok(())
}

/// Discards working-tree changes of a file (matching Go `service.Discard`).
pub fn discard_file<P: AsRef<Path>>(
    dir: P,
    rel_path: &str,
    untracked: bool,
) -> Result<(), GitError> {
    let clean = validate_git_path(&dir, rel_path)?;
    if untracked {
        git_out(dir, &["clean", "-fd", "--", &clean])?;
    } else {
        git_out(dir, &["checkout", "--", &clean])?;
    }
    Ok(())
}

/// Reads content from git index: `git show :<path>`.
pub fn show_index<P: AsRef<Path>>(dir: P, rel_path: &str) -> Result<String, GitError> {
    let clean = validate_git_path(&dir, rel_path)?;
    let spec = format!(":{}", clean);
    git_out(dir, &["show", &spec])
}

/// Reads content from git HEAD: `git show HEAD:<path>`.
pub fn show_head<P: AsRef<Path>>(dir: P, rel_path: &str) -> Result<String, GitError> {
    let clean = validate_git_path(&dir, rel_path)?;
    let spec = format!("HEAD:{}", clean);
    git_out(dir, &["show", &spec])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_git_path_safety() {
        let temp = std::env::temp_dir();
        // Valid normal relative paths
        assert_eq!(
            validate_git_path(&temp, "src/main.rs").unwrap(),
            "src/main.rs"
        );
        assert_eq!(
            validate_git_path(&temp, "folder with spaces/file.txt").unwrap(),
            "folder with spaces/file.txt"
        );
        assert_eq!(
            validate_git_path(&temp, r"nested\windows\path.rs").unwrap(),
            "nested/windows/path.rs"
        );

        // Path traversal attempts must be rejected
        assert!(validate_git_path(&temp, "../outside.txt").is_err());
        assert!(validate_git_path(&temp, "src/../../outside.txt").is_err());
        assert!(validate_git_path(&temp, "..").is_err());

        // Empty path must be rejected
        assert!(validate_git_path(&temp, "").is_err());
        assert!(validate_git_path(&temp, "   ").is_err());

        // Absolute path must be rejected
        #[cfg(windows)]
        assert!(validate_git_path(&temp, "C:/Windows/System32").is_err());
        #[cfg(not(windows))]
        assert!(validate_git_path(&temp, "/etc/passwd").is_err());
    }
}
