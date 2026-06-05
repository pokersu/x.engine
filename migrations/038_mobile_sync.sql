-- Mobile sync tables for device status reporting, approval proxy, and command inbox.

CREATE TABLE IF NOT EXISTS mobile_devices (
    device_id    TEXT PRIMARY KEY,
    tenant_id    TEXT NOT NULL DEFAULT '',
    push_token   TEXT,
    platform     TEXT NOT NULL DEFAULT 'ios',
    app_version  TEXT,
    active       BOOLEAN NOT NULL DEFAULT TRUE,
    last_sync_at TIMESTAMPTZ,
    registered_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS mobile_instance_status (
    device_id     TEXT NOT NULL,
    instance_id   TEXT NOT NULL,
    sequence_name TEXT,
    state         TEXT NOT NULL,
    current_step  TEXT,
    handler       TEXT,
    context_summary TEXT,
    steps         TEXT,
    updated_at    TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (device_id, instance_id)
);

CREATE TABLE IF NOT EXISTS mobile_approval_requests (
    id            TEXT PRIMARY KEY,
    device_id     TEXT NOT NULL,
    tenant_id     TEXT NOT NULL DEFAULT '',
    instance_id   TEXT NOT NULL,
    block_id      TEXT NOT NULL,
    sequence_name TEXT,
    prompt        TEXT,
    choices       TEXT,
    store_as      TEXT,
    timeout_secs  INTEGER,
    metadata      TEXT,
    state         TEXT NOT NULL DEFAULT 'pending',
    resolution    TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at   TIMESTAMPTZ,
    UNIQUE (device_id, instance_id, block_id)
);
CREATE INDEX IF NOT EXISTS idx_mobile_approvals_state ON mobile_approval_requests(state);
CREATE INDEX IF NOT EXISTS idx_mobile_approvals_device ON mobile_approval_requests(device_id);

CREATE TABLE IF NOT EXISTS mobile_commands (
    id           TEXT PRIMARY KEY,
    device_id    TEXT NOT NULL,
    command_type TEXT NOT NULL,
    payload      TEXT NOT NULL DEFAULT '{}',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    acked_at     TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_mobile_commands_device_pending
    ON mobile_commands(device_id) WHERE acked_at IS NULL;
