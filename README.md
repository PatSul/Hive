<p align="center">
  <img src="hive/assets/hive_bee.png" width="80" alt="Hive logo" />
</p>

<h1 align="center">Hive</h1>

<p align="center">
  <strong>Your AI that learns, protects, and works while you sleep.</strong>
</p>

<p align="center">
  <a href="https://hivecode.app"><strong>hivecode.app</strong></a>
</p>

<p align="center">
  <a href="https://hivecode.app"><img src="https://img.shields.io/badge/website-hivecode.app-f59e0b" alt="Website" /></a>
  <a href="https://github.com/PatSul/Hive/releases"><img src="https://img.shields.io/github/v/release/PatSul/Hive?label=download&color=brightgreen&cache=1" alt="Download" /></a>
  <img src="https://img.shields.io/badge/version-0.3.26-blue" alt="Version" />
  <img src="https://img.shields.io/badge/language-Rust-orange?logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/tests-targeted%20matrix-brightgreen" alt="Tests" />
  <img src="https://img.shields.io/badge/crates-21-blue" alt="Crates" />
  <img src="https://img.shields.io/badge/warnings-tracked-yellow" alt="Warnings" />
  <img src="https://img.shields.io/badge/lines-200k%2B-informational" alt="Lines of Rust" />
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20(Apple%20Silicon)%20%7C%20Linux%20(x64%20%2B%20ARM64)-informational" alt="Windows | macOS (Apple Silicon) | Linux (x64 + ARM64)" />
  <img src="https://img.shields.io/badge/UI-GPUI-blueviolet" alt="GPUI" />
</p>

---

## What Is Hive?

