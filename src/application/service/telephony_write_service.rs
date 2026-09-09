//! The hand-authored telephony write path (user-owned; survives regen).
//!
//! A communication-adjacent CDR log: a provider posts a call (a webhook, at-least-once) and it is recorded
//! **idempotently** on external_id within whatever fence the composing service installs (ADR-0029), with
//! its outcome + talk-time, linked to what it concerns. A completed call publishes `CallLogged`; an
//! unanswered inbound call publishes `MissedCall` (a callback signal). The routing event is staged in the
//! SAME tx as the call insert (durable). Posts NO GL.
//!
//! Tenancy: none, by design (ADR-0029). No write here takes a tenant key and none binds one of its own.
//! The one exception is the transactional outbox: `outbox_events` is a framework-owned, still-company-
//! keyed surface — every staged record carries its owning tenant — so the write path reads the ambient
//! org scope's legacy company and fails closed when the composing service mounted no scope.

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
    /// The write needs the owning tenant for the still-company-keyed outbox record (and the routing
    /// event built from it), but the request carries no org scope whose legacy company could name it.
    /// This is a composition fault — the service must be mounted under a scope-resolving auth
    /// middleware — not a caller error, so it fails loud instead of guessing.
    #[error("no org scope bound: {0}")]
    OrgScopeRequired(&'static str),
}

/// A call as delivered by a provider CDR webhook (or a manual log).
pub struct InboundCdr {
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

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
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

    /// The pool this service was built with — the composing app's boot pool.
    /// Handlers use it as the fallback for callers that carry no
    /// tenant-dedicated pool on the request.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Record a call CDR — idempotent on external_id (within the composing service's fence) when a
    /// provider id is present. Computes talk-time, records the outcome, and publishes `CallLogged`
    /// (completed) or `MissedCall` (unanswered inbound), staged in the same tx. A redelivered CDR
    /// returns the original with `duplicate=true`.
    ///
    /// `pool` is the caller-named pool this write belongs to: under a tenant router the handler
    /// passes the request's tenant-dedicated pool (the service's own pool is the composing app's
    /// boot pool — the wrong database for any other tenant). Unfenced deployments pass their own.
    pub async fn record_call(
        &self,
        pool: &PgPool,
        c: InboundCdr,
        events: &dyn TelephonyEventSink,
    ) -> Result<CallOutcome, TelephonyError> {
        if c.from_number.trim().is_empty() || c.to_number.trim().is_empty() {
            return Err(TelephonyError::Invalid(
                "a call needs from/to numbers".into(),
            ));
        }

        // The durable event path (and the routing event built from it) must name the owning tenant:
        // the outbox is a still-company-keyed surface (ADR-0011). Take it from the ambient org scope
        // the composing service resolved — never guess — and fail closed when absent.
        let scope = backbone_orm::org_scope::current_org_scope();
        let owning_company = scope.as_ref().and_then(|s| s.legacy_company_id()).ok_or(
            TelephonyError::OrgScopeRequired(
                "recording a call stages an outbox event that must carry the owning tenant",
            ),
        )?;

        // Fast path: already seen this provider CDR → return the original, publish nothing. Rides the
        // scoped-execute helper, so under the composing service's fence the probe sees only the
        // caller's rows; dedup is per-fence, never global by construction.
        if let Some(ext) = &c.external_id {
            if let Some(row) = self.calls.find_outcome_by_external_id(pool, ext).await? {
                return Ok(CallOutcome {
                    call_id: row.id,
                    duration_seconds: row.duration_seconds,
                    duplicate: true,
                });
            }
        }

        let duration = talk_time(c.answered_at, c.ended_at);
        let call_id = Uuid::new_v4();

        let mut tx = pool.begin().await?;
        // Propagate the ambient request scope, when one is bound, onto this transaction: the
        // repositories' scoped helpers ride the request-dedicated connection, but this pool
        // transaction does not. Binding relay-only keeps the module posture-agnostic on the calls
        // table itself AND satisfies the still-company-keyed outbox fence for the stage below
        // (the org scope's legacy company sets `app.company_id` on the transaction).
        if let Some(scope) = &scope {
            backbone_orm::org_scope::bind_org_scope_on(&mut *tx, scope).await?;
        }
        if let Err(e) = self
            .calls
            .insert_cdr(
                &mut tx,
                &NewCallRow {
                    id: call_id,
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
                },
            )
            .await
        {
            // A concurrent duplicate that lost the composing service's per-unit external_id unique
            // is not an error: roll back and answer idempotently with the winner.
            if e.as_database_error()
                .map(|d| d.is_unique_violation())
                .unwrap_or(false)
            {
                tx.rollback().await?;
                if let Some(ext) = &c.external_id {
                    if let Some(row) = self.calls.find_outcome_by_external_id(pool, ext).await? {
                        return Ok(CallOutcome {
                            call_id: row.id,
                            duration_seconds: row.duration_seconds,
                            duplicate: true,
                        });
                    }
                }
            }
            return Err(e.into());
        }

        // The event: a missed inbound call raises a callback; anything else is a logged activity. Route
        // MissedCall only when the call was GENUINELY unanswered — a "missed" status that nonetheless
        // carries an answered_at is a contradictory CDR and is logged as the real conversation it was,
        // not a spurious callback (maturity council 2026-07-09). `company_id` stays on the event: the
        // consuming CRM/support context is still company-keyed, so the event names the owning tenant.
        let event = if c.status == "missed" && c.answered_at.is_none() {
            TelephonyEvent::MissedCall(MissedCall {
                call_id,
                company_id: owning_company,
                party_id: c.party_id,
                agent_id: c.agent_id,
                subject_type: c.subject_type.clone(),
                subject_id: c.subject_id,
                from_number: c.from_number.clone(),
            })
        } else {
            TelephonyEvent::CallLogged(CallLogged {
                call_id,
                company_id: owning_company,
                direction: c.direction.clone(),
                party_id: c.party_id,
                agent_id: c.agent_id,
                subject_type: c.subject_type.clone(),
                subject_id: c.subject_id,
                from_number: c.from_number.clone(),
                duration_seconds: duration,
            })
        };
        let record = backbone_outbox::OutboxRecord::new(
            event_type_of(&event),
            "Call",
            call_id.to_string(),
            owning_company,
            serde_json::to_value(&event).map_err(|e| TelephonyError::Invalid(e.to_string()))?,
            Utc::now(),
        );
        backbone_outbox::outbox::stage(&mut *tx, "telephony", &record)
            .await
            .map_err(|e| TelephonyError::Invalid(format!("outbox stage: {e}")))?;

        tx.commit().await?;
        events.publish(&event);
        Ok(CallOutcome {
            call_id,
            duration_seconds: duration,
            duplicate: false,
        })
    }

    /// Attach a call to what it concerns (a lead, an issue).
    ///
    /// No tenant argument (ADR-0029): the update rides the scoped-execute helper, so under the
    /// composing service's fence another unit's call is simply not matched — a principal cannot
    /// attach a call they do not own by knowing its id. A mismatched tenant is indistinguishable
    /// from a missing call (`NotFound`), so this does not leak whether the id exists.
    pub async fn link_call(
        &self,
        call_id: Uuid,
        subject_type: &str,
        subject_id: Uuid,
    ) -> Result<(), TelephonyError> {
        let n = self
            .calls
            .update_subject(&self.pool, call_id, subject_type, subject_id)
            .await?;
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
