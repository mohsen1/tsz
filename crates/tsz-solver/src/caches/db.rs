//! Type database abstraction for the solver.
//!
//! This trait isolates solver logic from concrete storage so we can
//! swap in a query system (e.g., Salsa) without touching core logic.

use crate::caches::instantiation_cache::InstantiationCacheKey;
use crate::caches::subtype_reduction_cache::SubtypeReductionKey;
use crate::def::DefId;
use crate::def::DefinitionStore;
use crate::intern::type_factory::TypeFactory;
use crate::intern::{PredicateCacheKind, TypeInterner};
use crate::narrowing;
use crate::objects::element_access::{ElementAccessEvaluator, ElementAccessResult};
use crate::objects::{CollectPropertiesResultCache, ObjectLiteralBuilder};
use crate::relations::relation_queries::{
    RelationContext, RelationKind, RelationPolicy, query_relation,
};
use crate::relations::subtype::TypeResolver;
use crate::types::{
    CallableShape, CallableShapeId, ConditionalType, ConditionalTypeId, FunctionShape,
    FunctionShapeId, IndexInfo, IntrinsicKind, MappedType, MappedTypeId, ObjectFlags, ObjectShape,
    ObjectShapeId, PropertyInfo, PropertyLookup, RelationCacheKey, StringIntrinsicKind, SymbolRef,
    TemplateLiteralId, TemplateSpan, TupleElement, TupleListId, TypeApplication, TypeApplicationId,
    TypeData, TypeId, TypeListId, TypeParamInfo, Variance,
};
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

pub use crate::caches::display_provenance::{TypeDisplayProvenance, UnionComplexityCheckpoint};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntersectionMergeCacheEntry {
    Merged(TypeId),
    NotEligible,
}

impl IntersectionMergeCacheEntry {
    #[inline]
    pub const fn from_result(result: Option<TypeId>) -> Self {
        match result {
            Some(type_id) => Self::Merged(type_id),
            None => Self::NotEligible,
        }
    }

    #[inline]
    pub const fn into_result(self) -> Option<TypeId> {
        match self {
            Self::Merged(type_id) => Some(type_id),
            Self::NotEligible => None,
        }
    }
}

/// Read-only access to interned type storage.
///
/// This is the narrow capability for helpers that only inspect existing
/// type data and do not need construction, provenance, cache, or policy hooks.
pub trait TypeStore {
    fn lookup(&self, id: TypeId) -> Option<TypeData>;
    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]>;
}

impl<T: TypeDatabase + ?Sized> TypeStore for T {
    fn lookup(&self, id: TypeId) -> Option<TypeData> {
        TypeDatabase::lookup(self, id)
    }

    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]> {
        TypeDatabase::type_list(self, id)
    }
}

pub use super::db_base_traits::{
    IntersectionDisplayReduction, JsSignatureDisplaySource, TypeCompilerOptions,
    TypePredicateCache, TypeRawIntersectionConstruction, TypeTupleLimitSignal,
};

/// Per-file cache hooks for evaluated generic applications.
///
/// The application-eval cache is keyed by `(DefId, args,
/// no_unchecked_indexed_access, exact_optional_property_types)`. Use it only
/// from authoritative full-resolver contexts; limited/noop resolvers can skip
/// fallback behavior that is part of recursive and inference parity. Keeping
/// this separate from [`TypeDatabase`] avoids growing the storage interface.
pub trait TypeApplicationEvalCache {
    /// Project-wide (#14345) instantiation result lookup. Default `None`.
    fn lookup_proto_instantiation_cache(
        &self,
        _key: &crate::caches::instantiation_cache::InstantiationCacheKey,
    ) -> Option<TypeId> {
        None
    }

    /// Project-wide (#14345) instantiation result store. Default no-op.
    fn insert_proto_instantiation_cache(
        &self,
        _key: crate::caches::instantiation_cache::InstantiationCacheKey,
        _result: TypeId,
    ) {
    }

    /// Look up a shared cache entry for evaluated generic applications.
    ///
    /// The default returns `None` so raw `TypeInterner` backends and tests opt
    /// out.
    fn lookup_application_eval_cache(
        &self,
        _def_id: DefId,
        _args: &[TypeId],
        _no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        None
    }

    /// Store an evaluated generic application result in the shared per-file
    /// cache. The default is a no-op so non-cache backends opt out.
    fn insert_application_eval_cache(
        &self,
        _def_id: DefId,
        _args: &[TypeId],
        _no_unchecked_indexed_access: bool,
        _result: TypeId,
    ) {
    }

    /// Drop every cached eval-family entry that depends on `def_id`.
    ///
    /// This includes application-eval entries keyed by the def directly or by
    /// lazy refs in their args/results, ordinary eval-memo entries whose key or
    /// result closure mentions the def, and closed-eval entries with the same
    /// dependency. The concrete `QueryCache` implementation also invalidates
    /// shared eval-family entries when a shared cache is attached.
    ///
    /// Called when a definition body is (re)registered with different
    /// content: results computed under the previous body (or before any body
    /// existed) are stale and must be recomputed on the next query. The
    /// default is a no-op so raw `TypeInterner` backends and tests opt out.
    fn invalidate_application_eval_cache_for_def(&self, _def_id: DefId) {}

    /// Look up a persisted evaluation memo entry for `type_id`.
    ///
    /// Backed by the same per-file (plus shared cross-file) eval cache that
    /// `evaluate_type_with_options` consults at its top-level boundary; this
    /// hook lets a *plain* evaluator (`NoopResolver`, default mode flags)
    /// read those entries at nested nodes too, instead of re-walking
    /// subtrees an earlier evaluator in the same file scope already
    /// evaluated (issue #13097). Every stored entry is a clean
    /// (limit-untainted) result keyed by
    /// `(TypeId, no_unchecked_indexed_access, exact_optional_property_types)`
    /// and written only from plain evaluators, so serving it at a nested
    /// node is the same semantic operation the top-level boundary already
    /// performs. Default returns `None` so raw `TypeInterner` backends and
    /// tests opt out.
    fn lookup_eval_memo(
        &self,
        _type_id: TypeId,
        _no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        None
    }

    /// Store a clean evaluation result in the persistent eval memo.
    ///
    /// Write-through counterpart of [`Self::lookup_eval_memo`], called from
    /// a plain evaluator's memo insert when the entry's evaluation window
    /// saw no limit event (the per-entry taint discrimination of issue
    /// #13241) and no union-complexity overflow. First write wins, matching
    /// the boundary drain's `or_insert`. Default is a no-op.
    fn insert_eval_memo(
        &self,
        _type_id: TypeId,
        _no_unchecked_indexed_access: bool,
        _result: TypeId,
    ) {
    }

    /// Look up a cached evaluation result for a *closed* type (one with no free
    /// type parameters, `this`, `infer`, or type-query operands).
    ///
    /// Evaluating a closed type is resolver-independent and deterministic per
    /// interner, so the result can be memoized project-wide and reused across
    /// the many fresh `TypeEvaluator` instances created during instantiation
    /// (`evaluate_index_access`, `evaluate_keyof`, …). The key is keyed by the
    /// `TypeId` plus both option flags, which can change `T[K]` results.
    /// Default returns `None` so non-cache backends opt out.
    fn lookup_closed_eval_cache(
        &self,
        _type_id: TypeId,
        _no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        None
    }

    /// Store an evaluation result for a closed type. Default is a no-op.
    fn insert_closed_eval_cache(
        &self,
        _type_id: TypeId,
        _no_unchecked_indexed_access: bool,
        _result: TypeId,
    ) {
    }

    /// Look up a cached *conditional-branch* subtype verdict — whether
    /// `check <: extends` for the purpose of selecting a conditional type's
    /// branch (issues #8356 / #13097).
    ///
    /// This is a distinct relation from plain subtyping: the conditional
    /// branch probe applies tsc's conditional-only fast paths (the global
    /// `Function` intrinsic satisfying a callable target; primitives never
    /// extending `Function`), so its verdict must never be served from — or
    /// stored into — the ordinary subtype relation cache. Only *definitive*
    /// verdicts (`Holds`/`Fails`) that consumed no unregistered `Lazy` body,
    /// took no depth bail, and tripped no recursion/iteration limit are
    /// persisted, so a stored verdict is stable for `(check, extends,
    /// no_unchecked_indexed_access, exact_optional_property_types)` across the
    /// fresh `TypeEvaluator` instances instantiation spins up. Default returns
    /// `None` so non-cache backends opt out.
    fn lookup_conditional_branch_verdict(
        &self,
        _check: TypeId,
        _extends: TypeId,
        _no_unchecked_indexed_access: bool,
        _exact_optional_property_types: bool,
    ) -> Option<bool> {
        None
    }

