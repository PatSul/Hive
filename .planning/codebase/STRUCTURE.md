# Codebase Structure

**Analysis Date:** 2026-06-18

## Directory Layout

```text
Hive/
|-- .github/workflows/        # CI, release, and auto-release workflows
|-- docs/                     # Architecture, feature, strategy, and test docs
|-- hive/                     # Rust workspace root
|   |-- crates/               # Workspace crates
|   |-- assets/               # Embedded icons/images
|   |-- docker/               # Sandbox Dockerfile
|   |-- docs/                 # Crate-adjacent planning docs
|   |-- Cargo.toml            # Workspace manifest
|   |-- Cargo.lock            # Locked dependency graph
|   |-- build.*               # Cargo wrapper scripts
|   `-- verify.*              # Targeted validation scripts
|-- scripts/                  # Install scripts
|-- specs/                    # Specify/GSD-style feature specs
|-- .planning/codebase/       # Generated codebase map
|-- README.md                 # Product and build documentation
|-- SECURITY.md               # Security policy
`-- run_verify.bat            # Local wrapper around verify.bat
```

## Directory Purposes

**`hive/crates/`:**
- Purpose: Main Rust source tree.
- Contains: 22 workspace crates in this active checkout.
- Key files: `hive/crates/hive_app/src/main.rs`, `hive/crates/hive_ui/src/workspace.rs`, `hive/crates/hive_core/src/lib.rs`, `hive/crates/hive_ai/src/lib.rs`, `hive/crates/hive_agents/src/lib.rs`.
- Subdirectories: One crate per product subsystem.

**`hive/assets/`:**
- Purpose: Embedded desktop assets.
- Contains: `hive_bee.png` and many SVG icons.
- Key files: `hive/assets/hive_bee.png`, `hive/assets/icons/*.svg`.

**`hive/crates/hive_ui/src/workspace/`:**
- Purpose: Main workspace action handlers and shell modules.
- Contains: `*_actions.rs`, `navigation.rs`, `panel_router.rs`, `sidebar_shell.rs`, `context_rail.rs`, `terminal_host.rs`.
- Key files: `hive/crates/hive_ui/src/workspace/chat_actions.rs`, `hive/crates/hive_ui/src/workspace/review_actions.rs`.

**`hive/crates/hive_ui_panels/src/panels/`:**
- Purpose: Individual GPUI panel implementations.
- Contains: `chat.rs`, `files.rs`, `review.rs`, `agents.rs`, `workflow_builder.rs`, `settings.rs`, and other panels.
- Key files: `hive/crates/hive_ui_panels/src/panels/mod.rs`.

**`docs/` and `specs/`:**
- Purpose: Product plans, feature documentation, test plan, and spec-driven development artifacts.
- Contains: Markdown docs and feature specs.
- Key files: `docs/TEST_PLAN.md`, `docs/FEATURE_DOCUMENTATION.md`, `specs/002-learning-cortex/`.

**`.github/workflows/`:**
- Purpose: GitHub Actions automation.
- Contains: `ci.yml`, `release.yml`, `auto-release.yml`.

## Key File Locations

**Entry Points:**
- `hive/crates/hive_app/src/main.rs` - Desktop app entry point and service bootstrap.
- `hive/crates/hive_cli/src/main.rs` - Terminal CLI entry point.
- `hive/crates/hive_admin/src/main.rs` - Admin TUI entry point.
- `hive/crates/hive_cloud/src/main.rs` - Axum cloud service entry point.

**Configuration:**
- `hive/Cargo.toml` - Workspace members and shared dependencies.
- `hive/crates/*/Cargo.toml` - Crate-level dependencies and binary names.
- `.github/workflows/ci.yml` - Supported CI verification path.
- `hive/build.bat`, `hive/build.sh` - Local cargo wrappers.
- `hive/verify.bat`, `hive/verify.sh` - Targeted local verification.

