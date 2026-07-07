use super::super::type_node::TypeNodeChecker;
use crate::query_boundaries::enum_analysis as enum_query;
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
        enum_query::full_enum_member_union_parent_type(self.ctx, type_id)
    }
}
