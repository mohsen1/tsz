//! Core implementation of the type interning engine.
//!
//! Split into submodules:
//! - `interner`: Data structures, sharded storage, and core `TypeInterner` methods
//! - `constructors`: Type construction convenience methods (literal, union, etc.)

mod constructors;
mod interner;

// Re-export everything that was previously public from core.rs
pub use interner::TypeInterner;
pub use interner::clear_thread_local_cache;
pub(crate) use interner::{
    PROPERTY_MAP_THRESHOLD, TEMPLATE_LITERAL_EXPANSION_LIMIT, TypeListBuffer,
};
