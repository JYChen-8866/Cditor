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

core_runtime_boundary_violations=$(
  grep -R -n -E 'cditor_storage_postgres|(^|[^[:alnum:]_])(sqlx|gpui)([^[:alnum:]_]|$)' \
    --include='*.rs' crates/core/src crates/runtime/src || true
)
if [ -n "$core_runtime_boundary_violations" ]; then
  echo 'error: core/runtime source crossed the storage/UI boundary:' >&2
  echo "$core_runtime_boundary_violations" >&2
  exit 1
fi

if grep -Eq '(^|[[:space:]])gpui[[:space:]]*=' crates/text/Cargo.toml; then
  echo 'error: cditor-text must not depend on GPUI' >&2
  exit 1
fi

text_gpui_violations=$(
  grep -R -n -E '(^|[^[:alnum:]_])gpui([^[:alnum:]_]|$)' --include='*.rs' crates/text/src || true
)
if [ -n "$text_gpui_violations" ]; then
  echo 'error: cditor-text source crossed the GPUI adapter boundary:' >&2
  echo "$text_gpui_violations" >&2
  exit 1
fi

parley_manifest_violations=$(
  find crates -mindepth 2 -maxdepth 2 -name Cargo.toml ! -path 'crates/text/Cargo.toml' -exec grep -H -n -E '(^|[[:space:]])parley[[:space:]]*=' {} + || true
)
parley_source_violations=$(
  find crates -path 'crates/text' -prune -o -type f -name '*.rs' -print | xargs grep -n -E '(^|[^[:alnum:]_])parley::' || true
)
if [ -n "$parley_manifest_violations$parley_source_violations" ]; then
  echo 'error: Parley may only be used directly by cditor-text:' >&2
  [ -z "$parley_manifest_violations" ] || echo "$parley_manifest_violations" >&2
  [ -z "$parley_source_violations" ] || echo "$parley_source_violations" >&2
  exit 1
fi

for legacy_geometry_file in \
  crates/app/src/gui/text/layout.rs \
  crates/app/src/gui/text/fallback_render.rs \
  crates/app/src/gui/overlay/caret_overlay.rs
do
  if [ -e "$legacy_geometry_file" ]; then
    echo "error: legacy App text geometry file must stay removed: $legacy_geometry_file" >&2
    exit 1
  fi
done

legacy_app_geometry_violations=$(
  grep -R -n -E \
    '(^|[^[:alnum:]_])(GpuiWrappedLine|CaretGeometryCache|VisualLineLayout|RichTextLayoutCache|CachedRichTextLayout)([^[:alnum:]_]|$)' \
    --include='*.rs' crates/app/src || true
)
if [ -n "$legacy_app_geometry_violations" ]; then
  echo 'error: App text geometry must come from the cditor-text Parley snapshot:' >&2
  echo "$legacy_app_geometry_violations" >&2
  exit 1
fi

if grep -Eq 'text_offset' crates/editor/src/scroll/anchor.rs; then
  echo 'error: CaretAnchor must remain geometry-only; text focus belongs to EditingSession selection' >&2
  exit 1
fi

duplicate_caret_truth_violations=$(
  grep -R -n -E 'caret_anchor\.text_offset' --include='*.rs' crates || true
)
if [ -n "$duplicate_caret_truth_violations" ]; then
  echo 'error: caret text focus must be derived from EditingSession selected_range:' >&2
  echo "$duplicate_caret_truth_violations" >&2
  exit 1
fi

printable_keydown_violations=$(
  grep -R -n -E 'InsertChar|InsertSpaceOrMarkdownShortcut' --include='*.rs' crates/app/src || true
)
if [ -n "$printable_keydown_violations" ]; then
  echo 'error: printable text must enter through GPUI EntityInputHandler, not a keydown command:' >&2
  echo "$printable_keydown_violations" >&2
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
