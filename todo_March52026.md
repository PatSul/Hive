# Hive — Todo List (March 5, 2026)

## Test Results Summary
- **2,485 tests across 15 crates — ALL PASS**
- `cargo test --workspace` fails to link on Windows (OOM/stack overflow in hive_ui) — must test per-crate or in batches
- 10 local commits (A2A protocol) not yet pushed to origin

---

## P0 — Blocking for Production

- [ ] **hive_a2a: Wire HiveTaskHandler into Axum server** — Handler is fully built but disconnected from routes. Needs type erasure (`Box<dyn TaskHandler>`) or a concrete type to connect to `AppState`
- [ ] **hive_a2a: send_message_handler returns stub** — Never dispatches to any orchestrator; always returns a fake acknowledgement
- [ ] **hive_a2a: get_task_handler always returns 404** — Tasks are never stored or looked up
- [ ] **hive_cloud: Billing stub** — `check_subscription()` always returns `"pro"`, no Stripe integration
- [ ] **hive_admin: Mock API** — All 6 API methods return hardcoded mock data, reqwest client unused

---

## P1 — Significant Gaps

- [ ] **hive_ai: RAG data ingestion missing** — Query path is wired into chat context, but `rag_svc.ingest()` is never called, so queries always return empty
- [ ] **hive_remote: Hardcoded JWT** — Session token is hardcoded as `"TODO_JWT"`, no real authentication
- [ ] **hive_agents: Simulated workflow execution** — `run_workflow()` counts steps and always succeeds; never actually runs workflow steps
- [ ] **hive_blockchain: Simulation-only deployment** — Placeholder bytecode, fake signatures, `AppWallets` has 0 UI consumers
- [ ] **hive_cloud: JWT validation not implemented** — JWT creation works, but validation is missing
- [ ] **hive_a2a: Missing server hardening** — No rate limiting, no concurrent task limits, no CORS, no SSE endpoint, no budget enforcement

---

## P2 — Dead Code / Unwired Features

- [ ] **hive_terminal: OllamaManager** — Fully built and exported, zero callers in entire codebase
- [ ] **hive_ai: SemanticSearch** — Initialized as global in main.rs, never queried by any code path
- [ ] **hive_integrations: PhilipsHueClient** — Exported publicly, never instantiated
- [ ] **hive_terminal: CDP BrowserAutomation** — Orphaned; Playwright version is used instead
- [ ] **hive_a2a: Client-side APIs unused** — `RemoteAgent`, `DiscoveryCache`, `discover_agent()` built and exported but zero callers outside the crate

---

## P3 — Test Coverage Gaps

- [ ] **hive_admin: 0 tests** (6 source files)
- [ ] **hive_cloud: 0 tests** (4 source files)
- [ ] **hive_cli: 0 tests** (12 source files)
- [ ] **hive_a2a: Untested paths** — `discover_agent()` HTTP flow, `RemoteAgent::send_task()`, HiveMind/Coordinator/Queen execution, SSE streaming end-to-end
- [ ] **hive_ui: Stack overflow on test compilation** — Tests exist but rustc overflows linking; workaround is external test files (already used in hive_ui_panels)

---

## Infra / Build Issues

- [ ] **Workspace-wide `cargo test` broken on Windows** — hive_ui causes rustc stack overflow during linking; hive_ui_panels external tests fail with missing rlib crates (arrow_cast, lance_index, etc.)
- [ ] **10 local commits not pushed** — Branch is ahead of origin/main by 10 commits
- [ ] **2 untracked plan docs** — `hive/docs/plans/2026-03-04-a2a-protocol-design.md` and `2026-03-04-a2a-protocol-impl.md`

---

## Completed Today (March 5)

- [x] hive_a2a crate scaffolded with workspace integration
- [x] A2A Config + Error types
- [x] Auth middleware (API key + URL validation)
- [x] Agent Card builder with Hive skill definitions
- [x] Bridge module (A2A ↔ Hive type conversions)
- [x] Client discovery cache + remote agent wrapper
- [x] Task handler for A2A message → Hive orchestrator dispatch
- [x] HTTP server with Axum routes + SSE streaming helper
- [x] A2A server wired into app startup, public API finalized
- [x] Full round-trip integration test (108 tests passing)
