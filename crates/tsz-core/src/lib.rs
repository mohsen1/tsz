//! Clean-slate TypeScript compiler engine.
//!
//! A source revision is parsed and bound once; semantic types belong to one
//! checker universe; every deferred query has one forcing owner; parallel
//! phases return deterministic values that are merged only at phase barriers.

pub mod bind;
pub mod diagnostics;
pub mod emit;
pub mod program;
pub mod semantics;
pub mod service;
pub mod source;
pub mod standard_library;
pub mod syntax;

pub use program::{CompileOutput, Compiler, CompilerOptions, EmittedFile, SourceInput};
