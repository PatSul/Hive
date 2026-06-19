# Technology Stack

**Analysis Date:** 2026-06-18

## Languages

**Primary:**
- Rust 2024 edition - Native desktop app, backend services, CLI tools, and most application logic under `hive/crates/`.

**Secondary:**
- Shell and batch scripts - Build and verification wrappers in `hive/build.sh`, `hive/verify.sh`, `hive/build.bat`, and `hive/verify.bat`.
- Markdown/TOML/YAML/HTML/CSS/JS - Documentation, specs, workflow definitions, embedded remote web UI, and skill/config formats.

## Runtime

**Environment:**
- Rust stable toolchain - Required for the workspace in `hive/Cargo.toml`.
- Native desktop runtime via GPUI, not Electron or a browser wrapper.
- Tokio async runtime - Used across AI providers, remote services, networking, cloud, and background work.

**Package Manager:**
- Cargo - Workspace package manager and build runner.
- Lockfile: `hive/Cargo.lock` present.

## Frameworks

**Core:**
- `gpui` 0.2.2 - Native GPU UI framework for the desktop app.
- `gpui-component` 0.5.1 - UI components, title bar, icons, inputs, and scroll support.
- `tokio` 1.x - Async runtime.
- `axum` 0.7/0.8 - HTTP services for A2A, remote, and cloud crates.

**Testing:**
- Rust built-in test harness - Unit and integration tests under crate `tests/` folders and `#[cfg(test)]` modules.
- `tokio::test` - Async tests for provider, remote, A2A, and service code.
- Targeted validation scripts - `hive/verify.bat` and `hive/verify.sh`.

**Build/Dev:**
- Cargo profiles - Release profile in `hive/Cargo.toml` uses `opt-level = 3` and `strip = true`.
- GitHub Actions - CI and release workflows in `.github/workflows/`.
- Windows helper scripts - MSVC/SDK setup wrappers in `hive/build.bat` and `build_check.ps1`.

## Key Dependencies

**Critical:**
- `reqwest` - REST/SSE API calls for AI providers, integrations, updates, and cloud services.
- `serde`, `serde_json`, `toml`, `toon-format` - Configuration, provider payloads, skills, and token-efficient context formats.
- `rusqlite` with bundled SQLite - Conversation, memory, learning, cost, and app persistence.
- `lancedb`, `arrow-array`, `arrow-schema` - Embedded vector memory in `hive_ai`.
- `git2` - Git operations and repository workflows.
- `aes-gcm`, `argon2`, `sha2`, `rand` - Encrypted storage, secrets handling, and crypto primitives.
- `notify`, `ignore`, `regex` - File watching, gitignore-aware indexing/search, and scanners.
- `a2a-rs`, `tower`, `tower-http` - Agent-to-Agent HTTP service and middleware.

**Infrastructure:**
- `tracing`, `tracing-subscriber`, `tracing-appender` - Structured logging and file rotation.
- `uuid`, `chrono`, `dirs`, `once_cell`, `parking_lot` - Cross-cutting platform utilities.
- `rust-embed`, `image`, `notify-rust` - Embedded assets, tray icon image decoding, and OS notifications.

## Configuration

**Environment:**
- User config is centered on `~/.hive/config.json` through `hive_core::config::HiveConfig`.
- API keys and provider settings are read from local config and secure storage. Do not put real keys in docs.
- Some integrations also read environment variables, including `BITBUCKET_USERNAME`, `BITBUCKET_APP_PASSWORD`, `GITLAB_PRIVATE_TOKEN`, `HIVE_NODE_NAME`, and `HIVE_CLOUD_BIND`.

**Build:**
- `hive/Cargo.toml` defines the workspace and shared dependencies.
- `.github/workflows/ci.yml` runs the targeted verification matrix on Windows, macOS, and Linux.
- `.github/workflows/release.yml` builds Windows, macOS arm64, and Linux x64 release artifacts.
- `.github/workflows/auto-release.yml` bumps versions and invokes the release workflow on `main` changes under `hive/**`.

## Platform Requirements

**Development:**
- Windows/macOS/Linux with Rust stable.
- GPUI platform dependencies: Linux requires Vulkan/Wayland/XKB/GTK-style system packages listed in CI and README.
- Protocol Buffers compiler is installed in CI through `arduino/setup-protoc`.
- Windows local scripts assume MSVC/Windows SDK availability.

**Production:**
- Desktop binary releases: Windows x64 zip, macOS Apple Silicon DMG/tarball, Linux x64 tarball.
- `hive_cloud` can run as a separate Axum HTTP service, defaulting to `127.0.0.1:3000` unless `HIVE_CLOUD_BIND` is set.

---

*Stack analysis: 2026-06-18*
*Update after major dependency or platform changes*
