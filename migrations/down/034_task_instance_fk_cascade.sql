-- Rollback 034_task_instance_fk_cascade.sql
--
-- Undoes: ON DELETE CASCADE / ON DELETE SET NULL FK replacements on five tables.
-- Restores: the original FK constraints with no ON DELETE action (i.e. RESTRICT,
--           the PostgreSQL default).
--
-- This is safe to run online using the same NOT VALID + VALIDATE pattern
-- that the forward migration used.

-- block_outputs.instance_id -> RESTRICT (original from 004)
ALTER TABLE block_outputs
    DROP CONSTRAINT IF EXISTS block_outputs_instance_id_fkey;
ALTER TABLE block_outputs
    ADD CONSTRAINT block_outputs_instance_id_fkey
    FOREIGN KEY (instance_id) REFERENCES task_instances(id)
    NOT VALID;
ALTER TABLE block_outputs
    VALIDATE CONSTRAINT block_outputs_instance_id_fkey;

-- execution_tree.instance_id -> RESTRICT (original from 003)
ALTER TABLE execution_tree
    DROP CONSTRAINT IF EXISTS execution_tree_instance_id_fkey;
ALTER TABLE execution_tree
    ADD CONSTRAINT execution_tree_instance_id_fkey
    FOREIGN KEY (instance_id) REFERENCES task_instances(id)
    NOT VALID;
ALTER TABLE execution_tree
    VALIDATE CONSTRAINT execution_tree_instance_id_fkey;

-- signal_inbox.instance_id -> RESTRICT (original from 007)
ALTER TABLE signal_inbox
    DROP CONSTRAINT IF EXISTS signal_inbox_instance_id_fkey;
ALTER TABLE signal_inbox
    ADD CONSTRAINT signal_inbox_instance_id_fkey
    FOREIGN KEY (instance_id) REFERENCES task_instances(id)
    NOT VALID;
ALTER TABLE signal_inbox
    VALIDATE CONSTRAINT signal_inbox_instance_id_fkey;

-- worker_tasks.instance_id -> RESTRICT (original from 012)
ALTER TABLE worker_tasks
    DROP CONSTRAINT IF EXISTS worker_tasks_instance_id_fkey;
ALTER TABLE worker_tasks
    ADD CONSTRAINT worker_tasks_instance_id_fkey
    FOREIGN KEY (instance_id) REFERENCES task_instances(id)
    NOT VALID;
ALTER TABLE worker_tasks
    VALIDATE CONSTRAINT worker_tasks_instance_id_fkey;

-- task_instances.parent_instance_id -> RESTRICT (original from 017)
-- Note: forward migration changed this from plain REFERENCES to ON DELETE SET NULL.
-- Rollback restores the original plain FK (no ON DELETE action = RESTRICT).
ALTER TABLE task_instances
    DROP CONSTRAINT IF EXISTS task_instances_parent_instance_id_fkey;
ALTER TABLE task_instances
    ADD CONSTRAINT task_instances_parent_instance_id_fkey
    FOREIGN KEY (parent_instance_id) REFERENCES task_instances(id)
    NOT VALID;
ALTER TABLE task_instances
    VALIDATE CONSTRAINT task_instances_parent_instance_id_fkey;
