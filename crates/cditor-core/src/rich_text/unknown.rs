//! 无法解码的 wire 形态原样保留（总设计 6.3 的 forward compatibility 规则）。
//!
//! `BlockPayload` 与 `RichBlockKind` 都是 externally tagged enum，直接以 JSON
//! 落库。新版本新增一个变体（例如 `Video`），旧版本读到未知 tag 时 serde 会报
//! `unknown variant`，整篇文档打不开。
//!
//! [`UnknownWire`] 把这种"本 build 读不懂"的 JSON 原始字节包起来：
//!
//! - 反序列化：未知 tag（或与本 build schema 不匹配的形状）落进包装变体；
//! - 序列化：写回 [`UnknownWire::json`] 的原始字节，**不加任何包装**。
//!
//! 于是 load → save 字节不变：旧版本能打开新版本写的文档、显示只读占位块，
//! 保存后新版本仍然看到自己的 `Video` 载荷。JSON 语法错误不走这条路（那是真
//! 正的数据损坏，仍然报错）；只有语法合法但本 build 解释不了的值才被保留。

use std::hash::{Hash, Hasher};

use serde::{Serialize, Serializer};
use serde_json::value::RawValue;

/// 本 build 无法解码、但必须无损保留的 JSON 值。
#[derive(Debug, Clone)]
pub struct UnknownWire {
    /// externally tagged enum 的 tag：`{"Video": …}` 取 `Video`，裸字符串
    /// `"Video"` 取其内容本身。仅用于诊断与占位文案。
    tag: String,
    /// 原始 JSON 字节。序列化时原样写回。
    json: Box<RawValue>,
    /// 本 build 解码失败的原因，用于占位文案与日志；不落盘。
    reason: String,
}

impl UnknownWire {
    /// 从原始 JSON 构造。`reason` 是本 build 的解码错误。
    pub fn new(json: Box<RawValue>, reason: impl Into<String>) -> Self {
        let tag = tag_of(json.get());
        Self {
            tag,
            json,
            reason: reason.into(),
        }
    }

    /// externally tagged enum 的 tag（未知块类型名）。
    pub fn tag(&self) -> &str {
        &self.tag
    }

    /// 原始 JSON 字节：无损证据。
    pub fn json(&self) -> &str {
        self.json.get()
    }

    /// 本 build 解码失败的原因。
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// Heap bytes directly owned by this forward-compatible wire value.
    ///
    /// The raw JSON, extracted tag and decode reason are three independent
    /// allocations. Runtime cache accounting needs all three so an unknown
    /// payload cannot bypass the document's resident-byte budget.
    pub fn estimated_heap_bytes(&self) -> usize {
        self.json
            .get()
            .len()
            .saturating_add(self.tag.capacity())
            .saturating_add(self.reason.capacity())
    }

    /// 占位块展示用的短标签。
    pub fn placeholder_label(&self) -> String {
        if self.tag.is_empty() {
            "Unsupported block".to_owned()
        } else {
            format!("Unsupported block: {}", self.tag)
        }
    }
}

/// 只比较原始字节：`reason` 随 build 变化，不参与相等性。
impl PartialEq for UnknownWire {
    fn eq(&self, other: &Self) -> bool {
        self.json.get() == other.json.get()
    }
}

impl Eq for UnknownWire {}

impl Hash for UnknownWire {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.json.get().hash(state);
    }
}

impl Serialize for UnknownWire {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.json.serialize(serializer)
    }
}

/// 取 externally tagged enum 的 tag，不做完整解析：`{"Video":…}` → `Video`，
/// `"Video"` → `Video`，其他形状 → 空串。
fn tag_of(json: &str) -> String {
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum TagProbe {
        Unit(String),
        Tagged(std::collections::BTreeMap<String, serde::de::IgnoredAny>),
    }

    match serde_json::from_str::<TagProbe>(json) {
        Ok(TagProbe::Unit(tag)) => tag,
        Ok(TagProbe::Tagged(map)) => map.into_keys().next().unwrap_or_default(),
        Err(_) => String::new(),
    }
}

/// 把任意 `Deserializer` 的输入抓成原始 JSON 字节。
///
/// `BlockPayload::Opaque` 已经用 [`RawValue`] 保存字节，因此这两个类型本来就
/// 只支持 JSON；这里沿用同一约束。
pub(crate) fn raw_json<'de, D>(deserializer: D) -> Result<Box<RawValue>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    Box::<RawValue>::deserialize(deserializer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_read_from_both_unit_and_tagged_shapes() {
        assert_eq!(tag_of("\"Video\""), "Video");
        assert_eq!(tag_of("{\"Video\":{\"source\":\"a.mp4\"}}"), "Video");
        assert_eq!(tag_of("[1,2]"), "");
    }

    #[test]
    fn serialization_writes_the_original_bytes_verbatim() {
        let raw = RawValue::from_string("{\"Video\":{ \"source\" :\"a.mp4\"}}".to_owned()).unwrap();
        let unknown = UnknownWire::new(raw, "unknown variant `Video`");

        assert_eq!(
            serde_json::to_string(&unknown).unwrap(),
            "{\"Video\":{ \"source\" :\"a.mp4\"}}"
        );
        assert_eq!(unknown.tag(), "Video");
        assert_eq!(unknown.placeholder_label(), "Unsupported block: Video");
        assert!(unknown.estimated_heap_bytes() >= unknown.json().len());
    }
}
