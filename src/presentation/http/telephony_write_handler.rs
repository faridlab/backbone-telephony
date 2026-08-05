//! Validated telephony write surface (hand-authored, user-owned; survives regen).
//!
//! This is the production write path for the Call entity — the alternative to the unguarded
//! generic `BackboneCrudHandler` write routes. It serves `TelephonyWriteService::record_call`
//! (provider CDR ingest, idempotent on (company, external_id), with talk-time, subject link, and
//! a same-tx outbox event) and `TelephonyWriteService::link_call` (attach a call to what it
//! concerns). `company_id` is taken from the request body: a provider webhook is not in a caller's
//! tenant session, so the scoping is explicit on the CDR rather than ambient (ADR-0008).
//!
//! Mount via `TelephonyModule::validated_routes()`, which merges these writes onto the read-only
//! base. The durable event path is the transactional outbox (staged in `record_call`); the
//! in-process `TelephonyEventSink` is a low-fanout notify that defaults to `LoggingSink` and can be
//! overridden on the module with `with_event_sink`.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::application::service::{
    CallOutcome, InboundCdr, TelephonyError, TelephonyEventSink, TelephonyWriteService,
};

// ============================================================================
// REQUEST DTOS
// ============================================================================

/// A call CDR as posted by a provider webhook (or a manual log). Maps 1:1 onto `InboundCdr`;
/// `direction` and `status` are strings to match the write-service contract.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestCallRequest {
    pub company_id: Uuid,
    pub direction: String,
    pub from_number: String,
    pub to_number: String,
    #[serde(default)]
    pub party_id: Option<Uuid>,
    #[serde(default)]
    pub agent_id: Option<Uuid>,
    /// The provider CDR id — the dedup key. Optional for a manually-logged call.
    #[serde(default)]
    pub external_id: Option<String>,
    pub status: String,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub answered_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub recording_url: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub subject_type: Option<String>,
    #[serde(default)]
    pub subject_id: Option<Uuid>,
}

impl IngestCallRequest {
    fn into_inbound(self) -> InboundCdr {
        InboundCdr {
            company_id: self.company_id,
            direction: self.direction,
            from_number: self.from_number,
            to_number: self.to_number,
            party_id: self.party_id,
            agent_id: self.agent_id,
            external_id: self.external_id,
            status: self.status,
            started_at: self.started_at,
            answered_at: self.answered_at,
            ended_at: self.ended_at,
            recording_url: self.recording_url,
            notes: self.notes,
            subject_type: self.subject_type,
            subject_id: self.subject_id,
        }
    }
}

/// Attach a call to what it concerns (a lead, an issue). `company_id` scopes the update so a
/// principal of company A cannot attach company B's call by knowing its id (ADR-0008).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCallRequest {
    pub company_id: Uuid,
    pub subject_type: String,
    pub subject_id: Uuid,
}

// ============================================================================
// ERROR -> HTTP
// ============================================================================

impl IntoResponse for TelephonyError {
    fn into_response(self) -> axum::response::Response {
        let (status, code) = match &self {
            Self::Invalid(_) => (StatusCode::BAD_REQUEST, "TELEPHONY_INVALID"),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "TELEPHONY_NOT_FOUND"),
            Self::Db(_) => (StatusCode::INTERNAL_SERVER_ERROR, "TELEPHONY_DATABASE"),
        };
        let body = serde_json::json!({
            "success": false,
            "error": code,
            "message": self.to_string(),
        });
        (status, Json(body)).into_response()
    }
}

// ============================================================================
// STATE + HANDLERS
// ============================================================================

/// Injection container for the validated write handlers.
#[derive(Clone)]
pub struct TelephonyWriteState {
    pub write_service: Arc<TelephonyWriteService>,
    pub event_sink: Arc<dyn TelephonyEventSink>,
}

/// `POST /calls` — record a provider CDR. Idempotent on (company, external_id): a redelivery
/// returns the original call with `duplicate: true` and publishes nothing. The CallLogged /
/// MissedCall event is staged in the same tx as the insert (durable via the outbox).
pub async fn ingest_call(
    State(st): State<TelephonyWriteState>,
    Json(req): Json<IngestCallRequest>,
) -> Result<Json<CallOutcome>, TelephonyError> {
    let cdr = req.into_inbound();
    let outcome = st.write_service.record_call(cdr, &*st.event_sink).await?;
    Ok(Json(outcome))
}

/// `POST /calls/:id/link` — attach a call to what it concerns (lead | issue). A tenant mismatch is
/// indistinguishable from a missing call (`NotFound`), so this does not leak whether the id exists.
pub async fn link_call_handler(
    State(st): State<TelephonyWriteState>,
    Path(call_id): Path<Uuid>,
    Json(req): Json<LinkCallRequest>,
) -> Result<StatusCode, TelephonyError> {
    st.write_service
        .link_call(call_id, req.company_id, &req.subject_type, req.subject_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// ROUTER
// ============================================================================

/// Build the validated write router (CDR ingest + subject link). GET /calls comes from the read
/// router merged separately in `TelephonyModule::validated_routes()` — different HTTP methods on
/// the same path, no collision.
pub fn create_telephony_validated_write_routes(
    write_service: Arc<TelephonyWriteService>,
    event_sink: Arc<dyn TelephonyEventSink>,
) -> Router {
    let state = TelephonyWriteState {
        write_service,
        event_sink,
    };
    Router::new()
        .route("/calls", post(ingest_call))
        .route("/calls/:id/link", post(link_call_handler))
        .with_state(state)
}
