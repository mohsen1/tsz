use std::sync::Arc;

// Assuming these imports exist based on the project context
// If base::AstEntity is in a different module, ensure the path is correct.
use crate::parser::ast::base::AstEntity;
use crate::parser::ast::jsx::JsxAttributeValue;
use crate::parser::ast::ThinNode; // Assuming Arc<Node> is aliased here or in mod.rs
use crate::parser::token::TokenType;

/// Represents a TypeScript or JavaScript type annotation.
#[derive(Debug, Clone, PartialEq)]
pub struct TsType {
    pub span: (usize, usize),
    pub kind: TsTypeKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TsTypeKind {
    // Basic types
    Any,
    Unknown,
    Void,
    Never,
    Null,
    Undefined,
    Boolean,
    Number,
    String,
    Symbol,
    Object,

    // Complex types
    Array(ThinNode<TsType>),
    Tuple(Vec<ThinNode<TsType>>),
    Union(Vec<ThinNode<TsType>>),
    Intersection(Vec<ThinNode<TsType>>),
    Function(TsFunctionSignature),
    TypeLiteral(ThinNode<TsTypeLiteral>),
    TypeRef {
        type_name: String,
        type_params: Vec<ThinNode<TsType>>,
    },
}

impl AstEntity for TsType {
    fn span(&self) -> (usize, usize) {
        self.span
    }
}

/// Represents a function signature in a type context.
#[derive(Debug, Clone, PartialEq)]
pub struct TsFunctionSignature {
    pub span: (usize, usize),
    pub params: Vec<ThinNode<TsTypeParameter>>,
    pub return_type: ThinNode<TsType>,
    pub type_params: Option<Vec<ThinNode<TsTypeParameter>>>,
}

impl AstEntity for TsFunctionSignature {
    fn span(&self) -> (usize, usize) {
        self.span
    }
}

/// Represents a generic type parameter (e.g., `T extends string`).
#[derive(Debug, Clone, PartialEq)]
pub struct TsTypeParameter {
    pub span: (usize, usize),
    pub name: String,
    pub constraint: Option<ThinNode<TsType>>,
    pub default: Option<ThinNode<TsType>>,
}

impl AstEntity for TsTypeParameter {
    fn span(&self) -> (usize, usize) {
        self.span
    }
}

/// Represents an object literal type (e.g., `{ a: string }`).
#[derive(Debug, Clone, PartialEq)]
pub struct TsTypeLiteral {
    pub span: (usize, usize),
    pub members: Vec<ThinNode<TsTypeElement>>,
}

impl AstEntity for TsTypeLiteral {
    fn span(&self) -> (usize, usize) {
        self.span
    }
}

/// Elements inside a TypeLiteral or Interface.
#[derive(Debug, Clone, PartialEq)]
pub enum TsTypeElement {
    Property(TsPropertySignature),
    Method(TsMethodSignature),
    Index(TsIndexSignature),
}

impl AstEntity for TsTypeElement {
    fn span(&self) -> (usize, usize) {
        match self {
            TsTypeElement::Property(p) => p.span,
            TsTypeElement::Method(m) => m.span,
            TsTypeElement::Index(i) => i.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TsPropertySignature {
    pub span: (usize, usize),
    pub name: String,
    pub readonly: bool,
    pub optional: bool,
    pub type_ann: Option<ThinNode<TsType>>,
}

impl AstEntity for TsPropertySignature {
    fn span(&self) -> (usize, usize) {
        self.span
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TsMethodSignature {
    pub span: (usize, usize),
    pub name: String,
    pub optional: bool,
    pub signature: ThinNode<TsFunctionSignature>,
}

impl AstEntity for TsMethodSignature {
    fn span(&self) -> (usize, usize) {
        self.span
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TsIndexSignature {
    pub span: (usize, usize),
    pub key: ThinNode<TsType>,
    pub value: ThinNode<TsType>,
    pub readonly: bool,
}

impl AstEntity for TsIndexSignature {
    fn span(&self) -> (usize, usize) {
        self.span
    }
}
```

### File 2: src/parser/ast/jsx.rs

```rust
//
