impl WhiteboardView {
    fn render_read_only(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let WhiteboardStyle {
            bg,
            grid,
            text,
            ink,
            panel,
            ..
        } = (self.style)(cx);
        let camera = self.scene.camera;
        let bounds_cell = self.bounds.clone();
        let render_viewport = self.render_viewport(Some(window.viewport_size()));
        let visible_element_ids = self
            .scene
            .elements
            .iter()
            .filter(|element| {
                render_viewport.is_none_or(|viewport| viewport.intersects(bbox(&element.kind)))
            })
            .map(|element| element.id)
            .collect::<HashSet<_>>();
        self.text_layout_cache
            .retain(|element_id, _| visible_element_ids.contains(element_id));
        self.label_layout_cache
            .retain(|element_id, _| visible_element_ids.contains(element_id));
        let layers = build_thumbnail_layers(
            &self.scene,
            &self.font,
            camera,
            ThumbnailPalette {
                ink,
                text,
                grid,
                panel,
            },
            render_viewport,
            Some(&mut self.text_layout_cache),
            Some(&mut self.label_layout_cache),
        );
        let board_layer = canvas(
            move |bounds, _, _| bounds_cell.set(bounds),
            move |bounds, _, window, _| paint_board(bounds, camera, bg, grid, window),
        )
        .absolute()
        .size_full();
        let element_layers = layers.into_iter().map(|layer| match layer {
            Layer::Band(elements) => band_canvas(elements, camera).into_any_element(),
            Layer::Overlay(element) => element,
        });

        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .cursor(if self.panning {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .child(board_layer)
            .children(element_layers)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_left_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_left_up))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_middle_down))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_middle_up))
            .on_mouse_move(cx.listener(Self::on_move))
            .on_pinch(cx.listener(Self::on_pinch))
            .child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .bottom(px(8.0))
                    .text_size(px(11.0))
                    .text_color(text)
                    .child(SharedString::from(format!("{:.0}%", camera.zoom * 100.0))),
            )
            .into_any_element()
    }
}

