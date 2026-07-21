//! Typed domain operations applied to the transaction staging state.
//!
//! Every operation validates its carried `before` value before changing the
//! copy-on-touch overlay. This makes stale SDK, AI, journal, and future sync
//! operations fail atomically instead of overwriting newer local state.

use cditor_core::document::{BLOCK_FLAG_FOLDED, BLOCK_FLAG_LOCKED};
use cditor_core::edit::{
    AssetEditOperation, BlockEditOperation, CollectionEditOperation, CollectionPropertyValue,
    CollectionRecordSnapshot, CommentEditOperation, TextEditOperation,
};
use cditor_core::rich_text::{
    BlockPayload, CollectionPayload, InlineSpan, kind_tag_for_rich_block_kind,
    plain_text_from_spans, splice_spans_at_range,
};

use super::transaction_apply::StagingState;
use super::transaction_apply_domain_validation::{
    current_asset, current_thread, mark_anchor_dirty, replace_plain_surface_text, require_equal,
    require_thread, validate_comment_anchor, validate_property_value, validate_record,
    validate_unique_comment_ids,
};
use super::transaction_apply_structure::position_of;
use super::*;

pub(super) fn apply_text_operation(
    runtime: &DocumentRuntime,
    staging: &mut StagingState,
    operation: &TextEditOperation,
) -> Result<(), String> {
    match operation {
        TextEditOperation::ReplaceSpans {
            surface_id,
            range,
            old_spans,
            new_spans,
        } => {
            if let SurfaceId::Block(block_id) = surface_id {
                let record = runtime.staged_payload(staging, *block_id)?;
                match &mut record.payload {
                    BlockPayload::Code { text, .. } => {
                        return replace_plain_surface_text(text, range, old_spans, new_spans);
                    }
                    BlockPayload::Html { html, sanitized } => {
                        *sanitized = false;
                        return replace_plain_surface_text(html, range, old_spans, new_spans);
                    }
                    BlockPayload::RichText { .. } => {}
                    _ => {
                        return Err(format!("block {block_id} is not an editable text surface"));
                    }
                }
            }
            let spans = surface_spans_mut(runtime, staging, *surface_id)?;
            validate_span_replacement(spans, range, old_spans)?;
            *spans = splice_spans_at_range(spans, range.clone(), new_spans.clone());
            merge_inline_spans(spans);
            Ok(())
        }
    }
}

pub(super) fn apply_block_operation(
    runtime: &DocumentRuntime,
    staging: &mut StagingState,
    operation: &BlockEditOperation,
) -> Result<(), String> {
    match operation {
        BlockEditOperation::ReplacePayload {
            block_id,
            before_kind,
            before_payload,
            after_kind,
            after_payload,
        } => {
            let position = position_of(&staging.records, *block_id)
                .ok_or_else(|| format!("block {block_id} does not exist"))?;
            {
                let record = runtime.staged_payload(staging, *block_id)?;
                if record.kind != *before_kind || record.payload != *before_payload {
                    return Err(format!(
                        "block {block_id} payload no longer matches before state"
                    ));
                }
                record.kind = after_kind.clone();
                record.payload = after_payload.clone();
            }
            let next_kind_tag = kind_tag_for_rich_block_kind(after_kind);
            let height_estimate =
                estimate_block_height(after_kind, after_payload, DEFAULT_LAYOUT_WIDTH_PX);
            let layout = &mut staging.records[position].layout_meta;
            layout.estimated_height = height_estimate.height;
            layout.measured_height = None;
            layout.dirty = true;
            if staging.records[position].kind_tag != next_kind_tag {
                staging.records[position].kind_tag = next_kind_tag;
                staging.structure_changed = true;
            }
            Ok(())
        }
        BlockEditOperation::SetAttrs {
            block_id,
            before,
            after,
        } => {
            let position = position_of(&staging.records, *block_id)
                .ok_or_else(|| format!("block {block_id} does not exist"))?;
            let current = staging
                .block_attrs
                .get(block_id)
                .cloned()
                .unwrap_or_else(|| runtime.block_attrs(*block_id));
            if current != *before {
                return Err(format!(
                    "block {block_id} attrs no longer match before state"
                ));
            }

            // Requiring the payload to be hydrated keeps content/layout version
            // updates in the same owner and prevents an attrs edit from creating
            // a second version source for unloaded blocks.
            runtime.staged_payload(staging, *block_id)?;
            staging.block_attrs.insert(*block_id, after.clone());
            let record = &mut staging.records[position];
            set_flag(&mut record.flags, BLOCK_FLAG_FOLDED, after.folded);
            set_flag(&mut record.flags, BLOCK_FLAG_LOCKED, after.locked);
            if before.folded != after.folded || before.locked != after.locked {
                staging.structure_changed = true;
            }
            Ok(())
        }
    }
}

