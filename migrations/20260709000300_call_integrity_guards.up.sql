-- Storage-layer backstop for the CDR's DERIVED value. The schema's @default(0) never emitted a CHECK, so
-- talk-time integrity lived only in the Rust write path (maturity council 2026-07-09). A skewed CDR
-- (ended_at < answered_at) is still RECORDED — losing a call to clock skew would drop its missed-call
-- callback — but its derived duration is clamped >= 0; this CHECK backstops that against ANY writer,
-- including the generic PATCH. The raw provider timestamps are preserved as-is for audit.
ALTER TABLE telephony.calls
  ADD CONSTRAINT calls_duration_non_negative CHECK (duration_seconds >= 0);
