use std::collections::{HashMap, VecDeque};

use crate::{
    canvas::CanvasDocument,
    shapes::{Shape, ShapeId},
};

use super::DrafftBoard;

const MAX_HISTORY_ENTRIES: usize = 100;
const MAX_HISTORY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Default)]
struct PendingChange {
    before_shapes: HashMap<ShapeId, Option<Shape>>,
    before_order: Option<Vec<ShapeId>>,
    estimated_bytes: usize,
}

impl PendingChange {
    fn capture_shapes(
        &mut self,
        document: &CanvasDocument,
        ids: impl IntoIterator<Item = ShapeId>,
    ) -> bool {
        for id in ids {
            if self.before_shapes.contains_key(&id) {
                continue;
            }
            let shape = document.get_shape(id);
            let shape_bytes = serialized_shape_bytes(shape);
            if self.estimated_bytes.saturating_add(shape_bytes) > MAX_HISTORY_BYTES {
                return false;
            }
            self.estimated_bytes += shape_bytes;
            self.before_shapes.insert(id, shape.cloned());
        }
        true
    }

    fn capture_created(&mut self, ids: impl IntoIterator<Item = ShapeId>) {
        for id in ids {
            self.before_shapes.entry(id).or_insert(None);
        }
    }

    fn capture_order(&mut self, document: &CanvasDocument) -> bool {
        if self.before_order.is_some() {
            return true;
        }
        let order_bytes = document.z_order.len() * std::mem::size_of::<ShapeId>();
        if self.estimated_bytes.saturating_add(order_bytes) > MAX_HISTORY_BYTES {
            return false;
        }
        self.estimated_bytes += order_bytes;
        self.before_order = Some(document.z_order.clone());
        true
    }
}

#[derive(Clone, Debug)]
struct ShapeChange {
    id: ShapeId,
    before: Option<Shape>,
    after: Option<Shape>,
}

#[derive(Clone, Debug)]
struct HistoryEntry {
    changes: Vec<ShapeChange>,
    order: Option<(Vec<ShapeId>, Vec<ShapeId>)>,
    estimated_bytes: usize,
}

impl HistoryEntry {
    fn finish(pending: PendingChange, document: &CanvasDocument) -> Option<Self> {
        let mut changes = Vec::with_capacity(pending.before_shapes.len());
        for (id, before) in pending.before_shapes {
            let after = document.get_shape(id).cloned();
            if shapes_match(before.as_ref(), after.as_ref()) {
                continue;
            }
            changes.push(ShapeChange { id, before, after });
        }

        let order = pending.before_order.and_then(|before| {
            (before != document.z_order).then(|| (before, document.z_order.clone()))
        });
        if changes.is_empty() && order.is_none() {
            return None;
        }

        let estimated_bytes = changes
            .iter()
            .map(|change| {
                serialized_shape_bytes(change.before.as_ref())
                    + serialized_shape_bytes(change.after.as_ref())
            })
            .sum::<usize>()
            + order.as_ref().map_or(0, |(before, after)| {
                (before.len() + after.len()) * std::mem::size_of::<ShapeId>()
            });
        Some(Self {
            changes,
            order,
            estimated_bytes,
        })
    }

    fn apply_before(&self, document: &mut CanvasDocument) {
        apply_changes(document, &self.changes, true);
        if let Some((before, _)) = &self.order {
            document.z_order.clone_from(before);
        }
    }

