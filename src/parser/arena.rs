//! Node arena for AST storage.

use super::ast::{Node, NodeIndex};
use super::thin_node::{NodeAccess, NodeInfo};
use serde::Serialize;

/// Arena-based storage for AST nodes.
/// Nodes are stored contiguously and referenced by index.
#[derive(Debug, Default, Serialize)]
pub struct NodeArena {
    pub nodes: Vec<Node>,
}

impl NodeArena {
    pub fn new() -> NodeArena {
        NodeArena { nodes: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> NodeArena {
        NodeArena {
            nodes: Vec::with_capacity(capacity),
        }
    }

    /// Add a node to the arena and return its index
    pub fn add(&mut self, node: Node) -> NodeIndex {
        let index = self.nodes.len() as u32;
        self.nodes.push(node);
        NodeIndex(index)
    }

    /// Get a node by index
    pub fn get(&self, index: NodeIndex) -> Option<&Node> {
        if index.is_none() {
            None
        } else {
            self.nodes.get(index.0 as usize)
        }
    }

    /// Get a mutable node by index
    pub fn get_mut(&mut self, index: NodeIndex) -> Option<&mut Node> {
        if index.is_none() {
            None
        } else {
            self.nodes.get_mut(index.0 as usize)
        }
    }

    /// Replace a node at the given index
    /// Returns the old node if successful
    pub fn replace(&mut self, index: NodeIndex, new_node: Node) -> Option<Node> {
        if index.is_none() {
            None
        } else {
            self.nodes
                .get_mut(index.0 as usize)
                .map(|old| std::mem::replace(old, new_node))
        }
    }

    /// Get the number of nodes
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the arena is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Implementation of NodeAccess for NodeArena
impl NodeAccess for NodeArena {
    fn node_info(&self, index: NodeIndex) -> Option<NodeInfo> {
        let node = self.get(index)?;
        let base = node.base();
        Some(NodeInfo {
            kind: base.kind,
            flags: base.flags,
            modifier_flags: base.modifier_flags,
            pos: base.pos,
            end: base.end,
            parent: base.parent,
            id: base.id,
        })
    }

    fn kind(&self, index: NodeIndex) -> Option<u16> {
        self.get(index).map(|n| n.base().kind)
    }

    fn pos_end(&self, index: NodeIndex) -> Option<(u32, u32)> {
        self.get(index).map(|n| (n.base().pos, n.base().end))
    }

    fn get_identifier_text(&self, index: NodeIndex) -> Option<&str> {
        match self.get(index)? {
            Node::Identifier(ident) | Node::PrivateIdentifier(ident) => Some(&ident.escaped_text),
            _ => None,
        }
    }

    fn get_literal_text(&self, index: NodeIndex) -> Option<&str> {
        match self.get(index)? {
            Node::StringLiteral(lit)
            | Node::NoSubstitutionTemplateLiteral(lit)
            | Node::TemplateHead(lit)
            | Node::TemplateMiddle(lit)
            | Node::TemplateTail(lit) => Some(&lit.text),
            Node::NumericLiteral(lit) => Some(&lit.text),
            Node::BigIntLiteral(lit) => Some(&lit.text),
            Node::RegularExpressionLiteral(lit) => Some(&lit.text),
            _ => None,
        }
    }

    fn get_children(&self, index: NodeIndex) -> Vec<NodeIndex> {
        if index.is_none() {
            return Vec::new();
        }

        let node = match self.get(index) {
            Some(n) => n,
            None => return Vec::new(),
        };

        // Helper to add optional NodeIndex (ignoring NONE)
        let add_opt = |children: &mut Vec<NodeIndex>, idx: NodeIndex| {
            if idx.is_some() {
                children.push(idx);
            }
        };

        // Helper to add NodeList (expanding to individual nodes)
        let add_list = |children: &mut Vec<NodeIndex>, list: &super::ast::NodeList| {
            children.extend(list.nodes.iter().copied());
        };

        // Helper to add optional NodeList
        let add_opt_list = |children: &mut Vec<NodeIndex>, list: &Option<super::ast::NodeList>| {
            if let Some(l) = list {
                children.extend(l.nodes.iter().copied());
            }
        };

        let mut children = Vec::new();

        // Match on node variants and extract child NodeIndex fields
        match node {
            Node::QualifiedName { left, right, .. } => {
                add_opt(&mut children, *left);
                add_opt(&mut children, *right);
            }
            Node::ComputedPropertyName { expression, .. } => {
                add_opt(&mut children, *expression);
            }
            Node::BinaryExpression(expr) => {
                add_opt(&mut children, expr.left);
                add_opt(&mut children, expr.right);
            }
            Node::PrefixUnaryExpression(expr) => {
                add_opt(&mut children, expr.operand);
            }
            Node::PostfixUnaryExpression(expr) => {
                add_opt(&mut children, expr.operand);
            }
            Node::CallExpression(expr) => {
                add_opt(&mut children, expr.expression);
                add_opt_list(&mut children, &expr.type_arguments);
                add_list(&mut children, &expr.arguments);
            }
            Node::NewExpression(expr) => {
                add_opt(&mut children, expr.expression);
                add_opt_list(&mut children, &expr.type_arguments);
                add_opt_list(&mut children, &expr.arguments);
            }
            Node::PropertyAccessExpression(expr) => {
                add_opt(&mut children, expr.expression);
                add_opt(&mut children, expr.name);
            }
            Node::ElementAccessExpression(expr) => {
                add_opt(&mut children, expr.expression);
                add_opt(&mut children, expr.argument_expression);
            }
            Node::ConditionalExpression(expr) => {
                add_opt(&mut children, expr.condition);
                add_opt(&mut children, expr.when_true);
                add_opt(&mut children, expr.when_false);
            }
            Node::ArrowFunction(func) => {
                add_opt_list(&mut children, &func.type_parameters);
                add_list(&mut children, &func.parameters);
                add_opt(&mut children, func.type_annotation);
                add_opt(&mut children, func.body);
            }
            Node::FunctionExpression(func) => {
                add_opt(&mut children, func.name);
                add_opt_list(&mut children, &func.type_parameters);
                add_list(&mut children, &func.parameters);
                add_opt(&mut children, func.type_annotation);
                add_opt(&mut children, func.body);
            }
            Node::ObjectLiteralExpression(obj) => {
                add_list(&mut children, &obj.properties);
            }
            Node::ArrayLiteralExpression(arr) => {
                add_list(&mut children, &arr.elements);
            }
            Node::ParenthesizedExpression(expr) => {
                add_opt(&mut children, expr.expression);
            }
            Node::AsExpression(expr) => {
                add_opt(&mut children, expr.expression);
                add_opt(&mut children, expr.type_node);
            }
            Node::SatisfiesExpression(expr) => {
                add_opt(&mut children, expr.expression);
                add_opt(&mut children, expr.type_node);
            }
            Node::NonNullExpression(expr) => {
                add_opt(&mut children, expr.expression);
            }
            Node::TypeAssertion(expr) => {
                add_opt(&mut children, expr.type_node);
                add_opt(&mut children, expr.expression);
            }
            Node::VariableStatement(stmt) => {
                add_opt(&mut children, stmt.declaration_list);
            }
            Node::VariableDeclarationList(list) => {
                add_list(&mut children, &list.declarations);
            }
            Node::VariableDeclaration(decl) => {
                add_opt(&mut children, decl.name);
                add_opt(&mut children, decl.type_annotation);
                add_opt(&mut children, decl.initializer);
            }
            Node::ExpressionStatement(stmt) => {
                add_opt(&mut children, stmt.expression);
            }
            Node::IfStatement(stmt) => {
                add_opt(&mut children, stmt.expression);
                add_opt(&mut children, stmt.then_statement);
                add_opt(&mut children, stmt.else_statement);
            }
            Node::WhileStatement(stmt) => {
                add_opt(&mut children, stmt.expression);
                add_opt(&mut children, stmt.statement);
            }
            Node::DoStatement(stmt) => {
                add_opt(&mut children, stmt.statement);
                add_opt(&mut children, stmt.expression);
            }
            Node::ForStatement(stmt) => {
                add_opt(&mut children, stmt.initializer);
                add_opt(&mut children, stmt.condition);
                add_opt(&mut children, stmt.incrementor);
                add_opt(&mut children, stmt.statement);
            }
            Node::Block(block) => {
                add_list(&mut children, &block.statements);
            }
            Node::FunctionDeclaration(func) => {
                add_opt(&mut children, func.name);
                add_opt_list(&mut children, &func.type_parameters);
                add_list(&mut children, &func.parameters);
                add_opt(&mut children, func.type_annotation);
                add_opt(&mut children, func.body);
            }
            Node::ReturnStatement(stmt) => {
                add_opt(&mut children, stmt.expression);
            }
            Node::ThrowStatement(stmt) => {
                add_opt(&mut children, stmt.expression);
            }
            Node::TryStatement(stmt) => {
                add_opt(&mut children, stmt.try_block);
                add_opt(&mut children, stmt.catch_clause);
                add_opt(&mut children, stmt.finally_block);
            }
            Node::CatchClause(clause) => {
                add_opt(&mut children, clause.variable_declaration);
                add_opt(&mut children, clause.block);
            }
            // Tokens and simple nodes typically have no children
            Node::Token(_) |
            Node::Identifier(_) |
            Node::PrivateIdentifier(_) |
            Node::StringLiteral(_) |
            Node::NumericLiteral(_) |
            Node::BigIntLiteral(_) |
            Node::RegularExpressionLiteral(_) |
            Node::NoSubstitutionTemplateLiteral(_) |
            Node::TemplateHead(_) |
            Node::TemplateMiddle(_) |
            Node::TemplateTail(_) |
            Node::EmptyStatement(_) |
            Node::BreakStatement(_) |
            Node::ContinueStatement(_) |
            Node::DebuggerStatement(_) => {
                // No children for these node types
            }
            // For any unhandled node types, return empty children
            // TODO: Add support for more node types as needed
            _ => {
                // Fallback for unhandled node types
            }
        }

        children
    }
}
