//! Control Flow Analysis for type narrowing.
//!
//! This module provides flow-sensitive type analysis that walks the control flow
//! graph backwards from identifier usages to determine narrowed types.
//!
//! Example:
//! ```typescript
//! function foo(x: string | number) {
//!     if (typeof x === "string") {
//!         // FlowAnalyzer walks back and sees TRUE_CONDITION (typeof x === "string")
//!         // Returns: string (narrowed from string | number)
//!         console.log(x.length);
//!     } else {
//!         // FlowAnalyzer sees FALSE_CONDITION
//!         // Returns: number
//!         console.log(x.toFixed(2));
//!     }
//! }
//! ```

pub(crate) mod alias_narrowing;
pub(crate) mod assignment;
mod assignment_fallback;
mod call_condition_narrowing;
pub(crate) mod condition_narrowing;
mod core;
pub(crate) mod narrowing;
mod narrowing_helpers;
pub(crate) mod references;
pub(crate) mod type_guards;
pub(crate) mod var_utils;

pub(crate) use self::core::{CallPredicateMap, PredicateSignature, PropertyKey};
pub use self::core::{FlowAnalyzer, FlowGraph};
