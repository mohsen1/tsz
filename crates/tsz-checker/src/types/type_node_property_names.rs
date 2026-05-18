//! Property-name helpers for `TypeNodeChecker`.

use super::type_node::TypeNodeChecker;
use super::unique_symbol_arena::{is_symbol_call_initializer, is_unique_symbol_type_annotation};
use crate::symbols_domain::name_text::expression_name_text_in_arena;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::SymbolRef;

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    /// Get property name from a property name node.
    fn get_property_name(&self, name_idx: NodeIndex) -> Option<String> {
        crate::types_domain::queries::core::get_literal_property_name(self.ctx.arena, name_idx)
    }

    fn register_well_known_symbol_name_mapping(&mut self, name: &str, sym_id: SymbolId) {
        if !name.starts_with("[Symbol.") {
            return;
        }

        let symbol_ref = SymbolRef(sym_id.0);
        let name_key = name.to_string();

        if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
            env.register_well_known_symbol_name(name_key.clone(), symbol_ref);
        }
        if let Ok(mut env) = self.ctx.type_environment.try_borrow_mut() {
            env.register_well_known_symbol_name(name_key, symbol_ref);
        }
    }

    /// Resolve a property name, including computed names backed by unique symbols.
    pub(super) fn get_property_name_resolved(&mut self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;

        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return self.get_property_name(name_idx);
        }

        let computed = self.ctx.arena.get_computed_property(name_node)?;

        if let Some(name) = self.get_property_name(name_idx)
            && (name.starts_with("[Symbol.") || name.starts_with("__unique_"))
        {
            if name.starts_with("[Symbol.")
                && let Some(sym_id) = self.resolve_computed_property_symbol(computed.expression)
            {
                self.register_well_known_symbol_name_mapping(&name, sym_id);
            }
            return Some(name);
        }

        if let Some(symbol_name) = self.get_well_known_symbol_property_name(computed.expression) {
            if let Some(sym_id) = self.resolve_computed_property_symbol(computed.expression) {
                self.register_well_known_symbol_name_mapping(&symbol_name, sym_id);
            }
            return Some(symbol_name);
        }

        let sym_id = self.resolve_computed_property_symbol(computed.expression)?;
        if self.symbol_refers_to_unique_symbol(sym_id) {
            return Some(format!("__unique_{}", sym_id.0));
        }

        self.get_property_name(name_idx)
    }

    pub(super) fn is_symbol_property_name(&mut self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return false;
        };

        if self
            .get_property_name_resolved(name_idx)
            .is_some_and(|name| name.starts_with("[Symbol."))
        {
            return true;
        }

        self.resolve_computed_property_symbol(computed.expression)
            .is_some_and(|sym_id| self.symbol_refers_to_unique_symbol(sym_id))
    }

    pub(super) fn computed_property_expression_name_atom(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<Atom> {
        if let Some(symbol_name) = self.get_well_known_symbol_property_name(expr_idx) {
            return Some(self.ctx.types.intern_string(&symbol_name));
        }

        let sym_id = self.resolve_computed_property_symbol(expr_idx)?;
        self.symbol_refers_to_unique_symbol(sym_id).then(|| {
            self.ctx
                .types
                .intern_string(&format!("__unique_{}", sym_id.0))
        })
    }

    pub(super) fn computed_property_expression_is_symbol_named(&self, expr_idx: NodeIndex) -> bool {
        self.get_well_known_symbol_property_name(expr_idx).is_some()
            || self
                .resolve_computed_property_symbol(expr_idx)
                .is_some_and(|sym_id| self.symbol_refers_to_unique_symbol(sym_id))
    }

    fn get_well_known_symbol_property_name(&self, expr_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            return self.get_well_known_symbol_property_name(paren.expression);
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }

        let access = self.ctx.arena.get_access_expr(node)?;
        let base_node = self.ctx.arena.get(access.expression)?;
        let base_ident = self.ctx.arena.get_identifier(base_node)?;
        if base_ident.escaped_text != "Symbol" {
            return None;
        }

        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(format!("[Symbol.{}]", ident.escaped_text));
        }

        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        ) && let Some(lit) = self.ctx.arena.get_literal(name_node)
            && !lit.text.is_empty()
        {
            return Some(format!("[Symbol.{}]", lit.text));
        }

        None
    }

    fn resolve_computed_property_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(expr_idx)?;

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            return self.resolve_computed_property_symbol(paren.expression);
        }

        if node.kind == SyntaxKind::Identifier as u16 {
            let sym_id = self
                .resolve_value_symbol_with_libs(expr_idx)
                .map(SymbolId)?;
            let mut current = sym_id;
            let mut hops = 0usize;
            while hops < 32 {
                hops += 1;
                let Some(next) = self.ctx.binder.resolve_import_symbol(current) else {
                    break;
                };
                if next == current {
                    break;
                }
                current = next;
            }
            return Some(current);
        }

        let qualified = self.expression_name_text(expr_idx)?;
        self.resolve_entity_name_text_symbol(&qualified)
    }

    fn expression_name_text(&self, idx: NodeIndex) -> Option<String> {
        expression_name_text_in_arena(self.ctx.arena, idx)
    }

    fn symbol_refers_to_unique_symbol(&self, sym_id: SymbolId) -> bool {
        self.symbol_refers_to_unique_symbol_anywhere(sym_id)
    }

    fn declaration_is_unique_symbol(&self, sym_id: SymbolId, decl_idx: NodeIndex) -> bool {
        let mut candidate_arenas: Vec<&tsz_parser::parser::node::NodeArena> = Vec::new();
        if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
            candidate_arenas.extend(arenas.iter().map(std::convert::AsRef::as_ref));
        }
        if let Some(symbol_arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
            candidate_arenas.push(symbol_arena.as_ref());
        }
        candidate_arenas.push(self.ctx.arena);

        candidate_arenas.into_iter().any(|arena| {
            let Some(node) = arena.get(decl_idx) else {
                return false;
            };
            if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                return false;
            }
            let Some(var_decl) = arena.get_variable_declaration(node) else {
                return false;
            };
            (var_decl.type_annotation.is_some()
                && is_unique_symbol_type_annotation(arena, var_decl.type_annotation))
                || is_symbol_call_initializer(arena, var_decl.initializer)
        })
    }

    fn is_unique_symbol_type_annotation_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        type_annotation: NodeIndex,
    ) -> bool {
        let Some(type_node) = arena.get(type_annotation) else {
            return false;
        };

        match type_node.kind {
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                arena.get_type_operator(type_node).is_some_and(|op| {
                    op.operator == SyntaxKind::UniqueKeyword as u16
                        && self.is_symbol_type_node_in_arena(arena, op.type_node)
                })
            }
            _ => false,
        }
    }

    fn is_symbol_type_node_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        type_annotation: NodeIndex,
    ) -> bool {
        let Some(type_node) = arena.get(type_annotation) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }

        let Some(type_ref) = arena.get_type_ref(type_node) else {
            return false;
        };

        let Some(name_node) = arena.get(type_ref.type_name) else {
            return false;
        };

        arena
            .get_identifier(name_node)
            .is_some_and(|ident| ident.escaped_text == "symbol")
    }

    fn is_symbol_call_initializer_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        init_idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(init_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }

        arena
            .get_call_expr(node)
            .and_then(|call| arena.get(call.expression))
            .and_then(|expr_node| arena.get_identifier(expr_node))
            .is_some_and(|ident| ident.escaped_text == "Symbol")
    }
}
