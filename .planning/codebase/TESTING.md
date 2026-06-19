# Testing Patterns

**Analysis Date:** 2026-06-18

## Test Framework

**Runner:**
- Rust built-in test harness via `cargo test`.
- `tokio::test` for async tests.
- CI uses targeted verification scripts rather than a blanket `cargo test --workspace` as the primary gate.

**Assertion Library:**
- Rust standard assertions: `assert!`, `assert_eq!`, `assert_ne!`.
- HTTP/service tests use crate-specific helpers and `reqwest`, `tower`, or Axum utilities where needed.

**Run Commands:**

```bash
cd hive
./verify.sh
verify.bat
cargo check -p hive_cloud -p hive_admin -p hive_terminal -p hive_blockchain -p hive_ui_panels -p hive_ui -p hive_app
cargo test -p hive_core -p hive_agents -q
cargo test -p hive_a2a -p hive_cloud -p hive_admin -p hive_cli -p hive_terminal -p hive_blockchain -q
cargo test -p hive_ui --test test_token_launch -q
```

## Test File Organization

**Location:**
- Integration tests live in `hive/crates/{crate}/tests/*.rs`.
- Unit tests live inline in source files behind `#[cfg(test)] mod tests`.
- UI panel tests live in `hive/crates/hive_ui/tests/` and `hive/crates/hive_ui_panels/tests/`.

**Naming:**
- Integration test files often use `test_{feature}.rs` in UI crates.
- Backend/service tests use feature names such as `protocol_tests.rs`, `relay_tests.rs`, and `daemon_tests.rs`.

**Structure:**

```text
hive/crates/
|-- hive_core/src/security.rs          # inline #[cfg(test)] tests
|-- hive_ai/tests/memory_store_tests.rs
|-- hive_agents/tests/tiered_memory_tests.rs
|-- hive_remote/tests/protocol_tests.rs
|-- hive_ui/tests/test_workspace.rs
`-- hive_ui_panels/tests/test_chat.rs
```

## Test Structure

**Suite Organization:**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_expected_case() {
        let result = function_under_test();
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn handles_async_case() {
        let result = async_function().await.unwrap();
        assert!(result.is_valid());
    }
}
```

**Patterns:**
- Prefer focused unit tests for pure logic, parsers, security checks, and persistence helpers.
- Use integration tests for HTTP routes, remote protocol behavior, memory stores, and UI panel data flows.
- Use `tempfile` for filesystem-isolated tests.

## Mocking

**Framework:**
- No central mocking framework is used.
- Tests commonly use mock structs, local test servers, temp directories, and fake providers.

**Patterns:**

```rust
struct MockExecutor;

#[async_trait::async_trait]
impl SomeExecutor for MockExecutor {
    async fn execute(&self, input: Input) -> Result<Output> {
        Ok(Output::default())
    }
}
```

**What to Mock:**
- External AI providers.
- HTTP services and API clients.
- Filesystem paths with temp directories.
- Time-sensitive scheduler state when practical.

**What NOT to Mock:**
- SecurityGateway pattern checks.
- Serialization/deserialization of persisted config.
- Pure routing and parsing logic.

## Fixtures and Factories

**Test Data:**
- Inline factories are common in tests.
- `tempfile` is used for ephemeral paths.
- HTTP tests often bind local listeners or use Axum/Tower test utilities.

**Location:**
- Most fixtures are local to test files.
- Shared test fixtures are not centralized across the entire workspace.

## Coverage

**Requirements:**
- No single coverage percentage is enforced in local scripts.
- `docs/TEST_PLAN.md` tracks important crate slices and test areas.
- CI runs the current targeted MVP matrix on Windows, macOS, and Linux.

**Configuration:**
- No repository-level coverage config was found.

## Test Types

**Unit Tests:**
- Security validation, config serialization, storage helpers, schedulers, theme logic, parsing, routing, cost accounting, and pure data transforms.

**Integration Tests:**
- A2A HTTP/SSE behavior, remote web API/session/relay/pairing, AI memory stores, CLI/cloud/admin/service slices.

**UI Tests:**
- Rust tests validate panel data and selected UI behaviors.
- Full visual GPUI validation remains manual unless a future harness is added.

**Manual/E2E Tests:**
- Documented in `docs/TEST_PLAN.md`.
- Provider tests require real API keys and should not be run blindly.

## Common Patterns

**Async Testing:**

```rust
#[tokio::test]
async fn endpoint_returns_expected_response() {
    let response = client.get(url).send().await.unwrap();
    assert!(response.status().is_success());
}
```

**Error Testing:**

```rust
let err = fallible_operation().unwrap_err();
assert!(err.to_string().contains("expected message"));
```

**Security Regression Testing:**
- Add regression tests near the relevant scanner or gateway module.
- Include both blocked and allowed examples so guardrails do not over-block.

---

*Testing analysis: 2026-06-18*
*Update when the validation matrix changes*
