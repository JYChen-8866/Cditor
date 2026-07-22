use std::time::Duration;

use cditor_core::edit::DocumentSelection;
use gpui::Context;

use crate::app::cditor_v2_view::CditorV2View;

pub(in crate::app) const SELECTION_TOOLBAR_DELAY: Duration = Duration::from_millis(500);

pub(in crate::app) const fn floating_toolbar_passes_selection_delay(
    has_text_selection: bool,
    selection_toolbar_ready: bool,
) -> bool {
    !has_text_selection || selection_toolbar_ready
}

#[derive(Debug, Default)]
pub(in crate::app) struct SelectionToolbarDelay {
    target: Option<DocumentSelection>,
    generation: u64,
    ready: bool,
}

impl SelectionToolbarDelay {
    fn observe(&mut self, target: Option<DocumentSelection>) -> Option<u64> {
        if self.target == target {
            return None;
        }
        self.target = target;
        self.ready = false;
        self.generation = self.generation.wrapping_add(1);
        target.map(|_| self.generation)
    }

    fn reveal(&mut self, generation: u64) -> bool {
        if self.target.is_none() || self.generation != generation || self.ready {
            return false;
        }
        self.ready = true;
        true
    }

    fn is_ready_for(&self, target: Option<DocumentSelection>) -> bool {
        target.is_some() && self.target == target && self.ready
    }
}

impl CditorV2View {
    pub(in crate::app) fn sync_selection_toolbar_delay(&mut self, cx: &mut Context<Self>) -> bool {
        let target = (self.gutter_toolbar_block_id.is_none())
            .then(|| {
                self.ready_runtime_ref().and_then(|runtime| {
                    (runtime.has_document_text_selection()
                        && !runtime.has_entire_document_text_selection())
                    .then(|| runtime.document_selection_snapshot())
                    .flatten()
                })
            })
            .flatten();

        if let Some(generation) = self.selection_toolbar_delay.observe(target) {
            let delay = cx.background_executor().timer(SELECTION_TOOLBAR_DELAY);
            cx.spawn(async move |view, cx| {
                delay.await;
                let _ = view.update(cx, |view, cx| {
                    if view.selection_toolbar_delay.reveal(generation) {
                        cx.notify();
                    }
                });
            })
            .detach();
        }

        self.selection_toolbar_delay.is_ready_for(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::edit::TextPosition;

    fn selection(start: usize, end: usize) -> DocumentSelection {
        DocumentSelection {
            anchor: TextPosition::downstream(1, start),
            focus: TextPosition::downstream(1, end),
        }
    }

    #[test]
    fn selection_must_wait_for_its_matching_timer() {
        let mut delay = SelectionToolbarDelay::default();
        let target = Some(selection(1, 4));
        let generation = delay.observe(target).unwrap();

        assert!(!delay.is_ready_for(target));
        assert!(delay.reveal(generation));
        assert!(delay.is_ready_for(target));
        assert_eq!(delay.observe(target), None);
        assert!(delay.is_ready_for(target));
    }

    #[test]
    fn changing_selection_restarts_delay_and_rejects_the_stale_timer() {
        let mut delay = SelectionToolbarDelay::default();
        let first = Some(selection(1, 4));
        let second = Some(selection(1, 8));
        let first_generation = delay.observe(first).unwrap();
        let second_generation = delay.observe(second).unwrap();

        assert!(!delay.reveal(first_generation));
        assert!(!delay.is_ready_for(first));
        assert!(!delay.is_ready_for(second));
        assert!(delay.reveal(second_generation));
        assert!(delay.is_ready_for(second));
    }

    #[test]
    fn clearing_selection_cancels_pending_and_visible_toolbar() {
        let mut delay = SelectionToolbarDelay::default();
        let target = Some(selection(1, 4));
        let generation = delay.observe(target).unwrap();
        assert!(delay.reveal(generation));
        assert!(delay.is_ready_for(target));

        assert_eq!(delay.observe(None), None);
        assert!(!delay.is_ready_for(target));
        assert!(!delay.reveal(generation));
    }

    #[test]
    fn delay_contract_is_half_a_second() {
        assert_eq!(SELECTION_TOOLBAR_DELAY, Duration::from_millis(500));
    }

    #[test]
    fn delay_filters_only_text_selection_toolbars() {
        assert!(!floating_toolbar_passes_selection_delay(true, false));
        assert!(floating_toolbar_passes_selection_delay(true, true));
        assert!(floating_toolbar_passes_selection_delay(false, false));
        assert!(floating_toolbar_passes_selection_delay(false, true));
    }
}
