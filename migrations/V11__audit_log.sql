-- Audit log: append-only record of all state mutations.
-- Phase 1 of temporal database support (Date & Darwen / Datomic inspired).
--
-- Transaction time only — records when facts were recorded, not when they were true.
-- Agents operate in "now"; time travel is purely retrospective and query-time.

CREATE TABLE IF NOT EXISTS audit_log (
    id          BIGSERIAL       PRIMARY KEY,
    ts          TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    user_id     TEXT            NOT NULL,
    entity_type TEXT            NOT NULL,   -- 'conversation', 'message', 'setting', etc.
    entity_id   TEXT            NOT NULL,   -- UUID or composite key serialized
    action      TEXT            NOT NULL,   -- 'create', 'update', 'delete'
    field       TEXT,                       -- specific field changed (NULL = whole entity)
    old_value   JSONB,                      -- previous value (NULL for creates)
    new_value   JSONB,                      -- new value (NULL for deletes)
    metadata    JSONB                       -- optional context (channel, job_id, etc.)
);

CREATE INDEX IF NOT EXISTS idx_audit_log_ts ON audit_log (ts);
CREATE INDEX IF NOT EXISTS idx_audit_log_entity ON audit_log (entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_user ON audit_log (user_id, ts);
