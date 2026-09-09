use async_trait::async_trait;
use cditor_core::document::BlockIndexRecord;
use cditor_core::ids::{BlockId, DocumentId};
use cditor_core::rich_text::BlockPayloadRecord;
use cditor_storage::{
    DocumentStorage, LoadDocumentRequest, LoadedDocument, LoadedPayloadBatch,
    StorageBackendKind, StorageCapabilities, StorageDocumentMetadata, StorageError,
    StorageResult, StorageSaveBatch, StorageSaveOutcome,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};
use std::io::Write;

use crate::parser::parse_markdown_to_blocks;
use crate::generator::blocks_to_markdown;

/// Markdown 文件存储后端
///
/// 将每个文档存储为单独的 .md 文件
/// 文件名格式: {document_id}.md
pub struct MarkdownStorage {
    base_dir: PathBuf,
    /// 缓存已加载的文档，避免重复解析
    cache: Arc<Mutex<HashMap<DocumentId, CachedDocument>>>,
    commit_lock: tokio::sync::Mutex<()>,
}

struct CachedDocument {
    source: String,
    index_records: Vec<BlockIndexRecord>,
    payloads: Vec<BlockPayloadRecord>,
    structure_version: u64,
}

impl MarkdownStorage {
    /// 创建新的 Markdown 存储后端
    ///
    /// # Arguments
    /// * `base_dir` - Markdown 文件的根目录
    pub fn new(base_dir: PathBuf) -> Self {
        info!("Creating MarkdownStorage with base_dir: {:?}", base_dir);

        Self {
            base_dir,
            cache: Arc::new(Mutex::new(HashMap::new())),
            commit_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// 获取文档的文件路径
    fn document_path(&self, document_id: DocumentId) -> PathBuf {
        self.base_dir.join(format!("{}.md", document_id))
    }

    /// 从缓存或文件加载文档
    async fn load_or_parse(&self, document_id: DocumentId) -> StorageResult<CachedDocument> {
        // 先检查缓存
        {
            let cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(&document_id) {
                debug!("Document {} found in cache", document_id);
                return Ok(cached.clone());
            }
        }

        // 从文件解析
        let path = self.document_path(document_id);
        debug!("Loading document from: {:?}", path);

        let markdown = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| {
                warn!("Failed to read markdown file {:?}: {}", path, e);
                StorageError::Io(format!("Failed to read markdown file: {}", e))
            })?;

        let (index_records, payloads) = parse_markdown_to_blocks(&markdown, document_id)?;

        let cached = CachedDocument {
            source: markdown,
            index_records: index_records.clone(),
            payloads: payloads.clone(),
            structure_version: 1,
        };

        // 更新缓存
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(document_id, cached.clone());
        }

        info!(
            "Loaded document {} with {} blocks",
            document_id,
            index_records.len()
        );

        Ok(cached)
    }
}

impl Clone for CachedDocument {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            index_records: self.index_records.clone(),
            payloads: self.payloads.clone(),
            structure_version: self.structure_version,
        }
    }
}

#[async_trait]
impl DocumentStorage for MarkdownStorage {
    fn backend_kind(&self) -> StorageBackendKind {
        StorageBackendKind::Local
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities {
            payload_window: false,  // 所有内容一次加载
            emergency_log: false,   // 不支持应急日志
        }
    }

    async fn load_document(
        &self,
        request: LoadDocumentRequest,
    ) -> StorageResult<LoadedDocument> {
        info!("Loading document {}", request.document_id);

        let cached = self.load_or_parse(request.document_id).await?;

        Ok(LoadedDocument {
            metadata: StorageDocumentMetadata {
                document_id: request.document_id,
                title: format!("Document {}", request.document_id),
                icon_json: None,
                cover_json: None,
                structure_version: cached.structure_version,
                content_version: cached.structure_version,
                layout_version: 0,
                schema_version: 1,
            },
            records: cached.index_records,
            block_attrs: vec![],
            initial_payloads: cached.payloads,
            initial_payload_window_end: 0,
            index_from_snapshot: false,
            layout_cache_hits: 0,
            page_layout_snapshot: None,
        })
    }