    /// Store a definitive conditional-branch subtype verdict. Default is a
    /// no-op. See [`Self::lookup_conditional_branch_verdict`] for the stability
    /// gates the caller must enforce before publishing.
    fn insert_conditional_branch_verdict(
        &self,
        _check: TypeId,
        _extends: TypeId,
        _no_unchecked_indexed_access: bool,
        _exact_optional_property_types: bool,
        _verdict: bool,
    ) {
    }

    /// Look up a cached result for tsc's permissive-instantiation
    /// false-branch gate (`getConditionalType`).
    ///
    /// The key is the original `(check, extends)` pair plus both compiler
    /// option bits. The caller must publish only when the helper's instantiated
    /// permissive relation was certified by the conditional-branch verdict cache
    /// and the surrounding evaluation request stayed stable. Default returns
    /// `None` so raw-interner backends opt out.
    fn lookup_permissive_false_branch_verdict(
        &self,
        _check: TypeId,
        _extends: TypeId,
        _no_unchecked_indexed_access: bool,
        _exact_optional_property_types: bool,
    ) -> Option<bool> {
        None
    }

    /// Store a cached result for tsc's permissive-instantiation false-branch
    /// gate. Default is a no-op. See
    /// [`Self::lookup_permissive_false_branch_verdict`] for the required
    /// publication gates.
    fn insert_permissive_false_branch_verdict(
        &self,
        _check: TypeId,
        _extends: TypeId,
        _no_unchecked_indexed_access: bool,
        _exact_optional_property_types: bool,
        _verdict: bool,
    ) {
    }
}

/// Cache for the canonical `widen_type` result keyed by `TypeId`.
///
/// Kept separate from [`TypeDatabase`] so the broad query trait stays under
/// its method cap (#8205); `TypeDatabase` re-exposes these via the supertrait
/// bound, so `&dyn TypeDatabase` callers are unaffected.
pub trait TypeWidenCache {
    /// Look up the memoized canonical `widen_type` result for `type_id`.
    /// Default `None` (no caching). Only the canonical semantic `widen_type`
    /// entry populates this; see `TypeInterner::widen_type_cache`.
    fn widen_type_memo(&self, _type_id: TypeId) -> Option<TypeId> {
        None
    }

    /// Record the canonical `widen_type` result for `type_id`. Default no-op.
    fn set_widen_type_memo(&self, _type_id: TypeId, _result: TypeId) {}
}

/// Construction hook for conditional-flow substitution wrapper types.
///
/// Kept separate from [`TypeDatabase`] so the broad storage/query trait stays
/// within its method cap (#8205). `TypeDatabase` inherits this capability, so
/// existing `&dyn TypeDatabase` callers can still construct substitution
/// wrappers without depending on concrete interners.
pub trait TypeSubstitutionConstruction {
    fn substitution(&self, base_type: TypeId, constraint: TypeId) -> TypeId;
}

/// Cache for the `extract_type_params_from_type` result keyed by `TypeId`.
///
/// The reachable type-parameter set of a type is a pure function of the
/// immutable interned structure, so it can be memoized project-wide and reused
/// across the many fresh evaluators created during instantiation. Kept separate
/// from [`TypeDatabase`] so the broad query trait stays under its method cap
/// (#8205); `TypeDatabase` re-exposes these via the supertrait bound, so
/// `&dyn TypeDatabase` callers are unaffected.
pub trait TypeExtractParamsCache {
    /// Look up the memoized `extract_type_params_from_type` result for
    /// `type_id`. Default `None` (no caching).
    fn extract_type_params_memo(&self, _type_id: TypeId) -> Option<Arc<[TypeParamInfo]>> {
        None
    }

    /// Record the `extract_type_params_from_type` result for `type_id`.
    /// Default no-op.
    fn set_extract_type_params_memo(&self, _type_id: TypeId, _params: Arc<[TypeParamInfo]>) {}

    /// Look up the memoized `collect_contravariant_infer_names` name list for a
    /// pattern `type_id`. Default `None` (no caching).
    fn contravariant_infer_names_memo(&self, _type_id: TypeId) -> Option<Arc<[Atom]>> {
        None
    }

    /// Record the `collect_contravariant_infer_names` name list for a pattern
    /// `type_id`. Default no-op.
    fn set_contravariant_infer_names_memo(&self, _type_id: TypeId, _names: Arc<[Atom]>) {}
}

/// Cache for the pure structural `contains_type_by_id(root, target)` transitive
/// containment walk, keyed by the `(root, target)` pair.
///
/// Transitive `TypeId` containment is a pure function of the immutable interned
/// type `DAG`, so it can be memoized project-wide and reused across the many fresh
/// evaluators created during instantiation. Kept separate from [`TypeDatabase`]
/// so the broad query trait stays under its method cap (#8205); `TypeDatabase`
/// re-exposes these via the supertrait bound, so `&dyn TypeDatabase` callers are
/// unaffected.
pub trait TypeContainsByIdCache {
    /// Look up the memoized `contains_type_by_id(root, target)` result.
    /// Default `None` (no caching).
    fn contains_type_by_id_memo(&self, _root: TypeId, _target: TypeId) -> Option<bool> {
        None
    }

    /// Record the `contains_type_by_id(root, target)` result. Default no-op.
    fn set_contains_type_by_id_memo(&self, _root: TypeId, _target: TypeId, _result: bool) {}
}

/// Cache for the pure structural `prune_impossible_object_union_members(type_id)`
/// result, keyed by the union `TypeId`.
///
/// Pruning a union's impossible object members consults only structural
/// predicates over the immutable interned type `DAG` (literal-discriminant
/// conflicts, never-typed or impossible-unit required properties) plus
/// resolver-free `evaluate_type` / `is_subtype_of` walks; it threads no resolver,
/// substitution environment, or compiler option, so the pruned result is a pure
/// function of the input union `TypeId` and stable within one interner. Kept
/// separate from [`TypeDatabase`] so the broad query trait stays under its method
/// cap (#8205); `TypeDatabase` re-exposes these via the supertrait bound, so
/// `&dyn TypeDatabase` callers are unaffected.
pub trait TypePruneUnionCache {
    /// Look up the memoized `prune_impossible_object_union_members(type_id)`
    /// result. Default `None` (no caching).
    fn prune_union_members_memo(&self, _type_id: TypeId) -> Option<TypeId> {
        None
    }

    /// Record the `prune_impossible_object_union_members(type_id)` result.
    /// Default no-op.
    fn set_prune_union_members_memo(&self, _type_id: TypeId, _result: TypeId) {}
}

/// Registered lib.d.ts builtin type access (`Array<T>`, `ReadonlyArray<T>`,
/// boxed primitive interfaces).
///
/// Split out of [`TypeDatabase`] (#8205) so solver paths that only need the
/// builtin registry can take this narrow capability, and the broad storage
/// trait stays under its method-count ratchet. Defaults return "not
/// registered"; the interner and query cache override with live lookups.
pub trait TypeBuiltinAccess {
    /// Get the canonical `Array<T>` base type registered from lib.d.ts.
    ///
    /// This is used by solver-only paths that need array member metadata
    /// (for example mapped-type display ordering) even when no richer
    /// `TypeResolver` is available.
    fn get_array_base_type(&self) -> Option<TypeId> {
        None
    }

    /// Get the registered `Array<T>` base type parameters.
    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        &[]
    }

    /// Get the `Array<T>` base type used for display-order-sensitive queries.
    fn get_array_display_base_type(&self) -> Option<TypeId> {
        None
    }

    /// Get the `ReadonlyArray<T>` base type registered from lib.d.ts.
    ///
    /// Used by property access to resolve only non-mutating members when the
    /// receiver is `readonly T[]` or a readonly tuple.
    fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        None
    }

    /// Get the boxed interface type for a primitive intrinsic kind.
    ///
    /// For example, `IntrinsicKind::Function` returns the `TypeId` of the `Function` interface
    /// from lib.d.ts. This bypasses `TypeResolver` (which may fail due to `RefCell` borrow
    /// conflicts) by reading directly from the interner's `DashMap`.
    fn get_boxed_type(&self, _kind: IntrinsicKind) -> Option<TypeId> {
        None
    }

    /// Check if a `DefId` corresponds to a boxed type of the given kind.
    ///
    /// For example, checking if a `DefId` represents the `Function` interface.
    /// This bypasses `TypeResolver` by reading directly from the interner's storage.
    fn is_boxed_def_id(&self, _def_id: DefId, _kind: IntrinsicKind) -> bool {
        false
    }
}

