use futures_util::StreamExt;

use cursor_sdk::{
    CreateAgentRequest, CursorClient, ModelSelection, Prompt, RepositoryRef, RunStreamEvent,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let repo_url = args
        .next()
        .expect("usage: cargo run -p cursor-sdk --example quickstart -- <repo-url> <prompt>");
    let prompt_text = args
        .next()
        .expect("usage: cargo run -p cursor-sdk --example quickstart -- <repo-url> <prompt>");

    let client = CursorClient::from_env()?;
    let response = client
        .create_agent(&CreateAgentRequest {
            prompt: Prompt {
                text: prompt_text,
                images: Vec::new(),
            },
            model: Some(ModelSelection {
                id: "composer-2".to_owned(),
            }),
            repos: vec![RepositoryRef {
                url: Some(repo_url),
                starting_ref: None,
                pr_url: None,
            }],
            branch_name: None,
            auto_generate_branch: None,
            auto_create_pr: Some(false),
            skip_reviewer_request: None,
        })
        .await?;

    eprintln!("agent: {}", response.agent.id);
    eprintln!("run: {}", response.run.id);

    let mut stream = client
        .stream_run(&response.agent.id, &response.run.id, None)
        .await?;

    while let Some(message) = stream.next().await {
        match message?.event {
            RunStreamEvent::Assistant { text } => print!("{text}"),
            RunStreamEvent::Thinking { text } => eprintln!("[thinking] {text}"),
            RunStreamEvent::Status { status, .. } => eprintln!("[status] {status}"),
            RunStreamEvent::Result { status, .. } => eprintln!("\n[result] {status}"),
            RunStreamEvent::Error { message, .. } => eprintln!("[error] {message}"),
            RunStreamEvent::Heartbeat
            | RunStreamEvent::Done
            | RunStreamEvent::ToolCall { .. }
            | RunStreamEvent::Unknown { .. } => {}
        }
    }

    Ok(())
}
