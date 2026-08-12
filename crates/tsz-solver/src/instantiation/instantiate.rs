//! Generic type instantiation and substitution.
//!
//! This module implements type parameter substitution for generic types.
//! When a generic function/type is instantiated, we replace type parameters
//! with concrete types throughout the type structure.
//!
//! Key features:
//! - Type substitution map (type parameter name -> `TypeId`)
//! - Deep recursive substitution through nested types
//! - Handling of constraints and defaults

use crate::caches::db::QueryDatabase;
use crate::construction::TypeDatabase;
use crate::instantiation::result::InstantiationTermination;
#[cfg(test)]
use crate::types::*;
use crate::types::{
    IndexSignature, MappedType, ObjectShape, ParamInfo, TupleElement, TupleListId, TypeData,
    TypeId, TypeParamInfo, TypePredicate,
};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;

/// Maximum depth for recursive type *substitution*.
///
/// NOTE: distinct from (and half of) the checker-side
/// `tsz_common::limits::MAX_INSTANTIATION_DEPTH` (100, tsc's
/// `instantiationDepth` parity) despite the shared name; see the divergence
/// notes at the canonical definition in [`crate::limits`].
pub const MAX_INSTANTIATION_DEPTH: u32 = crate::limits::MAX_TYPE_SUBSTITUTION_DEPTH;
const MAX_TUPLE_SPREAD_FLATTEN_ELEMENTS: usize = 8192;

/// Named owner for one instantiation walk's recursion-depth verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InstantiationWalkState {
    depth: u32,
    max_depth: u32,
    termination: InstantiationTermination,
}

/// Entry decision for an instantiation recursion frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InstantiationFrameState {
    Entered,
    DepthExceeded,
}

/// Per-walk instantiation memo state.
///
/// Active entries are cycle breakers and remain valid while a nested generic
/// installs rewritten local bindings. Completed entries depend on the exact
/// shadowing, local-binding, and declaration-preservation environment in which
/// they were computed, so their epoch must match before reuse.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InstantiationMemoEntry {
    Active,
    Completed {
        result: TypeId,
        environment_epoch: u64,
    },
}

/// State restored after leaving a generic shadowing scope.
struct ShadowingScopeSnapshot {
    visiting: FxHashMap<TypeId, InstantiationMemoEntry>,
    environment_epoch: u64,
}

/// One rewritten type parameter in a nested generic scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LocalTypeParamBinding {
    declaration: TypeParamInfo,
    placeholder: TypeParamInfo,
    instantiated: TypeId,
}

impl LocalTypeParamBinding {
    const fn new(declaration: TypeParamInfo, instantiated: TypeId) -> Self {
        // Lowering first binds this declaration-shaped placeholder so its own
        // constraint/default can refer to it, then replaces that binding with
        // the complete declaration info for later signature positions.
        let placeholder = TypeParamInfo {
            constraint: None,
            default: None,
            ..declaration
        };
        Self {
            declaration,
            placeholder,
            instantiated,
        }
    }

    fn matches(&self, candidate: &TypeParamInfo) -> bool {
        self.declaration.is_same_binder(*candidate)
    }
}

impl InstantiationWalkState {
    const fn new(max_depth: u32) -> Self {
        Self {
            depth: 0,
            max_depth,
            termination: InstantiationTermination::Complete,
        }
    }

    const fn depth(&self) -> u32 {
        self.depth
    }

    const fn has_depth_exceeded(&self) -> bool {
        self.termination.depth_exceeded()
    }

    const fn termination(&self) -> InstantiationTermination {
        self.termination
    }

    const fn mark_depth_exceeded(&mut self) {
        self.termination = InstantiationTermination::DepthExceeded;
    }

    const fn enter_frame(&mut self) -> InstantiationFrameState {
        if self.has_depth_exceeded() || self.depth >= self.max_depth {
            self.mark_depth_exceeded();
            return InstantiationFrameState::DepthExceeded;
        }
        self.depth += 1;
        InstantiationFrameState::Entered
    }

    const fn leave_frame(&mut self) {
        self.depth -= 1;
    }
}

/// Instantiator for applying type substitutions.
pub struct TypeInstantiator<'a> {
    interner: &'a dyn TypeDatabase,
    query_db: Option<&'a dyn QueryDatabase>,
    substitution: &'a TypeSubstitution,
    /// Track visited types to handle cycles
    visiting: FxHashMap<TypeId, InstantiationMemoEntry>,
    /// Type parameter binders that are shadowed in the current scope.
    shadowed: Vec<TypeParamInfo>,
    /// Freshly-instantiated local type parameters for the current nested generic scope.
    ///
    /// Authoritative declaration origins select the exact local even after its
    /// constraint/default changes. Legacy unstamped `User` parameters retain
    /// the historical name-keyed fallback because signatures store
    /// `TypeParamInfo`, not the declaration-scoped fresh `TypeId` (#14344).
    local_type_params: Vec<LocalTypeParamBinding>,
    /// Version of the semantic environment that affects per-walk memo results.
    /// Incrementing this is O(1); completed entries from older versions are
    /// recomputed lazily, while active entries continue to break cycles.
    memo_environment_epoch: u64,
    substitute_infer: bool,
    preserve_meta_types: bool,
    preserve_unsubstituted_type_params: bool,
    /// When set, substitutes `ThisType` with this concrete type.
    pub this_type: Option<TypeId>,
    /// When set with `this_type`, ONLY substitute `ThisType` references at
    /// type-combinator positions (Intersection / Union / `IndexAccess` / `KeyOf` /
    /// Conditional, etc.). Skip recursion into Object, Function, and Callable
    /// internals so their stored method bodies' `this` references remain
    /// polymorphic for property-access-time rebinding.
    ///
    /// Required for `apply_this_substitution_to_call_return`: when a method
    /// returns `this & T` and the receiver is `Label`, we want
    /// `Label & T_inferred`, NOT a re-baked `Label_obj_with_this_substituted`.
    /// Re-baking poisons subsequent intersection wrapping (the chained
    /// `extend({a}).extend({b})` pattern in `intersectionThisTypes.ts`).
    ///
    /// MUST stay false for class-specialization paths (heritage merge,
    /// `instantiate_type_with_this`) where the substitution legitimately
    /// means "specialize this method body for this class".
    pub shallow_this_only: bool,
    walk_state: InstantiationWalkState,
    /// Cached: `true` when every key in `substitution.map` is a solver
    /// inference variable (`__infer_*`). The substitution is immutable for the
    /// lifetime of the instantiator, so this is computed once at construction.
    substitution_is_inference_only: bool,
    /// Set when this walk ever bailed through the SHARED cross-operation
    /// solver-frame budget (see [`crate::recursion::with_solver_frame`]),
    /// as opposed to this instance's own local `walk_state` depth cap.
    ///
    /// The distinction matters for the project-wide instantiation cache
    /// (#14345): the local depth cap is a per-instance counter that always
    /// starts at 0 (`InstantiationWalkState::new`), so a local depth-exceeded
    /// verdict is a pure, reproducible function of this request's
    /// `(type_id, this_type, subst, options)` and is safe to memoize. The
    /// shared frame budget is ambient state shared with every other
    /// concurrently-nested solver operation, so the SAME request could
    /// legitimately succeed or bail depending on unrelated call-stack depth
    /// at the time it runs — memoizing that verdict would be unsound.
    ambient_frame_exhausted: bool,
}

