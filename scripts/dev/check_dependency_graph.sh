#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")/../.."

expected=doc/architecture/dependency-graph-v2.txt
actual=$(mktemp)
trap 'rm -f "$actual"' EXIT HUP INT TERM

{
  sed -n '1,2p' "$expected"
  cargo metadata --no-deps --format-version 1 \
    | jq -r '
      .packages[]
      | .name as $name
      | ([.dependencies[] | select(.path != null and .kind == null) | .name]
          | unique | sort) as $production
      | ([.dependencies[] | select(.path != null and .kind == "dev") | .name]
          | unique | sort) as $development
      | "\($name) | prod=\($production | join(",")) | dev=\($development | join(","))"
    ' \
    | sort
} > "$actual"

if ! diff -u "$expected" "$actual"; then
  echo 'error: cargo metadata workspace dependency graph differs from the documented graph' >&2
  exit 1
fi

echo 'Dependency graph check passed.'
