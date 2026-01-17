//! JavaScript Transforms
//!
//! This module contains transforms that convert TypeScript/ES2015+ code to
//! earlier JavaScript versions (ES5, ES3) for compatibility.
//!
//! The transforms are used by thin_emitter.rs for ES5 downleveling.

pub mod arrow_es5;
pub mod async_es5;
pub mod block_scoping_es5;
pub mod class_es5;
mod emit_utils;
pub mod enum_es5;
pub mod helpers;
pub mod module_commonjs;
pub mod namespace_es5;
pub mod private_fields_es5;

#[cfg(test)]
mod module_commonjs_tests;
