# Hive Cloud Monetization Design

**Date:** 2026-02-26
**Status:** Approved
**Model:** Obsidian-style — open source app, paid server services

---

## Philosophy

Everything in the desktop app is free and open source. Users pay for **server services** that cost real money to run. Self-hosters never pay a dime. The server IS the product — no license keys, no DRM, no client-side gating.

Skills and integrations are always free for everyone. No marketplace commissions.

---

## Product Tiers

| | **Free** | **Pro ($8/mo)** | **Team ($20/mo/seat)** |
|---|---|---|---|
| Desktop app | Full app | Full app | Full app |
| AI providers | Own API keys | Own keys + Managed Gateway | Same + team cost dashboard |
| Remote access | LAN only | Cloud relay | Cloud relay + multi-user |
| Sync | Local only | Encrypted cloud sync | Sync + shared workspaces |
| Skills/Agents | All free | All free | All free |
| Integrations | All free | All free | All free |
| Shield scanning | Local | Local + cloud compliance reports | Same + team audit logs |
| Support | Community | Email support | Priority support |

---

## Repository Structure

Two repos. Client is public, server is private. They communicate via HTTPS/WSS only — zero shared Rust code.

```
github.com/AirglowStudios/hive          (PUBLIC)
├── crates/
│   ├── hive_app/        — desktop app binary
│   ├── hive_remote/     — relay CLIENT code (connect, encrypt, pair)
│   ├── hive_ai/         — provider integrations + HiveGatewayProvider
│   ├── hive_core/       — config, SecureStorage (JWT persistence)
│   ├── hive_agents/     — orchestration (emits live task events)
│   ├── hive_ui/         — workspace, account settings panel
│   └── ...all other crates

github.com/AirglowStudios/hive-cloud    (PRIVATE)
├── src/
│   ├── main.rs          — Axum router, shared state
│   ├── auth/            — JWT issuing/refresh, Stripe webhooks, tier checks
│   ├── relay/           — WebSocket relay hub (room management)
│   ├── gateway/         — AI provider proxy, token metering, cost markup
│   ├── sync/            — Encrypted blob storage (zero-knowledge)
│   └── billing/         — Stripe subscription management, usage tracking
├── migrations/          — Postgres schema
├── Dockerfile
└── fly.toml             — Deploy to Fly.io
```

---

## Auth & Billing Flow

### Login
1. User clicks "Sign in to Hive Cloud" in Settings
2. App opens browser to `hive.dev/login`
3. User signs in via magic link (email) or GitHub OAuth
4. Browser redirects to localhost callback with auth code
5. App exchanges code for JWT, stores in `hive_core::SecureStorage`
6. App sends JWT with every cloud request

No passwords stored in the app. JWT expires, app refreshes silently.

### Purchase
1. User clicks "Upgrade to Pro" in Settings
2. App opens browser to Stripe Checkout (hosted by Stripe)
3. User pays. Stripe webhook hits hive-cloud, updates account tier
4. Next JWT check returns `tier=pro`
5. Cloud features activate in the app

We never touch credit cards. Stripe handles payment UI and PCI compliance.

### Server-Side Tier Check
```rust
async fn relay_connect(jwt: JWT) -> Result<WebSocket> {
    let account = auth::verify(jwt)?;
    if account.tier < Tier::Pro {
        return Err(PaymentRequired("Cloud relay requires Pro"));
    }
    // proceed
}
```

---

## Cloud Service 1: Relay (Ship First)

Takes the existing `hive_remote` relay protocol and runs it on our server.

### Client Side (existing in hive_remote)
- WebSocket connect to `wss://relay.hive.dev`
- Authenticate with JWT
- Paired device connects to same room
- Encrypted frames flow (X25519 + AES-256-GCM)

### Server Endpoints
```
POST   /auth/login          → issue JWT
GET    /relay/ws             → WebSocket upgrade (Pro+ only)
POST   /relay/rooms          → create relay room
GET    /relay/rooms/:id      → join relay room
```

### What Exists (85%)
- Pairing protocol, encryption, frame format, session journal — all done
- Relay frame types: Register, Authenticate, CreateRoom, JoinRoom, Forward, Ping/Pong

### What to Build
- Server-side room management (stateful WebSocket hub)
- JWT auth middleware on connection
- Rate limiting (max 3 concurrent devices on Pro, 10 on Team)
- Bandwidth metering

### Effort: ~2 weeks

---

## Cloud Service 2: Managed AI Gateway

Proxies AI requests through our server so users don't need their own API keys.

### Client Side (modify hive_ai)
Add `HiveGatewayProvider` implementing existing `AiProvider` trait:
```
App → POST https://api.hive.dev/v1/chat → server proxies to real provider → streams back
```

