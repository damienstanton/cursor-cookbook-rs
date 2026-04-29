use anyhow::{bail, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use cursor_sdk::{
    CreateAgentRequest, CreateRunRequest, CursorClient, ListAgentsParams, ListRunsParams,
    ModelSelection, Prompt, RepositoryRef, RunStreamEvent, RunStreamMessage, WaitForRunOptions,
};
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::json;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(name = "cursor-cookbook", version, about = "Rust cookbook CLI for Cursor Cloud Agents")]
struct Cli {
    #[arg(long, env = "CURSOR_API_KEY")]
    api_key: Option<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    output: OutputFormat,
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Subcommand)]
enum Command {
    Me,
    Models,
    Repositories,
    Quickstart(QuickstartArgs),
    #[command(subcommand)]
    Agents(AgentsCommand),
    #[command(subcommand)]
    Runs(RunsCommand),
}

#[derive(Debug, Args)]
struct QuickstartArgs {
    #[arg(long)]
    repo_url: String,
    #[arg(long)]
    starting_ref: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long, default_value_t = false)]
    auto_create_pr: bool,
    #[arg(long)]
    branch_name: Option<String>,
    prompt: String,
}

#[derive(Debug, Subcommand)]
enum AgentsCommand {
    List {
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long)]
        pr_url: Option<String>,
        #[arg(long)]
        include_archived: Option<bool>,
    },
    Get {
        agent_id: String,
    },
    Archive {
        agent_id: String,
    },
    Unarchive {
        agent_id: String,
    },
    Delete {
        agent_id: String,
    },
    Artifacts {
        agent_id: String,
    },
    ArtifactUrl {
        agent_id: String,
        path: String,
    },
}

