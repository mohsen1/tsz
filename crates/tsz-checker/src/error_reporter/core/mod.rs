//! Core error emission helpers and type formatting utilities.
//!
//! Split into submodules for maintainability:
//! - `type_display`: type normalization, formatting, and display helpers
//! - `diagnostic_source`: diagnostic source/target expression analysis

mod diagnostic_source;
mod identifier_source_display;
mod type_display;
