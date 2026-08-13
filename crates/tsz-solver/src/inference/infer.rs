//! Type inference engine using Union-Find.
//!
//! This module implements type inference for generic functions using
//! the `ena` crate's Union-Find data structure.
//!
//! Key features:
//! - Inference variables for generic type parameters
//! - Constraint collection during type checking
//! - Bounds checking (L <: α <: U)
//! - Best common type calculation
//! - Efficient unification with path compression

use super::infer_guard_state as guard_state;
use crate::construction::{QueryDatabase, TypeDatabase};
#[cfg(test)]
use crate::types::*;
use crate::types::{InferencePriority, TemplateSpan, TypeData, TypeId};
use crate::visitor::{
    array_element_union_widens_literals, contains_type_parameter_named, is_literal_type,
};
use ena::unify::{InPlaceUnificationTable, NoError, UnifyKey, UnifyValue};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use tsz_common::interner::Atom;

/// Helper function to extend a vector with deduplicated items.
/// Uses a `HashSet` for O(1) lookups instead of O(n) contains checks.
fn extend_dedup<T>(target: &mut Vec<T>, items: &[T])
where
    T: Copy + Eq + std::hash::Hash,
{
    if items.is_empty() {
        return;
    }

    // Hot path for inference: most merges add a single item.
    // Avoid allocating/hash-building a set for that case.
    if items.len() == 1 {
        let item = &items[0];
        if !target.contains(item) {
            target.push(*item);
        }
        return;
    }

    let mut existing: FxHashSet<_> = target.iter().copied().collect();
    for item in items {
        if existing.insert(*item) {
            target.push(*item);
        }
    }
}

/// An inference variable representing an unknown type.
/// These are created when instantiating generic functions.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct InferenceVar(pub(crate) u32);

// Uses TypeScript-standard InferencePriority from types.rs

/// A candidate type for an inference variable.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct InferenceCandidate {
    pub(crate) type_id: TypeId,
    pub(crate) priority: InferencePriority,
    pub(crate) is_fresh_literal: bool,
    pub(crate) from_object_property: bool,
    pub from_index_signature: bool,
    pub object_property_index: Option<u32>,
    pub object_property_name: Option<Atom>,
    pub(crate) source_is_type_annotation: bool,
    /// Candidate came from array element inference (`T[]` vs a literal array).
    /// tsc's BCT widening applies to these in `NoInfer<T>` positions.
    pub(crate) from_array_element: bool,
    /// Candidate came from a readonly array-like source. Used when mixed
    /// co/contra inference would otherwise replace a direct readonly argument
    /// with a mutable callback parameter candidate.
    pub(crate) from_readonly_source: bool,
    /// Candidate was inferred from an argument that fills a *function-typed*
    /// parameter — the parameter-level trigger for `tsc`-style context
    /// sensitivity. Such a candidate reaches the type parameter only through the
    /// parameter's contravariant/covariant legs, not as a direct value, so the
    /// final resolution refuses to let it *widen* a type parameter already fixed
    /// from the mandatory arguments (issue #17282). Broader than `tsc`'s
    /// argument-level `isContextSensitive`: it also covers a fully-annotated
    /// function argument, which must keep the fixed variable immutable too.
    pub(crate) from_context_sensitive_arg: bool,
}

#[derive(Copy, Clone, Debug, Default)]
struct CandidateContext {
    from_object_property: bool,
    from_index_signature: bool,
    object_property_index: Option<u32>,
    object_property_name: Option<Atom>,
    source_is_fresh: bool,
}

/// Value stored for each inference variable root.
#[derive(Clone, Debug, Default)]
pub(crate) struct InferenceInfo {
    pub(crate) candidates: Vec<InferenceCandidate>,
    /// Candidates from contravariant positions (e.g., function parameters).
    /// When only `contra_candidates` exist (no covariant candidates), the
    /// resolution uses common-subtype selection for ordinary priorities and
    /// intersection for combination priorities, matching tsc behavior.
    pub(crate) contra_candidates: Vec<InferenceCandidate>,
    pub(crate) upper_bounds: Vec<TypeId>,
    pub(crate) resolved: Option<TypeId>,
}

impl InferenceInfo {
    pub(crate) const fn is_empty(&self) -> bool {
        self.candidates.is_empty()
            && self.contra_candidates.is_empty()
            && self.upper_bounds.is_empty()
    }
}

impl UnifyKey for InferenceVar {
    type Value = InferenceInfo;

    fn index(&self) -> u32 {
        self.0
    }

    fn from_index(u: u32) -> Self {
        Self(u)
    }

    fn tag() -> &'static str {
        "InferenceVar"
    }
}

impl UnifyValue for InferenceInfo {
    type Error = NoError;

    fn unify_values(a: &Self, b: &Self) -> Result<Self, Self::Error> {
        let mut merged = a.clone();

        // Deduplicate candidates using helper
        extend_dedup(&mut merged.candidates, &b.candidates);
        extend_dedup(&mut merged.contra_candidates, &b.contra_candidates);

        // Deduplicate upper bounds using helper
        extend_dedup(&mut merged.upper_bounds, &b.upper_bounds);

        if b.resolved.is_some() {
            merged.resolved = b.resolved;
        }
        Ok(merged)
    }
}

/// Inference error
#[derive(Clone, Debug)]
// Constructed throughout inference as a control-flow signal; the variant
// payload is retained for `Debug`/future error reporting, and only the variant
// kind and `BoundsViolation.lower` are read today.
#[expect(dead_code)]
pub(crate) enum InferenceError {
    /// Two incompatible types were unified
    Conflict(TypeId, TypeId),
    /// Inference variable was not resolved
    Unresolved(InferenceVar),
    /// Circular unification detected (occurs-check)
    OccursCheck { var: InferenceVar, ty: TypeId },
    /// Lower bound is not subtype of upper bound
    BoundsViolation {
        var: InferenceVar,
        lower: TypeId,
        upper: TypeId,
    },
    /// Variance violation detected
    VarianceViolation {
        var: InferenceVar,
        expected_variance: &'static str,
        position: TypeId,
    },
}

/// Constraint set for an inference variable.
/// Tracks both lower bounds (L <: α) and upper bounds (α <: U).
#[derive(Clone, Debug, Default)]
pub(crate) struct ConstraintSet {
    /// Lower bounds: types that must be subtypes of this variable
    /// e.g., from argument types being assigned to a parameter
    pub(crate) lower_bounds: Vec<TypeId>,
    /// Upper bounds: types that this variable must be a subtype of
    /// e.g., from `extends` constraints on type parameters
    pub(crate) upper_bounds: Vec<TypeId>,
}

impl ConstraintSet {
    pub fn from_info(info: &InferenceInfo) -> Self {
        let mut lower_bounds = Vec::new();
        let mut upper_bounds = Vec::new();
        let mut seen_lower = FxHashSet::default();
        let mut seen_upper = FxHashSet::default();

        for candidate in &info.candidates {
            if seen_lower.insert(candidate.type_id) {
                lower_bounds.push(candidate.type_id);
            }
        }

        for &upper in &info.upper_bounds {
            if seen_upper.insert(upper) {
                upper_bounds.push(upper);
            }
        }

        Self {
            lower_bounds,
            upper_bounds,
        }
    }

    /// Check if there are any constraints
    pub const fn is_empty(&self) -> bool {
        self.lower_bounds.is_empty() && self.upper_bounds.is_empty()
    }
}

/// Maximum iterations for constraint strengthening loops to prevent infinite loops.
pub(crate) const MAX_CONSTRAINT_ITERATIONS: usize = 100;

/// Maximum recursion depth for type containment checks.
pub(crate) const MAX_TYPE_RECURSION_DEPTH: usize = 100;

/// Operation-local cache statistics for [`InferenceContext`].
///
/// Owner: one inference request. The subtype memo is scoped to that request
/// and is dropped with the context.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct InferenceContextCacheStatistics {
    /// Entries in the inference subtype memo keyed by source and target `TypeId`.
    pub(crate) subtype_entries: usize,
    estimated_size_bytes: usize,
}

impl InferenceContextCacheStatistics {
    /// Estimated heap bytes owned by inference memo tables.
    #[must_use]
    #[allow(dead_code)] // Inference cache accounting; consumed by inference unit tests
    pub(crate) const fn estimated_size_bytes(self) -> usize {
        self.estimated_size_bytes
    }
}

/// Algorithmic parameter-recovery mode for constraint walks that deliberately
/// reverse source and target independently of structural variance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ParameterRecoveryMode {
    #[default]
    None,
    /// Placeholder-free dependency recovery. A source inference placeholder is
    /// recorded as a candidate even when the live structural polarity is covariant.
    StandaloneReverse,
    /// A complex target containing a call-local placeholder. Recovery walks
    /// retain the live structural polarity so nested contravariant edges can
    /// toggle candidate routing back to covariance.
    ComplexPlaceholder,
}

