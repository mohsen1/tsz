//! Output generation for the tsz emitter.
//!
//! This module groups the output layer:
//! - [`printer`]: High-level AST-to-JavaScript printing interface
//! - [`source_writer`]: Low-level text buffer with source map tracking

pub mod printer;
pub mod source_writer;
