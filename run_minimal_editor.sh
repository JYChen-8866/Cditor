#!/bin/bash
set -euo pipefail

echo "Starting CDitor V2 minimal editor..."
export CDITOR_TRACE_TABLE="${CDITOR_TRACE_TABLE:-0}"
cargo run -p cditor-app
