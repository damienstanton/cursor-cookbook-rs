# cursor-cookbook

An idiomatic Rust 2024 workspace inspired by [cursor/cookbook](https://github.com/cursor/cookbook).

This repo mirrors the upstream split between a reusable SDK surface and small runnable examples:

- `crates/cursor-sdk`: async Rust client for Cursor's documented Cloud Agents API.
- `crates/cursor-cookbook-cli`: a small CLI that demonstrates the core flows.

## Scope

Cursor's public Rust surface today is the Cloud Agents HTTP API. The TypeScript SDK also supports the local runtime, but that runtime is not documented as a public protocol, so this workspace implements the documented cloud features cleanly instead of guessing at private internals.

Implemented today:

- API key inspection via `GET /v1/me`
- model and repository discovery
- agent creation, listing, lookup, archive lifecycle hooks
- follow-up runs, run lookup, cancellation
- run streaming via server-sent events
- artifact listing and download URL lookup

## Quickstart

Export a Cursor API key from the integrations dashboard:

```bash
export CURSOR_API_KEY="crsr_..."
```

List models:

```bash
cargo run -p cursor-cookbook-cli -- models
```

Create a cloud agent and stream its initial run:

```bash
cargo run -p cursor-cookbook-cli -- quickstart \
  --repo-url https://github.com/cursor/cookbook \
  --starting-ref main \
  "Summarize the structure of this repository"
```

Send a follow-up prompt to an existing agent:

```bash
cargo run -p cursor-cookbook-cli -- runs prompt bc-... "Also list the most relevant examples"
```

Emit structured JSON instead of text:

```bash
cargo run -p cursor-cookbook-cli -- --output json models
```

Resume a wait after a dropped SSE connection by passing the last seen event id:

```bash
cargo run -p cursor-cookbook-cli -- runs wait bc-... run-... \
  --last-event-id 1713033010000-0 \
  --poll-interval-ms 1000 \
  --timeout-ms 300000
```

Cap fallback polling attempts instead of waiting indefinitely:

```bash
cargo run -p cursor-cookbook-cli -- runs wait bc-... run-... \
  --max-poll-attempts 30 \
  --poll-interval-ms 2000
```

## Workspace layout

```text
crates/
  cursor-sdk/
    examples/
      quickstart.rs
    src/
      client.rs
      error.rs
      lib.rs
      types.rs
  cursor-cookbook-cli/
    src/
      main.rs
```