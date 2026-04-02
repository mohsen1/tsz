//! Type evaluation for meta-types (conditional, mapped, index access).
//!
//! Meta-types are "type-level functions" that compute output types from input types.
//! This module provides evaluation logic for:
//! - Conditional types: T extends U ? X : Y
//! - Distributive conditional types: (A | B) extends U ? X : Y
//! - Index access types: T[K]
//!
//! Key design:
//! - Lazy evaluation: only evaluate when needed for subtype checking
//! - Handles deferred evaluation when type parameters are unknown
//! - Supports distributivity for naked type parameters in unions

use crate::TypeDatabase;
use crate::caches::db::QueryDatabase;
use crate::def::DefId;
use crate::instantiation::instantiate::instantiate_generic;
use crate::relations::subtype::{NoopResolver, TypeResolver};
#[cfg(test)]
use crate::types::*;
use crate::types::{
    ConditionalType, ConditionalTypeId, MappedType, MappedTypeId, StringIntrinsicKind,
    TemplateLiteralId, TemplateSpan, TupleElement, TupleListId, TypeApplicationId, TypeData,
    TypeId, TypeListId, TypeParamInfo,
};
use crate::visitors::visitor_predicates::{contains_type_matching, is_primitive_type};
use rustc_hash::{FxHashMap, FxHashSet};

/// Controls which subtype direction makes a member redundant when simplifying
/// a union or intersection.
enum SubtypeDirection {
    /// member[i] <: member[j] → member[i] is redundant (union semantics).
    SourceSubsumedByOther,
    /// member[j] <: member[i] → member[i] is redundant (intersection semantics).
    OtherSubsumedBySource,
}

/// Type evaluator for meta-types.
///
/// # Salsa Preparation
/// This struct uses `&mut self` methods instead of `RefCell` + `&self`.
/// This makes the evaluator thread-safe (Send) and prepares for future
/// Salsa integration where state is managed by the database runtime.
pub struct TypeEvaluator<'a, R: TypeResolver = NoopResolver> {
    interner: &'a dyn TypeDatabase,
    /// Optional query database for Salsa-backed memoization.
    query_db: Option<&'a dyn QueryDatabase>,
    resolver: &'a R,
    no_unchecked_indexed_access: bool,
    cache: FxHashMap<TypeId, TypeId>,
    /// Unified recursion guard for `TypeId` cycle detection, depth, and iteration limits.
    guard: crate::recursion::RecursionGuard<TypeId>,
    /// Per-DefId recursion depth counter.
    /// Allows recursive type aliases (like `TrimRight`) to expand up to `MAX_DEF_DEPTH`
    /// times before stopping, matching tsc's TS2589 "Type instantiation is excessively
    /// deep and possibly infinite" behavior. Unlike a set-based cycle detector, this
    /// permits legitimate bounded recursion where each expansion converges.
    def_depth: FxHashMap<DefId, u32>,
    /// When true, suppress `this` type substitution during Lazy type evaluation.
    /// Used during intersection evaluation to prevent premature `this` binding to
    /// individual members instead of the full intersection type.
    suppress_this_binding: bool,
    /// PERF: Cache for subtype check results used in conditional type evaluation.
    /// Key: (`check_type`, `extends_type`), Value: `is_subtype`.
    /// Deeply recursive conditional types (`DeepReadonly`, `Compute`, etc.) often check
    /// the same (check, extends) pair many times across distributed branches and
    /// tail-recursion iterations. Caching avoids redundant structural comparison.
    conditional_subtype_cache: FxHashMap<(TypeId, TypeId), bool>,
    /// Ceiling for eager mapped-key expansion before bailing out.
    max_mapped_keys: usize,
}

#[cfg(target_arch = "wasm32")]
const DEFAULT_MAX_MAPPED_KEYS: usize = 250;
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_MAX_MAPPED_KEYS: usize = 500;

/// Array methods that return any (used for apparent type computation).
pub(crate) const ARRAY_METHODS_RETURN_ANY: &[&str] = &[
    "concat",
    "filter",
    "flat",
    "flatMap",
    "map",
    "reverse",
    "slice",
    "sort",
    "splice",
    "toReversed",
    "toSorted",
    "toSpliced",
    "with",
    "at",
    "find",
    "findLast",
    "pop",
    "shift",
    "entries",
    "keys",
    "values",
    "reduce",
    "reduceRight",
];
/// Array methods that return boolean.
pub(crate) const ARRAY_METHODS_RETURN_BOOLEAN: &[&str] = &["every", "includes", "some"];
/// Array methods that return number.
pub(crate) const ARRAY_METHODS_RETURN_NUMBER: &[&str] = &[
    "findIndex",
    "findLastIndex",
    "indexOf",
    "lastIndexOf",
    "push",
    "unshift",
];
/// Array methods that return void.
pub(crate) const ARRAY_METHODS_RETURN_VOID: &[&str] = &["forEach", "copyWithin", "fill"];
/// Array methods that return string.
pub(crate) const ARRAY_METHODS_RETURN_STRING: &[&str] = &["join", "toLocaleString", "toString"];

