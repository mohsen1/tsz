use std::sync::Arc;

use crate::parser::ast::base::AstEntity;
use crate::parser::ast::expression::Expression; // Assuming Expression exists and implements AstEntity
use crate::parser::ast::ThinNode; // Type alias for Arc<Node>

/// Represents a JSX element (e.g., `<div>...</div>`).
#[derive(Debug, Clone, PartialEq)]
pub struct JsxElement {
    pub span: (usize, usize),
    pub opening: ThinNode<JsxOpeningElement>,
    pub children: Vec<ThinNode<JsxChild>>,
    pub closing: Option<ThinNode<JsxClosingElement>>,
}

impl AstEntity for JsxElement {
    fn span(&self) -> (usize, usize) {
        self.span
    }
}

/// Represents the opening part of a JSX tag (e.g., `<div className="foo">`).
#[derive(Debug, Clone, PartialEq)]
pub struct JsxOpeningElement {
    pub span: (usize, usize),
    pub name: ThinNode<JsxTagName>,
    pub attributes: Vec<ThinNode<JsxAttributeOrSpread>>,
    pub self_closing: bool,
}

impl AstEntity for JsxOpeningElement {
    fn span(&self) -> (usize, usize) {
        self.span
    }
}

/// Represents the closing part of a JSX tag (e.g., `</div>`).
#[derive(Debug, Clone, PartialEq)]
pub struct JsxClosingElement {
    pub span: (usize, usize),
    pub name: ThinNode<JsxTagName>,
}

impl AstEntity for JsxClosingElement {
    fn span(&self) -> (usize, usize) {
        self.span
    }
}

/// Valid names for JSX tags.
#[derive(Debug, Clone, PartialEq)]
pub enum JsxTagName {
    Identifier(String),
    MemberExpr(ThinNode<Expression>), // e.g. Foo.Bar
    // Namespaced names like ns:tag are usually handled as Identifier in loose parsers, 
    // but could be strict here if needed.
}

impl AstEntity for JsxTagName {
    fn span(&self) -> (usize, usize) {
        match self {
            JsxTagName::Identifier(_) => (0, 0), // Span should be stored separately or calculated
            JsxTagName::MemberExpr(e) => e.span(),
        }
    }
}

/// Represents a child node inside a JSX element.
#[derive(Debug, Clone, PartialEq)]
pub enum JsxChild {
    Element(ThinNode<JsxElement>),
    Fragment(ThinNode<JsxFragment>),
    Text(String),
    Expression(ThinNode<JsxExpressionContainer>),
    Spread(ThinNode<JsxSpreadChild>),
}

impl AstEntity for JsxChild {
    fn span(&self) -> (usize, usize) {
        match self {
            JsxChild::Element(el) => el.span(),
            JsxChild::Fragment(frag) => frag.span(),
            JsxChild::Text(_) => (0, 0), // Text span tracking depends on lexer implementation
            JsxChild::Expression(expr) => expr.span(),
            JsxChild::Spread(spread) => spread.span(),
        }
    }
}

/// Represents a container for a JavaScript expression inside JSX (e.g., `{foo}`).
#[derive(Debug, Clone, PartialEq)]
pub struct JsxExpressionContainer {
    pub span: (usize, usize),
    pub expression: ThinNode<Expression>,
}

impl AstEntity for JsxExpressionContainer {
    fn span(&self) -> (usize, usize) {
        self.span
    }
}

/// Represents a spread child inside JSX (e.g., `{...props}`).
#[derive(Debug, Clone, PartialEq)]
pub struct JsxSpreadChild {
    pub span: (usize, usize),
    pub expression: ThinNode<Expression>,
}

impl AstEntity for JsxSpreadChild {
    fn span(&self) -> (usize, usize) {
        self.span
    }
}

/// Represents a JSX Fragment (`<>...</>`).
#[derive(Debug, Clone, PartialEq)]
pub struct JsxFragment {
    pub span: (usize, usize),
    pub opening: ThinNode<JsxOpeningFragment>,
    pub
