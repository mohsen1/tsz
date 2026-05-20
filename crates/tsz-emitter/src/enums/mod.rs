//! Enum Support Module
//!
//! This module provides comprehensive support for TypeScript enums including:
//!
//! - Numeric enums with auto-incrementing values
//! - String enums
//! - Const enums with value inlining
//! - Ambient (declare) enums
//! - Computed enum member evaluation
//! - Reverse mappings for numeric enums
//!
//! # Example
//!
//! ```typescript
//! // Numeric enum - supports reverse mapping
//! enum Direction { Up, Down, Left, Right }
//!
//! // String enum - no reverse mapping
//! enum Color { Red = "RED", Green = "GREEN" }
//!
//! // Const enum - inlined at usage sites
//! const enum Flags { None = 0, Read = 1, Write = 2 }
//! ```

pub mod checker;
pub mod evaluator;
pub mod transform;

pub use checker::EnumChecker;
pub use evaluator::{EnumEvaluator, EnumValue};
pub use transform::EnumTransformer;
