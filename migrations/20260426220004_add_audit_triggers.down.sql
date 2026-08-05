-- Rollback the audit timestamp triggers + their function. Raw provider timestamps on the rows
-- are unaffected; only the automatic metadata->created_at/updated_at maintenance is removed.
DROP TRIGGER IF EXISTS calls_update_audit ON telephony.calls;
DROP TRIGGER IF EXISTS calls_insert_audit ON telephony.calls;
DROP FUNCTION IF EXISTS telephony.calls_audit_timestamp();
