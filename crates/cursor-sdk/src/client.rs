use std::pin::Pin;
use std::time::Instant;

use eventsource_stream::Eventsource;
use futures_util::{Stream, StreamExt};
use reqwest::{Method, RequestBuilder, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{CursorError, Result};
use crate::types::{
    Agent, AgentSummary, ApiKeyInfo, Artifact, CreateAgentRequest, CreateAgentResponse,
    CreateRunRequest, CreateRunResponse, DownloadArtifactResponse, ListResponse, ModelListResponse,
    RepositoryListResponse, Run, RunStreamEvent, RunStreamMessage, WaitForRunOptions,
    WaitForRunResult,
};

const DEFAULT_BASE_URL: &str = "https://api.cursor.com";
const DEFAULT_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Default)]
pub struct ListAgentsParams {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub pr_url: Option<String>,
    pub include_archived: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct ListRunsParams {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CursorClientBuilder {
    api_key: Option<String>,
    base_url: String,
    user_agent: String,
}

impl Default for CursorClientBuilder {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: DEFAULT_BASE_URL.to_owned(),
            user_agent: DEFAULT_USER_AGENT.to_owned(),
        }
    }
}

impl CursorClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("CURSOR_API_KEY").map_err(|_| CursorError::MissingApiKey)?;
        Ok(Self::new().api_key(api_key))
    }

    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    pub fn build(self) -> Result<CursorClient> {
        let api_key = self.api_key.ok_or(CursorError::MissingApiKey)?;
        let http = reqwest::Client::builder()
            .user_agent(self.user_agent)
            .build()?;

        Ok(CursorClient {
            http,
            api_key,
            base_url: self.base_url.trim_end_matches('/').to_owned(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct CursorClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl CursorClient {
    pub fn builder() -> CursorClientBuilder {
        CursorClientBuilder::new()
    }

    pub fn from_env() -> Result<Self> {
        CursorClientBuilder::from_env()?.build()
    }

    pub async fn me(&self) -> Result<ApiKeyInfo> {
        self.send(self.request(Method::GET, "/v1/me")).await
    }

    pub async fn list_models(&self) -> Result<ModelListResponse> {
        self.send(self.request(Method::GET, "/v1/models")).await
    }

    pub async fn list_repositories(&self) -> Result<RepositoryListResponse> {
        self.send(self.request(Method::GET, "/v1/repositories"))
            .await
    }

    pub async fn create_agent(&self, request: &CreateAgentRequest) -> Result<CreateAgentResponse> {
        self.send(self.request(Method::POST, "/v1/agents").json(request))
            .await
    }

    pub async fn list_agents(
        &self,
        params: &ListAgentsParams,
    ) -> Result<ListResponse<AgentSummary>> {
        self.send(self.request(Method::GET, "/v1/agents").query(&[
            ("limit", params.limit.map(|value| value.to_string())),
            ("cursor", params.cursor.clone()),
            ("prUrl", params.pr_url.clone()),
            (
                "includeArchived",
                params.include_archived.map(|value| value.to_string()),
            ),
        ]))
        .await
    }

    pub async fn get_agent(&self, agent_id: &str) -> Result<Agent> {
        self.send(self.request(Method::GET, &format!("/v1/agents/{agent_id}")))
            .await
    }

    pub async fn create_run(
        &self,
        agent_id: &str,
        request: &CreateRunRequest,
    ) -> Result<CreateRunResponse> {
        self.send(
            self.request(Method::POST, &format!("/v1/agents/{agent_id}/runs"))
                .json(request),
        )
        .await
    }

    pub async fn list_runs(
        &self,
        agent_id: &str,
        params: &ListRunsParams,
    ) -> Result<ListResponse<Run>> {
        self.send(
            self.request(Method::GET, &format!("/v1/agents/{agent_id}/runs"))
                .query(&[
                    ("limit", params.limit.map(|value| value.to_string())),
                    ("cursor", params.cursor.clone()),
                ]),
        )
        .await
    }

    pub async fn get_run(&self, agent_id: &str, run_id: &str) -> Result<Run> {
        self.send(self.request(Method::GET, &format!("/v1/agents/{agent_id}/runs/{run_id}")))
            .await
    }

    pub async fn cancel_run(&self, agent_id: &str, run_id: &str) -> Result<Value> {
        self.send(self.request(
            Method::POST,
            &format!("/v1/agents/{agent_id}/runs/{run_id}/cancel"),
        ))
        .await
    }

    pub async fn list_artifacts(&self, agent_id: &str) -> Result<ListResponse<Artifact>> {
        self.send(self.request(Method::GET, &format!("/v1/agents/{agent_id}/artifacts")))
            .await
    }

    pub async fn download_artifact_url(
        &self,
        agent_id: &str,
        path: &str,
    ) -> Result<DownloadArtifactResponse> {
        self.send(
            self.request(
                Method::GET,
                &format!("/v1/agents/{agent_id}/artifacts/download"),
            )
            .query(&[("path", path)]),
        )
        .await
    }

    pub async fn archive_agent(&self, agent_id: &str) -> Result<Value> {
        self.send(self.request(Method::POST, &format!("/v1/agents/{agent_id}/archive")))
            .await
    }

    pub async fn unarchive_agent(&self, agent_id: &str) -> Result<Value> {
        self.send(self.request(Method::POST, &format!("/v1/agents/{agent_id}/unarchive")))
            .await
    }

    pub async fn delete_agent(&self, agent_id: &str) -> Result<()> {
        self.send_empty(self.request(Method::DELETE, &format!("/v1/agents/{agent_id}")))
            .await
    }

    pub async fn stream_run(
        &self,
        agent_id: &str,
        run_id: &str,
        last_event_id: Option<&str>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<RunStreamMessage>> + Send>>> {
        let mut request = self
            .request(
                Method::GET,
                &format!("/v1/agents/{agent_id}/runs/{run_id}/stream"),
            )
            .header(reqwest::header::ACCEPT, "text/event-stream");

        if let Some(last_event_id) = last_event_id {
            request = request.header("Last-Event-ID", last_event_id);
        }

        let response = self.send_raw(request).await?;
        let stream = response
            .bytes_stream()
            .eventsource()
            .map(|item| match item {
                Ok(event) => parse_stream_event(Some(event.id), event.event, event.data),
                Err(error) => Err(CursorError::Stream(error.to_string())),
            });

        Ok(Box::pin(stream))
    }

    pub async fn wait_for_run(
        &self,
        agent_id: &str,
        run_id: &str,
        options: WaitForRunOptions,
    ) -> Result<WaitForRunResult> {
        let WaitForRunOptions {
            mut last_event_id,
            poll_interval,
            timeout,
            max_poll_attempts,
        } = options;

        let mut stream_messages = Vec::new();
        let mut used_polling_fallback = false;
        let wait_started_at = Instant::now();
        let mut poll_attempts = 0u32;

        match self
            .stream_run(agent_id, run_id, last_event_id.as_deref())
            .await
        {
            Ok(mut stream) => {
                while let Some(message) = stream.next().await {
                    let message = message?;
                    if let Some(id) = message.id.clone() {
                        last_event_id = Some(id);
                    }
                    stream_messages.push(message);
                }
            }
            Err(error) if error.is_stream_expired() => {
                used_polling_fallback = true;
            }
            Err(error) => return Err(error),
        }

        let mut run = self.get_run(agent_id, run_id).await?;
        while !run.is_terminal() {
            used_polling_fallback = true;

            if let Some(timeout) = timeout {
                if wait_started_at.elapsed() >= timeout {
                    return Err(CursorError::WaitTimeout {
                        agent_id: agent_id.to_owned(),
                        run_id: run_id.to_owned(),
                        timeout_ms: timeout.as_millis(),
                    });
                }
            }

            if let Some(max_poll_attempts) = max_poll_attempts {
                if poll_attempts >= max_poll_attempts {
                    return Err(CursorError::MaxPollAttemptsExceeded {
                        agent_id: agent_id.to_owned(),
                        run_id: run_id.to_owned(),
                        max_attempts: max_poll_attempts,
                    });
                }
            }

            tokio::time::sleep(poll_interval).await;
            poll_attempts += 1;
            run = self.get_run(agent_id, run_id).await?;
        }

        Ok(WaitForRunResult {
            run,
            stream_messages,
            last_event_id,
            used_polling_fallback,
        })
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, format!("{}{}", self.base_url, path))
            .basic_auth(&self.api_key, Some(""))
    }

    async fn send<T>(&self, request: RequestBuilder) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let response = self.send_raw(request).await?;
        Ok(response.json::<T>().await?)
    }

    async fn send_empty(&self, request: RequestBuilder) -> Result<()> {
        self.send_raw(request).await?;
        Ok(())
    }

    async fn send_raw(&self, request: RequestBuilder) -> Result<reqwest::Response> {
        let response = request.send().await?;

        if response.status().is_success() {
            return Ok(response);
        }

        Err(api_error(response).await)
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorEnvelope, fallback_status_message, parse_stream_event};
    use crate::error::CursorError;
    use crate::types::RunStreamEvent;
    use reqwest::StatusCode;

    #[test]
    fn fallback_status_messages_cover_expected_codes() {
        assert_eq!(
            fallback_status_message(StatusCode::UNAUTHORIZED),
            "unauthorized"
        );
        assert_eq!(fallback_status_message(StatusCode::FORBIDDEN), "forbidden");
        assert_eq!(fallback_status_message(StatusCode::NOT_FOUND), "not found");
        assert_eq!(fallback_status_message(StatusCode::CONFLICT), "conflict");
        assert_eq!(
            fallback_status_message(StatusCode::TOO_MANY_REQUESTS),
            "rate limited"
        );
        assert_eq!(
            fallback_status_message(StatusCode::BAD_GATEWAY),
            "request failed"
        );
    }

    #[test]
    fn parse_stream_event_decodes_status() {
        let message = parse_stream_event(
            Some("evt-1".to_owned()),
            "status".to_owned(),
            r#"{"runId":"run-1","status":"RUNNING"}"#.to_owned(),
        )
        .expect("parse status event");

        assert_eq!(message.id.as_deref(), Some("evt-1"));
        assert!(matches!(
            message.event,
            RunStreamEvent::Status { ref run_id, ref status }
                if run_id == "run-1" && status == "RUNNING"
        ));
    }

    #[test]
    fn parse_stream_event_preserves_unknown_events() {
        let message = parse_stream_event(None, "custom".to_owned(), r#"{"x":1}"#.to_owned())
            .expect("parse unknown event");

        assert!(matches!(
            message.event,
            RunStreamEvent::Unknown { ref name, .. } if name == "custom"
        ));
    }

    #[test]
    fn stream_expired_detection_is_specific() {
        let stream_expired = CursorError::Api {
            status: StatusCode::GONE,
            message: "expired".to_owned(),
            code: Some("stream_expired".to_owned()),
        };
        let other = CursorError::Api {
            status: StatusCode::GONE,
            message: "different".to_owned(),
            code: Some("other".to_owned()),
        };

        assert!(stream_expired.is_stream_expired());
        assert!(!other.is_stream_expired());
    }

    #[test]
    fn error_envelope_deserializes_api_shapes() {
        let envelope: ErrorEnvelope = serde_json::from_str(
            r#"{"error":"Bad Request","code":"invalid_request","message":"oops"}"#,
        )
        .expect("deserialize envelope");

        assert_eq!(envelope.error.as_deref(), Some("Bad Request"));
        assert_eq!(envelope.code.as_deref(), Some("invalid_request"));
        assert_eq!(envelope.message.as_deref(), Some("oops"));
    }
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: Option<String>,
    code: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StatusPayload {
    #[serde(rename = "runId")]
    run_id: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct TextPayload {
    text: String,
}

#[derive(Debug, Deserialize)]
struct ErrorPayload {
    code: Option<String>,
    message: String,
}

async fn api_error(response: reqwest::Response) -> CursorError {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    match serde_json::from_str::<ErrorEnvelope>(&body) {
        Ok(parsed) => {
            let message = parsed
                .message
                .or(parsed.error)
                .unwrap_or_else(|| fallback_status_message(status).to_owned());
            CursorError::Api {
                status,
                message,
                code: parsed.code,
            }
        }
        Err(_) => CursorError::Api {
            status,
            message: if body.trim().is_empty() {
                fallback_status_message(status).to_owned()
            } else {
                body
            },
            code: None,
        },
    }
}

fn fallback_status_message(status: StatusCode) -> &'static str {
    match status {
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "not found",
        StatusCode::CONFLICT => "conflict",
        StatusCode::TOO_MANY_REQUESTS => "rate limited",
        _ => "request failed",
    }
}

fn parse_stream_event(id: Option<String>, event: String, data: String) -> Result<RunStreamMessage> {
    let parsed = match event.as_str() {
        "status" => {
            let payload = serde_json::from_str::<StatusPayload>(&data)?;
            RunStreamEvent::Status {
                run_id: payload.run_id,
                status: payload.status,
            }
        }
        "assistant" => {
            let payload = serde_json::from_str::<TextPayload>(&data)?;
            RunStreamEvent::Assistant { text: payload.text }
        }
        "thinking" => {
            let payload = serde_json::from_str::<TextPayload>(&data)?;
            RunStreamEvent::Thinking { text: payload.text }
        }
        "tool_call" => RunStreamEvent::ToolCall {
            payload: serde_json::from_str::<Value>(&data)?,
        },
        "heartbeat" => RunStreamEvent::Heartbeat,
        "result" => {
            let payload = serde_json::from_str::<StatusPayload>(&data)?;
            RunStreamEvent::Result {
                run_id: payload.run_id,
                status: payload.status,
            }
        }
        "error" => {
            let payload = serde_json::from_str::<ErrorPayload>(&data)?;
            RunStreamEvent::Error {
                code: payload.code,
                message: payload.message,
            }
        }
        "done" => RunStreamEvent::Done,
        _ => RunStreamEvent::Unknown {
            name: event,
            payload: serde_json::from_str::<Value>(&data).unwrap_or(Value::String(data)),
        },
    };

    Ok(RunStreamMessage { id, event: parsed })
}
