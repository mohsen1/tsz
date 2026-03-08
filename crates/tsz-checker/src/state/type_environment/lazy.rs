//! Lazy type resolution and type environment population.

use crate::query_boundaries::state::type_environment as query;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_solver::TypeId;
use tsz_solver::visitor::{
    collect_enum_def_ids, collect_lazy_def_ids, collect_referenced_types, collect_type_queries,
    lazy_def_id,
};

// Thread-local depth counter for `ensure_application_symbols_resolved` nesting.
//
// This must be thread-local rather than per-context because cross-arena symbol
// delegation (`delegate_cross_arena_symbol_resolution`) creates child CheckerContexts.
// A per-context counter would reset to 0 in the child, defeating the depth guard.
thread_local! {
    static APP_SYMBOL_RESOLUTION_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    // Global fuel counter for total DefId resolutions within `ensure_application_symbols_resolved`.
    // Limits total work across all nesting levels and context boundaries. Resets when
    // the outermost `ensure_application_symbols_resolved` call completes.
    static APP_SYMBOL_RESOLUTION_FUEL: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

// Maximum depth for nested `ensure_application_symbols_resolved` calls.
// Prevents explosive recursion when resolving lazy DefIds triggers type evaluation
// (compute_type_of_symbol → evaluate_application_type → evaluate_type_with_env)
// that calls back into ensure_application_symbols_resolved with new types.
const MAX_APP_SYMBOL_RESOLUTION_DEPTH: u32 = 1;

// Maximum total DefId resolutions across all nesting levels of
// `ensure_application_symbols_resolved`. This acts as a global work budget:
// once exhausted, all nested `ensure_application_symbols_resolved` calls bail out.
// Prevents exponential work on deeply-nested generic type graphs (e.g., react16.d.ts
// with InferProps<V>, RequiredKeys<V>, Validator<T> chains).
const MAX_APP_SYMBOL_RESOLUTION_FUEL: u32 = 200;

impl<'a> CheckerState<'a> {
    /// Evaluate a type with symbol resolution (Lazy types resolved to their concrete types).
    pub(crate) fn evaluate_type_with_resolution(&mut self, type_id: TypeId) -> TypeId {
        match query::classify_for_type_resolution(self.ctx.types, type_id) {
            query::TypeResolutionKind::Lazy(def_id) => {
                // Resolve Lazy(DefId) types by looking up the symbol and getting its concrete type
                // Prefer `resolve_and_insert_def_type` to ensure class instance mapping is respected
                // and the environment contains a concrete type for the definition.
                let resolved = if let Some(resolved) = self.resolve_and_insert_def_type(def_id) {
                    resolved
                } else if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
                    self.get_type_of_symbol(sym_id)
                } else {
                    type_id
                };
                if resolved == type_id {
                    return type_id;
                }

                // FIX: Detect identity loop by comparing DefId, not TypeId.
                // When get_type_of_symbol hits a circular reference, it returns a Lazy placeholder
                // for the same symbol. Even though the TypeId might be different (due to fresh interning),
                // the DefId should be the same. This detects the cycle and breaks infinite recursion.
                // This happens in cases like: class C { static { C.#x; } static #x = 123; }
                let resolved_def_id = query::lazy_def_id(self.ctx.types, resolved);
                if resolved_def_id == Some(def_id) {
                    return type_id;
                }
                // Recursively resolve if still Lazy (handles Lazy chains)
                if query::lazy_def_id(self.ctx.types, resolved).is_some() {
                    self.evaluate_type_with_resolution(resolved)
                } else {
                    // Further evaluate compound types (IndexAccess, KeyOf, Mapped, etc.)
                    // that need reduction. E.g., type NameType = Person["name"] resolves
                    // to IndexAccess(Person, "name") which must be evaluated to "string".
                    self.evaluate_type_for_assignability(resolved)
                }
            }
            query::TypeResolutionKind::Application => self.evaluate_application_type(type_id),
            query::TypeResolutionKind::Resolved => type_id,
        }
    }

    pub(crate) fn evaluate_type_with_env(&mut self, type_id: TypeId) -> TypeId {
        use tsz_solver::TypeEvaluator;

        // Fast path: intrinsic types don't need evaluation
        if type_id.is_intrinsic() {
            return type_id;
        }

        // Check shared evaluation cache
        if let Some(&cached) = self.ctx.env_eval_cache.borrow().get(&type_id) {
            return cached;
        }

        self.ensure_relation_input_ready(type_id);

        // Use type_env (not type_environment) because type_env is updated during
        // type checking with user-defined DefId→TypeId mappings, while
        // type_environment only has the initial lib symbols from build_type_environment().
        let result = {
            let env = self.ctx.type_env.borrow();
            let mut evaluator = TypeEvaluator::with_resolver(self.ctx.types, &*env);
            let result = evaluator.evaluate(type_id);
            if evaluator.is_depth_exceeded() {
                *self.ctx.depth_exceeded.borrow_mut() = true;
            }
            result
        };

        // If the result still contains IndexAccess types, try again with the full
        // checker context as resolver (which can resolve type parameters etc.)
        let final_result = if query::index_access_types(self.ctx.types, result).is_some() {
            let mut evaluator = TypeEvaluator::with_resolver(self.ctx.types, &self.ctx);
            let result = evaluator.evaluate(type_id);
            if evaluator.is_depth_exceeded() {
                *self.ctx.depth_exceeded.borrow_mut() = true;
            }
            result
        } else {
            result
        };

        // Cache the final result
        self.ctx
            .env_eval_cache
            .borrow_mut()
            .insert(type_id, final_result);
        final_result
    }

    pub(crate) fn resolve_global_interface_type(&mut self, name: &str) -> Option<TypeId> {
        // First try file_locals (includes user-defined globals and merged lib symbols)
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            return Some(self.type_reference_symbol_type(sym_id));
        }
        // Then try using get_global_type to check lib binders
        let lib_binders = self.get_lib_binders();
        if let Some(sym_id) = self
            .ctx
            .binder
            .get_global_type_with_libs(name, &lib_binders)
        {
            return Some(self.type_reference_symbol_type(sym_id));
        }
        // Fall back to resolve_lib_type_by_name for lowering types from lib contexts
        self.resolve_lib_type_by_name(name)
    }

    pub(crate) fn resolve_type_for_property_access(&mut self, type_id: TypeId) -> TypeId {
        use rustc_hash::FxHashSet;

        if let Some(&cached) = self
            .ctx
            .narrowing_cache
            .resolve_cache
            .borrow()
            .get(&type_id)
        {
            return cached;
        }

        // Fast path: already property-access-ready types do not need relation-input
        // preparation or recursive resolution.
        if matches!(
            query::classify_for_property_access_resolution(self.ctx.types, type_id),
            query::PropertyAccessResolutionKind::Resolved
                | query::PropertyAccessResolutionKind::FunctionLike
        ) {
            return type_id;
        }

        self.ensure_relation_input_ready(type_id);

        let mut visited = FxHashSet::default();
        let result = self.resolve_type_for_property_access_inner(type_id, &mut visited);
        self.ctx
            .narrowing_cache
            .resolve_cache
            .borrow_mut()
            .insert(type_id, result);
        result
    }

    pub(crate) fn resolve_type_for_property_access_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> TypeId {
        use tsz_binder::SymbolId;
        let factory = self.ctx.types.factory();

        if !visited.insert(type_id) {
            return type_id;
        }

        // Recursion depth check to prevent stack overflow
        if !self.ctx.enter_recursion() {
            return type_id;
        }

        let classification =
            query::classify_for_property_access_resolution(self.ctx.types, type_id);
        let result = match classification {
            query::PropertyAccessResolutionKind::Lazy(def_id) => {
                // Resolve lazy type from definition store
                let body_opt = self.ctx.definition_store.get_body(def_id);
                if let Some(body) = body_opt {
                    if body == type_id {
                        type_id
                    } else {
                        self.resolve_type_for_property_access_inner(body, visited)
                    }
                } else {
                    // Definition not found in store - try to resolve via symbol lookup.
                    // Use def_to_symbol_id_with_fallback to handle cross-context DefIds
                    // (e.g., Lazy types created in lib-file child checkers whose
                    // def_to_symbol mappings aren't in the main context).
                    let sym_id_opt = self.ctx.def_to_symbol_id_with_fallback(def_id);
                    if let Some(sym_id) = sym_id_opt {
                        // Enums in value position behave like objects (runtime enum object).
                        // For numeric enums, this includes a number index signature for reverse mapping.
                        // This is the same logic as Ref branch above - check for ENUM flags
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                            if symbol.flags & symbol_flags::ENUM != 0
                                && let Some(enum_object) = self.enum_object_type(sym_id)
                            {
                                if enum_object != type_id {
                                    let r = self.resolve_type_for_property_access_inner(
                                        enum_object,
                                        visited,
                                    );
                                    self.ctx.leave_recursion();
                                    return r;
                                }
                                self.ctx.leave_recursion();
                                return enum_object;
                            }

                            // Classes in type position should resolve to instance type,
                            // not constructor type. This matches the behavior of
                            // resolve_lazy() in context.rs which checks
                            // symbol_instance_types for CLASS symbols.
                            // Without this, contextually typed parameters like:
                            //   var f: (a: A) => void = (a) => a.foo;
                            // would fail because get_type_of_symbol returns the
                            // constructor type (Callable), not the instance type.
                            if symbol.flags & symbol_flags::CLASS != 0 {
                                // Try the symbol-indexed cache first (populated
                                // after class building completes).
                                let cached = self.ctx.symbol_instance_types.get(&sym_id).copied();

                                // Fallback: check the node-indexed cache for
                                // in-progress class builds.  During
                                // get_class_instance_type_inner, the partial
                                // instance type (properties + placeholder
                                // methods) is cached in class_instance_type_cache
                                // before method signatures are processed.  This
                                // lets Lazy(DefId) resolve to the partial type so
                                // property access on self-referential parameters
                                // (e.g. `p.x` where `p: Point` inside class
                                // Point) can find properties.
                                let from_node_cache = if cached.is_none() {
                                    let decl = if !symbol.value_declaration.is_none() {
                                        Some(symbol.value_declaration)
                                    } else {
                                        symbol.declarations.first().copied()
                                    };
                                    decl.and_then(|idx| {
                                        self.ctx.class_instance_type_cache.get(&idx).copied()
                                    })
                                } else {
                                    None
                                };

                                // If neither cache has it, try building via
                                // class_instance_type_from_symbol (will create
                                // the instance type if the class isn't in the
                                // resolution set).
                                let from_build = if cached.is_none() && from_node_cache.is_none() {
                                    self.class_instance_type_from_symbol(sym_id)
                                } else {
                                    None
                                };

                                let instance_type = cached.or(from_node_cache).or(from_build);
                                if let Some(instance_type) = instance_type {
                                    if instance_type != type_id {
                                        let r = self.resolve_type_for_property_access_inner(
                                            instance_type,
                                            visited,
                                        );
                                        self.ctx.leave_recursion();
                                        return r;
                                    }
                                    self.ctx.leave_recursion();
                                    return instance_type;
                                }
                            }
                        }

                        let resolved = self.get_type_of_symbol(sym_id);
                        if resolved == type_id {
                            type_id
                        } else {
                            self.resolve_type_for_property_access_inner(resolved, visited)
                        }
                    } else {
                        type_id
                    }
                }
            }
            query::PropertyAccessResolutionKind::TypeQuery(sym_ref) => {
                let resolved = self.get_type_of_symbol(SymbolId(sym_ref.0));
                if resolved == type_id {
                    type_id
                } else {
                    self.resolve_type_for_property_access_inner(resolved, visited)
                }
            }
            query::PropertyAccessResolutionKind::Application(_app_id) => {
                // For property access on Application types (e.g., Box<number>),
                // we need to expand the Application to its concrete type.
                // This is critical for unions like `Box<number> | Box<string>`
                // where the solver can't resolve Lazy bases in Application types.
                let evaluated = self.evaluate_application_type(type_id);
                if evaluated != type_id {
                    self.resolve_type_for_property_access_inner(evaluated, visited)
                } else {
                    type_id
                }
            }
            query::PropertyAccessResolutionKind::TypeParameter { constraint } => {
                if let Some(constraint) = constraint {
                    if constraint == type_id {
                        type_id
                    } else {
                        self.resolve_type_for_property_access_inner(constraint, visited)
                    }
                } else {
                    type_id
                }
            }
            query::PropertyAccessResolutionKind::NeedsEvaluation => {
                let evaluated = self.evaluate_type_with_env(type_id);
                if evaluated == type_id {
                    type_id
                } else {
                    self.resolve_type_for_property_access_inner(evaluated, visited)
                }
            }
            query::PropertyAccessResolutionKind::Union(members) => {
                // Each union member must be resolved with a fresh visited set.
                // Without this, when two union branches contain the same Application type
                // (e.g., `Foo<number> & { a: string } | Foo<number> & { b: number }`),
                // the visited set from the first branch prevents the Application from
                // being evaluated in the second branch, causing false TS2339 errors.
                let resolved_members: Vec<TypeId> = members
                    .iter()
                    .map(|&member| {
                        let mut branch_visited = visited.clone();
                        self.resolve_type_for_property_access_inner(member, &mut branch_visited)
                    })
                    .collect();
                factory.union_preserve_members(resolved_members)
            }
            query::PropertyAccessResolutionKind::Intersection(members) => {
                let resolved_members: Vec<TypeId> = members
                    .iter()
                    .map(|&member| self.resolve_type_for_property_access_inner(member, visited))
                    .collect();
                factory.intersection(resolved_members)
            }
            query::PropertyAccessResolutionKind::Readonly(inner) => {
                self.resolve_type_for_property_access_inner(inner, visited)
            }
            query::PropertyAccessResolutionKind::FunctionLike => {
                // Function/Callable types already handle function properties
                // (call, apply, bind, toString, length, prototype, arguments, caller)
                // through resolve_function_property in the solver. Creating an
                // intersection with the Function interface is redundant and harmful:
                // when the Function Lazy type can't be resolved by the solver,
                // property access falls back to ANY, masking PropertyNotFound errors
                // (e.g., this.instanceProp in static methods succeeds instead of
                // emitting TS2339).
                type_id
            }
            query::PropertyAccessResolutionKind::Resolved => type_id,
        };

        self.ctx.leave_recursion();
        result
    }

    /// Resolve a lazy type (type alias) to its body type.
    ///
    /// This function resolves `TypeData::Lazy(DefId)` types by looking up the
    /// definition's body in the definition store. This is necessary for
    /// type aliases like `type Tuple = [string, number]` where the reference
    /// to `Tuple` is stored as a lazy type.
    ///
    /// The function handles recursive type aliases by checking if the body
    /// is itself a lazy type and resolving it recursively.
    pub fn resolve_lazy_type(&mut self, type_id: TypeId) -> TypeId {
        // Fast path: non-lazy types don't need resolution or cycle detection.
        if lazy_def_id(self.ctx.types, type_id).is_none() {
            return type_id;
        }
        use rustc_hash::FxHashSet;

        let mut visited = FxHashSet::default();
        self.resolve_lazy_type_inner(type_id, &mut visited)
    }

    fn resolve_lazy_type_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> TypeId {
        // Prevent infinite loops in circular type aliases
        if !visited.insert(type_id) {
            return type_id;
        }

        // Check if this is a lazy type
        if let Some(def_id) = lazy_def_id(self.ctx.types, type_id) {
            // First, check the type_env for the resolved type.
            // This is critical for class types: the type_env's resolve_lazy returns
            // the instance type (via class_instance_types), while get_type_of_symbol
            // returns the constructor type. Since Lazy(DefId) in type position should
            // resolve to the instance type, we must check type_env first.
            {
                let env = self.ctx.type_env.borrow();
                if let Some(resolved) =
                    tsz_solver::TypeResolver::resolve_lazy(&*env, def_id, self.ctx.types)
                    && resolved != type_id
                {
                    drop(env);
                    return self.resolve_lazy_type_inner(resolved, visited);
                }
            }

            // Try to look up the definition's body in the definition store
            if let Some(body) = self.ctx.definition_store.get_body(def_id) {
                // Recursively resolve in case the body is also a lazy type
                return self.resolve_lazy_type_inner(body, visited);
            }

            // If not in the definition store or type_env, try to resolve via symbol lookup
            // This handles type aliases that are resolved through compute_type_of_symbol
            let sym_id_opt = self.ctx.def_to_symbol.borrow().get(&def_id).copied();
            if let Some(sym_id) = sym_id_opt {
                let resolved = self.get_type_of_symbol(sym_id);
                // Only recurse if the resolved type is different from the original
                if resolved != type_id {
                    return self.resolve_lazy_type_inner(resolved, visited);
                }
            }
        }

        // Handle unions and intersections - resolve each member
        // Only create a new union/intersection if members actually changed
        if let Some(resolved) = tsz_solver::type_queries::map_compound_members_if_changed(
            self.ctx.types,
            type_id,
            |member| self.resolve_lazy_type_inner(member, visited),
        ) {
            return resolved;
        }

        type_id
    }

    /// Get keyof a type - extract the keys of an object type.
    /// Ensure all symbols referenced in Application types are resolved in the `type_env`.
    /// This walks the type structure and calls `get_type_of_symbol` for any Application base symbols.
    pub(crate) fn ensure_application_symbols_resolved(&mut self, type_id: TypeId) {
        use rustc_hash::FxHashSet;

        if self.ctx.application_symbols_resolved.contains(&type_id) {
            return;
        }
        if !self.ctx.application_symbols_resolution_set.insert(type_id) {
            return;
        }

        // Check global fuel first - if exhausted from a previous call, bail immediately.
        let fuel = APP_SYMBOL_RESOLUTION_FUEL.get();
        if fuel >= MAX_APP_SYMBOL_RESOLUTION_FUEL {
            self.ctx.application_symbols_resolution_set.remove(&type_id);
            return;
        }

        // Bail out when nested too deeply. Uses thread-local counter because
        // cross-arena delegation creates child CheckerContexts that would reset
        // a per-context counter to 0.
        let depth = APP_SYMBOL_RESOLUTION_DEPTH.get();
        if depth >= MAX_APP_SYMBOL_RESOLUTION_DEPTH {
            self.ctx.application_symbols_resolution_set.remove(&type_id);
            return;
        }

        let is_outermost = depth == 0;
        if is_outermost {
            // Reset fuel for each top-level resolution
            APP_SYMBOL_RESOLUTION_FUEL.set(0);
        }
        APP_SYMBOL_RESOLUTION_DEPTH.set(depth + 1);

        let mut visited: FxHashSet<TypeId> = FxHashSet::default();
        let fully_resolved = self.ensure_application_symbols_resolved_inner(type_id, &mut visited);
        self.ctx.application_symbols_resolution_set.remove(&type_id);
        APP_SYMBOL_RESOLUTION_DEPTH.set(depth);
        if fully_resolved {
            self.ctx.application_symbols_resolved.extend(visited);
        }
    }

    pub(crate) fn insert_type_env_symbol(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        resolved: TypeId,
    ) -> bool {
        use tsz_solver::SymbolRef;

        if resolved == TypeId::ANY || resolved == TypeId::ERROR {
            return true;
        }

        // CRITICAL FIX: Only skip registering Lazy types if they point to THEMSELVES.
        // Skipping all Lazy types breaks alias chains (type A = B).
        let current_def_id = self.ctx.symbol_to_def.borrow().get(&sym_id).copied();
        if let Some(target_def_id) = query::lazy_def_id(self.ctx.types, resolved)
            && Some(target_def_id) == current_def_id
        {
            return true; // Skip self-recursive alias (A -> A)
        }

        let symbol_ref = SymbolRef(sym_id.0);
        let def_id = self.ctx.symbol_to_def.borrow().get(&sym_id).copied();

        // Reuse cached params already in the environment when available.
        let mut cached_env_params: Option<Vec<tsz_solver::TypeParamInfo>> = None;
        let mut symbol_already_registered = false;
        let mut def_already_registered = def_id.is_none();
        if let Ok(env) = self.ctx.type_env.try_borrow() {
            symbol_already_registered = env.contains(symbol_ref);
            cached_env_params = env.get_params(symbol_ref).map(|s| s.to_vec());
            if let Some(def_id) = def_id {
                def_already_registered = env.contains_def(def_id);
            }
        }
        let had_env_params = cached_env_params.is_some();
        let type_params = if let Some(params) = cached_env_params {
            params
        } else if let Some(def_id) = def_id {
            self.ctx
                .get_def_type_params(def_id)
                .unwrap_or_else(|| self.get_type_params_for_symbol(sym_id))
        } else {
            self.get_type_params_for_symbol(sym_id)
        };

        if let Some(def_id) = def_id
            && !type_params.is_empty()
        {
            self.ctx.insert_def_type_params(def_id, type_params.clone());
        }

        // Already fully registered with params (or not generic), nothing to do.
        if symbol_already_registered
            && def_already_registered
            && (had_env_params || type_params.is_empty())
        {
            return true;
        }

        // Use try_borrow_mut to avoid panic if type_env is already borrowed.
        // This can happen during recursive type resolution.
        if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
            if type_params.is_empty() {
                env.insert(symbol_ref, resolved);
                if let Some(def_id) = def_id {
                    env.insert_def(def_id, resolved);
                }
            } else {
                env.insert_with_params(symbol_ref, resolved, type_params.clone());
                if let Some(def_id) = def_id {
                    env.insert_def_with_params(def_id, resolved, type_params);
                }
            }
            true
        } else {
            false
        }
    }

    /// Resolve a `DefId` to a concrete type and insert a `DefId` mapping into the type environment.
    ///
    /// Returns the resolved type when a symbol bridge exists; returns `None` when the `DefId`
    /// is unknown to the checker. For `ANY`/`ERROR`, we intentionally skip env insertion.
    pub(crate) fn resolve_and_insert_def_type(
        &mut self,
        def_id: tsz_solver::DefId,
    ) -> Option<TypeId> {
        let sym_id = self.ctx.def_to_symbol_id_with_fallback(def_id)?;
        let resolved = if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            if symbol.flags & symbol_flags::CLASS != 0 {
                // Keep class references in type position as instance types to avoid
                // constructor/instance split diagnostics (e.g. `Type 'Dataset' is not
                // assignable to type 'Dataset'` in parser harness regressions).
                self.ctx
                    .symbol_instance_types
                    .get(&sym_id)
                    .copied()
                    .unwrap_or_else(|| self.get_type_of_symbol(sym_id))
            } else {
                self.get_type_of_symbol(sym_id)
            }
        } else {
            self.get_type_of_symbol(sym_id)
        };

        if resolved != TypeId::ERROR
            && resolved != TypeId::ANY
            && let Ok(mut env) = self.ctx.type_env.try_borrow_mut()
        {
            // Insert the type params alongside the def type so that
            // Application evaluation via TypeEnvironment can instantiate
            // generic types correctly, even for DefIds created in different
            // checker contexts (e.g., PromiseLike mapped multiple times).
            if let Some(params) = self.ctx.get_def_type_params(def_id) {
                env.insert_def_with_params(def_id, resolved, params);
            } else {
                env.insert_def(def_id, resolved);
            }
        }
        Some(resolved)
    }

    pub(crate) fn ensure_application_symbols_resolved_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> bool {
        let mut fully_resolved = true;

        // Use a worklist so we resolve dependencies transitively, including
        // definitions discovered while traversing lazily resolved references.
        let mut worklist: Vec<TypeId> = vec![type_id];
        let mut seen_types: rustc_hash::FxHashSet<TypeId> = rustc_hash::FxHashSet::default();
        let mut seen_def_ids: rustc_hash::FxHashSet<tsz_solver::DefId> =
            rustc_hash::FxHashSet::default();
        let mut resolved_types: rustc_hash::FxHashSet<TypeId> = rustc_hash::FxHashSet::default();

        while let Some(current) = worklist.pop() {
            // Check global fuel - bail if exhausted (prevents unbounded work
            // on deeply-nested generic type graphs like react16.d.ts).
            if APP_SYMBOL_RESOLUTION_FUEL.get() >= MAX_APP_SYMBOL_RESOLUTION_FUEL {
                fully_resolved = false;
                break;
            }

            if !seen_types.insert(current) {
                continue;
            }

            resolved_types.insert(current);

            for next in collect_referenced_types(self.ctx.types, current) {
                worklist.push(next);
            }

            for def_id in collect_lazy_def_ids(self.ctx.types, current) {
                if !seen_def_ids.insert(def_id) {
                    continue;
                }

                // Consume fuel for each DefId resolution (the expensive part)
                APP_SYMBOL_RESOLUTION_FUEL.set(APP_SYMBOL_RESOLUTION_FUEL.get() + 1);

                match self.resolve_lazy_def_for_type_env(def_id) {
                    Some((inserted, resolved)) => {
                        fully_resolved &= inserted;
                        if resolved != TypeId::ANY && resolved != TypeId::ERROR {
                            worklist.push(resolved);
                        }
                    }
                    None => {
                        fully_resolved = false;
                    }
                }
            }

            for def_id in collect_enum_def_ids(self.ctx.types, current) {
                if !seen_def_ids.insert(def_id) {
                    continue;
                }

                // Consume fuel for enum resolution too
                APP_SYMBOL_RESOLUTION_FUEL.set(APP_SYMBOL_RESOLUTION_FUEL.get() + 1);

                match self.resolve_enum_def_for_type_env(def_id) {
                    Some((inserted, resolved)) => {
                        fully_resolved &= inserted;
                        if resolved != TypeId::ANY && resolved != TypeId::ERROR {
                            worklist.push(resolved);
                        }
                    }
                    None => {
                        fully_resolved = false;
                    }
                }
            }

            for symbol_ref in collect_type_queries(self.ctx.types, current) {
                let sym_id = SymbolId(symbol_ref.0);
                if self.ctx.binder.get_symbol(sym_id).is_none() {
                    continue;
                }

                // Consume fuel for type query resolution
                APP_SYMBOL_RESOLUTION_FUEL.set(APP_SYMBOL_RESOLUTION_FUEL.get() + 1);

                let resolved = self.type_reference_symbol_type(sym_id);
                let inserted = self.insert_type_env_symbol(sym_id, resolved);
                fully_resolved &= inserted;
                if resolved != TypeId::ANY && resolved != TypeId::ERROR {
                    worklist.push(resolved);
                }
            }
        }

        if fully_resolved {
            visited.extend(resolved_types);
        }

        fully_resolved
    }

    fn resolve_lazy_def_for_type_env(
        &mut self,
        def_id: tsz_solver::DefId,
    ) -> Option<(bool, TypeId)> {
        if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
            // Use get_type_of_symbol (not type_reference_symbol_type) because
            // type_reference_symbol_type returns Lazy(DefId) for interfaces/classes,
            // which insert_type_env_symbol rejects as a self-recursive alias.
            // We need the concrete structural type for TypeEnvironment resolution.
            let resolved = self.get_type_of_symbol(sym_id);
            let inserted = self.insert_type_env_symbol(sym_id, resolved);
            Some((inserted, resolved))
        } else {
            None
        }
    }

    fn resolve_enum_def_for_type_env(
        &mut self,
        def_id: tsz_solver::DefId,
    ) -> Option<(bool, TypeId)> {
        if let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) {
            let resolved = self.type_reference_symbol_type(sym_id);
            let inserted = self.insert_type_env_symbol(sym_id, resolved);
            Some((inserted, resolved))
        } else {
            None
        }
    }
}
