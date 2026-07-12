#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/../.."

echo "Starting Cditor minimal editor..."
export CDITOR_TRACE_TABLE="${CDITOR_TRACE_TABLE:-0}"
export CDITOR_DATABASE_URL="${CDITOR_DATABASE_URL:-postgres://cditor:cditor@localhost:5432/cditor_dev}"
cargo run -p cditor-app
