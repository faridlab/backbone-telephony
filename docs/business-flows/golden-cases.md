# backbone-telephony — business flows & golden cases

## Flow: provider CDR → dedup → derive → route (durably)
```
record_call (provider CDR webhook, at-least-once)
   │
   ▼  dedup on (company, external_id) — redelivery → duplicate=true, write nothing, publish nothing
   │
   ▼  derive talk-time (answered→ended, clamped ≥ 0) + INSERT the call
   │
   ▼  STAGE CallLogged / MissedCall in the outbox — SAME tx as the insert (durable) → commit
   │
   ├▶ publish (in-proc, immediate) + relay drains the outbox (at-least-once)
   │
   └▶ CallLogged → CRM/support attach an activity · MissedCall → CRM raises a callback lead
```
Posts NO GL. `link_call` attaches a call to the lead/issue it concerns.

## Golden cases (`tests/telephony_golden_cases.rs`)
- **TGC-1 — completed call logs duration.** Answered 10:00:10 → ended 10:03:10 = 180s; `CallLogged` carries
  180.
- **TGC-2 — redelivered CDR idempotent.** The same (company, external_id) twice → one row, one publish.
- **TGC-3 — missed inbound raises MissedCall.** An unanswered inbound → `MissedCall`, duration 0.
- **TGC-4 — event carries the handling agent.** A call recorded with an agent → `CallLogged.agent_id` set,
  so a consumer attributes the activity to the rep (agent-productivity) from the event alone.

## Integrity probes (`tests/integrity_probes.rs`)
- **TIP-1 — numbers required.**
- **TIP-2 — manual calls never collide.** Two manual logs (null external_id) are distinct.
- **TIP-3 — routing event durable.** With the in-proc publish lost (dropping sink), `CallLogged` is still
  staged in the outbox.
- **TIP-4 — negative duration cannot be stored.** A CDR with `ended < answered` clamps talk-time to 0
  (recorded, not rejected); the DB CHECK backstops any writer. Proven-by-revert.

## Seam (`tests/telephony_crm_seam.rs`)
- **TSEAM-1 — missed call raises a REAL crm lead.** A missed inbound → `MissedCall` → REAL backbone-crm
  `create_lead` (a callback lead carrying the caller's number). Zero normal Cargo edge.

## §5 round-trip (`scripts/telephony_crm_seam_roundtrip.sh`)
Regen (`--force`) leaves the seam files (`telephony_events.rs`, `telephony_write_service.rs`)
byte-identical; the oracle + seam re-run green.
