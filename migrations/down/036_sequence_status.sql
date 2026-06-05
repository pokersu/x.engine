-- Rollback 036_sequence_status.sql
--
-- Undoes: ALTER TABLE sequences ADD COLUMN status TEXT NOT NULL DEFAULT 'production'
--         + CREATE INDEX idx_sequences_status.
--
-- WARNING: existing status values will be lost. All sequences will lose their
-- status column and implicitly revert to the "always production" behavior.

DROP INDEX IF EXISTS idx_sequences_status;

ALTER TABLE sequences DROP COLUMN IF EXISTS status;
