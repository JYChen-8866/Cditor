use crate::ids::{DocumentId, SurfaceId};

pub type StructureVersion = u64;
pub type LayoutVersionNumber = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SnapshotIdentity {
    pub document_id: DocumentId,
    pub structure_version: StructureVersion,
    pub surface_id: Option<SurfaceId>,
    pub content_version: Option<u64>,
    pub layout_version: LayoutVersionNumber,
    pub font_epoch: u64,
    pub scale_epoch: u64,
    pub viewport_epoch: u64,
    pub generation: u64,
}

impl SnapshotIdentity {
    pub const fn document(
        document_id: DocumentId,
        structure_version: StructureVersion,
        layout_version: LayoutVersionNumber,
        font_epoch: u64,
        scale_epoch: u64,
        viewport_epoch: u64,
        generation: u64,
    ) -> Self {
        Self {
            document_id,
            structure_version,
            surface_id: None,
            content_version: None,
            layout_version,
            font_epoch,
            scale_epoch,
            viewport_epoch,
            generation,
        }
    }

    pub const fn with_surface(
        mut self,
        surface_id: SurfaceId,
        content_version: u64,
        layout_version: LayoutVersionNumber,
    ) -> Self {
        self.surface_id = Some(surface_id);
        self.content_version = Some(content_version);
        self.layout_version = layout_version;
        self
    }

    pub fn validate_against(self, current: Self) -> Result<(), SnapshotIdentityMismatch> {
        if self.document_id != current.document_id {
            return Err(SnapshotIdentityMismatch::Document {
                expected: current.document_id,
                actual: self.document_id,
            });
        }
        if self.structure_version != current.structure_version {
            return Err(SnapshotIdentityMismatch::Structure {
                expected: current.structure_version,
                actual: self.structure_version,
            });
        }
        if self.surface_id != current.surface_id {
            return Err(SnapshotIdentityMismatch::Surface {
                expected: current.surface_id,
                actual: self.surface_id,
            });
        }
        if self.content_version != current.content_version {
            return Err(SnapshotIdentityMismatch::Content {
                expected: current.content_version,
                actual: self.content_version,
            });
        }
        if self.layout_version != current.layout_version {
            return Err(SnapshotIdentityMismatch::Layout {
                expected: current.layout_version,
                actual: self.layout_version,
            });
        }
        if self.font_epoch != current.font_epoch {
            return Err(SnapshotIdentityMismatch::Font {
                expected: current.font_epoch,
                actual: self.font_epoch,
            });
        }
        if self.scale_epoch != current.scale_epoch {
            return Err(SnapshotIdentityMismatch::Scale {
                expected: current.scale_epoch,
                actual: self.scale_epoch,
            });
        }
        if self.viewport_epoch != current.viewport_epoch {
            return Err(SnapshotIdentityMismatch::Viewport {
                expected: current.viewport_epoch,
                actual: self.viewport_epoch,
            });
        }
        if self.generation != current.generation {
            return Err(SnapshotIdentityMismatch::Generation {
                expected: current.generation,
                actual: self.generation,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotIdentityMismatch {
    Document {
        expected: DocumentId,
        actual: DocumentId,
    },
    Structure {
        expected: StructureVersion,
        actual: StructureVersion,
    },
    Surface {
        expected: Option<SurfaceId>,
        actual: Option<SurfaceId>,
    },
    Content {
        expected: Option<u64>,
        actual: Option<u64>,
    },
    Layout {
        expected: LayoutVersionNumber,
        actual: LayoutVersionNumber,
    },
    Font {
        expected: u64,
        actual: u64,
    },
    Scale {
        expected: u64,
        actual: u64,
    },
    Viewport {
        expected: u64,
        actual: u64,
    },
    Generation {
        expected: u64,
        actual: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> SnapshotIdentity {
        SnapshotIdentity::document(7, 3, 0, 5, 6, 8, 9).with_surface(SurfaceId::Block(11), 13, 17)
    }

    #[test]
    fn matching_snapshot_identity_validates() {
        let identity = identity();
        assert_eq!(identity.validate_against(identity), Ok(()));
    }

    #[test]
    fn snapshot_identity_reports_the_exact_stale_dimension() {
        let current = identity();
        let mut stale = current;
        stale.scale_epoch -= 1;

        assert_eq!(
            stale.validate_against(current),
            Err(SnapshotIdentityMismatch::Scale {
                expected: 6,
                actual: 5,
            })
        );
    }

    #[test]
    fn document_identity_and_surface_identity_do_not_compare_equal() {
        let current = identity();
        let document = SnapshotIdentity::document(7, 3, 17, 5, 6, 8, 9);

        assert!(matches!(
            document.validate_against(current),
            Err(SnapshotIdentityMismatch::Surface { .. })
        ));
    }
}
