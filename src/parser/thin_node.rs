//! # Thin Node
//!
//! This module defines `ThinNode`, the primary container for the Abstract Syntax Tree (AST).
//! Unlike a "fat node" which might contain heterogeneous data directly, `ThinNode` acts as a
//! lightweight wrapper (handle) around a concrete type implementing `AstEntity`.
//!
//! By using a generic `Inner: AstEntity` instead of a trait object (`dyn AstEntity`),
//! we avoid the overhead of dynamic dispatch for nodes whose type is known at compile time
//! (via generic constraints), or we centralize the dynamic dispatch at the entity level
//! if the entity itself is an enum. This optimizes vtable lookups and improves cache locality.

use crate::parser::ast_entity::AstEntity;
use std::fmt;

/// A lightweight wrapper for AST nodes.
///
/// `ThinNode` acts as the sole container unit for the AST. It ensures that
/// all nodes interact via the `AstEntity` trait while maintaining concrete
/// type information for the compiler to optimize layout and performance.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ThinNode<Inner>
where
    Inner: AstEntity,
{
    /// The inner data representing the node's specific entity (e.g., Identifier, FunctionDef).
    /// By keeping this generic, we allow the compiler to monomorphize the container
    /// based on the specific node type, improving density and reducing indirection.
    pub(crate) inner: Inner,
}

impl<Inner> ThinNode<Inner>
where
    Inner: AstEntity,
{
    /// Creates a new `ThinNode` wrapping the provided inner entity.
    pub fn new(inner: Inner) -> Self {
        Self { inner }
    }

    /// Consumes the `ThinNode` and returns the inner entity.
    pub fn into_inner(self) -> Inner {
        self.inner
    }

    /// Returns a reference to the inner entity.
    pub fn inner(&self) -> &Inner {
        &self.inner
    }

    /// Returns a mutable reference to the inner entity.
    pub fn inner_mut(&mut self) -> &mut Inner {
        &mut self.inner
    }

    /// Maps the inner entity to a new type using a function.
    /// This is useful for transforming nodes without changing the wrapper structure.
    pub fn map<F, U>(self, f: F) -> ThinNode<U>
    where
        F: FnOnce(Inner) -> U,
        U: AstEntity,
    {
        ThinNode {
            inner: f(self.inner),
        }
    }
}

// Implement Debug by forwarding to the inner type's Debug implementation.
impl<Inner> fmt::Debug for ThinNode<Inner>
where
    Inner: AstEntity + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

// Ensure that `ThinNode` itself acts as an `AstEntity` by delegating calls to the inner type.
// This allows `ThinNode<SpecificType>` to be used wherever `AstEntity` is required
// without manual unwrapping.
impl<Inner> AstEntity for ThinNode<Inner>
where
    Inner: AstEntity,
{
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn span(&self) -> crate::lexer::token::Span {
        self.inner.span()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::token::Span;

    // Mock implementation of AstEntity for testing
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct MockEntity {
        name: String,
        span: Span,
    }

    impl AstEntity for MockEntity {
        fn name(&self) -> &str {
            &self.name
        }

        fn span(&self) -> Span {
            self.span
        }
    }

    #[test]
    fn test_thin_node_creation() {
        let span = Span::new(0, 10);
        let entity = MockEntity {
            name: "test_node".to_string(),
            span,
        };
        let node = ThinNode::new(entity);

        assert_eq!(node.name(), "test_node");
        assert_eq!(node.span(), span);
    }

    #[test]
    fn test_thin_node_map() {
        let span = Span::new(0, 10);
        let entity = MockEntity {
            name: "original".to_string(),
            span,
        };
        let node = ThinNode::new(entity);

        let transformed = node.map(|inner| MockEntity {
            name: format!("transformed_{}", inner.name),
            span,
        });

        assert_eq!(transformed.name(), "transformed_original");
    }

    #[test]
    fn test_thin_node_size() {
        // ThinNode should be the size of the inner struct, with no additional overhead
        // (other than alignment, if any).
        let entity = MockEntity {
            name: "size_check".to_string(),
            span: Span::new(0, 0),
        };
        let node = ThinNode::new(entity);

        // Assert that ThinNode wraps MockEntity without bloating significantly
        // (In practice, ThinNode should be transparent-like).
        assert_eq!(
            std::mem::size_of_val(&node),
            std::mem::size_of::<MockEntity>()
        );
    }
}
```
