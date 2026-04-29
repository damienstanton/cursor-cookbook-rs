use thiserror::Error;

pub type Result<T> = std::result::Result<T, CursorError>;

#[derive(Debug, Error)]
pub enum CursorError {
    #[error("missing Cursor API key; set CURSOR_API_KEY or provide one explicitly")]
    MissingApiKey,
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("json decode failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("cursor api error {status}: {message}")]
    Api {
        status: reqwest::StatusCode,
        message: String,
        code: Option<String>,
    },
    #[error("stream decode failed: {0}")]
    Stream(String),
    #[error("timed out waiting for run {run_id} on agent {agent_id} after {timeout_ms}ms")]
    WaitTimeout {
        agent_id: String,
        run_id: String,
        timeout_ms: u128,
    },
    #[error(
        "exceeded max poll attempts waiting for run {run_id} on agent {agent_id}: {max_attempts}"
    )]
    MaxPollAttemptsExceeded {
        agent_id: String,
        run_id: String,
        max_attempts: u32,
    },
}

impl CursorError {
    pub fn is_stream_expired(&self) -> bool {
        matches!(
            self,
            CursorError::Api {
                status,
                code,
                ..
            } if *status == reqwest::StatusCode::GONE && code.as_deref() == Some("stream_expired")
        )
    }
}
