/// The active tool. UI state — not part of the persisted scene.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    /// Drag to pan the canvas (the default — navigation before drawing).
    Pan,
    Select,
    Pen,
    Rect,
    Ellipse,
    Diamond,
    Triangle,
    RoundRect,
    Star,
    Hexagon,
    Line,
    Arrow,
    DashedArrow,
    Text,
    MindMap,
    Flowchart,
    Embed,
    Image,
}

impl Tool {
    /// A glyph for the toolbar button (dependency-free; the host has no icon set
    /// in this crate).
    fn glyph(self) -> &'static str {
        match self {
            // A dingbat hand (pre-emoji, so it always renders flat/monochrome —
            // unlike ✋, which macOS re-colors even with a VS15 text request).
            Tool::Pan => "☞",
            Tool::Select => "↖",
            Tool::Pen => "✎",
            Tool::Rect => "▭",
            Tool::Ellipse => "◯",
            Tool::Diamond => "◇",
            Tool::Triangle => "△",
            Tool::RoundRect => "▢",
            Tool::Star => "☆",
            Tool::Hexagon => "⬡",
            Tool::Line => "╱",
            Tool::Arrow => "↗",
            Tool::DashedArrow => "⇢",
            Tool::Text => "T",
            Tool::MindMap => "◎",
            Tool::Flowchart => "⇅",
            Tool::Embed => "▤",
            Tool::Image => "▦",
        }
    }

    /// A human label for the tooltip (the toolbar is icon-only), with the
    /// keyboard shortcut where one exists (see [`shortcut`](Tool::shortcut)).
    fn label(self) -> &'static str {
        match self {
            Tool::Pan => "Pan — drag to move (H)",
            Tool::Select => "Select (V)",
            Tool::Pen => "Pen (P)",
            Tool::Rect => "Rectangle (R)",
            Tool::Ellipse => "Ellipse (O)",
            Tool::Diamond => "Diamond (D)",
            Tool::Triangle => "Triangle (G)",
            Tool::RoundRect => "Rounded rectangle (U)",
            Tool::Star => "Star (S)",
            Tool::Hexagon => "Hexagon (X)",
            Tool::Line => "Line (L)",
            Tool::Arrow => "Arrow (A)",
            Tool::DashedArrow => "Dashed arrow (K)",
            Tool::Text => "Text (T)",
            Tool::MindMap => "Mind map (M)",
            Tool::Flowchart => "Flowchart (F)",
            Tool::Embed => "Page card",
            Tool::Image => "Image (I) — click to place",
        }
    }

    /// The single-key shortcut that selects this tool, if any.
    fn shortcut(key: &str) -> Option<Tool> {
        Some(match key {
            "h" => Tool::Pan,
            "v" => Tool::Select,
            "p" => Tool::Pen,
            "r" => Tool::Rect,
            "o" => Tool::Ellipse,
            "d" => Tool::Diamond,
            "g" => Tool::Triangle,
            "u" => Tool::RoundRect,
            "s" => Tool::Star,
            "x" => Tool::Hexagon,
            "l" => Tool::Line,
            "a" => Tool::Arrow,
            "k" => Tool::DashedArrow,
            "t" => Tool::Text,
            "m" => Tool::MindMap,
            "f" => Tool::Flowchart,
            "i" => Tool::Image,
            _ => return None,
        })
    }

    /// The bundled SVG icon for this tool as `(cache-key, bytes)`, or `None` to
    /// fall back to [`glyph`]. Rendered flat in the theme color via gpui's SVG
    /// rasterizer (the SVG's own colors are ignored — it's tinted as an alpha
    /// mask). Lucide, ISC-licensed (see `assets/icons/LICENSE`).
    ///
    /// [`glyph`]: Tool::glyph
    fn icon(self) -> Option<(&'static str, &'static [u8])> {
        const PAN: &[u8] = include_bytes!("../assets/icons/pan.svg");
        const SELECT: &[u8] = include_bytes!("../assets/icons/select.svg");
        const PEN: &[u8] = include_bytes!("../assets/icons/pen.svg");
        const RECT: &[u8] = include_bytes!("../assets/icons/rect.svg");
        const ELLIPSE: &[u8] = include_bytes!("../assets/icons/ellipse.svg");
        const DIAMOND: &[u8] = include_bytes!("../assets/icons/diamond.svg");
        const TRIANGLE: &[u8] = include_bytes!("../assets/icons/triangle.svg");
        const ROUND_RECT: &[u8] = include_bytes!("../assets/icons/round-rect.svg");
        const STAR: &[u8] = include_bytes!("../assets/icons/star.svg");
        const HEXAGON: &[u8] = include_bytes!("../assets/icons/hexagon.svg");
        const LINE: &[u8] = include_bytes!("../assets/icons/line.svg");
        const ARROW: &[u8] = include_bytes!("../assets/icons/arrow.svg");
        const TEXT: &[u8] = include_bytes!("../assets/icons/text.svg");
        const MINDMAP: &[u8] = include_bytes!("../assets/icons/mindmap.svg");
        const FLOWCHART: &[u8] = include_bytes!("../assets/icons/flowchart.svg");
        const EMBED: &[u8] = include_bytes!("../assets/icons/embed.svg");
        const IMAGE: &[u8] = include_bytes!("../assets/icons/image.svg");
        match self {
            Tool::Pan => Some(("wb-icon-pan", PAN)),
            Tool::Select => Some(("wb-icon-select", SELECT)),
            Tool::Pen => Some(("wb-icon-pen", PEN)),
            Tool::Rect => Some(("wb-icon-rect", RECT)),
            Tool::Ellipse => Some(("wb-icon-ellipse", ELLIPSE)),
            Tool::Diamond => Some(("wb-icon-diamond", DIAMOND)),
            Tool::Triangle => Some(("wb-icon-triangle", TRIANGLE)),
            Tool::RoundRect => Some(("wb-icon-round-rect", ROUND_RECT)),
            Tool::Star => Some(("wb-icon-star", STAR)),
            Tool::Hexagon => Some(("wb-icon-hexagon", HEXAGON)),
            Tool::Line => Some(("wb-icon-line", LINE)),
            Tool::Arrow => Some(("wb-icon-arrow", ARROW)),
            Tool::DashedArrow => None,
            Tool::Text => Some(("wb-icon-text", TEXT)),
            Tool::MindMap => Some(("wb-icon-mindmap", MINDMAP)),
            Tool::Flowchart => Some(("wb-icon-flowchart", FLOWCHART)),
            Tool::Embed => Some(("wb-icon-embed", EMBED)),
            Tool::Image => Some(("wb-icon-image", IMAGE)),
        }
    }
}

