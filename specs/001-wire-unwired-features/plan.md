# Implementation Plan: Wire All Unwired Features

**Branch**: `001-wire-unwired-features` | **Date**: 2026-03-22 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-wire-unwired-features/spec.md`

## Summary

Wire 5 remaining gaps in the Hive desktop app to achieve 100% functional coverage. Research revealed that RAG/semantic search is already integrated in the chat pipeline (L2 tier) — the actual gaps are: (1) missing `pub use` exports for OllamaManager, (2) missing MCP tool handler for RAG search, (3) Queen never receives RAG service, (4) blockchain deploy tools are stubs, (5) no MCP tools for workflow execution. All services are instantiated; this is purely connecting existing implementations to their consumers.

## Technical Context

**Language/Version**: Rust (stable)
**Primary Dependencies**: GPUI, serde_json, tokio (async for blockchain calls)
**Storage**: ~/.hive/ (config.toml, wallets.enc, workflows/, kanban.json)
**Testing**: cargo test --workspace --exclude hive_app
**Target Platform**: Windows (primary), macOS/Linux (future)
**Project Type**: Desktop application
**Performance Goals**: 60 fps UI, non-blocking tool handlers
**Constraints**: All blockchain operations must be async (network calls); workflow execution on background thread
**Scale/Scope**: ~7 files modified, ~300 lines added, 0 new crates

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Native Performance | PASS | All wiring uses existing GPUI patterns; async for I/O |
| II. Security Gate | PASS | No new shell commands; blockchain key access requires password; no user input in format! macros |
| III. AI Integration Quality | PASS | RAG tool adds context enrichment; tool handlers follow established patterns |
| IV. Simplicity & Elegance | PASS | Pure wiring — no new abstractions, no new crates, no wrapper types |
| V. Comprehensive Testing | PASS | Will add tests for new tool handlers; verify zero regressions |
| VI. UX Consistency | PASS | No UI changes (tools exposed via AI chat, existing patterns) |

## Project Structure

### Documentation (this feature)

```text
specs/001-wire-unwired-features/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── mcp-tools.md     # MCP tool handler contracts
└── tasks.md             # Phase 2 output (created by /speckit.tasks)
```

### Source Code (files to modify)

```text
hive/crates/
├── hive_terminal/src/lib.rs              # Add pub use exports for local_ai
├── hive_agents/src/integration_tools.rs  # Add 7 new MCP tool handlers + wire 2 stubs
├── hive_app/src/main.rs                  # Wire Queen .with_rag()
└── tests/                                # New tool handler tests
```

**Structure Decision**: Existing Rust workspace crate layout. No new crates, modules, or files needed. All changes are additions to existing files following established patterns.

## Complexity Tracking

> No constitution violations. All changes are minimal wiring using established patterns.
