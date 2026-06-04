use crate::construction::TypeDatabase;

#[cfg(test)]
use crate::types::*;

use crate::types::{InferencePriority, TemplateSpan, TypeData, TypeId};

use crate::visitor::{array_element_union_widens_literals, is_literal_type};

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
    /// resolution uses intersection instead of union, matching tsc behavior.
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
#[allow(dead_code)] // Variants/fields reserved for full inference error reporting
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
#[allow(dead_code)] // Methods reserved for constraint-based inference resolution
pub(crate) struct ConstraintSet {
    /// Lower bounds: types that must be subtypes of this variable
    /// e.g., from argument types being assigned to a parameter
    pub(crate) lower_bounds: Vec<TypeId>,
    /// Upper bounds: types that this variable must be a subtype of
    /// e.g., from `extends` constraints on type parameters
    pub(crate) upper_bounds: Vec<TypeId>,
}

#[allow(dead_code)] // Methods reserved for constraint-based inference resolution
impl ConstraintSet {
    pub const fn new() -> Self {
        Self {
            lower_bounds: Vec::new(),
            upper_bounds: Vec::new(),
        }
    }

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

    /// Add a lower bound constraint: L <: α
    pub fn add_lower_bound(&mut self, ty: TypeId) {
        if !self.lower_bounds.contains(&ty) {
            self.lower_bounds.push(ty);
        }
    }

    /// Add an upper bound constraint: α <: U
    pub fn add_upper_bound(&mut self, ty: TypeId) {
        if !self.upper_bounds.contains(&ty) {
            self.upper_bounds.push(ty);
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
#[allow(dead_code)] // Used by conditional type inference (not yet wired up)
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
    pub(crate) const fn estimated_size_bytes(self) -> usize {
        self.estimated_size_bytes
    }
}

/// Type inference context for a single function call or expression.
pub(crate) struct InferenceContext<'a> {
    pub(crate) interner: &'a dyn TypeDatabase,
    /// Type resolver for semantic lookups (e.g., base class queries)
    pub(crate) resolver: Option<&'a dyn crate::relations::subtype::TypeResolver>,
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
    /// Visited (source, target) pairs during structural inference.
    /// Prevents re-visiting the same pair, breaking cycles in
    /// self-referential type hierarchies.
    pub(crate) infer_visited: FxHashSet<(TypeId, TypeId)>,
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
}
