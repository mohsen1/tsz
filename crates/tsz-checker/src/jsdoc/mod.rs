//! JSDoc subsystem — parsing, resolution, params, and diagnostics.
//!
//! # Module layout
//!
//! | Module        | Responsibility                                        |
//! |---------------|-------------------------------------------------------|
//! | `types`       | Shared data structures (`JsdocTypedefInfo`, etc.)     |
//! | `parsing`     | Pure string-level parsing (no `&self`/`&mut self`)    |
//! | `resolution`  | Type expression → `TypeId` resolution                 |
//! | `lookup`      | AST annotation lookup, metadata, scoping helpers      |
//! | `params`      | `@param` tag validation, comment finding, text parse  |
//! | `diagnostics` | Typedef/satisfies diagnostic emission                 |
//!
//! # Architecture guard
//!
//! New JSDoc work belongs in this subsystem, not in `types/utilities/`.
//! If you are adding JSDoc functionality:
//! - Pure string parsing → `parsing.rs`
//! - Type resolution (needs `&mut self`) → `resolution.rs`
//! - Parameter handling / comment lookup → `params.rs`
//! - Diagnostic emission → `diagnostics.rs`
//! - New data structures → `types.rs`

pub(crate) mod diagnostics;
pub(crate) mod lookup;
pub(crate) mod params;
pub(crate) mod parsing;
pub(crate) mod resolution;
pub(crate) mod types;
