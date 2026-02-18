# HiveCode — Master TODO

> Generated 2025-02-17 from comprehensive audit of both repos.
> Nothing leaves this list until it's actually fixed and verified.

---

## CRITICAL — Broken or Misleading

- [ ] **Blockchain deploy is simulation-only** — `deploy_token()` in `hive_blockchain/src/evm.rs:156-200` and `create_spl_token()` in `solana.rs:130-180` never sign or broadcast real transactions. `_private_key` param is unused. Token Launch panel appears to succeed but deploys nothing.
- [ ] **ERC-20 placeholder bytecode** — `hive_blockchain/src/erc20_bytecode.rs:100` returns `"0x_PLACEHOLDER_BYTECODE"`. Even if signing worked, this contract can never deploy.
- [ ] **Integration tool stubs fail silently** — 14 MCP tools in `hive_agents/src/integration_tools.rs:22-58` start as stubs returning `{"note": "Connect ... in Settings"}`. If `wire_integration_handlers()` fails, stubs silently remain active with no error surfaced to user.
- [ ] **Site: favicon.ico path mismatch** — `app/layout.tsx:63` metadata references `/favicon.ico` in public, but file is at `app/favicon.ico`. Verify Vercel serves it correctly or move to `public/`.
- [ ] **Site: React Fragment missing key** — `components/comparison.tsx:222-255` — `<>...</>` in `.map()` needs `<Fragment key={...}>`.

---

## MAJOR — Significant Missing Functionality

### Rust: Modules Built But Never Wired

- [ ] **guardian** (~1100 lines) — `hive_agents/src/guardian.rs` — Full AI output safety scanning. Never called from app.
- [ ] **hiveloop** — `hive_agents/src/hiveloop.rs` (253 lines) — Never imported by any consumer.
- [ ] **fleet_learning** — `hive_ai/src/fleet_learning.rs` — Never imported.
- [ ] **rag** — `hive_ai/src/rag.rs` — Never imported.
- [ ] **semantic_search** — `hive_ai/src/semantic_search.rs` — Never imported.
- [ ] **enterprise** — `hive_core/src/enterprise.rs` — Never imported.
- [ ] **canvas** — `hive_core/src/canvas.rs` — Never imported (WorkflowBuilder uses its own separate state).
- [ ] **webhooks** — `hive_integrations/src/webhooks.rs` — Never imported.

### Rust: Unwired Integrations

- [ ] **PhilipsHue smart home** — `hive_integrations/src/smart_home.rs` — Fully implemented (bridge discovery, light control, scene activation) but never initialized from `hive_app`, `hive_ui`, or `hive_agents`.
- [ ] **ClawdTalk** — `hive_integrations/src/clawdtalk.rs` — Settings panel has a toggle (`clawdtalk_enabled`), config stores it, but module is never instantiated. Toggle does nothing.
- [ ] **AssistantPlugin trait** — `hive_assistant/src/plugin.rs` — Only implementation is `MockPlugin` in `#[cfg(test)]`. No production plugins exist.

### Rust: Stale/Placeholder Logic

- [ ] **Cost estimation placeholder** — `hive_agents/src/knowledge_acquisition.rs:660-663` — Uses flat `$0.000003/token` regardless of model. Inaccurate for all current models.
- [ ] **Hardcoded model benchmark scores** — `hive_ai/src/routing/capability_router.rs:188-350+` — `KNOWN_MODEL_STRENGTHS` has scores "as of early 2025" that will go stale.

### Site: Performance

- [ ] **No loading.tsx files** — No `loading.tsx` in `/app/`, `/app/docs/`, or any route. Docs page is 909 lines — blank screen during navigation.
- [ ] **No Suspense boundaries** — Zero `Suspense` or `next/dynamic` imports despite 7 client components. Entire page loads monolithically.
- [ ] **Honeycomb animation unthrottled** — `components/honeycomb-bg.tsx` — Runs `requestAnimationFrame` at full FPS (~2,400 hexagons redrawn every frame). No `IntersectionObserver`, no `prefers-reduced-motion` check, no mobile throttling. Drains battery on phones.
- [ ] **Image dimensions mismatch** — `components/hero.tsx:98-99` and `components/showcase.tsx:89-90` declare `width={2560} height={1602}` but actual images are 2784x1826. Causes layout shift.
- [ ] **No `sizes` prop on `<Image>` components** — Oversized images served to mobile viewports.
- [ ] **Large screenshots (~5.4MB total)** — 7 PNGs in `public/screenshots/` at 2784x1826 (568KB-937KB each). No explicit `sizes` prop means Next.js may serve larger-than-needed images.
- [ ] **No image preloading on Showcase tabs** — `components/showcase.tsx` — Each tab switch triggers a fresh image load with visible delay. No prefetching or skeleton.

---

## MEDIUM — Consistency, Quality, UX

### Site: Number Inconsistencies

- [ ] **Skills count: 43+ vs 34+** — Landing page (`stats.tsx:5`, `pillars.tsx:15`, `showcase.tsx:23`) says "43+". Docs (`docs/page.tsx:49,382`) says "34+". Pick one.
- [ ] **Panels count: 22 vs 20+ vs 21** — Landing claims "22 live panels". Docs heading says "20+". Actual enumerated list has 21.
- [ ] **Features grid shows 16 panels, claims 22** — `components/features.tsx:123-128` renders only 16 panel names in the grid.
- [ ] **AI providers: 11 claimed, 9 shown** — `components/features.tsx:46-51` shows a 3x3 grid (9 names). FAQ says "11 AI providers". Docs lists 10.

