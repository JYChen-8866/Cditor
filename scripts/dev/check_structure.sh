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

for removed_package in cditor-theme-types cditor-collaboration cditor-editor cditor-editor-core ding-board
do
  if find crates components apps -name Cargo.toml -exec grep -H -E "^name[[:space:]]*=[[:space:]]*\"$removed_package\"" {} + 2>/dev/null | grep -q .; then
    echo "error: removed package name must not return: $removed_package" >&2
    exit 1
  fi
done

for manifest in $(find crates components apps -name Cargo.toml -print)
do
  if sed -n '/^\[dependencies\]/,/^\[/p' "$manifest" | grep -q 'cditor-test-support'; then
    echo "error: production dependency must not reference cditor-test-support: $manifest" >&2
    exit 1
  fi
done

if [ ! -d crates/cditor-test-support/src/acceptance ] \
  || [ ! -f crates/cditor-test-support/src/storage.rs ] \
  || [ ! -f crates/cditor-test-support/benches/frame_baseline.rs ]; then
  echo 'error: Test Support must retain acceptance, fixture/contract, and benchmark ownership' >&2
  exit 1
fi

if grep -R -n -E 'cditor-app|cditor_api|cditor-api' \
  --include='*.yml' --include='*.yaml' --include='*.sh' --exclude='check_structure.sh' \
  .github scripts/packaging scripts/dev | grep -q . \
  || ! grep -q 'target/${target_triple}/release/cditor-desktop' scripts/packaging/package_macos.sh \
  || ! grep -q 'cargo build --locked --release -p cditor-desktop' .github/workflows/desktop-builds.yml; then
  echo 'error: Desktop workflows and packaging must target cditor-desktop exclusively' >&2
  exit 1
fi

if grep -Eq 'cditor-storage-postgres|cditor-storage-sqlite|(^|[[:space:]])sqlx[[:space:]]*=|(^|[[:space:]])gpui[[:space:]]*=' crates/cditor-runtime/Cargo.toml; then
  echo 'error: runtime must not depend on PostgreSQL, SQLx, or GPUI' >&2
  exit 1
fi

if grep -Eq '^[[:space:]]*cditor-import-export[[:space:]]*=' crates/cditor-runtime/Cargo.toml \
  || grep -R -n -E 'cditor_import_export|decode_metadata|parse_markdown_document|looks_like_markdown_paste|parse_(csv|tsv)_rows' \
    --include='*.rs' crates/cditor-runtime/src | grep -q .; then
  echo 'error: Runtime must apply typed import plans and must not depend on external format parsers' >&2
  exit 1
fi

if [ ! -f crates/cditor-core/src/import_plan.rs ] \
  || [ ! -f crates/cditor-import-export/src/import_plan.rs ] \
  || [ ! -f crates/cditor-session/src/import_port.rs ] \
  || ! grep -q 'pub fn apply_import_plan' crates/cditor-runtime/src/document_runtime/import_plan.rs \
  || ! grep -q 'plan_clipboard_import' crates/cditor-session/src/import_port.rs; then
  echo 'error: typed ImportPlan must flow from Import/Export through Session into Runtime' >&2
  exit 1
fi

for required_import_test in \
  planning_rejects_input_before_dispatch_when_limits_are_exceeded \
  malformed_metadata_is_reported_and_plain_text_still_applies \
  readonly_and_stale_clipboard_targets_are_rejected_before_apply \
  session_plans_markdown_and_applies_it_as_one_undo_unit \
  session_dispatches_preparsed_tsv_to_the_focused_table
do
  if ! grep -R -q "$required_import_test" \
    crates/cditor-import-export/src crates/cditor-session/src; then
    echo "error: required typed import test is missing: $required_import_test" >&2
    exit 1
  fi
done

if grep -Eq 'cditor-storage-postgres|cditor-storage-sqlite|(^|[[:space:]])sqlx[[:space:]]*=|(^|[[:space:]])gpui[[:space:]]*=' crates/cditor-session/Cargo.toml; then
  echo 'error: session must depend on ports, not GPUI or concrete storage adapters' >&2
  exit 1
fi

if grep -Eq '^[[:space:]]*tokio[[:space:]]*=' crates/cditor-storage/Cargo.toml \
  || grep -R -n -E 'StorageSession|block_on_storage|tokio::' \
    --include='*.rs' crates/cditor-storage/src | grep -q .; then
  echo 'error: storage must remain a runtime-free port/DTO/error crate' >&2
  exit 1
fi

if ! grep -q -E '^pub struct DocumentPersistence' \
  crates/cditor-session/src/document_persistence.rs \
  || ! grep -q -E '^pub struct SessionIoExecutor' \
    crates/cditor-session/src/io_executor.rs; then
  echo 'error: Session must own document persistence policy and the async host bridge' >&2
  exit 1
fi

if grep -R -n -E 'StorageSession|block_on_storage' --include='*.rs' \
  crates/cditor-session/src apps/cditor-desktop/src crates/cditor-editor-gpui/src \
  | grep -q .; then
  echo 'error: removed storage session/runtime compatibility names must not return' >&2
  exit 1
fi

for adapter_contract in \
  crates/cditor-storage-sqlite/tests/shared_contract.rs \
  crates/cditor-storage-postgres/src/postgres_integration.rs
do
  if ! grep -q 'run_document_storage_contract' "$adapter_contract"; then
    echo "error: storage adapter must run the shared contract suite: $adapter_contract" >&2
    exit 1
  fi
