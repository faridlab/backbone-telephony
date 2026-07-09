# backbone-telephony — BRD

## Documents
Call (a CDR against a party). Own Postgres schema `telephony`. Posts **no GL**. Publishes call events over
the event bus.

## Business rules

**BR-1 (idempotent recording).** `record_call` records a CDR, deduped on `(company, external_id)` when a
provider id is present (webhooks are at-least-once; a redelivery is a no-op). A manually-logged call
(no `external_id`) never collides. From/to numbers are required.

**BR-2 (talk-time + outcome).** Talk-time is `answered_at → ended_at` in whole seconds, and 0 for a call
never answered. The call's outcome (`completed | missed | failed`) and its timestamps must be
**self-consistent** — a stored duration is never negative, and the emitted `status` matches the facts
(maturity council 2026-07-09).

**BR-3 (routing).** A completed call publishes `CallLogged` (attach an activity to what it concerns); an
unanswered inbound call publishes `MissedCall` (raise a callback task). The event is **staged in the same
tx as the call insert** (durable — a crash between commit and publish can't drop it).

**BR-4 (linking).** `link_call(subject_type, subject_id)` attaches a call to the lead/issue it concerns.

## Events
`CallLogged` (call_id, company_id, direction, party_id?, **agent_id?**, subject_type?, subject_id?,
from_number, duration_seconds), `MissedCall` (call_id, company_id, party_id?, **agent_id?**, subject_type?,
subject_id?, from_number) — `agent_id` lets a consumer attribute the activity / route the callback
(completeness council 2026-07-09).

## Deferred (with reason)
The concrete PBX/provider integration, live call control/IVR/queuing, recording storage/transcription,
voice analytics (tier5-deferred §5).
