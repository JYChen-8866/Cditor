use std::rc::Rc;

use gpui::{App, Context, Window};

use super::DrafftBoardView;

pub type FocusRequestFn = Rc<dyn Fn(&mut App) -> bool>;

impl DrafftBoardView {
    pub fn set_on_focus_request(&mut self, on_focus_request: FocusRequestFn) {
        self.on_focus_request = Some(on_focus_request);
    }

    pub(super) fn request_focus(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self
            .on_focus_request
            .as_ref()
            .is_some_and(|request| !request(cx))
        {
            return false;
        }
        self.focus.focus(window, cx);
        true
    }
}