/// A toolbar category whose tools live in a click-to-open flyout, keeping the
/// main bar trim. The category button shows the active tool of the group (or a
/// representative when none is active).
#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolGroup {
    /// Freehand pen and the closed shapes.
    Shapes,
    /// Line and arrow connectors.
    Lines,
    /// Page-cards (and, later, images).
    PagesImages,
}

impl ToolGroup {
    const ALL: [ToolGroup; 3] = [ToolGroup::Shapes, ToolGroup::Lines, ToolGroup::PagesImages];

    /// The tools shown in this group's flyout.
    fn tools(self) -> &'static [Tool] {
        match self {
            ToolGroup::Shapes => &[
                Tool::Rect,
                Tool::RoundRect,
                Tool::Ellipse,
                Tool::Diamond,
                Tool::Triangle,
                Tool::Hexagon,
                Tool::Star,
            ],
            ToolGroup::Lines => &[Tool::Pen, Tool::Line, Tool::Arrow, Tool::DashedArrow],
            ToolGroup::PagesImages => &[Tool::MindMap, Tool::Flowchart, Tool::Embed, Tool::Image],
        }
    }

    fn contains(self, t: Tool) -> bool {
        self.tools().contains(&t)
    }

    /// The icon shown on the category button when none of its tools is active.
    fn representative(self) -> Tool {
        match self {
            ToolGroup::Shapes => Tool::Rect,
            ToolGroup::Lines => Tool::Arrow,
            ToolGroup::PagesImages => Tool::Flowchart,
        }
    }

    fn label(self) -> &'static str {
        match self {
            ToolGroup::Shapes => "Shapes",
            ToolGroup::Lines => "Lines",
            ToolGroup::PagesImages => "Pages & images",
        }
    }
}

