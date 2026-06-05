-- Rollback 035_telemetry.sql
--
-- Undoes: CREATE TABLE telemetry_mobile_events + telemetry_mobile_errors
--         and their associated indexes.
--
-- WARNING: all telemetry data will be permanently deleted.

DROP INDEX IF EXISTS idx_telemetry_errors_received;
DROP INDEX IF EXISTS idx_telemetry_errors_tenant;
DROP INDEX IF EXISTS idx_telemetry_errors_device;

DROP TABLE IF EXISTS telemetry_mobile_errors;

DROP INDEX IF EXISTS idx_telemetry_events_received;
DROP INDEX IF EXISTS idx_telemetry_events_tenant;
DROP INDEX IF EXISTS idx_telemetry_events_device;
DROP INDEX IF EXISTS idx_telemetry_events_type;

DROP TABLE IF EXISTS telemetry_mobile_events;