/// Query interface for the solver.
///
/// This keeps solver components generic and prevents them from reaching
/// into concrete storage structures directly.
pub trait TypeDatabase:
    JsSignatureDisplaySource
    + TypeBuiltinAccess
    + TypePredicateCache
    + TypeTupleLimitSignal
    + TypeDisplayProvenance
    + TypeCompilerOptions
    + TypeApplicationEvalCache
    + TypeWidenCache
    + TypeSubstitutionConstruction
    + TypeExtractParamsCache
    + TypeContainsByIdCache
    + TypePruneUnionCache
{
    /// Process-local identity for this `TypeDatabase` owner.
    ///
    /// Fresh-evaluator session memos use this as a discriminator because
    /// `TypeId` values are arena-local: the same numeric id can name a
    /// different shape in a sibling checker arena.
    fn type_database_identity(&self) -> usize {
        std::ptr::from_ref(self).cast::<()>() as usize
    }

    fn intern(&self, key: TypeData) -> TypeId;
    fn lookup(&self, id: TypeId) -> Option<TypeData>;
    fn lookup_alloc_order(&self, _id: TypeId) -> Option<u32> {
        None
    }
    fn intern_string(&self, s: &str) -> Atom;
    fn resolve_atom(&self, atom: Atom) -> String;
    fn resolve_atom_ref(&self, atom: Atom) -> Arc<str>;
    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]>;
    fn tuple_list(&self, id: TupleListId) -> Arc<[TupleElement]>;
    fn template_list(&self, id: TemplateLiteralId) -> Arc<[TemplateSpan]>;
    fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape>;
    fn object_property_index(&self, shape_id: ObjectShapeId, name: Atom) -> PropertyLookup;
    fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape>;
    fn callable_shape(&self, id: CallableShapeId) -> Arc<CallableShape>;
    fn conditional_type(&self, id: ConditionalTypeId) -> Arc<ConditionalType>;
    fn mapped_type(&self, id: MappedTypeId) -> Arc<MappedType>;

    /// Get conditional type by value (Copy, no Arc overhead).
    fn get_conditional(&self, id: ConditionalTypeId) -> ConditionalType {
        *self.conditional_type(id)
    }
    /// Get mapped type by value (Copy, no Arc overhead).
    fn get_mapped(&self, id: MappedTypeId) -> MappedType {
        *self.mapped_type(id)
    }
    fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication>;

    /// Read a universe-shared variance mask for a generic definition.
    ///
    /// Backed by the `TypeInterner`'s `def_variance_masks` store. The value is
    /// `(mask, gap_defs)`: the mask is canonical (computed with no in-flight
    /// def dependency) and `gap_defs` is its resolution-failure fingerprint —
    /// the defs whose lazy resolution failed during the walk. The mask is a
    /// pure function of (def structure, failure set); consumers must validate
    /// that every fingerprint def still fails to resolve under their resolver
    /// before replaying the mask. Databases that do not wrap an
    /// interner-backed store return `None` (the variance computer then simply
    /// recomputes).
    fn shared_def_variance(
        &self,
        _def_id: crate::def::DefId,
    ) -> Option<crate::intern::SharedDefVariance> {
        None
    }

    /// Store a universe-shared variance mask for a generic definition with
    /// its resolution-failure fingerprint.
    ///
    /// Callers must only insert canonical masks whose every resolution gap is
    /// listed in `gaps` (see [`Self::shared_def_variance`]).
    /// Default is a no-op.
    fn insert_shared_def_variance(
        &self,
        _def_id: crate::def::DefId,
        _mask: Arc<[Variance]>,
        _gaps: Arc<[crate::def::DefId]>,
    ) {
    }

    fn literal_string(&self, value: &str) -> TypeId;
    fn literal_number(&self, value: f64) -> TypeId;
    fn literal_boolean(&self, value: bool) -> TypeId;
    fn literal_bigint(&self, value: &str) -> TypeId;
    fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId;

    fn union(&self, members: Vec<TypeId>) -> TypeId;
    /// Create a union from a borrowed slice, avoiding allocation when callers
    /// already have an `Arc<[TypeId]>` or `&[TypeId]`.
    fn union_from_slice(&self, members: &[TypeId]) -> TypeId;
    /// Create a union with literal-only reduction (no subtype reduction).
    /// Matches tsc's `UnionReduction.Literal` behavior for type annotations.
    fn union_literal_reduce(&self, members: Vec<TypeId>) -> TypeId;
    fn union_from_sorted_vec(&self, flat: Vec<TypeId>) -> TypeId;
    fn union2(&self, left: TypeId, right: TypeId) -> TypeId;
    fn union3(&self, first: TypeId, second: TypeId, third: TypeId) -> TypeId;
    fn intersection(&self, members: Vec<TypeId>) -> TypeId;
    fn intersection2(&self, left: TypeId, right: TypeId) -> TypeId;
    /// Raw intersection without normalization (used to avoid infinite recursion)
    fn intersect_types_raw2(&self, left: TypeId, right: TypeId) -> TypeId;
    fn array(&self, element: TypeId) -> TypeId;
    fn tuple(&self, elements: Vec<TupleElement>) -> TypeId;
    fn object(&self, properties: Vec<PropertyInfo>) -> TypeId;
    fn object_with_flags(&self, properties: Vec<PropertyInfo>, flags: ObjectFlags) -> TypeId;
    fn object_with_flags_and_symbol(
        &self,
        properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
        symbol: Option<SymbolId>,
    ) -> TypeId;
    fn object_fresh(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.object_with_flags(properties, ObjectFlags::FRESH_LITERAL)
    }
    /// Get the TypeId for an already-interned Object shape (O(1) cache hit).
    fn object_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId;
    /// Get the TypeId for an already-interned `ObjectWithIndex` shape.
    fn object_with_index_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId;
    /// Create a fresh object type with both widened properties (for type checking)
    /// and display properties (for error messages, implementing tsc's freshness model).
    fn object_fresh_with_display(
        &self,
        widened_properties: Vec<PropertyInfo>,
        display_properties: Vec<PropertyInfo>,
    ) -> TypeId {
        // Default: just create a fresh object (implementations can store display props)
        let _ = display_properties;
        self.object_fresh(widened_properties)
    }
    fn object_with_index(&self, shape: ObjectShape) -> TypeId;
    fn function(&self, shape: FunctionShape) -> TypeId;
    fn callable(&self, shape: CallableShape) -> TypeId;
    fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId;
    fn conditional(&self, conditional: ConditionalType) -> TypeId;
    fn mapped(&self, mapped: MappedType) -> TypeId;
    fn reference(&self, symbol: SymbolRef) -> TypeId;
    fn lazy(&self, def_id: DefId) -> TypeId;
    fn bound_parameter(&self, index: u32) -> TypeId;
    fn recursive(&self, depth: u32) -> TypeId;
    fn type_param(&self, info: TypeParamInfo) -> TypeId;
    fn unresolved_type_name(&self, name: Atom) -> TypeId;
    fn type_query(&self, symbol: SymbolRef) -> TypeId;
    fn enum_type(&self, def_id: DefId, structural_type: TypeId) -> TypeId;
    fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId;

    fn literal_string_atom(&self, atom: Atom) -> TypeId;
    fn union_preserve_members(&self, members: Vec<TypeId>) -> TypeId;
    fn readonly_type(&self, inner: TypeId) -> TypeId;
    fn keyof(&self, inner: TypeId) -> TypeId;
    fn index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId;
    fn this_type(&self) -> TypeId;
    fn no_infer(&self, inner: TypeId) -> TypeId;
    fn unique_symbol(&self, symbol: SymbolRef) -> TypeId;
    fn infer(&self, info: TypeParamInfo) -> TypeId;
    fn string_intrinsic(&self, kind: StringIntrinsicKind, type_arg: TypeId) -> TypeId;

    /// Get the base class type for a symbol (class/interface).
    /// Returns the `TypeId` of the extends clause, or None if the symbol doesn't extend anything.
    /// This is used by the BCT algorithm to find common base classes.
    fn get_class_base_type(&self, symbol_id: SymbolId) -> Option<TypeId>;

    /// Check if a type can be compared by `TypeId` identity alone (O(1) equality).
    /// Identity-comparable types include literals, enum members, unique symbols, null, undefined,
    /// void, never, and tuples composed entirely of identity-comparable types.
    /// Results are cached for O(1) lookup after first computation.
    fn is_identity_comparable_type(&self, type_id: TypeId) -> bool;

    /// Check if a `DefId` corresponds to the `ThisType` marker interface.
    fn is_this_type_marker_def_id(&self, _def_id: DefId) -> bool {
        false
    }

    /// Increment the global evaluation fuel counter and return whether fuel is exhausted.
    ///
    /// This counter tracks cumulative evaluation work across ALL `TypeEvaluator` instances.
    /// Unlike per-evaluator iteration limits, this enforces a system-wide budget that prevents
    /// deeply recursive type libraries (like ts-toolbelt) from consuming unbounded memory
    /// through type instantiation.
    ///
    /// Returns `true` if fuel is exhausted (evaluation should stop).
    ///
    /// Mirrors TypeScript's global `instantiationCount` which limits total type
    /// instantiation work across the entire program check.
    fn consume_evaluation_fuel(&self, _amount: u32) -> bool {
        false
    }

    /// Check whether global evaluation fuel is exhausted without consuming any.
    fn is_evaluation_fuel_exhausted(&self) -> bool {
        false
    }

    /// Reset the global evaluation fuel counter at a top-level check
    /// boundary (per file-check session). Mirrors `tsc` resetting
    /// `instantiationCount` per checked source element: the budget bounds
    /// per-check runaway, not cumulative whole-program work. Default no-op
    /// for databases without a fuel counter.
    fn reset_evaluation_fuel(&self) {}
}