#[derive(Debug, Subcommand)]
enum RunsCommand {
    List {
        agent_id: String,
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        cursor: Option<String>,
    },
    Get {
        agent_id: String,
        run_id: String,
    },
    Prompt {
        agent_id: String,
        prompt: String,
    },
    Stream {
        agent_id: String,
        run_id: String,
        #[arg(long)]
        last_event_id: Option<String>,
    },
    Wait {
        agent_id: String,
        run_id: String,
        #[arg(long)]
        last_event_id: Option<String>,
        #[arg(long, default_value_t = 2000)]
        poll_interval_ms: u64,
        #[arg(long)]
        timeout_ms: Option<u64>,
        #[arg(long)]
        max_poll_attempts: Option<u32>,
    },
    Cancel {
        agent_id: String,
        run_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = build_client(cli.api_key)?;
    let output = cli.output;

    match cli.command {
        Command::Me => {
            let me = client.me().await?;
            emit_value(output, &me, || {
                println!("apiKeyName: {}", me.api_key_name);
                println!("createdAt: {}", me.created_at);
                if let Some(user_email) = &me.user_email {
                    println!("userEmail: {user_email}");
                }
                Ok(())
            })?;
        }
        Command::Models => {
            let models = client.list_models().await?;
            emit_value(output, &models, || {
                for model in &models.items {
                    println!("{model}");
                }
                Ok(())
            })?;
        }
        Command::Repositories => {
            let repositories = client.list_repositories().await?;
            emit_value(output, &repositories, || {
                for repo in &repositories.items {
                    println!("{}", repo.url);
                }
                Ok(())
            })?;
        }
        Command::Quickstart(args) => quickstart(&client, output, args).await?,
        Command::Agents(command) => handle_agents(&client, output, command).await?,
        Command::Runs(command) => handle_runs(&client, output, command).await?,
    }

    Ok(())
}

fn build_client(api_key: Option<String>) -> Result<CursorClient> {
    let api_key = match api_key {
        Some(api_key) => api_key,
        None => std::env::var("CURSOR_API_KEY")?,
    };

    let mut builder = CursorClient::builder().api_key(api_key);
    if let Ok(base_url) = std::env::var("CURSOR_BASE_URL") {
        builder = builder.base_url(base_url);
    }

    Ok(builder.build()?)
}

async fn quickstart(client: &CursorClient, output: OutputFormat, args: QuickstartArgs) -> Result<()> {
    let response = client
        .create_agent(&CreateAgentRequest {
            prompt: Prompt {
                text: args.prompt,
                images: Vec::new(),
            },
            model: args.model.map(|id| ModelSelection { id }),
            repos: vec![RepositoryRef {
                url: Some(args.repo_url),
                starting_ref: args.starting_ref,
                pr_url: None,
            }],
            branch_name: args.branch_name,
            auto_generate_branch: None,
            auto_create_pr: Some(args.auto_create_pr),
            skip_reviewer_request: None,
        })
        .await?;

    emit_run_created(output, &response.agent.id, &response.run.id, &response.agent.url)?;
    stream_run(client, output, &response.agent.id, &response.run.id, None).await
}

async fn handle_agents(client: &CursorClient, output: OutputFormat, command: AgentsCommand) -> Result<()> {
    match command {
        AgentsCommand::List {
            limit,
            cursor,
            pr_url,
            include_archived,
        } => {
            let response = client
                .list_agents(&ListAgentsParams {
                    limit,
                    cursor,
                    pr_url,
                    include_archived,
                })
                .await?;

            emit_value(output, &response, || {
                for agent in &response.items {
                    println!("{}\t{}\t{}\t{}", agent.id, agent.status, agent.name, agent.url);
                }

                if let Some(next_cursor) = &response.next_cursor {
                    println!("nextCursor\t{next_cursor}");
                }
                Ok(())
            })?;
        }
        AgentsCommand::Get { agent_id } => {
            let agent = client.get_agent(&agent_id).await?;
            emit_value(output, &agent, || {
                println!("{}", serde_json::to_string_pretty(&agent)?);
                Ok(())
            })?;
        }
        AgentsCommand::Archive { agent_id } => {
            let response = client.archive_agent(&agent_id).await?;
            emit_value(output, &json!({"agentId": agent_id, "archived": true, "response": response}), || {
                println!("archived {agent_id}");
                Ok(())
            })?;
        }
        AgentsCommand::Unarchive { agent_id } => {
            let response = client.unarchive_agent(&agent_id).await?;
            emit_value(output, &json!({"agentId": agent_id, "archived": false, "response": response}), || {
                println!("unarchived {agent_id}");
                Ok(())
            })?;
        }
        AgentsCommand::Delete { agent_id } => {
            client.delete_agent(&agent_id).await?;
            emit_value(output, &json!({"agentId": agent_id, "deleted": true}), || {
                println!("deleted {agent_id}");
                Ok(())
            })?;
        }
        AgentsCommand::Artifacts { agent_id } => {
            let artifacts = client.list_artifacts(&agent_id).await?;
            emit_value(output, &artifacts, || {
                for artifact in &artifacts.items {
                    println!("{}\t{}\t{}", artifact.path, artifact.size_bytes, artifact.updated_at);
                }
                Ok(())
            })?;
        }
        AgentsCommand::ArtifactUrl { agent_id, path } => {
            let artifact = client.download_artifact_url(&agent_id, &path).await?;
            emit_value(output, &artifact, || {
                println!("url: {}", artifact.url);
                println!("expiresAt: {}", artifact.expires_at);
                Ok(())
            })?;
        }
    }

    Ok(())
}

async fn handle_runs(client: &CursorClient, output: OutputFormat, command: RunsCommand) -> Result<()> {
    match command {
        RunsCommand::List {
            agent_id,
            limit,
            cursor,
        } => {
            let response = client
                .list_runs(&agent_id, &ListRunsParams { limit, cursor })
                .await?;

            emit_value(output, &response, || {
                for run in &response.items {
                    println!("{}\t{}\t{}", run.id, run.status, run.updated_at);
                }
                Ok(())
            })?;
        }
        RunsCommand::Get { agent_id, run_id } => {
            let run = client.get_run(&agent_id, &run_id).await?;
            emit_value(output, &run, || {
                println!("{}", serde_json::to_string_pretty(&run)?);
                Ok(())
            })?;
        }
        RunsCommand::Prompt { agent_id, prompt } => {
            let response = client
                .create_run(
                    &agent_id,
                    &CreateRunRequest {
                        prompt: Prompt {
                            text: prompt,
                            images: Vec::new(),
                        },
                    },
                )
                .await?;
            emit_run_created(output, &agent_id, &response.run.id, "")?;
            stream_run(client, output, &agent_id, &response.run.id, None).await?;
        }
        RunsCommand::Stream {
            agent_id,
            run_id,
            last_event_id,
        } => {
            stream_run(client, output, &agent_id, &run_id, last_event_id.as_deref()).await?;
        }
        RunsCommand::Wait {
            agent_id,
            run_id,
            last_event_id,
            poll_interval_ms,
            timeout_ms,
            max_poll_attempts,
        } => {
            let result = client
                .wait_for_run(
                    &agent_id,
                    &run_id,
                    WaitForRunOptions {
                        last_event_id,
                        poll_interval: Duration::from_millis(poll_interval_ms),
                        timeout: timeout_ms.map(Duration::from_millis),
                        max_poll_attempts,
                    },
                )
                .await?;

            match output {
                OutputFormat::Text => {
                    for message in &result.stream_messages {
                        render_stream_message_text(message)?;
                    }
                    eprintln!("[final-status] {}", result.run.status);
                    if let Some(last_event_id) = &result.last_event_id {
                        eprintln!("[last-event-id] {last_event_id}");
                    }
                    if result.used_polling_fallback {
                        eprintln!("[fallback] polling");
                    }
                }
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string(&json_wait_result(&result))?);
                }
            }
        }
        RunsCommand::Cancel { agent_id, run_id } => {
            let response = client.cancel_run(&agent_id, &run_id).await?;
            emit_value(output, &json!({"agentId": agent_id, "runId": run_id, "response": response}), || {
                println!("cancel requested for {run_id}");
                Ok(())
            })?;
        }
    }

    Ok(())
}

