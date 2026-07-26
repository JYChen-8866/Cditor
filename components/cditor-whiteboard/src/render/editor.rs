impl Render for WhiteboardView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.read_only {
            return self.render_read_only(window, cx);
        }
        let WhiteboardStyle {
            bg,
            grid,
            text,
            ink,
            panel,
            panel_strong,
            accent,
            selection,
            swatches,
        } = (self.style)();
        let cam = self.scene.camera;
        let zoom = cam.zoom.max(MIN_ZOOM);
        let bounds_cell = self.bounds.clone();
        let board_bounds = self.bounds.get();
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

        // Decoded bitmaps for image elements, fetched from the host (which decodes
        // off-thread and re-renders when ready). Pre-fetched here — before the
        // element walk below — so the host callback can borrow `window`/`cx`
        // without clashing with the `iter_mut`.
        // Keyed by element id (not src) so two elements sharing a file but at
        // different angles don't collide. The rotation is snapped to a quarter
        // turn (images rotate in 90° steps), so a steady angle hits the host's
        // cache and only re-rotates as the drag crosses a 90° boundary.
        let img_sources: HashMap<u64, gpui::ImageSource> = {
            let items: Vec<(u64, String, f32)> = self
                .scene
                .elements
                .iter()
                .filter(|element| visible_element_ids.contains(&element.id))
                .filter_map(|e| match &e.kind {
                    ElementKind::Image(im) => {
                        Some((e.id, im.src.clone(), snap_quarter(im.rotation)))
                    }
                    _ => None,
                })
                .collect();
            let mut map = HashMap::new();
            if let Some(f) = self.on_image.clone() {
                for (id, src, rot) in items {
                    if let Some(s) = f(&src, rot, window, cx) {
                        map.insert(id, s);
                    }
                }
            }
            map
        };

        // One ordered pass over the elements, building the paint stack as a list
        // of layers in `elements` order (later = on top). Canvas-drawn kinds
        // (shapes / lines / pen / text) accumulate into a "band" canvas; an image
        // or page-card flushes the band and adds its overlay div, so a shape can
        // sit above or below an image. Text is laid out here (measured extent for
        // selection/hit-test + outline segments) so it z-orders and rotates with
        // shapes. Camera-independent glyph outlines are cached by content/style;
        // camera movement only rebuilds their screen-space paths.
        let font = self.font.clone();
        let editing = self.editing;
        let (caret_at, sel_anchor) = (self.caret, self.sel_anchor);
        // A translucent accent fills the selected glyphs (kept readable).
        let sel_fill = gpui::hsla(selection.h, selection.s, selection.l, 0.30);
        let mindmap_connector_styles: HashMap<u64, MindMapConnectorStyle> = self
            .scene
            .elements
            .iter()
            .filter(|element| visible_element_ids.contains(&element.id))
            .filter_map(|element| {
                self.mindmap_connector_style_for_element(&element.kind)
                    .map(|style| (element.id, style))
            })
            .collect();
        let text_layout_cache = &mut self.text_layout_cache;
        let label_layout_cache = &mut self.label_layout_cache;
        let mut layers: Vec<Layer> = Vec::new();
        let mut band: Vec<ElemPaint> = Vec::new();
        for e in self.scene.elements.iter_mut() {
            if !visible_element_ids.contains(&e.id) {
                continue;
            }
            let id = e.id;
            let stroke = e.stroke.map_or(ink, u32_to_hsla);
            let fill = e.fill.map(u32_to_hsla);
            // Disjoint field borrows (vs `&mut e.kind` below) so the text arms can
            // read the label, its color, and the styling without cloning.
            let label = e.label.as_deref();
            let label_color = e.label_color;
            let styles = e.styles.as_slice();
            match &mut e.kind {
                // Page-card: a titled box (top-aligned header + hint) that links
                // to a host page. Subtle border — the accent is the selection.
                ElementKind::Embed(em) => {
                    if !band.is_empty() {
                        layers.push(Layer::Band(std::mem::take(&mut band)));
                    }
                    layers.push(Layer::Overlay(
                        div()
                            .absolute()
                            .left(px((em.x - cam.x) * zoom))
                            .top(px((em.y - cam.y) * zoom))
                            .w(px(em.w * zoom))
                            .h(px(em.h * zoom))
                            .bg(panel)
                            .border_1()
                            .border_color(grid)
                            .rounded(px(8.0))
                            .overflow_hidden()
                            .p(px(10.0 * zoom))
                            .flex()
                            .flex_col()
                            .gap(px(3.0 * zoom))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0 * zoom))
                                    .text_size(px(14.0 * zoom))
                                    .text_color(ink)
                                    .child(div().text_color(accent).child("▤"))
                                    .child(SharedString::from(em.title.clone())),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0 * zoom))
                                    .text_color(text)
                                    .child("Double-click to open"),
                            )
                            .into_any_element(),
                    ));
                }
                // Image: the decoded bitmap (when the host has it ready), placed
                // in the element box's quarter-turn-rotated AABB; else a
                // placeholder while it loads.
                ElementKind::Image(im) => {
                    if !band.is_empty() {
                        layers.push(Layer::Band(std::mem::take(&mut band)));
                    }
                    let rot = snap_quarter(im.rotation);
                    let (bx, by, bw, bh) = if rot.abs() < ROT_EPS {
                        (im.x, im.y, im.w, im.h)
                    } else {
                        let c = box_padded_corners(im.x, im.y, im.w, im.h, rot, 0.0);
                        let (x0, y0, x1, y1) = aabb(&c);
                        (x0, y0, x1 - x0, y1 - y0)
                    };
                    let frame = div()
                        .absolute()
                        .left(px((bx - cam.x) * zoom))
                        .top(px((by - cam.y) * zoom))
                        .w(px(bw * zoom))
                        .h(px(bh * zoom))
                        .overflow_hidden()
                        .rounded(px(2.0));
                    let el = match img_sources.get(&id) {
                        // Set only the width and let gpui derive the height from the
                        // bitmap's aspect (its `Img` forces an `aspect_ratio` from the
                        // image, then ignores it unless a dimension is `Auto` — so
                        // `size_full` makes it overflow the box and clip). The bitmap is
                        // pre-rotated to the box's quarter-turn aspect, so width alone
                        // reproduces the rotated AABB exactly. `Contain` guards rounding.
                        Some(src) => frame.child(
                            gpui::img(src.clone())
                                .w(px(bw * zoom))
                                .object_fit(ObjectFit::Contain),
                        ),
                        None => frame
                            .bg(panel)
                            .border_1()
                            .border_color(grid)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .text_size(px(11.0 * zoom))
                                    .text_color(text)
                                    .child("Loading…"),
                            ),
                    };
                    layers.push(Layer::Overlay(el.into_any_element()));
                }
                // Canvas-drawn kinds: shapes / lines / pen / text.
                kind => {
                    let text = if let ElementKind::Text(t) = kind {
                        let layout = cached_text_layout(
                            text_layout_cache,
                            &font,
                            id,
                            &t.content,
                            t.size,
                            None,
                            styles,
                        );
                        t.measured_w = layout.width;
                        t.measured_h = layout.height;
                        // While editing: the caret at its byte offset and the
                        // selection rects (both text-local).
                        let active = editing == Some(id);
                        let caret = active.then(|| font.caret_pos(&t.content, t.size, caret_at));
                        let (s, e) = (caret_at.min(sel_anchor), caret_at.max(sel_anchor));
                        let selection = if active {
                            font.selection_rects(&t.content, t.size, s, e)
                        } else {
                            Vec::new()
                        };
                        Some(TextOutline {
                            segs: layout.segs.clone(),
                            bold_segs: layout.bold_segs.clone(),
                            bold_width: layout.bold_width,
                            color: stroke,
                            x: t.x,
                            y: t.y,
                            rotation: t.rotation,
                            pivot: [t.x + layout.width / 2.0, t.y + layout.height / 2.0],
                            line_height: layout.line_height,
                            caret,
                            selection,
                            sel_color: sel_fill,
                            decorations: layout.decorations.clone(),
                        })
                    } else if is_closed_shape(kind)
                        && let Some((bx, by, bw, bh, rot)) = box_like(kind)
                        && (editing == Some(id) || label.is_some_and(|s| !s.trim().is_empty()))
                    {
                        // Auto-shrink + word-wrap the label to fit inside the shape,
                        // centered. Its block center coincides with the box center, so
                        // it rotates with the shape. The shared `shape_label_block`
                        // keeps this identical to what the editor's caret math uses.
                        // Built while editing even when empty, so the caret shows the
                        // moment you double-click (before the first keystroke).
                        let active = editing == Some(id);
                        let text = label.map_or("", str::trim);
                        let label_layout = cached_label_layout(
                            label_layout_cache,
                            &font,
                            id,
                            kind,
                            LabelBox {
                                x: bx,
                                y: by,
                                w: bw,
                                h: bh,
                            },
                            text,
                            styles,
                        );
                        let caret = active.then(|| {
                            font.caret_pos_wrapped(
                                text,
                                label_layout.size,
                                Some(label_layout.wrap),
                                caret_at,
                            )
                        });
                        let (s, e) = (caret_at.min(sel_anchor), caret_at.max(sel_anchor));
                        let selection = if active {
                            font.selection_rects_wrapped(
                                text,
                                label_layout.size,
                                Some(label_layout.wrap),
                                s,
                                e,
                            )
                        } else {
                            Vec::new()
                        };
                        Some(TextOutline {
                            segs: label_layout.text.segs.clone(),
                            bold_segs: label_layout.text.bold_segs.clone(),
                            bold_width: label_layout.text.bold_width,
                            color: label_color.map_or(stroke, u32_to_hsla),
                            x: bx + label_layout.offset_x,
                            y: by + label_layout.offset_y,
                            rotation: rot,
                            pivot: [bx + bw / 2.0, by + bh / 2.0],
                            line_height: label_layout.text.line_height,
                            caret,
                            selection,
                            sel_color: sel_fill,
                            decorations: label_layout.text.decorations.clone(),
                        })
                    } else {
                        None
                    };
                    band.push(ElemPaint {
                        kind: kind.clone(),
                        stroke,
                        fill,
                        text,
                        mindmap_connector_style: mindmap_connector_styles.get(&id).copied(),
                    });
                }
            }
        }
        if !band.is_empty() {
            layers.push(Layer::Band(band));
        }

        // The in-progress element previews in the current active color / fill.
        let pending_ink = self.active_stroke.map_or(ink, u32_to_hsla);
        let pending_fill = self.active_fill.map(u32_to_hsla);
        let pending = self.pending.as_ref().map(|p| p.kind.clone());
        // A single selection gets the full box + handles (unless it's the text
        // being edited — then just the caret). A multi-selection shows a single
        // enclosing group box instead of per-element outlines (one box stays
        // legible while rotating), with resize corners and — when at least one
        // member can rotate — a shared rotate grip.
        let single_sel = self
            .selected_single()
            .filter(|id| Some(*id) != self.editing)
            .and_then(|id| self.scene.elements.iter().find(|e| e.id == id))
            .map(|e| e.kind.clone());
        let group_sel = (self.selected.len() > 1)
            .then(|| self.selection_bbox())
            .flatten()
            .map(|bb| (bb, self.group_rotatable()));
        let marquee = self.marquee;
        let alignment_guides = self.alignment_guides;
        let snap_target = self.connecting.and_then(|connection| {
            self.hovered_connector
                .filter(|target| target.id != connection.from.id)
                .and_then(|target| {
                    self.scene
                        .elements
                        .iter()
                        .find(|element| element.id == target.id)
                        .map(|element| (element.kind.clone(), target.index))
                })
        });
        const CONNECTOR_ICONS: [(&str, &[u8]); 4] = [
            ("wb-connector-up", include_bytes!("../../assets/icons/up.svg")),
            (
                "wb-connector-right",
                include_bytes!("../../assets/icons/right.svg"),
            ),
            (
                "wb-connector-down",
                include_bytes!("../../assets/icons/down.svg"),
            ),
            (
                "wb-connector-left",
                include_bytes!("../../assets/icons/left.svg"),
            ),
        ];
        let connector_buttons: Vec<gpui::AnyElement> = single_sel
            .as_ref()
            .filter(|_| self.connecting.is_none() && self.pending.is_none())
            .filter(|kind| connector_capable(kind))
            .map(|kind| {
                connector_button_centers(kind, cam, board_bounds.origin)
                    .into_iter()
                    .enumerate()
                    .map(|(index, center)| {
                        let (key, bytes) = CONNECTOR_ICONS[index];
                        div()
                            .id(("wb-connector-button", index))
                            .absolute()
                            .left(
                                center.x - board_bounds.origin.x - px(CONNECTOR_BUTTON_SIZE / 2.0),
                            )
                            .top(center.y - board_bounds.origin.y - px(CONNECTOR_BUTTON_SIZE / 2.0))
                            .size(px(CONNECTOR_BUTTON_SIZE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(panel_strong)
                            .text_color(selection)
                            .shadow_sm()
                            .cursor_pointer()
                            .child(svg_icon(key, bytes, selection, CONNECTOR_BUTTON_SIZE))
                            .into_any_element()
                    })
                    .collect()
            })
            .unwrap_or_default();
        const ROTATE_ICON: &[u8] = include_bytes!("../../assets/icons/refresh.svg");
        let rotate_position = single_sel
            .as_ref()
            .filter(|kind| rotatable(kind))
            .map(|kind| rotate_handle_screen(kind, cam, board_bounds.origin))
            .or_else(|| {
                group_sel
                    .filter(|(_, can_rotate)| *can_rotate)
                    .map(|(bounds, _)| rotate_handle_for_bbox(bounds, cam, board_bounds.origin))
            });
        let rotate_button = rotate_position.map(|(x, y)| {
            div()
                .id("wb-rotate-button")
                .absolute()
                .left(px(x) - board_bounds.origin.x - px(CONNECTOR_BUTTON_SIZE / 2.0))
                .top(px(y) - board_bounds.origin.y - px(CONNECTOR_BUTTON_SIZE / 2.0))
                .size(px(CONNECTOR_BUTTON_SIZE))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(panel_strong)
                .shadow_sm()
                .cursor_pointer()
                .child(svg_icon(
                    "wb-icon-refresh",
                    ROTATE_ICON,
                    selection,
                    CONNECTOR_BUTTON_SIZE - 4.0,
                ))
        });
        let selected_mindmap_root = self.selected_mindmap_root();

        let (toolbar, tb_pos, vertical, open_group, cur_swatch, tool_btn) =
            include!("toolbar.rs");
        let (
            flyout,
            format_panel,
            width_flyout,
            font_flyout,
            menu,
            text_submenu,
            popover_anchor,
        ) = include!("tool_popovers.rs");
        let templates_modal = include!("templates_modal.rs");
        let picker_panel = include!("color_picker.rs");
        // Pan tool shows a grab cursor (closed while dragging) to read as "drag
        // to move the canvas"; other tools use the default arrow.
        let board_cursor = if self.panning {
            CursorStyle::ClosedHand
        } else if self.tool == Tool::Pan {
            CursorStyle::OpenHand
        } else {
            CursorStyle::Arrow
        };

        // The board paints as a stack of layers (back → front): the grid /
        // background; then the element layers (canvas "bands" interleaved with
        // image / page-card overlays, in z-order); then a top "chrome" canvas for
        // the in-progress element, selection box, and marquee — kept above the
        // content so handles stay visible over images.
        let board_layer = canvas(
            move |bounds, _, _| bounds_cell.set(bounds),
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
        let chrome_layer = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                if let Some(k) = &pending {
                    paint_element(
                        k,
                        None,
                        cam,
                        bounds.origin,
                        pending_ink,
                        pending_fill,
                        window,
                    );
                }
                if let Some(k) = &single_sel {
                    paint_selection(k, cam, bounds.origin, selection, window);
                }
                if let Some((kind, active)) = &snap_target {
                    paint_snap_points(kind, *active, cam, bounds.origin, selection, window);
                }
                // Group: resize handles without an enclosing blue frame, plus a
                // shared rotate grip when the group can rotate.
                if let Some((bb, _can_rotate)) = group_sel {
                    let tl = to_screen(bb.0, bb.1, cam, bounds.origin);
                    let br = to_screen(bb.2, bb.3, cam, bounds.origin);
                    let m = 0.0;
                    let (x0, y0) = (f32::from(tl.x) - m, f32::from(tl.y) - m);
                    let (x1, y1) = (f32::from(br.x) + m, f32::from(br.y) + m);
                    // Four corners (proportional) plus four edge midpoints (per-axis
                    // stretch). The midpoints align with `handle_hit`'s edge grips.
                    let (mx, my) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                    for (hx, hy) in [
                        (x0, y0),
                        (x1, y0),
                        (x0, y1),
                        (x1, y1),
                        (mx, y0),
                        (mx, y1),
                        (x0, my),
                        (x1, my),
                    ] {
                        draw_handle(hx, hy, selection, window);
                    }
                }
                if let Some((a, b)) = marquee {
                    paint_marquee(a, b, cam, bounds.origin, selection, window);
                }
                paint_alignment_guides(alignment_guides, bounds, cam, selection, window);
            },
        )
        .absolute()
        .size_full();

        let root = div()
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .overflow_hidden()
            .cursor(board_cursor)
            .child(board_layer)
            .children(connector_buttons)
            .children(rotate_button)
            .child(
                div()
                    .absolute()
                    .size_full()
                    .child(WhiteboardInputElement::new(cx.entity())),
            )
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_left_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_left_up))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_right_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_middle_down))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_middle_up))
            .on_mouse_move(cx.listener(Self::on_move));
        let root = if accepts_wheel_input(self.read_only) {
            root.on_scroll_wheel(cx.listener(Self::on_scroll))
        } else {
            root
        };
        root.on_pinch(cx.listener(Self::on_pinch))
            .on_key_down(cx.listener(Self::on_key))
            // Files dragged from the OS land as `ExternalPaths`; hand them to the
            // host (which imports any images) at the drop point.
            .on_drop::<gpui::ExternalPaths>(cx.listener(
                |this, paths: &gpui::ExternalPaths, window, cx| {
                    if let Some(f) = this.on_drop_files.clone() {
                        let w = this.event_to_world(window.mouse_position());
                        f(paths.paths().to_vec(), w[0], w[1], window, cx);
                    }
                },
            ))
            .children(element_layers)
            .child(chrome_layer)
            .child(toolbar)
            .children(flyout)
            .children(format_panel)
            .children(width_flyout)
            .children(font_flyout)
            .children(menu)
            .children(text_submenu)
            .children(picker_panel)
            .children(templates_modal)
            .child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .bottom(px(8.0))
                    .text_size(px(11.0))
                    .text_color(text)
                    .child(SharedString::from(format!("{:.0}%", cam.zoom * 100.0))),
            )
            .into_any_element()
    }
}
