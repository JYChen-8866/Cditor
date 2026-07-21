use super::*;

impl DocumentRuntime {
    pub fn collection_records_snapshot(
        &self,
        collection_id: CollectionId,
    ) -> &[CollectionRecordSnapshot] {
        self.collection_records
            .get(&collection_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn comment_thread_snapshot(
        &self,
        thread_id: CommentThreadId,
    ) -> Option<&CommentThreadSnapshot> {
        self.comment_threads.get(&thread_id)
    }

    pub fn asset_snapshot(&self, asset_id: AssetId) -> Option<&AssetSnapshot> {
        self.assets.get(&asset_id)
    }

    pub fn attached_asset_ids(&self, block_id: BlockId) -> Vec<AssetId> {
        self.block_asset_ids
            .get(&block_id)
            .map(|asset_ids| asset_ids.iter().copied().collect())
            .unwrap_or_default()
    }
}
