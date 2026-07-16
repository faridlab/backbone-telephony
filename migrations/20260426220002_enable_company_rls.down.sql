-- Down: remove the company RLS fence for telephony module

-- Reverse the company RLS fence for telephony.calls
DROP POLICY IF EXISTS calls_company_isolation ON telephony.calls;
ALTER TABLE telephony.calls NO FORCE ROW LEVEL SECURITY;
ALTER TABLE telephony.calls DISABLE ROW LEVEL SECURITY;

