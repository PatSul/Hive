# Data Model: Wire All Unwired Features

**Date**: 2026-03-22
**Branch**: 001-wire-unwired-features

## Entities

This feature is purely wiring — no new data structures are introduced. All entities below already exist.

### OllamaManager (hive_terminal::local_ai)
- **Fields**: base_url (String)
- **Methods**: pull_model, delete_model, show_model, list_models
- **State**: Stateless service, configured with base URL
- **Change**: Export from crate root (no structural changes)

### RagService (hive_ai::rag)
- **Fields**: chunks (Vec<DocumentChunk>), chunk_size, overlap
- **Methods**: index_file, index_directory, query, build_context, clear_index, stats
- **Key types**: RagQuery (query, max_results, min_similarity), RagResult (chunks, context)
- **Change**: Add MCP tool handler that wraps query() call

### WalletStore (hive_blockchain::wallet_store)
- **Fields**: wallets (Vec<WalletEntry>)
- **Key types**: WalletEntry (id, name, chain, address, encrypted_key), Chain (Ethereum, Base, Solana)
- **Persistence**: ~/.hive/wallets.enc (encrypted JSON)
- **Change**: Add MCP tool handlers for CRUD operations

### AutomationService (hive_agents::automation)
- **Fields**: workflows (Vec<Workflow>), run_history (Vec<WorkflowRunResult>)
- **Key types**: Workflow, WorkflowStep, ActionType, TriggerType, WorkflowStatus
- **Persistence**: ~/.hive/workflows/ (JSON per workflow)
- **Change**: Add MCP tool handlers that delegate to existing methods

## Relationships

```
OllamaManager ──── exported by ────→ hive_terminal (lib.rs)
                                      │
RagService ──── queried by ─────────→ chat_actions (L2 enrichment)
            ──── new: queried by ───→ MCP tool handler (search_knowledge)
            ──── passed to ─────────→ Queen.with_rag()
                                      │
WalletStore ──── new: exposed by ──→ MCP tool handlers (wallet_*)
            ──── new: called by ───→ token_deploy_* handlers
                                      │
AutomationService ── run by ───────→ workflow_actions (UI)
                  ── new: run by ──→ MCP tool handlers (run_workflow)
```
