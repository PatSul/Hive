# Hive Cloud Monetization Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a subscription-based cloud services layer (relay, AI gateway, sync) that monetizes the open-source Hive desktop app.

**Architecture:** Two repos — `hive` (public, desktop app) and `hive-cloud` (private, Axum server). Client communicates with server via HTTPS/WSS. Auth via JWT, billing via Stripe. All cloud features gated server-side by subscription tier.

**Tech Stack:** Rust, Axum, SQLx (Postgres), Stripe API, JWT (jsonwebtoken crate), Fly.io deployment, Tokio for async

**Design Doc:** `docs/plans/2026-02-26-hive-cloud-monetization-design.md`

---

## Phase 1: hive-cloud Repo + Auth/Billing Foundation (~2 weeks)

### Task 1.1: Create hive-cloud Repository

**Files:**
- Create: `hive-cloud/Cargo.toml` (workspace root)
- Create: `hive-cloud/src/main.rs` (Axum server entry)
- Create: `hive-cloud/src/config.rs` (server configuration)
- Create: `hive-cloud/.gitignore`
- Create: `hive-cloud/.env.example`

**Step 1: Initialize the project**

Create a new Rust project outside the hive workspace. This is a SEPARATE repo.

```bash
cd H:\WORK\AG\AIrglowStudio
mkdir hive-cloud
cd hive-cloud
cargo init
```

**Step 2: Set up Cargo.toml with dependencies**

```toml
[package]
name = "hive-cloud"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = { version = "0.7", features = ["ws", "macros"] }
axum-extra = { version = "0.9", features = ["typed-header"] }
tokio = { version = "1", features = ["full"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.7", features = ["runtime-tokio", "postgres", "chrono", "uuid"] }
jsonwebtoken = "9"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
reqwest = { version = "0.12", features = ["json", "stream"] }
stripe-rust = "0.23"
dotenvy = "0.15"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
argon2 = "0.5"
rand = "0.8"
thiserror = "1"
```

**Step 3: Create minimal Axum server in main.rs**

```rust
use axum::{routing::get, Router};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

mod config;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("hive_cloud=debug,tower_http=debug")
        .init();

    let app = Router::new()
        .route("/health", get(health_check))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    tracing::info!("hive-cloud listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "ok"
}
```

**Step 4: Create config.rs**

```rust
use std::env;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub stripe_secret_key: String,
    pub stripe_webhook_secret: String,
    pub server_port: u16,
    pub allowed_origins: Vec<String>,
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        Self {
            database_url: env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set"),
            jwt_secret: env::var("JWT_SECRET")
                .expect("JWT_SECRET must be set"),
            stripe_secret_key: env::var("STRIPE_SECRET_KEY")
                .expect("STRIPE_SECRET_KEY must be set"),
            stripe_webhook_secret: env::var("STRIPE_WEBHOOK_SECRET")
                .expect("STRIPE_WEBHOOK_SECRET must be set"),
            server_port: env::var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("PORT must be a valid u16"),
            allowed_origins: env::var("ALLOWED_ORIGINS")
                .unwrap_or_else(|_| "http://localhost:3000".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
        }
    }
}
```

**Step 5: Create .env.example**

```
DATABASE_URL=postgres://hive:hive@localhost:5432/hive_cloud
JWT_SECRET=change-me-to-a-random-64-char-string
STRIPE_SECRET_KEY=sk_test_...
STRIPE_WEBHOOK_SECRET=whsec_...
PORT=8080
ALLOWED_ORIGINS=http://localhost:3000,https://hive.dev
```

**Step 6: Verify it compiles and runs**

```bash
cd H:\WORK\AG\AIrglowStudio\hive-cloud
cargo build
```

Expected: builds without errors.

**Step 7: Commit**

```bash
git init
git add -A
git commit -m "feat: initialize hive-cloud with Axum server skeleton"
```

---

### Task 1.2: Database Schema + Accounts Table

**Files:**
- Create: `hive-cloud/migrations/001_accounts.sql`
- Create: `hive-cloud/src/db.rs`

**Step 1: Write migration SQL**

