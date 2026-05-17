use super::type_node::TypeNodeChecker;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{SymbolRef, TypeId};

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    pub(super) fn declared_type_for_type_query_symbol(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<TypeId> {
        if let Some(type_id) = self
            .ctx
            .symbol_types
            .get(&sym_id)
            .copied()
            .filter(|&t| t != TypeId::ANY && t != TypeId::ERROR)
        {
            return Some(type_id);
        }

        let decl = self.ctx.binder.get_symbol(sym_id)?.value_declaration;
        if decl.is_none() {
            return None;
        }
        let decl_node = self.ctx.arena.get(decl)?;
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
            if var_decl.type_annotation.is_some() {
                return Some(self.check(var_decl.type_annotation))
                    .filter(|&t| t != TypeId::ANY && t != TypeId::ERROR);
            }
            if self.ctx.arena.is_const_variable_declaration(decl)
                && var_decl.initializer.is_some()
                && (self.is_global_symbol_call_initializer(var_decl.initializer)
                    || self.is_global_symbol_for_call_initializer(var_decl.initializer))
            {
                return Some(self.ctx.types.unique_symbol(SymbolRef(sym_id.0)));
            }
            None
        } else if decl_node.kind == syntax_kind_ext::PARAMETER {
            let param = self.ctx.arena.get_parameter(decl_node)?;
            param
                .type_annotation
                .is_some()
                .then_some(param.type_annotation)
        } else if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let parent = self.ctx.arena.get_extended(decl)?.parent;
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::PARAMETER {
                let param = self.ctx.arena.get_parameter(parent_node)?;
                (param.name == decl && param.type_annotation.is_some())
                    .then_some(param.type_annotation)
            } else {
                None
            }
        } else {
            None
        }
        .and_then(|type_ann| {
            Some(self.check(type_ann)).filter(|&t| t != TypeId::ANY && t != TypeId::ERROR)
        })
    }

    fn is_global_symbol_call_initializer(&self, init_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(init_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return false;
        };
        self.identifier_is_global_symbol_value(call.expression)
    }

    fn is_global_symbol_for_call_initializer(&self, init_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(init_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return false;
        };
        let Some(callee_node) = self.ctx.arena.get(call.expression) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        self.identifier_is_global_symbol_value(access.expression)
            && self
                .ctx
                .arena
                .get_identifier_text(access.name_or_argument)
                .is_some_and(|name| name == "for")
    }

    fn identifier_is_global_symbol_value(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return false;
        };
        if ident.escaped_text != "Symbol" {
            return false;
        }
        let Some(sym_id) = self.ctx.binder.resolve_identifier(self.ctx.arena, idx) else {
            return false;
        };
        self.ctx
            .binder
            .get_symbol(sym_id)
            .is_some_and(|symbol| symbol.escaped_name == "Symbol")
            && (self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                || self.ctx.symbol_is_from_lib(sym_id))
    }
}
