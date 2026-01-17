//! JSX AST nodes.

use super::base::{NodeBase, NodeIndex, NodeList};
use serde::Serialize;

/// A JSX element (<Foo>children</Foo>).
#[derive(Clone, Debug, Serialize)]
pub struct JsxElement {
    pub base: NodeBase,
    pub opening_element: NodeIndex, // JsxOpeningElement
    pub children: NodeList,         // JsxChild[]
    pub closing_element: NodeIndex, // JsxClosingElement
}

/// A JSX self-closing element (<Foo />).
#[derive(Clone, Debug, Serialize)]
pub struct JsxSelfClosingElement {
    pub base: NodeBase,
    pub tag_name: NodeIndex, // JsxTagNameExpression
    pub type_arguments: Option<NodeList>,
    pub attributes: NodeIndex, // JsxAttributes
}

/// A JSX opening element (<Foo attr="value">).
#[derive(Clone, Debug, Serialize)]
pub struct JsxOpeningElement {
    pub base: NodeBase,
    pub tag_name: NodeIndex, // JsxTagNameExpression
    pub type_arguments: Option<NodeList>,
    pub attributes: NodeIndex, // JsxAttributes
}

/// A JSX closing element (</Foo>).
#[derive(Clone, Debug, Serialize)]
pub struct JsxClosingElement {
    pub base: NodeBase,
    pub tag_name: NodeIndex, // JsxTagNameExpression
}

/// A JSX fragment (<>children</>).
#[derive(Clone, Debug, Serialize)]
pub struct JsxFragment {
    pub base: NodeBase,
    pub opening_fragment: NodeIndex, // JsxOpeningFragment
    pub children: NodeList,          // JsxChild[]
    pub closing_fragment: NodeIndex, // JsxClosingFragment
}

/// A JSX opening fragment (<>).
#[derive(Clone, Debug, Serialize)]
pub struct JsxOpeningFragment {
    pub base: NodeBase,
}

/// A JSX closing fragment (</>).
#[derive(Clone, Debug, Serialize)]
pub struct JsxClosingFragment {
    pub base: NodeBase,
}

/// JSX attributes container ({ className: "foo" }).
#[derive(Clone, Debug, Serialize)]
pub struct JsxAttributes {
    pub base: NodeBase,
    pub properties: NodeList, // JsxAttributeLike[]
}

/// A JSX attribute (name="value" or name={expr}).
#[derive(Clone, Debug, Serialize)]
pub struct JsxAttribute {
    pub base: NodeBase,
    pub name: NodeIndex,        // Identifier or JsxNamespacedName
    pub initializer: NodeIndex, // StringLiteral or JsxExpression (optional)
}

/// A JSX spread attribute ({...props}).
#[derive(Clone, Debug, Serialize)]
pub struct JsxSpreadAttribute {
    pub base: NodeBase,
    pub expression: NodeIndex,
}

/// A JSX expression container ({expression}).
#[derive(Clone, Debug, Serialize)]
pub struct JsxExpression {
    pub base: NodeBase,
    pub dot_dot_dot_token: bool, // For spread in expression position
    pub expression: NodeIndex,   // Expression (optional)
}

/// A JSX text node (plain text between tags).
#[derive(Clone, Debug, Serialize)]
pub struct JsxText {
    pub base: NodeBase,
    pub text: String,
    pub contains_only_trivia_white_spaces: bool,
}

/// A JSX namespaced name (ns:name).
#[derive(Clone, Debug, Serialize)]
pub struct JsxNamespacedName {
    pub base: NodeBase,
    pub namespace: NodeIndex, // Identifier
    pub name: NodeIndex,      // Identifier
}