    fn apply_after(&self, document: &mut CanvasDocument) {
        apply_changes(document, &self.changes, false);
        if let Some((_, after)) = &self.order {
            document.z_order.clone_from(after);
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct DocumentHistory {
    undo: VecDeque<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    pending: Option<PendingChange>,
    pending_rejected: bool,
    retained_bytes: usize,
}

impl DocumentHistory {
    pub(super) fn begin(&mut self) {
        if !self.pending_rejected {
            self.pending.get_or_insert_with(PendingChange::default);
        }
    }

    pub(super) fn capture_shapes(
        &mut self,
        document: &CanvasDocument,
        ids: impl IntoIterator<Item = ShapeId>,
    ) {
        if self.pending_rejected {
            return;
        }
        let accepted = self
            .pending
            .get_or_insert_with(PendingChange::default)
            .capture_shapes(document, ids);
        if !accepted {
            self.clear();
            self.pending_rejected = true;
        }
    }

    pub(super) fn capture_created(&mut self, ids: impl IntoIterator<Item = ShapeId>) {
        if self.pending_rejected {
            return;
        }
        self.pending
            .get_or_insert_with(PendingChange::default)
            .capture_created(ids);
    }

    pub(super) fn capture_order(&mut self, document: &CanvasDocument) {
        if self.pending_rejected {
            return;
        }
        let accepted = self
            .pending
            .get_or_insert_with(PendingChange::default)
            .capture_order(document);
        if !accepted {
            self.clear();
            self.pending_rejected = true;
        }
    }

    pub(super) fn commit(&mut self, document: &CanvasDocument) {
        if std::mem::take(&mut self.pending_rejected) {
            return;
        }
        let Some(pending) = self.pending.take() else {
            return;
        };
        let after_bytes = pending
            .before_shapes
            .keys()
            .map(|id| serialized_shape_bytes(document.get_shape(*id)))
            .sum::<usize>()
            + pending.before_order.as_ref().map_or(0, |_| {
                document.z_order.len() * std::mem::size_of::<ShapeId>()
            });
        if pending.estimated_bytes.saturating_add(after_bytes) > MAX_HISTORY_BYTES {
            self.clear();
            return;
        }
        let Some(entry) = HistoryEntry::finish(pending, document) else {
            return;
        };
        self.drop_redo();
        if entry.estimated_bytes > MAX_HISTORY_BYTES {
            self.clear();
            return;
        }
        self.retained_bytes += entry.estimated_bytes;
        self.undo.push_back(entry);
        while self.undo.len() > MAX_HISTORY_ENTRIES || self.retained_bytes > MAX_HISTORY_BYTES {
            let Some(removed) = self.undo.pop_front() else {
                break;
            };
            self.retained_bytes = self.retained_bytes.saturating_sub(removed.estimated_bytes);
        }
    }

    pub(super) fn cancel_pending(&mut self) {
        self.pending = None;
        self.pending_rejected = false;
    }

    pub(super) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(super) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(super) fn undo(&mut self, document: &mut CanvasDocument) -> bool {
        let Some(entry) = self.undo.pop_back() else {
            return false;
        };
        entry.apply_before(document);
        self.redo.push(entry);
        true
    }

    pub(super) fn redo(&mut self, document: &mut CanvasDocument) -> bool {
        let Some(entry) = self.redo.pop() else {
            return false;
        };
        entry.apply_after(document);
        self.undo.push_back(entry);
        true
    }

    pub(super) fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.pending = None;
        self.pending_rejected = false;
        self.retained_bytes = 0;
    }

    fn drop_redo(&mut self) {
        for entry in self.redo.drain(..) {
            self.retained_bytes = self.retained_bytes.saturating_sub(entry.estimated_bytes);
        }
    }
}

impl DrafftBoard {
    pub(crate) fn document_revision(&self) -> u64 {
        self.document_revision
    }

    pub(crate) fn mark_document_changed(&mut self) {
        self.history.commit(&self.canvas.document);
        self.bump_document_revision();
    }

    pub(crate) fn bump_document_revision(&mut self) {
        self.document_revision = self.document_revision.wrapping_add(1);
    }

    pub(crate) fn begin_history_transaction(&mut self) {
        let selected = self.canvas.selection.clone();
        self.history.begin();
        self.history.capture_shapes(&self.canvas.document, selected);
    }

    pub(crate) fn capture_history_shapes(&mut self, ids: impl IntoIterator<Item = ShapeId>) {
        self.history.capture_shapes(&self.canvas.document, ids);
    }

    pub(crate) fn capture_created_shapes(&mut self, ids: impl IntoIterator<Item = ShapeId>) {
        self.history.capture_created(ids);
        self.history.capture_order(&self.canvas.document);
    }

    pub(crate) fn capture_history_order(&mut self) {
        self.history.capture_order(&self.canvas.document);
    }

    pub(crate) fn begin_order_history_transaction(&mut self) {
        self.history.begin();
        self.history.capture_order(&self.canvas.document);
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo(&mut self) -> bool {
        self.cancel_pointer();
        self.canvas.clear_selection();
        self.history.cancel_pending();
        let changed = self.history.undo(&mut self.canvas.document);
        if changed {
            self.bump_document_revision();
        }
        changed
    }

    pub fn redo(&mut self) -> bool {
        self.cancel_pointer();
        self.canvas.clear_selection();
        self.history.cancel_pending();
        let changed = self.history.redo(&mut self.canvas.document);
        if changed {
            self.bump_document_revision();
        }
        changed
    }
}

fn shapes_match(before: Option<&Shape>, after: Option<&Shape>) -> bool {
    match (before, after) {
        (None, None) => true,
        (Some(before), Some(after)) => {
            serde_json::to_vec(before).ok() == serde_json::to_vec(after).ok()
        }
        _ => false,
    }
}

fn serialized_shape_bytes(shape: Option<&Shape>) -> usize {
    let Some(shape) = shape else {
        return 0;
    };
    let mut counter = ByteCounter::default();
    serde_json::to_writer(&mut counter, shape).map_or(0, |()| counter.bytes)
}

#[derive(Default)]
struct ByteCounter {
    bytes: usize,
}

impl std::io::Write for ByteCounter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.bytes = self.bytes.saturating_add(buffer.len());
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn apply_changes(document: &mut CanvasDocument, changes: &[ShapeChange], before: bool) {
    for change in changes {
        let shape = if before {
            change.before.as_ref()
        } else {
            change.after.as_ref()
        };
        if let Some(shape) = shape {
            document.shapes.insert(change.id, shape.clone());
        } else {
            document.shapes.remove(&change.id);
        }
    }
}
