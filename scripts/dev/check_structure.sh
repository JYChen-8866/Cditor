#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")/../.."

for legacy_crate in app core editor runtime store store-postgres store-sqlite text ai
do
  if [ -d "crates/$legacy_crate" ]; then
    echo "error: legacy crate directory still exists: crates/$legacy_crate" >&2
    exit 1
  fi
done

for root in crates components apps
do
  [ -d "$root" ] || continue
  find "$root" -mindepth 2 -maxdepth 2 -name Cargo.toml -print | while IFS= read -r manifest
  do
    crate_dir=$(basename "$(dirname "$manifest")")
    package_name=$(sed -n 's/^name[[:space:]]*=[[:space:]]*"\([^"]*\)"/\1/p' "$manifest" | head -n 1)
    case "$package_name" in
      cditor-*) ;;
      *)
        echo "error: workspace package must use the cditor- prefix: $manifest ($package_name)" >&2
        exit 1
        ;;
    esac
    if [ "$crate_dir" != "$package_name" ]; then
      echo "error: crate directory must match its Cargo package: $crate_dir != $package_name" >&2
      exit 1
    fi
  done
done

for removed_package in cditor-theme-types cditor-collaboration cditor-editor-core ding-board
do
  if find crates components apps -name Cargo.toml -exec grep -H -E "^name[[:space:]]*=[[:space:]]*\"$removed_package\"" {} + 2>/dev/null | grep -q .; then
    echo "error: removed package name must not return: $removed_package" >&2
    exit 1
  fi
done

if grep -Eq 'cditor-storage-postgres|cditor-storage-sqlite|(^|[[:space:]])sqlx[[:space:]]*=|(^|[[:space:]])gpui[[:space:]]*=' crates/cditor-runtime/Cargo.toml; then
  echo 'error: runtime must not depend on PostgreSQL, SQLx, or GPUI' >&2
  exit 1
fi

if grep -Eq 'cditor-storage-postgres|cditor-storage-sqlite|(^|[[:space:]])sqlx[[:space:]]*=|(^|[[:space:]])gpui[[:space:]]*=' crates/cditor-session/Cargo.toml; then
  echo 'error: session must depend on ports, not GPUI or concrete storage adapters' >&2
  exit 1
fi

if [ ! -f crates/cditor-session/tests/session_integration.rs ]; then
  echo 'error: cditor-session must retain its headless integration test target' >&2
  exit 1
fi

if grep -R -n -E 'Arc[[:space:]]*<[[:space:]]*Mutex[[:space:]]*<[[:space:]]*DocumentRuntime' \
  --include='*.rs' crates/cditor-session/src | grep -q .; then
  echo 'error: session must keep one serial Runtime owner instead of Arc<Mutex<DocumentRuntime>>' >&2
  exit 1
fi

if grep -R -n -E 'StoragePersistenceState' --include='*.rs' crates/cditor-editor/src crates/cditor-session/src | grep -q .; then
  echo 'error: legacy Editor-owned StoragePersistenceState must not return' >&2
  exit 1
fi

if grep -R -n -E 'ready_runtime(_ref)?|\.storage_persistence|storage_persistence[[:space:]]*:' \
  --include='*.rs' crates/cditor-editor/src | grep -q .; then
  echo 'error: GPUI Editor must access the document and persistence policy through EditorSessionHandle' >&2
  exit 1
fi

if grep -R -n -E 'Ready[[:space:]]*\([[:space:]]*(Box[[:space:]]*<[[:space:]]*)?DocumentRuntime|Ready[[:space:]]*\([[:space:]]*Box::new' \
  --include='*.rs' crates/cditor-editor/src | grep -q .; then
  echo 'error: CditorViewState::Ready must contain EditorSessionHandle, not DocumentRuntime' >&2
  exit 1
fi

if grep -n -E 'StorageSession' \
  crates/cditor-editor/src/app/lifecycle.rs \
  crates/cditor-editor/src/app/cditor_v2_view.rs \
  crates/cditor-editor/src/app/render.rs \
  crates/cditor-editor/src/app/sdk.rs | grep -q .; then
  echo 'error: GPUI View construction and state must not accept or retain StorageSession' >&2
  exit 1
fi

