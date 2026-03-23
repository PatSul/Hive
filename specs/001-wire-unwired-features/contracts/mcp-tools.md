# MCP Tool Contracts: New Tool Handlers

**Date**: 2026-03-22

These are the new MCP tool definitions to be added to `integration_tools.rs`.

## search_knowledge

**Purpose**: Query the RAG index for relevant context
**Input**:
```json
{
  "query": "string (required) — natural language search query",
  "max_results": "integer (optional, default 10) — max chunks to return",
  "min_similarity": "float (optional, default 0.1) — minimum relevance score"
}
```
**Output**:
```json
{
  "results": [
    {
      "file_path": "string",
      "content": "string — matching chunk",
      "score": "float — relevance score"
    }
  ],
  "total_indexed": "integer — total chunks in index",
  "context": "string — pre-assembled context for prompt injection"
}
```

## wallet_create

**Purpose**: Generate a new wallet and store encrypted
**Input**:
```json
{
  "name": "string (required) — human-readable wallet name",
  "chain": "string (required) — 'ethereum' | 'base' | 'solana'",
  "password": "string (required) — encryption password for private key"
}
```
**Output**:
```json
{
  "wallet_id": "string",
  "address": "string — public address",
  "chain": "string"
}
```

## wallet_list

**Purpose**: List all stored wallets (no private keys exposed)
**Input**: `{}` (no parameters)
**Output**:
```json
{
  "wallets": [
    {
      "id": "string",
      "name": "string",
      "chain": "string",
      "address": "string"
    }
  ]
}
```

## wallet_balance

**Purpose**: Check balance of a wallet
**Input**:
```json
{
  "wallet_id": "string (required)"
}
```
**Output**:
```json
{
  "address": "string",
  "chain": "string",
  "balance": "float",
  "unit": "string — 'ETH' | 'SOL'"
}
```

## run_workflow

**Purpose**: Execute a saved workflow by ID
**Input**:
```json
{
  "workflow_id": "string (required)"
}
```
**Output**:
```json
{
  "success": "boolean",
  "steps_completed": "integer",
  "total_steps": "integer",
  "error": "string | null",
  "duration_ms": "integer"
}
```

## list_workflows

**Purpose**: List all available workflows
**Input**: `{}` (no parameters)
**Output**:
```json
{
  "workflows": [
    {
      "id": "string",
      "name": "string",
      "description": "string",
      "status": "string — 'draft' | 'active' | 'paused'",
      "step_count": "integer",
      "run_count": "integer"
    }
  ]
}
```

## describe_workflow

**Purpose**: Get detailed info about a workflow
**Input**:
```json
{
  "workflow_id": "string (required)"
}
```
**Output**:
```json
{
  "id": "string",
  "name": "string",
  "description": "string",
  "status": "string",
  "trigger": "object — trigger configuration",
  "steps": [
    {
      "id": "string",
      "name": "string",
      "action_type": "string",
      "timeout_secs": "integer",
      "retry_count": "integer"
    }
  ],
  "run_count": "integer",
  "last_run": "string | null — ISO timestamp"
}
```