done

if grep -R -n -E '^([[:space:]]*)pub[[:space:]]+(struct|enum|type|fn|async fn).*(SqliteRow|SqlitePool|SqliteConnection|SqliteWriter)' \
  --include='*.rs' crates/cditor-storage-sqlite/src \
  | grep -v 'test_pool' | grep -q . \
  || grep -q -E '^[[:space:]]*pub[[:space:]]+fn[[:space:]]+pool\(' \
    crates/cditor-storage-sqlite/src/storage.rs; then
  echo 'error: SQLite rows, codecs, connections, and writer internals must stay crate-private' >&2
  exit 1
fi

if sed -n '/^\[dependencies\]/,/^\[/p' apps/cditor-desktop/Cargo.toml \
  | grep -q 'features[[:space:]]*=[[:space:]]*\[[^]]*"test-support"'; then
  echo 'error: Desktop production dependencies must not enable SQLite test support' >&2
  exit 1
fi

if grep -Eq '^pub mod (adapter|demo_seed|queue|runtime|stores|types);|^pub use (adapter|demo_seed|queue|runtime|stores|types)::' \
  crates/cditor-storage-postgres/src/lib.rs \
  || grep -R -n -E '^pub (struct|enum|type) Db' \
    --include='*.rs' crates/cditor-storage-postgres/src/types | grep -q . \
  || [ -e crates/cditor-storage-postgres/src/demo_seed.rs ] \
  || [ -e crates/cditor-storage-postgres/src/runtime.rs ]; then
  echo 'error: PostgreSQL rows, codecs, repositories, runtime, and demo policy must stay out of its public API' >&2
  exit 1
fi

if grep -R -n -E 'DocumentRow|PostgresDocument(Store|Storage)|PostgresPayloadStore|pg_document_id_from_runtime' \
  --include='*.rs' apps/cditor-desktop/src | grep -q .; then
  echo 'error: Desktop must compose the PostgreSQL provider, not adapter row/repository internals' >&2
  exit 1
fi

if grep -Eq '(^|[[:space:]])(reqwest|dotenvy|toml|serde_json)[[:space:]]*=|^\[features\]|openai' \
  crates/cditor-ai/Cargo.toml \
  || grep -R -n -E 'reqwest::|dotenvy::|std::env|std::fs|OpenAi' \
    --include='*.rs' crates/cditor-ai/src | grep -q .; then
  echo 'error: AI contract crate must remain provider/mock-only and environment/network-free' >&2
  exit 1
fi

if [ ! -f crates/cditor-ai-openai/src/openai.rs ] \
  || ! grep -q 'impl AiProvider for OpenAiCompatibleProvider' \
    crates/cditor-ai-openai/src/openai.rs; then
  echo 'error: OpenAI-compatible network adapter must remain in cditor-ai-openai' >&2
  exit 1
fi

if ! grep -q 'OpenAiCompatibleProvider::from_env' apps/cditor-desktop/src/main.rs; then
  echo 'error: Desktop composition root must own OpenAI-compatible provider selection' >&2
  exit 1
fi

if grep -Eq 'cditor-ai-openai|reqwest' crates/cditor-runtime/Cargo.toml \
  crates/cditor-session/Cargo.toml crates/cditor-editor-gpui/Cargo.toml; then
  echo 'error: Runtime, Session, and Editor must depend on the AI contract, not OpenAI/HTTP' >&2
  exit 1
fi

for required_security_test in \
  envelope_rejects_oversize_malformed_unknown_schema_and_bad_checksum \
  provider_dispatch_redacts_sensitive_document_context_before_leaving_session \
  matching_and_stale_stream_events_preserve_runtime_request_identity \
  cancellation_between_formal_migrations_automatically_restores_backup
do
  if ! grep -R -q "$required_security_test" \
    crates/cditor-core/src crates/cditor-import-export/src crates/cditor-session/src \
    crates/cditor-storage-sqlite/tests; then
    echo "error: required import/AI/migration security test is missing: $required_security_test" >&2
    exit 1
  fi
done

if [ ! -f crates/cditor-session/tests/session_integration.rs ]; then
  echo 'error: cditor-session must retain its headless integration test target' >&2
  exit 1
fi

if grep -R -n -E 'Arc[[:space:]]*<[[:space:]]*Mutex[[:space:]]*<[[:space:]]*DocumentRuntime' \
  --include='*.rs' crates/cditor-session/src | grep -q .; then
  echo 'error: session must keep one serial Runtime owner instead of Arc<Mutex<DocumentRuntime>>' >&2
  exit 1
fi

if grep -R -n -E 'StoragePersistenceState' --include='*.rs' crates/cditor-editor-gpui/src crates/cditor-session/src | grep -q .; then
  echo 'error: legacy Editor-owned StoragePersistenceState must not return' >&2
  exit 1
fi

if grep -R -n -E 'ready_runtime(_ref)?|\.storage_persistence|storage_persistence[[:space:]]*:' \
  --include='*.rs' crates/cditor-editor-gpui/src | grep -q .; then
  echo 'error: GPUI Editor must access the document and persistence policy through EditorSessionHandle' >&2
  exit 1
fi

legacy_editor_task_state_violations=$(
  grep -R -n -E \
    'PayloadWindowLoadScheduler|payload_window_load_scheduler|undo_spill_in_flight|history_hydration_in_flight|selection_materialization_in_flight|undo_cleanup_in_flight' \
    --include='*.rs' crates/cditor-editor-gpui/src || true
)
if [ -n "$legacy_editor_task_state_violations" ]; then
  echo 'error: GPUI Editor background task policy must remain owned by cditor-session:' >&2
  echo "$legacy_editor_task_state_violations" >&2
  exit 1
