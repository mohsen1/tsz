//! Symbol resolution for LSP operations.
//!
//! The Binder maps declaration nodes to symbols, but LSP needs to resolve
//! identifier *usages* to symbols as well. This module provides a lightweight
//! scope walker that reconstructs scope chains on demand.

mod children;
mod core;

pub use self::core::{ScopeCache, ScopeCacheStats, ScopeWalker};

#[cfg(test)]
#[path = "../../tests/resolver_tests.rs"]
mod resolver_tests;