pub(super) fn apply_collection_operation(
    runtime: &DocumentRuntime,
    staging: &mut StagingState,
    operation: &CollectionEditOperation,
) -> Result<(), String> {
    let block_id = operation.block_id();
    match operation {
        CollectionEditOperation::SetTitle {
            collection_id,
            before,
            after,
            ..
        } => {
            let collection = collection_mut(runtime, staging, block_id, *collection_id)?;
            require_equal(&collection.title, before, "collection title")?;
            collection.title = after.clone();
        }
        CollectionEditOperation::InsertProperty {
            collection_id,
            index,
            property,
            ..
        } => {
            let collection = collection_mut(runtime, staging, block_id, *collection_id)?;
            if *index > collection.properties.len() {
                return Err(format!("property insertion index {index} is out of bounds"));
            }
            if collection
                .properties
                .iter()
                .any(|candidate| candidate.property_id == property.property_id)
            {
                return Err(format!("property {} already exists", property.property_id));
            }
            collection.properties.insert(*index, property.clone());
            bump_schema_version(collection);
        }
        CollectionEditOperation::DeleteProperty {
            collection_id,
            index,
            property,
            ..
        } => {
            let collection = collection_mut(runtime, staging, block_id, *collection_id)?;
            let current = collection
                .properties
                .get(*index)
                .ok_or_else(|| format!("property deletion index {index} is out of bounds"))?;
            require_equal(current, property, "deleted property")?;
            collection.properties.remove(*index);
            bump_schema_version(collection);
        }
        CollectionEditOperation::UpdateProperty {
            collection_id,
            before,
            after,
            ..
        } => {
            if before.property_id != after.property_id {
                return Err("property update cannot change stable property id".to_owned());
            }
            let collection = collection_mut(runtime, staging, block_id, *collection_id)?;
            let current = collection
                .properties
                .iter_mut()
                .find(|property| property.property_id == before.property_id)
                .ok_or_else(|| format!("property {} does not exist", before.property_id))?;
            require_equal(current, before, "updated property")?;
            *current = after.clone();
            bump_schema_version(collection);
        }
        CollectionEditOperation::InsertView {
            collection_id,
            index,
            view,
            ..
        } => {
            let collection = collection_mut(runtime, staging, block_id, *collection_id)?;
            if *index > collection.views.len() {
                return Err(format!("view insertion index {index} is out of bounds"));
            }
            if collection
                .views
                .iter()
                .any(|candidate| candidate.view_id == view.view_id)
            {
                return Err(format!("view {} already exists", view.view_id));
            }
            collection.views.insert(*index, view.clone());
            bump_schema_version(collection);
        }
        CollectionEditOperation::DeleteView {
            collection_id,
            index,
            view,
            ..
        } => {
            let collection = collection_mut(runtime, staging, block_id, *collection_id)?;
            let current = collection
                .views
                .get(*index)
                .ok_or_else(|| format!("view deletion index {index} is out of bounds"))?;
            require_equal(current, view, "deleted view")?;
            if collection.active_view_id == Some(view.view_id) {
                return Err("active view must be changed before it can be deleted".to_owned());
            }
            collection.views.remove(*index);
            bump_schema_version(collection);
        }
        CollectionEditOperation::UpdateView {
            collection_id,
            before,
            after,
            ..
        } => {
            if before.view_id != after.view_id {
                return Err("view update cannot change stable view id".to_owned());
            }
            let collection = collection_mut(runtime, staging, block_id, *collection_id)?;
            let current = collection
                .views
                .iter_mut()
                .find(|view| view.view_id == before.view_id)
                .ok_or_else(|| format!("view {} does not exist", before.view_id))?;
            require_equal(current, before, "updated view")?;
            *current = after.clone();
            bump_schema_version(collection);
        }
        CollectionEditOperation::InsertRecord { index, record, .. } => {
            let collection = collection_snapshot(runtime, staging, block_id)?;
            validate_record(&collection, record)?;
            let records = collection_records_mut(runtime, staging, record.collection_id);
            if *index > records.len() {
                return Err(format!("record insertion index {index} is out of bounds"));
            }
            if records
                .iter()
                .any(|candidate| candidate.record_id == record.record_id)
            {
                return Err(format!("record {} already exists", record.record_id));
            }
            records.insert(*index, record.clone());
            staging.mark_dirty(block_id);
        }
        CollectionEditOperation::DeleteRecord { index, record, .. } => {
            let collection = collection_snapshot(runtime, staging, block_id)?;
            if collection.collection_id != record.collection_id {
                return Err("record belongs to a different collection".to_owned());
            }
            let records = collection_records_mut(runtime, staging, record.collection_id);
            let current = records
                .get(*index)
                .ok_or_else(|| format!("record deletion index {index} is out of bounds"))?;
            require_equal(current, record, "deleted record")?;
            records.remove(*index);
            staging.mark_dirty(block_id);
        }
        CollectionEditOperation::SetRecordValue {
            collection_id,
            record_id,
            property_id,
            before,
            after,
            ..
        } => {
            let collection = collection_snapshot(runtime, staging, block_id)?;
            if collection.collection_id != *collection_id {
                return Err("record value targets a different collection".to_owned());
            }
            let property = collection
                .properties
                .iter()
                .find(|property| property.property_id == *property_id)
                .ok_or_else(|| format!("property {property_id} does not exist"))?;
            validate_property_value(property.kind, after)?;
            let records = collection_records_mut(runtime, staging, *collection_id);
            let record = records
                .iter_mut()
                .find(|record| record.record_id == *record_id)
                .ok_or_else(|| format!("record {record_id} does not exist"))?;
            let position = record
                .values
                .iter()
                .position(|(candidate, _)| candidate == property_id);
            let current = position
                .and_then(|position| record.values.get(position).map(|(_, value)| value))
                .cloned()
                .unwrap_or(CollectionPropertyValue::Empty);
            require_equal(&current, before, "record property value")?;
            match (position, after) {
                (Some(position), CollectionPropertyValue::Empty) => {
                    record.values.remove(position);
                }
                (Some(position), value) => record.values[position].1 = value.clone(),
                (None, CollectionPropertyValue::Empty) => {}
                (None, value) => record.values.push((*property_id, value.clone())),
            }
            staging.mark_dirty(block_id);
        }
        CollectionEditOperation::SetActiveView {
            collection_id,
            before,
            after,
            ..
        } => {
            let collection = collection_mut(runtime, staging, block_id, *collection_id)?;
            if collection.active_view_id != Some(*before) {
                return Err(format!(
                    "active view is {:?}, expected {before}",
                    collection.active_view_id
                ));
            }
            if !collection.views.iter().any(|view| view.view_id == *after) {
                return Err(format!("view {after} does not exist"));
            }
            collection.active_view_id = Some(*after);
        }
    }
    Ok(())
}

