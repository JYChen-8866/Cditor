//! GPUI-native whiteboard — unified core model + adapter.
//!
//! Combines Drafft Ink's data model (shapes, canvas, tools, etc.) with
//! the GPUI rendering adapter in a single crate.

// --- Core data model modules (from drafftink-core) ---
pub mod camera;
pub mod canvas;
pub mod collaboration;
pub mod crdt;
pub mod elbow;
pub mod excalidraw;
mod image_decode;
pub mod input;
pub mod mermaid;
pub mod selection;
pub mod shapes;
pub mod snap;
pub mod storage;
pub mod sync;
pub mod tools;
pub mod widget;

// --- GPUI adapter modules derived from the Drafft Ink integration ---
pub mod font;
pub mod model_host;
pub mod paint;
pub mod theme;
pub mod view;

// --- Core re-exports ---
pub use camera::Camera;
pub use canvas::{Canvas, CanvasDocument};
pub use collaboration::CollaborationManager;
pub use crdt::CrdtDocument;
pub use excalidraw::{LibraryItem, library_from_excalidrawlib, library_layout_grid};
pub use input::InputState;
pub use mermaid::shapes_from_mermaid;
pub use selection::{ManipulationState, MultiMoveState};
pub use snap::{
    ENDPOINT_SNAP_RADIUS, EQUAL_SPACING_SNAP_RADIUS, GRID_SIZE, MULTI_MOVE_SNAP_RADIUS,
    SMART_GUIDE_THRESHOLD, SmartGuide, SmartGuideKind, SmartGuideResult, SnapResult,
    detect_smart_guides, detect_smart_guides_for_point, snap_point, snap_ray_to_smart_guides,
    snap_to_grid,
};
pub use sync::{ConnectionState, PlatformWebSocket, SyncEvent};
pub use widget::{EditingKind, Handle, HandleKind, HandleShape, WidgetManager, WidgetState};

// --- Adapter re-exports ---
pub use font::{CANVAS_FONT_FAMILY, UI_FONT_FAMILY, bundled_fonts, cjk_fallback_fonts};
pub use model_host::document::{parse_document, parse_document_json, parse_library};
pub use model_host::{DrafftBoard, PointerOutcome};
pub use theme::WhiteboardTheme;
pub use view::{
    DrafftBoardView, DrafftChromeMode, FocusRequestFn, SceneChangeFn, bind_drafft_keys,
};