```sql
-- 001_accounts.sql
CREATE TABLE IF NOT EXISTS accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    display_name TEXT,
    github_id TEXT UNIQUE,
    tier TEXT NOT NULL DEFAULT 'free' CHECK (tier IN ('free', 'pro', 'team')),
    stripe_customer_id TEXT UNIQUE,
    stripe_subscription_id TEXT,
    subscription_expires_at TIMESTAMPTZ,
    token_budget_cents INTEGER NOT NULL DEFAULT 0,
    token_used_cents INTEGER NOT NULL DEFAULT 0,
    budget_reset_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    refresh_token_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_sessions_account ON sessions(account_id);
CREATE INDEX idx_accounts_email ON accounts(email);
CREATE INDEX idx_accounts_stripe ON accounts(stripe_customer_id);
```

**Step 2: Write db.rs connection pool**

```rust
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn create_pool(database_url: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
        .expect("Failed to connect to database")
}

pub async fn run_migrations(pool: &PgPool) {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to run migrations");
}
```

**Step 3: Wire pool into main.rs (add shared state)**

Update main.rs to create pool on startup and pass as Axum state.

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: add accounts/sessions schema and db pool"
```

---

### Task 1.3: JWT Auth Module

**Files:**
- Create: `hive-cloud/src/auth/mod.rs`
- Create: `hive-cloud/src/auth/jwt.rs`
- Create: `hive-cloud/src/auth/middleware.rs`
- Create: `hive-cloud/src/auth/login.rs`

**Step 1: Write JWT token creation and verification**

`auth/jwt.rs`:
- `create_access_token(account_id: Uuid, tier: Tier, secret: &str) -> String` — 1hr expiry
- `create_refresh_token(account_id: Uuid, secret: &str) -> String` — 30 day expiry
- `verify_token(token: &str, secret: &str) -> Result<Claims, AuthError>`
- `Claims` struct: `sub: Uuid, tier: String, exp: usize, iat: usize`

**Step 2: Write auth middleware (Axum extractor)**

`auth/middleware.rs`:
- `AuthUser` struct extracted from `Authorization: Bearer <jwt>` header
- Implements `FromRequestParts` for Axum
- Verifies JWT, returns `AuthUser { id: Uuid, tier: Tier }`
- Returns 401 if missing/invalid, 403 if subscription expired

**Step 3: Write test for JWT round-trip**

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_jwt_create_and_verify() {
        let token = create_access_token(Uuid::new_v4(), Tier::Pro, "test-secret");
        let claims = verify_token(&token, "test-secret").unwrap();
        assert_eq!(claims.tier, "pro");
    }

    #[test]
    fn test_expired_token_rejected() {
        // Create token with past expiry, verify it fails
    }
}
```

**Step 4: Write magic link login endpoint**

`auth/login.rs`:
- `POST /auth/login` — accepts `{ email }`, sends magic link email, returns 200
- `GET /auth/callback?token=<magic>` — verifies magic link, creates/gets account, returns JWT pair
- `POST /auth/refresh` — accepts refresh token, returns new access token
- `POST /auth/logout` — invalidates refresh token

For MVP: magic link can be a simple signed URL with expiry. Email sending via Resend API (or console log in dev).

**Step 5: Write test for login flow**

**Step 6: Commit**

```bash
git add -A
git commit -m "feat: add JWT auth with magic link login"
```

---

### Task 1.4: GitHub OAuth Login

**Files:**
- Create: `hive-cloud/src/auth/github.rs`

**Step 1: Write GitHub OAuth flow**

- `GET /auth/github` — redirects to GitHub OAuth authorize URL
- `GET /auth/github/callback?code=<code>` — exchanges code for GitHub token, fetches user email, creates/gets account, returns JWT pair
- Store `github_id` on account for future logins

