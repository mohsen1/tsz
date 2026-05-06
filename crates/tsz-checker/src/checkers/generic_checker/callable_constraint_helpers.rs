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
        let Some((object, _index)) = query::index_access_components(db, type_id) else {
            return false;
        };
        if let Some(_mapped_id) = query::mapped_type_id(db, object)
            && self.mapped_template_resolves_to_callable_through_constraint(object)
        {
            return true;
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

    /// Check if a type argument coinductively satisfies a recursive constraint
    /// via its heritage chain.
    ///
    /// When an interface extends a generic base (e.g., `interface BB extends AA<AA<BB>>`),
    /// and the constraint is an Application of that same base (e.g., `AA<BB>`), the
    /// structural subtype check becomes circular. The subtype checker can't detect the
    /// cycle because pre-evaluation destroys DefId identity. This method detects the
    /// pattern and returns true (coinductive assumption).
    pub(super) fn satisfies_recursive_heritage_constraint(
        &self,
        type_arg: TypeId,
        constraint: TypeId,
    ) -> bool {
        let db = self.ctx.types.as_type_database();

        // Get the Application base DefId from the constraint.
        // e.g., for AA<BB>, get the DefId of AA.
        let Some(constraint_base_def) = query::application_base_def_id(db, constraint) else {
            return false;
        };

        // Get the type_arg's DefId (it must be an interface/class, i.e., Lazy type).
        let type_arg_def = query::lazy_def_id(db, type_arg);
        let Some(type_arg_def) = type_arg_def else {
            // When the type_arg is an Application (e.g., AA<BB>) with the same base
            // as the constraint (e.g., AA<AA<BB>>), AND the inner type arguments of
            // the Application type_arg extend the constraint base via heritage, the
            // constraint is coinductively satisfied. This handles recursive constraints
            // like `T extends AA<T>` where `interface BB extends AA<AA<BB>>` — checking
            // `AA<BB>` against `AA<AA<BB>>` leads to infinite nesting that tsc resolves
            // via deeply-nested type detection.
            if let Some((Some(type_arg_base_def), ref type_arg_args)) =
                query::application_base_def_and_args(db, type_arg)
                && type_arg_base_def == constraint_base_def
            {
                // Same base type (e.g., both are AA<...>).
                // Check if any inner type argument extends the constraint base,
                // which would create the circular recursion pattern.
                for &inner_arg in type_arg_args.iter() {
                    if let Some(inner_def) = query::lazy_def_id(db, inner_arg) {
                        let inner_sym = self.ctx.def_to_symbol_id(inner_def);
                        let constraint_sym = self.ctx.def_to_symbol_id(constraint_base_def);
                        if let (Some(inner_sym_id), Some(constraint_sym_id)) =
                            (inner_sym, constraint_sym)
                            && self.interface_extends_symbol(inner_sym_id, constraint_sym_id)
                        {
                            return true;
                        }
                    }
                }
            }
            return false;
        };

        // Resolve DefIds to SymbolIds
        let type_arg_sym = self.ctx.def_to_symbol_id(type_arg_def);
        let constraint_base_sym = self.ctx.def_to_symbol_id(constraint_base_def);

        let (Some(type_arg_sym_id), Some(constraint_base_sym_id)) =
            (type_arg_sym, constraint_base_sym)
        else {
            return false;
        };

        // Check if type_arg's interface heritage chain includes the constraint's
        // base interface. Walk the heritage clauses in the binder to find if BB
        // extends any instantiation of AA.
        self.interface_extends_symbol(type_arg_sym_id, constraint_base_sym_id)
    }

    /// Check if an interface symbol extends (directly or transitively) a target symbol.
    fn interface_extends_symbol(
        &self,
        interface_sym_id: tsz_binder::SymbolId,
        target_sym_id: tsz_binder::SymbolId,
    ) -> bool {
        if interface_sym_id == target_sym_id {
            return true;
        }

        let Some(symbol) = self.ctx.binder.get_symbol(interface_sym_id) else {
            return false;
        };

        // Check each declaration's heritage clauses
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };
            let Some(ref heritage_clauses) = interface.heritage_clauses else {
                continue;
            };
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };
                    // Extract the base expression (might be an ExpressionWithTypeArguments)
                    let expr_idx = if let Some(eta) = self.ctx.arena.get_expr_type_args(type_node) {
                        eta.expression
                    } else {
                        type_idx
                    };
                    if let Some(base_sym) = self.resolve_heritage_symbol(expr_idx)
                        && base_sym == target_sym_id
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if a symbol's declaration has type parameters, even if they couldn't be
    /// resolved via `get_type_params_for_symbol` (e.g., cross-arena lib types).
    pub(crate) fn symbol_declaration_has_type_parameters(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders);
        let Some(symbol) = symbol else {
            return false;
        };

        // Check the value declaration and all declarations for type parameters
        for decl_idx in symbol.all_declarations() {
            // Try current arena first
            if let Some(node) = self.ctx.arena.get(decl_idx) {
                if let Some(ta) = self.ctx.arena.get_type_alias(node) {
                    if ta.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(iface) = self.ctx.arena.get_interface(node) {
                    if iface.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(class) = self.ctx.arena.get_class(node) {
                    if class.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
            }

            // Try cross-arena (lib files)
            if let Some(decl_arena) = self.ctx.binder.symbol_arenas.get(&sym_id)
                && let Some(node) = decl_arena.get(decl_idx)
            {
                if let Some(ta) = decl_arena.get_type_alias(node) {
                    if ta.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(iface) = decl_arena.get_interface(node) {
                    if iface.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(class) = decl_arena.get_class(node) {
                    if class.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
            }

            // Try declaration_arenas
            if let Some(decl_arena) = self
                .ctx
                .binder
                .declaration_arenas
                .get(&(sym_id, decl_idx))
                .and_then(|v| v.first())
                && let Some(node) = decl_arena.get(decl_idx)
            {
                if let Some(ta) = decl_arena.get_type_alias(node) {
                    if ta.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(iface) = decl_arena.get_interface(node) {
                    if iface.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(class) = decl_arena.get_class(node) {
                    if class.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
            }
        }

        false
    }
}
