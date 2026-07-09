//! Telephony domain events (hand-authored, user-owned) — the public routing surface.
//!
//! A logged call becomes a domain event CRM/support subscribe to: `CallLogged` (attach a note/activity to
//! the lead/issue it concerns) and `MissedCall` (raise a callback task — an unanswered inbound customer
//! call must not vanish). Published over the event bus — a consuming service supplies the sink.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A call was logged (completed) — the signal to attach an activity to what it concerns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CallLogged {
    pub call_id: Uuid,
    pub company_id: Uuid,
    pub direction: String,
    pub party_id: Option<Uuid>,
    /// The internal agent who handled the call — so a CRM consumer can attribute the activity and run
    /// agent-productivity reporting from the event alone (completeness council 2026-07-09).
    pub agent_id: Option<Uuid>,
    pub subject_type: Option<String>,
    pub subject_id: Option<Uuid>,
    pub from_number: String,
    pub duration_seconds: i32,
}

/// An inbound call rang but was never answered — the signal to raise a callback task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MissedCall {
    pub call_id: Uuid,
    pub company_id: Uuid,
    pub party_id: Option<Uuid>,
    /// The agent the inbound call was routed to (if any) — so the callback can go to the owning rep.
    pub agent_id: Option<Uuid>,
    pub subject_type: Option<String>,
    pub subject_id: Option<Uuid>,
    pub from_number: String,
}

/// The telephony domain-event union.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum TelephonyEvent {
    CallLogged(CallLogged),
    MissedCall(MissedCall),
}

/// Sink the write path publishes to. A consuming service supplies its own (bus, outbox, …).
pub trait TelephonyEventSink: Send + Sync {
    fn publish(&self, event: &TelephonyEvent);
}

/// A no-op/logging sink for tests and single-process composition.
#[derive(Debug, Default, Clone)]
pub struct LoggingSink;

impl TelephonyEventSink for LoggingSink {
    fn publish(&self, event: &TelephonyEvent) {
        tracing::info!(?event, "telephony event");
    }
}
