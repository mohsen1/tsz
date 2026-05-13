//! Property access type resolution, global augmentation property lookup,
//! and expando function pattern detection.

mod class_recovery;
mod helpers;
mod import_meta;
mod imported_array_to_enum;
mod known_globals;
mod namespace_fallbacks;
mod nullish_access;
mod partial_initializer;
mod resolve;

#[cfg(test)]
mod resolve_tests;
