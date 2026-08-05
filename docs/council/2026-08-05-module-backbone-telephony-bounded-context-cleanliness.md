---
date: 2026-08-05
repo_type: module
unit: backbone-telephony
focus: bounded-context-cleanliness
roster: chair, skeptic, steelman, yagni-business, ddd-bounded-context, contract-seat, domain-expert(telephony)
---

# Council — module:backbone-telephony — focus: bounded-context-cleanliness

## Best call
**Wire `TelephonyWriteService` into the public API by mounting `create_call_write_routes` (or a successor guarded router) on the production Router inside `TelephonyModuleBuilder::build`, and STOP exporting the unguarded `all_crud_routes()` as a write surface.**

The regen was SAFE — no custom logic was lost; the two removals were dead code (`pub mod value_objects;`, generated `TelephonyQueryService` trait), the 5 `user_owned` files are intact, and the regen actually *fixed* a real bug (the missing `pub mod exports;` in lib.rs at HEAD). So the answer to question (2) is: regen is safe, nothing diverged, no custom code destroyed. The answer to question (1) is: the module is NOT complete. Its entire business value — the validated write engine hardened across v0.3.0→v0.4.1 (ADR-0008 scope-contract + ADR-0011 outbox parity) — is pub-exported at `src/application/service/mod.rs:22` but referenced NOWHERE outside tests. A provider webhook hitting `POST /calls` today flows through `all_crud_routes()` → generic `BackboneCrudHandler` over `CallService` and SKIPS dedup, the outbox event, the subject link, and `company_id`/RLS fencing. The deprecated `routes()` points callers toward a "guarded router for production" that DOES NOT EXIST. The module ships a read API plus an unguarded admin shortcut masquerading as a bounded context.

- Residual negative value: ~4 releases of hardening (ADR-0008, ADR-0011, the dedup probe, integrity guards, outbox staging) currently deliver ZERO production value; every live write bypasses them. The `unwire + de-export all_crud_routes` half is reversible in minutes; the `wire TelephonyWriteService behind create_call_write_routes` half is where the real risk sits (~1–2 days to mount, route, and add a handler smoke test). Risk surface added: one new axum mount + one route fn actually invoked; net coupling unchanged because `TelephonyWriteService` already depends only on types already in the crate.
- Reversibility: easy. The wiring is additive behind an already-exported route fn; if it misbehaves, unmount it and you are back to the (broken) status quo — no schema or migration change.
- What would flip this: evidence of a SECOND consumer that already constructs `TelephonyWriteService` outside this repo's tests (e.g. a `backend-service` project calling `TelephonyWriteService::new(...)` directly, bypassing `routes()`). The in-tree public surface has none. If one exists downstream, the call downgrades to "delete `all_crud_routes()` from the write surface."

## Disagreement map

1. **Crux: what is the #1 blocker this month?** yagni-business holds that the missing `backbone-crm` workspace checkout (blocks ALL local check/build/test) is the gating issue. Skeptic/contract-seat hold that the unwired write engine is the root defect — CRM-coupling is a resolved-by-design async seam and the checkout issue is a tenanting chore, not a design flaw. **Chair sides with skeptic:** the CRM dep is test-only and architecturally correct; the unwired engine is the actual bounded-context break.

2. **Crux: is `routes()`-deprecation-without-replacement a contract bug or an in-progress refactor?** contract-seat calls it a self-contradictory public contract (deprecated pointer to a nonexistent guarded router). ddd-bounded-context reads it as a tightening in flight. **Chair sides with contract-seat:** the replacement does not exist, so today the deprecation is a lie — fixable only by shipping the replacement (the Best call).

3. **Crux: severity of the 2 compiled example modules.** ddd-bounded-context treats `example_dto.rs` + `example_saga_workflow.rs` (declared in `dto/mod.rs:8` and `workflows/mod.rs:1`) as load-bearing bounded-context pollution. yagni-business ranks them below the write-engine and the CRM checkout. **Chair sides with yagni-business on ordering** (these are cruft, not correctness), but they DO ship in the `.rlib` and are in-scope for this lens — parked in the table, not the Best call.

## Recommendations (ranked by leverage)

| # | Move | Leverage | Residual negative | Reversibility | Evidence to flip |
|---|------|----------|-------------------|---------------|------------------|
| 1 | **Mount `TelephonyWriteService` behind the production Router in `build()`; remove `all_crud_routes()` from the write surface (keep GETs).** | Restores the entire domain model to the live path; makes ADR-0008 + ADR-0011 actually enforce. | ~1–2 days to mount + smoke-test one handler; new route fn invoked in prod (small blast radius). | Easy (additive). | A downstream consumer found that already constructs `TelephonyWriteService` directly. |
| 2 | **Fix the contract lie: either un-deprecate `routes()` or make it return the guarded write router from move #1.** | Removes the self-contradiction contract-seat flagged; callers get one obvious path. | None beyond move #1. | Easy. | N/A — depends on #1. |
| 3 | **Delete the 2 compiled example modules (`example_dto.rs`, `example_saga_workflow.rs`) and their `mod` declarations; drop the 9 other orphaned example/value_objects files.** | Stops shipping non-telephony code in the telephony `.rlib`; shrinks the bounded context to Call. | Loses scaffold reference (still in codegen templates, so recoverable). | Easy. | If any consumer `use`s an example type (none found in-tree). |
| 4 | **Add `.down.sql` for the 3 up-only migrations.** | Restores reversibility that earlier migrations already have; matches migration-specialist norms. | Time to write tested downs (~½ day). | Costly but correct. | A migration that is genuinely irreversible (audit triggers may be). |
| 5 | **Resolve the `backbone-crm` workspace checkout so `cargo` resolves locally.** | Unblocks local build/test for every contributor right now. | None design-wise; it is a tenanting fix. | Easy. | N/A — pure environment fix. |

## Maturity scorecard
(SKIP — focus is bounded-context-cleanliness.)

## Parking lot
- `backbone-crm` [dev-dependencies] missing-checkout blocking local builds — real but a workspace-tenanting issue, not a bounded-context defect; the async-event seam (not a Cargo edge) is the correct design.
- 11 orphaned example/value_objects files unreachable from `lib.rs` — cleanup hygiene, downstream of move #3.
- `exports/services.rs` holds an empty CUSTOM section — fine to leave; it is the extension point for future CRM-facing ports.
- The `TelephonyWriteService`-from-tests-only pattern suggests the integration tests are the de-facto spec for the engine; consider promoting one to a golden-case regression once it is wired.

## Relevant files
- `src/application/service/mod.rs` (line 22 — the orphaned `pub use TelephonyWriteService`)
- `src/lib.rs` (`TelephonyModuleBuilder::build` — engine not constructed; `pub mod exports;` now wired by regen)
- `src/presentation/http/call_handler.rs` (line 141 — `create_call_write_routes` exported but uncalled)
- `src/presentation/versioning/version_router.rs` (the deprecated `routes()` with no guarded replacement)
- `src/application/dto/mod.rs` (line 8 — compiled `example_dto`) and `src/application/workflows/mod.rs` (line 1 — compiled `example_saga_workflow`)
- The 5 intact `user_owned` files under `src/application/service/` and `src/infrastructure/persistence/`