**Step 2: Write test**

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: add GitHub OAuth login"
```

---

### Task 1.5: Stripe Billing Integration

**Files:**
- Create: `hive-cloud/src/billing/mod.rs`
- Create: `hive-cloud/src/billing/stripe_webhooks.rs`
- Create: `hive-cloud/src/billing/checkout.rs`

**Step 1: Write checkout session creation**

`billing/checkout.rs`:
- `POST /billing/checkout` (auth required) — creates Stripe Checkout session for Pro plan
- Returns `{ checkout_url }` — app opens this in browser
- Stripe price IDs stored in config (Pro monthly, Team monthly)

**Step 2: Write Stripe webhook handler**

`billing/stripe_webhooks.rs`:
- `POST /billing/webhooks` — receives Stripe events
- Handles: `checkout.session.completed` → update account tier to Pro
- Handles: `customer.subscription.updated` → update tier/expiry
- Handles: `customer.subscription.deleted` → downgrade to Free
- Handles: `invoice.payment_failed` → mark subscription at risk
- Verifies webhook signature using `stripe_webhook_secret`

**Step 3: Write tier check helper**

```rust
pub fn require_tier(user: &AuthUser, minimum: Tier) -> Result<(), AppError> {
    if user.tier < minimum {
        return Err(AppError::PaymentRequired(
            format!("This feature requires {} tier", minimum)
        ));
    }
    Ok(())
}
```

**Step 4: Write tests for webhook handling**

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: add Stripe checkout + webhook billing"
```

---

### Task 1.6: Account Settings API

**Files:**
- Create: `hive-cloud/src/routes/account.rs`

**Step 1: Write account endpoints**

- `GET /account` (auth required) — returns account info (email, tier, usage, expiry)
- `PATCH /account` — update display_name
- `GET /account/usage` — returns current period token usage + cost

**Step 2: Wire all routes into main.rs router**

```rust
let app = Router::new()
    .route("/health", get(health_check))
    .nest("/auth", auth::routes())
    .nest("/billing", billing::routes())
    .nest("/account", account::routes())
    .with_state(state);
```

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: add account info + usage endpoints"
```

---

## Phase 2: Cloud Relay Server (~2 weeks)

### Task 2.1: Relay Room Management

**Files:**
- Create: `hive-cloud/src/relay/mod.rs`
- Create: `hive-cloud/src/relay/room.rs`
- Create: `hive-cloud/src/relay/hub.rs`

**Step 1: Write relay room state**

`relay/room.rs`:
```rust
pub struct RelayRoom {
    pub id: String,
    pub created_by: Uuid, // account_id
    pub participants: HashMap<String, Participant>, // node_id -> participant
    pub created_at: Instant,
}

pub struct Participant {
    pub node_id: String,
    pub sender: mpsc::UnboundedSender<Message>,
}
```

**Step 2: Write relay hub (manages all rooms)**

`relay/hub.rs`:
```rust
pub struct RelayHub {
    rooms: RwLock<HashMap<String, RelayRoom>>,
    account_rooms: RwLock<HashMap<Uuid, Vec<String>>>, // account_id -> room_ids
}

impl RelayHub {
    pub async fn create_room(&self, account_id: Uuid, room_id: String) -> Result<()>;
    pub async fn join_room(&self, room_id: &str, node_id: String, tx: mpsc::UnboundedSender<Message>) -> Result<()>;
    pub async fn leave_room(&self, room_id: &str, node_id: &str);
    pub async fn forward(&self, room_id: &str, from: &str, to: Option<&str>, payload: Vec<u8>) -> Result<()>;
    pub async fn room_count_for_account(&self, account_id: Uuid) -> usize;
}
```

**Step 3: Write tests for room lifecycle**

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: add relay room management hub"
```

---

### Task 2.2: WebSocket Relay Endpoint

**Files:**
- Create: `hive-cloud/src/relay/ws.rs`

**Step 1: Write WebSocket upgrade handler**

- `GET /relay/ws` — upgrades to WebSocket (requires Pro+ tier via JWT in query param or header)
- On connect: wait for `Register` frame with session token
- Parse incoming `RelayFrame` messages (same enum as hive_remote)
- Handle: CreateRoom, JoinRoom, LeaveRoom, Forward, Ping/Pong
- Rate limit: max 3 concurrent rooms per account (Pro), 10 (Team)
- Bandwidth metering: count bytes per forward

The `RelayFrame` enum should match `hive/crates/hive_remote/src/relay.rs` exactly so client and server speak the same protocol.

**Step 2: Wire into router**

```rust
.nest("/relay", relay::routes())
```

