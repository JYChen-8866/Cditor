//! Handle definitions for widget manipulation.

use kurbo::Point;

/// A manipulation handle on a widget.
#[derive(Debug, Clone)]
pub struct Handle {
    /// The kind of handle (determines behavior).
    pub kind: HandleKind,
    /// Position in world coordinates.
    pub position: Point,
    /// Visual shape of the handle.
    pub shape: HandleShape,
}

/// The kind of handle - determines what manipulation it performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandleKind {
    // Corner handles (for rectangles, ellipses, images)
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    // Edge handles (for resizing)
    Top,
    Bottom,
    Left,
    Right,
    // Endpoint handles (for lines, arrows)
    Start,
    End,
    // Control point handles (for curves)
    Control(usize),
    // Rotation handle
    Rotate,
}

/// Visual shape of a handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HandleShape {
    /// Square handle (default for corners).
    #[default]
    Square,
    /// Circular handle (for line endpoints).
    Circle,
    /// Diamond handle (for control points).
    Diamond,
}

impl Handle {
    /// Create a new handle.
    pub fn new(kind: HandleKind, position: Point) -> Self {
        Self {
            kind,
            position,
            shape: HandleShape::default(),
        }
    }

    /// Set the handle shape.
    pub fn with_shape(mut self, shape: HandleShape) -> Self {
        self.shape = shape;
        self
    }
}