impl TypePredicateCache for TypeInterner {
    fn contains_this_type_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsThis)
    }

    fn set_contains_this_type_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(type_id, PredicateCacheKind::ContainsThis, result);
    }

    fn contains_infer_types_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsInfer)
    }

    fn set_contains_infer_types_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(type_id, PredicateCacheKind::ContainsInfer, result);
    }

    fn contains_type_query_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsTypeQuery)
    }

    fn set_contains_type_query_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(type_id, PredicateCacheKind::ContainsTypeQuery, result);
    }

    fn contains_type_query_full_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsTypeQueryFull)
    }

    fn set_contains_type_query_full_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(type_id, PredicateCacheKind::ContainsTypeQueryFull, result);
    }

    fn contains_never_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsNever)
    }

    fn set_contains_never_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(type_id, PredicateCacheKind::ContainsNever, result);
    }

    fn contains_error_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsError)
    }

    fn set_contains_error_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(type_id, PredicateCacheKind::ContainsError, result);
    }

    fn contains_type_params_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsTypeParams)
    }

    fn set_contains_type_params_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(type_id, PredicateCacheKind::ContainsTypeParams, result);
    }

    fn contains_lazy_or_recursive_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsLazyOrRecursive)
    }

    fn set_contains_lazy_or_recursive_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(type_id, PredicateCacheKind::ContainsLazyOrRecursive, result);
    }

    fn contains_unresolved_application_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsUnresolvedApplication)
    }

    fn set_contains_unresolved_application_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(
            type_id,
            PredicateCacheKind::ContainsUnresolvedApplication,
            result,
        );
    }

    fn contains_resolver_dependent_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsResolverDependent)
    }

    fn set_contains_resolver_dependent_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(
            type_id,
            PredicateCacheKind::ContainsResolverDependent,
            result,
        );
    }

    fn structurally_eval_inert_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::StructurallyEvalInert)
    }

    fn set_structurally_eval_inert_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(type_id, PredicateCacheKind::StructurallyEvalInert, result);
    }

    fn contains_conditional_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsConditional)
    }

    fn set_contains_conditional_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(type_id, PredicateCacheKind::ContainsConditional, result);
    }

    fn contains_param_or_infer_root_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsParamOrInferRoot)
    }

    fn set_contains_param_or_infer_root_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(
            type_id,
            PredicateCacheKind::ContainsParamOrInferRoot,
            result,
        );
    }

    fn contains_free_type_params_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsFreeTypeParams)
    }

    fn set_contains_free_type_params_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(type_id, PredicateCacheKind::ContainsFreeTypeParams, result);
    }

    fn contains_extractable_type_params_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsExtractableTypeParams)
    }

    fn set_contains_extractable_type_params_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(
            type_id,
            PredicateCacheKind::ContainsExtractableTypeParams,
            result,
        );
    }

    fn contains_free_infer_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsFreeInfer)
    }

    fn set_contains_free_infer_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(type_id, PredicateCacheKind::ContainsFreeInfer, result);
    }

    fn contains_generic_params_root_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsGenericParamsRoot)
    }

    fn set_contains_generic_params_root_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(
            type_id,
            PredicateCacheKind::ContainsGenericParamsRoot,
            result,
        );
    }

    fn is_generic_with_union_constraint_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::IsGenericWithUnionConstraint)
    }

    fn set_is_generic_with_union_constraint_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(
            type_id,
            PredicateCacheKind::IsGenericWithUnionConstraint,
            result,
        );
    }

    fn is_generic_without_nullable_constraint_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(
            type_id,
            PredicateCacheKind::IsGenericWithoutNullableConstraint,
        )
    }

    fn set_is_generic_without_nullable_constraint_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(
            type_id,
            PredicateCacheKind::IsGenericWithoutNullableConstraint,
            result,
        );
    }

    fn eval_contains_infer_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::EvalContainsInfer)
    }

    fn set_eval_contains_infer_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(type_id, PredicateCacheKind::EvalContainsInfer, result);
    }

    fn contains_file_relative_cached(&self, type_id: TypeId) -> Option<bool> {
        self.predicate_cache_get(type_id, PredicateCacheKind::ContainsFileRelative)
    }

    fn set_contains_file_relative_cache(&self, type_id: TypeId, result: bool) {
        self.predicate_cache_set(type_id, PredicateCacheKind::ContainsFileRelative, result);
    }
}

impl TypeTupleLimitSignal for TypeInterner {
    fn take_tuple_too_large(&self) -> bool {
        Self::take_tuple_too_large(self)
    }

    fn mark_tuple_too_large(&self) {
        self.set_tuple_too_large();
    }

    fn is_tuple_too_large(&self) -> bool {
        Self::is_tuple_too_large(self)
    }

    fn is_poisoned(&self) -> bool {
        Self::is_poisoned(self)
    }
}

impl JsSignatureDisplaySource for TypeInterner {
    fn function_with_arity_optional_mask(&self, shape: FunctionShape, mask: &[bool]) -> TypeId {
        Self::function_with_arity_optional_mask(self, shape, mask)
    }

    fn function_shape_arity_optional_mask(&self, id: FunctionShapeId) -> Option<Arc<[bool]>> {
        Self::function_shape_arity_optional_mask(self, id)
    }
}

impl TypeCompilerOptions for TypeInterner {
    fn no_unchecked_indexed_access(&self) -> bool {
        TypeInterner::no_unchecked_indexed_access(self)
    }

    fn exact_optional_property_types(&self) -> bool {
        TypeInterner::exact_optional_property_types(self)
    }

    fn strict_null_checks(&self) -> bool {
        TypeInterner::strict_null_checks(self)
    }
}

impl TypeApplicationEvalCache for TypeInterner {
    // #14345: the interner backs the project-wide instantiation cache.
    fn lookup_proto_instantiation_cache(
        &self,
        key: &crate::caches::instantiation_cache::InstantiationCacheKey,
    ) -> Option<TypeId> {
        self.proto_instantiation_memo(key)
    }

    fn insert_proto_instantiation_cache(
        &self,
        key: crate::caches::instantiation_cache::InstantiationCacheKey,
        result: TypeId,
    ) {
        self.set_proto_instantiation_memo(key, result);
    }
}

impl TypeWidenCache for TypeInterner {
    fn widen_type_memo(&self, type_id: TypeId) -> Option<TypeId> {
        Self::widen_type_memo(self, type_id)
    }

    fn set_widen_type_memo(&self, type_id: TypeId, result: TypeId) {
        Self::set_widen_type_memo(self, type_id, result);
    }
}

impl TypeSubstitutionConstruction for TypeInterner {
    fn substitution(&self, base_type: TypeId, constraint: TypeId) -> TypeId {
        Self::substitution(self, base_type, constraint)
    }
}

impl TypeExtractParamsCache for TypeInterner {
    fn extract_type_params_memo(&self, type_id: TypeId) -> Option<Arc<[TypeParamInfo]>> {
        Self::extract_type_params_memo(self, type_id)
    }

    fn set_extract_type_params_memo(&self, type_id: TypeId, params: Arc<[TypeParamInfo]>) {
        Self::set_extract_type_params_memo(self, type_id, params);
    }

    fn contravariant_infer_names_memo(&self, type_id: TypeId) -> Option<Arc<[Atom]>> {
        Self::contravariant_infer_names_memo(self, type_id)
    }

    fn set_contravariant_infer_names_memo(&self, type_id: TypeId, names: Arc<[Atom]>) {
        Self::set_contravariant_infer_names_memo(self, type_id, names);
    }
}

impl TypeContainsByIdCache for TypeInterner {
    fn contains_type_by_id_memo(&self, root: TypeId, target: TypeId) -> Option<bool> {
        Self::contains_type_by_id_memo(self, root, target)
    }

    fn set_contains_type_by_id_memo(&self, root: TypeId, target: TypeId, result: bool) {
        Self::set_contains_type_by_id_memo(self, root, target, result);
    }
}

impl TypePruneUnionCache for TypeInterner {
    fn prune_union_members_memo(&self, type_id: TypeId) -> Option<TypeId> {
        Self::prune_union_members_memo(self, type_id)
    }

    fn set_prune_union_members_memo(&self, type_id: TypeId, result: TypeId) {
        Self::set_prune_union_members_memo(self, type_id, result);
    }
}

