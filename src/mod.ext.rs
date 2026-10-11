// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// Types the hand-authored module wiring below (validated write surface) references.
use application::service::{LoggingSink, TelephonyEventSink, TelephonyWriteService};