    async fn load_payloads(
        &self,
        document_id: DocumentId,
        block_ids: &[BlockId],
    ) -> StorageResult<LoadedPayloadBatch> {
        debug!(
            "Loading {} payloads for document {}",
            block_ids.len(),
            document_id
        );

        // Markdown 模式下，所有内容在 load_document 时已加载
        // 这里从缓存查找
        let cache = self.cache.lock().unwrap();

        if let Some(cached) = cache.get(&document_id) {
            let mut records = Vec::new();
            let mut missing = Vec::new();

            for &block_id in block_ids {
                if let Some(payload) = cached.payloads.iter().find(|p| p.block_id == block_id) {
                    records.push(payload.clone());
                } else {
                    missing.push(block_id);
                }
            }

            Ok(LoadedPayloadBatch {
                records,
                missing_block_ids: missing,
            })
        } else {
            // 缓存中没有，返回全部缺失
            Ok(LoadedPayloadBatch {
                records: vec![],
                missing_block_ids: block_ids.to_vec(),
            })
        }
    }

    async fn commit(&self, batch: StorageSaveBatch) -> StorageResult<StorageSaveOutcome> {
        let _commit_guard = self.commit_lock.lock().await;
        info!(
            "Saving document {} with {} blocks",
            batch.document_id,
            batch.index_records.len()
        );

        let path = self.document_path(batch.document_id);
        let previous = if tokio::fs::try_exists(&path).await.map_err(|e| StorageError::Io(e.to_string()))? {
            Some(self.load_or_parse(batch.document_id).await?)
        } else { None };
        let index_records = if batch.index_records.is_empty() {
            previous.as_ref().map(|p| p.index_records.clone()).unwrap_or_default()
        } else { batch.index_records.clone() };
        let mut payloads = previous.as_ref().map(|p| p.payloads.clone()).unwrap_or_default();
        for payload in &batch.payloads {
            if let Some(old) = payloads.iter_mut().find(|p| p.block_id == payload.block_id) {
                if payload.content_version < old.content_version {
                    return Err(StorageError::Conflict("stale payload version".into()));
                }
                *old = payload.clone();
            } else { payloads.push(payload.clone()); }
        }
        payloads.retain(|p| index_records.iter().any(|r| r.id == p.block_id));
        let markdown = if let Some(old) = &previous {
            if batch.structure_version < old.structure_version {
                return Err(StorageError::Conflict("stale structure version".into()));
            }
            crate::source_sync::sync_source(&old.source, &old.index_records, &old.payloads, &index_records, &payloads)?
        } else { blocks_to_markdown(&index_records, &payloads)? };

        // 写入文件
        if let Some(old) = &previous {
            let disk = tokio::fs::read_to_string(&path).await.map_err(|e| StorageError::Io(e.to_string()))?;
            if disk != old.source { return Err(StorageError::Conflict("Markdown changed on disk; reload before saving".into())); }
        }
        // Same-directory temporary file prevents a failed write truncating the note.
        // The host still needs filesystem locking for concurrent external writers.
        let mut temporary = tempfile::NamedTempFile::new_in(&self.base_dir).map_err(|e| StorageError::Io(e.to_string()))?;
        if let Ok(metadata) = std::fs::metadata(&path) {
            temporary.as_file().set_permissions(metadata.permissions()).map_err(|e| StorageError::Io(e.to_string()))?;
        }
        temporary.write_all(markdown.as_bytes()).map_err(|e| StorageError::Io(e.to_string()))?;
        temporary.as_file().sync_all().map_err(|e| StorageError::Io(e.to_string()))?;
        temporary.persist(&path).map_err(|e| StorageError::Io(e.to_string()))?;

        // 收集保存的 payload 版本（在移动 batch 之前）
        let saved_payload_versions: Vec<(BlockId, u64)> = batch
            .payloads
            .iter()
            .map(|p| (p.block_id, p.content_version))
            .collect();

        let saved_structure_version = batch.saved_structure_version();

        // 更新缓存
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(
                batch.document_id,
                CachedDocument {
                    source: markdown,
                    index_records,
                    payloads,
                    structure_version: batch.structure_version,
                },
            );
        }

        info!("Successfully saved document {}", batch.document_id);

        Ok(StorageSaveOutcome {
            saved_structure_version,
            saved_payload_versions,
        })
    }
}
