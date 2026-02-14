//! Type definitions module.
//!
//! This module contains all type-related definitions for the type checker.
pub mod diagnostics;
pub mod flags;
pub mod type_def;

// Re-export commonly used items
pub use diagnostics::{Diagnostic, DiagnosticCategory};
