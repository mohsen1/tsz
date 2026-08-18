//! Generic type subtype checking.
//!
//! This module handles subtyping for TypeScript's generic and reference types:
//! - Lazy(DefId) types (nominal references to type aliases, classes, interfaces)
//! - `TypeQuery` (typeof expressions)
//! - Type applications (Generic<T, U>)
//! - Mapped types ({ [K in keyof T]: T[K] })
//! - Type expansion and instantiation

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver, are_types_structurally_identical};
pub(crate) use super::mapped_chain::flatten_mapped_chain;
use crate::def::DefId;
use crate::instantiation::instantiate::fill_application_defaults;
use crate::types::{MappedModifier, MappedType, TypeData};
use crate::types::{MappedTypeId, SymbolRef, TypeApplicationId, TypeId};
use crate::visitor::{
    application_id, array_element_type, mapped_type_id, tuple_list_id, type_param_info,
    union_list_id,
};
use crate::visitors::visitor_predicates::is_primitive_type;

#[path = "generics_application_helpers.rs"]
mod generics_application_helpers;
#[cfg(test)]
pub(crate) use generics_application_helpers::ONE_SIDED_APP_EXPANSION_MAX_DEPTH;
pub(crate) use generics_application_helpers::merge_bivariant_usage;

fn args_contain_type_parameters(
    interner: &dyn crate::construction::TypeDatabase,
    args: &[TypeId],
) -> bool {
    args.iter()
        .any(|arg| crate::visitor::contains_type_parameters(interner, *arg))
}

