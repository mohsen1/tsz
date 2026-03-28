//! Type interning for structural deduplication.
//!
//! This module implements the type interning engine that converts
//! `TypeData` structures into lightweight `TypeId` handles.
//!
//! Benefits:
//! - O(1) type equality (just compare `TypeId` values)
//! - Memory efficient (each unique structure stored once)
//! - Cache-friendly (work with u32 arrays instead of heap objects)
//!
//! # Concurrency Strategy
//!
//! The `TypeInterner` uses a sharded DashMap-based architecture for lock-free
//! concurrent access:
//!
//! - **Sharded Type Storage**: 64 shards based on hash of `TypeData` to minimize contention
//! - **`DashMap` for Interning**: Each shard uses `DashMap` for lock-free read/write operations
//! - **Arc for Immutability**: Type data is stored in Arc<T> for cheap cloning
//! - **No `RwLock`<Vec<T>>**: Avoids the read-then-write deadlock pattern
//!
//! This design allows true parallel type checking without lock contention.

mod core;
mod intersection;
mod normalize;
mod template;
pub mod type_factory;

// Re-export primary public type from core implementation
pub use self::core::TypeInterner;
pub use self::core::clear_thread_local_cache;
pub(crate) use self::core::{TEMPLATE_LITERAL_EXPANSION_LIMIT, TypeListBuffer};
// Used by intern_tests.rs (included via #[path] below)
#[allow(unused_imports)]
pub(crate) use self::core::PROPERTY_MAP_THRESHOLD;

// Re-export types used by sibling submodules (intersection.rs, normalize.rs, template.rs)
// via `use super::*` or `use super::{...}` patterns
pub(super) use crate::types::{
    CallSignature, CallableShape, IndexSignature, ObjectFlags, ObjectShape, PropertyInfo, TypeData,
    TypeId, Visibility,
};
pub(super) use rustc_hash::FxHashSet;
pub(super) use smallvec::SmallVec;
pub(super) use std::sync::Arc;
pub(super) use tsz_common::interner::Atom;

#[cfg(test)]
use crate::def::DefId;
#[cfg(test)]
use crate::types::*;

#[cfg(test)]
#[path = "../../tests/intern_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/concurrent_tests.rs"]
mod concurrent_tests;

#[cfg(test)]
#[path = "../../tests/intern_normalize_tests.rs"]
mod intern_normalize_tests;
