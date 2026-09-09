//! Route-level proof that the validated write path is wired (council 2026-08-05, move #1).
//!
//! Posts CDRs through `TelephonyModule::validated_routes()` in-process (tower::oneshot) and asserts
//! the same idempotency the engine guarantees directly: a first post records, a replayed provider
//! CDR returns `duplicate: true`. This proves the HTTP surface reaches `TelephonyWriteService`,
//! not the unguarded generic CRUD it replaced.
//!
//! Tenancy (ADR-0029): request bodies carry no tenant key. Each post runs under an org request
//! scope — the durable-event path stages into the still-company-keyed outbox, so the deployment
//! (here, the probe) must supply the scope a composing service's auth middleware would.

mod common;
use common::*;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use backbone_telephony::TelephonyModule;
use tower::ServiceExt;
use uuid::Uuid;

/// POST a JSON CDR to the router, return (status, body).
async fn post_cdr(
    router: axum::Router,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method("POST")
        .uri("/calls")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

fn cdr_json(ext: &str, status: &str) -> serde_json::Value {
    serde_json::json!({
        "direction": "inbound",
        "fromNumber": "+628111",
        "toNumber": "+628999",
        "externalId": ext,
        "status": status,
        "startedAt": "2026-08-05T10:00:00Z",
        "answeredAt": if status == "completed" { Some("2026-08-05T10:00:10Z") } else { None },
        "endedAt": if status == "missed" { Some("2026-08-05T10:00:20Z") } else { Some("2026-08-05T10:03:10Z") },
    })
}

// VWR-1 — a provider CDR posted to validated_routes() records and reports duplicate=false.
#[tokio::test]
async fn vwr1_first_post_records() {
    let pool = pool().await;
    let module = TelephonyModule::builder()
        .with_database(pool.clone())
        .build()
        .unwrap();
    let (status, json) = scoped(&pool, async {
        post_cdr(
            module.validated_routes(),
            cdr_json(&format!("c-{}", Uuid::new_v4()), "completed"),
        )
        .await
    })
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "validated write path serves the ingest"
    );
    assert_eq!(json["duplicate"], false, "first post is not a duplicate");
}

// VWR-2 — a redelivered provider CDR (same externalId) is idempotent: duplicate=true.
#[tokio::test]
async fn vwr2_replay_is_idempotent() {
    let pool = pool().await;
    let module = TelephonyModule::builder()
        .with_database(pool.clone())
        .build()
        .unwrap();
    let ext = format!("c-{}", Uuid::new_v4());

    let (s1, j1) = scoped(&pool, async {
        post_cdr(module.validated_routes(), cdr_json(&ext, "completed")).await
    })
    .await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(j1["duplicate"], false);

    // Fresh router from the same module — the dedup is in the DB, not router state.
    let (s2, j2) = scoped(&pool, async {
        post_cdr(module.validated_routes(), cdr_json(&ext, "completed")).await
    })
    .await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(j2["duplicate"], true, "replayed CDR is idempotent");
    assert_eq!(
        j1["callId"], j2["callId"],
        "idempotent replay returns the original call id"
    );
}

// VWR-3 — a missed inbound call is accepted by the validated path (the MissedCall event it raises
// is the CRM seam, exercised in the workspace-level telephony-crm-seam crate).
#[tokio::test]
async fn vwr3_missed_call_accepted() {
    let pool = pool().await;
    let module = TelephonyModule::builder()
        .with_database(pool.clone())
        .build()
        .unwrap();
    let (status, json) = scoped(&pool, async {
        post_cdr(
            module.validated_routes(),
            cdr_json(&format!("c-{}", Uuid::new_v4()), "missed"),
        )
        .await
    })
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        json["duration_seconds"], 0,
        "a missed call records zero talk-time"
    );
}