/// #14351 lazy-reference relation kill switch. Default-OFF (opt-in via
/// `TSZ_LAZY_REF_RELATION=1`) so flag-off is byte-identical to `main` — the
/// cross-base heritage branch at the variance fast path takes the unchanged
/// `return None` (structural-expansion) path when this is false. A dedicated
/// env flag, NOT `perf_counters::enabled_fast`, because that gates pre-existing
/// behavior; this must be a pure feature toggle so flag-off-vs-on is a clean
/// single-variable delta.
fn lazy_ref_relation_enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("TSZ_LAZY_REF_RELATION").is_ok_and(|v| v == "1"))
}

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Helper for resolving two Ref/TypeQuery symbols and checking subtype.
    ///
    /// Handles the common pattern of:
    /// - Both resolved: check `s_type` <: `t_type`
    /// - Only source resolved: check `s_type` <: target
    /// - Only target resolved: check source <: `t_type`
    /// - Neither resolved: False
    pub(crate) fn check_resolved_pair_subtype(
        &mut self,
        source: TypeId,
        target: TypeId,
        s_resolved: Option<TypeId>,
        t_resolved: Option<TypeId>,
    ) -> SubtypeResult {
        let s_resolved = s_resolved.map(|resolved| self.bind_polymorphic_this(source, resolved));
        let t_resolved = t_resolved.map(|resolved| self.bind_polymorphic_this(target, resolved));
        match (s_resolved, t_resolved) {
            (Some(s_type), Some(t_type)) => self.check_subtype(s_type, t_type),
            (Some(s_type), None) => self.check_subtype(s_type, target),
            (None, Some(t_type)) => self.check_subtype(source, t_type),
            (None, None) => SubtypeResult::False,
        }
    }

    /// O(1) nominal heritage subtype test over two `Lazy(DefId)` references.
    ///
    /// Returns `true` when a registered `InheritanceGraph` edge makes `s_def`
    /// nominally derive from `t_def` AND that edge authoritatively implies a
    /// *structural* subtype — i.e. the verdict cannot depend on the source's (or
    /// an intermediate base's) type arguments threading into the target's member
    /// types. That holds in two cases:
    ///   * both ends are classes — tsc treats class assignability nominally, so a
    ///     registered subclass edge is authoritative regardless of generics; or
    ///   * the target is non-generic — its member set is fixed, and heritage
    ///     (interface `extends`, enforced compatible by TS2430) guarantees a
    ///     derived type carries those members compatibly. This covers the deep,
    ///     non-generic DOM/lib interface heritage (`Worker <: EventTarget`,
    ///     `MessageEvent <: Event`) without lowering the base's members — the
    ///     #13935 consumer-side lever.
    ///
    /// A generic target reached through an interface edge returns `false` here so
    /// the caller falls through to the structural check, which honors variance.
    /// #14351: pure nominal-heritage REACHABILITY — does `s_def` derive from
    /// `t_def` via the `InheritanceGraph` (transitive `extends`/`implements`)?
    /// Unlike [`Self::nominal_heritage_subtype`] this does NOT apply the
    /// `both_classes || target_non_generic` authoritativeness gate: reachability
    /// alone does not settle a *generic* relation (the per-argument variance
    /// still has to run), but it is the candidate predicate for the
    /// lazy-reference relation, which relates the instantiated bases by variance.
    pub(crate) fn nominal_heritage_reachable(&self, s_def: DefId, t_def: DefId) -> bool {
        let Some(graph) = self.inheritance_graph else {
            return false;
        };
        let (Some(s_sym), Some(t_sym)) = (
            self.resolver.def_to_symbol_id(s_def),
            self.resolver.def_to_symbol_id(t_def),
        ) else {
            return false;
        };
        graph.is_derived_from(s_sym, t_sym)
    }

    pub(crate) fn nominal_heritage_subtype(&self, s_def: DefId, t_def: DefId) -> bool {
        let Some(graph) = self.inheritance_graph else {
            return false;
        };
        let (Some(s_sym), Some(t_sym)) = (
            self.resolver.def_to_symbol_id(s_def),
            self.resolver.def_to_symbol_id(t_def),
        ) else {
            return false;
        };
        // Check the heritage edge first: it rejects the common no-edge case with
        // an O(1) hashmap miss (no closure built, no allocation), so the
        // authoritativeness gate — which may clone the target's type-parameter
        // list — only runs once a real derived edge exists.
        if !graph.is_derived_from(s_sym, t_sym) {
            return false;
        }
        let both_classes = self
            .is_class_symbol
            .is_some_and(|is_class| is_class(SymbolRef(s_sym.0)) && is_class(SymbolRef(t_sym.0)));
        let target_non_generic = self
            .resolver
            .get_lazy_type_params(t_def)
            .is_none_or(|params| params.is_empty());
        both_classes || target_non_generic
    }

    /// Check Lazy(DefId) to Lazy(DefId) subtype with optional identity shortcut.
    ///
    /// For class-to-class checks, uses `InheritanceGraph` for O(1) nominal subtyping
    /// before falling back to structural checking. This is critical for:
    /// - Performance: Avoids expensive member-by-member comparison
    /// - Correctness: Properly handles private/protected members (nominal, not structural)
    /// - Recursive types: Breaks cycles in class inheritance (e.g., `class Box { next: Box }`)
    ///
    /// Uses the `InheritanceGraph` bridge for O(1) nominal class subtype checking
    /// and `RecursionGuard` for cycle detection at the DefId level.
    pub(crate) fn check_lazy_lazy_subtype(
        &mut self,
        source: TypeId,
        target: TypeId,
        s_def: DefId,
        t_def: DefId,
    ) -> SubtypeResult {
        // =======================================================================
        // IDENTITY CHECK: O(1) DefId equality
        // =======================================================================
        // If both DefIds are the same, we're checking the same type against itself.
        // This implements coinductive semantics: a recursive type is a subtype of itself.
        if s_def == t_def {
            return SubtypeResult::True;
        }

        // =======================================================================
        // CYCLE DETECTION: DefId-level tracking
        // =======================================================================
        // This catches cycles in recursive type aliases at the DefId level,
        // preventing infinite expansion. We check this BEFORE resolving the DefIds
        // to their structural forms.
        // =======================================================================
        let def_pair = (s_def, t_def);

        // Check reversed pair for bivariant cross-recursion
        if self.def_guard.is_visiting(&(t_def, s_def)) {
            return self.cycle_result();
        }

        // The parent check_subtype (cache.rs) may have already entered this pair
        // into the def_guard. When bypass_evaluation is true (used by the evaluator's
        // simplify_union_members), the Lazy types are not evaluated before reaching
        // check_subtype_inner, which dispatches here. The double-entry causes a false
        // cycle detection that incorrectly returns True, collapsing distinct union
        // members (e.g., Lazy({type:'a'}) | Lazy({type:'b'}) → just one member).
        //
        // When bypass_evaluation is false, the coinductive assumption on a double-entry
        // is correct: the pair is genuinely being compared recursively through type
        // evaluation, and assuming True on cycle prevents infinite expansion.
        let already_visiting = self.bypass_evaluation && self.def_guard.is_visiting(&def_pair);

        use crate::recursion::RecursionResult;
        if !already_visiting {
            match self.def_guard.enter(def_pair) {
                RecursionResult::Cycle => return self.cycle_result(),
                RecursionResult::DepthExceeded | RecursionResult::IterationExceeded => {
                    return self.depth_result();
                }
                RecursionResult::Entered => {}
            }
        }

        // =======================================================================
        // O(1) NOMINAL HERITAGE SUBTYPE CHECKING (InheritanceGraph Bridge)
        // =======================================================================
        // Short-circuit expensive structural member materialization when a
        // registered heritage edge authoritatively settles the relation. The
        // shortcut also runs as a pre-evaluation fast path in `check_subtype`
        // (so the base's members are never materialized); this is the in-dispatch
        // fallback for the bypass-evaluation / both-unresolvable paths that reach
        // here with two bare `Lazy(DefId)` references. See
        // `nominal_heritage_subtype` for the soundness gate.
        if self.nominal_heritage_subtype(s_def, t_def) {
            if !already_visiting {
                self.def_guard.leave(def_pair);
            }
            return SubtypeResult::True;
        }

        // Resolve DefIds to their structural forms. A `None` here means the
        // body is not yet registered (re-entrant lib resolution); record an
        // undetermined-result event so the enclosing `check_subtype`'s
        // cache write is skipped instead of caching a False that depended on
        // a transiently-unresolvable ref.
        let s_resolved = self.resolver.resolve_lazy(s_def, self.interner);
        if s_resolved.is_none() {
            self.note_unresolved_lazy_relation_event();
        }
        let t_resolved = self.resolver.resolve_lazy(t_def, self.interner);
        if t_resolved.is_none() {
            self.note_unresolved_lazy_relation_event();
        }

        // Detect self-referencing Lazy types (namespace circular references).
        // When a namespace's DefId resolves back to Lazy(same_DefId), it means
        // the type environment has a circular entry (no structural type available).
        // In this case, check_resolved_pair_subtype would re-enter check_subtype,
        // hit the def_guard cycle detection, and return True (coinductive assumption).
        // This incorrectly treats all namespace types as compatible, suppressing TS2741.
        //
        // Fix: if EITHER side resolves to itself (Lazy(same_DefId)), the types
        // are opaque and not structurally comparable. Since s_def != t_def
        // (checked above), they represent different semantic entities → not subtypes.
        let s_is_circular = s_resolved
            .is_some_and(|r| crate::visitor::lazy_def_id(self.interner, r) == Some(s_def));
        let t_is_circular = t_resolved
            .is_some_and(|r| crate::visitor::lazy_def_id(self.interner, r) == Some(t_def));
        if s_is_circular || t_is_circular {
            if !already_visiting {
                self.def_guard.leave(def_pair);
            }
            return SubtypeResult::False;
        }

        let result = self.check_resolved_pair_subtype(source, target, s_resolved, t_resolved);

        // Leave def_guard only if we entered it ourselves
        if !already_visiting {
            self.def_guard.leave(def_pair);
        }

        result
    }

    /// Resolve a `TypeQuery(SymbolRef)` to its value-space type.
    ///
    /// `TypeQuery` represents `typeof X` — a value-space type query. For classes,
    /// the value-space type is the **constructor type** (stored in `symbol_types`),
    /// NOT the instance type (stored in `symbol_instance_types`).
    ///
    /// `resolve_lazy` returns the instance type for class symbols, which is correct
    /// for `Lazy(DefId)` but wrong for `TypeQuery`. Delegate to the resolver's
    /// dedicated value-space hook so imported classes and merged value/type symbols
    /// resolve through the same constructor-side path as evaluation.
    pub(crate) fn resolve_type_query_symbol(&self, sym: SymbolRef) -> Option<TypeId> {
        self.resolver.resolve_type_query(sym, self.interner)
    }

    /// Check `TypeQuery` to `TypeQuery` subtype with optional identity shortcut.
    pub(crate) fn check_typequery_typequery_subtype(
        &mut self,
        source: TypeId,
        target: TypeId,
        s_sym: SymbolRef,
        t_sym: SymbolRef,
    ) -> SubtypeResult {
        if s_sym == t_sym {
            return SubtypeResult::True;
        }

        let s_resolved = self.resolve_type_query_symbol(s_sym);
        let t_resolved = self.resolve_type_query_symbol(t_sym);
        self.check_resolved_pair_subtype(source, target, s_resolved, t_resolved)
    }

    /// Check `TypeQuery` (typeof) to structural type subtype.
    pub(crate) fn check_typequery_subtype(
        &mut self,
        _source: TypeId,
        target: TypeId,
        sym: SymbolRef,
    ) -> SubtypeResult {
        match self.resolve_type_query_symbol(sym) {
            Some(s_resolved) => self.check_subtype(s_resolved, target),
            None => SubtypeResult::False,
        }
    }

    /// Check structural type to `TypeQuery` (typeof) subtype.
    pub(crate) fn check_to_typequery_subtype(
        &mut self,
        source: TypeId,
        _target: TypeId,
        sym: SymbolRef,
    ) -> SubtypeResult {
        match self.resolve_type_query_symbol(sym) {
            Some(t_resolved) => self.check_subtype(source, t_resolved),
            None => SubtypeResult::False,
        }
    }

    /// Check if a generic type application is a subtype of another application.
    ///
    /// Variance-aware generic assignability checking.
    ///
    /// This function implements O(1) generic type assignability by using variance
    /// annotations to avoid expensive structural expansion. When both applications
    /// have the same base type, we use the variance mask to check each type argument:
    /// - Covariant: check `s_arg` <: `t_arg`
    /// - Contravariant: check `t_arg` <: `s_arg` (reversed)
    /// - Invariant: check both directions (mutual subtyping)
    /// - Independent: skip (no constraint needed)
    ///
    /// If variance is unavailable or bases differ, fall back to structural expansion.
    pub(crate) fn check_application_to_application_subtype(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
        s_app_id: TypeApplicationId,
        t_app_id: TypeApplicationId,
    ) -> SubtypeResult {
        let s_app = self.interner.type_application(s_app_id);
        let t_app = self.interner.type_application(t_app_id);

        // Synthetic Promise fallback: when lib resolution cannot find the real Promise
        // symbol, checker-side async lowering uses PROMISE_BASE as the application base.
        // That base has no DefId, variance metadata, or structural body to expand, so the
        // generic slow path would otherwise reject even trivially compatible cases like
        // Promise<[1, "two"]> <: Promise<[number, string]>. Treat the synthetic wrapper
        // as a covariant single-parameter container.
        if s_app.base == TypeId::PROMISE_BASE
            && t_app.base == TypeId::PROMISE_BASE
            && s_app.args.len() == 1
            && t_app.args.len() == 1
        {
            return self.check_subtype(s_app.args[0], t_app.args[0]);
        }

        // ===================================================================
        // ARITY NORMALIZATION: Fill in type parameter defaults when same base
        // ===================================================================
        // When both applications share the same base type but have different
        // arg counts (e.g., Generator<T, void, unknown> vs Generator<T>),
        // normalize the shorter one by filling in type parameter defaults.
        // This lets the variance fast path handle cases like Generator<T>
        // which should be treated as Generator<T, any, unknown>.
        // ===================================================================
        if s_app.base == t_app.base
            && s_app.args.len() != t_app.args.len()
            && let Some(def_id) = self.application_base_def_id(s_app.base)
            && let Some(type_params) = self.resolver.get_lazy_type_params(def_id)
        {
            let s_norm = fill_application_defaults(self.interner, &s_app.args, &type_params);
            let t_norm = fill_application_defaults(self.interner, &t_app.args, &type_params);
            if let (Some(s_new_args), Some(t_new_args)) = (&s_norm, &t_norm)
                && s_new_args.len() == t_new_args.len()
            {
                let s_new = if s_new_args.len() != s_app.args.len() {
                    self.interner.application(s_app.base, s_new_args.clone())
                } else {
                    source_type
                };
                let t_new = if t_new_args.len() != t_app.args.len() {
                    self.interner.application(t_app.base, t_new_args.clone())
                } else {
                    target_type
                };
                return self.check_subtype(s_new, t_new);
            }
        }

        // ===================================================================
        // SAME-BASE IDENTICAL-ARGS IDENTITY SHORTCUT
        // ===================================================================
        // When both applications share the same base TypeId AND all type
        // arguments are identical TypeIds, the two applications denote the
        // same type — the same definition applied to the same arguments must
        // produce the same structural form regardless of evaluation context.
        // This fires before variance resolution or structural expansion so
        // that cross-file alias references (e.g. `ReferenceExpression<DB, TB>`
        // from two scope-push paths) relate by identity just as tsc does
        // (tsc caches `getTypeFromTypeReference` per node, so both sides are
        // the same object; tsz may intern the same Application at distinct
        // TypeIds). Without this shortcut, the expansion slow path can diverge
        // for generic-dependent union aliases across file boundaries (#13044).
        // ===================================================================
        if s_app.base == t_app.base
            && s_app.args.len() == t_app.args.len()
            && !s_app.args.is_empty()
            && s_app
                .args
                .iter()
                .zip(t_app.args.iter())
                .all(|(&s, &t)| s == t)
        {
            return SubtypeResult::True;
        }

        let same_arity = s_app.args.len() == t_app.args.len();
        // Same definition family: identical base TypeIds, or bases whose
        // `DefId`s canonicalize to one definition through import-alias
        // forwarding (an alias-keyed application and the declaring module's
        // own key must compare as one family, not degrade to a structural
        // mismatch between an expanded shape and an opaque application).
        // `family_via_forwarding`: the bases' `DefId`s differ but canonicalize
        // to one definition through import-alias forwarding. Such unification
        // is ACCEPTANCE-ONLY — it may prove the relation true (e.g. the
        // `T<any>` shortcut for an alias-keyed/declaring-keyed pair), but a
        // variance rejection under it must fall through to the structural
        // path, which is what the two differently-keyed applications took
        // before forwarding existed (tsc relates the expanded forms there).
        let mut family_via_forwarding = false;
        let variance_def_id = if !same_arity {
            None
        } else if s_app.base == t_app.base {
            self.application_base_def_id(s_app.base)
        } else {
            match (
                self.application_base_def_id(s_app.base),
                self.application_base_def_id(t_app.base),
            ) {
                // Only forwarding-based unification: raw-def equality across
                // *different* base TypeIds kept its historical structural
                // path (`Lazy(def)` vs symbol-keyed object bases).
                (Some(s_def), Some(t_def)) if s_def != t_def => {
                    let canonical = self.resolver.canonical_def_id(s_def);
                    let unified = canonical != s_def || canonical != t_def;
                    let unified = unified && canonical == self.resolver.canonical_def_id(t_def);
                    family_via_forwarding = unified;
                    unified.then_some(canonical)
                }
                _ => None,
            }
        };
        let same_application_family =
            (same_arity && s_app.base == t_app.base) || variance_def_id.is_some();

        let same_base_any_never_pair = same_arity
            && s_app.base == t_app.base
            && s_app
                .args
                .iter()
                .zip(t_app.args.iter())
                .any(|(&source, &target)| {
                    (source.is_any() && target == TypeId::NEVER)
                        || (source == TypeId::NEVER && target.is_any())
                });
        if same_base_any_never_pair
            && let Some(result) = self.try_same_base_any_never_variance_result(s_app_id, t_app_id)
        {
            return result;
        }

        if !same_application_family
            && s_app.args.len() == 1
            && t_app.args.len() == 1
            && let Some(query_db) = self.query_db
            && (crate::type_queries::is_promise_like(query_db, source_type)
                || crate::type_queries::is_promise_like(query_db, self.evaluate_type(source_type)))
            && (crate::type_queries::is_promise_like(query_db, target_type)
                || crate::type_queries::is_promise_like(query_db, self.evaluate_type(target_type)))
            && self.application_has_promise_like_then_contract(query_db, source_type, s_app.args[0])
            && self.application_has_promise_like_then_contract(query_db, target_type, t_app.args[0])
        {
            return self.check_subtype(s_app.args[0], t_app.args[0]);
        }

        if same_application_family
            && !family_via_forwarding
            && self.iterator_protocol_mismatch_for_same_application_family(source_type, target_type)
        {
            return SubtypeResult::False;
        }

        // =======================================================================
        // VARIANCE-AWARE FAST PATH: Same base type with variance checking
        // =======================================================================
        // When both applications have the same base (e.g., Array<T>), we can use
        // variance annotations to check type arguments without expanding the
        // entire structure. This is critical for O(1) performance.
        //
        // Exception: an indexed-access type-alias base (a transform such as
        // TypeBox's `Static<T,P> = (T & {params:P})['static']`) has no sound
        // declared variance — `DefKind::TypeAlias` is transparent and `tsc`
        // always expands it. Comparing the raw arguments here instead lets nested
        // same-base applications hit the coinductive cycle assumption and wrongly
        // report `Static<A>` assignable to `Static<B>`. Skip the fast path for
        // those bases so the structural-expansion slow path evaluates both to
        // concrete shapes. (Conditional-bodied alias bases keep their existing
        // variance handling, which is intentionally retained for differing args.)
        // =======================================================================
        if same_application_family
            && !same_base_any_never_pair
            && !self.is_indexed_access_alias_base_inline(s_app.base)
        {
            // Try to resolve DefId from the base to query variance
            let def_id = variance_def_id;

            if let Some(def_id) = def_id {
                let variances = self.resolve_application_variances(def_id);
                tracing::trace!(
                    ?def_id,
                    ?variances,
                    s_args = ?s_app.args,
                    t_args = ?t_app.args,
                    "app-vs-app variance fast path"
                );
                if let Some(variances) = variances {
                    // Ensure variance count matches arg count (may differ with defaults)
                    if variances.len() == s_app.args.len() {
                        let needs_structural_fallback =
                            variances.iter().any(|v| v.needs_structural_fallback());
                        let mut all_ok = true;
                        let mut any_checked = false;
                        for (i, variance) in variances.iter().enumerate() {
                            let s_arg = s_app.args[i];
                            let t_arg = t_app.args[i];

                            // Apply variance rules for each type argument
                            if variance.is_invariant() {
                                any_checked = true;
                                // Invariant: Must be mutually assignable (effectively equal)
                                // Both directions must hold for soundness
                                if !self.check_subtype(s_arg, t_arg).is_true()
                                    || !self.check_subtype(t_arg, s_arg).is_true()
                                {
                                    all_ok = false;
                                    break;
                                }
                            } else if variance.is_covariant() {
                                any_checked = true;
                                // Covariant: source <: target (normal direction)
                                if !self.check_subtype(s_arg, t_arg).is_true() {
                                    all_ok = false;
                                    break;
                                }
                            } else if variance.is_contravariant() {
                                any_checked = true;
                                // Contravariant: target <: source (reversed direction)
                                // Function parameters are the classic example
                                if !self.check_subtype(t_arg, s_arg).is_true() {
                                    all_ok = false;
                                    break;
                                }
                            }
                            // Independent: No check needed (type parameter not used)
                        }

                        if any_checked && all_ok {
                            // When any type parameter's variance is marked as needing
                            // structural fallback (due to mapped type modifiers like -?/+?),
                            // don't trust the variance shortcut — fall through to structural
                            // comparison. This handles cases like Required<{a?}> vs Required<{b?}>
                            // where the type args are mutually assignable but the mapped results
                            // are structurally incompatible.
                            if !needs_structural_fallback {
                                return SubtypeResult::True;
                            }
                        }
                        let rejection_unreliable =
                            variances.iter().any(|v| v.rejection_unreliable())
                                || family_via_forwarding;
                        if any_checked
                            && !all_ok
                            && !needs_structural_fallback
                            && !rejection_unreliable
                        {
                            let source_args_contain_type_parameters =
                                args_contain_type_parameters(self.interner, &s_app.args);
                            // For two applications of the same generic definition with
                            // concrete type arguments, a variance failure is conclusive.
                            if !source_args_contain_type_parameters
                                || (variances.iter().any(|v| v.has_direct_usage())
                                    && !self.conditional_infer_alias_base(s_app.base)
                                    && !self.conditional_infer_alias_base(t_app.base)
                                    && !self.expanded_application_pair_has_method_property(
                                        source_type,
                                        s_app_id,
                                        target_type,
                                        t_app_id,
                                    ))
                            {
                                return SubtypeResult::False;
                            }
                        }
                        // When variance check fails but structural fallback is needed
                        // (mapped types with modifiers like Partial<T>, Required<T>),
                        // check if the rejection can be trusted based on direct usage.
                        //
                        // When a type parameter has DIRECT_USAGE (appears in non-mapped-type
                        // positions like function params, return types, or properties), the
                        // variance signal is reliable and the rejection is definitive. This
                        // matches tsc's probe-based variance: interfaces with both call
                        // signatures and mapped-type members get plain Invariant (not
                        // Unmeasurable), so tsc trusts the rejection.
                        //
                        // Without direct usage, evaluate both applications to their
                        // structural forms and compare directly. This handles cases like
                        // Partial<{a}> vs Partial<{a, b}> where both expand to
                        // all-optional objects that are mutually assignable despite
                        // differing type arguments.
                        if any_checked && !all_ok && needs_structural_fallback {
                            let has_reliable_rejection =
                                variances.iter().any(|v| v.has_direct_usage());
                            if has_reliable_rejection && !rejection_unreliable {
                                return SubtypeResult::False;
                            }
                            let s_eval = self.evaluate_type(source_type);
                            let t_eval = self.evaluate_type(target_type);
                            if s_eval != source_type || t_eval != target_type {
                                let eval_result = self.check_subtype(s_eval, t_eval);
                                // Structural collapse (s_eval == t_eval) erases the distinction
                                // that REJECTION_UNRELIABLE variance correctly detected. For
                                // concrete args, trust variance over the collapsed result; for
                                // type-param args, fall through — expanded forms may introduce
                                // index signatures that make structural True valid.
                                if rejection_unreliable
                                    && !family_via_forwarding
                                    && s_eval == t_eval
                                    && eval_result.is_true()
                                    && !args_contain_type_parameters(self.interner, &s_app.args)
                                {
                                    return SubtypeResult::False;
                                }
                                return eval_result;
                            }
                        }
                    }
                }
            }
        }

        // =======================================================================
        // CYCLE DETECTION: DefId-level tracking for Application base pairs
        // =======================================================================
        // When checking App(List, args1) <: App(Seq, args2), structural expansion
        // can produce recursive applications (e.g., List<Pair<T,S>> <: Seq<Pair<T,S>>
        // expanding to members that return List<Pair<Pair<T,S>,S2>> <: Seq<Pair<...>>).
        // Without cycle detection at the base-type level, this infinite expansion
        // leads to false negatives. We detect cycles by tracking (source_base_DefId,
        // target_base_DefId) pairs — coinductive semantics assume the relation holds.
        // =======================================================================
        let s_base_def = self.application_base_def_id(s_app.base);
        let t_base_def = self.application_base_def_id(t_app.base);

        let app_def_pair = match (s_base_def, t_base_def) {
            (Some(s_def), Some(t_def)) => Some((s_def, t_def)),
            _ => None,
        };

        let entered_app_def_pair = if let Some(def_pair) = app_def_pair {
            // A reversed pair already on the guard is genuine bivariant
            // cross-recursion (`A<..> <: B<..>` while `B<..> <: A<..>` is in
            // flight above), so the coinductive assumption is correct.
            if self.def_guard.is_visiting(&(def_pair.1, def_pair.0)) {
                let unsound =
                    self.application_cycle_with_concrete_differing_args_is_unsound(&s_app, &t_app);
                tracing::trace!(
                    ?def_pair,
                    unsound,
                    s_args = ?s_app.args,
                    t_args = ?t_app.args,
                    "app-vs-app def-pair reversed cycle"
                );
                return if unsound {
                    SubtypeResult::False
                } else {
                    self.cycle_result()
                };
            }
            // The parent `check_subtype` (cache.rs) enters this base pair into
            // the def_guard before dispatching here. Under `bypass_evaluation`
            // (the evaluator's `simplify_union_members` reduction) source/target
            // are not evaluated first, so a *forward* pair already on the guard
            // is that parent double-entry, not a real cycle. Treating it as one
            // returns a coinductive `True` that wrongly collapses distinct union
            // members whose only difference lives inside opaque `Application`
            // return types (e.g. `new () => WrapA<X>` vs `new () => WrapB<X>`).
            // Mirror `check_lazy_lazy_subtype`'s `already_visiting`: skip the
            // cycle short-circuit and the re-entry, and let the owning parent
            // frame leave the guard. With `bypass_evaluation` off, a genuine
            // recursive double-entry still yields `Cycle` from `enter` below.
            let already_visiting = self.bypass_evaluation && self.def_guard.is_visiting(&def_pair);
            if already_visiting {
                None
            } else {
                use crate::recursion::RecursionResult;
                match self.def_guard.enter(def_pair) {
                    RecursionResult::Cycle => {
                        return if self.application_cycle_with_concrete_differing_args_is_unsound(
                            &s_app, &t_app,
                        ) {
                            SubtypeResult::False
                        } else {
                            self.cycle_result()
                        };
                    }
                    RecursionResult::DepthExceeded | RecursionResult::IterationExceeded => {
                        return self.depth_result();
                    }
                    RecursionResult::Entered => Some(def_pair),
                }
            }
        } else {
            None
        };

        // =======================================================================
        // SLOW PATH: Structural expansion for mismatched bases or unknown variance
        // =======================================================================
        // When bases differ or variance is unavailable, we expand both applications
        // to their structural forms and compare. This handles cases like:
        // - interface Child<T> extends Parent<T>
        // - Generic types without variance annotations
        // - Type aliases with complex transformations
        // =======================================================================
        let s_expanded = self.try_expand_application_type(source_type, s_app_id);
        let t_expanded = self.try_expand_application_type(target_type, t_app_id);
        let result = match (s_expanded, t_expanded) {
            (Some(s_struct), Some(t_struct)) => self.check_expanded_application_subtype(
                s_struct,
                t_struct,
                source_type,
                target_type,
            ),
            (Some(s_struct), None) => self.check_subtype(s_struct, target_type),
            (None, Some(t_struct)) => self.check_subtype(source_type, t_struct),
            (None, None) => {
                // Evaluation fallback: when try_expand_application fails for both sides
                // (common for lib type aliases like Partial<T>, Required<T>, Readonly<T>
                // where the resolver can't resolve the definition body), try full type
                // evaluation. This can resolve Application types through the evaluation
                // pipeline (including mapped type expansion) to produce concrete objects.
                let s_eval = self.evaluate_type(source_type);
                let t_eval = self.evaluate_type(target_type);
                if s_eval != source_type || t_eval != target_type {
                    self.check_subtype(s_eval, t_eval)
                } else if same_application_family
                    && are_types_structurally_identical(
                        self.interner,
                        self.resolver,
                        source_type,
                        target_type,
                    )
                {
                    // Same base, opaque/unresolvable body (neither expansion nor
                    // evaluation made progress), but the two applications are
                    // structurally identical — they denote the same type. This is
                    // the reflexive identity that type-parameter `default` /
                    // constraint-resolution-snapshot fragmentation (#13609) splits
                    // into distinct `TypeId`s: `App(Base, [R = "json"])` vs
                    // `App(Base, [R])`. The query-db canonical fast path in
                    // `check_subtype` recovers it when a `QueryDatabase` is present,
                    // but relation paths constructed without one (instanceof,
                    // element-access, contextual, property lookup, `CompatChecker`
                    // before `set_query_db`) never reached it, so two identical
                    // applications were falsely reported non-assignable. Recover the
                    // identity directly here; this runs only in this rare opaque-base
                    // arm, so it adds no hot-path cost. Structural identity implies
                    // mutual assignability under any variance, so this only proves
                    // the relation, never weakens a genuine rejection.
                    SubtypeResult::True
                } else {
                    // Same base but not structurally identical (or a different
                    // family): the body is opaque and evaluation stalled, so we
                    // cannot assume covariant assignability — the variance-aware
                    // check above already tried, and an unsound covariant fallback
                    // would e.g. make `Promise<Bar>` assignable to `Promise<Foo>`
                    // when `T` is contravariant (a function-parameter position).
                    // Reject.
                    SubtypeResult::False
                }
            }
        };

        // Clean up cycle detection guard — only the frame that entered the pair
        // leaves it. Under a `bypass_evaluation` parent double-entry we skipped
        // the re-entry, so the owning parent frame owns the leave.
        if let Some(def_pair) = entered_app_def_pair {
            self.def_guard.leave(def_pair);
        }

        result
    }

    /// Pre-evaluation variance fast path for Application types.
    ///
    /// When both source and target are Application types with the same base generic
    /// definition and matching arity, check type arguments using variance annotations
    /// WITHOUT evaluating the types to their structural forms first.
    ///
    /// This is critical for recursive generic interfaces like `FunctionComponent<P>`
    /// where evaluation converts Application → Object, losing the generic identity
    /// needed for variance-based rejection. Without this, the structural comparison
    /// falls through to Object-to-Object with coinductive cycle detection, which
    /// incorrectly assumes compatibility for structurally recursive types whose
    /// type arguments differ.
    ///
    /// Returns `Some(result)` if variance gives a conclusive answer, `None` otherwise.
    pub(crate) fn try_variance_fast_path(
        &mut self,
        s_app_id: TypeApplicationId,
        t_app_id: TypeApplicationId,
    ) -> Option<SubtypeResult> {
        let s_app = self.interner.type_application(s_app_id);
        let t_app = self.interner.type_application(t_app_id);

        // Must be the same generic definition. The base TypeIds can differ
        // for one definition when an importing file lowers through its
        // import-alias `DefId` while the declaring module uses its own —
        // canonicalize both through the resolver's alias forwarding before
        // concluding the bases name different definitions.
        // `family_via_forwarding` unification is ACCEPTANCE-ONLY: a variance
        // rejection for an alias-keyed/declaring-keyed pair must fall back to
        // the structural path those pairs always took (see
        // `check_application_to_application_subtype`).
        let mut family_via_forwarding = false;
        let def_id = if s_app.base == t_app.base {
            self.application_base_def_id(s_app.base)?
        } else {
            // Only forwarding-based unification (see above): raw-def equality
            // across different base TypeIds keeps the historical structural
            // path.
            let s_def = self.application_base_def_id(s_app.base)?;
            let t_def = self.application_base_def_id(t_app.base)?;
            if s_def == t_def {
                return None;
            }
            let canonical = self.resolver.canonical_def_id(s_def);
            if (canonical != s_def || canonical != t_def)
                && canonical == self.resolver.canonical_def_id(t_def)
            {
                family_via_forwarding = true;
                canonical
            } else {
                // Acceptance-only pass-through type-alias unification for the
                // permissive `any`-argument shortcut (e.g. `Async<any>` vs
                // `Promise<X>` where `Async<T> = Promise<T>`); see
                // `try_pass_through_alias_any_unification`. Any other shape
                // falls through to the historical structural path unchanged.
                if let Some(result) =
                    self.try_pass_through_alias_any_unification(&s_app, &t_app, s_def, t_def)
                {
                    return Some(result);
                }
                // #14351 lazy-reference relation. For cross-base pairs whose
                // source nominally derives from the target's def
                // (`Apply1<A>` <: `Functor1<B>`), relate them by per-argument
                // variance on the source's INSTANTIATED target-base
                // (`Functor1<A>`, captured at lowering) instead of eagerly
                // expanding both to structural objects and walking members. The
                // instantiated base shares the target's base, so the EXISTING
                // same-base variance fast path (`try_variance_fast_path`) does
                // the per-arg variance read of `A` vs `B` — verdict-preserving
                // (it reads the actual bound-var identity, never a canonicalized
                // representative, so it cannot reproduce the alpha/brand
                // representative-substitution corruption of the refuted levers).
                //
                // ACCEPTANCE-ONLY: only a conclusive `True` short-circuits; any
                // other outcome (`False`/`Unknown`/no instantiated base) falls
                // through to the unchanged structural `return None` below, so the
                // branch can only REMOVE the eager member walk on pairs the
                // variance check accepts — it can never flip a `False` to `True`
                // or vice versa. Flag-OFF is byte-identical to `main`.
                //
                // The reachability probe (`nominal_heritage_reachable`, an
                // `InheritanceGraph` transitive-derivation walk) is computed ONLY
                // when it can be observed: behind the relation flag (which gates
                // the verdict short-circuit) OR the perf-counter probe (the
                // measure-only denominator). The default production config — both
                // off — takes the unchanged structural `return None` with NO extra
                // graph traversal on this hot cross-base seam, so flag-off is now
                // cost-identical to `main`, not merely verdict-identical.
                let lazy_ref_on = lazy_ref_relation_enabled();
                let probe_on = tsz_common::perf_counters::enabled_fast();
                if lazy_ref_on || probe_on {
                    let reachable = self.nominal_heritage_reachable(s_def, t_def);
                    // Resolve the instantiated heritage base once when reachable;
                    // both the (flag-gated) verdict short-circuit and the
                    // (counter-gated) measure-only probe read this single result,
                    // so the accessor map is never queried twice for one pair.
                    let heritage_base = if reachable {
                        self.resolver.get_heritage_instantiation(s_def, t_def)
                    } else {
                        None
                    };
                    // Flag-gated, acceptance-only verdict short-circuit: relate the
                    // source's INSTANTIATED target-base by per-argument variance via
                    // the existing same-base fast path. Only a conclusive `True`
                    // short-circuits; any other outcome falls through to the
                    // structural `return None` below, so it can never flip a verdict
                    // (and `application_id`/variance only run under the flag).
                    if lazy_ref_on
                        && let Some(base) = heritage_base
                        && let Some(base_app_id) = application_id(self.interner, base)
                        && matches!(
                            self.try_variance_fast_path(base_app_id, t_app_id),
                            Some(SubtypeResult::True)
                        )
                    {
                        return Some(SubtypeResult::True);
                    }
                    // Measure-only accessor probe (only when counters enabled): the
                    // resolved/reachable ratio on fp-ts proves the capture populates
                    // the map for real heritage edges. Independent of the flag so
                    // the denominator is observable even with the relation OFF.
                    if probe_on {
                        tsz_common::perf_counters::record_relation_lazy_ref_probe(
                            reachable,
                            heritage_base.is_some(),
                        );
                    }
                }
                return None;
            }
        };

        // An indexed-access type alias (`Static<T,P> = (T & {params:P})['static']`)
        // is transparent and has no sound declared variance: comparing its raw
        // arguments here lets nested same-base applications hit the coinductive
        // cycle assumption and hide a real leaf mismatch (a missing property in
        // the expanded object). `tsc` always expands type aliases, so force the
        // structural-expansion path for these bases instead of the variance fast
        // path.
        if self.is_indexed_access_alias_base_inline(s_app.base) {
            return None;
        }

        // Arity normalization: when both applications share the same base but have
        // different arg counts (e.g., Generator<T, void, any> vs Generator<T>),
        // fill in type parameter defaults to normalize both to the same arity.
        // Without this, the variance fast path bails out and types get structurally
        // expanded, which can fail for complex recursive interfaces like Generator.
        let (s_args, t_args) = if s_app.args.len() != t_app.args.len() {
            let type_params = self.resolver.get_lazy_type_params(def_id)?;
            let s_norm = fill_application_defaults(self.interner, &s_app.args, &type_params)?;
            let t_norm = fill_application_defaults(self.interner, &t_app.args, &type_params)?;
            if s_norm.len() != t_norm.len() {
                return None;
            }
            (s_norm, t_norm)
        } else {
            (s_app.args.clone(), t_app.args.clone())
        };

        let has_any_never_pair = s_args.iter().zip(t_args.iter()).any(|(&source, &target)| {
            (source.is_any() && target == TypeId::NEVER)
                || (source == TypeId::NEVER && target.is_any())
        });
        if has_any_never_pair {
            let classification =
                self.classify_application_args_any_never_variance(def_id, &s_args, &t_args)?;
            if classification.rejects {
                return Some(SubtypeResult::False);
            }
            if !classification.has_unresolved_exceptional
                && let Some(result) = self.try_application_variance_with_mask(
                    &classification.variances,
                    &s_args,
                    &t_args,
                )
            {
                return Some(result);
            }
            if classification.accepted_indices.is_empty() {
                // Transform aliases, inferred nonstrict variance, unreliable
                // masks, and registration gaps remain structural decisions.
                return None;
            }
            let mut masked_source_args = s_args.to_vec();
            for index in classification.accepted_indices {
                masked_source_args[index] = t_args[index];
            }
            let masked_source = self.interner.application(s_app.base, masked_source_args);
            let target = self.interner.application(t_app.base, t_args.to_vec());
            return Some(self.check_subtype(masked_source, target));
        }

        // T<X> <: T<any> and T<any> <: T<X> are true when any-propagation is
        // enabled, except for the directional `any`/`never` variance rule. Skip
        // the general variance walk for the common case rather than risking
        // structural expansion. The shortcut requires `any` to be permissive on
        // BOTH sides: under asymmetric modes (overload subtype pass)
        // `T<any> <: T<X>` must fall through to the per-argument variance checks.
        let allow_any = self
            .any_propagation
            .allows_any_source_at_depth(self.guard.depth())
            && self
                .any_propagation
                .allows_any_target_at_depth(self.guard.depth());
        if allow_any && (s_args.iter().all(|a| a.is_any()) || t_args.iter().all(|a| a.is_any())) {
            return Some(SubtypeResult::True);
        }

        let variances = self.resolve_application_variances(def_id)?;

        if variances.len() != s_args.len() {
            return None;
        }

        let needs_structural_fallback = variances.iter().any(|v| v.needs_structural_fallback());

        // Walk the per-argument variance positions through the single shared
        // loop (`run_application_variance_arg_loop`) so this engine fast path
        // and the relation-query boundary
        // (`relation_queries::check_application_variance`) cannot drift on
        // argument orientation. The engine relates arguments through the raw
        // judge (`check_subtype`); the boundary uses the lawyer.
        let crate::relations::variance::VarianceArgLoopOutcome {
            any_checked,
            all_ok,
            forward_rejected,
        } = crate::relations::variance::run_application_variance_arg_loop(
            &variances,
            &s_args,
            &t_args,
            |s_arg, t_arg| self.check_subtype(s_arg, t_arg).is_true(),
        );

        // Accept when the per-argument variance walk found no mismatch and no
        // position needs the structural fallback. This covers two cases: at
        // least one argument occupied a variance-relevant position and related
        // (`any_checked`), OR every parameter is independent/bivariant so no
        // argument is variance-relevant at all (`!any_checked`, with a
        // non-empty variance list). In the all-bivariant case `tsc` relates the
        // two instantiations via `relateVariances` before any structural
        // expansion; without it a generic interface whose only type-parameter
        // usages are bivariant (e.g. a member returning `R extends TB[] ? X : Y`,
        // where `TB` appears solely in a conditional extends position) falls
        // through to a structural member walk and spuriously rejects the pair
        // because the deferred conditional members differ only in that bivariant
        // argument (the kysely/valibot/zod `T`-not-assignable-to-`T` family).
        // `needs_structural_fallback` keeps any not-cleanly-bivariant position
        // (e.g. a conditional *check* position, or a modifier mapped type) on
        // the structural path.
        if all_ok && !needs_structural_fallback && (any_checked || !variances.is_empty()) {
            return Some(SubtypeResult::True);
        }
        // When structural fallback is needed (mapped types), variance failures
        // are NOT definitive because the expanded structural types may still be
        // compatible even when type arguments fail the variance check. For example,
        // `ToA<{a: any}>` <: `ToA<{}>` fails the invariant check on type args
        // ({a: any} is not bidirectionally assignable to {}) but the expanded
        // types `{a: Type<any>}` and `{}` ARE structurally compatible.
        //
        // Similarly, when any source arg is a type parameter, variance failures
        // are not definitive — the expanded form may introduce implicit index
        // signatures (e.g., homomorphic mapped types `{ [K in keyof T]: T[K] }`)
        // that make structural comparison succeed.
        //
        // For non-mapped types with all-concrete args, variance failures are
        // definitive: incompatible type args means incompatible generic types.
        let rejection_unreliable =
            variances.iter().any(|v| v.rejection_unreliable()) || family_via_forwarding;
        if any_checked
            && !all_ok
            && !needs_structural_fallback
            && !rejection_unreliable
            && !args_contain_type_parameters(self.interner, &s_args)
        {
            return Some(SubtypeResult::False);
        }

        if any_checked
            && !all_ok
            && needs_structural_fallback
            && !rejection_unreliable
            && forward_rejected
            && self.recursive_mapped_alias_base_reaches_self(s_app.base)
            && self.application_args_are_concrete(&s_args)
            && self.application_args_are_concrete(&t_args)
        {
            return Some(SubtypeResult::False);
        }

        // NOTE: A previous heuristic tried to trust invariant variance rejection
        // for concrete type args (no type parameters), but this proved too
        // aggressive — mapped types like `{ [k in keyof S]: Type<S[k]> }` can
        // appear invariant in variance computation while actually being covariant.
        // The structural fallback is needed to handle these cases correctly.
        // See: varianceProblingAndZeroOrderIndexSignatureRelationsAlign tests.

        None
    }

    /// Pre-evaluation variance check for Application source vs Union target.
    ///
    /// When the target is a Union containing an Application with the same base
    /// as the source (common for optional properties: `FC<X> | undefined`),
    /// try variance checking BEFORE evaluation. This prevents the source
    /// Application from being evaluated to an Object, which would lose the
    /// generic identity needed for variance-based rejection.
    ///
    /// Returns `Some(result)` if variance gives a conclusive answer, `None` otherwise.
    pub(crate) fn try_variance_against_union_target(
        &mut self,
        source_type: TypeId,
        s_app_id: TypeApplicationId,
        target: TypeId,
    ) -> Option<SubtypeResult> {
        let target_members = union_list_id(self.interner, target)?;
        let members = self.interner.type_list(target_members);

        // Find Application members and non-Application members of the union
        let mut app_member_id = None;
        let mut non_app_members = Vec::new();

        for &member in members.iter() {
            if let Some(t_app_id) = application_id(self.interner, member) {
                // Check if this Application has the same base as the source
                let s_app = self.interner.type_application(s_app_id);
                let t_app = self.interner.type_application(t_app_id);
                if s_app.base == t_app.base && s_app.args.len() == t_app.args.len() {
                    app_member_id = Some(t_app_id);
                } else {
                    non_app_members.push(member);
                }
            } else {
                non_app_members.push(member);
            }
        }

        let t_app_id = app_member_id?;

        // Try variance check between source Application and matching target Application
        match self.try_variance_fast_path(s_app_id, t_app_id) {
            Some(SubtypeResult::True) => Some(SubtypeResult::True),
            Some(SubtypeResult::False) => {
                // Variance rejected the Application member. Check if the source
                // is a subtype of any non-Application member (e.g., undefined).
                // For a non-nullable Application type, this is typically false.
                for &non_app in &non_app_members {
                    if self.check_subtype(source_type, non_app).is_true() {
                        return Some(SubtypeResult::True);
                    }
                }
                Some(SubtypeResult::False)
            }
            _ => None,
        }
    }

    /// Check application-to-application structural comparison.
    ///
    /// When both source and target are type applications that resolve to mapped types
    /// over the same type parameter (e.g., `Readonly<T>` vs `Partial<T>`), compare
    /// the mapped type structure directly rather than trying to expand.
    pub(crate) fn check_application_to_application(
        &mut self,
        source: TypeId,
        target: TypeId,
        s_app_id: TypeApplicationId,
        t_app_id: TypeApplicationId,
    ) -> SubtypeResult {
        // Try to resolve both applications to see if they are mapped types
        let s_resolved = self.try_resolve_application_body(source, s_app_id);
        let t_resolved = self.try_resolve_application_body(target, t_app_id);

        // If both resolve to mapped types, try direct mapped-to-mapped comparison
        if let (Some(s_body), Some(t_body)) = (s_resolved, t_resolved)
            && let (Some(s_mapped_id), Some(t_mapped_id)) = (
                mapped_type_id(self.interner, s_body),
                mapped_type_id(self.interner, t_body),
            )
        {
            return self.check_mapped_to_mapped(source, target, s_mapped_id, t_mapped_id);
        }

        SubtypeResult::False
    }

    /// Try to resolve the body of a type application (instantiated with its args),
    /// without requiring concrete expansion. This resolves the base type alias/interface
    /// body and instantiates it with the provided type arguments.
    fn try_resolve_application_body(
        &mut self,
        app_type: TypeId,
        app_id: TypeApplicationId,
    ) -> Option<TypeId> {
        use crate::instantiation::instantiate::TypeSubstitution;

        let app = self.interner.type_application(app_id);

        let def_id = self.application_base_def_id(app.base)?;
        let type_params = self.resolver.get_lazy_type_params(def_id)?;
        let resolved_body = match self.resolver.resolve_lazy(def_id, self.interner) {
            Some(body) => body,
            None => {
                // Re-entrant lib resolution: the application's base def has
                // no body registered yet. The caller propagates `None` into a
                // structural fallback that can produce a cacheable False —
                // record the undetermined-result event so the enclosing
                // `check_subtype` call skips caching for this pair.
                self.note_unresolved_lazy_relation_event();
                return None;
            }
        };
        let effective_body = if matches!(
            self.resolver.get_def_kind(def_id),
            Some(crate::def::DefKind::Class)
        ) {
            match self.interner.lookup(resolved_body) {
                Some(TypeData::Callable(cs_id)) => {
                    let shape = self.interner.callable_shape(cs_id);
                    shape
                        .construct_signatures
                        .first()
                        .map(|sig| sig.return_type)
                        .unwrap_or(resolved_body)
                }
                _ => resolved_body,
            }
        } else {
            resolved_body
        };

        // Skip if self-referential
        if let Some(resolved_app_id) = application_id(self.interner, effective_body)
            && resolved_app_id == app_id
        {
            return None;
        }

        let substitution = TypeSubstitution::from_args(self.interner, &type_params, &app.args);
        let mut instantiated = crate::instantiation::instantiate::instantiate_type_cached(
            self.interner,
            self.query_db,
            effective_body,
            &substitution,
        );
        if crate::contains_this_type(self.interner, instantiated) {
            instantiated = crate::instantiation::instantiate::substitute_this_type_cached(
                self.interner,
                self.query_db,
                instantiated,
                app_type,
            );
        }
        Some(instantiated)
    }

    /// Check Application expansion to target (one-sided Application case).
    ///
    /// When the source is an Application type, try structural expansion first.
    /// If that fails, fall back to type evaluation.
    pub(crate) fn check_application_expansion_target(
        &mut self,
        source: TypeId,
        target: TypeId,
        app_id: TypeApplicationId,
    ) -> SubtypeResult {
        if self.is_readonly_application_assignable_to_target(app_id, target) {
            return SubtypeResult::True;
        }

        // Bound one-sided application expansion by recursion identity (tsc's
        // `isDeeplyNestedType`): if this generic definition is already nested at
        // the limit, assume related instead of expanding further. See
        // `ONE_SIDED_APP_EXPANSION_MAX_DEPTH`.
        let base = self.interner.type_application(app_id).base;
        let def_id = self.application_base_def_id(base);
        if let Some(def) = def_id
            && !self.enter_app_expansion_depth(def)
        {
            return self.depth_result();
        }

        let result = match self.try_expand_application_type(source, app_id) {
            Some(expanded) => self.check_subtype(expanded, target),
            None => {
                let s_eval = self.evaluate_type(source);
                if s_eval != source {
                    self.check_subtype(s_eval, target)
                } else {
                    SubtypeResult::False
                }
            }
        };

        if let Some(def) = def_id {
            self.leave_app_expansion_depth(def);
        }

        result
    }

    fn is_readonly_application_assignable_to_target(
        &mut self,
        app_id: TypeApplicationId,
        target: TypeId,
    ) -> bool {
        if array_element_type(self.interner, target).is_some()
            || tuple_list_id(self.interner, target).is_some()
        {
            return false;
        }

        let app = self.interner.type_application(app_id);
        let Some(def_id) = self.application_base_def_id(app.base) else {
            return false;
        };
        let Some(name) = self.resolver.get_def_name(def_id) else {
            return false;
        };
        if self.interner.resolve_atom_ref(name).as_ref() != "Readonly" {
            return false;
        }
        let Some(&inner) = app.args.first() else {
            return false;
        };

        self.check_subtype(inner, target).is_true()
    }

    /// Check source to Application expansion (one-sided Application case).
    ///
    /// When the target is an Application type that can be expanded (e.g., mapped
    /// types like Readonly<T>), we first try structural expansion. If that fails
    /// (common for lib types where the resolver doesn't have type params), fall
    /// back to type evaluation which has broader resolution capabilities.
    pub(crate) fn check_source_to_application_expansion(
        &mut self,
        source: TypeId,
        target: TypeId,
        app_id: TypeApplicationId,
    ) -> SubtypeResult {
        if let Some(inner) = self.readonly_application_or_display_alias_inner(source)
            && array_element_type(self.interner, target).is_none()
            && tuple_list_id(self.interner, target).is_none()
            && self.check_subtype(inner, target).is_true()
        {
            return SubtypeResult::True;
        }

        // Bound one-sided application expansion by recursion identity (tsc's
        // `isDeeplyNestedType`): if this generic definition is already nested at
        // the limit, assume related instead of expanding further. See
        // `ONE_SIDED_APP_EXPANSION_MAX_DEPTH`.
        let base = self.interner.type_application(app_id).base;
        let def_id = self.application_base_def_id(base);
        if let Some(def) = def_id
            && !self.enter_app_expansion_depth(def)
        {
            return self.depth_result();
        }

        let result = match self.try_expand_application_type(target, app_id) {
            Some(expanded) => {
                let expanded_result = self.check_subtype(source, expanded);
                if expanded_result.is_true() {
                    expanded_result
                } else {
                    self.check_source_to_collected_application_properties(source, target)
                        .unwrap_or(expanded_result)
                }
            }
            None => {
                // Evaluation fallback: when try_expand_application fails
                // (common for lib type aliases like Readonly<T>, Partial<T>
                // where the resolver can't resolve the definition body), try
                // full type evaluation which can resolve through the evaluation
                // pipeline (including mapped type expansion).
                let t_eval = self.evaluate_type(target);
                if t_eval != target {
                    self.check_subtype(source, t_eval)
                } else {
                    self.check_source_to_collected_application_properties(source, target)
                        .unwrap_or(SubtypeResult::False)
                }
            }
        };

        if let Some(def) = def_id {
            self.leave_app_expansion_depth(def);
        }

        result
    }

    fn check_source_to_collected_application_properties(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeResult> {
        use crate::objects::{PropertyCollectionResult, collect_properties_cached};

        match collect_properties_cached(target, self.interner, self.resolver, self.query_db) {
            PropertyCollectionResult::Properties {
                properties,
                string_index,
                number_index,
                symbol_index,
            } if !properties.is_empty()
                || string_index.is_some()
                || number_index.is_some()
                || symbol_index.is_some() =>
            {
                let target_shape = crate::types::ObjectShape {
                    flags: crate::types::ObjectFlags::empty(),
                    properties,
                    string_index,
                    number_index,
                    symbol_index,
                    symbol: None,
                };
                let target_object = if target_shape.string_index.is_some()
                    || target_shape.number_index.is_some()
                    || target_shape.symbol_index.is_some()
                {
                    self.interner.object_with_index(target_shape)
                } else {
                    self.interner.object(target_shape.properties)
                };
                Some(self.check_subtype(source, target_object))
            }
            PropertyCollectionResult::Any => Some(self.check_subtype(source, TypeId::ANY)),
            PropertyCollectionResult::Properties { .. } | PropertyCollectionResult::NonObject => {
                None
            }
        }
    }

    /// Check mapped-to-mapped structural comparison.
    ///
    /// When both source and target are mapped types, compare their structure directly
    /// rather than trying to expand (which fails for generic type parameters).
    ///
    /// This handles cases like:
    /// - `Readonly<T>` assignable to `Partial<T>` (template `T[K]` is same, target adds `?`)
    /// - `Partial<Readonly<T>>` assignable to `Readonly<Partial<T>>` (equivalent)
    /// - `T` wrapped in nested homomorphic mapped types
    ///
    /// The rule from tsc: when both mapped types have the same constraint, compare
    /// the template types. If the target adds optional (`?`), the source template
    /// must be assignable to `target_template | undefined`.
    pub(crate) fn check_mapped_to_mapped(
        &mut self,
        _source: TypeId,
        _target: TypeId,
        source_mapped_id: MappedTypeId,
        target_mapped_id: MappedTypeId,
    ) -> SubtypeResult {
        // Fast path: flatten nested homomorphic chains (e.g. Partial<Readonly<T>>).
        // `flatten_mapped_chain` returns None for any mapped type that has a
        // name_type (`as` clause), so name-type compatibility is implicit here.
        if let (Some(s_flat), Some(t_flat)) = (
            flatten_mapped_chain(self.interner, source_mapped_id),
            flatten_mapped_chain(self.interner, target_mapped_id),
        ) {
            let constraints_match =
                self.mapped_key_constraint_covers(s_flat.key_constraint, t_flat.key_constraint);
            let sources_match = if s_flat.source == t_flat.source {
                true
            } else {
                self.check_subtype(s_flat.source, t_flat.source).is_true()
            };

            if constraints_match && sources_match {
                if s_flat.has_optional && !t_flat.has_optional {
                    return SubtypeResult::False;
                }
                return SubtypeResult::True;
            }
        }

        // Fallback: single-level mapped type comparison.
        let source_mapped = self.interner.get_mapped(source_mapped_id);
        let target_mapped = self.interner.get_mapped(target_mapped_id);

        // Name-type compatibility is always required: a source with no `as`
        // clause cannot be a subtype of a target that renames its keys (and
        // vice-versa), regardless of how the raw key constraints relate.
        let name_types_ok = self.mapped_name_types_compatible(&source_mapped, &target_mapped);
        let constraints_match = name_types_ok
            && (self
                .mapped_key_constraint_covers(source_mapped.constraint, target_mapped.constraint)
                || self
                    .check_subtype(target_mapped.constraint, source_mapped.constraint)
                    .is_true());

        if !constraints_match {
            return SubtypeResult::False;
        }

        let source_template = source_mapped.template;
        let mut target_template = target_mapped.template;

        let target_adds_optional = target_mapped.optional_modifier == Some(MappedModifier::Add);
        let source_adds_optional = source_mapped.optional_modifier == Some(MappedModifier::Add);

        if target_adds_optional && !source_adds_optional {
            target_template = self.interner.union2(target_template, TypeId::UNDEFINED);
        }

        let target_removes_optional =
            target_mapped.optional_modifier == Some(MappedModifier::Remove);
        let source_removes_optional =
            source_mapped.optional_modifier == Some(MappedModifier::Remove);
        if target_removes_optional && !source_removes_optional {
            return SubtypeResult::False;
        }

        let source_param = self.interner.type_param(source_mapped.type_param);
        let target_param = self.interner.type_param(target_mapped.type_param);
        let equiv_start = self.type_param_equivalences.len();
        self.type_param_equivalences
            .push(crate::relations::subtype::TypeParamEquivalence::ids(
                source_param,
                target_param,
            ));

        let result = if let (Some(s_inner_mapped), Some(t_inner_mapped)) = (
            mapped_type_id(self.interner, source_template),
            mapped_type_id(self.interner, target_template),
        ) {
            self.check_mapped_to_mapped(
                source_template,
                target_template,
                s_inner_mapped,
                t_inner_mapped,
            )
        } else {
            self.check_subtype(source_template, target_template)
        };
        self.type_param_equivalences.truncate(equiv_start);

        result
    }

    /// Try to expand an Application while preserving the caller's known
    /// interned `TypeId` for the application itself.
    pub(crate) fn try_expand_application_type(
        &mut self,
        app_type: TypeId,
        app_id: TypeApplicationId,
    ) -> Option<TypeId> {
        use crate::instantiation::instantiate::TypeSubstitution;

        let app = self.interner.type_application(app_id);

        let def_id = self.application_base_def_id(app.base)?;
        let type_params = self.resolver.get_lazy_type_params(def_id)?;
        let resolved_body = match self.resolver.resolve_lazy(def_id, self.interner) {
            Some(body) => body,
            None => {
                // Re-entrant lib resolution: the application's base def has
                // no body registered yet. The caller propagates `None` into a
                // structural fallback that can produce a cacheable False —
                // record the undetermined-result event so the enclosing
                // `check_subtype` call skips caching for this pair.
                self.note_unresolved_lazy_relation_event();
                return None;
            }
        };
        let effective_body = if matches!(
            self.resolver.get_def_kind(def_id),
            Some(crate::def::DefKind::Class)
        ) {
            match self.interner.lookup(resolved_body) {
                Some(TypeData::Callable(cs_id)) => {
                    let shape = self.interner.callable_shape(cs_id);
                    shape
                        .construct_signatures
                        .first()
                        .map(|sig| sig.return_type)
                        .unwrap_or(resolved_body)
                }
                _ => resolved_body,
            }
        } else {
            resolved_body
        };

        // Skip expansion if the resolved type is just this Application
        // (prevents infinite recursion on self-referential types)
        if let Some(resolved_app_id) = application_id(self.interner, effective_body)
            && resolved_app_id == app_id
        {
            return None;
        }

        // Homomorphic identity mapped type passthrough: if the body is
        // `{ [K in keyof T]: T[K] }` and the argument for T is a genuine primitive type,
        // return the arg directly. This mirrors evaluate_application().
        // Only applies for identity templates (T[K]), not arbitrary ones like Data.
        // For `any`: only passthrough when the type parameter is constrained to array/tuple.
        // Otherwise, `any` must flow through mapped type expansion to produce
        // `{ [x: string]: any }` (matching tsc's behavior for `Objectish<any>`).
        if let Some(TypeData::Mapped(mapped_id)) = self.interner.lookup(effective_body) {
            let mapped = self.interner.get_mapped(mapped_id);
            if let Some(TypeData::KeyOf(source)) = self.interner.lookup(mapped.constraint)
                && let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(source)
                && let Some(idx) = type_params.iter().position(|p| p.name == tp.name)
                && idx < app.args.len()
                // Verify template is T[K] (identity indexed access)
                && let Some(TypeData::IndexAccess(obj, key)) = self.interner.lookup(mapped.template)
                && obj == source
                && matches!(self.interner.lookup(key), Some(TypeData::TypeParameter(kp)) if kp.name == mapped.type_param.name)
            {
                let arg = app.args[idx];
                let is_any_like = arg == TypeId::ANY
                    || arg == TypeId::UNKNOWN
                    || arg == TypeId::NEVER
                    || arg == TypeId::ERROR;
                let should_passthrough = if is_any_like {
                    tp.constraint.is_some_and(|c| {
                        matches!(
                            self.interner.lookup(c),
                            Some(TypeData::Array(_) | TypeData::Tuple(_))
                        )
                    })
                } else {
                    is_primitive_type(self.interner, arg)
                };
                if should_passthrough {
                    return Some(arg);
                }
            }
        }

        // Create substitution and instantiate
        let substitution = TypeSubstitution::from_args(self.interner, &type_params, &app.args);

        let mut instantiated = crate::instantiation::instantiate::instantiate_type_cached(
            self.interner,
            self.query_db,
            effective_body,
            &substitution,
        );
        if crate::contains_this_type(self.interner, instantiated) {
            instantiated = crate::instantiation::instantiate::substitute_this_type_cached(
                self.interner,
                self.query_db,
                instantiated,
                app_type,
            );
        }

        // Evaluate the instantiated body before returning. When the distributive
        // conditional path in TypeInstantiator distributes a union-typed parameter
        // over conditional branches, it produces a union of unevaluated Conditional
        // nodes. Those Conditionals must be evaluated here so the SubtypeChecker
        // sees concrete types (tuples, objects, etc.) rather than structural
        // Conditional nodes that it cannot directly compare to source types.
        let evaluated = self.evaluate_type(instantiated);
        Some(if evaluated != instantiated {
            evaluated
        } else {
            instantiated
        })
    }
}