if grep -n -E 'CditorRuntimeLoadResult|load_runtime_from_|pub[[:space:]]+(runtime|storage_session)[[:space:]]*:' \
  crates/cditor-app/src/storage_host.rs crates/cditor-app/src/wiring.rs | grep -q .; then
  echo 'error: App cold start must return a prepared EditorSession, not Runtime or StorageSession ownership' >&2
  exit 1
fi

if grep -n -E 'apply_(loaded|recovered)_runtime' \
  crates/cditor-editor/src/app/lifecycle.rs crates/cditor-app/src/wiring.rs | grep -q .; then
  echo 'error: GPUI Editor loading must adopt EditorSessionHandle instead of DocumentRuntime' >&2
  exit 1
fi

if grep -Eq 'cditor-storage-postgres|cditor-storage-sqlite|(^|[[:space:]])cditor-runtime[[:space:]]*=|(^|[[:space:]])cditor-editor[[:space:]]*=|(^|[[:space:]])cditor-whiteboard[[:space:]]*=|(^|[[:space:]])sqlx[[:space:]]*=' crates/cditor-api/Cargo.toml; then
  echo 'error: API contracts must not depend on concrete storage, runtime, editor, whiteboard engine, or SQLx' >&2
  exit 1
fi

api_backend_violations=$(
  grep -R -n -E 'cditor_storage_(postgres|sqlite)|(^|[^[:alnum:]_])sqlx([^[:alnum:]_]|$)' \
    --include='*.rs' crates/cditor-api/src || true
)
if [ -n "$api_backend_violations" ]; then
  echo 'error: API source crossed the concrete storage boundary:' >&2
  echo "$api_backend_violations" >&2
  exit 1
fi

core_runtime_boundary_violations=$(
  grep -R -n -E 'cditor_storage_postgres|(^|[^[:alnum:]_])(sqlx|gpui)([^[:alnum:]_]|$)' \
    --include='*.rs' crates/cditor-core/src crates/cditor-runtime/src || true
)
if [ -n "$core_runtime_boundary_violations" ]; then
  echo 'error: core/runtime source crossed the storage/UI boundary:' >&2
  echo "$core_runtime_boundary_violations" >&2
  exit 1
fi

if grep -Eq '(^|[[:space:]])gpui[[:space:]]*=' crates/cditor-text/Cargo.toml; then
  echo 'error: cditor-text must not depend on GPUI' >&2
  exit 1
fi

text_gpui_violations=$(
  grep -R -n -E '(^|[^[:alnum:]_])gpui([^[:alnum:]_]|$)' --include='*.rs' crates/cditor-text/src || true
)
if [ -n "$text_gpui_violations" ]; then
  echo 'error: cditor-text source crossed the GPUI adapter boundary:' >&2
  echo "$text_gpui_violations" >&2
  exit 1
fi

parley_manifest_violations=$(
  find crates -mindepth 2 -maxdepth 2 -name Cargo.toml ! -path 'crates/cditor-text/Cargo.toml' -exec grep -H -n -E '(^|[[:space:]])parley[[:space:]]*=' {} + || true
)
parley_source_violations=$(
  find crates -path 'crates/cditor-text' -prune -o -type f -name '*.rs' -print | xargs grep -n -E '(^|[^[:alnum:]_])parley::' || true
)
if [ -n "$parley_manifest_violations$parley_source_violations" ]; then
  echo 'error: Parley may only be used directly by cditor-text:' >&2
  [ -z "$parley_manifest_violations" ] || echo "$parley_manifest_violations" >&2
  [ -z "$parley_source_violations" ] || echo "$parley_source_violations" >&2
  exit 1
fi

for legacy_geometry_file in \
  crates/cditor-editor/src/text/layout.rs \
  crates/cditor-editor/src/text/fallback_render.rs \
  crates/cditor-editor/src/overlay/caret_overlay.rs
do
  if [ -e "$legacy_geometry_file" ]; then
    echo "error: legacy App text geometry file must stay removed: $legacy_geometry_file" >&2
    exit 1
  fi
done

