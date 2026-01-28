//! TypeScript API Compatibility Layer
//!
//! This module exposes TypeScript-compatible APIs via wasm-bindgen,
//! allowing TypeScript's test harness to run against tsz.
//!
//! # Architecture
//!
//! The API is organized to match TypeScript's structure:
//! - `TsProgram` - Program interface (createProgram equivalent)
//! - `TsTypeChecker` - TypeChecker interface
//! - `TsSourceFile` - SourceFile interface with AST access
//! - `TsNode` - AST node interface
//! - `TsType` / `TsSymbol` - Type system interfaces
//!
//! # Handle-Based Design
//!
//! Objects are stored in Rust and exposed to JS via handles (u32).
//! This maintains:
//! - Object identity (same handle = same object)
//! - Memory efficiency (no duplication)
//! - Lazy evaluation (compute on demand)
//!
//! # Example Usage (JavaScript)
//!
//! ```javascript
//! import { TsProgram, createTsProgram } from 'tsz-wasm';
//!
//! const program = createTsProgram(['file.ts'], options, host);
//! const sourceFiles = program.getSourceFiles();
//! const checker = program.getTypeChecker();
//! const type = checker.getTypeAtLocation(node);
//! console.log(checker.typeToString(type));
//! ```

pub mod ast;
pub mod diagnostics;
pub mod emit;
pub mod enums;
pub mod language_service;
pub mod program;
pub mod source_file;
pub mod type_checker;
pub mod types;
pub mod utilities;

// Re-export main types
pub use diagnostics::TsDiagnostic;
pub use program::TsProgram;
pub use source_file::TsSourceFile;
pub use type_checker::TsTypeChecker;
pub use types::{TsSignature, TsSymbol, TsType};
