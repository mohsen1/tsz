//! Context objects for the emitter pipeline.
//!
//! - [`EmitContext`]: Transform state management (block scoping, private fields, helpers).
//! - [`EmitPlan`]: File-level direct-to-target plan consumed by the printer.
//! - [`TransformContext`]: Projection layer that stores transform directives for the print pass.

pub mod emit;
pub mod plan;
pub mod target_facts;
pub mod transform;

pub use emit::*;
pub use plan::*;
pub use target_facts::*;
pub use transform::*;
