#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheVersionPolicy {
    pub expected_schema_version: u32,
    pub expected_cache_version: u32,
}

impl Default for CacheVersionPolicy {
    fn default() -> Self {
        Self {
            expected_schema_version: 1,
            expected_cache_version: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheManifest {
    pub schema_version: u32,
    pub cache_version: u32,
    pub block_layout_present: bool,
    pub page_layout_present: bool,
    pub index_snapshot_present: bool,
    pub index_snapshot_valid: bool,
    pub fts_present: bool,
    pub fts_valid: bool,
    pub thumbnail_cache_present: bool,
}

impl Default for CacheManifest {
    fn default() -> Self {
        Self {
            schema_version: 1,
            cache_version: 1,
            block_layout_present: true,
            page_layout_present: true,
            index_snapshot_present: true,
            index_snapshot_valid: true,
            fts_present: true,
            fts_valid: true,
            thumbnail_cache_present: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheOpenMode {
    ReadWrite,
    ReadOnlyNeedsMigration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheRecoveryAction {
    UseDocumentTruth,
    RunSchemaMigration,
    OpenReadOnlyAndPromptRepair,
    DropLayoutCache,
    DropIndexSnapshot,
    RebuildIndexSnapshotFromBlocks,
    ScheduleFtsRebuild,
    MarkSearchIndexing,
    GenerateThumbnailOnDemand,
    LoadHistoricalHeightHints,
    FirstPaintAllowed,
    ScheduleBackgroundConvergence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupWorkKind {
    LoadDocumentMeta,
    LoadDocumentStructure,
    LoadOrRebuildDocumentIndex,
    LoadFirstWindowPayload,
    LoadHistoricalHeightCache,
    FirstPaint,
    BackgroundCacheRepair,
    BackgroundFtsRebuild,
    OnDemandThumbnailGeneration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupWorkItem {
    pub kind: StartupWorkKind,
    pub blocks_first_paint: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheRecoveryPlan {
    pub open_mode: CacheOpenMode,
    pub actions: Vec<CacheRecoveryAction>,
    pub startup_work: Vec<StartupWorkItem>,
    pub layout_cache_available_for_first_paint: bool,
    pub index_snapshot_available: bool,
    pub search_available: bool,
    pub thumbnails_available: bool,
    pub full_text_shaping_on_startup: bool,
    pub full_page_measure_on_startup: bool,
}

impl CacheRecoveryPlan {
    pub fn allows_first_paint(&self) -> bool {
        self.startup_work
            .iter()
            .any(|item| item.kind == StartupWorkKind::FirstPaint && !item.blocks_first_paint)
    }

    pub fn has_action(&self, action: CacheRecoveryAction) -> bool {
        self.actions.contains(&action)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheRecoveryPlanner {
    pub policy: CacheVersionPolicy,
}

impl CacheRecoveryPlanner {
    pub const fn new(policy: CacheVersionPolicy) -> Self {
        Self { policy }
    }

    pub fn plan(&self, manifest: CacheManifest, migration_can_run: bool) -> CacheRecoveryPlan {
        let mut actions = vec![CacheRecoveryAction::UseDocumentTruth];
        let mut open_mode = CacheOpenMode::ReadWrite;

        if manifest.schema_version != self.policy.expected_schema_version {
            if migration_can_run {
                actions.push(CacheRecoveryAction::RunSchemaMigration);
            } else {
                actions.push(CacheRecoveryAction::OpenReadOnlyAndPromptRepair);
                open_mode = CacheOpenMode::ReadOnlyNeedsMigration;
            }
        }

        let cache_version_matches = manifest.cache_version == self.policy.expected_cache_version;
        let layout_cache_available =
            cache_version_matches && manifest.block_layout_present && manifest.page_layout_present;
        if !cache_version_matches || !manifest.block_layout_present || !manifest.page_layout_present
        {
            actions.push(CacheRecoveryAction::DropLayoutCache);
        } else {
            actions.push(CacheRecoveryAction::LoadHistoricalHeightHints);
        }

        let index_snapshot_available = cache_version_matches
            && manifest.index_snapshot_present
            && manifest.index_snapshot_valid;
        if !index_snapshot_available {
            actions.push(CacheRecoveryAction::DropIndexSnapshot);
            actions.push(CacheRecoveryAction::RebuildIndexSnapshotFromBlocks);
        }

        let search_available = manifest.fts_present && manifest.fts_valid && cache_version_matches;
        if !search_available {
            actions.push(CacheRecoveryAction::ScheduleFtsRebuild);
            actions.push(CacheRecoveryAction::MarkSearchIndexing);
        }

        let thumbnails_available = manifest.thumbnail_cache_present && cache_version_matches;
        if !thumbnails_available {
            actions.push(CacheRecoveryAction::GenerateThumbnailOnDemand);
        }

        actions.push(CacheRecoveryAction::FirstPaintAllowed);
        actions.push(CacheRecoveryAction::ScheduleBackgroundConvergence);

        CacheRecoveryPlan {
            open_mode,
            actions,
            startup_work: startup_sequence(
                layout_cache_available,
                !index_snapshot_available,
                !search_available,
                !thumbnails_available,
            ),
            layout_cache_available_for_first_paint: layout_cache_available,
            index_snapshot_available,
            search_available,
            thumbnails_available,
            full_text_shaping_on_startup: false,
            full_page_measure_on_startup: false,
        }
    }
}

impl Default for CacheRecoveryPlanner {
    fn default() -> Self {
        Self::new(CacheVersionPolicy::default())
    }
}

fn startup_sequence(
    load_height_cache: bool,
    rebuild_index_snapshot: bool,
    rebuild_fts: bool,
    generate_thumbnail_on_demand: bool,
) -> Vec<StartupWorkItem> {
    let mut items = vec![
        StartupWorkItem {
            kind: StartupWorkKind::LoadDocumentMeta,
            blocks_first_paint: true,
        },
        StartupWorkItem {
            kind: StartupWorkKind::LoadDocumentStructure,
            blocks_first_paint: true,
        },
        StartupWorkItem {
            kind: StartupWorkKind::LoadOrRebuildDocumentIndex,
            blocks_first_paint: true,
        },
        StartupWorkItem {
            kind: StartupWorkKind::LoadFirstWindowPayload,
            blocks_first_paint: true,
        },
    ];

    if load_height_cache {
        items.push(StartupWorkItem {
            kind: StartupWorkKind::LoadHistoricalHeightCache,
            blocks_first_paint: false,
        });
    }

    items.push(StartupWorkItem {
        kind: StartupWorkKind::FirstPaint,
        blocks_first_paint: false,
    });

    if rebuild_index_snapshot {
        items.push(StartupWorkItem {
            kind: StartupWorkKind::BackgroundCacheRepair,
            blocks_first_paint: false,
        });
    }
    if rebuild_fts {
        items.push(StartupWorkItem {
            kind: StartupWorkKind::BackgroundFtsRebuild,
            blocks_first_paint: false,
        });
    }
    if generate_thumbnail_on_demand {
        items.push(StartupWorkItem {
            kind: StartupWorkKind::OnDemandThumbnailGeneration,
            blocks_first_paint: false,
        });
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleting_block_layout_does_not_block_document_open() {
        let planner = CacheRecoveryPlanner::default();
        let manifest = CacheManifest {
            block_layout_present: false,
            ..CacheManifest::default()
        };

        let plan = planner.plan(manifest, true);

        assert!(plan.allows_first_paint());
        assert!(plan.has_action(CacheRecoveryAction::DropLayoutCache));
        assert!(!plan.layout_cache_available_for_first_paint);
        assert!(!plan.full_text_shaping_on_startup);
        assert!(!plan.full_page_measure_on_startup);
    }

    #[test]
    fn deleting_page_layout_does_not_block_document_open() {
        let planner = CacheRecoveryPlanner::default();
        let manifest = CacheManifest {
            page_layout_present: false,
            ..CacheManifest::default()
        };

        let plan = planner.plan(manifest, true);

        assert!(plan.allows_first_paint());
        assert!(plan.has_action(CacheRecoveryAction::DropLayoutCache));
        assert!(!plan.layout_cache_available_for_first_paint);
    }

    #[test]
    fn damaged_fts_opens_document_and_schedules_background_rebuild() {
        let planner = CacheRecoveryPlanner::default();
        let manifest = CacheManifest {
            fts_valid: false,
            ..CacheManifest::default()
        };

        let plan = planner.plan(manifest, true);

        assert!(plan.allows_first_paint());
        assert!(!plan.search_available);
        assert!(plan.has_action(CacheRecoveryAction::ScheduleFtsRebuild));
        assert!(plan.has_action(CacheRecoveryAction::MarkSearchIndexing));
        assert!(
            plan.startup_work
                .iter()
                .any(|item| item.kind == StartupWorkKind::BackgroundFtsRebuild
                    && !item.blocks_first_paint)
        );
    }

    #[test]
    fn damaged_index_snapshot_rebuilds_from_blocks_without_losing_document_truth() {
        let planner = CacheRecoveryPlanner::default();
        let manifest = CacheManifest {
            index_snapshot_valid: false,
            ..CacheManifest::default()
        };

        let plan = planner.plan(manifest, true);

        assert!(plan.has_action(CacheRecoveryAction::UseDocumentTruth));
        assert!(plan.has_action(CacheRecoveryAction::DropIndexSnapshot));
        assert!(plan.has_action(CacheRecoveryAction::RebuildIndexSnapshotFromBlocks));
        assert!(!plan.index_snapshot_available);
        assert!(plan.allows_first_paint());
    }

    #[test]
    fn cache_version_mismatch_discards_rebuildable_caches_not_body_data() {
        let planner = CacheRecoveryPlanner::default();
        let manifest = CacheManifest {
            cache_version: 0,
            ..CacheManifest::default()
        };

        let plan = planner.plan(manifest, true);

        assert!(plan.has_action(CacheRecoveryAction::UseDocumentTruth));
        assert!(plan.has_action(CacheRecoveryAction::DropLayoutCache));
        assert!(plan.has_action(CacheRecoveryAction::DropIndexSnapshot));
        assert!(plan.has_action(CacheRecoveryAction::ScheduleFtsRebuild));
        assert!(plan.allows_first_paint());
    }

    #[test]
    fn schema_version_mismatch_runs_migration_or_readonly_repair() {
        let planner = CacheRecoveryPlanner::default();
        let manifest = CacheManifest {
            schema_version: 0,
            ..CacheManifest::default()
        };

        let migrated = planner.plan(manifest, true);
        assert_eq!(migrated.open_mode, CacheOpenMode::ReadWrite);
        assert!(migrated.has_action(CacheRecoveryAction::RunSchemaMigration));

        let readonly = planner.plan(manifest, false);
        assert_eq!(readonly.open_mode, CacheOpenMode::ReadOnlyNeedsMigration);
        assert!(readonly.has_action(CacheRecoveryAction::OpenReadOnlyAndPromptRepair));
    }

    #[test]
    fn missing_thumbnail_cache_is_on_demand_not_startup_blocking() {
        let planner = CacheRecoveryPlanner::default();
        let manifest = CacheManifest {
            thumbnail_cache_present: false,
            ..CacheManifest::default()
        };

        let plan = planner.plan(manifest, true);

        assert!(plan.has_action(CacheRecoveryAction::GenerateThumbnailOnDemand));
        assert!(plan.allows_first_paint());
        assert!(plan.startup_work.iter().any(|item| item.kind
            == StartupWorkKind::OnDemandThumbnailGeneration
            && !item.blocks_first_paint));
    }
}