impl<'a> TypeEvaluator<'a, NoopResolver> {
    /// Create a new evaluator without a resolver.
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        static NOOP: NoopResolver = NoopResolver;
        TypeEvaluator {
            interner,
            query_db: None,
            resolver: &NOOP,
            no_unchecked_indexed_access: false,
            cache: FxHashMap::default(),
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::TypeEvaluation,
            ),
            def_depth: FxHashMap::default(),
            suppress_this_binding: false,
            conditional_subtype_cache: FxHashMap::default(),
            max_mapped_keys: DEFAULT_MAX_MAPPED_KEYS,
        }
    }
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    fn has_nested_complex_marker(&self, type_id: TypeId) -> bool {
        contains_type_matching(self.interner, type_id, |key| {
            matches!(
                key,
                TypeData::Conditional(_)
                    | TypeData::Mapped(_)
                    | TypeData::IndexAccess(_, _)
                    | TypeData::KeyOf(_)
                    | TypeData::TypeQuery(_)
                    | TypeData::TemplateLiteral(_)
                    | TypeData::ReadonlyType(_)
                    | TypeData::StringIntrinsic { .. }
                    | TypeData::ThisType
                    | TypeData::Lazy(_)
                    | TypeData::Application(_)
            )
        })
    }

    /// Maximum recursive expansion depth for a single `DefId`.
    /// Matches TypeScript's instantiation depth limit that triggers TS2589.
    const MAX_DEF_DEPTH: u32 = 100;

    /// Create a new evaluator with a custom resolver.
    pub fn with_resolver(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        TypeEvaluator {
            interner,
            query_db: None,
            resolver,
            no_unchecked_indexed_access: false,
            cache: FxHashMap::default(),
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::TypeEvaluation,
            ),
            def_depth: FxHashMap::default(),
            suppress_this_binding: false,
            conditional_subtype_cache: FxHashMap::default(),
            max_mapped_keys: DEFAULT_MAX_MAPPED_KEYS,
        }
    }

    /// Set the query database for Salsa-backed memoization.
    pub fn with_query_db(mut self, db: &'a dyn QueryDatabase) -> Self {
        self.query_db = Some(db);
        self
    }

    /// Suppress `this` type substitution during Lazy type evaluation.
    /// When set, `ThisType` references inside resolved Lazy types are preserved
    /// rather than being bound to the Lazy type's own identity. This is used
    /// during interface heritage merging so that `this` can later be correctly
    /// bound to the final derived interface type.
    pub const fn with_suppress_this_binding(mut self) -> Self {
        self.suppress_this_binding = true;
        self
    }

    /// Drain the evaluator's internal cache, returning all intermediate results.
    /// This allows callers to persist intermediate evaluation results
    /// (e.g., from recursive mapped type expansion) into a longer-lived cache.
    pub fn drain_cache(&mut self) -> impl Iterator<Item = (TypeId, TypeId)> + '_ {
        self.cache.drain()
    }

    /// Pre-seed the evaluator's cache with previously computed evaluation results.
    /// This prevents re-evaluation of intermediate types (e.g., nested generic
    /// applications) that were already computed in earlier evaluator runs.
    pub fn seed_cache(&mut self, entries: impl Iterator<Item = (TypeId, TypeId)>) {
        self.cache.extend(entries);
    }

    pub fn set_no_unchecked_indexed_access(&mut self, enabled: bool) {
        if self.no_unchecked_indexed_access != enabled {
            self.cache.clear();
        }
        self.no_unchecked_indexed_access = enabled;
    }

    pub const fn set_max_mapped_keys(&mut self, max_mapped_keys: usize) {
        self.max_mapped_keys = max_mapped_keys;
    }

    /// Reset per-evaluation state so this evaluator can be reused.
    ///
    /// Clears the cache, cycle detection sets, and counters while preserving
    /// configuration and borrowed references. Uses `.clear()` to reuse memory.
    #[inline]
    pub fn reset(&mut self) {
        self.cache.clear();
        self.guard.reset();
        self.def_depth.clear();
    }

    // =========================================================================
    // Accessor methods for evaluate_rules modules
    // =========================================================================

    /// Get the type interner.
    #[inline]
    pub(crate) fn interner(&self) -> &'a dyn TypeDatabase {
        self.interner
    }

    /// Get the type resolver.
    #[inline]
    pub(crate) const fn resolver(&self) -> &'a R {
        self.resolver
    }

    #[inline]
    pub(crate) const fn max_mapped_keys(&self) -> usize {
        self.max_mapped_keys
    }

    /// Get the query database when one is available.
    #[inline]
    pub(crate) const fn query_db(&self) -> Option<&'a dyn QueryDatabase> {
        self.query_db
    }

    /// PERF: Look up a cached subtype result from conditional type evaluation.
    #[inline]
    pub(crate) fn cached_conditional_subtype(
        &self,
        check: TypeId,
        extends: TypeId,
    ) -> Option<bool> {
        self.conditional_subtype_cache
            .get(&(check, extends))
            .copied()
    }

    /// PERF: Cache a subtype result from conditional type evaluation.
    #[inline]
    pub(crate) fn cache_conditional_subtype(
        &mut self,
        check: TypeId,
        extends: TypeId,
        result: bool,
    ) {
        self.conditional_subtype_cache
            .insert((check, extends), result);
    }

    /// Check if `no_unchecked_indexed_access` is enabled.
    #[inline]
    pub(crate) const fn no_unchecked_indexed_access(&self) -> bool {
        self.no_unchecked_indexed_access
    }

    /// Check if depth limit was exceeded.
    #[inline]
    pub const fn is_depth_exceeded(&self) -> bool {
        self.guard.is_exceeded()
    }

    /// Mark the guard as exceeded, causing subsequent evaluations to bail out.
    ///
    /// Used when an external condition (e.g. mapped key count or distribution
    /// size exceeds its limit) means further recursive evaluation should stop.
    #[inline]
    pub(crate) const fn mark_depth_exceeded(&mut self) {
        self.guard.mark_exceeded();
    }

    /// Instantiate an Application type WITHOUT recursively evaluating the result.
    ///
    /// For tail-call optimization in conditional types: expands `TrimLeft<T>`
    /// to its body with args substituted, but does NOT call `evaluate()` on
    /// the result. This avoids incrementing the depth guard, allowing the
    /// tail-call loop in `evaluate_conditional` to handle the result directly.
    ///
    /// Returns `Some(instantiated_body)` if the type is an Application that
    /// could be instantiated. Returns `None` if the type is not an Application,
    /// or if it couldn't be resolved/instantiated.
    pub(crate) fn try_instantiate_application_for_tail_call(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let app_id = match self.interner.lookup(type_id) {
            Some(TypeData::Application(app_id)) => app_id,
            _ => return None,
        };

        let app = self.interner.type_application(app_id);

        let base_key = self.interner.lookup(app.base)?;
        let def_id = match base_key {
            TypeData::Lazy(def_id) => Some(def_id),
            TypeData::TypeQuery(sym_ref) => self.resolver.symbol_to_def_id(sym_ref),
            _ => None,
        }?;

        let type_params = self.resolver.get_lazy_type_params(def_id)?;
        let resolved = self.resolver.resolve_lazy(def_id, self.interner)?;

        // Do NOT check/increment def_depth for tail-call instantiations.
        // The caller (tail-call loop in evaluate_conditional) has its own
        // MAX_TAIL_RECURSION_DEPTH (1000) limit. Incrementing def_depth here
        // defeats tail-call optimization by hitting MAX_DEF_DEPTH (100) first.
        // Example: `type Trim<S> = S extends ` ${infer T}` ? Trim<T> : S`
        // needs 128+ iterations for long strings.

        // Expand type arguments
        let body_is_conditional_with_app_infer =
            self.is_conditional_with_application_infer(resolved);
        let expanded_args: std::borrow::Cow<'_, [TypeId]> = if body_is_conditional_with_app_infer {
            std::borrow::Cow::Owned(self.expand_type_args_preserve_applications(&app.args))
        } else {
            self.expand_type_args(&app.args)
        };

        // Instantiate the body with the type arguments — but do NOT evaluate
        let instantiated =
            instantiate_generic(self.interner, resolved, &type_params, &expanded_args);
        Some(instantiated)
    }

    /// Global thread-local depth counter for cross-evaluator stack overflow prevention.
    ///
    /// Each `SubtypeChecker::evaluate_type` creates a fresh `TypeEvaluator` with fresh
    /// per-evaluator guards. But the OS stack accumulates across ALL of them. For example,
    /// `Vector<T> implements Seq<T>` where `Opt<T>` has `toVector(): Vector<T>` and
    /// `Vector` has `Exclude<T, U>` in an overload return type: each structural comparison
    /// level creates ~8 evaluate calls, and the subtype checker recurses 10+ levels deep,
    /// producing 100+ nested evaluate frames that overflow the 8MB default stack.
    ///
    /// This counter tracks cumulative `evaluate` frames across all `TypeEvaluator` instances
    /// on the current thread's call stack. When it exceeds `MAX_GLOBAL_EVAL_DEPTH`, we
    /// bail out with ERROR to prevent stack overflow.
    const MAX_GLOBAL_EVAL_DEPTH: u32 = 200;

    /// Evaluate a type, resolving any meta-types if possible.
    /// Returns the evaluated type (may be the same if no evaluation needed).
    #[inline]
    pub fn evaluate(&mut self, type_id: TypeId) -> TypeId {
        // Fast path for intrinsics
        if type_id.is_intrinsic() {
            return type_id;
        }

        // Fast path: check local cache BEFORE depth checks.
        // Most evaluate() calls are for already-evaluated types (cache hits),
        // so checking the cache first avoids unnecessary guard operations.
        if let Some(&cached) = self.cache.get(&type_id) {
            return cached;
        }

        // Check if depth was already exceeded in a previous call
        if self.guard.is_exceeded() {
            return TypeId::ERROR;
        }

        // Cross-evaluator stack overflow prevention.
        // Only check thread-local global depth when the local guard depth
        // is already significant (>= 10). This avoids expensive TLS access
        // on the vast majority of shallow evaluations.
        if self.guard.depth() >= 10 {
            thread_local! {
                static GLOBAL_EVAL_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
            }
            let global_depth = GLOBAL_EVAL_DEPTH.with(|d| {
                let v = d.get();
                d.set(v + 1);
                v
            });
            if global_depth >= Self::MAX_GLOBAL_EVAL_DEPTH {
                GLOBAL_EVAL_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
                self.guard.mark_exceeded();
                return TypeId::ERROR;
            }
            let result = self.evaluate_guarded(type_id);
            GLOBAL_EVAL_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            return result;
        }

        self.evaluate_guarded(type_id)
    }

    /// Inner evaluate logic, called after global depth check.
    ///
    /// Wrapped with `stacker::maybe_grow()` so that deeply nested conditional/
    /// mapped type chains (ts-toolbelt, ts-essentials) can grow the stack
    /// dynamically instead of crashing even if the logical recursion guard
    /// has not yet tripped.
    fn evaluate_guarded(&mut self, type_id: TypeId) -> TypeId {
        stacker::maybe_grow(256 * 1024, 2 * 1024 * 1024, || {
            self.evaluate_guarded_inner(type_id)
        })
    }

    /// Interval for checking global evaluation fuel.
    ///
    /// We amortize the atomic load by only checking the global fuel counter
    /// every N iterations of the per-evaluator guard. This keeps the hot path
    /// fast while still catching runaway expansion within a few hundred iterations.
    const FUEL_CHECK_INTERVAL: u32 = 128;

    /// Actual evaluate logic -- separated so `stacker::maybe_grow` can wrap it.
    fn evaluate_guarded_inner(&mut self, type_id: TypeId) -> TypeId {
        use crate::recursion::RecursionResult;

        if let Some(&cached) = self.cache.get(&type_id) {
            return cached;
        }

        // Unified enter: checks iterations, depth, cycle detection, and visiting set size
        match self.guard.enter(type_id) {
            RecursionResult::Entered => {}
            RecursionResult::Cycle => {
                // Recursion guard for self-referential mapped/application types.
                // Recursive mapped types must stay deferred here. Collapsing them to
                // `{}` loses the constraint structure and can incorrectly make
                // self-referential generic constraints look satisfied.
                let key = self.interner.lookup(type_id);
                if matches!(key, Some(TypeData::Mapped(_))) {
                    self.cache.insert(type_id, type_id);
                    return type_id;
                }
                return type_id;
            }
            RecursionResult::DepthExceeded => {
                self.cache.insert(type_id, TypeId::ERROR);
                return TypeId::ERROR;
            }
            RecursionResult::IterationExceeded => {
                self.cache.insert(type_id, type_id);
                return type_id;
            }
        }

        // Global fuel check: amortized to every FUEL_CHECK_INTERVAL iterations.
        // This prevents deeply recursive type libraries (ts-toolbelt, ts-essentials)
        // from consuming unbounded memory through type instantiation that creates
        // new TypeIds on each expansion. Mirrors tsc's global `instantiationCount`.
        if self
            .guard
            .iterations()
            .is_multiple_of(Self::FUEL_CHECK_INTERVAL)
            && self
                .interner
                .consume_evaluation_fuel(Self::FUEL_CHECK_INTERVAL)
        {
            self.guard.mark_exceeded();
            self.guard.leave(type_id);
            self.cache.insert(type_id, TypeId::ERROR);
            return TypeId::ERROR;
        }

        let key = match self.interner.lookup(type_id) {
            Some(k) => k,
            None => {
                self.guard.leave(type_id);
                return type_id;
            }
        };

        // Visitor pattern: dispatch to appropriate visit_* method
        let result = self.visit_type_key(type_id, &key);

        // Symmetric cleanup: leave guard and cache result
        self.guard.leave(type_id);
        self.cache.insert(type_id, result);

        result
    }

    /// Evaluate a generic type application: Base<Args>
    ///
    /// Algorithm:
    /// 1. Look up the base type - if it's a Ref, resolve it
    /// 2. Get the type parameters for the base symbol
    /// 3. If we have type params, instantiate the resolved type with args
    /// 4. Recursively evaluate the result
    fn evaluate_application(
        &mut self,
        app_id: TypeApplicationId,
        original_type_id: TypeId,
    ) -> TypeId {
        let app = self.interner.type_application(app_id);

        // Look up the base type
        let base_key = match self.interner.lookup(app.base) {
            Some(k) => k,
            // The application was already interned — return its original TypeId
            // instead of cloning args to re-intern the same thing.
            None => return original_type_id,
        };

        // Task B: Resolve TypeQuery bases to DefId for expansion
        // This fixes the "Ref(5)<error>" diagnostic issue where generic types
        // aren't expanded to their underlying function/object types
        // Note: Ref(SymbolRef) was migrated to Lazy(DefId)
        let def_id = match base_key {
            TypeData::Lazy(def_id) => Some(def_id),
            TypeData::TypeQuery(sym_ref) => self.resolver.symbol_to_def_id(sym_ref),
            _ => None,
        };

        tracing::trace!(
            base = app.base.0,
            base_key = ?base_key,
            def_id = ?def_id,
            num_args = app.args.len(),
            "evaluate_application"
        );
        // If the base is a DefId (Lazy, Ref, or TypeQuery), try to resolve and instantiate
        if let Some(def_id) = def_id {
            // =======================================================================
            // PER-DEFID DEPTH LIMITING
            // =======================================================================
            // This catches expansive recursion in type aliases like `type T<X> = T<Box<X>>`
            // that produce new TypeIds on each evaluation, bypassing the `visiting` set.
            //
            // Unlike a set-based cycle detector (which blocks ANY re-entry), we use a
            // per-DefId counter that allows up to MAX_DEF_DEPTH recursive expansions.
            // This correctly handles legitimate recursive types like:
            //   type TrimRight<S> = S extends `${infer R} ` ? TrimRight<R> : S;
            // which need multiple re-entries of the same DefId to converge.
            // =======================================================================
            let depth = self.def_depth.entry(def_id).or_insert(0);
            if *depth >= Self::MAX_DEF_DEPTH {
                self.guard.mark_exceeded();
                return TypeId::ERROR;
            }
            *depth += 1;

            // Try to get the type parameters for this DefId
            let type_params = self.resolver.get_lazy_type_params(def_id);
            let resolved = self.resolver.resolve_lazy(def_id, self.interner);

            tracing::trace!(
                ?def_id,
                has_type_params = type_params.is_some(),
                type_params_count = type_params.as_ref().map(std::vec::Vec::len),
                has_resolved = resolved.is_some(),
                resolved_key = ?resolved.and_then(|r| self.interner.lookup(r)),
                "evaluate_application resolve"
            );
            let result = if let Some(type_params) = type_params {
                // Resolve the base type to get the body
                if let Some(resolved) = resolved {
                    // Pre-expand type arguments that are TypeQuery or Application.
                    // For conditional type bodies with Application extends containing infer,
                    // preserve Application args so the conditional evaluator can match
                    // at the Application level (e.g., Promise<string> vs Promise<infer U>).
                    let body_is_conditional_with_app_infer =
                        self.is_conditional_with_application_infer(resolved);
                    let expanded_args: std::borrow::Cow<'_, [TypeId]> =
                        if body_is_conditional_with_app_infer {
                            std::borrow::Cow::Owned(
                                self.expand_type_args_preserve_applications(&app.args),
                            )
                        } else {
                            self.expand_type_args(&app.args)
                        };
                    let no_unchecked_indexed_access = self.no_unchecked_indexed_access;

                    if let Some(db) = self.query_db
                        && let Some(cached) = db.lookup_application_eval_cache(
                            def_id,
                            &expanded_args,
                            no_unchecked_indexed_access,
                        )
                    {
                        if let Some(d) = self.def_depth.get_mut(&def_id) {
                            *d = d.saturating_sub(1);
                        }
                        return cached;
                    }

                    // HOMOMORPHIC MAPPED TYPE PASSTHROUGH FOR NON-OBJECT ARGUMENTS
                    // tsc's `instantiateMappedType` checks: if the resolved body is a
                    // homomorphic mapped type (constraint is `keyof T`) and the type
                    // argument for T is not an object type, return the argument directly.
                    // This makes `Partial<number>` = `number`, `DeepReadonly<string>` = `string`.
                    //
                    // We MUST check this BEFORE instantiation because `instantiate_type`
                    // eagerly evaluates `keyof T` when T is concrete, destroying the
                    // structural information needed for passthrough detection later.
                    if let Some(TypeData::Mapped(mapped_id)) = self.interner.lookup(resolved) {
                        let mapped = self.interner.get_mapped(mapped_id);
                        if let Some(TypeData::KeyOf(source)) =
                            self.interner.lookup(mapped.constraint)
                            && let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(source)
                            && let Some(idx) = type_params.iter().position(|p| p.name == tp.name)
                            && idx < expanded_args.len()
                        {
                            let arg = expanded_args[idx];
                            let resolved_arg = self.evaluate(arg);
                            // Passthrough for genuine primitives (number, string, boolean, etc.)
                            // For `any`: only passthrough when the type parameter is constrained
                            // to array/tuple types (e.g., `Arrayish<T extends unknown[]>`).
                            // Otherwise, `any` must flow through mapped type expansion to produce
                            // `{ [x: string]: any }` (matching tsc's behavior for `Objectish<any>`).
                            let is_any_like = resolved_arg == TypeId::ANY
                                || resolved_arg == TypeId::UNKNOWN
                                || resolved_arg == TypeId::NEVER
                                || resolved_arg == TypeId::ERROR;
                            let should_passthrough = if is_any_like {
                                // Check if the type parameter has an array/tuple constraint
                                tp.constraint.is_some_and(|c| {
                                    let eval_c = self.evaluate(c);
                                    matches!(
                                        self.interner.lookup(eval_c),
                                        Some(TypeData::Array(_) | TypeData::Tuple(_))
                                    )
                                })
                            } else {
                                is_primitive_type(self.interner, resolved_arg)
                            };
                            if should_passthrough {
                                if let Some(db) = self.query_db {
                                    db.insert_application_eval_cache(
                                        def_id,
                                        &expanded_args,
                                        no_unchecked_indexed_access,
                                        resolved_arg,
                                    );
                                }
                                if let Some(d) = self.def_depth.get_mut(&def_id) {
                                    *d = d.saturating_sub(1);
                                }
                                return resolved_arg;
                            }
                            // Objectish<any>: identity homomorphic mapped type with `any` arg
                            // and non-array constraint. tsc produces `{ [x: string]: any; [x: number]: any }`
                            // (NOT `any`). This ensures `Objectish<any>` is not assignable to `any[]`.
                            // Previously this was handled by checker-local object construction;
                            // now centralized in solver for architectural correctness.
                            if resolved_arg == TypeId::ANY
                                && let Some((obj, key)) =
                                    crate::index_access_parts(self.interner, mapped.template)
                                && obj == source
                                && matches!(
                                    self.interner.lookup(key),
                                    Some(TypeData::TypeParameter(kp)) if kp.name == mapped.type_param.name
                                )
                            {
                                use crate::types::{IndexSignature, ObjectShape};
                                let result = self.interner.object_with_index(ObjectShape {
                                    flags: crate::types::ObjectFlags::empty(),
                                    properties: vec![],
                                    string_index: Some(IndexSignature {
                                        key_type: TypeId::STRING,
                                        value_type: TypeId::ANY,
                                        readonly: false,
                                        param_name: None,
                                    }),
                                    number_index: Some(IndexSignature {
                                        key_type: TypeId::NUMBER,
                                        value_type: TypeId::ANY,
                                        readonly: false,
                                        param_name: None,
                                    }),
                                    symbol: None,
                                });
                                if let Some(db) = self.query_db {
                                    db.insert_application_eval_cache(
                                        def_id,
                                        &expanded_args,
                                        no_unchecked_indexed_access,
                                        result,
                                    );
                                }
                                if let Some(d) = self.def_depth.get_mut(&def_id) {
                                    *d = d.saturating_sub(1);
                                }
                                return result;
                            }
                        }
                    }

                    // CLASS INSTANCE TYPE EXTRACTION
                    // When a class type (Callable with construct signatures) is used in
                    // type position via Application (e.g., `Component<P, S>`), we need the
                    // INSTANCE type, not the class constructor type. Extract the instance
                    // type from the first construct signature's return type.
                    // This handles cases where class_instance_types wasn't populated for
                    // the DefId (e.g., lib types referenced indirectly via interfaces).
                    //
                    // Only apply for DefKind::Class, NOT for interfaces with construct
                    // signatures (e.g., `ComponentClass<P>`). Interfaces should keep their
                    // Callable shape with construct signatures intact.
                    let is_class_def = matches!(
                        self.resolver.get_def_kind(def_id),
                        Some(crate::def::DefKind::Class)
                    );
                    let effective_body = if is_class_def
                        && let Some(TypeData::Callable(cs_id)) = self.interner.lookup(resolved)
                    {
                        let shape = self.interner.callable_shape(cs_id);
                        if let Some(construct_sig) = shape.construct_signatures.first() {
                            construct_sig.return_type
                        } else {
                            resolved
                        }
                    } else {
                        resolved
                    };

                    // Instantiate the resolved type with the type arguments.
                    // Then rebind polymorphic `this` to the concrete application
                    // so interface bodies like `constraint: Constraint<this>`
                    // preserve their receiver-specific invariance.
                    let mut instantiated = instantiate_generic(
                        self.interner,
                        effective_body,
                        &type_params,
                        &expanded_args,
                    );
                    if crate::contains_this_type(self.interner, instantiated) {
                        // Use original_type_id as the app_type — it's the same
                        // Application(base, args) that was already interned.
                        instantiated = crate::substitute_this_type(
                            self.interner,
                            instantiated,
                            original_type_id,
                        );
                    }
                    // Preserve discriminated object intersections after instantiation.
                    // Re-evaluating them here distributes impossible branches again,
                    // which breaks both fresh EPC and `keyof` on generic applications.
                    let evaluated = if crate::type_queries::is_discriminated_object_intersection(
                        self.interner,
                        instantiated,
                    ) {
                        instantiated
                    } else {
                        self.evaluate(instantiated)
                    };
                    let evaluated = crate::type_queries::prune_impossible_object_union_members(
                        self.interner,
                        evaluated,
                    );
                    if let Some(db) = self.query_db {
                        db.insert_application_eval_cache(
                            def_id,
                            &expanded_args,
                            no_unchecked_indexed_access,
                            evaluated,
                        );
                    }
                    evaluated
                } else {
                    original_type_id
                }
            } else if let Some(resolved) = resolved {
                // Fallback: try to extract type params from the resolved type's properties
                let extracted_params = self.extract_type_params_from_type(resolved);
                if !extracted_params.is_empty() && extracted_params.len() == app.args.len() {
                    // Pre-expand type arguments
                    let expanded_args = self.expand_type_args(&app.args);
                    let no_unchecked_indexed_access = self.no_unchecked_indexed_access;

                    if let Some(db) = self.query_db
                        && let Some(cached) = db.lookup_application_eval_cache(
                            def_id,
                            &expanded_args,
                            no_unchecked_indexed_access,
                        )
                    {
                        if let Some(d) = self.def_depth.get_mut(&def_id) {
                            *d = d.saturating_sub(1);
                        }
                        return cached;
                    }

                    let mut instantiated = instantiate_generic(
                        self.interner,
                        resolved,
                        &extracted_params,
                        &expanded_args,
                    );
                    if crate::contains_this_type(self.interner, instantiated) {
                        instantiated = crate::substitute_this_type(
                            self.interner,
                            instantiated,
                            original_type_id,
                        );
                    }
                    let evaluated = if crate::type_queries::is_discriminated_object_intersection(
                        self.interner,
                        instantiated,
                    ) {
                        instantiated
                    } else {
                        self.evaluate(instantiated)
                    };
                    if let Some(db) = self.query_db {
                        db.insert_application_eval_cache(
                            def_id,
                            &expanded_args,
                            no_unchecked_indexed_access,
                            evaluated,
                        );
                    }
                    evaluated
                } else {
                    original_type_id
                }
            } else {
                original_type_id
            };

            // Decrement per-DefId depth after evaluation
            if let Some(d) = self.def_depth.get_mut(&def_id) {
                *d = d.saturating_sub(1);
            }

            // Store reverse mapping for diagnostic display: when the evaluated
            // result differs from the original Application, record the mapping
            // so the formatter can display `Dictionary<string>` instead of the
            // expanded `{ [index: string]: string; }`.
            // Only store when args are fully concrete to avoid conflating
            // generic contexts where the same type arises from different sources.
            if result != original_type_id
                && !app.args.iter().any(|&arg| {
                    crate::type_queries::contains_type_parameters_db(self.interner, arg)
                })
            {
                self.interner.store_display_alias(result, original_type_id);
            }

            result
        } else {
            // If we can't expand, return the original application
            original_type_id
        }
    }

    /// Check if a type is a Conditional whose `extends_type` is an Application containing infer.
    /// This detects patterns like `T extends Promise<infer U> ? U : T`.
    fn is_conditional_with_application_infer(&self, type_id: TypeId) -> bool {
        let Some(TypeData::Conditional(cond_id)) = self.interner.lookup(type_id) else {
            return false;
        };
        let cond = self.interner.get_conditional(cond_id);
        matches!(
            self.interner.lookup(cond.extends_type),
            Some(TypeData::Application(_))
        )
    }

    /// Like `expand_type_args` but preserves Application types without evaluating them.
    /// Used for conditional type bodies so the conditional evaluator can match
    /// at the Application level for infer pattern matching.
    fn expand_type_args_preserve_applications(&mut self, args: &[TypeId]) -> Vec<TypeId> {
        // Fast path: check if any non-Application arg needs expansion.
        let needs_expansion = args.iter().any(|&arg| {
            if arg.is_intrinsic() {
                return false;
            }
            matches!(
                self.interner.lookup(arg),
                Some(
                    TypeData::TypeQuery(_)
                        | TypeData::Conditional(_)
                        | TypeData::Mapped(_)
                        | TypeData::TemplateLiteral(_)
                        | TypeData::KeyOf(_)
                        | TypeData::Lazy(_)
                )
            )
        });
        if !needs_expansion {
            return args.to_vec();
        }
        let mut expanded = Vec::with_capacity(args.len());
        for &arg in args {
            let Some(key) = self.interner.lookup(arg) else {
                expanded.push(arg);
                continue;
            };
            match key {
                TypeData::Application(_) => {
                    expanded.push(arg);
                }
                _ => expanded.push(self.try_expand_type_arg(arg)),
            }
        }
        expanded
    }

    /// Expand type arguments by evaluating any that are `TypeQuery` or Application.
    /// Uses a loop instead of closure to allow mutable self access.
    pub(crate) fn expand_type_args<'b>(
        &mut self,
        args: &'b [TypeId],
    ) -> std::borrow::Cow<'b, [TypeId]> {
        // Fast path: check if any arg needs expansion before allocating.
        // Most type args are simple types that pass through unchanged.
        let needs_expansion = args.iter().any(|&arg| self.needs_type_arg_expansion(arg));
        if !needs_expansion {
            return std::borrow::Cow::Borrowed(args);
        }
        let mut expanded = Vec::with_capacity(args.len());
        for &arg in args {
            expanded.push(self.try_expand_type_arg(arg));
        }
        std::borrow::Cow::Owned(expanded)
    }

    /// Check if a type arg needs expansion (without actually expanding it).
    #[inline]
    fn needs_type_arg_expansion(&self, arg: TypeId) -> bool {
        if arg.is_intrinsic() {
            return false;
        }
        matches!(
            self.interner.lookup(arg),
            Some(
                TypeData::TypeQuery(_)
                    | TypeData::Application(_)
                    | TypeData::Conditional(_)
                    | TypeData::Mapped(_)
                    | TypeData::TemplateLiteral(_)
                    | TypeData::KeyOf(_)
                    | TypeData::Lazy(_)
            )
        )
    }

    /// Extract type parameter infos from a type by scanning for `TypeParameter` types.
    fn extract_type_params_from_type(&self, type_id: TypeId) -> Vec<TypeParamInfo> {
        let mut seen = FxHashSet::default();
        let mut params = Vec::new();
        self.collect_type_params(type_id, &mut seen, &mut params);
        params
    }

    /// Recursively collect `TypeParameter` types from a type.
    fn collect_type_params(
        &self,
        type_id: TypeId,
        seen: &mut FxHashSet<tsz_common::interner::Atom>,
        params: &mut Vec<TypeParamInfo>,
    ) {
        if type_id.is_intrinsic() {
            return;
        }

        let Some(key) = self.interner.lookup(type_id) else {
            return;
        };

        match key {
            TypeData::TypeParameter(ref info) => {
                if !seen.contains(&info.name) {
                    seen.insert(info.name);
                    params.push(*info);
                }
            }
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.collect_type_params(prop.type_id, seen, params);
                }
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                for param in &shape.params {
                    self.collect_type_params(param.type_id, seen, params);
                }
                self.collect_type_params(shape.return_type, seen, params);
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    self.collect_type_params(member, seen, params);
                }
            }
            TypeData::Array(elem) => {
                self.collect_type_params(elem, seen, params);
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(cond_id);
                self.collect_type_params(cond.check_type, seen, params);
                self.collect_type_params(cond.extends_type, seen, params);
                self.collect_type_params(cond.true_type, seen, params);
                self.collect_type_params(cond.false_type, seen, params);
            }
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.collect_type_params(app.base, seen, params);
                for &arg in &app.args {
                    self.collect_type_params(arg, seen, params);
                }
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.get_mapped(mapped_id);
                // Note: mapped.type_param is the iteration variable (e.g., K in "K in keyof T")
                // We should NOT add it directly - the outer type param (T) is found in the constraint.
                // For DeepPartial<T> = { [K in keyof T]?: DeepPartial<T[K]> }:
                //   - type_param is K (iteration var, NOT the outer param)
                //   - constraint is "keyof T" (contains T, the actual param to extract)
                //   - template is DeepPartial<T[K]> (also contains T)
                self.collect_type_params(mapped.constraint, seen, params);
                self.collect_type_params(mapped.template, seen, params);
                if let Some(name_type) = mapped.name_type {
                    self.collect_type_params(name_type, seen, params);
                }
            }
            TypeData::KeyOf(operand) => {
                // Extract type params from the operand of keyof
                // e.g., keyof T -> extract T
                self.collect_type_params(operand, seen, params);
            }
            TypeData::IndexAccess(obj, idx) => {
                // Extract type params from both object and index
                // e.g., T[K] -> extract T and K
                self.collect_type_params(obj, seen, params);
                self.collect_type_params(idx, seen, params);
            }
            TypeData::TemplateLiteral(spans) => {
                // Extract type params from template literal interpolations
                let spans = self.interner.template_list(spans);
                for span in spans.iter() {
                    if let TemplateSpan::Type(inner) = span {
                        self.collect_type_params(*inner, seen, params);
                    }
                }
            }
            _ => {}
        }
    }

    /// Try to expand a type argument that may be a `TypeQuery` or Application.
    /// Returns the expanded type, or the original if it can't be expanded.
    /// This ensures type arguments are resolved before instantiation.
    ///
    /// NOTE: This method uses `self.evaluate()` for Application, Conditional, Mapped,
    /// and `TemplateLiteral` types to ensure recursion depth limits are enforced.
    fn try_expand_type_arg(&mut self, arg: TypeId) -> TypeId {
        let Some(key) = self.interner.lookup(arg) else {
            return arg;
        };
        match key {
            TypeData::TypeQuery(sym_ref) => {
                // Resolve the TypeQuery to get the VALUE type (constructor for classes).
                // Use resolve_type_query which returns constructor types for classes,
                // unlike resolve_ref which may return instance types.
                if let Some(resolved) = self.resolver.resolve_type_query(sym_ref, self.interner) {
                    resolved
                } else if let Some(def_id) = self.resolver.symbol_to_def_id(sym_ref) {
                    self.resolver
                        .resolve_lazy(def_id, self.interner)
                        .unwrap_or(arg)
                } else {
                    arg
                }
            }
            TypeData::Application(_)
            | TypeData::Conditional(_)
            | TypeData::Mapped(_)
            | TypeData::TemplateLiteral(_)
            | TypeData::KeyOf(_) => {
                // Use evaluate() to ensure depth limits are enforced.
                // KeyOf must be expanded here so that after generic instantiation,
                // the mapped type constraint and template reference the same source
                // object TypeId (critical for homomorphic mapped type detection).
                self.evaluate(arg)
            }
            TypeData::Lazy(def_id) => {
                // Resolve Lazy types in type arguments
                // This helps with generic instantiation accuracy
                self.resolver
                    .resolve_lazy(def_id, self.interner)
                    .unwrap_or(arg)
            }
            _ => arg,
        }
    }

    /// Check if a type is "complex" and requires full evaluation for identity.
    ///
    /// Complex types are those whose structural identity depends on evaluation context:
    /// - `TypeParameter`: Opaque until instantiation
    /// - Lazy: Requires resolution
    /// - Conditional: Requires evaluation of extends clause
    /// - Mapped: Requires evaluation of mapped type
    /// - `IndexAccess`: Requires evaluation of T[K]
    /// - `KeyOf`: Requires evaluation of keyof
    /// - Application: Requires expansion of Base<Args>
    /// - `TypeQuery`: Requires resolution of typeof
    /// - `TemplateLiteral`: Requires evaluation of template parts
    /// - `ReadonlyType`: Wraps another type
    /// - `StringIntrinsic`: Uppercase, Lowercase, Capitalize, Uncapitalize
    ///
    /// These types are NOT safe for simplification because bypassing evaluation
    /// would produce incorrect results (e.g., treating T[K] as a distinct type from
    /// the value it evaluates to).
    ///
    /// ## Task #37: Deep Structural Simplification
    ///
    /// After implementing the Canonicalizer (Task #32), we can now safely handle
    /// `Lazy` (type aliases) and `Application` (generics) structurally. These types
    /// are now "unlocked" for simplification because:
    /// - `Lazy` types are canonicalized using De Bruijn indices
    /// - `Application` types are recursively canonicalized
    /// - The `SubtypeChecker`'s fast-path (Task #36) uses O(1) structural identity
    ///
    /// Types that remain "complex" are those that are **inherently deferred**:
    /// - `TypeParameter`, `Infer`: Waiting for generic substitution
    /// - `Conditional`, `Mapped`, `IndexAccess`, `KeyOf`: Require type-level computation
    /// - These cannot be compared structurally until they are fully evaluated
    fn is_complex_type(&self, type_id: TypeId) -> bool {
        let Some(key) = self.interner.lookup(type_id) else {
            return false;
        };

        match key {
            TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::Conditional(_)
            | TypeData::Mapped(_)
            | TypeData::IndexAccess(_, _)
            | TypeData::KeyOf(_)
            | TypeData::TypeQuery(_)
            | TypeData::TemplateLiteral(_)
            | TypeData::ReadonlyType(_)
            | TypeData::StringIntrinsic { .. }
            | TypeData::ThisType => true,
            // Intersection/union types containing complex members are also complex.
            // Without this, the evaluator's subtype-based simplification can incorrectly
            // collapse union members like `(T&U&1) | (T&U&2) | (T&U&3)` to just `T&U&2`
            // because the constraint fallback determines some branches are always `never`.
            // TSC does not perform such simplification on unions with type parameters.
            TypeData::Intersection(list_id) | TypeData::Union(list_id) => {
                let members = self.interner.type_list(list_id);
                members.iter().any(|&m| self.is_complex_type(m))
            }
            TypeData::Array(_) | TypeData::Tuple(_) => self.has_nested_complex_marker(type_id),
            // Function types with Application return types are complex because
            // bypass_evaluation doesn't prevent check_return_compat from fully
            // evaluating Application return types. This causes structurally similar
            // but distinct return types (e.g., Generator<T> vs AsyncGenerator<T>)
            // to be incorrectly collapsed via remove_redundant_members.
            TypeData::Function(fn_id) => {
                let shape = self.interner.function_shape(fn_id);
                matches!(
                    self.interner.lookup(shape.return_type),
                    Some(TypeData::Application(_) | TypeData::Lazy(_))
                )
            }
            _ => false,
        }
    }

    /// Evaluate an intersection type by recursively evaluating members and re-interning.
    /// This enables "deferred reduction" where intersections containing meta-types
    /// (e.g., `string & T[K]`) are reduced after the meta-types are evaluated.
    ///
    /// Example: `string & T[K]` where `T[K]` evaluates to `number` will become
    /// `string & number`, which then reduces to `never` via the interner's normalization.
    fn evaluate_intersection(&mut self, list_id: TypeListId) -> TypeId {
        let members = self.interner.type_list(list_id);

        // Suppress `this` binding during member evaluation so that methods
        // returning `this` keep it as `ThisType` rather than binding to
        // individual members. The `this` type will be correctly bound later
        // during property access when the full intersection receiver is known.
        let prev_suppress = self.suppress_this_binding;
        self.suppress_this_binding = true;

        let mut evaluated_members = Vec::with_capacity(members.len());
        for &member in members.iter() {
            evaluated_members.push(self.evaluate(member));
        }

        self.suppress_this_binding = prev_suppress;

        // Deep structural simplification using SubtypeChecker
        self.simplify_intersection_members(&mut evaluated_members);

        self.interner.intersection(evaluated_members)
    }

    /// Evaluate a union type by recursively evaluating members and re-interning.
    /// This enables "deferred reduction" where unions containing meta-types
    /// (e.g., `string | T[K]`) are reduced after the meta-types are evaluated.
    ///
    /// Example: `string | T[K]` where `T[K]` evaluates to `string` will become
    /// `string | string`, which then reduces to `string` via the interner's normalization.
    fn evaluate_union(&mut self, list_id: TypeListId) -> TypeId {
        let members = self.interner.type_list(list_id);
        let mut evaluated_members = Vec::with_capacity(members.len());

        for &member in members.iter() {
            evaluated_members.push(self.evaluate(member));
        }

        // Deep structural simplification using SubtypeChecker
        self.simplify_union_members(&mut evaluated_members);

        self.interner.union(evaluated_members)
    }

    /// Simplify union members by removing redundant types using deep subtype checks.
    /// If A <: B, then A | B = B (A is redundant in the union).
    ///
    /// This uses `SubtypeChecker` with `bypass_evaluation=true` to prevent infinite
    /// recursion, since `TypeEvaluator` has already evaluated all members.
    ///
    /// Performance: O(N²) where N is the number of members. We skip simplification
    /// if the union has more than 25 members to avoid excessive computation.
    ///
    /// ## Strategy
    ///
    /// 1. **Early exit for large unions** (>25 members) to avoid O(N²) explosion
    /// 2. **Skip complex types** that require full resolution:
    ///    - `TypeParameter`, Infer, Conditional, Mapped, `IndexAccess`, `KeyOf`, `TypeQuery`
    ///    - `TemplateLiteral`, `ReadonlyType`, String manipulation types
    ///    - Note: Lazy and Application are NOW safe (Task #37: handled by Canonicalizer)
    /// 3. **Fast-path for any/unknown**: If any member is any, entire union becomes any
    /// 4. **Identity check**: O(1) structural identity via `SubtypeChecker` (Task #36 fast-path)
    /// 5. **Depth limit**: `MAX_SUBTYPE_DEPTH` enables deep recursive type simplification (Task #37)
    ///
    /// ## Example Reductions
    ///
    /// - `"a" | string` → `string` (literal absorbed by primitive)
    /// - `number | 1 | 2` → `number` (literals absorbed by primitive)
    /// - `{ a: string } | { a: string; b: number }` → `{ a: string; b: number }`
    fn simplify_union_members(&mut self, members: &mut Vec<TypeId>) {
        // Single-pass early-exit: check for unknown (skip entirely) and whether all
        // members are identity-comparable (disjoint, so O(n²) loop finds nothing).
        let mut all_identity = true;
        for &id in members.iter() {
            if id.is_unknown() {
                return;
            }
            if all_identity && !self.interner.is_identity_comparable_type(id) {
                all_identity = false;
            }
        }
        if all_identity {
            return;
        }
        // In a union, A <: B means A is redundant (B subsumes it).
        // E.g. `"a" | string` => "a" is redundant, result: `string`
        self.remove_redundant_members(members, SubtypeDirection::SourceSubsumedByOther);
    }

    /// Simplify intersection members by removing redundant types using deep subtype checks.
    /// If A <: B, then A & B = A (B is redundant in the intersection).
    ///
    /// ## Example Reductions
    ///
    /// - `{ a: string } & { a: string; b: number }` → `{ a: string; b: number }`
    /// - `{ readonly a: string } & { a: string }` → `{ readonly a: string }`
    /// - `number & 1` → `1` (literal is more specific)
    fn simplify_intersection_members(&mut self, members: &mut Vec<TypeId>) {
        // In an intersection, A <: B means B is redundant (A is more specific).
        // We check if other members are subtypes of the candidate to remove the supertype.
        self.remove_redundant_members(members, SubtypeDirection::OtherSubsumedBySource);
    }

    /// Remove redundant members from a type list using subtype checks.
    ///
    /// This is the shared O(n²) core for both union and intersection simplification.
    /// The `direction` parameter controls which subtype relationship makes a member
    /// redundant:
    /// - `SourceSubsumedByOther`: member[i] <: member[j] → i is redundant (union semantics)
    /// - `OtherSubsumedBySource`: member[j] <: member[i] → i is redundant (intersection semantics)
    ///
    /// Common early exits (size guards, `any` check, complex-type check) are applied here.
    fn remove_redundant_members(&mut self, members: &mut Vec<TypeId>, direction: SubtypeDirection) {
        // Performance guard: skip small or very large type lists
        const MAX_SIMPLIFICATION_SIZE: usize = 25;
        if members.len() < 2 || members.len() > MAX_SIMPLIFICATION_SIZE {
            return;
        }

        // Single-pass early-exit check instead of two separate O(N) scans.
        for &id in members.iter() {
            if id.is_any() || self.is_complex_type(id) {
                return;
            }
        }

        use crate::relations::subtype::{MAX_SUBTYPE_DEPTH, SubtypeChecker};
        let mut checker = SubtypeChecker::with_resolver(self.interner, self.resolver);
        checker.bypass_evaluation = true;
        checker.max_depth = MAX_SUBTYPE_DEPTH;
        checker.no_unchecked_indexed_access = self.no_unchecked_indexed_access;

        // Pre-compute property name sets for all members once, avoiding O(N²) FxHashSet
        // allocations in the inner loop. Each entry is None for non-object types.
        let prop_names: Vec<Option<FxHashSet<u32>>> = members
            .iter()
            .map(|&id| {
                let mut names = FxHashSet::default();
                Self::collect_property_names(self.interner, id, &mut names);
                if names.is_empty() { None } else { Some(names) }
            })
            .collect();

        // Use mark-and-compact instead of Vec::remove() which is O(N) per removal.
        // Since max size is 25 (from guard above), a u32 bitset avoids heap allocation.
        let len = members.len();
        let mut keep: u32 = (1u32 << len) - 1; // all bits set
        for i in 0..len {
            if keep & (1u32 << i) == 0 {
                continue;
            }
            for j in 0..len {
                if i == j || keep & (1u32 << j) == 0 {
                    continue;
                }
                if members[i] == members[j] {
                    continue;
                }

                let is_subtype = match direction {
                    SubtypeDirection::SourceSubsumedByOther => {
                        checker.is_subtype_of(members[i], members[j])
                            && !Self::has_unique_properties_cached(&prop_names[i], &prop_names[j])
                            // Don't remove a member with an index signature when the
                            // subsuming member lacks one. The index signature carries
                            // semantic information that affects assignability checks
                            // against targets with index signatures.
                            // E.g., `Dict<string> | {}` must not simplify to `{}`
                            // because Dict<string> has `[index: string]: string` which
                            // can fail assignability to `Record<string, number>`.
                            && !Self::has_index_signature_not_in(self.interner, members[i], members[j])
                    }
                    SubtypeDirection::OtherSubsumedBySource => {
                        // For intersections: member[j] <: member[i] means member[i] is
                        // a candidate for removal. But if member[i] contributes properties
                        // that member[j] doesn't have, it must be kept — removing it would
                        // lose those property declarations from the intersection type.
                        // This matters for optional properties: {a: string} <: {b?: number}
                        // but {a: string} & {b?: number} must preserve both properties.
                        checker.is_subtype_of(members[j], members[i])
                            && !Self::has_unique_properties_cached(&prop_names[i], &prop_names[j])
                    }
                };
                if is_subtype {
                    keep &= !(1u32 << i);
                    break;
                }
            }
        }
        // Compact: retain only non-redundant elements
        let mut write = 0;
        for read in 0..len {
            if keep & (1u32 << read) != 0 {
                if write != read {
                    members[write] = members[read];
                }
                write += 1;
            }
        }
        members.truncate(write);
    }

    /// Check if `candidate` has any property names that `subsuming` doesn't have,
    /// using pre-computed property name sets to avoid repeated allocation.
    fn has_unique_properties_cached(
        candidate_names: &Option<FxHashSet<u32>>,
        subsuming_names: &Option<FxHashSet<u32>>,
    ) -> bool {
        let Some(candidate) = candidate_names else {
            return false; // No properties → can't contribute unique ones
        };
        let Some(subsuming) = subsuming_names else {
            return true; // Candidate has properties but subsuming doesn't
        };
        candidate.iter().any(|name| !subsuming.contains(name))
    }

    /// Check if `candidate` has an index signature that `subsuming` lacks.
    ///
    /// In a union, removing a member with an index signature when the subsuming
    /// member doesn't have one changes assignability behavior. TypeScript checks
    /// each union member individually against a target, so a member with
    /// `[index: string]: T` can fail assignability to `{[index: string]: U}`
    /// even though the plain `{}` supertype passes. Preserving the index-signature
    /// member ensures tsz matches tsc's per-member union assignability semantics.
    fn has_index_signature_not_in(
        db: &dyn crate::caches::db::TypeDatabase,
        candidate: TypeId,
        subsuming: TypeId,
    ) -> bool {
        let candidate_has_idx = matches!(db.lookup(candidate), Some(TypeData::ObjectWithIndex(_)));
        let subsuming_has_idx = matches!(db.lookup(subsuming), Some(TypeData::ObjectWithIndex(_)));
        candidate_has_idx && !subsuming_has_idx
    }

    /// Collect property name atoms from an object type into the provided set.
    fn collect_property_names(
        db: &dyn crate::caches::db::TypeDatabase,
        type_id: TypeId,
        names: &mut FxHashSet<u32>,
    ) {
        match db.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = db.object_shape(shape_id);
                for prop in &shape.properties {
                    names.insert(prop.name.0);
                }
            }
            Some(TypeData::Intersection(list_id)) => {
                let sub_members = db.type_list(list_id);
                for &sub in sub_members.iter() {
                    Self::collect_property_names(db, sub, names);
                }
            }
            // Array and Tuple types have implicit properties (length, push, etc.)
            // that aren't in the type data. Use a sentinel to mark them as having
            // unique properties, preventing incorrect union simplification.
            // Without this, `T | T[]` unions collapse to `T` when T has only
            // optional properties (the array vacuously satisfies the optional check
            // but loses its array semantics).
            Some(TypeData::Array(_) | TypeData::Tuple(_)) => {
                names.insert(u32::MAX);
            }
            _ => {}
        }
    }

    // =========================================================================
    // Visitor Pattern Implementation (North Star Rule 2)
    // =========================================================================

    /// Visit a `TypeData` and return its evaluated form.
    ///
    /// This is the visitor dispatch method that routes to specific visit_* methods.
    /// The `visiting.remove()` and `cache.insert()` are handled in `evaluate()` for symmetry.
    fn visit_type_key(&mut self, type_id: TypeId, key: &TypeData) -> TypeId {
        match key {
            TypeData::Conditional(cond_id) => self.visit_conditional(*cond_id),
            TypeData::IndexAccess(obj, idx) => self.visit_index_access(*obj, *idx),
            TypeData::Mapped(mapped_id) => self.visit_mapped(*mapped_id),
            TypeData::KeyOf(operand) => self.visit_keyof(*operand),
            TypeData::TypeQuery(symbol) => self.visit_type_query(symbol.0, type_id),
            TypeData::Application(app_id) => self.visit_application(*app_id, type_id),
            TypeData::TemplateLiteral(spans) => self.visit_template_literal(*spans),
            TypeData::Lazy(def_id) => self.visit_lazy(*def_id, type_id),
            TypeData::StringIntrinsic { kind, type_arg } => {
                self.visit_string_intrinsic(*kind, *type_arg)
            }
            TypeData::Intersection(list_id) => self.visit_intersection(*list_id),
            TypeData::Union(list_id) => self.visit_union(*list_id),
            TypeData::Tuple(tuple_list_id) => self.visit_tuple(*tuple_list_id, type_id),
            TypeData::NoInfer(inner) => {
                // NoInfer<T> evaluates to T (strip wrapper, evaluate inner)
                self.evaluate(*inner)
            }
            // All other types pass through unchanged (default behavior)
            _ => type_id,
        }
    }

    /// Visit a conditional type: T extends U ? X : Y
    fn visit_conditional(&mut self, cond_id: ConditionalTypeId) -> TypeId {
        let cond = self.interner.get_conditional(cond_id);
        self.evaluate_conditional(&cond)
    }

    /// Visit an index access type: T[K]
    fn visit_index_access(&mut self, object_type: TypeId, index_type: TypeId) -> TypeId {
        self.evaluate_index_access(object_type, index_type)
    }

    /// Visit a mapped type: { [K in Keys]: V }
    fn visit_mapped(&mut self, mapped_id: MappedTypeId) -> TypeId {
        let mapped = self.interner.get_mapped(mapped_id);
        self.evaluate_mapped(&mapped)
    }

    /// Visit a keyof type: keyof T
    fn visit_keyof(&mut self, operand: TypeId) -> TypeId {
        self.evaluate_keyof(operand)
    }

    /// Visit a type query: typeof expr
    ///
    /// `TypeQuery` represents `typeof X` which must resolve to the VALUE-space type
    /// (constructor type for classes). We use `resolve_ref` which returns the
    /// constructor type stored under `SymbolRef`, NOT `resolve_lazy` which returns
    /// the instance type for classes. This distinction is critical: `typeof A`
    /// for a class A should give the constructor type (with static members and
    /// construct signatures), not the instance type.
    fn visit_type_query(&mut self, symbol_ref: u32, original_type_id: TypeId) -> TypeId {
        use crate::types::SymbolRef;
        let symbol = SymbolRef(symbol_ref);

        // Use resolve_type_query which returns the VALUE type (constructor for classes).
        // Unlike resolve_ref, resolve_type_query is aware that TypeQuery needs the
        // constructor type, not the instance type that may be stored under SymbolRef
        // in TypeEnvironment (inserted by type_reference_symbol_type).
        if let Some(resolved) = self.resolver.resolve_type_query(symbol, self.interner) {
            return resolved;
        }

        // Fallback: try DefId-based resolution if no SymbolRef mapping exists
        if let Some(def_id) = self.resolver.symbol_to_def_id(symbol)
            && let Some(resolved) = self.resolver.resolve_lazy(def_id, self.interner)
        {
            return resolved;
        }

        original_type_id
    }

    /// Visit a generic type application: Base<Args>
    fn visit_application(&mut self, app_id: TypeApplicationId, original_type_id: TypeId) -> TypeId {
        self.evaluate_application(app_id, original_type_id)
    }

    /// Visit a template literal type: `hello${T}world`
    fn visit_template_literal(&mut self, spans: TemplateLiteralId) -> TypeId {
        self.evaluate_template_literal(spans)
    }

    /// Visit a lazy type reference: Lazy(DefId)
    fn visit_lazy(&mut self, def_id: DefId, original_type_id: TypeId) -> TypeId {
        if let Some(resolved) = self.resolver.resolve_lazy(def_id, self.interner) {
            let resolved = if !self.suppress_this_binding
                && crate::contains_this_type(self.interner, resolved)
            {
                crate::substitute_this_type(self.interner, resolved, original_type_id)
            } else {
                resolved
            };

            // When a bare Lazy(DefId) is used without an Application wrapper,
            // but the underlying type has type parameters that all have defaults
            // (e.g., `Uint8Array<T extends ArrayBufferLike = ArrayBuffer>`),
            // we must instantiate the resolved body with those defaults.
            // Otherwise the body retains unsubstituted type parameters.
            let resolved = if let Some(type_params) = self.resolver.get_lazy_type_params(def_id) {
                if !type_params.is_empty() && type_params.iter().all(|p| p.default.is_some()) {
                    let default_args: Vec<_> = type_params
                        .iter()
                        .map(|p| p.default.unwrap_or(TypeId::ERROR))
                        .collect();
                    instantiate_generic(self.interner, resolved, &type_params, &default_args)
                } else {
                    resolved
                }
            } else {
                resolved
            };

            // Re-evaluate the resolved type in case it needs further evaluation
            self.evaluate(resolved)
        } else {
            original_type_id
        }
    }

    /// Visit a string manipulation intrinsic type: Uppercase<T>, Lowercase<T>, etc.
    fn visit_string_intrinsic(&mut self, kind: StringIntrinsicKind, type_arg: TypeId) -> TypeId {
        self.evaluate_string_intrinsic(kind, type_arg)
    }

    /// Visit an intersection type: A & B & C
    fn visit_intersection(&mut self, list_id: TypeListId) -> TypeId {
        self.evaluate_intersection(list_id)
    }

    /// Visit a tuple type: [A, B, ...C]
    ///
    /// Evaluates each element's type if it is a meta-type that can simplify
    /// (`IndexAccess`, Mapped, Conditional, etc.). For rest/spread elements
    /// whose evaluated type is itself a tuple, flattens them inline.
    /// For example: `[string, ...([number, boolean])]` → `[string, number, boolean]`
    ///
    /// Conservative: only evaluates element types that are known meta-types
    /// to avoid exponential blowup with recursive conditional types that
    /// produce tuples.
    fn visit_tuple(&mut self, tuple_list_id: TupleListId, original_type_id: TypeId) -> TypeId {
        let elements = self.interner.tuple_list(tuple_list_id);

        // Quick check: does any element need evaluation?
        let needs_eval = elements
            .iter()
            .any(|elem| Self::is_evaluable_meta_type(self.interner, elem.type_id));
        if !needs_eval {
            return original_type_id;
        }

        let mut result: Vec<TupleElement> = Vec::with_capacity(elements.len());
        let mut changed = false;

        for elem in elements.iter() {
            // Only evaluate element types that are meta-types (IndexAccess,
            // Mapped, Lazy, Application, etc.) — skip type parameters,
            // primitives, and already-concrete types to avoid blowup.
            let evaluated = if Self::is_evaluable_meta_type(self.interner, elem.type_id) {
                self.evaluate(elem.type_id)
            } else {
                elem.type_id
            };
            if evaluated != elem.type_id {
                changed = true;
            }

            // For rest/spread elements, if the evaluated type is a tuple,
            // flatten its elements inline (spreading the inner tuple).
            if elem.rest {
                if let Some(TypeData::Tuple(inner_list_id)) = self.interner.lookup(evaluated) {
                    let inner_elements = self.interner.tuple_list(inner_list_id);
                    for inner_elem in inner_elements.iter() {
                        result.push(*inner_elem);
                    }
                    changed = true;
                    continue;
                } else if let Some(TypeData::Array(element_type)) = self.interner.lookup(evaluated)
                {
                    // Rest element evaluating to an array stays as rest
                    result.push(TupleElement {
                        type_id: element_type,
                        name: elem.name,
                        optional: elem.optional,
                        rest: true,
                    });
                    if element_type != elem.type_id {
                        changed = true;
                    }
                    continue;
                }
            }

            result.push(TupleElement {
                type_id: evaluated,
                name: elem.name,
                optional: elem.optional,
                rest: elem.rest,
            });
        }

        if !changed {
            return original_type_id;
        }

        self.interner.tuple(result)
    }

    /// Check if a type is a meta-type that would benefit from evaluation
    /// inside a tuple element. Excludes type parameters and concrete types
    /// to avoid recursive blowup.
    fn is_evaluable_meta_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        let Some(key) = db.lookup(type_id) else {
            return false;
        };
        matches!(
            key,
            TypeData::IndexAccess(_, _)
                | TypeData::Mapped(_)
                | TypeData::Lazy(_)
                | TypeData::Application(_)
                | TypeData::KeyOf(_)
                | TypeData::TemplateLiteral(_)
                | TypeData::StringIntrinsic { .. }
                | TypeData::ReadonlyType(_)
                | TypeData::TypeQuery(_)
        )
    }

    /// Visit a union type: A | B | C
    fn visit_union(&mut self, list_id: TypeListId) -> TypeId {
        self.evaluate_union(list_id)
    }
}

/// Convenience function for evaluating conditional types
pub fn evaluate_conditional(interner: &dyn TypeDatabase, cond: &ConditionalType) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_conditional(cond)
}

/// Convenience function for evaluating index access types
pub fn evaluate_index_access(
    interner: &dyn TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_index_access(object_type, index_type)
}

/// Convenience function for evaluating index access types with options.
pub fn evaluate_index_access_with_options(
    interner: &dyn TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
    no_unchecked_indexed_access: bool,
) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
    evaluator.evaluate_index_access(object_type, index_type)
}

/// Convenience function for full type evaluation
pub fn evaluate_type(interner: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate(type_id)
}

/// Convenience function for evaluating mapped types
pub fn evaluate_mapped(interner: &dyn TypeDatabase, mapped: &MappedType) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_mapped(mapped)
}

/// Convenience function for evaluating keyof types
pub fn evaluate_keyof(interner: &dyn TypeDatabase, operand: TypeId) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_keyof(operand)
}

// Re-enabled evaluate tests - verifying API compatibility
#[cfg(test)]
#[path = "../../tests/evaluate_tests.rs"]
mod tests;
