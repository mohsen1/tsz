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

        if let Some(&ns_type) = self.ctx.enum_namespace_types.get(&sym_id) {
            return Some(ns_type);
        }

        // Cache miss: the enum namespace type hasn't been computed yet (it is
        // populated lazily when `get_type_of_symbol` runs for the enum, which
        // happens after this type-node check for inline `(typeof Enum)["K"]`
        // expressions). Build a minimal property-existence object from the
        // binder's export table so that the TS2339 key check below can verify
        // whether the key actually exists on the enum namespace without
        // requiring the full `merge_namespace_exports_into_object` path (which
        // is only available on `CheckerState`).  The actual indexed-access
        // return type is computed by the solver via `evaluated_indexed_type`,
        // independently of this object.
        let exports = symbol.exports.as_ref()?;
        let factory = self.ctx.types.factory();
        let props: Vec<tsz_solver::PropertyInfo> = exports
            .iter()
            .map(|(name, _)| {
                let name_atom = self.ctx.types.intern_string(name);
                tsz_solver::PropertyInfo {
                    name: name_atom,
                    type_id: tsz_solver::TypeId::ANY,
                    write_type: tsz_solver::TypeId::ANY,
                    optional: false,
                    readonly: true,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: tsz_common::Visibility::Public,
                    parent_id: None,
                    declaration_order: 0,
                    is_string_named: false,
                    is_symbol_named: false,
                    single_quoted_name: false,
                    non_widening: false,
                }
            })
            .collect();
        Some(factory.object(props))
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
            if let Some(parent_type) = self.ctx.symbol_types.get(&parent) {
                return Some(parent_type);
            }
            // Resolve the parent enum's `DefId` from the shared store's
            // `symbol_only_index` in O(1). Previously this scanned the local
            // `def_to_symbol` map (whose completeness depended on the eager
            // whole-program warm), which was O(program-symbols) per call.
            self.ctx
                .definition_store
                .find_def_by_symbol(parent.0)
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