impl TypeBuiltinAccess for TypeInterner {
    fn get_array_base_type(&self) -> Option<TypeId> {
        Self::get_array_base_type(self)
    }

    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        Self::get_array_base_type_params(self)
    }

    fn get_array_display_base_type(&self) -> Option<TypeId> {
        Self::get_array_display_base_type(self)
    }

    fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        TypeInterner::get_readonly_array_base_type(self)
    }

    fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        Self::get_boxed_type(self, kind)
    }

    fn is_boxed_def_id(&self, def_id: DefId, kind: IntrinsicKind) -> bool {
        Self::is_boxed_def_id(self, def_id, kind)
    }
}

impl TypeDatabase for TypeInterner {
    fn intern(&self, key: TypeData) -> TypeId {
        Self::intern(self, key)
    }

    fn lookup(&self, id: TypeId) -> Option<TypeData> {
        Self::lookup(self, id)
    }

    fn lookup_alloc_order(&self, id: TypeId) -> Option<u32> {
        Self::lookup_alloc_order(self, id)
    }

    fn intern_string(&self, s: &str) -> Atom {
        Self::intern_string(self, s)
    }

    fn resolve_atom(&self, atom: Atom) -> String {
        Self::resolve_atom(self, atom)
    }

    fn resolve_atom_ref(&self, atom: Atom) -> Arc<str> {
        Self::resolve_atom_ref(self, atom)
    }

    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]> {
        Self::type_list(self, id)
    }

    fn tuple_list(&self, id: TupleListId) -> Arc<[TupleElement]> {
        Self::tuple_list(self, id)
    }

    fn template_list(&self, id: TemplateLiteralId) -> Arc<[TemplateSpan]> {
        Self::template_list(self, id)
    }

    fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape> {
        Self::object_shape(self, id)
    }

    fn object_property_index(&self, shape_id: ObjectShapeId, name: Atom) -> PropertyLookup {
        Self::object_property_index(self, shape_id, name)
    }

    fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape> {
        Self::function_shape(self, id)
    }

    fn callable_shape(&self, id: CallableShapeId) -> Arc<CallableShape> {
        Self::callable_shape(self, id)
    }

    fn conditional_type(&self, id: ConditionalTypeId) -> Arc<ConditionalType> {
        Self::conditional_type(self, id)
    }

    fn mapped_type(&self, id: MappedTypeId) -> Arc<MappedType> {
        Self::mapped_type(self, id)
    }

    fn get_conditional(&self, id: ConditionalTypeId) -> ConditionalType {
        TypeInterner::get_conditional(self, id)
    }

    fn get_mapped(&self, id: MappedTypeId) -> MappedType {
        TypeInterner::get_mapped(self, id)
    }

    fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication> {
        Self::type_application(self, id)
    }

    fn shared_def_variance(
        &self,
        def_id: crate::def::DefId,
    ) -> Option<crate::intern::SharedDefVariance> {
        Self::shared_def_variance(self, def_id)
    }

    fn insert_shared_def_variance(
        &self,
        def_id: crate::def::DefId,
        mask: Arc<[Variance]>,
        gaps: Arc<[crate::def::DefId]>,
    ) {
        Self::insert_shared_def_variance(self, def_id, mask, gaps);
    }

    fn literal_string(&self, value: &str) -> TypeId {
        Self::literal_string(self, value)
    }

    fn literal_number(&self, value: f64) -> TypeId {
        Self::literal_number(self, value)
    }

    fn literal_boolean(&self, value: bool) -> TypeId {
        Self::literal_boolean(self, value)
    }

    fn literal_bigint(&self, value: &str) -> TypeId {
        Self::literal_bigint(self, value)
    }

    fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId {
        Self::literal_bigint_with_sign(self, negative, digits)
    }

    fn union(&self, members: Vec<TypeId>) -> TypeId {
        Self::union(self, members)
    }

    fn union_from_slice(&self, members: &[TypeId]) -> TypeId {
        Self::union_from_slice(self, members)
    }

    fn union_literal_reduce(&self, members: Vec<TypeId>) -> TypeId {
        Self::union_literal_reduce(self, members)
    }

    fn union_from_sorted_vec(&self, flat: Vec<TypeId>) -> TypeId {
        Self::union_from_sorted_vec(self, flat)
    }

    fn union2(&self, left: TypeId, right: TypeId) -> TypeId {
        Self::union2(self, left, right)
    }

    fn union3(&self, first: TypeId, second: TypeId, third: TypeId) -> TypeId {
        Self::union3(self, first, second, third)
    }

    fn intersection(&self, members: Vec<TypeId>) -> TypeId {
        Self::intersection(self, members)
    }

    fn intersection2(&self, left: TypeId, right: TypeId) -> TypeId {
        Self::intersection2(self, left, right)
    }

    fn intersect_types_raw2(&self, left: TypeId, right: TypeId) -> TypeId {
        Self::intersect_types_raw2(self, left, right)
    }

    fn array(&self, element: TypeId) -> TypeId {
        Self::array(self, element)
    }

    fn tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        Self::tuple(self, elements)
    }

    fn object(&self, properties: Vec<PropertyInfo>) -> TypeId {
        Self::object(self, properties)
    }

    fn object_with_flags(&self, properties: Vec<PropertyInfo>, flags: ObjectFlags) -> TypeId {
        Self::object_with_flags(self, properties, flags)
    }

    fn object_with_flags_and_symbol(
        &self,
        properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
        symbol: Option<SymbolId>,
    ) -> TypeId {
        Self::object_with_flags_and_symbol(self, properties, flags, symbol)
    }

    fn object_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId {
        Self::object_type_from_shape(self, shape_id)
    }

    fn object_with_index_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId {
        Self::object_with_index_type_from_shape(self, shape_id)
    }

    fn object_with_index(&self, shape: ObjectShape) -> TypeId {
        Self::object_with_index(self, shape)
    }

    fn object_fresh_with_display(
        &self,
        widened_properties: Vec<PropertyInfo>,
        display_properties: Vec<PropertyInfo>,
    ) -> TypeId {
        Self::object_fresh_with_display(self, widened_properties, display_properties)
    }

    fn function(&self, shape: FunctionShape) -> TypeId {
        Self::function(self, shape)
    }

    fn callable(&self, shape: CallableShape) -> TypeId {
        Self::callable(self, shape)
    }

    fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId {
        Self::template_literal(self, spans)
    }

    fn conditional(&self, conditional: ConditionalType) -> TypeId {
        Self::conditional(self, conditional)
    }

    fn mapped(&self, mapped: MappedType) -> TypeId {
        Self::mapped(self, mapped)
    }

    fn reference(&self, symbol: SymbolRef) -> TypeId {
        Self::reference(self, symbol)
    }

    fn lazy(&self, def_id: DefId) -> TypeId {
        Self::lazy(self, def_id)
    }

    fn bound_parameter(&self, index: u32) -> TypeId {
        Self::bound_parameter(self, index)
    }

    fn recursive(&self, depth: u32) -> TypeId {
        Self::recursive(self, depth)
    }

    fn type_param(&self, info: TypeParamInfo) -> TypeId {
        Self::type_param(self, info)
    }

    fn unresolved_type_name(&self, name: Atom) -> TypeId {
        Self::unresolved_type_name(self, name)
    }

    fn type_query(&self, symbol: SymbolRef) -> TypeId {
        Self::type_query(self, symbol)
    }

    fn enum_type(&self, def_id: DefId, structural_type: TypeId) -> TypeId {
        Self::enum_type(self, def_id, structural_type)
    }

    fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId {
        Self::application(self, base, args)
    }

    fn literal_string_atom(&self, atom: Atom) -> TypeId {
        Self::literal_string_atom(self, atom)
    }

    fn union_preserve_members(&self, members: Vec<TypeId>) -> TypeId {
        Self::union_preserve_members(self, members)
    }

    fn readonly_type(&self, inner: TypeId) -> TypeId {
        Self::readonly_type(self, inner)
    }

    fn keyof(&self, inner: TypeId) -> TypeId {
        Self::keyof(self, inner)
    }

    fn index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
        Self::index_access(self, object_type, index_type)
    }

    fn this_type(&self) -> TypeId {
        Self::this_type(self)
    }

    fn no_infer(&self, inner: TypeId) -> TypeId {
        Self::no_infer(self, inner)
    }

    fn unique_symbol(&self, symbol: SymbolRef) -> TypeId {
        Self::unique_symbol(self, symbol)
    }

    fn infer(&self, info: TypeParamInfo) -> TypeId {
        Self::infer(self, info)
    }

    fn string_intrinsic(&self, kind: StringIntrinsicKind, type_arg: TypeId) -> TypeId {
        Self::string_intrinsic(self, kind, type_arg)
    }

    fn get_class_base_type(&self, _symbol_id: SymbolId) -> Option<TypeId> {
        // TypeInterner doesn't have access to the Binder, so it can't resolve base classes.
        // The Checker will override this to provide the actual implementation.
        None
    }

    fn is_identity_comparable_type(&self, type_id: TypeId) -> bool {
        Self::is_identity_comparable_type(self, type_id)
    }

    fn is_this_type_marker_def_id(&self, def_id: DefId) -> bool {
        Self::is_this_type_marker_def_id(self, def_id)
    }

    fn consume_evaluation_fuel(&self, amount: u32) -> bool {
        Self::consume_evaluation_fuel(self, amount)
    }

    fn is_evaluation_fuel_exhausted(&self) -> bool {
        Self::is_evaluation_fuel_exhausted(self)
    }

    fn reset_evaluation_fuel(&self) {
        Self::reset_evaluation_fuel(self);
    }
}

