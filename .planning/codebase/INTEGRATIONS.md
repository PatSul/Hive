# External Integrations

**Analysis Date:** 2026-06-18

## APIs & External Services

**AI Providers:**
- Anthropic, OpenAI, OpenRouter, Google, Groq, HuggingFace, xAI, Mistral, Venice, z.ai, LiteLLM, Ollama, LM Studio, generic local/OpenAI-compatible endpoints, and Kilo.
  - SDK/Client: Mostly REST/SSE over `reqwest`, wrapped by provider modules in `hive/crates/hive_ai/src/providers/`.
  - Auth: API keys in local config/secure storage; local providers usually use configured localhost URLs.
  - Endpoints used: Chat/completions, streaming, model discovery, embeddings, and TTS depending on provider.

**Text-to-Speech and Voice:**
- OpenAI, HuggingFace, ElevenLabs, Telnyx, Qwen-style provider selection.
  - SDK/Client: `hive_ai::tts`.
  - Auth: Provider keys in local config/secure storage.

**Project Management:**
- Jira, Linear, GitHub Issues, Asana.
  - SDK/Client: `hive/crates/hive_integrations/src/project_management/`.
  - Auth: Jira base URL/email/API token, Linear API key, GitHub token, Asana credentials through config or provider-specific setup.

**Git Hosting:**
- GitHub, GitLab, Bitbucket.
  - SDK/Client: `hive_integrations::github`, `gitlab`, `bitbucket`, plus `git2`.
  - Auth: GitHub token in config, `GITLAB_PRIVATE_TOKEN`, `BITBUCKET_USERNAME`, `BITBUCKET_APP_PASSWORD`.

**Messaging and Collaboration:**
- Slack, Discord, Telegram, WhatsApp, Matrix, Google Chat, Microsoft Teams, Signal, WebChat, and macOS iMessage.
  - SDK/Client: `hive/crates/hive_integrations/src/messaging/`.
  - Auth: Platform tokens in config or secure storage; Teams uses connected account OAuth.

**Knowledge and Documents:**
- Notion, Obsidian, Google Drive/Docs/Sheets, local docs index.
  - SDK/Client: `hive_integrations::knowledge`, `hive_integrations::google`, `hive_docs`.
  - Auth: Notion API key, Google OAuth tokens, local filesystem access.

**Cloud and DevOps:**
- AWS, Azure, GCP, Docker, Kubernetes, Vercel/Supabase/Cloudflare-style cloud service hooks.
  - SDK/Client: `hive_integrations::cloud`, `docker`, `kubernetes`, `database`.
  - Auth: Provider-specific credentials and local CLIs/config.

**Smart Home:**
- Philips Hue.
  - SDK/Client: `hive_integrations::smart_home` and Hue client globals.
  - Auth: Bridge IP and API key.

## Data Storage

**Databases:**
- SQLite - Primary embedded persistence through `rusqlite`.
  - Connection: Opened by `hive_core::persistence::Database` and other services.
  - Location: Under `~/.hive/`, including `memory.db`, `learning.db`, `assistant.db`, and related files.
- External databases - PostgreSQL, MySQL, SQLite via `DatabaseHub`.
  - Connection: User-configured integration credentials.

**Vector Storage:**
- LanceDB - Embedded vector memory in `hive_ai::memory::HiveMemory`.
  - Location: `~/.hive/hive_memory.lance` or similar configured local path.
  - Embeddings: OpenAI when configured, otherwise Ollama fallback.

**File Storage:**
- Local filesystem - Conversations, workflows, skills, config, session, docs cache, and app logs under `~/.hive/`.
- Cloud storage - AWS/Azure/GCP integration clients for user-configured cloud workflows.

**Caching:**
- In-process caches for model discovery, quick index, provider status, and UI panel data.
- SQLite/LanceDB-backed durable memory where persistence is needed.

## Authentication & Identity

**Local Config and Secure Storage:**
- Implementation: `hive_core::config::ConfigManager` and `hive_core::secure_storage::SecureStorage`.
- Token storage: Local config plus AES-GCM encrypted vault.
- Session management: Local `SessionState` and service-specific OAuth tokens.

**OAuth Integrations:**
- Google and Microsoft account integrations.
- Credentials/tokens are held locally and used by integration clients.

**A2A Auth:**
- A2A server supports API key style auth through `x-hive-key` in relevant tests and config.

## Monitoring & Observability

**Error Tracking:**
- No external error tracking service is required by default.
- App uses local tracing logs and UI notifications.

**Analytics:**
- Desktop app is documented as local-first/no telemetry.

**Logs:**
- Local tracing logs through `hive_core::logging`.
- Cloud/admin services log to stdout/stderr through tracing subscribers.

## CI/CD & Deployment

**Hosting/Release:**
- GitHub Releases for desktop artifacts.
- Homebrew tap update in `.github/workflows/release.yml`.
- `hive_cloud` is a standalone Axum service deployable wherever Rust services can run.

**CI Pipeline:**
- GitHub Actions.
- Workflows: `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `.github/workflows/auto-release.yml`.
- Secrets: GitHub release uses `GITHUB_TOKEN`; Homebrew tap update uses `HOMEBREW_TAP_TOKEN`.

## Environment Configuration

**Development:**
- Required: Rust stable, platform build deps, protoc.
- Optional provider keys: AI, messaging, project management, cloud, smart home, and git hosting credentials.
- Local service URLs: Ollama, LM Studio, LiteLLM, Kilo, generic local provider.

**Production:**
- Desktop app stores secrets locally.
- Release automation publishes signed/packaged artifacts but does not require app user secrets.
- `hive_cloud` uses `HIVE_CLOUD_BIND` for bind address.

## Webhooks & Callbacks

**Incoming:**
- Workflow automation supports webhook triggers.
- OAuth callback server exists in `hive_integrations::oauth_callback`.
- Remote and cloud crates expose HTTP/WebSocket endpoints.

**Outgoing:**
- Integrations call third-party APIs for AI, messaging, project management, git hosting, cloud, documents, and smart home actions.
- Updates call GitHub Releases through `hive_core::updater`.

---

*Integration audit: 2026-06-18*
*Update when adding/removing external services or credential paths*
