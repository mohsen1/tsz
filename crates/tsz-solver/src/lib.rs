//! TypeScript type solver for the tsz compiler.
//!
//! This crate provides the query-based structural type system:
//! - `TypeInterner` - Interned type storage
//! - Type resolution and inference
//! - Subtype checking
//! - Type narrowing
//! - Union/intersection reduction
