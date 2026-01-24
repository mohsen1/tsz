//! Transform Emitter Trait
//!
//! This module defines a trait-based interface for transform emitters to break
//! the circular dependency between emitter and transforms.
//!
//! # Architecture
//!
//! **Problem (Before):**
//! ```text
//! emitter/mod.rs imports transforms::class_es5::ClassES5Emitter
//! transforms/* imports from emitter
//! ```
//! This creates a circular dependency.
//!
//! **Solution (After):**
//! ```text
//! emitter/mod.rs imports transforms::ClassES5Emitter (re-export)
//! transforms/mod.rs re-exports concrete types
//! No direct dependency on internal submodules!
//! ```

use crate::emit_context::EmitContext;
use crate::parser::{NodeArena, NodeIndex};

/// Trait for transform emitters that can emit transformed JavaScript code
///
/// This trait allows the emitter to work with transform emitters without
/// depending on concrete types, breaking the circular dependency between
/// emitter and transforms.
///
/// # Example
///
/// ```ignore
/// impl TransformEmitter for ClassES5Emitter<'_> {
///     fn emit(&mut self, node: NodeIndex) -> Option<String> {
///         // Transform class to ES5 and emit JavaScript
///     }
/// }
/// ```
pub trait TransformEmitter {
    /// Emit transformed JavaScript for a given AST node
    ///
    /// Returns `Some(String)` with the emitted JavaScript if this emitter
    /// handles the node type, or `None` if the node should be emitted differently.
    ///
    /// # Parameters
    /// - `node`: The AST node index to emit
    ///
    /// # Returns
    /// - `Some(String)`: Emitted JavaScript code
    /// - `None`: This emitter doesn't handle this node type
    fn emit(&mut self, node: NodeIndex) -> Option<String>;

    /// Check if this emitter can handle the given node
    ///
    /// This is a fast check that can be used before calling `emit()` to avoid
    /// unnecessary work.
    ///
    /// # Parameters
    /// - `arena`: The node arena
    /// - `node`: The AST node index to check
    ///
    /// # Returns
    /// - `true`: This emitter can handle the node
    /// - `false`: This emitter cannot handle the node
    fn can_emit(&self, arena: &NodeArena, node: NodeIndex) -> bool;

    /// Get the emit context
    ///
    /// Returns a reference to the emit context, which contains compilation
    /// options like target version and module kind.
    fn context(&self) -> &EmitContext;
}

/// Factory for creating transform emitters
///
/// This trait allows the emitter to create transform emitters without
/// knowing their concrete types.
pub trait TransformEmitterFactory {
    type Emitter: TransformEmitter;

    /// Create a new transform emitter
    ///
    /// # Parameters
    /// - `arena`: The node arena
    /// - `ctx`: The emit context
    ///
    /// # Returns
    /// A new transform emitter instance
    fn create(arena: &NodeArena, ctx: &EmitContext) -> Self::Emitter;
}
