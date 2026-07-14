#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")/../.."

if [ -d crates/engine ]; then
  echo 'error: crates/engine was renamed to crates/runtime' >&2
  exit 1
fi

if grep -Eq 'cditor-storage-postgres|(^|[[:space:]])sqlx[[:space:]]*=|(^|[[:space:]])gpui[[:space:]]*=' crates/runtime/Cargo.toml; then
  echo 'error: runtime must not depend on PostgreSQL, SQLx, or GPUI' >&2
  exit 1
fi

runtime_boundary_violations=$(
  grep -R -n -E 'cditor_storage_postgres|(^|[^[:alnum:]_])(sqlx|gpui)([^[:alnum:]_]|$)' \
    --include='*.rs' crates/runtime/src || true
)
if [ -n "$runtime_boundary_violations" ]; then
  echo 'error: runtime source crossed the storage/UI boundary:' >&2
  echo "$runtime_boundary_violations" >&2
  exit 1
fi

oversized=$(
  find crates \
    -path 'crates/ding-board' -prune -o \
    -type f -name '*.rs' -exec wc -l {} + \
    | awk '$2 != "total" && $1 > 700 { print $1 " " $2 }'
)
if [ -n "$oversized" ]; then
  echo 'error: non-whiteboard Rust files must not exceed 700 lines:' >&2
  echo "$oversized" >&2
  exit 1
fi

system_files=$(
  find . \
    -path './.git' -prune -o \
    -path './target' -prune -o \
    -path './crates/ding-board' -prune -o \
    -name '.DS_Store' -print
)
if [ -n "$system_files" ]; then
  echo 'error: system metadata found outside the excluded whiteboard crate:' >&2
  echo "$system_files" >&2
  exit 1
fi

echo 'Structure checks passed.'
