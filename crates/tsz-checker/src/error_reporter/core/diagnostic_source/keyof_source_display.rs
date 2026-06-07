//! `keyof` assignment-source display helpers.

use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// tsc renders a `keyof <operand>` assignment source by its reduced key set
    /// when the operand is *anonymous* (object type literal, `typeof obj`), and
    /// by the `keyof Name` spelling when the operand is a named interface / class
    /// / type alias. Against a literal-sensitive target (`0`, `"x"`, an enum, a
    /// template literal, …) the reduced key set keeps its literal members
    /// (`"a" | "b"`, `string | number`); against any other target the general
    /// path widens the source to `string`, matching tsc — so this only fires for
    /// literal-sensitive targets. tsz otherwise leaks the unreduced `keyof { … }`
    /// form or a widened `string` here, depending on how the source was built.
    ///
    /// A non-generic type alias whose body is a `keyof …` operator is resolved to
    /// that body first, so `type K = keyof { … }` is displayed identically to the
    /// inline operator. Deferred generic `keyof T` forms contain a type parameter
    /// and are left untouched.
    pub(in crate::error_reporter) fn keyof_source_assignment_display(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        if !self.is_literal_sensitive_assignment_target(target) {
            return None;
        }
        let resolved = self.resolve_non_generic_alias_body_for_display(source);
        let operand = diagnostic_query::keyof_inner_type(self.ctx.types, resolved)?;
        if diagnostic_query::contains_type_parameters(self.ctx.types, resolved) {
            return None;
        }
        // A named interface / class / type-alias operand keeps its `keyof Name`
        // spelling; any anonymous operand (object type literal, `typeof value`)
        // reduces to its key set.
        if let Some(name) = self.keyof_operand_writable_name(operand) {
            return Some(format!("keyof {name}"));
        }
        let evaluated = self.evaluate_type_for_assignability(resolved);
        if evaluated == TypeId::ERROR
            || diagnostic_query::keyof_inner_type(self.ctx.types, evaluated).is_some()
        {
            return None;
        }
        Some(self.format_assignability_type_for_message(evaluated, target))
    }

    /// The user-writable type name of a `keyof` operand, if any: a named
    /// interface, class, or non-generic type alias. An inline object type
    /// literal, a `typeof value`, or any other anonymous operand returns `None`,
    /// signalling that tsc reduces the `keyof` to its key set rather than keeping
    /// the `keyof Name` spelling. The gate is the operand's *declaration kind*,
    /// not the mere presence of a binder symbol — inline type literals carry a
    /// synthetic symbol yet have no writable name.
    fn keyof_operand_writable_name(&self, operand: TypeId) -> Option<String> {
        if self.ctx.types.get_display_alias(operand).is_some() {
            return None;
        }
        if let Some(def_id) = diagnostic_query::lazy_def_id(self.ctx.types, operand)
            .or_else(|| self.ctx.definition_store.find_def_for_type(operand))
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && matches!(
                def.kind,
                tsz_solver::def::DefKind::Interface
                    | tsz_solver::def::DefKind::Class
                    | tsz_solver::def::DefKind::TypeAlias
            )
        {
            let name = self.ctx.types.resolve_atom(def.name);
            if !name.is_empty() {
                return Some(name);
            }
        }
        if let Some(shape) = diagnostic_query::object_shape_for_type(self.ctx.types, operand)
            && let Some(sym_id) = shape.symbol
            && let Some(symbol) = self.get_cross_file_symbol(sym_id)
            && symbol.has_any_flags(
                tsz_binder::symbol_flags::INTERFACE | tsz_binder::symbol_flags::CLASS,
            )
            && !symbol.escaped_name.is_empty()
        {
            return Some(symbol.escaped_name.clone());
        }
        None
    }

    /// Resolve a non-generic type-alias `ty` to its registered body so an
    /// operator shape (e.g. `keyof …`) behind a `Lazy(DefId)` or body-registered
    /// alias becomes visible. Returns `ty` unchanged when it is not such an
    /// alias.
    fn resolve_non_generic_alias_body_for_display(&self, ty: TypeId) -> TypeId {
        if diagnostic_query::keyof_inner_type(self.ctx.types, ty).is_some() {
            return ty;
        }
        let Some(def_id) = diagnostic_query::lazy_def_id(self.ctx.types, ty)
            .or_else(|| self.ctx.definition_store.find_def_for_type(ty))
        else {
            return ty;
        };
        self.ctx
            .definition_store
            .get(def_id)
            .filter(|def| {
                def.kind == tsz_solver::def::DefKind::TypeAlias && def.type_params.is_empty()
            })
            .and_then(|def| def.body)
            .unwrap_or(ty)
    }
}
