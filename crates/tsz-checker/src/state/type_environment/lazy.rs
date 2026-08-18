//! Lazy type resolution and type environment population.

use crate::query_boundaries::common::{
    TypeResolver, contains_lazy_or_recursive, enum_def_id, get_type_query_symbol_ref, lazy_def_id,
};
use crate::query_boundaries::definition_identity::symbol_ref_to_symbol_id;
use crate::query_boundaries::state::type_environment as query;
use crate::query_boundaries::type_defaults::fill_application_defaults;
use crate::query_boundaries::type_predicates::contains_conditional_with_application_extends;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_solver::TypeId;

use super::lazy_guard_state::{
    ApplicationResolutionEntryState, ApplicationResolutionWorkState,
    application_resolution_entry_state, application_resolution_local_fuel_state,
    application_resolution_post_consume_state,
};
use super::property_access_visited::PropertyAccessVisited;
use crate::query_boundaries::state::type_environment::{
    CacheEntryCollection, EvaluateTypeWithCacheOptions, for_each_direct_referenced_type,
};

/// Compatibility hook for the file-boundary guard reset path.
///
/// Lazy-readiness depth/fuel state is now stored on `EvaluationSession`, so
/// fresh checker contexts start clean and reused contexts reset through
/// `CheckerContext::reset_for_next_file`.
pub(crate) const fn reset_all_thread_local_state() {}

