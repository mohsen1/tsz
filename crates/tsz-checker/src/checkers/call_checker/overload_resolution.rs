//! Overload resolution for call expressions.
//!
//! Split from the parent `call_checker` module — pure code motion.

mod contextual_retry;
mod helpers;
mod mismatch_helpers;
mod resolve_signatures;
mod retry_state;
mod return_context;

// Re-exported for the child submodules `return_context` and `contextual_retry`,
// which reference these via `super::`. The overload-resolution methods
// themselves live in the `resolve_signatures` child module; the speculation
// snapshot/rollback helpers live alongside the other speculative-call rollback
// helpers in `diagnostics.rs`.
use super::{CallableContext, SelectedTypePredicate};
