use crate::types_domain::type_node::TypeNodeChecker;
use crate::types_domain::unique_symbol_arena::is_unique_symbol_type_annotation_unwrapped;
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
        let type_ann = if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
            var_decl
                .type_annotation
                .is_some()
                .then_some(var_decl.type_annotation)
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
        }?;

        if is_unique_symbol_type_annotation_unwrapped(self.ctx.arena, type_ann) {
            return Some(self.ctx.types.unique_symbol(SymbolRef(sym_id.0)));
        }

        Some(self.check(type_ann)).filter(|&t| t != TypeId::ANY && t != TypeId::ERROR)
    }
}
