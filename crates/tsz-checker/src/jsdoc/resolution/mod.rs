//! JSDoc type annotation resolution — converting JSDoc type expressions to `TypeId`.
//!
//! This module owns the **authoritative JSDoc reference-resolution kernel**:
//!
//! - `resolve_jsdoc_reference` — the ONE canonical entry point for resolving
//!   any JSDoc type expression to a `TypeId`. All callers should use this
//!   instead of re-deriving the resolution chain.
//!
//! Internal resolution components (called by the kernel, not directly):
//! - Type expression parsing (`jsdoc_type_from_expression`)
//! - Type name resolution (`resolve_jsdoc_type_name`)
//! - Typedef resolution (`resolve_jsdoc_typedef_type`, `type_from_jsdoc_typedef`)
//! - Symbol resolution (`resolve_jsdoc_symbol_type`, `resolve_jsdoc_entity_name_symbol`)
//! - Generic instantiation (`resolve_jsdoc_generic_type`)
//! - Import type resolution (`resolve_jsdoc_import_type_reference`)

mod name_resolution;
mod type_construction;
