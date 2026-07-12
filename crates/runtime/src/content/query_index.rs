use std::collections::{HashMap, HashSet, VecDeque};

use cditor_core::ids::{BlockId, DocumentId};
use cditor_editor::scroll::{
    BlockScrollResolver, LayoutPx, ResolvedBlockScrollTarget, ScrollPrecision,
};

pub const BLOCK_FTS_SCHEMA: &str = r#"CREATE VIRTUAL TABLE block_fts USING fts5(
    document_id UNINDEXED,
    block_id UNINDEXED,
    plain_text,
    tokenize = 'unicode61'
);

CREATE TABLE block_fts_state (
    block_id TEXT PRIMARY KEY,
    content_version INTEGER NOT NULL,
    indexed_at INTEGER NOT NULL
);"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockPayloadForQuery {
    PlainText(String),
    Markdown(String),
    Html(String),
    Image { alt: String, caption: String },
    Embed { title: String, url: String },
}

impl BlockPayloadForQuery {
    pub fn plain_text(&self) -> String {
        match self {
            Self::PlainText(text) | Self::Markdown(text) => normalize_whitespace(text),
            Self::Html(html) => normalize_whitespace(&strip_tags(html)),
            Self::Image { alt, caption } => normalize_whitespace(&format!("{alt} {caption}")),
            Self::Embed { title, url } => normalize_whitespace(&format!("{title} {url}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtsUpdateTask {
    pub document_id: DocumentId,
    pub block_id: BlockId,
    pub content_version: u64,
    pub payload: BlockPayloadForQuery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtsEntry {
    pub document_id: DocumentId,
    pub block_id: BlockId,
    pub content_version: u64,
    pub indexed_at: u64,
    pub plain_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResult {
    pub block_id: BlockId,
    pub content_version: u64,
    pub score: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueryScrollTarget {
    pub block_id: BlockId,
    pub block_index: usize,
    pub global_scroll_top: LayoutPx,
    pub offset_in_block: LayoutPx,
    pub precision: ScrollPrecision,
    pub restore_anchor_after_load: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtsApplyResult {
    pub applied: usize,
    pub remaining: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocumentQueryIndex {
    entries: HashMap<(DocumentId, BlockId), FtsEntry>,
    tokens: HashMap<(DocumentId, String), HashSet<BlockId>>,
    pending_updates: VecDeque<FtsUpdateTask>,
    indexed_clock: u64,
}

impl DocumentQueryIndex {
    pub fn schema() -> &'static str {
        BLOCK_FTS_SCHEMA
    }

    pub fn enqueue_update(&mut self, task: FtsUpdateTask) {
        self.pending_updates.push_back(task);
    }

    pub fn pending_updates(&self) -> usize {
        self.pending_updates.len()
    }

    pub fn apply_pending_updates(&mut self, max_updates: usize) -> FtsApplyResult {
        let mut applied = 0;
        for _ in 0..max_updates {
            let Some(task) = self.pending_updates.pop_front() else {
                break;
            };
            self.apply_update(task);
            applied += 1;
        }
        FtsApplyResult {
            applied,
            remaining: self.pending_updates.len(),
        }
    }

    pub fn query(&self, document_id: DocumentId, query: &str, limit: usize) -> Vec<QueryResult> {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() || limit == 0 {
            return Vec::new();
        }

        let mut candidates: Option<HashSet<BlockId>> = None;
        for token in &query_tokens {
            let blocks = self
                .tokens
                .get(&(document_id, token.clone()))
                .cloned()
                .unwrap_or_default();
            candidates = Some(match candidates.take() {
                Some(existing) => existing.intersection(&blocks).copied().collect(),
                None => blocks,
            });
        }

        let mut results = candidates
            .unwrap_or_default()
            .into_iter()
            .filter_map(|block_id| self.entries.get(&(document_id, block_id)))
            .map(|entry| QueryResult {
                block_id: entry.block_id,
                content_version: entry.content_version,
                score: score_entry(entry, &query_tokens),
            })
            .collect::<Vec<_>>();
        results.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.block_id.cmp(&right.block_id))
        });
        results.truncate(limit);
        results
    }

    pub fn scroll_to_block<R: BlockScrollResolver>(
        &self,
        block_id: BlockId,
        resolver: &R,
    ) -> Option<QueryScrollTarget> {
        let resolved = resolver.resolve_block_scroll_target(block_id)?;
        Some(QueryScrollTarget::from_resolved(block_id, resolved))
    }

    pub fn contains_block(&self, document_id: DocumentId, block_id: BlockId) -> bool {
        self.entries.contains_key(&(document_id, block_id))
    }

    fn apply_update(&mut self, task: FtsUpdateTask) {
        let key = (task.document_id, task.block_id);
        if let Some(previous) = self.entries.remove(&key) {
            for token in tokenize(&previous.plain_text) {
                if let Some(blocks) = self.tokens.get_mut(&(previous.document_id, token)) {
                    blocks.remove(&previous.block_id);
                }
            }
        }

        self.indexed_clock = self.indexed_clock.saturating_add(1);
        let plain_text = task.payload.plain_text();
        let entry = FtsEntry {
            document_id: task.document_id,
            block_id: task.block_id,
            content_version: task.content_version,
            indexed_at: self.indexed_clock,
            plain_text,
        };
        for token in tokenize(&entry.plain_text) {
            self.tokens
                .entry((entry.document_id, token))
                .or_default()
                .insert(entry.block_id);
        }
        self.entries.insert(key, entry);
    }
}

impl QueryScrollTarget {
    fn from_resolved(block_id: BlockId, resolved: ResolvedBlockScrollTarget) -> Self {
        Self {
            block_id,
            block_index: resolved.block_index,
            global_scroll_top: resolved.global_scroll_top,
            offset_in_block: resolved.offset_in_block,
            precision: resolved.precision,
            restore_anchor_after_load: resolved.precision != ScrollPrecision::Exact,
        }
    }
}

fn score_entry(entry: &FtsEntry, query_tokens: &[String]) -> usize {
    let entry_tokens = tokenize(&entry.plain_text);
    query_tokens
        .iter()
        .map(|query| entry_tokens.iter().filter(|token| *token == query).count())
        .sum()
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                result.push(' ');
            }
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct MockResolver {
        block_id: BlockId,
        target: ResolvedBlockScrollTarget,
    }

    impl BlockScrollResolver for MockResolver {
        fn resolve_block_scroll_target(
            &self,
            block_id: BlockId,
        ) -> Option<ResolvedBlockScrollTarget> {
            (block_id == self.block_id).then_some(self.target)
        }
    }

    #[test]
    fn schema_declares_sqlite_fts5_block_table() {
        assert!(DocumentQueryIndex::schema().contains("CREATE VIRTUAL TABLE block_fts USING fts5"));
        assert!(DocumentQueryIndex::schema().contains("plain_text"));
        assert!(DocumentQueryIndex::schema().contains("block_fts_state"));
    }

    #[test]
    fn plain_text_extraction_does_not_need_ui_entity() {
        let payload = BlockPayloadForQuery::Html(
            "<h1 onclick='x()'>Title</h1><p>Hello <strong>world</strong></p>".to_owned(),
        );
        assert_eq!(payload.plain_text(), "Title Hello world");
    }

    #[test]
    fn searches_100k_blocks_and_returns_block_id_outside_current_window() {
        let mut index = DocumentQueryIndex::default();
        for block_id in 1..=100_000 {
            let text = if block_id == 88_888 {
                "needle in a remote page".to_owned()
            } else {
                format!("ordinary block {block_id}")
            };
            index.enqueue_update(FtsUpdateTask {
                document_id: 1,
                block_id,
                content_version: 1,
                payload: BlockPayloadForQuery::PlainText(text),
            });
        }

        let applied = index.apply_pending_updates(100_000);
        assert_eq!(applied.applied, 100_000);
        assert_eq!(applied.remaining, 0);

        let results = index.query(1, "needle", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].block_id, 88_888);
    }

    #[test]
    fn fts_update_is_background_incremental_and_does_not_block_typing() {
        let mut index = DocumentQueryIndex::default();
        for block_id in 1..=1_000 {
            index.enqueue_update(FtsUpdateTask {
                document_id: 1,
                block_id,
                content_version: 1,
                payload: BlockPayloadForQuery::PlainText(format!("block {block_id}")),
            });
        }

        let first_frame = index.apply_pending_updates(16);
        assert_eq!(first_frame.applied, 16);
        assert_eq!(first_frame.remaining, 984);
        assert_eq!(index.query(1, "block", 1).len(), 1);
    }

    #[test]
    fn search_result_jump_to_remote_page_uses_estimated_scroll_target_and_anchor_restore() {
        let mut index = DocumentQueryIndex::default();
        index.enqueue_update(FtsUpdateTask {
            document_id: 1,
            block_id: 9_000,
            content_version: 1,
            payload: BlockPayloadForQuery::PlainText("remote result".to_owned()),
        });
        index.apply_pending_updates(1);

        let resolver = MockResolver {
            block_id: 9_000,
            target: ResolvedBlockScrollTarget {
                block_index: 8_999,
                offset_in_block: 0.0,
                global_scroll_top: 8_999.0 * 32.0,
                precision: ScrollPrecision::Estimated,
            },
        };
        let target = index.scroll_to_block(9_000, &resolver).unwrap();

        assert_eq!(target.block_id, 9_000);
        assert_eq!(target.block_index, 8_999);
        assert_eq!(target.precision, ScrollPrecision::Estimated);
        assert!(target.restore_anchor_after_load);
    }
}
