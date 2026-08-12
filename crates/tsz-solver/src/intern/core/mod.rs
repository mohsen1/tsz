//! Core implementation of the type interning engine.
//!
//! Split into submodules:
//! - `interner`: `TypeInterner` struct, intern/lookup hot paths, and component
//!   accessors (further split into `storage`, `display`, and `cache`)
//! - `constructors`: Type construction convenience methods (literal, union, etc.)

mod constructors;
mod interner;

// Re-export everything that was previously public from core.rs
#[cfg(test)]
pub(crate) use interner::PROPERTY_MAP_THRESHOLD;
pub use interner::SharedDefVariance;
pub use interner::TypeInterner;
pub(crate) use interner::apply_arity_optional_display_mask;
pub use interner::clear_thread_local_cache;
pub(crate) use interner::{PredicateCacheKind, TEMPLATE_LITERAL_EXPANSION_LIMIT, TypeListBuffer};