Hive is a **native Rust desktop AI platform** built on [GPUI](https://gpui.rs) — no Electron, no web wrappers. It unifies a development environment, a personal assistant framework, and a security-first architecture into a single application. Instead of one chatbot, Hive runs a **multi-agent swarm** that can plan, build, test, and orchestrate workflows while learning your preferences over time — all while ensuring no secret or PII ever leaves your machine without approval.

What makes Hive different: it **learns from every interaction** (locally, privately), it **remembers context across conversations** via LanceDB vector memory, it **detects its own knowledge gaps** and autonomously researches and acquires new skills, and it **federates** across instances for distributed swarm execution.

---

## The Three Pillars

<table>
<tr>
<td width="33%" valign="top">

### Development Excellence
- Multi-agent swarm (Queen + teams)
- 15 AI providers with capability-aware routing
- **LanceDB vector memory** (chunks + durable memories)
- **Embedding providers** (OpenAI + Ollama)
- **Background code indexing** with change detection
- Git worktree isolation per team
- Full Git Ops (commits, PRs, branches, gitflow, LFS)
- Context engine (TF-IDF scoring + RAG + vector search)
- **TOON encoding** — token-efficient prompt compression (~30-40% savings)
- Cost tracking & budget enforcement
- Code review & testing automation
- **Universal Skills** — one set of TOML-based skills shared across all AI models
- Skills Marketplace (15 built-in + user-created skills)
- Autonomous skill acquisition (self-teaching)
- Automation workflows (cron, event, webhook triggers)
- Docker & Kubernetes orchestration
- Database connectivity (Postgres, MySQL, SQLite)
- Cloud platform integration (AWS, Azure, GCP)
- Project management (Jira, Linear, Asana)
- Knowledge base sync (Notion, Obsidian)
- Git hosting (GitHub, GitLab, Bitbucket)
- **Tool approval gate** with diff preview (approve/reject file writes)
- **Built-in terminal** (interactive shell with streaming output)
- **Built-in code viewer** (syntax-highlighted file browser)
- MCP client + server (19 tools)
- P2P federation across instances
- **Remote control** (WebSocket relay, QR pairing, web UI)
- **Terminal CLI client** (chat, sync, config, models)
- **A2A protocol** (Agent-to-Agent interoperability)

</td>
<td width="33%" valign="top">

### Assistant Excellence
- Email triage & AI-powered drafting
- Calendar integration & daily briefings
- Reminders (time, recurring cron, event-triggered)
- Approval workflows with audit trails
- Document generation (7 formats)
- Smart home control
- Voice assistant (wake word + intent)

</td>
<td width="33%" valign="top">

### Safety Excellence
- PII detection (11+ types)
- Secrets scanning with risk levels
- Vulnerability assessment
- SecurityGateway command filtering
- Encrypted storage (AES-256-GCM)
- Provider trust-based access control
- **Memory flush** — extract insights before context compaction
- Local-first — no telemetry

</td>
</tr>
</table>

---

## AI & Multi-Agent System

Hive does not use a single AI agent. It uses a **hierarchical swarm** modeled on a beehive:

```
                    +-------------+
                    |    QUEEN    |   Meta-coordinator
                    |  (Planning) |   Goal decomposition
                    +------+------+   Budget enforcement
                           |          Cross-team synthesis
              +------------+------------+
              |            |            |
        +-----v----+ +----v-----+ +----v-----+
        |  TEAM 1  | |  TEAM 2  | |  TEAM 3  |
        | HiveMind | |Coordinator| |SingleShot|
        +----+-----+ +----+-----+ +----------+
             |             |
       +-----+-----+  +---+---+
       |     |     |  |       |
      Arch  Code  Rev Inv    Impl
```

**Queen** decomposes high-level goals into team objectives with dependency ordering, dispatches teams with the appropriate orchestration mode, enforces budget and time limits, shares cross-team insights, synthesizes results, and records learnings to collective memory.

**HiveMind teams** use specialized agents — Architect, Coder, Reviewer, Tester, Security — that reach consensus through structured debate.

**Coordinator teams** decompose work into dependency-ordered tasks (investigate, implement, verify) with persona-specific prompts.

Every team gets its own **git worktree** (`swarm/{run_id}/{team_id}`) for conflict-free parallel execution, merging back on completion.

### AI Providers

15 providers with **capability-aware routing** and fallback:

| Cloud | Local |
|---|---|
| Anthropic (Claude) | Ollama |
| OpenAI (GPT) | LM Studio |
| Google (Gemini) | Generic OpenAI-compatible |
| OpenRouter (100+ models) | LiteLLM proxy |
| Groq (fast inference) | |
| xAI (Grok) | |
| Venice AI | |
| Mistral | |
| Doubao | |
| HuggingFace | |
| HiveGateway (cloud proxy) | |

Features: **capability-aware task routing** (12 task types, 19 model profiles), complexity classification, 14-entry fallback chain, per-model cost tracking, streaming support, budget enforcement, speculative decoding. Venice uses OpenAI-compatible SSE streaming via `/api/v1/chat/completions`.

### Streaming

All AI responses stream token-by-token through the UI. Streaming is implemented end-to-end: SSE parsing at the provider layer, async channel transport, and incremental UI rendering. Shell output streams in real time through async `mpsc` channels. WebSocket-based P2P transport supports bidirectional streaming between federated instances.

---

## Vector Memory & RAG

Hive maintains **durable, searchable memory** across conversations using LanceDB — a columnar vector database that runs embedded (no external server).

```
User query
    |
    v
EmbeddingProvider ─── OpenAI text-embedding-3-small (1536d)
    |                  or Ollama nomic-embed-text (768d)
    |                  with automatic fallback
    v
┌─────────────────────────────────────────────┐
│              LanceDB MemoryStore             │
│                                              │
│  chunks table          memories table        │
│  ┌──────────────┐      ┌──────────────────┐  │
│  │ source_file  │      │ content          │  │
│  │ content      │      │ category         │  │
│  │ embedding    │      │ importance       │  │
│  │ start_line   │      │ embedding        │  │
│  │ end_line     │      │ conversation_id  │  │
│  └──────────────┘      │ decay_exempt     │  │
│                        └──────────────────┘  │
└─────────────────────────────────────────────┘
    |                          |
    v                          v
Code search results       Recalled memories
    |                          |
    v                          v
# Retrieved Context       # Recalled Memories
(injected as system msg)  (dedicated system msg)
```

### How It Works

| Component | Purpose |
|---|---|
| **EmbeddingProvider** | Async trait with OpenAI, Ollama, and Mock implementations. Auto-detects available provider at startup. |
| **MemoryStore** | LanceDB wrapper managing two tables — `chunks` (indexed code) and `memories` (durable insights). Vector similarity search via nearest-neighbor queries. |
| **HiveMemory** | Unified API combining MemoryStore + embeddings. 50-line chunks with 10-line overlap for code indexing. Combined query returns both code chunks and recalled memories. |
| **BackgroundIndexer** | Walks project directories recursively, skips binary/hidden/vendor dirs, tracks file hashes for incremental re-indexing. Triggered on workspace open. |
| **MemoryExtractor** | Pre-compaction flush: builds an LLM prompt to extract key insights from a conversation before context window is compacted. Parses JSON response, filters by importance threshold. |

### Memory Categories

Memories are categorized for relevance scoring:

- **UserPreference** — Coding style, tooling choices, formatting preferences
- **CodePattern** — Recurring patterns and architectural decisions
- **TaskProgress** — What was done, what's pending
- **Decision** — Technical decisions and their rationale
- **General** — Catch-all for other durable insights

### Injection Flow

On every message send:
1. **Code chunks** from the indexed workspace are injected into `# Retrieved Context` (curated by ContextEngine)
2. **Recalled memories** from previous conversations are injected as a separate `# Recalled Memories` system message with importance scores and categories
3. Both are available to the AI alongside the conversation history

---

## Autonomous Skill Acquisition

Hive doesn't just execute what it already knows — it **recognizes what it doesn't know** and teaches itself. This is the closed-loop system that lets Hive grow its own capabilities in real time:

```
User request
    |
    v
Competence Detection ─── "I know this" ───> Normal execution
    |
    "I don't know this"
    |
    v
Search ClawdHub / Sources ─── Found sufficient skill? ───> Install & use
    |
    Not found (or insufficient)
    |
    v
Knowledge Acquisition ───> Fetch docs, parse, synthesize
    |
    v
Skill Authoring Pipeline ───> Generate, security-scan, test, install
    |
    v
New skill available for future requests
```

### Competence Detection

The **CompetenceDetector** scores Hive's confidence on every incoming request using a weighted formula across four signals:

| Signal | Weight | Source |
|---|---|---|
| Skill match | 30% | Exact trigger/name match in skills registry |
| Pattern match | 20% | Keyword overlap with marketplace skill descriptions |
| Memory match | 15% | Relevant entries in collective memory |
| AI assessment | 35% | Lightweight model call rating confidence 0-10 |

When confidence drops below the learning threshold (default 0.4), the system identifies **competence gaps** — missing skills, missing knowledge, low-quality skills, or absent patterns — and triggers the acquisition pipeline automatically.

A **quick assessment** mode (no AI call) is available for low-latency checks using purely pattern-based matching.

### Knowledge Acquisition

The **KnowledgeAcquisitionAgent** is a research agent that autonomously:

1. **Identifies** the best documentation URLs for a topic (AI-orchestrated)
2. **Fetches** pages via HTTPS with domain allowlisting and private-IP blocking
3. **Parses** HTML to clean text — strips scripts, styles, nav, footers; extracts `<code>` blocks with language detection
4. **Caches** locally (`~/.hive/knowledge/`) with SHA-256 content hashing and configurable TTL (default 7 days)
5. **Synthesizes** knowledge via AI into structured summaries (key concepts, relevant commands, code examples)
6. **Injects** results into the ContextEngine as `Documentation` sources for future queries

Security: HTTPS-only, 23+ allowlisted documentation domains (docs.rs, kubernetes.io, react.dev, MDN, etc.), private IP rejection, content scanned for injection before storage, configurable page-size limits.

### Skill Authoring Pipeline

When no existing skill is found, the **SkillAuthoringPipeline** creates one:

1. **Search existing skills first** — Queries ClawdHub directory and remote sources. Each candidate is AI-scored for sufficiency (0-10). Skills scoring >= 7 are installed directly.
2. **Research** — Delegates to KnowledgeAcquisitionAgent if no sufficient existing skill is found
3. **Generate** — AI creates a skill definition (name, trigger, category, prompt template, test input)
4. **Security scan** — Runs the same 6-category injection scan used for community skills. Retries up to 2x on failure.
5. **Test** — Validates the skill produces relevant output for the sample input
6. **Install** — Adds to marketplace with `/hive-` trigger prefix, disabled by default until user enables

All auto-generated skills are logged to CollectiveMemory for auditability. The pipeline fails gracefully at every step — a failed scan or test never installs a broken skill.

### Universal Skills (Cross-Model)

Skills are stored as TOML files in `~/.hive/skills/` with **capability tags** — a single skill definition works across all 27 AI providers. The runtime adapts execution per model:

```toml
[skill]
name = "code-review"
description = "Review code for bugs, style, and security"
category = "code_generation"
author = "hivecode"
source = "builtin"

[requirements]
capabilities = ["tool_use"]           # model MUST have these
preferred = ["extended_thinking"]      # enhances prompt if available
min_tier = "mid"

[prompt]
template = "Analyze code for bugs, security issues, and improvements."
tool_use_hint = "Use read_file to examine the code."
structured_output_hint = "Return findings as JSON with severity, file, line."

[tools]
required = ["read_file"]
optional = ["search_files"]
```

The **SkillExecutor** validates that the active model satisfies the skill's requirements, then enhances the prompt based on available capabilities (e.g., prepends thinking instructions for models with `ExtendedThinking`, requests JSON for models with `StructuredOutput`). Incompatible models get a clear error message suggesting alternatives.

15 built-in skills ship as embedded TOML and are written to `~/.hive/skills/` on first run. Users can edit, disable, or create new skills — all changes persist to disk with SHA-256 integrity verification and injection scanning.

---

## Personal Assistant

The assistant uses the same AI infrastructure as the development platform — same model routing, same security scanning, same learning loop.

| Capability | Details |
|---|---|
| **Email** | Gmail and Outlook inbox polling via real REST APIs (Gmail API, Microsoft Graph v1.0). Email digest generation, AI-powered composition and reply drafting with shield-scanned outbound content. |
| **Calendar** | Google Calendar and Outlook event fetching, daily briefing generation, conflict detection and scheduling logic. |
| **Reminders** | Time-based, recurring (cron), and event-triggered. Snooze/dismiss. Project-scoped. Native OS notifications. SQLite persistence. |
| **Approvals** | Multi-level workflows (Low / Medium / High / Critical). Submit, approve, reject with severity tracking. |
| **Documents** | Generate CSV, DOCX, XLSX, HTML, Markdown, PDF, and PPTX from templates or AI. |
| **Smart Home** | Philips Hue control — lighting scenes, routines, individual light states. |
| **Plugins** | `AssistantPlugin` trait with `PluginRegistry`. First production plugin: `ReminderPlugin`. |
| **Scheduler** | Background tick driver (60-second interval) for automated reminders and recurring tasks. Native OS thread with dedicated tokio runtime. |

---

## Security & Privacy

Security is the **foundation**, not a feature bolted on. Every outgoing message is scanned. Every command is validated.

### HiveShield — 4 Layers of Protection

| Layer | What It Does |
|---|---|
| **PII Detection** | 11+ types (email, phone, SSN, credit card, IP, name, address, DOB, passport, driver's license, bank account). Cloaking modes: Placeholder, Hash, Redact. |
| **Secrets Scanning** | API keys, tokens, passwords, private keys. Risk levels: Critical, High, Medium, Low. |
| **Vulnerability Assessment** | Prompt injection detection, jailbreak attempts, unsafe code patterns, threat scoring. |
| **Access Control** | Policy-based data classification. Provider trust levels: Local, Trusted, Standard, Untrusted. |

### SecurityGateway

Hive routes command execution paths through `SecurityGateway` checks and blocks destructive filesystem ops, credential theft, privilege escalation, and common exfiltration patterns.

### Local-First

- All data in `~/.hive/` — config, conversations, learning data, collective memory, kanban boards, vector memory
- Encrypted key storage (AES-256-GCM + Argon2id key derivation)
- **No telemetry. No analytics. No cloud dependency.**
- Cloud providers used only for AI inference when you choose cloud models — and even then, HiveShield scans every request

---

## Self-Improvement Engine

Hive gets smarter every time you use it. Entirely local. No data leaves your machine.

```
  User interacts with Hive
          |
          v
  +-------+--------+
  | Outcome Tracker |  Records: accepted, rejected, edited, ignored
  +-------+--------+
          |
    +-----+-----+-----+-----+
    |     |     |     |     |
    v     v     v     v     v
  Route  Pref  Prompt Pat  Self
  Learn  Model Evolve Lib  Eval
```

| System | Function |
|---|---|
| **Outcome Tracker** | Quality scores per model and task type. Edit distance and follow-up penalties. |
| **Routing Learner** | EMA analysis adjusts model tier selection. Wired into `ModelRouter` via `TierAdjuster`. |
| **Preference Model** | Bayesian confidence tracking. Learns tone, detail level, formatting from observation. |
| **Prompt Evolver** | Versioned prompts per persona. Quality-gated refinements with rollback support. |
| **Pattern Library** | Extracts code patterns from accepted responses (6 languages: Rust, Python, JS/TS, Go, Java/Kotlin, C/C++). |
| **Self-Evaluator** | Comprehensive report every 200 interactions. Trend analysis, misroute rate, cost-per-quality-point. |

All learning data stored locally in SQLite (`~/.hive/learning.db`). Every preference is transparent, reviewable, and deletable.

---

## Automation & Skills

| Feature | Details |
|---|---|
| **Automation Workflows** | Multi-step workflows with triggers (manual, cron schedule, event, webhook) and 6 action types (run command, send message, call API, create task, send notification, execute skill). YAML-based definitions in `~/.hive/workflows/`. Visual drag-and-drop workflow builder in the UI. |
| **Universal Skills** | Cross-model skill system with TOML-based definitions in `~/.hive/skills/`. Capability tags declare requirements (`tool_use`, `extended_thinking`, etc.) and the `SkillExecutor` adapts prompts per model. One skill definition works across all 27 providers. |
| **Skills Marketplace** | Browse, install, remove, and toggle skills from 5 sources (ClawdHub, Anthropic, OpenAI, Google, Community). Create custom skills. Add remote skill sources. 15 built-in skills including 9 integration skills (/slack, /jira, /notion, /db, /docker, /k8s, /deploy, /browse, /index-docs). Security scanning on install. |
| **Autonomous Skill Creation** | When Hive encounters an unfamiliar domain, it searches existing skill sources first, then researches documentation and authors a new skill if nothing sufficient exists. See [Autonomous Skill Acquisition](#autonomous-skill-acquisition). |
| **Personas** | Named agent personalities with custom system prompts, prompt overrides per task type, and configurable model preferences. |
| **Auto-Commit** | Watches for staged changes and generates AI-powered commit messages. |
| **Daily Standups** | Automated agent activity summaries across all teams and workflows. |
| **Voice Assistant** | Wake-word detection, natural-language voice commands, intent recognition, and state-aware responses. |

---

## Terminal & Execution

| Feature | Details |
|---|---|
| **Interactive Terminal** | Built-in terminal panel with a real interactive shell (cmd.exe / bash). Async streaming output, color-coded stdout/stderr, command history echo, kill/restart controls. Backed by `InteractiveShell` with `tokio::process::Command`. |
| **Shell Execution** | Run commands with configurable timeout, async streaming output capture, working directory management, and exit code tracking. Real process spawning via `tokio::process::Command`. |
| **Docker Sandbox** | Full container lifecycle: create, start, stop, exec, pause, unpause, remove. Real Docker CLI integration with simulation fallback for testing. Dual-mode: production and test. |
| **Browser Automation** | Chrome DevTools Protocol over WebSocket: navigation, screenshots, JavaScript evaluation, DOM manipulation. |
| **CLI Service** | Built-in commands (`/doctor`, `/clear`, etc.) and system health checks. |
| **Local AI Detection** | Auto-discovers Ollama, LM Studio, and llama.cpp running on localhost. |

---

## Remote Control

Hive can be controlled from any device on your network — phone, tablet, or another computer — via a built-in web server with real-time streaming.

| Feature | Details |
|---|---|
| **HiveDaemon** | Background service holding canonical session state (active panel, conversation, agent runs). Broadcasts events to all connected clients. |
| **Web Server** | Axum-based HTTP server with embedded static assets. REST API endpoints: `/api/state`, `/api/chat`, `/api/panels`, `/api/agents`. |
| **WebSocket Streaming** | Real-time bidirectional communication for chat streaming, agent status, and panel updates. |
| **QR Pairing** | Secure device pairing via QR code using X25519 elliptic-curve key exchange + AES-GCM encryption. No passwords needed. |
| **Session Journal** | JSONL-based persistence of daemon events for crash recovery and audit. |
| **Event Types** | SendMessage, SwitchPanel, StartAgentTask, CancelAgentTask, StreamChunk, StreamComplete, AgentStatus, StateSnapshot, PanelData. |

---

## Terminal Client (CLI)

A full-featured terminal client for interacting with Hive without the GUI:

```bash
hive chat                    # Interactive chat TUI with streaming
hive chat --model claude-3   # Chat with a specific model
hive models                  # List available AI models
hive status                  # Show account tier, usage, sync status
hive sync push               # Push data to cloud
hive sync pull               # Pull data from cloud
hive config                  # View all config
hive config key value        # Set a config value
hive login                   # Authenticate with Hive Cloud
hive remote                  # Show remote connection status
```

Built with Ratatui for a polished terminal UI with keyboard navigation, scrolling, and real-time streaming display.

---

## Hive Cloud

Optional cloud services for users who want cross-device sync, cloud AI routing, and team features:

| Feature | Details |
|---|---|
| **HiveGateway** | Cloud AI proxy — routes requests through Hive's infrastructure for users without their own API keys. Configurable in Settings. |
| **Cloud Sync** | Push/pull config, conversations, and learning data across devices via encrypted blob storage. |
| **Admin TUI** | Terminal dashboard (`hive-admin`) with 6 tabs: Dashboard, Users, Gateway, Relay, Sync, Teams. Real-time refresh. |
| **Relay** | Event relay service for remote control across networks (not just LAN). |

Cloud features are entirely optional. Hive works fully offline with local models.

---

## P2P Federation

Hive instances can discover and communicate with each other over the network, enabling distributed swarm execution and shared learning.

| Feature | Details |
|---|---|
| **Peer Discovery** | UDP broadcast for automatic LAN discovery, plus manual bootstrap peers |
| **WebSocket Transport** | Bidirectional P2P connections with split-sink/stream architecture |
| **Typed Protocol** | 12 built-in message kinds (Hello, Welcome, Heartbeat, TaskRequest, TaskResult, AgentRelay, ChannelSync, FleetLearn, StateSync, etc.) plus extensible custom types |
| **Channel Sync** | Synchronize agent channel messages across federated instances |
| **Fleet Learning** | Share learning outcomes across a distributed fleet of nodes |
| **Peer Registry** | Persistent tracking of known peers with connection state management |

---

## Integrations

All integrations make **real API calls** to their respective services. Blockchain token deployment operates in simulation mode with real gas/rent price queries. The `deploy_trigger` tool dispatches via deploy scripts, Makefile targets, or GitHub Actions CLI.

<table>
<tr><td><strong>Google</strong></td><td>Gmail (REST API), Calendar, Contacts, Drive, Docs, Sheets, Tasks</td></tr>
<tr><td><strong>Microsoft</strong></td><td>Outlook Email (Graph v1.0), Outlook Calendar</td></tr>
<tr><td><strong>Messaging</strong></td><td>Slack (Web API), Discord, Teams, Telegram, Matrix, WebChat, WhatsApp (Business Cloud API), Signal, Google Chat, iMessage (macOS)</td></tr>
<tr><td><strong>Git Hosting</strong></td><td>GitHub (REST API), GitLab (REST API), Bitbucket (REST API v2.0)</td></tr>
<tr><td><strong>Cloud Platforms</strong></td><td>AWS (EC2, S3, Lambda, CloudWatch), Azure (VMs, Blob, Functions, Monitor), GCP (Compute, Storage, Functions, Logging)</td></tr>
<tr><td><strong>Databases</strong></td><td>PostgreSQL, MySQL, SQLite — query execution, schema introspection, connection pooling</td></tr>
<tr><td><strong>DevOps</strong></td><td>Docker (full container lifecycle), Kubernetes (pods, deployments, services, logs)</td></tr>
<tr><td><strong>Knowledge</strong></td><td>Notion (pages, databases, blocks), Obsidian (vault management, frontmatter)</td></tr>
<tr><td><strong>Project Mgmt</strong></td><td>Jira (issues, projects, transitions), Linear (issues, teams, cycles), Asana (tasks, projects, sections)</td></tr>
<tr><td><strong>Cloud Services</strong></td><td>Cloudflare, Vercel, Supabase</td></tr>
<tr><td><strong>Smart Home</strong></td><td>Philips Hue</td></tr>
<tr><td><strong>Voice</strong></td><td>ClawdTalk (voice-over-phone via Telnyx)</td></tr>
<tr><td><strong>Browser</strong></td><td>Headless Chrome automation (navigation, screenshots, JS evaluation, DOM interaction)</td></tr>
<tr><td><strong>Protocol</strong></td><td>MCP client + server (19 tools), OAuth2 (PKCE), Webhooks, P2P federation</td></tr>
</table>

---

## Blockchain / Web3

| Chain | Features |
|---|---|
| **EVM** (Ethereum, Polygon, Arbitrum, BSC, Avalanche, Optimism, Base) | Wallet management, real JSON-RPC (`eth_getBalance`, `eth_gasPrice`), per-chain RPC configuration, ERC-20 token deployment with cost estimation |
| **Solana** | Wallet management, real JSON-RPC (`getBalance`, `getTokenAccountsByOwner`, `getMinimumBalanceForRentExemption`), SPL token deployment with rent cost estimation |
| **Security** | Encrypted private key storage (AES-256-GCM), no keys ever sent to AI providers |

---

## Persistence & Data Storage

All state persists between sessions. Nothing is lost on restart.

| Data | Storage | Location |
|---|---|---|
| **Conversations** | SQLite + JSON files | `~/.hive/memory.db` + `~/.hive/conversations/{id}.json` |
| **Messages** | SQLite | `~/.hive/memory.db` |
| **Conversation search** | SQLite FTS5 | `~/.hive/memory.db` (Porter stemming + unicode61) |
| **Vector memory** | LanceDB | `~/.hive/memory.lance` |
| **Cost records** | SQLite | `~/.hive/memory.db` |
| **Application logs** | SQLite | `~/.hive/memory.db` |
| **Collective memory** | SQLite (WAL mode) | `~/.hive/memory.db` |
| **Learning data** | SQLite | `~/.hive/learning.db` |
| **Kanban boards** | JSON | `~/.hive/kanban.json` |
| **Config & API keys** | JSON + encrypted vault | `~/.hive/config.json` |
| **Session state** | JSON | `~/.hive/session.json` (window size, crash recovery) |
| **Session journal** | JSONL | `~/.hive/session_journal.jsonl` (remote daemon events) |
| **Knowledge cache** | HTML/text files | `~/.hive/knowledge/` |
| **Workflows** | YAML definitions | `~/.hive/workflows/` |
| **Skills** | TOML with capability tags | `~/.hive/skills/{name}.toml` (15 built-in + user-created) |

On startup, Hive automatically backfills any JSON-only conversations into SQLite and builds FTS5 search indexes. Path traversal protection on all file operations. SQLite databases use WAL mode with `NORMAL` synchronous and foreign key enforcement.

---

## Architecture — 21-Crate Workspace

```
hive/crates/
├── hive_app           Binary entry point — window, tray, build.rs (winres)
│                      3 files · 965 lines
├── hive_ui            Workspace shell, chat service, learning bridge, title/status bars
│                      21 files · 11,000+ lines
├── hive_ui_core       Theme, actions, globals, sidebar, welcome screen
│                      6 files · 900+ lines
├── hive_ui_panels     All panel implementations (20+ panels)
│                      42 files · 26,000+ lines
├── hive_core          Config, SecurityGateway, persistence (SQLite), Kanban, channels, scheduling
│                      18 files · 9,800+ lines
├── hive_ai            10 AI providers, capability-aware router, complexity classifier, context engine,
│                      RAG, embeddings (OpenAI + Ollama), LanceDB memory, background indexer, TOON encoding
│                      50+ files · 22,000+ lines
├── hive_agents        Queen, HiveMind, Coordinator, collective memory, MCP (19 tools),
│                      Universal Skills (SkillLoader + SkillExecutor, TOML persistence),
│                      personas, knowledge acquisition, competence detection, skill authoring
│                      28+ files · 23,000+ lines
├── hive_shield        PII detection, secrets scanning, vulnerability assessment, access control
│                      6 files · 2,005 lines
├── hive_learn         Outcome tracking, routing learner, preference model, prompt evolution
│                      10 files · 5,438 lines
├── hive_assistant     Email, calendar, reminders, approval workflows, daily briefings
│                      13 files · 4,421 lines
├── hive_fs            File operations, git integration, file watchers, search
│                      5 files · 1,145 lines
├── hive_terminal      Command execution, Docker sandbox, browser automation, local AI detection
│                      8 files · 5,869 lines
├── hive_docs          Document generation — CSV, DOCX, XLSX, HTML, Markdown, PDF, PPTX
│                      8 files · 1,478 lines
├── hive_blockchain    EVM + Solana wallets, RPC config, token deployment with real JSON-RPC
│                      6 files · 1,669 lines
├── hive_integrations  Google, Microsoft, GitHub, GitLab, Bitbucket, messaging, databases,
│                      cloud platforms, DevOps, knowledge bases, project management, OAuth2
│                      55 files · 33,007 lines
├── hive_network       P2P federation, WebSocket transport, UDP discovery, peer registry, sync
│                      11 files · 2,765 lines
├── hive_remote        Background daemon, WebSocket relay, QR pairing, web UI, REST API
│                      9 files · 943 lines  (+ 1,916 lines tests)
├── hive_cloud         Cloud Axum backend, WebSocket Relay Hub, JWT Auth, Stripe billing
│                      4 files · 300+ lines [binary: hive_cloud]
├── hive_admin         Cloud admin TUI dashboard (6 tabs: dashboard, users, gateway, relay, sync, teams)
│                      11 files · 905 lines  [binary: hive-admin]
├── hive_a2a           Agent-to-Agent protocol — HTTP server, SSE streaming, task handler, bridge
│                      8 files · 1,200+ lines
└── hive_cli           Terminal AI client (chat, sync, config, models, login, remote, status)
                       12 files · 938 lines  [binary: hive]
```

### Dependency Flow

```
hive_app
  └── hive_ui
        ├── hive_ui_core
        ├── hive_ui_panels
        ├── hive_ai ──────── hive_core
        ├── hive_agents ──── hive_ai, hive_learn, hive_core
        ├── hive_shield
        ├── hive_learn ───── hive_core
        ├── hive_assistant ─ hive_core, hive_ai
        ├── hive_fs
        ├── hive_terminal
        ├── hive_docs
        ├── hive_blockchain
        ├── hive_integrations
        ├── hive_network
        └── hive_remote ──── hive_core, hive_ai, hive_agents, hive_network

hive_a2a ─────────────── hive_core, hive_agents
hive_admin (standalone binary)
hive_cloud (standalone binary) ── hive_core, hive_remote
hive_cli   (standalone binary) ── hive_core
```

---

## UI — 26 Panels

All panels are wired to live backend data. No mock data in the production path. **8 built-in themes** (HiveCode Dark/Light, Nord, Dracula, Solarized Dark, Monokai, One Dark, GitHub Dark) with community voting and custom theme support via `~/.hive/themes/`.

| Panel | Description | Data Source |
|---|---|---|
| Chat | Main AI conversation with streaming responses | AI providers via `ChatService` |
| History | Conversation history browser | `~/.hive/conversations/` |
| Files | Project file browser with built-in code viewer (split layout, syntax highlighting) | Filesystem via `hive_fs` |
| Specs | Specification management | `AppSpecs` global |
| Agents | Multi-agent swarm orchestration with task tree drill-down | `AppAgents` global |
| Workflows | Visual workflow builder (drag-and-drop nodes) | `AppWorkflows` global |
| Channels | Agent messaging channels (Telegram/Slack-style) | `AppChannels` global |
| Kanban | Persistent task board with drag-and-drop | `~/.hive/kanban.json` |
| Monitor | Real-time system monitoring (CPU, RAM, disk, provider status) | `sysctl`, `ps`, `df` |
| Logs | Application logs viewer with level filtering | Tracing subscriber |
| Costs | AI cost tracking and budget with CSV export | `CostTracker` |
| Git Ops | Full git workflow: staging, commits, push, PRs, branches, gitflow, LFS | `git2` + CLI |
| Skills | Universal skill marketplace: browse, install, remove, toggle, create — cross-model TOML skills | `SkillsRegistry` + `SkillExecutor` |
| Routing | Model routing configuration | `ModelRouter` |
| Models | Model registry browser | Provider catalogs |
| Learning | Self-improvement dashboard with metrics, preferences, insights | `LearningService` |
| Shield | Security scanning status | `HiveShield` |
| Assistant | Personal assistant: email, calendar, reminders | `AssistantService` |
| Token Launch | Token deployment wizard with chain selection | `hive_blockchain` |
| Code Map | Symbol browser: functions, structs, traits, enums grouped by file with search | `AppQuickIndex` |
| Prompt Library | Save, browse, and reuse prompt templates with tag-based search | `~/.hive/prompts/` |
| Settings | Application configuration with persist-on-save, cloud account fields | `HiveConfig` |
| Terminal | Interactive shell with real PTY, command history, kill/restart | `InteractiveShell` via `hive_terminal` |
| Network | P2P federation peer browser | `hive_network` |
| Quick Start | Guided project onboarding with goal-driven AI | `AppConfig` + AI providers |
| Help | Documentation and guides | Static content |

---

## Error Handling & Production Quality

Hive is built for production robustness:

- **Graceful error handling** — Production code paths use `Result<T>` with `?` propagation, `.unwrap_or_default()`, or explicit `match` blocks. Remaining `expect()` calls are limited to compile-time-constant regex patterns and application startup invariants.
- **Warning cleanup is tracked continuously** — keep validated crate slices warning-free and use targeted `cargo check` commands while the workspace-wide validation path is being tightened.
- **Clippy is part of the quality bar** — run it on the actively validated crate slices instead of assuming the entire workspace is green by default.
- **Documented APIs** — Public structs, enums, traits, and functions have `///` documentation comments describing purpose and behavior.
- **Extensive automated coverage** — the repo contains thousands of unit and integration tests. Use the commands in `docs/TEST_PLAN.md` for the currently supported verification matrix.

---

## Installation

### Option 1: Download Pre-Built Binary (Recommended)

Grab the latest release for your platform from [**GitHub Releases**](https://github.com/PatSul/Hive/releases).

| Platform | Download | Runtime Requirements |
|---|---|---|
| **Windows** (x64) | `hive-windows-x64.zip` | Windows 10/11, GPU with DirectX 12 |
| **macOS** (Apple Silicon) | `hive-macos-arm64.tar.gz` | macOS 12+, Metal-capable GPU |
| **Linux** (x64) | `hive-linux-x64.tar.gz` | Vulkan-capable GPU + drivers (see below) |
| **Linux** (ARM64) | `hive-linux-arm64.tar.gz` | 64-bit ARM (RPi 5 not supported — no Vulkan GPU), Vulkan-capable GPU + drivers |

**Windows:** Extract the zip, run `hive.exe`. No installer needed.

**macOS:** Extract, then `chmod +x hive && ./hive` (or move to `/usr/local/bin/`).

**Linux:** Extract, then `chmod +x hive && ./hive`. You need Vulkan drivers installed:
```bash
# Ubuntu/Debian
sudo apt install mesa-vulkan-drivers vulkan-tools

# Fedora
sudo dnf install mesa-vulkan-drivers vulkan-tools

# Arch
sudo pacman -S vulkan-icd-loader vulkan-tools
```

> **Note on Raspberry Pi:** Hive requires a Vulkan-capable GPU for rendering. Raspberry Pi's VideoCore GPU does not support Vulkan, so Hive cannot run directly on RPi hardware. The ARM Linux build targets ARM servers and desktops with discrete/integrated GPUs that have Vulkan support (e.g., NVIDIA Jetson, Ampere Altra, AWS Graviton with GPU instances). A headless mode for RPi is planned for a future release.

### Option 2: Build from Source

#### Prerequisites

1. **Rust toolchain** — install from [rustup.rs](https://rustup.rs):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **protoc** (Protocol Buffers compiler) — required by LanceDB:
   ```bash
   # macOS
   brew install protobuf

   # Ubuntu/Debian
   sudo apt install protobuf-compiler

   # Windows (via scoop)
   scoop bucket add extras && scoop install extras/protobuf
   ```

3. **Platform-specific dependencies:**

   <details>
   <summary><strong>Windows</strong></summary>

   - [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022) with C++ workload (`Microsoft.VisualStudio.Component.VC.Tools.x86.x64`)
   - Run from **VS Developer Command Prompt** or set `INCLUDE`/`LIB` environment variables
   </details>

   <details>
   <summary><strong>macOS</strong></summary>

   ```bash
   xcode-select --install
   ```
   </details>

   <details>
   <summary><strong>Linux</strong></summary>

   ```bash
   # Ubuntu/Debian
   sudo apt install build-essential libssl-dev pkg-config \
     libvulkan-dev libwayland-dev libxkbcommon-dev \
     libxcb-shape0-dev libxcb-xfixes0-dev \
     libglib2.0-dev libgtk-3-dev libxdo-dev

   # Fedora
   sudo dnf install gcc openssl-devel pkg-config \
     vulkan-devel wayland-devel libxkbcommon-devel

   # Arch
   sudo pacman -S base-devel openssl pkg-config \
     vulkan-icd-loader wayland libxkbcommon
   ```
   </details>

#### Build & Run

```bash
git clone https://github.com/PatSul/Hive.git
cd Hive/hive
cargo build --release
cargo run --release
```

#### Run Tests

```bash
cd hive
./verify.sh
```

```powershell
cd hive
.\verify.bat
```

---

## Configuration

On first launch, Hive creates `~/.hive/config.json`. Add your API keys to enable cloud providers:

```json
{
  "anthropic_api_key": "sk-ant-...",
  "openai_api_key": "sk-...",
  "google_api_key": "AIza...",
  "ollama_url": "http://localhost:11434",
  "lmstudio_url": "http://localhost:1234"
}
```

All keys are stored locally and never transmitted except to their respective providers. HiveShield scans every outbound request before it leaves your machine.

Configure provider preferences, model routing rules, budget limits, and security policies through the **Settings** panel in the UI.

---

## Project Stats

| Metric | Value |
|---|---|
| Version | 0.3.26 |
| Crates | 21 |
| Rust source files | 400+ |
| Lines of Rust | 200,000+ |
| Tests | Targeted verification matrix |
| Compiler warnings | Tracked per validated slice |
| Clippy warnings | Checked per validated slice |
| Memory footprint | < 50 MB |
| Startup time | < 1 second |
| UI rendering | 120fps (GPU-accelerated via GPUI) |

---

## Agent-to-Agent Protocol (A2A)

Hive implements the [A2A protocol](https://google.github.io/A2A/) for interoperability with external AI agents:

| Feature | Details |
|---|---|
| **HTTP Server** | Axum-based A2A endpoint with JSON-RPC request handling |
| **Agent Card** | Auto-generated agent card advertising Hive's skills and capabilities |
| **Task Handler** | Dispatches incoming A2A messages to the Hive orchestrator |
| **SSE Streaming** | Server-Sent Events for real-time task progress streaming |
| **Bridge** | Bidirectional type conversion between A2A and Hive internal types |
| **Auth Middleware** | API key validation and URL security for incoming requests |
| **Client Discovery** | Caches remote agent cards for efficient repeat communication |

A2A lets Hive participate in multi-agent ecosystems — receiving tasks from and delegating to other A2A-compatible agents across the network.

---

## Changelog

### v0.3.26

**Security Hardening, RAG Pipeline Wiring & MCP Tool Dispatch**

- **Path Traversal Protection** — Template IDs in `PromptLibrary` now validated to `[A-Za-z0-9_-]` only; context file attachments block absolute paths and `..` traversal.
- **JS Eval Approval Gate** — `browser_evaluate_script` validates JavaScript before execution, blocking cookie/storage/fetch access patterns.
- **SSRF Hardening** — URL validation rejects missing hosts, IPv4-mapped IPv6 private addresses (`::ffff:127.0.0.1`), and `None`-host bypass.
- **RAG Pipeline Wiring** — `RagService` (TF-IDF context curation) now flows through the full chain: workspace → Queen → Coordinator → Pipeline `curate_context()`.
- **MCP Integration Tool Dispatch** — All MCP integration tools (messaging, project management, browser, deploy, etc.) are now included in AI requests via `list_tools()` and routed through `route_unknown_to_mcp()` on both normal and rejected-write_file dispatch paths.
- **Real UUID Generation** — Replaced timestamp-based ID generation in prompt templates with `uuid::Uuid::new_v4()`.

### v0.3.25

**Hybrid Pipeline, Code Map, Prompt Library & Context Attachments**

- **Hybrid Task Pipeline** — Deterministic AI execution using the Stripe Minions pattern: `CURATE_CONTEXT → AI_EXECUTE → VALIDATE → retry/complete`. Configurable validation gates (empty-output, refusal detection, security scan, code-block presence, custom patterns) with automatic retry.
- **Code Map Panel** — Symbol browser powered by the background code index. Displays functions, structs, traits, and enums grouped by file with color-coded icons and real-time search.
- **Prompt Library Panel** — Save, browse, and reuse prompt templates stored as JSON in `~/.hive/prompts/`. Search by name, description, or tags; one-click load into chat.
- **Context File Attachments** — Check files in the Files panel to attach them as AI context. Selected files appear as chips above the chat input with token counts.
- **Apply Code Blocks** — AI-generated code blocks with file paths now show an "Apply" button to write changes directly to disk.
- **Markdown Renderer** — Extracted reusable markdown rendering component (headings, code blocks, bold, italic, lists, horizontal rules).
- **Monitor: Background Tasks** — Live task-tree display with progress bars, cost tracking, and per-task status.
- **Response Parser** — Parse AI output for file-targeted edits in two formats (fenced ````lang:path```` and XML `<edit>`).
- **AI Context Export** — XML-wrapped project context generation for structured AI consumption (overview, dependencies, symbols, git history).
- **Ollama Model Management** — Pull and delete models on connected Ollama instances from the UI.
- **Voice Intent Classification** — Process text through wake-word + intent pipeline for voice-driven commands.
- **Reminder Notifications** — In-app reminder delivery from the assistant tick driver.

### v0.3.24

**Project Quick-Switcher**

- **Titlebar Dropdown** — Click the project name in the titlebar to see pinned and recent projects. One-click switching instead of navigating the OS file picker every time.
- **Pin/Unpin Projects** — Star icon toggles pin state; pinned projects persist across sessions and always appear at the top.
- **Keyboard Dismiss** — Escape closes the dropdown; clicking outside the dropdown also dismisses it.

### v0.3.23

**CI Hardening for LanceDB + Windows MSVC**

- **Windows MSVC Linker Fixes** — Added `codegen-units=1` and removed thin LTO to resolve lance/arrow linker errors on Windows MSVC builds.
- **Release Matrix Cleanup** — Disabled Linux ARM64 cross-compile (unstable), fixed DEB822 format for ARM64 sources, simplified CI matrix.

### v0.3.21

**TOON Encoding + ARM64 Build Fixes**

- **TOON Encoding** — Token-Oriented Object Notation for ~30-40% token savings when injecting structured context (file types, dependencies, symbols, git history) into LLM prompts. Graceful plain-text fallback.
- **ARM64 Release Build** — Fixed cross-compilation for ARM64 Linux targets.

### v0.3.20

**Universal Skills — Cross-Model Skill Sharing**

- **Universal Skills System** — One set of TOML-based skills shared across all 27 AI providers. Each skill declares capability requirements (`tool_use`, `extended_thinking`, `structured_output`, etc.) and the `SkillExecutor` adapts prompts per model at runtime.
- **SkillLoader** — File-backed skill persistence in `~/.hive/skills/*.toml`. Loads, saves, deletes, toggles skills with SHA-256 integrity verification. 15 built-in skills embedded and written on first run.
- **SkillExecutor** — Capability-aware execution pipeline: gates on required capabilities, enhances prompts for preferred capabilities, injects required tools, validates model tier.
- **SkillsRegistry Refactor** — Dual-mode registry: `new()` for in-memory tests, `with_loader()` for file-backed production. Backward-compatible `dispatch()` API preserved.
- **15 Built-in TOML Skills** — help, web-search, code-review, git-commit, generate-docs, test-gen, slack, jira, notion, db, docker, k8s, deploy, browse, index-docs.
- **TOON Encoding** (`toon.rs`) — Token-Oriented Object Notation for ~30-40% token savings when injecting context (file types, dependencies, symbols, git history) into LLM prompts. Graceful plain-text fallback.

### v0.3.19

**Dogfooding: Terminal, File Viewer, Tool Approval**

- **Interactive Terminal Panel** — New sidebar panel wrapping `InteractiveShell` from `hive_terminal`. Spawns a real shell process (cmd.exe / bash) with async streaming output via tokio channels. Color-coded stdout/stderr/stdin, kill/restart/clear controls, and a real GPUI text input for command entry.
- **Built-In File Viewer** — Files panel now has a split layout: file tree on the left, syntax-highlighted code viewer on the right. Supports language detection by extension, line numbers, and scrollable content via `render_code_block()`.
- **Tool Approval Gate** — AI `write_file` tool calls are now intercepted before execution. A diff preview card shows the proposed changes (additions in green, removals in red, context lines) with Approve/Reject buttons. Uses a `oneshot` channel to pause the async tool loop until the user decides. Rejected writes return a rejection `ToolResult` to the AI so it can adapt.
- **3 New Panels** — Terminal, Quick Start, and Channels panels added to sidebar (21 → 24 total).

### v0.3.18

**A2A Protocol + Cross-Crate Improvements**

- **A2A Protocol** (`hive_a2a`) — Full Agent-to-Agent protocol implementation: HTTP server with Axum routes, SSE streaming, task handler dispatching to Hive orchestrator, agent card builder, client discovery cache, auth middleware, and bidirectional type bridge. Wired into app startup with round-trip integration test.
- **UI Overhaul** — Workspace, agents panel, settings panel, help panel, and token launch panel refreshed with improved layout and functionality.
- **Blockchain** — Enhanced wallet store, EVM and Solana integrations, and RPC configuration.
- **CLI** — Updated chat, config, sync, and tools commands.
- **Cloud** — Admin and main entry point improvements.
- **MCP Server** — Expanded integration tools and MCP server capabilities.
- **CI/CD** — Fixed missing `protoc` dependency on macOS and OpenSSL for ARM64 cross-compilation.

### v0.3.15

**Hive Cloud Backend + Live Task Tree GPUI**

- **Cloud Foundation** (`hive_cloud`) — New centralized Axum backend for web-based relay, federated authentication (JWT), and subscription administration (Stripe).
- **Cloud Relay Hub** — WebSocket hub in `hive-cloud` for real-time E2E message routing between nodes. Replaces P2P LAN limitations with true remote web connectivity.
- **Relay Client Integration** — `hive_remote` updated to authenticate against `hive_cloud` via JWT for secure relay tunneling.
- **Live Task Tree UI** — New hierarchical, collapsible `TaskTreeView` GPUI component. Coordinator now emits `TaskEvent`s (Started, Progress, Completed, Failed) natively, enabling beautiful "parallel wave" visualizations for multi-agent execution.

### v0.3.14

**Vector Memory + File-Based Skills + Remote Control + CLI**

- **LanceDB Vector Memory** — Persistent, searchable memory across conversations using LanceDB embedded vector database. Two-table design: `chunks` (indexed code with 50-line overlapping windows) and `memories` (durable insights with importance scoring and categories).
- **Embedding Providers** — `EmbeddingProvider` trait with OpenAI (text-embedding-3-small, 1536d) and Ollama (nomic-embed-text, 768d) implementations. Auto-detects available provider at startup with graceful fallback.
- **Background Indexer** — Recursive directory scanner that indexes workspace code files on project open. Tracks file hashes for incremental re-indexing. Skips binary, hidden, and vendor directories.
- **Memory Injection** — On every message, recalled memories are injected as a dedicated system message (separate from code context) with importance scores and categories.
- **Memory Flush** — Pre-compaction extraction: builds LLM prompts to extract key insights from conversations before the context window is compacted. Filters by importance threshold.
- **File-Based SkillManager** — User-created skills stored as markdown with YAML frontmatter in `~/.hive/skills/`. Full CRUD (create, update, delete, toggle) with injection scanning on every write.
- **Skill Dispatch** — `/command` routing checks both built-in SkillsRegistry and file-based SkillManager, injecting skill instructions as a system message.
- **SkillManager in Skills Panel** — User skills appear alongside built-in and marketplace skills in the UI.
- **Remote Control** (`hive_remote`) — Background daemon with Axum web server, WebSocket streaming, QR device pairing (X25519 + AES-GCM), and session journal persistence.
- **Terminal CLI** (`hive_cli`) — Full terminal client with 7 commands: chat (streaming TUI), models, status, sync, config, login, remote.
- **Admin TUI** (`hive_admin`) — Cloud admin dashboard with 6 tabs for managing users, gateway, relay, sync, and teams.
- **Hive Cloud** — HiveGateway cloud AI proxy, cloud sync module, and account management fields in Settings.
- **Agent Task Events** — Coordinator emits live TaskEvents; Agents panel has task tree drill-down.

### v0.3.9

**Venice AI + Headless Mode + Dynamic Agent Scripting**

- **Venice AI Integration** — Venice is now fully integrated as an AI provider alongside OpenAI, Anthropic, Gemini, etc. Models are securely wired into the backend capability router. Enter your Venice API key in the Settings panel to start using it immediately.
- **Headless / Background Mode** — The app natively supports running without spawning the main GUI. Pass the `--tray` argument at launch to suppress the main window, allowing Hive to run silently in the background or purely from the system tray.
- **Dynamic Agent Scripting (run_python tool)** — When an agent decides to write and run a Python script, the `run_python` tool spins up an ephemeral Docker container (`python:3.11-slim`) locked down with `--network none` for strict isolation. If Docker isn't installed or fails to launch, the MCP server automatically falls back to executing the script via the native Python binary (`python -c`). All tests passing in the `hive_agents` crate.

### v0.3.5
- Added xAI/Grok as 13th AI provider (OpenAI-compatible)
- Interactive Privacy Shield controls: global toggle, per-rule toggles, custom blocking rules
- Shield settings persist across restarts via HiveConfig
- Model age display in Models Browser (days/months/years since release)
- Dual licensing: AGPL-3.0 for app/agents, MIT for library crates

### v0.3.4

**Theme System + Settings Security**

- **8 built-in themes** with community ratings: HiveCode Dark, HiveCode Light, Nord, Dracula, Solarized Dark, Monokai, One Dark, GitHub Dark
- **ThemeManager** with custom theme support — load and manage themes from `~/.hive/themes/`
- **Settings export/import** with AES-256-GCM encryption + Argon2id key derivation for secure config portability
- **Community theme voting** on hivecode.app via Vercel KV backend
- **Theme switching** wired to all 21 panels

### v0.3.3

**Security Hardening + Messaging Expansion + Module Wiring + Site Overhaul**

- **Security fixes**: Eliminated command injection in deploy trigger (now uses safe `Command::env()` API), hardened iMessage provider against AppleScript and SQL injection with input validation and comprehensive escaping, added multi-statement SQL injection guard on database queries, added CSP and security headers to website
- **4 new messaging providers**: WhatsApp (Business Cloud API), Signal (CLI REST API), Google Chat (Workspace API), iMessage (macOS AppleScript + chat.db) — all with full MessagingProvider trait implementation and comprehensive test suites
- **Module wiring**: Connected previously unwired core modules to the application, including Guardian, HiveLoop, FleetLearning, RAG, SemanticSearch, Enterprise, Canvas, Webhooks, and the AssistantPlugin system
- **Scheduler tick driver**: Background OS thread with 60-second tokio interval timer drives scheduler and reminder service ticks
- **Plugin system**: PluginRegistry with `with_defaults()` auto-loading ReminderPlugin — first production AssistantPlugin implementation
- **Blockchain labeling**: Deploy functions now clearly labeled as simulation mode with SIM_ prefixed identifiers and `simulated: true` field
- **Blockchain deployment labeling**: Token-launch flows remain explicitly labeled where deployment is still simulation-backed or not yet wired end-to-end

### v0.3.2

**Gap Closure + P2P Wiring + Website Overhaul**

- Wired `hive_network` P2P federation to app startup — node starts on background thread with dedicated tokio runtime, LAN discovery and WebSocket server active
- Replaced `deploy_trigger` stub with real deployment dispatch (deploy.sh, Makefile targets, or `gh workflow run`)
- Implemented real CLI doctor checks — `check_disk_space()` reads filesystem stats via `statvfs`, `check_network()` performs DNS resolution
- Implemented MCP SSE transport — HTTP POST for requests, Server-Sent Events for streaming responses

### v0.3.1

**Integration Wiring + Tool Fixes**

- Wired all 13 MCP integration tools to live services (replaced stub handlers)
- Added `deploy_trigger` as 13th integration tool
- Fixed all 9 integration skill `/commands` to reference correct tool names
- Added `IntegrationServices` struct for clean dependency injection of Arc services
- Added `block_on_async()` helper for sync→async bridging in tool handlers

### v0.3.0

**Integration Platform + Capability-Aware AI Routing**

- **Integration Platform** — 13 new external service integrations with real API clients and connection pooling
- **Capability-Aware AI Routing** — `CapabilityRouter` (1,365 lines) ranking AI models by task-specific strengths across 12 task types with 19 model profiles
- **Speculative Decoding** — Guess-and-check optimization for faster AI responses
- **9 New Integration Skills** — `/slack`, `/jira`, `/notion`, `/db`, `/docker`, `/k8s`, `/deploy`, `/browse`, `/index-docs`
- **13 New MCP Integration Tools** — exposing integration capabilities to external MCP clients

### v0.2.0

**Autonomous Skill Acquisition + Production Hardening**

- Knowledge Acquisition Agent, Competence Detection, Skill Authoring Pipeline
- P2P Federation, Blockchain/Web3, Docker Sandbox
- Eliminated ~800+ `.unwrap()` calls with proper error handling
- SQLite persistence, FTS5 search, persistent logs
- Targeted validation across the actively maintained crate matrix

### v0.1.0

Initial release with 16-crate architecture, multi-agent swarm, 11 AI providers, HiveShield security, self-improvement engine, skill marketplace, personal assistant, 20+ UI panels, automation workflows, and full Git Ops.

---

## Contributing

Hive is open source under the MIT license. Contributions are welcome! Please open an issue before submitting large PRs.

## License

This project is licensed under the **MIT License**. See [LICENSE](LICENSE) for details.

## Security

Hive is built on a local-first, zero-trust architecture with a 4-layer outbound firewall (HiveShield), command-level SecurityGateway, and AES-256-GCM encrypted storage. For the full technical deep-dive, see [SECURITY.md](SECURITY.md).

To report a security vulnerability, please email the author directly rather than opening a public issue.

---

<p align="center">
  <sub>Built with Rust, GPUI, and an unreasonable amount of ambition.</sub>
</p>
