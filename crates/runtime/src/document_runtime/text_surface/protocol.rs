use std::ops::Range;

use cditor_core::ids::SurfaceId;
use cditor_core::rich_text::{InlineMark, InlineSpan};

use super::{TextSurfaceCapabilities, TextSurfaceSnapshot, TextSurfaceSnapshotIdentity};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichTextDelta {
    pub spans: Vec<InlineSpan>,
}

impl RichTextDelta {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            spans: vec![InlineSpan::plain(text)],
        }
    }

    pub fn plain_text(&self) -> String {
        cditor_core::rich_text::plain_text_from_spans(&self.spans)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSurfaceEditResult {
    pub identity_before: TextSurfaceSnapshotIdentity,
    pub identity_after: TextSurfaceSnapshotIdentity,
    pub replaced_range: Range<usize>,
    pub inserted_range: Range<usize>,
}

pub trait TextSurface {
    fn surface_id(&self) -> SurfaceId;
    fn snapshot(&self) -> Result<TextSurfaceSnapshot, String>;
    fn replace(
        &mut self,
        range: Range<usize>,
        delta: RichTextDelta,
    ) -> Result<TextSurfaceEditResult, String>;

    fn marks_at(&self, offset: usize) -> Result<Vec<InlineMark>, String> {
        Ok(self.snapshot()?.marks_at(offset).to_vec())
    }

    fn capabilities(&self) -> Result<TextSurfaceCapabilities, String> {
        Ok(self.snapshot()?.capabilities)
    }
}
