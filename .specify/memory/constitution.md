<!--
  Sync Impact Report
  ===================
  Version change: 0.0.0 → 1.0.0 (MAJOR — initial ratification)

  Added principles:
    - I. Native Performance First
    - II. Security Gate (NON-NEGOTIABLE)
    - III. AI Integration Quality
    - IV. Simplicity & Elegance
    - V. Comprehensive Testing
    - VI. UX Consistency

  Added sections:
    - Technology Constraints
    - Development Workflow
    - Governance

  Templates requiring updates:
    ✅ .specify/templates/plan-template.md — no changes needed (Constitution Check is generic)
    ✅ .specify/templates/spec-template.md — no changes needed (section structure compatible)
    ✅ .specify/templates/tasks-template.md — no changes needed (phase structure compatible)

  Deferred items: None
-->

# Hive Constitution

## Core Principles

### I. Native Performance First

Every feature MUST target native desktop performance through Rust and GPUI.

- All UI rendering MUST use GPUI's retained-mode framework — no web views, no Electron, no embedded browsers for core UI.
- Allocations in hot paths (render loops, input handling, streaming) MUST be minimized. Prefer stack allocation, arena patterns, and borrowed references.
- Async operations MUST NOT block the main thread. Use GPUI tasks, background executors, or `spawn_blocking` for I/O and computation.
- New dependencies MUST be evaluated for binary size, compile time, and runtime overhead before adoption. Prefer well-maintained crates with minimal transitive dependencies.
- Frame budget: UI interactions MUST remain responsive at 60 fps. Throttle expensive operations (status bar updates at 500ms, discovery scans at 120s).

### II. Security Gate (NON-NEGOTIABLE)

All code changes MUST pass the security gate defined in CLAUDE.md §7 before completion.

- **Shell commands**: Every command MUST route through `SecurityGateway::check_command()`. Direct `Command::new(user_input)` is forbidden.
- **User input**: All user-supplied strings MUST be validated and sanitized before use in shell commands, file paths, or queries. No `format!("...{user_input}...")` in executable contexts.
- **File paths**: MUST be canonicalized and validated. Block system roots (`/`, `C:\`) and sensitive directories (`/.ssh`, `/.aws`, `/.gnupg`).
- **Network**: HTTPS only. No user-controlled URLs without domain allowlist. Block private IP ranges and metadata endpoints (`169.254.169.254`).
- **API keys**: Always in headers or OS keychain, never in URL query parameters or plaintext config files.
- **AI content**: All AI-generated text is untrusted input. Sanitize before rendering. Skill instructions MUST be validated and integrity-checked.
- **Patterns that MUST NEVER appear**: `Command::new(user_input)`, `std::process::Command` without SecurityGateway validation, `format!()` with user input in shell/SQL contexts.

### III. AI Integration Quality

AI features MUST deliver production-grade streaming, multi-provider support, and graceful degradation.

- All LLM interactions MUST stream responses token-by-token to the UI. No blocking calls that wait for full completions.
- Provider switching MUST be seamless — the same conversation MUST work across Anthropic, OpenAI, Ollama, and other configured providers without data loss.
- AI failures (network errors, rate limits, model unavailability) MUST degrade gracefully with user-visible error messages and retry options. Silent failures are forbidden.
- Tool calling, structured output, and multi-step reasoning MUST follow the patterns established in `hive_ai` — no ad-hoc implementations in UI crates.
- Token usage, cost estimates, and model metadata MUST be surfaced to the user when available.

### IV. Simplicity & Elegance

Every change MUST be as simple as possible. Reject nested code, premature abstractions, and unnecessary indirection.

- Prefer deletion over refactoring. Prefer fixes over new features. Three similar lines are better than a premature abstraction.
- No helper utilities, wrapper types, or design patterns for one-time operations. Introduce abstractions only when the same pattern appears in 3+ distinct call sites.
- No dead code. If a function, struct, or module has zero callers, delete it. No `_unused` prefixes, no commented-out blocks, no "might need later" retention.
- Implementations MUST be complete — no `todo!()` in shipped code, no `...` ellipsis, no "you do this part" comments, no skipped edge cases.
- Bug fixes MUST be surgical. Fix the bug, add a test for it, leave surrounding code untouched unless the fix requires structural changes.

### V. Comprehensive Testing

Every feature MUST be verified with tests before it ships. Trust evidence, not assumptions.

- New functionality MUST include tests that exercise the happy path and at least one error/edge case.
- Bug fixes MUST include a regression test that fails without the fix and passes with it.
- Test files for `hive_ui_panels` MUST be in external test files (not inline) to avoid rustc stack overflow on the 20k-line crate.
- `gpui_component::IconName` has no `PartialEq`/`Debug` — use `matches!()` not `assert_eq!()`.
- `cargo test --workspace --exclude hive_app` MUST pass before any feature is considered complete.
- Never mark a task as done without running the tests and confirming they pass.

### VI. UX Consistency

The desktop app MUST present a unified, coherent experience across all panels and workflows.

- Accent color: `#00D4FF` (cyan). All interactive highlights, selection indicators, and focus rings MUST use this color.
- All panels MUST follow the workspace layout patterns established in `hive_ui` — sidebar navigation, content area, status bar.
- Loading states, error states, and empty states MUST be handled explicitly in every view. No blank screens, no unhandled errors, no missing placeholders.
- Keyboard navigation MUST work for all primary workflows. Mouse-only interactions are a bug.
- Configuration lives in `~/.hive/`. Migration from `~/.hivecode/` MUST be handled transparently on startup.

## Technology Constraints

- **Language**: Rust (latest stable)
- **UI Framework**: GPUI (Zed's GPU-accelerated UI framework)
- **Build**: `cargo build` / `cargo test` from `hive/` directory
- **Platform**: Windows (primary), macOS/Linux (future)
- **Build requirement**: VS Developer Command Prompt or INCLUDE/LIB environment variables on Windows
- **Crate workspace**: All code lives under `hive/crates/` with clear crate boundaries (see CLAUDE.md for layout)
- **Config directory**: `~/.hive/`
- **regex crate limitation**: No lookahead/lookbehind support — use alternative parsing strategies

## Development Workflow

- **Plan before code**: Enter plan mode for any non-trivial task (3+ steps or architectural decisions).
- **Subagents for research**: Offload exploration and parallel analysis to subagents. Keep the main context clean.
- **Self-improvement loop**: After any correction, update `instructions.md` or CLAUDE.md so the mistake only happens once.
- **Verification before done**: Never mark something complete without proving it works. Run tests, check logs, collect evidence.
- **Version increment on push**: Always bump the version before pushing to main.
- **Security gate on every change**: The §7 checklist in CLAUDE.md is mandatory for all code modifications.

## Governance

- This constitution supersedes all other development practices for the Hive project. When in conflict, constitution principles take precedence.
- Amendments require: (1) documented rationale, (2) review of impact on existing code, (3) update to this file with version increment.
- Version follows semantic versioning: MAJOR for principle removals/redefinitions, MINOR for new principles or material expansions, PATCH for clarifications and wording fixes.
- All code reviews MUST verify compliance with applicable principles. Complexity beyond what the task requires MUST be justified in the PR description.
- Use CLAUDE.md as the runtime development guidance file — it contains the operational checklist that implements these principles.

**Version**: 1.0.0 | **Ratified**: 2026-03-22 | **Last Amended**: 2026-03-22