impl<'a> TypeInstantiator<'a> {
    /// Create a new instantiator.
    pub fn new(interner: &'a dyn TypeDatabase, substitution: &'a TypeSubstitution) -> Self {
        let substitution_is_inference_only = !substitution.map.is_empty()
            && substitution.map.keys().all(|key| {
                // Substitution keys are bare atoms (no `TypeParamInfo` reachable),
                // so this classifies the inference-placeholder key by name.
                crate::operations::generic_call::atom_names_inference_placeholder(
                    interner.resolve_atom_ref(*key).as_ref(),
                )
            });
        TypeInstantiator {
            interner,
            query_db: None,
            substitution,
            visiting: FxHashMap::default(),
            shadowed: Vec::new(),
            local_type_params: Vec::new(),
            memo_environment_epoch: 0,
            substitute_infer: false,
            preserve_meta_types: false,
            preserve_unsubstituted_type_params: false,
            this_type: None,
            shallow_this_only: false,
            walk_state: InstantiationWalkState::new(MAX_INSTANTIATION_DEPTH),
            substitution_is_inference_only,
            ambient_frame_exhausted: false,
        }
    }

    fn is_shadowed(&self, info: &TypeParamInfo) -> bool {
        self.shadowed
            .iter()
            .any(|shadowed| shadowed.is_same_binder(*info))
    }

    pub(crate) fn with_query_db(mut self, query_db: Option<&'a dyn QueryDatabase>) -> Self {
        // Keep the resolver-aware `QueryDatabase` at the outer cache boundary.
        // Nested instantiation evaluation must stay resolver-less: routing
        // `evaluate_*` through query-backed semantic helpers is not cache-only
        // and can change inference/conformance behavior.
        //
        // #14345 dormant re-reduce: when `TSZ_INST_RESOLVER_REREDUCE=1`,
        // thread the resolver-aware db through so the gated re-reduce sites in
        // `instantiate_index_access`/`instantiate_conditional` can resolve the
        // cross-arena `Lazy` base instead of re-deferring resolver-less. The OFF
        // path is byte-identical to the prior unconditional `None`.
        if flags::inst_resolver_rereduce_enabled() {
            self.query_db = query_db;
        } else {
            self.query_db = None;
        }
        self
    }

