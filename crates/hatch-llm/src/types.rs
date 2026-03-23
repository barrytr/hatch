use crate::Message;

/// Request payload for a completion call.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// Model identifier (provider-specific).
    pub model: String,
    /// Conversation messages.
    pub messages: Vec<Message>,
    /// Optional max tokens to generate.
    pub max_tokens: Option<u32>,
    /// Sampling temperature.
    pub temperature: Option<f32>,
    /// Optional system string (some providers accept this separately).
    pub system: Option<String>,
}

/// Response from a non-streaming completion.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    /// Aggregated text content.
    pub content: String,
    /// Model that served the request, if returned by the provider.
    pub model: Option<String>,
}