/// Implement `TypeResolver` for `TypeInterner` with noop resolution.
///
/// `TypeInterner` doesn't have access to the Binder or type environment,
/// so it cannot resolve symbol references or `DefIds`. Only `resolve_ref`
/// (required) is explicitly implemented; all other resolution methods
/// inherit the trait's default `None`/`false` behavior. The three boxed/array
/// methods delegate to `TypeInterner`'s own inherent methods.
impl TypeResolver for TypeInterner {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        TypeInterner::get_boxed_type(self, kind)
    }

    fn get_array_base_type(&self) -> Option<TypeId> {
        self.get_array_base_type()
    }

    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        self.get_array_base_type_params()
    }

    fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        TypeInterner::get_readonly_array_base_type(self)
    }
}

impl TypeRawIntersectionConstruction for TypeInterner {
    fn intersect_types_raw_for_replay(&self, members: Vec<TypeId>) -> TypeId {
        TypeInterner::intersect_types_raw_for_replay(self, members)
    }
}

/// Query layer for higher-level solver operations.
///
/// This is the incremental boundary where caching and (future) salsa hooks live.
/// Inherits from `TypeResolver` to enable Lazy/Ref type resolution through `evaluate_type()`.
pub trait QueryDatabase:
    TypeDatabase
    + TypeResolver
    + CollectPropertiesResultCache
    + TypeRawIntersectionConstruction
    + IntersectionDisplayReduction
{
    /// Expose the underlying `TypeDatabase` view for legacy entry points.
    fn as_type_database(&self) -> &dyn TypeDatabase;

    /// Expose the `TypeResolver` view for inference contexts that need
    /// to expand type alias Applications (variance-aware inference).
    fn as_type_resolver(&self) -> &dyn TypeResolver;

    /// Expose the shared, arena-invariant `DefinitionStore` when one is
    /// attached, for the generic-call inference HKT-reduce shim
    /// (`StoreOnlyResolver`, issue #14344 / #14345, default-OFF behind
    /// `TSZ_INFER_HKT_REDUCE`).
    ///
    /// The generic-call inference site holds only `&dyn QueryDatabase`, but the
    /// store-backed `resolve_lazy`/`get_lazy_type_params` it needs to expand a
    /// cross-arena `Lazy(DefId)` base live on the concrete `QueryCache`. This
    /// accessor surfaces the program-global store (never mutated through here)
    /// so the inference site can build a `StoreOnlyResolver` against it without
    /// reaching into `QueryCache` internals. The default returns `None` so
    /// non-`QueryCache` databases (raw `TypeInterner`, tests) keep the existing
    /// resolver-less inference path and stay byte-identical.
    fn definition_store_for_inference(&self) -> Option<&DefinitionStore> {
        None
    }

    /// Allocate a declaration-scoped type parameter whose identity must not be
    /// collapsed with another same-shaped declaration.
    fn fresh_type_param(&self, info: TypeParamInfo) -> TypeId {
        self.as_type_database().type_param(info)
    }

    /// Expose the checked construction surface for type constructors.
    #[inline]
    fn factory(&self) -> TypeFactory<'_> {
        TypeFactory::new(self.as_type_database())
    }

    /// Register the canonical `Array<T>` base type used by property access resolution.
    ///
    /// Some call paths resolve properties through a `TypeInterner`-backed database,
    /// while others use a `TypeEnvironment`-backed resolver. Implementations should
    /// store this in whichever backing stores they use so `T[]` methods/properties
    /// (e.g. `push`, `length`) resolve consistently.
    fn register_array_base_type(&self, _type_id: TypeId, _type_params: Vec<TypeParamInfo>) {}

    /// Register the `Array<T>` base type used for display-order-sensitive queries.
    fn register_array_display_base_type(&self, _type_id: TypeId) {}

    /// Register the `ReadonlyArray<T>` base type used by property access resolution.
    ///
    /// Enables property access on `readonly T[]` to resolve against the
    /// `ReadonlyArray<T>` interface (which lacks mutating methods) rather than
    /// the mutable `Array<T>` interface.
    fn register_readonly_array_base_type(&self, _type_id: TypeId) {}

    /// Register a boxed interface type for a primitive intrinsic kind.
    ///
    /// Similar to `register_array_base_type`, this ensures that property access
    /// resolution can find the correct interface type (e.g., String, Number) for
    /// primitive types, regardless of which database backend is used.
    fn register_boxed_type(&self, _kind: IntrinsicKind, _type_id: TypeId) {}

    /// Register a `DefId` as belonging to a boxed type.
    fn register_boxed_def_id(&self, _kind: IntrinsicKind, _def_id: DefId) {}

    /// Register a `DefId` as belonging to the `ThisType` marker interface.
    fn register_this_type_def_id(&self, _def_id: DefId) {}

    fn evaluate_conditional(&self, cond: &ConditionalType) -> TypeId {
        crate::evaluation::evaluate::evaluate_conditional(self.as_type_database(), cond)
    }

    fn evaluate_index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
        self.evaluate_index_access_with_options(
            object_type,
            index_type,
            self.no_unchecked_indexed_access(),
        )
    }

    fn evaluate_index_access_with_options(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
        crate::evaluation::evaluate::evaluate_index_access_with_options(
            self.as_type_database(),
            object_type,
            index_type,
            no_unchecked_indexed_access,
        )
    }

    fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        crate::evaluation::evaluate::evaluate_type(self.as_type_database(), type_id)
    }

    fn evaluate_type_with_options(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
        if !no_unchecked_indexed_access {
            return self.evaluate_type(type_id);
        }

        let mut evaluator =
            crate::evaluation::evaluate::TypeEvaluator::new(self.as_type_database());
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator.evaluate(type_id)
    }

    fn evaluate_mapped(&self, mapped: &MappedType) -> TypeId {
        crate::evaluation::evaluate::evaluate_mapped(self.as_type_database(), mapped)
    }

    /// Look up a cross-call `instantiate_type` cache entry.
    ///
    /// The default returns `None` so non-`QueryCache` databases (raw
    /// `TypeInterner`, tests) don't need to implement it. Cache-aware
    /// instantiation entry points consult this after their existing leaf fast
    /// paths when callers pass `Some(&dyn QueryDatabase)`.
    fn lookup_instantiation_cache(&self, _key: &InstantiationCacheKey) -> Option<TypeId> {
        None
    }

    /// Store an `instantiate_type` result in the cross-call cache. Callers that
    /// do not have a stability verdict use the stable-publication path, which
    /// preserves the pre-existing direct test helper behavior.
    fn insert_instantiation_cache(&self, key: InstantiationCacheKey, result: TypeId) {
        self.insert_instantiation_cache_with_project_stability(key, result, true);
    }

    /// Store an `instantiate_type` result while naming whether it may be
    /// promoted to a project-wide shared cache. Per-file caches may still keep
    /// unstable but non-overflowed results; cross-file caches must only see
    /// results whose ambient request state stayed stable.
    fn insert_instantiation_cache_with_project_stability(
        &self,
        _key: InstantiationCacheKey,
        _result: TypeId,
        _stable_for_project_cache: bool,
    ) {
    }

    /// Look up a cached `remove_subtypes_for_bct` result.
    ///
    /// Mirrors `lookup_instantiation_cache`. The default returns `None` so
    /// non-`QueryCache` databases (raw `TypeInterner`, tests) don't need
    /// to implement it. Hit/miss counters live on `QueryCache`.
    ///
    /// Closes the O(N²) hot loop in `compute_best_common_type` when the
    /// same input candidate list shows up at multiple call sites — the
    /// `BCT candidates=200` bench fixture exercises four such sites with
    /// the same 200-element list, collapsing three of four to O(1).
    fn lookup_subtype_reduction_cache(
        &self,
        _key: &SubtypeReductionKey,
    ) -> Option<std::sync::Arc<[TypeId]>> {
        None
    }

    /// Store a `remove_subtypes_for_bct` result in the cross-call cache.
    /// Default is a no-op for the same reason as
    /// `lookup_subtype_reduction_cache`.
    fn insert_subtype_reduction_cache(
        &self,
        _key: SubtypeReductionKey,
        _result: std::sync::Arc<[TypeId]>,
    ) {
    }

    fn evaluate_keyof(&self, operand: TypeId) -> TypeId {
        crate::evaluation::evaluate::evaluate_keyof(self.as_type_database(), operand)
    }

    fn narrow(&self, type_id: TypeId, narrower: TypeId) -> TypeId
    where
        Self: Sized,
    {
        crate::narrowing::NarrowingContext::new(self).narrow(type_id, narrower)
    }

    fn resolve_property_access(
        &self,
        object_type: TypeId,
        prop_name: &str,
    ) -> crate::operations::property::PropertyAccessResult;

    /// Resolve property access with an already-interned property name,
    /// avoiding the re-hash that the `&str` entry pays at the boundary.
    fn resolve_property_access_atom(
        &self,
        object_type: TypeId,
        prop_atom: Atom,
    ) -> crate::operations::property::PropertyAccessResult;

    fn resolve_property_access_with_options(
        &self,
        object_type: TypeId,
        prop_name: &str,
        no_unchecked_indexed_access: bool,
    ) -> crate::operations::property::PropertyAccessResult;

    /// Resolve a value-level element access whose index expression has type
    /// `any` (e.g. `obj[someAny]`) against the receiver's applicable index
    /// signature.
    ///
    /// `T[any]` at the type level resolves to `any`; this helper is for the
    /// value-level case where tsc instead routes the access through the
    /// applicable index signature so `noUncheckedIndexedAccess` keeps
    /// widening reads to `T | undefined` and rejecting `undefined` writes
    /// against the un-widened slot type. Returns `None` when the receiver
    /// has no string or number index signature.
    fn resolve_any_index_access(
        &self,
        object_type: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> Option<crate::operations::property::PropertyAccessResult>;

    fn property_access_type(
        &self,
        object_type: TypeId,
        prop_name: &str,
    ) -> crate::operations::property::PropertyAccessResult {
        self.resolve_property_access_with_options(
            object_type,
            prop_name,
            self.no_unchecked_indexed_access(),
        )
    }

    fn set_no_unchecked_indexed_access(&self, _enabled: bool) {}

    fn set_exact_optional_property_types(&self, _enabled: bool) {}

    fn set_strict_null_checks(&self, _enabled: bool) {}

    fn contextual_property_type(&self, expected: TypeId, prop_name: &str) -> Option<TypeId> {
        let ctx = crate::computation::ContextualTypeContext::with_expected(
            self.as_type_database(),
            expected,
        );
        ctx.get_property_type(prop_name)
    }

    /// Like [`QueryDatabase::contextual_property_type`], but for a *present*
    /// property value under `exactOptionalPropertyTypes`: an optional
    /// property's own declared type (`number` for `y?: number`) rather than
    /// the read-side type with `undefined` unioned in (`number | undefined`).
    /// A property whose type already includes `undefined` explicitly
    /// (`y?: number | undefined`) is unaffected, since its declared type
    /// already carries that `undefined`.
    fn contextual_property_assignment_type(
        &self,
        expected: TypeId,
        prop_name: &str,
    ) -> Option<TypeId> {
        let ctx = crate::computation::ContextualTypeContext::with_expected(
            self.as_type_database(),
            expected,
        );
        ctx.get_property_assignment_type(prop_name)
    }

    fn is_property_readonly(&self, object_type: TypeId, prop_name: &str) -> bool {
        crate::operations::property::property_is_readonly(
            self.as_type_database(),
            object_type,
            prop_name,
        )
    }

    fn is_readonly_index_signature(
        &self,
        object_type: TypeId,
        wants_string: bool,
        wants_number: bool,
    ) -> bool {
        crate::operations::property::is_readonly_index_signature(
            self.as_type_database(),
            object_type,
            wants_string,
            wants_number,
        )
    }

    /// Resolve element access (array/tuple indexing) with detailed error reporting
    fn resolve_element_access(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> ElementAccessResult {
        let mut evaluator = ElementAccessEvaluator::new(self.as_type_database());
        let flag = self.no_unchecked_indexed_access();
        evaluator.set_no_unchecked_indexed_access(flag);
        evaluator.resolve_element_access(object_type, index_type, literal_index)
    }

    /// Resolve element access type with cache-friendly error normalization.
    fn resolve_element_access_type(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> TypeId {
        match self.resolve_element_access(object_type, index_type, literal_index) {
            ElementAccessResult::Success(type_id) => type_id,
            _ => TypeId::ERROR,
        }
    }

    /// Collect properties that can be spread into object literals.
    fn collect_object_spread_properties(&self, spread_type: TypeId) -> Vec<PropertyInfo> {
        let builder = ObjectLiteralBuilder::new(self.as_type_database());
        builder.collect_spread_properties(spread_type)
    }

    /// Get index signatures for a type
    fn get_index_signatures(&self, type_id: TypeId) -> IndexInfo;

    /// Check if a type contains null or undefined
    fn is_nullish_type(&self, type_id: TypeId) -> bool;

    /// Remove null and undefined from a type
    fn remove_nullish(&self, type_id: TypeId) -> TypeId;

    /// Get the canonical `TypeId` for a type, achieving O(1) structural identity checks.
    ///
    /// This memoizes the Canonicalizer output so that structurally identical types
    /// (e.g., `type A = Box<Box<string>>` and `type B = Box<Box<string>>`) return
    /// the same canonical `TypeId`.
    ///
    /// The implementation must:
    /// - Use a fresh Canonicalizer with empty stacks (for absolute De Bruijn indices)
    /// - Only expand `TypeAlias` (`DefKind::TypeAlias`), preserving nominal types
    /// - Cache the result for O(1) subsequent lookups
    ///
    /// Task #49: Global Canonical Mapping
    fn canonical_id(&self, type_id: TypeId) -> TypeId;

    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool {
        // Default implementation: use non-strict mode for backward compatibility
        self.is_subtype_of_with_policy(source, target, RelationPolicy::unflagged_compatibility())
    }

    /// Subtype check with a typed relation policy.
    ///
    /// Prefer this for new relation paths. It keeps relation behavior and cache
    /// partitioning described by [`RelationPolicy`] instead of extending the
    /// legacy packed `u16` flag protocol.
    fn is_subtype_of_with_policy(
        &self,
        source: TypeId,
        target: TypeId,
        policy: RelationPolicy,
    ) -> bool {
        query_relation(
            self.as_type_database(),
            source,
            target,
            RelationKind::Subtype,
            policy,
            RelationContext::default(),
        )
        .related
    }

    /// TypeScript assignability check with full compatibility rules (The Lawyer).
    ///
    /// This is distinct from `is_subtype_of`:
    /// - `is_subtype_of` = Strict structural subtyping (The Judge) - for internal solver use
    /// - `is_assignable_to` = Loose with TS rules (The Lawyer) - for Checker diagnostics
    ///
    /// The Lawyer handles:
    /// - Any type propagation (any is assignable to/from everything)
    /// - Legacy null/undefined assignability (without strictNullChecks)
    /// - Weak type detection (excess property checking)
    /// - Empty object accepts any non-nullish value
    /// - Function bivariance (when not in strictFunctionTypes mode)
    ///
    /// Uses separate cache from `is_subtype_of` to prevent cache poisoning.
    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        // Default implementation: use non-strict mode for backward compatibility
        self.is_assignable_to_with_policy(source, target, RelationPolicy::unflagged_compatibility())
    }

    /// Assignability check with a typed relation policy.
    ///
    /// Prefer this for new relation paths. It keeps relation behavior and cache
    /// partitioning described by [`RelationPolicy`] instead of extending the
    /// legacy packed `u16` flag protocol.
    fn is_assignable_to_with_policy(
        &self,
        source: TypeId,
        target: TypeId,
        policy: RelationPolicy,
    ) -> bool {
        query_relation(
            self.as_type_database(),
            source,
            target,
            RelationKind::Assignable,
            policy,
            RelationContext::default(),
        )
        .related
    }

    /// Look up a cached subtype result for the given key.
    /// Returns `None` if the result is not cached.
    /// Default implementation returns `None` (no caching).
    fn lookup_subtype_cache(&self, _key: RelationCacheKey) -> Option<bool> {
        None
    }

    /// Cache a subtype result for the given key.
    /// Default implementation is a no-op.
    fn insert_subtype_cache(&self, _key: RelationCacheKey, _result: bool) {}

    /// Look up the full cached subtype entry, including budget-conditional
    /// [`crate::types::RelationCacheValue::LimitTrue`] verdicts that the plain
    /// boolean [`lookup_subtype_cache`](Self::lookup_subtype_cache) hides.
    /// Default implementation surfaces only definitive entries.
    fn lookup_subtype_cache_value(
        &self,
        key: RelationCacheKey,
    ) -> Option<crate::types::RelationCacheValue> {
        self.lookup_subtype_cache(key)
            .map(crate::types::RelationCacheValue::from_bool)
    }

    /// Promote a coinductively validated maybe-key (a relation that resolved
    /// through a cycle assumption whose outermost relation completed
    /// successfully — `tsc`'s `maybeKeys` promotion in `checkTypeRelatedTo`)
    /// to a definitive `true` entry. Must NOT overwrite an existing definitive
    /// entry: a sibling checker may have computed an honest `false` under a
    /// different budget regime. Default implementation is a no-op.
    fn promote_subtype_cache_true(&self, _key: RelationCacheKey) {}

    /// Record an assumed-related limit verdict
    /// ([`crate::types::RelationCacheValue::LimitTrue`]) valid for queries
    /// whose remaining global fuel budget is at most `fuel_band`. Must NOT
    /// overwrite an existing definitive entry; an existing `LimitTrue` keeps
    /// the larger band. Default implementation is a no-op.
    fn insert_subtype_limit_true(&self, _key: RelationCacheKey, _fuel_band: u32) {}

    /// Look up a cached intersection-to-merged-object result.
    /// Used by `build_object_intersection_target` to avoid expensive property
    /// collection for large intersections that are checked multiple times.
    /// The stamp must come from the resolver used to resolve lazy members.
    /// Returns `None` if not cached.
    fn lookup_intersection_merge(
        &self,
        _intersection_id: TypeId,
        _resolver_generation: u64,
    ) -> Option<IntersectionMergeCacheEntry> {
        None
    }

    /// Cache an intersection-to-merged-object result.
    /// `result` is `Some(merged_type_id)` on success, `None` if the intersection
    /// is not eligible for merging (contains callables, non-objects, etc.).
    fn insert_intersection_merge(
        &self,
        _intersection_id: TypeId,
        _resolver_generation: u64,
        _result: Option<TypeId>,
    ) {
    }

    /// Look up a cached assignability result for the given key.
    /// Returns `None` if the result is not cached.
    /// Default implementation returns `None` (no caching).
    fn lookup_assignability_cache(&self, _key: RelationCacheKey) -> Option<bool> {
        None
    }

    /// Cache an assignability result for the given key.
    /// Default implementation is a no-op.
    fn insert_assignability_cache(&self, _key: RelationCacheKey, _result: bool) {}

    #[allow(dead_code, private_interfaces)] // Reserved for full inference pipeline integration
    fn new_inference_context(&self) -> crate::inference::infer::InferenceContext<'_> {
        crate::inference::infer::InferenceContext::new(self.as_type_database())
    }

    /// Task #41: Get the variance mask for a generic type definition.
    ///
    /// Returns the variance of each type parameter for the given `DefId`.
    /// Returns None if the `DefId` is not a generic type or variance cannot be determined.
    fn get_type_param_variance(&self, def_id: DefId) -> Option<Arc<[Variance]>>;

    /// Pure session-cache lookup for a previously stored variance mask.
    ///
    /// Unlike [`Self::get_type_param_variance`], this never computes a result on
    /// a miss (it does not run `resolve_lazy`/`compute_variance` using the query
    /// database's own resolver). It only returns an entry that was already
    /// inserted via [`Self::insert_type_param_variance`]. Resolver-aware callers
    /// use it to memoize variance keyed by `DefId` without delegating the
    /// computation to a resolver that may not see local alias bodies.
    ///
    /// Default implementation always misses.
    fn get_cached_type_param_variance(&self, _def_id: DefId) -> Option<Arc<[Variance]>> {
        None
    }

    /// Store a resolver-computed variance mask for reuse by later relation checks.
    fn insert_type_param_variance(&self, _def_id: DefId, _variance: Arc<[Variance]>) {}
}

