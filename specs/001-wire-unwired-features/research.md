# Research: Wire All Unwired Features

**Date**: 2026-03-22
**Branch**: 001-wire-unwired-features

## Research Findings

### Gap 1: OllamaManager Export

**Decision**: Add `pub use local_ai::{...}` re-exports to `hive_terminal/src/lib.rs`

**Rationale**: The `local_ai` module is declared as `pub mod local_ai` but no types are re-exported. While consumers CAN access types via `hive_terminal::local_ai::OllamaManager`, this breaks the crate's API convention — every other module (cli, docker, executor, sandbox, shell) has explicit `pub use` re-exports.

**Alternatives considered**: None — this is a straightforward API consistency fix.

**Types to export**: `LocalAiDetector`, `OllamaManager`, `LocalProviderInfo`, `PullProgress`, `OllamaModelInfo`

---

### Gap 2: RAG/Semantic Search Integration

**Decision**: RAG is ALREADY wired in the chat pipeline. The real gap is: (a) no MCP tool for agents to query the RAG index, and (b) Queen agent never calls `.with_rag()`.

**Rationale**: Research revealed that `chat_actions.rs` (lines 302-365) already queries both RagService and SemanticSearchService when `ContextTier::L2` is detected. The services ARE integrated — what's missing is agent-side access via MCP tool handlers, and the Queen orchestrator never receives the RAG service despite having a `.with_rag()` builder method.

**What needs to happen**:
1. Add `search_knowledge` MCP tool handler in `integration_tools.rs` that delegates to RagService.query()
2. Wire Queen construction in main.rs to call `.with_rag(rag_service.clone())`
3. Both are small changes to existing wiring code

**Alternatives considered**: Building a separate search API endpoint — rejected because MCP tool handlers are the established pattern in this codebase.

---

### Gap 3: Blockchain Tool Handlers

**Decision**: Wire the existing stubs `token_deploy_erc20` and `token_deploy_spl` to their real implementations. Add wallet management tools.

**Rationale**: The `token_estimate_cost` tool is already fully wired and working. The deploy tools explicitly return error messages ("Use the Blockchain panel") — they are intentional stubs waiting for real handlers. The EVM and Solana modules have complete deployment APIs (`deploy_token()`, `create_spl_token()`) that just need to be called.

**Security consideration**: Token deployment involves spending real funds. The handler MUST:
- Validate parameters before building the transaction
- Show estimated cost to user
- Require explicit confirmation before broadcasting
- Never auto-broadcast without user approval

**What needs to happen**:
1. Wire `token_deploy_erc20` handler → `hive_blockchain::evm::deploy_token_with_rpc()`
2. Wire `token_deploy_spl` handler → `hive_blockchain::solana::create_spl_token_with_rpc()`
3. Add `wallet_create`, `wallet_list`, `wallet_balance` tool handlers
4. Add private key retrieval from WalletStore with password (confirmation flow)

**Alternatives considered**: Separate blockchain daemon — rejected because operations are infrequent and async calls within tool handlers are sufficient.

---

### Gap 4: Workflow Execution MCP Tools

**Decision**: Add MCP tool handlers for `run_workflow`, `list_workflows`, `get_workflow_status`, and `describe_workflow`.

**Rationale**: Research revealed the workflow execution path is ALREADY COMPLETE:
- `execute_workflow_blocking()` works and handles all action types
- `workflow_actions.rs` has a UI handler that spawns a background thread and runs workflows
- Workflow persistence to `~/.hive/workflows/` works

The only gap is that agents cannot trigger workflows via MCP tool calls — there's no tool handler. The UI path works but the agent path doesn't.

**What needs to happen**:
1. Add `run_workflow` tool in `integration_tools.rs` → calls `AutomationService::execute_workflow_blocking()`
2. Add `list_workflows` tool → calls `AutomationService::list_workflows()`
3. Add `describe_workflow` tool → calls `AutomationService::get_workflow(id)`

**Alternatives considered**: Exposing workflow execution as a Server Action — rejected because MCP tools are the established agent-tool pattern.