fi

if grep -Eq '^[[:space:]]*(cditor-storage|reqwest|tokio)[[:space:]]*=|cditor-ai[^#]*features[[:space:]]*=[[:space:]]*\[[^]]*"openai"' \
  crates/cditor-editor-gpui/Cargo.toml; then
  echo 'error: GPUI Editor must not depend on Storage, Tokio, reqwest, or a concrete AI adapter' >&2
  exit 1
fi

editor_infrastructure_violations=$(
  grep -R -n -E 'cditor_storage|reqwest::|tokio::|OpenAiCompatibleProvider|block_on_storage' \
    --include='*.rs' crates/cditor-editor-gpui/src || true
)
if [ -n "$editor_infrastructure_violations" ]; then
  echo 'error: GPUI Editor source crossed the Session, AI-provider, or host-network boundary:' >&2
  echo "$editor_infrastructure_violations" >&2
  exit 1
fi

if ! grep -q -E 'tasks:[[:space:]]+crate::task_port::SessionTaskCoordinator' \
  crates/cditor-session/src/session.rs; then
  echo 'error: EditorSession must retain the headless background task coordinator' >&2
  exit 1
fi

if [ -d crates/cditor-editor-gpui/src/app/interaction ]; then
  echo 'error: interaction adapters must live in the crate-root interaction module' >&2
  exit 1
fi

if [ -d crates/cditor-editor-gpui/src/app/input ] \
  || grep -R -n -E 'crate::app::input|crate::app::input_trace' \
    --include='*.rs' crates/cditor-editor-gpui/src | grep -q .; then
  echo 'error: GPUI input adapters must live in the single crate-root input module' >&2
  exit 1
fi

if [ -e crates/cditor-editor-gpui/src/app/cditor_v2_view.rs ] \
  || [ -d crates/cditor-editor-gpui/src/app/cditor_v2_view ] \
  || [ -e crates/cditor-editor-gpui/src/app/state.rs ] \
  || [ -e crates/cditor-editor-gpui/src/app/lifecycle.rs ] \
  || [ -e crates/cditor-editor-gpui/src/app/render.rs ]; then
  echo 'error: View composition, state, lifecycle, and render must live in editor_view/' >&2
  exit 1
fi

if grep -R -n -E 'crate::app::interaction|pub\(in crate::app\)' \
  --include='*.rs' crates/cditor-editor-gpui/src/interaction | grep -q .; then
  echo 'error: root interaction must not depend on the former app/interaction module boundary' >&2
  exit 1
fi

if grep -R -n -E '\.plan_payload_window_load(_if_needed)?\(' \
  --include='*.rs' crates/cditor-editor-gpui/src | grep -q .; then
  echo 'error: GPUI Editor must schedule payload hydration through the Session task coordinator' >&2
  exit 1
fi

if sed -n '/^pub struct CditorV2View {/,/^}/p' \
  crates/cditor-editor-gpui/src/editor_view/mod.rs \
  | grep -E '^[[:space:]]+pub.*(requested_readonly|readonly_reason|readonly|dirty|save_status)[[:space:]]*:' \
  | grep -q .; then
  echo 'error: readonly, dirty, and save status must remain grouped in EditorStatusUiState' >&2
  exit 1
fi

if sed -n '/^pub struct CditorV2View {/,/^}/p' \
  crates/cditor-editor-gpui/src/editor_view/mod.rs \
  | grep -E '^[[:space:]]+pub.*(last_wheel_delta_y|scroll_accumulator|editor_viewport_handle|table_scroll_state|scrollbar_drag|text_drag_selection|text_drag_auto_scroll_scheduled|block_drag_selection|table_interaction_mode|hovered_block_id|action_block_id|gutter_block_drag|gutter_drag_auto_scroll_scheduled|image_resize_drag|table_resize_drag|table_reorder_drag|table_hscroll_drag|projected_block_rects)[[:space:]]*:' \
  | grep -q .; then
  echo 'error: scroll, hit-test, and drag lifecycle fields must remain grouped in InteractionUiState' >&2
  exit 1
fi

if sed -n '/^pub struct CditorV2View {/,/^}/p' \
  crates/cditor-editor-gpui/src/editor_view/mod.rs \
  | grep -E '^[[:space:]]+pub.*(ai_provider|ai_enabled|ai_prompt|ai_preview_scroll_handle|whiteboard_editor|code_language_edit|code_theme_menu_block_id|code_highlight_theme|slash_menu|toast|table_menu_ui|gutter_toolbar_block_id|selection_toolbar_delay|block_transform_menu_open|color_menu_open|color_menu_hover_generation|color_menu_scroll_handle|last_color_action)[[:space:]]*:' \
  | grep -q .; then
  echo 'error: feature configuration and transient overlays must remain grouped in FeatureUiState and OverlayUiState' >&2
  exit 1
fi

if sed -n '/^pub struct CditorV2View {/,/^}/p' \
  crates/cditor-editor-gpui/src/editor_view/mod.rs \
  | grep -E '^[[:space:]]+pub.*(text_layouts|table_cell_layouts|text_surface_layouts|code_highlights|mermaid_renders|mermaid_source_blocks|whiteboard_thumbnails|show_debug)[[:space:]]*:' \
  | grep -q .; then
  echo 'error: presentation caches and diagnostics must remain grouped in RenderCacheState and EditorDiagnosticsState' >&2
  exit 1
