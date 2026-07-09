# backbone-telephony — Extension Guide

## Public surface (stable)
- **Events** (`application::service::telephony_events`): `CallLogged`, `MissedCall`, the `TelephonyEvent`
  union, and `TelephonyEventSink`. CRM/support subscribe (over the bus or by draining the transactional
  outbox) to attach an activity or raise a callback. The event carries what a consumer needs (company,
  party, subject, from, duration) — it never re-queries this module.
- **Write path** (`application::service::telephony_write_service::TelephonyWriteService`): `record_call`
  (the idempotent CDR intake) + `link_call`, with `InboundCdr` / `CallOutcome` DTOs.
- **Durability**: `CallLogged`/`MissedCall` is staged in this module's `telephony.outbox_events` in the
  same tx as the call insert; a composing service runs a relay to deliver it, and a consumer's inbox makes
  the effect exactly-once.

## How a consuming service uses telephony
Wire your PBX/provider webhook to `record_call(InboundCdr { … })`. Subscribe to `MessageReceived`… to
`CallLogged` to append a call activity to the linked lead/issue, and to `MissedCall` to raise a callback
task (proven: a REAL backbone-crm callback lead). Call `link_call(subject_type, subject_id)` to attach a
call to what it concerns. Never mutate telephony's tables directly.

## Not a contract
- The 12 generated CRUD endpoints per entity are convenience scaffolding. Do **not** insert a call or set
  a duration/status through the generic PATCH surface — it bypasses the dedup, the talk-time derivation,
  and the outbox staging. Use `TelephonyWriteService`.
- `// <<< CUSTOM` blocks preserve local edits only; not a cross-module extension point.

## Invariants a consumer must not break
- One call per `(company, external_id)`; `record_call` is the only CDR writer.
- `duration_seconds` is non-negative and consistent with the call's outcome; the routing event is durable.
- A missed inbound call always surfaces (`MissedCall`) — an unanswered customer call never silently vanishes.
