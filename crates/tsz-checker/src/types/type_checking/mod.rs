//! Type checking validation modules.
//!
//! Organized into focused submodules:
//! - `core` — utility methods, AST traversal helpers, member/declaration validation
//! - `declarations` — declaration-specific type checking (variable, function, class)
//! - `declarations_utils` — shared utilities for declaration checking
//! - `duplicate_identifiers` — duplicate identifier/declaration conflict detection
//! - `global` — global-scope type checking
//! - `property_init` — property initializer validation
//! - `type_alias_checking` — type alias declaration checking, type node validation
//! - `unused` — unused variable/parameter detection

mod commonjs_object_exports;
mod core;
mod core_statement_checks;
mod cross_file_conflicts;
mod declarations;
mod declarations_utils;
mod duplicate_identifier_conflict_kinds;
mod duplicate_identifiers;
mod duplicate_identifiers_constructor;
mod duplicate_identifiers_helpers;
mod duplicate_index_signatures;
mod global;
mod indexed_access;
mod property_init;
mod type_alias_checking;
mod unused;