legacy_app_geometry_violations=$(
  grep -R -n -E \
    '(^|[^[:alnum:]_])(GpuiWrappedLine|CaretGeometryCache|VisualLineLayout|RichTextLayoutCache|CachedRichTextLayout)([^[:alnum:]_]|$)' \
    --include='*.rs' crates/cditor-editor/src || true
)
if [ -n "$legacy_app_geometry_violations" ]; then
  echo 'error: App text geometry must come from the cditor-text Parley snapshot:' >&2
  echo "$legacy_app_geometry_violations" >&2
  exit 1
fi

if grep -Eq 'text_offset' crates/cditor-viewport/src/scroll/anchor.rs; then
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

migrated_runtime_mutation_violations=$(
  grep -R -n -E \
    '\.(undo_focused_block|redo_focused_block|select_all_command|delete_selected_block_selection|apply_slash_block_kind|toggle_block_fold|apply_ai_preview|set_block_color|toggle_inline_mark_on_selection|set_inline_color_on_selection|insert_paragraph_after_block|insert_paragraph_after_focused|focus_or_create_down_placer_paragraph|insert_image_asset_after_focused|insert_soft_line_break|insert_markdown_paste|paste_clipboard_selection|paste_delimited_table_text_at_focused_cell|replace_text_from_paste|handle_enter|indent_focused_block|outdent_focused_block|move_block_subtree_before|move_block_subtree_to_parent|delete_block_by_id|toggle_todo_checked|set_code_block_language|convert_focused_block_kind|set_table_header_rows|set_table_header_columns|insert_table_row|insert_table_column|delete_table_row|delete_table_column|duplicate_table_row|duplicate_table_column|clear_table_range|set_table_cell_background_color|update_image_display_width_ratio|set_table_row_height|set_table_column_width|move_table_row|move_table_column)\(|runtime\.(delete_backward|delete_forward)\(' \
    --include='*.rs' crates/cditor-editor/src || true
)
if [ -n "$migrated_runtime_mutation_violations" ]; then
  echo 'error: Editor must route migrated document mutations through Runtime dispatch:' >&2
  echo "$migrated_runtime_mutation_violations" >&2
  exit 1
fi

legacy_platform_input_mutation_violations=$(
  grep -R -n -E \
    '\.(replace_text_from_platform|begin_or_update_composition|begin_or_update_composition_with_selection|commit_composition|cancel_composition|commit_composition_before_external_focus|delete_active_selection)\(' \
    --include='*.rs' crates/cditor-editor/src || true
)
if [ -n "$legacy_platform_input_mutation_violations" ]; then
  echo 'error: GPUI platform input must use the versioned Runtime realtime port:' >&2
  echo "$legacy_platform_input_mutation_violations" >&2
  exit 1
fi

direct_document_selection_violations=$(
  grep -R -n -E '\.(set_document_selection|select_visible_block_range|set_document_text_selection|select_all_visible_blocks)\(' \
    --include='*.rs' crates/cditor-editor/src || true
)
if [ -n "$direct_document_selection_violations" ]; then
  echo 'error: Editor must route semantic document and block selection through Runtime dispatch:' >&2
  echo "$direct_document_selection_violations" >&2
  exit 1
fi

direct_runtime_selection_primitive_violations=$(
  grep -R -n -E \
    '\.(focus_block_at_offset|focus_table_cell_at_offset|set_focused_table_cell_text_selection|set_focused_table_cell_text_selection_position|focus_text_surface_at_offset|set_inline_color_for_range|replace_text_in_focused_range)\(' \
    --include='*.rs' crates/cditor-editor/src --exclude='test_support.rs' || true
)
if [ -n "$direct_runtime_selection_primitive_violations" ]; then
  echo 'error: Editor must use command or realtime ports instead of Runtime selection primitives:' >&2
  echo "$direct_runtime_selection_primitive_violations" >&2
  exit 1
fi

direct_whiteboard_mutation_violations=$(
  grep -R -n -E '\.update_whiteboard_scene_json\(' \
    --include='*.rs' crates/cditor-editor/src || true
)
if [ -n "$direct_whiteboard_mutation_violations" ]; then
  echo 'error: Editor must route whiteboard scene updates through Runtime dispatch:' >&2
  echo "$direct_whiteboard_mutation_violations" >&2
  exit 1
fi

