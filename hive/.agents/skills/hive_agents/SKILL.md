---
name: Hive Agent Architecture
description: Patterns for Model Context Protocol (MCP) servers, tools, and agent workflows.
---

# Hive Agent Architecture

## MCP Server Implementation
- **Location**: `crates/hive_agents/src/mcp_server.rs`
- Tools are registered via `register` taking a `McpTool` definition and a `ToolHandler` boxed closure.
- Handlers are async-compatible via `block_on` from tokio within a separate thread (if called from a tokio runtime context) or direct block_on.

## Creating a new MCP Tool
1. Define the `McpTool` struct detailing `name`, `description`, and `input_schema` (JSON Schema object).
2. Write a `ToolHandler` that extracts arguments from the `serde_json::Value` arguments payload.
3. Validate arguments robustly using `args.get("arg").and_then(|v| v.as_str()).ok_or(...)`.
4. Return `Ok(json!(...))` or `Err("...".to_string())`.

## Agent Types & Integrations
- Hive supports local model auto-detection (Ollama, LM Studio) and cloud integrations (OpenRouter, OpenAI, Anthropic, Google, Groq).
- Interaction with the host filesystem is handled by `hive_fs::FileService` and commands by `hive_terminal::CommandExecutor`.
- UI Automation tools (`click`, `type_text`, `press_enter`) are exposed to the agent via `crate::ui_automation::UiDriver`.

## System Prompts & Workflows
Ensure agent system prompts accurately reflect the capabilities registered in the MCP server.