**Step 3: Write integration test (two clients, one room, forward messages)**

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: add WebSocket relay endpoint with room forwarding"
```

---

### Task 2.3: Client-Side Cloud Relay Connection (in hive desktop)

**Files:**
- Modify: `hive/crates/hive_remote/src/relay.rs`
- Modify: `hive/crates/hive_core/src/config.rs`

**Step 1: Add cloud relay URL to HiveConfig**

In `hive/crates/hive_core/src/config.rs`, add to `HiveConfig`:
```rust
pub cloud_relay_url: Option<String>,  // e.g. "wss://relay.hive.dev"
pub cloud_jwt: Option<String>,        // stored encrypted in SecureStorage
```

Default `cloud_relay_url` to `Some("wss://relay.hive.dev".into())`.

**Step 2: Add cloud relay mode to relay.rs**

When `cloud_relay_url` is set and user has JWT, connect to cloud relay instead of/in addition to LAN relay. The existing `RelayFrame` protocol stays identical — the client doesn't care if it's talking to a LAN relay or cloud relay.

**Step 3: Write test for cloud relay connection (mock server)**

**Step 4: Commit in hive repo**

```bash
cd H:\WORK\AG\AIrglowStudio
git add hive/crates/hive_remote/src/relay.rs hive/crates/hive_core/src/config.rs
git commit -m "feat: add cloud relay connection support to hive_remote"
```

---

## Phase 3: Live Task Tree UI (~1 week)

### Task 3.1: Coordinator Task Events

**Files:**
- Modify: `hive/crates/hive_agents/src/coordinator.rs`

**Step 1: Define task event types**

Add to coordinator.rs:
```rust
#[derive(Clone, Debug, Serialize)]
pub enum TaskEvent {
    PlanCreated { plan: TaskPlan },
    TaskStarted { task_id: String, description: String, persona: String },
    TaskProgress { task_id: String, progress: f32, message: String },
    TaskCompleted { task_id: String, result: TaskResult },
    TaskFailed { task_id: String, error: String },
    AllComplete { result: CoordinatorResult },
}
```

**Step 2: Add event channel to Coordinator**

Add a `tokio::sync::broadcast::Sender<TaskEvent>` to the Coordinator struct. Emit events at each stage of execution (before running a task, after completion, on error).

**Step 3: Expose `subscribe()` method**

```rust
impl<E: AiExecutor> Coordinator<E> {
    pub fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        self.event_tx.subscribe()
    }
}
```

**Step 4: Write test that subscribes and receives events**

**Step 5: Commit**

```bash
git add hive/crates/hive_agents/src/coordinator.rs
git commit -m "feat: emit live TaskEvents from Coordinator"
```

---

### Task 3.2: Task Tree Chat Component

**Files:**
- Create: `hive/crates/hive_ui_panels/src/components/task_tree.rs`
- Modify: `hive/crates/hive_ui_panels/src/components/mod.rs`

**Step 1: Define TaskTreeState**

```rust
pub struct TaskTreeState {
    pub title: String,
    pub tasks: Vec<TaskDisplay>,
    pub collapsed: bool,
}

pub struct TaskDisplay {
    pub id: String,
    pub description: String,
    pub persona: String,
    pub status: TaskStatus,
    pub duration_ms: Option<u64>,
    pub cost: Option<f64>,
    pub output: Option<String>,
    pub expanded: bool,
}

pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
}
```

**Step 2: Implement GPUI rendering**

Render as a collapsible block in chat:
- Header: title + overall progress bar + task counter
- Each task: status icon (pending circle / spinner / checkmark / X) + description + duration + cost
- Click task to expand and see output
- Tasks in same parallel wave shown with a subtle visual grouping

**Step 3: Wire TaskEvent subscription into chat_service**

When a Coordinator run starts, chat_service subscribes to events and creates a TaskTreeState. Each incoming TaskEvent updates the state, which triggers re-render.

**Step 4: Write test for TaskTreeState updates**

**Step 5: Commit**

```bash
git add hive/crates/hive_ui_panels/src/components/task_tree.rs hive/crates/hive_ui_panels/src/components/mod.rs
git commit -m "feat: add live task tree component to chat"
```

---

### Task 3.3: Update Agents Panel with Task Drill-Down

**Files:**
- Modify: `hive/crates/hive_ui_panels/src/panels/agents.rs`

**Step 1: Enhance RunDisplay with task-level data**

Add `tasks: Vec<TaskDisplay>` to `RunDisplay` struct so the agents panel can show per-task breakdown when a run is selected.

**Step 2: Add click handler to expand run details**

Clicking a run card in the agents panel shows its constituent tasks with the same TaskDisplay rendering.

**Step 3: Commit**

```bash
git add hive/crates/hive_ui_panels/src/panels/agents.rs
git commit -m "feat: add task drill-down to agents panel"
```

---

## Phase 4: Managed AI Gateway (~3 weeks)

### Task 4.1: Gateway Proxy Server

**Files:**
- Create: `hive-cloud/src/gateway/mod.rs`
- Create: `hive-cloud/src/gateway/proxy.rs`
- Create: `hive-cloud/src/gateway/metering.rs`

**Step 1: Write provider proxy logic**

`gateway/proxy.rs`:
- Receives `ChatRequest` from client
- Looks up which real provider to call based on `model_id`
- Holds server-side API keys for all providers (Anthropic, OpenAI, Google, etc.)
- Calls real provider API, streams response back to client
- Meters input/output tokens per request

**Step 2: Write metering module**

`gateway/metering.rs`:
- `record_usage(account_id, model_id, input_tokens, output_tokens, cost_cents)` → writes to DB
- `get_usage(account_id, period_start) -> UsageSummary`
- `check_budget(account_id) -> Result<(), BudgetExceeded>`
- Budget check runs BEFORE proxying request

**Step 3: Write gateway endpoints**

```
POST   /v1/chat              → proxy to provider, stream response (Pro+ only)
POST   /v1/chat/estimate     → return cost estimate (Pro+ only)
GET    /v1/usage             → return usage for billing period (auth required)
GET    /v1/models            → list available models for user's tier (public)
```

**Step 4: Write tests for proxy + metering**

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: add AI gateway proxy with token metering"
```

---

### Task 4.2: Gateway Migration (add usage table)

**Files:**
- Create: `hive-cloud/migrations/002_gateway_usage.sql`

**Step 1: Write migration**

```sql
CREATE TABLE IF NOT EXISTS gateway_usage (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id),
    model_id TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost_cents INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_usage_account_date ON gateway_usage(account_id, created_at);
```

**Step 2: Commit**

```bash
git add -A
git commit -m "feat: add gateway usage tracking table"
```

---

### Task 4.3: HiveGatewayProvider (in hive desktop)

**Files:**
- Create: `hive/crates/hive_ai/src/providers/hive_gateway.rs`
- Modify: `hive/crates/hive_ai/src/providers/mod.rs`

**Step 1: Implement AiProvider trait for HiveGatewayProvider**

```rust
pub struct HiveGatewayProvider {
    gateway_url: String,  // https://api.hive.dev
    jwt: String,
    client: reqwest::Client,
}

#[async_trait]
impl AiProvider for HiveGatewayProvider {
    fn provider_type(&self) -> ProviderType { ProviderType::HiveGateway }
    fn name(&self) -> &str { "Hive Gateway" }

    async fn is_available(&self) -> bool {
        // GET /health returns 200
    }

    async fn get_models(&self) -> Vec<ModelInfo> {
        // GET /v1/models
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        // POST /v1/chat (non-streaming)
    }

    async fn stream_chat(&self, request: &ChatRequest)
        -> Result<mpsc::Receiver<StreamChunk>, ProviderError> {
        // POST /v1/chat with streaming
    }
}
```

**Step 2: Add HiveGateway to ProviderType enum**

In `hive/crates/hive_ai/src/providers/mod.rs`, add `HiveGateway` variant.

**Step 3: Register provider when user has cloud JWT**

When HiveConfig has a cloud JWT, automatically register HiveGatewayProvider alongside any user-configured providers.

**Step 4: Write tests with mock gateway server**

**Step 5: Commit**

```bash
git add hive/crates/hive_ai/src/providers/hive_gateway.rs hive/crates/hive_ai/src/providers/mod.rs
git commit -m "feat: add HiveGatewayProvider for cloud AI proxy"
```

---

## Phase 5: Cloud Sync (~3 weeks)

### Task 5.1: Sync Server Endpoints

**Files:**
- Create: `hive-cloud/src/sync/mod.rs`
- Create: `hive-cloud/src/sync/blobs.rs`
- Create: `hive-cloud/migrations/003_sync_blobs.sql`