impl QueryDatabase for TypeInterner {
    fn as_type_database(&self) -> &dyn TypeDatabase {
        self
    }

    fn as_type_resolver(&self) -> &dyn TypeResolver {
        self
    }

    fn fresh_type_param(&self, info: TypeParamInfo) -> TypeId {
        Self::fresh_type_param(self, info)
    }

    fn register_array_base_type(&self, type_id: TypeId, type_params: Vec<TypeParamInfo>) {
        self.set_array_base_type(type_id, type_params);
    }

    fn register_array_display_base_type(&self, type_id: TypeId) {
        self.set_array_display_base_type(type_id);
    }

    fn register_readonly_array_base_type(&self, type_id: TypeId) {
        self.set_readonly_array_base_type(type_id);
    }

    fn register_boxed_type(&self, kind: IntrinsicKind, type_id: TypeId) {
        TypeInterner::set_boxed_type(self, kind, type_id);
    }

    fn register_boxed_def_id(&self, kind: IntrinsicKind, def_id: DefId) {
        TypeInterner::register_boxed_def_id(self, kind, def_id);
    }

    fn register_this_type_def_id(&self, def_id: DefId) {
        TypeInterner::register_this_type_def_id(self, def_id);
    }

    fn get_index_signatures(&self, type_id: TypeId) -> IndexInfo {
        crate::objects::index_signatures::IndexSignatureResolver::new(self).get_index_info(type_id)
    }