    #[inline]
    pub(super) fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        if let Some(db) = self.query_db {
            db.evaluate_type(type_id)
        } else {
            crate::evaluation::evaluate::evaluate_type(self.interner, type_id)
        }
    }

    #[inline]
    pub(super) fn evaluate_index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
        if let Some(db) = self.query_db {
            db.evaluate_index_access(object_type, index_type)
        } else {
            crate::evaluation::evaluate::evaluate_index_access(
                self.interner,
                object_type,
                index_type,
            )
        }
    }

    #[inline]
    pub(super) fn evaluate_keyof(&self, operand: TypeId) -> TypeId {
        if let Some(db) = self.query_db {
            db.evaluate_keyof(operand)
        } else {
            crate::evaluation::evaluate::evaluate_keyof(self.interner, operand)
        }
    }

    /// Pre-trip the sticky depth-exceeded guard so the very next
    /// `instantiate` call takes the bail path. Test-only helper used to
    /// exercise `bail_value` deterministically without constructing a type
    /// deeper than `MAX_INSTANTIATION_DEPTH`.
    #[cfg(test)]
    pub(crate) const fn force_depth_exceeded_for_test(&mut self) {
        self.walk_state.termination = InstantiationTermination::DepthExceeded;
    }

    pub(crate) const fn has_depth_exceeded(&self) -> bool {
        self.walk_state.has_depth_exceeded()
    }

    /// Whether this walk ever bailed through the SHARED cross-operation
    /// solver-frame budget rather than only its own local depth cap. See the
    /// field doc on [`Self::ambient_frame_exhausted`] for why the project-wide
    /// instantiation cache must gate on this instead of on
    /// [`Self::has_depth_exceeded`] alone.
    pub(crate) const fn ambient_frame_exhausted(&self) -> bool {
        self.ambient_frame_exhausted
    }

    pub(crate) const fn termination(&self) -> InstantiationTermination {
        self.walk_state.termination()
    }

    pub(crate) const fn mark_depth_exceeded(&mut self) {
        self.walk_state.mark_depth_exceeded();
    }

    /// Restore the homomorphic modifier source of a self-indexed mapped type
    /// `{ [Q in P]: T[P] }` when its constraint parameter `P` is substituted by a
    /// single property key.
    ///
    /// tsc derives a mapped type's modifier source from its iteration
    /// constraint's type parameter (`getModifiersTypeFromMappedType` follows
    /// `P`'s `keyof T` constraint to `T`). Bare substitution of `P := "k"`
    /// rewrites the template `T[P]` to `T["k"]`, erasing that link, so the
    /// inherited `readonly`/optional modifiers of `T`'s key are dropped. This is
    /// the substrate of ts-essentials' `ReadonlyKeys` / `WritableKeys` /
    /// `MarkWritable`, where the `IfEquals` identity trick compares
    /// `{ [Q in P]: T[P] }` against `{ -readonly [Q in P]: T[P] }`.
    ///
    /// Because the sole iterated key `Q` equals `P` here, `T[P]` denotes `T[Q]`.
    /// Rewriting the template index from the constraint parameter `P` to the
    /// iteration variable `Q` restores the homomorphic form so the evaluator
    /// inherits `T`'s per-key modifiers, matching tsc — without changing the
    /// property's value type. The single-key guard is essential: for a union of
    /// keys `T[P]` is the union of all key values, which `T[Q]` (per-key) would
    /// not preserve.
    fn rewrite_single_key_self_indexed_template(&self, mapped: &MappedType) -> Option<TypeId> {
        // Constraint must be a bare type parameter `P` (the homomorphic
        // iteration variable), distinct from this mapped type's own iteration
        // variable `Q`, neither shadowed nor still free. This gate rejects the
        // common `{ [K in keyof T]: ... }` form (constraint is `KeyOf`) up front.
        let TypeData::TypeParameter(constraint_param) = self.interner.lookup(mapped.constraint)?
        else {
            return None;
        };
        if constraint_param.is_same_binder(mapped.type_param) || self.is_shadowed(&constraint_param)
        {
            return None;
        }
        // Template must be the self-index `T[P]` (index is the constraint
        // parameter, not already the iteration variable `Q`). Checked before the
        // potentially-expensive evaluation below so non-matching templates exit
        // cheaply.
        let TypeData::IndexAccess(source, index) = self.interner.lookup(mapped.template)? else {
            return None;
        };
        let TypeData::TypeParameter(index_param) = self.interner.lookup(index)? else {
            return None;
        };
        if !index_param.is_same_binder(constraint_param) {
            return None;
        }
        // `P` must be substituted with a single property key. A union of keys
        // would change `T[P]` (the union of all key values) into a per-key
        // `T[Q]`, so it is intentionally excluded.
        let substituted = self
            .substitution
            .get_for_type_parameter(&constraint_param)?;
        let resolved = self.evaluate_type(substituted);
        if !crate::type_queries::is_type_usable_as_property_name(self.interner, resolved) {
            return None;
        }
        // Rewrite `T[P]` to `T[Q]` (Q = this mapped type's iteration variable).
        let iter_var = self.interner.type_param(mapped.type_param);
        Some(self.interner.index_access(source, iter_var))
    }

    /// Whether the unmapped-TypeParameter constraint-fallback at
    /// `instantiate_inner` is safe to apply for `name`.
    ///
    /// When the substitution binds only inference variables (`__infer_*`) and
    /// `name` is user-defined, the parameter belongs to a different generic
    /// scope, so walking its constraint with this foreign substitution would
    /// expand `T[K]`-style constraints into unions and collapse
    /// `keyof (A | B)` to `never` (#8725). In that case the parameter must
    /// stay put.
    fn should_apply_constraint_fallback(&self, name: Atom) -> bool {
        if !self.substitution_is_inference_only {
            return true;
        }
        // `name` is the bare type-parameter atom being walked; classify by name
        // since no `TypeParamInfo` is in hand here.
        crate::operations::generic_call::atom_names_inference_placeholder(
            self.interner.resolve_atom_ref(name).as_ref(),
        )
    }

    /// Extract the element type from an array-like type (Array, ReadonlyType(Array),
    /// or ReadonlyArray as `ObjectWithIndex`). Returns `(element_type, source_readonly)`;
    /// `source_readonly` tells homomorphic mapped types what to copy.
    fn extract_array_element(
        interner: &dyn TypeDatabase,
        type_id: TypeId,
    ) -> Option<(TypeId, bool)> {
        match interner.lookup(type_id) {
            Some(TypeData::Array(element_type)) => Some((element_type, false)),
            Some(TypeData::Substitution { constraint, .. }) => {
                Self::extract_array_element(interner, constraint)
            }
            Some(TypeData::ReadonlyType(inner)) => {
                let inner_resolved = crate::evaluation::evaluate::evaluate_type(interner, inner);
                match interner.lookup(inner_resolved) {
                    Some(TypeData::Array(element_type)) => Some((element_type, true)),
                    Some(TypeData::Substitution { constraint, .. }) => {
                        Self::extract_array_element(interner, constraint)
                            .map(|(element_type, _)| (element_type, true))
                    }
                    _ => None,
                }
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                // A readonly numeric index signature alone does NOT make this a
                // `ReadonlyArray`: a plain `{ readonly [k: number]: V }` object
                // (optionally with named members) has one too, and tsc maps it
                // to an object with a readonly numeric index signature, not to
                // `readonly V[]`. Require the array marker methods
                // (`slice`/`concat`) — the same structural signal the mapped
                // evaluator and conditional `infer` array paths use — before
                // taking the array shortcut. Without this guard a homomorphic
                // mapped alias over such a source was reshaped into an array and
                // then failed to evaluate, surfacing as a spurious TS2322.
                let shape = interner.object_shape(shape_id);
                shape
                    .number_index
                    .as_ref()
                    .filter(|idx| {
                        idx.readonly
                            && crate::type_queries::object_shape_has_array_marker_methods_db(
                                interner, &shape,
                            )
                    })
                    .map(|idx| (idx.value_type, true))
            }
            _ => None,
        }
    }

    fn extract_tuple_source(
        interner: &dyn TypeDatabase,
        type_id: TypeId,
    ) -> Option<(TupleListId, bool)> {
        let resolved = crate::evaluation::evaluate::evaluate_type(interner, type_id);
        if resolved.is_intrinsic() {
            return None;
        }
        match interner.lookup(resolved) {
            Some(TypeData::Tuple(tuple_id)) => Some((tuple_id, false)),
            Some(TypeData::Substitution { constraint, .. }) => {
                Self::extract_tuple_source(interner, constraint)
            }
            Some(TypeData::ReadonlyType(inner)) => {
                let inner_resolved = crate::evaluation::evaluate::evaluate_type(interner, inner);
                match interner.lookup(inner_resolved) {
                    Some(TypeData::Tuple(tuple_id)) => Some((tuple_id, true)),
                    Some(TypeData::Substitution { constraint, .. }) => {
                        Self::extract_tuple_source(interner, constraint)
                            .map(|(tuple_id, _)| (tuple_id, true))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Check if a type is array-or-tuple-like, handling:
    /// - Direct Array types
    /// - Tuple types
    /// - `ReadonlyType` wrapping Array or Tuple
    /// - Union types where ALL members are array-or-tuple-like
    ///   (e.g., `readonly unknown[] | []` from Promise.all's T constraint)
    fn is_array_or_tuple_like(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
        let evaluated = crate::evaluation::evaluate::evaluate_type(interner, type_id);
        match interner.lookup(evaluated) {
            Some(TypeData::Array(_)) | Some(TypeData::Tuple(_)) => true,
            Some(TypeData::Substitution { constraint, .. }) => {
                Self::is_array_or_tuple_like(interner, constraint)
            }
            Some(TypeData::ReadonlyType(inner)) => {
                let inner_eval = crate::evaluation::evaluate::evaluate_type(interner, inner);
                match interner.lookup(inner_eval) {
                    Some(TypeData::Array(_) | TypeData::Tuple(_)) => true,
                    Some(TypeData::Substitution { constraint, .. }) => {
                        Self::is_array_or_tuple_like(interner, constraint)
                    }
                    _ => false,
                }
            }
            Some(TypeData::Union(members)) => {
                let members = interner.type_list(members);
                !members.is_empty()
                    && members
                        .iter()
                        .all(|m| Self::is_array_or_tuple_like(interner, *m))
            }
            _ => Self::extract_array_element(interner, evaluated).is_some(),
        }
    }

    fn is_primitive_or_primitive_union(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
        if crate::visitors::visitor_predicates::is_primitive_type(interner, type_id) {
            return true;
        }
        let Some(TypeData::Union(members)) = interner.lookup(type_id) else {
            return false;
        };
        interner
            .type_list(members)
            .iter()
            .all(|&member| crate::visitors::visitor_predicates::is_primitive_type(interner, member))
    }

    /// Check whether a mapped template actually depends on the source object's
    /// indexed member type `source_obj[K]`. Array/tuple preservation is only
    /// valid for these homomorphic-style templates; unrelated templates like
    /// `Obj[K]` must degrade to ordinary object expansion.
    fn mapped_template_uses_source_index(
        interner: &dyn TypeDatabase,
        template: TypeId,
        source_obj: TypeId,
        param: TypeParamInfo,
    ) -> bool {
        crate::visitor::collect_all_types(interner, template)
            .into_iter()
            .any(|candidate| {
                if candidate.is_intrinsic() {
                    return false;
                }
                match interner.lookup(candidate) {
                    Some(TypeData::IndexAccess(obj, idx)) if obj == source_obj => {
                        !idx.is_intrinsic()
                            && matches!(
                                interner.lookup(idx),
                                Some(TypeData::TypeParameter(info)) if param.is_same_binder(info)
                            )
                    }
                    _ => false,
                }
            })
    }

    /// Instantiate an optional index signature.
    fn instantiate_index_signature_if_changed(
        &mut self,
        idx: &IndexSignature,
    ) -> Option<IndexSignature> {
        let key_type = self.instantiate(idx.key_type);
        let value_type = self.instantiate(idx.value_type);
        (key_type != idx.key_type || value_type != idx.value_type).then_some(IndexSignature {
            key_type,
            value_type,
            readonly: idx.readonly,
            param_name: idx.param_name,
        })
    }

    /// Instantiate type parameter constraints and defaults, binding each changed
    /// declaration before walking later dependent declarations.
    ///
    /// A signature such as `<T extends Outer<X>, U extends Ref<X, T>>`
    /// needs every occurrence of `T` after outer `X` is substituted to refer to
    /// the rewritten `T`, not the declaration-scoped pre-instantiation `TypeId`.
    /// Bind changed parameters incrementally so later constraints/defaults and
    /// the signature body share that identity. Unchanged parameters stay fresh
    /// and unbound, preserving their declaration identity.
    fn instantiate_type_params_if_changed(
        &mut self,
        type_params: &[TypeParamInfo],
    ) -> Option<Vec<TypeParamInfo>> {
        // Nongeneric functions and call signatures reach this helper too.
        // Their empty declaration list changes no semantic environment, so do
        // not churn the epoch and invalidate sibling memo entries twice.
        if type_params.is_empty() {
            return None;
        }

        let saved_preserve_unsubstituted = self.preserve_unsubstituted_type_params;
        self.set_preserve_unsubstituted_type_params(true);

        let mut instantiated: Option<Vec<TypeParamInfo>> = None;
        for (index, type_param) in type_params.iter().enumerate() {
            let constraint = type_param.constraint.map(|c| self.instantiate(c));
            let default = type_param.default.map(|d| self.instantiate(d));
            let new_type_param = TypeParamInfo {
                is_const: false,
                name: type_param.name,
                constraint,
                default,
                origin: type_param.origin,
            };
            if new_type_param != *type_param {
                let local_type = self.interner.type_param(new_type_param);
                self.local_type_params
                    .push(LocalTypeParamBinding::new(*type_param, local_type));

                // Constraints/defaults walked before this binding may have
                // completed memo entries that contain the old local identity,
                // including shared composite nodes. Advance the environment
                // version so those entries miss lazily in O(1); active entries
                // remain usable cycle breakers.
                self.advance_memo_environment();
            }
            if let Some(instantiated) = &mut instantiated {
                instantiated.push(new_type_param);
            } else if new_type_param != *type_param {
                let mut changed = Vec::with_capacity(type_params.len());
                changed.extend_from_slice(&type_params[..index]);
                changed.push(new_type_param);
                instantiated = Some(changed);
            }
        }

        self.set_preserve_unsubstituted_type_params(saved_preserve_unsubstituted);
        instantiated
    }

    /// Instantiate function/signature parameters.
    fn instantiate_params_if_changed(&mut self, params: &[ParamInfo]) -> Option<Vec<ParamInfo>> {
        let mut instantiated: Option<Vec<ParamInfo>> = None;
        for (index, param) in params.iter().enumerate() {
            let type_id = self.instantiate(param.type_id);
            let original = *param;
            let param = ParamInfo {
                suppress_display_optional: false,
                type_id,
                ..original
            };
            if let Some(instantiated) = &mut instantiated {
                instantiated.push(param);
            } else if param != original {
                let mut changed = Vec::with_capacity(params.len());
                changed.extend_from_slice(&params[..index]);
                changed.push(param);
                instantiated = Some(changed);
            }
        }
        instantiated
    }

    /// Instantiate a list of type IDs, allocating a replacement list only
    /// after the first member changes.
    fn instantiate_type_list_if_changed(&mut self, members: &[TypeId]) -> Option<Vec<TypeId>> {
        let mut instantiated: Option<Vec<TypeId>> = None;
        for (index, &member) in members.iter().enumerate() {
            let inst = self.instantiate(member);
            if let Some(instantiated) = &mut instantiated {
                instantiated.push(inst);
            } else if inst != member {
                let mut changed = Vec::with_capacity(members.len());
                changed.extend_from_slice(&members[..index]);
                changed.push(inst);
                instantiated = Some(changed);
            }
        }
        instantiated
    }

    /// Enter a shadowing scope for type parameters.
    ///
    /// Returns `(saved_shadowed_len, saved_scope)` for restoring via
    /// [`exit_shadowing_scope`].
    fn enter_shadowing_scope(
        &mut self,
        type_params: &[TypeParamInfo],
    ) -> (usize, Option<ShadowingScopeSnapshot>) {
        let shadowed_len = self.shadowed.len();
        let saved_visiting = if type_params.is_empty() {
            None
        } else if self.visiting.is_empty() {
            // PERF: When visiting map is empty (common for top-level generic
            // instantiation), no clone needed — just remove the type params
            // (which are no-ops on an empty map) and return an empty map
            // as the "saved" state.
            Some(ShadowingScopeSnapshot {
                visiting: FxHashMap::default(),
                environment_epoch: self.memo_environment_epoch,
            })
        } else {
            let saved = self.visiting.clone();
            for tp in type_params {
                let tp_id = self.interner.type_param(*tp);
                self.visiting.remove(&tp_id);
            }
            Some(ShadowingScopeSnapshot {
                visiting: saved,
                environment_epoch: self.memo_environment_epoch,
            })
        };
        if !type_params.is_empty() {
            // Completed composite entries can contain a type parameter whose
            // binder becomes shadowed here. Advance the environment version so
            // those entries miss lazily; removing only the direct parameter
            // entry would leave composites rewritten by an outer binding.
            self.advance_memo_environment();
        }
        self.shadowed.extend_from_slice(type_params);
        (shadowed_len, saved_visiting)
    }

    /// Exit a shadowing scope, restoring the previous state.
    fn exit_shadowing_scope(
        &mut self,
        shadowed_len: usize,
        saved_visiting: Option<ShadowingScopeSnapshot>,
    ) {
        self.shadowed.truncate(shadowed_len);
        if let Some(saved) = saved_visiting {
            self.visiting = saved.visiting;
            self.memo_environment_epoch = saved.environment_epoch;
        }
    }

    /// Advance the semantic environment used by completed per-walk memo
    /// entries. A single walk cannot approach 2^64 transitions, so wrapping
    /// cannot alias a live epoch.
    const fn advance_memo_environment(&mut self) {
        self.memo_environment_epoch = self.memo_environment_epoch.wrapping_add(1);
    }

    /// Set declaration-preservation mode and invalidate completed memo entries
    /// when the mode changes. Constraint fallback is deliberately disabled in
    /// preservation mode, so cached results cannot cross this boundary.
    const fn set_preserve_unsubstituted_type_params(&mut self, preserve: bool) {
        if self.preserve_unsubstituted_type_params != preserve {
            self.preserve_unsubstituted_type_params = preserve;
            self.advance_memo_environment();
        }
    }
    fn lookup_local_type_param(&self, info: &TypeParamInfo) -> Option<TypeId> {
        self.local_type_params
            .iter()
            .rev()
            .find_map(|binding| binding.matches(info).then_some(binding.instantiated))
    }

    /// Apply the substitution to a type, returning the instantiated type.
    ///
    /// Wrapped with `stacker::maybe_grow()` to handle deeply nested generic
    /// instantiation chains that would otherwise overflow the stack.
    pub fn instantiate(&mut self, type_id: TypeId) -> TypeId {
        let _span = tracing::trace_span!(
            "instantiate",
            ty = type_id.0,
            depth = self.walk_state.depth(),
        )
        .entered();

        // Fast path: intrinsic types don't need instantiation
        if type_id.is_intrinsic() {
            return type_id;
        }

        if matches!(
            self.walk_state.enter_frame(),
            InstantiationFrameState::DepthExceeded
        ) {
            return self.bail_value(type_id);
        }

        // Shared cross-operation stack-frame breaker. The per-instance `depth`
        // guard above resets whenever a fresh `TypeInstantiator` is built mid
        // `evaluate -> instantiate -> evaluate` cycle; this thread-local frame
        // budget bounds the combined recursion that no single instance sees
        // (issue #7574). On exhaustion bail like the depth-limit path above.
        // `depth` is adjusted inside the body so it only counts frames we
        // actually descend into, never the exhausted-bail path.
        crate::recursion::with_solver_frame(|| {
            let result = self.instantiate_inner(type_id);
            self.walk_state.leave_frame();
            result
        })
        .unwrap_or_else(|| {
            self.walk_state.leave_frame();
            self.mark_depth_exceeded();
            self.ambient_frame_exhausted = true;
            self.bail_value(type_id)
        })
    }

    /// Relation-preserving value returned when the per-instance substitution
    /// depth cap or the shared cross-operation stack-frame budget is exhausted.
    ///
    /// The historical sentinel was `TypeId::ERROR`. That dropped the active
    /// substitution: a downstream consumer (e.g. iterator-element resolution on
    /// a fully-concrete `Map<K, V>`) would then fall back to the *original*,
    /// un-instantiated element type and surface a bare bound type parameter
    /// (`T`) into a concrete context, producing false TS2488 / TS2345 (#13652).
    ///
    /// tsc returns the type un-instantiated/deferred at its instantiation-depth
    /// cap; no free inner type parameter that the substitution binds escapes.
    /// To match that — and to never leak a substitution-domain parameter — the
    /// bail applies only the *head* substitution:
    ///
    /// - A bailing `TypeParameter` whose name is bound by the substitution
    ///   resolves to its binding (so `T` with `{T: number}` becomes `number`,
    ///   never a leaked `T`). A bound name resolved through a fresh local scope
    ///   or constraint fallback is handled the same as the normal walk.
    /// - A `TypeParameter` not bound by the substitution is genuinely free at
    ///   this scope; returning it unchanged preserves true positives (an
    ///   actually-generic iteration still reports the diagnostic).
    /// - Any other shape is returned opaque (un-walked). This is
    ///   relation-preserving: the outer wrapper sees the original closed type
    ///   rather than a half-substituted shape that mixes `ERROR` and stale
    ///   inner parameters.
    ///
    /// O(1) at the bail site; no throughput cost on the success path.
    fn bail_value(&self, type_id: TypeId) -> TypeId {
        let Some(TypeData::TypeParameter(info)) = self.interner.lookup(type_id) else {
            // Non-parameter shapes are returned opaque: the substitution can
            // only introduce a free parameter by rewriting a `TypeParameter`
            // node, and we are not walking into the structure here.
            return type_id;
        };
        if let Some(local_type_param) = self.lookup_local_type_param(&info) {
            return local_type_param;
        }
        if self.is_shadowed(&info) {
            return type_id;
        }
        self.substitution
            .get_for_type_parameter(&info)
            .unwrap_or(type_id)
    }

    fn instantiate_inner(&mut self, type_id: TypeId) -> TypeId {
        // Check if we're already processing this type (cycle detection)
        if let Some(entry) = self.visiting.get(&type_id).copied() {
            let cached = match entry {
                InstantiationMemoEntry::Active => Some(type_id),
                InstantiationMemoEntry::Completed {
                    result,
                    environment_epoch,
                } if environment_epoch == self.memo_environment_epoch => Some(result),
                InstantiationMemoEntry::Completed { .. } => None,
            };
            if let Some(cached) = cached {
                if cached != type_id
                    || matches!(
                        self.interner.lookup(type_id),
                        Some(TypeData::TypeParameter(_))
                    )
                {
                    tracing::trace!(
                        type_id = type_id.0,
                        cached = cached.0,
                        key = ?self.interner.lookup(type_id),
                        "instantiate_inner: VISITING CACHE HIT"
                    );
                }
                return cached;
            } else {
                // The type graph is immutable, but its instantiated result can
                // depend on a newly-installed nested-local binding. Overwrite
                // stale completed state on the recomputation path below.
                self.visiting.remove(&type_id);
            }
        }

        // Look up the type structure
        let key = match self.interner.lookup(type_id) {
            Some(k) => k,
            None => return type_id,
        };

        if Self::is_instantiation_leaf(&key) {
            return type_id;
        }

        // Mark as active before descending so recursive references break their
        // cycle even if a nested generic advances the completed-entry epoch.
        self.visiting
            .insert(type_id, InstantiationMemoEntry::Active);

        let result = self.instantiate_key(type_id, &key);

        // Update the cache with the actual result
        self.visiting.insert(
            type_id,
            InstantiationMemoEntry::Completed {
                result,
                environment_epoch: self.memo_environment_epoch,
            },
        );

        result
    }

    fn instantiate_type_predicate_if_changed(
        &mut self,
        predicate: &TypePredicate,
    ) -> Option<TypePredicate> {
        let type_id = predicate.type_id.map(|type_id| self.instantiate(type_id));
        (type_id != predicate.type_id).then_some(TypePredicate {
            type_id,
            ..*predicate
        })
    }

    #[inline]
    const fn is_instantiation_leaf(key: &TypeData) -> bool {
        matches!(
            key,
            TypeData::Intrinsic(_)
                | TypeData::Literal(_)
                | TypeData::UnresolvedTypeName(_)
                | TypeData::Error
                | TypeData::Lazy(_)
                | TypeData::Recursive(_)
                | TypeData::BoundParameter(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
        )
    }

    /// Instantiate a `TypeData`.
    fn instantiate_key(&mut self, type_id: TypeId, key: &TypeData) -> TypeId {
        match key {
            // Type parameters get substituted
            TypeData::TypeParameter(info) => {
                if let Some(local_type_param) = self.lookup_local_type_param(info) {
                    return local_type_param;
                }
                if self.is_shadowed(info) {
                    tracing::trace!(
                        name = ?self.interner.resolve_atom_ref(info.name),
                        shadowed = ?self.shadowed.iter().map(|p| self.interner.resolve_atom_ref(p.name)).collect::<Vec<_>>(),
                        "instantiate TypeParameter: SHADOWED"
                    );
                    // Return the ORIGINAL id, not a structural re-intern:
                    // declaration-scoped type parameters are interned fresh
                    // (`intern_fresh` bypasses the dedupe table), so
                    // `intern(*key)` would silently rewrite them to the
                    // structural canonical and erase declaration identity
                    // (#13044).
                    return type_id;
                }
                if let Some(substituted) = self
                    .substitution
                    .get_for_type_parameter(info)
                {
                    tracing::trace!(
                        name = ?self.interner.resolve_atom_ref(info.name),
                        substituted = substituted.0,
                        "instantiate TypeParameter: SUBSTITUTED"
                    );
                    substituted
                } else {
                    if !self.substitution.protects_type_parameter_name(info.name)
                        && !self.preserve_unsubstituted_type_params
                        && self.should_apply_constraint_fallback(info.name)
                    {
                        // No direct substitution found. If the type parameter has a constraint
                        // that references substituted type parameters, instantiate the constraint.
                        // Example: Actions extends ActionsObject<State>, with {State: number}
                        // → use ActionsObject<number> instead of Actions.
                        //
                        // This fallback is intentionally disabled while instantiating
                        // type-parameter declarations themselves so self-references like
                        // `Exclude<keyof P, ...>` stay anchored to `P` instead of collapsing
                        // into an error/constraint expansion.
                        if let Some(constraint) = info.constraint {
                            let instantiated_constraint = self.instantiate(constraint);
                            // Only use the constraint if instantiation changed it
                            if instantiated_constraint != constraint {
                                return instantiated_constraint;
                            }
                        }
                    }
                    // No substitution and no instantiated constraint: return
                    // the ORIGINAL id. A structural re-intern would rewrite
                    // declaration-scoped fresh type parameters to the
                    // structural canonical, splitting identity between
                    // instantiated and never-instantiated mentions of the
                    // same declaration (#13044).
                    type_id
                }
            }

            // Intrinsics don't change. `type_id` is the canonical id for each
            // of these keys (none are `intern_fresh`), so re-interning `*key`
            // would recompute the same id; reuse it directly. These kinds are
            // also instantiation leaves, so this arm is normally reached only on
            // a direct `instantiate_key` call.
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::UnresolvedTypeName(_)
            | TypeData::Error
            // Lazy types might resolve to something that needs substitution
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_) => type_id,

            // Enum types: instantiate the member type (structural part)
            // The DefId (nominal identity) stays the same
            TypeData::Enum(def_id, member_type) => {
                let instantiated_member = self.instantiate(*member_type);
                self.interner.enum_type(*def_id, instantiated_member)
            }

            // Application: instantiate base and args
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(*app_id);
                let base = self.instantiate(app.base);
                let args = self.instantiate_type_list_if_changed(&app.args);
                if base == app.base && args.is_none() {
                    type_id
                } else {
                    self.interner
                        .application(base, args.unwrap_or_else(|| app.args.clone()))
                }
            }

            // This type: substitute with concrete this_type if provided
            TypeData::ThisType => {
                if let Some(this_type) = self.this_type {
                    this_type
                } else {
                    type_id
                }
            }

            // Union: instantiate all members, skip re-intern if nothing changed
            TypeData::Union(members) => {
                let canonical_members = self.interner.type_list(*members);
                let origin_members = self.interner.get_union_origin(type_id);
                let members = origin_members
                    .as_deref()
                    .map_or(canonical_members.as_ref(), Vec::as_slice);
                if let Some(instantiated) = self.instantiate_type_list_if_changed(members) {
                    let result = self.interner.union_from_slice(&instantiated);
                    self.interner.store_union_origin(result, instantiated);
                    result
                } else {
                    type_id
                }
            }

            // Intersection: instantiate all members, skip re-intern if nothing changed
            TypeData::Intersection(members) => {
                let members = self.interner.type_list(*members);
                if let Some(instantiated) = self.instantiate_type_list_if_changed(members.as_ref())
                {
                    let result = self.interner.intersection(instantiated);
                    // Propagate display properties from original members to the result.
                    self.propagate_display_properties_for_intersection(members.as_ref(), result);
                    result
                } else {
                    type_id
                }
            }

            // Array: instantiate element type. When the element is unchanged the
            // re-interned array is the same canonical id we already hold.
            TypeData::Array(elem) => {
                let instantiated_elem = self.instantiate(*elem);
                if instantiated_elem == *elem {
                    type_id
                } else {
                    self.interner.array(instantiated_elem)
                }
            }

            TypeData::Tuple(elements) => {
                use tsz_common::limits::MAX_REPRESENTABLE_TUPLE_LENGTH;
                let elements = self.interner.tuple_list(*elements);
                let mut instantiated: Vec<TupleElement> = Vec::with_capacity(elements.len());
                // Tracks the semantic (represented) cardinality — sum of each
                // spread arm's inner element count, not the physical slot count.
                // Needed to catch the case where a large spread is kept as a
                // single physical rest element by the soft gate but the total
                // represented length still exceeds `MAX_REPRESENTABLE_TUPLE_LENGTH`.
                let mut represented_len: usize = 0;
                // Only normalize (merge adjacent Array rests) when substitution
                // actually occurred. Pre-existing concrete tuples (e.g. annotation
                // types with no free type params) must not be normalized here —
                // tsc keeps them in their original form even after re-instantiation.
                let mut changed = false;
                for e in elements.iter() {
                    let inst_type = self.instantiate(e.type_id);
                    if inst_type != e.type_id {
                        changed = true;
                    }
                    if e.rest {
                        // Check if the instantiated type is a tuple — if so,
                        // flatten its elements into the parent tuple.
                        if let Some(TypeData::Tuple(inner_elems)) = self.interner.lookup(inst_type)
                        {
                            let inner = self.interner.tuple_list(inner_elems);
                            let represented_after =
                                represented_len.saturating_add(inner.len());
                            // Hard gate: refuse to materialize tuples wider than
                            // MAX_REPRESENTABLE_TUPLE_LENGTH. Fires before the soft gate
                            // so the unbounded Vec is never allocated.
                            if represented_after > MAX_REPRESENTABLE_TUPLE_LENGTH {
                                self.interner.mark_tuple_too_large();
                                return TypeId::ERROR;
                            }
                            // Soft gate: keep very large spreads as a single rest
                            // element to avoid exponential physical slot growth.
                            if represented_after > MAX_TUPLE_SPREAD_FLATTEN_ELEMENTS {
                                instantiated.push(TupleElement {
                                    type_id: inst_type,
                                    name: e.name,
                                    optional: e.optional,
                                    rest: true,
                                });
                            } else {
                                changed = true; // flattening always changes structure
                                for ie in inner.iter() {
                                    instantiated.push(TupleElement {
                                        type_id: ie.type_id,
                                        name: ie.name,
                                        optional: ie.optional,
                                        rest: ie.rest,
                                    });
                                }
                            }
                            represented_len = represented_after;
                        } else {
                            instantiated.push(TupleElement {
                                type_id: inst_type,
                                name: e.name,
                                optional: e.optional,
                                rest: true,
                            });
                            represented_len = represented_len.saturating_add(1);
                        }
                    } else {
                        instantiated.push(TupleElement {
                            type_id: inst_type,
                            name: e.name,
                            optional: e.optional,
                            rest: false,
                        });
                        represented_len = represented_len.saturating_add(1);
                    }
                }
                if !changed {
                    return type_id;
                }
                crate::intern::tuple_normalized(self.interner, instantiated)
            }

            // Object: instantiate all property types
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                // Shallow-this mode: don't walk into Object internals when the
                // Object has a backing symbol (named interface / class). Such
                // types own a polymorphic `this` scope that must stay raw for
                // property-access-time rebinding when the Object becomes part
                // of an intersection. Anonymous Object literals (no symbol)
                // share the outer `this` scope.
                if self.shallow_this_only && shape.symbol.is_some() {
                    // `type_id` is the canonical id for this Object key (Object
                    // shapes are never `intern_fresh`), so re-interning `*key`
                    // would recompute the same id. Reuse it directly to avoid a
                    // redundant hash + intern-cache probe on the hot path.
                    return type_id;
                }
                if let Some(instantiated) =
                    self.instantiate_properties_if_changed(&shape.properties)
                {
                    let result = self.interner.object_with_flags_and_symbol(
                        instantiated,
                        shape.flags,
                        shape.symbol,
                    );
                    // `type_id` already names the original (pre-substitution)
                    // Object; no need to re-intern `*key` to recover it before
                    // propagating display/application provenance.
                    self.propagate_instantiated_display_properties(type_id, result);
                    self.propagate_instantiated_application_origin(type_id, result);
                    self.propagate_instantiated_merged_intersection_origin(type_id, result);
                    result
                } else {
                    type_id
                }
            }

            // Object with index signatures: instantiate all types
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                if self.shallow_this_only && shape.symbol.is_some() {
                    return type_id;
                }
                let instantiated_props =
                    self.instantiate_properties_if_changed(&shape.properties);
                let instantiated_string_idx = shape
                    .string_index
                    .as_ref()
                    .and_then(|idx| self.instantiate_index_signature_if_changed(idx));
                let instantiated_number_idx = shape
                    .number_index
                    .as_ref()
                    .and_then(|idx| self.instantiate_index_signature_if_changed(idx));
                let instantiated_symbol_idx = shape
                    .symbol_index
                    .as_ref()
                    .and_then(|idx| self.instantiate_index_signature_if_changed(idx));
                if instantiated_props.is_some()
                    || instantiated_string_idx.is_some()
                    || instantiated_number_idx.is_some()
                    || instantiated_symbol_idx.is_some()
                {
                    let result = self.interner.object_with_index(ObjectShape {
                        flags: shape.flags,
                        properties: instantiated_props.unwrap_or_else(|| shape.properties.clone()),
                        string_index: instantiated_string_idx.or(shape.string_index),
                        number_index: instantiated_number_idx.or(shape.number_index),
                        symbol_index: instantiated_symbol_idx.or(shape.symbol_index),
                        symbol: shape.symbol,
                    });
                    self.propagate_instantiated_display_properties(type_id, result);
                    self.propagate_instantiated_application_origin(type_id, result);
                    self.propagate_instantiated_merged_intersection_origin(type_id, result);
                    result
                } else {
                    type_id
                }
            }

            // Function: instantiate params and return type
            // Note: Type params in the function create a new scope - don't substitute those
            TypeData::Function(shape_id) => self.instantiate_function(shape_id, type_id),

            // Callable: instantiate all signatures and properties
            TypeData::Callable(shape_id) => self.instantiate_callable(shape_id, type_id),

            // Conditional: instantiate all parts
            TypeData::Conditional(cond_id) => self.instantiate_conditional(type_id, cond_id),

            // Mapped: instantiate constraint and template
            TypeData::Mapped(mapped_id) => self.instantiate_mapped(mapped_id),

            // Index access: instantiate both parts and evaluate immediately
            // Task #46: Meta-type reduction for O(1) equality
            TypeData::IndexAccess(obj, idx) => self.instantiate_index_access(obj, idx),

            // KeyOf: instantiate the operand and evaluate immediately
            // Task #46: Meta-type reduction for O(1) equality
            TypeData::KeyOf(operand) => self.instantiate_keyof(operand),

            // ReadonlyType: instantiate the operand
            TypeData::ReadonlyType(operand) => {
                let inst_operand = self.instantiate(*operand);
                if inst_operand == *operand {
                    type_id
                } else {
                    self.interner.readonly_type(inst_operand)
                }
            }

            // NoInfer: preserve wrapper, instantiate inner
            TypeData::NoInfer(inner) => {
                let inst_inner = self.instantiate(*inner);
                if inst_inner == *inner {
                    type_id
                } else {
                    self.interner.no_infer(inst_inner)
                }
            }

            // Substitution: instantiate the base and the constraint, then
            // re-derive through the simplifying constructor. Once the base is
            // concrete the narrowing is fully determined and collapses to base.
            TypeData::Substitution {
                base_type,
                constraint,
            } => {
                let inst_base = self.instantiate(*base_type);
                let inst_constraint = self.instantiate(*constraint);
                if inst_base == *base_type && inst_constraint == *constraint {
                    type_id
                } else {
                    self.interner.substitution(inst_base, inst_constraint)
                }
            }

            // Template literal: instantiate embedded types
            // After substitution, if any type span becomes a union of string literals,
            // we trigger evaluation to expand the template literal into a union of strings.
            TypeData::TemplateLiteral(spans) => self.instantiate_template_literal(spans),

            // StringIntrinsic: instantiate the type argument
            // After substitution, if the type argument becomes a concrete type that can
            // be evaluated (like a string literal or union), trigger evaluation.
            TypeData::StringIntrinsic { kind, type_arg } => {
                self.instantiate_string_intrinsic(kind, type_arg)
            }

            // Infer: keep as-is unless explicitly substituting inference variables
            TypeData::Infer(info) => {
                if self.substitute_infer
                    && !self.is_shadowed(info)
                    && let Some(substituted) = self
                        .substitution
                        .get_for_type_parameter(info)
                {
                    return substituted;
                }
                // Instantiate the constraint if it references type parameters being substituted.
                // e.g., `infer A extends keyof T` when T is being substituted with Obj
                // needs the constraint updated to `keyof Obj`.
                if let Some(constraint) = info.constraint {
                    let new_constraint = self.instantiate(constraint);
                    if new_constraint != constraint {
                        return self.interner.infer(TypeParamInfo {
                            constraint: Some(new_constraint),
                            ..*info
                        });
                    }
                }
                type_id
            }
        }
    }
}

mod api;
mod api_lazy;
mod cache_stability;
mod conditional;
mod display_properties;
mod exact_rewrite;
pub(crate) mod flags;
mod homomorphic;
mod indexed;
mod mapped;
mod signatures;
mod substitution;

pub use self::api::*;
use self::api_lazy::{
    conditional_condition_needs_resolver, index_access_operand_needs_resolver,
    mapped_constraint_needs_resolver, template_has_lazy_application_in_composite,
    type_contains_lazy_application,
};
pub use self::exact_rewrite::{
    ExactRewriteAborted, ExactRewriteMemo, substitute_exact_type, substitute_exact_types,
    substitute_exact_types_with_memo,
};
pub(crate) use self::substitution::IdentitySubstitutionDomain;
pub use self::substitution::TypeSubstitution;
#[cfg(test)]
#[path = "../../tests/instantiate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/instantiate_readonly_mapped_tests.rs"]
mod readonly_mapped_tests;
