-- Recovery delivery is a separate process with its own role. The outbox gains
-- a lease so a crashed worker retries instead of losing or double-claiming work.
ALTER TABLE recovery_outbox
    ADD COLUMN delivery_attempts INT NOT NULL DEFAULT 0 CHECK (delivery_attempts >= 0),
    ADD COLUMN lease_until TIMESTAMPTZ;

CREATE INDEX recovery_outbox_pending
    ON recovery_outbox (created_at)
    WHERE delivered_at IS NULL;
