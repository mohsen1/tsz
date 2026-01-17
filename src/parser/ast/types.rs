//! Type node AST nodes.

use super::base::{NodeBase, NodeIndex, NodeList};
use serde::Serialize;

/// A type reference (Foo, Foo<T>).
#[derive(Clone, Debug, Serialize)]
pub struct TypeReference {
    pub base: NodeBase,
    pub type_name: NodeIndex, // Identifier or QualifiedName
    pub type_arguments: Option<NodeList>,
}

/// A function type ((x: number) => string).
#[derive(Clone, Debug, Serialize)]
pub struct FunctionType {
    pub base: NodeBase,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_node: NodeIndex, // Return type
}

/// A constructor type (new (x: number) => Foo).
#[derive(Clone, Debug, Serialize)]
pub struct ConstructorType {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_node: NodeIndex,
}

/// A type query (typeof x).
#[derive(Clone, Debug, Serialize)]
pub struct TypeQuery {
    pub base: NodeBase,
    pub expr_name: NodeIndex,
    pub type_arguments: Option<NodeList>,
}

/// A type literal ({ x: number }).
#[derive(Clone, Debug, Serialize)]
pub struct TypeLiteral {
    pub base: NodeBase,
    pub members: NodeList,
}

/// An array type (number[]).
#[derive(Clone, Debug, Serialize)]
pub struct ArrayType {
    pub base: NodeBase,
    pub element_type: NodeIndex,
}

/// A tuple type ([number, string]).
#[derive(Clone, Debug, Serialize)]
pub struct TupleType {
    pub base: NodeBase,
    pub elements: NodeList,
}

/// An optional type (T?).
#[derive(Clone, Debug, Serialize)]
pub struct OptionalType {
    pub base: NodeBase,
    pub type_node: NodeIndex,
}

/// A rest type (...T).
#[derive(Clone, Debug, Serialize)]
pub struct RestType {
    pub base: NodeBase,
    pub type_node: NodeIndex,
}

/// A union type (A | B).
#[derive(Clone, Debug, Serialize)]
pub struct UnionType {
    pub base: NodeBase,
    pub types: NodeList,
}

/// An intersection type (A & B).
#[derive(Clone, Debug, Serialize)]
pub struct IntersectionType {
    pub base: NodeBase,
    pub types: NodeList,
}

/// A conditional type (T extends U ? X : Y).
#[derive(Clone, Debug, Serialize)]
pub struct ConditionalType {
    pub base: NodeBase,
    pub check_type: NodeIndex,
    pub extends_type: NodeIndex,
    pub true_type: NodeIndex,
    pub false_type: NodeIndex,
}

/// An infer type (infer T).
#[derive(Clone, Debug, Serialize)]
pub struct InferType {
    pub base: NodeBase,
    pub type_parameter: NodeIndex,
}

/// A parenthesized type ((T)).
#[derive(Clone, Debug, Serialize)]
pub struct ParenthesizedType {
    pub base: NodeBase,
    pub type_node: NodeIndex,
}

/// A type operator (keyof T, unique symbol, readonly T).
#[derive(Clone, Debug, Serialize)]
pub struct TypeOperator {
    pub base: NodeBase,
    pub operator: u16, // KeyOfKeyword, UniqueKeyword, ReadonlyKeyword
    pub type_node: NodeIndex,
}

/// An indexed access type (T[K]).
#[derive(Clone, Debug, Serialize)]
pub struct IndexedAccessType {
    pub base: NodeBase,
    pub object_type: NodeIndex,
    pub index_type: NodeIndex,
}

/// A mapped type ({ [K in T]: U }).
#[derive(Clone, Debug, Serialize)]
pub struct MappedType {
    pub base: NodeBase,
    pub readonly_token: Option<u16>, // ReadonlyKeyword, PlusToken, MinusToken
    pub type_parameter: NodeIndex,
    pub name_type: NodeIndex, // Optional
    pub question_token: Option<u16>,
    pub type_node: NodeIndex, // Optional
    pub members: Option<NodeList>,
}

/// A literal type ("foo", 42, true).
#[derive(Clone, Debug, Serialize)]
pub struct LiteralType {
    pub base: NodeBase,
    pub literal: NodeIndex,
}

/// A template literal type (`${T}`).
#[derive(Clone, Debug, Serialize)]
pub struct TemplateLiteralType {
    pub base: NodeBase,
    pub head: NodeIndex,
    pub template_spans: NodeList,
}

/// A named tuple member (name: Type or name?: Type).
#[derive(Clone, Debug, Serialize)]
pub struct NamedTupleMember {
    pub base: NodeBase,
    pub dot_dot_dot_token: bool,
    pub name: NodeIndex,
    pub question_token: bool,
    pub type_node: NodeIndex,
}

/// A type predicate (x is T, asserts x, asserts x is T).
#[derive(Clone, Debug, Serialize)]
pub struct TypePredicate {
    pub base: NodeBase,
    pub asserts_modifier: bool,    // true if `asserts` keyword present
    pub parameter_name: NodeIndex, // Identifier or ThisKeyword token
    pub type_node: NodeIndex,      // The type after 'is' (optional, NONE for just `asserts x`)
}
