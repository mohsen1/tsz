//! Type analysis: qualified name resolution, symbol type computation,
//! type queries, and contextual literal type analysis.

pub(crate) mod computed;
pub(crate) mod computed_helpers;
mod core;
pub(crate) mod cross_file;
