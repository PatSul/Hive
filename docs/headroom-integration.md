# Headroom (context compression) — integration guide

[Headroom](https://github.com/chopratejas/headroom) (Apache-2.0) compresses tool
output, RAG chunks, files, and history *before* they reach an LLM — "60–95% fewer
tokens, same answers." It runs as a local sidecar (Python), exposing both an
**OpenAI-compatible proxy** and an **MCP server**.

**HIVE needs no new code to use it.** HIVE's provider/route layer (the
multi-provider routing added in the Δ3/Δ4 work) already supports any
OpenAI-compatible endpoint, and HIVE has an MCP client. So Headroom plugs into
the existing extensibility two ways.

## Option A — as an OpenAI-compatible compression proxy (recommended)

1. Install + run Headroom locally, configured with your real provider target/key:
   ```bash
   pip install "headroom-ai[all]"
   headroom proxy --port 8787
   ```
2. Point HIVE at the proxy using the **existing** OpenAI-compatible proxy config
   — set `litellm_url` (or `local_provider_url`) to the Headroom base URL
   (e.g. `http://localhost:8787/v1`) in `~/.hive/config.json` or via `hive config`.
   Verify the exact base-URL suffix Headroom expects for OpenAI-SDK clients.
3. Optionally add a per-model **custom route** in the Models page pointing at the
   Headroom URL, so only chosen models route through compression.

What still applies when routing through Headroom:
- **Δ3 cost/policy routing** and **cost tracking** — HIVE still picks the model and records spend.
- **Δ5 egress redaction** — HIVE scrubs secrets/keys from outbound content *before* handing off to Headroom.
- Headroom then compresses and forwards to your real provider.

## Option B — as an MCP server (compression + reversible-retrieval tools)

Headroom also runs as an MCP server. Add it in HIVE's MCP client settings so the
swarm/agents can call its tools (e.g. reversible-compression `headroom_retrieve`).

## Notes
- **Optional.** HIVE works fully without Headroom. It *stacks* with HIVE's own
  token-efficiency (TOON encoding, the TF-IDF context engine) and the Δ3 cost
  router — orthogonal savings.
- Headroom is a separate process; HIVE talks to it over HTTP / MCP, so there is
  no in-process coupling and nothing to build into HIVE itself.