**Step 1: Write migration**

```sql
CREATE TABLE IF NOT EXISTS sync_blobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id),
    blob_key TEXT NOT NULL,
    data BYTEA NOT NULL,
    size_bytes INTEGER NOT NULL,
    checksum TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(account_id, blob_key)
);

CREATE INDEX idx_blobs_account ON sync_blobs(account_id);
```

**Step 2: Write sync endpoints**

```
PUT    /v1/sync/blobs/:key        → upsert encrypted blob (Pro+ only)
GET    /v1/sync/blobs/:key        → retrieve encrypted blob (Pro+ only)
GET    /v1/sync/manifest          → list all blobs with timestamps (Pro+ only)
DELETE /v1/sync/blobs/:key        → delete a blob (Pro+ only)
```

All blobs are opaque bytes — server never decrypts. Storage quota: 100MB per Pro account, 1GB per Team.

**Step 3: Write tests**

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: add encrypted sync blob storage"
```

---

### Task 5.2: Client-Side Sync Module (in hive desktop)

**Files:**
- Create: `hive/crates/hive_core/src/sync.rs`
- Modify: `hive/crates/hive_core/src/lib.rs`

**Step 1: Write SyncClient**

```rust
pub struct SyncClient {
    api_url: String,
    jwt: String,
    encryption_key: [u8; 32], // derived from account credentials
    client: reqwest::Client,
}

impl SyncClient {
    pub async fn push(&self, key: &str, plaintext: &[u8]) -> Result<()>;
    pub async fn pull(&self, key: &str) -> Result<Vec<u8>>;
    pub async fn manifest(&self) -> Result<Vec<BlobManifestEntry>>;
    pub async fn sync_all(&self, local_state: &LocalSyncState) -> Result<SyncResult>;
}
```

- `push`: encrypt locally with AES-256-GCM → PUT to server
- `pull`: GET from server → decrypt locally
- `sync_all`: fetch manifest, compare timestamps, push newer locals, pull newer remotes

**Step 2: Write LocalSyncState tracking**

Track last-synced timestamps per blob key in `~/.hive/sync_state.json`.

**Step 3: Define what syncs**

Blob keys:
- `conversations` — serialized conversation history
- `settings` — HiveConfig (minus secrets)
- `agent_configs` — agent/skill configurations
- `api_keys` — encrypted API key vault (double-encrypted)

**Step 4: Write tests**

**Step 5: Commit**

```bash
git add hive/crates/hive_core/src/sync.rs hive/crates/hive_core/src/lib.rs
git commit -m "feat: add client-side encrypted sync module"
```

---

### Task 5.3: Auto-Sync on Change

**Files:**
- Modify: `hive/crates/hive_core/src/sync.rs`

**Step 1: Implement change detection + debounced sync**

- Watch for config/conversation changes (reuse existing file watcher in hive_fs)
- Debounce: sync at most once per 30 seconds
- On app startup: pull all, then start watching
- On app shutdown: push any pending changes

**Step 2: Add sync status indicator**

Add sync status to statusbar: syncing spinner, last synced timestamp, error indicator.

**Step 3: Commit**

```bash
git add hive/crates/hive_core/src/sync.rs
git commit -m "feat: add auto-sync with debounce and status indicator"
```

---

## Phase 6: hive.dev Marketing Site (~1 week)

### Task 6.1: Create Landing Page

**Files:**
- Create: `hive-web/` directory (separate project, Astro or plain HTML)

**Step 1: Set up static site**

Simple marketing site with pages:
- `/` — hero, features, CTA to download
- `/pricing` — Free vs Pro vs Team comparison table
- `/docs` — link to GitHub wiki or docs
- `/login` — redirects to auth flow
- `/download` — links to GitHub releases

Deploy on Vercel (free tier). Domain: `hive.dev`.

**Step 2: Create Stripe pricing page integration**

The "Upgrade to Pro" button links to Stripe Checkout (hosted by Stripe). No custom payment UI.

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: create hive.dev marketing site"
```

---

## Phase 7: Team Tier (~2 weeks)

### Task 7.1: Team Management

