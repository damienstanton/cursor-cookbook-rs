use std::time::Duration;

use cursor_sdk::{
    CreateRunRequest, CursorClient, ListAgentsParams, Prompt, RunStreamEvent, WaitForRunOptions,
};
use wiremock::matchers::{basic_auth, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(server: &MockServer) -> CursorClient {
    CursorClient::builder()
        .api_key("test-key")
        .base_url(server.uri())
        .build()
        .expect("client")
}

#[tokio::test]
async fn list_agents_sends_basic_auth_and_query_params() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/agents"))
        .and(basic_auth("test-key", ""))
        .and(query_param("limit", "5"))
        .and(query_param("cursor", "cursor-1"))
        .and(query_param("prUrl", "https://github.com/acme/repo/pull/1"))
        .and(query_param("includeArchived", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"items":[{"id":"bc-1","name":"demo","status":"ACTIVE","env":{"type":"cloud"},"url":"https://cursor.com/agents?id=bc-1","createdAt":"2026-04-13T18:30:00.000Z","updatedAt":"2026-04-13T18:45:00.000Z","latestRunId":"run-1"}],"nextCursor":"cursor-2"}"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let response = client(&server)
        .list_agents(&ListAgentsParams {
            limit: Some(5),
            cursor: Some("cursor-1".to_owned()),
            pr_url: Some("https://github.com/acme/repo/pull/1".to_owned()),
            include_archived: Some(false),
        })
        .await
        .expect("list agents");

    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].id, "bc-1");
    assert_eq!(response.next_cursor.as_deref(), Some("cursor-2"));
}

#[tokio::test]
async fn stream_run_parses_sse_messages_and_ids() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/agents/bc-1/runs/run-1/stream"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_raw(
            concat!(
                "id: evt-1\n",
                "event: status\n",
                "data: {\"runId\":\"run-1\",\"status\":\"RUNNING\"}\n\n",
                "id: evt-2\n",
                "event: assistant\n",
                "data: {\"text\":\"hello\"}\n\n",
                "id: evt-3\n",
                "event: result\n",
                "data: {\"runId\":\"run-1\",\"status\":\"FINISHED\"}\n\n",
                "id: evt-3\n",
                "event: done\n",
                "data: {}\n\n"
            ),
            "text/event-stream",
        ))
        .mount(&server)
        .await;

    let mut stream = client(&server)
        .stream_run("bc-1", "run-1", None)
        .await
        .expect("stream run");

    let mut messages = Vec::new();
    use futures_util::StreamExt;
    while let Some(message) = stream.next().await {
        messages.push(message.expect("stream item"));
    }

    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0].id.as_deref(), Some("evt-1"));
    assert!(matches!(
        messages[1].event,
        RunStreamEvent::Assistant { ref text } if text == "hello"
    ));
    assert!(matches!(
        messages[2].event,
        RunStreamEvent::Result { ref status, .. } if status == "FINISHED"
    ));
}

#[tokio::test]
async fn wait_for_run_falls_back_to_polling_when_stream_expired() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/agents/bc-1/runs/run-1/stream"))
        .respond_with(ResponseTemplate::new(410).set_body_raw(
            r#"{"code":"stream_expired","message":"stream retention elapsed"}"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/agents/bc-1/runs/run-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(
                    r#"{"id":"run-1","agentId":"bc-1","status":"RUNNING","createdAt":"2026-04-13T18:30:00.000Z","updatedAt":"2026-04-13T18:31:00.000Z"}"#,
                    "application/json",
                )
                .set_delay(Duration::from_millis(1)),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/agents/bc-1/runs/run-1"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"id":"run-1","agentId":"bc-1","status":"FINISHED","createdAt":"2026-04-13T18:30:00.000Z","updatedAt":"2026-04-13T18:32:00.000Z"}"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let result = client(&server)
        .wait_for_run(
            "bc-1",
            "run-1",
            WaitForRunOptions {
                last_event_id: Some("evt-9".to_owned()),
                poll_interval: Duration::from_millis(1),
                timeout: None,
                max_poll_attempts: None,
            },
        )
        .await
        .expect("wait for run");

    assert_eq!(result.run.status, "FINISHED");
    assert!(result.used_polling_fallback);
    assert_eq!(result.last_event_id.as_deref(), Some("evt-9"));
    assert!(result.stream_messages.is_empty());
}

#[tokio::test]
async fn wait_for_run_respects_max_poll_attempts() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/agents/bc-1/runs/run-1/stream"))
        .respond_with(ResponseTemplate::new(410).set_body_raw(
            r#"{"code":"stream_expired","message":"stream retention elapsed"}"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/agents/bc-1/runs/run-1"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"id":"run-1","agentId":"bc-1","status":"RUNNING","createdAt":"2026-04-13T18:30:00.000Z","updatedAt":"2026-04-13T18:31:00.000Z"}"#,
            "application/json",
        ))
        .expect(2)
        .mount(&server)
        .await;

    let error = client(&server)
        .wait_for_run(
            "bc-1",
            "run-1",
            WaitForRunOptions {
                last_event_id: None,
                poll_interval: Duration::from_millis(1),
                timeout: None,
                max_poll_attempts: Some(1),
            },
        )
        .await
        .expect_err("wait should hit max poll attempts");

    match error {
        cursor_sdk::CursorError::MaxPollAttemptsExceeded {
            agent_id,
            run_id,
            max_attempts,
        } => {
            assert_eq!(agent_id, "bc-1");
            assert_eq!(run_id, "run-1");
            assert_eq!(max_attempts, 1);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[tokio::test]
async fn wait_for_run_respects_timeout() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/agents/bc-1/runs/run-1/stream"))
        .respond_with(ResponseTemplate::new(410).set_body_raw(
            r#"{"code":"stream_expired","message":"stream retention elapsed"}"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/agents/bc-1/runs/run-1"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"id":"run-1","agentId":"bc-1","status":"RUNNING","createdAt":"2026-04-13T18:30:00.000Z","updatedAt":"2026-04-13T18:31:00.000Z"}"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let error = client(&server)
        .wait_for_run(
            "bc-1",
            "run-1",
            WaitForRunOptions {
                last_event_id: None,
                poll_interval: Duration::from_millis(5),
                timeout: Some(Duration::from_millis(0)),
                max_poll_attempts: None,
            },
        )
        .await
        .expect_err("wait should time out");

    match error {
        cursor_sdk::CursorError::WaitTimeout {
            agent_id,
            run_id,
            timeout_ms,
        } => {
            assert_eq!(agent_id, "bc-1");
            assert_eq!(run_id, "run-1");
            assert_eq!(timeout_ms, 0);
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[tokio::test]
async fn create_run_posts_prompt_payload() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/agents/bc-1/runs"))
        .and(basic_auth("test-key", ""))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"run":{"id":"run-2","agentId":"bc-1","status":"CREATING","createdAt":"2026-04-13T18:50:00.000Z","updatedAt":"2026-04-13T18:50:00.000Z"}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let response = client(&server)
        .create_run(
            "bc-1",
            &CreateRunRequest {
                prompt: Prompt {
                    text: "Hello".to_owned(),
                    images: Vec::new(),
                },
            },
        )
        .await
        .expect("create run");

    assert_eq!(response.run.id, "run-2");
}