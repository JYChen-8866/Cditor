impl WhiteboardView {
    fn on_scroll(&mut self, ev: &ScrollWheelEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let (dx, dy) = match ev.delta {
            ScrollDelta::Pixels(p) => (f32::from(p.x), f32::from(p.y)),
            ScrollDelta::Lines(p) => (p.x * LINE_PX, p.y * LINE_PX),
        };
        if ev.modifiers.platform || ev.modifiers.control {
            let (rx, ry) = self.relative(ev.position);
            let factor = (1.0 + dy * 0.0025).clamp(0.5, 2.0);
            self.scene.camera.zoom_about(rx, ry, factor);
        } else {
            self.scene.camera.pan_by(dx, dy);
        }
        self.dirty = true;
        cx.notify();
    }

    fn on_pinch(&mut self, ev: &PinchEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let (rx, ry) = self.relative(ev.position);
        self.scene.camera.zoom_about(rx, ry, 1.0 + ev.delta);
        self.dirty = true;
        cx.notify();
    }

    /// Canvas-relative position of a window-coords event point.
    fn relative(&self, p: Point<Pixels>) -> (f32, f32) {
        let o = self.bounds.get().origin;
        (f32::from(p.x - o.x), f32::from(p.y - o.y))
    }

    /// A clone of the text element being edited (its content + size + placement).
    /// The text currently being edited — a `Text` element's content, or a closed
    /// shape's centered label — with everything the caret math and click
    /// hit-testing need. `wrap` is `None` for free text, `Some(inner_width)` for a
    /// label (which word-wraps inside its shape). `x`/`y`/`w`/`h`/`rotation` place
    /// the laid-out block in the world.
    fn edit_target(&self, id: u64) -> Option<EditTarget> {
        let e = self.scene.elements.iter().find(|e| e.id == id)?;
        match &e.kind {
            ElementKind::Text(t) => Some(EditTarget {
                content: t.content.clone(),
                size: t.size,
                wrap: None,
                x: t.x,
                y: t.y,
                rotation: t.rotation,
                pivot: [t.x + t.measured_w / 2.0, t.y + t.measured_h / 2.0],
            }),
            kind if is_closed_shape(kind) => {
                let (bx, by, bw, bh, rot) = box_like(kind)?;
                let label = e.label.clone().unwrap_or_default();
                let blk = shape_label_block(&self.font, kind, bx, by, bw, bh, &label);
                Some(EditTarget {
                    content: label,
                    size: blk.size,
                    wrap: Some(blk.wrap),
                    x: blk.x,
                    y: blk.y,
                    rotation: rot,
                    pivot: [bx + bw / 2.0, by + bh / 2.0],
                })
            }
            _ => None,
        }
    }

    /// The selection as an ordered byte range `[start, end)` (empty when the caret
    /// and anchor coincide).
    fn sel_range(&self) -> (usize, usize) {
        (
            self.caret.min(self.sel_anchor),
            self.caret.max(self.sel_anchor),
        )
    }

    /// Move the caret to byte offset `to`; unless `extend` (Shift), collapse the
    /// selection there too.
    fn move_caret(&mut self, to: usize, extend: bool, cx: &mut Context<Self>) {
        self.caret = to;
        if !extend {
            self.sel_anchor = to;
        }
        self.pending_style = None; // a deliberate move ends a pending toggle
        cx.notify();
    }

    /// Replace the editing text's `[s, e)` with `ins`, landing the caret just after
    /// it (collapsed). The single mutation point for typing, deletion, and paste.
    fn replace_range(&mut self, id: u64, s: usize, e: usize, ins: &str, cx: &mut Context<Self>) {
        let pending = self.pending_style;
        let Some(el) = self.scene.elements.iter_mut().find(|el| el.id == id) else {
            return;
        };
        // The inserted text takes a pending toggle (⌘B with no selection), else it
        // inherits the run to the left of the caret.
        let insert_style = pending.unwrap_or_else(|| style_at(&el.styles, s.saturating_sub(1)));
        // Mutate the text — a `Text` element's content, or a closed shape's label.
        let edited = if let ElementKind::Text(t) = &mut el.kind {
            t.content.replace_range(s..e, ins);
            true
        } else if is_closed_shape(&el.kind) {
            el.label
                .get_or_insert_with(String::new)
                .replace_range(s..e, ins);
            true
        } else {
            false
        };
        if edited {
            // Keep the styling aligned to the edited text.
            el.styles = splice_styles(&el.styles, s, e, ins.len(), insert_style);
            self.caret = s + ins.len();
            self.sel_anchor = self.caret;
            self.marked_range = None;
            self.dirty = true;
            cx.notify();
        }
    }

    /// Replace the current selection (or insert at the caret) with `ins`.
    fn replace_selection(&mut self, id: u64, ins: &str, cx: &mut Context<Self>) {
        let (s, e) = self.sel_range();
        self.replace_range(id, s, e, ins, cx);
    }

    fn editing_content(&self) -> Option<String> {
        self.editing
            .and_then(|id| self.edit_target(id).map(|tg| tg.content))
    }

    // Kept byte-for-byte equivalent to gpui-markdown-editor's UTF-16 bridge.
    fn utf16_to_utf8_in(text: &str, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in text.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn utf8_to_utf16_in(text: &str, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in text.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn utf16_range_to_utf8_in(text: &str, range_utf16: &Range<usize>) -> Range<usize> {
        Self::utf16_to_utf8_in(text, range_utf16.start)
            ..Self::utf16_to_utf8_in(text, range_utf16.end)
    }

    fn utf8_range_to_utf16_in(text: &str, range: &Range<usize>) -> Range<usize> {
        Self::utf8_to_utf16_in(text, range.start)..Self::utf8_to_utf16_in(text, range.end)
    }

    /// Whiteboard storage adapter for the editor's `replace_text_in_visible_range`.
    /// The full inserted text is the marked range; the IME's relative selection is
    /// tracked independently, exactly as in gpui-markdown-editor.
    fn replace_text_in_visible_range(
        &mut self,
        visible_range: Range<usize>,
        new_text: &str,
        selected_range_relative: Option<Range<usize>>,
        mark_inserted_text: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.editing else {
            return;
        };
        let insert_start = visible_range.start;
        self.replace_range(id, visible_range.start, visible_range.end, new_text, cx);

        self.marked_range = if mark_inserted_text && !new_text.is_empty() {
            Some(insert_start..insert_start + new_text.len())
        } else {
            None
        };
        let selected_range = selected_range_relative
            .map(|relative| insert_start + relative.start..insert_start + relative.end);
        self.caret = selected_range
            .as_ref()
            .map(|range| range.end)
            .unwrap_or(insert_start + new_text.len());
        self.sel_anchor = selected_range
            .as_ref()
            .map(|range| range.start)
            .unwrap_or(self.caret);
        cx.notify();
    }

    /// The formatting active across the current selection (or, collapsed, the
    /// pending toggle / the run left of the caret) — for menu checkmarks. Plain
    /// when not editing text.
    fn selection_style(&self) -> RunStyle {
        let Some(id) = self.editing else {
            return RunStyle::default();
        };
        let (s, e) = self.sel_range();
        if s >= e
            && let Some(p) = self.pending_style
        {
            return p;
        }
        self.scene
            .elements
            .iter()
            .find(|el| el.id == id)
            .map_or(RunStyle::default(), |el| active_style(&el.styles, s, e))
    }

    /// Toggle a boolean format over the selection while editing text; with a
    /// collapsed caret, arm a pending toggle for the next typed text instead.
    fn apply_format(&mut self, format: Format, cx: &mut Context<Self>) {
        let Some(id) = self.editing else {
            return;
        };
        let (s, e) = self.sel_range();
        if s < e {
            if let Some(el) = self.scene.elements.iter_mut().find(|el| el.id == id) {
                el.styles = toggle_format(&el.styles, s, e, format);
                self.dirty = true;
            }
        } else {
            let mut p = self.selection_style();
            let on = !format.get(&p);
            format.set(&mut p, on);
            self.pending_style = Some(p);
        }
        cx.notify();
    }

    /// Like [`apply_format`](Self::apply_format) for the highlight color.
    fn apply_highlight(&mut self, color: u32, cx: &mut Context<Self>) {
        let Some(id) = self.editing else {
            return;
        };
        let (s, e) = self.sel_range();
        if s < e {
            if let Some(el) = self.scene.elements.iter_mut().find(|el| el.id == id) {
                el.styles = toggle_highlight(&el.styles, s, e, color);
                self.dirty = true;
            }
        } else {
            let mut p = self.selection_style();
            p.highlight = (p.highlight != Some(color)).then_some(color);
            self.pending_style = Some(p);
        }
        cx.notify();
    }

    /// The formatting menu panel — a ✓-marked toggle per format — shared by the
    /// right-click submenu and the toolbar fly-out. Toggling a row keeps the menu
    /// open so the checkmarks update live.
    fn format_menu(
        &self,
        ink: Hsla,
        text: Hsla,
        grid: Hsla,
        bg: Hsla,
        cx: &mut Context<Self>,
    ) -> Div {
        let st = self.selection_style();
        let frow = |id: &'static str, label: &'static str, sc: &'static str, on: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(5.0))
                .mx(px(4.0))
                .rounded(px(6.0))
                .text_size(px(12.0))
                .text_color(ink)
                .hover(|s| s.bg(grid))
                .child(div().w(px(12.0)).child(if on { "✓" } else { "" }))
                .child(div().flex_1().child(label))
                .child(div().text_size(px(11.0)).text_color(text).child(sc))
        };
        div()
            .min_w(px(184.0))
            .py(px(4.0))
            .rounded(px(8.0))
            .bg(bg)
            .shadow_lg()
            .border_1()
            .border_color(grid)
            .flex()
            .flex_col()
            .child(
                frow("wb-fmt-bold", "Bold", "⌘B", st.bold)
                    .on_click(cx.listener(|this, _ev, _w, cx| this.apply_format(Format::Bold, cx))),
            )
            .child(
                frow("wb-fmt-italic", "Italic", "⌘I", st.italic).on_click(
                    cx.listener(|this, _ev, _w, cx| this.apply_format(Format::Italic, cx)),
                ),
            )
            .child(
                frow("wb-fmt-underline", "Underline", "⌘U", st.underline).on_click(
                    cx.listener(|this, _ev, _w, cx| this.apply_format(Format::Underline, cx)),
                ),
            )
            .child(
                frow("wb-fmt-strike", "Strikethrough", "⇧⌘X", st.strike).on_click(
                    cx.listener(|this, _ev, _w, cx| this.apply_format(Format::Strike, cx)),
                ),
            )
            .child(
                frow(
                    "wb-fmt-highlight",
                    "Highlight",
                    "⇧⌘H",
                    st.highlight.is_some(),
                )
                .on_click(
                    cx.listener(|this, _ev, _w, cx| this.apply_highlight(HIGHLIGHT_DEFAULT, cx)),
                ),
            )
    }

    /// The caret offset one line up (`dir = -1`) or down (`dir = 1`), keeping the
    /// current column (x). Clamps at the first / last line.
    fn caret_vertical(&self, content: &str, size: f32, wrap: Option<f32>, dir: i32) -> usize {
        let pos = self.font.caret_pos_wrapped(content, size, wrap, self.caret);
        let lh = self.font.measure("", size).1.max(1.0);
        // Aim mid-target-line so `index_at`'s floor lands on it despite rounding.
        let y = (pos[1] + dir as f32 * lh + lh * 0.5).max(0.0);
        self.font.index_at_wrapped(content, size, wrap, [pos[0], y])
    }

    /// Apply one key press while editing text: caret navigation (arrows / Home /
    /// End, ⇧ extends), selection (⌘A, click-drag set elsewhere), clipboard
    /// (⌘C/X/V on the system clipboard), and insertion / deletion. Escape commits.
    fn text_edit_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.editing else {
            return;
        };
        let Some(tg) = self.edit_target(id) else {
            self.commit_text(window, cx);
            return;
        };
        let (content, size, wrap) = (tg.content, tg.size, tg.wrap);
        // Keep the caret/anchor valid against the live content (defensive).
        self.caret = floor_boundary(&content, self.caret);
        self.sel_anchor = floor_boundary(&content, self.sel_anchor);
        let ks = &ev.keystroke;
        if ks.is_ime_in_progress() {
            return;
        }
        let cmd = ks.modifiers.platform || ks.modifiers.control;
        let shift = ks.modifiers.shift;

        if ks.key == "escape" {
            self.commit_text(window, cx);
            return;
        }
        if cmd {
            match ks.key.as_str() {
                "a" => {
                    self.sel_anchor = 0;
                    self.caret = content.len();
                    cx.notify();
                }
                "b" => self.apply_format(Format::Bold, cx),
                "i" => self.apply_format(Format::Italic, cx),
                "u" => self.apply_format(Format::Underline, cx),
                "x" if shift => self.apply_format(Format::Strike, cx),
                "h" if shift => self.apply_highlight(HIGHLIGHT_DEFAULT, cx),
                "c" | "x" => {
                    let (s, e) = self.sel_range();
                    if s < e {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                            content[s..e].into(),
                        ));
                        if ks.key == "x" {
                            self.replace_range(id, s, e, "", cx);
                        }
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                        self.replace_selection(id, &text, cx);
                    }
                }
                _ => cx.propagate(), // ⌘Z / ⌘W / … belong to the host
            }
            return;
        }

        if ks.key == "tab" && self.is_mindmap_node(id) {
            self.commit_text(window, cx);
            self.add_mindmap_relative(id, false, window, cx);
            return;
        }
        if ks.key == "enter" && self.is_mindmap_node(id) {
            self.commit_text(window, cx);
            self.add_mindmap_relative(id, true, window, cx);
            return;
        }

        match ks.key.as_str() {
            "left" => {
                let (s, e) = self.sel_range();
                let to = if !shift && s < e {
                    s
                } else {
                    caret_left(&content, self.caret)
                };
                self.move_caret(to, shift, cx);
            }
            "right" => {
                let (s, e) = self.sel_range();
                let to = if !shift && s < e {
                    e
                } else {
                    caret_right(&content, self.caret)
                };
                self.move_caret(to, shift, cx);
            }
            "up" => {
                let to = self.caret_vertical(&content, size, wrap, -1);
                self.move_caret(to, shift, cx);
            }
            "down" => {
                let to = self.caret_vertical(&content, size, wrap, 1);
                self.move_caret(to, shift, cx);
            }
            "home" => self.move_caret(line_start(&content, self.caret), shift, cx),
            "end" => self.move_caret(line_end(&content, self.caret), shift, cx),
            "backspace" => {
                let (s, e) = self.sel_range();
                if s < e {
                    self.replace_range(id, s, e, "", cx);
                } else if self.caret > 0 {
                    self.replace_range(id, caret_left(&content, self.caret), self.caret, "", cx);
                }
            }
            "delete" => {
                let (s, e) = self.sel_range();
                if s < e {
                    self.replace_range(id, s, e, "", cx);
                } else if self.caret < content.len() {
                    self.replace_range(id, self.caret, caret_right(&content, self.caret), "", cx);
                }
            }
            "enter" => self.replace_selection(id, "\n", cx),
            "tab" => cx.propagate(),
            _ => {
                // Printable text is handled by GPUI's ElementInputHandler path.
                // Inserting `key_char` here duplicates IME composition: pinyin is
                // inserted by keydown, then the committed Chinese text is inserted
                // by the input handler. Keep keydown for navigation/deletion only.
                if ks
                    .key_char
                    .as_deref()
                    .is_none_or(|c| c.chars().next().is_none_or(|ch| ch.is_control()))
                {
                    cx.propagate();
                }
            }
        }
    }

    /// Enter edit mode on text `id`, placing the caret at byte offset `at`.
    fn begin_text_edit(&mut self, id: u64, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = Some(id);
        self.caret = at;
        self.sel_anchor = at;
        self.marked_range = None;
        self.focus.focus(window, cx);
    }

    /// Whether world point `p` lands on the text being edited (its padded bounds).
    fn point_in_editing_text(&self, id: u64, p: [f32; 2]) -> bool {
        let pad = SELECT_PAD / self.scene.camera.zoom.max(MIN_ZOOM);
        self.scene
            .elements
            .iter()
            .find(|e| e.id == id)
            .is_some_and(|e| {
                let (x0, y0, x1, y1) = bbox(&e.kind);
                p[0] >= x0 - pad && p[0] <= x1 + pad && p[1] >= y0 - pad && p[1] <= y1 + pad
            })
    }

    /// A press inside the text being edited: place the caret at the nearest letter,
    /// extend on Shift, select the word on a double-click, else start a drag-select.
    fn place_caret_from_click(
        &mut self,
        id: u64,
        p: [f32; 2],
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tg) = self.edit_target(id) else {
            return;
        };
        let local = block_local(tg.x, tg.y, tg.rotation, tg.pivot, p);
        let idx = self
            .font
            .index_at_wrapped(&tg.content, tg.size, tg.wrap, local);
        if ev.click_count >= 2 {
            let (s, e) = word_range(&tg.content, idx);
            self.sel_anchor = s;
            self.caret = e;
            self.text_selecting = false;
        } else {
            self.caret = idx;
            if !ev.modifiers.shift {
                self.sel_anchor = idx;
            }
            self.text_selecting = true;
        }
        // Clicking establishes a new native selection and cancels any stale
        // composition range left by an IME that did not send `unmark_text`.
        self.marked_range = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Finish editing the current text element, dropping it if it's empty.
    fn commit_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.text_selecting = false;
        self.pending_style = None;
        self.marked_range = None;
        self.format_flyout = false;
        let Some(id) = self.editing.take() else {
            return;
        };
        if let Some(e) = self.scene.elements.iter_mut().find(|e| e.id == id)
            && is_closed_shape(&e.kind)
            && e.label.as_deref().is_none_or(|s| s.trim().is_empty())
        {
            // A shape stays put; an empty label is just cleared (not persisted).
            e.label = None;
        }
        // An empty free-text element has no purpose of its own → remove it.
        self.scene.elements.retain(|e| {
            e.id != id || !matches!(&e.kind, ElementKind::Text(t) if t.content.trim().is_empty())
        });
        self.dirty = true;
        cx.notify();
        self.flush(window, cx);
    }

    /// Handle a board keyboard shortcut (the board has focus and isn't editing
    /// text). Returns whether the key was consumed. Single letters pick a tool;
    /// ⌫/Del clears the selection's elements; ⌘Z / ⌘⇧Z undo / redo; ⌘C / ⌘X / ⌘V
    /// copy / cut / paste; ⌘] / ⌘[ (± ⇧) reorder z-order; Esc deselects. ⌘V with no
    /// copied elements and other modified chords (⌘W, …) pass through to the host.
    fn handle_shortcut(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let ks = &ev.keystroke;
        let cmd = ks.modifiers.platform || ks.modifiers.control;
        if cmd && ks.key == "z" {
            if ks.modifiers.shift {
                self.redo(window, cx);
            } else {
                self.undo(window, cx);
            }
            return true;
        }
        // Z-order: ⌘] / ⌘[ nudge one step, ⌘⇧] / ⌘⇧[ go all the way. Some keymaps
        // report the shifted bracket as `}` / `{`, so treat that as "all the way"
        // too. Only consumed when something is selected.
        let close = ks.key == "]" || ks.key == "}";
        let open = ks.key == "[" || ks.key == "{";
        if cmd && (close || open) {
            if self.selected.is_empty() {
                return false;
            }
            let all_the_way = ks.modifiers.shift || ks.key == "}" || ks.key == "{";
            let op = match (close, all_the_way) {
                (true, true) => ZOrder::ToFront,
                (true, false) => ZOrder::Forward,
                (false, true) => ZOrder::ToBack,
                (false, false) => ZOrder::Backward,
            };
            self.reorder_selection(op, window, cx);
            return true;
        }
        // Copy / cut the selection to the clipboard (the host's `on_copy` writes
        // it). ⌘V paste is left to propagate so the host can read the clipboard and
        // prefer elements over an image. ⌘C/⌘X are consumed even with nothing
        // selected, so they never fall through to a text copy on the board.
        if cmd && ks.key == "c" {
            self.copy_selection(window, cx);
            return true;
        }
        if cmd && ks.key == "x" {
            if self.copy_selection(window, cx) {
                self.delete_selected(window, cx);
            }
            return true;
        }
        if cmd && ks.key == "v" {
            // Paste copied elements; if the clipboard holds none, fall through so
            // the host can paste a clipboard image instead.
            return self.try_paste(window, cx);
        }
        if cmd || ks.modifiers.alt {
            return false;
        }
        if let Some(tool) = Tool::shortcut(&ks.key) {
            self.set_tool(tool, cx);
            return true;
        }
        match ks.key.as_str() {
            "backspace" | "delete" => self.delete_selected(window, cx),
            "escape" if !self.selected.is_empty() => {
                self.selected.clear();
                cx.notify();
            }
            _ => return false,
        }
        true
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            cx.propagate();
            return;
        }
        // Escape cancels an in-progress connector without losing the source
        // shape selection; its four direction buttons become visible again.
        if ev.keystroke.key == "escape" && self.connecting.is_some() {
            self.connecting = None;
            self.pending = None;
            self.hovered_connector = None;
            cx.notify();
            return;
        }
        // Escape closes an open color picker or the templates modal (when the
        // board holds focus).
        if ev.keystroke.key == "escape" && (self.picker.is_some() || self.templates_open) {
            self.picker = None;
            self.templates_open = false;
            cx.notify();
            return;
        }
        // While dragging the toolbar, `R` flips its orientation (row ↔ column);
        // other keys are swallowed so they can't change tools mid-drag.
        if self.toolbar_drag.is_some() {
            let ks = &ev.keystroke;
            if ks.key == "r" && !(ks.modifiers.platform || ks.modifiers.control || ks.modifiers.alt)
            {
                self.toggle_toolbar_orientation(window, cx);
            }
            return;
        }
        // Not editing text → keys are board shortcuts (tools, delete, undo/redo).
        if self.editing.is_none() {
            if !self.handle_shortcut(ev, window, cx) {
                cx.propagate();
            }
            return;
        }
        // Editing → full text-box key handling (caret, selection, edit, clipboard).
        self.text_edit_key(ev, window, cx);
    }
}
