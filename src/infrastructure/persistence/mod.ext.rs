// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// The hand-written Call SQL's parameter/projection types (see `call_repository`, which is declared
// `user_owned` in metaphor.codegen.yaml).
pub use call_repository::{CallOutcomeRow, NewCallRow};