/// Type inference context for a single function call or expression.
pub(crate) struct InferenceContext<'a> {
    pub(crate) interner: &'a dyn TypeDatabase,
    /// Type resolver for semantic lookups (e.g., base class queries)
    pub(crate) resolver: Option<&'a dyn crate::relations::subtype::TypeResolver>,
    // Shared query database for cache-aware inference-time instantiation.
    pub(crate) query_db: Option<&'a dyn QueryDatabase>,
    /// Memoized subtype checks used by BCT and bound validation.
    pub(crate) subtype_cache: RefCell<FxHashMap<(TypeId, TypeId), bool>>,
    /// Active subtype checks used for coinductive cycle breaking in the
    /// simplified BCT/bounds subtype helper.
    pub(crate) active_subtype_checks: RefCell<FxHashSet<(TypeId, TypeId)>>,
    /// Unification table for inference variables
    pub(crate) table: InPlaceUnificationTable<InferenceVar>,
    /// Map from type parameter names to inference variables, with const flag
    pub(crate) type_params: Vec<(Atom, InferenceVar, bool)>,
    /// Declared `extends` constraints per inference variable (from type parameter declarations).
    /// Separate from `upper_bounds` which also includes contextual type bounds.
    /// Used to decide literal type preservation: `T extends string` preserves `"z"`,
    /// but contextual `Box<boolean>` should NOT preserve `false`.
    pub(crate) declared_constraints: FxHashMap<InferenceVar, TypeId>,
    /// Constraints whose semantic form preserves fresh literal candidates, even if
    /// the raw constraint is an alias/conditional that this context cannot expand.
    pub(crate) literal_preserving_declared_constraints: FxHashSet<InferenceVar>,
    /// Depth counter for `TypeApplication` expansion during inference.
    /// Prevents infinite recursion when inferring through recursive type aliases
    /// like `type Spec<T> = { [P in keyof T]: Spec<T[P]> }`.
    pub(crate) app_expansion_depth: u32,
    /// When true, candidates are routed to `contra_candidates` instead of
    /// regular `candidates`. This is set during the forward inference pass
    /// of callback parameters (contravariant context) so that structural
    /// decomposition produces contra-candidates matching tsc's behavior:
    /// contra-candidates are resolved via intersection and are only used
    /// when no covariant candidates exist.
    pub(crate) in_contra_mode: bool,
    /// Whether inference is currently below at least one contravariant
    /// structural edge. TSZ uses this sticky state separately from
    /// `in_contra_mode`: nested contravariant edges toggle candidate polarity
    /// back to covariance, but the structural matcher must still treat source-side
    /// inference placeholders as parameter-inference evidence instead of hard bounds.
    pub(crate) in_variance_walk: bool,
    /// Algorithmic parameter-recovery state, separate from structural variance.
    pub(crate) parameter_recovery_mode: ParameterRecoveryMode,
    /// Whether the current contravariant signature walk was entered through a
    /// target signature whose declaration origin grants parameter bivariance.
    /// TypeScript still walks those parameters in the contravariant direction,
    /// but records every inference reached beneath them as an ordinary
    /// candidate. The mode is scoped to one inference request and restored
    /// before return-type inference.
    pub(crate) in_bivariant_mode: bool,
    /// Method metadata carried by an object property until its top-level
    /// function/callable signature is reached. `PropertyInfo::is_method` can
    /// survive structural transformations even when a rebuilt function shape
    /// has lost its declaration-kind bit, so inference consumes this one-level
    /// hint at the signature boundary.
    pub(crate) pending_target_method: bool,
    /// Properties accumulated during reverse mapped type inference.
    /// When a homomorphic mapped type `{ [K in keyof T]: Template<T[K]> }`
    /// is matched against a source object, we accumulate (`key_atom`, `value_type`)
    /// pairs for each `T[K]` position encountered during template inference.
    /// After the mapped type loop completes, these are flushed into a single
    /// object candidate for T.
    pub(crate) reverse_mapped_properties: FxHashMap<InferenceVar, Vec<(Atom, TypeId)>>,
    /// When true, literal type candidates are marked as non-fresh (not eligible
    /// for widening). This is set when constraining from type annotation contexts
    /// (e.g., type predicate types like `x is 'B'`) where the literal comes from
    /// a type annotation rather than a fresh expression. Matches tsc's model where
    /// only types with `RequiresWidening` (from expression context) are widened.
    pub(crate) source_is_type_annotation: bool,
    /// Depth counter for `infer_from_types` structural recursion.
    /// Prevents infinite recursion when inferring through self-referential
    /// interface hierarchies (e.g., `ArrayIterator<T>` which has
    /// `[Symbol.iterator](): ArrayIterator<T>` returning itself).
    pub(crate) infer_depth: u32,
    /// Visited `(source, target, mode)` states during structural inference.
    /// The request-local mode packs polarity, sticky variance-walk, bivariant,
    /// pending-method, and one three-state parameter-recovery mode; each affects
    /// how nested placeholders are recorded.
    pub(crate) infer_visited: FxHashSet<(TypeId, TypeId, u8)>,
    /// Inference variables whose corresponding type parameter appears at the
    /// top level of the signature's return type and has not yet been "fixed".
    ///
    /// Mirrors the `inference.isFixed || !isTypeParameterAtTopLevelInReturnType`
    /// gate in tsc's `getCovariantInference` (checker.ts ~26595): when a type
    /// parameter is at top level in the return type and has not been fixed yet,
    /// fresh literal candidates are NOT widened during covariant resolution.
    /// This preserves literals across the Round 1 → Round 2 boundary so that
    /// deferred (context-sensitive) arguments see literal target types matching
    /// tsc (e.g., `(a: T) => U` becomes `(a: number) => 1` rather than
    /// `(a: number) => number` for `f<T,U>(x: T, cb: (a: T) => U, y: U)` called
    /// as `f(1, function(a){return ''}, 1)`).
    pub(crate) top_level_in_return_type_unfixed: FxHashSet<InferenceVar>,
    /// Inference vars whose candidates were rewritten after resolving
    /// higher-order source placeholders. The union table can retain the
    /// pre-rewrite placeholder candidate, so resolution may drop only those
    /// stale call-local placeholders for these vars.
    pub(crate) vars_with_substituted_candidates: FxHashSet<InferenceVar>,
    /// Set during array element inference so candidates get `from_array_element = true`.
    pub(crate) in_array_element_context: bool,
    /// Set while collecting inference from an argument that fills a
    /// function-typed parameter, so candidates get
    /// `from_context_sensitive_arg = true` (see that field on
    /// [`InferenceCandidate`] and issue #17282).
    pub(crate) in_context_sensitive_arg: bool,
    /// Set while inference is descending through a `readonly` array/tuple source
    /// (e.g. from an `as const` argument or a `readonly T[]` annotation). Literal
    /// candidates collected in this context are non-fresh — tsc does not widen the
    /// element literals of a readonly array/tuple, so `new Set([1, 2] as const)`
    /// infers `Set<1 | 2>` rather than `Set<number>`.
    pub(crate) in_readonly_source_context: bool,
    /// Implied arity per inference variable, mirroring tsc's `InferenceInfo.impliedArity`.
    ///
    /// Set during call-argument inference when a signature's non-array rest type is
    /// a bare type parameter (`function f<T>(...args: T)` or `...rest: T`). The
    /// implied arity is the number of trailing arguments that fall into the rest
    /// parameter, and it lets variadic tuple inference distribute the middle of a
    /// `[...A, ...B]` target between two adjacent variadic elements. Keyed by the
    /// root inference variable (see [`InferenceContext::set_implied_arity`]).
    pub(crate) implied_arities: FxHashMap<InferenceVar, usize>,
    /// Maps each tracked type parameter's *original* (declared) name to its
    /// inference variable. Distinct from `type_params`, which keys on the
    /// unique `__infer_*` placeholder name. The original name is needed only to
    /// recognize a self-referential inference — `T` (original) flowing into the
    /// variable that represents `T` — which carries no information and must not
    /// seed a (contra-)candidate. Mirrors tsc's `inferFromTypes` early-return
    /// when source and target are the same type parameter, which tsz's
    /// placeholder rename would otherwise miss.
    pub(crate) original_type_param_for_var: FxHashMap<Atom, InferenceVar>,
    /// Inference variables whose candidates come from a tuple packed out of
    /// trailing rest arguments (tsc's `getSpreadArgumentType` output), keyed
    /// by root to the literal-preservation mode of the call site. Candidate
    /// resolution widens such a variable's tuple result per element against
    /// its declared constraint instead of blanket literal-widening the whole
    /// tuple. Keying by variable (not tuple `TypeId`) keeps the mark across
    /// walker slicing and partial re-widening, and cannot collide with an
    /// identical tuple inferred for an unrelated variable.
    pub(crate) spread_rest_var_modes:
        FxHashMap<InferenceVar, crate::inference::spread_rest_literals::SpreadRestLiteralMode>,
    /// Snapshot of the inference variables *fixed* from the mandatory
    /// (non-context-sensitive) arguments, taken at the Round 1 boundary by
    /// [`InferenceContext::freeze_resolved_variables`]. It does not block later
    /// candidate collection; it records which fixes were authoritative so the
    /// final per-variable resolution can restore one when re-derivation from the
    /// merged candidate set only *widened* it. Approximates tsc's
    /// `InferenceInfo.isFixed`. Without it, two function parameters referencing
    /// the same two type parameters in swapped covariant/contravariant roles
    /// (e.g. `(z: U) => T` alongside `(x: T) => U`) union both fixed variables to
    /// the same widened type, silently accepting calls tsc rejects (issue
    /// #17282).
    pub(crate) frozen_vars: FxHashSet<InferenceVar>,
}

