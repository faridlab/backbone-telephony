//! Golden cases — the CDR log oracle: a completed call records its talk-time and publishes CallLogged; a
//! redelivered provider CDR is idempotent; a missed inbound call publishes MissedCall (the callback signal).

mod common;
use common::*;

use backbone_telephony::application::service::telephony_events::TelephonyEvent;
use backbone_telephony::application::service::telephony_write_service::*;
use uuid::Uuid;

fn cdr(company: Uuid, ext: &str, status: &str) -> InboundCdr {
    InboundCdr {
        company_id: company, direction: "inbound".into(), from_number: "+628111".into(),
        to_number: "+628999".into(), party_id: Some(Uuid::new_v4()), agent_id: Some(Uuid::new_v4()),
        external_id: Some(ext.into()), status: status.into(),
        started_at: ts(2026, 7, 9, 10, 0, 0),
        answered_at: if status == "completed" { Some(ts(2026, 7, 9, 10, 0, 10)) } else { None },
        ended_at: if status == "missed" { Some(ts(2026, 7, 9, 10, 0, 20)) } else { Some(ts(2026, 7, 9, 10, 3, 10)) },
        recording_url: None, notes: None, subject_type: None, subject_id: None,
    }
}

// TGC-1 — a completed call records talk-time (answered→ended) and publishes CallLogged.
#[tokio::test]
async fn tgc1_completed_call_logs_duration() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let svc = TelephonyWriteService::new(pool.clone());
    let sink = CapturingSink::new();

    let out = svc.record_call(cdr(company, &format!("c-{}", Uuid::new_v4()), "completed"), &sink).await.unwrap();
    assert!(!out.duplicate);
    assert_eq!(out.duration_seconds, 180, "10:00:10 → 10:03:10 = 180s");
    assert_eq!(sink.logged(), 1);
    match sink.last() {
        TelephonyEvent::CallLogged(c) => assert_eq!(c.duration_seconds, 180),
        _ => panic!("expected CallLogged"),
    }
}

// TGC-2 — a redelivered provider CDR (same company + external_id) is idempotent.
#[tokio::test]
async fn tgc2_redelivered_cdr_idempotent() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let svc = TelephonyWriteService::new(pool.clone());
    let sink = CapturingSink::new();
    let ext = format!("c-{}", Uuid::new_v4());

    let first = svc.record_call(cdr(company, &ext, "completed"), &sink).await.unwrap();
    let second = svc.record_call(cdr(company, &ext, "completed"), &sink).await.unwrap();
    assert!(!first.duplicate);
    assert!(second.duplicate);
    assert_eq!(first.call_id, second.call_id);
    assert_eq!(sink.logged(), 1, "published once despite the redelivery");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM telephony.calls WHERE company_id=$1 AND external_id=$2")
        .bind(company).bind(&ext).fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1);
}

// TGC-3 — a missed inbound call publishes MissedCall (a callback signal), duration 0.
#[tokio::test]
async fn tgc3_missed_inbound_raises_missed_call() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let svc = TelephonyWriteService::new(pool.clone());
    let sink = CapturingSink::new();

    let out = svc.record_call(cdr(company, &format!("c-{}", Uuid::new_v4()), "missed"), &sink).await.unwrap();
    assert_eq!(out.duration_seconds, 0);
    assert_eq!(sink.missed(), 1);
    assert_eq!(sink.logged(), 0);
    match sink.last() {
        TelephonyEvent::MissedCall(m) => assert_eq!(m.from_number, "+628111"),
        _ => panic!("expected MissedCall"),
    }
}

// TGC-4 — the event carries the handling agent (completeness council 2026-07-09) so a CRM consumer can
// attribute the call activity to the rep and run agent-productivity reporting from the event alone.
#[tokio::test]
async fn tgc4_event_carries_agent() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let svc = TelephonyWriteService::new(pool.clone());
    let sink = CapturingSink::new();
    let agent = Uuid::new_v4();

    let mut c = cdr(company, &format!("c-{}", Uuid::new_v4()), "completed");
    c.agent_id = Some(agent);
    svc.record_call(c, &sink).await.unwrap();
    match sink.last() {
        TelephonyEvent::CallLogged(l) => assert_eq!(l.agent_id, Some(agent), "activity attributable to the agent"),
        _ => panic!("expected CallLogged"),
    }
}
