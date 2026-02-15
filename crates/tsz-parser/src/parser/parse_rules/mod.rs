//! Parsing rule modules
//!
//! This module contains extracted parsing logic organized by category.
//! Each module focuses on a specific aspect of parsing (expressions, statements, etc.)
//!
//! ## Design Notes
//!
//! Expression parsing logic is implemented directly in `state.rs` using methods
//! on `ParserState` for optimal performance and simpler control flow. The precedence
//! climbing algorithm for binary expressions and all primary/unary expression parsing
//! are integrated into the main parser state.
//!
//! JSX fragment detection (`<>`) is performed inline during parsing rather than via
//! lookahead for efficiency - no backtracking is needed when we can check for `>`
//! immediately after consuming `<`.

mod utils;

pub use utils::{
    is_identifier_or_keyword, look_ahead_is, look_ahead_is_abstract_declaration,
    look_ahead_is_async_declaration, look_ahead_is_const_enum, look_ahead_is_import_call,
    look_ahead_is_import_equals, look_ahead_is_module_declaration,
    look_ahead_is_type_alias_declaration,
};
