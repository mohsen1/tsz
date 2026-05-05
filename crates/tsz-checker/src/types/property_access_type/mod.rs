//! Property access type resolution, global augmentation property lookup,
//! and expando function pattern detection.

mod class_recovery;
mod helpers;
mod known_globals;
mod partial_initializer;
mod resolve;

#[cfg(test)]
mod resolve_tests;
