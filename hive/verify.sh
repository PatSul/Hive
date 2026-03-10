#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${ROOT_DIR}"

echo "[1/3] Checking validated MVP crates..."
cargo check -p hive_cloud -p hive_admin -p hive_terminal -p hive_blockchain -p hive_ui_panels -p hive_ui -p hive_app

echo "[2/3] Running backend/service tests..."
cargo test -p hive_a2a -p hive_cloud -p hive_admin -p hive_cli -p hive_terminal -p hive_blockchain -q

echo "[3/3] Running token launch UI tests..."
cargo test -p hive_ui --test test_token_launch -q

echo "Verification complete."
