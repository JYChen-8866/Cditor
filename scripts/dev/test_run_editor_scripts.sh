#!/usr/bin/env sh
set -eu

ROOT_DIR=$(CDPATH= cd "$(dirname "$0")/../.." && pwd)
COMPAT_SCRIPT="$ROOT_DIR/scripts/dev/run_editor.sh"
POSTGRES_SCRIPT="$ROOT_DIR/scripts/dev/run_editor_postgres.sh"
SQLITE_SCRIPT="$ROOT_DIR/scripts/dev/run_editor_sqlite.sh"

sh -n "$COMPAT_SCRIPT"
sh -n "$POSTGRES_SCRIPT"
sh -n "$SQLITE_SCRIPT"

compat_output=$(CDITOR_DRY_RUN=1 "$COMPAT_SCRIPT")
case "$compat_output" in
  *'PostgreSQL (document 1)'*) ;;
  *)
    printf 'The compatibility launch script no longer defaults to PostgreSQL.\n' >&2
    exit 1
    ;;
esac

postgres_output=$(
  CDITOR_DRY_RUN=1 \
  CDITOR_DOCUMENT_ID=42 \
  CDITOR_DATABASE_URL='postgres://user:super-secret@localhost/cditor' \
  CDITOR_SQLITE_PATH='/tmp/must-not-win.db' \
  "$POSTGRES_SCRIPT"
)
case "$postgres_output" in
  *'PostgreSQL (document 42)'*) ;;
  *)
    printf 'PostgreSQL launch dry-run did not select the expected backend.\n' >&2
    exit 1
    ;;
esac
case "$postgres_output" in
  *'super-secret'*)
    printf 'PostgreSQL launch output exposed the database URL.\n' >&2
    exit 1
    ;;
esac

sqlite_output=$(
  CDITOR_DRY_RUN=1 \
  CDITOR_DOCUMENT_ID=43 \
  CDITOR_DATABASE_URL='postgres://must-not-win' \
  CDITOR_SQLITE_PATH='/tmp/cditor-script-test.db' \
  "$SQLITE_SCRIPT"
)
case "$sqlite_output" in
  *'SQLite (document 43)'*'/tmp/cditor-script-test.db'*) ;;
  *)
    printf 'SQLite launch dry-run did not select the expected backend and path.\n' >&2
    exit 1
    ;;
esac

if CDITOR_DRY_RUN=1 CDITOR_DOCUMENT_ID=invalid "$SQLITE_SCRIPT" >/dev/null 2>&1; then
  printf 'SQLite launch accepted an invalid document ID.\n' >&2
  exit 1
fi
if CDITOR_DRY_RUN=1 CDITOR_DOCUMENT_ID=invalid "$POSTGRES_SCRIPT" >/dev/null 2>&1; then
  printf 'PostgreSQL launch accepted an invalid document ID.\n' >&2
  exit 1
fi

printf 'Editor backend launch scripts passed.\n'
