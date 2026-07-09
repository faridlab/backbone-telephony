//! The missed-call seam against the REAL backbone-crm module. A missed inbound call routes through
//! `MissedCall`; a consumer (here, the test as the composing service) raises a REAL callback lead from the
//! event alone. Proves the event carries what a consumer needs. ZERO normal Cargo edge — crm is a
//! dev-dependency only, and routing is an event, not a driven call.

mod common;
use common::*;

use backbone_crm::application::service::crm_write_service::{CrmWriteService, NewLead};
use backbone_telephony::application::service::telephony_events::TelephonyEvent;
use backbone_telephony::application::service::telephony_write_service::*;
use uuid::Uuid;

// TSEAM-1 — a missed inbound call raises a REAL CRM callback lead carrying the caller's number.
#[tokio::test]
async fn tseam1_missed_call_raises_real_crm_lead() {
    let pool = pool().await;
    let company = Uuid::new_v4();
    let svc = TelephonyWriteService::new(pool.clone());
    let crm = CrmWriteService::new(pool.clone());
    let sink = CapturingSink::new();

    let out = svc.record_call(InboundCdr {
        company_id: company, direction: "inbound".into(), from_number: "+628123456".into(),
        to_number: "+628999".into(), party_id: Some(Uuid::new_v4()), agent_id: None,
        external_id: Some(format!("c-{}", Uuid::new_v4())), status: "missed".into(),
        started_at: ts(2026, 7, 9, 9, 0, 0), answered_at: None, ended_at: Some(ts(2026, 7, 9, 9, 0, 30)),
        recording_url: None, notes: None, subject_type: None, subject_id: None,
    }, &sink).await.unwrap();
    let _ = out;

    // The composing service consumes MissedCall and raises a REAL callback lead from it alone.
    let missed = match sink.last() {
        TelephonyEvent::MissedCall(m) => m,
        _ => panic!("expected MissedCall"),
    };
    let lead_id = crm.create_lead(NewLead {
        company_id: missed.company_id,
        lead_name: format!("Missed call {}", missed.from_number),
        organization_name: None,
        phone: Some(missed.from_number.clone()),
        whatsapp_no: None,
        email: None,
        source: "other".into(),
        campaign_id: None,
        notes: Some("Auto-created from a missed inbound call — call back".into()),
    }).await.expect("real crm raises the callback lead");

    let (phone, status): (Option<String>, String) = sqlx::query_as(
        "SELECT phone, status::text FROM crm.leads WHERE id=$1")
        .bind(lead_id).fetch_one(&pool).await.unwrap();
    assert_eq!(phone.as_deref(), Some("+628123456"), "the callback lead carries the caller's number");
    assert_eq!(status, "new");
}
