use serde::{Deserialize, Serialize};

/// Role of a chat message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// System instruction.
    System,
    /// User message.
    User,
    /// Assistant reply.
    Assistant,
}

/// A single chat turn for completion APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of this message.
    pub role: MessageRole,
    /// Message body.
    pub content: String,
}
