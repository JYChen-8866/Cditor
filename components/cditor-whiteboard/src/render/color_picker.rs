{
        // Color picker panel (below the toolbar), built only while open. Not
        // occluded: presses fall through to `on_left_down`, which routes the SV
        // square / hue strip to drags (via the captured bounds), consumes presses
        // elsewhere on the panel, and closes on a press outside it.
        let sv_cell = self.sv_bounds.clone();
        let hue_cell = self.hue_bounds.clone();
        let alpha_cell = self.alpha_bounds.clone();
        let panel_cell = self.picker_bounds.clone();
        let swatch_list = swatches;
        let white = hsla(0.0, 0.0, 1.0, 1.0);
        // The stroke / fill colors backing the two target tabs (selection's, else
        // the active value). `None` = theme ink (stroke) or unfilled (fill).
        let stroke_disp = self
            .selected_single()
            .and_then(|id| self.scene.elements.iter().find(|e| e.id == id))
            .and_then(|e| e.stroke)
            .or(self.active_stroke);
        let fill_disp = self
            .selected_single()
            .and_then(|id| self.scene.elements.iter().find(|e| e.id == id))
            .and_then(|e| e.fill)
            .or(self.active_fill);
        let text_disp = self
            .selected_single()
            .and_then(|id| self.scene.elements.iter().find(|e| e.id == id))
            .and_then(|e| e.label_color)
            .or(self.active_text);
        self.picker.map(|p| {
            let cur = hsva_to_u32(p.h, p.s, p.v, p.a);
            let hex = format!("#{:06X}", cur >> 8);
            let clear = hsla(0.0, 0.0, 0.0, 0.0);

            // Stroke / fill target tabs. The active one is highlighted; clicking
            // re-seeds the controls from that property's color.
            let tab = |active: bool, sw: Hsla, label: &'static str, id: &'static str| {
                let mut d = div()
                    .id(id)
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .text_size(px(12.0))
                    .text_color(ink);
                if active {
                    d = d.bg(accent);
                }
                d.child(
                    div()
                        .size(px(12.0))
                        .rounded(px(3.0))
                        .bg(sw)
                        .border_1()
                        .border_color(grid),
                )
                .child(label)
            };
            let tabs = div()
                .flex()
                .gap(px(6.0))
                .child(
                    tab(
                        p.target == PickerTarget::Stroke,
                        stroke_disp.map_or(ink, u32_to_hsla),
                        "Stroke",
                        "wb-tab-stroke",
                    )
                    .on_click(cx.listener(|this, _ev, _w, cx| {
                        this.set_picker_target(PickerTarget::Stroke, cx)
                    })),
                )
                .child(
                    tab(
                        p.target == PickerTarget::Fill,
                        fill_disp.map_or(clear, u32_to_hsla),
                        "Fill",
                        "wb-tab-fill",
                    )
                    .on_click(cx.listener(|this, _ev, _w, cx| {
                        this.set_picker_target(PickerTarget::Fill, cx)
                    })),
                )
                .child(
                    tab(
                        p.target == PickerTarget::Text,
                        text_disp.map_or(ink, u32_to_hsla),
                        "Text",
                        "wb-tab-text",
                    )
                    .on_click(cx.listener(|this, _ev, _w, cx| {
                        this.set_picker_target(PickerTarget::Text, cx)
                    })),
                );

            let sv_square = div()
                .relative()
                .w(px(SV_W))
                .h(px(SV_H))
                .rounded(px(5.0))
                .overflow_hidden()
                .bg(hsla(p.h, 1.0, 0.5, 1.0))
                .child(div().absolute().size_full().bg(linear_gradient(
                    90.0,
                    linear_color_stop(white, 0.0),
                    linear_color_stop(hsla(0.0, 0.0, 1.0, 0.0), 1.0),
                )))
                .child(div().absolute().size_full().bg(linear_gradient(
                    180.0,
                    linear_color_stop(hsla(0.0, 0.0, 0.0, 0.0), 0.0),
                    linear_color_stop(hsla(0.0, 0.0, 0.0, 1.0), 1.0),
                )))
                .child(
                    canvas(move |b, _, _| sv_cell.set(b), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(p.s * SV_W - 7.0))
                        .top(px((1.0 - p.v) * SV_H - 7.0))
                        .size(px(14.0))
                        .rounded_full()
                        .border_2()
                        .border_color(white),
                );

            let seg = |from: f32, to: f32| {
                div().flex_1().h_full().bg(linear_gradient(
                    90.0,
                    linear_color_stop(hsla(from, 1.0, 0.5, 1.0), 0.0),
                    linear_color_stop(hsla(to, 1.0, 0.5, 1.0), 1.0),
                ))
            };
            let hue_strip = div()
                .relative()
                .w(px(SV_W))
                .h(px(HUE_H))
                .rounded(px(4.0))
                .overflow_hidden()
                .flex()
                .child(seg(0.0, 1.0 / 6.0))
                .child(seg(1.0 / 6.0, 2.0 / 6.0))
                .child(seg(2.0 / 6.0, 3.0 / 6.0))
                .child(seg(3.0 / 6.0, 4.0 / 6.0))
                .child(seg(4.0 / 6.0, 5.0 / 6.0))
                .child(seg(5.0 / 6.0, 1.0))
                .child(
                    canvas(move |b, _, _| hue_cell.set(b), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(p.h * SV_W - 1.5))
                        .top(px(-2.0))
                        .w(px(3.0))
                        .h(px(HUE_H + 4.0))
                        .rounded(px(2.0))
                        .bg(white)
                        .border_1()
                        .border_color(hsla(0.0, 0.0, 0.0, 0.5)),
                );

            // Alpha (opacity) strip: transparent → the current color, opaque.
            let alpha_strip = div()
                .relative()
                .w(px(SV_W))
                .h(px(HUE_H))
                .rounded(px(4.0))
                .overflow_hidden()
                .bg(linear_gradient(
                    90.0,
                    linear_color_stop(clear, 0.0),
                    linear_color_stop(u32_to_hsla(hsv_to_u32(p.h, p.s, p.v)), 1.0),
                ))
                .child(
                    canvas(move |b, _, _| alpha_cell.set(b), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(p.a * SV_W - 1.5))
                        .top(px(-2.0))
                        .w(px(3.0))
                        .h(px(HUE_H + 4.0))
                        .rounded(px(2.0))
                        .bg(white)
                        .border_1()
                        .border_color(hsla(0.0, 0.0, 0.0, 0.5)),
                );

            // Reset means "back to theme ink" for stroke, "no fill" for fill.
            let reset_label = if p.target == PickerTarget::Fill {
                "None"
            } else {
                "Auto"
            };
            let info_row = div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .size(px(22.0))
                        .rounded(px(4.0))
                        .bg(u32_to_hsla(cur))
                        .border_1()
                        .border_color(grid),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(px(12.0))
                        .text_color(text)
                        .child(SharedString::from(hex)),
                )
                .child(
                    div()
                        .id("wb-color-auto")
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(px(5.0))
                        .border_1()
                        .border_color(grid)
                        .text_size(px(12.0))
                        .text_color(ink)
                        .child(reset_label)
                        .on_click(
                            cx.listener(|this, _ev, window, cx| this.pick_color(None, window, cx)),
                        ),
                );

            let mut swatch_views = Vec::with_capacity(swatch_list.len());
            for (i, c) in swatch_list.iter().enumerate() {
                let col = *c;
                swatch_views.push(
                    div()
                        .id(("wb-swatch", i))
                        .size(px(20.0))
                        .rounded(px(4.0))
                        .bg(col)
                        .border_1()
                        .border_color(grid)
                        .on_click(cx.listener(move |this, _ev, window, cx| {
                            this.pick_color(Some(hsla_to_u32(col)), window, cx)
                        })),
                );
            }
            // Theme swatches, kept on one line. Its width (`n` swatches of 20px +
            // 6px gaps) sets the panel width, and the Saved column is sized to the
            // space it leaves beside the controls — so the panel crops to this row
            // (no dead space) and saved colors wrap rather than run off the edge.
            let theme_row_w = (swatch_views.len() as f32 * 26.0 - 6.0).max(0.0);
            let saved_col_w = (theme_row_w - SV_W - 12.0).max(64.0);
            let swatch_grid = div().flex().flex_wrap().gap(px(6.0)).children(swatch_views);

            // The gradient controls (the swatch row spans the full panel below).
            let controls_col = div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(tabs)
                .child(sv_square)
                .child(hue_strip)
                .child(alpha_strip)
                .child(info_row);

            // The user's saved palette: the right column (filling the dead space). A
            // `+` saves the current color; each swatch applies on click, removes on
            // right-click. Persisted by the host via `on_save_colors`.
            let mut saved_grid = div().flex().flex_wrap().gap(px(6.0));
            if self.saved_colors.is_empty() {
                saved_grid = saved_grid.child(
                    div()
                        .w_full()
                        .text_size(px(11.0))
                        .text_color(text)
                        .child("Tap + to save a color"),
                );
            } else {
                for (i, &c) in self.saved_colors.iter().enumerate() {
                    saved_grid = saved_grid.child(
                        div()
                            .id(("wb-saved", i))
                            .size(px(20.0))
                            .rounded(px(4.0))
                            .bg(u32_to_hsla(c))
                            .border_1()
                            .border_color(grid)
                            .tooltip(self.tip("Click to use · right-click to remove"))
                            .on_click(cx.listener(move |this, _ev, window, cx| {
                                this.pick_color(Some(c), window, cx)
                            }))
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |this, _ev, window, cx| {
                                    this.remove_saved_color(c, window, cx)
                                }),
                            ),
                    );
                }
            }
            // Sized to the space the one-line swatch row leaves beside the controls,
            // so the panel crops to that row (no dead space) and the saved swatches
            // wrap within this column instead of forming one long row.
            let saved_col = div()
                .flex()
                .flex_col()
                .flex_none()
                .w(px(saved_col_w))
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_size(px(11.0)).text_color(text).child("Saved"))
                        .child(
                            div()
                                .id("wb-save-color")
                                .size(px(20.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.0))
                                .border_1()
                                .border_color(grid)
                                .text_size(px(14.0))
                                .text_color(ink)
                                .hover(|s| s.bg(grid))
                                .child("+")
                                .tooltip(self.tip("Save current color"))
                                .on_click(cx.listener(|this, _ev, window, cx| {
                                    this.save_current_color(window, cx)
                                })),
                        ),
                )
                .child(saved_grid);

            // Top: the gradient controls with the Saved palette beside them (in the
            // space the one-line swatch row leaves free). Swatch row spans below.
            let top_row = div()
                .flex()
                .flex_row()
                .items_start()
                .gap(px(12.0))
                .child(controls_col)
                .child(saved_col);

            popover_anchor().child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .p(px(10.0))
                    .rounded(px(10.0))
                    .bg(panel_strong)
                    .shadow_lg()
                    .border_1()
                    .border_color(grid)
                    .child(
                        canvas(move |b, _, _| panel_cell.set(b), |_, _, _, _| {})
                            .absolute()
                            .size_full(),
                    )
                    .child(top_row)
                    .child(swatch_grid),
            )
        })
}
