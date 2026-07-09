# backbone-telephony — PRD

Thin-channels pillar (Tier 5) · a **communication-adjacent CDR log** · posts **no GL** · publishes call events.

## Why
A call-center or high-touch-sales SMB wants its **calls on the customer timeline**: who called, how long,
what it concerned, and — critically — **which inbound calls were missed** so someone calls back. This is
the lean telephony core: a call detail record (CDR) log against a party that turns a call into a domain
event CRM/support attach to. Promoted against a named call-center requirement (tier5-deferred §5).

## Scope (KEEP — tier5-deferred.md §5)
- **Call** — a CDR: direction, from/to numbers, party, agent, outcome (`ringing → answered → completed |
  missed | failed`), talk-time, recording URL, notes, linked to the lead/issue it concerns.
- **Idempotent recording** — `record_call` dedups on `(company, external_id)` (provider webhooks are
  at-least-once); a redelivered CDR is a no-op.
- **Call routing** — a completed call publishes `CallLogged` (attach an activity); an unanswered inbound
  call publishes `MissedCall` (raise a callback task) — the durable event a consumer routes on.
- **Linking** — `link_call` attaches a call to a lead/issue after the fact.

## Non-goals (CUT / DEFER — tier5-deferred.md §5)
- The concrete PBX/SIP/provider integration (Twilio, a local SIP trunk) — wired by a composing service;
  the module records the CDR it's handed.
- Live call control (dial, transfer, hold), IVR, call queuing/ACD, real-time presence.
- Call recording storage / transcription (a `recording_url` reference only).
- Voice analytics / sentiment.

## Success criteria
- A CDR is recorded **exactly once** under at-least-once webhook redelivery, and the routing event is
  **durable** (survives a crash between commit and publish).
- A missed inbound call raises a real CRM callback lead (proven against REAL backbone-crm).
- Zero normal Cargo edge; survives a full codegen regen (§5). Posts no GL.
