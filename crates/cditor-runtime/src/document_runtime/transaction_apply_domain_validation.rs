use cditor_core::edit::{CollectionPropertyValue, CollectionRecordSnapshot};
use cditor_core::rich_text::{CollectionPayload, CollectionPropertyKind, plain_text_from_spans};

use super::transaction_apply::StagingState;
use super::transaction_apply_domain::{surface_spans_snapshot, validate_range};
use super::*;

pub(super) fn require_equal<T: PartialEq>(
    current: &T,
    expected: &T,
    label: &str,
) -> Result<(), String> {
    if current == expected {
        Ok(())
    } else {
        Err(format!("{label} no longer matches before state"))
    }
}

pub(super) fn replace_plain_surface_text(
    text: &mut String,
    range: &Range<usize>,
    expected: &[InlineSpan],
    replacement: &[InlineSpan],
) -> Result<(), String> {
    validate_range(text, range)?;
    if expected.iter().any(|span| !span.marks.is_empty())
        || replacement.iter().any(|span| !span.marks.is_empty())
    {
        return Err("code and HTML text surfaces do not accept inline marks".to_owned());
    }
    let current = &text[range.clone()];
    if plain_text_from_spans(expected) != current {
        return Err("text replacement before spans no longer match before state".to_owned());
    }
    text.replace_range(range.clone(), &plain_text_from_spans(replacement));
    Ok(())
}

pub(super) fn validate_record(
    collection: &CollectionPayload,
    record: &CollectionRecordSnapshot,
) -> Result<(), String> {
    if record.collection_id != collection.collection_id {
        return Err("record belongs to a different collection".to_owned());
    }
    let mut seen = BTreeSet::new();
    for (property_id, value) in &record.values {
        if !seen.insert(*property_id) {
            return Err(format!("record repeats property {property_id}"));
        }
        let property = collection
            .properties
            .iter()
            .find(|property| property.property_id == *property_id)
            .ok_or_else(|| format!("property {property_id} does not exist"))?;
        validate_property_value(property.kind, value)?;
    }
    Ok(())
}

pub(super) fn validate_property_value(
    kind: CollectionPropertyKind,
    value: &CollectionPropertyValue,
) -> Result<(), String> {
    let valid = match (kind, value) {
        (_, CollectionPropertyValue::Empty) => true,
        (
            CollectionPropertyKind::Title | CollectionPropertyKind::Text,
            CollectionPropertyValue::RichText(_),
        ) => true,
        (CollectionPropertyKind::Number, CollectionPropertyValue::Number(number)) => {
            number.is_finite()
        }
        (CollectionPropertyKind::Checkbox, CollectionPropertyValue::Checkbox(_))
        | (
            CollectionPropertyKind::Select | CollectionPropertyKind::MultiSelect,
            CollectionPropertyValue::Select(_),
        )
        | (
            CollectionPropertyKind::Date
            | CollectionPropertyKind::CreatedTime
            | CollectionPropertyKind::UpdatedTime,
            CollectionPropertyValue::Date { .. },
        )
        | (CollectionPropertyKind::Url, CollectionPropertyValue::Url(_))
        | (CollectionPropertyKind::Email, CollectionPropertyValue::Email(_))
        | (CollectionPropertyKind::Phone, CollectionPropertyValue::Phone(_)) => true,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!("property value is incompatible with {kind:?}"))
    }
}

pub(super) fn validate_comment_anchor(
    runtime: &DocumentRuntime,
    staging: &mut StagingState,
    anchor: &cditor_core::edit::CommentAnchor,
) -> Result<(), String> {
    let spans = surface_spans_snapshot(runtime, staging, anchor.surface_id)?;
    let text = plain_text_from_spans(&spans);
    validate_range(&text, &anchor.range)?;
    if text[anchor.range.clone()] != anchor.quoted_text {
        return Err("comment quoted text no longer matches its anchor".to_owned());
    }
    Ok(())
}

pub(super) fn current_thread(
    runtime: &DocumentRuntime,
    staging: &StagingState,
    thread_id: CommentThreadId,
) -> Option<CommentThreadSnapshot> {
    staging
        .comment_threads
        .get(&thread_id)
        .cloned()
        .unwrap_or_else(|| runtime.document.comment_threads.get(&thread_id).cloned())
}

pub(super) fn require_thread(
    runtime: &DocumentRuntime,
    staging: &StagingState,
    thread_id: CommentThreadId,
) -> Result<CommentThreadSnapshot, String> {
    current_thread(runtime, staging, thread_id)
        .ok_or_else(|| format!("comment thread {thread_id} does not exist"))
}

pub(super) fn validate_unique_comment_ids(
    messages: &[cditor_core::edit::CommentMessageSnapshot],
) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    if let Some(duplicate) = messages
        .iter()
        .map(|message| message.comment_id)
        .find(|comment_id| !ids.insert(*comment_id))
    {
        return Err(format!("comment {duplicate} is duplicated in thread"));
    }
    Ok(())
}

pub(super) fn mark_anchor_dirty(
    staging: &mut StagingState,
    anchor: &cditor_core::edit::CommentAnchor,
) {
    if let Some(block_id) = anchor.surface_id.block_id() {
        staging.mark_dirty(block_id);
    }
}

pub(super) fn current_asset(
    runtime: &DocumentRuntime,
    staging: &StagingState,
    asset_id: AssetId,
) -> Option<AssetSnapshot> {
    staging
        .assets
        .get(&asset_id)
        .cloned()
        .unwrap_or_else(|| runtime.document.assets.get(&asset_id).cloned())
}
