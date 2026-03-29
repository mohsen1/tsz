// Class helper utilities for the checker.
//
// These functions inspect class declarations for base classes, heritage clauses,
// super call requirements, and constructor properties. They are used by the
// checker's class-related diagnostics (TS2376, TS2401, etc.).

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// Check if a class has a base class (extends clause).
    ///
    /// Returns true if the class has any heritage clause with `extends` keyword.
    pub(crate) fn class_has_base(&self, class: &tsz_parser::parser::node::ClassData) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(ref heritage_clauses) = class.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };
            if heritage.token == SyntaxKind::ExtendsKeyword as u16 {
                return true;
            }
        }

        false
    }

    /// Check whether a class extends `null`.
    pub(crate) fn class_extends_null(&self, class: &tsz_parser::parser::node::ClassData) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(ref heritage_clauses) = class.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            let Some(&first_type_idx) = heritage.types.nodes.first() else {
                continue;
            };

            let expr_idx = if let Some(type_node) = self.ctx.arena.get(first_type_idx)
                && let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
            {
                expr_type_args.expression
            } else {
                first_type_idx
            };

            // Skip through parenthesized expressions: `extends (null)` → `extends null`
            let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);

            let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                continue;
            };

            if expr_node.kind == SyntaxKind::NullKeyword as u16 {
                return true;
            }

            if expr_node.kind == SyntaxKind::Identifier as u16
                && self
                    .ctx
                    .arena
                    .get_identifier(expr_node)
                    .is_some_and(|id| id.escaped_text == "null")
            {
                return true;
            }
        }

        false
    }

    /// Check whether a class declaration merges with an interface declaration
    /// that has an extends clause.
    pub(crate) fn class_has_merged_interface_extends(
        &self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        if class.name.is_none() {
            return false;
        }

        let Some(name_node) = self.ctx.arena.get(class.name) else {
            return false;
        };
        let Some(name_ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };

        let Some(sym_id) = self.ctx.binder.file_locals.get(&name_ident.escaped_text) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind != syntax_kind_ext::INTERFACE_DECLARATION {
                continue;
            }
            let Some(iface) = self.ctx.arena.get_interface(decl_node) else {
                continue;
            };
            let Some(heritage_clauses) = &iface.heritage_clauses else {
                continue;
            };
            if !heritage_clauses.nodes.is_empty() {
                return true;
            }
        }

        false
    }

    /// Check whether a class requires a `super()` call in its constructor.
    pub(crate) fn class_requires_super_call(
        &self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        self.class_has_base(class) && !self.class_extends_null(class)
    }

    /// Check whether a class has features that require strict `super()` placement checks.
    ///
    /// Matches TypeScript diagnostics TS2376/TS2401 trigger conditions:
    /// initialized instance properties, constructor parameter properties,
    /// or private identifiers.
    pub(crate) fn class_has_super_call_position_sensitive_members(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        self.summarize_class_initialization(class_idx, class)
            .has_super_call_position_sensitive_members
    }

    /// Find the constructor body in a class member list.
    ///
    /// Returns the body node of the first constructor member that has a body.
    pub(crate) fn find_constructor_body(
        &self,
        members: &tsz_parser::parser::NodeList,
    ) -> Option<NodeIndex> {
        for &member_idx in &members.nodes {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(node) else {
                continue;
            };
            if ctor.body.is_some() {
                return Some(ctor.body);
            }
        }
        None
    }
}
