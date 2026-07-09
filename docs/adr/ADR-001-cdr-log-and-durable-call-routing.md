# ADR-001 — The CDR log, idempotent recording, and durable call routing

Status: accepted · 2026-07-09 · Thin-channels pillar (Tier 5; posts no GL)

## Context
A call-center/high-touch-sales SMB wants calls on the customer timeline and — critically — missed inbound
calls surfaced for callback. Telephony is a communication-adjacent CDR log: it records the call a provider
hands it and turns it into a domain event CRM/support attach to. Promoted against a named call-center
requirement (tier5-deferred §5).

## Decision
1. **Recording is idempotent on (company, external_id) — a DB invariant.** Provider CDR webhooks are
   at-least-once; the unique index + `INSERT … ON CONFLICT DO NOTHING` guarantees one call per provider id.
   A manual log (no `external_id`) never collides (NULLs distinct).
2. **Routing is an EVENT, not a driven call.** `record_call` publishes `CallLogged` (attach an activity) or
   `MissedCall` (raise a callback); CRM/support subscribe. Zero Cargo edge — proven by TSEAM-1 raising a
   REAL backbone-crm callback lead from the event alone.
3. **The routing event is DURABLE — staged in the transactional outbox in the same tx as the call insert.**
   A crash between commit and the in-proc publish can't drop it; the relay delivers at-least-once, consumers
   dedup via the inbox.
4. **CDR content is derived-and-clamped, not trusted.** Talk-time is clamped ≥ 0 (a skewed `ended<answered`
   CDR stores zero, never negative), and `MissedCall` is routed only when genuinely unanswered; a DB CHECK
   backstops the derived duration against any writer. Raw provider timestamps are preserved for audit
   (maturity council 2026-07-09).
5. **The provider is a port.** The PBX/SIP/CDR webhook is wired by a composing service into `record_call`;
   no provider SDK leaks in.
6. **Posts no GL.** Calls link to a party and (once a consumer decides) to a lead/issue via a polymorphic
   logical reference.

## Consequences
- Turn telephony off and no call routes; it is the one place call traffic becomes domain events.
- Proven against REAL backbone-crm and durable across a lost publish; survives regen (§5).

## Parking lot (each with a gate)
- **Events dropped the handling agent** — FIXED (completeness council 2026-07-09): `CallLogged`/`MissedCall`
  omitted `agent_id` (persisted on the row), so a CRM consumer couldn't attribute the activity or run
  agent-productivity reporting without re-querying telephony's private table; added `agent_id` to both
  events (TGC-4, proven-by-revert).
- **Negative talk-time from a skewed CDR** — FIXED (maturity council 2026-07-09): `talk_time` clamps ≥ 0
  with an ordering guard, `MissedCall` derives from the facts, + a `duration_seconds >= 0` DB CHECK (TIP-4,
  proven-by-revert).
- **`status` not normalized before the enum cast** — an unmapped token fails at the DB cast. Gate: a status
  whitelist on input.
- **Corrected/re-versioned CDRs dropped** — `DO NOTHING` keeps the first version; a corrected CDR under the
  same id is ignored. Gate: an upsert-on-newer policy.
- **Live call control / IVR / recording storage / voice analytics** — deferred (PRD non-goals).