/// A flat, theme-colored toolbar icon: render the bundled SVG `bytes` (a 16×16
/// Lucide glyph) tinted to `color` via gpui's rasterizer, in a `size`-px box.
/// `key` is a stable per-icon cache id.
fn svg_icon(key: &'static str, bytes: &'static [u8], color: Hsla, sz: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let _ = window.paint_svg(
                bounds,
                SharedString::from(key),
                Some(bytes),
                TransformationMatrix::default(),
                color,
                cx,
            );
        },
    )
    .w(px(sz))
    .h(px(sz))
}

/// A hairline vertical divider separating toolbar tool groups.
fn toolbar_divider(color: Hsla, vertical: bool) -> gpui::AnyElement {
    let d = div().bg(color);
    // A row's dividers are vertical bars; a column's are horizontal.
    if vertical {
        d.h(px(1.0)).w(px(16.0)).my(px(3.0))
    } else {
        d.w(px(1.0)).h(px(16.0)).mx(px(3.0))
    }
    .into_any_element()
}

/// A minimal themed tooltip view. gpui has the `.tooltip()` *hook* but no
/// tooltip *view* (those live in UI crates this crate doesn't depend on), so —
/// like `gpui-pdf` — we render our own small label.
struct Tip {
    text: SharedString,
    fg: Hsla,
    bg: Hsla,
    border: Hsla,
}

impl Render for Tip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // gpui anchors the tooltip at the cursor; a small transparent top
        // padding drops the visible box just clear of the hovered button.
        div().pt(px(16.0)).child(
            div()
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(self.border)
                .bg(self.bg)
                .text_color(self.fg)
                .text_size(px(11.0))
                .child(self.text.clone()),
        )
    }
}

/// Theme colors, read at paint time (via [`WhiteboardStyleFn`]) so the board
/// follows live theme changes per window.
#[derive(Clone, Debug)]
pub struct WhiteboardStyle {
    /// The canvas background.
    pub bg: Hsla,
    /// The background grid dots.
    pub grid: Hsla,
    /// HUD / muted on-canvas text.
    pub text: Hsla,
    /// Ink (stroke/shape color). Per-element color comes with the color picker.
    pub ink: Hsla,
    /// Toolbar / flyout panel background — small pills, so it can be quite glassy.
    pub panel: Hsla,
    /// Background for the larger color-picker panel. Wants to stay readable over
    /// a busy canvas, so it should be much more opaque than `panel`.
    pub panel_strong: Hsla,
    /// Active-tool highlight (a subtle fill behind the current tool button).
    pub accent: Hsla,
    /// Selection outline — wants to be clearly visible, so a strong color.
    pub selection: Hsla,
    /// Palette shown as quick swatches in the color picker. The host supplies
    /// these (typically its theme colors) so the picker matches the app.
    pub swatches: Vec<Hsla>,
}

/// A `() -> WhiteboardStyle` the host supplies; called each paint so the board
/// tracks theme changes without the host pushing updates.
pub type WhiteboardStyleFn = Rc<dyn Fn() -> WhiteboardStyle>;

/// Called when the board changes (an element committed/moved/deleted, the camera
/// moved), with the serialized scene JSON, so the host can persist it.
pub type ChangeFn = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// Called when the page-card tool is clicked at world `(x, y)` — the host picks
/// a page and calls [`WhiteboardView::add_embed`].
pub type PlaceEmbedFn = Rc<dyn Fn(f32, f32, &mut Window, &mut App)>;

/// Called to open a page (double-clicking a card) — the host opens it in a tab.
pub type OpenPageFn = Rc<dyn Fn(i64, &mut Window, &mut App)>;

/// Called when the user saves the current selection as a template, with the
/// selected elements serialized (normalized to origin). The host names + stores
/// it, then feeds the updated list back via [`WhiteboardView::set_templates`].
pub type SaveTemplateFn = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// Called to delete a stored template by its host id (right-click a card).
pub type DeleteTemplateFn = Rc<dyn Fn(i64, &mut Window, &mut App)>;

