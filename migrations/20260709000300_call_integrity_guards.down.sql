-- Rollback the talk-time non-negative guard. The Rust write path still clamps duration >= 0; this
-- only removes the storage-level CHECK backstop.
ALTER TABLE telephony.calls DROP CONSTRAINT IF EXISTS calls_duration_non_negative;
