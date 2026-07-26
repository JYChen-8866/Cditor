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

has_explicit_profile=0
scan_cargo_args=1
for arg in "$@"; do
  if [ "$scan_cargo_args" = 0 ]; then
    continue
  fi
  case "$arg" in
    --)
      scan_cargo_args=0
      ;;
    --release|-r|--profile|--profile=*)
      has_explicit_profile=1
      ;;
  esac
done

if [ "$has_explicit_profile" = 0 ]; then
  set -- --profile editor-dev "$@"
fi
set -- -p cditor-desktop "$@"

if [ "$DRY_RUN" = 1 ]; then
  printf 'Dry run:'
  printf ' %s' "$CARGO_BIN" run "$@"
  printf '\n'
  exit 0
fi

exec "$CARGO_BIN" run "$@"
