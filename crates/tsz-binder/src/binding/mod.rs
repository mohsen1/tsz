//! Binder declaration binding and post-binding validation.
//!
//! - `declaration.rs`: declaration binding, accessors, and flow graph construction.
//! - `validation.rs`: post-binding validation, lib symbol diagnostics, and resolution statistics.

pub(crate) mod declaration;
mod validation;

pub(crate) use declaration::SemanticDefDetails;
