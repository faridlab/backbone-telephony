-- Hand-authored (user-owned). Not regenerated.
--
-- Strip every company-fence artifact from the calls table (ADR-0029): the module is
-- tenant-agnostic; org scoping is installed by the COMPOSING service's tenancy decorator,
-- never by the module. Dropped here, per the table: the company-leading indexes, the
-- calls_company_isolation RLS policy, and the company_id column itself.
--
-- Ordering guard (the decorator must run FIRST on any database with data): the module
-- never moves tenancy data. The table is safe to strip when EITHER
--   a) it carries org_unit_id with no NULLs — the decorator backfilled it from company_id —
--      or b) it is empty (a fresh database: the earlier chain files created it empty).
-- Otherwise the strip RAISEs, naming the decorator step, rather than dropping a column
-- that still holds the only tenancy key. The file is re-runnable (every drop is IF EXISTS
-- and the tracker has no checksums), so a failed run retries cleanly after the decorator
-- lands.
--
-- RLS enable/force flags are deliberately NOT touched: the decorator owns those now.
-- The company fence on telephony.outbox_events is likewise NOT touched: the outbox is a
-- framework-owned, still-company-keyed surface (every staged record carries its owning
-- tenant), outside this module's strip.

DO $$
DECLARE
    t text;
    has_org boolean;
    org_nulls bigint;
    total bigint;
    offenders text := '';
BEGIN
    FOREACH t IN ARRAY ARRAY['calls']
    LOOP
        IF to_regclass(format('telephony.%I', t)) IS NULL THEN
            CONTINUE; -- chain not fully applied on this database; nothing to strip
        END IF;

        SELECT EXISTS (
                   SELECT 1 FROM information_schema.columns
                   WHERE table_schema = 'telephony' AND table_name = t AND column_name = 'org_unit_id'
               )
        INTO has_org;

        EXECUTE format('SELECT count(*) FROM telephony.%I', t) INTO total;

        IF has_org THEN
            EXECUTE format(
                'SELECT count(*) FROM telephony.%I WHERE org_unit_id IS NULL', t)
            INTO org_nulls;
        ELSE
            org_nulls := total; -- no org column: every row's only tenancy key is company_id
        END IF;

        IF has_org AND org_nulls = 0 THEN
            CONTINUE; -- decorator backfilled: safe
        END IF;
        IF total = 0 THEN
            CONTINUE; -- empty table (fresh database): safe
        END IF;
        offenders := offenders || format(' telephony.%s (%s rows, %s rows not covered by org_unit_id);', t, total, org_nulls);
    END LOOP;

    IF offenders <> '' THEN
        RAISE EXCEPTION 'refusing to strip company_id — these tables are not yet covered by the tenancy decorator:%. Apply the composing service''s tenancy decorator (it backfills org_unit_id from company_id) and re-run; it is the only step that moves tenancy data.', offenders;
    END IF;
END $$;

-- ── calls ───────────────────────────────────────────────────────────────────────
DROP INDEX IF EXISTS telephony.idx_calls_company_id_external_id;
DROP INDEX IF EXISTS telephony.idx_calls_company_id_party_id;
DROP POLICY IF EXISTS calls_company_isolation ON telephony.calls;
ALTER TABLE telephony.calls DROP COLUMN IF EXISTS company_id;

-- The (company_id, external_id) dedup unique was deployment POSTURE, not a domain
-- invariant, and is intentionally NOT restored in any form: dedup keys on external_id
-- within whatever fence the composer installs. The composing service's tenancy
-- descriptor owns the per-unit external_id unique (the pre-fence global form would
-- forbid two units of one tenant sharing a provider CDR id). No tenant-free unique
-- exists on this table to restore.
