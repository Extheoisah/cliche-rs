use serde::{Deserialize, Serialize};

/// Whether a failed command could have reached cliche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Execution {
    /// Command transmission never began.
    NotSent,
    /// The command might already have executed. Reconcile before retrying.
    Unknown,
}

/// Stable failure categories for Rust and JSON consumers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// Invalid configuration or command parameters.
    InvalidArgument,
    /// Transport unavailable.
    Connection,
    /// Invalid wire message.
    Protocol,
    /// cliche returned a command error.
    Upstream,
    /// Deadline elapsed, possibly after execution began.
    Timeout,
    /// Client closed.
    Closed,
    /// Resource limit reached.
    Overloaded,
    /// Managed process failed or exited.
    Process,
}

/// A serializable failure. Upstream messages may contain sensitive information;
/// display/debug intentionally omit their content. Inspect `message` explicitly.
#[derive(Clone, Serialize, Deserialize, thiserror::Error)]
#[error("cliche {kind:?} error ({execution:?})")]
pub struct Error {
    /// Failure category.
    pub kind: ErrorKind,
    /// Human-readable detail. Treat upstream-provided content as sensitive.
    pub message: String,
    /// Whether transmission began.
    pub execution: Execution,
    /// Original cliche error code, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_code: Option<i64>,
}
impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Error")
            .field("kind", &self.kind)
            .field("execution", &self.execution)
            .finish_non_exhaustive()
    }
}
#[allow(dead_code)]
impl Error {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>, execution: Execution) -> Self {
        Self {
            kind,
            message: message.into(),
            execution,
            upstream_code: None,
        }
    }
    pub(crate) fn invalid(message: &str) -> Self {
        Self::new(ErrorKind::InvalidArgument, message, Execution::NotSent)
    }
}