impl CheckerState<'_> {
    fn evaluate_type_with_env_impl(&mut self, type_id: TypeId, use_cache: bool) -> TypeId {
        use crate::query_boundaries::state::type_environment::{
            contains_infer_types_db, contains_type_query_db, evaluate_type_with_cache,
        };

        if type_id.is_intrinsic() {
            return type_id;
        }

        if use_cache && let Some(cached) = self.ctx.lookup_env_eval_cache(type_id) {
            if cached.depth_exceeded {
                self.ctx.depth_exceeded.set(true);
            }
            return cached.result;
        }

        // On a cache miss, fold a fully-concrete standard-library `Awaited<...>`
        // before delegating to the solver evaluator. The environment evaluator
        // reached here (relation source/target evaluation, annotation
        // resolution) hits the solver directly, so it needs the same fold
        // `evaluate_application_type` already applies — otherwise nested promises
        // bail to a deferred conditional and assignability reports a spurious
        // `TS2322`. See `fold_concrete_awaited_application`.
        if let Some(folded) = self.fold_concrete_awaited_application(type_id) {
            if use_cache {
                self.ctx.cache_env_eval_result(type_id, folded, false);
            }
            return folded;
        }

        // Depth guard: evaluate_type_with_env_impl can recurse through
        // ensure_relation_input_ready → resolve_and_insert_def_type →
        // get_type_of_symbol → evaluate_type_with_env_impl, causing
        // unbounded stack growth on cross-referencing module augmentations
        // (e.g., react + create-emotion-styled). The counter lives in the
        // shared evaluation session so cross-arena child contexts see the
        // same recursion budget.
        let eval_session = std::rc::Rc::clone(&self.ctx.eval_session);
        let Some(eval_depth_entry) = eval_session.enter_eval_env_depth() else {
            return type_id;
        };
        let eval_depth = eval_depth_entry.prior_depth();

        // Set this_type on the TypeEnvironment so the evaluator can resolve `keyof this`
        // and similar constructs that depend on the enclosing class type.
        let class_this_type = self.current_this_type();
        let mut set_this_type = false;
        if class_this_type.is_some()
            && let Ok(mut env) = self.ctx.type_env.try_borrow_mut()
        {
            env.set_this_type(class_this_type);
            set_this_type = true;
        }

        // Only resolve refs when not already inside an evaluate_type_with_env_impl
        // call AND not inside symbol resolution. Nested evaluation or active symbol
        // resolution can trigger compute_type_of_symbol → merge_interface_heritage_types,
        // which creates large merged types that cause OOM in the solver's evaluator
        // (module augmentations like react + create-emotion-styled).
        if eval_depth == 0
            && self.ctx.symbol_resolution_depth.get() == 0
            && self.ctx.heritage_merge_depth.get() == 0
            && !eval_session.refs_resolution_fuel_exhausted()
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

        // The intermediate seed/persist memo is a *speed-only* optimization: it
        // pre-seeds a fresh evaluator's per-run cache with already-computed
        // `(key -> value)` pairs and saves drained intermediates for later
        // runs. Because each call re-marshals the entire growing persistent
        // cache, that round-trip is O(cache_size) per call and O(N^2) across a
        // file with many alias-sharing positions. Once the cache exceeds a
        // structural soft cap the marshalling dominates the memo benefit, so we
        // skip it. Skipping never changes results — the deterministic evaluator
        // recomputes the same sub-term values — only the authoritative
        // top-level result memo (`use_cache`) below affects correctness.
        let seed_persist = use_cache && self.ctx.env_eval_seed_persist_enabled();

        // When the evaluator observes an `Application` over a `DefId` whose body
        // is not yet registered, it reports the registration window via
        // `unresolved_def_seen`. Only an opaque no-progress result from that pass
        // is the cache-poisoning artifact: persisting `input -> input` in the
        // `TypeId`-keyed `env_eval_cache` would permanently shadow the expansion
        // available after the def registers (issue #13980). A tainted pass can
        // still make useful progress on the root while observing an unresolved
        // sub-term; declaration-portability checks rely on caching those resolved
        // roots, so the poison bit below is gated on both the flag and no progress.
        //
        // The intermediate seed/persist memo remains speed-only:
        // `persist_env_eval_cache_entries` already filters unsafe shapes.
        let backstop_active =
            !crate::context::env_eval_cache::unresolved_def_cache_backstop_disabled();

        let mut depth_exceeded = false;
        let first_pass_silent_bailed;
        // Whether the first pass's result is an unresolved-def artifact that must
        // not be persisted (the solver's flag, masked by the active backstop).
        let first_pass_poisoned;
        let result = {
            // First pass: evaluate with TypeEnvironment resolver.
            let env = self.ctx.type_env.borrow();
            // PERF: Only collect seed entries when cache is non-empty. The
            // helper returns an owned Vec so no RefCell borrow overlaps
            // evaluate_type_with_cache.
            let seed_iter = if seed_persist {
                self.ctx.env_eval_cache_seed_entries()
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
                EvaluateTypeWithCacheOptions {
                    expand_application_display_alias_args: self.ctx.is_declaration_file()
                        || self.ctx.emit_declarations(),
                    query_db: Some(self.ctx.types),
                    authoritative: false,
                    cache_entry_collection: CacheEntryCollection::when_enabled(seed_persist),
                },
            );
            if eval_result.depth_exceeded {
                depth_exceeded = true;
                self.ctx.depth_exceeded.set(true);
            }
            first_pass_silent_bailed = eval_result.silent_depth_bailed;
            first_pass_poisoned =
                backstop_active && eval_result.unresolved_def_seen && eval_result.result == type_id;
            // Persist intermediate evaluation results to the shared cache. The
            // helper skips entries whose result contains unbound `infer` types
            // or type queries; the top-level poisoned root is gated below.
            if seed_persist {
                self.persist_eval_cache_entries(eval_result.cache_entries);
            }
            eval_result.result
        };

        // Second pass with CheckerContext as resolver: the first pass uses
        // TypeEnvironment which has limited Lazy resolution. If the result still
        // contains unresolved IndexAccess or Mapped types, retry with the full
        // CheckerContext resolver which can resolve Lazy(DefId) on the fly via
        // get_type_of_symbol.
        //
        // If the first pass silently bailed on structural depth AND made no
        // progress on the root (`result == type_id`), running the same walk with
        // a more powerful resolver hits the same structural protection limit at
        // the same shape — it burns roughly the same time without producing a
        // better answer. Recursive `ts-toolbelt` patterns like `ComputeDeep<A,
        // Seen>` and `_Invert<O>` reach this condition; before this gate the
        // redundant pass dominated their type-check time. The second pass still
        // runs when first-pass progress was made (`result != type_id`), since
        // the more powerful resolver may then lower sub-terms further.
        // An unchanged `Application(UnresolvedTypeName(name), args)` residue is
        // the exception to the silent-bail no-progress short-circuit: the
        // `TypeEnvironment` resolver only resolves such names from a lazily-seeded
        // map and returns `None` for an import-alias name it has not seen, so the
        // first pass *cannot* make progress regardless of depth. The residue
        // arises when a `"prop" in x` narrowing captures a receiver typed through
        // an import alias. Two things must still happen: (1) cross-arena delegate
        // the declaring def's body even when `result == type_id` (the no-progress
        // case the old `result != type_id` gate skipped) — without it
        // `resolve_lazy(def)` stays `None` because the foreign interface body was
        // never registered into this importing checker's envs; (2) run the
        // `CheckerContext` resolver pass, which recovers the def by name and
        // expands the now-registered body. Without both, property access reports
        // false `TS2339` on inherited (and own) members.
        let result_has_unresolved_application =
            crate::query_boundaries::spread::contains_unresolved_application(
                self.ctx.types,
                result,
            );
        let first_pass_made_no_progress =
            first_pass_silent_bailed && result == type_id && !result_has_unresolved_application;
        let first_pass_unresolved_application = result_has_unresolved_application;
        if first_pass_unresolved_application {
            self.resolve_unresolved_application_bodies(result);
        }
        let needs_resolver_pass = !first_pass_made_no_progress
            && (query::index_access_types(self.ctx.types, result).is_some()
                || query::mapped_type_id(self.ctx.types, result).is_some()
                || (contains_lazy_or_recursive(self.ctx.types, result)
                    && (crate::query_boundaries::common::string_intrinsic_components(
                        self.ctx.types,
                        result,
                    )
                    .is_some()
                        || crate::query_boundaries::common::is_template_literal_type(
                            self.ctx.types,
                            result,
                        )))
                // When the first pass leaves an
                // `Application(UnresolvedTypeName(...), args)` residue from
                // cross-file lowering, retry with `CheckerContext` as the
                // resolver. CheckerContext can walk the merged binder graph
                // via `resolve_unresolved_type_name`, recover the alias's
                // `DefId`, and let the application expand normally.
                || result_has_unresolved_application
                // `result != type_id` guards against re-running the second pass
                // when the first pass deferred a generic conditional unchanged
                // (type params present); we only retry when the first pass
                // actually produced a different type containing deferred
                // conditionals whose extends-type is still an Application
                // (e.g. Pick/Readonly not yet expandable by TypeEnvironment).
                || (result != type_id
                    && contains_conditional_with_application_extends(self.ctx.types, result)));
        let (final_result, final_poisoned) = if needs_resolver_pass {
            // Recompute the speed-only seed/persist gate after the first pass:
            // persisting first-pass intermediates can push the cache over the
            // structural cap, so the second pass must not reuse a stale `true`
            // decision and then drain entries that will be discarded.
            let second_pass_seed_persist = use_cache && self.ctx.env_eval_seed_persist_enabled();
            let seed_iter = if second_pass_seed_persist && !first_pass_unresolved_application {
                self.ctx.env_eval_cache_seed_entries()
            } else {
                Vec::new()
            };
            let has_seed = !seed_iter.is_empty();
            let eval_result = evaluate_type_with_cache(
                self.ctx.types,
                &self.ctx,
                if first_pass_unresolved_application {
                    result
                } else {
                    type_id
                },
                seed_iter.into_iter(),
                has_seed,
                EvaluateTypeWithCacheOptions {
                    expand_application_display_alias_args: self.ctx.is_declaration_file()
                        || self.ctx.emit_declarations(),
                    query_db: Some(self.ctx.types),
                    authoritative: true,
                    cache_entry_collection: CacheEntryCollection::when_enabled(
                        second_pass_seed_persist,
                    ),
                },
            );
            if eval_result.depth_exceeded {
                depth_exceeded = true;
                self.ctx.depth_exceeded.set(true);
            }
            let second_pass_input = if first_pass_unresolved_application {
                result
            } else {
                type_id
            };
            let second_pass_poisoned = backstop_active
                && eval_result.unresolved_def_seen
                && eval_result.result == second_pass_input;
            if second_pass_seed_persist {
                self.persist_eval_cache_entries(eval_result.cache_entries);
            }
            // When the resolver pass makes no progress (`result == type_id`),
            // `final_result` is the first pass's value, so its taint governs;
            // otherwise the resolver pass produced the value and its own flag
            // governs (issue #13980).
            if eval_result.result == type_id {
                (result, first_pass_poisoned)
            } else {
                (eval_result.result, second_pass_poisoned)
            }
        } else {
            (result, first_pass_poisoned)
        };

        // Same Infer guard for the top-level result: don't cache results
        // containing unbound infer types from partially-evaluated conditional
        // types, nor an opaque registration-window artifact from an unresolved
        // def (issue #13980).
        if use_cache
            && !final_poisoned
            && !crate::query_boundaries::common::contains_this_type(self.ctx.types, type_id)
            && !crate::query_boundaries::common::contains_this_type(self.ctx.types, final_result)
            && !contains_infer_types_db(self.ctx.types, final_result)
            && !contains_type_query_db(self.ctx.types, final_result)
        {
            self.ctx
                .cache_env_eval_result(type_id, final_result, depth_exceeded);
        }

        // Restore the this_type to avoid leaking class context into other checks.
        if set_this_type && let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
            env.set_this_type(None);
        }

        final_result
    }

    fn resolve_unresolved_application_bodies(&mut self, type_id: TypeId) {
        let names = crate::query_boundaries::spread::collect_unresolved_application_names(
            self.ctx.types,
            type_id,
        );
        let declaring_file_idx = self.unresolved_application_declaring_file_idx(type_id);
        for name in names {
            let Some(def_id) = declaring_file_idx
                .and_then(|file_idx| {
                    self.ctx
                        .resolve_unresolved_type_name_from_file(name.as_str(), file_idx)
                })
                .or_else(|| TypeResolver::resolve_unresolved_type_name(&self.ctx, name.as_str()))
            else {
                continue;
            };
            self.ctx
                .register_unresolved_resolution_in_envs(name.clone(), def_id);
            if self.ctx.definition_store.get_body(def_id).is_some() {
                continue;
            }
            let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id) else {
                continue;
            };
            let Some((body, params)) = self.delegate_cross_arena_symbol_resolution(sym_id) else {
                continue;
            };
            let params = if params.is_empty() {
                self.ctx.get_def_type_params(def_id).unwrap_or_default()
            } else {
                params
            };
            self.ctx
                .register_def_auto_params_in_envs(def_id, body, params);
        }
    }

    fn unresolved_application_declaring_file_idx(&self, type_id: TypeId) -> Option<usize> {
        let owner_def_id =
            crate::query_boundaries::spread::application_or_display_alias_lazy_def_id(
                self.ctx.types,
                type_id,
            )?;
        self.ctx
            .definition_store
            .get(owner_def_id)
            .and_then(|info| info.file_id)
            .map(|file_idx| file_idx as usize)
    }

    /// Persist evaluator cache entries to the shared `env_eval_cache`.
    ///
    /// Filters out entries that would poison the cache:
    /// - Entries containing unbound `infer` types (from partially-evaluated conditionals)
    /// - Entries containing type query references
    /// - Union→Application entries (incomplete evaluation artifacts)
    fn persist_eval_cache_entries(&self, entries: Vec<(TypeId, TypeId)>) {
        self.ctx.persist_env_eval_cache_entries(entries);
    }

    /// Evaluate a type with symbol resolution (Lazy types resolved to their concrete types).
    ///
    /// Wrapped with `stacker::maybe_grow()` to prevent stack overflow when resolving
    /// long Lazy alias chains (e.g., a chain of re-exported type aliases across modules).
    pub(crate) fn evaluate_type_with_resolution(&mut self, type_id: TypeId) -> TypeId {
        // Cycle guard against infinite mutual recursion (evaluate → prune →
        // impossible-property → evaluate) on recursive type aliases like
        // `type Box2 = Box<Box2 | number>`. Track types currently being resolved
        // (keyed by `CanonicalAppKey` to collapse import-alias variants).
        let key = crate::context::CanonicalAppKey::build(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            type_id,
        );
        if !self.ctx.type_resolution_visiting.insert(key.clone()) {
            return type_id;
        }
        let result = stacker::maybe_grow(256 * 1024, 2 * 1024 * 1024, || {
            self.evaluate_type_with_resolution_inner(type_id)
        });
        self.ctx.type_resolution_visiting.remove(&key);
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

                // Guard: when a global interface (Function, Object, RegExp, Date,
                // Error, etc.) resolves to an empty Object shape via cross-file
                // delegation, it means the interface members were not fully
                // populated. Preserve the original Lazy(DefId) so the subtype
                // checker can recognise it via `is_boxed_def_id` and apply the
                // correct intrinsic semantics (e.g., callable source ⊂ Function).
                // Without this guard, `number` becomes assignable to the empty
                // `{}` object, silencing TS2345/TS2769 errors.
                if let Some(shape_id) = crate::query_boundaries::common::object_shape_id(
                    self.ctx.types.as_type_database(),
                    resolved,
                ) {
                    let shape = self.ctx.types.object_shape(shape_id);
                    if shape.properties.is_empty()
                        && shape.string_index.is_none()
                        && shape.number_index.is_none()
                    {
                        // Narrow to Function only. Extending this guard to Object
                        // re-enters the same cross-file resolution path for any
                        // `{}`-shaped anonymous type (e.g. `Record<string, unknown>`
                        // members inside recursive mapped types), causing quadratic
                        // blow-up on patterns like `Definition<T[K]>`.
                        use crate::query_boundaries::common::IntrinsicKind;
                        let db = self.ctx.types.as_type_database();
                        if db.is_boxed_def_id(def_id, IntrinsicKind::Function) {
                            return type_id;
                        }
                    }
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

    pub(crate) fn evaluate_type_with_env(&mut self, type_id: TypeId) -> TypeId {
        self.evaluate_type_with_env_impl(type_id, true)
    }

    /// Prefer full environment evaluation before the lighter application evaluator.
    ///
    /// Imported conditional aliases can materialize nested application bases
    /// that the lighter application evaluator cannot resolve by itself.
    pub(crate) fn evaluate_property_access_receiver_type(&mut self, type_id: TypeId) -> TypeId {
        let env_evaluated = self.evaluate_type_with_env(type_id);
        if env_evaluated != type_id
            && env_evaluated != TypeId::ANY
            && env_evaluated != TypeId::ERROR
        {
            env_evaluated
        } else {
            self.evaluate_application_type(type_id)
        }
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
        let type_queries = self.ctx.collect_type_queries_cached(type_id);
        for symbol_ref in type_queries.iter().copied() {
            let sym_id = symbol_ref_to_symbol_id(symbol_ref);
            let _ = self.get_type_of_symbol(sym_id);
            let value_type = self.ctx.symbol_types.get(&sym_id).unwrap_or(TypeId::ERROR);
            // When circular resolution causes ERROR (e.g. `let Anon = class<T> {}` and
            // `typeof Anon` appears in the class body), inserting ERROR into the TypeEnvironment
            // would poison all TypeQuery resolutions for this symbol. Instead, build a minimal
            // provisional constructor Callable so the solver can satisfy `InstanceType<typeof Anon<T>>`
            // without a false-positive TS2322.
            let effective_type = if value_type == TypeId::ERROR {
                self.try_provisional_class_expr_ctor_type(sym_id)
                    .unwrap_or(TypeId::ERROR)
            } else {
                value_type
            };
            if effective_type != TypeId::ERROR {
                // Route through the env-write authority: dual-writes the flow
                // env and defers (instead of silently skipping) when either
                // env is borrowed (#14348).
                self.ctx.register_symbol_type_in_envs(
                    tsz_solver::SymbolRef(sym_id.0),
                    effective_type,
                    Vec::new(),
                );
            }
        }
    }

    /// Build a minimal provisional constructor callable for a class expression variable
    /// that is currently being circularly resolved.
    ///
    /// When `let Anon = class<T> {}` is computed and `typeof Anon` appears inside the
    /// class body (e.g. as `InstanceType<(typeof Anon<T>)>`), `get_type_of_symbol` hits
    /// a circular reference and returns `TypeId::ERROR`. The provisional callable has one
    /// construct signature per class type-parameter count, returning `any`, so the
    /// conditional `typeof Anon<T> extends abstract new (...) => infer R ? R : any`
    /// resolves to `any` instead of staying deferred.
    fn try_provisional_class_expr_ctor_type(&self, sym_id: SymbolId) -> Option<TypeId> {
        use tsz_parser::parser::base::NodeIndex;
        use tsz_parser::parser::syntax_kind_ext;

        // Only handle VARIABLE symbols (class declarations already return Lazy on circular ref).
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(symbol_flags::VARIABLE)
            || symbol.has_any_flags(symbol_flags::CLASS)
        {
            return None;
        }

        // Get the variable's primary declaration node.
        let decl_idx = symbol.primary_declaration()?;
        let decl_node = self.ctx.arena.get(decl_idx)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;

        if var_decl.initializer == NodeIndex::NONE {
            return None;
        }

        // Check if the initializer is a class expression.
        let init_node = self.ctx.arena.get(var_decl.initializer)?;
        if init_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return None;
        }

        // Count the class type parameters so the provisional sig has the right arity.
        let class_data = self.ctx.arena.get_class(init_node)?;
        let n_type_params = class_data
            .type_parameters
            .as_ref()
            .map(|tp| tp.nodes.len())
            .unwrap_or(0);

        // Build provisional type params with placeholder names.
        let prov_type_params: Vec<_> = (0..n_type_params)
            .map(|i| {
                let name = self.ctx.types.intern_string(&format!("$$prov{i}"));
                query::provisional_class_expression_type_param(name)
            })
            .collect();

        // Build construct surface: `new<$$prov0, ...>() => any`.
        // The return type is `any` so `InstanceType<typeof Anon<T>>` reduces to `any`
        // during circular resolution, which is assignable to everything.
        Some(query::provisional_class_expression_constructor_type(
            self.ctx.types,
            prov_type_params,
        ))
    }

    pub(crate) fn evaluate_type_with_env_uncached(&mut self, type_id: TypeId) -> TypeId {
        self.evaluate_type_with_env_impl(type_id, false)
    }

    /// Evaluate a type for TS2589 detection at type alias definition sites.
    ///
    /// Like `evaluate_type_with_env_uncached` but uses an evaluator that flags
    /// `depth_exceeded` when cycle detection fires on an Application type.
    /// This catches self-referential conditional types that produce the same
    /// Application TypeId on each expansion.
    ///
    /// Returns true if depth was exceeded (TS2589 should be emitted).
    pub(crate) fn evaluate_type_for_ts2589_check(
        &mut self,
        type_id: TypeId,
        alias_def_id: tsz_solver::def::DefId,
    ) -> bool {
        let env = self.ctx.type_env.borrow();
        // First try: evaluate with flag that detects Application cycles
        let eval_result =
            crate::query_boundaries::state::type_environment::evaluate_type_for_ts2589(
                self.ctx.types,
                &*env,
                type_id,
            );
        if eval_result.depth_exceeded {
            return true;
        }

        // Second check: a concrete self-application of the alias can survive the
        // first evaluation because the evaluator leaves a recursive reference in a
        // non-tail position (a function return or object/mapped property, e.g. the
        // `Curry<T, R>` inside `(h: H) => Curry<T, R>`) deferred — so a residual
        // `Application(alias, args)` is the norm, not proof of infinite expansion.
        // It is divergence evidence only when it makes no *progress*: at a use site
        // (the checked type is itself a concrete application of the alias) compare
        // the structural argument weight of the input against each residual. A
        // residual whose argument weight is strictly larger than the input grows on
        // every step along an unbounded dimension (a template-literal string that
        // gains characters, a tuple that gains elements) and is genuinely
        // divergent. A residual that stays the same size or shrinks is *not* proof
        // of divergence:
        //   * it may shrink along a dimension the coarse metric scores flat — a
        //     numeric depth counter (`N` -> `Exclude<N, 0>`) or a structural descent
        //     into `T[K]` — and so terminate at a base case (e.g. `DeepObject<T, N>`);
        //   * or it may tie a finite knot the way `tsc` defers recursive object and
        //     mapped-property references (`{ [K in keyof T]: Rec<T[K]> }`), which is
        //     accepted, not flagged.
        // The same-identity stall (`Foo<unknown>` -> `Foo<unknown>`) that this check
        // once caught here is already detected earlier as an Application cycle
        // (`eval_result.depth_exceeded`), and any residual the weight metric cannot
        // see is still bounded by the per-`DefId` instantiation-depth limit, so
        // requiring strict growth here only removes false positives. When there is
        // no input application to compare against (the definition-site pass evaluates
        // the conditional body directly), any surviving concrete self-reference stays
        // divergent, preserving definition-site TS2589.
        use crate::query_boundaries::state::type_environment as qb;
        let result = eval_result.result;
        if result != type_id && result != TypeId::ERROR {
            let db = self.ctx.types.as_type_database();
            let weight = |t| qb::self_application_arg_weight(db, &*env, t, alias_def_id);
            match weight(type_id) {
                // Definition-site pass: any surviving concrete self-reference is
                // divergent.
                None => {
                    let residuals =
                        qb::collect_concrete_applications_with_def(db, result, alias_def_id);
                    return !residuals.is_empty();
                }
                // Only a residual at an *eager* position is divergence evidence;
                // one `tsc` defers (object property / function / mapped template)
                // is a finite knot, not infinite instantiation (#17028). See
                // `collect_eager_concrete_applications_with_def`.
                Some(input_weight) => {
                    let residuals =
                        qb::collect_eager_concrete_applications_with_def(db, result, alias_def_id);
                    let diverges = residuals
                        .iter()
                        .any(|&r| weight(r).is_none_or(|rw| rw > input_weight));
                    if diverges {
                        return true;
                    }
                }
            }
        }
        false
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

    /// When `type_id` is a union with at least one `Application` member, evaluate
    /// those application members through the type environment and return the
    /// rebuilt union. Returns `None` when `type_id` is not such a union or no
    /// application member made progress, so callers can fall through to their
    /// normal resolution path.
    fn resolve_union_application_members(&mut self, type_id: TypeId) -> Option<TypeId> {
        use crate::query_boundaries::state::type_environment::is_application_type;
        let members = crate::query_boundaries::common::union_members(self.ctx.types, type_id)?;
        if !members
            .iter()
            .any(|&m| is_application_type(self.ctx.types, m))
        {
            return None;
        }
        let mut changed = false;
        let resolved: Vec<TypeId> = members
            .iter()
            .map(|&member| {
                if is_application_type(self.ctx.types, member) {
                    let evaluated = self.evaluate_application_type_for_property_access(member);
                    if evaluated != member {
                        changed = true;
                    }
                    evaluated
                } else {
                    member
                }
            })
            .collect();
        changed.then(|| self.ctx.types.union(resolved))
    }

    /// Like [`Self::resolve_type_for_property_access`] but always materializes an
    /// eligible lib-interface `Lazy` receiver instead of leaving it lazy. Used by
    /// the property-access path when the lazy single-member fast path missed
    /// (e.g. a heritage-inherited member) and the full structural shape is needed.
    pub(crate) fn resolve_type_for_property_access_force(&mut self, type_id: TypeId) -> TypeId {
        self.ensure_relation_input_ready(type_id);
        let mut visited = PropertyAccessVisited::default();
        self.resolve_type_for_property_access_inner(type_id, &mut visited)
    }

    pub(crate) fn resolve_type_for_property_access(&mut self, mut type_id: TypeId) -> TypeId {
        // A union whose members are `Application(Lazy(DefId), …)` instantiations
        // of generic lib references (e.g. `Int32Array | Uint8Array`) keeps those
        // members opaque under the solver's environment-free evaluator, hiding
        // the referenced interface's members and index signatures. Such unions
        // arise from narrowing or constraint-position substitution, where the
        // members are interned in their raw application form rather than the
        // resolved object form a directly-declared union would carry. Resolve
        // the application members through the type environment first so property
        // and element access see the interface shape; the loop below then runs
        // the normal per-member resolution on the resolved union. This precedes
        // the per-id resolve cache, which can otherwise return a stale identity
        // entry recorded on an earlier pass before the members were resolvable.
        // Bounded by a fuel counter (matching `resolve_type_uncached`'s cycle
        // guard): a cross-file generic union member can re-evaluate to a
        // freshly interned but structurally-equal application each pass, so
        // unbounded recursion here previously stack-overflowed instead of
        // reaching a fixed point.
        let mut fuel = 100;
        while fuel > 0 {
            fuel -= 1;
            match self.resolve_union_application_members(type_id) {
                Some(resolved) if resolved != type_id => type_id = resolved,
                _ => break,
            }
        }

        // Lazy single-member fast path: a bare `Lazy(DefId)` reference to a
        // simple lib interface is left unresolved here so the property-access
        // member lookup can resolve only the accessed member instead of
        // materializing the interface's full shape (e.g. `document.title`).
        // The named-property lookup sites re-resolve the single member via
        // `try_lazy_lib_member_property_access`; any consumer needing the full
        // shape (keyof/spread/relation) still calls `ensure_relation_input_ready`
        // on the bare Lazy itself. Eligibility is gated by the
        // `TSZ_DISABLE_LAZY_MEMBER_ACCESS` kill-switch.
        if self.lazy_lib_member_receiver_def_id(type_id).is_some() {
            return type_id;
        }

        if let Some(&cached) = self
            .ctx
            .flow_shared
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
                .flow_shared
                .narrowing_cache
                .resolve_cache
                .borrow_mut()
                .insert(type_id, type_id);
            return type_id;
        }

        self.ensure_relation_input_ready(type_id);

        let mut visited = PropertyAccessVisited::default();
        let result = self.resolve_type_for_property_access_inner(type_id, &mut visited);
        // Use entry().or_insert() to avoid overwriting a value that evaluate_application_type
        // may have stored in this cache during the inner call above. For homomorphic mapped
        // types over union constraints (e.g. `T extends [number] | readonly [string]`),
        // evaluate_application_type correctly produces a union of the mapped members, but
        // resolve_type_for_property_access_inner strips ReadonlyType wrappers, causing both
        // union members to deduplicate to a single tuple — losing the readonly variant.
        //
        // Return the *cache entry* (i.e. whichever value won the entry/or_insert race),
        // not the local `result`. Otherwise the first caller would see the stripped
        // tuple while every subsequent caller sees the correct cached union, which
        // makes type-checking results depend on call order. The fix keeps both caller
        // paths in sync with whatever evaluate_application_type pre-populated.
        *self
            .ctx
            .flow_shared
            .narrowing_cache
            .resolve_cache
            .borrow_mut()
            .entry(type_id)
            .or_insert(result)
    }

    pub(crate) fn resolve_type_for_property_access_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut PropertyAccessVisited,
    ) -> TypeId {
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
                // A bare reference to a generic type whose parameters all have
                // defaults is still an instantiation in type position. Property
                // access must see the instantiated body, not the raw alias body,
                // or nested member signatures can leak unsubstituted parameters
                // (for example `Chainable<Config = {}>["option"]` retaining
                // `keyof Config` in its conditional key parameter).
                if let Some(type_params) = self.ctx.get_def_type_params(def_id)
                    && !type_params.is_empty()
                    && type_params.iter().all(|p| p.default.is_some())
                    && let Some(default_args) =
                        fill_application_defaults(self.ctx.types, &[], &type_params)
                {
                    let app = self.ctx.types.application(type_id, default_args);
                    let evaluated = self.evaluate_application_type(app);
                    if evaluated != type_id && evaluated != app {
                        let resolved =
                            self.resolve_type_for_property_access_inner(evaluated, visited);
                        self.ctx.leave_recursion();
                        return resolved;
                    }
                }

                // First consult the type environment. Cross-file interface and
                // alias references commonly register their structural body there
                // even when the current binder cannot re-compute the symbol.
                let env_resolved = if let Ok(env) = self.ctx.type_env.try_borrow() {
                    TypeResolver::resolve_lazy(&*env, def_id, self.ctx.types)
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
                            if symbol.has_any_flags(symbol_flags::ENUM)
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
                            if symbol.has_any_flags(symbol_flags::CLASS) {
                                // Try the symbol-indexed cache first (populated
                                // after class building completes).
                                let cached = self.ctx.symbol_instance_types.get(&sym_id);

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
                                    symbol.primary_declaration().and_then(|idx| {
                                        self.ctx
                                            .class_instance_type_cache
                                            .borrow()
                                            .get(&idx)
                                            .copied()
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
                let resolved = self.get_type_of_symbol(symbol_ref_to_symbol_id(sym_ref));
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
                let evaluated = self.evaluate_application_type_for_property_access(type_id);
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
                        let checkpoint = visited.checkpoint();
                        let resolved = self.resolve_type_for_property_access_inner(member, visited);
                        visited.rollback_to(checkpoint);
                        resolved
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

    /// Read a definition body only when it represents progress beyond the
    /// unresolved alias placeholder.
    fn registered_alias_body(&self, def_id: tsz_solver::DefId) -> Option<TypeId> {
        let is_usable = |body: TypeId| {
            body != TypeId::ERROR
                && body != TypeId::UNKNOWN
                && lazy_def_id(self.ctx.types, body) != Some(def_id)
        };

        if let Ok(env) = self.ctx.type_env.try_borrow()
            && let Some(body) = TypeResolver::resolve_lazy(&*env, def_id, self.ctx.types)
            && is_usable(body)
        {
            return Some(body);
        }

        self.ctx
            .definition_store
            .get_body(def_id)
            .filter(|&body| is_usable(body))
    }

    /// Materialize a canonical standard-library type-alias body on demand.
    ///
    /// Lib alias lowering deliberately returns `Lazy(DefId)` so generic
    /// applications retain their alias identity, while publishing the actual
    /// body into the definition store and both type environments as a side
    /// effect. On-demand interface forcing does not own alias lowering, so a
    /// consumer that first reaches a member's alias-typed result must trigger
    /// that publication through the mutable checker boundary and then re-read
    /// the body for the same canonical `DefId`.
    pub(super) fn materialize_actual_lib_alias_body(
        &mut self,
        def_id: tsz_solver::DefId,
    ) -> Option<TypeId> {
        if !self.ctx.has_lib_loaded()
            // This helper runs from relation-readiness hot paths. Reject ordinary
            // program aliases before resolving their names or scanning lib binders;
            // the canonical actual-lib identity check below remains authoritative
            // for non-program ambient definitions.
            || !self.ctx.definition_store.def_is_non_program(def_id)
            || self.ctx.definition_store.get_kind(def_id)
                != Some(tsz_solver::def::DefKind::TypeAlias)
        {
            return None;
        }

        let name_atom = self.ctx.definition_store.get_name(def_id)?;
        let name = self.ctx.types.resolve_atom(name_atom);
        if self.ctx.actual_lib_def_id_for_bare_name(&name) != Some(def_id) {
            return None;
        }

        if let Some(body) = self.registered_alias_body(def_id) {
            return Some(body);
        }
        if self.lib_name_resolution_in_progress(&name) {
            return None;
        }

        // The return value is intentionally the public Lazy wrapper for type
        // aliases. The structural body is the side effect read below.
        let _ = self.resolve_lib_type_by_name(&name);
        self.registered_alias_body(def_id)
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
                if let Some(resolved) = TypeResolver::resolve_lazy(&*env, def_id, self.ctx.types)
                    && resolved != type_id
                {
                    drop(env);
                    // Register resolved type → DefId so TypeFormatter can recover
                    // the named display (e.g., "Num" instead of structural expansion).
                    // Only register for interfaces and classes — NOT type aliases.
                    // tsc expands type alias bodies in error messages but preserves
                    // interface/class names.
                    if resolved != TypeId::ERROR
                        && resolved != TypeId::ANY
                        && resolved != TypeId::UNKNOWN
                        && self
                            .ctx
                            .definition_store
                            .find_def_for_type(resolved)
                            .is_none()
                        && self.ctx.definition_store.get(def_id).is_some_and(|def| {
                            matches!(
                                def.kind,
                                tsz_solver::def::DefKind::Interface
                                    | tsz_solver::def::DefKind::Class
                            )
                        })
                    {
                        self.ctx
                            .definition_store
                            .register_type_to_def(resolved, def_id);
                    }
                    return self.resolve_lazy_type_inner(resolved, visited);
                }
                drop(env);
            }

            // Try to look up the definition's body in the definition store.
            // A class def's published body can be the VALUE (constructor) side
            // — never a valid type-position resolution (#17570).
            if let Some(body) = self
                .ctx
                .definition_store
                .get_body(def_id)
                .filter(|&body| !self.ctx.is_class_value_side_body(def_id, body))
            {
                // Recursively resolve in case the body is also a lazy type
                return self.resolve_lazy_type_inner(body, visited);
            }

            // If not in the definition store or type_env, try to resolve via symbol lookup
            // This handles type aliases that are resolved through compute_type_of_symbol
            //
            // Raw `SymbolId`s are binder-relative: a def minted in a delegated
            // child checker (lib binder) carries a lib-local id that can
            // collide with an unrelated symbol in THIS binder. Resolving the
            // colliding id would silently substitute that symbol's type (e.g.
            // `JSX.Element` resolving to `parseInt`'s type, issue #15687), so
            // the fallback only fires when this binder's symbol at that id
            // actually names the def.
            let sym_id_opt = self
                .ctx
                .def_to_symbol_id(def_id)
                .filter(|&sym_id| self.ctx.def_matches_local_symbol(def_id, sym_id));
            if let Some(sym_id) = sym_id_opt {
                // Trigger type computation for this symbol first.
                // For CLASS symbols, this populates symbol_instance_types as a side effect.
                let resolved = self.get_type_of_symbol(sym_id);

                // For CLASS symbols in type position, prefer the instance type over the
                // constructor type. get_type_of_symbol returns the constructor (value-side)
                // type, but Lazy(DefId) in type position means the instance type.
                if let Some(instance_type) = self.ctx.symbol_instance_types.get(&sym_id)
                    && instance_type != type_id
                {
                    return self.resolve_lazy_type_inner(instance_type, visited);
                }

                // A CLASS symbol whose instance type is still unavailable (its
                // build is deferred because one of its own member initializers
                // is in flight) must stay a deferred `Lazy` — substituting the
                // constructor-shaped value-side result here swaps in `typeof C`
                // for a type position, which then fails constraint checks the
                // instance satisfies (spurious TS2344 on `[R extends C]` inside
                // `C`'s own property initializers, #17570). A later resolution —
                // after the class statement finishes building — takes the
                // `symbol_instance_types` branch above.
                if self
                    .ctx
                    .binder
                    .symbols
                    .get(sym_id)
                    .is_some_and(|s| s.has_any_flags(tsz_binder::symbol_flags::CLASS))
                    && self.ctx.is_class_value_side_body(def_id, resolved)
                {
                    return type_id;
                }

                // Only recurse if the resolved type is different from the original
                if resolved != type_id {
                    return self.resolve_lazy_type_inner(resolved, visited);
                }
            }

            // Fourth fallback: resolve actual-lib aliases and interfaces by name.
            //
            // When a lib interface (e.g., ProxyConstructor) is referenced in a type
            // annotation (e.g., `declare var Proxy: ProxyConstructor`), the Lazy(DefId)
            // may not have a SymbolId mapping or type_env entry if the lib file's
            // checker context didn't propagate them to the main context. The
            // DefinitionStore still has the name, so we can materialize the interface
            // type through the lib type resolution system.
            // Keep the actual-lib forcing path completely out of no-lib and
            // ordinary program resolution. Besides avoiding shared-store work
            // on a hot fallback, this preserves the existing cross-file
            // program-definition ordering when no library can contribute the
            // requested alias.
            if self.ctx.has_lib_loaded()
                && self.ctx.definition_store.def_is_non_program(def_id)
                && let Some(body) = self.materialize_actual_lib_alias_body(def_id)
            {
                return self.resolve_lazy_type_inner(body, visited);
            }
            if self.ctx.has_lib_loaded()
                && self.ctx.definition_store.get_kind(def_id)
                    == Some(tsz_solver::def::DefKind::Interface)
                && let Some(name_atom) = self.ctx.definition_store.get_name(def_id)
            {
                let name = self.ctx.types.resolve_atom(name_atom);
                if let Some(lib_type) = self.resolve_lib_type_by_name(&name)
                    && lib_type != type_id
                    && lib_type != TypeId::ERROR
                    && lib_type != TypeId::ANY
                {
                    // Re-check the type_env: resolve_lib_type_by_name
                    // materializes the interface and registers it in
                    // the type_env as a side effect.
                    let env = self.ctx.type_env.borrow();
                    if let Some(resolved) =
                        TypeResolver::resolve_lazy(&*env, def_id, self.ctx.types)
                        && resolved != type_id
                    {
                        drop(env);
                        return self.resolve_lazy_type_inner(resolved, visited);
                    }
                    drop(env);
                    // If type_env still doesn't have it, use the lib type directly
                    return self.resolve_lazy_type_inner(lib_type, visited);
                }
            }
        }

        // Handle unions and intersections - resolve each member
        // Only create a new union/intersection if members actually changed
        if let Some(resolved) = crate::query_boundaries::common::map_compound_members_if_changed(
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

        let already_resolved = self.ctx.application_symbols_resolved.contains(&type_id);
        let inserted_active_visit = if already_resolved {
            true
        } else {
            self.ctx.application_symbols_resolution_set.insert(type_id)
        };
        let eval_session = std::rc::Rc::clone(&self.ctx.eval_session);
        let depth = eval_session.app_symbol_resolution_depth();
        let entry_state = application_resolution_entry_state(
            already_resolved,
            inserted_active_visit,
            eval_session.app_symbol_resolution_fuel(),
            eval_session.app_symbol_resolution_fuel_limit(),
            depth,
            eval_session.app_symbol_resolution_depth_limit(),
        );
        let is_outermost = match entry_state {
            ApplicationResolutionEntryState::Entered { outermost } => outermost,
            ApplicationResolutionEntryState::AlreadyResolved
            | ApplicationResolutionEntryState::AlreadyVisiting => return,
            ApplicationResolutionEntryState::FuelExhausted
            | ApplicationResolutionEntryState::DepthExceeded => {
                self.ctx.application_symbols_resolution_set.remove(&type_id);
                return;
            }
        };
        if is_outermost {
            // Reset fuel for each top-level resolution
            eval_session.reset_app_symbol_resolution_fuel();
        }
        let app_symbol_depth_entry = eval_session.enter_app_symbol_resolution_depth();
        debug_assert_eq!(app_symbol_depth_entry.outermost(), is_outermost);

        let mut visited: FxHashSet<TypeId> = FxHashSet::default();
        let fully_resolved = self.ensure_application_symbols_resolved_inner(type_id, &mut visited);
        self.ctx.application_symbols_resolution_set.remove(&type_id);
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
        // A symbol lookup for a materialized standard-library alias can still
        // observe its registration-window `UNKNOWN` result. The symbol cache
        // may retain that result, but the `DefId` cache must keep the structural
        // body already published by alias lowering. Otherwise this direct env
        // bridge makes definition publication non-monotone and `keyof` sees an
        // opaque alias again.
        let definition_body = def_id.and_then(|def_id| {
            self.ctx
                .definition_body_for_env_registration(def_id, resolved)
        });
        let definition_registration = def_id.zip(definition_body);

        // Reuse cached params already in the environment when available.
        let mut cached_env_params: Option<Vec<tsz_solver::TypeParamInfo>> = None;
        let mut symbol_already_registered = false;
        let mut def_already_registered = def_id.is_none();
        if let Ok(env) = self.ctx.type_env.try_borrow() {
            symbol_already_registered = env.contains(symbol_ref);
            cached_env_params = env.get_params(symbol_ref).map(|s| s.to_vec());
            if let Some((def_id, body)) = definition_registration {
                def_already_registered = env.get_def(def_id) == Some(body);
            }
        }
        let had_env_params = cached_env_params.is_some();
        let type_params = if let Some(params) = cached_env_params {
            params
        } else if let Some(def_id) = def_id {
            match self.ctx.get_def_type_params(def_id) {
                Some(params)
                    if !params.is_empty()
                        && params
                            .iter()
                            .all(|param| param.constraint.is_none() && param.default.is_none()) =>
                {
                    self.get_type_params_for_symbol(sym_id)
                }
                Some(params) => params,
                None => self.get_type_params_for_symbol(sym_id),
            }
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
        // This can happen during recursive type resolution. On contention,
        // queue the writes through the context authority but report the
        // traversal as incomplete so callers do not memoize a fully-resolved
        // walk before the evaluator env has replayed the deferred write.
        if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
            if type_params.is_empty() {
                self.ctx
                    .insert_symbol_type_and_mirror(&mut env, symbol_ref, resolved, Vec::new());
                if let Some((def_id, body)) = definition_registration {
                    env.insert_def(def_id, body);
                }
            } else {
                self.ctx.insert_symbol_type_and_mirror(
                    &mut env,
                    symbol_ref,
                    resolved,
                    type_params.clone(),
                );
                if let Some((def_id, body)) = definition_registration {
                    env.insert_def_with_params(def_id, body, type_params.clone());
                }
            }
            drop(env);
            if let Some((def_id, body)) = definition_registration {
                self.mirror_application_def_resolution(Some(def_id), body, &type_params);
            }
            def_id.is_none() || definition_registration.is_some()
        } else {
            self.ctx
                .register_symbol_type_in_envs(symbol_ref, resolved, type_params.clone());
            if let Some((def_id, body)) = definition_registration {
                self.ctx
                    .register_def_auto_params_in_envs(def_id, body, type_params);
            }
            false
        }
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
        let mut seen_type_queries: rustc_hash::FxHashSet<tsz_solver::SymbolRef> =
            rustc_hash::FxHashSet::default();
        let mut resolved_types: rustc_hash::FxHashSet<TypeId> = rustc_hash::FxHashSet::default();

        while let Some(current) = worklist.pop() {
            // Check local application-symbol fuel - bail if exhausted
            // (prevents unbounded work on deeply-nested generic type graphs
            // like react16.d.ts).
            let eval_session = std::rc::Rc::clone(&self.ctx.eval_session);
            match application_resolution_local_fuel_state(
                eval_session.app_symbol_resolution_fuel(),
                eval_session.app_symbol_resolution_fuel_limit(),
            ) {
                ApplicationResolutionWorkState::Continue => {}
                ApplicationResolutionWorkState::LocalFuelExhausted
                | ApplicationResolutionWorkState::GlobalFuelExhausted => {
                    fully_resolved = false;
                    break;
                }
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

            for_each_direct_referenced_type(self.ctx.types, current, |next| {
                worklist.push(next);
            });

            if let Some(def_id) = lazy_def_id(self.ctx.types, current) {
                if !seen_def_ids.insert(def_id) {
                    continue;
                }

                // Consume only readiness-local fuel here. The resolver's
                // actual type-resolution work charges the shared lazy fuel
                // through `CheckerContext::consume_fuel`.
                eval_session.increment_app_symbol_resolution_fuel();
                match application_resolution_post_consume_state(
                    eval_session.lazy_resolution_fuel_exhausted(),
                ) {
                    ApplicationResolutionWorkState::Continue => {}
                    ApplicationResolutionWorkState::GlobalFuelExhausted
                    | ApplicationResolutionWorkState::LocalFuelExhausted => {
                        fully_resolved = false;
                        break;
                    }
                }

                match self.resolve_lazy_def_for_type_env(def_id) {
                    Some((inserted, resolved)) => {
                        fully_resolved &= inserted;
                        // Lazy own-member lowering (`TSZ_LAZY_OWN_MEMBERS`): a
                        // non-generic lib interface is resolved (its member SET is
                        // now in the env, so `keyof`/index lookups work), but we do
                        // NOT push its resolved body onto the worklist. That stops
                        // the transitive walk from resolving every member type's
                        // body — which would force each member interface's full
                        // heritage merge (the method-call materialization tax, e.g.
                        // `document.createElement("div")` forcing HTMLDivElement's
                        // entire closure). Member types stay `Lazy` and resolve on
                        // demand (#8638). Inert flag-off.
                        let keep_members_lazy =
                            crate::state_checking::lazy_lib_member::lazy_own_members_enabled()
                                && self.force_eligible_lib_def(def_id);
                        if resolved != TypeId::ANY
                            && resolved != TypeId::ERROR
                            && !keep_members_lazy
                        {
                            worklist.push(resolved);
                        }
                    }
                    None => {
                        fully_resolved = false;
                    }
                }
            } else if let Some(def_id) = enum_def_id(self.ctx.types, current) {
                if !seen_def_ids.insert(def_id) {
                    continue;
                }

                // Consume only readiness-local fuel here. The resolver's
                // actual type-resolution work charges the shared lazy fuel.
                eval_session.increment_app_symbol_resolution_fuel();
                match application_resolution_post_consume_state(
                    eval_session.lazy_resolution_fuel_exhausted(),
                ) {
                    ApplicationResolutionWorkState::Continue => {}
                    ApplicationResolutionWorkState::GlobalFuelExhausted
                    | ApplicationResolutionWorkState::LocalFuelExhausted => {
                        fully_resolved = false;
                        break;
                    }
                }

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
            } else if let Some(symbol_ref) = get_type_query_symbol_ref(self.ctx.types, current) {
                if !seen_type_queries.insert(symbol_ref) {
                    continue;
                }

                let sym_id = symbol_ref_to_symbol_id(symbol_ref);
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

                // Consume only readiness-local fuel here. The symbol resolver's
                // actual type-resolution work charges the shared lazy fuel.
                eval_session.increment_app_symbol_resolution_fuel();
                match application_resolution_post_consume_state(
                    eval_session.lazy_resolution_fuel_exhausted(),
                ) {
                    ApplicationResolutionWorkState::Continue => {}
                    ApplicationResolutionWorkState::GlobalFuelExhausted
                    | ApplicationResolutionWorkState::LocalFuelExhausted => {
                        fully_resolved = false;
                        break;
                    }
                }

                let resolved = if symbol.as_ref().is_some_and(|s| {
                    s.has_any_flags(symbol_flags::TYPE_ALIAS | symbol_flags::VARIABLE)
                }) {
                    let value_decl =
                        symbol.map_or(tsz_parser::NodeIndex::NONE, |s| s.value_declaration);
                    self.type_of_value_declaration_for_symbol(sym_id, value_decl)
                } else {
                    self.get_type_of_symbol(sym_id)
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
        if let Some(body) = self.published_program_alias_body(def_id) {
            if body != TypeId::ANY {
                self.try_insert_def_in_type_env(def_id, body);
            }
            return Some((true, body));
        }
        if let Some((original_sym_id, owner_file_idx)) = self.ctx.def_symbol_identity(def_id) {
            if let Some(file_idx) = owner_file_idx
                && file_idx != self.ctx.current_file_idx
            {
                self.ctx
                    .register_symbol_file_target(original_sym_id, file_idx);
            }
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
                    let target_sym = self.get_cross_file_symbol(target);
                    let is_class_target = target_sym
                        .is_some_and(|s| s.has_any_flags(tsz_binder::symbol_flags::CLASS));
                    if is_class_target {
                        (target, target_sym, true)
                    } else {
                        (
                            original_sym_id,
                            self.get_cross_file_symbol(original_sym_id),
                            false,
                        )
                    }
                } else {
                    (
                        original_sym_id,
                        self.get_cross_file_symbol(original_sym_id),
                        false,
                    )
                }
            };
            let is_class = symbol.is_some_and(|s| s.has_any_flags(tsz_binder::symbol_flags::CLASS));
            let resolved = if let Some(symbol) = symbol
                && is_class
            {
                self.ctx
                    .symbol_instance_types
                    .get(&sym_id)
                    .or_else(|| {
                        symbol.primary_declaration().and_then(|idx| {
                            self.ctx
                                .class_instance_type_cache
                                .borrow()
                                .get(&idx)
                                .copied()
                        })
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
            if was_alias_resolved {
                if is_class {
                    self.ctx.register_class_instance_in_envs(def_id, resolved);
                }
                // Register the original alias `DefId` through the same
                // evaluator/flow authority so a recursive borrow queues the
                // write instead of dropping it (#14348).
                self.ctx.register_def_in_envs(def_id, resolved);
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
        if let Some((sym_id, owner_file_idx)) = self.ctx.def_symbol_identity(def_id) {
            if let Some(file_idx) = owner_file_idx
                && file_idx != self.ctx.current_file_idx
            {
                self.ctx.register_symbol_file_target(sym_id, file_idx);
            }
            let resolved = self.type_reference_symbol_type(sym_id);
            let inserted = self.insert_type_env_symbol(sym_id, resolved);
            Some((inserted, resolved))
        } else {
            None
        }
    }
}
