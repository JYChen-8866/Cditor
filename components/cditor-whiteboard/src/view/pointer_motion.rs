impl WhiteboardView {
    fn on_left_up(&mut self, _ev: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Finish a toolbar drag (persist the new position).
        if self.toolbar_drag.is_some() {
            self.commit_toolbar_drag(window, cx);
            return;
        }
        // End a text-selection drag (the selection is applied live in `on_move`).
        if self.text_selecting {
            self.text_selecting = false;
            return;
        }
        // End a Pan-tool drag (left-button pan).
        if self.panning {
            self.panning = false;
            cx.notify();
            self.flush(window, cx);
            return;
        }
        // End a picker drag: the live changes are already applied; just persist.
        if self.picker_drag.take().is_some() {
            self.flush(window, cx);
            return;
        }
        if self.resizing.take().is_some()
            || self.group_resizing.take().is_some()
            || self.endpoint.take().is_some()
            || self.rotating.take().is_some()
        {
            self.dirty = true;
            cx.notify();
            self.flush(window, cx);
            return;
        }
        if self.drag_from.take().is_some() {
            if self.moved {
                self.dirty = true;
            }
            self.moved = false;
            self.alignment_guides = AlignmentGuides::default();
            cx.notify();
            self.flush(window, cx);
            return;
        }
        // Finish a marquee: add every element whose bounds intersect the box.
        if let Some((a, b)) = self.marquee.take() {
            let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
            let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
            for e in &self.scene.elements {
                let bb = bbox(&e.kind);
                let hits = bb.0 <= x1 && bb.2 >= x0 && bb.1 <= y1 && bb.3 >= y0;
                if hits && !self.selected.contains(&e.id) {
                    self.selected.push(e.id);
                }
            }
            cx.notify();
            return;
        }
        if let Some(pending) = self.pending.take() {
            let completed_connection = self.connecting.take().is_some();
            if committable(&pending.kind) {
                self.push_undo();
                let id = self.next_id;
                self.next_id += 1;
                // Fill applies only to closed shapes.
                let fill = if is_closed_shape(&pending.kind) {
                    self.active_fill
                } else {
                    None
                };
                self.scene.elements.push(Element {
                    id,
                    kind: pending.kind,
                    stroke: self.active_stroke,
                    fill,
                    label: None,
                    label_color: self.active_text,
                    styles: Vec::new(),
                    mindmap: None,
                });
                if completed_connection {
                    // The newly-created connector becomes the active object so
                    // its endpoints can be adjusted immediately.
                    self.selected = vec![id];
                    self.focus.focus(window, cx);
                }
                self.dirty = true;
            }
            cx.notify();
        }
        self.flush(window, cx);
    }

    /// Right-click: with a selection (and a host save hook), open a small menu to
    /// save it as a template; otherwise just dismiss any open menu.
    fn on_right_down(&mut self, ev: &MouseDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            self.context_menu = None;
            cx.notify();
            return;
        }
        // A right-click inside the open color picker (e.g. removing a saved swatch)
        // shouldn't also open the board context menu.
        if self.picker.is_some() && self.picker_bounds.get().contains(&ev.position) {
            return;
        }
        // Show the menu when there's a selection (copy / cut / z-order / save) or
        // paste is wired (so you can paste onto empty canvas). Positioned at the click.
        if self.selected.is_empty() && self.on_paste.is_none() {
            self.context_menu = None;
        } else {
            let b = self.bounds.get();
            self.context_menu = Some(point(
                ev.position.x - b.origin.x,
                ev.position.y - b.origin.y,
            ));
            self.ctx_text_sub = false;
        }
        cx.notify();
    }

    /// Paste board elements from the clipboard (via the host's `on_paste` hook),
    /// centered + selected. Returns whether anything was pasted, so ⌘V can fall
    /// through to image paste when the clipboard holds no board elements.
    fn try_paste(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(f) = self.on_paste.clone()
            && let Some(json) = f(window, cx)
        {
            self.paste_elements(&json, window, cx);
            true
        } else {
            false
        }
    }

    /// Context-menu Paste.
    fn paste_from_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.context_menu = None;
        self.try_paste(window, cx);
    }

    fn on_middle_down(
        &mut self,
        ev: &MouseDownEvent,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if self.pending.is_some()
            || self.drag_from.is_some()
            || self.resizing.is_some()
            || self.group_resizing.is_some()
            || self.endpoint.is_some()
            || self.rotating.is_some()
            || self.picker_drag.is_some()
            || self.marquee.is_some()
        {
            return;
        }
        self.panning = true;
        self.last = ev.position;
    }

    fn on_middle_up(&mut self, _ev: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.panning {
            self.panning = false;
            cx.notify();
        }
        self.flush(window, cx);
    }

    fn on_move(&mut self, ev: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // Dragging the toolbar (its pill follows the cursor).
        if self.toolbar_drag.is_some() {
            self.drag_toolbar(ev.position, cx);
            return;
        }
        // Extending a text selection by dragging — the caret tracks the cursor
        // while the anchor stays put.
        if self.text_selecting
            && let Some(id) = self.editing
            && let Some(tg) = self.edit_target(id)
        {
            let local = block_local(
                tg.x,
                tg.y,
                tg.rotation,
                tg.pivot,
                self.event_to_world(ev.position),
            );
            self.caret = self
                .font
                .index_at_wrapped(&tg.content, tg.size, tg.wrap, local);
            cx.notify();
            return;
        }
        // Dragging a line out of a connector point. Snaps the endpoint to another
        // connector if the cursor is close to one, so shape-to-shape links land cleanly.
        if let Some(conn) = self.connecting {
            let target = self.snap_connector_at(ev.position, conn.from.id);
            let cur = if let Some((target, snapped)) = target {
                self.hovered_connector = Some(target);
                if snapped {
                    target.pos
                } else {
                    self.event_to_world(ev.position)
                }
            } else {
                self.hovered_connector = None;
                self.event_to_world(ev.position)
            };
            if let Some(pending) = self.pending.as_mut()
                && let ElementKind::Line(s) | ElementKind::Arrow(s) = &mut pending.kind
            {
                s.x1 = conn.from.pos[0];
                s.y1 = conn.from.pos[1];
                s.x2 = cur[0];
                s.y2 = cur[1];
                s.start_anchor = Some(SegmentAnchor {
                    element_id: conn.from.id,
                    connector: conn.from.index,
                });
                s.end_anchor = target.and_then(|(target, snapped)| {
                    snapped.then_some(SegmentAnchor {
                        element_id: target.id,
                        connector: target.index,
                    })
                });
            }
            cx.notify();
            return;
        }
        // Dragging inside the color picker (SV square, hue strip, alpha strip) or
        // the thickness flyout's width slider.
        if let Some(drag) = self.picker_drag {
            let pos = ev.position;
            if drag == PickerDrag::Width {
                let w = Self::width_from_frac(self.frac_x(self.width_bounds.get(), pos));
                self.set_width_live(w, cx);
                return;
            }
            match drag {
                PickerDrag::Sv => {
                    let (s, v) = self.sv_from_pos(pos);
                    if let Some(p) = self.picker.as_mut() {
                        (p.s, p.v) = (s, v);
                    }
                }
                PickerDrag::Hue => {
                    let h = self.frac_x(self.hue_bounds.get(), pos);
                    if let Some(p) = self.picker.as_mut() {
                        p.h = h;
                    }
                }
                PickerDrag::Alpha => {
                    let a = self.frac_x(self.alpha_bounds.get(), pos);
                    if let Some(p) = self.picker.as_mut() {
                        p.a = a;
                    }
                }
                PickerDrag::Width => unreachable!("handled above"),
            }
            if let Some(c) = self.picker_u32() {
                self.set_color_live(Some(c), cx);
            }
            return;
        }
        // Rotating the selection (rotate-handle drag). Shift snaps to 15° steps.
        if let Some(mut rot) = self.rotating.take() {
            let cur = self.event_to_world(ev.position);
            let ang = (cur[1] - rot.center[1]).atan2(cur[0] - rot.center[0]);
            let mut total = ang - rot.start_pointer;
            match rot.base {
                // Box/text/line: work in absolute orientation so Shift gives
                // clean 15° angles and, unmodified, it snaps to horizontal /
                // vertical when within ROT_SNAP (the easy-squaring the user wants).
                Some(base) => total = snap_angle(base + total, ev.modifiers.shift) - base,
                // Freehand: no absolute orientation; Shift still steps relatively.
                None => {
                    if ev.modifiers.shift {
                        let step = std::f32::consts::PI / 12.0;
                        total = (total / step).round() * step;
                    }
                }
            }
            // Apply only the change since last frame, normalized to [-π, π] so the
            // atan2 wrap-around at ±π doesn't spin the element a full turn.
            let tau = std::f32::consts::TAU;
            let mut delta = total - rot.applied;
            delta -= (delta / tau).round() * tau;
            // Every selected element turns about the shared pivot (a single
            // selection is just the one, pivoting on its own center).
            let sel = self.selected.clone();
            for e in self.scene.elements.iter_mut() {
                if sel.contains(&e.id) {
                    rotate_element(&mut e.kind, rot.center[0], rot.center[1], delta);
                }
            }
            self.sync_segment_anchors_for(&sel);
            rot.applied += delta;
            self.rotating = Some(rot);
            cx.notify();
            return;
        }
        // Resizing a multi-selection by a group-bounds corner: scale every
        // member uniformly (proportional) about the opposite corner, each from
        // its geometry at grab so the scaling never compounds.
        if let Some(gr) = self.group_resizing.take() {
            let cur = self.event_to_world(ev.position);
            let mut target = [cur[0] + gr.grab[0], cur[1] + gr.grab[1]];
            if ev.modifiers.alt {
                target = [snap_grid(target[0]), snap_grid(target[1])];
            }
            // A corner scales both axes together (proportional); an edge stretches
            // just its own axis, the other held at 1.
            let (sx, sy) = match gr.handle {
                ResizeHandle::Corner => {
                    let s = diagonal_scale(gr.anchor, gr.from, target);
                    (s, s)
                }
                ResizeHandle::EdgeX => (axis_scale(gr.anchor[0], gr.from[0], target[0]), 1.0),
                ResizeHandle::EdgeY => (1.0, axis_scale(gr.anchor[1], gr.from[1], target[1])),
            };
            let font = self.font.clone();
            for (id, orig) in &gr.orig {
                let mut kind = orig.clone();
                resize_about(&mut kind, gr.anchor[0], gr.anchor[1], sx, sy);
                if let ElementKind::Text(t) = &mut kind {
                    let (w, h) = font.measure(&t.content, t.size);
                    (t.measured_w, t.measured_h) = (w, h);
                }
                if let Some(e) = self.scene.elements.iter_mut().find(|e| e.id == *id) {
                    e.kind = kind;
                }
            }
            let changed: Vec<u64> = gr.orig.iter().map(|(id, _)| *id).collect();
            self.sync_segment_anchors_for(&changed);
            self.group_resizing = Some(gr);
            cx.notify();
            return;
        }
        // Resizing the selection (corner- or edge-handle drag).
        if let Some(r) = self.resizing.as_ref() {
            let (id, handle, anchor, from, grab, mut kind) =
                (r.id, r.handle, r.anchor, r.from, r.grab, r.orig.clone());
            let cur = self.event_to_world(ev.position);
            // Where the dragged handle should sit: cursor + the grab offset, so it
            // tracks the cursor without jumping when the drag starts. The snap
            // modifier (Option) lands it on the grid.
            let mut target = [cur[0] + grab[0], cur[1] + grab[1]];
            if ev.modifiers.alt {
                target = [snap_grid(target[0]), snap_grid(target[1])];
            }
            let (sx, sy) = match handle {
                // An edge grip stretches just its axis (the explicit per-axis ask,
                // so it overrides the proportional defaults — even for text/image).
                ResizeHandle::EdgeX => (axis_scale(anchor[0], from[0], target[0]), 1.0),
                ResizeHandle::EdgeY => (1.0, axis_scale(anchor[1], from[1], target[1])),
                // A corner: text and images scale proportionally (text is a single
                // font size; an image would distort otherwise); Shift does so for
                // shapes; and a *rotated* box-like element must (its anchor is the
                // center, so a uniform scale keeps it correct under rotation). All
                // use the diagonal projection so the corner tracks the cursor at the
                // right rate. Otherwise free resize is per-axis.
                ResizeHandle::Corner => {
                    let rotated = box_like(&kind).is_some_and(|(.., r)| r.abs() > ROT_EPS);
                    let proportional = ev.modifiers.shift
                        || rotated
                        || matches!(kind, ElementKind::Text(_) | ElementKind::Image(_));
                    if proportional {
                        let s = diagonal_scale(anchor, from, target);
                        (s, s)
                    } else {
                        (
                            axis_scale(anchor[0], from[0], target[0]),
                            axis_scale(anchor[1], from[1], target[1]),
                        )
                    }
                }
            };
            resize_about(&mut kind, anchor[0], anchor[1], sx, sy);
            let font = self.font.clone();
            if let Some(e) = self.scene.elements.iter_mut().find(|e| e.id == id) {
                e.kind = kind;
                // Re-measure text now so its box tracks the cursor this frame.
                if let ElementKind::Text(t) = &mut e.kind {
                    let (w, h) = font.measure(&t.content, t.size);
                    t.measured_w = w;
                    t.measured_h = h;
                }
            }
            self.sync_segment_anchors_for(&[id]);
            cx.notify();
            return;
        }
        // Dragging a line/arrow endpoint (Shift snaps the angle to 45°, Option
        // snaps the endpoint to the grid).
        if let Some(ep) = self.endpoint {
            let cur = self.event_to_world(ev.position);
            let shift = ev.modifiers.shift;
            if let Some(e) = self.scene.elements.iter_mut().find(|e| e.id == ep.id)
                && let ElementKind::Line(s) | ElementKind::Arrow(s) = &mut e.kind
            {
                let (ox, oy) = if ep.which == 0 {
                    (s.x2, s.y2)
                } else {
                    (s.x1, s.y1)
                };
                let (nx, ny) = if shift {
                    snap_45(ox, oy, cur[0], cur[1])
                } else if ev.modifiers.alt {
                    (snap_grid(cur[0]), snap_grid(cur[1]))
                } else {
                    (cur[0], cur[1])
                };
                if ep.which == 0 {
                    s.x1 = nx;
                    s.y1 = ny;
                    s.start_anchor = None;
                } else {
                    s.x2 = nx;
                    s.y2 = ny;
                    s.end_anchor = None;
                }
            }
            if !ev.modifiers.shift && !ev.modifiers.alt {
                if let Some((target, snapped)) = self.snap_connector_at(ev.position, ep.id)
                    && snapped
                {
                    self.hovered_connector = Some(target);
                    self.set_segment_endpoint_anchor(
                        ep.id,
                        ep.which,
                        Some(SegmentAnchor {
                            element_id: target.id,
                            connector: target.index,
                        }),
                    );
                } else {
                    self.hovered_connector = None;
                }
            } else {
                self.hovered_connector = None;
            }
            cx.notify();
            return;
        }
        // Moving the selection (all selected elements together). The target is
        // the primary's grab position plus the *total* cursor delta from the
        // fixed grab anchor; the snap modifier (Option) rounds that target to the
        // grid. Computing the absolute target each frame (vs. snapping the
        // per-frame delta) keeps the shape under the cursor and never loses
        // sub-grid motion — so it moves on every axis, not just one.
        if let Some(from) = self.drag_from {
            let cur = self.event_to_world(ev.position);
            let target = move_target(self.move_origin, from, cur, ev.modifiers.alt);
            // Where the primary sits now → the delta to apply this frame. Every
            // element kind's bbox-min translates 1:1, so this tracks exactly.
            let cur_min = self
                .selected
                .first()
                .and_then(|&pid| self.scene.elements.iter().find(|e| e.id == pid))
                .map(|e| {
                    let (x, y, ..) = bbox(&e.kind);
                    [x, y]
                })
                .unwrap_or(self.move_origin);
            let (raw_dx, raw_dy) = (target[0] - cur_min[0], target[1] - cur_min[1]);
            let (dx, dy, guides) = if ev.modifiers.alt {
                (raw_dx, raw_dy, AlignmentGuides::default())
            } else {
                self.aligned_move_delta(raw_dx, raw_dy)
            };
            self.alignment_guides = guides;
            if dx != 0.0 || dy != 0.0 {
                if !self.moved {
                    self.push_undo();
                    self.moved = true;
                }
                let sel = self.selected.clone();
                self.detach_segment_bindings_for_move(&sel);
                for e in self.scene.elements.iter_mut() {
                    if sel.contains(&e.id) {
                        translate(&mut e.kind, dx, dy);
                    }
                }
                self.sync_segment_anchors_for(&sel);
                cx.notify();
            }
            return;
        }
        // Dragging a marquee box (started on empty space).
        if let Some((start, _)) = self.marquee {
            let cur = self.event_to_world(ev.position);
            self.marquee = Some((start, cur));
            cx.notify();
            return;
        }
        // Creating an element.
        if self.pending.is_some() {
            let cur = self.event_to_world(ev.position);
            let z = self.scene.camera.zoom.max(MIN_ZOOM);
            let Some(pending) = self.pending.as_mut() else {
                return;
            };
            let anchor = pending.anchor;
            // Snap the growing corner / endpoint to the grid while Option is held
            // (freehand strokes keep the raw point).
            let c = if ev.modifiers.alt {
                [snap_grid(cur[0]), snap_grid(cur[1])]
            } else {
                cur
            };
            match &mut pending.kind {
                ElementKind::Draw(s) => {
                    if let Some(last) = s.points.last() {
                        let (ddx, ddy) = ((cur[0] - last[0]) * z, (cur[1] - last[1]) * z);
                        if ddx * ddx + ddy * ddy < MIN_POINT_PX * MIN_POINT_PX {
                            return;
                        }
                    }
                    s.points.push(cur);
                }
                ElementKind::Rect(b)
                | ElementKind::Ellipse(b)
                | ElementKind::Diamond(b)
                | ElementKind::Triangle(b)
                | ElementKind::RoundRect(b)
                | ElementKind::Star(b)
                | ElementKind::Hexagon(b) => {
                    b.x = anchor[0].min(c[0]);
                    b.y = anchor[1].min(c[1]);
                    b.w = (c[0] - anchor[0]).abs();
                    b.h = (c[1] - anchor[1]).abs();
                }
                ElementKind::Line(s) | ElementKind::Arrow(s) => {
                    s.x2 = c[0];
                    s.y2 = c[1];
                }
                // Text/cards/images aren't created by dragging, never pending here.
                ElementKind::Text(_) | ElementKind::Embed(_) | ElementKind::Image(_) => {}
            }
            cx.notify();
            return;
        }
        self.update_hover_connector(ev.position, cx);
        // Panning.
        if self.panning {
            let dx = f32::from(ev.position.x - self.last.x);
            let dy = f32::from(ev.position.y - self.last.y);
            self.last = ev.position;
            self.scene.camera.pan_by(dx, dy);
            self.dirty = true;
            cx.notify();
        }
    }

}
