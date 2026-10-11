// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

pub mod telephony_events;
pub mod telephony_write_service;
pub use telephony_events::{
    CallLogged, LoggingSink, MissedCall, TelephonyEvent, TelephonyEventSink,
};
pub use telephony_write_service::{CallOutcome, InboundCdr, TelephonyError, TelephonyWriteService};
