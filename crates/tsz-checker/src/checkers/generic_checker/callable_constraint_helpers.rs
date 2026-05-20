//! Callable and recursive heritage helpers for generic constraint validation.

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check if a type represents the global `Function` interface from lib.d.ts.
    ///
    /// Checks via Lazy(DefId) against the interner's registered boxed `DefIds`,
    /// or by direct TypeId match against the interner's registered boxed type.
    pub(super) fn is_function_constraint(&self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        // Direct match against interner's boxed Function TypeId
        if query::is_boxed_function_type(db, type_id) {
            return true;
        }
        // A function signature type (e.g., `(...args: any) => any`) is also a
        // function constraint. This handles cases like `Parameters<F>` where
        // the constraint is `T extends (...args: any) => any` and F extends Function.
        if query::is_callable_type(db, type_id) {
            return true;
        }
        // Cross-arena DefId equality alone is not strong enough here: imported
        // aliases can reuse a Lazy(DefId) shape that collides with boxed lib
        // DefIds, which falsely classifies unrelated constraints as `Function`.
        // Only accept the boxed-def fallback when the resolved symbol itself is
        // the lib `Function` symbol.
        if !query::is_boxed_function_def(db, type_id) {
            return false;
        }

        let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id) else {
            return false;
        };
        if !self.ctx.symbol_is_from_lib(sym_id) {
            return false;
        }

        let lib_binders = self.get_lib_binders();
        self.ctx
            .binder
            .get_symbol_with_libs(sym_id, &lib_binders)
            .is_some_and(|symbol| symbol.escaped_name == "Function")
    }

    pub(super) fn is_global_function_interface_constraint(&self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        if query::is_boxed_function_type(db, type_id) {
            return true;
        }
        if !query::is_boxed_function_def(db, type_id) {
            return false;
        }

        let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id) else {
            return false;
        };
        if !self.ctx.symbol_is_from_lib(sym_id) {
            return false;
        }

        let lib_binders = self.get_lib_binders();
        self.ctx
            .binder
            .get_symbol_with_libs(sym_id, &lib_binders)
            .is_some_and(|symbol| symbol.escaped_name == "Function")
    }

    /// Check if a type parameter has a callable constraint (e.g., `F extends Function`).
    /// Used during constraint satisfaction to accept callable type parameters
    /// against function signature constraints.
    pub(super) fn type_parameter_has_callable_constraint(&self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        if let Some(tp) =
            crate::query_boundaries::type_computation::complex::type_parameter_info(db, type_id)
            && let Some(constraint) = tp.constraint
        {
            return query::is_callable_type(db, constraint)
                || self.is_function_constraint(constraint);
        }
        false
    }

    /// Check if an indexed access still depends on a free type parameter.
    fn is_generic_indexed_access(&self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        if let Some((object, _)) = query::index_access_components(db, type_id) {
            return query::contains_type_parameters(self.ctx.types, object);
        }
        false
    }

    /// Return the indexed-access subject used for TS2344 callable checks.
    ///
    /// Supports both direct indexed-access type arguments (`A[B]`) and
    /// application-wrapped aliases whose instantiated body is indexed access
    /// (e.g., `Alias<T, F>` where `type Alias<T, F> = A[T][F]`).
    pub(super) fn generic_indexed_access_subject(&mut self, type_id: TypeId) -> Option<TypeId> {
        if self.is_generic_indexed_access(type_id) {
            return Some(type_id);
        }

        let db = self.ctx.types.as_type_database();
        let (Some(base_def), app_args) = query::application_base_def_and_args(db, type_id)? else {
            return None;
        };
        let def_info = self.ctx.definition_store.get(base_def)?;
        if def_info.kind != tsz_solver::def::DefKind::TypeAlias {
            return None;
        }
        let body = self.ctx.definition_store.get_body(base_def)?;

        let mut instantiated_body = body;
        if let Some(type_params) = self.ctx.definition_store.get_type_params(base_def)
            && !type_params.is_empty()
            && !app_args.is_empty()
        {
            let mut subst = crate::query_boundaries::common::TypeSubstitution::new();
            for (param, arg) in type_params.iter().zip(app_args.iter()) {
                subst.insert(param.name, *arg);
            }
            if !subst.is_empty() {
                instantiated_body =
                    crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &subst);
            }
        }

        self.is_generic_indexed_access(instantiated_body)
            .then_some(instantiated_body)
    }

    /// Check if an indexed access type `T[M]` resolves to a callable type
    /// through its constraint chain. This handles cases like:
    /// `T[M]` where `T extends { [K in keyof T]: () => unknown }` and `M extends keyof T`.
    /// The mapped type template `() => unknown` is callable, so `T[M]` resolves
    /// to a callable type. It also handles callable string/number index
    /// signatures like `T extends { [key: string]: (...args: any) => void }`.
    pub(super) fn indexed_access_resolves_to_callable(&mut self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some((object, index)) = query::index_access_components(db, type_id) else {
            return false;
        };
        if let Some(mapped_id) = query::mapped_type_id(db, object) {
            let mapped = db.mapped_type(mapped_id);
            if mapped.name_type.is_some()
                && query::contains_type_parameters(db, index)
                && crate::query_boundaries::common::is_template_literal_type(self.ctx.types, index)
            {
                return false;
            }
            if self.mapped_template_resolves_to_callable_through_constraint(object) {
                return true;
            }
        }
        // Resolve the object type's constraint chain to find a mapped type
        let object_constraint = if query::is_bare_type_parameter(db, object) {
            let base = query::base_constraint_of_type(db, object);
            if base != object {
                self.evaluate_type_for_assignability(base)
            } else {
                return false;
            }
        } else {
            return false;
        };
        let db = self.ctx.types.as_type_database();
        // Check if the resolved constraint is a mapped type with callable template
        if let Some(template) = query::mapped_type_template(db, object_constraint) {
            let template_eval = self.evaluate_type_for_assignability(template);
            let db2 = self.ctx.types.as_type_database();
            return query::is_callable_type(db2, template_eval)
                || query::callable_shape_for_type(db2, template_eval).is_some()
                || query::is_callable_type(db2, template);
        }
        for value_type in query::index_signature_value_types(db, object_constraint)
            .into_iter()
            .flatten()
        {
            let value_eval = self.evaluate_type_for_assignability(value_type);
            let db2 = self.ctx.types.as_type_database();
            if query::is_callable_type(db2, value_eval)
                || query::callable_shape_for_type(db2, value_eval).is_some()
                || query::is_callable_type(db2, value_type)
                || query::callable_shape_for_type(db2, value_type).is_some()
            {
                return true;
            }
        }
        false
    }

    pub(super) fn invalid_remapped_mapped_template_index_access(
        &mut self,
        type_id: TypeId,
    ) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some((object, index)) = query::index_access_components(db, type_id) else {
            return false;
        };
        let object_for_check = self.evaluate_type_for_assignability(object);
        let mapped_id = query::mapped_type_id(db, object)
            .or_else(|| query::mapped_type_id(db, object_for_check));
        let Some(mapped_id) = mapped_id else {
            return false;
        };
        let mapped = db.mapped_type(mapped_id);
        mapped.name_type.is_some()
            && query::contains_type_parameters(db, index)
            && crate::query_boundaries::common::is_template_literal_type(self.ctx.types, index)
    }

    pub(super) fn emit_invalid_remapped_mapped_template_index_constraint_error(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        arg_idx: Option<tsz_parser::parser::NodeIndex>,
    ) -> bool {
        let constraint_resolved = self.resolve_lazy_type(constraint);
        let constraint_evaluated = self.evaluate_type_for_assignability(constraint_resolved);
        let db = self.ctx.types.as_type_database();
        let constraint_is_callable = query::is_callable_type(db, constraint_resolved)
            || query::is_callable_type(db, constraint_evaluated)
            || self.is_function_constraint(constraint)
            || self.is_function_constraint(constraint_resolved);
        if !constraint_is_callable || !self.invalid_remapped_mapped_template_index_access(type_arg)
        {
            return false;
        }
        if let Some(arg_idx) = arg_idx {
            self.error_type_constraint_not_satisfied(type_arg, constraint_resolved, arg_idx);
        }
        true
    }

    pub(super) fn mapped_template_resolves_to_callable_through_constraint(
        &mut self,
        mapped_type: TypeId,
    ) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some(template) = query::mapped_type_template(db, mapped_type) else {
            return false;
        };

        let template_eval = self.evaluate_type_for_assignability(template);
        let db = self.ctx.types.as_type_database();
        query::is_callable_type(db, template_eval)
            || query::callable_shape_for_type(db, template_eval).is_some()
            || query::is_callable_type(db, template)
            || query::callable_shape_for_type(db, template).is_some()
            || self.indexed_access_resolves_to_callable(template)
    }
}
