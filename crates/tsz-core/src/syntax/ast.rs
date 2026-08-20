use crate::source::{NodeId, Span};

#[derive(Debug, Clone)]
pub struct SourceUnit {
    pub statements: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Statement {
    pub id: NodeId,
    pub span: Span,
    pub kind: StatementKind,
}

#[derive(Debug, Clone)]
pub enum StatementKind {
    Variable(VariableDeclaration),
    Function(FunctionDeclaration),
    TypeAlias(TypeAliasDeclaration),
    Interface(InterfaceDeclaration),
    Return(Option<Expression>),
    Block(Vec<Statement>),
    Expression(Expression),
    Empty,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableKind {
    Let,
    Const,
    Var,
}

#[derive(Debug, Clone)]
pub struct VariableDeclaration {
    pub declaration_kind: VariableKind,
    pub name: String,
    pub name_span: Span,
    pub annotation: Option<TypeNode>,
    pub initializer: Option<Expression>,
    pub exported: bool,
}

#[derive(Debug, Clone)]
pub struct FunctionDeclaration {
    pub name: String,
    pub name_span: Span,
    pub type_parameters: Vec<String>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeNode>,
    pub body: Vec<Statement>,
    pub exported: bool,
    pub is_async: bool,
    pub declared: bool,
}

#[derive(Debug, Clone)]
pub struct TypeAliasDeclaration {
    pub name: String,
    pub name_span: Span,
    pub type_parameters: Vec<String>,
    pub ty: TypeNode,
    pub exported: bool,
}

#[derive(Debug, Clone)]
pub struct InterfaceDeclaration {
    pub name: String,
    pub name_span: Span,
    pub type_parameters: Vec<String>,
    pub properties: Vec<TypeProperty>,
    pub exported: bool,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub name_span: Span,
    pub annotation: Option<TypeNode>,
    pub optional: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeProperty {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeNode,
    pub optional: bool,
    pub readonly: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeNode {
    pub span: Span,
    pub kind: TypeNodeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordType {
    Any,
    Unknown,
    Never,
    Void,
    Undefined,
    Null,
    Boolean,
    Number,
    String,
    BigInt,
}

#[derive(Debug, Clone)]
pub enum TypeNodeKind {
    Keyword(KeywordType),
    Literal(Literal),
    Array(Box<TypeNode>),
    Tuple(Vec<TypeNode>),
    Union(Vec<TypeNode>),
    Intersection(Vec<TypeNode>),
    Object(Vec<TypeProperty>),
    Function {
        parameters: Vec<Parameter>,
        return_type: Box<TypeNode>,
    },
    Reference {
        name: String,
        name_span: Span,
        arguments: Vec<TypeNode>,
    },
    KeyOf(Box<TypeNode>),
    IndexedAccess {
        object: Box<TypeNode>,
        index: Box<TypeNode>,
    },
    Parenthesized(Box<TypeNode>),
    Missing,
}

#[derive(Debug, Clone)]
pub struct Expression {
    pub id: NodeId,
    pub span: Span,
    pub kind: ExpressionKind,
}

#[derive(Debug, Clone)]
pub enum ExpressionKind {
    Identifier {
        name: String,
        name_span: Span,
    },
    Literal(Literal),
    Object(Vec<ObjectProperty>),
    Array(Vec<Expression>),
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },
    Member {
        object: Box<Expression>,
        name: String,
        name_span: Span,
    },
    Arrow {
        parameters: Vec<Parameter>,
        body: ArrowBody,
    },
    Binary {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
    },
    Assignment {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    As {
        expression: Box<Expression>,
        ty: TypeNode,
    },
    Parenthesized(Box<Expression>),
    Missing,
}

#[derive(Debug, Clone)]
pub enum ArrowBody {
    Expression(Box<Expression>),
    Block(Vec<Statement>),
}

#[derive(Debug, Clone)]
pub struct ObjectProperty {
    pub name: String,
    pub name_span: Span,
    pub value: Expression,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    String(String),
    Number(String),
    Boolean(bool),
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}
