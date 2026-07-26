-- FTS5 UNINDEXED identity columns cannot support indexed DELETE predicates. Keep the
-- storage-neutral block identity in the ordinary state table and map it to FTS rowid.
-- The projection is derived data, so upgrading safely resets it for bounded rebuild.
DELETE FROM block_fts;
DELETE FROM block_fts_state;

ALTER TABLE block_fts_state ADD COLUMN fts_rowid INTEGER;

CREATE UNIQUE INDEX idx_block_fts_state_rowid
ON block_fts_state(fts_rowid)
WHERE fts_rowid IS NOT NULL;
