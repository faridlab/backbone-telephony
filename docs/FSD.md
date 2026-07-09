# backbone-telephony — FSD

## Entities
Call (`company_id`, `direction`, `from_number`/`to_number`, `party_id?` logical, `agent_id?` logical,
`status`, `external_id?`, `subject_type?`/`subject_id?` polymorphic logical ref, `started_at`,
`answered_at?`, `ended_at?`, `duration_seconds`, `recording_url?`, `notes?`; unique `(company_id,
external_id)` — the CDR dedup guard, NULLs distinct; CHECK `duration_seconds >= 0`). Enums: CallDirection
{inbound, outbound}, CallStatus {ringing, answered, completed, missed, failed}.

## Write path (`TelephonyWriteService`, hand-authored, user-owned)
- `record_call(InboundCdr, &dyn TelephonyEventSink)` → dedup on (company, external_id); derive talk-time
  (clamped ≥ 0); insert + **stage `CallLogged`/`MissedCall` in the same tx (outbox)** + publish; returns
  `CallOutcome {call_id, duration_seconds, duplicate}`. Idempotent + durable.
- `link_call(call_id, subject_type, subject_id)` → attach a call to a lead/issue.

Errors: `TelephonyError {Db, NotFound, Invalid}`.

## Seams (ports — zero normal Cargo edge)
- **Inbound → event bus (proven, TSEAM-1):** `CallLogged`/`MissedCall` staged to the outbox + published to
  a `TelephonyEventSink`; a consumer (proven: REAL backbone-crm `create_lead`) raises a callback lead from
  the event alone. Routing is an event, not a driven call.
- **Provider:** the PBX/SIP/CDR webhook is wired by a composing service into `record_call`; the module
  ships no provider SDK.

## Test oracle
`telephony_golden_cases` (4: TGC-1 completed call logs duration, TGC-2 redelivered CDR idempotent, TGC-3
missed inbound raises MissedCall, TGC-4 event carries the handling agent),
`integrity_probes` (4: TIP-1 numbers required, TIP-2 manual calls never collide, TIP-3 routing event
durable via outbox, TIP-4 negative duration cannot be stored),
`telephony_crm_seam` (1: TSEAM-1 missed call raises a REAL crm callback lead) + §5 round-trip. **9 tests.**

> The generated `integration_tests.rs` hits an external HTTP server and is environmental scaffolding, not
> part of this module's correctness gate.