fi

if sed -n '/^pub struct CditorV2View {/,/^}/p' \
  crates/cditor-editor-gpui/src/editor_view/mod.rs \
  | grep -E '^[[:space:]]+pub.*(code_language_focus|ai_prompt_focus|sdk_focus_observers_registered|last_emitted_selection|platform_input_target|platform_input_session_identity|platform_input_layout_identity|preferred_text_navigation_x)[[:space:]]*:' \
  | grep -q .; then
  echo 'error: focus and platform input lifecycle fields must remain grouped UI state' >&2
  exit 1
fi

if sed -n '/^pub struct CditorV2View {/,/^}/p' \
  crates/cditor-editor-gpui/src/editor_view/mod.rs \
  | grep -E '^[[:space:]]+pub.*[[:space:]]+[a-zA-Z0-9_]+[[:space:]]*:' \
  | grep -v -E '^[[:space:]]+pub.*[[:space:]]+(state|focus|input|features|overlay|diagnostics|status|interaction|cache|scheduling)[[:space:]]*:' \
  | grep -q .; then
  echo 'error: CditorV2View top-level fields must remain explicit lifecycle-owned UI substates' >&2
  exit 1
fi

for field in state focus input features overlay diagnostics status interaction cache scheduling; do
  if ! sed -n '/^pub struct CditorV2View {/,/^}/p' \
    crates/cditor-editor-gpui/src/editor_view/mod.rs \
    | grep -E "^[[:space:]]+pub.*[[:space:]]+${field}[[:space:]]*:" \
    | grep -q .; then
    echo "error: CditorV2View is missing required lifecycle-owned state field: ${field}" >&2
    exit 1
  fi
done

if grep -R -n -E 'Ready[[:space:]]*\([[:space:]]*(Box[[:space:]]*<[[:space:]]*)?DocumentRuntime|Ready[[:space:]]*\([[:space:]]*Box::new' \
  --include='*.rs' crates/cditor-editor-gpui/src | grep -q .; then
  echo 'error: CditorViewState::Ready must contain EditorSessionHandle, not DocumentRuntime' >&2
  exit 1
fi

if grep -n -E 'StorageSession' \
  crates/cditor-editor-gpui/src/editor_view/lifecycle.rs \
  crates/cditor-editor-gpui/src/editor_view/mod.rs \
  crates/cditor-editor-gpui/src/editor_view/render.rs \
  crates/cditor-editor-gpui/src/app/sdk.rs | grep -q .; then
  echo 'error: GPUI View construction and state must not accept or retain StorageSession' >&2
  exit 1
fi

if grep -n -E 'CditorRuntimeLoadResult|load_runtime_from_|pub[[:space:]]+(runtime|storage_session)[[:space:]]*:' \
  apps/cditor-desktop/src/storage_host.rs apps/cditor-desktop/src/wiring.rs | grep -q .; then
  echo 'error: App cold start must return a prepared EditorSession, not Runtime or StorageSession ownership' >&2
  exit 1
fi

if grep -n -E 'apply_(loaded|recovered)_runtime' \
  crates/cditor-editor-gpui/src/editor_view/lifecycle.rs apps/cditor-desktop/src/wiring.rs | grep -q .; then
  echo 'error: GPUI Editor loading must adopt EditorSessionHandle instead of DocumentRuntime' >&2
  exit 1
fi

if grep -Eq 'cditor-storage-postgres|cditor-storage-sqlite|(^|[[:space:]])cditor-runtime[[:space:]]*=|(^|[[:space:]])cditor-editor[[:space:]]*=|(^|[[:space:]])cditor-whiteboard[[:space:]]*=|(^|[[:space:]])(sqlx|gpui)[[:space:]]*=' crates/cditor-sdk/Cargo.toml; then
  echo 'error: SDK contracts must remain framework-free and independent from concrete adapters' >&2
  exit 1
fi

if [ ! -f crates/cditor-sdk/tests/public_api.rs ] \
  || ! grep -q 'framework_free_sdk_surface_compiles_for_external_consumers' \
    crates/cditor-sdk/tests/public_api.rs \
  || find crates -maxdepth 1 -type d \( -name cditor-api -o -name cditor-app \) | grep -q . \
  || [ ! -f apps/cditor-desktop/Cargo.toml ]; then
  echo 'error: SDK compile contract and Desktop target layout must remain in the Phase 8 target state' >&2
  exit 1
fi

if [ -e crates/cditor-sdk/src/builder.rs ] \
  || grep -R -n -E 'CditorBuilder|pub mod builder' --include='*.rs' crates/cditor-sdk/src | grep -q .; then
  echo 'error: removed SDK builder compatibility alias must not return' >&2
  exit 1
fi

for removed_import_facade in clipboard media_resource table_clipboard
do
  if [ -e "crates/cditor-import-export/src/$removed_import_facade.rs" ]; then
    echo "error: Core-owned import facade must not return: $removed_import_facade" >&2
    exit 1
  fi
done

if [ -e scripts/dev/run_editor.sh ]; then
  echo 'error: ambiguous compatibility launch script must not return' >&2
  exit 1
fi

if [ -e doc/architecture-v2.md ]; then
  echo 'error: superseded architecture-v2 migration plan must remain archived' >&2
  exit 1
fi

