#!/usr/bin/env sh
set -eu

ROOT_DIR=$(CDPATH= cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT_DIR"

DOCUMENT_ID="${CDITOR_DOCUMENT_ID:-1}"
SQLITE_PATH="${CDITOR_SQLITE_PATH:-$ROOT_DIR/workspace.cditor.db}"
DRY_RUN="${CDITOR_DRY_RUN:-0}"
CARGO_BIN="${CARGO:-cargo}"

case "$DOCUMENT_ID" in
  ''|*[!0-9]*)
    printf 'CDITOR_DOCUMENT_ID must be an unsigned integer, got: %s\n' "$DOCUMENT_ID" >&2
    exit 2
    ;;
esac

case "$DRY_RUN" in
  0|1) ;;
  *)
    printf 'CDITOR_DRY_RUN must be 0 or 1, got: %s\n' "$DRY_RUN" >&2
    exit 2
    ;;
esac

unset CDITOR_DATABASE_URL
export CDITOR_SQLITE_PATH="$SQLITE_PATH"
export CDITOR_DOCUMENT_ID="$DOCUMENT_ID"
export CDITOR_TRACE_TABLE="${CDITOR_TRACE_TABLE:-0}"

printf 'Starting Cditor with SQLite (document %s).\n' "$DOCUMENT_ID"
printf 'SQLite database: %s\n' "$SQLITE_PATH"

if [ "$DRY_RUN" = 1 ]; then
  printf 'Dry run: %s run -p cditor-app\n' "$CARGO_BIN"
  exit 0
fi

exec "$CARGO_BIN" run -p cditor-app "$@"
