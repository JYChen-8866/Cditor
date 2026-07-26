pub(super) mod geometry;
mod gutter_action;
pub(super) mod gutter_drag;
pub(super) mod gutter_drag_commit;
pub(super) mod gutter_drag_metrics;
pub(super) mod image_resize;
pub(super) mod scrollbar;
pub(crate) mod selection_drag;
pub(super) mod table_mode;
pub(super) mod table_reorder;
#[allow(dead_code)]
pub(super) mod table_resize;
pub(super) mod table_scroll;

fn take_drag<T>(slot: &mut Option<T>) -> Option<T> {
    slot.take()
}

#[cfg(test)]
mod tests {
    use super::take_drag;

    #[test]
    fn drag_commit_gate_consumes_a_preview_exactly_once() {
        let mut drag = Some(7);

        assert_eq!(take_drag(&mut drag), Some(7));
        assert_eq!(take_drag(&mut drag), None);
    }
}
