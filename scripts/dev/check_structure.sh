#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")/../.."

if [ -d crates/engine ]; then
  echo 'error: crates/engine was renamed to crates/runtime' >&2
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
