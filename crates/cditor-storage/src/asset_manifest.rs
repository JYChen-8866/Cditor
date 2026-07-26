use cditor_core::edit::AssetSnapshot;
use cditor_core::ids::{AssetId, DocumentId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionalAssetRequest {
    pub workspace_id: u64,
    pub asset: AssetSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetManifestRecord {
    pub workspace_id: u64,
    pub asset: AssetSnapshot,
    pub canonical_asset_id: Option<AssetId>,
    pub upload_session_id: Option<String>,
    pub uploaded_bytes: u64,
    pub remote_object_key: Option<String>,
    pub public_url: Option<String>,
    pub attempt_count: u32,
    pub last_error: Option<String>,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetUploadMutation {
    Begin {
        upload_session_id: String,
    },
    Progress {
        upload_session_id: String,
        uploaded_bytes: u64,
    },
    Complete {
        upload_session_id: String,
        canonical_asset_id: AssetId,
        remote_object_key: String,
        public_url: Option<String>,
    },
    Fail {
        upload_session_id: Option<String>,
        error: String,
    },
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetReference {
    pub document_id: DocumentId,
    pub block_id: u64,
    pub asset_id: AssetId,
    pub role: String,
}
