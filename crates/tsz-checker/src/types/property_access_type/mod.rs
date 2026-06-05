//! Property access type resolution, global augmentation property lookup,
//! and expando function pattern detection.

mod class_recovery;
mod enum_namespace_access;
mod helpers;
mod identifier_resolution;
mod imported_array_to_enum;
pub(crate) mod known_globals;
mod nullish_access;
mod optional_chain_cache;
mod optional_fast_path;
mod partial_initializer;
mod resolve;

#[cfg(test)]
mod resolve_tests;
