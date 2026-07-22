//! 跨 storage/clipboard/native export 共用的 unknown plugin 无损 fixture。

use serde_json::value::RawValue;

use crate::ids::BlockId;
use crate::rich_text::{BlockPayload, BlockPayloadRecord, RichBlockKind};
use crate::schema::{SchemaDomain, SchemaVersion, VersionedEnvelope};

pub const UNKNOWN_PLUGIN_KIND: &str = "future.vendor/interactive-card";
pub const UNKNOWN_PLUGIN_FALLBACK: &str = "Unsupported interactive card";
pub const UNKNOWN_PLUGIN_BODY: &str = "{\n  \"zzz\": \"\\u4f60\\u597d\",   \"kind\":\"interactive-card\",\"future_field\":{\"order\":[3, 2 ,1]},\"flag\":true}";

pub fn unknown_plugin_payload(block_id: BlockId) -> BlockPayloadRecord {
    let envelope = VersionedEnvelope::from_raw_parts(
        SchemaDomain::BlockPayload,
        SchemaVersion::new(1, 99),
        RawValue::from_string(UNKNOWN_PLUGIN_BODY.to_owned()).expect("fixture JSON is valid"),
    );
    BlockPayloadRecord {
        block_id,
        content_version: 7,
        kind: RichBlockKind::Custom(UNKNOWN_PLUGIN_KIND.to_owned()),
        payload: BlockPayload::opaque(envelope, UNKNOWN_PLUGIN_FALLBACK)
            .expect("fixture uses block payload domain"),
    }
}

pub fn assert_unknown_plugin_bytes(payload: &BlockPayloadRecord) {
    assert_eq!(
        payload.kind,
        RichBlockKind::Custom(UNKNOWN_PLUGIN_KIND.to_owned())
    );
    let BlockPayload::Opaque {
        envelope,
        plain_text_fallback,
    } = &payload.payload
    else {
        panic!("expected opaque plugin payload")
    };
    assert_eq!(envelope.body_bytes(), UNKNOWN_PLUGIN_BODY);
    assert_eq!(plain_text_fallback, UNKNOWN_PLUGIN_FALLBACK);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_survives_serde_copy_and_move_without_normalizing_raw_body() {
        let original = unknown_plugin_payload(7);
        let copied = original.clone();
        let encoded = serde_json::to_string(&copied).unwrap();
        let moved: BlockPayloadRecord = serde_json::from_str(&encoded).unwrap();

        assert_unknown_plugin_bytes(&moved);
        assert_eq!(moved, original);
    }
}