if grep -q -E '^pub use cditor_(core|editor_gpui|runtime|sdk|storage)' \
  apps/cditor-desktop/src/lib.rs; then
  echo 'error: Desktop must not act as a compatibility re-export facade' >&2
  exit 1
fi

api_backend_violations=$(
  grep -R -n -E 'cditor_storage_(postgres|sqlite)|(^|[^[:alnum:]_])sqlx([^[:alnum:]_]|$)' \
    --include='*.rs' crates/cditor-sdk/src || true
)
if [ -n "$api_backend_violations" ]; then
  echo 'error: SDK source crossed the concrete storage boundary:' >&2
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

core_presentation_violations=$(
  grep -R -n -E \
    'SlashMenuMetadata|TransformMenuMetadata|BlockMenuMetadata|slash_descriptors|transform_descriptors|menu[[:space:]]*:[[:space:]]*BlockMenuMetadata|\.menu\.(slash|transform|create_from_text)' \
    --include='*.rs' crates/cditor-core/src || true
)
if [ -n "$core_presentation_violations" ]; then
  echo 'error: Core must not own block menu labels, icons, or ordering metadata:' >&2
  echo "$core_presentation_violations" >&2
  exit 1
fi

legacy_presentation_adapter_violations=$(
  grep -R -n -E 'slash_block_presentations|transform_block_presentations|transform_presentation_(for_kind|by_tag)' \
    --include='*.rs' crates/cditor-editor-gpui/src || true
)
if [ -n "$legacy_presentation_adapter_violations" ]; then
  echo 'error: Editor block menus must consume BlockPresentationRegistry directly:' >&2
  echo "$legacy_presentation_adapter_violations" >&2
  exit 1
fi

if grep -Eq '(^|[[:space:]])gpui[[:space:]]*=' crates/cditor-text/Cargo.toml; then
  echo 'error: cditor-text must not depend on GPUI' >&2
  exit 1
fi

text_gpui_violations=$(
  grep -R -n -E '^[[:space:]]*((pub|pub\([^)]*\))[[:space:]]+)?use[[:space:]]+gpui(::|[[:space:]]|;)|^[[:space:]]*extern[[:space:]]+crate[[:space:]]+gpui' \
    --include='*.rs' crates/cditor-text/src || true
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

editor_parley_name_violations=$(
  grep -R -n -i 'parley' --include='*.rs' crates/cditor-editor-gpui/src || true
)
text_public_parley_violations=$(
  grep -R -n -E '^[[:space:]]*pub[[:space:]][^/].*Parley|^[[:space:]]*pub[[:space:]]+use.*Parley' \
    --include='*.rs' crates/cditor-text/src || true
)
if [ -n "$editor_parley_name_violations$text_public_parley_violations" ]; then
  echo 'error: Text public API and Editor call sites must not expose the Parley engine:' >&2
  [ -z "$editor_parley_name_violations" ] || echo "$editor_parley_name_violations" >&2
  [ -z "$text_public_parley_violations" ] || echo "$text_public_parley_violations" >&2
  exit 1
fi

if [ -e crates/cditor-editor-gpui/src/text/parley_adapter ]; then
  echo 'error: legacy Parley-named GPUI adapter directory must not return' >&2
  exit 1
fi

for legacy_geometry_file in \
  crates/cditor-editor-gpui/src/text/layout.rs \
  crates/cditor-editor-gpui/src/text/fallback_render.rs \
  crates/cditor-editor-gpui/src/overlays/caret_overlay.rs
do
  if [ -e "$legacy_geometry_file" ]; then
    echo "error: legacy App text geometry file must stay removed: $legacy_geometry_file" >&2
    exit 1
  fi
done

legacy_app_geometry_violations=$(
  grep -R -n -E \
    '(^|[^[:alnum:]_])(GpuiWrappedLine|CaretGeometryCache|VisualLineLayout|RichTextLayoutCache|CachedRichTextLayout)([^[:alnum:]_]|$)' \
    --include='*.rs' crates/cditor-editor-gpui/src || true
)
if [ -n "$legacy_app_geometry_violations" ]; then
  echo 'error: App text geometry must come from the cditor-text Parley snapshot:' >&2
  echo "$legacy_app_geometry_violations" >&2
  exit 1
fi

editor_range_geometry_violations=$(
  grep -R -n -E '\.selection_rects\(' --include='*.rs' crates/cditor-editor-gpui/src || true
)
if [ -n "$editor_range_geometry_violations" ]; then
  echo 'error: Editor range geometry must use the normalized cditor-text snapshot API:' >&2
  echo "$editor_range_geometry_violations" >&2
  exit 1
fi

duplicate_editor_text_cache_violations=$(
  grep -R -n -E 'struct PlatformLayoutCache|snapshot\.estimated_bytes\(' \
    --include='*.rs' crates/cditor-editor-gpui/src || true
)
if [ -n "$duplicate_editor_text_cache_violations" ]; then
  echo 'error: Text snapshot memory and eviction policy belong exclusively to cditor-text:' >&2
  echo "$duplicate_editor_text_cache_violations" >&2
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
    --include='*.rs' crates/cditor-editor-gpui/src || true
)
if [ -n "$migrated_runtime_mutation_violations" ]; then
  echo 'error: Editor must route migrated document mutations through Runtime dispatch:' >&2
  echo "$migrated_runtime_mutation_violations" >&2
  exit 1
fi

