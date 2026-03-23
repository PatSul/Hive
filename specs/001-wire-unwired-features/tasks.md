# Tasks: Wire All Unwired Features

**Input**: Design documents from `/specs/001-wire-unwired-features/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/mcp-tools.md

**Tests**: Tests included — constitution requires comprehensive testing (Principle V).

**Organization**: Tasks grouped by user story for independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup (Verification)

**Purpose**: Confirm baseline — all existing tests pass before any changes

- [ ] T001 Run `cargo test --workspace --exclude hive_app` from `hive/` and confirm all tests pass (baseline)

**Checkpoint**: Baseline confirmed — safe to proceed

---

## Phase 2: Foundational (OllamaManager Export)

**Purpose**: Fix the crate export that blocks all other crates from accessing local AI management

**Why blocking**: Other crates cannot cleanly import OllamaManager until this is fixed. While they can use the long path `hive_terminal::local_ai::OllamaManager`, the public API convention requires re-exports.

- [ ] T002 Add `pub use local_ai::{LocalAiDetector, OllamaManager, LocalProviderInfo, PullProgress, OllamaModelInfo}` to `hive/crates/hive_terminal/src/lib.rs`
- [ ] T003 Run `cargo build` from `hive/` and confirm compilation succeeds with new exports

**Checkpoint**: OllamaManager exported — crate API consistent with all other modules

---

## Phase 3: User Story 1 — AI-Powered Context Search (Priority: P1) MVP

**Goal**: Expose RAG search as an MCP tool so agents can query the knowledge base, and wire Queen with RAG service

**Independent Test**: Invoke the `search_knowledge` tool via AI chat and confirm it returns indexed content; verify Queen receives RAG service at startup

### Tests for User Story 1

- [ ] T004 [P] [US1] Add unit test for `search_knowledge` tool handler in `hive/crates/hive_agents/src/integration_tools.rs` — test with empty index returns empty results, test with indexed content returns scored matches

### Implementation for User Story 1

- [ ] T005 [US1] Add `search_knowledge` tool definition (name, description, input schema) in `hive/crates/hive_agents/src/integration_tools.rs` alongside existing tool definitions
- [ ] T006 [US1] Add `search_knowledge` tool handler that retrieves `AppRagService` from context, calls `rag_svc.query(RagQuery { query, max_results, min_similarity })`, and returns results as JSON in `hive/crates/hive_agents/src/integration_tools.rs`
- [ ] T007 [US1] Wire Queen construction with `.with_rag(rag_service.clone())` in `hive/crates/hive_app/src/main.rs` where the Queen/Coordinator is built
- [ ] T008 [US1] Run `cargo test --workspace --exclude hive_app` and confirm zero regressions

**Checkpoint**: RAG search available as MCP tool; Queen has RAG context for task pipeline enrichment

---

## Phase 4: User Story 2 — Blockchain Wallet & Token Operations (Priority: P2)

**Goal**: Wire token deployment stubs to real implementations and add wallet management tools

**Independent Test**: Invoke `wallet_create`, `wallet_list`, `wallet_balance` tools via AI chat; verify `token_deploy_erc20` and `token_deploy_spl` call the real blockchain APIs instead of returning error stubs

### Tests for User Story 2

- [ ] T009 [P] [US2] Add unit test for `wallet_create` tool handler — test creates wallet entry, verifies it appears in wallet list, in `hive/crates/hive_agents/src/integration_tools.rs`
- [ ] T010 [P] [US2] Add unit test for `wallet_list` tool handler — test with empty store returns empty list, test with wallets returns all entries, in `hive/crates/hive_agents/src/integration_tools.rs`
- [ ] T011 [P] [US2] Add unit test for `wallet_balance` tool handler — test with invalid wallet_id returns error, in `hive/crates/hive_agents/src/integration_tools.rs`

### Implementation for User Story 2

- [ ] T012 [US2] Add `wallet_create` tool definition and handler in `hive/crates/hive_agents/src/integration_tools.rs` — calls `generate_wallet_material(chain)`, `encrypt_key(key, password)`, `wallet_store.add_wallet(name, chain, address, encrypted_key)`, saves store to disk
- [ ] T013 [US2] Add `wallet_list` tool definition and handler in `hive/crates/hive_agents/src/integration_tools.rs` — calls `wallet_store.list_wallets()`, returns id/name/chain/address (never private keys)
- [ ] T014 [US2] Add `wallet_balance` tool definition and handler in `hive/crates/hive_agents/src/integration_tools.rs` — looks up wallet by ID, calls `get_balance_with_rpc(address, chain, rpc_url)` async
- [ ] T015 [US2] Replace `token_deploy_erc20` stub handler with real implementation in `hive/crates/hive_agents/src/integration_tools.rs` — validates params, retrieves wallet key (requires password), calls `deploy_token_with_rpc(params, private_key, rpc_url)`, returns DeployResult
- [ ] T016 [US2] Replace `token_deploy_spl` stub handler with real implementation in `hive/crates/hive_agents/src/integration_tools.rs` — validates params, retrieves wallet key, calls `create_spl_token_with_rpc(params, payer, rpc_url)`, returns SplDeployResult
- [ ] T017 [US2] Run `cargo test --workspace --exclude hive_app` and confirm zero regressions

**Checkpoint**: Full blockchain tool suite operational — wallets and token deployment accessible via AI chat

---

## Phase 5: User Story 3 — Workflow Execution (Priority: P2)

**Goal**: Add MCP tool handlers so agents can list, describe, and execute workflows

**Independent Test**: Invoke `list_workflows` via AI chat and see available workflows; invoke `run_workflow` with a workflow ID and confirm execution completes

### Tests for User Story 3

- [ ] T018 [P] [US3] Add unit test for `list_workflows` tool handler — test returns built-in workflows after `ensure_builtin_workflows()`, in `hive/crates/hive_agents/src/integration_tools.rs`
- [ ] T019 [P] [US3] Add unit test for `describe_workflow` tool handler — test with valid ID returns workflow details, test with invalid ID returns error, in `hive/crates/hive_agents/src/integration_tools.rs`

### Implementation for User Story 3

- [ ] T020 [US3] Add `list_workflows` tool definition and handler in `hive/crates/hive_agents/src/integration_tools.rs` — calls `automation_service.list_workflows()`, returns id/name/description/status/step_count/run_count
- [ ] T021 [US3] Add `describe_workflow` tool definition and handler in `hive/crates/hive_agents/src/integration_tools.rs` — calls `automation_service.get_workflow(id)`, returns full workflow details including steps and trigger config
- [ ] T022 [US3] Add `run_workflow` tool definition and handler in `hive/crates/hive_agents/src/integration_tools.rs` — looks up workflow, spawns background thread, calls `execute_workflow_blocking(workflow, working_dir)`, returns WorkflowRunResult as JSON
- [ ] T023 [US3] Run `cargo test --workspace --exclude hive_app` and confirm zero regressions

**Checkpoint**: Workflows fully operational via both UI canvas and AI agent tool calls

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final verification across all wired features

- [ ] T024 Run full `cargo build` from `hive/` and confirm clean compilation
- [ ] T025 Run full `cargo test --workspace --exclude hive_app` and confirm all tests pass (including new tests)
- [ ] T026 Verify no security gate violations: grep for `Command::new` with user input, unvalidated `format!()` in shell contexts, private keys in logs

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies — verification only
- **Phase 2 (Foundational)**: Depends on Phase 1 — OllamaManager export
- **Phase 3 (US1 - RAG)**: Depends on Phase 2 — can start immediately after
- **Phase 4 (US2 - Blockchain)**: Depends on Phase 2 — can run in parallel with Phase 3
- **Phase 5 (US3 - Workflow)**: Depends on Phase 2 — can run in parallel with Phase 3 and 4
- **Phase 6 (Polish)**: Depends on all user story phases completing

### User Story Dependencies

- **US1 (RAG Search)**: Independent — only touches integration_tools.rs (search tools) and main.rs
- **US2 (Blockchain)**: Independent — only touches integration_tools.rs (wallet/token tools)
- **US3 (Workflow)**: Independent — only touches integration_tools.rs (workflow tools)

**Note**: US1, US2, and US3 all modify `integration_tools.rs` but touch DIFFERENT sections (different tool handlers). They can proceed in parallel if each agent works on distinct tool definitions.

### Within Each User Story

- Tests MUST be written first (fail before implementation)
- Tool definitions before handlers
- Handlers before wiring
- Regression test after each story completes

### Parallel Opportunities

- T004, T009, T010, T011, T018, T019 — all test tasks can be written in parallel (different test functions, same file but non-conflicting sections)
- US1, US2, US3 implementation can proceed in parallel (different tool handlers)

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Verify baseline
2. Complete Phase 2: Export OllamaManager
3. Complete Phase 3: RAG search tool + Queen wiring
4. **STOP and VALIDATE**: Test search_knowledge tool independently
5. Commit MVP

### Incremental Delivery

1. Phase 1 + 2 → Foundation ready
2. + US1 (RAG) → AI context search operational → Commit
3. + US2 (Blockchain) → Wallet & token tools operational → Commit
4. + US3 (Workflow) → Workflow execution via agents → Commit
5. Phase 6 → Final verification → Commit

---

## Notes

- [P] tasks = different files or non-conflicting sections, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable
- All new tool handlers follow the existing pattern in integration_tools.rs
- Blockchain deploy handlers MUST validate before executing (check balance, require confirmation)
- Commit after each story completion
