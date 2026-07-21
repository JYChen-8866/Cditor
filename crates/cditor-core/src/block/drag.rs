use crate::ids::BlockId;

pub const GUTTER_DRAG_THRESHOLD_PX: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragPoint {
    pub x: f32,
    pub y: f32,
}

impl DragPoint {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockDropTarget {
    pub insert_before_block_id: Option<BlockId>,
    pub target_visible_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GutterBlockDragState {
    pub block_id: BlockId,
    pub start_position: DragPoint,
    pub current_position: DragPoint,
    pub exceeded_threshold: bool,
    pub target: Option<BlockDropTarget>,
}

impl GutterBlockDragState {
    pub const fn new(block_id: BlockId, start_position: DragPoint) -> Self {
        Self {
            block_id,
            start_position,
            current_position: start_position,
            exceeded_threshold: false,
            target: None,
        }
    }

    pub fn update_position(&mut self, position: DragPoint) -> bool {
        self.current_position = position;
        let exceeded = gutter_drag_exceeded_threshold(self.start_position, position);
        let changed = self.exceeded_threshold != exceeded;
        self.exceeded_threshold = exceeded;
        changed
    }
}

pub fn gutter_drag_exceeded_threshold(start: DragPoint, current: DragPoint) -> bool {
    let dx = current.x - start.x;
    let dy = current.y - start.y;
    dx.hypot(dy) >= GUTTER_DRAG_THRESHOLD_PX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutter_drag_threshold_matches_v1_contract() {
        assert!(!gutter_drag_exceeded_threshold(
            DragPoint::new(0.0, 0.0),
            DragPoint::new(3.0, 0.0),
        ));
        assert!(gutter_drag_exceeded_threshold(
            DragPoint::new(0.0, 0.0),
            DragPoint::new(4.0, 0.0),
        ));
        assert!(gutter_drag_exceeded_threshold(
            DragPoint::new(0.0, 0.0),
            DragPoint::new(3.0, 3.0),
        ));
    }
}
