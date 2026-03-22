//! Parallel Processing Module
//!
//! Provides parallel file parsing, binding, skeleton extraction,
//! symbol merging, and type checking using Rayon.

mod core;
pub mod dep_graph;
pub mod residency;
pub mod skeleton;

// Re-export everything from submodules for backward compatibility
pub use self::core::*;
pub use dep_graph::*;
pub use residency::*;
pub use skeleton::*;
