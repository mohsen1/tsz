//! Diagnostic core types for the solver.
//!
//! This module defines the core data types for type checking diagnostics:
//!
//! - **Tracer pattern** (`SubtypeTracer`, `DynSubtypeTracer`): Zero-cost
//!   abstraction for tracing subtype check failures without logic drift.
//! - **Failure reasons** (`SubtypeFailureReason`): Structured enum capturing all the
//!   ways a subtype check can fail.
//! - **Lazy diagnostics** (`PendingDiagnostic`, `DiagnosticArg`): Deferred formatting
//!   to avoid expensive `type_to_string()` calls in tentative contexts.
//! - **Diagnostic codes** (`codes`): TypeScript error code aliases.
//! - **Data types** (`TypeDiagnostic`, `SourceSpan`, etc.): Core diagnostic structures.
//!
//! For eagerly-rendered diagnostic builders, see [`builders`].

pub mod builders;
mod core;
pub mod format;
pub mod reduce;

pub use self::core::*;
