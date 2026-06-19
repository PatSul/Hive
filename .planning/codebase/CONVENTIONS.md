# Coding Conventions

**Analysis Date:** 2026-06-18

## Naming Patterns

**Files:**
- Rust source files use `snake_case.rs`.
- UI action handlers commonly use `{area}_actions.rs`, such as `chat_actions.rs` and `settings_actions.rs`.
- Integration tests in UI crates commonly use `test_{area}.rs`, such as `test_sidebar.rs`.
- Product docs and plans use Markdown, often uppercase for canonical docs and dated kebab-case for plans.

**Functions:**
- Rust functions use `snake_case`.
- UI event handlers usually use `handle_{action}`.
- Constructors are typically `new()`, `open()`, `load()`, `load_or_create()`, or `with_*()` depending on ownership and persistence.

**Variables:**
- Rust variables use `snake_case`.
- Constants use `UPPER_SNAKE_CASE`, such as `VERSION`, `MAX_RECENT_WORKSPACES`, and `DEFAULT_TOKEN_BUDGET`.
- GPUI global wrappers use PascalCase newtypes prefixed with `App`, such as `AppAiService`.

**Types:**
- Structs, enums, traits, and actions use `PascalCase`.
- Crates and modules use `snake_case`.
- Public crate APIs are re-exported from `lib.rs` when intended for cross-crate use.

## Code Style

**Formatting:**
- Rust code should be formatted with `cargo fmt`.
- Follow idiomatic Rust 2024 edition style.
- GPUI render functions are written as fluent builder chains.
- Long UI builder blocks are common; prefer extracting helpers when a block becomes hard to scan.

**Linting:**
- Use `cargo clippy` on affected crate slices when practical.
- The repository docs emphasize warning cleanup on validated crate slices, not assuming the entire workspace is always warning-free.

## Import Organization

**Order:**
1. Standard library imports.
2. External crate imports.
3. Internal workspace crate imports.
4. Local `crate::` or `super::` imports.

**Grouping:**
- Existing files generally group imports by source, but do not enforce strict alphabetical sorting everywhere.
- Large UI modules often group imports by domain to make action/type imports manageable.

**Path Aliases:**
- Rust uses crate paths and module visibility rather than aliases.
- Workspace crates are addressed by crate name, such as `hive_ai`, `hive_core`, and `hive_ui_panels`.

## Error Handling

**Patterns:**
- Use `anyhow::Result` for application/bootstrap flows that aggregate many error types.
- Use `thiserror` for domain-specific error enums where callers need structured handling.
- Optional integrations should fail closed or skip with a warning when credentials/config are absent.
- User-visible failures are often logged and pushed through `AppNotification`.

**Error Types:**
- Throw/return errors for invalid config, failed persistence, failed network calls, and security denials.
- Return graceful fallbacks for optional UI data, provider discovery, local model discovery, and degraded external services when the app can continue.

## Logging

**Framework:**
- `tracing` is the standard logging framework.
- Common levels: `info`, `warn`, `error`.

**Patterns:**
- Log startup milestones and service initialization.
- Log integration registration/skip decisions without leaking secrets.
- Log background task failures with context.
- Avoid printing real secrets or tokens.

## Comments

**When to Comment:**
- Explain non-obvious GPUI lifecycle constraints, thread/async boundaries, security decisions, and platform workarounds.
- Avoid comments that just restate simple assignments.
- Existing code uses short comments to mark large sections in complex bootstrap files.

**TODO Comments:**
- Use precise TODOs when tracking a known follow-up, ideally with the owning module or plan.
- Broader enhancements belong in planning docs rather than scattered comments.

## Function Design

**Size:**
- Keep new functions focused when possible.
- Large existing functions such as app bootstrap are legacy aggregation points; avoid making them larger unless required.
- Extract render helpers for repeated GPUI UI patterns.

**Parameters:**
- Prefer explicit typed structs for cross-module data.
- GPUI handlers conventionally receive `&mut Context<T>`, `&mut Window`, action refs, and state refs as required by the framework.

**Return Values:**
- Use `Result<T>` for fallible operations.
- Use domain result structs for agent/orchestration output.
- UI render helpers return `impl IntoElement`, `AnyElement`, or GPUI element types.

## Module Design

**Exports:**
- Keep crate public surfaces in `lib.rs`.
- Re-export stable cross-crate APIs; keep implementation details module-private where possible.
- Use `pub(super)` for workspace UI helpers meant only for neighboring modules.

**Barrel Files:**
- `lib.rs` is the crate-level public API.
- `mod.rs` is used for some nested module directories.
- Avoid new circular dependencies between workspace crates; use traits in lower-level crates when bridging would otherwise cycle.

---

*Convention analysis: 2026-06-18*
*Update when formatting/linting or module patterns change*