pub(super) fn apply_comment_operation(
    runtime: &DocumentRuntime,
    staging: &mut StagingState,
    operation: &CommentEditOperation,
) -> Result<(), String> {
    match operation {
        CommentEditOperation::CreateThread { thread } => {
            if current_thread(runtime, staging, thread.thread_id).is_some() {
                return Err(format!(
                    "comment thread {} already exists",
                    thread.thread_id
                ));
            }
            validate_comment_anchor(runtime, staging, &thread.anchor)?;
            validate_unique_comment_ids(&thread.messages)?;
            staging
                .comment_threads
                .insert(thread.thread_id, Some(thread.clone()));
            mark_anchor_dirty(staging, &thread.anchor);
        }
        CommentEditOperation::DeleteThread { thread } => {
            let current = current_thread(runtime, staging, thread.thread_id)
                .ok_or_else(|| format!("comment thread {} does not exist", thread.thread_id))?;
            require_equal(&current, thread, "deleted comment thread")?;
            staging.comment_threads.insert(thread.thread_id, None);
            mark_anchor_dirty(staging, &thread.anchor);
        }
        CommentEditOperation::AddMessage {
            thread_id,
            index,
            message,
        } => {
            let mut thread = require_thread(runtime, staging, *thread_id)?;
            if *index > thread.messages.len() {
                return Err(format!("comment insertion index {index} is out of bounds"));
            }
            if thread
                .messages
                .iter()
                .any(|candidate| candidate.comment_id == message.comment_id)
            {
                return Err(format!("comment {} already exists", message.comment_id));
            }
            thread.messages.insert(*index, message.clone());
            mark_anchor_dirty(staging, &thread.anchor);
            staging.comment_threads.insert(*thread_id, Some(thread));
        }
        CommentEditOperation::DeleteMessage {
            thread_id,
            index,
            message,
        } => {
            let mut thread = require_thread(runtime, staging, *thread_id)?;
            let current = thread
                .messages
                .get(*index)
                .ok_or_else(|| format!("comment deletion index {index} is out of bounds"))?;
            require_equal(current, message, "deleted comment")?;
            thread.messages.remove(*index);
            mark_anchor_dirty(staging, &thread.anchor);
            staging.comment_threads.insert(*thread_id, Some(thread));
        }
        CommentEditOperation::UpdateMessage {
            thread_id,
            before,
            after,
        } => {
            if before.comment_id != after.comment_id
                || before.author_id != after.author_id
                || before.created_at_ms != after.created_at_ms
            {
                return Err("comment update cannot change stable identity fields".to_owned());
            }
            let mut thread = require_thread(runtime, staging, *thread_id)?;
            let current = thread
                .messages
                .iter_mut()
                .find(|message| message.comment_id == before.comment_id)
                .ok_or_else(|| format!("comment {} does not exist", before.comment_id))?;
            require_equal(current, before, "updated comment")?;
            *current = after.clone();
            mark_anchor_dirty(staging, &thread.anchor);
            staging.comment_threads.insert(*thread_id, Some(thread));
        }
        CommentEditOperation::SetResolved {
            thread_id,
            before,
            after,
        } => {
            let mut thread = require_thread(runtime, staging, *thread_id)?;
            if thread.resolved != *before {
                return Err(format!(
                    "comment resolved state is stale for thread {thread_id}"
                ));
            }
            thread.resolved = *after;
            mark_anchor_dirty(staging, &thread.anchor);
            staging.comment_threads.insert(*thread_id, Some(thread));
        }
        CommentEditOperation::MoveAnchor {
            thread_id,
            before,
            after,
        } => {
            let mut thread = require_thread(runtime, staging, *thread_id)?;
            require_equal(&thread.anchor, before, "comment anchor")?;
            validate_comment_anchor(runtime, staging, after)?;
            mark_anchor_dirty(staging, &thread.anchor);
            mark_anchor_dirty(staging, after);
            thread.anchor = after.clone();
            staging.comment_threads.insert(*thread_id, Some(thread));
        }
    }
    Ok(())
}