legacy_platform_input_mutation_violations=$(
  grep -R -n -E \
    '\.(replace_text_from_platform|begin_or_update_composition|begin_or_update_composition_with_selection|commit_composition|cancel_composition|commit_composition_before_external_focus|delete_active_selection)\(' \
    --include='*.rs' crates/cditor-editor-gpui/src || true
)
if [ -n "$legacy_platform_input_mutation_violations" ]; then
  echo 'error: GPUI platform input must use the versioned Runtime realtime port:' >&2
  echo "$legacy_platform_input_mutation_violations" >&2
  exit 1
fi

direct_document_selection_violations=$(
  grep -R -n -E '\.(set_document_selection|select_visible_block_range|set_document_text_selection|select_all_visible_blocks)\(' \
    --include='*.rs' crates/cditor-editor-gpui/src || true
)
if [ -n "$direct_document_selection_violations" ]; then
  echo 'error: Editor must route semantic document and block selection through Runtime dispatch:' >&2
  echo "$direct_document_selection_violations" >&2
  exit 1
fi

direct_runtime_selection_primitive_violations=$(
  grep -R -n -E \
    '\.(focus_block_at_offset|focus_table_cell_at_offset|set_focused_table_cell_text_selection|set_focused_table_cell_text_selection_position|focus_text_surface_at_offset|set_inline_color_for_range|replace_text_in_focused_range)\(' \
    --include='*.rs' crates/cditor-editor-gpui/src --exclude='test_support.rs' || true
)
if [ -n "$direct_runtime_selection_primitive_violations" ]; then
  echo 'error: Editor must use command or realtime ports instead of Runtime selection primitives:' >&2
  echo "$direct_runtime_selection_primitive_violations" >&2
  exit 1
fi

direct_whiteboard_mutation_violations=$(
  grep -R -n -E '\.update_whiteboard_scene_json\(' \
    --include='*.rs' crates/cditor-editor-gpui/src || true
)
if [ -n "$direct_whiteboard_mutation_violations" ]; then
  echo 'error: Editor must route whiteboard scene updates through Runtime dispatch:' >&2
  echo "$direct_whiteboard_mutation_violations" >&2
  exit 1
fi

direct_interaction_focus_violations=$(
  grep -n -E '\.focus_block\(' \
    crates/cditor-editor-gpui/src/interaction/gutter_drag.rs \
    crates/cditor-editor-gpui/src/interaction/image_resize.rs \
    crates/cditor-editor-gpui/src/interaction/table_resize.rs \
    crates/cditor-editor-gpui/src/interaction/table_reorder.rs || true
)
if [ -n "$direct_interaction_focus_violations" ]; then
  echo 'error: block interactions must route focus through Runtime dispatch:' >&2
  echo "$direct_interaction_focus_violations" >&2
  exit 1
fi

direct_table_cell_focus_violations=$(
  grep -n -E '\.(focus_table_cell|focus_table_cell_at_offset|blur_table_cell|move_focused_table_cell_(left|right|up|down|tab|to_text_position)|extend_focused_table_cell_selection_(left|right|to_offset))\(' \
    crates/cditor-editor-gpui/src/features/table/actions.rs \
    crates/cditor-editor-gpui/src/input/routing.rs || true
)
if [ -n "$direct_table_cell_focus_violations" ]; then
  echo 'error: table cell focus and blur must route through Runtime dispatch:' >&2
  echo "$direct_table_cell_focus_violations" >&2
  exit 1
fi

direct_auxiliary_surface_focus_violations=$(
  {
    sed '/^#\[cfg(test)\]/,$d' crates/cditor-editor-gpui/src/surfaces/text.rs
  } | grep -n -E '\.(focus_text_surface_at_offset|move_focused_text_surface_to_offset)\(' || true
)
if [ -n "$direct_auxiliary_surface_focus_violations" ]; then
  echo 'error: auxiliary text surface focus must route through Runtime dispatch:' >&2
  echo "$direct_auxiliary_surface_focus_violations" >&2
  exit 1
fi

if [ ! -f crates/cditor-editor-gpui/src/surfaces/text.rs ] \
  || [ ! -f crates/cditor-editor-gpui/src/surfaces/table_cell.rs ] \
  || [ ! -f crates/cditor-editor-gpui/src/surfaces/caption.rs ] \
  || [ ! -f crates/cditor-editor-gpui/src/surfaces/collection_title.rs ] \
  || [ -e crates/cditor-editor-gpui/src/app/text_hit.rs ] \
  || [ -e crates/cditor-editor-gpui/src/editor_view/text_surface.rs ]; then
  echo 'error: editable text surface adapters must live in surfaces/' >&2
  exit 1
fi

surface_adapter_location_violations=$(
  grep -R -n -E 'fn (current_text_surface_layout_cache|current_table_cell_layout_cache|text_position_for_surface_at_position|text_position_for_table_cell_at_position|text_surface_render_state)' \
    --include='*.rs' crates/cditor-editor-gpui/src/app crates/cditor-editor-gpui/src/editor_view || true
)
if [ -n "$surface_adapter_location_violations" ]; then
  echo 'error: surface layout identity, hit-test, and render projection adapters must live in surfaces/:' >&2
  echo "$surface_adapter_location_violations" >&2
  exit 1
fi

if [ ! -f crates/cditor-editor-gpui/src/overlays/mod.rs ] \
  || [ -d crates/cditor-editor-gpui/src/overlay ] \
  || [ -e crates/cditor-editor-gpui/src/overlays/command_menu.rs ] \
  || grep -R -n -E 'crate::overlay([:;]|$)|pub mod overlays' \
    --include='*.rs' crates/cditor-editor-gpui/src | grep -q .; then
  echo 'error: overlay controllers and renderers must use the crate-private overlays/ boundary' >&2
  exit 1