    fn is_nullish_type(&self, type_id: TypeId) -> bool {
        narrowing::is_nullish_type(self, type_id)
    }

    fn remove_nullish(&self, type_id: TypeId) -> TypeId {
        narrowing::remove_nullish_query(self, type_id)
    }

    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        // Default implementation: use non-strict mode for backward compatibility
        self.is_assignable_to_with_policy(source, target, RelationPolicy::unflagged_compatibility())
    }

    fn resolve_property_access(
        &self,
        object_type: TypeId,
        prop_name: &str,
    ) -> crate::operations::property::PropertyAccessResult {
        // TypeInterner doesn't have TypeResolver capability, so it can't resolve Lazy types
        // Use PropertyAccessEvaluator with QueryDatabase (self implements both TypeDatabase and TypeResolver)
        let mut evaluator = crate::operations::property::PropertyAccessEvaluator::new(self);
        evaluator
            .set_exact_optional_property_types(TypeInterner::exact_optional_property_types(self));
        evaluator.resolve_property_access(object_type, prop_name)
    }

    fn resolve_property_access_atom(
        &self,
        object_type: TypeId,
        prop_atom: Atom,
    ) -> crate::operations::property::PropertyAccessResult {
        let mut evaluator = crate::operations::property::PropertyAccessEvaluator::new(self);
        evaluator
            .set_exact_optional_property_types(TypeInterner::exact_optional_property_types(self));
        evaluator.resolve_property_access_atom(object_type, prop_atom)
    }

    fn resolve_property_access_with_options(
        &self,
        object_type: TypeId,
        prop_name: &str,
        no_unchecked_indexed_access: bool,
    ) -> crate::operations::property::PropertyAccessResult {
        let mut evaluator = crate::operations::property::PropertyAccessEvaluator::new(self);
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator
            .set_exact_optional_property_types(TypeInterner::exact_optional_property_types(self));
        evaluator.resolve_property_access(object_type, prop_name)
    }

    fn resolve_any_index_access(
        &self,
        object_type: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> Option<crate::operations::property::PropertyAccessResult> {
        let mut evaluator = crate::operations::property::PropertyAccessEvaluator::new(self);
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator
            .set_exact_optional_property_types(TypeInterner::exact_optional_property_types(self));
        evaluator.resolve_any_index_access(object_type)
    }

    fn resolve_element_access(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> ElementAccessResult {
        let mut evaluator = ElementAccessEvaluator::new(self.as_type_database());
        evaluator.set_no_unchecked_indexed_access(TypeInterner::no_unchecked_indexed_access(self));
        evaluator.resolve_element_access(object_type, index_type, literal_index)
    }

    fn resolve_element_access_type(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> TypeId {
        match self.resolve_element_access(object_type, index_type, literal_index) {
            ElementAccessResult::Success(type_id) => type_id,
            _ => TypeId::ERROR,
        }
    }

    fn set_no_unchecked_indexed_access(&self, enabled: bool) {
        TypeInterner::set_no_unchecked_indexed_access(self, enabled);
    }

    fn set_exact_optional_property_types(&self, enabled: bool) {
        TypeInterner::set_exact_optional_property_types(self, enabled);
    }

    fn set_strict_null_checks(&self, enabled: bool) {
        TypeInterner::set_strict_null_checks(self, enabled);
    }

    fn get_type_param_variance(&self, _def_id: DefId) -> Option<Arc<[Variance]>> {
        // TypeInterner doesn't have access to type parameter information.
        // The Checker will override this to provide the actual implementation.
        None
    }

    fn canonical_id(&self, type_id: TypeId) -> TypeId {
        // TypeInterner doesn't have caching, so compute directly
        use crate::canonicalize::Canonicalizer;
        let mut canon = Canonicalizer::new(self, self);
        canon.canonicalize(type_id)
    }
}
