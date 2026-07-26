impl WhiteboardView {
    fn on_left_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.panning {
            return;
        }
        if self.read_only {
            self.panning = true;
            self.last = ev.position;
            return;
        }

        // The draggable toolbar (its pill isn't occluded — like the picker): a
        // press on the grip starts a drag (double-click resets to top-center); a
        // press anywhere else on the pill is consumed so its buttons handle their
        // own clicks. Both must be caught before any canvas logic below.
        if self.toolbar_grip_bounds.get().contains(&ev.position) {
            self.start_toolbar_drag(ev.position, ev.click_count >= 2, window, cx);
            return;
        }
        if self.toolbar_bounds.get().contains(&ev.position) {
            return;
        }

        // A press dismisses an open right-click menu (its own button is occluded,
        // so a press reaching here is outside it).
        if self.context_menu.take().is_some() {
            cx.notify();
            return;
        }
        // A press on the canvas closes an open tool flyout (the flyout itself is
        // occluded, so a press reaching here is outside it).
        if self.open_group.is_some() {
            self.open_group = None;
            cx.notify();
            return;
        }
        // Same for the font flyout (occluded; a press here is outside it).
        if self.font_open {
            self.font_open = false;
            cx.notify();
            return;
        }
        // The thickness flyout: a press on its slider starts a width drag; a press
        // elsewhere on the panel is consumed (presets fire via their own `on_click`);
        // a press outside dismisses it. The panel isn't occluded so drags reach here,
        // like the color picker.
        if self.width_open {
            let pos = ev.position;
            if self.width_bounds.get().contains(&pos) {
                if !self.selected.is_empty() {
                    self.push_undo();
                }
                self.picker_drag = Some(PickerDrag::Width);
                let w = Self::width_from_frac(self.frac_x(self.width_bounds.get(), pos));
                self.set_width_live(w, cx);
                return;
            }
            if self.width_panel_bounds.get().contains(&pos) {
                return;
            }
            self.width_open = false;
            cx.notify();
            return;
        }

        // The color picker takes input priority while open. Its draggable regions
        // (SV square, hue strip) start a drag here; presses on the rest of the
        // panel are consumed (the swatch / Auto buttons fire via their own
        // `on_click`); a press anywhere else closes it.
        if self.picker.is_some() {
            let pos = ev.position;
            if self.sv_bounds.get().contains(&pos) {
                if !self.selected.is_empty() {
                    self.push_undo();
                }
                self.picker_drag = Some(PickerDrag::Sv);
                let (s, v) = self.sv_from_pos(pos);
                if let Some(p) = self.picker.as_mut() {
                    (p.s, p.v) = (s, v);
                }
                if let Some(c) = self.picker_u32() {
                    self.set_color_live(Some(c), cx);
                }
                return;
            }
            if self.hue_bounds.get().contains(&pos) {
                if !self.selected.is_empty() {
                    self.push_undo();
                }
                self.picker_drag = Some(PickerDrag::Hue);
                let h = self.frac_x(self.hue_bounds.get(), pos);
                if let Some(p) = self.picker.as_mut() {
                    p.h = h;
                }
                if let Some(c) = self.picker_u32() {
                    self.set_color_live(Some(c), cx);
                }
                return;
            }
            if self.alpha_bounds.get().contains(&pos) {
                if !self.selected.is_empty() {
                    self.push_undo();
                }
                self.picker_drag = Some(PickerDrag::Alpha);
                let a = self.frac_x(self.alpha_bounds.get(), pos);
                if let Some(p) = self.picker.as_mut() {
                    p.a = a;
                }
                if let Some(c) = self.picker_u32() {
                    self.set_color_live(Some(c), cx);
                }
                return;
            }
            if self.picker_bounds.get().contains(&pos) {
                return;
            }
            self.picker = None;
            cx.notify();
            return;
        }

        // Take keyboard focus so the board's shortcuts (tool keys, ⌫, ⌘Z…) work
        // after a click on the canvas.
        self.focus.focus(window, cx);

        let p = self.event_to_world(ev.position);
        let zoom = self.scene.camera.zoom.max(MIN_ZOOM);

        // A press inside the text being edited drives its caret / selection (no
        // commit). A press anywhere else commits the edit, then falls through.
        if let Some(id) = self.editing {
            if self.point_in_editing_text(id, p) {
                self.place_caret_from_click(id, p, ev, window, cx);
                return;
            }
            self.commit_text(window, cx);
        }

        // Ctrl + left-drag always pans the canvas, regardless of the active tool
        // or what's under the pointer. Reuses the same panning state as the Pan
        // tool and middle-button drag.
        if ev.modifiers.control {
            self.panning = true;
            self.last = ev.position;
            return;
        }

        if ev.click_count >= 2 {
            self.pending = None;
            // Existing text and shape labels have one consistent edit gesture:
            // double-click, regardless of which tool happened to be active.
            if let Some(id) = self.text_at(p, SELECT_PAD / zoom) {
                self.selected = vec![id];
                self.editing = Some(id);
                self.place_caret_from_click(id, p, ev, window, cx);
                return;
            }
            if let Some(id) = self.shape_at(p, SELECT_PAD / zoom) {
                self.selected = vec![id];
                self.editing = Some(id);
                self.place_caret_from_click(id, p, ev, window, cx);
                return;
            }
            if self.tool == Tool::Select {
                // Double-click a page-card opens its page.
                if let Some((id, page_id)) = self.embed_at(p, SELECT_PAD / zoom) {
                    self.selected = vec![id];
                    if let Some(f) = self.on_open.clone() {
                        f(page_id, window, cx);
                    }
                    cx.notify();
                    return;
                }
            }
            self.reset_view(cx);
            return;
        }

        // A single click on any existing element always means selection, even if
        // a drawing/text tool is currently active. This prevents accidentally
        // drawing over a shape when the user only meant to select it. A second
        // click (handled above) is the sole path into text/label editing.
        if self.tool != Tool::Select {
            let pad = SELECT_PAD / zoom;
            let hit = self
                .scene
                .elements
                .iter()
                .rev()
                .find(|element| hit_test(&element.kind, p[0], p[1], pad))
                .map(|element| element.id);
            if let Some(id) = hit {
                self.tool = Tool::Select;
                if ev.modifiers.shift {
                    if let Some(index) = self.selected.iter().position(|&selected| selected == id) {
                        self.selected.remove(index);
                    } else {
                        self.selected.push(id);
                    }
                } else {
                    self.selected = vec![id];
                }
                self.drag_from = None;
                cx.notify();
                return;
            }
        }

        // Pan tool: a left-drag pans the canvas (the default navigation tool;
        // double-click above still recenters). Reuses the middle-drag machinery.
        if self.tool == Tool::Pan {
            self.panning = true;
            self.last = ev.position;
            return;
        }

        if self.tool == Tool::Text {
            // A single click on existing text only selects it. Editing existing
            // content is deliberately double-click-only; clicking empty canvas
            // still creates a fresh text element and immediately edits that.
            if let Some(id) = self.text_at(p, SELECT_PAD / zoom) {
                self.selected = vec![id];
                cx.notify();
            } else {
                self.push_undo();
                let id = self.next_id;
                self.next_id += 1;
                self.scene.elements.push(Element {
                    id,
                    kind: ElementKind::Text(TextGeom {
                        x: p[0],
                        y: p[1],
                        content: String::new(),
                        size: TEXT_SIZE / zoom,
                        rotation: 0.0,
                        measured_w: 0.0,
                        measured_h: 0.0,
                    }),
                    stroke: self.active_stroke,
                    fill: None,
                    label: None,
                    label_color: None,
                    styles: Vec::new(),
                    mindmap: None,
                });
                self.selected = vec![id];
                self.begin_text_edit(id, 0, window, cx);
                self.dirty = true;
                cx.notify();
            }
            return;
        }

        if self.tool == Tool::MindMap {
            self.add_mindmap_seed(p[0], p[1], cx);
            return;
        }

        if self.tool == Tool::Flowchart {
            self.add_flowchart_seed(p[0], p[1], cx);
            return;
        }

        if self.tool == Tool::Embed {
            // The host picks a page, then calls back into `add_embed`.
            if let Some(f) = self.on_place_embed.clone() {
                f(p[0], p[1], window, cx);
            }
            return;
        }

        if self.tool == Tool::Image {
            // The host picks an image file, then calls back into `add_image_at`.
            if let Some(f) = self.on_place_image.clone() {
                f(p[0], p[1], window, cx);
            }
            return;
        }

        if self.tool == Tool::Select {
            // A connector point on a hovered shape starts drawing a line from that
            // exact side/midpoint without switching away from Select.
            if let Some(cp) = self.connector_at(ev.position) {
                let width = self.active_width / zoom;
                self.pending = Some(Pending {
                    anchor: cp.pos,
                    kind: ElementKind::Arrow(SegGeom {
                        x1: cp.pos[0],
                        y1: cp.pos[1],
                        x2: cp.pos[0],
                        y2: cp.pos[1],
                        width,
                        style: SegmentStyle::Solid,
                        start_anchor: Some(SegmentAnchor {
                            element_id: cp.id,
                            connector: cp.index,
                        }),
                        end_anchor: None,
                    }),
                });
                self.connecting = Some(ConnectDrag { from: cp });
                self.hovered_connector = Some(cp);
                cx.notify();
                return;
            }
            // A handle on the current selection takes priority.
            if let Some(grab) = self.handle_hit(ev.position) {
                self.push_undo();
                match grab {
                    HandleGrab::Corner(rs) => self.resizing = Some(rs),
                    HandleGrab::GroupCorner(gr) => self.group_resizing = Some(gr),
                    HandleGrab::Endpoint(ep) => self.endpoint = Some(ep),
                    HandleGrab::Rotate => {
                        // Pivot = the whole selection's bounds center (a single
                        // element's own center, or the group's). Snap on the lone
                        // element's orientation, or — for a group — the first
                        // oriented member's, so it squares to horizontal/vertical
                        // (falling back to quarter-turns if nothing's oriented).
                        if let Some(bb) = self.selection_bbox() {
                            let center = [(bb.0 + bb.2) / 2.0, (bb.1 + bb.3) / 2.0];
                            let base = match self.selected_single() {
                                Some(id) => self
                                    .scene
                                    .elements
                                    .iter()
                                    .find(|e| e.id == id)
                                    .and_then(|e| reference_angle(&e.kind)),
                                None => self
                                    .scene
                                    .elements
                                    .iter()
                                    .filter(|e| self.is_selected(e.id))
                                    .find_map(|e| reference_angle(&e.kind))
                                    .or(Some(0.0)),
                            };
                            let start_pointer = (p[1] - center[1]).atan2(p[0] - center[0]);
                            self.rotating = Some(Rotating {
                                center,
                                start_pointer,
                                applied: 0.0,
                                base,
                            });
                        }
                    }
                }
                cx.notify();
                return;
            }
            // Otherwise hit-test topmost-first.
            let pad = SELECT_PAD / zoom;
            let hit = self
                .scene
                .elements
                .iter()
                .rev()
                .find(|e| hit_test(&e.kind, p[0], p[1], pad))
                .map(|e| e.id);
            match hit {
                Some(id) if ev.modifiers.shift => {
                    // Shift-click toggles membership (no move).
                    if let Some(pos) = self.selected.iter().position(|&s| s == id) {
                        self.selected.remove(pos);
                    } else {
                        self.selected.push(id);
                    }
                    self.drag_from = None;
                }
                Some(id) => {
                    // Click an unselected element selects only it; clicking one
                    // already in the selection keeps the group (so a drag moves
                    // them all). Either way, arm a move.
                    if !self.is_selected(id) {
                        self.selected = vec![id];
                    }
                    self.drag_from = Some(p);
                    // Capture the primary element's top-left so the move can drive
                    // an absolute target (and snap it) without drifting.
                    self.move_origin = self
                        .selected
                        .first()
                        .and_then(|&pid| self.scene.elements.iter().find(|e| e.id == pid))
                        .map(|e| {
                            let (x, y, ..) = bbox(&e.kind);
                            [x, y]
                        })
                        .unwrap_or(p);
                    self.moved = false;
                }
                None => {
                    // Empty space: clear (unless extending) and start a marquee.
                    if !ev.modifiers.shift {
                        self.selected.clear();
                    }
                    self.marquee = Some((p, p));
                    self.drag_from = None;
                }
            }
            cx.notify();
            return;
        }

        let width = self.active_width / zoom;
        // While the snap modifier (Option) is held, start the shape on a grid
        // line; the move handler snaps the opposite corner / endpoint too.
        let anchor = if ev.modifiers.alt {
            [snap_grid(p[0]), snap_grid(p[1])]
        } else {
            p
        };
        // A zero-size box anchored at the press; the move handler grows it.
        let box0 = BoxGeom {
            x: anchor[0],
            y: anchor[1],
            w: 0.0,
            h: 0.0,
            width,
            rotation: 0.0,
        };
        let kind = match self.tool {
            // Freehand keeps the raw point — strokes aren't grid-aligned.
            Tool::Pen => ElementKind::Draw(Stroke {
                points: vec![p],
                width,
            }),
            Tool::Rect => ElementKind::Rect(box0),
            Tool::Ellipse => ElementKind::Ellipse(box0),
            Tool::Diamond => ElementKind::Diamond(box0),
            Tool::Triangle => ElementKind::Triangle(box0),
            Tool::RoundRect => ElementKind::RoundRect(box0),
            Tool::Star => ElementKind::Star(box0),
            Tool::Hexagon => ElementKind::Hexagon(box0),
            Tool::Line => ElementKind::Line(SegGeom {
                x1: anchor[0],
                y1: anchor[1],
                x2: anchor[0],
                y2: anchor[1],
                width,
                style: SegmentStyle::Solid,
                start_anchor: None,
                end_anchor: None,
            }),
            Tool::Arrow => ElementKind::Arrow(SegGeom {
                x1: anchor[0],
                y1: anchor[1],
                x2: anchor[0],
                y2: anchor[1],
                width,
                style: SegmentStyle::Solid,
                start_anchor: None,
                end_anchor: None,
            }),
            Tool::DashedArrow => ElementKind::Arrow(SegGeom {
                x1: anchor[0],
                y1: anchor[1],
                x2: anchor[0],
                y2: anchor[1],
                width,
                style: SegmentStyle::Dashed,
                start_anchor: None,
                end_anchor: None,
            }),
            // These tools don't create a drag-element here (handled earlier).
            Tool::Pan
            | Tool::Select
            | Tool::Text
            | Tool::MindMap
            | Tool::Flowchart
            | Tool::Embed
            | Tool::Image => return,
        };
        self.pending = Some(Pending { anchor, kind });
        cx.notify();
    }

}