direct_interaction_focus_violations=$(
  grep -n -E '\.focus_block\(' \
    crates/cditor-editor/src/app/interaction/gutter_drag.rs \
    crates/cditor-editor/src/app/interaction/image_resize.rs \
    crates/cditor-editor/src/app/interaction/table_resize.rs \
    crates/cditor-editor/src/app/interaction/table_reorder.rs || true
)
if [ -n "$direct_interaction_focus_violations" ]; then
  echo 'error: block interactions must route focus through Runtime dispatch:' >&2
  echo "$direct_interaction_focus_violations" >&2
  exit 1
fi

direct_table_cell_focus_violations=$(
  grep -n -E '\.(focus_table_cell|focus_table_cell_at_offset|blur_table_cell|move_focused_table_cell_(left|right|up|down|tab|to_text_position)|extend_focused_table_cell_selection_(left|right|to_offset))\(' \
    crates/cditor-editor/src/app/cditor_v2_view/table_actions.rs \
    crates/cditor-editor/src/app/input/actions.rs || true
)
if [ -n "$direct_table_cell_focus_violations" ]; then
  echo 'error: table cell focus and blur must route through Runtime dispatch:' >&2
  echo "$direct_table_cell_focus_violations" >&2
  exit 1
fi

direct_auxiliary_surface_focus_violations=$(
  {
    sed '/^#\[cfg(test)\]/,$d' crates/cditor-editor/src/app/cditor_v2_view/text_surface.rs
  } | grep -n -E '\.(focus_text_surface_at_offset|move_focused_text_surface_to_offset)\(' || true
)
if [ -n "$direct_auxiliary_surface_focus_violations" ]; then
  echo 'error: auxiliary text surface focus must route through Runtime dispatch:' >&2
  echo "$direct_auxiliary_surface_focus_violations" >&2
  exit 1
fi

direct_caret_navigation_violations=$(
  grep -n -E '\.(move_caret_(left|right|up|down|to_document_boundary)|move_focused_caret_(by_word|to_line_boundary|to_offset|to_text_position)|move_focused_text_surface_to_offset)\(' \
    crates/cditor-editor/src/app/input/keyboard.rs || true
)
if [ -n "$direct_caret_navigation_violations" ]; then
  echo 'error: caret navigation must route semantic fallback and Parley targets through Runtime dispatch:' >&2
  echo "$direct_caret_navigation_violations" >&2
  exit 1
fi

direct_history_router_violations=$(
  grep -R -n -E '\.execute_history_action\(' \
    --include='*.rs' crates/cditor-editor/src/app \
    --exclude='command_router.rs' \
    --exclude='persistence_bridge.rs' || true
)
if [ -n "$direct_history_router_violations" ]; then
  echo 'error: Editor history commands must enter through dispatch_command:' >&2
  echo "$direct_history_router_violations" >&2
  exit 1
fi

direct_external_undo_blob_violations=$(
  grep -R -n -E '\.(begin_external_undo_spill|complete_external_undo_spill|abort_external_undo_spill|drain_orphaned_external_undo_blobs|restore_orphaned_external_undo_blobs)\(' \
    --include='*.rs' crates/cditor-editor/src || true
)
if [ -n "$direct_external_undo_blob_violations" ]; then
  echo 'error: Editor external undo blob lifecycle must use cditor-session history ports:' >&2
  echo "$direct_external_undo_blob_violations" >&2
  exit 1
fi

direct_persistence_capture_violations=$(
  grep -R -n -E '\.(note_content_changed|structure_version|drain_pending_structure_transactions|loaded_payload_records_snapshot|block_attrs_snapshot|index_records_snapshot|page_layout_snapshot|mark_payload_versions_persisted|mark_layout_saved|restore_pending_structure_transactions)\(' \
    --include='*.rs' --exclude='test_support.rs' crates/cditor-editor/src || true
)
if [ -n "$direct_persistence_capture_violations" ]; then
  echo 'error: Editor save capture and completion must use cditor-session persistence ports:' >&2
  echo "$direct_persistence_capture_violations" >&2
  exit 1
fi

