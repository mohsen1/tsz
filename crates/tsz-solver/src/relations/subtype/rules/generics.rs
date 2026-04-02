//! Generic type subtype checking.
//!
//! This module handles subtyping for TypeScript's generic and reference types:
//! - Lazy(DefId) types (nominal references to type aliases, classes, interfaces)
//! - `TypeQuery` (typeof expressions)
//! - Type applications (Generic<T, U>)
//! - Mapped types ({ [K in keyof T]: T[K] })
//! - Type expansion and instantiation

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};
use crate::def::DefId;
use crate::instantiation::instantiate::fill_application_defaults;
use crate::types::{MappedModifier, MappedType, TypeData, TypeParamInfo, Visibility};
use crate::types::{MappedTypeId, SymbolRef, TypeApplicationId, TypeId};
use crate::visitor::{
    application_id, contains_type_parameter_named, index_access_parts, intersection_list_id,
    is_empty_object_type, keyof_inner_type, lazy_def_id, literal_value, mapped_type_id,
    object_shape_id, object_with_index_shape_id, type_param_info, union_list_id,
};
use crate::visitors::visitor_predicates::is_primitive_type;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    fn iterator_protocol_mismatch_for_same_application_family(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
    ) -> bool {
        let Some(query_db) = self.query_db else {
            return false;
        };

        let iterator_mismatch = |checker: &mut Self, is_async: bool| {
            let source_info = crate::operations::get_iterator_info(query_db, source_type, is_async);
            let target_info = crate::operations::get_iterator_info(query_db, target_type, is_async);
            source_info
                .zip(target_info)
                .is_some_and(|(source, target)| {
                    !checker
                        .check_subtype(source.yield_type, target.yield_type)
                        .is_true()
                        || !checker
                            .check_subtype(source.return_type, target.return_type)
                            .is_true()
                        || !checker
                            .check_subtype(target.next_type, source.next_type)
                            .is_true()
                })
        };

        iterator_mismatch(self, false) || iterator_mismatch(self, true)
    }

    fn application_cycle_with_concrete_differing_args_is_unsound(
        &self,
        s_app: &crate::types::TypeApplication,
        t_app: &crate::types::TypeApplication,
    ) -> bool {
        if s_app.args == t_app.args {
            return false;
        }

        s_app.args.iter().chain(t_app.args.iter()).all(|&arg| {
            !crate::contains_type_parameters(self.interner, arg)
                && !crate::contains_this_type(self.interner, arg)
        })
    }

    fn application_base_def_id(&self, base: TypeId) -> Option<DefId> {
        match self.interner.lookup(base) {
            Some(TypeData::Lazy(def_id)) => Some(def_id),
            Some(TypeData::TypeQuery(sym_ref)) => {
                let def_id = self.resolver.symbol_to_def_id(sym_ref)?;
                matches!(
                    self.resolver.get_def_kind(def_id),
                    Some(crate::def::DefKind::Interface | crate::def::DefKind::TypeAlias)
                )
                .then_some(def_id)
            }
            _ => None,
        }
    }

    fn shared_application_base_def_id(
        &self,
        source_base: TypeId,
        target_base: TypeId,
    ) -> Option<DefId> {
        let source_def = self.application_base_def_id(source_base)?;
        let target_def = self.application_base_def_id(target_base)?;
        self.resolver
            .defs_are_equivalent(source_def, target_def)
            .then_some(source_def)
    }

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
        // O(1) NOMINAL CLASS SUBTYPE CHECKING (InheritanceGraph Bridge)
        // =======================================================================
        // This short-circuits expensive structural checks for class inheritance.
        // We use the def_to_symbol bridge to map DefIds back to SymbolIds, then
        // use the existing InheritanceGraph for O(1) nominal subtype checking.
        // =======================================================================
        if let Some(graph) = self.inheritance_graph
            && let (Some(s_sym), Some(t_sym)) = (
                self.resolver.def_to_symbol_id(s_def),
                self.resolver.def_to_symbol_id(t_def),
            )
            && let Some(is_class) = self.is_class_symbol
        {
            // Check if both symbols are classes (not interfaces or type aliases)
            let s_is_class = is_class(SymbolRef(s_sym.0));
            let t_is_class = is_class(SymbolRef(t_sym.0));

            if s_is_class && t_is_class {
                // Both are classes - use nominal inheritance check
                if graph.is_derived_from(s_sym, t_sym) {
                    // O(1) bitset check: source is a subclass of target
                    if !already_visiting {
                        self.def_guard.leave(def_pair);
                    }
                    return SubtypeResult::True;
                }
                // Not a subclass - fall through to structural check below
            }
        }

        // Resolve DefIds to their structural forms
        let s_resolved = self.resolver.resolve_lazy(s_def, self.interner);
        let t_resolved = self.resolver.resolve_lazy(t_def, self.interner);

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
    /// for `Lazy(DefId)` but wrong for `TypeQuery`. We must use `resolve_ref` first
    /// (which looks up `symbol_types` → constructor type), and only fall back to
    /// `resolve_lazy` for non-class symbols (e.g., module namespaces).
    pub(crate) fn resolve_type_query_symbol(&self, sym: SymbolRef) -> Option<TypeId> {
        // First try resolve_ref which returns the value from symbol_types
        // (constructor type for classes, function type for functions, etc.)
        let ref_resolved = self.resolver.resolve_ref(sym, self.interner);
        if ref_resolved.is_some() {
            return ref_resolved;
        }
        // Fall back to DefId-based resolution for symbols without a symbol_types entry
        if let Some(def_id) = self.resolver.symbol_to_def_id(sym) {
            self.resolver.resolve_lazy(def_id, self.interner)
        } else {
            None
        }
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
                    self.interner.application(s_app.base, s_app.args.clone())
                };
                let t_new = if t_new_args.len() != t_app.args.len() {
                    self.interner.application(t_app.base, t_new_args.clone())
                } else {
                    self.interner.application(t_app.base, t_app.args.clone())
                };
                return self.check_subtype(s_new, t_new);
            }
        }

        let same_arity = s_app.args.len() == t_app.args.len();
        let shared_base_def = self.shared_application_base_def_id(s_app.base, t_app.base);
        let same_application_family =
            same_arity && (s_app.base == t_app.base || shared_base_def.is_some());
        let variance_def_id = if s_app.base == t_app.base {
            self.application_base_def_id(s_app.base)
        } else {
            shared_base_def
        };
        let source_type = self.interner.application(s_app.base, s_app.args.clone());
        let target_type = self.interner.application(t_app.base, t_app.args.clone());

        if same_application_family
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
        // =======================================================================
        if same_application_family {
            // Try to resolve DefId from the base to query variance
            let def_id = variance_def_id;

            if let Some(def_id) = def_id {
                use crate::caches::db::QueryDatabase;
                let variances = self
                    .query_db
                    .and_then(|db| QueryDatabase::get_type_param_variance(db, def_id))
                    .or_else(|| self.resolver.get_type_param_variance(def_id))
                    .or_else(|| {
                        crate::relations::variance::compute_type_param_variances_with_resolver(
                            self.interner,
                            self.resolver,
                            def_id,
                        )
                    });
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
                            variances.iter().any(|v| v.rejection_unreliable());
                        if any_checked
                            && !all_ok
                            && !needs_structural_fallback
                            && !rejection_unreliable
                        {
                            // For two applications of the same generic definition with
                            // concrete type arguments, a variance failure is conclusive.
                            // However, when any source arg is a type parameter, we must
                            // fall through to structural comparison — the expanded form
                            // may introduce implicit index signatures (e.g., homomorphic
                            // mapped types like `{ [K in keyof T]: T[K] }`) that make
                            // the structural check succeed even though the variance
                            // check on the raw type parameter fails.
                            let source_has_type_param = s_app
                                .args
                                .iter()
                                .any(|arg| crate::contains_type_parameters(self.interner, *arg));
                            if !source_has_type_param {
                                return SubtypeResult::False;
                            }
                        }
                        // When variance check fails but structural fallback is needed
                        // (mapped types with modifiers like Partial<T>, Required<T>),
                        // evaluate both applications to their structural forms and
                        // compare directly. This handles cases like Partial<{a}> vs
                        // Partial<{a, b}> where both expand to all-optional objects
                        // that are mutually assignable despite differing type arguments.
                        if any_checked && !all_ok && needs_structural_fallback {
                            let s_eval = self.evaluate_type(source_type);
                            let t_eval = self.evaluate_type(target_type);
                            if s_eval != source_type || t_eval != target_type {
                                return self.check_subtype(s_eval, t_eval);
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

        if let Some(def_pair) = app_def_pair {
            let visiting = self.def_guard.is_visiting(&def_pair);
            let visiting_rev = self.def_guard.is_visiting(&(def_pair.1, def_pair.0));
            // Check for cycles before expansion
            if visiting || visiting_rev {
                return if self
                    .application_cycle_with_concrete_differing_args_is_unsound(&s_app, &t_app)
                {
                    SubtypeResult::False
                } else {
                    self.cycle_result()
                };
            }
            use crate::recursion::RecursionResult;
            match self.def_guard.enter(def_pair) {
                RecursionResult::Cycle => {
                    return if self
                        .application_cycle_with_concrete_differing_args_is_unsound(&s_app, &t_app)
                    {
                        SubtypeResult::False
                    } else {
                        self.cycle_result()
                    };
                }
                RecursionResult::DepthExceeded | RecursionResult::IterationExceeded => {
                    return self.depth_result();
                }
                RecursionResult::Entered => {}
            }
        }

        // =======================================================================
        // SLOW PATH: Structural expansion for mismatched bases or unknown variance
        // =======================================================================
        // When bases differ or variance is unavailable, we expand both applications
        // to their structural forms and compare. This handles cases like:
        // - interface Child<T> extends Parent<T>
        // - Generic types without variance annotations
        // - Type aliases with complex transformations
        // =======================================================================
        let s_expanded = self.try_expand_application(s_app_id);
        let t_expanded = self.try_expand_application(t_app_id);
        let result = match (s_expanded, t_expanded) {
            (Some(s_struct), Some(t_struct)) => self.check_subtype(s_struct, t_struct),
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
                } else if same_application_family {
                    // When both applications share the same base but cannot be structurally
                    // expanded or evaluated, we cannot safely assume covariant assignability.
                    // The variance-aware check above already attempted to determine the
                    // correct variance; if that failed (e.g., variance unavailable or needs
                    // structural fallback but evaluation didn't change types), we must
                    // reject the assignability rather than using an unsound covariant fallback.
                    // This fixes cases like `Promise<Bar>` being incorrectly assignable to
                    // `Promise<Foo>` when T is contravariant (appears in function parameter
                    // position in the Promise interface).
                    SubtypeResult::False
                } else {
                    SubtypeResult::False
                }
            }
        };

        // Clean up cycle detection guard
        if let Some(def_pair) = app_def_pair {
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

        // Arity must match for variance comparison
        if s_app.args.len() != t_app.args.len() {
            return None;
        }

        // Must be the same base type (same generic definition)
        let same_base = s_app.base == t_app.base
            || self
                .shared_application_base_def_id(s_app.base, t_app.base)
                .is_some();
        if !same_base {
            return None;
        }

        let variance_def_id = if s_app.base == t_app.base {
            self.application_base_def_id(s_app.base)
        } else {
            self.shared_application_base_def_id(s_app.base, t_app.base)
        };

        let def_id = variance_def_id?;

        use crate::caches::db::QueryDatabase;
        let variances = self
            .query_db
            .and_then(|db| QueryDatabase::get_type_param_variance(db, def_id))
            .or_else(|| self.resolver.get_type_param_variance(def_id))
            .or_else(|| {
                crate::relations::variance::compute_type_param_variances_with_resolver(
                    self.interner,
                    self.resolver,
                    def_id,
                )
            })?;

        if variances.len() != s_app.args.len() {
            return None;
        }

        let needs_structural_fallback = variances.iter().any(|v| v.needs_structural_fallback());
        let mut all_ok = true;
        let mut any_checked = false;

        for (i, variance) in variances.iter().enumerate() {
            let s_arg = s_app.args[i];
            let t_arg = t_app.args[i];

            if variance.is_invariant() {
                any_checked = true;
                if !self.check_subtype(s_arg, t_arg).is_true()
                    || !self.check_subtype(t_arg, s_arg).is_true()
                {
                    all_ok = false;
                    break;
                }
            } else if variance.is_covariant() {
                any_checked = true;
                if !self.check_subtype(s_arg, t_arg).is_true() {
                    all_ok = false;
                    break;
                }
            } else if variance.is_contravariant() {
                any_checked = true;
                if !self.check_subtype(t_arg, s_arg).is_true() {
                    all_ok = false;
                    break;
                }
            }
        }

        if any_checked && all_ok && !needs_structural_fallback {
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
        let rejection_unreliable = variances.iter().any(|v| v.rejection_unreliable());
        if any_checked && !all_ok && !needs_structural_fallback && !rejection_unreliable {
            let source_has_type_param = s_app
                .args
                .iter()
                .any(|arg| crate::contains_type_parameters(self.interner, *arg));
            if !source_has_type_param {
                return Some(SubtypeResult::False);
            }
        }

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
                let same_base = s_app.base == t_app.base
                    || self
                        .shared_application_base_def_id(s_app.base, t_app.base)
                        .is_some();
                if same_base && s_app.args.len() == t_app.args.len() {
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
                let s_app = self.interner.type_application(s_app_id);
                let source_type = self.interner.application(s_app.base, s_app.args.clone());
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
        let s_resolved = self.try_resolve_application_body(s_app_id);
        let t_resolved = self.try_resolve_application_body(t_app_id);

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
    fn try_resolve_application_body(&mut self, app_id: TypeApplicationId) -> Option<TypeId> {
        use crate::{TypeSubstitution, instantiate_type};

        let app = self.interner.type_application(app_id);

        let def_id = self.application_base_def_id(app.base)?;
        let type_params = self.resolver.get_lazy_type_params(def_id)?;
        let resolved_body = self.resolver.resolve_lazy(def_id, self.interner)?;
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
        let app_type = self.interner.application(app.base, app.args.clone());
        let mut instantiated = instantiate_type(self.interner, effective_body, &substitution);
        if crate::contains_this_type(self.interner, instantiated) {
            instantiated = crate::substitute_this_type(self.interner, instantiated, app_type);
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
        match self.try_expand_application(app_id) {
            Some(expanded) => self.check_subtype(expanded, target),
            None => {
                let s_eval = self.evaluate_type(source);
                if s_eval != source {
                    self.check_subtype(s_eval, target)
                } else {
                    SubtypeResult::False
                }
            }
        }
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
        match self.try_expand_application(app_id) {
            Some(expanded) => self.check_subtype(source, expanded),
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
                    SubtypeResult::False
                }
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
        // Try the flattened chain approach first: flatten nested homomorphic mapped
        // types to get the ultimate source type and combined modifiers.
        // This handles Partial<Readonly<T>> vs Readonly<Partial<T>> vs Partial<T> etc.
        if let (Some(s_flat), Some(t_flat)) = (
            flatten_mapped_chain(self.interner, source_mapped_id),
            flatten_mapped_chain(self.interner, target_mapped_id),
        ) {
            // Check if both have the same underlying source type
            let sources_match = if s_flat.source == t_flat.source {
                true
            } else {
                self.check_subtype(s_flat.source, t_flat.source).is_true()
            };

            if sources_match {
                // Modifier compatibility:
                // - Source has optional (?) but target doesn't: REJECT
                //   (source may have missing properties that target requires)
                // - Readonly differences: always OK
                //   (readonly is a read-side restriction, not relevant for assignability)
                if s_flat.has_optional && !t_flat.has_optional {
                    return SubtypeResult::False;
                }
                return SubtypeResult::True;
            }
        }

        // Fallback: single-level mapped type comparison
        let source_mapped = self.interner.get_mapped(source_mapped_id);
        let target_mapped = self.interner.get_mapped(target_mapped_id);

        // Both must have the same constraint for this optimization to apply.
        // First try identity comparison, then evaluate to normalize.
        // This handles e.g. keyof(Application(Readonly, [T])) == keyof(T)
        // because evaluate_keyof simplifies keyof(MappedType) → keyof(source).
        let constraints_match = if source_mapped.constraint == target_mapped.constraint {
            true
        } else {
            let s_eval = self.evaluate_type(source_mapped.constraint);
            let t_eval = self.evaluate_type(target_mapped.constraint);
            s_eval == t_eval
        };

        if !constraints_match {
            return SubtypeResult::False;
        }

        // Check template types.
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

        // Handle nested mapped types in templates.
        if let (Some(s_inner_mapped), Some(t_inner_mapped)) = (
            mapped_type_id(self.interner, source_template),
            mapped_type_id(self.interner, target_template),
        ) {
            return self.check_mapped_to_mapped(
                source_template,
                target_template,
                s_inner_mapped,
                t_inner_mapped,
            );
        }

        // Compare templates directly
        self.check_subtype(source_template, target_template)
    }

    /// Check Mapped expansion to target (one-sided Mapped case).
    ///
    /// When the target is a Mapped type that can be expanded (e.g., `{ [K in keyof T]: T[K] }`),
    /// we first expand it and then check subtyping.
    pub(crate) fn check_mapped_expansion_target(
        &mut self,
        _source: TypeId,
        target: TypeId,
        mapped_id: MappedTypeId,
    ) -> SubtypeResult {
        match self.try_expand_mapped(mapped_id) {
            Some(expanded) => self.check_subtype(expanded, target),
            None => {
                if let Some(expanded) = self.try_expand_mapped_with_constraint(mapped_id) {
                    let result = self.check_subtype(expanded, target);
                    if result.is_true() {
                        return result;
                    }
                }

                // Reverse homomorphic mapped type check:
                // { [K in keyof T]+?: T[K] } (Partial<T>, Readonly<T>, etc.) is
                // assignable to T. In tsc 6.0, homomorphic mapped types are
                // bidirectionally assignable to their source type parameter.
                if self.check_homomorphic_mapped_to_target(mapped_id, target) {
                    return SubtypeResult::True;
                }

                // Generic mapped type to index signature target:
                // A generic mapped type like Partial<T> has an implicit string index
                // signature derived from its template type. If the target has a string
                // index signature and the template is assignable to the element type,
                // the mapped type is assignable.
                if self.check_generic_mapped_to_index_target(mapped_id, target) {
                    return SubtypeResult::True;
                }

                SubtypeResult::False
            }
        }
    }

    /// Check if a homomorphic mapped type is assignable to a type parameter target.
    ///
    /// `{ [K in keyof T]: T[K] }` (identity, Readonly, Required) is assignable to T
    /// because these preserve or narrow the shape of T.
    ///
    /// `Partial<T>` (+? modifier) is NOT assignable to T because it widens properties
    /// to optional — a `Partial<T>` value may have `undefined` where T requires a value.
    pub(crate) fn check_homomorphic_mapped_to_target(
        &mut self,
        mapped_id: MappedTypeId,
        target: TypeId,
    ) -> bool {
        let mapped = self.interner.get_mapped(mapped_id);

        // Must not have name remapping (as clause) — remapping can change keys
        if mapped.name_type.is_some() {
            return false;
        }

        // Mapped types that ADD optionality (Partial<T>) are wider than T,
        // so Partial<T> is NOT assignable to T.
        if mapped.optional_modifier == Some(MappedModifier::Add) {
            return false;
        }

        // Constraint must be keyof(S) for some S
        let Some(constraint_source) = keyof_inner_type(self.interner, mapped.constraint) else {
            return false;
        };

        // Check template compatibility with the source's property type S[K].
        // Fast path: if template is exactly S[K] (identity form), no further check needed.
        // General case: check if the template is a subtype of S[K].
        // This handles cases like Denullified<T> where template is NonNullable<T[K]>,
        // which is always <: T[K], so Denullified<T> is assignable to T.
        let template_ok = if let Some((template_obj, template_idx)) =
            index_access_parts(self.interner, mapped.template)
            && let Some(idx_param) = type_param_info(self.interner, template_idx)
            && idx_param.name == mapped.type_param.name
            && template_obj == constraint_source
        {
            true
        } else {
            let k_type_id = self.interner.type_param(TypeParamInfo {
                name: mapped.type_param.name,
                constraint: Some(mapped.constraint),
                default: None,
                is_const: false,
            });
            let source_value_type = self.interner.index_access(constraint_source, k_type_id);
            self.check_subtype(mapped.template, source_value_type)
                .is_true()
        };

        if !template_ok {
            return false;
        }

        // The target must be the same type parameter as the constraint source,
        // or assignable to it.
        if constraint_source == target {
            return true;
        }
        if let Some(target_param) = type_param_info(self.interner, target) {
            if let Some(source_param) = type_param_info(self.interner, constraint_source)
                && source_param.name == target_param.name
            {
                return true;
            }
            // Also check if the target's constraint makes it related
            if let Some(target_constraint) = target_param.constraint {
                return self
                    .check_subtype(target_constraint, constraint_source)
                    .is_true()
                    || self
                        .check_subtype(constraint_source, target_constraint)
                        .is_true();
            }
        }

        false
    }

    /// Check if a generic mapped type (source) is assignable to a target with
    /// a string index signature.
    ///
    /// A generic mapped type like `Partial<T>` or `Readonly<T>` has an implicit
    /// string index signature derived from its template. When the target is
    /// `{ [x: string]: E }`, we check if the template type is assignable to E.
    fn check_generic_mapped_to_index_target(
        &mut self,
        mapped_id: MappedTypeId,
        target: TypeId,
    ) -> bool {
        // Target must have a string index signature
        let t_shape_id = object_with_index_shape_id(self.interner, target)
            .or_else(|| object_shape_id(self.interner, target));
        let Some(t_shape_id) = t_shape_id else {
            return false;
        };
        let t_shape = self.interner.object_shape(t_shape_id);
        let Some(ref string_index) = t_shape.string_index else {
            return false;
        };
        let index_value_type = string_index.value_type;

        // Target must not have required named properties that the mapped type can't satisfy
        if t_shape.properties.iter().any(|p| !p.optional) {
            return false;
        }

        let mapped = self.interner.get_mapped(mapped_id);

        // The mapped type's template produces the value type for each property.
        // Check if the template is assignable to the index value type.
        // For Partial<T> with template T[P], T[P] <: any is always true.
        self.check_subtype(mapped.template, index_value_type)
            .is_true()
    }

    /// Check source to Mapped expansion (one-sided Mapped case).
    ///
    /// When the target is a Mapped type, first try expansion. If expansion fails
    /// (e.g., keyof T where T is a type parameter), fall back to homomorphic
    /// mapped type assignability: source <: { [K in keyof S]: S[K] } holds when
    /// source <: S and the mapped type doesn't remove optionality.
    pub(crate) fn check_source_to_mapped_expansion(
        &mut self,
        source: TypeId,
        _target: TypeId,
        mapped_id: MappedTypeId,
    ) -> SubtypeResult {
        // Try distributing homomorphic mapped types over intersection arguments
        // BEFORE expansion. Expansion of mapped types like Readonly<T & { name: string }>
        // is lossy when T is a type parameter: it only produces the concrete properties
        // (e.g., { readonly name: string }), losing the generic T constraint.
        // Distribution preserves the full type structure:
        //   Readonly<T & { name: string }> → Readonly<T> & Readonly<{ name: string }>
        if let Some(distributed) = self.try_distribute_mapped_over_intersection(mapped_id) {
            let result = self.check_subtype(source, distributed);
            if result.is_true() {
                return result;
            }
        }

        match self.try_expand_mapped(mapped_id) {
            Some(expanded) => self.check_subtype(source, expanded),
            None => {
                // tsc: an empty object {} is assignable to any mapped type that adds
                // the optional modifier (+?), like Partial<T>. All properties are optional,
                // so an empty object trivially satisfies all constraints.
                {
                    let mapped = self.interner.get_mapped(mapped_id);
                    if mapped.optional_modifier == Some(MappedModifier::Add)
                        && is_empty_object_type(self.interner, source)
                    {
                        return SubtypeResult::True;
                    }
                }

                if let Some(expanded) = self.try_expand_mapped_with_constraint(mapped_id) {
                    let result = self.check_subtype(source, expanded);
                    if result.is_true() {
                        return result;
                    }
                }

                // Homomorphic mapped type shortcut:
                // source <: { [K in keyof S]+?: S[K] } when source <: S
                // and the mapped type doesn't remove optional.
                if self.check_source_to_homomorphic_mapped(source, mapped_id) {
                    return SubtypeResult::True;
                }

                SubtypeResult::False
            }
        }
    }

    /// Check if any source type is assignable to a homomorphic mapped type.
    ///
    /// S <: { [K in keyof S]: S[K] } when S is the same as the constraint source
    /// and the mapped type doesn't REMOVE optionality. Removing `-?` (Required)
    /// makes the target NARROWER than the source, so S → Required<S> fails
    /// because S may have optional properties that Required demands.
    fn check_source_to_homomorphic_mapped(
        &mut self,
        source: TypeId,
        mapped_id: MappedTypeId,
    ) -> bool {
        let mapped = self.interner.get_mapped(mapped_id);

        // If there's an as-clause (name_type), it must be a filtering conditional
        // (produces only P or never) for this optimization to apply.
        // Renaming as-clauses (e.g., `as \`bool${P}\``) change property keys,
        // so T is not necessarily assignable to the result type.
        if let Some(name_type) = mapped.name_type
            && !is_filtering_name_type(self.interner, name_type, &mapped)
        {
            return false;
        }

        // Mapped types that REMOVE optionality (-?) like Required<T> are NARROWER
        // than T. The source (which may have optional properties) cannot satisfy
        // the target which demands all properties be present.
        if mapped.optional_modifier == Some(MappedModifier::Remove) {
            return false;
        }

        // Constraint must be keyof(S) for some S
        let Some(constraint_source) = keyof_inner_type(self.interner, mapped.constraint) else {
            return false;
        };

        // Fast path: Template is exactly S[K] where K is the iteration parameter
        if let Some((template_obj, template_idx)) =
            index_access_parts(self.interner, mapped.template)
            && let Some(idx_param) = type_param_info(self.interner, template_idx)
            && idx_param.name == mapped.type_param.name
            && template_obj == constraint_source
        {
            return self.check_subtype(source, constraint_source).is_true();
        }

        // General case: construct the source's property value type S[K] where K is
        // the iteration parameter with constraint `keyof S`, then check S[K] <: Template.
        //
        // This handles mapped types like {[P in keyof T]: T[keyof T]} where the template
        // uses a broader index than just the iteration parameter. The visit_index_access
        // rule in the subtype visitor handles S[I] <: T[J] by checking S <: T AND I <: J,
        // and check_type_parameter_subtype handles K <: keyof S via K's constraint.
        let k_type_id = self.interner.type_param(TypeParamInfo {
            name: mapped.type_param.name,
            constraint: Some(mapped.constraint),
            default: None,
            is_const: false,
        });
        let source_value_type = self.interner.index_access(constraint_source, k_type_id);
        if self
            .check_subtype(source_value_type, mapped.template)
            .is_true()
            && self.check_subtype(source, constraint_source).is_true()
        {
            return true;
        }

        false
    }

    /// Distribute a homomorphic mapped type over an intersection argument.
    ///
    /// When the mapped type has the form `{ [K in keyof (A & B)]: (A & B)[K] }`
    /// (possibly with readonly/optional modifiers), this is equivalent to
    /// `{ [K in keyof A]: A[K] } & { [K in keyof B]: B[K] }` with the same
    /// modifiers. This implements the tsc equivalence:
    ///   `Readonly<A & B>` ≡ `Readonly<A> & Readonly<B>`
    ///
    /// Returns `Some(distributed_intersection)` if distribution applies, `None` otherwise.
    fn try_distribute_mapped_over_intersection(
        &mut self,
        mapped_id: MappedTypeId,
    ) -> Option<TypeId> {
        let mapped = self.interner.get_mapped(mapped_id);

        // Must not have name remapping (as clause)
        if mapped.name_type.is_some() {
            return None;
        }

        // Constraint must be keyof(S) for some S
        let constraint_source = keyof_inner_type(self.interner, mapped.constraint)?;

        // S must be an intersection
        let list_id = intersection_list_id(self.interner, constraint_source)?;
        let members = self.interner.type_list(list_id).to_vec();

        if members.len() < 2 {
            return None;
        }

        // Template must be S[K] (identity indexed access form)
        let (template_obj, template_idx) = index_access_parts(self.interner, mapped.template)?;
        let idx_param = type_param_info(self.interner, template_idx)?;
        if idx_param.name != mapped.type_param.name || template_obj != constraint_source {
            return None;
        }

        // Distribute: for each member M, create { [K in keyof M]: M[K] } with same modifiers
        let mut distributed_members = Vec::with_capacity(members.len());
        for &member in &members {
            let member_constraint = self.interner.keyof(member);
            let member_k = self.interner.type_param(TypeParamInfo {
                name: mapped.type_param.name,
                constraint: Some(member_constraint),
                default: None,
                is_const: false,
            });
            let member_template = self.interner.index_access(member, member_k);
            let member_mapped = self.interner.mapped(MappedType {
                type_param: mapped.type_param,
                constraint: member_constraint,
                name_type: None,
                template: member_template,
                readonly_modifier: mapped.readonly_modifier,
                optional_modifier: mapped.optional_modifier,
            });
            distributed_members.push(member_mapped);
        }

        Some(self.interner.intersection(distributed_members))
    }

    fn try_expand_mapped_with_constraint(&mut self, mapped_id: MappedTypeId) -> Option<TypeId> {
        use crate::{TypeSubstitution, instantiate_type};
        let mapped = self.interner.get_mapped(mapped_id);
        if let Some(TypeData::KeyOf(source)) = self.interner.lookup(mapped.constraint)
            && let Some(TypeData::TypeParameter(param)) = self.interner.lookup(source)
            && let Some(constraint) = param.constraint
        {
            // A self-referential bound like `T extends Box<T>` is not a concrete
            // structural expansion source. Substituting it back into a mapped type
            // can make recursive constraints look satisfiable simply because the
            // relation checker re-enters the same bound coinductively.
            if contains_type_parameter_named(self.interner, constraint, param.name) {
                return None;
            }

            let mut subst = TypeSubstitution::new();
            subst.insert(param.name, constraint);
            // Use keyof(constraint) directly to prevent eager evaluation
            // which would break array/tuple preservation in evaluate_mapped.
            let inst_constraint = self.interner.keyof(constraint);
            let inst_template = instantiate_type(self.interner, mapped.template, &subst);
            let inst_name = mapped
                .name_type
                .map(|n| instantiate_type(self.interner, n, &subst));
            let new_mapped_id = self.interner.mapped(MappedType {
                type_param: mapped.type_param,
                constraint: inst_constraint,
                name_type: inst_name,
                template: inst_template,
                optional_modifier: mapped.optional_modifier,
                readonly_modifier: mapped.readonly_modifier,
            });
            if let Some(TypeData::Mapped(m_id)) = self.interner.lookup(new_mapped_id) {
                let new_mapped = self.interner.get_mapped(m_id);
                let res = crate::evaluation::evaluate::evaluate_mapped(self.interner, &new_mapped);
                if res != TypeId::ERROR && res != new_mapped_id {
                    return Some(res);
                }
            }
        }
        None
    }

    /// Try to expand an Application type to its structural form.
    /// Returns None if the application cannot be expanded (missing type params or body).
    ///
    pub(crate) fn try_expand_application(&mut self, app_id: TypeApplicationId) -> Option<TypeId> {
        use crate::{TypeSubstitution, instantiate_type};

        let app = self.interner.type_application(app_id);

        let def_id = self.application_base_def_id(app.base)?;
        let type_params = self.resolver.get_lazy_type_params(def_id)?;
        let resolved_body = self.resolver.resolve_lazy(def_id, self.interner)?;
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
        let app_type = self.interner.application(app.base, app.args.clone());

        let mut instantiated = instantiate_type(self.interner, effective_body, &substitution);
        if crate::contains_this_type(self.interner, instantiated) {
            instantiated = crate::substitute_this_type(self.interner, instantiated, app_type);
        }

        Some(instantiated)
    }

    /// Try to expand a Mapped type to its structural form.
    /// Returns None if the mapped type cannot be expanded (unresolvable constraint).
    pub(crate) fn try_expand_mapped(&mut self, mapped_id: MappedTypeId) -> Option<TypeId> {
        use crate::{MappedModifier, PropertyInfo, TypeSubstitution, instantiate_type};

        let mapped = self.interner.get_mapped(mapped_id);

        // Get concrete keys from the constraint
        let keys = self.try_evaluate_mapped_constraint(mapped.constraint)?;
        if keys.is_empty() {
            return None;
        }

        let (source_object, is_homomorphic) =
            match index_access_parts(self.interner, mapped.template) {
                Some((obj, idx)) => {
                    let is_homomorphic = type_param_info(self.interner, idx)
                        .is_some_and(|param| param.name == mapped.type_param.name);
                    let source_object = is_homomorphic.then_some(obj);
                    (source_object, is_homomorphic)
                }
                None => (None, false),
            };

        // Helper to get original property modifiers
        let get_original_modifiers = |key_name: tsz_common::interner::Atom| -> (bool, bool) {
            if let Some(source_obj) = source_object {
                let shape_id = object_shape_id(self.interner, source_obj)
                    .or_else(|| object_with_index_shape_id(self.interner, source_obj));
                if let Some(shape_id) = shape_id {
                    let shape = self.interner.object_shape(shape_id);
                    for prop in &shape.properties {
                        if prop.name == key_name {
                            return (prop.optional, prop.readonly);
                        }
                    }
                }
            }
            (false, false)
        };

        // Build properties by instantiating template for each key
        let mut properties = Vec::new();
        for key_name in keys {
            // Convert atom to the correct TypeId for substitution.
            // `__unique_<id>` atoms must become UniqueSymbol types so that the template
            // `(p: K) => void` instantiates to `(p: typeof A) => void` rather than
            // `(p: "__unique_<id>") => void` for symbol-keyed mapped types.
            let key_name_str = self.interner.resolve_atom(key_name);
            let key_literal = if let Some(sym_str) = key_name_str.strip_prefix("__unique_")
                && let Ok(id) = sym_str.parse::<u32>()
            {
                self.interner.unique_symbol(SymbolRef(id))
            } else {
                self.interner.literal_string_atom(key_name)
            };

            let mut subst = TypeSubstitution::new();
            subst.insert(mapped.type_param.name, key_literal);

            let instantiated_type = instantiate_type(self.interner, mapped.template, &subst);
            let property_type = self.evaluate_type(instantiated_type);

            // Determine modifiers based on mapped type configuration
            let (original_optional, original_readonly) = get_original_modifiers(key_name);
            let optional = match mapped.optional_modifier {
                Some(MappedModifier::Add) => true,
                Some(MappedModifier::Remove) => false,
                None => {
                    if is_homomorphic {
                        original_optional
                    } else {
                        false
                    }
                }
            };
            let readonly = match mapped.readonly_modifier {
                Some(MappedModifier::Add) => true,
                Some(MappedModifier::Remove) => false,
                None => {
                    if is_homomorphic {
                        original_readonly
                    } else {
                        false
                    }
                }
            };

            properties.push(PropertyInfo {
                name: key_name,
                type_id: property_type,
                write_type: property_type,
                optional,
                readonly,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
            });
        }

        Some(self.interner.object(properties))
    }

    /// Try to evaluate a mapped type constraint to get concrete string/symbol keys.
    /// Returns None if the constraint can't be resolved to concrete keys.
    pub(crate) fn try_evaluate_mapped_constraint(
        &mut self,
        constraint: TypeId,
    ) -> Option<Vec<tsz_common::interner::Atom>> {
        use crate::LiteralValue;

        // Evaluate the constraint using the resolver-aware evaluator to handle types
        // like `T['type']` that evaluate to concrete unions `typeof A | typeof B`.
        let evaluated = self.evaluate_type(constraint);
        if evaluated != constraint {
            return self.try_evaluate_mapped_constraint(evaluated);
        }

        if let Some(operand) = keyof_inner_type(self.interner, constraint) {
            // Try to resolve the operand to get concrete keys
            return self.try_get_keyof_keys(operand);
        }

        if let Some(LiteralValue::String(name)) = literal_value(self.interner, constraint) {
            return Some(vec![name]);
        }

        // Single unique symbol constraint (e.g., `[K in typeof A]: ...`)
        if let Some(TypeData::UniqueSymbol(sym)) = self.interner.lookup(constraint) {
            let atom = self.interner.intern_string(&format!("__unique_{}", sym.0));
            return Some(vec![atom]);
        }

        if let Some(list_id) = union_list_id(self.interner, constraint) {
            let members = self.interner.type_list(list_id);
            let mut keys = Vec::new();
            for &member in members.iter() {
                if let Some(LiteralValue::String(name)) = literal_value(self.interner, member) {
                    keys.push(name);
                } else if let Some(TypeData::UniqueSymbol(sym)) = self.interner.lookup(member) {
                    // Symbol-keyed constraints: `typeof A | typeof B` use `"__unique_<id>"` atoms.
                    let atom = self.interner.intern_string(&format!("__unique_{}", sym.0));
                    keys.push(atom);
                }
            }
            return if keys.is_empty() { None } else { Some(keys) };
        }

        None
    }

    /// Try to get keys from keyof an operand type.
    pub(crate) fn try_get_keyof_keys(
        &mut self,
        operand: TypeId,
    ) -> Option<Vec<tsz_common::interner::Atom>> {
        self.try_get_keyof_keys_depth(operand, 0)
    }

    fn try_get_keyof_keys_depth(
        &mut self,
        operand: TypeId,
        depth: u32,
    ) -> Option<Vec<tsz_common::interner::Atom>> {
        if depth > 5 {
            return None;
        }
        let shape_id = object_shape_id(self.interner, operand)
            .or_else(|| object_with_index_shape_id(self.interner, operand));
        if let Some(shape_id) = shape_id {
            let shape = self.interner.object_shape(shape_id);
            if shape.properties.is_empty() {
                return None;
            }
            return Some(shape.properties.iter().map(|p| p.name).collect());
        }

        if let Some(def_id) = lazy_def_id(self.interner, operand) {
            let resolved = self
                .resolver
                .resolve_lazy(def_id, self.interner)
                .map(|resolved| self.bind_polymorphic_this(operand, resolved))?;
            if resolved == operand {
                return None; // Avoid infinite recursion
            }
            return self.try_get_keyof_keys_depth(resolved, depth + 1);
        }

        // When the operand is a TypeParameter with a constraint, resolve through
        // the constraint. E.g., for `keyof T` where `T extends { content: C }`,
        // the keys are determined by the constraint `{ content: C }`.
        if let Some(tp) = type_param_info(self.interner, operand)
            && let Some(constraint) = tp.constraint
        {
            // Evaluate the constraint first (e.g., Application(IData, [C]) → { content: C })
            let evaluated = self.evaluate_type(constraint);
            if evaluated != operand {
                return self.try_get_keyof_keys_depth(evaluated, depth + 1);
            }
        }

        None
    }
}

/// Result of flattening a chain of nested homomorphic mapped types.
pub(crate) struct FlattenedMapped {
    /// The ultimate source type (e.g., T in Partial<Readonly<T>>)
    pub source: TypeId,
    /// Whether any mapped type in the chain adds optional (?)
    pub has_optional: bool,
    /// Whether any mapped type in the chain adds readonly
    pub has_readonly: bool,
}

/// Flatten a chain of nested homomorphic mapped types into a single descriptor.
///
/// For `Partial<Readonly<T>>`, this produces:
///   source=T, `has_optional=true` (from Partial), `has_readonly=true` (from Readonly)
///
/// For `Required<Partial<T>>`, this produces:
///   source=T, `has_optional=false` (Remove cancels Add), `has_readonly=false`
///
/// Returns None if the mapped type isn't in homomorphic form (e.g., has name remapping,
/// or template isn't `X[K]` where K is the iteration param).
pub(crate) fn flatten_mapped_chain(
    interner: &dyn crate::TypeDatabase,
    mapped_id: MappedTypeId,
) -> Option<FlattenedMapped> {
    use crate::types::MappedModifier;

    let mapped = interner.mapped_type(mapped_id);

    // Can't flatten mapped types with name remapping (as clause)
    if mapped.name_type.is_some() {
        return None;
    }

    let has_optional = mapped.optional_modifier == Some(MappedModifier::Add);
    let has_readonly = mapped.readonly_modifier == Some(MappedModifier::Add);
    let removes_optional = mapped.optional_modifier == Some(MappedModifier::Remove);
    let removes_readonly = mapped.readonly_modifier == Some(MappedModifier::Remove);

    // Check if template is X[K] where K is the iteration param (homomorphic form)
    let (obj, idx) = index_access_parts(interner, mapped.template)?;
    let param = type_param_info(interner, idx)?;
    if param.name != mapped.type_param.name {
        return None;
    }

    // Try to flatten through the source object if it's itself a mapped type.
    // Evaluate first to normalize Application types (e.g. Application(Partial, [T]))
    // to their Mapped form, so nested chains like Readonly<Partial<T>> flatten correctly.
    let obj_eval = crate::evaluation::evaluate::evaluate_type(interner, obj);
    if let Some(inner_mapped_id) = mapped_type_id(interner, obj_eval)
        && let Some(inner) = flatten_mapped_chain(interner, inner_mapped_id)
    {
        return Some(FlattenedMapped {
            source: inner.source,
            has_optional: if removes_optional {
                false
            } else {
                has_optional || inner.has_optional
            },
            has_readonly: if removes_readonly {
                false
            } else {
                has_readonly || inner.has_readonly
            },
        });
    }

    // Base case: source object is not a mapped type
    Some(FlattenedMapped {
        source: obj,
        has_optional,
        has_readonly,
    })
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
    interner: &dyn crate::TypeDatabase,
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
    interner: &dyn crate::TypeDatabase,
    type_id: TypeId,
    name: tsz_common::interner::Atom,
) -> bool {
    matches!(
        type_param_info(interner, type_id),
        Some(info) if info.name == name
    )
}
