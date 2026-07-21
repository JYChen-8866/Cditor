use std::collections::{HashMap, HashSet};

use crate::document::BlockIndexRecord;
use crate::ids::BlockId;

use super::{
    BlockPayload, BlockPayloadRecord, ColumnsGroupPayload, RichBlockKind, rich_block_kind_from_tag,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnsStructureError {
    MissingGroupPayload(BlockId),
    InvalidGroupPayload(BlockId),
    ColumnWithoutGroupParent(BlockId),
    GroupContainsNonColumn {
        group_id: BlockId,
        block_id: BlockId,
    },
    PayloadColumnOrderMismatch(BlockId),
}

pub fn validate_columns_structure(
    records: &[BlockIndexRecord],
    payloads: &[BlockPayloadRecord],
) -> Result<(), ColumnsStructureError> {
    let records_by_id = records
        .iter()
        .map(|record| (record.id, record))
        .collect::<HashMap<_, _>>();
    let payloads_by_id = payloads
        .iter()
        .map(|payload| (payload.block_id, payload))
        .collect::<HashMap<_, _>>();

    for record in records {
        match rich_block_kind_from_tag(record.kind_tag) {
            RichBlockKind::ColumnsGroup => {
                let payload = payloads_by_id
                    .get(&record.id)
                    .ok_or(ColumnsStructureError::MissingGroupPayload(record.id))?;
                let BlockPayload::Columns(columns) = &payload.payload else {
                    return Err(ColumnsStructureError::InvalidGroupPayload(record.id));
                };
                columns
                    .layout_model(record.id)
                    .map_err(|_| ColumnsStructureError::InvalidGroupPayload(record.id))?;
                let direct_children = records
                    .iter()
                    .filter(|candidate| candidate.parent_id == Some(record.id))
                    .collect::<Vec<_>>();
                for child in &direct_children {
                    if !matches!(
                        rich_block_kind_from_tag(child.kind_tag),
                        RichBlockKind::Column
                    ) {
                        return Err(ColumnsStructureError::GroupContainsNonColumn {
                            group_id: record.id,
                            block_id: child.id,
                        });
                    }
                }
                let actual = direct_children
                    .iter()
                    .map(|child| child.id)
                    .collect::<Vec<_>>();
                let expected = columns
                    .columns
                    .iter()
                    .map(|column| column.block_id)
                    .collect::<Vec<_>>();
                if actual != expected {
                    return Err(ColumnsStructureError::PayloadColumnOrderMismatch(record.id));
                }
            }
            RichBlockKind::Column => {
                let Some(parent) = record.parent_id.and_then(|id| records_by_id.get(&id)) else {
                    return Err(ColumnsStructureError::ColumnWithoutGroupParent(record.id));
                };
                if !matches!(
                    rich_block_kind_from_tag(parent.kind_tag),
                    RichBlockKind::ColumnsGroup
                ) {
                    return Err(ColumnsStructureError::ColumnWithoutGroupParent(record.id));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn columns_payload_references(payload: &ColumnsGroupPayload) -> HashSet<BlockId> {
    payload
        .columns
        .iter()
        .map(|column| column.block_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::BlockIndexRecord;
    use crate::layout::ColumnSpec;
    use crate::rich_text::kind_tag_for_rich_block_kind;

    fn record(
        id: BlockId,
        parent_id: Option<BlockId>,
        depth: u16,
        kind: RichBlockKind,
    ) -> BlockIndexRecord {
        BlockIndexRecord::new(id, parent_id, depth, kind_tag_for_rich_block_kind(&kind), 0)
    }

    fn valid_fixture() -> (Vec<BlockIndexRecord>, Vec<BlockPayloadRecord>) {
        let records = vec![
            record(1, None, 0, RichBlockKind::ColumnsGroup),
            record(2, Some(1), 1, RichBlockKind::Column),
            record(3, Some(2), 2, RichBlockKind::Paragraph),
            record(4, Some(1), 1, RichBlockKind::Column),
            record(5, Some(4), 2, RichBlockKind::Image),
        ];
        let payloads = vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::ColumnsGroup,
            payload: BlockPayload::Columns(ColumnsGroupPayload {
                columns: vec![
                    ColumnSpec {
                        block_id: 2,
                        weight: 500_000,
                    },
                    ColumnSpec {
                        block_id: 4,
                        weight: 500_000,
                    },
                ],
                gap_milli: 24_000,
            }),
        }];
        (records, payloads)
    }

    #[test]
    fn valid_group_matches_payload_order_and_allows_column_content() {
        let (records, payloads) = valid_fixture();
        assert_eq!(validate_columns_structure(&records, &payloads), Ok(()));
    }

    #[test]
    fn orphan_column_non_column_direct_child_and_order_mismatch_are_rejected() {
        let records = vec![record(2, None, 0, RichBlockKind::Column)];
        assert_eq!(
            validate_columns_structure(&records, &[]),
            Err(ColumnsStructureError::ColumnWithoutGroupParent(2))
        );

        let (mut records, payloads) = valid_fixture();
        records[1].kind_tag = kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph);
        assert_eq!(
            validate_columns_structure(&records, &payloads),
            Err(ColumnsStructureError::GroupContainsNonColumn {
                group_id: 1,
                block_id: 2,
            })
        );

        let (records, mut payloads) = valid_fixture();
        let BlockPayload::Columns(columns) = &mut payloads[0].payload else {
            unreachable!()
        };
        columns.columns.swap(0, 1);
        assert_eq!(
            validate_columns_structure(&records, &payloads),
            Err(ColumnsStructureError::PayloadColumnOrderMismatch(1))
        );
    }
}
