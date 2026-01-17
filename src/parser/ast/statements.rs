//! Statement AST nodes.

use super::base::{NodeBase, NodeIndex, NodeList};
use serde::Serialize;

/// Variable declaration kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum VariableDeclarationKind {
    Var,
    Let,
    Const,
    Using,
    AwaitUsing,
}

/// A variable statement (var/let/const declarations).
#[derive(Clone, Debug, Serialize)]
pub struct VariableStatement {
    pub base: NodeBase,
    pub modifiers: Option<NodeList>,
    pub declaration_list: NodeIndex, // VariableDeclarationList
}

/// A variable declaration list.
#[derive(Clone, Debug, Serialize)]
pub struct VariableDeclarationList {
    pub base: NodeBase,
    pub declarations: NodeList, // VariableDeclaration[]
}

/// A single variable declaration.
#[derive(Clone, Debug, Serialize)]
pub struct VariableDeclaration {
    pub base: NodeBase,
    pub name: NodeIndex,            // Identifier or BindingPattern
    pub exclamation_token: bool,    // Definite assignment assertion
    pub type_annotation: NodeIndex, // TypeNode (optional)
    pub initializer: NodeIndex,     // Expression (optional)
}

/// An expression statement.
#[derive(Clone, Debug, Serialize)]
pub struct ExpressionStatement {
    pub base: NodeBase,
    pub expression: NodeIndex,
}

/// An if statement.
#[derive(Clone, Debug, Serialize)]
pub struct IfStatement {
    pub base: NodeBase,
    pub expression: NodeIndex,
    pub then_statement: NodeIndex,
    pub else_statement: NodeIndex, // Optional
}

/// A return statement.
#[derive(Clone, Debug, Serialize)]
pub struct ReturnStatement {
    pub base: NodeBase,
    pub expression: NodeIndex, // Optional
}

/// A block statement.
#[derive(Clone, Debug, Serialize)]
pub struct Block {
    pub base: NodeBase,
    pub statements: NodeList,
    pub multi_line: bool,
}

/// A while statement.
#[derive(Clone, Debug, Serialize)]
pub struct WhileStatement {
    pub base: NodeBase,
    pub expression: NodeIndex,
    pub statement: NodeIndex,
}

/// A do-while statement.
#[derive(Clone, Debug, Serialize)]
pub struct DoStatement {
    pub base: NodeBase,
    pub statement: NodeIndex,
    pub expression: NodeIndex,
}

/// A for statement.
#[derive(Clone, Debug, Serialize)]
pub struct ForStatement {
    pub base: NodeBase,
    pub initializer: NodeIndex, // Optional
    pub condition: NodeIndex,   // Optional
    pub incrementor: NodeIndex, // Optional
    pub statement: NodeIndex,
}

/// A for-in statement.
#[derive(Clone, Debug, Serialize)]
pub struct ForInStatement {
    pub base: NodeBase,
    pub initializer: NodeIndex,
    pub expression: NodeIndex,
    pub statement: NodeIndex,
}

/// A for-of statement.
#[derive(Clone, Debug, Serialize)]
pub struct ForOfStatement {
    pub base: NodeBase,
    pub await_modifier: bool,
    pub initializer: NodeIndex,
    pub expression: NodeIndex,
    pub statement: NodeIndex,
}

/// A switch statement.
#[derive(Clone, Debug, Serialize)]
pub struct SwitchStatement {
    pub base: NodeBase,
    pub expression: NodeIndex,
    pub case_block: NodeIndex,
}

/// A case block (the part with case/default clauses).
#[derive(Clone, Debug, Serialize)]
pub struct CaseBlock {
    pub base: NodeBase,
    pub clauses: NodeList,
}

/// A case clause.
#[derive(Clone, Debug, Serialize)]
pub struct CaseClause {
    pub base: NodeBase,
    pub expression: NodeIndex,
    pub statements: NodeList,
}

/// A default clause.
#[derive(Clone, Debug, Serialize)]
pub struct DefaultClause {
    pub base: NodeBase,
    pub statements: NodeList,
}

/// A throw statement.
#[derive(Clone, Debug, Serialize)]
pub struct ThrowStatement {
    pub base: NodeBase,
    pub expression: NodeIndex,
}

/// A try statement.
#[derive(Clone, Debug, Serialize)]
pub struct TryStatement {
    pub base: NodeBase,
    pub try_block: NodeIndex,
    pub catch_clause: NodeIndex,  // Optional
    pub finally_block: NodeIndex, // Optional
}

/// A catch clause.
#[derive(Clone, Debug, Serialize)]
pub struct CatchClause {
    pub base: NodeBase,
    pub variable_declaration: NodeIndex, // Optional
    pub block: NodeIndex,
}

/// A labeled statement.
#[derive(Clone, Debug, Serialize)]
pub struct LabeledStatement {
    pub base: NodeBase,
    pub label: NodeIndex,
    pub statement: NodeIndex,
}

/// A break statement.
#[derive(Clone, Debug, Serialize)]
pub struct BreakStatement {
    pub base: NodeBase,
    pub label: NodeIndex, // Optional
}

/// A continue statement.
#[derive(Clone, Debug, Serialize)]
pub struct ContinueStatement {
    pub base: NodeBase,
    pub label: NodeIndex, // Optional
}

/// A with statement.
#[derive(Clone, Debug, Serialize)]
pub struct WithStatement {
    pub base: NodeBase,
    pub expression: NodeIndex,
    pub statement: NodeIndex,
}

/// A debugger statement.
#[derive(Clone, Debug, Serialize)]
pub struct DebuggerStatement {
    pub base: NodeBase,
}

/// An empty statement (;).
#[derive(Clone, Debug, Serialize)]
pub struct EmptyStatement {
    pub base: NodeBase,
}
