use std::ops::Range;

use cditor_core::ids::SurfaceId;

use super::{RichTextDelta, TextSurface, TextSurfaceEditResult, TextSurfaceSnapshot};
use crate::DocumentRuntime;

pub struct RuntimeTextSurface<'a> {
    runtime: &'a mut DocumentRuntime,
    surface_id: SurfaceId,
}

impl<'a> RuntimeTextSurface<'a> {
    pub(super) fn new(runtime: &'a mut DocumentRuntime, surface_id: SurfaceId) -> Self {
        Self {
            runtime,
            surface_id,
        }
    }
}

impl TextSurface for RuntimeTextSurface<'_> {
    fn surface_id(&self) -> SurfaceId {
        self.surface_id
    }

    fn snapshot(&self) -> Result<TextSurfaceSnapshot, String> {
        self.runtime
            .text_surface_snapshot(self.surface_id)
            .ok_or_else(|| format!("missing text surface {:?}", self.surface_id))
    }

    fn replace(
        &mut self,
        range: Range<usize>,
        delta: RichTextDelta,
    ) -> Result<TextSurfaceEditResult, String> {
        self.runtime
            .replace_text_surface_range(self.surface_id, range, delta)
    }
}
