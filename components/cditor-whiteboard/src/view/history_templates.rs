impl WhiteboardView {
    /// Whether a *group* rotation applies: more than one element selected, at
    /// least one of which can rotate (so an all-cards group offers no grip).
    fn group_rotatable(&self) -> bool {
        self.selected.len() > 1
            && self
                .scene
                .elements
                .iter()
                .any(|e| self.selected.contains(&e.id) && rotatable(&e.kind))
    }

    /// The active tool (e.g. for host-driven chrome).
    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// Switch the active drawing tool. Leaving Select clears the selection.
    /// Always closes an open tool flyout (the tool was just chosen).
    pub fn set_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        if self.read_only {
            self.tool = Tool::Pan;
            self.selected.clear();
            self.open_group = None;
            cx.notify();
            return;
        }
        self.open_group = None;
        if self.tool != tool {
            self.tool = tool;
            if tool != Tool::Select {
                self.selected.clear();
            }
        }
        cx.notify();
    }

    /// Reset the viewport to the origin at 100% (also bound to double-click).
    pub fn reset_view(&mut self, cx: &mut Context<Self>) {
        self.scene.camera = Camera::default();
        self.dirty = true;
        cx.notify();
    }

    /// Zoom in/out a step, centered on the canvas.
    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.zoom_centered(1.2, cx);
    }
    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.zoom_centered(1.0 / 1.2, cx);
    }

    fn zoom_centered(&mut self, factor: f32, cx: &mut Context<Self>) {
        let b = self.bounds.get();
        let rx = f32::from(b.size.width) / 2.0;
        let ry = f32::from(b.size.height) / 2.0;
        self.scene.camera.zoom_about(rx, ry, factor);
        self.dirty = true;
        cx.notify();
    }

    /// Snapshot the scene for undo (before a mutation), capping history.
    fn push_undo(&mut self) {
        self.history.push(self.scene.clone());
        if self.history.len() > UNDO_CAP {
            self.history.remove(0);
        }
        self.redo.clear();
    }

    /// Revert the last change.
    pub fn undo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(prev) = self.history.pop() {
            self.redo.push(std::mem::replace(&mut self.scene, prev));
            self.selected.clear();
            self.dirty = true;
            cx.notify();
            self.flush(window, cx);
        }
    }

    /// Re-apply the last undone change.
    pub fn redo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(next) = self.redo.pop() {
            self.history.push(std::mem::replace(&mut self.scene, next));
            self.selected.clear();
            self.dirty = true;
            cx.notify();
            self.flush(window, cx);
        }
    }

    /// Delete the selected elements.
    fn delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            return;
        }
        self.push_undo();
        let gone = std::mem::take(&mut self.selected);
        self.scene.elements.retain(|e| !gone.contains(&e.id));
        self.editing = None;
        self.dirty = true;
        cx.notify();
        self.flush(window, cx);
    }

    /// Move the selected elements through the paint order (their position in
    /// `elements`; later = painted on top, so it can cover earlier ones). One step
    /// or all the way, per `op`. A no-op (already at that edge) leaves undo/redo
    /// untouched.
    fn reorder_selection(&mut self, op: ZOrder, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            return;
        }
        let sel = self.selected.clone();
        let on = |id: u64| sel.contains(&id);
        self.push_undo();
        let before: Vec<u64> = self.scene.elements.iter().map(|e| e.id).collect();
        let els = &mut self.scene.elements;
        match op {
            // Stable partition: the non-selected keep their order and the selected
            // keep theirs, so a multi-selection moves as a block.
            ZOrder::ToFront => els.sort_by_key(|e| on(e.id)),
            ZOrder::ToBack => els.sort_by_key(|e| !on(e.id)),
            // One step: swap each selected past its adjacent non-selected neighbor,
            // walking away from the destination edge so an element isn't moved twice
            // and selected elements don't leapfrog each other.
            ZOrder::Forward => {
                for i in (0..els.len().saturating_sub(1)).rev() {
                    if on(els[i].id) && !on(els[i + 1].id) {
                        els.swap(i, i + 1);
                    }
                }
            }
            ZOrder::Backward => {
                for i in 1..els.len() {
                    if on(els[i].id) && !on(els[i - 1].id) {
                        els.swap(i, i - 1);
                    }
                }
            }
        }
        if self.scene.elements.iter().map(|e| e.id).eq(before) {
            self.history.pop(); // nothing moved — drop the speculative snapshot
            return;
        }
        self.dirty = true;
        cx.notify();
        self.flush(window, cx);
    }

    /// Flush pending changes through the host's persistence hook.
    fn flush(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if let Some(f) = self.on_change.clone() {
            f(self.scene.to_json(), window, cx);
        }
    }

    // --- templates ---------------------------------------------------------

    /// Serialize the current selection: the selected elements translated so their
    /// collective bounding box starts at the origin (so the group can be re-based
    /// anywhere when applied). `None` if nothing is selected. Used for both saving
    /// a template and copying to the clipboard — the two share this format, so a
    /// copied selection can be pasted on any board (see [`Self::paste_elements`]).
    fn selection_json(&self) -> Option<String> {
        let sel: Vec<&Element> = self
            .scene
            .elements
            .iter()
            .filter(|e| self.selected.contains(&e.id))
            .collect();
        if sel.is_empty() {
            return None;
        }
        let (minx, miny) = sel
            .iter()
            .fold((f32::INFINITY, f32::INFINITY), |(mx, my), e| {
                let (x0, y0, ..) = bbox(&e.kind);
                (mx.min(x0), my.min(y0))
            });
        let elems: Vec<Element> = sel
            .iter()
            .map(|e| {
                let mut c = (*e).clone();
                translate(&mut c.kind, -minx, -miny);
                c
            })
            .collect();
        serde_json::to_string(&elems).ok()
    }

    /// Hand the current selection to the host to be saved as a named template.
    fn save_selection_as_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.context_menu = None;
        if let Some(json) = self.selection_json()
            && let Some(f) = self.on_save_template.clone()
        {
            f(json, window, cx);
        }
        cx.notify();
    }

    /// Stamp template `index` onto the board, centered in the current viewport,
    /// with fresh ids; the new elements become the selection.
    fn apply_template(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(elems) = self.templates.get(index).map(|t| t.elements.clone()) else {
            return;
        };
        self.templates_open = false;
        self.stamp_elements(&elems, window, cx);
    }

    /// Place `elems` (origin-normalized, as produced by [`Self::selection_json`])
    /// onto the board, centered in the current viewport with fresh ids; they
    /// become the new selection. Shared by template apply and clipboard paste.
    /// No-op for an empty group.
    fn stamp_elements(&mut self, elems: &[Element], window: &mut Window, cx: &mut Context<Self>) {
        if elems.is_empty() {
            return;
        }
        self.open_group = None;
        self.push_undo();
        // Center the (origin-normalized) group in the viewport.
        let b = self.bounds.get();
        let cam = self.scene.camera;
        let z = cam.zoom.max(MIN_ZOOM);
        let (tw, th) = elements_extent(elems);
        let off = [
            cam.x + (f32::from(b.size.width) / 2.0) / z - tw / 2.0,
            cam.y + (f32::from(b.size.height) / 2.0) / z - th / 2.0,
        ];
        let mut new_ids = Vec::with_capacity(elems.len());
        for e in elems {
            let mut c = e.clone();
            translate(&mut c.kind, off[0], off[1]);
            c.id = self.next_id;
            self.next_id += 1;
            new_ids.push(c.id);
            self.scene.elements.push(c);
        }
        self.selected = new_ids;
        self.tool = Tool::Select;
        self.dirty = true;
        self.flush(window, cx);
        cx.notify();
    }

    /// Copy the selection to the clipboard via the host's `on_copy` hook (the
    /// crate can't touch the system clipboard). Returns whether anything was
    /// copied. `⌘X` reuses this, then deletes.
    fn copy_selection(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(json) = self.selection_json() else {
            return false;
        };
        if let Some(f) = self.on_copy.clone() {
            f(json, window, cx);
        }
        true
    }

    /// Paste a serialized `Vec<Element>` (the JSON a [`CopyFn`] wrote) onto the
    /// board — centered in the viewport, selected, with fresh ids. Ignores invalid
    /// JSON. The host calls this from its [`PasteFn`] when the clipboard holds
    /// whiteboard elements rather than an image.
    pub fn paste_elements(&mut self, json: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(elems) = serde_json::from_str::<Vec<Element>>(json) {
            self.stamp_elements(&elems, window, cx);
        }
    }

    /// Ask the host to delete a stored template (right-click a card). The host
    /// confirms, removes it, and feeds the updated list back.
    fn delete_template(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(f) = self.on_delete_template.clone() {
            f(id, window, cx);
        }
    }

    /// A template preview card for the gallery modal: a scaled mini-paint of the
    /// template's shapes over its name. Click to stamp it; right-click to delete.
    /// (Text and page-cards don't appear in the mini-paint — only drawn shapes —
    /// but they're still placed on apply.)
    fn template_card(
        &self,
        index: usize,
        ink: Hsla,
        text: Hsla,
        grid: Hsla,
        bg: Hsla,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let t = &self.templates[index];
        let id = t.id;
        let name: SharedString = t.name.clone().into();
        let elems = t.elements.clone();
        let (tw, th) = elements_extent(&elems);
        let preview = canvas(
            |_, _, _| {},
            move |bounds, _, window: &mut Window, _: &mut App| {
                let pad = 8.0;
                let aw = f32::from(bounds.size.width) - 2.0 * pad;
                let ah = f32::from(bounds.size.height) - 2.0 * pad;
                if tw <= 0.0 || th <= 0.0 || aw <= 0.0 || ah <= 0.0 {
                    return;
                }
                // Fit the (origin-normalized) template into the card, centered,
                // never magnifying past 1:1.
                let scale = (aw / tw).min(ah / th).min(1.0);
                let ox = (f32::from(bounds.size.width) - tw * scale) / 2.0;
                let oy = (f32::from(bounds.size.height) - th * scale) / 2.0;
                let cam = Camera {
                    x: -ox / scale,
                    y: -oy / scale,
                    zoom: scale,
                };
                for e in &elems {
                    let stroke = e.stroke.map_or(ink, u32_to_hsla);
                    let fill = e.fill.map(u32_to_hsla);
                    paint_element(&e.kind, None, cam, bounds.origin, stroke, fill, window);
                }
            },
        )
        .size_full();
        div()
            .id(("wb-template", index))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(5.0))
            .p(px(6.0))
            .rounded(px(8.0))
            .hover(|s| s.bg(grid))
            .child(
                div()
                    .w(px(150.0))
                    .h(px(104.0))
                    .rounded(px(6.0))
                    .bg(bg)
                    .border_1()
                    .border_color(grid)
                    .child(preview),
            )
            .child(
                div()
                    .w(px(150.0))
                    .h(px(15.0))
                    .overflow_hidden()
                    .text_size(px(11.0))
                    .text_color(text)
                    .child(name),
            )
            .on_click(
                cx.listener(move |this, _ev, window, cx| this.apply_template(index, window, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _ev, window, cx| this.delete_template(id, window, cx)),
            )
            .into_any_element()
    }

    // --- color picker ------------------------------------------------------

}
