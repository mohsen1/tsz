//! JSDoc subsystem — parsing, resolution, params, and diagnostics.
//!
//! # Module layout
//!
//! | Module        | Responsibility                                        |
//! |---------------|-------------------------------------------------------|
//! | `types`       | Shared data structures (`JsdocTypedefInfo`, etc.)     |
//! | `parsing`     | Pure string-level parsing (no `&self`/`&mut self`)    |
//! | `resolution`  | **Authoritative reference-resolution kernel** +       |
//! |               | type expression → `TypeId` resolution                 |
//! | `lookup`      | AST annotation lookup, metadata, scoping helpers      |
//! | `params`      | `@param` tag validation, comment finding, text parse  |
//! | `diagnostics` | Typedef/satisfies diagnostic emission                 |
//!
//! # Reference resolution kernel
//!
//! `resolution::resolve_jsdoc_reference()` is the ONE authoritative entry
//! point for resolving JSDoc type names/expressions to `TypeId`. It handles:
//! - typedef lookup
//! - import type lookup (`import("module").Member`)
//! - template parameter scope lookup
//! - callback/typedef reference resolution
//!
//! **All callers must use `resolve_jsdoc_reference`** instead of re-deriving
//! the resolution chain. The `resolve_jsdoc_type_str` alias exists for
//! backward compatibility but delegates to the kernel.
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
pub(crate) mod diagnostics_imports;
pub(crate) mod diagnostics_templates;
pub(crate) mod lookup;
pub(crate) mod params;
pub(crate) mod params_generic_instantiation;
pub(crate) mod parsing;
pub(crate) mod resolution;
pub(crate) mod types;
