use gpui::{
    AnyElement, AnyView, App, Bounds, Display, Element, ElementId, Entity, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, StyleRefinement, Styled, Window,
    relative,
};

use super::CditorV2View;

/// Embeds Cditor in a host whose size can change independently of the window.
///
/// GPUI calls `Render::render` before ordinary child bounds are available. An
/// editor that reads a `ScrollHandle` in `render` can consequently project one
/// frame with the previous width after a dock or split-pane resize. This
/// element reserves the host layout first, injects its final bounds into
/// Cditor during prepaint, and only then renders the editor subtree.
pub struct CditorHostElement {
    view: Entity<CditorV2View>,
}

impl CditorHostElement {
    #[must_use]
    pub fn new(view: Entity<CditorV2View>) -> Self {
        Self { view }
    }
}

impl IntoElement for CditorHostElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CditorHostElement {
    type RequestLayoutState = (AnyElement, LayoutId);
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut editor = AnyView::from(self.view.clone())
            .cached(StyleRefinement::default().flex().size_full())
            .into_any_element();
        let editor_layout = editor.request_layout(window, cx);
        let mut style = Style {
            display: Display::Flex,
            ..Style::default()
        };
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (
            window.request_layout(style, [editor_layout], cx),
            (editor, editor_layout),
        )
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (editor, editor_layout): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let editor_bounds = window.layout_bounds(*editor_layout);
        self.view.update(cx, |view, _| {
            view.interaction
                .prepare_editor_viewport_for_render(editor_bounds);
        });
        // AnyView's cached prepaint path invokes Render::render only after it
        // sees the final child bounds. The viewport prepared above is
        // therefore consumed by the same frame's document projection.
        editor.prepaint(window, cx);
        ()
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (editor, _): &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        editor.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use gpui::{
        AppContext as _, Entity, ParentElement as _, Render, Styled as _, TestAppContext, div, px,
    };

    use super::{CditorHostElement, CditorV2View};

    struct ResizableHost {
        editor: Entity<CditorV2View>,
        editor_width: gpui::Pixels,
    }

    impl Render for ResizableHost {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            div()
                .w(self.editor_width)
                .h(px(700.0))
                .child(CditorHostElement::new(self.editor.clone()))
        }
    }

    #[gpui::test]
    fn host_resize_does_not_request_a_correction_frame(cx: &mut TestAppContext) {
        let (host, cx) = cx.add_window_view(|_, cx| ResizableHost {
            editor: cx.new(|cx| CditorV2View::loading("Loading", false, cx)),
            editor_width: px(620.0),
        });

        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        cx.update(|_, cx| {
            let editor = host.read(cx).editor.clone();
            assert_eq!(
                editor
                    .read(cx)
                    .interaction
                    .rendered_editor_viewport_bounds()
                    .expect("initial host viewport")
                    .size
                    .width,
                px(620.0),
            );
            assert_eq!(
                editor
                    .read(cx)
                    .interaction
                    .editor_viewport_correction_requests(),
                0,
            );
        });

        host.update(cx, |host, cx| {
            host.editor_width = px(900.0);
            cx.notify();
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });

        cx.update(|_, cx| {
            let editor = host.read(cx).editor.clone();
            assert_eq!(
                editor
                    .read(cx)
                    .interaction
                    .rendered_editor_viewport_bounds()
                    .expect("resized host viewport")
                    .size
                    .width,
                px(900.0),
            );
            assert_eq!(
                editor
                    .read(cx)
                    .interaction
                    .editor_viewport_correction_requests(),
                0,
            );
        });
    }
}
