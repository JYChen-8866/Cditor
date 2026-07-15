use std::collections::BTreeMap;

use cditor_core::edit::EditTransaction;
use cditor_core::rich_text::{BlockAttrs, TextAlign};
use serde::{Deserialize, Serialize};

use crate::error::serialization_error;

#[derive(Serialize, Deserialize)]
struct StoredBlockAttrs {
    color: Option<String>,
    background_color: Option<String>,
    text_align: StoredTextAlign,
    indent: u16,
    folded: bool,
    locked: bool,
    custom: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize)]
enum StoredTextAlign {
    Start,
    Center,
    End,
}

pub(crate) fn encode_attrs(attrs: &BlockAttrs) -> Result<String, cditor_storage::StorageError> {
    let stored = StoredBlockAttrs {
        color: attrs.color.clone(),
        background_color: attrs.background_color.clone(),
        text_align: match attrs.text_align {
            TextAlign::Start => StoredTextAlign::Start,
            TextAlign::Center => StoredTextAlign::Center,
            TextAlign::End => StoredTextAlign::End,
        },
        indent: attrs.indent,
        folded: attrs.folded,
        locked: attrs.locked,
        custom: attrs.custom.clone(),
    };
    serde_json::to_string(&stored).map_err(serialization_error)
}

pub(crate) fn decode_attrs(value: &str) -> Result<BlockAttrs, cditor_storage::StorageError> {
    let stored: StoredBlockAttrs = serde_json::from_str(value).map_err(serialization_error)?;
    Ok(BlockAttrs {
        color: stored.color,
        background_color: stored.background_color,
        text_align: match stored.text_align {
            StoredTextAlign::Start => TextAlign::Start,
            StoredTextAlign::Center => TextAlign::Center,
            StoredTextAlign::End => TextAlign::End,
        },
        indent: stored.indent,
        folded: stored.folded,
        locked: stored.locked,
        custom: stored.custom,
    })
}

pub(crate) fn encode_transaction(
    transaction: &EditTransaction,
) -> Result<String, cditor_storage::StorageError> {
    serde_json::to_string(transaction).map_err(serialization_error)
}
