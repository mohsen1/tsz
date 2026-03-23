//! Type analysis: qualified name resolution, symbol type computation,
//! type queries, and contextual literal type analysis.

pub(crate) mod computed;
mod computed_alias;
mod computed_commonjs;
pub(crate) mod computed_helpers;
mod computed_helpers_binding;
mod computed_helpers_private;
mod computed_loops;
mod core;
mod core_type_query;
pub(crate) mod cross_file;
mod symbol_type_helpers;
