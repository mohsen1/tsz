//! Context objects for the emitter pipeline.
//!
//! - [`EmitContext`]: Transform state management (block scoping, private fields, helpers).
//! - [`TransformContext`]: Projection layer that stores transform directives for the print pass.

pub mod emit;
pub mod transform;

pub use emit::*;
pub use transform::*;
