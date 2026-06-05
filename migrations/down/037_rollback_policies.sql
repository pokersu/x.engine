-- Rollback 037_rollback_policies.sql
--
-- Undoes: CREATE TABLE rollback_policies + rollback_history and their indexes.
--
-- WARNING: all rollback policy configuration and rollback history records
-- will be permanently deleted.

DROP INDEX IF EXISTS idx_rollback_history_triggered;
DROP INDEX IF EXISTS idx_rollback_history_tenant;

DROP TABLE IF EXISTS rollback_history;

DROP INDEX IF EXISTS idx_rollback_policies_enabled;
DROP INDEX IF EXISTS idx_rollback_policies_tenant;

DROP TABLE IF EXISTS rollback_policies;
