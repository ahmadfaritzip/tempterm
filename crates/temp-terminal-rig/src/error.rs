use thiserror::Error;

#[derive(Debug, Error)]
pub enum RigError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("ticket not found: {0}")]
    TicketNotFound(String),

    #[error("run not found: {0}")]
    RunNotFound(String),

    #[error("invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("unmerged worktree: {0}")]
    UnmergedWorktree(String),

    #[error("git error: {0}")]
    GitError(String),
}
