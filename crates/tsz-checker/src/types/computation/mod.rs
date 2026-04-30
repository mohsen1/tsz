//! Type computation for `CheckerState`.
//!
//! This module group handles all expression-level type computation:
//! - `array_literal` — array literal type construction and contextual typing
//! - `assignment_target` — assignment target write-surface helpers
//! - `helpers` — foundational helpers, contextual typing, relationship queries
//! - `access` — property/element access type resolution
//! - `binary` — binary expression operators
//! - `call` — call expression resolution and overload handling
//! - `call_display` — display skeleton and constructor-propagation helpers for calls
//! - `call_helpers` — shared helpers for call/new expressions
//! - `complex` — new expression type computation core
//! - `complex_new_target` — new expression target validation and abstract constructor detection
//! - `complex_js_constructor` — JS constructor instance type synthesis
//! - `identifier` — identifier reference resolution
//! - `identifier_flow` — flow-based helpers for identifier type computation (evolving arrays, implicit any)
//! - `object_literal` — object literal type construction
//! - `object_literal_context` — contextual property type resolution helpers for object literals
//! - `tagged_template` — tagged template expression type resolution

pub(crate) mod access;
pub(crate) mod access_await;
pub(crate) mod access_helpers;
pub(crate) mod access_super;
pub(crate) mod array_literal;
pub(crate) mod assignment_target;
pub(crate) mod binary;
pub(crate) mod call;
pub(crate) mod call_display;
pub(crate) mod call_finalize;
pub(crate) mod call_helpers;
pub(crate) mod call_inference;
pub(crate) mod call_result;
pub(crate) mod complex;
pub(crate) mod complex_constructors;
pub(crate) mod complex_js_constructor;
pub(crate) mod complex_new_target;
pub(crate) mod contextual;
pub mod helpers;
pub(crate) mod identifier;
pub(crate) mod identifier_flow;
pub(crate) mod object_literal;
pub(crate) mod object_literal_circularity;
pub(crate) mod object_literal_context;
pub(crate) mod object_literal_support;
pub(crate) mod tagged_template;
pub(crate) mod type_operators;
