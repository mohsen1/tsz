//! Expression AST nodes.

use super::base::{NodeBase, NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use serde::Serialize;

/// A binary expression (a + b, a = b, etc.).
#[derive(Clone, Debug, Serialize)]
pub struct BinaryExpression {
    pub base: NodeBase,
    pub left: NodeIndex,
    pub operator_token: SyntaxKind,
    pub right: NodeIndex,
}

/// A prefix unary expression (!x, ++x, etc.).
#[derive(Clone, Debug, Serialize)]
pub struct PrefixUnaryExpression {
    pub base: NodeBase,
    pub operator: SyntaxKind,
    pub operand: NodeIndex,
}

/// A postfix unary expression (x++, x--).
#[derive(Clone, Debug, Serialize)]
pub struct PostfixUnaryExpression {
    pub base: NodeBase,
    pub operand: NodeIndex,
    pub operator: SyntaxKind,
}

/// A call expression (fn()).
#[derive(Clone, Debug, Serialize)]
pub struct CallExpression {
    pub base: NodeBase,
    pub expression: NodeIndex,
    pub type_arguments: Option<NodeList>,
    pub arguments: NodeList,
}

/// A property access expression (obj.prop).
#[derive(Clone, Debug, Serialize)]
pub struct PropertyAccessExpression {
    pub base: NodeBase,
    pub expression: NodeIndex,
    pub question_dot_token: bool, // Optional chaining
    pub name: NodeIndex,          // Identifier or PrivateIdentifier
}

/// An element access expression (arr[idx]).
#[derive(Clone, Debug, Serialize)]
pub struct ElementAccessExpression {
    pub base: NodeBase,
    pub expression: NodeIndex,
    pub question_dot_token: bool, // Optional chaining
    pub argument_expression: NodeIndex,
}

/// A conditional expression (a ? b : c).
#[derive(Clone, Debug, Serialize)]
pub struct ConditionalExpression {
    pub base: NodeBase,
    pub condition: NodeIndex,
    pub when_true: NodeIndex,
    pub when_false: NodeIndex,
}

/// An arrow function expression.
#[derive(Clone, Debug, Serialize)]
pub struct ArrowFunction {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex, // Return type (optional)
    pub equals_greater_than_token: bool,
    pub body: NodeIndex, // Block or Expression
}

/// A function expression.
#[derive(Clone, Debug, Serialize)]
pub struct FunctionExpression {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub asterisk_token: bool,
    pub name: NodeIndex, // Optional
    pub type_parameters: Option<NodeList>,
    pub parameters: NodeList,
    pub type_annotation: NodeIndex,
    pub body: NodeIndex,
}

/// An object literal expression.
#[derive(Clone, Debug, Serialize)]
pub struct ObjectLiteralExpression {
    pub base: NodeBase,
    pub properties: NodeList,
    pub multi_line: bool,
}

/// An array literal expression.
#[derive(Clone, Debug, Serialize)]
pub struct ArrayLiteralExpression {
    pub base: NodeBase,
    pub elements: NodeList,
    pub multi_line: bool,
}

/// A parenthesized expression.
#[derive(Clone, Debug, Serialize)]
pub struct ParenthesizedExpression {
    pub base: NodeBase,
    pub expression: NodeIndex,
}

/// A new expression (new Foo()).
#[derive(Clone, Debug, Serialize)]
pub struct NewExpression {
    pub base: NodeBase,
    pub expression: NodeIndex,
    pub type_arguments: Option<NodeList>,
    pub arguments: Option<NodeList>,
}

/// A tagged template expression (`tag`template``).
#[derive(Clone, Debug, Serialize)]
pub struct TaggedTemplateExpression {
    pub base: NodeBase,
    pub tag: NodeIndex,
    pub type_arguments: Option<NodeList>,
    pub template: NodeIndex, // TemplateLiteral
}

/// A template expression (`hello ${world}`).
#[derive(Clone, Debug, Serialize)]
pub struct TemplateExpression {
    pub base: NodeBase,
    pub head: NodeIndex,          // TemplateHead
    pub template_spans: NodeList, // TemplateSpan[]
}

/// A yield expression (yield x).
#[derive(Clone, Debug, Serialize)]
pub struct YieldExpression {
    pub base: NodeBase,
    pub asterisk_token: bool,
    pub expression: NodeIndex, // Optional
}

/// An await expression (await x).
#[derive(Clone, Debug, Serialize)]
pub struct AwaitExpression {
    pub base: NodeBase,
    pub expression: NodeIndex,
}

/// A spread element (...x).
#[derive(Clone, Debug, Serialize)]
pub struct SpreadElement {
    pub base: NodeBase,
    pub expression: NodeIndex,
}

/// An as expression (x as Type).
#[derive(Clone, Debug, Serialize)]
pub struct AsExpression {
    pub base: NodeBase,
    pub expression: NodeIndex,
    pub type_node: NodeIndex,
}

/// A satisfies expression (x satisfies Type).
#[derive(Clone, Debug, Serialize)]
pub struct SatisfiesExpression {
    pub base: NodeBase,
    pub expression: NodeIndex,
    pub type_node: NodeIndex,
}

/// A non-null expression (x!).
#[derive(Clone, Debug, Serialize)]
pub struct NonNullExpression {
    pub base: NodeBase,
    pub expression: NodeIndex,
}

/// A type assertion (<Type>x).
#[derive(Clone, Debug, Serialize)]
pub struct TypeAssertion {
    pub base: NodeBase,
    pub type_node: NodeIndex,
    pub expression: NodeIndex,
}
