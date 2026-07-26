impl WhiteboardView {
    /// The color the picker should start from for `target`: the single
    /// selection's color (if any), else the active color, else a default.
    fn seed_color(&self, target: PickerTarget) -> u32 {
        let from_sel = self
            .selected_single()
            .and_then(|id| self.scene.elements.iter().find(|e| e.id == id))
            .and_then(|e| match target {
                PickerTarget::Stroke => e.stroke,
                PickerTarget::Fill => e.fill,
                PickerTarget::Text => e.label_color,
            });
        let active = match target {
            PickerTarget::Stroke => self.active_stroke,
            PickerTarget::Fill => self.active_fill,
            PickerTarget::Text => self.active_text,
        };
        from_sel.or(active).unwrap_or(0x4080f0ff)
    }

    /// Point the picker's HSVA controls at `target`'s current color.
    fn seed_picker(&mut self, target: PickerTarget) {
        let c = self.seed_color(target);
        let (h, s, v) = u32_to_hsv(c);
        self.picker = Some(Picker {
            target,
            h,
            s,
            v,
            a: u32_alpha(c),
        });
    }

    /// Open or close the color picker. Opening seeds the controls from the
    /// stroke color (selection's, else active, else a default).
    fn toggle_picker(&mut self, cx: &mut Context<Self>) {
        self.open_group = None;
        self.templates_open = false;
        self.width_open = false;
        self.font_open = false;
        if self.picker.is_some() {
            self.picker = None;
        } else {
            self.seed_picker(PickerTarget::Stroke);
        }
        cx.notify();
    }

    /// Open / close the thickness-preset flyout (closing the other popovers).
    fn toggle_width(&mut self, cx: &mut Context<Self>) {
        self.picker = None;
        self.open_group = None;
        self.templates_open = false;
        self.context_menu = None;
        self.font_open = false;
        self.width_open = !self.width_open;
        cx.notify();
    }

    /// Set the active stroke thickness (screen px) for new elements and apply it to
    /// the selection, *without* undo/flush — used for live slider drags (undo is
    /// pushed at drag start, flush on release, like the color strips).
    fn set_width_live(&mut self, w: f32, cx: &mut Context<Self>) {
        self.active_width = w;
        if !self.selected.is_empty() {
            let zoom = self.scene.camera.zoom.max(MIN_ZOOM);
            let sel = self.selected.clone();
            for e in self.scene.elements.iter_mut() {
                if sel.contains(&e.id) {
                    set_kind_width(&mut e.kind, w / zoom);
                }
            }
            self.dirty = true;
        }
        cx.notify();
    }

    /// A discrete thickness choice (a preset swatch): pushes undo, applies, and
    /// flushes, then closes the flyout.
    fn set_width(&mut self, w: f32, window: &mut Window, cx: &mut Context<Self>) {
        self.width_open = false;
        if !self.selected.is_empty() {
            self.push_undo();
        }
        self.set_width_live(w, cx);
        self.flush(window, cx);
    }

    /// Map a 0..1 slider fraction to a width (screen px), snapped to 0.5px steps.
    fn width_from_frac(frac: f32) -> f32 {
        let w = WIDTH_MIN + frac.clamp(0.0, 1.0) * (WIDTH_MAX - WIDTH_MIN);
        (w * 2.0).round() / 2.0
    }

    /// Open the given tool category's flyout (or close it if already open).
    /// Closes the color picker so only one popover shows at a time.
    fn toggle_group(&mut self, group: ToolGroup, cx: &mut Context<Self>) {
        self.picker = None;
        self.templates_open = false;
        self.width_open = false;
        self.font_open = false;
        self.open_group = if self.open_group == Some(group) {
            None
        } else {
            Some(group)
        };
        cx.notify();
    }

    /// Open / close the templates gallery modal (closing the other popovers).
    fn toggle_templates(&mut self, cx: &mut Context<Self>) {
        self.picker = None;
        self.open_group = None;
        self.width_open = false;
        self.context_menu = None;
        self.font_open = false;
        self.templates_open = !self.templates_open;
        cx.notify();
    }

    /// Open / close the font flyout (upload a face / revert to default), closing
    /// the other popovers.
    fn toggle_font(&mut self, cx: &mut Context<Self>) {
        self.picker = None;
        self.open_group = None;
        self.width_open = false;
        self.templates_open = false;
        self.context_menu = None;
        self.font_open = !self.font_open;
        cx.notify();
    }

    /// Switch which property (stroke / fill) the picker edits, re-seeding its
    /// controls from that property's current color.
    fn set_picker_target(&mut self, target: PickerTarget, cx: &mut Context<Self>) {
        if self.picker.map(|p| p.target) != Some(target) {
            self.seed_picker(target);
            cx.notify();
        }
    }

    /// The picker's current target (stroke unless the picker says otherwise).
    fn picker_target(&self) -> PickerTarget {
        self.picker.map_or(PickerTarget::Stroke, |p| p.target)
    }

    /// Apply `color` to the active target on the active swatch and the selection,
    /// *without* undo/flush — used for live picker drags (undo is pushed once at
    /// drag start; the flush happens on release).
    fn set_color_live(&mut self, color: Option<u32>, cx: &mut Context<Self>) {
        let target = self.picker_target();
        match target {
            PickerTarget::Stroke => self.active_stroke = color,
            PickerTarget::Fill => self.active_fill = color,
            PickerTarget::Text => self.active_text = color,
        }
        if !self.selected.is_empty() {
            let sel = self.selected.clone();
            for e in self.scene.elements.iter_mut() {
                if !sel.contains(&e.id) {
                    continue;
                }
                match target {
                    PickerTarget::Stroke => e.stroke = color,
                    // Fill + label color only attach to closed shapes.
                    PickerTarget::Fill => {
                        if is_closed_shape(&e.kind) {
                            e.fill = color;
                        }
                    }
                    PickerTarget::Text => {
                        if is_closed_shape(&e.kind) {
                            e.label_color = color;
                        }
                    }
                }
            }
            self.dirty = true;
        }
        cx.notify();
    }

    /// A discrete, undoable color choice (a swatch, or the Auto / None reset).
    /// Recolors the selection and syncs the picker controls to the chosen color.
    fn pick_color(&mut self, color: Option<u32>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            self.push_undo();
        }
        if let (Some(c), Some(p)) = (color, self.picker.as_mut()) {
            let (h, s, v) = u32_to_hsv(c);
            // Keep the hue stable on greys (s == 0) so the strip thumb won't jump.
            if s > 0.0 {
                p.h = h;
            }
            p.s = s;
            p.v = v;
            p.a = u32_alpha(c);
        }
        self.set_color_live(color, cx);
        self.flush(window, cx);
    }

    /// Saturation/brightness under a window-coords position in the SV square.
    fn sv_from_pos(&self, pos: Point<Pixels>) -> (f32, f32) {
        let b = self.sv_bounds.get();
        let w = f32::from(b.size.width).max(1.0);
        let h = f32::from(b.size.height).max(1.0);
        let s = ((f32::from(pos.x) - f32::from(b.origin.x)) / w).clamp(0.0, 1.0);
        let v = 1.0 - ((f32::from(pos.y) - f32::from(b.origin.y)) / h).clamp(0.0, 1.0);
        (s, v)
    }

    /// A 0..1 fraction along a horizontal strip (hue or alpha) under `pos`.
    fn frac_x(&self, bounds: Bounds<Pixels>, pos: Point<Pixels>) -> f32 {
        let w = f32::from(bounds.size.width).max(1.0);
        ((f32::from(pos.x) - f32::from(bounds.origin.x)) / w).clamp(0.0, 1.0)
    }

    /// The picker's current color as a packed int (for live application).
    fn picker_u32(&self) -> Option<u32> {
        self.picker.map(|p| hsva_to_u32(p.h, p.s, p.v, p.a))
    }

    /// World point under a window-coords event position.
    fn event_to_world(&self, p: Point<Pixels>) -> [f32; 2] {
        let (rx, ry) = self.relative(p);
        let (wx, wy) = self.scene.camera.screen_to_world(rx, ry);
        [wx, wy]
    }

    /// If `pos` (window coords) is on a manipulation handle of the current
    /// selection, what to begin. Lines/arrows manipulate by their two
    /// endpoints; everything else by its bounding-box corners (a line's bbox is
    /// degenerate, which would make corner-resize wildly imprecise).
    fn handle_hit(&self, pos: Point<Pixels>) -> Option<HandleGrab> {
        let cam = self.scene.camera;
        let origin = self.bounds.get().origin;
        let cursor = self.event_to_world(pos);
        let near = |wx: f32, wy: f32, ox: f32, oy: f32| {
            let s = to_screen(wx, wy, cam, origin);
            let (dx, dy) = (
                f32::from(pos.x) - (f32::from(s.x) + ox),
                f32::from(pos.y) - (f32::from(s.y) + oy),
            );
            dx * dx + dy * dy <= HANDLE_GRAB * HANDLE_GRAB
        };

        // A multi-selection offers a group rotate grip (if anything's rotatable)
        // and proportional corner-resize of the group bounds.
        if self.selected.len() > 1 {
            let bb = self.selection_bbox()?;
            if self.group_rotatable() {
                let (rx, ry) = rotate_handle_for_bbox(bb, cam, origin);
                let (dx, dy) = (f32::from(pos.x) - rx, f32::from(pos.y) - ry);
                if dx * dx + dy * dy <= HANDLE_GRAB * HANDLE_GRAB {
                    return Some(HandleGrab::Rotate);
                }
            }
            let wc = [(bb.0, bb.1), (bb.2, bb.1), (bb.0, bb.3), (bb.2, bb.3)];
            let collect_orig = || -> Vec<(u64, ElementKind)> {
                self.scene
                    .elements
                    .iter()
                    .filter(|e| self.is_selected(e.id))
                    .map(|e| (e.id, e.kind.clone()))
                    .collect()
            };
            for i in 0..4 {
                if near(wc[i].0, wc[i].1, 0.0, 0.0) {
                    let opp = wc[3 - i];
                    return Some(HandleGrab::GroupCorner(GroupResizing {
                        handle: ResizeHandle::Corner,
                        anchor: [opp.0, opp.1],
                        from: [wc[i].0, wc[i].1],
                        grab: [wc[i].0 - cursor[0], wc[i].1 - cursor[1]],
                        orig: collect_orig(),
                    }));
                }
            }
            // Edge midpoints stretch one axis (per-axis group resize), each about
            // the opposite edge: a left/right grip scales x, a top/bottom grip y.
            let (mx, my) = ((bb.0 + bb.2) / 2.0, (bb.1 + bb.3) / 2.0);
            let edges = [
                (ResizeHandle::EdgeX, [bb.0, my], (0.0, 0.0), [bb.2, my]),
                (ResizeHandle::EdgeX, [bb.2, my], (0.0, 0.0), [bb.0, my]),
                (ResizeHandle::EdgeY, [mx, bb.1], (0.0, 0.0), [mx, bb.3]),
                (ResizeHandle::EdgeY, [mx, bb.3], (0.0, 0.0), [mx, bb.1]),
            ];
            for (handle, from, (ox, oy), anchor) in edges {
                if near(from[0], from[1], ox, oy) {
                    return Some(HandleGrab::GroupCorner(GroupResizing {
                        handle,
                        anchor,
                        from,
                        grab: [from[0] - cursor[0], from[1] - cursor[1]],
                        orig: collect_orig(),
                    }));
                }
            }
            return None;
        }

        let id = self.selected_single()?;
        let kind = &self.scene.elements.iter().find(|e| e.id == id)?.kind;

        // The rotate handle floats above every rotatable element (not text/cards).
        if rotatable(kind) {
            let (rx, ry) = rotate_handle_screen(kind, cam, origin);
            let (dx, dy) = (f32::from(pos.x) - rx, f32::from(pos.y) - ry);
            if dx * dx + dy * dy <= HANDLE_GRAB * HANDLE_GRAB {
                return Some(HandleGrab::Rotate);
            }
        }

        if let ElementKind::Line(s) | ElementKind::Arrow(s) = kind {
            for (which, (wx, wy)) in [(s.x1, s.y1), (s.x2, s.y2)].into_iter().enumerate() {
                if near(wx, wy, 0.0, 0.0) {
                    return Some(HandleGrab::Endpoint(EndpointDrag { id, which }));
                }
            }
            return None;
        }

        // Box-like (rect/ellipse/text): corners on the (possibly rotated) box.
        // Upright resizes about the opposite corner (free aspect ratio); rotated
        // resizes proportionally about the center — a similarity transform that
        // stays correct under rotation (set up here, applied in `on_move`).
        if let Some((x, y, w, h, rot)) = box_like(kind) {
            let cu = box_padded_corners(x, y, w, h, rot, 0.0);
            let cp = cu;
            let center = [x + w / 2.0, y + h / 2.0];
            let rotated = rot.abs() > ROT_EPS;
            for i in 0..4 {
                if near(cp[i][0], cp[i][1], 0.0, 0.0) {
                    let anchor = if rotated { center } else { cu[(i + 2) % 4] };
                    return Some(HandleGrab::Corner(Resizing {
                        id,
                        handle: ResizeHandle::Corner,
                        anchor,
                        from: cu[i],
                        grab: [cu[i][0] - cursor[0], cu[i][1] - cursor[1]],
                        orig: kind.clone(),
                    }));
                }
            }
            // Edge midpoints stretch one axis. Offered only upright (a rotated
            // box's edges aren't world-axis-aligned) and not for text — a single
            // font size can't stretch one axis, so its edges would just duplicate
            // the proportional corners.
            if !rotated && !matches!(kind, ElementKind::Text(_)) {
                let mid = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
                if let Some(r) = self.edge_handle_hit(
                    id,
                    kind,
                    &near,
                    cursor,
                    mid(cu[0], cu[1]),
                    mid(cu[1], cu[2]),
                    mid(cu[2], cu[3]),
                    mid(cu[3], cu[0]),
                ) {
                    return Some(r);
                }
            }
            return None;
        }

        // Draw / Embed: corners on the padded AABB (offset the hit to match).
        let bb = bbox(kind);
        let wc = [(bb.0, bb.1), (bb.2, bb.1), (bb.0, bb.3), (bb.2, bb.3)];
        for i in 0..4 {
            if near(wc[i].0, wc[i].1, 0.0, 0.0) {
                let opp = wc[3 - i];
                return Some(HandleGrab::Corner(Resizing {
                    id,
                    handle: ResizeHandle::Corner,
                    anchor: [opp.0, opp.1],
                    from: [wc[i].0, wc[i].1],
                    grab: [wc[i].0 - cursor[0], wc[i].1 - cursor[1]],
                    orig: kind.clone(),
                }));
            }
        }
        // Edge midpoints stretch one axis (these kinds are always upright).
        let (mx, my) = ((bb.0 + bb.2) / 2.0, (bb.1 + bb.3) / 2.0);
        self.edge_handle_hit(
            id,
            kind,
            &near,
            cursor,
            [mx, bb.1],
            [bb.2, my],
            [mx, bb.3],
            [bb.0, my],
        )
    }

    /// Shared edge-handle hit-test for a single element: the four edge midpoints
    /// (`top`/`right`/`bottom`/`left`, world space) each stretch one axis about the
    /// opposite edge. `near` is the caller's screen-space proximity test.
    #[allow(clippy::too_many_arguments)]
    fn edge_handle_hit(
        &self,
        id: u64,
        kind: &ElementKind,
        near: &dyn Fn(f32, f32, f32, f32) -> bool,
        cursor: [f32; 2],
        top: [f32; 2],
        right: [f32; 2],
        bottom: [f32; 2],
        left: [f32; 2],
    ) -> Option<HandleGrab> {
        let edges = [
            (ResizeHandle::EdgeY, top, (0.0, 0.0), bottom),
            (ResizeHandle::EdgeY, bottom, (0.0, 0.0), top),
            (ResizeHandle::EdgeX, right, (0.0, 0.0), left),
            (ResizeHandle::EdgeX, left, (0.0, 0.0), right),
        ];
        for (handle, from, (ox, oy), anchor) in edges {
            if near(from[0], from[1], ox, oy) {
                return Some(HandleGrab::Corner(Resizing {
                    id,
                    handle,
                    anchor,
                    from,
                    grab: [from[0] - cursor[0], from[1] - cursor[1]],
                    orig: kind.clone(),
                }));
            }
        }
        None
    }

    /// The topmost connector point under the cursor. Connectors are exposed on
    /// box-like visual elements (closed shapes, text, image) at the midpoints of
    /// their rotated top/right/bottom/left edges.
    fn connector_at(&self, pos: Point<Pixels>) -> Option<ConnectPoint> {
        let origin = self.bounds.get().origin;
        let near_px = CONNECTOR_BUTTON_SIZE * 0.65;
        let (sx, sy) = (f32::from(pos.x), f32::from(pos.y));
        let id = self.selected_single()?;
        let element = self
            .scene
            .elements
            .iter()
            .find(|element| element.id == id && connector_capable(&element.kind))?;
        let points = connector_points(&element.kind);
        let buttons = connector_button_centers(&element.kind, self.scene.camera, origin);
        buttons.into_iter().enumerate().find_map(|(index, button)| {
            let dx = f32::from(button.x) - sx;
            let dy = f32::from(button.y) - sy;
            (dx * dx + dy * dy <= near_px * near_px).then_some(ConnectPoint {
                id,
                index,
                pos: points[index],
            })
        })
    }

    /// Update hover connector state and request repaint only when it changes.
    fn update_hover_connector(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let next = if self.tool == Tool::Select
            && self.editing.is_none()
            && self.pending.is_none()
            && self.connecting.is_none()
            && self.drag_from.is_none()
            && self.resizing.is_none()
            && self.group_resizing.is_none()
            && self.endpoint.is_none()
            && self.rotating.is_none()
            && self.marquee.is_none()
        {
            self.connector_at(pos)
        } else {
            None
        };
        if self.hovered_connector != next {
            self.hovered_connector = next;
            cx.notify();
        }
    }

    /// The topmost text element under a world point (within `pad`), if any.
    fn text_at(&self, p: [f32; 2], pad: f32) -> Option<u64> {
        self.scene
            .elements
            .iter()
            .rev()
            .find(|e| matches!(e.kind, ElementKind::Text(_)) && hit_test(&e.kind, p[0], p[1], pad))
            .map(|e| e.id)
    }

    /// The topmost closed shape (rect / ellipse / …) under a world point — for
    /// editing its centered label.
    fn shape_at(&self, p: [f32; 2], pad: f32) -> Option<u64> {
        self.scene
            .elements
            .iter()
            .rev()
            .find(|e| is_closed_shape(&e.kind) && hit_test(&e.kind, p[0], p[1], pad))
            .map(|e| e.id)
    }

    /// The topmost page-card under a world point: `(element id, page id)`.
    fn embed_at(&self, p: [f32; 2], pad: f32) -> Option<(u64, i64)> {
        self.scene
            .elements
            .iter()
            .rev()
            .find_map(|e| match &e.kind {
                ElementKind::Embed(em) if hit_test(&e.kind, p[0], p[1], pad) => {
                    Some((e.id, em.page_id))
                }
                _ => None,
            })
    }

    /// Nearest edge connector on another shape while a connection is being
    /// dragged. This path is intentionally separate from `connector_at`, whose
    /// hit targets are the selected source shape's outward arrow buttons.
    fn snap_connector_at(
        &self,
        pos: Point<Pixels>,
        source_id: u64,
    ) -> Option<(ConnectPoint, bool)> {
        const SHOW_DISTANCE_PX: f32 = 64.0;
        const SNAP_DISTANCE_PX: f32 = 20.0;
        let origin = self.bounds.get().origin;
        let (sx, sy) = (f32::from(pos.x), f32::from(pos.y));
        let world = self.event_to_world(pos);
        let target = self
            .scene
            .elements
            .iter()
            .rev()
            .filter(|element| element.id != source_id && connector_capable(&element.kind))
            .find(|element| {
                hit_test(
                    &element.kind,
                    world[0],
                    world[1],
                    SHOW_DISTANCE_PX / self.scene.camera.zoom.max(MIN_ZOOM),
                )
            })?;
        connector_points(&target.kind)
            .into_iter()
            .enumerate()
            .map(|(index, point)| {
                let screen = to_screen(point[0], point[1], self.scene.camera, origin);
                let dx = f32::from(screen.x) - sx;
                let dy = f32::from(screen.y) - sy;
                let distance_sq = dx * dx + dy * dy;
                (
                    distance_sq,
                    ConnectPoint {
                        id: target.id,
                        index,
                        pos: point,
                    },
                )
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(distance_sq, connector)| {
                (
                    connector,
                    distance_sq <= SNAP_DISTANCE_PX * SNAP_DISTANCE_PX,
                )
            })
    }

}
