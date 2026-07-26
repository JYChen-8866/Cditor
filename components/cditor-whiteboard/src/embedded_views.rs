impl BoardEmbedView {
    /// Build a read-only embedded board view. The inner board starts in
    /// read-only forced-pan mode.
    pub fn new(scene: Scene, style: WhiteboardStyleFn, cx: &mut Context<Self>) -> Self {
        let board_style = style.clone();
        let board = cx.new(|cx| WhiteboardView::new_read_only(scene, board_style, cx));
        Self {
            board,
            style,
            on_expand: None,
        }
    }

    /// Access the inner board entity for host-driven inspection or updates.
    pub fn board(&self) -> Entity<WhiteboardView> {
        self.board.clone()
    }

    /// Install the callback fired by the embed view's "edit" affordance.
    pub fn set_on_expand(&mut self, f: ExpandEmbedFn) {
        self.on_expand = Some(f);
    }
}

impl Render for BoardEmbedView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let st = (self.style)();
        let ink = st.ink;
        let panel = st.panel_strong;
        let grid = st.grid;
        let accent = st.accent;
        let button = self.on_expand.as_ref().map(|_| {
            div()
                .id("board-embed-expand")
                .absolute()
                .top(px(10.0))
                .right(px(10.0))
                .h(px(30.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .rounded(px(8.0))
                .bg(panel)
                .border_1()
                .border_color(grid.opacity(0.5))
                .hover(|s| s.bg(accent))
                .text_size(px(12.0))
                .text_color(ink)
                .child("↗")
                .child("编辑")
                .on_click(cx.listener(|this, _ev, window, cx| {
                    if let Some(f) = this.on_expand.clone() {
                        f(window, cx);
                    }
                }))
        });
        div()
            .size_full()
            .relative()
            .child(self.board.clone())
            .children(button)
    }
}

impl BoardThumbnailView {
    pub fn new(snapshot: LocalThumbnailSnapshot, style: WhiteboardStyleFn) -> Self {
        Self {
            snapshot,
            style,
            font: Font::default(),
        }
    }

    pub fn snapshot(&self) -> &LocalThumbnailSnapshot {
        &self.snapshot
    }

    pub fn set_snapshot(&mut self, snapshot: LocalThumbnailSnapshot) {
        self.snapshot = snapshot;
    }
}

impl Render for BoardThumbnailView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let WhiteboardStyle {
            bg,
            grid,
            text,
            ink,
            panel,
            ..
        } = (self.style)();
        let cam = self.snapshot.spec.camera;
        let layers = build_thumbnail_layers(
            &self.snapshot.scene,
            &self.font,
            cam,
            ThumbnailPalette {
                ink,
                text,
                grid,
                panel,
            },
            None,
            None,
            None,
        );
        let board_layer = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| paint_board(bounds, cam, bg, grid, window),
        )
        .absolute()
        .size_full();
        let element_layers: Vec<gpui::AnyElement> = layers
            .into_iter()
            .map(|l| match l {
                Layer::Band(es) => band_canvas(es, cam).into_any_element(),
                Layer::Overlay(el) => el,
            })
            .collect();
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .child(board_layer)
            .children(element_layers)
    }
}
