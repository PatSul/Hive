# Quickstart: Wire All Unwired Features

**Branch**: 001-wire-unwired-features

## Prerequisites

- Rust toolchain (stable)
- VS Developer Command Prompt (or INCLUDE/LIB env vars set)
- Working `cargo build` from `hive/` directory

## Verify Current State

```bash
cd hive
cargo test --workspace --exclude hive_app
```

All existing tests must pass before starting.

## Changes Summary

1. **hive_terminal/src/lib.rs** — Add `pub use local_ai::{...}` exports
2. **hive_agents/src/integration_tools.rs** — Add 7 new MCP tool handlers (search_knowledge, wallet_create, wallet_list, wallet_balance, run_workflow, list_workflows, describe_workflow) + wire token_deploy stubs
3. **hive_app/src/main.rs** — Wire Queen with `.with_rag()` call

## Verify After Changes

```bash
cd hive
cargo build
cargo test --workspace --exclude hive_app
```

Both must succeed with zero regressions.

## Test New Tools

After building, launch the app and verify via AI chat:
- "Search my knowledge base for X" → triggers search_knowledge tool
- "List my wallets" → triggers wallet_list tool
- "List available workflows" → triggers list_workflows tool
- "Run the build-and-test workflow" → triggers run_workflow tool
