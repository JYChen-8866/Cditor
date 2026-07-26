{
        // Tool palette + actions (top-center). The pill `occlude()`s so a press
        // on a button doesn't also act on the board beneath it. Layout, left→right:
        //   pan · select · mindmap · color │ shapes&text▾ · pages&images▾ │ undo · redo · delete
        // `MindMap` is promoted to a first-class toolbar button in the main tool area.
        let active = self.tool;
        let open_group = self.open_group;

        // A bare tool button (icon + active highlight). The caller attaches the
        // tooltip and click handler, so this borrows nothing from `self`/`cx` and
        // can be reused for both the main bar and the flyout.
        let tool_btn = move |t: Tool| {
            let icon: gpui::AnyElement = match t.icon() {
                Some((key, bytes)) => svg_icon(key, bytes, ink, 16.0).into_any_element(),
                None => t.glyph().into_any_element(),
            };
            let mut b = div()
                .id(("wb-tool", t as usize))
                .size(px(30.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .text_size(px(15.0))
                .text_color(ink)
                .child(icon);
            // The hover tint also makes gpui repaint on hover transitions, which
            // is what lets a tooltip dismiss when the cursor leaves the button
            // (the canvas doesn't repaint on a bare mouse-move otherwise).
            if t == active {
                b = b.bg(accent);
            } else {
                b = b.hover(|s| s.bg(grid));
            }
            b
        };

        // A category button: shows the group's active tool (else a representative)
        // with a ▾ affordance, and highlights while its group owns the active tool
        // or its flyout is open.
        let cat_btn = |g: ToolGroup| {
            let shown = if g.contains(active) {
                active
            } else {
                g.representative()
            };
            let icon: gpui::AnyElement = match shown.icon() {
                Some((key, bytes)) => svg_icon(key, bytes, ink, 16.0).into_any_element(),
                None => shown.glyph().into_any_element(),
            };
            let mut b = div()
                .id(("wb-group", g as usize))
                .h(px(30.0))
                .px(px(6.0))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(1.0))
                .rounded(px(6.0))
                .text_color(ink)
                .child(icon)
                .child(div().text_size(px(8.0)).text_color(text).child("▾"));
            if open_group == Some(g) || g.contains(active) {
                b = b.bg(accent);
            } else {
                b = b.hover(|s| s.bg(grid));
            }
            b
        };

        // The category buttons (one per `ToolGroup`), with the standalone Text
        // tool slotted in right after the Lines group.
        let mut cats: Vec<gpui::AnyElement> = Vec::with_capacity(ToolGroup::ALL.len() + 1);
        for &g in ToolGroup::ALL.iter() {
            cats.push(
                cat_btn(g)
                    .tooltip(self.tip(g.label()))
                    .on_click(cx.listener(move |this, _ev, window, cx| {
                        this.focus.focus(window, cx);
                        this.toggle_group(g, cx);
                    }))
                    .into_any_element(),
            );
            if g == ToolGroup::Lines {
                cats.push(
                    tool_btn(Tool::Text)
                        .tooltip(self.tip(Tool::Text.label()))
                        .on_click(cx.listener(|this, _ev, _w, cx| this.set_tool(Tool::Text, cx)))
                        .into_any_element(),
                );
            }
        }

        const UNDO_ICON: &[u8] = include_bytes!("../../assets/icons/undo.svg");
        const REDO_ICON: &[u8] = include_bytes!("../../assets/icons/redo.svg");
        const DELETE_ICON: &[u8] = include_bytes!("../../assets/icons/delete.svg");
        let act = |id: usize, key: &'static str, bytes: &'static [u8]| {
            div()
                .id(("wb-act", id))
                .size(px(30.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .hover(|s| s.bg(grid))
                .child(svg_icon(key, bytes, ink, 16.0))
        };
        // Color button: a swatch of the current ink that toggles the picker.
        let cur_swatch = self.active_stroke.map_or(ink, u32_to_hsla);
        let mut color_btn = div()
            .id("wb-color")
            .size(px(30.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0));
        if self.picker.is_some() {
            color_btn = color_btn.bg(accent);
        } else {
            color_btn = color_btn.hover(|s| s.bg(grid));
        }
        let color_btn = color_btn
            .child(
                div()
                    .size(px(16.0))
                    .rounded(px(4.0))
                    .bg(cur_swatch)
                    .border_1()
                    .border_color(grid),
            )
            .tooltip(self.tip("Color"))
            .on_click(cx.listener(|this, _ev, window, cx| {
                this.focus.focus(window, cx);
                this.toggle_picker(cx);
            }));
        // Thickness button: a bar of the current stroke weight (in the current ink)
        // that toggles the thickness flyout — sits next to color.
        let mut width_btn = div()
            .id("wb-width")
            .size(px(30.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0));
        if self.width_open {
            width_btn = width_btn.bg(accent);
        } else {
            width_btn = width_btn.hover(|s| s.bg(grid));
        }
        let width_btn = width_btn
            .child(
                div()
                    .w(px(16.0))
                    .h(px(self.active_width.clamp(1.0, 8.0)))
                    .rounded_full()
                    .bg(cur_swatch),
            )
            .tooltip(self.tip("Thickness"))
            .on_click(cx.listener(|this, _ev, window, cx| {
                this.focus.focus(window, cx);
                this.toggle_width(cx);
            }));
        // Font button: a per-board text face ("Aa"); opens a small flyout to upload
        // a `.ttf`/`.otf` or revert to the default. Hidden without a host hook.
        let font_btn = self.on_pick_font.is_some().then(|| {
            let mut b = div()
                .id("wb-font")
                .size(px(30.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0));
            if self.font_open {
                b = b.bg(accent);
            } else {
                b = b.hover(|s| s.bg(grid));
            }
            b.child(div().text_size(px(15.0)).text_color(ink).child("Aa"))
                .tooltip(self.tip("Font"))
                .on_click(cx.listener(|this, _ev, window, cx| {
                    this.focus.focus(window, cx);
                    this.toggle_font(cx);
                }))
        });
        // Templates button: opens the gallery modal (its own toolbar item, since
        // a gallery of cards doesn't belong among the tool icons).
        const TEMPLATES_ICON: &[u8] = include_bytes!("../../assets/icons/templates.svg");
        let mut templates_btn = div()
            .id("wb-templates")
            .size(px(30.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .child(svg_icon("wb-icon-templates", TEMPLATES_ICON, ink, 16.0));
        if self.templates_open {
            templates_btn = templates_btn.bg(accent);
        } else {
            templates_btn = templates_btn.hover(|s| s.bg(grid));
        }
        let templates_btn = templates_btn
            .tooltip(self.tip("Templates"))
            .on_click(cx.listener(|this, _ev, window, cx| {
                this.focus.focus(window, cx);
                this.toggle_templates(cx);
            }));
        // Dotted drag grip + bounds capture so the toolbar can be moved. The pill
        // is NOT occluded (like the color picker): a grip press starts a drag and a
        // press elsewhere on the pill is consumed in `on_left_down`, so the buttons
        // still fire their own clicks.
        let grip_cell = self.toolbar_grip_bounds.clone();
        let pill_cell = self.toolbar_bounds.clone();
        let vertical = self.toolbar_vertical;
        let dot_row = move || {
            div()
                .flex()
                .gap(px(3.0))
                .child(div().size(px(2.5)).rounded_full().bg(text))
                .child(div().size(px(2.5)).rounded_full().bg(text))
        };
        let grip = div()
            .id("wb-grip")
            .relative()
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(3.0))
            .px(px(4.0))
            .h(px(30.0))
            .cursor(CursorStyle::OpenHand)
            .tooltip(self.tip("Drag to move · Tap R to flip · double-click to reset"))
            .child(
                canvas(move |b, _, _| grip_cell.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(dot_row())
            .child(dot_row())
            .child(dot_row());
        // A "Format" button — shown only while editing text — toggling the
        // text-formatting fly-out.
        let format_btn = self.editing.is_some().then(|| {
            let mut b = div()
                .id("wb-format-btn")
                .size(px(30.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .text_size(px(14.0))
                .text_color(ink)
                .tooltip(self.tip("Text formatting"))
                .child("A");
            if self.format_flyout {
                b = b.bg(accent);
            } else {
                b = b.hover(|s| s.bg(grid));
            }
            b.on_click(cx.listener(|this, _ev, _w, cx| {
                this.format_flyout = !this.format_flyout;
                cx.notify();
            }))
        });
        let mut pill = div()
            .relative()
            .flex()
            .items_center()
            .gap(px(2.0))
            .p(px(3.0))
            .rounded(px(9.0))
            .bg(panel);
        if vertical {
            pill = pill.flex_col();
        }
        let mut pill = pill
            .child(
                canvas(move |b, _, _| pill_cell.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(grip)
            .child(toolbar_divider(grid, vertical))
            // navigate + color
            .child(
                tool_btn(Tool::Pan)
                    .tooltip(self.tip(Tool::Pan.label()))
                    .on_click(cx.listener(|this, _ev, _w, cx| this.set_tool(Tool::Pan, cx))),
            )
            .child(
                tool_btn(Tool::Select)
                    .tooltip(self.tip(Tool::Select.label()))
                    .on_click(cx.listener(|this, _ev, _w, cx| this.set_tool(Tool::Select, cx))),
            );
        if let Some(root_id) = selected_mindmap_root {
            let direction = self.mindmap_root_direction(root_id);
            let connector_style = self.mindmap_connector_style_for_root(root_id);
            let chip = |id: &'static str, active: bool, icon: gpui::AnyElement| {
                let mut d = div()
                    .id(id)
                    .px(px(8.0))
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .text_size(px(11.0))
                    .text_color(ink);
                if active {
                    d = d.bg(accent);
                } else {
                    d = d.hover(|s| s.bg(grid));
                }
                d.child(icon)
            };
            let icon_color = ink;
            let draw_mm_icon = move |_id: &'static str, kind: &'static str| -> gpui::AnyElement {
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let w = f32::from(bounds.size.width);
                        let h = f32::from(bounds.size.height);
                        let ox = f32::from(bounds.origin.x);
                        let oy = f32::from(bounds.origin.y);
                        let p = |x: f32, y: f32| point(px(ox + x), px(oy + y));
                        let mut stroke = |segments: &[([f32; 2], [f32; 2])]| {
                            let mut pb = PathBuilder::stroke(px(1.75));
                            for &([x1, y1], [x2, y2]) in segments {
                                pb.move_to(p(x1, y1));
                                pb.line_to(p(x2, y2));
                            }
                            if let Ok(path) = pb.build() {
                                window.paint_path(path, icon_color);
                            }
                        };
                        match kind {
                            "dir-both" => stroke(&[
                                ([3.0, h / 2.0], [w - 3.0, h / 2.0]),
                                ([3.0, h / 2.0], [6.0, h / 2.0 - 3.0]),
                                ([3.0, h / 2.0], [6.0, h / 2.0 + 3.0]),
                                ([w - 3.0, h / 2.0], [w - 6.0, h / 2.0 - 3.0]),
                                ([w - 3.0, h / 2.0], [w - 6.0, h / 2.0 + 3.0]),
                            ]),
                            "dir-right" => stroke(&[
                                ([3.0, h / 2.0], [w - 3.0, h / 2.0]),
                                ([w - 3.0, h / 2.0], [w - 6.0, h / 2.0 - 3.0]),
                                ([w - 3.0, h / 2.0], [w - 6.0, h / 2.0 + 3.0]),
                            ]),
                            "dir-left" => stroke(&[
                                ([3.0, h / 2.0], [w - 3.0, h / 2.0]),
                                ([3.0, h / 2.0], [6.0, h / 2.0 - 3.0]),
                                ([3.0, h / 2.0], [6.0, h / 2.0 + 3.0]),
                            ]),
                            "line-straight" => stroke(&[([3.0, h / 2.0], [w - 3.0, h / 2.0])]),
                            "line-bezier" => {
                                let mut pb = PathBuilder::stroke(px(1.75));
                                pb.move_to(p(2.5, h - 4.0));
                                pb.cubic_bezier_to(
                                    p(w - 2.5, 4.0),
                                    p(w * 0.35, h - 4.0),
                                    p(w * 0.65, 4.0),
                                );
                                if let Ok(path) = pb.build() {
                                    window.paint_path(path, icon_color);
                                }
                            }
                            "line-orthogonal" => stroke(&[
                                ([3.0, h - 4.0], [w * 0.45, h - 4.0]),
                                ([w * 0.45, h - 4.0], [w * 0.45, 4.0]),
                                ([w * 0.45, 4.0], [w - 3.0, 4.0]),
                            ]),
                            _ => {}
                        }
                    },
                )
                .w(px(14.0))
                .h(px(14.0))
                .into_any_element()
            };
            pill = pill
                .child(toolbar_divider(grid, vertical))
                .child(
                    div()
                        .px(px(4.0))
                        .text_size(px(11.0))
                        .text_color(text)
                        .child("方向"),
                )
                .child(
                    chip(
                        "wb-mm-dir-both",
                        direction == MindMapRootDirection::Both,
                        draw_mm_icon("wb-mm-dir-both-icon", "dir-both"),
                    )
                    .on_click(cx.listener(move |this, _ev, _window, cx| {
                        this.set_mindmap_root_direction(root_id, MindMapRootDirection::Both, cx);
                    })),
                )
                .child(
                    chip(
                        "wb-mm-dir-right",
                        direction == MindMapRootDirection::Right,
                        draw_mm_icon("wb-mm-dir-right-icon", "dir-right"),
                    )
                    .on_click(cx.listener(move |this, _ev, _window, cx| {
                        this.set_mindmap_root_direction(root_id, MindMapRootDirection::Right, cx);
                    })),
                )
                .child(
                    chip(
                        "wb-mm-dir-left",
                        direction == MindMapRootDirection::Left,
                        draw_mm_icon("wb-mm-dir-left-icon", "dir-left"),
                    )
                    .on_click(cx.listener(move |this, _ev, _window, cx| {
                        this.set_mindmap_root_direction(root_id, MindMapRootDirection::Left, cx);
                    })),
                )
                .child(toolbar_divider(grid, vertical))
                .child(
                    div()
                        .px(px(4.0))
                        .text_size(px(11.0))
                        .text_color(text)
                        .child("连线"),
                )
                .child(
                    chip(
                        "wb-mm-line-straight",
                        connector_style == MindMapConnectorStyle::Straight,
                        draw_mm_icon("wb-mm-line-straight-icon", "line-straight"),
                    )
                    .on_click(cx.listener(move |this, _ev, _window, cx| {
                        this.set_mindmap_connector_style(
                            root_id,
                            MindMapConnectorStyle::Straight,
                            cx,
                        );
                    })),
                )
                .child(
                    chip(
                        "wb-mm-line-bezier",
                        connector_style == MindMapConnectorStyle::Bezier,
                        draw_mm_icon("wb-mm-line-bezier-icon", "line-bezier"),
                    )
                    .on_click(cx.listener(move |this, _ev, _window, cx| {
                        this.set_mindmap_connector_style(
                            root_id,
                            MindMapConnectorStyle::Bezier,
                            cx,
                        );
                    })),
                )
                .child(
                    chip(
                        "wb-mm-line-orthogonal",
                        connector_style == MindMapConnectorStyle::Orthogonal,
                        draw_mm_icon("wb-mm-line-orthogonal-icon", "line-orthogonal"),
                    )
                    .on_click(cx.listener(move |this, _ev, _window, cx| {
                        this.set_mindmap_connector_style(
                            root_id,
                            MindMapConnectorStyle::Orthogonal,
                            cx,
                        );
                    })),
                );
        } else {
            pill = pill
                .child(color_btn)
                .child(width_btn)
                .children(font_btn)
                .children(format_btn)
                .child(toolbar_divider(grid, vertical))
                // tool categories (each opens a flyout of its tools)
                .children(cats)
                .child(templates_btn);
        }
        let pill = pill
            .child(toolbar_divider(grid, vertical))
            // actions
            .child(
                act(0, "wb-icon-undo", UNDO_ICON)
                    .tooltip(self.tip("Undo (⌘Z)"))
                    .on_click(cx.listener(|this, _ev, window, cx| this.undo(window, cx))),
            )
            .child(
                act(1, "wb-icon-redo", REDO_ICON)
                    .tooltip(self.tip("Redo (⌘⇧Z)"))
                    .on_click(cx.listener(|this, _ev, window, cx| this.redo(window, cx))),
            )
            .child(
                act(2, "wb-icon-delete", DELETE_ICON)
                    .tooltip(self.tip("Delete selection (⌫)"))
                    .on_click(
                        cx.listener(|this, _ev, window, cx| this.delete_selected(window, cx)),
                    ),
            );
        // Default top-center; once dragged, an absolute board-relative position
        // (clamped to the board each paint, so a position persisted under a larger
        // window can't strand the bar — and its grip — off-screen).
        let tb_pos = self.toolbar_pos.map(|(x, y)| self.clamp_toolbar(x, y));
        let toolbar = match tb_pos {
            Some((x, y)) => div().absolute().left(px(x)).top(px(y)).child(pill),
            None => div()
                .absolute()
                .top(px(10.0))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(pill),
        };
    (toolbar, tb_pos, vertical, open_group, cur_swatch, tool_btn)
}
