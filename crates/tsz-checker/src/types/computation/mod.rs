//! Type computation for `CheckerState`.
//!
//! This module group handles all expression-level type computation:
//! - `helpers` — foundational helpers, contextual typing, relationship queries
//! - `access` — property/element access type resolution
//! - `binary` — binary expression operators
//! - `call` — call expression resolution and overload handling
//! - `call_helpers` — shared helpers for call/new expressions
//! - `complex` — complex expression type computation (conditional, etc.)
//! - `identifier` — identifier reference resolution
//! - `object_literal` — object literal type construction
//! - `tagged_template` — tagged template expression type resolution

pub(crate) mod access;
pub(crate) mod binary;
pub(crate) mod call;
pub(crate) mod call_helpers;
pub(crate) mod complex;
pub mod helpers;
pub(crate) mod identifier;
pub(crate) mod object_literal;
pub(crate) mod tagged_template;
