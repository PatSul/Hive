# Feature Specification: Wire All Unwired Features

**Feature Branch**: `001-wire-unwired-features`
**Created**: 2026-03-22
**Status**: Draft
**Input**: Wire all remaining unwired features to achieve 100% functional coverage

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Local AI Model Management (Priority: P1)

A developer using the Hive desktop app wants to manage local AI models through Ollama. They navigate to the terminal or AI settings, and the system provides full model lifecycle management — listing installed models, pulling new ones, starting/stopping the Ollama server — without errors or missing functionality.

**Why this priority**: Local AI is a core differentiator. If the model management service exists but cannot be accessed by other parts of the system, users encounter broken workflows when attempting to use local models.

**Independent Test**: Can be verified by confirming that any module referencing OllamaManager can import it without compilation errors, and that model listing/pulling operations complete successfully.

**Acceptance Scenarios**:

1. **Given** the app is running with Ollama installed, **When** an agent requests the list of available local models, **Then** the system returns the full model inventory without import or linkage errors.
2. **Given** any crate in the workspace imports OllamaManager, **When** the project compiles, **Then** compilation succeeds without "unresolved import" errors.

---

### User Story 2 - AI-Powered Context Search (Priority: P1)

A developer is working on a complex codebase and asks the AI assistant a question about their project. The system automatically enriches the prompt with relevant context from previously indexed documents, code files, and knowledge base entries — delivering more accurate, project-aware responses.

**Why this priority**: RAG (Retrieval Augmented Generation) dramatically improves AI response quality. The services are fully built but disconnected from the chat pipeline, meaning users get generic responses instead of context-aware ones.

**Independent Test**: Can be verified by indexing sample documents, then asking a question in chat and confirming the response references indexed content.

**Acceptance Scenarios**:

1. **Given** documents have been indexed into the knowledge base, **When** a user asks a question in chat, **Then** the system retrieves relevant context and includes it in the AI prompt.
2. **Given** the AI agent is performing a multi-step task, **When** it needs project context, **Then** it can invoke a search tool that queries the RAG index and returns ranked results.
3. **Given** no documents are indexed, **When** a user asks a question, **Then** the system gracefully falls back to standard AI responses without errors.

---

### User Story 3 - Blockchain Wallet & Token Operations (Priority: P2)

A user wants to create a wallet, deploy a token, or sign a transaction directly from the Hive desktop app. They access blockchain operations through the AI assistant (via tool calls) or through a dedicated UI panel, and the system handles the full lifecycle — wallet creation, key storage, transaction building, signing, and broadcasting.

**Why this priority**: The blockchain subsystem is fully implemented but has zero exposure to users. While not the core AI workflow, it represents a complete feature that was built and never connected.

**Independent Test**: Can be verified by creating a wallet through the AI assistant, confirming the wallet appears in storage, and verifying that a test transaction can be built and signed (without broadcasting to mainnet).

**Acceptance Scenarios**:

1. **Given** a user asks the AI to create a new wallet, **When** the tool handler processes the request, **Then** a new wallet is created, encrypted, and stored locally.
2. **Given** a wallet exists, **When** a user asks to deploy a token, **Then** the system builds the deployment transaction and presents it for signing confirmation.
3. **Given** a user attempts a blockchain operation, **When** the operation fails (network error, insufficient funds), **Then** the system returns a clear error message with suggested next steps.

---

### User Story 4 - Workflow Execution (Priority: P2)

A user has designed a multi-step workflow in the visual Workflow Builder canvas — connecting triggers, actions, conditions, and outputs. They want to save the workflow and execute it, seeing real-time progress as each step completes. The AI assistant can also trigger workflow execution through tool calls.

**Why this priority**: The visual builder exists and renders beautifully, but workflows cannot be executed. This makes the entire builder panel decorative rather than functional.

**Independent Test**: Can be verified by creating a simple 3-step workflow in the builder, clicking execute, and confirming each step runs in sequence with visible progress.

**Acceptance Scenarios**:

1. **Given** a workflow is designed in the visual builder, **When** the user clicks "Execute Workflow", **Then** the system runs each step in order and displays progress indicators.
2. **Given** the AI agent needs to run an automation, **When** it invokes the workflow execution tool with a workflow ID, **Then** the workflow executes and returns step-by-step results.
3. **Given** a workflow step fails during execution, **When** the error occurs, **Then** the system stops execution, highlights the failed step, and displays the error message.

---

### Edge Cases

- What happens when the Ollama server is not running and a model operation is requested? System MUST return a clear "Ollama not available" message rather than a generic connection error.
- What happens when the RAG index is empty and context enrichment is attempted? System MUST skip enrichment gracefully and proceed with the standard prompt.
- What happens when a user attempts to deploy a token without sufficient funds? System MUST check balance before building the transaction and report the shortfall.
- What happens when a workflow contains a circular dependency between steps? System MUST detect the cycle before execution and report which steps form the loop.
- What happens when a blockchain RPC endpoint is unreachable? System MUST timeout within 10 seconds and report the connectivity issue.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST export all public services from their respective library crates so that dependent crates can import them without compilation errors.
- **FR-002**: System MUST enrich AI chat prompts with relevant context from the RAG index when indexed documents are available.
- **FR-003**: System MUST expose a "search knowledge base" tool that agents can invoke to query the semantic search index.
- **FR-004**: System MUST provide tool handlers for wallet creation, token deployment, transaction signing, and transaction broadcasting.
- **FR-005**: System MUST encrypt and securely store all wallet private keys using the existing wallet store mechanism.
- **FR-006**: System MUST provide a UI action to execute workflows from the Workflow Builder panel.
- **FR-007**: System MUST expose a "execute workflow" tool handler that agents can invoke programmatically.
- **FR-008**: System MUST display execution progress when a workflow is running, indicating which step is active and which have completed.
- **FR-009**: System MUST handle failures in all newly wired features gracefully — no panics, no silent failures, clear user-facing error messages.
- **FR-010**: System MUST pass all existing tests after wiring changes are complete (zero regressions).

### Key Entities

- **OllamaManager**: Service managing local AI model lifecycle (list, pull, start, stop). Currently built in hive_terminal but not exported.
- **RagService**: Retrieval Augmented Generation service that indexes documents and retrieves relevant context for AI prompts. Built in hive_ai.
- **SemanticSearchService**: Full-text and semantic search across indexed content. Built in hive_ai, paired with RagService.
- **WalletStore**: Encrypted wallet key storage. Built in hive_blockchain, initialized but not exposed via tools.
- **Workflow**: Multi-step automation definition with triggers, actions, conditions, and outputs. Built in hive_agents, visual builder exists in hive_ui_panels.

### Assumptions

- All services referenced above are already instantiated in the application entry point (main.rs) and are structurally complete.
- This work is purely wiring — connecting existing implementations to their consumers. No new algorithms, data structures, or service architectures are needed.
- Blockchain operations will be available through AI tool calls only (no standalone UI panel in this iteration); wallet management uses the existing encrypted store.
- RAG context enrichment will be opt-in via configuration — users can enable/disable it per conversation or globally.
- Workflow execution uses the existing `execute_workflow_blocking()` implementation and does not require new execution engine work.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of previously built services are accessible from their intended consumers — zero "unresolved import" or "missing function" compilation errors.
- **SC-002**: AI responses in conversations with indexed documents are enriched with relevant context at least 80% of the time (when context exists in the index).
- **SC-003**: All 5 blockchain tool operations (create wallet, list wallets, deploy token, sign transaction, check balance) complete successfully when invoked by the AI agent.
- **SC-004**: Workflows created in the visual builder can be executed with a single action, and 100% of workflow steps execute in the correct order.
- **SC-005**: All existing tests continue to pass after wiring changes (zero regressions).
- **SC-006**: Every newly wired operation returns a meaningful error message on failure rather than crashing or failing silently.