All existing providers still work (own API keys). Gateway is an additional option.

### Server Endpoints
```
POST   /v1/chat              → proxy to provider, stream response
POST   /v1/chat/estimate     → cost estimate before executing
GET    /v1/usage             → token/cost usage for billing period
GET    /v1/models            → available models for user's tier
```

### Billing Model
- Server holds API keys for all 15+ providers
- User request → server calls real API → streams back
- Meter tokens, charge cost x 1.3 markup
- Pro tier includes $20/mo of token budget, overage billed separately
- Existing `CostTracker` logic reused server-side per-user

### Effort: ~3 weeks

---

## Cloud Service 3: Sync

Encrypted sync of conversations, configs, and settings across devices.

### Client Side (new module in hive_core)
```
On change → encrypt blob locally → PUT /v1/blobs/:key
On startup → GET /v1/blobs/manifest → pull newer items → decrypt locally
```

### Server Endpoints
```
PUT    /v1/blobs/:key        → store encrypted blob
GET    /v1/blobs/:key        → retrieve encrypted blob
GET    /v1/blobs/manifest    → list all blobs with timestamps
DELETE /v1/blobs/:key        → delete a blob
```

### Zero-Knowledge Design
- Encryption/decryption happens client-side
- Server stores opaque bytes, never sees plaintext
- Key derived from user's account credentials
- Last-write-wins with timestamps (no server-side merge)

### What Syncs
- Conversation history
- Agent/skill configurations
- App settings & theme
- API key vault (double-encrypted)

### Effort: ~3 weeks

---

## UI Enhancement: Live Task Tree

Upgrade from aggregate progress bar to real-time expandable task tree in chat.

### What It Looks Like
```
HiveMind working on "Refactor auth module"
├── Done    Analyze current auth flow          2.1s   $0.003
├── Done    Identify breaking dependencies     1.8s   $0.002
├── Running Rewrite token validation           ...
├── Pending Update unit tests
└── Pending Run test suite
━━━━━━━━━━━━━━━━━━━━━━━ 2/5 tasks  40%
```

### What to Build
1. Coordinator emits live events: TaskStarted, TaskProgress, TaskCompleted, TaskFailed
2. Chat service subscribes and renders expandable task block inline
3. Real-time updates — each event updates tree in place (no message spam)
4. Collapsible — click to expand, shows output per task
5. Parallel wave indication — tasks in same wave show concurrency

### This is a free feature for everyone. Makes the product feel premium.

### Effort: ~1 week

---

## Marketing: hive.dev

Simple landing page / marketing site:
- What is Hive (open source AI coding assistant)
- Pricing page (Free / Pro / Team)
- Login / Sign up
- Documentation
- Download links (GitHub releases)

Static site (Astro or plain HTML) deployed on Vercel (free tier).

### Effort: ~1 week

---

## Hosting & Cost Estimates

### Fly.io (server)
- 1 shared CPU VM: ~$5/mo
- Persistent storage (sync blobs): ~$0.15/GB/mo
- Bandwidth: 100GB free, then $0.02/GB
- Break-even: ~1 Pro subscriber covers hosting costs

### Scaling
- At 100+ subscribers: move to dedicated VMs
- At 1000+: consider multi-region for relay latency
- Until then: single Fly.io machine handles everything

### External Services
- Stripe: 2.9% + $0.30 per transaction
- Email (magic links): Resend.com free tier (3k/mo)
- Domain: hive.dev (~$15/yr)

---

## Build Order

| Phase | What | Effort | Revenue Impact |
|-------|------|--------|---------------|
| **1** | `hive-cloud` repo + auth/billing (Stripe + JWT) | 2 weeks | Foundation |
| **2** | Cloud Relay server + client wiring | 2 weeks | **Pro subs begin** |
| **3** | Live Task Tree UI | 1 week | Product quality |
| **4** | Managed AI Gateway server + HiveGatewayProvider | 3 weeks | Adds gateway to Pro |
| **5** | Cloud Sync server + client sync module | 3 weeks | Adds sync to Pro |
| **6** | hive.dev marketing site + docs | 1 week | Discoverability |
| **7** | Team tier (shared configs, cost dashboard) | 2 weeks | **Team subs begin** |

**Total: ~14 weeks to full monetization stack**

---

## Security Considerations

- All cloud traffic over HTTPS/WSS (TLS 1.3)
- JWT tokens short-lived (1hr), refresh tokens in SecureStorage
- Sync is zero-knowledge (server never sees plaintext)
- Relay uses end-to-end encryption (X25519 + AES-256-GCM)
- Gateway API keys stored server-side only, never sent to client
- Stripe handles all payment data (PCI compliance)
- Rate limiting on all endpoints to prevent abuse
- No user data in URL parameters
