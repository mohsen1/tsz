use std::fmt;
use crate::span::Span;
use crate::types::Type; // Assuming a types module exists

/// Shared metadata for all AST nodes.
/// 
/// This struct packs the location information (Span) and the compile-time
/// type information (Type) into a single cache-line friendly structure,
/// reducing the overhead of the main AST enums.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeMeta {
    /// The location in the source code.
    pub span: Span,
    /// The inferred type of the node.
    pub ty: Type,
}

impl NodeMeta {
    /// Creates a new `NodeMeta` with the given span and type.
    pub fn new(span: Span, ty: Type) -> Self {
        Self { span, ty }
    }

    /// Creates a new `NodeMeta` with an unknown or default type.
    pub fn with_unknown_type(span: Span) -> Self {
        Self {
            span,
            ty: Type::Unknown,
        }
    }
}

/// A marker trait for all valid AST entities.
///
/// Implementing this trait allows a node to be stored within the unified
/// `ThinNode` system. It ensures that the object can be downcast
/// and provides basic metadata access.
pub trait AstEntity: fmt::Debug + Send + Sync + 'static {
    /// Returns a reference to the node's metadata (span and type).
    fn meta(&self) -> &NodeMeta;

    /// Returns a mutable reference to the node's metadata.
    fn meta_mut(&mut self) -> &mut NodeMeta;

    /// Helper to get the span directly.
    fn span(&self) -> Span {
        self.meta().span
    }

    /// Helper to get the type directly.
    fn ty(&self) -> &Type {
        &self.meta().ty
    }
}

// Blanket implementation for Boxed AST entities to simplify trait bounds
impl<T: AstEntity + ?Sized> AstEntity for Box<T> {
    fn meta(&self) -> &NodeMeta {
        (**self).meta()
    }

    fn meta_mut(&mut self) -> &mut NodeMeta {
        (**self).meta_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock Type and Span for testing purposes if not linked
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum MockType { Unknown, Int }
    type Type = MockType;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct MockType { Unknown, Int }

    // Manually defining a mock Span for demonstration
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Span {
        pub lo: u32,
        pub hi: u32,
    }
    
    // ... (Real implementation details would be in src/span.rs and src/types.rs)
    
    #[test]
    fn test_node_meta_creation() {
        let s = Span { lo: 0, hi: 10 };
        let meta = NodeMeta::new(s, Type::Int);
        assert_eq!(meta.span.lo, 0);
        assert_eq!(matches!(meta.ty, Type::Int), true);
    }

    // Example AST Node implementing AstEntity
    struct MockExpression {
        meta: NodeMeta,
        value: i32,
    }

    impl AstEntity for MockExpression {
        fn meta(&self) -> &NodeMeta {
            &self.meta
        }

        fn meta_mut(&mut self) -> &mut NodeMeta {
            &mut self.meta
        }
    }

    #[test]
    fn test_ast_entity_trait() {
        let span = Span { lo: 5, hi: 15 };
        let node = MockExpression {
            meta: NodeMeta::new(span, Type::Unknown),
            value: 42,
        };

        assert_eq!(node.span().lo, 5);
        assert!(matches!(node.ty(), Type::Unknown));
    }
}
```
