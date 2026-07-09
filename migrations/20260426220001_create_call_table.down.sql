-- Down: drop telephony.calls table
DROP TABLE IF EXISTS telephony.calls CASCADE;
DROP FUNCTION IF EXISTS telephony.calls_audit_timestamp() CASCADE;