fi

if [ ! -f crates/cditor-editor-gpui/src/cache/platform_layout.rs ] \
  || [ ! -f crates/cditor-editor-gpui/src/cache/state.rs ] \
  || [ ! -f crates/cditor-editor-gpui/src/presentation/block_registry.rs ] \
  || [ ! -f crates/cditor-editor-gpui/src/presentation/rich_text.rs ] \
  || [ -e crates/cditor-editor-gpui/src/app/platform_layout_cache.rs ] \
  || [ -e crates/cditor-editor-gpui/src/rich_text.rs ]; then
  echo 'error: render cache and UI presentation adapters must live in cache/ and presentation/' >&2
  exit 1
fi

editor_public_module_violations=$(
  grep -n -E '^pub mod ' crates/cditor-editor-gpui/src/lib.rs || true
)
if [ -n "$editor_public_module_violations" ]; then
  echo 'error: cditor-editor-gpui must expose explicit root API items, not compatibility modules:' >&2
  echo "$editor_public_module_violations" >&2
  exit 1
fi

editor_composition_root_violations=$(
  grep -n -E '^impl CditorV2View' crates/cditor-editor-gpui/src/editor_view/mod.rs || true
)
if [ -n "$editor_composition_root_violations" ]; then
  echo 'error: editor_view/mod.rs must remain a composition root without feature or interaction behavior:' >&2
  echo "$editor_composition_root_violations" >&2
  exit 1
fi

for editor_responsibility_file in \
  crates/cditor-editor-gpui/src/cache/layout_update.rs \
  crates/cditor-editor-gpui/src/features/code/actions.rs \
  crates/cditor-editor-gpui/src/features/mermaid/actions.rs \
  crates/cditor-editor-gpui/src/input/mouse/block.rs \
  crates/cditor-editor-gpui/src/interaction/gutter_action.rs
do
  if [ ! -f "$editor_responsibility_file" ]; then
    echo "error: GPUI Editor behavior must retain its responsibility module: $editor_responsibility_file" >&2
    exit 1
  fi
done

for legacy_editor_path in \
  crates/cditor-editor-gpui/src/block/code_toolbar \
  crates/cditor-editor-gpui/src/native_menu \
  crates/cditor-editor-gpui/src/text/layout_adapter/exact_raster/fixtures
do
  if [ -e "$legacy_editor_path" ]; then
    echo "error: removed GPUI Editor path must not return: $legacy_editor_path" >&2
    exit 1
  fi
done

presentation_registry_violations=$(
  grep -R -n -E 'builtin_block_registry|SlashMenuMetadata|TransformMenuMetadata' \
    --include='*.rs' crates/cditor-editor-gpui/src \
    --exclude='block_registry.rs' || true
)
if [ -n "$presentation_registry_violations" ]; then
  echo 'error: GPUI consumers must access Core menu metadata through presentation/block_registry.rs:' >&2
  echo "$presentation_registry_violations" >&2
  exit 1
fi

if [ ! -f crates/cditor-editor-gpui/src/features/text/mod.rs ] \
  || [ ! -f crates/cditor-editor-gpui/src/features/code/mod.rs ] \
  || [ ! -f crates/cditor-editor-gpui/src/features/table/mod.rs ] \
  || [ ! -f crates/cditor-editor-gpui/src/features/media/mod.rs ] \
  || [ ! -f crates/cditor-editor-gpui/src/features/mermaid/mod.rs ] \
  || [ ! -f crates/cditor-editor-gpui/src/features/whiteboard/mod.rs ] \
  || [ -d crates/cditor-editor-gpui/src/block/code ] \
  || [ -d crates/cditor-editor-gpui/src/block/table ] \
  || [ -d crates/cditor-editor-gpui/src/block/mermaid ] \
  || [ -d crates/cditor-editor-gpui/src/block/whiteboard ] \
  || [ -e crates/cditor-editor-gpui/src/block/media.rs ] \
  || [ -e crates/cditor-editor-gpui/src/block/collection.rs ] \
  || [ -e crates/cditor-editor-gpui/src/block/heading.rs ] \
  || [ -e crates/cditor-editor-gpui/src/block/list.rs ] \
  || [ -e crates/cditor-editor-gpui/src/block/paragraph.rs ] \
  || [ -e crates/cditor-editor-gpui/src/block/quote.rs ] \
  || [ -e crates/cditor-editor-gpui/src/editor_view/code_language.rs ] \
  || [ -e crates/cditor-editor-gpui/src/editor_view/code_theme.rs ] \
  || [ -e crates/cditor-editor-gpui/src/editor_view/table_actions.rs ] \
  || [ -e crates/cditor-editor-gpui/src/editor_view/whiteboard.rs ]; then
  echo 'error: feature renderers and command adapters must live in features/' >&2
  exit 1
fi

direct_caret_navigation_violations=$(
  grep -n -E '\.(move_caret_(left|right|up|down|to_document_boundary)|move_focused_caret_(by_word|to_line_boundary|to_offset|to_text_position)|move_focused_text_surface_to_offset)\(' \
    crates/cditor-editor-gpui/src/input/keyboard.rs || true
)
if [ -n "$direct_caret_navigation_violations" ]; then
  echo 'error: caret navigation must route semantic fallback and Parley targets through Runtime dispatch:' >&2
  echo "$direct_caret_navigation_violations" >&2
  exit 1