direct_ai_session_mutation_violations=$(
  grep -R -n -E '\.(apply_ai_session_request|begin_ai_request|begin_ai_request_with_presentation|apply_ai_stream_event|cancel_ai_request|reject_ai_preview|accept_ai_preview)\(' \
    --include='*.rs' crates/cditor-editor/src || true
)
if [ -n "$direct_ai_session_mutation_violations" ]; then
  echo 'error: Editor AI session lifecycle must use apply_ai_session_request:' >&2
  echo "$direct_ai_session_mutation_violations" >&2
  exit 1
fi

direct_layout_mutation_violations=$(
  grep -R -n -E \
    '\.(queue_measured_height|apply_measured_height|flush_pending_height_corrections|flush_pending_height_corrections_with_priority|sync_viewport_height|scroll_by_delta|apply_scroll_accumulator_frame|scroll_focused_block_into_view|scroll_to_block_with_alignment|begin_scrollbar_drag|drag_scrollbar_to_thumb_top|finish_scrollbar_drag|current_page_window_planned|set_window_memory_pressure|set_table_horizontal_scroll_offset_px)\(' \
    --include='*.rs' crates/cditor-editor/src || true
)
if [ -n "$direct_layout_mutation_violations" ]; then
  echo 'error: Editor layout mutations must use cditor-session layout/render ports:' >&2
  echo "$direct_layout_mutation_violations" >&2
  exit 1
fi

if grep -Eq '^[[:space:]]+pub[[:space:]]+document_id:' crates/cditor-runtime/src/document_runtime/state.rs; then
  echo 'error: DocumentRuntime identity must remain private and be read through document_id()' >&2
  exit 1
fi

direct_runtime_state_violations=$(
  grep -R -n -E \
    'runtime\.(document|layout|editing|selection|history|transactions|ai_session|next_ai_request_id)(\.|[[:space:]]|,|;)|runtime\.document_id([[:space:]]*[,;.)]|$)' \
    --include='*.rs' crates/cditor-editor/src crates/cditor-app/src || true
)
if [ -n "$direct_runtime_state_violations" ]; then
  echo 'error: Editor/App must use Runtime queries, projections, commands, or narrow realtime ports instead of child state fields:' >&2
  echo "$direct_runtime_state_violations" >&2
  exit 1
fi

printable_keydown_violations=$(
  grep -R -n -E 'InsertChar|InsertSpaceOrMarkdownShortcut' --include='*.rs' crates/cditor-editor/src || true
)
if [ -n "$printable_keydown_violations" ]; then
  echo 'error: printable text must enter through GPUI EntityInputHandler, not a keydown command:' >&2
  echo "$printable_keydown_violations" >&2
  exit 1
fi

if grep -Eq 'cditor-editor|cditor-runtime|(^|[[:space:]])gpui[[:space:]]*=|(^|[[:space:]])sqlx[[:space:]]*=' crates/cditor-storage/Cargo.toml; then
  echo 'error: storage contracts must not depend on editor, runtime, GPUI, or SQLx' >&2
  exit 1
fi

if grep -Eq 'cditor-storage|cditor-runtime|(^|[[:space:]])gpui[[:space:]]*=|(^|[[:space:]])sqlx[[:space:]]*=' crates/cditor-viewport/Cargo.toml; then
  echo 'error: viewport must remain a storage- and framework-independent algorithm crate' >&2
  exit 1
fi

if grep -Eq '^[[:space:]]*cditor-(runtime|viewport|storage|editor|api|ai|import-export)[[:space:]]*=|^[[:space:]]*(gpui|sqlx|reqwest|parley)[[:space:]]*=' crates/cditor-editor-protocol/Cargo.toml; then
  echo 'error: editor protocol may only depend on core and serialization support' >&2
  exit 1
fi

if grep -Eq '(^|[[:space:]])gpui[[:space:]]*=|(^|[[:space:]])sqlx[[:space:]]*=' crates/cditor-core/Cargo.toml; then
  echo 'error: core must remain independent from GPUI and SQLx' >&2
  exit 1
fi

oversized=$(
  find crates \
    -path 'components/cditor-whiteboard' -prune -o \
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
    -path './components/cditor-whiteboard' -prune -o \
    -name '.DS_Store' -print
)
if [ -n "$system_files" ]; then
  echo 'error: system metadata found outside the excluded whiteboard crate:' >&2
  echo "$system_files" >&2
  exit 1
fi

echo 'Structure checks passed.'
