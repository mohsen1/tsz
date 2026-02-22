//! Class hierarchy and inheritance graph.
//!
//! Groups class-related solver concerns:
//! - `class_hierarchy`: Class type construction (merging base/derived members)
//! - `inheritance`: Nominal inheritance graph (cycle detection, MRO, transitive closure)

pub mod class_hierarchy;
pub mod inheritance;