/// Check if a mapped type's `name_type` (as-clause) is a "filtering" conditional.
///
/// A filtering as-clause only produces either the iteration parameter P or `never`,
/// meaning it can only REMOVE keys from the source type, never rename them.
/// Example: `{ [P in keyof T as T[P] extends Function ? P : never]: T[P] }`
///
/// This is used by `check_source_to_homomorphic_mapped` to allow T to be assignable
/// to mapped types that filter keys via as-clauses, since all properties in the
/// result type are also properties of T with the same types.
pub(crate) fn is_filtering_name_type(
    interner: &dyn crate::construction::TypeDatabase,
    name_type: TypeId,
    mapped: &MappedType,
) -> bool {
    // The name_type must be a conditional type (C extends D ? X : Y)
    let Some(TypeData::Conditional(cond_id)) = interner.lookup(name_type) else {
        return false;
    };
    let cond = interner.conditional_type(cond_id);

    // One branch must be the iteration parameter P and the other must be `never`.
    // Pattern 1: C extends D ? P : never (filter-in pattern)
    // Pattern 2: C extends D ? never : P (filter-out/invert pattern)
    let iter_param_name = mapped.type_param.name;

    let true_is_param = is_type_param_with_name(interner, cond.true_type, iter_param_name);
    let false_is_param = is_type_param_with_name(interner, cond.false_type, iter_param_name);
    let true_is_never = cond.true_type == TypeId::NEVER;
    let false_is_never = cond.false_type == TypeId::NEVER;

    (true_is_param && false_is_never) || (false_is_param && true_is_never)
}

/// Check if a type is a type parameter with the given name.
fn is_type_param_with_name(
    interner: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
    name: tsz_common::interner::Atom,
) -> bool {
    matches!(
        type_param_info(interner, type_id),
        Some(info) if info.name == name
    )
}
