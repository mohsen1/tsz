//! Binder declaration binding and post-binding validation.
//!
//! - `declaration.rs`: declaration binding, accessors, and flow graph construction.
//! - `validation.rs`: post-binding validation, lib symbol diagnostics, and resolution statistics.

mod accessors_flow;
pub(crate) mod declaration;
mod expression_flow;
mod semantic_defs;
pub(crate) mod stack_guard;
mod validation;

pub(crate) use declaration::SemanticDefDetails;
