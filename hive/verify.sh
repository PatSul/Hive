#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${ROOT_DIR}"

echo "[1/4] Checking validated MVP crates..."
cargo check -p hive_cloud -p hive_admin -p hive_terminal -p hive_blockchain -p hive_ui_panels -p hive_ui -p hive_app

echo "[2/4] Running security-critical crate tests..."
cargo test -p hive_core -p hive_agents -q

echo "[3/4] Running backend/service tests..."
cargo test -p hive_a2a -p hive_cloud -p hive_admin -p hive_cli -p hive_terminal -p hive_blockchain -q

echo "[4/4] Running token launch UI tests..."
cargo test -p hive_ui --test test_token_launch -q

echo "Verification complete."
