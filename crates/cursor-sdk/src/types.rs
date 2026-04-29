use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyInfo {
    pub api_key_name: String,
    pub created_at: String,
    pub user_email: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResponse<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelListResponse {
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RepositoryListResponse {
    pub items: Vec<RepositoryItem>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RepositoryItem {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EnvironmentInfo {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryRef {
    pub url: Option<String>,
    pub starting_ref: Option<String>,
    pub pr_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelSelection {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInput {
    pub data: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Prompt {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageInput>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub env: EnvironmentInfo,
    pub url: String,
    pub created_at: String,
    pub updated_at: String,
    pub latest_run_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub status: String,
    pub env: EnvironmentInfo,
    #[serde(default)]
    pub repos: Vec<RepositoryRef>,
    pub branch_name: Option<String>,
    pub auto_generate_branch: Option<bool>,
    pub auto_create_pr: Option<bool>,
    pub skip_reviewer_request: Option<bool>,
    pub url: String,
    pub created_at: String,
    pub updated_at: String,
    pub latest_run_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub agent_id: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Run {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status.as_str(),
            "FINISHED" | "ERROR" | "CANCELLED" | "EXPIRED"
        )
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgentRequest {
    pub prompt: Prompt,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelSelection>,
    pub repos: Vec<RepositoryRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_generate_branch: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_create_pr: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_reviewer_request: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateRunRequest {
    pub prompt: Prompt,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateAgentResponse {
    pub agent: Agent,
    pub run: Run,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateRunResponse {
    pub run: Run,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub path: String,
    pub size_bytes: u64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadArtifactResponse {
    pub url: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunStreamMessage {
    pub id: Option<String>,
    pub event: RunStreamEvent,
}

#[derive(Debug, Clone, Serialize)]
pub enum RunStreamEvent {
    Status {
        run_id: String,
        status: String,
    },
    Assistant {
        text: String,
    },
    Thinking {
        text: String,
    },
    ToolCall {
        payload: Value,
    },
    Heartbeat,
    Result {
        run_id: String,
        status: String,
    },
    Error {
        code: Option<String>,
        message: String,
    },
    Done,
    Unknown {
        name: String,
        payload: Value,
    },
}

#[derive(Debug, Clone)]
pub struct WaitForRunOptions {
    pub last_event_id: Option<String>,
    pub poll_interval: Duration,
    pub timeout: Option<Duration>,
    pub max_poll_attempts: Option<u32>,
}

impl Default for WaitForRunOptions {
    fn default() -> Self {
        Self {
            last_event_id: None,
            poll_interval: Duration::from_secs(2),
            timeout: None,
            max_poll_attempts: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WaitForRunResult {
    pub run: Run,
    pub stream_messages: Vec<RunStreamMessage>,
    pub last_event_id: Option<String>,
    pub used_polling_fallback: bool,
}

#[cfg(test)]
mod tests {
    use super::Run;

    fn run_with_status(status: &str) -> Run {
        Run {
            id: "run-1".to_owned(),
            agent_id: "bc-1".to_owned(),
            status: status.to_owned(),
            created_at: "2026-04-13T18:30:00.000Z".to_owned(),
            updated_at: "2026-04-13T18:30:00.000Z".to_owned(),
        }
    }

    #[test]
    fn terminal_statuses_are_detected() {
        for status in ["FINISHED", "ERROR", "CANCELLED", "EXPIRED"] {
            assert!(
                run_with_status(status).is_terminal(),
                "status {status} should be terminal"
            );
        }

        for status in ["CREATING", "RUNNING", "ACTIVE"] {
            assert!(
                !run_with_status(status).is_terminal(),
                "status {status} should not be terminal"
            );
        }
    }
}
