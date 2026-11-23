use thiserror::Error;

/// Structured errors. Every variant has a stable machine-readable `code()`
/// so the CLI (`--json`) and MCP server can report them without string matching.
#[derive(Debug, Error)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("invalid state transition: {0}")]
    Transition(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("verification required: {0}")]
    VerificationRequired(String),
    #[error("unsupported capability: {0}")]
    Unsupported(String),
    #[error("state drift detected: {0}")]
    Drift(String),
    #[error("git error: {0}")]
    Git(String),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Error::NotFound(_) => "not_found",
            Error::Invalid(_) => "invalid_input",
            Error::Transition(_) => "invalid_transition",
            Error::Conflict(_) => "conflict",
            Error::VerificationRequired(_) => "verification_required",
            Error::Unsupported(_) => "unsupported",
            Error::Drift(_) => "state_drift",
            Error::Git(_) => "git_error",
            Error::Db(_) => "database_error",
            Error::Io(_) => "io_error",
            Error::Json(_) => "serialization_error",
            Error::Other(_) => "error",
        }
    }

    /// Process exit code for the CLI.
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::NotFound(_) => 3,
            Error::Invalid(_) => 2,
            Error::Transition(_) | Error::VerificationRequired(_) => 4,
            Error::Conflict(_) | Error::Drift(_) => 5,
            Error::Unsupported(_) => 6,
            _ => 1,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