/// Called on ⌘C / ⌘X with the selection serialized (same format as
/// [`SaveTemplateFn`]); the host writes it to the system clipboard. Paste is the
/// reverse: the host reads the clipboard and calls [`WhiteboardView::paste_elements`].
pub type CopyFn = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// Called by the context-menu **Paste**: the host reads the clipboard and returns
/// previously copied whiteboard elements (the JSON a [`CopyFn`] wrote — same format
/// as [`SaveTemplateFn`]), or `None` if it holds no board elements. Pass the JSON to
/// [`WhiteboardView::paste_elements`]. (Keyboard ⌘V is handled internally.)
pub type PasteFn = Rc<dyn Fn(&mut Window, &mut App) -> Option<String>>;

/// Called when the user's saved-color palette changes (a swatch added or removed),
/// with the full list (packed `0xRRGGBBAA`). The host persists it and feeds it back
/// via [`WhiteboardView::set_saved_colors`]. Without it, the palette is per-session.
pub type SavedColorsFn = Rc<dyn Fn(Vec<u32>, &mut Window, &mut App)>;

/// Called each render to fetch the decoded bitmap for an image element's `src`,
/// rotated by `rotation` radians (0 = upright). The host serves it from its image
/// cache, decoding/rotating on demand (returning `None` until ready, then
/// re-rendering the board); a steady angle hits the cache, so it only re-rotates
/// when the angle changes.
pub type ImageFn = Rc<dyn Fn(&str, f32, &mut Window, &mut App) -> Option<gpui::ImageSource>>;

/// Called when the image tool is clicked at world `(x, y)` — the host picks a
/// file and calls [`WhiteboardView::add_image_at`].
pub type PlaceImageFn = Rc<dyn Fn(f32, f32, &mut Window, &mut App)>;

/// Called when files are dropped onto the canvas at world `(x, y)` — the host
/// imports any images and places them via [`WhiteboardView::add_image_at`].
pub type DropFilesFn = Rc<dyn Fn(Vec<std::path::PathBuf>, f32, f32, &mut Window, &mut App)>;

/// Which face the Font flyout offers — upload one from disk or revert to the
/// bundled default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontPick {
    /// Pick a `.ttf`/`.otf` from disk (the host shows the file dialog).
    Upload,
    /// Revert to the bundled default face.
    Default,
}

/// Called when the user picks from the Font flyout. The host loads the face and
/// calls [`WhiteboardView::set_font`] (and persists the per-board choice). Without
/// it, the Font toolbar button is hidden.
pub type PickFontFn = Rc<dyn Fn(FontPick, &mut Window, &mut App)>;

/// Called when the toolbar is moved, reset, or re-oriented, with its new
/// board-relative top-left (`None` = default top-center) and whether it's vertical.
/// The host persists both and feeds them back via [`WhiteboardView::set_toolbar_pos`]
/// / [`set_toolbar_vertical`](WhiteboardView::set_toolbar_vertical). Without it, the
/// layout is per-session.
pub type MoveToolbarFn = Rc<dyn Fn(Option<(f32, f32)>, bool, &mut Window, &mut App)>;

/// Host callback fired by an embed view when the user requests "open / maximize
/// for editing". The host owns the actual layout transition.
pub type ExpandEmbedFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// A reusable group of elements the user can stamp onto a board. Element
/// positions are normalized so the group's bounding box starts at the origin;
/// applying re-bases them to the viewport. The host owns persistence and the
/// `id`; the crate renders the preview + instantiates on click.
#[derive(Clone, Debug)]
pub struct Template {
    pub id: i64,
    pub name: String,
    pub elements: Vec<Element>,
}

impl Template {
    /// Build from the host's stored row. `elements_json` is a serialized
    /// `Vec<Element>` (the JSON a [`SaveTemplateFn`] handed the host); malformed
    /// JSON yields an empty (still-listable) template.
    pub fn from_json(id: i64, name: impl Into<String>, elements_json: &str) -> Self {
        Template {
            id,
            name: name.into(),
            elements: serde_json::from_str(elements_json).unwrap_or_default(),
        }
    }
}

