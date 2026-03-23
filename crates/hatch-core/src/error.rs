/// Convenient result alias using [`HatchError`].
pub type Result<T> = std::result::Result<T, HatchError>;

/// Errors that can occur across HATCH crates.
#[derive(Debug, thiserror::Error)]
pub enum HatchError {
    /// I/O error (e.g. reading agent templates).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization or deserialization failed.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// TOML deserialization failed.
    #[error("toml error: {0}")]
    TomlDeserialize(#[from] toml::de::Error),

    /// HTTP client or transport error (stringified for crates that do not re-export `reqwest`).
    #[error("http error: {0}")]
    Http(String),

    /// Message bus operation failed (e.g. no active subscribers).
    #[error("message bus error: {0}")]
    Bus(String),

    /// Planner produced invalid or unexpected content.
    #[error("planner error: {0}")]
    Planner(String),

    /// LLM provider returned an error response or empty completion.
    #[error("llm error: {0}")]
    Llm(String),

    /// Configuration or environment is invalid.
    #[error("config error: {0}")]
    Config(String),

    /// Requested agent template or type was not found.
    #[error("template error: {0}")]
    Template(String),

    /// Agent execution failed.
    #[error("agent error: {0}")]
    Agent(String),

    /// Supervisor timed out or received inconsistent state.
    #[error("supervisor error: {0}")]
    Supervisor(String),

    /// UTF-8 conversion failed.
    #[error("utf8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}
