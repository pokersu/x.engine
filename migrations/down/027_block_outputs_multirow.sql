-- Rollback 027_block_outputs_multirow.sql
--
-- Undoes: DROP CONSTRAINT block_outputs_instance_id_block_id_key
--         + CREATE INDEX idx_block_outputs_instance_block_created.
-- Restores: the original UNIQUE constraint on (instance_id, block_id).
--
-- PREREQUISITE: if multiple rows exist for the same (instance_id, block_id),
-- you must deduplicate them first (keep only the latest per pair) before
-- running this rollback. Otherwise the ADD CONSTRAINT will fail.
--
-- Example pre-cleanup:
--   DELETE FROM block_outputs a USING block_outputs b
--   WHERE a.instance_id = b.instance_id
--     AND a.block_id    = b.block_id
--     AND a.created_at  < b.created_at;

DROP INDEX IF EXISTS idx_block_outputs_instance_block_created;

ALTER TABLE block_outputs
    ADD CONSTRAINT block_outputs_instance_id_block_id_key
    UNIQUE (instance_id, block_id);
