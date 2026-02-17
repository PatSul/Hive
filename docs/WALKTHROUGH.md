# Hive App Walkthrough & Testing Guide

A comprehensive guide to every feature in Hive, how they work together, and how to test them.

---

## Table of Contents

1. [First Launch & Setup](#1-first-launch--setup)
2. [Navigation & Layout](#2-navigation--layout)
3. [Chat (Cmd/Ctrl+1)](#3-chat-cmdctrl1)
4. [History (Cmd/Ctrl+2)](#4-history-cmdctrl2)
5. [Files (Cmd/Ctrl+3)](#5-files-cmdctrl3)
6. [Specs (Cmd/Ctrl+4)](#6-specs-cmdctrl4)
7. [Agents (Cmd/Ctrl+5)](#7-agents-cmdctrl5)
8. [Workflows (Cmd/Ctrl+6)](#8-workflows-cmdctrl6)
9. [Channels (Cmd/Ctrl+7)](#9-channels-cmdctrl7)
10. [Kanban (Cmd/Ctrl+8)](#10-kanban-cmdctrl8)
11. [Monitor (Cmd/Ctrl+9)](#11-monitor-cmdctrl9)
12. [Logs (Cmd/Ctrl+0)](#12-logs-cmdctrl0)
13. [Costs](#13-costs)
14. [Review (Git Ops)](#14-review-git-ops)
15. [Skills & Marketplace](#15-skills--marketplace)
16. [Routing](#16-routing)
17. [Models Browser](#17-models-browser)
18. [Learning](#18-learning)
19. [Shield (Security)](#19-shield-security)
20. [Assistant (Personal)](#20-assistant-personal)
21. [Token Launch (Blockchain)](#21-token-launch-blockchain)
22. [Settings](#22-settings)
23. [Workflow Builder](#23-workflow-builder)
24. [Help](#24-help)
25. [System Tray & Background Mode](#25-system-tray--background-mode)
26. [Feature Integration Map](#26-feature-integration-map)
27. [End-to-End Test Scenarios](#27-end-to-end-test-scenarios)

---

## 1. First Launch & Setup

### What Happens on Startup

1. **Config directories created** -- `~/.hive/` is ensured to exist with subdirectories for conversations, workflows, and data.
2. **Three SQLite databases opened in parallel:**
   - `~/.hive/memory.db` -- conversations, messages, costs, logs, FTS5 search indexes
   - `~/.hive/learning.db` -- outcome tracking, preferences, prompt evolution, pattern library
   - `~/.hive/assistant.db` -- personal assistant data (emails, calendar, reminders, approvals)
3. **JSON backfill** -- Any conversations stored as JSON files are automatically imported into SQLite with FTS5 search indexes built.
4. **Services initialized:** AI providers, HiveShield security, TTS engine, Skills registry, Marketplace, Persona registry, Automation engine, MCP server, Spec manager, CLI service, Wallet store, RPC config, IDE integration, Channel store.
5. **Window size restored** from last session (saved in `~/.hive/session.json`).
6. **System tray** icon created for background mode.

### How to Test

- [ ] Launch the app -- verify it opens without errors
- [ ] Check `~/.hive/` directory is created with `memory.db`, `learning.db`, `assistant.db`
- [ ] Verify the window opens at the correct size (or defaults to 1280x800)
- [ ] Check the system tray icon appears (menu bar on macOS)

---

## 2. Navigation & Layout

### Sidebar Structure

The sidebar is organized into five groups:

| Group | Panels |
|-------|--------|
| **Core** | Chat, History, Files |
| **Flow** | Specs, Agents, Workflows, Channels |
| **Observe** | Kanban, Monitor, Logs, Costs |
| **Project** | Review, Skills, Routing, Models Browser, Learning, Shield |
| **System** | Assistant, Token Launch, Settings, Help |

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd/Ctrl + Q` | Quit Hive |
| `Cmd/Ctrl + ,` | Open Settings |
| `Cmd/Ctrl + P` | Toggle Privacy Mode |
| `Cmd/Ctrl + N` | New conversation |
| `Cmd/Ctrl + L` | Clear chat |
| `Cmd/Ctrl + 1` through `0` | Switch to panels (Chat, History, Files, Specs, Agents, Workflows, Channels, Kanban, Monitor, Logs) |
| `Cmd/Ctrl + Shift + W` | Open Workflow Builder |
| `Cmd/Ctrl + S` | Save workflow (in builder) |
| `Cmd/Ctrl + Shift + R` | Run workflow (in builder) |
| `Delete / Backspace` | Delete selected node (in builder) |
| `Cmd/Ctrl + Z` | Undo (in builder) |
| `Cmd/Ctrl + Shift + Z` | Redo (in builder) |
| `Enter` | Send message |
| `Shift + Enter` | New line in chat |
| `Escape` | Cancel streaming / close modal |

### How to Test

- [ ] Click each sidebar group and verify all panels load
- [ ] Test every keyboard shortcut listed above
- [ ] Verify panel switching is instant (GPUI renders at 120fps)
- [ ] Resize the window and verify layout adapts

---

## 3. Chat (Cmd/Ctrl+1)

### Features

- **Streaming AI responses** with real-time token display
- **Full Markdown rendering** with pulldown-cmark: headings, bold, italic, inline code, code blocks with syntax highlighting, lists, horizontal rules, tables
- **Markdown cache** for performance -- parsed ASTs are cached by content hash (FNV-1a) to avoid re-parsing every render frame
- **Welcome screen** shown when no messages exist
- **Message roles:** User, Assistant, System
- **Multi-provider support** -- messages sent through whichever provider/model is configured

### How to Test

- [ ] Type a message and press Enter -- verify streaming response appears
- [ ] Ask for a code block (e.g., "Write a Python hello world") -- verify syntax highlighting
- [ ] Ask for markdown formatting (headings, bold, lists) -- verify correct rendering
- [ ] Press `Cmd/Ctrl + N` -- verify new conversation starts with welcome screen
- [ ] Press `Cmd/Ctrl + L` -- verify chat clears
- [ ] Press `Escape` during streaming -- verify it cancels
- [ ] Use `Shift + Enter` to create multi-line input
- [ ] Send multiple messages to build a conversation thread

---

## 4. History (Cmd/Ctrl+2)

### Features

- **Conversation list** with titles, timestamps, and message counts
- **Full-text search** powered by FTS5 with Porter stemming and unicode61 tokenizer
- **Search across all conversations** -- finds matches in message content, not just titles
- **Click to load** any previous conversation into the chat panel
- **SQLite-backed** -- all conversations persist between sessions

### How to Test

- [ ] Create several conversations with distinct topics
- [ ] Open History panel and verify all conversations appear
- [ ] Use the search bar -- type a keyword that appears in a previous conversation
- [ ] Verify search returns relevant results (FTS5 stemming means "running" matches "ran")
- [ ] Click a search result to load the conversation
- [ ] Verify the conversation loads correctly in the chat panel

---

## 5. Files (Cmd/Ctrl+3)

### Features

- **File tree browser** showing the project directory structure
- **Git-aware** file walking using the `ignore` crate (respects `.gitignore`)
- **File watching** via `notify` crate for real-time updates
- **File status indicators** showing modified, added, deleted, untracked files

### How to Test

- [ ] Open Files panel -- verify your project directory tree appears
- [ ] Create a new file in your project -- verify it appears in the tree
- [ ] Modify a file -- verify the status indicator updates
- [ ] Verify `.gitignore`-excluded files don't appear

---

## 6. Specs (Cmd/Ctrl+4)

### Features

- **Project specifications** capture the intent and requirements for features
- **Spec manager** persists specs and links them to agent runs
- **Used by agents** to understand what to build/fix/review
- **Linked to Kanban** tasks for project tracking

### How to Test

- [ ] Open Specs panel
- [ ] Create a new specification describing a feature
- [ ] Verify it persists when you switch panels and come back
- [ ] Link a spec to an agent run (see Agents section)

---

## 7. Agents (Cmd/Ctrl+5)

### Agent System Architecture

Hive uses a multi-agent swarm with three orchestration modes:

1. **HiveMind** -- Full multi-agent pipeline with 9 specialized roles
2. **Coordinator** -- Dependency-ordered task dispatch
3. **NativeProvider** -- Use the provider's native multi-agent capability
4. **SingleShot** -- Single AI call (simplest and cheapest)

A **Queen** meta-coordinator manages team objectives, dispatches teams in parallel, validates dependencies (topological sort with cycle detection), and synthesizes results.

### The 9 Agent Roles

| Role | Description |
|------|-------------|
| **Architect** | System design & planning |
| **Coder** | Implementation & code generation |
| **Reviewer** | Code review & feedback |
| **Tester** | Test creation & validation |
| **Documenter** | Documentation & technical writing |
| **Debugger** | Bug diagnosis & fixes |
| **Security** | Security audit & hardening |
| **Output Reviewer** | Output quality checks |
| **Task Verifier** | Task completion verification |

### 6 Built-in Personas (with tools)

The agent system also includes 6 named personas (distinct from the 9 monitor roles) that execute tasks:

| Persona | Kind | Model Tier | Tools | Max Tokens |
|---------|------|-----------|-------|------------|
| **Investigator** | Investigate | Premium | read_file, search_symbol, find_references, list_directory | 8192 |
| **Implementer** | Implement | Mid | read_file, write_file, run_command, search_symbol | 8192 |
| **Verifier** | Verify | Mid | run_command, read_file, write_file | 4096 |
| **Critic** | Critique | Premium | read_file, search_symbol | 4096 |
| **Debugger** | Debug | Mid | read_file, run_command, search_symbol, find_references | 8192 |
| **Code Reviewer** | CodeReview | Premium | read_file, search_symbol | 4096 |

### Panel Contents

The Agents panel shows:
- **Personas** -- all 9 agent roles with their kind, description, model tier, and active status
- **Workflows** -- runnable automation workflows with run count, status, and trigger info
- **Active Runs** -- currently executing orchestration runs with progress bars
- **Run History** -- completed runs with spec title, status, cost, and elapsed time

### SwarmConfig Defaults

| Setting | Default |
|---------|---------|
| Queen model | claude-sonnet-4-5 |
| Max parallel teams | 3 |
| Total cost limit | $25.00 |
| Total time limit | 30 minutes |
| Per-team cost limit | $5.00 |
| Per-team time limit | 5 minutes |

### How to Test

- [ ] Open Agents panel -- verify all 9 personas are listed
- [ ] Check each persona shows correct icon, name, description, model tier
- [ ] Verify workflow list loads from `.hive/workflows/` directory
- [ ] Click "Reload Workflows" -- verify it refreshes
- [ ] Run a workflow and observe the active runs section
- [ ] After completion, verify the run appears in history with cost/elapsed data

---

## 8. Workflows (Cmd/Ctrl+6)

### Features

- **Automation workflows** defined in YAML format
- **Stored in** `.hive/workflows/` directory
- **Triggers:** cron schedules, events, webhooks, or manual
- **Steps:** sequential or parallel execution of commands, API calls, notifications
- **Built-in templates:** Build & Test, Code Review, Debug Issue, Deploy, Research & Implement, Full Pipeline

### Built-in Templates

| Template | Steps |
|----------|-------|
| **Build & Test** | cargo check, build release, run all tests |
| **Code Review** | Investigate changes, run clippy, run tests |
| **Debug Issue** | Reproduce, investigate root cause, verify fix |
| **Deploy** | Test, build release, check binary |
| **Research & Implement** | Investigate approaches, implement, verify, review |
| **Full Pipeline** | End-to-end: investigate, build, test, lint, verify |

### How to Test

- [ ] Open Workflows panel -- verify templates appear
- [ ] Create a YAML workflow in `.hive/workflows/`
- [ ] Reload workflows and verify it loads
- [ ] Run a workflow and observe execution
- [ ] Verify workflow status updates (Pending, Running, Completed, Failed)

---

## 9. Channels (Cmd/Ctrl+7)

### Features

- **Telegram-style messaging channels** for multi-agent conversations
- **Persistent channels** with message history
- **Agent presence panel** on the right showing which agents are assigned
- **Default channels** created on startup

### Default Channels

| Channel | Assigned Agents |
|---------|----------------|
| **#general** | All 9 roles: Architect, Coder, Reviewer, Tester, Documenter, Debugger, Security, Output Reviewer, Task Verifier |
| **#code-review** | CodeReview + Critique agents |
| **#debug** | Debug + Investigate agents |
| **#research** | Investigate + Implement agents |

### How to Test

- [ ] Open Channels panel -- verify the 4 default channels appear
- [ ] Select #general -- verify all 9 agents are listed in presence panel
- [ ] Type a message in #general -- verify agents respond
- [ ] Switch to #code-review -- verify only relevant agents are assigned
- [ ] Check message persistence by leaving and returning to a channel

---

## 10. Kanban (Cmd/Ctrl+8)

### Features

- **4-column board:** To Do, In Progress, Review, Done
- **Priority levels:** Low, Medium, High, Critical
- **Task cards** with title, description, priority, creation date, and optional assigned model
- **Serializable** -- board state persists via serde (JSON)
- **Linked to workflows** -- tasks can trigger agent runs

### How to Test

- [ ] Open Kanban panel -- verify 4 columns appear
- [ ] Click "Add Task" -- create a task with title, description, and priority
- [ ] Verify the task appears in the "To Do" column
- [ ] Move a task between columns
- [ ] Create tasks with different priorities (Low, Medium, High, Critical) and verify color coding
- [ ] Verify tasks persist when switching panels

---

## 11. Monitor (Cmd/Ctrl+9)

### Features

- **Real-time agent system monitoring**
- **System status:** Idle, Running, Paused, Error (with color-coded dots)
- **Individual agent status:** Idle, Working, Waiting, Done, Failed
- **All 9 agent roles** displayed with their current state
- **Refresh button** to pull latest data

### How to Test

- [ ] Open Monitor panel -- verify all 9 agent roles are listed
- [ ] Check system status shows "Idle" when no work is running
- [ ] Start an agent run from the Agents panel
- [ ] Switch to Monitor -- verify agents show "Working" status
- [ ] After completion, verify agents return to "Idle"
- [ ] Click Refresh to force update

---

## 12. Logs (Cmd/Ctrl+0)

### Features

- **Application log viewer** displaying structured tracing output
- **Log levels:** TRACE, DEBUG, INFO, WARN, ERROR
- **Persistent** -- logs stored in SQLite (`memory.db`)
- **Real-time streaming** of new log entries
- **Filterable** by log level

### How to Test

- [ ] Open Logs panel -- verify log entries appear
- [ ] Verify startup sequence logs (service initialization messages)
- [ ] Trigger an action (e.g., send a chat message) and verify related logs appear
- [ ] Filter by log level and verify filtering works
- [ ] Verify logs persist across app restarts

---

## 13. Costs

### Features

- **Cost tracking dashboard** aggregated from all AI API calls
- **Per-model breakdown:** model ID, request count, input/output tokens, total cost
- **Summary metrics:** today's cost, all-time cost, total requests, total tokens
- **Budget enforcement:** daily and monthly limits (configurable in Settings)
- **Export to CSV** for external analysis
- **Reset today / Clear history** controls

### How to Test

- [ ] Open Costs panel -- verify dashboard shows $0.00 if no API calls made
- [ ] Send several chat messages to different models
- [ ] Verify cost entries appear with correct model, token counts, and costs
- [ ] Check "Today's Cost" updates in real-time
- [ ] Click "Export CSV" -- verify a CSV file is generated with correct data
- [ ] Click "Reset Today" -- verify today's counter resets
- [ ] Set a daily budget in Settings and verify warnings appear when approaching it

---

## 14. Review (Git Ops)

### Features

This is a comprehensive Git operations panel, not just code review:

- **File status** with M/A/D/R/? indicators
- **Inline diff viewer** with context/addition/deletion/hunk line types
- **Recent commits** with hash, message, author, time
- **Staging controls:** Stage All, Unstage All, Discard All
- **Commit with message** (AI-generated commit messages available)
- **Push / Push Set Upstream**
- **Branch management:** Create, Switch, Delete
- **Gitflow:** Init, Start feature/release/hotfix, Finish
- **Git LFS:** Track patterns, Pull, Push
- **Pull Request:** Create and AI-generate PR description
- **Review verdicts:** Pending, Approved, Changes Requested, Rejected

### Tabs

The Review panel has multiple tabs:

1. **Changes** -- staged/unstaged file list with diff viewer
2. **Commits** -- recent commit history
3. **Branches** -- branch management and switching
4. **PRs** -- pull request creation and management

### How to Test

- [ ] Open Review panel -- verify it detects your Git repository
- [ ] Make a file change -- verify it appears in the changes list with correct status
- [ ] Click a file to view the inline diff
- [ ] Stage a file and verify it moves to the staged section
- [ ] Enter a commit message and commit
- [ ] Use "AI Commit Message" to generate a message automatically
- [ ] Create a new branch and switch to it
- [ ] Test Gitflow initialization and feature start/finish
- [ ] Push to remote
- [ ] Verify recent commits show correctly

---

## 15. Skills & Marketplace

### Features

- **4-tab interface:** Installed, Directory, Create, Add Source
- **Installed skills** with integrity hash tracking and enable/disable toggle
- **Directory** with categories: Code Quality, Testing, DevOps, Security, Documentation, Database, Productivity, Other
- **Search and filter** by category
- **Install/Remove** community skills with security scanning
- **Create** custom skills
- **Add Source** for third-party skill repositories
- **Autonomous Skill Authoring** -- 6-step pipeline: Search directory -> Research domain -> Generate skill -> Security scan -> Smoke test -> Install

### How to Test

- [ ] Open Skills panel -- verify "Installed" tab shows built-in skills
- [ ] Switch to "Directory" tab -- browse available skills by category
- [ ] Search for a skill by name
- [ ] Install a skill and verify it appears in "Installed"
- [ ] Toggle a skill on/off and verify the change persists
- [ ] Switch to "Create" tab -- verify the skill creation interface
- [ ] Switch to "Add Source" tab -- verify source management

---

## 16. Routing

### Features

- **Smart model routing** that assigns the best model per task
- **Auto-routing** toggle (enable in Settings)
- **Task-type to model-tier mappings** with performance scores
- **Custom routing rules** -- name, condition, target model, enabled/disabled
- **Provider health monitoring:** Healthy, Degraded, Down
- **Performance metrics:** total requests, failures, avg latency, healthy provider count
- **Auto-fallback** when a provider goes down
- **Learning integration** -- the LearnerTierAdjuster feeds outcome data back into routing decisions

### How to Test

- [ ] Open Routing panel -- verify provider status entries appear
- [ ] Check each configured provider shows health status
- [ ] Enable auto-routing in Settings
- [ ] Send messages with different complexity levels and observe model selection
- [ ] Add a custom routing rule and verify it takes effect
- [ ] Disable a provider's API key and verify fallback engages
- [ ] Verify performance metrics update after each request

---

## 17. Models Browser

### Features

- **Browse all available models** across all providers
- **Two view modes:** Browse (all models) and Project (curated list)
- **Provider filter** -- filter by specific provider
- **Search** by model name
- **API key gating** -- only shows models from providers with valid keys
- **Cloud catalog fetching** from OpenRouter, OpenAI, Anthropic, Google, Groq, HuggingFace
- **Local model discovery** from Ollama, LM Studio, and generic local providers
- **Add/remove models** to your project list
- **Tier guide** showing model capability tiers
- **Collapsible provider groups** with max 10 visible per group before expansion

### How to Test

- [ ] Open Models Browser -- verify it shows models from your configured providers
- [ ] Search for a model by name (e.g., "claude" or "gpt")
- [ ] Filter by provider
- [ ] Add a model to your Project list
- [ ] Switch to "Project" view mode -- verify only your curated models appear
- [ ] Remove a model from the project list
- [ ] If Ollama is running, verify local models appear under discovery
- [ ] Verify model tier tags are shown correctly

---

## 18. Learning

### Features

Hive learns from your interactions over time:

- **Outcome Tracker** -- records whether AI responses were accepted, edited, or rejected
- **Routing Learner** -- adjusts model tier assignments based on observed quality (analysis every 50 interactions)
- **Preference Model** -- learns your preferences (e.g., tone, detail level) with confidence scores
- **Prompt Evolver** -- refines agent prompts based on quality scores, with version history and rollback
- **Pattern Library** -- extracts coding patterns from accepted code for style consistency
- **Self Evaluator** -- periodic self-assessment of overall quality (every 200 interactions)
- **Learning Log** -- transparency view of all learning events
- **User controls:** reject preferences, accept/rollback prompt refinements, reset all learned data

### How to Test

- [ ] Open Learning panel -- verify initial state shows 0 interactions
- [ ] Have several conversations and accept/reject responses
- [ ] Verify interaction count increments
- [ ] Check the learning log shows recorded outcomes
- [ ] Verify preferences start appearing after multiple interactions with consistent patterns
- [ ] Test "Reject Preference" -- remove a learned preference
- [ ] Test "Reset All" -- clear all learned data
- [ ] Verify prompt evolution after extended use

---

## 19. Shield (Security)

### Features

**HiveShield** scans every outbound message through 4 layers:

1. **PII Detection** -- 11+ types: email, phone, SSN, credit card, IP address, and more. Supports cloaking modes (replace PII with placeholders).
2. **Secrets Scanning** -- API keys, tokens, passwords, private keys with risk-level classification.
3. **Vulnerability Assessment** -- Prompt injection detection, jailbreak attempts, unsafe code patterns.
4. **Access Control** -- Policy-based data classification with provider trust levels.

### Panel Contents

- **Shield enabled/disabled** toggle
- **Counters:** PII detections, secrets blocked, threats caught
- **Recent events** with timestamp, event type, severity (critical/high/medium/low/info), and detail
- **Provider access policies** showing trust level, max classification, and PII cloaking status

### How to Test

- [ ] Open Shield panel -- verify shield is enabled by default
- [ ] Send a message containing a fake email address -- verify PII detection event appears
- [ ] Send a message containing `sk-test123456789` -- verify secrets scanning catches it
- [ ] Verify event severity color coding (red=critical/high, yellow=medium/warning, cyan=low/info)
- [ ] Check provider policies list shows correct trust levels
- [ ] Toggle shield off/on and verify behavior changes

---

## 20. Assistant (Personal)

### Features

The Personal Assistant panel requires connected accounts (set up in Settings):

- **Email Triage** -- Gmail and Outlook inbox polling, AI-powered digest generation and reply drafting
- **Calendar Integration** -- Google Calendar and Outlook event fetching, daily briefings, conflict detection
- **Reminders** -- Time-based, recurring (cron expressions), and event-triggered with native OS notifications
- **Approval Workflows** -- Multi-level approval (Low/Medium/High/Critical) with audit trails
- **Document Generation** -- Export to 7 formats: PDF, DOCX, XLSX, PPTX, CSV, HTML, Markdown
- **Smart Home** -- Philips Hue lighting control (scenes, routines, individual light states)
- **Voice Assistant** -- Wake-word detection ("hey hive", "ok hive"), natural-language commands, intent classification (SendMessage, SearchFiles, RunCommand, OpenPanel, CreateTask, ReadNotifications, CheckSchedule), voice states (Idle, Listening, Processing, Speaking, Error)

### How to Test

- [ ] Open Assistant panel -- verify it loads (may show empty state without connected accounts)
- [ ] Connect a Google account in Settings (see Connected Accounts section)
- [ ] Verify email data appears in the Assistant panel
- [ ] Verify calendar events appear
- [ ] Create a reminder and verify it fires at the correct time
- [ ] Generate a document in each format (PDF, DOCX, XLSX, PPTX, CSV, HTML, MD)
- [ ] Test voice commands if a microphone is available

---

## 21. Token Launch (Blockchain)

### Features

A 4-step wizard for deploying tokens on blockchain:

1. **Select Chain** -- Choose from Solana (SPL Token), Ethereum (ERC-20), or Base (ERC-20)
2. **Token Details** -- Configure name, symbol, supply, decimals (defaults: 9 for Solana, 18 for EVM)
3. **Wallet Setup** -- Connect or create a wallet (stored encrypted in `~/.hive/wallets.enc`)
4. **Deploy** -- Execute the deployment transaction

### Wallet Support

- **EVM chains:** Ethereum, Base (7 EVM chains total)
- **Solana**
- **Encrypted key storage** using AES-256-GCM with Argon2id key derivation
- **RPC config** with default endpoints for all supported chains

### How to Test

- [ ] Open Token Launch panel -- verify the 4-step wizard appears
- [ ] Select each chain option and verify details update (SPL vs ERC-20, decimal defaults)
- [ ] Fill in token details (name, symbol, supply)
- [ ] Proceed to Wallet Setup -- verify wallet management UI
- [ ] Verify wallets persist in `~/.hive/wallets.enc` (encrypted)
- [ ] Test deployment on a testnet (if available)

---

## 22. Settings

### Configuration Options

#### API Keys (Left Column)
| Provider | Key Field |
|----------|-----------|
| Anthropic | API key |
| OpenAI | API key |
| OpenRouter | API key |
| Google | API key |
| Groq | API key |
| HuggingFace | API key |
| LiteLLM | URL + API key |
| ElevenLabs | API key (TTS) |
| Telnyx | API key (TTS) |

#### Local Providers
| Provider | Config |
|----------|--------|
| Ollama | URL (default: `http://localhost:11434`) |
| LM Studio | URL (default: `http://localhost:1234`) |
| Generic Local | URL |

#### General Settings
| Setting | Type | Default |
|---------|------|---------|
| Privacy Mode | Toggle | Off |
| Auto Routing | Toggle | On |
| Auto Update | Toggle | On |
| Notifications | Toggle | On |
| Default Model | Dropdown | (from project models) |
| Theme | Select | Dark |
| Font Size | Number | 14 |
| Log Level | Select | Info |
| Daily Budget (USD) | Number | $10.00 |
| Monthly Budget (USD) | Number | $100.00 |

#### TTS (Text-to-Speech)
| Setting | Type | Default |
|---------|------|---------|
| TTS Enabled | Toggle | Off |
| TTS Auto-Speak | Toggle | Off |
| TTS Provider | Select | Qwen3 |
| TTS Speed | Slider | 1.0 |
| ClawdTalk Enabled | Toggle | Off |

TTS Providers: OpenAI TTS, ElevenLabs, Telnyx, F5, Qwen3, HuggingFace

#### Connected Accounts (Right Column)

OAuth integration with:
- **Google** -- Gmail, Calendar, Drive, Contacts, Tasks
- **Microsoft** -- Outlook Email, Calendar, Teams
- **GitHub** -- Repos, Issues, PRs, Activity feed
- **Slack** -- Channels, DMs, Mentions
- **Discord** -- Servers, Channels, DMs
- **Telegram** -- Chats, Groups, Bots

### How to Test

- [ ] Open Settings (`Cmd/Ctrl + ,`)
- [ ] Enter an API key for at least one provider
- [ ] Verify the key persists after closing and reopening Settings
- [ ] Toggle Privacy Mode and verify it affects AI interactions
- [ ] Toggle Auto Routing
- [ ] Change theme/font size and verify UI updates
- [ ] Set budget limits and verify they appear in Costs panel
- [ ] Enable TTS and test speech output
- [ ] Click "Connect" for a platform -- verify OAuth flow opens in browser
- [ ] After connecting, verify data appears in the Assistant panel
- [ ] Change log level and verify Logs panel reflects the change

---

## 23. Workflow Builder

### Features

A visual drag-and-drop workflow editor:

- **Canvas** with node palette on left, properties inspector on right
- **Node types** with distinct colors:
  - Green: Trigger nodes
  - Cyan: Action nodes (Run Command, Call API)
  - Yellow: Condition nodes
  - Pink: Output/End nodes
- **Edge connections** form the execution path between nodes
- **Save/Load** workflow configurations
- **Convert and Run** -- workflows are converted to automation workflows and executed by the agent system
- **Undo/Redo** support

### Node Types

| Type | Color | Purpose |
|------|-------|---------|
| Trigger | Green | What starts the workflow (manual, cron, event, webhook) |
| Run Command | Cyan | Execute a shell command |
| Call API | Cyan | Make an HTTP API call |
| Send Notification | Cyan | Send a notification |
| Condition | Yellow | Branch based on a condition |
| End | Pink | Terminate the workflow |

### How to Test

- [ ] Open Workflow Builder (`Cmd/Ctrl + Shift + W`)
- [ ] Verify node palette appears on the left
- [ ] Add each node type to the canvas
- [ ] Click a node to see its properties in the right inspector
- [ ] Connect nodes to form a workflow path
- [ ] Save the workflow (`Cmd/Ctrl + S`)
- [ ] Run the workflow (`Cmd/Ctrl + Shift + R`)
- [ ] Delete a node (`Delete` or `Backspace`)
- [ ] Test Undo/Redo (`Cmd/Ctrl + Z` / `Cmd/Ctrl + Shift + Z`)

---

## 24. Help

### Features

- **Quick Start guide** -- 5 steps to get running
- **Visual Workflow Builder tutorial** -- step-by-step with built-in templates
- **AI Agent Channels guide** -- channel descriptions and usage
- **Connected Accounts setup** -- OAuth flow walkthrough
- **Personal Assistant feature guide**
- **Security & Search overview** -- HiveShield 4 layers + FTS5 + encryption
- **Project-Oriented Flow** explanation
- **Keyboard shortcuts** reference
- **Features Overview** -- 20 feature cards in 2-column grid
- **Open Source Credits** -- tiered dependency listing (GPUI, Tokio, Rust + categories)
- **About section** with version and links
- **Support section** with bug report and feature request links

### How to Test

- [ ] Open Help panel -- verify all sections load and render correctly
- [ ] Scroll through the entire page
- [ ] Verify version badge shows correct version
- [ ] Check all feature cards are present (should be 20)
- [ ] Verify keyboard shortcuts match actual functionality
- [ ] Check Open Source Credits render correctly (3 tiers)

---

## 25. System Tray & Background Mode

### Features

- **Tray icon** in system tray (Windows/Linux) or menu bar (macOS)
- **Toggle visibility** from tray -- show/hide the main window
- **Close-to-tray** behavior on window close:
  - Prompt: "Quit Hive", "Minimize to Tray", or "Cancel"
  - If minimized, Hive continues running for scheduled tasks and reminders
- **Quit from tray** completely exits the app

### How to Test

- [ ] Close the main window -- verify the prompt appears
- [ ] Click "Minimize to Tray" -- verify window disappears but tray icon remains
- [ ] Click the tray icon -- verify window reappears
- [ ] Click "Quit Hive" from the prompt -- verify app fully exits
- [ ] While minimized, verify scheduled reminders still fire

---

## 26. Feature Integration Map

This shows how features connect and feed into each other:

```
Settings
  |
  +--> API Keys --> AI Service --> Chat (streaming responses)
  |                    |
  |                    +--> Model Router --> Routing Panel
  |                    |       |
  |                    |       +--> Learning (tier adjustments)
  |                    |
  |                    +--> Cost Tracker --> Costs Panel
  |
  +--> Connected Accounts --> Assistant Panel (emails, calendar)
  |
  +--> Privacy Mode --> HiveShield --> Shield Panel

Chat --> History (stored in SQLite, FTS5 indexed)
  |
  +--> Messages --> Learning (outcome tracking)

Specs --> Agents (orchestration targets)
  |
  +--> Kanban (task tracking)

Agents --> Monitor (real-time status)
  |
  +--> Workflows (execution engine)
  |       |
  |       +--> Workflow Builder (visual editor)
  |
  +--> Channels (multi-agent chat)

Review (Git) --> Agents (code review workflows)

Skills --> Agents (extend capabilities)
  |
  +--> Marketplace (community skills)

Models Browser --> Settings (project model list)
  |
  +--> Routing (model selection)

Token Launch --> Wallet Store (encrypted keys)
```

### Key Integration Flows

1. **Chat to Learning loop:** User chats --> AI responds --> User accepts/edits/rejects --> Learning records outcome --> Routing adjusts model selection --> Better responses over time

2. **Project development flow:** Create Spec --> Plan in Kanban --> Agents execute (HiveMind swarm) --> Monitor progress --> Review code (Git Ops) --> Deploy via workflow

3. **Security flow:** User types message --> HiveShield scans (PII, secrets, injection) --> Shield panel shows events --> Safe content sent to AI provider with appropriate trust level

4. **Model management flow:** Browse Models --> Add to project list --> Settings model selector shows curated list --> Auto-routing assigns per task --> Costs tracks spending

---

## 27. End-to-End Test Scenarios

### Scenario 1: First-Time Setup & Chat

1. Launch Hive
2. Open Settings (`Cmd/Ctrl + ,`)
3. Enter an Anthropic API key
4. Verify the key persists
5. Open Chat (`Cmd/Ctrl + 1`)
6. Send "Hello, what can you do?"
7. Verify streaming response
8. Open Costs -- verify the request appears with token counts
9. Open History -- verify the conversation is listed
10. Search for "hello" in History -- verify it finds the conversation

### Scenario 2: Full Development Workflow

1. Create a Spec describing a small feature
2. Open Kanban -- create a task for it
3. Open Agents -- run a Build & Test workflow
4. Monitor progress in Monitor panel
5. When complete, open Review to see code changes
6. Stage, commit, and push changes
7. Verify cost tracking in Costs panel

### Scenario 3: Multi-Agent Channel Conversation

1. Open Channels (`Cmd/Ctrl + 7`)
2. Select #general channel
3. Verify all 9 agents are in the presence panel
4. Ask a complex question that requires multiple perspectives
5. Observe multiple agents responding with their specialized knowledge
6. Switch to #code-review and ask for a code review
7. Verify only review-focused agents respond

### Scenario 4: Model Selection & Routing

1. Open Models Browser -- add 3 models to your project list
2. Enable Auto Routing in Settings
3. Open Chat and send messages of varying complexity
4. Open Routing panel -- verify different models were selected based on task type
5. Open Learning panel -- verify outcomes are being tracked
6. After 50+ interactions, check if routing has adjusted tier assignments

### Scenario 5: Security & Privacy

1. Open Shield panel -- verify shield is enabled
2. Toggle Privacy Mode (`Cmd/Ctrl + P`)
3. Send a message containing a fake email and phone number
4. Open Shield -- verify PII detection events
5. Send a message containing a fake API key
6. Verify secrets scanning catches it
7. Review provider policies in Shield panel

### Scenario 6: Personal Assistant

1. Open Settings -- connect a Google account
2. Open Assistant panel -- verify email and calendar data loads
3. Create a reminder for 5 minutes from now
4. Minimize Hive to tray
5. Verify the reminder fires as a native OS notification
6. Generate a document in PDF and DOCX formats

### Scenario 7: Visual Workflow Builder

1. Open Workflow Builder (`Cmd/Ctrl + Shift + W`)
2. Add a Trigger node (manual trigger)
3. Add two Run Command nodes
4. Add a Condition node
5. Add an End node
6. Connect them in a flow: Trigger --> Cmd1 --> Condition --> Cmd2 --> End
7. Save the workflow
8. Run the workflow
9. Open Agents panel -- verify the workflow run appears
10. Open Monitor -- verify agents are working

---

## Quick Reference: MCP Server Built-in Tools

The agents use 6 built-in tools via the MCP server:

| Tool | Purpose |
|------|---------|
| `read_file` | Read file contents from the workspace |
| `write_file` | Write/create files in the workspace |
| `run_command` | Execute shell commands |
| `search_symbol` | Search for symbols/patterns in code |
| `find_references` | Find references to a symbol across the codebase |
| `list_directory` | List files and directories |

---

## Quick Reference: Data Locations

| Data | Location |
|------|----------|
| Configuration | `~/.hive/config.toml` |
| Main database | `~/.hive/memory.db` |
| Learning database | `~/.hive/learning.db` |
| Assistant database | `~/.hive/assistant.db` |
| Wallet store | `~/.hive/wallets.enc` |
| Session state | `~/.hive/session.json` |
| Conversations (JSON) | `~/.hive/conversations/` |
| Workflows (YAML) | `.hive/workflows/` (project-relative) |
| Logs | SQLite + `~/.hive/logs/` |
