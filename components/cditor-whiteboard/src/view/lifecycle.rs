impl WhiteboardView {
    pub fn new(scene: Scene, style: WhiteboardStyleFn, cx: &mut Context<Self>) -> Self {
        let next_id = scene
            .elements
            .iter()
            .map(|e| e.id)
            .max()
            .map_or(0, |m| m + 1);
        Self {
            scene,
            style,
            read_only: false,
            on_change: None,
            on_place_embed: None,
            on_open: None,
            on_save_template: None,
            on_delete_template: None,
            on_image: None,
            on_place_image: None,
            on_drop_files: None,
            on_copy: None,
            on_paste: None,
            on_save_colors: None,
            on_pick_font: None,
            on_move_toolbar: None,
            saved_colors: Vec::new(),
            templates: Vec::new(),
            context_menu: None,
            ctx_text_sub: false,
            format_flyout: false,
            font: Font::default(),
            text_layout_cache: HashMap::new(),
            label_layout_cache: HashMap::new(),
            tool: Tool::Pan,
            focus: cx.focus_handle(),
            editing: None,
            caret: 0,
            sel_anchor: 0,
            text_selecting: false,
            marked_range: None,
            bounds: Rc::new(Cell::new(Bounds::default())),
            pending: None,
            selected: Vec::new(),
            marquee: None,
            hovered_connector: None,
            connecting: None,
            drag_from: None,
            move_origin: [0.0, 0.0],
            moved: false,
            alignment_guides: AlignmentGuides::default(),
            resizing: None,
            group_resizing: None,
            endpoint: None,
            rotating: None,
            active_stroke: None,
            active_fill: None,
            active_text: None,
            pending_style: None,
            active_width: NIB,
            picker: None,
            open_group: None,
            width_open: false,
            font_open: false,
            templates_open: false,
            picker_drag: None,
            picker_bounds: Rc::new(Cell::new(Bounds::default())),
            sv_bounds: Rc::new(Cell::new(Bounds::default())),
            hue_bounds: Rc::new(Cell::new(Bounds::default())),
            alpha_bounds: Rc::new(Cell::new(Bounds::default())),
            width_panel_bounds: Rc::new(Cell::new(Bounds::default())),
            width_bounds: Rc::new(Cell::new(Bounds::default())),
            toolbar_bounds: Rc::new(Cell::new(Bounds::default())),
            toolbar_grip_bounds: Rc::new(Cell::new(Bounds::default())),
            toolbar_pos: None,
            toolbar_vertical: false,
            toolbar_drag: None,
            history: Vec::new(),
            redo: Vec::new(),
            panning: false,
            last: Point::default(),
            next_id,
            dirty: false,
        }
    }

    /// Build a read-only board view. Useful when embedding inside other editors
    /// that should only preview the board and allow viewport movement.
    pub fn new_read_only(scene: Scene, style: WhiteboardStyleFn, cx: &mut Context<Self>) -> Self {
        let mut this = Self::new(scene, style, cx);
        this.read_only = true;
        this.tool = Tool::Pan;
        this
    }

    /// Install the persistence hook (called with the serialized scene on change).
    pub fn set_on_change(&mut self, f: ChangeFn) {
        self.on_change = Some(f);
    }

    /// Install the page-card placement hook (page-card tool click).
    pub fn set_on_place_embed(&mut self, f: PlaceEmbedFn) {
        self.on_place_embed = Some(f);
    }

    /// Install the open-page hook (double-click a card).
    pub fn set_on_open(&mut self, f: OpenPageFn) {
        self.on_open = Some(f);
    }

    /// Install the save-template hook (right-click selection → save).
    pub fn set_on_save_template(&mut self, f: SaveTemplateFn) {
        self.on_save_template = Some(f);
    }

    /// Install the delete-template hook (right-click a template card → delete).
    pub fn set_on_delete_template(&mut self, f: DeleteTemplateFn) {
        self.on_delete_template = Some(f);
    }

    /// Install the image-fetch hook (decoded bitmap for an element's `src`).
    pub fn set_on_image(&mut self, f: ImageFn) {
        self.on_image = Some(f);
    }

    /// Install the place-image hook (image tool click → host file picker).
    pub fn set_on_place_image(&mut self, f: PlaceImageFn) {
        self.on_place_image = Some(f);
    }

    /// Install the file-drop hook (files dropped on the canvas).
    pub fn set_on_drop_files(&mut self, f: DropFilesFn) {
        self.on_drop_files = Some(f);
    }

    /// Install the copy hook (⌘C / ⌘X → write the selection to the clipboard).
    pub fn set_on_copy(&mut self, f: CopyFn) {
        self.on_copy = Some(f);
    }

    /// Install the paste hook (context-menu Paste → read board elements from the
    /// clipboard). Without it, the Paste menu item is hidden.
    pub fn set_on_paste(&mut self, f: PasteFn) {
        self.on_paste = Some(f);
    }

    /// Install the saved-colors hook (the palette changed → host persists it).
    pub fn set_on_save_colors(&mut self, f: SavedColorsFn) {
        self.on_save_colors = Some(f);
    }

    /// Install the font-picker hook (the Font toolbar button). Without it, the
    /// Font button is hidden. The host shows the file dialog, builds the face, and
    /// calls [`set_font`](Self::set_font).
    pub fn set_on_pick_font(&mut self, f: PickFontFn) {
        self.on_pick_font = Some(f);
    }

    /// Install the toolbar-moved hook (the host persists the new position).
    pub fn set_on_move_toolbar(&mut self, f: MoveToolbarFn) {
        self.on_move_toolbar = Some(f);
    }

    /// Toggle read-only mode. In this mode the board behaves like a fixed move
    /// tool: left-drag pans the canvas and edit interactions are ignored.
    pub fn set_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        self.read_only = read_only;
        if read_only {
            self.tool = Tool::Pan;
            self.selected.clear();
            self.editing = None;
            self.pending = None;
            self.connecting = None;
            self.hovered_connector = None;
            self.context_menu = None;
            self.open_group = None;
            self.font_open = false;
            self.width_open = false;
            self.templates_open = false;
            self.picker = None;
            self.format_flyout = false;
            self.text_selecting = false;
            self.marked_range = None;
        }
        cx.notify();
    }

    /// Whether the board is currently in read-only (forced pan) mode.
    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// Set the toolbar's board-relative top-left (`None` = default top-center). The
    /// host pushes the persisted position on open and after a change.
    pub fn set_toolbar_pos(&mut self, pos: Option<(f32, f32)>, cx: &mut Context<Self>) {
        self.toolbar_pos = pos;
        cx.notify();
    }

    /// Set the toolbar orientation (vertical = a column). The host pushes the
    /// persisted value on open and after a change.
    pub fn set_toolbar_vertical(&mut self, vertical: bool, cx: &mut Context<Self>) {
        self.toolbar_vertical = vertical;
        cx.notify();
    }

    /// Flip the toolbar orientation (row ↔ column) and persist. Bound to `R` while
    /// the bar is being dragged.
    fn toggle_toolbar_orientation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.toolbar_vertical = !self.toolbar_vertical;
        if let Some(f) = self.on_move_toolbar.clone() {
            f(self.toolbar_pos, self.toolbar_vertical, window, cx);
        }
        cx.notify();
    }

    /// Clamp a board-relative toolbar top-left so the pill stays fully on-board.
    fn clamp_toolbar(&self, x: f32, y: f32) -> (f32, f32) {
        let board = self.bounds.get().size;
        let pill = self.toolbar_bounds.get().size;
        let maxx = (f32::from(board.width) - f32::from(pill.width)).max(0.0);
        let maxy = (f32::from(board.height) - f32::from(pill.height)).max(0.0);
        (x.clamp(0.0, maxx), y.clamp(0.0, maxy))
    }

    /// Start dragging the toolbar from a grip press (window coords). A double-click
    /// resets it to the default top-center.
    fn start_toolbar_drag(
        &mut self,
        p: Point<Pixels>,
        double: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if double {
            self.toolbar_drag = None;
            self.toolbar_pos = None;
            if let Some(f) = self.on_move_toolbar.clone() {
                f(None, self.toolbar_vertical, window, cx);
            }
            cx.notify();
            return;
        }
        // Close any popover so it doesn't trail the bar while it's dragged.
        self.picker = None;
        self.open_group = None;
        self.width_open = false;
        self.font_open = false;
        self.templates_open = false;
        self.context_menu = None;
        // Take focus so `R` (flip orientation) reaches the key handler mid-drag.
        self.focus.focus(window, cx);
        let pill = self.toolbar_bounds.get().origin;
        self.toolbar_drag = Some((
            f32::from(pill.x) - f32::from(p.x),
            f32::from(pill.y) - f32::from(p.y),
        ));
        cx.notify();
    }

    /// Update the toolbar position while dragging (window-coords cursor).
    fn drag_toolbar(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((ox, oy)) = self.toolbar_drag else {
            return;
        };
        let board = self.bounds.get().origin;
        let x = f32::from(p.x) + ox - f32::from(board.x);
        let y = f32::from(p.y) + oy - f32::from(board.y);
        self.toolbar_pos = Some(self.clamp_toolbar(x, y));
        cx.notify();
    }

    /// Finish a toolbar drag and persist the new position.
    fn commit_toolbar_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.toolbar_drag.take().is_none() {
            return;
        }
        if let Some(f) = self.on_move_toolbar.clone() {
            f(self.toolbar_pos, self.toolbar_vertical, window, cx);
        }
    }

    /// Replace the user's saved-color palette (the host pushes the persisted list
    /// on open and after a change).
    pub fn set_saved_colors(&mut self, colors: Vec<u32>, cx: &mut Context<Self>) {
        self.saved_colors = colors;
        cx.notify();
    }

    /// Save the picker's current color to the palette (the `+` in the picker),
    /// then notify the host to persist. Ignores duplicates.
    fn save_current_color(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = self.picker_u32()
            && !self.saved_colors.contains(&c)
        {
            self.saved_colors.push(c);
            if let Some(f) = self.on_save_colors.clone() {
                f(self.saved_colors.clone(), window, cx);
            }
        }
        cx.notify();
    }

    /// Remove a saved color from the palette (right-click a swatch), then persist.
    fn remove_saved_color(&mut self, c: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.saved_colors.retain(|&x| x != c);
        if let Some(f) = self.on_save_colors.clone() {
            f(self.saved_colors.clone(), window, cx);
        }
        cx.notify();
    }

    /// Replace the stored templates shown in the Pages & Images flyout. The host
    /// calls this on open and after any save/delete.
    pub fn set_templates(&mut self, templates: Vec<Template>, cx: &mut Context<Self>) {
        self.templates = templates;
        cx.notify();
    }

    /// Swap the font used to render text (e.g. a user-uploaded face). Build one
    /// with [`Font::from_bytes`].
    pub fn set_font(&mut self, font: Font, cx: &mut Context<Self>) {
        self.font = font;
        self.text_layout_cache.clear();
        self.label_layout_cache.clear();
        cx.notify();
    }

    /// Build a `.tooltip(..)` closure for a toolbar control — a small themed
    /// [`Tip`], reading colors through the style closure at show time.
    fn tip(
        &self,
        text: impl Into<SharedString>,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let style_fn = self.style.clone();
        let text = text.into();
        move |_window, cx| {
            let s = style_fn();
            let text = text.clone();
            cx.new(move |_| Tip {
                text,
                fg: s.ink,
                bg: s.panel,
                border: s.grid,
            })
            .into()
        }
    }

    /// Insert a page-card at world `(x, y)` and select it. Called by the host
    /// after the user picks a page (in response to [`PlaceEmbedFn`]). Does *not*
    /// fire `on_change` — the host calls this mid-update, so a re-entrant save
    /// would panic; the host persists explicitly via [`scene`](Self::scene).
    pub fn add_embed(
        &mut self,
        page_id: i64,
        title: impl Into<String>,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.push_undo();
        let id = self.next_id;
        self.next_id += 1;
        let zoom = self.scene.camera.zoom.max(MIN_ZOOM);
        self.scene.elements.push(Element {
            id,
            kind: ElementKind::Embed(EmbedGeom {
                page_id,
                title: title.into(),
                x,
                y,
                w: EMBED_W / zoom,
                h: EMBED_H / zoom,
            }),
            stroke: None,
            fill: None,
            label: None,
            label_color: None,
            styles: Vec::new(),
            mindmap: None,
        });
        self.selected = vec![id];
        self.tool = Tool::Select;
        cx.notify();
    }

    /// Add an image element referencing `src`, centered at world `(cx_world,
    /// cy_world)` and sized from its pixel dimensions (`px_w`/`px_h`) so the longest
    /// edge gets a sensible default on-screen size (aspect preserved). Like
    /// [`add_embed`], the host persists afterward (this is called mid-host-update).
    ///
    /// [`add_embed`]: Self::add_embed
    pub fn add_image_at(
        &mut self,
        src: impl Into<String>,
        px_w: f32,
        px_h: f32,
        cx_world: f32,
        cy_world: f32,
        cx: &mut Context<Self>,
    ) {
        self.push_undo();
        let id = self.next_id;
        self.next_id += 1;
        let zoom = self.scene.camera.zoom.max(MIN_ZOOM);
        let longest = px_w.max(px_h).max(1.0);
        let scale = IMAGE_PLACE_PX / longest / zoom;
        let (w, h) = (px_w * scale, px_h * scale);
        self.scene.elements.push(Element {
            id,
            kind: ElementKind::Image(ImageGeom {
                src: src.into(),
                x: cx_world - w / 2.0,
                y: cy_world - h / 2.0,
                w,
                h,
                rotation: 0.0,
            }),
            stroke: None,
            fill: None,
            label: None,
            label_color: None,
            styles: Vec::new(),
            mindmap: None,
        });
        self.selected = vec![id];
        self.tool = Tool::Select;
        cx.notify();
    }

}
