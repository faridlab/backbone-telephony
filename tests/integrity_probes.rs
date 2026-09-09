//! Integrity probes — the CDR log's invariants: numbers required, dedup on the provider id, and the
//! routing event is durable (staged in the outbox, survives a lost in-proc publish).
//!
//! Tenancy (ADR-0029): the module is tenant-agnostic — no company on any CDR, no fence of its own.
//! The one tenancy-adjacent dependency is the durable-event path: `outbox_events` is a framework-
//! owned, still-company-keyed surface, so probes that run a real write wrap it in an org request
//! scope carrying a legacy company — the same thing a composing service's scope-resolving auth
//! middleware provides. Everything asserted is domain; fence behavior is the composing service's
//! to prove.

mod common;
use common::*;

use backbone_telephony::application::service::telephony_write_service::*;
use uuid::Uuid;

fn cdr(ext: Option<String>) -> InboundCdr {
    InboundCdr {
        direction: "inbound".into(),
        from_number: "+628111".into(),
        to_number: "+628999".into(),
        party_id: None,
        agent_id: None,
        external_id: ext,
        status: "completed".into(),
        started_at: ts(2026, 7, 9, 10, 0, 0),
        answered_at: Some(ts(2026, 7, 9, 10, 0, 5)),
        ended_at: Some(ts(2026, 7, 9, 10, 1, 5)),
        recording_url: None,
        notes: None,
        subject_type: None,
        subject_id: None,
    }
}

// TIP-1 — a call needs from/to numbers.
#[tokio::test]
async fn tip1_numbers_required() {
    let pool = pool().await;
    let svc = TelephonyWriteService::new(pool.clone());
    let mut c = cdr(None);
    c.from_number = "  ".into();
    let r = scoped(&pool, async {
        svc.record_call(&pool, c, &CapturingSink::new()).await
    })
    .await;
    assert!(matches!(r, Err(TelephonyError::Invalid(_))));
}

// TIP-2 — dedup keys on external_id within the composing service's fence; a manual call (no
// external_id) never collides.
#[tokio::test]
async fn tip2_manual_calls_never_collide() {
    let pool = pool().await;
    let svc = TelephonyWriteService::new(pool.clone());
    let (a, b) = scoped(&pool, async {
        let a = svc
            .record_call(&pool, cdr(None), &CapturingSink::new())
            .await
            .unwrap();
        let b = svc
            .record_call(&pool, cdr(None), &CapturingSink::new())
            .await
            .unwrap();
        (a, b)
    })
    .await;
    assert!(!a.duplicate);
    assert!(
        !b.duplicate,
        "two manual calls (null external_id) are distinct"
    );
    assert_ne!(a.call_id, b.call_id);
}

// TIP-3 — the routing event is durable: even with the in-proc publish lost (dropping sink), the event is
// staged in the outbox for the relay.
#[tokio::test]
async fn tip3_routing_event_durable_via_outbox() {
    let pool = pool().await;
    let svc = TelephonyWriteService::new(pool.clone());
    let out = scoped(&pool, async {
        svc.record_call(
            &pool,
            cdr(Some(format!("c-{}", Uuid::new_v4()))),
            &DroppingSink,
        )
        .await
        .unwrap()
    })
    .await;
    let staged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM telephony.outbox_events WHERE aggregate_id=$1 AND event_type='CallLogged'")
        .bind(out.call_id.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        staged, 1,
        "CallLogged durably staged despite the lost publish"
    );
}

// TIP-4 — a CDR with ended_at < answered_at (clock skew / out-of-order retransmit) must NOT store a
// negative talk-time (maturity council 2026-07-09). talk_time clamps ≥ 0; the DB CHECK backstops any writer.
#[tokio::test]
async fn tip4_negative_duration_cannot_be_stored() {
    let pool = pool().await;
    let svc = TelephonyWriteService::new(pool.clone());
    let mut c = cdr(Some(format!("c-{}", Uuid::new_v4())));
    // Inverted timestamps: answered after ended.
    c.answered_at = Some(ts(2026, 7, 9, 10, 5, 0));
    c.ended_at = Some(ts(2026, 7, 9, 10, 0, 0));
    let out = scoped(&pool, async {
        svc.record_call(&pool, c, &CapturingSink::new()).await
    })
    .await
    .expect("skewed CDR is clamped, not rejected mid-insert");
    assert!(
        out.duration_seconds >= 0,
        "duration clamped ≥ 0 (got {})",
        out.duration_seconds
    );

    let stored: i32 =
        sqlx::query_scalar("SELECT duration_seconds FROM telephony.calls WHERE id=$1")
            .bind(out.call_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored, 0,
        "a skewed CDR stores zero talk-time, never negative"
    );
}

// TIP-5 — link_call attaches a call to a subject. No tenant argument (ADR-0029): under a composing
// service's fence another unit's call is simply not matched; on the undecorated probe database this
// exercises the domain flip only (record → link → verify the columns).
#[tokio::test]
async fn tip5_link_call_attaches_subject() {
    let pool = pool().await;
    let svc = TelephonyWriteService::new(pool.clone());
    let out = scoped(&pool, async {
        svc.record_call(
            &pool,
            cdr(Some(format!("c-{}", Uuid::new_v4()))),
            &CapturingSink::new(),
        )
        .await
        .unwrap()
    })
    .await;

    let subject_id = Uuid::new_v4();
    svc.link_call(out.call_id, "lead", subject_id)
        .await
        .expect("subject attach succeeds");

    let (st, sid): (Option<String>, Option<Uuid>) =
        sqlx::query_as("SELECT subject_type, subject_id FROM telephony.calls WHERE id=$1")
            .bind(out.call_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(st.as_deref(), Some("lead"));
    assert_eq!(sid, Some(subject_id), "subject is recorded on the call");
}
