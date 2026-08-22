use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Point, Window,
};

/// Defers an element at the window coordinate origin without inheriting the
/// editor panel's content mask. This is the rendering boundary required by
/// media fullscreen: the editor remains the state owner while the media layer
/// can cover sibling docks and titlebars owned by the host application.
pub(super) struct WindowOverlay {
    child: Option<AnyElement>,
    priority: usize,
}

impl WindowOverlay {
    pub(super) fn new(child: impl IntoElement) -> Self {
        Self {
            child: Some(child.into_any_element()),
            priority: usize::MAX,
        }
    }
}

impl Element for WindowOverlay {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.as_mut().unwrap().request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) {
        window.defer_draw(
            self.child.take().unwrap(),
            Point::default(),
            self.priority,
            None,
        );
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }
}

impl IntoElement for WindowOverlay {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
