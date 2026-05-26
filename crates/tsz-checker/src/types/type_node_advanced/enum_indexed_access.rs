use super::super::type_node::TypeNodeChecker;
use tsz_solver::TypeId;

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    pub(super) fn enum_namespace_property_object(
        &self,
        surface_type: TypeId,
        resolved_type: TypeId,
        surface_is_type_query_node: bool,
    ) -> Option<TypeId> {
        if !surface_is_type_query_node && !self.type_surface_is_type_query_alias(surface_type) {
            return None;
        }

        let sym_id = self.ctx.resolve_type_to_symbol_id(resolved_type)?;
        let symbol = self.ctx.binder.symbols.get(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::ENUM)
            || symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
        {
            return None;
        }

        self.ctx.enum_namespace_types.get(&sym_id).copied()
    }

    fn type_surface_is_type_query_alias(&self, type_id: TypeId) -> bool {
        if crate::query_boundaries::common::is_type_query_type(self.ctx.types, type_id) {
            return true;
        }

        let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)
        else {
            return false;
        };
        let Some(def) = self.ctx.definition_store.get(def_id) else {
            return false;
        };
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return false;
        }

        def.symbol_id
            .map(tsz_binder::SymbolId)
            .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
            .is_some_and(|symbol| {
                symbol.declarations.iter().any(|&decl_idx| {
                    let Some(node) = self.ctx.arena.get(decl_idx) else {
                        return false;
                    };
                    let Some(alias) = self.ctx.arena.get_type_alias(node) else {
                        return false;
                    };
                    self.ctx.arena.get(alias.type_node).is_some_and(|body| {
                        body.kind == tsz_parser::parser::syntax_kind_ext::TYPE_QUERY
                    })
                })
            })
    }

    pub(super) fn full_enum_member_union_parent_type(&self, type_id: TypeId) -> Option<TypeId> {
        let list_id = crate::query_boundaries::common::union_list_id(self.ctx.types, type_id)?;
        let members = self.ctx.types.type_list(list_id);
        if members.is_empty() {
            return None;
        }

        let mut parent = tsz_binder::SymbolId::NONE;
        for &member_type in members.iter() {
            let member_parent = self.enum_parent_for_member_like_type(member_type)?;
            if parent.is_none() {
                parent = member_parent;
            } else if parent != member_parent {
                return None;
            }
        }

        let parent_symbol = self.ctx.binder.symbols.get(parent)?;
        let exports = parent_symbol.exports.as_ref()?;
        let enum_member_count = exports
            .iter()
            .filter(|(_, sym_id)| {
                self.ctx.binder.symbols.get(**sym_id).is_some_and(|symbol| {
                    symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
                })
            })
            .count();

        if enum_member_count == members.len() {
            if let Some(parent_type) = self.ctx.symbol_types.get(&parent).copied() {
                return Some(parent_type);
            }
            self.ctx
                .def_to_symbol
                .borrow()
                .iter()
                .find_map(|(&def_id, &sym_id)| (sym_id == parent).then_some(def_id))
                .map(|parent_def_id| self.ctx.types.factory().enum_type(parent_def_id, type_id))
        } else {
            None
        }
    }

    fn enum_parent_for_member_like_type(&self, type_id: TypeId) -> Option<tsz_binder::SymbolId> {
        if let Some((def_id, _)) =
            crate::query_boundaries::common::enum_components(self.ctx.types, type_id)
        {
            let member_sym_id = self.ctx.def_to_symbol_id(def_id)?;
            let member_symbol = self.ctx.binder.symbols.get(member_sym_id)?;
            if member_symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
                && member_symbol.parent.is_some()
            {
                return Some(member_symbol.parent);
            }
            return None;
        }

        let (object_type, index_type) =
            crate::query_boundaries::common::index_access_parts(self.ctx.types, type_id)?;
        let parent =
            crate::query_boundaries::common::type_shape_symbol(self.ctx.types, object_type)
                .or_else(|| {
                    crate::query_boundaries::common::enum_components(self.ctx.types, object_type)
                        .and_then(|(def_id, _)| self.ctx.def_to_symbol_id(def_id))
                })?;
        let parent_symbol = self.ctx.binder.symbols.get(parent)?;
        if !parent_symbol.has_any_flags(tsz_binder::symbol_flags::ENUM) {
            return None;
        }
        let member_name = crate::query_boundaries::type_computation::access::literal_property_name(
            self.ctx.types,
            index_type,
        )?;
        let member_name_text = self.ctx.types.resolve_atom(member_name);
        let member_sym_id = parent_symbol
            .exports
            .as_ref()?
            .get(member_name_text.as_ref())?;
        let member_symbol = self.ctx.binder.symbols.get(member_sym_id)?;
        if member_symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
            && member_symbol.parent == parent
        {
            Some(parent)
        } else {
            None
        }
    }
}
