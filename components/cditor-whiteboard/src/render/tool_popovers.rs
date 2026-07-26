{
        // Flyouts / picker hang off the toolbar: under a horizontal bar (centered
        // by default, else under its top-left — 42px matches the 10→52 gap), or to
        // the right of a vertical bar (anchored to its captured bounds, so it works
        // whether centered or dragged). Call `.child(panel)` on the result.
        let pill_b = self.toolbar_bounds.get();
        let board_o = self.bounds.get().origin;
        let pill_top = f32::from(pill_b.origin.y) - f32::from(board_o.y);
        let pill_right =
            f32::from(pill_b.origin.x) - f32::from(board_o.x) + f32::from(pill_b.size.width);
        let has_bounds = f32::from(pill_b.size.width) > 1.0;
        let popover_anchor = move || -> Div {
            if vertical && has_bounds {
                div()
                    .absolute()
                    .left(px(pill_right + 6.0))
                    .top(px(pill_top))
            } else {
                match tb_pos {
                    Some((x, y)) => div().absolute().left(px(x)).top(px(y + 42.0)),
                    None => div()
                        .absolute()
                        .top(px(52.0))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center(),
                }
            }
        };

        // Tool-category flyout (centered below the toolbar), built only while a
        // group is open. Occluded like the main bar; picking a tool activates it
        // and closes the flyout (via `set_tool`), and a press elsewhere on the
        // canvas closes it (see `on_left_down`).
        let flyout =
            open_group.map(|g| {
                let mut row = div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .p(px(3.0))
                    .rounded(px(9.0))
                    .bg(panel_strong)
                    .shadow_lg()
                    .occlude();
                for &t in g.tools() {
                    row = row.child(tool_btn(t).tooltip(self.tip(t.label())).on_click(
                        cx.listener(move |this, _ev, window, cx| {
                            this.focus.focus(window, cx);
                            this.set_tool(t, cx);
                        }),
                    ));
                }
                popover_anchor().child(row)
            });

        // The toolbar's text-formatting fly-out (the same panel as the right-click
        // submenu), shown while the "Format" button is toggled on during a text edit.
        let format_panel = (self.editing.is_some() && self.format_flyout).then(|| {
            popover_anchor().child(
                self.format_menu(ink, text, grid, panel_strong, cx)
                    .occlude(),
            )
        });

        // Thickness flyout (centered below the toolbar): a row of preset weights
        // (the active one highlighted) over a slider for any custom width. Presets
        // fire via `on_click`; the slider drags via `on_left_down`/`on_move` (so the
        // panel is *not* occluded — presses fall through, like the color picker).
        // A press outside the panel dismisses it (see `on_left_down`).
        let width_cell = self.width_bounds.clone();
        let width_panel_cell = self.width_panel_bounds.clone();
        let width_frac =
            ((self.active_width - WIDTH_MIN) / (WIDTH_MAX - WIDTH_MIN)).clamp(0.0, 1.0);
        let width_flyout = self.width_open.then(|| {
            let mut presets = div().flex().items_center().gap(px(2.0));
            for (i, w) in WIDTH_PRESETS.into_iter().enumerate() {
                let active = (self.active_width - w).abs() < 0.01;
                let mut opt = div()
                    .id(("wb-width-opt", i))
                    .size(px(30.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0));
                if active {
                    opt = opt.bg(accent);
                } else {
                    opt = opt.hover(|s| s.bg(grid));
                }
                presets = presets.child(
                    opt.child(
                        div()
                            .w(px(18.0))
                            .h(px(w.clamp(1.0, 9.0)))
                            .rounded_full()
                            .bg(cur_swatch),
                    )
                    .on_click(
                        cx.listener(move |this, _ev, window, cx| this.set_width(w, window, cx)),
                    ),
                );
            }
            // The custom-width slider: a bar whose height *is* the current weight,
            // with a thumb at the value. Dragging it lands in `on_left_down`/`on_move`.
            let slider = div()
                .relative()
                .w(px(WIDTH_SLIDER_W))
                .h(px(WIDTH_MAX + 6.0))
                .flex()
                .items_center()
                .child(
                    canvas(move |b, _, _| width_cell.set(b), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(self.active_width.clamp(1.0, WIDTH_MAX)))
                        .rounded_full()
                        .bg(cur_swatch),
                )
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(width_frac * WIDTH_SLIDER_W - 1.5))
                        .w(px(3.0))
                        .rounded(px(2.0))
                        .bg(hsla(0.0, 0.0, 1.0, 1.0))
                        .border_1()
                        .border_color(hsla(0.0, 0.0, 0.0, 0.45)),
                );
            let panel = div()
                .relative()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.0))
                .p(px(6.0))
                .rounded(px(9.0))
                .bg(panel_strong)
                .shadow_lg()
                .child(
                    canvas(move |b, _, _| width_panel_cell.set(b), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .child(presets)
                .child(slider);
            popover_anchor().child(panel)
        });

        // Font flyout: upload a `.ttf`/`.otf`, or revert to the bundled default.
        // Occluded (a press outside dismisses it via `on_left_down`); each row fires
        // the host hook, which loads the face and calls `set_font` for this board.
        let font_flyout = (self.font_open && self.on_pick_font.is_some()).then(|| {
            let row = |id: &'static str, label: &'static str| {
                div()
                    .id(id)
                    .px(px(12.0))
                    .py(px(6.0))
                    .mx(px(4.0))
                    .rounded(px(6.0))
                    .text_size(px(12.0))
                    .text_color(ink)
                    .hover(|s| s.bg(grid))
                    .child(label)
            };
            let panel = div()
                .occlude()
                .py(px(4.0))
                .min_w(px(168.0))
                .rounded(px(9.0))
                .bg(panel_strong)
                .shadow_lg()
                .border_1()
                .border_color(grid)
                .flex()
                .flex_col()
                .child(row("wb-font-upload", "Upload font…").on_click(cx.listener(
                    |this, _ev, window, cx| {
                        this.font_open = false;
                        if let Some(f) = this.on_pick_font.clone() {
                            f(FontPick::Upload, window, cx);
                        }
                        cx.notify();
                    },
                )))
                .child(row("wb-font-default", "Use default").on_click(cx.listener(
                    |this, _ev, window, cx| {
                        this.font_open = false;
                        if let Some(f) = this.on_pick_font.clone() {
                            f(FontPick::Default, window, cx);
                        }
                        cx.notify();
                    },
                )));
            popover_anchor().child(panel)
        });

        // Right-click context menu (a selection's "Save as template"), anchored at
        // the cursor. Occluded so its button doesn't fall through to the canvas;
        // any other press dismisses it (see `on_left_down`).
        let menu =
            self.context_menu.map(|pos| {
                // One clickable row; clicking runs `act` and closes the menu.
                let row = |id: &'static str, label: &'static str, shortcut: &'static str| {
                    div()
                        .id(id)
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(16.0))
                        .px(px(10.0))
                        .py(px(5.0))
                        .mx(px(4.0))
                        .rounded(px(6.0))
                        .text_size(px(12.0))
                        .text_color(ink)
                        .hover(|s| s.bg(grid))
                        .child(label)
                        .child(div().text_size(px(11.0)).text_color(text).child(shortcut))
                };
                let divider = || div().my(px(4.0)).mx(px(8.0)).h(px(1.0)).bg(grid);
                let has_sel = !self.selected.is_empty();
                let mut panel = div()
                    .absolute()
                    .left(pos.x)
                    .top(pos.y)
                    .occlude()
                    .min_w(px(176.0))
                    .py(px(4.0))
                    .rounded(px(8.0))
                    .bg(panel_strong)
                    .shadow_lg()
                    .border_1()
                    .border_color(grid)
                    .flex()
                    .flex_col();
                // While editing text, a "Text ▸" row expands the formatting submenu.
                if self.editing.is_some() {
                    panel = panel
                        .child(row("wb-ctx-text", "Text", "▸").on_click(cx.listener(
                            |this, _ev, _w, cx| {
                                this.ctx_text_sub = !this.ctx_text_sub;
                                cx.notify();
                            },
                        )))
                        .child(divider());
                }
                // Z-order + copy / cut act on the selection, so they show only with one.
                if has_sel {
                    panel =
                        panel
                            .child(row("wb-ctx-front", "Bring to Front", "⌘⇧]").on_click(
                                cx.listener(|this, _ev, window, cx| {
                                    this.context_menu = None;
                                    this.reorder_selection(ZOrder::ToFront, window, cx);
                                }),
                            ))
                            .child(row("wb-ctx-forward", "Bring Forward", "⌘]").on_click(
                                cx.listener(|this, _ev, window, cx| {
                                    this.context_menu = None;
                                    this.reorder_selection(ZOrder::Forward, window, cx);
                                }),
                            ))
                            .child(row("wb-ctx-backward", "Send Backward", "⌘[").on_click(
                                cx.listener(|this, _ev, window, cx| {
                                    this.context_menu = None;
                                    this.reorder_selection(ZOrder::Backward, window, cx);
                                }),
                            ))
                            .child(
                                row("wb-ctx-back", "Send to Back", "⌘⇧[").on_click(cx.listener(
                                    |this, _ev, window, cx| {
                                        this.context_menu = None;
                                        this.reorder_selection(ZOrder::ToBack, window, cx);
                                    },
                                )),
                            )
                            .child(divider())
                            .child(row("wb-ctx-copy", "Copy", "⌘C").on_click(cx.listener(
                                |this, _ev, window, cx| {
                                    this.context_menu = None;
                                    this.copy_selection(window, cx);
                                },
                            )))
                            .child(row("wb-ctx-cut", "Cut", "⌘X").on_click(cx.listener(
                                |this, _ev, window, cx| {
                                    this.context_menu = None;
                                    if this.copy_selection(window, cx) {
                                        this.delete_selected(window, cx);
                                    }
                                },
                            )));
                }
                // Paste shows whenever the host wired it (so it works on empty canvas).
                if self.on_paste.is_some() {
                    panel = panel.child(row("wb-ctx-paste", "Paste", "⌘V").on_click(
                        cx.listener(|this, _ev, window, cx| this.paste_from_menu(window, cx)),
                    ));
                }
                // "Save as template" only with a selection and a wired host callback.
                if has_sel && self.on_save_template.is_some() {
                    panel = panel.child(divider()).child(
                        row("wb-ctx-save-template", "Save as template", "").on_click(cx.listener(
                            |this, _ev, window, cx| {
                                this.context_menu = None;
                                this.save_selection_as_template(window, cx);
                            },
                        )),
                    );
                }
                panel
            });

        // The "Text ▸" formatting submenu — a fly-out beside the context menu with
        // a ✓ on each active format. Toggling a row keeps the menu open so the
        // checkmarks update live; clicking off (anywhere else) dismisses it.
        let text_submenu = self
            .context_menu
            .filter(|_| self.ctx_text_sub && self.editing.is_some())
            .map(|pos| {
                self.format_menu(ink, text, grid, panel_strong, cx)
                    .absolute()
                    .left(pos.x + px(184.0))
                    .top(pos.y)
                    .occlude()
            });

    (
        flyout,
        format_panel,
        width_flyout,
        font_flyout,
        menu,
        text_submenu,
        popover_anchor,
    )
}
