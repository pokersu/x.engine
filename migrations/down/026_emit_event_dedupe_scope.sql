-- Rollback 026_emit_event_dedupe_scope.sql
--
-- Undoes: DROP TABLE emit_event_dedupe (v2 with scope_kind/scope_value)
--         + CREATE TABLE emit_event_dedupe (v2).
-- Restores: the original v1 table from migration 025 with
--           PRIMARY KEY (parent_instance_id, dedupe_key).
--
-- WARNING: any rows in the v2 table that used scope_kind = 'tenant' will be
-- lost because the v1 schema has no representation for tenant-scoped dedupes.
-- Rows with scope_kind = 'parent' can be migrated manually before running this
-- by extracting scope_value::uuid -> parent_instance_id.

DROP TABLE IF EXISTS emit_event_dedupe;

CREATE TABLE IF NOT EXISTS emit_event_dedupe (
    parent_instance_id  UUID         NOT NULL,
    dedupe_key          TEXT         NOT NULL,
    child_instance_id   UUID         NOT NULL,
    created_at          TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (parent_instance_id, dedupe_key)
);

CREATE INDEX IF NOT EXISTS emit_event_dedupe_created_at_idx
    ON emit_event_dedupe (created_at);
