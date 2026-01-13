#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_PATH="${LOG_PATH:-/tmp/codex_router.log}"
RUST_LOG="${RUST_LOG:-debug}"
RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

cd "$ROOT_DIR"
echo "Running in: $ROOT_DIR"
echo "Log file: $LOG_PATH"

RUST_LOG="$RUST_LOG" RUST_BACKTRACE="$RUST_BACKTRACE" cargo run 2>&1 | tee "$LOG_PATH"