pub(super) fn apply_asset_operation(
    runtime: &DocumentRuntime,
    staging: &mut StagingState,
    operation: &AssetEditOperation,
) -> Result<(), String> {
    let block_id = operation.block_id();
    if position_of(&staging.records, block_id).is_none() || staging.deleted.contains(&block_id) {
        return Err(format!("block {block_id} does not exist"));
    }
    let mut attached = staging
        .block_asset_ids
        .get(&block_id)
        .cloned()
        .unwrap_or_else(|| {
            runtime
                .block_asset_ids
                .get(&block_id)
                .cloned()
                .unwrap_or_default()
        });
    match operation {
        AssetEditOperation::Attach { asset, .. } => {
            if attached.contains(&asset.asset_id) {
                return Err(format!("asset {} is already attached", asset.asset_id));
            }
            if let Some(current) = current_asset(runtime, staging, asset.asset_id)
                && current != *asset
            {
                return Err(format!("asset {} has conflicting metadata", asset.asset_id));
            }
            attached.insert(asset.asset_id);
            staging.assets.insert(asset.asset_id, Some(asset.clone()));
        }
        AssetEditOperation::Detach { asset, .. } => {
            let current = current_asset(runtime, staging, asset.asset_id)
                .ok_or_else(|| format!("asset {} does not exist", asset.asset_id))?;
            require_equal(&current, asset, "detached asset")?;
            if !attached.remove(&asset.asset_id) {
                return Err(format!("asset {} is not attached", asset.asset_id));
            }
        }
        AssetEditOperation::Update { before, after, .. } => {
            if before.asset_id != after.asset_id {
                return Err("asset update cannot change stable asset id".to_owned());
            }
            let current = current_asset(runtime, staging, before.asset_id)
                .ok_or_else(|| format!("asset {} does not exist", before.asset_id))?;
            require_equal(&current, before, "updated asset")?;
            if !attached.contains(&before.asset_id) {
                return Err(format!(
                    "asset {} is not attached to block",
                    before.asset_id
                ));
            }
            staging.assets.insert(after.asset_id, Some(after.clone()));
        }
    }
    staging.block_asset_ids.insert(block_id, attached);
    staging.mark_dirty(block_id);
    Ok(())
}

