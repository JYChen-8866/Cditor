use gpui::{
    AppContext as _, Entity, FocusHandle, InteractiveElement as _, ParentElement as _, Render,
    Styled as _, TestAppContext, div,
};

use crate::editor_view::CditorV2View;

struct FocusHost {
    external_focus: FocusHandle,
    editor: Entity<CditorV2View>,
}

impl Render for FocusHost {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        div()
            .size_full()
            .track_focus(&self.external_focus)
            .child(self.editor.clone())
    }
}

#[gpui::test]
fn rendering_editor_does_not_steal_external_focus(cx: &mut TestAppContext) {
    let (host, cx) = cx.add_window_view(|_, cx| FocusHost {
        external_focus: cx.focus_handle(),
        editor: cx.new(|cx| CditorV2View::loading("Loading", false, cx)),
    });

    cx.update(|window, cx| {
        let external_focus = host.read(cx).external_focus.clone();
        external_focus.focus(window, cx);
        let _ = window.draw(cx);
    });

    cx.update(|window, cx| {
        let host = host.read(cx);
        assert!(host.external_focus.is_focused(window));
        assert!(!host.editor.read(cx).focus.editor.is_focused(window));
    });
}
