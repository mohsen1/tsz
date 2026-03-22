//! Parallel Processing Module
//!
//! Provides parallel file parsing, binding, skeleton extraction,
//! symbol merging, and type checking using Rayon.

mod core;
pub mod skeleton;

// Re-export everything from core for backward compatibility
pub use self::core::*;
pub use skeleton::*;
