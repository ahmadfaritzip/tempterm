//! Core engine, session registry, and pane management for temp-terminal.

pub mod devurl;
pub mod git;
pub mod mail;
pub mod paths;
pub mod registry;
pub mod worktree;

pub use devurl::{host_only, is_valid_dev_host, pages, proxy::ProxyServer, Route, Router};
pub use git::{git_command_path, git_out, git_test, GitError, Repo};
pub use mail::{MailError, Mailbox};
pub use registry::{Pane, PaneInfo, PaneStatus, Registry, RegistryError, StartPaneOptions};
pub use worktree::{
    add_worktree, commit_pending, is_valid_branch, merge_worktree, remove_worktree, WorktreeError,
    WorktreeInfo, WORKTREE_FOLDER,
};
