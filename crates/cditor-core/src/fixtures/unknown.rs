//! 跨 storage/clipboard/native export 共用的无损 fixture。
//!
//! 两种"读不懂的块"：
//!
//! - unknown plugin：本 build 没装的插件块，写入方显式构造
//!   [`BlockPayload::Opaque`]；
//! - future build：更新版本新增的内建块类型（例如旧版本读到 `Video`），由
//!   `Deserialize` 落进 `Unknown` 包装变体。
//!
//! 两者都要求 load → save 字节不变。

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

/// 更新版本写下的 kind：本 build 只知道它的 wire 形态。
pub const FUTURE_BLOCK_KIND_JSON: &str = "\"Audio\"";
/// 更新版本写下的 payload：故意保留不规范空白，用来证明字节未被规范化。
pub const FUTURE_BLOCK_PAYLOAD_JSON: &str = "{\"Audio\":{ \"source\":\"assets/a.m4a\",\"gain_milli\":900 ,\"future\":{\"order\":[3, 2 ,1]}}}";

/// 模拟"更新版本写下的块"：kind 与 payload 都是本 build 读不懂的形态。
pub fn future_build_payload(block_id: BlockId) -> BlockPayloadRecord {
    BlockPayloadRecord {
        block_id,
        content_version: 11,
        kind: serde_json::from_str(FUTURE_BLOCK_KIND_JSON).expect("fixture kind is valid JSON"),
        payload: serde_json::from_str(FUTURE_BLOCK_PAYLOAD_JSON)
            .expect("fixture payload is valid JSON"),
    }
}

/// 断言 fixture 的原始字节在往返后一字不改。
pub fn assert_future_build_bytes(payload: &BlockPayloadRecord) {
    let RichBlockKind::Unknown(kind) = &payload.kind else {
        panic!("expected an unknown kind, got {:?}", payload.kind)
    };
    let BlockPayload::Unknown(unknown) = &payload.payload else {
        panic!("expected an unknown payload, got {:?}", payload.payload)
    };
    assert_eq!(kind.json(), FUTURE_BLOCK_KIND_JSON);
    assert_eq!(kind.tag(), "Audio");
    assert_eq!(unknown.json(), FUTURE_BLOCK_PAYLOAD_JSON);
    assert_eq!(unknown.tag(), "Audio");
    assert_eq!(
        serde_json::to_string(&payload.kind).unwrap(),
        FUTURE_BLOCK_KIND_JSON
    );
    assert_eq!(
        serde_json::to_string(&payload.payload).unwrap(),
        FUTURE_BLOCK_PAYLOAD_JSON
    );
}

#[cfg(test)]
mod future_build_tests {
    use super::*;

    #[test]
    fn future_build_fixture_survives_serde_copy_and_move_without_normalizing_bytes() {
        let original = future_build_payload(11);

        let encoded = serde_json::to_string(&original).unwrap();
        let moved: BlockPayloadRecord = serde_json::from_str(&encoded).unwrap();

        assert_future_build_bytes(&moved);
        assert_eq!(moved, original);
    }
}
