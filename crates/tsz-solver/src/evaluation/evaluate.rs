pub(in crate::evaluation) mod application_types;

mod array_methods;

use crate::caches::db::QueryDatabase;

use crate::construction::TypeDatabase;

use crate::def::{DefId, DefKind};

use crate::diagnostics::display_provenance::{
    self, AliasApplicationPriority, AliasApplicationProvenance,
    FreshObjectLiteralDisplayProvenance, UnionOriginProvenance,
};

use crate::evaluation::request::EvaluationRequest;

use crate::evaluation::result::EvaluationResult;

#[cfg(test)]
#[allow(unused_imports)]
use crate::instantiation::instantiate::instantiate_generic;

use crate::relations::subtype::{NoopResolver, TypeResolver};

#[cfg(test)]
use crate::types::*;

use crate::types::{
    ConditionalType, ConditionalTypeId, MappedType, MappedTypeId, StringIntrinsicKind,
    TemplateLiteralId, TemplateSpan, TupleElement, TupleListId, TypeApplicationId, TypeData,
    TypeId, TypeListId, TypeParamInfo,
};

use crate::visitors::visitor_predicates::contains_type_matching;

use application_types::{ApplicationEvalContext, ApplicationEvalOutcome, HomomorphicMappedArg};

pub(crate) use array_methods::{
    ARRAY_METHODS_RETURN_ANY, ARRAY_METHODS_RETURN_BOOLEAN, ARRAY_METHODS_RETURN_NUMBER,
    ARRAY_METHODS_RETURN_STRING, ARRAY_METHODS_RETURN_VOID,
};

use rustc_hash::{FxHashMap, FxHashSet};

use tsz_common::interner::Atom;

mod closed_eval;

mod support;

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
    /// Recursion guard for mapped-key constraint simplification.
    pub(super) keyof_constraint_guard: crate::recursion::RecursionGuard<TypeId>,
    /// Per-DefId recursion depth counter.
    /// Allows recursive type aliases (like `TrimRight`) to expand up to `MAX_DEF_DEPTH`
    /// times before stopping, matching tsc's TS2589 "Type instantiation is excessively
    /// deep and possibly infinite" behavior. Unlike a set-based cycle detector, this
    /// permits legitimate bounded recursion where each expansion converges.
    def_depth: FxHashMap<DefId, u32>,
    /// Number of currently active `DefId` expansions at or above the threshold
    /// that turns a structural recursion bailout into a real TS2589 failure.
    real_instantiation_depth_count: u32,
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
    /// PERF: Cache whether a type contains `infer`.
    /// Recursive conditionals can revisit the same application-shaped `extends`
    /// pattern thousands of times while checking whether the application-level
    /// infer fast path applies.
    contains_infer_cache: FxHashMap<TypeId, bool>,
    /// Ceiling for eager mapped-key expansion before bailing out.
    max_mapped_keys: usize,
    /// When true, flag `depth_exceeded` on Application cycle detection.
    /// Used for TS2589 detection at type alias definition sites where
    /// self-referential conditional types produce the same Application TypeId
    /// on each expansion, preventing the per-DefId depth counter from working.
    flag_depth_on_app_cycle: bool,
    /// When true, display aliases for evaluated applications preserve expanded
    /// argument types. Declaration emit opts into this to print reusable public
    /// surfaces without changing checker diagnostic display behavior.
    expand_application_display_alias_args: bool,
    /// Set by `evaluate_conditional` when a conditional branch resolved to an
    /// Application type (via tail-call expansion or direct evaluation).
    /// `evaluate_application` reads this to store a forward display alias
    /// so the formatter shows the intermediate alias name (e.g.
    /// `DeepReadonlyObject<Part>`) rather than the outer alias (`DeepReadonly<Part>`).
    pub(super) apparent_conditional_branch: Option<TypeId>,
    /// Tracks whether ANY structural depth bailout was silently converted to an
    /// opaque (identity) result during this evaluator's lifetime. Distinct from
    /// `guard.exceeded` (cleared as part of the silent-bail policy) and from
    /// `flag_depth_on_app_cycle`. Callers that run a follow-up pass with a more
    /// powerful resolver use this to skip the retry when the original bail was
    /// structural — a more powerful resolver does not change the structural cost
    /// of recursive type-tree walks like `ts-toolbelt`'s `ComputeDeep` /
    /// `Invert` mapped+conditional bodies. See `is_silent_depth_bailed`.
    silent_depth_bailed: bool,
    /// Per-`DefId` `(max_argument_weight, new_maxima_count)` used by the TS2589
    /// detection pass to recognize a divergent (unconditionally growing)
    /// recursive alias. See `recursive_growth::detect_recursive_growth`.
    pub(super) detection_growth_runs: FxHashMap<DefId, (u64, u32)>,
    /// Sticky flag: set when this run hit any recursion limit (a `DefId` reached
    /// `MAX_DEF_DEPTH`, a structural cycle/depth/iteration bail). Such depth-
    /// bounded runs must not be persisted in `closed_eval_cache`, or a later read
    /// could short-circuit the expansion that re-derives `TS2589`. See the
    /// `closed_eval` module.
    deep_recursion_seen: bool,
    /// Whether this evaluator may *write* the `closed_eval_cache`. Only the
    /// checker's authoritative, context-free type-resolution pass opts in (via
    /// `with_closed_eval_writes`). Evaluators running mid-relation, mid-inference
    /// (`infer` binding), mid-narrowing, or contextual typing must NOT write —
    /// their results can depend on inference/narrowing/contextual state the
    /// `(TypeId, no_unchecked)` key does not capture. All evaluators may still
    /// *read* (the stored value is a definite context-free answer).
    closed_eval_writes_allowed: bool,
}

