//! The hand-authored telephony write path (user-owned; survives regen).
//!
//! A communication-adjacent CDR log: a provider posts a call (a webhook, at-least-once) and it is recorded
//! **idempotently** on (company, external_id), with its outcome + talk-time, linked to what it concerns.
//! A completed call publishes `CallLogged`; an unanswered inbound call publishes `MissedCall` (a callback
//! signal). The routing event is staged in the SAME tx as the call insert (durable). Posts NO GL.

use backbone_orm::company_scope;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::infrastructure::persistence::{CallRepository, NewCallRow};

use super::telephony_events::*;

#[derive(Debug, thiserror::Error)]
pub enum TelephonyError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("not found: {0}")]
    NotFound(&'static str),
    #[error("invalid input: {0}")]
    Invalid(String),
}

/// A call as delivered by a provider CDR webhook (or a manual log).
pub struct InboundCdr {
    pub company_id: Uuid,
    pub direction: String, // inbound | outbound
    pub from_number: String,
    pub to_number: String,
    pub party_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    /// The provider CDR id — the dedup key. Optional for a manually-logged call.
    pub external_id: Option<String>,
    pub status: String, // completed | missed | failed
    pub started_at: DateTime<Utc>,
    pub answered_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub recording_url: Option<String>,
    pub notes: Option<String>,
    pub subject_type: Option<String>,
    pub subject_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallOutcome {
    pub call_id: Uuid,
    pub duration_seconds: i32,
    pub duplicate: bool,
}

pub struct TelephonyWriteService {
    pool: PgPool,
    calls: CallRepository,
}

impl TelephonyWriteService {
    pub fn new(pool: PgPool) -> Self {
        let calls = CallRepository::new(pool.clone());
        Self { pool, calls }
    }

    /// Record a call CDR — idempotent on (company, external_id) when a provider id is present. Computes
    /// talk-time, records the outcome, and publishes `CallLogged` (completed) or `MissedCall` (unanswered
    /// inbound), staged in the same tx. A redelivered CDR returns the original with `duplicate=true`.
    pub async fn record_call(
        &self,
        c: InboundCdr,
        events: &dyn TelephonyEventSink,
    ) -> Result<CallOutcome, TelephonyError> {
        if c.from_number.trim().is_empty() || c.to_number.trim().is_empty() {
            return Err(TelephonyError::Invalid("a call needs from/to numbers".into()));
        }

        // RLS scope (ADR-0008): the company is on the CDR — bind it for the whole body so the dedup
        // probe, the insert transaction, and the duplicate re-read all run with `app.company_id` set.
        // A provider webhook is not an HTTP request in the caller's tenant, so this explicit scope
        // (not an ambient request one) is what fences the write. Explicit `company_id` binds stay as
        // defense-in-depth.
        let company = c.company_id;
        company_scope::with_company_scope(Some(company), async move {
        // Fast path: already seen this provider CDR → return the original, publish nothing.
        if let Some(ext) = &c.external_id {
            if let Some(row) = self.calls.find_outcome_by_external_id(&self.pool, c.company_id, ext).await? {
                return Ok(CallOutcome {
                    call_id: row.id, duration_seconds: row.duration_seconds, duplicate: true,
                });
            }
        }

        let duration = talk_time(c.answered_at, c.ended_at);
        let call_id = Uuid::new_v4();

        let mut tx = self.pool.begin().await?;
        company_scope::bind_company_on(&mut tx, c.company_id).await?;
        let inserted = self.calls.insert_cdr(&mut tx, &NewCallRow {
            id: call_id,
            company_id: c.company_id,
            direction: &c.direction,
            from_number: &c.from_number,
            to_number: &c.to_number,
            party_id: c.party_id,
            agent_id: c.agent_id,
            status: &c.status,
            external_id: c.external_id.as_deref(),
            subject_type: c.subject_type.as_deref(),
            subject_id: c.subject_id,
            started_at: c.started_at,
            answered_at: c.answered_at,
            ended_at: c.ended_at,
            duration_seconds: duration,
            recording_url: c.recording_url.as_deref(),
            notes: c.notes.as_deref(),
        }).await?;

        let Some(call_id) = inserted else {
            tx.rollback().await?;
            let row = self.calls.fetch_outcome_by_external_id(
                &self.pool, c.company_id, c.external_id.as_deref().unwrap_or_default(),
            ).await?;
            return Ok(CallOutcome {
                call_id: row.id, duration_seconds: row.duration_seconds, duplicate: true,
            });
        };

        // The event: a missed inbound call raises a callback; anything else is a logged activity. Route
        // MissedCall only when the call was GENUINELY unanswered — a "missed" status that nonetheless
        // carries an answered_at is a contradictory CDR and is logged as the real conversation it was,
        // not a spurious callback (maturity council 2026-07-09).
        let event = if c.status == "missed" && c.answered_at.is_none() {
            TelephonyEvent::MissedCall(MissedCall {
                call_id, company_id: c.company_id, party_id: c.party_id, agent_id: c.agent_id,
                subject_type: c.subject_type.clone(), subject_id: c.subject_id,
                from_number: c.from_number.clone(),
            })
        } else {
            TelephonyEvent::CallLogged(CallLogged {
                call_id, company_id: c.company_id, direction: c.direction.clone(), party_id: c.party_id,
                agent_id: c.agent_id, subject_type: c.subject_type.clone(), subject_id: c.subject_id,
                from_number: c.from_number.clone(), duration_seconds: duration,
            })
        };
        let record = backbone_outbox::OutboxRecord::new(
            event_type_of(&event), "Call", call_id.to_string(),
            serde_json::to_value(&event).map_err(|e| TelephonyError::Invalid(e.to_string()))?,
            Utc::now(),
        );
        backbone_outbox::outbox::stage(&mut *tx, "telephony", &record)
            .await
            .map_err(|e| TelephonyError::Invalid(format!("outbox stage: {e}")))?;

        tx.commit().await?;
        events.publish(&event);
        Ok(CallOutcome { call_id, duration_seconds: duration, duplicate: false })
        }).await
    }

    /// Attach a call to what it concerns (a lead, an issue).
    pub async fn link_call(&self, call_id: Uuid, subject_type: &str, subject_id: Uuid) -> Result<(), TelephonyError> {
        // RLS scope (ADR-0008), ID-only pattern: identified by the call id alone — no company argument.
        // The write rides the REQUEST-dedicated connection (which carries the caller's `app.company_id`),
        // so another company's call is simply not matched. A non-request caller (event/job) must wrap
        // this in `with_company_scope(Some(company_id))`, otherwise it fails closed.
        let n = self.calls.update_subject(&self.pool, call_id, subject_type, subject_id).await?;
        if n != 1 {
            return Err(TelephonyError::NotFound("call"));
        }
        Ok(())
    }
}

/// Talk-time in whole seconds from answered→ended. Zero when never answered, and **clamped ≥ 0** so a
/// provider CDR with `ended_at < answered_at` (clock skew, out-of-order retransmit, timezone bug) can't
/// store a negative duration that corrupts talk-time analytics (maturity council 2026-07-09).
fn talk_time(answered_at: Option<DateTime<Utc>>, ended_at: Option<DateTime<Utc>>) -> i32 {
    match (answered_at, ended_at) {
        (Some(a), Some(e)) if e >= a => (e - a).num_seconds().min(i32::MAX as i64) as i32,
        _ => 0,
    }
}

fn event_type_of(e: &TelephonyEvent) -> &'static str {
    match e {
        TelephonyEvent::CallLogged(_) => "CallLogged",
        TelephonyEvent::MissedCall(_) => "MissedCall",
    }
}