async fn stream_run(
    client: &CursorClient,
    output: OutputFormat,
    agent_id: &str,
    run_id: &str,
    last_event_id: Option<&str>,
) -> Result<()> {
    let mut stream = client.stream_run(agent_id, run_id, last_event_id).await?;
    let mut saw_terminal = false;

    while let Some(message) = stream.next().await {
        let message = message?;
        if let RunStreamEvent::Result { .. } = message.event {
            saw_terminal = true;
        }

        match output {
            OutputFormat::Text => render_stream_message_text(&message)?,
            OutputFormat::Json => {
                println!("{}", serde_json::to_string(&json_stream_message(&message))?);
            }
        }

        if matches!(message.event, RunStreamEvent::Done) {
            break;
        }
    }

    if !saw_terminal {
        let run = client.get_run(agent_id, run_id).await?;
        match output {
            OutputFormat::Text => eprintln!("[final-status] {}", run.status),
            OutputFormat::Json => {
                println!("{}", serde_json::to_string(&json_final_status(&run))?);
            }
        }
    }

    Ok(())
}

fn json_stream_message(message: &RunStreamMessage) -> serde_json::Value {
    json!({
        "type": "stream_event",
        "id": message.id,
        "event": message.event,
    })
}

fn json_final_status(run: &cursor_sdk::Run) -> serde_json::Value {
    json!({
        "type": "final_status",
        "run": run,
    })
}

fn json_wait_result(result: &cursor_sdk::WaitForRunResult) -> serde_json::Value {
    json!({
        "type": "wait_result",
        "run": result.run,
        "streamMessages": result.stream_messages,
        "lastEventId": result.last_event_id,
        "usedPollingFallback": result.used_polling_fallback,
    })
}

fn emit_value<T, F>(output: OutputFormat, value: &T, render_text: F) -> Result<()>
where
    T: Serialize,
    F: FnOnce() -> Result<()>,
{
    match output {
        OutputFormat::Text => render_text(),
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(value)?);
            Ok(())
        }
    }
}