fn surface_spans_mut<'a>(
    runtime: &DocumentRuntime,
    staging: &'a mut StagingState,
    surface_id: SurfaceId,
) -> Result<&'a mut Vec<InlineSpan>, String> {
    let block_id = surface_id
        .block_id()
        .ok_or_else(|| "ephemeral text surfaces cannot be persisted".to_owned())?;
    let record = runtime.staged_payload(staging, block_id)?;
    match surface_id {
        SurfaceId::Block(_) => match &mut record.payload {
            BlockPayload::RichText { spans } => Ok(spans),
            _ => Err(format!("block {block_id} is not a rich-text surface")),
        },
        SurfaceId::TableCell { row, column, .. } => {
            let BlockPayload::Table(table) = &mut record.payload else {
                return Err(format!("block {block_id} is not a table"));
            };
            let (origin_row, origin_col) = table
                .cell_origin(row, column)
                .ok_or_else(|| format!("table cell ({row}, {column}) does not exist"))?;
            Ok(&mut table.rows[origin_row].cells[origin_col].spans)
        }
        SurfaceId::ImageCaption { .. } => match &mut record.payload {
            BlockPayload::Image(image) => Ok(&mut image.caption.spans),
            _ => Err(format!("block {block_id} is not an image")),
        },
        SurfaceId::CollectionTitle { .. } => match &mut record.payload {
            BlockPayload::Collection(collection) => Ok(&mut collection.title.spans),
            _ => Err(format!("block {block_id} is not a collection")),
        },
        SurfaceId::Ephemeral { .. } => unreachable!("checked above"),
    }
}

