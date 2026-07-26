use std::{collections::HashSet, sync::Arc};

use cditor_core::document::{DocumentIndex, VisibleDocumentIndex};
use cditor_core::layout::{PAGE_POLICY_VERSION, PageLayoutIndex, PagePolicy};
use cditor_core::schema::CURRENT_DOCUMENT_FORMAT;
use cditor_runtime::document_runtime::{DocumentRuntimeColdStartData, DocumentRuntimeIndexSource};
use cditor_sdk::options::{CditorBackend, CditorOptions};
use cditor_session::{
    DocumentPersistence, EmergencyRecoveryDecision, EmergencyRecoveryPlan, PersistencePipeline,
    PreparedSessionColdStartResult, SessionColdStartRequest, plan_emergency_recovery,
    prepare_editor_session_with_persistence,
};

use cditor_storage::layout_cache::LayoutCacheKey;
use cditor_storage::{
    DOCUMENT_INDEX_VISIBLE_VERSION, DocumentLoadProgress, DocumentLoadStage, DocumentStorage,
    LoadDocumentRequest, LoadedDocument, StorageBackendKind, StorageResult,
};

const DOCUMENT_LOAD_PROGRESS_UNITS: usize = 7;
const COLD_START_PROGRESS_UNITS: usize = 11;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CditorColdStartProgress {
    pub message: String,
    pub completed_units: usize,
    pub total_units: usize,
}

impl CditorColdStartProgress {
    fn new(message: impl Into<String>, completed_units: usize) -> Self {
        Self {
            message: message.into(),
            completed_units: completed_units.min(COLD_START_PROGRESS_UNITS),
            total_units: COLD_START_PROGRESS_UNITS,
        }
    }