**Core Logic:**
- `hive/crates/hive_core/src/` - Config, persistence, security, scheduling, notifications, channels, kanban.
- `hive/crates/hive_ai/src/` - AI providers, routing, context, RAG, memory, embeddings, cost.
- `hive/crates/hive_agents/src/` - Agent orchestration, skills, workflows, MCP, worktrees, mission automation.
- `hive/crates/hive_integrations/src/` - Third-party integrations.
- `hive/crates/hive_remote/src/` - Remote daemon/API/web UI.

**Testing:**
- `hive/crates/*/tests/*.rs` - Integration tests.
- `hive/crates/**/src/*.rs` with `#[cfg(test)]` - Unit tests.
- `docs/TEST_PLAN.md` - Current targeted test matrix and command examples.
- `hive/verify.*` - CI-aligned validation entry points.

**Documentation:**
- `README.md` - Product overview, architecture, installation, and release info.
- `docs/FEATURE_DOCUMENTATION.md` - Feature inventory and crate summaries.
- `docs/HIVE_PLATFORM_VISION.md` - Product strategy and architecture narrative.
- `SECURITY.md` - Security reporting and posture.

## Naming Conventions

**Files:**
- Rust modules use `snake_case.rs`.
- Test files use `test_*.rs` in crate `tests/` directories or inline `#[cfg(test)] mod tests`.
- Markdown docs use either uppercase project docs (`README.md`, `SECURITY.md`) or kebab-case dated plans.
- Built-in skill files use `.toml` under `hive/crates/hive_agents/src/builtin_skills/`.

**Directories:**
- Crates are named `hive_*`.
- Workspace action modules are named by concern plus `_actions.rs` when they handle UI actions.
- Feature specs use numbered directories, such as `specs/002-learning-cortex/`.

**Special Patterns:**
- `lib.rs` re-exports public crate APIs.
- `main.rs` marks binary crate entry points.
- `mod.rs` is used in some nested module directories, but many modules are flat files.

## Where to Add New Code

**New desktop panel:**
- Primary code: `hive/crates/hive_ui_panels/src/panels/{panel}.rs`.
- Shell wiring: `hive/crates/hive_ui/src/workspace.rs`, `hive/crates/hive_ui/src/workspace/navigation.rs`, and `hive/crates/hive_ui/src/workspace/panel_router.rs`.
- Shared actions/types: `hive/crates/hive_ui_core/src/`.
- Tests: `hive/crates/hive_ui_panels/tests/` or `hive/crates/hive_ui/tests/`.

**New backend/service capability:**
- Core persistent state: `hive/crates/hive_core/src/`.
- AI/model behavior: `hive/crates/hive_ai/src/`.
- Agent orchestration: `hive/crates/hive_agents/src/`.
- Third-party API: `hive/crates/hive_integrations/src/`.

**New external surface:**
- Remote control: `hive/crates/hive_remote/src/`.
- A2A protocol: `hive/crates/hive_a2a/src/`.
- Cloud API: `hive/crates/hive_cloud/src/`.
- CLI command: `hive/crates/hive_cli/src/commands/`.

**New tests:**
- Unit tests beside module code in `#[cfg(test)]`.
- Integration tests in `hive/crates/{crate}/tests/`.
- Add new validation slices to `docs/TEST_PLAN.md` and `hive/verify.*` only when they are reliable in CI.

## Special Directories

**`.planning/codebase/`:**
- Purpose: GSD-generated codebase map.
- Source: Created by codebase mapping.
- Committed: Should be committed as planning metadata.

**`hive/target/`:**
- Purpose: Cargo build output.
- Source: Generated by cargo.
- Committed: No.

**`hive/crates/hive_remote/web/`:**
- Purpose: Static web UI for remote control.
- Source: Checked-in HTML/CSS/JS.
- Committed: Yes.

**`hive/docker/sandbox/`:**
- Purpose: Docker sandbox support for safer execution.
- Source: Checked-in Dockerfile.
- Committed: Yes.

---

*Structure analysis: 2026-06-18*
*Update when crate layout or major directories change*
