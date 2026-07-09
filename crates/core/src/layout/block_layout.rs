use crate::ids::BlockId;
use crate::version::LayoutVersionNumber;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockLayoutMeta {
    pub block_id: BlockId,
    pub estimated_height: f64,
    pub measured_height: Option<f64>,
    pub width_bucket: u16,
    pub layout_version: LayoutVersionNumber,
    pub dirty: bool,
}

impl BlockLayoutMeta {
    pub const DEFAULT_ESTIMATED_HEIGHT: f64 = 24.0;

    pub const fn new(block_id: BlockId, estimated_height: f64) -> Self {
        Self {
            block_id,
            estimated_height,
            measured_height: None,
            width_bucket: 0,
            layout_version: 0,
            dirty: true,
        }
    }

    pub fn effective_height(&self) -> f64 {
        match self.measured_height {
            Some(height) => height,
            None => self.estimated_height,
        }
    }

    pub fn update_height(&mut self, height: f64) {
        self.measured_height = Some(height);
        self.dirty = false;
        self.layout_version = self.layout_version.saturating_add(1);
    }
}
