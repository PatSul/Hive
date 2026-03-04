---
name: Hive Core & Async
description: Best practices for async Rust, error handling, and database integration in Hive.
---

# Hive Core Backend

## Async Runtime
- **Tokio**: We use the full `tokio` feature set.
- Long-running CPU-bound tasks or blocking I/O (like SQLite queries) must be spawned on `tokio::task::spawn_blocking` to avoid stalling the executor.
- Communication with GPUI must use channels (`tx`/`rx`) or the contextual `app.update`.

## Database (rusqlite)
- Database schema and migrations are handled locally in SQLite.
- Connection accesses must be safe. Avoid concurrent mutating writes from multiple async threads without a connection pool or lock.

## Error Handling
- Use `anyhow::Result` for application-level errors and rapid prototyping.
- Use `thiserror` (e.g. `#[derive(thiserror::Error)]`) for library crates and explicitly typed errors.
- Prefer explicit `.map_err(|e| format!("...: {e}"))` when passing errors to MCP handlers so the agent sees a descriptive string.

## Cross-cutting concerns
- **Security**: Be very strict on `CommandExecutor` cwd paths. Use `resolve_path`.
- **Logging**: Use the `tracing` crate (`info!`, `error!`, `debug!`, `warn!`). Do NOT use `println!`.
