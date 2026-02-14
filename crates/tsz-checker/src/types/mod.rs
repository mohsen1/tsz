//! Type definitions module.
//!
//! This module contains all type-related definitions for the type checker.
#![allow(dead_code, unused_imports)]

pub mod diagnostics;
pub mod flags;
pub mod type_def;

// Re-export commonly used items
pub use diagnostics::{Diagnostic, DiagnosticCategory};
