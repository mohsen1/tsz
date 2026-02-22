//! AST node binding, hoisting, scope management, and name collection.
//!
//! Groups all node-level binder logic:
//! - `binding` — AST node binding dispatch, hoisting, scope/container management
//! - `names` — name collection utilities, identifier extraction, modifier helpers

mod binding;
mod names;
