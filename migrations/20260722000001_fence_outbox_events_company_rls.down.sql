DROP POLICY IF EXISTS outbox_events_company_isolation ON telephony.outbox_events;
ALTER TABLE telephony.outbox_events NO FORCE ROW LEVEL SECURITY;
ALTER TABLE telephony.outbox_events DISABLE ROW LEVEL SECURITY;
DROP INDEX IF EXISTS telephony.idx_telephony_outbox_company_id;
ALTER TABLE telephony.outbox_events DROP COLUMN IF EXISTS company_id;
