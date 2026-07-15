#!/usr/bin/env sh
set -eu

ROOT_DIR=$(CDPATH= cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT_DIR"

DOCUMENT_ID="${CDITOR_DOCUMENT_ID:-1}"
DATABASE_URL="${CDITOR_DATABASE_URL:-postgres://cditor:cditor@localhost:5432/cditor_dev}"
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

unset CDITOR_SQLITE_PATH
export CDITOR_DATABASE_URL="$DATABASE_URL"
export CDITOR_DOCUMENT_ID="$DOCUMENT_ID"
export CDITOR_TRACE_TABLE="${CDITOR_TRACE_TABLE:-0}"

printf 'Starting Cditor with PostgreSQL (document %s).\n' "$DOCUMENT_ID"
printf 'Database URL is configured via CDITOR_DATABASE_URL (value hidden).\n'

if [ "$DRY_RUN" = 1 ]; then
  printf 'Dry run: %s run -p cditor-app\n' "$CARGO_BIN"
  exit 0
fi

exec "$CARGO_BIN" run -p cditor-app "$@"