fn emit_run_created(
    output: OutputFormat,
    agent_id: &str,
    run_id: &str,
    agent_url: &str,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("agentId: {agent_id}");
            println!("runId: {run_id}");
            if !agent_url.is_empty() {
                println!("agentUrl: {agent_url}");
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "type": "run_created",
                    "agentId": agent_id,
                    "runId": run_id,
                    "agentUrl": if agent_url.is_empty() { None::<String> } else { Some(agent_url.to_owned()) },
                }))?
            );
        }
    }

    Ok(())
}

fn render_stream_message_text(message: &RunStreamMessage) -> Result<()> {
    match &message.event {
        RunStreamEvent::Assistant { text } => {
            print!("{text}");
        }
        RunStreamEvent::Thinking { text } => {
            eprintln!("[thinking] {text}");
        }
        RunStreamEvent::Status { status, .. } => {
            eprintln!("[status] {status}");
        }
        RunStreamEvent::ToolCall { payload } => {
            if let Some(id) = &message.id {
                eprintln!("[tool_call:{id}] {}", serde_json::to_string(payload)?);
            } else {
                eprintln!("[tool_call] {}", serde_json::to_string(payload)?);
            }
        }
        RunStreamEvent::Result { status, .. } => {
            if let Some(id) = &message.id {
                eprintln!("\n[result:{id}] {status}");
            } else {
                eprintln!("\n[result] {status}");
            }
        }
        RunStreamEvent::Error { code, message } => {
            let prefix = code.clone().unwrap_or_else(|| "stream_error".to_owned());
            bail!("{prefix}: {message}");
        }
        RunStreamEvent::Done => {
            if let Some(id) = &message.id {
                eprintln!("[done:{id}]");
            }
        }
        RunStreamEvent::Heartbeat => {}
        RunStreamEvent::Unknown { name, payload } => {
            if let Some(id) = &message.id {
                eprintln!("[unknown:{id}] {name} {}", serde_json::to_string(payload)?);
            } else {
                eprintln!("[unknown] {name} {}", serde_json::to_string(payload)?);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{json_final_status, json_stream_message, json_wait_result};
    use cursor_sdk::{Run, RunStreamEvent, RunStreamMessage, WaitForRunResult};
    use serde_json::json;

    fn sample_run(status: &str) -> Run {
        Run {
            id: "run-1".to_owned(),
            agent_id: "bc-1".to_owned(),
            status: status.to_owned(),
            created_at: "2026-04-13T18:30:00.000Z".to_owned(),
            updated_at: "2026-04-13T18:30:00.000Z".to_owned(),
        }
    }

    #[test]
    fn json_stream_message_shape_is_stable() {
        let value = json_stream_message(&RunStreamMessage {
            id: Some("evt-1".to_owned()),
            event: RunStreamEvent::Assistant {
                text: "hello".to_owned(),
            },
        });

        assert_eq!(
            value,
            json!({
                "type": "stream_event",
                "id": "evt-1",
                "event": {
                    "Assistant": {
                        "text": "hello"
                    }
                }
            })
        );
    }

    #[test]
    fn json_final_status_shape_is_stable() {
        let value = json_final_status(&sample_run("FINISHED"));

        assert_eq!(value["type"], "final_status");
        assert_eq!(value["run"]["status"], "FINISHED");
    }

    #[test]
    fn json_wait_result_shape_is_stable() {
        let value = json_wait_result(&WaitForRunResult {
            run: sample_run("FINISHED"),
            stream_messages: vec![RunStreamMessage {
                id: Some("evt-9".to_owned()),
                event: RunStreamEvent::Done,
            }],
            last_event_id: Some("evt-9".to_owned()),
            used_polling_fallback: true,
        });

        assert_eq!(value["type"], "wait_result");
        assert_eq!(value["run"]["status"], "FINISHED");
        assert_eq!(value["lastEventId"], "evt-9");
        assert_eq!(value["usedPollingFallback"], true);
        assert_eq!(value["streamMessages"][0]["id"], "evt-9");
    }
}