/// Operation-local memo table statistics for [`TypeEvaluator`].
///
/// Owner: one evaluator request. The caches are dropped with the evaluator and
/// are never shared across resolver, substitution, or compiler-option modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TypeEvaluatorCacheStatistics {
    /// Entries in the conditional subtype memo keyed by `(check_type, extends_type)`.
    pub conditional_subtype_entries: usize,
    /// Entries in the `contains infer` predicate memo keyed by `TypeId`.
    pub contains_infer_entries: usize,
    estimated_size_bytes: usize,
}

impl TypeEvaluatorCacheStatistics {
    /// Estimated heap bytes owned by the evaluator memo tables.
    #[must_use]
    pub const fn estimated_size_bytes(self) -> usize {
        self.estimated_size_bytes
    }
}

#[cfg(target_arch = "wasm32")]
const DEFAULT_MAX_MAPPED_KEYS: usize = 250;

#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_MAX_MAPPED_KEYS: usize = 500;

impl<'a> TypeEvaluator<'a, NoopResolver> {
    /// Create a new evaluator without a resolver.
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        static NOOP: NoopResolver = NoopResolver;
        Self::with_resolver_and_defaults(interner, &NOOP)
    }
}

include!("evaluate_parts/part1.rs");
include!("evaluate_parts/part2.rs");

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
    evaluate_type_with_request(interner, EvaluationRequest::new(type_id))
}

/// Convenience function for full type evaluation with an explicit resolver.
pub fn evaluate_type_with_resolver(
    interner: &dyn TypeDatabase,
    resolver: &impl TypeResolver,
    type_id: TypeId,
) -> TypeId {
    let mut evaluator = TypeEvaluator::with_resolver(interner, resolver);
    evaluator.evaluate(type_id)
}

/// Convenience function for full type evaluation with explicit request options.
pub fn evaluate_type_with_request(
    interner: &dyn TypeDatabase,
    request: EvaluationRequest,
) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_request_result(request).into_type_id()
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

#[cfg(test)]
#[path = "../../tests/evaluate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/evaluate_application_orchestrator_tests.rs"]
mod orchestrator_tests;
