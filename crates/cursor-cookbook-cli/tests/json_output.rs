use assert_cmd::Command;
use serde_json::Value;
use wiremock::matchers::{basic_auth, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn command(server: &MockServer) -> Command {
    let mut command = Command::cargo_bin("cursor-cookbook-cli").expect("binary");
    command.env("CURSOR_API_KEY", "test-key");
    command.env("CURSOR_BASE_URL", server.uri());
    command
}

#[tokio::test]
async fn runs_stream_json_emits_expected_event_shape_and_resume_header() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/agents/bc-1/runs/run-1/stream"))
        .and(basic_auth("test-key", ""))
        .and(header("last-event-id", "evt-0"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_raw(
            concat!(
                "id: evt-1\n",
                "event: assistant\n",
                "data: {\"text\":\"hello\"}\n\n",
                "id: evt-2\n",
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

    let output = command(&server)
        .args([
            "--output",
            "json",
            "runs",
            "stream",
            "bc-1",
            "run-1",
            "--last-event-id",
            "evt-0",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8(output).expect("utf8 stdout");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3);

    let first: Value = serde_json::from_str(lines[0]).expect("first json line");
    let second: Value = serde_json::from_str(lines[1]).expect("second json line");
    let third: Value = serde_json::from_str(lines[2]).expect("third json line");

    assert_eq!(first["type"], "stream_event");
    assert_eq!(first["id"], "evt-1");
    assert_eq!(first["event"]["Assistant"]["text"], "hello");

    assert_eq!(second["type"], "stream_event");
    assert_eq!(second["id"], "evt-2");
    assert_eq!(second["event"]["Result"]["run_id"], "run-1");
    assert_eq!(second["event"]["Result"]["status"], "FINISHED");

    assert_eq!(third["type"], "stream_event");
    assert_eq!(third["id"], "evt-3");
    assert_eq!(third["event"], "Done");
}

#[tokio::test]
async fn runs_wait_json_emits_wait_result_shape() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/agents/bc-1/runs/run-1/stream"))
        .and(basic_auth("test-key", ""))
        .respond_with(ResponseTemplate::new(410).set_body_raw(
            r#"{"code":"stream_expired","message":"stream retention elapsed"}"#,
            "application/json",
        ))
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

    let output = command(&server)
        .args([
            "--output",
            "json",
            "runs",
            "wait",
            "bc-1",
            "run-1",
            "--last-event-id",
            "evt-9",
            "--poll-interval-ms",
            "1",
            "--max-poll-attempts",
            "1",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8(output).expect("utf8 stdout");
    let value: Value = serde_json::from_str(stdout.trim()).expect("wait result json");

    assert_eq!(value["type"], "wait_result");
    assert_eq!(value["run"]["status"], "FINISHED");
    assert_eq!(value["lastEventId"], "evt-9");
    assert_eq!(value["usedPollingFallback"], true);
    assert_eq!(value["streamMessages"].as_array().map(Vec::len), Some(0));
}