**Files:**
- Create: `hive-cloud/src/teams/mod.rs`
- Create: `hive-cloud/migrations/004_teams.sql`

**Step 1: Write teams schema**

```sql
CREATE TABLE IF NOT EXISTS teams (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    owner_id UUID NOT NULL REFERENCES accounts(id),
    stripe_subscription_id TEXT,
    max_seats INTEGER NOT NULL DEFAULT 5,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS team_members (
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('owner', 'admin', 'member')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (team_id, account_id)
);
```

**Step 2: Write team endpoints**

```
POST   /teams                → create team (auth required)
GET    /teams/:id            → get team info
POST   /teams/:id/members    → invite member
DELETE /teams/:id/members/:uid → remove member
GET    /teams/:id/usage      → team cost dashboard (aggregated member usage)
```

**Step 3: Write team cost dashboard endpoint**

Aggregates `gateway_usage` across all team members. Returns per-member breakdown + team total.

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: add team management + cost dashboard"
```

---

### Task 7.2: Shared Workspaces

**Files:**
- Create: `hive-cloud/src/sync/shared.rs`

**Step 1: Extend sync to support team-shared blobs**

- Blobs with key prefix `team/<team_id>/` are accessible to all team members
- Shared agent configs, prompt libraries, workspace settings
- Same zero-knowledge encryption, but shared key derived from team secret

**Step 2: Commit**

```bash
git add -A
git commit -m "feat: add team shared workspace sync"
```

---

## Phase 8: Desktop Account UI (~1 week, parallel with any phase)

### Task 8.1: Account Settings Panel

**Files:**
- Modify: `hive/crates/hive_ui_panels/src/panels/settings.rs`

**Step 1: Add "Hive Cloud" section to settings**

- Show: login/logout button, current tier, subscription status
- If free: "Upgrade to Pro" button (opens Stripe checkout in browser)
- If pro: show usage (tokens used / budget), subscription expiry
- Cloud relay status: connected/disconnected
- Sync status: last synced, storage used

**Step 2: Add login flow**

- "Sign in" button opens `hive.dev/login` in default browser
- Localhost callback server receives JWT (like existing OAuth flow in HiveConfig)
- JWT stored in SecureStorage
- UI updates to show logged-in state

**Step 3: Commit**

```bash
git add hive/crates/hive_ui_panels/src/panels/settings.rs
git commit -m "feat: add Hive Cloud account section to settings"
```

---

## Deployment

### Task D.1: Fly.io Deployment

**Files:**
- Create: `hive-cloud/Dockerfile`
- Create: `hive-cloud/fly.toml`

**Step 1: Write Dockerfile**

```dockerfile
FROM rust:1.77 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/hive-cloud /usr/local/bin/
CMD ["hive-cloud"]
```

**Step 2: Write fly.toml**

```toml
app = "hive-cloud"
primary_region = "iad"

[build]
  dockerfile = "Dockerfile"

[http_service]
  internal_port = 8080
  force_https = true

[env]
  RUST_LOG = "hive_cloud=info"
```

**Step 3: Deploy**

```bash
fly launch
fly secrets set DATABASE_URL=... JWT_SECRET=... STRIPE_SECRET_KEY=... STRIPE_WEBHOOK_SECRET=...
fly deploy
```

**Step 4: Set up Fly Postgres**

```bash
fly postgres create --name hive-cloud-db
fly postgres attach hive-cloud-db
```

**Step 5: Commit**

```bash
git add Dockerfile fly.toml
git commit -m "feat: add Fly.io deployment config"
```

---

## Summary: Build Order & Dependencies

```
Phase 1 (auth/billing) ──────► Phase 2 (relay) ──────► Revenue starts
                        ├────► Phase 3 (task tree UI)   (free, parallel)
                        ├────► Phase 4 (AI gateway) ──► More Pro value
                        ├────► Phase 5 (sync) ─────────► More Pro value
                        ├────► Phase 6 (hive.dev) ─────► Discoverability
                        └────► Phase 7 (teams) ────────► Team revenue

Phase 8 (account UI) runs parallel with any phase.
Deployment (D.1) runs after Phase 1.
```

**Critical path:** Phase 1 → Phase 2 → Deploy = minimum viable monetization (~4 weeks)

**Total estimated effort:** ~14 weeks for everything
