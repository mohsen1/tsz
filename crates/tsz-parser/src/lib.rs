//! TypeScript parser and AST types for the tsz compiler.
//!
//! This crate provides:
//! - AST node types and `NodeArena` for cache-optimized storage
//! - `ParserState` - Recursive descent parser
//! - Syntax utilities for AST manipulation

pub mod parser;

// Syntax utilities - Shared helpers for AST and transforms
pub mod syntax;

// Re-export key parser types at crate root for convenience
pub use parser::base::{NodeIndex, NodeList, TextRange};
pub use parser::flags::{modifier_flags, node_flags, transform_flags};
pub use parser::node::NodeArena;
pub use parser::state::{ParseDiagnostic, ParserState};
pub use parser::syntax_kind_ext;
