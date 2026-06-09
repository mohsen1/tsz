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
mod comparison_types;
pub(crate) mod condition_narrowing;
mod condition_nullish;
mod core;
mod flow_dp;
pub(crate) mod narrowing;
mod narrowing_helpers;
mod optional_chain;
mod predicate_resolution;
pub(crate) mod references;
pub(crate) mod type_guards;
mod typeof_exclusions;
pub(crate) mod var_utils;
mod zod_literal_helpers;

pub(crate) use self::core::{
    CallPredicateMap, FLOW_CACHE_STRUCTURAL_ID_LIMIT, FLOW_CACHE_SUPER_BASE_KEY,
    FLOW_CACHE_THIS_BASE_KEY, PredicateSignature, PropertyKey, is_real_binder_symbol,
    is_session_stable_flow_cache_symbol, structural_flow_cache_symbol, symbol_first_identifier_ref,
};
pub use self::core::{FlowAnalyzer, FlowGraph};
