{
        // Templates gallery modal: a dimming scrim (click to dismiss) centering a
        // panel of preview cards. The panel `occlude()`s so clicks on it don't
        // reach the scrim; a card stamps its template and closes (see
        // `apply_template`), and Escape closes it (see `on_key`).
        self.templates_open.then(|| {
            let body = if self.templates.is_empty() {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(28.0))
                    .child(
                        div()
                            .max_w(px(320.0))
                            .text_size(px(12.0))
                            .text_color(text)
                            .child(
                                "No templates yet. Select shapes on the canvas, right-click, \
                                 and choose “Save as template”.",
                            ),
                    )
                    .into_any_element()
            } else {
                let mut grid_el = div().flex().flex_wrap().gap(px(8.0)).justify_center();
                for i in 0..self.templates.len() {
                    grid_el = grid_el.child(self.template_card(i, ink, text, grid, bg, cx));
                }
                div()
                    .id("wb-tmpl-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(12.0))
                    .child(grid_el)
                    .into_any_element()
            };
            let panel = div()
                .w(px(540.0))
                .max_h(px(460.0))
                .flex()
                .flex_col()
                .rounded(px(12.0))
                .bg(panel_strong)
                .shadow_lg()
                .border_1()
                .border_color(grid)
                .occlude()
                // header
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(px(14.0))
                        .py(px(10.0))
                        .border_b_1()
                        .border_color(grid)
                        .child(div().text_size(px(14.0)).text_color(ink).child("Templates"))
                        .child(
                            div()
                                .id("wb-tmpl-close")
                                .size(px(22.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.0))
                                .text_size(px(15.0))
                                .text_color(text)
                                .hover(|s| s.bg(grid))
                                .child("✕")
                                .on_click(cx.listener(|this, _ev, _w, cx| {
                                    this.templates_open = false;
                                    cx.notify();
                                })),
                        ),
                )
                .child(body)
                // footer hint
                .child(
                    div()
                        .px(px(14.0))
                        .py(px(8.0))
                        .border_t_1()
                        .border_color(grid)
                        .text_size(px(10.0))
                        .text_color(text)
                        .child("Click to add · right-click to delete"),
                );
            div()
                .absolute()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(0.0, 0.0, 0.0, 0.35))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _ev, _w, cx| {
                        this.templates_open = false;
                        cx.notify();
                    }),
                )
                .child(panel)
        })
}
