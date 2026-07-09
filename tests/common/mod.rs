//! Shared test helpers: a live pool and a capturing event sink.

#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use backbone_telephony::application::service::telephony_events::{TelephonyEvent, TelephonyEventSink};
use sqlx::PgPool;

pub fn dburl() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5433/backbone_telephony".into())
}
pub async fn pool() -> PgPool {
    PgPool::connect(&dburl()).await.expect("connect")
}
pub fn ts(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> chrono::DateTime<chrono::Utc> {
    chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, y, mo, d, h, mi, s).unwrap()
}

#[derive(Clone, Default)]
pub struct CapturingSink {
    pub events: Arc<Mutex<Vec<TelephonyEvent>>>,
}
impl CapturingSink {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn logged(&self) -> usize {
        self.events.lock().unwrap().iter().filter(|e| matches!(e, TelephonyEvent::CallLogged(_))).count()
    }
    pub fn missed(&self) -> usize {
        self.events.lock().unwrap().iter().filter(|e| matches!(e, TelephonyEvent::MissedCall(_))).count()
    }
    pub fn last(&self) -> TelephonyEvent {
        self.events.lock().unwrap().last().cloned().expect("an event")
    }
}
impl TelephonyEventSink for CapturingSink {
    fn publish(&self, event: &TelephonyEvent) {
        self.events.lock().unwrap().push(event.clone());
    }
}

/// A sink that drops every event — models a crash/loss between the DB commit and the in-proc publish.
pub struct DroppingSink;
impl TelephonyEventSink for DroppingSink {
    fn publish(&self, _e: &TelephonyEvent) {}
}
