#!/usr/bin/env sh
set -eu

ROOT_DIR=$(CDPATH= cd "$(dirname "$0")/../.." && pwd)
POSTGRES_SCRIPT="$ROOT_DIR/scripts/dev/run_editor_postgres.sh"
SQLITE_SCRIPT="$ROOT_DIR/scripts/dev/run_editor_sqlite.sh"

sh -n "$POSTGRES_SCRIPT"
sh -n "$SQLITE_SCRIPT"

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
case "$postgres_output" in
  *'Dry run: cargo run -p cditor-desktop --profile editor-dev'*) ;;
  *)
    printf 'PostgreSQL launch did not default to the editor-dev profile.\n' >&2
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
case "$sqlite_output" in
  *'Dry run: cargo run -p cditor-desktop --profile editor-dev'*) ;;
  *)
    printf 'SQLite launch did not default to the editor-dev profile.\n' >&2
    exit 1
    ;;
esac

sqlite_release_output=$(CDITOR_DRY_RUN=1 "$SQLITE_SCRIPT" --release)
case "$sqlite_release_output" in
  *'Dry run: cargo run -p cditor-desktop --release'*) ;;
  *)
    printf 'SQLite launch did not preserve an explicit release profile.\n' >&2
    exit 1
    ;;
esac
case "$sqlite_release_output" in
  *'--profile editor-dev'*)
    printf 'SQLite launch duplicated an explicit release profile.\n' >&2
    exit 1
    ;;
esac

postgres_profile_output=$(CDITOR_DRY_RUN=1 "$POSTGRES_SCRIPT" --profile custom-dev)
case "$postgres_profile_output" in
  *'Dry run: cargo run -p cditor-desktop --profile custom-dev'*) ;;
  *)
    printf 'PostgreSQL launch did not preserve an explicit named profile.\n' >&2
    exit 1
    ;;
esac
case "$postgres_profile_output" in
  *'--profile editor-dev'*)
    printf 'PostgreSQL launch duplicated an explicit named profile.\n' >&2
    exit 1
    ;;
esac

postgres_equals_profile_output=$(CDITOR_DRY_RUN=1 "$POSTGRES_SCRIPT" --profile=custom-dev)
case "$postgres_equals_profile_output" in
  *'Dry run: cargo run -p cditor-desktop --profile=custom-dev'*) ;;
  *)
    printf 'PostgreSQL launch did not preserve an equals-style named profile.\n' >&2
    exit 1
    ;;
esac
case "$postgres_equals_profile_output" in
  *'--profile editor-dev'*)
    printf 'PostgreSQL launch duplicated an equals-style named profile.\n' >&2
    exit 1
    ;;
esac

sqlite_binary_args_output=$(CDITOR_DRY_RUN=1 "$SQLITE_SCRIPT" -- --profile document-preview)
case "$sqlite_binary_args_output" in
  *'Dry run: cargo run -p cditor-desktop --profile editor-dev -- --profile document-preview'*) ;;
  *)
    printf 'SQLite launch treated a binary argument as a Cargo profile.\n' >&2
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
