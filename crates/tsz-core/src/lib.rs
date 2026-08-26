//! Clean-slate TypeScript compiler engine.
//!
//! A source revision is parsed and bound once; semantic types belong to one
//! checker universe; every deferred query has one forcing owner; parallel
//! phases return deterministic values that are merged only at phase barriers.

pub mod bind;
pub mod config;
pub mod diagnostics;
pub mod emit;
mod emit_paths;
pub mod host;
pub mod program;
pub mod project_graph;
mod semantics;
pub mod service;
pub mod source;
pub mod standard_library;
pub mod syntax;
mod text;

pub use program::{
    CompileExitStatus, CompileOutput, Compiler, CompilerOptions, EmittedFile, SemanticCompletion,
    SourceInput,
};