fi

direct_history_router_violations=$(
  grep -R -n -E '\.execute_history_action\(' \
    --include='*.rs' crates/cditor-editor-gpui/src/app \
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
    --include='*.rs' crates/cditor-editor-gpui/src || true
)
if [ -n "$direct_external_undo_blob_violations" ]; then
  echo 'error: Editor external undo blob lifecycle must use cditor-session history ports:' >&2
  echo "$direct_external_undo_blob_violations" >&2
  exit 1
fi

direct_persistence_capture_violations=$(
  grep -R -n -E '\.(note_content_changed|structure_version|drain_pending_structure_transactions|loaded_payload_records_snapshot|block_attrs_snapshot|index_records_snapshot|page_layout_snapshot|mark_payload_versions_persisted|mark_layout_saved|restore_pending_structure_transactions)\(' \
    --include='*.rs' --exclude='test_support.rs' crates/cditor-editor-gpui/src || true
)
if [ -n "$direct_persistence_capture_violations" ]; then
  echo 'error: Editor save capture and completion must use cditor-session persistence ports:' >&2
  echo "$direct_persistence_capture_violations" >&2
  exit 1
fi

direct_ai_session_mutation_violations=$(
  grep -R -n -E '\.(apply_ai_session_request|begin_ai_request|begin_ai_request_with_presentation|apply_ai_stream_event|cancel_ai_request|reject_ai_preview|accept_ai_preview)\(' \
    --include='*.rs' crates/cditor-editor-gpui/src || true
)
if [ -n "$direct_ai_session_mutation_violations" ]; then
  echo 'error: Editor AI session lifecycle must use apply_ai_session_request:' >&2
  echo "$direct_ai_session_mutation_violations" >&2
  exit 1
fi

direct_layout_mutation_violations=$(
  grep -R -n -E \
    '\.(queue_measured_height|apply_measured_height|flush_pending_height_corrections|flush_pending_height_corrections_with_priority|sync_viewport_height|scroll_by_delta|apply_scroll_accumulator_frame|scroll_focused_block_into_view|scroll_to_block_with_alignment|begin_scrollbar_drag|drag_scrollbar_to_thumb_top|finish_scrollbar_drag|current_page_window_planned|set_window_memory_pressure|set_table_horizontal_scroll_offset_px)\(' \
    --include='*.rs' crates/cditor-editor-gpui/src || true
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
    --include='*.rs' crates/cditor-editor-gpui/src apps/cditor-desktop/src || true
)
if [ -n "$direct_runtime_state_violations" ]; then
  echo 'error: Editor/App must use Runtime queries, projections, commands, or narrow realtime ports instead of child state fields:' >&2
  echo "$direct_runtime_state_violations" >&2
  exit 1
fi

printable_keydown_violations=$(
  grep -R -n -E 'InsertChar|InsertSpaceOrMarkdownShortcut' --include='*.rs' crates/cditor-editor-gpui/src || true
)
if [ -n "$printable_keydown_violations" ]; then
  echo 'error: printable text must enter through GPUI EntityInputHandler, not a keydown command:' >&2
  echo "$printable_keydown_violations" >&2
  exit 1
fi

for delayed_interaction in \
  crates/cditor-editor-gpui/src/editor_view/formatting/selection_delay.rs \
  crates/cditor-editor-gpui/src/editor_view/formatting/color.rs \
  crates/cditor-editor-gpui/src/interaction/selection_drag.rs \
  crates/cditor-editor-gpui/src/interaction/gutter_drag.rs
do
  if ! grep -q 'document_epoch' "$delayed_interaction"; then
    echo "error: delayed document interaction must reject callbacks from an older document epoch: $delayed_interaction" >&2
    exit 1
  fi
done

if ! grep -q 'session_identity' crates/cditor-editor-gpui/src/input/ime/adapter.rs \
  || ! grep -q 'epoch' crates/cditor-editor-gpui/src/text/caret_blink.rs; then
  echo 'error: IME callbacks and caret blink must retain their stale callback identities' >&2
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

if grep -Eq '^[[:space:]]*cditor-(runtime|viewport|storage|editor(-gpui)?|api|ai|import-export)[[:space:]]*=|^[[:space:]]*(gpui|sqlx|reqwest|parley)[[:space:]]*=' crates/cditor-editor-protocol/Cargo.toml; then
  echo 'error: editor protocol may only depend on core and serialization support' >&2
  exit 1
fi

if grep -Eq '(^|[[:space:]])gpui[[:space:]]*=|(^|[[:space:]])sqlx[[:space:]]*=' crates/cditor-core/Cargo.toml; then
  echo 'error: core must remain independent from GPUI and SQLx' >&2
  exit 1
fi

oversized=$(
  find crates components apps \
    -type f -name '*.rs' -exec wc -l {} + \
    | awk '$2 != "total" && $1 > 700 { print $1 " " $2 }'
)
if [ -n "$oversized" ]; then
  echo 'error: Rust files must not exceed 700 lines:' >&2
  echo "$oversized" >&2
  exit 1
fi

system_files=$(
  find . \
    -path './.git' -prune -o \
    -path './target' -prune -o \
    -name '.DS_Store' -print
)
if [ -n "$system_files" ]; then
  echo 'error: system metadata found in the repository:' >&2
  echo "$system_files" >&2
  exit 1
fi

echo 'Structure checks passed.'