/// An element being created by the current left-drag.
struct Pending {
    anchor: [f32; 2],
    kind: ElementKind,
}

/// A connector point shown on a hovered shape. `index` is 0/1/2/3 = top/right/bottom/left.
#[derive(Clone, Copy, PartialEq)]
struct ConnectPoint {
    id: u64,
    index: usize,
    pos: [f32; 2],
}

/// A line being dragged from a shape connector while the Select tool is active.
#[derive(Clone, Copy)]
struct ConnectDrag {
    from: ConnectPoint,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct AlignmentGuides {
    vertical: Option<f32>,
    horizontal: Option<f32>,
}

/// An in-progress resize of a single selected element by one of its handles.
struct Resizing {
    id: u64,
    /// Which handle is being dragged (corner = free/proportional, edge = one axis).
    handle: ResizeHandle,
    /// The fixed (opposite) corner/edge the scale is about, world space.
    anchor: [f32; 2],
    /// The dragged handle's original position, world space.
    from: [f32; 2],
    /// World offset from the cursor to the dragged handle at grab time, kept so
    /// it tracks the cursor 1:1 (no jump on grab).
    grab: [f32; 2],
    /// The element's geometry at the start of the resize.
    orig: ElementKind,
}

/// Which handle drives a resize ([`Resizing`] or [`GroupResizing`]): a corner
/// (both axes together) or an edge midpoint (one axis only).
#[derive(Clone, Copy)]
enum ResizeHandle {
    /// A corner grip — uniform scale about the opposite corner.
    Corner,
    /// A left/right edge grip — scales x only, about the opposite edge.
    EdgeX,
    /// A top/bottom edge grip — scales y only, about the opposite edge.
    EdgeY,
}

/// An in-progress resize of a multi-selection by a handle of its (axis-aligned)
/// group bounds. A corner scales uniformly about the opposite corner (the group
/// grows as one); an edge midpoint stretches a single axis about the opposite
/// edge. Each member is scaled from its geometry at grab so it never compounds.
struct GroupResizing {
    /// Which handle is being dragged (corner = both axes, edge = one).
    handle: ResizeHandle,
    /// The fixed point the scale is about, world space (opposite corner/edge).
    anchor: [f32; 2],
    /// The dragged handle's original position, world space.
    from: [f32; 2],
    /// Cursor → dragged-handle offset at grab (1:1 tracking, no jump).
    grab: [f32; 2],
    /// Each selected element's id + geometry at the start of the resize.
    orig: Vec<(u64, ElementKind)>,
}

/// An in-progress drag of one endpoint of a selected line/arrow.
#[derive(Clone, Copy)]
struct EndpointDrag {
    id: u64,
    /// Which endpoint: 0 = (x1,y1), 1 = (x2,y2).
    which: usize,
}

/// An in-progress rotation of the selection (one element or a group) about a
/// fixed center. Drives every selected element, so it needs no element id.
#[derive(Clone, Copy)]
struct Rotating {
    /// Pivot (world), captured at grab so it can't drift between frames.
    center: [f32; 2],
    /// Pointer angle about `center` at grab (radians).
    start_pointer: f32,
    /// Rotation already applied since grab (radians).
    applied: f32,
    /// Orientation to snap to horizontal/vertical: a single element's angle (box
    /// / text) or line direction; `Some(0)` for a group (snaps quarter-turns);
    /// `None` when there's nothing meaningful to snap (a lone freehand stroke).
    base: Option<f32>,
}

/// What a press on a selection handle begins.
enum HandleGrab {
    Corner(Resizing),
    Endpoint(EndpointDrag),
    Rotate,
    GroupCorner(GroupResizing),
}

/// Which property the picker is editing.
#[derive(Clone, Copy, PartialEq)]
enum PickerTarget {
    /// Outline / ink color (`None` = theme ink).
    Stroke,
    /// Shape fill (`None` = unfilled).
    Fill,
    /// Shape label color (`None` follows the stroke / theme ink).
    Text,
}

/// Open color-picker state: the HSVA the controls currently reflect, and which
/// property (stroke or fill) it edits. Recolors the selection live.
#[derive(Clone, Copy)]
struct Picker {
    target: PickerTarget,
    h: f32,
    s: f32,
    v: f32,
    a: f32,
}

/// Which picker control an in-progress drag is manipulating.
#[derive(Clone, Copy, PartialEq)]
enum PickerDrag {
    /// The saturation/brightness square.
    Sv,
    /// The hue strip.
    Hue,
    /// The alpha (opacity) strip.
    Alpha,
    /// The thickness flyout's custom-width slider.
    Width,
}

/// The whiteboard view entity. The host holds it in an `Entity<WhiteboardView>`
/// (keyed by board id) and renders it into a tab.
pub struct WhiteboardView {
    scene: Scene,
    style: WhiteboardStyleFn,
    read_only: bool,
    on_change: Option<ChangeFn>,
    on_place_embed: Option<PlaceEmbedFn>,
    on_open: Option<OpenPageFn>,
    on_save_template: Option<SaveTemplateFn>,
    on_delete_template: Option<DeleteTemplateFn>,
    on_image: Option<ImageFn>,
    on_place_image: Option<PlaceImageFn>,
    on_drop_files: Option<DropFilesFn>,
    on_copy: Option<CopyFn>,
    on_paste: Option<PasteFn>,
    on_save_colors: Option<SavedColorsFn>,
    on_pick_font: Option<PickFontFn>,
    on_move_toolbar: Option<MoveToolbarFn>,
    /// The user's saved colors (packed `0xRRGGBBAA`), shown in the picker's palette.
    /// Supplied + persisted by the host (see [`SavedColorsFn`]).
    saved_colors: Vec<u32>,
    /// Stored templates, supplied by the host; shown as cards in the Pages &
    /// Images flyout.
    templates: Vec<Template>,
    /// Screen position of an open right-click context menu (a selection's
    /// "save as template"), or `None`.
    context_menu: Option<Point<Pixels>>,
    /// Whether the context menu's "Text ▸" formatting submenu is expanded.
    ctx_text_sub: bool,
    /// Whether the toolbar's text-formatting fly-out is open.
    format_flyout: bool,
    /// The face used to render text as vector outlines. Defaults to the bundled
    /// JetBrains Mono; the host can swap in a custom/user-uploaded font.
    font: Font,
    /// Camera-independent glyph outlines keyed by element id and text/style
    /// signature. Panning and zooming can reuse these directly.
    text_layout_cache: HashMap<u64, CachedTextLayout>,
    label_layout_cache: HashMap<u64, CachedLabelLayout>,
    tool: Tool,
    /// Keyboard focus — grabbed while editing a text element.
    focus: FocusHandle,
    /// The text element currently being edited (Text tool / double-click).
    editing: Option<u64>,
    /// Caret position (byte offset into the editing text's content).
    caret: usize,
    /// The fixed end of the text selection (byte offset); `== caret` means no
    /// selection, just the caret.
    sel_anchor: usize,
    /// A click-drag text selection is in progress (extends the selection on move).
    text_selecting: bool,
    /// Active IME marked/composition byte range in the editing text.
    marked_range: Option<Range<usize>>,
    /// Canvas bounds in window coords, captured each paint so input handlers can
    /// map window-relative event positions into the board.
    bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The element being created by the in-progress left-drag.
    pending: Option<Pending>,
    /// The currently selected elements (Select tool).
    selected: Vec<u64>,
    /// In-progress marquee box (start, current) in world coords.
    marquee: Option<([f32; 2], [f32; 2])>,
    /// Connector point currently under/near the mouse, painted on hovered shapes.
    hovered_connector: Option<ConnectPoint>,
    /// Line creation started by pressing a connector point.
    connecting: Option<ConnectDrag>,
    /// The world point where an in-progress move-drag was grabbed (a *fixed*
    /// anchor — the move uses the total cursor delta from here, so grid-snapping
    /// stays cursor-synced and doesn't lose sub-grid motion).
    drag_from: Option<[f32; 2]>,
    /// The primary (first-selected) element's top-left at move-grab, the
    /// reference the move drives toward (`move_origin + total_delta`).
    move_origin: [f32; 2],
    /// Whether the current move-drag has actually moved (undo is pushed once).
    moved: bool,
    /// Active world-space smart-alignment guides while moving a selection.
    alignment_guides: AlignmentGuides,
    /// In-progress corner-resize of the selected box/stroke.
    resizing: Option<Resizing>,
    /// In-progress proportional resize of a multi-selection.
    group_resizing: Option<GroupResizing>,
    /// In-progress endpoint-drag of the selected line/arrow.
    endpoint: Option<EndpointDrag>,
    /// In-progress rotation of the selected element.
    rotating: Option<Rotating>,
    /// Current ink color for new elements (`None` follows the theme ink).
    active_stroke: Option<u32>,
    /// Current fill for new shapes (`None` = unfilled).
    active_fill: Option<u32>,
    /// Current label color for new shapes (`None` follows the stroke / theme ink).
    active_text: Option<u32>,
    /// Formatting to apply to the next typed text when there's no selection (set
    /// by a ⌘B/etc. toggle with a collapsed caret); cleared on caret move.
    pending_style: Option<RunStyle>,
    /// Current stroke thickness for new elements, in screen px (stored world-space
    /// as `active_width / zoom`, like [`NIB`]). Defaults to `NIB`.
    active_width: f32,
    /// Open color picker, if any.
    picker: Option<Picker>,
    /// The tool category whose flyout is open, if any.
    open_group: Option<ToolGroup>,
    /// Whether the thickness-preset flyout is open.
    width_open: bool,
    /// Whether the font flyout (upload / default) is open.
    font_open: bool,
    /// Whether the templates gallery modal is open.
    templates_open: bool,
    /// In-progress drag inside the open picker.
    picker_drag: Option<PickerDrag>,
    /// Screen bounds of the picker panel and its draggable regions, captured each
    /// paint so press/drag handlers can hit-test them.
    picker_bounds: Rc<Cell<Bounds<Pixels>>>,
    sv_bounds: Rc<Cell<Bounds<Pixels>>>,
    hue_bounds: Rc<Cell<Bounds<Pixels>>>,
    alpha_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Screen bounds of the thickness flyout panel and its width slider (captured
    /// each paint), so a press can route to the slider or dismiss the flyout.
    width_panel_bounds: Rc<Cell<Bounds<Pixels>>>,
    width_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Screen bounds of the toolbar pill and its drag grip (captured each paint),
    /// so a press routes to a drag (grip) or is consumed (pill) — the pill isn't
    /// occluded, like the picker.
    toolbar_bounds: Rc<Cell<Bounds<Pixels>>>,
    toolbar_grip_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The toolbar's board-relative top-left when the user has dragged it; `None`
    /// keeps the default top-center. Persisted by the host.
    toolbar_pos: Option<(f32, f32)>,
    /// Whether the toolbar is laid out vertically (a column) rather than as a row.
    /// Toggled with `R` while dragging it; persisted by the host.
    toolbar_vertical: bool,
    /// In-progress toolbar drag: the (pill origin − cursor) offset, board-relative.
    toolbar_drag: Option<(f32, f32)>,
    /// Undo / redo stacks of scene snapshots.
    history: Vec<Scene>,
    redo: Vec<Scene>,
    /// True while a middle-drag pan is in progress.
    panning: bool,
    /// Last pointer position (window coords) during a pan.
    last: Point<Pixels>,
    /// Next element id.
    next_id: u64,
    /// Unsaved changes since the last flush (flushed on mouse-up).
    dirty: bool,
}

/// A read-only whiteboard embedding surface for use inside rich-text editors and
/// other host containers. It overlays a small "edit / maximize" affordance and
/// delegates the actual expansion behavior back to the host.
pub struct BoardEmbedView {
    board: Entity<WhiteboardView>,
    style: WhiteboardStyleFn,
    on_expand: Option<ExpandEmbedFn>,
}

/// A lightweight, chrome-free thumbnail renderer for embedding a local board
/// snapshot in documents, lists, and rich-text blocks.
pub struct BoardThumbnailView {
    snapshot: LocalThumbnailSnapshot,
    style: WhiteboardStyleFn,
    font: Font,
}
