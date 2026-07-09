//! The hand-authored telephony write path (user-owned; survives regen).
//!
//! A communication-adjacent CDR log: a provider posts a call (a webhook, at-least-once) and it is recorded
//! **idempotently** on (company, external_id), with its outcome + talk-time, linked to what it concerns.
//! A completed call publishes `CallLogged`; an unanswered inbound call publishes `MissedCall` (a callback
//! signal). The routing event is staged in the SAME tx as the call insert (durable). Posts NO GL.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

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
}

impl TelephonyWriteService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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

        // Fast path: already seen this provider CDR → return the original, publish nothing.
        if let Some(ext) = &c.external_id {
            if let Some(row) = sqlx::query(
                "SELECT id, duration_seconds FROM telephony.calls WHERE company_id=$1 AND external_id=$2")
                .bind(c.company_id).bind(ext)
                .fetch_optional(&self.pool)
                .await?
            {
                return Ok(CallOutcome {
                    call_id: row.get("id"), duration_seconds: row.get("duration_seconds"), duplicate: true,
                });
            }
        }

        let duration = talk_time(c.answered_at, c.ended_at);
        let call_id = Uuid::new_v4();

        let mut tx = self.pool.begin().await?;
        let inserted: Option<Uuid> = sqlx::query_scalar(
            r#"INSERT INTO telephony.calls
                 (id, company_id, direction, from_number, to_number, party_id, agent_id, status,
                  external_id, subject_type, subject_id, started_at, answered_at, ended_at,
                  duration_seconds, recording_url, notes)
               VALUES ($1,$2,$3::call_direction,$4,$5,$6,$7,$8::call_status,$9,$10,$11,$12,$13,$14,$15,$16,$17)
               ON CONFLICT (company_id, external_id) DO NOTHING
               RETURNING id"#,
        )
        .bind(call_id).bind(c.company_id).bind(&c.direction).bind(&c.from_number).bind(&c.to_number)
        .bind(c.party_id).bind(c.agent_id).bind(&c.status).bind(&c.external_id)
        .bind(&c.subject_type).bind(c.subject_id).bind(c.started_at).bind(c.answered_at).bind(c.ended_at)
        .bind(duration).bind(&c.recording_url).bind(&c.notes)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(call_id) = inserted else {
            tx.rollback().await?;
            let row = sqlx::query(
                "SELECT id, duration_seconds FROM telephony.calls WHERE company_id=$1 AND external_id=$2")
                .bind(c.company_id).bind(c.external_id.as_deref().unwrap_or_default())
                .fetch_one(&self.pool)
                .await?;
            return Ok(CallOutcome {
                call_id: row.get("id"), duration_seconds: row.get("duration_seconds"), duplicate: true,
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
    }

    /// Attach a call to what it concerns (a lead, an issue).
    pub async fn link_call(&self, call_id: Uuid, subject_type: &str, subject_id: Uuid) -> Result<(), TelephonyError> {
        let n = sqlx::query(
            r#"UPDATE telephony.calls SET subject_type=$2, subject_id=$3
               WHERE id=$1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(call_id).bind(subject_type).bind(subject_id)
        .execute(&self.pool)
        .await?;
        if n.rows_affected() != 1 {
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
