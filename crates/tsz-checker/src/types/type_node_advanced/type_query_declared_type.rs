use crate::types_domain::type_node::TypeNodeChecker;
use crate::types_domain::unique_symbol_arena::{
    has_declared_unique_symbol_owner, is_unique_symbol_type_annotation_unwrapped,
};
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{SymbolRef, TypeId};

struct TypeQueryDeclaredAnnotation {
    type_annotation: NodeIndex,
    can_own_unique_symbol: bool,
}

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    pub(super) fn declared_type_for_type_query_symbol(
        &mut self,
        sym_id: SymbolId,
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

        self.declared_annotation_type_for_type_query_symbol(sym_id)
    }

    pub(super) fn declared_annotation_type_for_type_query_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<TypeId> {
        let info = self.type_query_declared_annotation(sym_id)?;

        if is_unique_symbol_type_annotation_unwrapped(self.ctx.arena, info.type_annotation) {
            return Some(if info.can_own_unique_symbol {
                self.ctx.types.unique_symbol(SymbolRef(sym_id.0))
            } else {
                TypeId::SYMBOL
            });
        }

        Some(self.check(info.type_annotation)).filter(|&t| t != TypeId::ANY && t != TypeId::ERROR)
    }

    fn type_query_declared_annotation(
        &self,
        sym_id: SymbolId,
    ) -> Option<TypeQueryDeclaredAnnotation> {
        let mut decl = self.ctx.binder.get_symbol(sym_id)?.value_declaration;
        if decl.is_none() {
            return None;
        }

        let mut decl_node = self.ctx.arena.get(decl)?;
        if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let parent = self.ctx.arena.get_extended(decl)?.parent;
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                || parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
            {
                decl = parent;
                decl_node = parent_node;
            } else if parent_node.kind == syntax_kind_ext::PARAMETER {
                let param = self.ctx.arena.get_parameter(parent_node)?;
                if param.name == decl && param.type_annotation.is_some() {
                    return Some(TypeQueryDeclaredAnnotation {
                        type_annotation: param.type_annotation,
                        can_own_unique_symbol: false,
                    });
                }
            }
        }

        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
            if var_decl.type_annotation.is_some() {
                return Some(TypeQueryDeclaredAnnotation {
                    type_annotation: var_decl.type_annotation,
                    can_own_unique_symbol: self.ctx.arena.is_const_variable_declaration(decl),
                });
            }
        }

        if decl_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
            let prop = self.ctx.arena.get_property_decl(decl_node)?;
            if prop.type_annotation.is_some() {
                return Some(TypeQueryDeclaredAnnotation {
                    type_annotation: prop.type_annotation,
                    can_own_unique_symbol: has_declared_unique_symbol_owner(
                        self.ctx.arena,
                        prop.type_annotation,
                    ),
                });
            }
        }

        if decl_node.kind == syntax_kind_ext::PARAMETER {
            let param = self.ctx.arena.get_parameter(decl_node)?;
            if param.type_annotation.is_some() {
                return Some(TypeQueryDeclaredAnnotation {
                    type_annotation: param.type_annotation,
                    can_own_unique_symbol: false,
                });
            }
        }

        None
    }
}
