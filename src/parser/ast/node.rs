```rust
//! @deprecated  This module is being deprecated in favor of a more granular AST definition.
//! The `Node` enum and `FatNode` logic will be removed in future versions.
//! Currently acting as a compatibility layer.

// Re-export the new specific node types to maintain compatibility for consumers
// still relying on `ast::node::*`.
pub use crate::parser::ast::expression::{
    BinaryOperator, Expression, Identifier, Literal,
};
pub use crate::parser::ast::statement::{Block, Statement};

/// @deprecated
/// The legacy `Node` enum. Do not use for new features.
/// This is temporarily stubbed to aid in the migration to the new AST structure.
///
/// Prefer using the specific `Expression` or `Statement` enums directly.
#[deprecated(since = "0.2.0", note = "Use `Expression` or `Statement` directly")]
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    // Stub variants to minimize breaking changes during the transition period.
    Statement(Statement),
    Expression(Expression),
}

impl Node {
    /// @deprecated Stub method to maintain API surface.
    #[deprecated(since = "0.2.0", note = "Use the inner types directly")]
    pub fn span(&self) -> std::ops::Range<usize> {
        // Return a dummy span for compatibility; actual span logic is now on the leaf nodes
        0..0
    }
}

/// @deprecated
/// The legacy `FatNode` logic. Do not use.
///
/// This previously handled span tracking via indices. This logic is now deprecated
/// and spans should be handled directly on the struct definitions.
#[deprecated(since = "0.2.0", note = "FatNode logic is removed; use fields on specific structs")]
pub struct FatNode {
    #[deprecated(since = "0.2.0")]
    pub node: Node,
    #[deprecated(since = "0.2.0")]
    pub parent: Option<usize>,
    #[deprecated(since = "0.2.0")]
    pub span: std::ops::Range<usize>,
}

#[allow(deprecated)]
impl FatNode {
    /// @deprecated Stub constructor.
    pub fn new(node: Node, parent: Option<usize>, span: std::ops::Range<usize>) -> Self {
        Self { node, parent, span }
    }
}
```
