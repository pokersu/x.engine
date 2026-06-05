-- Add ON DELETE CASCADE to every FK referencing task_instances(id) that
-- doesn't already have one. Without these, retention pruning in the
-- cloud control plane can't remove old task_instances rows: the FK
-- constraint blocks the delete.
--
-- Already cascading (no-op here):
--   audit_log, checkpoints, externalized_state (m024),
--   instance_kv_state, emit_event_dedupe.child_instance_id (m031)
--
-- Lock discipline mirrors m024:
--   ALTER TABLE ... DROP CONSTRAINT IF EXISTS — brief ACCESS EXCLUSIVE.
--   ADD CONSTRAINT ... NOT VALID — brief ACCESS EXCLUSIVE, no scan.
--   VALIDATE CONSTRAINT — SHARE UPDATE EXCLUSIVE; concurrent reads/writes
--   continue while the table scan runs.
--
-- block_outputs.instance_id
ALTER TABLE block_outputs
    DROP CONSTRAINT IF EXISTS block_outputs_instance_id_fkey;

ALTER TABLE block_outputs
    ADD CONSTRAINT block_outputs_instance_id_fkey
    FOREIGN KEY (instance_id) REFERENCES task_instances(id) ON DELETE CASCADE
    NOT VALID;

ALTER TABLE block_outputs
    VALIDATE CONSTRAINT block_outputs_instance_id_fkey;

-- execution_tree.instance_id
ALTER TABLE execution_tree
    DROP CONSTRAINT IF EXISTS execution_tree_instance_id_fkey;

ALTER TABLE execution_tree
    ADD CONSTRAINT execution_tree_instance_id_fkey
    FOREIGN KEY (instance_id) REFERENCES task_instances(id) ON DELETE CASCADE
    NOT VALID;

ALTER TABLE execution_tree
    VALIDATE CONSTRAINT execution_tree_instance_id_fkey;

-- signal_inbox.instance_id
ALTER TABLE signal_inbox
    DROP CONSTRAINT IF EXISTS signal_inbox_instance_id_fkey;

ALTER TABLE signal_inbox
    ADD CONSTRAINT signal_inbox_instance_id_fkey
    FOREIGN KEY (instance_id) REFERENCES task_instances(id) ON DELETE CASCADE
    NOT VALID;

ALTER TABLE signal_inbox
    VALIDATE CONSTRAINT signal_inbox_instance_id_fkey;

-- worker_tasks.instance_id
ALTER TABLE worker_tasks
    DROP CONSTRAINT IF EXISTS worker_tasks_instance_id_fkey;

ALTER TABLE worker_tasks
    ADD CONSTRAINT worker_tasks_instance_id_fkey
    FOREIGN KEY (instance_id) REFERENCES task_instances(id) ON DELETE CASCADE
    NOT VALID;

ALTER TABLE worker_tasks
    VALIDATE CONSTRAINT worker_tasks_instance_id_fkey;

-- task_instances.parent_instance_id (self-reference). m017 added the
-- column without ON DELETE behavior. Use SET NULL rather than CASCADE
-- so pruning a parent doesn't recursively delete a still-active child
-- — the child becomes a root, which is the safer outcome for retention.
ALTER TABLE task_instances
    DROP CONSTRAINT IF EXISTS task_instances_parent_instance_id_fkey;

ALTER TABLE task_instances
    ADD CONSTRAINT task_instances_parent_instance_id_fkey
    FOREIGN KEY (parent_instance_id) REFERENCES task_instances(id) ON DELETE SET NULL
    NOT VALID;

ALTER TABLE task_instances
    VALIDATE CONSTRAINT task_instances_parent_instance_id_fkey;
