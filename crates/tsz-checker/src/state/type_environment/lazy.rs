//! Lazy type resolution and type environment population.

use crate::query_boundaries::state::type_environment as query;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_solver::TypeId;
use tsz_solver::visitor::{collect_lazy_def_ids, collect_type_queries, lazy_def_id};

use crate::query_boundaries::state::type_environment::{
    collect_enum_def_ids, collect_referenced_types,
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
    // Fuel counter for total DefId resolutions across recursive `ensure_refs_resolved`
    // invocations. The cascade ensure_refs_resolved → resolve_and_insert_def_type →
    // get_type_of_symbol → evaluate_type_with_env → ensure_relation_input_ready →
    // ensure_refs_resolved can cause explosive work on React/JSX type graphs.
    // This fuel counter limits total resolution work rather than depth, allowing
    // deep-but-narrow chains while cutting off wide-and-deep type explosions.
    static REFS_RESOLUTION_FUEL: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    // Tracks whether we're inside a top-level `ensure_refs_resolved` call tree.
    static REFS_RESOLUTION_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    // Depth counter for recursive evaluate_type_with_env_impl calls.
    // The cycle evaluate_type_with_env_impl → ensure_relation_input_ready →
    // resolve_and_insert_def_type → get_type_of_symbol → evaluate_type_with_env_impl
    // can cause unbounded stack growth. Must be thread-local because cross-arena
    // delegation creates child CheckerContexts that reset per-context counters.
    static EVAL_ENV_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    // Global accumulating fuel counter that does NOT reset between top-level
    // ensure_relation_input_ready calls. Prevents OOM when many top-level calls
    // each reset per-call fuel but together create unbounded type data
    // (e.g., DOM types + module augmentation in reactTransitiveImportHasValidDeclaration).
    static GLOBAL_RESOLUTION_FUEL: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

// Maximum global resolution fuel across all top-level calls per thread.
// This must be high enough to process large files with many expressions
// (e.g., unionSubtypeReductionErrors.ts has 6000+ lines requiring ~15K+
// resolution ops). DOM-heavy React code with module augmentations can
// explode to hundreds of thousands; this limit prevents that while
// allowing legitimate large files.
const MAX_GLOBAL_RESOLUTION_FUEL: u32 = 50_000;

/// Check if global resolution fuel is exhausted.
pub(crate) fn global_resolution_fuel_exhausted() -> bool {
    GLOBAL_RESOLUTION_FUEL.get() >= MAX_GLOBAL_RESOLUTION_FUEL
}

/// Increment the global resolution fuel counter.
pub(crate) fn increment_global_resolution_fuel() {
    GLOBAL_RESOLUTION_FUEL.set(GLOBAL_RESOLUTION_FUEL.get() + 1);
}

/// Reset global resolution fuel (call at the start of each file's type-checking).
pub(crate) fn reset_global_resolution_fuel() {
    GLOBAL_RESOLUTION_FUEL.set(0);
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

// Maximum total DefId resolutions allowed across all recursive `ensure_refs_resolved`
// invocations within one top-level call. React/JSX type graphs (react16.d.ts) have
// hundreds of interconnected generic types that cascade through resolve_and_insert_def_type.
// This budget allows normal code (typically <100 resolutions) while cutting off explosions.
const MAX_REFS_RESOLUTION_FUEL: u32 = 2000;

/// Check if refs resolution fuel is exhausted.
pub(crate) fn refs_resolution_fuel_exhausted() -> bool {
    REFS_RESOLUTION_FUEL.get() >= MAX_REFS_RESOLUTION_FUEL
}

/// Increment the refs resolution fuel counter. Called from `ensure_refs_resolved`
/// each time a DefId is resolved via `resolve_and_insert_def_type`.
pub(crate) fn increment_refs_resolution_fuel() {
    REFS_RESOLUTION_FUEL.set(REFS_RESOLUTION_FUEL.get() + 1);
}

/// Enter a top-level refs resolution scope. Resets fuel if not already active.
/// Returns true if this is the outermost call (and thus responsible for cleanup).
pub(crate) fn enter_refs_resolution_scope() -> bool {
    if REFS_RESOLUTION_ACTIVE.get() {
        false
    } else {
        REFS_RESOLUTION_ACTIVE.set(true);
        REFS_RESOLUTION_FUEL.set(0);
        true
    }
}

/// Exit a top-level refs resolution scope.
pub(crate) fn exit_refs_resolution_scope() {
    REFS_RESOLUTION_ACTIVE.set(false);
}

impl<'a> CheckerState<'a> {
    fn evaluate_type_with_env_impl(&mut self, type_id: TypeId, use_cache: bool) -> TypeId {
        use crate::query_boundaries::state::type_environment::{
            contains_infer_types_db, contains_type_query_db, evaluate_type_with_cache,
        };

        if type_id.is_intrinsic() {
            return type_id;
        }

        if use_cache && let Some(&cached) = self.ctx.env_eval_cache.borrow().get(&type_id) {
            if cached.depth_exceeded {
                self.ctx.depth_exceeded.set(true);
            }
            return cached.result;
        }

        // Depth guard: evaluate_type_with_env_impl can recurse through
        // ensure_relation_input_ready → resolve_and_insert_def_type →
        // get_type_of_symbol → evaluate_type_with_env_impl, causing
        // unbounded stack growth on cross-referencing module augmentations
        // (e.g., react + create-emotion-styled). Uses thread-local counter
        // because cross-arena delegation resets per-context counters.
        let eval_depth = EVAL_ENV_DEPTH.get();
        if eval_depth >= 5 {
            return type_id;
        }
        EVAL_ENV_DEPTH.set(eval_depth + 1);

        // Only resolve refs when not already inside an evaluate_type_with_env_impl
        // call AND not inside symbol resolution. Nested evaluation or active symbol
        // resolution can trigger compute_type_of_symbol → merge_interface_heritage_types,
        // which creates large merged types that cause OOM in the solver's evaluator
        // (module augmentations like react + create-emotion-styled).
        if eval_depth == 0
            && self.ctx.symbol_resolution_depth.get() == 0
            && self.ctx.heritage_merge_depth.get() == 0
            && REFS_RESOLUTION_FUEL.get() < MAX_REFS_RESOLUTION_FUEL
        {
            self.ensure_relation_input_ready(type_id);
        } else if eval_depth == 0 {
            // Even during symbol resolution, resolve TypeQuery symbols (typeof X)
            // into the type environment so the evaluator can resolve them.
            // This is safe because it only calls get_type_of_symbol for the
            // referenced variable (not heritage chains), preventing the issue
            // where `Parameters<typeof x>` produces a deferred conditional
            // because `typeof x` can't be resolved during type alias processing.
            self.resolve_type_queries_for_eval(type_id);
        }

        let mut depth_exceeded = false;
        let result = {
            // First pass: evaluate with TypeEnvironment resolver.
            let env = self.ctx.type_env.borrow();
            // PERF: Only collect seed entries when cache is non-empty.
            // The collect is necessary because env_eval_cache's RefCell borrow
            // must not overlap with evaluate_type_with_cache. Checking is_empty()
            // first avoids an unnecessary Vec allocation when the cache is cold.
            let seed_iter = if use_cache {
                let cache = self.ctx.env_eval_cache.borrow();
                if cache.is_empty() {
                    Vec::new()
                } else {
                    cache
                        .iter()
                        .map(|(&k, v)| (k, v.result))
                        .collect::<Vec<_>>()
                }
            } else {
                Vec::new()
            };
            let has_seed = !seed_iter.is_empty();
            let eval_result = evaluate_type_with_cache(
                self.ctx.types,
                &*env,
                type_id,
                seed_iter.into_iter(),
                has_seed,
            );
            if eval_result.depth_exceeded {
                depth_exceeded = true;
                self.ctx.depth_exceeded.set(true);
            }
            // Persist intermediate evaluation results to the shared cache.
            // Skip entries whose result contains unbound `infer` types or type queries.
            if use_cache {
                self.persist_eval_cache_entries(eval_result.cache_entries);
            }
            eval_result.result
        };

        // Second pass with CheckerContext as resolver: the first pass uses
        // TypeEnvironment which has limited Lazy resolution. If the result still
        // contains unresolved IndexAccess or Mapped types, retry with the full
        // CheckerContext resolver which can resolve Lazy(DefId) on the fly via
        // get_type_of_symbol.
        let needs_resolver_pass = query::index_access_types(self.ctx.types, result).is_some()
            || query::mapped_type_id(self.ctx.types, result).is_some();
        let final_result = if needs_resolver_pass {
            let seed_iter = if use_cache {
                let cache = self.ctx.env_eval_cache.borrow();
                cache
                    .iter()
                    .map(|(&k, v)| (k, v.result))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let eval_result = evaluate_type_with_cache(
                self.ctx.types,
                &self.ctx,
                type_id,
                seed_iter.into_iter(),
                use_cache,
            );
            if eval_result.depth_exceeded {
                depth_exceeded = true;
                self.ctx.depth_exceeded.set(true);
            }
            if use_cache {
                self.persist_eval_cache_entries(eval_result.cache_entries);
            }
            eval_result.result
        } else {
            result
        };

        // Same Infer guard for the top-level result: don't cache results
        // containing unbound infer types from partially-evaluated conditional types.
        if use_cache
            && !contains_infer_types_db(self.ctx.types, final_result)
            && !contains_type_query_db(self.ctx.types, final_result)
        {
            self.ctx.env_eval_cache.borrow_mut().insert(
                type_id,
                crate::context::EnvEvalCacheEntry {
                    result: final_result,
                    depth_exceeded,
                },
            );
        }

        EVAL_ENV_DEPTH.set(eval_depth);
        final_result
    }

    /// Persist evaluator cache entries to the shared `env_eval_cache`.
    ///
    /// Filters out entries that would poison the cache:
    /// - Entries containing unbound `infer` types (from partially-evaluated conditionals)
    /// - Entries containing type query references
    /// - Union→Application entries (incomplete evaluation artifacts)
    fn persist_eval_cache_entries(&self, entries: Vec<(TypeId, TypeId)>) {
        use crate::query_boundaries::common::is_union_type;
        use crate::query_boundaries::state::type_environment::{
            contains_infer_types_db, contains_type_query_db, is_application_type,
        };

        let mut cache = self.ctx.env_eval_cache.borrow_mut();
        for (k, v) in entries {
            if k != v
                && !k.is_intrinsic()
                && !contains_infer_types_db(self.ctx.types, v)
                && !contains_type_query_db(self.ctx.types, v)
            {
                // Guard against union→non-union cache poisoning: when the
                // evaluator maps a union type to a non-union Application,
                // this indicates a failed or incomplete evaluation (e.g.,
                // an Application whose DefId wasn't yet resolved in the
                // TypeEnvironment). Caching such entries causes downstream
                // assignability checks to fail because union member checking
                // is bypassed.
                if is_union_type(self.ctx.types, k)
                    && !is_union_type(self.ctx.types, v)
                    && is_application_type(self.ctx.types, v)
                {
                    continue;
                }
                cache.entry(k).or_insert(crate::context::EnvEvalCacheEntry {
                    result: v,
                    depth_exceeded: false,
                });
            }
        }
    }

    /// Evaluate a type with symbol resolution (Lazy types resolved to their concrete types).
    pub(crate) fn evaluate_type_with_resolution(&mut self, type_id: TypeId) -> TypeId {
        // Cycle guard: evaluate_type_with_resolution → prune_impossible_object_union_members_with_env
        // → object_member_has_impossible_required_property_with_env → evaluate_type_with_resolution
        // can form an infinite mutual recursion on recursive type aliases like
        // `type Box2 = Box<Box2 | number>`. Track types currently being resolved and
        // bail out if we re-enter with the same type.
        if !self.ctx.type_resolution_visiting.insert(type_id) {
            return type_id;
        }
        let result = self.evaluate_type_with_resolution_inner(type_id);
        self.ctx.type_resolution_visiting.remove(&type_id);
        result
    }

    fn evaluate_type_with_resolution_inner(&mut self, type_id: TypeId) -> TypeId {
        let resolved = match query::classify_for_type_resolution(self.ctx.types, type_id) {
            query::TypeResolutionKind::Lazy(def_id) => {
                // When a bare Lazy(DefId) represents a generic interface/class with
                // all-defaulted type parameters (e.g., `Int32Array` which is
                // `Int32Array<TArrayBuffer extends ArrayBufferLike = ArrayBufferLike>`),
                // wrap it in Application(Lazy, defaults) and evaluate that instead.
                // In tsc, bare `Int32Array` in type position always means
                // `Int32Array<ArrayBufferLike>`. Without this, overload resolution
                // fails because assignability compares against the raw interface
                // with unresolved type parameters.
                if let Some(type_params) = self.ctx.get_def_type_params(def_id)
                    && !type_params.is_empty()
                    && type_params.iter().all(|p| p.default.is_some())
                {
                    let default_args: Vec<tsz_solver::TypeId> = type_params
                        .iter()
                        .map(|p| p.default.unwrap_or(tsz_solver::TypeId::UNKNOWN))
                        .collect();
                    let app = self.ctx.types.application(type_id, default_args);
                    let evaluated = self.evaluate_application_type(app);
                    return self.prune_impossible_object_union_members_with_env(evaluated);
                }

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
        };

        self.prune_impossible_object_union_members_with_env(resolved)
    }

    pub(crate) fn prune_impossible_object_union_members_with_env(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        // Guard against infinite mutual recursion: evaluate → prune → evaluate members → prune.
        // Pruning calls evaluate_type_with_resolution on each union member, which can resolve
        // to new unions that get pruned again. Since pruning is a speculative optimization
        // (removing provably-impossible union members), skipping nested calls is always safe.
        if self.ctx.pruning_union_members {
            return type_id;
        }
        self.ctx.pruning_union_members = true;
        let result = self.prune_impossible_object_union_members_inner(type_id);
        self.ctx.pruning_union_members = false;
        result
    }

    fn prune_impossible_object_union_members_inner(&mut self, type_id: TypeId) -> TypeId {
        let Some(members) =
            crate::query_boundaries::state::checking::union_members(self.ctx.types, type_id)
        else {
            return type_id;
        };
        let total_members = members.len();

        let retained: Vec<_> = members
            .into_iter()
            .filter(|&member| {
                !self.intersection_has_impossible_literal_discriminants_with_env(member)
                    && !self.object_member_has_impossible_required_property_with_env(member)
            })
            .collect();

        match retained.len() {
            0 => TypeId::NEVER,
            len if len == total_members => type_id,
            1 => retained[0],
            _ => self.ctx.types.union_preserve_members(retained),
        }
    }

    fn intersection_has_impossible_literal_discriminants_with_env(
        &mut self,
        type_id: TypeId,
    ) -> bool {
        let Some(members) =
            crate::query_boundaries::state::checking::intersection_members(self.ctx.types, type_id)
        else {
            return false;
        };

        let mut discriminants: rustc_hash::FxHashMap<tsz_common::Atom, Vec<TypeId>> =
            rustc_hash::FxHashMap::default();

        for member in members {
            let evaluated_member = self.evaluate_type_with_resolution(member);
            let Some(shape) = crate::query_boundaries::state::checking::object_shape(
                self.ctx.types,
                evaluated_member,
            ) else {
                continue;
            };

            for prop in &shape.properties {
                if !crate::query_boundaries::state::checking::is_unit_type(
                    self.ctx.types,
                    prop.type_id,
                ) {
                    continue;
                }

                let seen = discriminants.entry(prop.name).or_default();
                if seen.iter().any(|&other| {
                    !self.is_subtype_of(prop.type_id, other)
                        && !self.is_subtype_of(other, prop.type_id)
                }) {
                    return true;
                }
                if !seen.contains(&prop.type_id) {
                    seen.push(prop.type_id);
                }
            }
        }

        false
    }

    fn object_member_has_impossible_required_property_with_env(&mut self, type_id: TypeId) -> bool {
        let evaluated_type = self.evaluate_type_with_resolution(type_id);
        let Some(shape) =
            crate::query_boundaries::state::checking::object_shape(self.ctx.types, evaluated_type)
        else {
            return false;
        };

        shape.properties.iter().any(|prop| {
            !prop.optional && self.type_is_impossible_unit_intersection_with_env(prop.type_id)
        })
    }

    fn type_is_impossible_unit_intersection_with_env(&mut self, type_id: TypeId) -> bool {
        let evaluated = self.evaluate_type_with_resolution(type_id);
        if evaluated == TypeId::NEVER {
            return true;
        }

        let Some(members) = crate::query_boundaries::state::checking::intersection_members(
            self.ctx.types,
            evaluated,
        ) else {
            return false;
        };

        let mut units = Vec::new();
        for member in members {
            let evaluated_member = self.evaluate_type_with_resolution(member);
            if !crate::query_boundaries::state::checking::is_unit_type(
                self.ctx.types,
                evaluated_member,
            ) {
                continue;
            }

            if units.iter().any(|&other| {
                !self.is_subtype_of(evaluated_member, other)
                    && !self.is_subtype_of(other, evaluated_member)
            }) {
                return true;
            }

            if !units.contains(&evaluated_member) {
                units.push(evaluated_member);
            }
        }

        false
    }

    pub(crate) fn evaluate_type_with_env(&mut self, type_id: TypeId) -> TypeId {
        self.evaluate_type_with_env_impl(type_id, true)
    }

    /// Resolve `TypeQuery` symbols in a type into the type environment.
    ///
    /// This is a lightweight alternative to `ensure_relation_input_ready` that only
    /// resolves `typeof X` references. It's safe to call during symbol resolution
    /// because it only triggers `get_type_of_symbol` for the referenced variables
    /// (not full heritage chain resolution that can cause OOM).
    ///
    /// This fixes the case where `Parameters<typeof x>` evaluates during type alias
    /// processing: the evaluator needs `typeof x` resolved in the `TypeEnvironment` to
    /// correctly evaluate the conditional type, but `ensure_relation_input_ready` is
    /// skipped because we're inside symbol resolution.
    fn resolve_type_queries_for_eval(&mut self, type_id: TypeId) {
        use tsz_solver::visitor::collect_type_queries;

        let type_queries = collect_type_queries(self.ctx.types, type_id);
        for symbol_ref in type_queries {
            let sym_id = tsz_binder::SymbolId(symbol_ref.0);
            let _ = self.get_type_of_symbol(sym_id);
            if let Some(&value_type) = self.ctx.symbol_types.get(&sym_id)
                && let Ok(mut env) = self.ctx.type_env.try_borrow_mut()
            {
                env.insert(tsz_solver::SymbolRef(sym_id.0), value_type);
            }
        }
    }

    pub(crate) fn evaluate_type_with_env_uncached(&mut self, type_id: TypeId) -> TypeId {
        self.evaluate_type_with_env_impl(type_id, false)
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
        // preparation or recursive resolution. Cache the identity result to avoid
        // redundant classification checks on subsequent accesses with the same type.
        if matches!(
            query::classify_for_property_access_resolution(self.ctx.types, type_id),
            query::PropertyAccessResolutionKind::Resolved
                | query::PropertyAccessResolutionKind::FunctionLike
        ) {
            self.ctx
                .narrowing_cache
                .resolve_cache
                .borrow_mut()
                .insert(type_id, type_id);
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
                // First consult the type environment. Cross-file interface and
                // alias references commonly register their structural body there
                // even when the current binder cannot re-compute the symbol.
                let env_resolved = if let Ok(env) = self.ctx.type_env.try_borrow() {
                    tsz_solver::TypeResolver::resolve_lazy(&*env, def_id, self.ctx.types)
                } else {
                    None
                };
                if let Some(resolved) = env_resolved
                    && resolved != type_id
                {
                    let resolved = self.resolve_type_for_property_access_inner(resolved, visited);
                    self.ctx.leave_recursion();
                    return resolved;
                }

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
            query::PropertyAccessResolutionKind::TypeParameter { constraint: _ } => {
                // Don't resolve type parameters to their constraints here.
                // The solver's PropertyAccessEvaluator handles TypeParameter
                // by recursing into the constraint with skip_this_binding=true,
                // preserving ThisType for the checker to substitute with the
                // correct receiver (the type parameter, not the constraint).
                type_id
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

    /// For union types whose members are Lazy(DefId) references, resolve each
    /// member so that downstream consumers (e.g., the solver's `this` type
    /// checking in union call resolution) can inspect their callable shapes.
    ///
    /// The solver's `NoopResolver` can't resolve Lazy types, so this resolution
    /// must happen in the checker before passing types to the solver.
    pub(crate) fn resolve_lazy_members_in_union(&mut self, type_id: TypeId) -> TypeId {
        use crate::query_boundaries::common;
        let Some(members) = common::union_members(self.ctx.types, type_id) else {
            return type_id;
        };
        let mut changed = false;
        let resolved_members: Vec<_> = members
            .iter()
            .map(|&member| {
                let resolved = self.resolve_lazy_type(member);
                let resolved = self.evaluate_application_type(resolved);
                if resolved != member {
                    changed = true;
                }
                resolved
            })
            .collect();
        if !changed {
            return type_id;
        }
        self.ctx.types.union(resolved_members)
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
                // Trigger type computation for this symbol first.
                // For CLASS symbols, this populates symbol_instance_types as a side effect.
                let resolved = self.get_type_of_symbol(sym_id);

                // For CLASS symbols in type position, prefer the instance type over the
                // constructor type. get_type_of_symbol returns the constructor (value-side)
                // type, but Lazy(DefId) in type position means the instance type.
                if let Some(&instance_type) = self.ctx.symbol_instance_types.get(&sym_id)
                    && instance_type != type_id
                {
                    return self.resolve_lazy_type_inner(instance_type, visited);
                }

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
        let current_def_id = self.ctx.get_existing_def_id(sym_id);
        if let Some(target_def_id) = query::lazy_def_id(self.ctx.types, resolved)
            && Some(target_def_id) == current_def_id
        {
            return true; // Skip self-recursive alias (A -> A)
        }

        let symbol_ref = SymbolRef(sym_id.0);
        let def_id = current_def_id;

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
            && self.ctx.get_def_type_params(def_id).is_none()
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
                // Also check class_instance_type_cache for in-progress builds
                // (Phase 2 partial type), preventing constructor type fallback.
                self.ctx
                    .symbol_instance_types
                    .get(&sym_id)
                    .copied()
                    .or_else(|| {
                        let decl = if !symbol.value_declaration.is_none() {
                            Some(symbol.value_declaration)
                        } else {
                            symbol.declarations.first().copied()
                        };
                        decl.and_then(|idx| self.ctx.class_instance_type_cache.get(&idx).copied())
                    })
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

            // Skip types already resolved in a previous call — their transitive
            // dependencies are guaranteed to be resolved too.  Without this,
            // deeply-nested Application chains (e.g., 50-deep `merge(merge(…))`)
            // cause O(N²) re-traversal of already-resolved intermediate types.
            if self.ctx.application_symbols_resolved.contains(&current) {
                resolved_types.insert(current);
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
                increment_global_resolution_fuel();
                if global_resolution_fuel_exhausted() {
                    fully_resolved = false;
                    break;
                }

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
                increment_global_resolution_fuel();

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
                let symbol = self.ctx.binder.get_symbol(sym_id);
                if symbol.is_none() {
                    continue;
                }

                // TypeQuery represents `typeof X` — a value-space query.
                // If the symbol is already registered in the environment (e.g.,
                // as a class constructor type from get_type_of_symbol), skip
                // re-resolution. type_reference_symbol_type returns the TYPE-space
                // result (instance type for classes), which would incorrectly
                // overwrite the VALUE-space result (constructor type) needed by
                // typeof expressions.
                if let Ok(env) = self.ctx.type_env.try_borrow()
                    && env.contains(tsz_solver::SymbolRef(sym_id.0))
                {
                    continue;
                }

                // Consume fuel for type query resolution
                APP_SYMBOL_RESOLUTION_FUEL.set(APP_SYMBOL_RESOLUTION_FUEL.get() + 1);
                increment_global_resolution_fuel();

                // TypeQuery (`typeof X`) resolves to the VALUE type (constructor
                // for classes), not the type-reference type (instance for classes).
                // Using `get_type_of_symbol` returns the constructor/value type,
                // while `type_reference_symbol_type` returns the instance type for
                // classes — which would incorrectly overwrite the constructor type
                // already in the TypeEnvironment.
                let is_class = symbol.is_some_and(|s| s.flags & symbol_flags::CLASS != 0);
                let resolved = if is_class {
                    self.get_type_of_symbol(sym_id)
                } else {
                    self.type_reference_symbol_type(sym_id)
                };
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
        if let Some(original_sym_id) = self.ctx.def_to_symbol_id(def_id) {
            // For CLASS symbols, prefer the instance type over the constructor
            // type returned by get_type_of_symbol.  During class construction
            // (Phase 2 of get_class_instance_type_inner), symbol_instance_types
            // is not populated yet, but class_instance_type_cache holds the
            // partial instance type.  Without this, TypeEnvironment::resolve_lazy
            // returns the constructor type (Callable), causing false TS2339 on
            // property access for self-referential parameters (e.g. `p.x` where
            // `p: Point` inside class Point).
            // If the symbol is an import ALIAS whose target is a CLASS, follow
            // it to the actual target. This handles cross-file class references
            // in module augmentations where the DefId was created for the alias.
            // Only CLASS targets are followed to avoid interfering with type-only
            // exports and other alias semantics.
            let (sym_id, symbol, was_alias_resolved) = {
                let alias_target = self.ctx.resolve_import_alias_and_register(original_sym_id);
                if let Some(target) = alias_target {
                    let target_sym = self.get_symbol_globally(target);
                    let is_class_target = target_sym
                        .is_some_and(|s| (s.flags & tsz_binder::symbol_flags::CLASS) != 0);
                    if is_class_target {
                        (target, target_sym, true)
                    } else {
                        (
                            original_sym_id,
                            self.get_symbol_globally(original_sym_id),
                            false,
                        )
                    }
                } else {
                    (
                        original_sym_id,
                        self.get_symbol_globally(original_sym_id),
                        false,
                    )
                }
            };
            let is_class = symbol.is_some_and(|s| (s.flags & tsz_binder::symbol_flags::CLASS) != 0);
            let resolved = if let Some(symbol) = symbol
                && is_class
            {
                self.ctx
                    .symbol_instance_types
                    .get(&sym_id)
                    .copied()
                    .or_else(|| {
                        let decl = if !symbol.value_declaration.is_none() {
                            Some(symbol.value_declaration)
                        } else {
                            symbol.declarations.first().copied()
                        };
                        decl.and_then(|idx| self.ctx.class_instance_type_cache.get(&idx).copied())
                    })
                    .unwrap_or_else(|| {
                        // Try building the instance type directly from the class symbol.
                        // With cross_file_symbol_targets registered by resolve_import_alias,
                        // this can delegate to a child checker with the correct arena.
                        if let Some(inst) = self.class_instance_type_from_symbol(sym_id) {
                            return inst;
                        }
                        let constructor = self.get_type_of_symbol(sym_id);
                        // Re-check: get_type_of_symbol may have populated
                        // symbol_instance_types as a side effect of class
                        // type computation. Prefer instance type over
                        // constructor for type-position references.
                        self.ctx
                            .symbol_instance_types
                            .get(&sym_id)
                            .copied()
                            .or_else(|| self.instance_type_from_constructor_type(constructor))
                            .unwrap_or(constructor)
                    })
            } else {
                self.get_type_of_symbol(sym_id)
            };

            let inserted = self.insert_type_env_symbol(sym_id, resolved);

            // When import alias resolution remapped the symbol (e.g., ALIAS
            // SymbolId → CLASS SymbolId from another file), insert_type_env_symbol
            // registers under the CLASS symbol's DefId, not the original DefId from
            // the Lazy type. Register under the original def_id so Lazy(DefId)
            // resolves correctly during property access.
            if was_alias_resolved && let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                if is_class {
                    env.insert_class_instance_type(def_id, resolved);
                }
                env.insert_def(def_id, resolved);
            }

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