pub(super) fn surface_spans_snapshot(
    runtime: &DocumentRuntime,
    staging: &mut StagingState,
    surface_id: SurfaceId,
) -> Result<Vec<InlineSpan>, String> {
    let block_id = surface_id
        .block_id()
        .ok_or_else(|| "comments cannot anchor to an ephemeral surface".to_owned())?;
    let record = runtime.staged_payload_read(staging, block_id)?;
    match surface_id {
        SurfaceId::Block(_) => match &record.payload {
            BlockPayload::RichText { spans } => Ok(spans.clone()),
            _ => Err(format!("block {block_id} is not a rich-text surface")),
        },
        SurfaceId::TableCell { row, column, .. } => {
            let BlockPayload::Table(table) = &record.payload else {
                return Err(format!("block {block_id} is not a table"));
            };
            let (origin_row, origin_col) = table
                .cell_origin(row, column)
                .ok_or_else(|| format!("table cell ({row}, {column}) does not exist"))?;
            Ok(table.rows[origin_row].cells[origin_col].spans.clone())
        }
        SurfaceId::ImageCaption { .. } => match &record.payload {
            BlockPayload::Image(image) => Ok(image.caption.spans.clone()),
            _ => Err(format!("block {block_id} is not an image")),
        },
        SurfaceId::CollectionTitle { .. } => match &record.payload {
            BlockPayload::Collection(collection) => Ok(collection.title.spans.clone()),
            _ => Err(format!("block {block_id} is not a collection")),
        },
        SurfaceId::Ephemeral { .. } => unreachable!("checked above"),
    }
}

fn validate_span_replacement(
    spans: &[InlineSpan],
    range: &Range<usize>,
    expected: &[InlineSpan],
) -> Result<(), String> {
    let text = plain_text_from_spans(spans);
    validate_range(&text, range)?;
    let current = if range.is_empty() {
        Vec::new()
    } else {
        slice_rich_text_spans(spans, range.clone())
    };
    require_equal(
        &current,
        &expected.to_vec(),
        "text replacement before spans",
    )
}

pub(super) fn validate_range(text: &str, range: &Range<usize>) -> Result<(), String> {
    if range.start > range.end || range.end > text.len() {
        return Err(format!(
            "text range {range:?} is invalid for {} bytes",
            text.len()
        ));
    }
    if !text.is_char_boundary(range.start) || !text.is_char_boundary(range.end) {
        return Err(format!("text range {range:?} is not on UTF-8 boundaries"));
    }
    Ok(())
}

fn collection_mut<'a>(
    runtime: &DocumentRuntime,
    staging: &'a mut StagingState,
    block_id: BlockId,
    collection_id: CollectionId,
) -> Result<&'a mut CollectionPayload, String> {
    let record = runtime.staged_payload(staging, block_id)?;
    let BlockPayload::Collection(collection) = &mut record.payload else {
        return Err(format!("block {block_id} is not a collection"));
    };
    if collection.collection_id != collection_id {
        return Err(format!(
            "collection id is {}, expected {collection_id}",
            collection.collection_id
        ));
    }
    Ok(collection)
}

fn collection_snapshot(
    runtime: &DocumentRuntime,
    staging: &mut StagingState,
    block_id: BlockId,
) -> Result<CollectionPayload, String> {
    let record = runtime.staged_payload_read(staging, block_id)?;
    match &record.payload {
        BlockPayload::Collection(collection) => Ok(collection.clone()),
        _ => Err(format!("block {block_id} is not a collection")),
    }
}

fn collection_records_mut<'a>(
    runtime: &DocumentRuntime,
    staging: &'a mut StagingState,
    collection_id: CollectionId,
) -> &'a mut Vec<CollectionRecordSnapshot> {
    staging
        .collection_records
        .entry(collection_id)
        .or_insert_with(|| {
            runtime
                .collection_records
                .get(&collection_id)
                .cloned()
                .unwrap_or_default()
        })
}

fn bump_schema_version(collection: &mut CollectionPayload) {
    collection.schema_version = collection.schema_version.saturating_add(1);
}

fn set_flag(flags: &mut u32, flag: u32, enabled: bool) {
    if enabled {
        *flags |= flag;
    } else {
        *flags &= !flag;
    }
}