    pub fn percentage(&self) -> u8 {
        ((self.completed_units * 100) / self.total_units) as u8
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CditorColdStartPlan {
    Demo,
    LargeDemo,
    Memory,
    Persistent {
        document_id: cditor_core::ids::DocumentId,
        label: String,
        timeout: std::time::Duration,
    },
    Cloud {
        endpoint: String,
    },
    Invalid {
        reason: String,
    },
}

impl CditorColdStartPlan {
    pub fn from_options(options: &CditorOptions) -> Self {
        match &options.backend {
            CditorBackend::Demo => Self::Demo,
            CditorBackend::LargeDemo => Self::LargeDemo,
            CditorBackend::Memory => Self::Memory,
            CditorBackend::Persistent { provider } => match options.document_id {
                Some(document_id) => Self::Persistent {
                    document_id,
                    label: provider.label().to_owned(),
                    timeout: provider.open_timeout(),
                },
                None => Self::Invalid {
                    reason: format!("{} backend requires document_id", provider.label()),
                },
            },
            CditorBackend::Cloud { endpoint } => Self::Cloud {
                endpoint: endpoint.clone(),
            },
        }
    }

    pub fn persistent_label(&self) -> Option<String> {
        match self {
            Self::Persistent {
                document_id, label, ..
            } => Some(format!("{label} document {document_id}")),
            _ => None,
        }
    }

    pub fn timeout(&self) -> std::time::Duration {
        match self {
            Self::Persistent { timeout, .. } => *timeout,
            _ => std::time::Duration::from_secs(90),
        }
    }
}

#[derive(Debug)]
pub struct CditorSessionLoadResult {
    pub prepared: PreparedSessionColdStartResult,
    pub schema_access: DocumentSchemaAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentSchemaAccess {
    ReadWrite,
    ReadOnlyNewerMajor {
        written_major: u64,
        supported_major: u32,
    },
    ReadOnlyNewerOperationMajor {
        written_major: u32,
        supported_major: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRuntimeLoadOptions {
    pub viewport_height: u32,
    pub visible_index_version: i64,
    pub initial_payload_window_blocks: usize,
    pub layout_key: LayoutCacheKey,
    pub page_policy_version: u64,
    pub readonly: bool,
    pub autosave_interval: Option<std::time::Duration>,
}

impl Default for StorageRuntimeLoadOptions {
    fn default() -> Self {
        Self {
            viewport_height: 720,
            visible_index_version: DOCUMENT_INDEX_VISIBLE_VERSION,
            initial_payload_window_blocks: 64,
            layout_key: LayoutCacheKey {
                width_bucket: 10,
                exact_width_px: 800,
                content_version: 1,
                attrs_version: 0,
                style_version: 0,
                font_version: 0,
                theme_version: 0,
                scale_factor_milli: 1000,
            },
            page_policy_version: PAGE_POLICY_VERSION,
            readonly: false,
            autosave_interval: None,
        }
    }
}

pub async fn load_session_from_options(
    options: &CditorOptions,
) -> StorageResult<Option<CditorSessionLoadResult>> {
    load_session_from_options_with_progress(options, |_| {}).await
}

pub async fn load_session_from_options_with_progress(
    options: &CditorOptions,
    mut progress: impl FnMut(CditorColdStartProgress) + Send,
) -> StorageResult<Option<CditorSessionLoadResult>> {
    let document_id = match options.document_id {
        Some(document_id) => document_id,
        None => return Ok(None),
    };
    progress(CditorColdStartProgress::new("正在连接数据库", 0));
    let storage: Arc<dyn DocumentStorage> = match &options.backend {
        CditorBackend::Demo
        | CditorBackend::LargeDemo
        | CditorBackend::Memory
        | CditorBackend::Cloud { .. } => return Ok(None),
        CditorBackend::Persistent { provider } => provider.open().await?,
    };
    progress(CditorColdStartProgress::new(
        "已连接数据库，正在读取文档",
        1,
    ));
    load_session_from_storage(
        storage,
        document_id,
        options.workspace_id.unwrap_or(1),
        cold_start_options(options),
        &mut progress,
    )
    .await
    .map(Some)
}

async fn load_session_from_storage(
    storage: Arc<dyn DocumentStorage>,
    document_id: cditor_core::ids::DocumentId,
    workspace_id: u64,
    options: StorageRuntimeLoadOptions,
    progress: &mut (dyn FnMut(CditorColdStartProgress) + Send),
) -> StorageResult<CditorSessionLoadResult> {
    let backend_kind = storage.backend_kind();
    let mut report_document_progress = |event: DocumentLoadProgress| {
        let document_units = if event.total_units == 0 {
            0
        } else {
            event.completed_units.min(event.total_units) * DOCUMENT_LOAD_PROGRESS_UNITS
                / event.total_units
        };
        progress(CditorColdStartProgress::new(
            document_load_message(event.stage),
            1 + document_units,
        ));
    };
    let loaded = storage
        .load_document_with_progress(
            LoadDocumentRequest {
                document_id,
                workspace_id,
                initial_payload_window_blocks: options.initial_payload_window_blocks,
                visible_index_version: options.visible_index_version,
                layout_key: options.layout_key,
                page_policy_version: options.page_policy_version,
            },
            &mut report_document_progress,
        )
        .await?;
    drop(report_document_progress);
    let mut schema_access = document_schema_access(loaded.metadata.schema_version, backend_kind)?;
    let emergency_log_entries = if storage.capabilities().emergency_log {
        storage.load_emergency_transactions(document_id).await?
    } else {
        Vec::new()
    };
    progress(CditorColdStartProgress::new("已检查恢复日志", 9));
    let emergency_decision = if emergency_log_entries.is_empty() {
        EmergencyRecoveryDecision::Replay(EmergencyRecoveryPlan {
            transactions: Vec::new(),
            affected_block_ids: Vec::new(),
            through_sequence: None,
        })
    } else {
        plan_emergency_recovery(emergency_log_entries.clone())
            .map_err(cditor_storage::StorageError::CorruptData)?
    };
    let emergency_payloads = match &emergency_decision {
        EmergencyRecoveryDecision::Replay(plan)
            if matches!(schema_access, DocumentSchemaAccess::ReadWrite) =>
        {
            load_emergency_payloads(storage.as_ref(), &loaded, plan).await?
        }
        EmergencyRecoveryDecision::Replay(_) => Vec::new(),
        EmergencyRecoveryDecision::ReadOnlyNewerMajor { written_major, .. } => {
            schema_access = DocumentSchemaAccess::ReadOnlyNewerOperationMajor {
                written_major: *written_major,
                supported_major: cditor_core::schema::SchemaDomain::Operation
                    .current_version()
                    .major,
            };
            Vec::new()
        }
    };
    progress(CditorColdStartProgress::new("已恢复未保存内容", 10));

    let cached_page_layout = cached_page_layout(&loaded, &options);
    let cold_start_data = cold_start_data(loaded);
    let persistence = PersistencePipeline::for_session(
        DocumentPersistence::new(storage, document_id).with_layout_key(options.layout_key),
        options.autosave_interval,
    );
    let prepared = prepare_editor_session_with_persistence(
        SessionColdStartRequest {
            data: cold_start_data,
            viewport_height: f64::from(options.viewport_height),
            cached_page_layout,
            readonly: options.readonly || !matches!(schema_access, DocumentSchemaAccess::ReadWrite),
            emergency_log_entries,
            emergency_payloads,
        },
        persistence,
    )
    .map_err(|error| cditor_storage::StorageError::CorruptData(error.to_string()))?;
    progress(CditorColdStartProgress::new("文档已准备完成", 11));
    Ok(CditorSessionLoadResult {
        prepared,
        schema_access,
    })
}

fn document_load_message(stage: DocumentLoadStage) -> &'static str {
    match stage {
        DocumentLoadStage::EnsureDocument => "已检查文档",
        DocumentLoadStage::Metadata => "已读取文档信息",
        DocumentLoadStage::Structure => "已读取文档结构",
        DocumentLoadStage::BlockLayout => "已恢复块布局",
        DocumentLoadStage::PageLayout => "已恢复页面布局",
        DocumentLoadStage::Attributes => "已读取块属性",
        DocumentLoadStage::InitialPayloads => "已读取首屏内容",
    }
}

fn document_schema_access(
    stored_major: u64,
    backend: StorageBackendKind,
) -> StorageResult<DocumentSchemaAccess> {
    let supported_major = CURRENT_DOCUMENT_FORMAT.major;
    match stored_major.cmp(&u64::from(supported_major)) {
        std::cmp::Ordering::Equal => Ok(DocumentSchemaAccess::ReadWrite),
        std::cmp::Ordering::Greater => Ok(DocumentSchemaAccess::ReadOnlyNewerMajor {
            written_major: stored_major,
            supported_major,
        }),
        std::cmp::Ordering::Less => Err(cditor_storage::StorageError::Migration {
            backend,
            message: format!(
                "document schema v{stored_major} must be migrated to v{supported_major} before opening"
            ),
        }),
    }
}

fn cached_page_layout(
    loaded: &LoadedDocument,
    options: &StorageRuntimeLoadOptions,
) -> Option<PageLayoutIndex> {
    let document_index = DocumentIndex::new(
        loaded.metadata.document_id,
        loaded.records.iter().copied(),
        loaded.metadata.structure_version,
    )
    .ok()?;
    let visible_index = VisibleDocumentIndex::from_document_index(&document_index);
    loaded.page_layout_snapshot.as_ref().and_then(|snapshot| {
        snapshot
            .to_page_layout_index(
                options.visible_index_version,
                loaded.metadata.structure_version,
                options.layout_key,
                options.page_policy_version,
                PagePolicy::default(),
                &visible_index.visible_block_ids,
            )
            .ok()
    })
}

fn cold_start_data(loaded: LoadedDocument) -> DocumentRuntimeColdStartData {
    DocumentRuntimeColdStartData {
        document_id: loaded.metadata.document_id,
        document_title: loaded.metadata.title,
        structure_version: loaded.metadata.structure_version,
        records: loaded.records,
        block_attrs: loaded.block_attrs,
        initial_payloads: loaded.initial_payloads,
        initial_payload_window_end: loaded.initial_payload_window_end,
        index_source: if loaded.index_from_snapshot {
            DocumentRuntimeIndexSource::Snapshot
        } else {
            DocumentRuntimeIndexSource::Blocks
        },
        layout_cache_hits: loaded.layout_cache_hits,
    }
}

async fn load_emergency_payloads(
    storage: &dyn DocumentStorage,
    loaded: &LoadedDocument,
    plan: &EmergencyRecoveryPlan,
) -> StorageResult<Vec<cditor_core::rich_text::BlockPayloadRecord>> {
    let loaded_ids = loaded
        .initial_payloads
        .iter()
        .map(|payload| payload.block_id)
        .collect::<HashSet<_>>();
    let missing_from_window = plan
        .affected_block_ids
        .iter()
        .copied()
        .filter(|block_id| !loaded_ids.contains(block_id))
        .collect::<Vec<_>>();
    if missing_from_window.is_empty() {
        return Ok(Vec::new());
    }
    let batch = storage
        .load_payloads(loaded.metadata.document_id, &missing_from_window)
        .await?;
    if !batch.missing_block_ids.is_empty() {
        return Err(cditor_storage::StorageError::CorruptData(format!(
            "emergency recovery payloads are missing for blocks {:?}",
            batch.missing_block_ids
        )));
    }
    Ok(batch.records)
}

const MIN_INTERACTIVE_COLD_START_PAYLOAD_BLOCKS: usize = 256;

fn cold_start_options(options: &CditorOptions) -> StorageRuntimeLoadOptions {
    StorageRuntimeLoadOptions {
        initial_payload_window_blocks: options
            .payload_window_size
            .max(MIN_INTERACTIVE_COLD_START_PAYLOAD_BLOCKS),
        readonly: options.readonly,
        autosave_interval: options.autosave_interval,
        ..StorageRuntimeLoadOptions::default()
    }
}

#[cfg(test)]
#[path = "storage_host_tests.rs"]
mod tests;