impl<'a> InferenceContext<'a> {
    pub(crate) const UPPER_BOUND_INTERSECTION_FAST_PATH_LIMIT: usize = 8;
    pub(crate) const UPPER_BOUND_INTERSECTION_LARGE_SET_THRESHOLD: usize = 64;
    /// Maximum depth for expanding `TypeApplication` targets during inference.
    /// Prevents infinite recursion for recursive type aliases.
    pub(crate) const MAX_APP_EXPANSION_DEPTH: u32 = 5;
    /// Maximum depth for `infer_from_types` structural recursion.
    /// Self-referential interfaces (e.g., `ArrayIterator<T>` with
    /// `[Symbol.iterator](): ArrayIterator<T>`) can cause unbounded
    /// recursion during structural property inference.
    pub(crate) const MAX_INFER_DEPTH: u32 = 20;

    /// Run one nested inference operation without leaking its variance/member
    /// routing modes into the next operation on this context.
    #[inline]
    pub(crate) fn with_restored_inference_modes<R>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let saved = (
            self.in_contra_mode,
            self.in_variance_walk,
            self.parameter_recovery_mode,
            self.in_bivariant_mode,
            self.pending_target_method,
        );
        let result = operation(self);
        self.in_contra_mode = saved.0;
        self.in_variance_walk = saved.1;
        self.parameter_recovery_mode = saved.2;
        self.in_bivariant_mode = saved.3;
        self.pending_target_method = saved.4;
        result
    }

    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        InferenceContext {
            interner,
            resolver: None,
            query_db: None,
            subtype_cache: RefCell::new(FxHashMap::default()),
            active_subtype_checks: RefCell::new(FxHashSet::default()),
            table: InPlaceUnificationTable::new(),
            type_params: Vec::new(),
            declared_constraints: FxHashMap::default(),
            literal_preserving_declared_constraints: FxHashSet::default(),
            app_expansion_depth: 0,
            in_contra_mode: false,
            in_variance_walk: false,
            parameter_recovery_mode: ParameterRecoveryMode::None,
            in_bivariant_mode: false,
            pending_target_method: false,
            reverse_mapped_properties: FxHashMap::default(),
            source_is_type_annotation: false,
            infer_depth: 0,
            infer_visited: FxHashSet::default(),
            top_level_in_return_type_unfixed: FxHashSet::default(),
            vars_with_substituted_candidates: FxHashSet::default(),
            in_array_element_context: false,
            in_context_sensitive_arg: false,
            in_readonly_source_context: false,
            implied_arities: FxHashMap::default(),
            original_type_param_for_var: FxHashMap::default(),
            spread_rest_var_modes: FxHashMap::default(),
            frozen_vars: FxHashSet::default(),
        }
    }

    pub fn with_query_db(query_db: &'a dyn QueryDatabase) -> Self {
        InferenceContext {
            interner: query_db.as_type_database(),
            resolver: Some(query_db),
            query_db: Some(query_db),
            subtype_cache: RefCell::new(FxHashMap::default()),
            active_subtype_checks: RefCell::new(FxHashSet::default()),
            table: InPlaceUnificationTable::new(),
            type_params: Vec::new(),
            declared_constraints: FxHashMap::default(),
            literal_preserving_declared_constraints: FxHashSet::default(),
            app_expansion_depth: 0,
            in_contra_mode: false,
            in_variance_walk: false,
            parameter_recovery_mode: ParameterRecoveryMode::None,
            in_bivariant_mode: false,
            pending_target_method: false,
            reverse_mapped_properties: FxHashMap::default(),
            source_is_type_annotation: false,
            infer_depth: 0,
            infer_visited: FxHashSet::default(),
            top_level_in_return_type_unfixed: FxHashSet::default(),
            vars_with_substituted_candidates: FxHashSet::default(),
            in_array_element_context: false,
            in_context_sensitive_arg: false,
            in_readonly_source_context: false,
            implied_arities: FxHashMap::default(),
            original_type_param_for_var: FxHashMap::default(),
            spread_rest_var_modes: FxHashMap::default(),
            frozen_vars: FxHashSet::default(),
        }
    }

    /// Return entry and size accounting for this context's operation-local caches.
    #[must_use]
    #[allow(dead_code)] // Inference cache accounting; consumed by inference unit tests
    pub(crate) fn cache_statistics(&self) -> InferenceContextCacheStatistics {
        let subtype_entries = self.subtype_cache.borrow().len();
        let estimated_size_bytes =
            subtype_entries.saturating_mul(std::mem::size_of::<((TypeId, TypeId), bool)>());
        InferenceContextCacheStatistics {
            subtype_entries,
            estimated_size_bytes,
        }
    }

    /// Mark an inference variable as representing a type parameter that
    /// occurs at the top level of the signature's return type and has not
    /// yet been fixed. Such variables suppress literal-type widening during
    /// covariant resolution, matching tsc's `getCovariantInference` gate.
    pub fn mark_top_level_in_return_type_unfixed(&mut self, var: InferenceVar) {
        let root = self.table.find(var);
        self.top_level_in_return_type_unfixed.insert(root);
    }

    /// Record that `var` is inferred from a tuple packed out of trailing
    /// rest arguments, so candidate resolution widens its literal elements
    /// per the declared constraint (tsc's `getSpreadArgumentType` rule)
    /// instead of blanket-widening the whole tuple.
    pub fn mark_spread_rest_var(
        &mut self,
        var: InferenceVar,
        mode: crate::inference::spread_rest_literals::SpreadRestLiteralMode,
    ) {
        let root = self.table.find(var);
        self.spread_rest_var_modes.insert(root, mode);
    }

    /// The spread-rest literal mode recorded for `var`, if its candidates
    /// come from a packed rest-argument tuple.
    pub fn spread_rest_mode_of(
        &mut self,
        var: InferenceVar,
    ) -> Option<crate::inference::spread_rest_literals::SpreadRestLiteralMode> {
        let root = self.table.find(var);
        self.spread_rest_var_modes.get(&root).copied()
    }

    /// Create a fresh inference variable
    pub fn fresh_var(&mut self) -> InferenceVar {
        self.table.new_key(InferenceInfo::default())
    }

    /// Create an inference variable for a type parameter
    pub fn fresh_type_param(&mut self, name: Atom, is_const: bool) -> InferenceVar {
        let var = self.fresh_var();
        self.type_params.push((name, var, is_const));
        var
    }

    /// Register an existing inference variable as representing a type parameter.
    ///
    /// This is useful when the caller needs to compute a unique placeholder name
    /// (and corresponding placeholder `TypeId`) after allocating the inference variable.
    pub fn register_type_param(&mut self, name: Atom, var: InferenceVar, is_const: bool) {
        self.type_params.push((name, var, is_const));
    }

    /// Record the *original* (declared) name of the type parameter that `var`
    /// represents. Only consulted to detect a self-referential inference (the
    /// declared parameter flowing into its own variable); it must not change
    /// `find_type_param`, which keys on the renamed placeholder so outer-scope
    /// parameters that share a name cannot alias a local variable.
    pub fn register_original_type_param_name(&mut self, name: Atom, var: InferenceVar) {
        self.original_type_param_for_var.insert(name, var);
    }

    /// Look up an inference variable by type parameter name
    pub fn find_type_param(&self, name: Atom) -> Option<InferenceVar> {
        self.type_params
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|(_, v, _)| *v)
    }

    /// Returns true when `ty` is a bare named `TypeParameter` whose declared
    /// name is the original name of the type parameter that `var` represents —
    /// i.e. inferring `var` against `ty` is a self-reference. The placeholder
    /// rename means `ty`'s name never matches the variable's tracked
    /// (placeholder) name, so the original-name registry is the only way to
    /// recognize this case.
    pub(crate) fn type_is_own_original_type_param(
        &mut self,
        var: InferenceVar,
        ty: TypeId,
    ) -> bool {
        let Some(TypeData::TypeParameter(info)) = self.interner.lookup(ty) else {
            return false;
        };
        match self.original_type_param_for_var.get(&info.name) {
            Some(&mapped) => self.table.find(mapped) == self.table.find(var),
            None => false,
        }
    }

    /// Record the implied arity for an inference variable (tsc's
    /// `InferenceInfo.impliedArity`). Keyed by the root variable so it survives
    /// later unification.
    pub(crate) fn set_implied_arity(&mut self, var: InferenceVar, arity: usize) {
        let root = self.table.find(var);
        self.implied_arities.insert(root, arity);
    }

    /// Resolve the root inference variable named by a `TypeParameter`/`Infer`
    /// placeholder type, or `None` if the type does not name a tracked variable.
    fn type_param_root_for_type(&mut self, ty: TypeId) -> Option<InferenceVar> {
        let name = match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info.name,
            _ => return None,
        };
        let var = self.find_type_param(name)?;
        Some(self.table.find(var))
    }

    /// Look up the implied arity for a target type that names an inference
    /// variable (a `TypeParameter`/`Infer` placeholder). Returns `None` when the
    /// type is not an inference variable or has no recorded implied arity.
    pub(crate) fn implied_arity_for_type(&mut self, ty: TypeId) -> Option<usize> {
        let root = self.type_param_root_for_type(ty)?;
        self.implied_arities.get(&root).copied()
    }

    /// Fixed arity implied by the declared constraint of the type parameter named
    /// by `ty`. Mirrors tsc's use of `getBaseConstraintOfType(param)` in the
    /// `(variadic, rest)` / `(rest, variadic)` middle cases: when the constraint
    /// is a non-variadic tuple, its length is the implied arity.
    pub(crate) fn constraint_fixed_arity_for_type(&mut self, ty: TypeId) -> Option<usize> {
        let declared = match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info.constraint,
            _ => return None,
        };
        let constraint = declared.or_else(|| {
            let root = self.type_param_root_for_type(ty)?;
            self.declared_constraints.get(&root).copied()
        })?;
        let TypeData::Tuple(list_id) = self.interner.lookup(constraint)? else {
            return None;
        };
        let elements = self.interner.tuple_list(list_id);
        if elements.iter().any(|element| element.rest) {
            return None;
        }
        Some(elements.len())
    }

    pub(crate) fn fixed_tuple_candidate_len_for_type(&mut self, ty: TypeId) -> Option<usize> {
        let name = match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info.name,
            _ => return None,
        };
        let var = self.find_type_param(name)?;
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates
            .iter()
            .chain(info.contra_candidates.iter())
            .filter_map(|candidate| {
                let TypeData::Tuple(list_id) = self.interner.lookup(candidate.type_id)? else {
                    return None;
                };
                let elements = self.interner.tuple_list(list_id);
                if elements.iter().any(|element| element.rest) {
                    return None;
                }
                Some((candidate.priority, elements.len()))
            })
            .min_by_key(|(priority, len)| (*priority, *len))
            .map(|(_, len)| len)
    }

    /// Record the declared `extends` constraint for an inference variable.
    pub fn set_declared_constraint(&mut self, var: InferenceVar, constraint: TypeId) {
        // Key by the unification root so the lookup in `resolve_with_constraints`
        // (which reads `declared_constraints.get(&table.find(var))`) finds the
        // entry even after `var` is unified with another inference variable.
        // This mirrors `mark_declared_constraint_preserves_literals`, which
        // already normalises to the root. Without it, a constraint such as
        // `U extends [T, ...T[]]` (whose var unifies during constraint
        // strengthening) loses its declared constraint at resolution time and
        // literal inferences for `U` are widened (the zod `arrayToEnum`/
        // `ZodIssueCode` family).
        let root = self.table.find(var);
        self.declared_constraints.insert(root, constraint);
    }

    /// Record that the declared `extends` constraint semantically preserves literals.
    pub fn mark_declared_constraint_preserves_literals(&mut self, var: InferenceVar) {
        let root = self.table.find(var);
        self.literal_preserving_declared_constraints.insert(root);
    }

    /// Get the declared `extends` constraint for an inference variable.
    pub fn get_declared_constraint(&mut self, var: InferenceVar) -> Option<TypeId> {
        let root = self.table.find(var);
        self.declared_constraints.get(&root).copied()
    }

    /// Check if an inference variable is a const type parameter
    pub fn is_var_const(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        self.type_params
            .iter()
            .any(|(_, v, is_const)| self.table.find(*v) == root && *is_const)
    }

    /// Probe the current value of an inference variable
    pub fn probe(&mut self, var: InferenceVar) -> Option<TypeId> {
        self.table.probe_value(var).resolved
    }

    /// Unify an inference variable with a concrete type
    #[allow(dead_code)] // Reserved for full constraint-based inference
    pub fn unify_var_type(&mut self, var: InferenceVar, ty: TypeId) -> Result<(), InferenceError> {
        // Get the root variable
        let root = self.table.find(var);

        if self.occurs_in(root, ty) {
            return Err(InferenceError::OccursCheck { var: root, ty });
        }

        // Check current value
        match self.table.probe_value(root).resolved {
            None => {
                self.table.union_value(
                    root,
                    InferenceInfo {
                        resolved: Some(ty),
                        ..InferenceInfo::default()
                    },
                );
                Ok(())
            }
            Some(existing) => {
                if self.types_compatible(existing, ty) {
                    Ok(())
                } else {
                    Err(InferenceError::Conflict(existing, ty))
                }
            }
        }
    }

    /// Unify two inference variables
    pub fn unify_vars(&mut self, a: InferenceVar, b: InferenceVar) -> Result<(), InferenceError> {
        let root_a = self.table.find(a);
        let root_b = self.table.find(b);

        if root_a == root_b {
            return Ok(());
        }

        let value_a = self.table.probe_value(root_a).resolved;
        let value_b = self.table.probe_value(root_b).resolved;
        if let (Some(a_ty), Some(b_ty)) = (value_a, value_b)
            && !self.types_compatible(a_ty, b_ty)
        {
            return Err(InferenceError::Conflict(a_ty, b_ty));
        }

        self.table
            .unify_var_var(root_a, root_b)
            .map_err(|_| InferenceError::Conflict(TypeId::ERROR, TypeId::ERROR))?;
        Ok(())
    }

    /// Check if two types are compatible for unification
    fn types_compatible(&self, a: TypeId, b: TypeId) -> bool {
        if a == b {
            return true;
        }

        // Any is compatible with everything
        if a == TypeId::ANY || b == TypeId::ANY {
            return true;
        }

        // Unknown is compatible with everything
        if a == TypeId::UNKNOWN || b == TypeId::UNKNOWN {
            return true;
        }

        // Never is compatible with everything
        if a == TypeId::NEVER || b == TypeId::NEVER {
            return true;
        }

        false
    }

    pub(crate) fn occurs_in(&mut self, var: InferenceVar, ty: TypeId) -> bool {
        let root = self.table.find(var);
        if self.type_params.is_empty() {
            return false;
        }

        let mut visited = FxHashSet::default();
        for &(atom, param_var, _) in &self.type_params {
            if self.table.find(param_var) == root
                && self.type_contains_param(ty, atom, &mut visited)
            {
                return true;
            }
        }
        false
    }

    pub(crate) fn type_param_names_for_root(&mut self, root: InferenceVar) -> Vec<Atom> {
        self.type_params
            .iter()
            .filter(|&(_name, var, _)| self.table.find(*var) == root)
            .map(|(name, _var, _)| *name)
            .collect()
    }

    pub(crate) fn upper_bound_cycles_param(&mut self, bound: TypeId, targets: &[Atom]) -> bool {
        let mut params = FxHashSet::default();
        let mut visited = FxHashSet::default();
        self.collect_type_params(bound, &mut params, &mut visited);

        for name in params {
            let mut seen = FxHashSet::default();
            if self.param_depends_on_targets(name, targets, &mut seen) {
                return true;
            }
        }

        false
    }

    pub(crate) fn expand_cyclic_upper_bound(
        &mut self,
        root: InferenceVar,
        bound: TypeId,
        target_names: &[Atom],
        candidates: &mut Vec<InferenceCandidate>,
        upper_bounds: &mut Vec<TypeId>,
    ) {
        if bound.is_intrinsic() {
            return;
        }
        let name = match self.interner.lookup(bound) {
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info.name,
            _ => return,
        };

        let Some(var) = self.find_type_param(name) else {
            return;
        };

        if let Some(resolved) = self.probe(var) {
            if !upper_bounds.contains(&resolved) {
                upper_bounds.push(resolved);
            }
            return;
        }

        let bound_root = self.table.find(var);
        let info = self.table.probe_value(bound_root);

        for candidate in info.candidates {
            if self.occurs_in(root, candidate.type_id) {
                continue;
            }
            candidates.push(InferenceCandidate {
                type_id: candidate.type_id,
                priority: InferencePriority::Circular,
                is_fresh_literal: candidate.is_fresh_literal,
                from_object_property: candidate.from_object_property,
                from_index_signature: candidate.from_index_signature,
                object_property_index: candidate.object_property_index,
                object_property_name: candidate.object_property_name,
                source_is_type_annotation: candidate.source_is_type_annotation,
                from_array_element: candidate.from_array_element,
                from_readonly_source: candidate.from_readonly_source,
                from_context_sensitive_arg: candidate.from_context_sensitive_arg,
            });
        }

        for ty in info.upper_bounds {
            if self.occurs_in(root, ty) {
                continue;
            }
            if !target_names.is_empty() && self.upper_bound_cycles_param(ty, target_names) {
                continue;
            }
            if !upper_bounds.contains(&ty) {
                upper_bounds.push(ty);
            }
        }
    }

    fn collect_type_params(
        &self,
        ty: TypeId,
        params: &mut FxHashSet<Atom>,
        visited: &mut FxHashSet<TypeId>,
    ) {
        if ty.is_intrinsic() {
            return;
        }
        match guard_state::type_graph_visit_state(visited.insert(ty)) {
            guard_state::TypeGraphVisitState::Entered => {}
            guard_state::TypeGraphVisitState::AlreadyVisited => return,
        }
        let Some(key) = self.interner.lookup(ty) else {
            return;
        };

        match key {
            TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                params.insert(info.name);
            }
            TypeData::Array(elem) => {
                self.collect_type_params(elem, params, visited);
            }
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                for element in elements.iter() {
                    self.collect_type_params(element.type_id, params, visited);
                }
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    self.collect_type_params(member, params, visited);
                }
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.collect_type_params(prop.type_id, params, visited);
                }
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.collect_type_params(prop.type_id, params, visited);
                }
                if let Some(index) = shape.string_index.as_ref() {
                    self.collect_type_params(index.key_type, params, visited);
                    self.collect_type_params(index.value_type, params, visited);
                }
                if let Some(index) = shape.number_index.as_ref() {
                    self.collect_type_params(index.key_type, params, visited);
                    self.collect_type_params(index.value_type, params, visited);
                }
            }
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.collect_type_params(app.base, params, visited);
                for &arg in &app.args {
                    self.collect_type_params(arg, params, visited);
                }
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                for param in &shape.params {
                    self.collect_type_params(param.type_id, params, visited);
                }
                if let Some(this_type) = shape.this_type {
                    self.collect_type_params(this_type, params, visited);
                }
                self.collect_type_params(shape.return_type, params, visited);
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                for sig in &shape.call_signatures {
                    for param in &sig.params {
                        self.collect_type_params(param.type_id, params, visited);
                    }
                    if let Some(this_type) = sig.this_type {
                        self.collect_type_params(this_type, params, visited);
                    }
                    self.collect_type_params(sig.return_type, params, visited);
                }
                for sig in &shape.construct_signatures {
                    for param in &sig.params {
                        self.collect_type_params(param.type_id, params, visited);
                    }
                    if let Some(this_type) = sig.this_type {
                        self.collect_type_params(this_type, params, visited);
                    }
                    self.collect_type_params(sig.return_type, params, visited);
                }
                for prop in &shape.properties {
                    self.collect_type_params(prop.type_id, params, visited);
                }
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(cond_id);
                self.collect_type_params(cond.check_type, params, visited);
                self.collect_type_params(cond.extends_type, params, visited);
                self.collect_type_params(cond.true_type, params, visited);
                self.collect_type_params(cond.false_type, params, visited);
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.get_mapped(mapped_id);
                self.collect_type_params(mapped.constraint, params, visited);
                if let Some(name_type) = mapped.name_type {
                    self.collect_type_params(name_type, params, visited);
                }
                self.collect_type_params(mapped.template, params, visited);
            }
            TypeData::IndexAccess(obj, idx) => {
                self.collect_type_params(obj, params, visited);
                self.collect_type_params(idx, params, visited);
            }
            TypeData::KeyOf(operand) | TypeData::ReadonlyType(operand) => {
                self.collect_type_params(operand, params, visited);
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                for span in spans.iter() {
                    if let TemplateSpan::Type(inner) = span {
                        self.collect_type_params(*inner, params, visited);
                    }
                }
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.collect_type_params(type_arg, params, visited);
            }
            TypeData::Enum(_def_id, member_type) => {
                // Recurse into the structural member type
                self.collect_type_params(member_type, params, visited);
            }
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::UnresolvedTypeName(_)
            | TypeData::Error => {}
            TypeData::NoInfer(inner) => {
                self.collect_type_params(inner, params, visited);
            }
            TypeData::Substitution {
                base_type,
                constraint,
            } => {
                self.collect_type_params(base_type, params, visited);
                self.collect_type_params(constraint, params, visited);
            }
        }
    }

    fn param_depends_on_targets(
        &mut self,
        name: Atom,
        targets: &[Atom],
        visited: &mut FxHashSet<Atom>,
    ) -> bool {
        let is_target = targets.contains(&name);
        let inserted_visit = !is_target && visited.insert(name);
        match guard_state::param_dependency_state(is_target, inserted_visit) {
            guard_state::ParamDependencyState::TargetReached => return true,
            guard_state::ParamDependencyState::Entered => {}
            guard_state::ParamDependencyState::AlreadyVisited => return false,
        }
        let Some(var) = self.find_type_param(name) else {
            return false;
        };
        let root = self.table.find(var);
        let upper_bounds = self.table.probe_value(root).upper_bounds;

        for bound in upper_bounds {
            for target in targets {
                let mut seen = FxHashSet::default();
                if self.type_contains_param(bound, *target, &mut seen) {
                    return true;
                }
            }
            if !bound.is_intrinsic()
                && let Some(TypeData::TypeParameter(info)) = self.interner.lookup(bound)
                && self.param_depends_on_targets(info.name, targets, visited)
            {
                return true;
            }
        }

        false
    }

    fn type_contains_param(
        &self,
        ty: TypeId,
        target: Atom,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if ty.is_intrinsic() {
            return false;
        }
        match guard_state::type_graph_visit_state(visited.insert(ty)) {
            guard_state::TypeGraphVisitState::Entered => {}
            guard_state::TypeGraphVisitState::AlreadyVisited => return false,
        }

        let key = match self.interner.lookup(ty) {
            Some(key) => key,
            None => return false,
        };

        match key {
            TypeData::TypeParameter(info) | TypeData::Infer(info) => info.name == target,
            TypeData::Array(elem) => self.type_contains_param(elem, target, visited),
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|e| self.type_contains_param(e.type_id, target, visited))
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_param(member, target, visited))
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|p| self.type_contains_param(p.type_id, target, visited))
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|p| self.type_contains_param(p.type_id, target, visited))
                    || shape.string_index.as_ref().is_some_and(|idx| {
                        self.type_contains_param(idx.key_type, target, visited)
                            || self.type_contains_param(idx.value_type, target, visited)
                    })
                    || shape.number_index.as_ref().is_some_and(|idx| {
                        self.type_contains_param(idx.key_type, target, visited)
                            || self.type_contains_param(idx.value_type, target, visited)
                    })
            }
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.type_contains_param(app.base, target, visited)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_contains_param(arg, target, visited))
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                if shape.type_params.iter().any(|tp| tp.name == target) {
                    return false;
                }
                shape
                    .this_type
                    .is_some_and(|this_type| self.type_contains_param(this_type, target, visited))
                    || shape
                        .params
                        .iter()
                        .any(|p| self.type_contains_param(p.type_id, target, visited))
                    || self.type_contains_param(shape.return_type, target, visited)
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                let in_call = shape.call_signatures.iter().any(|sig| {
                    if sig.type_params.iter().any(|tp| tp.name == target) {
                        false
                    } else {
                        sig.this_type.is_some_and(|this_type| {
                            self.type_contains_param(this_type, target, visited)
                        }) || sig
                            .params
                            .iter()
                            .any(|p| self.type_contains_param(p.type_id, target, visited))
                            || self.type_contains_param(sig.return_type, target, visited)
                    }
                });
                if in_call {
                    return true;
                }
                let in_construct = shape.construct_signatures.iter().any(|sig| {
                    if sig.type_params.iter().any(|tp| tp.name == target) {
                        false
                    } else {
                        sig.this_type.is_some_and(|this_type| {
                            self.type_contains_param(this_type, target, visited)
                        }) || sig
                            .params
                            .iter()
                            .any(|p| self.type_contains_param(p.type_id, target, visited))
                            || self.type_contains_param(sig.return_type, target, visited)
                    }
                });
                if in_construct {
                    return true;
                }
                shape
                    .properties
                    .iter()
                    .any(|p| self.type_contains_param(p.type_id, target, visited))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(cond_id);
                self.type_contains_param(cond.check_type, target, visited)
                    || self.type_contains_param(cond.extends_type, target, visited)
                    || self.type_contains_param(cond.true_type, target, visited)
                    || self.type_contains_param(cond.false_type, target, visited)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.get_mapped(mapped_id);
                if mapped.type_param.name == target {
                    return false;
                }
                self.type_contains_param(mapped.constraint, target, visited)
                    || self.type_contains_param(mapped.template, target, visited)
            }
            TypeData::IndexAccess(obj, idx) => {
                self.type_contains_param(obj, target, visited)
                    || self.type_contains_param(idx, target, visited)
            }
            TypeData::KeyOf(operand) | TypeData::ReadonlyType(operand) => {
                self.type_contains_param(operand, target, visited)
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.type_contains_param(*inner, target, visited),
                })
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.type_contains_param(type_arg, target, visited)
            }
            TypeData::Enum(_def_id, member_type) => {
                // Recurse into the structural member type
                self.type_contains_param(member_type, target, visited)
            }
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::UnresolvedTypeName(_)
            | TypeData::Error => false,
            TypeData::NoInfer(inner) => self.type_contains_param(inner, target, visited),
            TypeData::Substitution {
                base_type,
                constraint,
            } => {
                self.type_contains_param(base_type, target, visited)
                    || self.type_contains_param(constraint, target, visited)
            }
        }
    }

    /// Resolve all type parameters to concrete types
    #[expect(dead_code)] // Reserved for full constraint-based inference
    pub fn resolve_all(&mut self) -> Result<Vec<(Atom, TypeId)>, InferenceError> {
        // Clone type_params to avoid borrow conflict
        let type_params: Vec<_> = self.type_params.clone();
        let mut results = Vec::new();
        for (name, var, _) in type_params {
            match self.probe(var) {
                Some(ty) => results.push((name, ty)),
                None => return Err(InferenceError::Unresolved(var)),
            }
        }
        Ok(results)
    }

    /// Get the interner reference
    #[expect(dead_code)] // Reserved for full constraint-based inference
    pub fn interner(&self) -> &dyn TypeDatabase {
        self.interner
    }

    /// Substitute source inference variable placeholders in the candidates
    /// and upper bounds of a set of target variables.
    ///
    /// When a generic function is passed as an argument to another generic function,
    /// the constraint collector creates "source" inference variables for the inner
    /// function's type parameters. These may leak into the outer variables' candidates
    /// as raw `TypeParameter` placeholders (e.g., `Array<__infer_src_3>`).
    ///
    /// This method resolves those source variables and substitutes their resolved
    /// types back into the outer variables' candidates, so the resolution phase
    /// sees concrete types instead of opaque placeholders.
    pub fn substitute_source_vars_in_targets(
        &mut self,
        target_vars: &[InferenceVar],
        source_subst: &crate::instantiation::instantiate::TypeSubstitution,
        interner: &dyn TypeDatabase,
    ) {
        use crate::instantiation::instantiate::instantiate_type;
        let target_set: FxHashSet<InferenceVar> =
            target_vars.iter().map(|v| self.table.find(*v)).collect();
        for &var in target_vars {
            let root = self.table.find(var);
            let info = self.table.probe_value(root);
            let mut changed = false;
            let mut new_candidates: Vec<InferenceCandidate> = info
                .candidates
                .iter()
                .map(|c| {
                    let subst_ty = instantiate_type(interner, c.type_id, source_subst);
                    if subst_ty != c.type_id {
                        changed = true;
                    }
                    InferenceCandidate {
                        type_id: subst_ty,
                        ..*c
                    }
                })
                .collect();
            let mut new_contra: Vec<InferenceCandidate> = info
                .contra_candidates
                .iter()
                .map(|c| {
                    let subst_ty = instantiate_type(interner, c.type_id, source_subst);
                    if subst_ty != c.type_id {
                        changed = true;
                    }
                    InferenceCandidate {
                        type_id: subst_ty,
                        ..*c
                    }
                })
                .collect();
            let new_upper: Vec<TypeId> = info
                .upper_bounds
                .iter()
                .map(|&ub| {
                    let subst_ty = instantiate_type(interner, ub, source_subst);
                    if subst_ty != ub {
                        changed = true;
                    }
                    subst_ty
                })
                .collect();
            if changed {
                // Filter out candidates that are themselves target inference variables.
                // After substitution, a candidate like `__infer_src_Y` might resolve to
                // `__infer_1`, which is another outer var. Remove such self-references
                // to prevent circular resolution.
                new_candidates.retain(|c| {
                    if let Some(TypeData::TypeParameter(_)) = interner.lookup(c.type_id) {
                        // Check if this type parameter is one of our target inference variables
                        !target_set
                            .iter()
                            .any(|&tv| self.table.probe_value(tv).resolved == Some(c.type_id))
                    } else {
                        true
                    }
                });
                new_contra.retain(|c| {
                    if let Some(TypeData::TypeParameter(_)) = interner.lookup(c.type_id) {
                        !target_set
                            .iter()
                            .any(|&tv| self.table.probe_value(tv).resolved == Some(c.type_id))
                    } else {
                        true
                    }
                });
                self.table.union_value(
                    root,
                    InferenceInfo {
                        candidates: new_candidates,
                        contra_candidates: new_contra,
                        upper_bounds: new_upper,
                        resolved: info.resolved,
                    },
                );
                self.vars_with_substituted_candidates.insert(root);
            }
        }
    }

    // =========================================================================
    // Constraint Collection
    // =========================================================================

    /// Add a lower bound constraint: ty <: var
    /// This is used when an argument type flows into a type parameter.
    /// Updated to use `NakedTypeVariable` (highest priority) for direct argument inference.
    #[allow(dead_code)] // Reserved for full constraint-based inference
    pub fn add_lower_bound(&mut self, var: InferenceVar, ty: TypeId) {
        self.add_candidate(var, ty, InferencePriority::NakedTypeVariable);
    }

    /// Add an inference candidate for a variable.
    pub fn add_candidate(&mut self, var: InferenceVar, ty: TypeId, priority: InferencePriority) {
        self.add_candidate_with_context(var, ty, priority, CandidateContext::default());
    }

    /// Add a contravariant inference candidate for a variable.
    /// Used when the type parameter appears in a contravariant position
    /// (e.g., function parameter types). When only `contra_candidates` exist
    /// (no covariant candidates), resolution uses tsc's priority-sensitive
    /// common-subtype or intersection behavior.
    pub fn add_contra_candidate(
        &mut self,
        var: InferenceVar,
        ty: TypeId,
        priority: InferencePriority,
    ) {
        // Inferring a type parameter against *itself* carries no information.
        // The placeholder rename hides this: the inference variable is tracked
        // under a unique `__infer_*` placeholder, while a callback parameter
        // contextually typed with the un-instantiated signature (e.g.
        // `(ev: EventMap[K]) => any`) leaks the *declared* `K` into the
        // contravariant matcher. Recovered via the original-name registry, such a
        // self-referential bare type parameter must not become a contra-candidate
        // — otherwise it overrides a legitimate covariant inference (e.g.
        // `K = "message"`). Mirrors tsc's `inferFromTypes` same-type-parameter
        // early return.
        if self.type_is_own_original_type_param(var, ty) {
            return;
        }
        let root = self.table.find(var);
        let candidate = InferenceCandidate {
            type_id: ty,
            priority,
            is_fresh_literal: is_literal_type(self.interner, ty)
                && !self.in_readonly_source_context,
            from_object_property: false,
            from_index_signature: false,
            object_property_index: None,
            object_property_name: None,
            source_is_type_annotation: self.source_is_type_annotation,
            from_array_element: self.in_array_element_context,
            from_readonly_source: self.candidate_is_from_readonly_source(ty),
            from_context_sensitive_arg: self.in_context_sensitive_arg,
        };
        self.table.union_value(
            root,
            InferenceInfo {
                contra_candidates: vec![candidate],
                ..InferenceInfo::default()
            },
        );
    }

    /// Add an inference candidate for a variable that originates from an object property.
    /// `object_property_index` captures the source property order and enables deterministic
    /// tie-breaking when repeated property candidates collapse to a union.
    /// `source_is_fresh` indicates whether the source object is a fresh literal (from an
    /// object literal expression). When true, literal property types will be widened during
    /// inference resolution (matching TSC's `RequiresWidening` behavior).
    pub fn add_property_candidate_with_index(
        &mut self,
        var: InferenceVar,
        ty: TypeId,
        priority: InferencePriority,
        object_property_index: u32,
        object_property_name: Option<Atom>,
        source_is_fresh: bool,
    ) {
        self.add_candidate_with_context(
            var,
            ty,
            priority,
            CandidateContext {
                from_object_property: true,
                object_property_index: Some(object_property_index),
                object_property_name,
                source_is_fresh,
                ..CandidateContext::default()
            },
        );
    }

    pub fn add_index_signature_candidate_with_index(
        &mut self,
        var: InferenceVar,
        ty: TypeId,
        priority: InferencePriority,
        object_property_index: u32,
        source_is_fresh: bool,
    ) {
        self.add_candidate_with_context(
            var,
            ty,
            priority,
            CandidateContext {
                from_object_property: true,
                from_index_signature: true,
                object_property_index: Some(object_property_index),
                source_is_fresh,
                ..CandidateContext::default()
            },
        );
    }

    fn add_candidate_with_context(
        &mut self,
        var: InferenceVar,
        ty: TypeId,
        priority: InferencePriority,
        context: CandidateContext,
    ) {
        // In a contravariant position, a candidate that is the variable's own
        // declared type parameter is a self-reference carrying no information
        // (see `add_contra_candidate`). The placeholder rename hides it from the
        // `occurs_in` self-referential filter at fixing time, so skip it here —
        // otherwise the leaked bare parameter becomes a contra-candidate that
        // overrides a legitimate covariant inference. Covariant routing keeps its
        // existing behavior (handled by `discard_self_referential_candidates`).
        if self.collects_contra_candidates() && self.type_is_own_original_type_param(var, ty) {
            return;
        }
        let root = self.table.find(var);
        // A candidate is a "fresh literal" (eligible for widening) when:
        // - It's a literal type AND
        // - Either it's NOT from an object property (direct arg like identity("hello")),
        //   OR the source object is a fresh literal (from object literal expression).
        // This matches TSC's RequiresWidening flag: literals from type annotations
        // (non-fresh sources) are NOT widened, but literals from object literal
        // expressions ARE widened.
        let candidate = InferenceCandidate {
            type_id: ty,
            priority,
            is_fresh_literal: (!context.from_object_property || context.source_is_fresh)
                && (is_literal_type(self.interner, ty)
                    || (self.in_array_element_context
                        && array_element_union_widens_literals(self.interner, ty)))
                && !self.source_is_type_annotation
                && !self.in_readonly_source_context,
            from_object_property: context.from_object_property,
            from_index_signature: context.from_index_signature,
            object_property_index: context.object_property_index,
            object_property_name: context.object_property_name,
            source_is_type_annotation: self.source_is_type_annotation,
            from_array_element: self.in_array_element_context,
            from_readonly_source: self.candidate_is_from_readonly_source(ty),
            from_context_sensitive_arg: self.in_context_sensitive_arg,
        };
        if self.collects_contra_candidates() {
            // In contravariant context (e.g., callback parameter structural
            // decomposition), route to contra_candidates so they are resolved
            // via intersection and only used when no covariant candidates exist.
            self.table.union_value(
                root,
                InferenceInfo {
                    contra_candidates: vec![candidate],
                    ..InferenceInfo::default()
                },
            );
        } else {
            self.table.union_value(
                root,
                InferenceInfo {
                    candidates: vec![candidate],
                    ..InferenceInfo::default()
                },
            );
        }
    }

    /// Whether candidates at the current point use TypeScript's
    /// contravariant candidate set. A target signature whose declaration
    /// origin grants parameter bivariance preserves the contravariant traversal
    /// direction while deliberately suppressing this routing.
    pub(crate) const fn collects_contra_candidates(&self) -> bool {
        self.in_contra_mode && !self.in_bivariant_mode
    }

    /// Compact behavior mode for the hot structural-matcher cycle key.
    pub(crate) const fn inference_visit_mode(&self) -> u8 {
        (self.in_contra_mode as u8)
            | ((self.in_variance_walk as u8) << 1)
            | ((self.in_bivariant_mode as u8) << 2)
            | ((self.pending_target_method as u8) << 3)
            | ((self.parameter_recovery_mode as u8) << 4)
    }

    /// Compact behavior mode for the constraint-walker cycle key. The sticky
    /// matcher-only variance bit is deliberately omitted so it cannot duplicate
    /// constraint work or consume the shared step budget.
    pub(crate) const fn constraint_visit_mode(&self) -> u8 {
        (self.in_contra_mode as u8)
            | ((self.in_bivariant_mode as u8) << 1)
            | ((self.pending_target_method as u8) << 2)
            | ((self.parameter_recovery_mode as u8) << 3)
    }

    fn candidate_is_from_readonly_source(&self, ty: TypeId) -> bool {
        self.in_readonly_source_context || self.type_is_readonly_array_like(ty)
    }

    fn type_is_readonly_array_like(&self, ty: TypeId) -> bool {
        if ty.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(ty) {
            Some(TypeData::ReadonlyType(inner)) => {
                matches!(
                    self.interner.lookup(inner),
                    Some(TypeData::Array(_) | TypeData::Tuple(_))
                ) || self.type_is_readonly_array_like(inner)
            }
            Some(TypeData::Union(members) | TypeData::Intersection(members)) => self
                .interner
                .type_list(members)
                .iter()
                .any(|&member| self.type_is_readonly_array_like(member)),
            _ => false,
        }
    }

    /// Add an upper bound constraint: var <: ty
    /// This is used for `extends` constraints on type parameters.
    pub fn add_upper_bound(&mut self, var: InferenceVar, ty: TypeId) {
        let root = self.table.find(var);
        self.table.union_value(
            root,
            InferenceInfo {
                upper_bounds: vec![ty],
                ..InferenceInfo::default()
            },
        );
    }

    /// Get the constraints for a variable
    pub fn get_constraints(&mut self, var: InferenceVar) -> Option<ConstraintSet> {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        if info.is_empty() {
            None
        } else {
            Some(ConstraintSet::from_info(&info))
        }
    }

    /// Check whether an inference variable has any candidates (covariant or contravariant).
    pub fn var_has_candidates(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        !info.candidates.is_empty() || !info.contra_candidates.is_empty()
    }

    /// Check whether an inference variable has `contra_candidates` with at least one
    /// concrete (non-TypeParameter) type. `TypeParameter` types in `contra_candidates`
    /// are typically unresolved source inference placeholders from generic function
    /// arguments and should not drive the resolution gate.
    #[expect(dead_code)] // Reserved contra-candidate resolution-gate query
    pub fn has_concrete_contra_candidates(
        &mut self,
        var: InferenceVar,
        db: &dyn crate::caches::db::TypeDatabase,
    ) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.contra_candidates.iter().any(|c| {
            c.type_id.is_intrinsic()
                || !matches!(db.lookup(c.type_id), Some(TypeData::TypeParameter(_)))
        })
    }

    /// Returns `true` if `type_id` is a **call-local** bare inference placeholder —
    /// a bare `__infer_*` `TypeParameter` whose name-atom is registered in this
    /// context's `type_params`. Placeholders from outer generic call scopes have
    /// atoms that are not in `type_params` and must not be filtered: they carry
    /// real cross-call inference evidence (e.g. a recursive call's argument type
    /// constrained by the outer function's unresolved type parameter).
    pub(crate) fn is_local_inference_placeholder(&self, type_id: TypeId) -> bool {
        if !crate::type_queries::data::is_bare_current_infer_placeholder_db(self.interner, type_id)
        {
            return false;
        }
        match self.interner.lookup(type_id) {
            // `TypeData::Infer` nodes are always created within the current context.
            Some(TypeData::TypeParameter(tp)) => {
                self.type_params.iter().any(|(atom, _, _)| *atom == tp.name)
            }
            _ => true,
        }
    }

    /// Check whether an inference variable has any contravariant candidates that are
    /// usable for resolution. Call-local inference placeholders like `__infer_*`
    /// are excluded, but higher-order source placeholders (`__infer_src_*`) and real
    /// outer type parameters are preserved because they carry cross-generic evidence.
    pub fn has_usable_contra_candidates(
        &mut self,
        var: InferenceVar,
        _db: &dyn crate::caches::db::TypeDatabase,
    ) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.contra_candidates
            .iter()
            .any(|c| !self.is_local_inference_placeholder(c.type_id))
    }

    /// Returns `true` when `candidate` should be kept as a concrete
    /// contra-variance candidate. Call-local `__infer_*` placeholders are
    /// excluded; foreign bare placeholders and composite types that contain
    /// real type parameters are kept.
    pub(crate) fn is_concrete_contra_candidate(&self, type_id: TypeId) -> bool {
        if self.is_local_inference_placeholder(type_id) {
            return false;
        }
        if crate::type_queries::data::is_bare_current_infer_placeholder_db(self.interner, type_id) {
            return true;
        }
        // Composite types built entirely from local placeholders are stale.
        if crate::type_queries::data::contains_current_infer_placeholder_db(self.interner, type_id)
            && !crate::type_queries::data::contains_non_infer_type_parameters_db(
                self.interner,
                type_id,
            )
        {
            return false;
        }
        true
    }

    /// Returns `true` if any covariant candidate for `var` is or contains an
    /// `IndexAccess` type (`T[K]` pattern). The circular-inference guard uses
    /// this to distinguish true circular inference (passing `T[K]` to `T`)
    /// from legitimate outer-`TypeParameter` forwarding (passing `T_outer` to
    /// `T_inner` where they happen to resolve to the same `TypeParameter`).
    pub fn has_index_access_covariant_candidate(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let db = self.interner;
        self.table
            .probe_value(root)
            .candidates
            .iter()
            .any(|c| type_contains_index_access(db, c.type_id))
    }

    /// Returns `true` when a covariant candidate for `var` is or contains an
    /// `IndexAccess` that references `var`'s OWN original declared type
    /// parameter — the circular self-inference signal of a recursive generic
    /// call, where `deepMap(value[key], fn)` infers `T` from `T[K]` with `T`
    /// being the very parameter under inference.
    ///
    /// The circular-inference guard normally recognizes this shape through a
    /// contravariant candidate contributed by the recursive call's other
    /// arguments (e.g. `fn: (v: T) => U`). In a self-recursive call, however,
    /// that contra candidate is the callee's OWN type parameter, so
    /// [`add_contra_candidate`](Self::add_contra_candidate) deliberately drops
    /// it as a non-informative self-reference. This covariant-side check keeps
    /// the guard able to fire without the suppressed contra candidate, while an
    /// independent call like `identity(value[key])` does not fire: there the
    /// index access references a *foreign* parameter (`deepMap`'s `T`), not
    /// `identity`'s own `U`.
    pub fn has_own_type_param_index_access_covariant_candidate(
        &mut self,
        var: InferenceVar,
    ) -> bool {
        let root = self.table.find(var);
        // Original declared names whose inference var unifies with this root.
        let entries: Vec<(Atom, InferenceVar)> = self
            .original_type_param_for_var
            .iter()
            .map(|(&name, &mapped)| (name, mapped))
            .collect();
        let own_names: Vec<Atom> = entries
            .into_iter()
            .filter(|&(_, mapped)| self.table.find(mapped) == root)
            .map(|(name, _)| name)
            .collect();
        if own_names.is_empty() {
            return false;
        }
        let candidate_types: Vec<TypeId> = self
            .table
            .probe_value(root)
            .candidates
            .iter()
            .map(|c| c.type_id)
            .collect();
        let db = self.interner;
        candidate_types.into_iter().any(|ty| {
            type_contains_index_access(db, ty)
                && own_names
                    .iter()
                    .any(|&name| contains_type_parameter_named(db, ty, name))
        })
    }

    /// Check whether a variable's inference came exclusively from contravariant positions.
    pub fn has_only_contra_candidates(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates.is_empty() && !info.contra_candidates.is_empty()
    }

    /// Return deduplicated contravariant candidate types for an inference variable.
    pub fn get_contra_candidate_types(&mut self, var: InferenceVar) -> Vec<TypeId> {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        let mut out = Vec::with_capacity(info.contra_candidates.len());
        for candidate in &info.contra_candidates {
            if !out.contains(&candidate.type_id) {
                out.push(candidate.type_id);
            }
        }
        out
    }

    pub fn has_index_signature_candidates(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates
            .iter()
            .any(|candidate| candidate.from_index_signature)
    }

    /// Check if all inference candidates for a variable have `ReturnType` priority.
    /// This indicates the type was inferred from callback return types (Round 2),
    /// not from direct arguments (Round 1).
    pub fn all_candidates_are_return_type(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        !info.candidates.is_empty()
            && info
                .candidates
                .iter()
                .all(|c| c.priority == InferencePriority::ReturnType)
    }

    /// Get the original un-widened literal candidate types for an inference variable.
    pub fn get_literal_candidates(&mut self, var: InferenceVar) -> Vec<TypeId> {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates
            .iter()
            .filter(|c| c.is_fresh_literal)
            .map(|c| c.type_id)
            .collect()
    }

    /// Check if all covariant candidates for a variable are fresh literals.
    /// When false, the resolved type should NOT be widened by `widen_literal_type`
    /// (matches tsc's `getWidenedLiteralType` which only widens fresh literals).
    pub fn all_candidates_are_fresh_literals(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        !info.candidates.is_empty() && info.candidates.iter().all(|c| c.is_fresh_literal)
    }

    /// Returns true when every candidate for `var` was inferred from an array
    /// element match (`T[]` vs `"a"[]`). Used to widen scalar fresh literals in
    /// `NoInfer<T>` positions, matching tsc's BCT widening of array literals.
    pub fn all_candidates_from_array_elements(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        !info.candidates.is_empty() && info.candidates.iter().all(|c| c.from_array_element)
    }

    /// Returns true when every candidate for `var` was inferred from an
    /// object-literal property match (`{ value: T }` vs `{ value: 1 }`). A
    /// literal inferred through an object-literal property is widened to its
    /// primitive in tsc's `getInferredType` regardless of whether the type
    /// parameter is also at the top level of the return type, so a sibling
    /// `NoInfer<T>` position must be checked against the widened type. Only a
    /// *direct* top-level argument (`value: T`) preserves the literal. Mirrors
    /// `all_candidates_from_array_elements` for the object-property case.
    pub fn all_candidates_from_object_property(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        !info.candidates.is_empty() && info.candidates.iter().all(|c| c.from_object_property)
    }

    /// Returns true when at least one fresh literal candidate came from array
    /// element inference. This is narrower than `all_candidates_from_array_elements`
    /// so mixed direct/callback inference can still recognize literal-array evidence.
    pub fn has_fresh_array_element_candidate(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates
            .iter()
            .any(|c| c.from_array_element && c.is_fresh_literal)
    }

    /// Returns `true` if any covariant candidate came from a type assertion (`expr as T`).
    /// Asserted types are non-fresh and must not be widened.
    pub fn has_type_annotation_candidates(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates.iter().any(|c| c.source_is_type_annotation)
    }

    /// Returns true when the winning covariant candidate type was produced
    /// while descending through a readonly array/tuple source.
    pub fn has_readonly_source_candidate_for(&mut self, var: InferenceVar, ty: TypeId) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        info.candidates
            .iter()
            .any(|candidate| candidate.type_id == ty && candidate.from_readonly_source)
    }

    pub fn set_resolved_type(&mut self, var: InferenceVar, ty: TypeId) {
        let root = self.table.find(var);
        let mut info = self.table.probe_value(root);
        info.resolved = Some(ty);
        self.table.union_value(root, info);
    }
}

/// Returns `true` when `ty` is or structurally contains an `IndexAccess` type.
fn type_contains_index_access(db: &dyn crate::construction::TypeDatabase, ty: TypeId) -> bool {
    if ty.is_intrinsic() {
        return false;
    }
    match db.lookup(ty) {
        Some(TypeData::IndexAccess(_, _)) => true,
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => db
            .type_list(list_id)
            .iter()
            .any(|&m| type_contains_index_access(db, m)),
        _ => false,
    }
}

// DISABLED: Tests use deprecated add_candidate / resolve_with_constraints API
// The inference system has been refactored to use unification-based inference.
#[cfg(test)]
#[path = "../../tests/infer_tests.rs"]
mod tests;
