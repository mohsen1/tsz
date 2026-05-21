//! JSX props/attribute checking: attribute type-checking (TS2322), spread property
//! validation, union props checking, and missing required props (TS2741).
//!
//! Props extraction lives in `extraction.rs`, overload resolution in `overloads.rs`.

mod attr_value;
mod generic_spread;
pub(crate) mod resolution;
mod special_attribute_callbacks;
mod synthesized_display;
mod validation;
