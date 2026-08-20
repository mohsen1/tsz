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
    Import(ImportDeclaration),
    Export(ExportDeclaration),
    Variable(VariableDeclaration),
    Function(FunctionDeclaration),
    Class(ClassDeclaration),
    TypeAlias(TypeAliasDeclaration),
    Interface(InterfaceDeclaration),
    If(IfStatement),
    Switch(SwitchStatement),
    Break(JumpStatement),
    Continue(JumpStatement),
    Return(Option<Expression>),
    Block(Vec<Statement>),
    Expression(Expression),
    Empty,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct IfStatement {
    pub condition: Expression,
    pub then_statement: Box<Statement>,
    pub else_statement: Option<Box<Statement>>,
}

#[derive(Debug, Clone)]
pub struct SwitchStatement {
    pub expression: Expression,
    pub clauses: Vec<SwitchClause>,
}

#[derive(Debug, Clone)]
pub struct SwitchClause {
    pub span: Span,
    pub kind: SwitchClauseKind,
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub enum SwitchClauseKind {
    Case(Expression),
    Default,
}

#[derive(Debug, Clone)]
pub struct JumpStatement {
    pub label: Option<String>,
    pub label_span: Option<Span>,
}

#[derive(Debug, Clone)]
pub struct ImportDeclaration {
    pub bindings: Vec<ImportBinding>,
    pub module_specifier: String,
    pub module_span: Span,
    pub type_only: bool,
    pub side_effect_only: bool,
}

#[derive(Debug, Clone)]
pub struct ImportBinding {
    pub imported: Option<String>,
    pub local: String,
    pub local_span: Span,
    pub type_only: bool,
    pub namespace: bool,
}

#[derive(Debug, Clone)]
pub struct ExportDeclaration {
    pub specifiers: Vec<ExportSpecifier>,
    pub module_specifier: Option<String>,
    pub module_span: Option<Span>,
    pub type_only: bool,
    pub export_all: bool,
    pub default_export: bool,
    pub assignment: Option<Expression>,
}

#[derive(Debug, Clone)]
pub struct ExportSpecifier {
    pub local: String,
    pub local_span: Span,
    pub exported: String,
    pub exported_span: Span,
    pub type_only: bool,
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
pub struct ClassDeclaration {
    pub name: String,
    pub name_span: Span,
    pub type_parameters: Vec<String>,
    pub extends: Option<TypeNode>,
    pub implements: Vec<TypeNode>,
    pub members: Vec<ClassMember>,
    pub exported: bool,
    pub default_export: bool,
    pub declared: bool,
    pub abstract_class: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ClassMemberModifiers {
    pub public: bool,
    pub protected: bool,
    pub private: bool,
    pub readonly: bool,
    pub static_member: bool,
    pub abstract_member: bool,
    pub declared: bool,
    pub async_member: bool,
}

#[derive(Debug, Clone)]
pub struct ClassMember {
    pub name: String,
    pub name_span: Span,
    pub span: Span,
    pub modifiers: ClassMemberModifiers,
    pub kind: ClassMemberKind,
}

#[derive(Debug, Clone)]
pub enum ClassMemberKind {
    Constructor {
        parameters: Vec<Parameter>,
        body: Vec<Statement>,
        has_body: bool,
    },
    Property {
        annotation: Option<TypeNode>,
        initializer: Option<Expression>,
        optional: bool,
        definite: bool,
    },
    Method {
        type_parameters: Vec<String>,
        parameters: Vec<Parameter>,
        return_type: Option<TypeNode>,
        body: Vec<Statement>,
        has_body: bool,
        accessor: Option<AccessorKind>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessorKind {
    Get,
    Set,
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
    pub extends: Vec<TypeNode>,
    pub properties: Vec<TypeProperty>,
    pub exported: bool,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub name_span: Span,
    pub annotation: Option<TypeNode>,
    pub optional: bool,
    pub rest: bool,
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
    Object,
    Symbol,
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
    Constructor {
        parameters: Vec<Parameter>,
        return_type: Box<TypeNode>,
        abstract_constructor: bool,
    },
    Reference {
        name: String,
        name_span: Span,
        arguments: Vec<TypeNode>,
    },
    TypeQuery {
        name: String,
        name_span: Span,
    },
    Infer {
        name: String,
        name_span: Span,
        constraint: Option<Box<TypeNode>>,
    },
    Predicate {
        parameter: String,
        parameter_span: Span,
        asserts: bool,
        ty: Option<Box<TypeNode>>,
    },
    KeyOf(Box<TypeNode>),
    Readonly(Box<TypeNode>),
    Conditional {
        check_type: Box<TypeNode>,
        extends_type: Box<TypeNode>,
        true_type: Box<TypeNode>,
        false_type: Box<TypeNode>,
    },
    Mapped {
        parameter: String,
        parameter_span: Span,
        constraint: Box<TypeNode>,
        name_type: Option<Box<TypeNode>>,
        value_type: Box<TypeNode>,
        readonly: Option<bool>,
        optional: Option<bool>,
    },
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
    New {
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
        return_type: Option<TypeNode>,
        body: ArrowBody,
    },
    Binary {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
    },
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression>,
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
    Remainder,
    LessThan,
    LessThanEquals,
    GreaterThan,
    GreaterThanEquals,
    Equals,
    NotEquals,
    StrictEquals,
    StrictNotEquals,
    LogicalAnd,
    LogicalOr,
    NullishCoalesce,
    In,
    InstanceOf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    Plus,
    Minus,
    Not,
    BitwiseNot,
    TypeOf,
    Void,
    Delete,
    Await,
}