### Site: Branding / Links

- [ ] **Footer copyright says "Pat" not "Airglow LLC"** — `components/footer.tsx:41` says `© Pat` but `layout.tsx:34-36` metadata says Airglow LLC.
- [ ] **Navbar anchor links broken from /docs** — `components/navbar.tsx:8-12` — `#features`, `#faq` etc. try to scroll on the current page. From `/docs`, they don't navigate to homepage sections.
- [ ] **Homepage sections missing `scroll-mt`** — `pillars.tsx` (id="features"), `integrations.tsx` (id="integrations"), `faq.tsx` (id="faq"), `features.tsx` (id="security") — No scroll offset for fixed navbar. Headings hidden behind nav on anchor click.
- [ ] **Apple touch icon is SVG** — `app/layout.tsx:66` — `apple: "/hive-bee.svg"` — Should be 180x180 PNG for iOS.
- [ ] **JSON-LD downloadUrl is not a direct download** — `app/layout.tsx:96` points to GitHub releases page, not a direct .dmg/.tar.gz link.
- [ ] **softwareVersion hardcoded in JSON-LD** — `app/layout.tsx:97` — `"0.3.2"` will go stale on every release.

### Site: Dead Code / Assets

- [ ] **Unused CSS classes** — `app/globals.css` — `grid-bg` (line 69), `animate-fade-in-up` (line 87), `animation-delay-100` through `animation-delay-500` (lines 91-95), `glow-honey` (line 30).
- [ ] **Orphan assets** — `public/hive_bee.png` (unreferenced), `public/screenshots/hive-main.png` (unreferenced), `public/favicon.svg` (unreferenced, only `hive-bee.svg` is used).
- [ ] **`Geist_Mono` font loaded globally** — `app/layout.tsx:10` — Entire monospace font loaded upfront for all pages but only used in a few code blocks.

### Site: Other

- [ ] **localStorage no try/catch** — `components/github-stars.tsx:21-22` — Privacy mode or full storage can throw. No error handling around `localStorage` access.
- [ ] **Comparison table: potentially fictitious competitors** — `components/comparison.tsx:3-4` — "OpenClaw" and "Codex 5.3" not well-known. May damage credibility.
- [ ] **Install command missing https://** — `components/hero.tsx:70` — Displays `curl -fsSL hivecode.app/install.sh | bash` without protocol prefix.
- [ ] **Mobile menu race condition** — `components/navbar.tsx:96` — Menu calls `setMobileOpen(false)` on `/docs` link click but page transition may be slow, causing stale state.

### Rust: Code Quality

- [ ] **47 `#[allow(dead_code)]` annotations** — Spread across UI panels, integrations, AI providers. See audit for full list.
- [ ] **151 `unwrap()`/`expect()` in production code** — Mostly safe regex compilations (~80), but ~30+ on runtime data in `docs_indexer.rs`, `cli.rs`, `main.rs`. Should use `?` or `.ok()`.
- [ ] **Unused config fields** — `hive_core/src/config.rs:306` `close_to_tray_notice_seen` and `:275` `clawdtalk_bot_pin` — defined/defaulted but never read.
- [ ] **Monitor panel placeholder data** — `hive_ui_panels/src/panels/monitor.rs:215-225` — `placeholder()` with hardcoded CPU/memory/disk values exists as fallback.
- [ ] **Shell helpers in wrong scope** — `hive_terminal/src/shell.rs:74-88` — `expected_shell_name()` marked `#[allow(dead_code)]` but should be inside `#[cfg(test)]`.
- [ ] **`conversations::MessageMeta::role` dead field** — `hive_core/src/conversations.rs:56-57` — Deserialized but never accessed.

---

## LOW — Polish

- [ ] **No ESLint config file** — `package.json` has eslint deps and `"lint": "next lint"` script but no `.eslintrc.*` or `eslint.config.*`.
- [ ] **manifest.json missing PWA fields** — `public/manifest.json` — Missing `lang`, `orientation`, `scope`, `categories`. Only SVG icons — needs 192x192 and 512x512 PNGs for install prompts.
- [ ] **robots.ts disallows `/api/` but no API routes exist** — Harmless but unnecessary.
- [ ] **Docs page is 909 lines on a single page** — No code splitting, no dynamic imports for sections below the fold.
- [ ] **No `rel="canonical"` explicitly set** — Next.js may handle via `metadataBase`, but worth verifying in production.

---

## COMPLETED (for reference)

- [x] ~~Secret scanning alert #1~~ — Fake API keys in test code built at runtime via `format!()`. Pushed `361e996`.
- [x] ~~Wire hive_network P2P to app startup~~ — Background tokio runtime.
- [x] ~~Implement MCP SSE transport~~ — Full SseTransport (~430 lines).
- [x] ~~Fix deploy_trigger stub~~ — Real shell dispatch.
- [x] ~~Fix CLI doctor checks~~ — Real statvfs + DNS resolution.
- [x] ~~Update README to v0.3.2~~ — Fixed accuracy claims.
- [x] ~~Add favicon, robots.txt, sitemap.xml, og:image~~ — All SEO assets.
- [x] ~~Custom 404 and error pages~~ — Branded not-found.tsx + error.tsx.
- [x] ~~Enable image optimization~~ — AVIF + WebP via Next.js.
- [x] ~~Mobile docs nav~~ — Floating bottom-sheet DocsMobileNav.
- [x] ~~Accessibility fixes~~ — aria-hidden, skip-nav, FAQ aria, focus-visible.
- [x] ~~SEO structured data~~ — JSON-LD, metadataBase, OG + Twitter cards.
