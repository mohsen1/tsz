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

use crate::construction::TypeDatabase;
#[cfg(test)]
use crate::types::*;
use crate::types::{
    IndexSignature, MappedType, ObjectShape, ParamInfo, TupleElement, TypeData, TypeId,
    TypeParamInfo, TypePredicate,
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

/// Instantiator for applying type substitutions.
pub struct TypeInstantiator<'a> {
    interner: &'a dyn TypeDatabase,
    substitution: &'a TypeSubstitution,
    /// Track visited types to handle cycles
    visiting: FxHashMap<TypeId, TypeId>,
    /// Type parameter names that are shadowed in the current scope.
    shadowed: Vec<Atom>,
    /// Freshly-instantiated local type parameters for the current nested generic scope.
    local_type_params: Vec<(Atom, TypeId)>,
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
    /// When `Some((source, iter_var, declared_type))`, any `IndexAccess(source, K)` where
    /// `K` is a `TypeParameter` with name == `iter_var` is replaced with `declared_type`
    /// instead of being evaluated. Used in homomorphic `-?` mapped type evaluation to feed
    /// the declared (non-optional) property type into the template, matching tsc behavior.
    pub declared_index_type: Option<(TypeId, Atom, TypeId)>,
    depth: u32,
    max_depth: u32,
    depth_exceeded: bool,
    /// Cached: `true` when every key in `substitution.map` is a solver
    /// inference variable (`__infer_*`). The substitution is immutable for the
    /// lifetime of the instantiator, so this is computed once at construction.
    substitution_is_inference_only: bool,
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
            substitution,
            visiting: FxHashMap::default(),
            shadowed: Vec::new(),
            local_type_params: Vec::new(),
            substitute_infer: false,
            preserve_meta_types: false,
            preserve_unsubstituted_type_params: false,
            this_type: None,
            shallow_this_only: false,
            declared_index_type: None,
            depth: 0,
            max_depth: MAX_INSTANTIATION_DEPTH,
            depth_exceeded: false,
            substitution_is_inference_only,
        }
    }

    fn is_shadowed(&self, name: Atom) -> bool {
        self.shadowed.contains(&name)
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
        let p_name = constraint_param.name;
        if p_name == mapped.type_param.name || self.is_shadowed(p_name) {
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
        if index_param.name != p_name {
            return None;
        }
        // `P` must be substituted with a single property key. A union of keys
        // would change `T[P]` (the union of all key values) into a per-key
        // `T[Q]`, so it is intentionally excluded.
        let substituted = self.substitution.get(p_name)?;
        let resolved = crate::evaluation::evaluate::evaluate_type(self.interner, substituted);
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
            Some(TypeData::ReadonlyType(inner)) => {
                let inner_resolved = crate::evaluation::evaluate::evaluate_type(interner, inner);
                if let Some(TypeData::Array(element_type)) = interner.lookup(inner_resolved) {
                    Some((element_type, true))
                } else {
                    None
                }
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = interner.object_shape(shape_id);
                shape
                    .number_index
                    .as_ref()
                    .filter(|idx| idx.readonly)
                    .map(|idx| (idx.value_type, true))
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
            Some(TypeData::ReadonlyType(inner)) => {
                let inner_eval = crate::evaluation::evaluate::evaluate_type(interner, inner);
                matches!(
                    interner.lookup(inner_eval),
                    Some(TypeData::Array(_) | TypeData::Tuple(_))
                )
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
        param_name: Atom,
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
                                Some(TypeData::TypeParameter(info)) if info.name == param_name
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

    /// Instantiate type parameter constraints and defaults.
    fn instantiate_type_params_if_changed(
        &mut self,
        type_params: &[TypeParamInfo],
    ) -> Option<Vec<TypeParamInfo>> {
        let saved_preserve_unsubstituted = self.preserve_unsubstituted_type_params;
        self.preserve_unsubstituted_type_params = true;

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
            if let Some(instantiated) = &mut instantiated {
                instantiated.push(new_type_param);
            } else if new_type_param != *type_param {
                let mut changed = Vec::with_capacity(type_params.len());
                changed.extend_from_slice(&type_params[..index]);
                changed.push(new_type_param);
                instantiated = Some(changed);
            }
        }

        self.preserve_unsubstituted_type_params = saved_preserve_unsubstituted;
        instantiated
    }

    /// Instantiate function/signature parameters.
    fn instantiate_params_if_changed(&mut self, params: &[ParamInfo]) -> Option<Vec<ParamInfo>> {
        let mut instantiated: Option<Vec<ParamInfo>> = None;
        for (index, param) in params.iter().enumerate() {
            let type_id = self.instantiate(param.type_id);
            let original = *param;
            let param = ParamInfo {
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
    /// Returns `(saved_shadowed_len, saved_visiting)` for restoring via
    /// [`exit_shadowing_scope`].
    fn enter_shadowing_scope(
        &mut self,
        type_params: &[TypeParamInfo],
    ) -> (usize, Option<FxHashMap<TypeId, TypeId>>) {
        let shadowed_len = self.shadowed.len();
        let saved_visiting = if type_params.is_empty() {
            None
        } else if self.visiting.is_empty() {
            // PERF: When visiting map is empty (common for top-level generic
            // instantiation), no clone needed — just remove the type params
            // (which are no-ops on an empty map) and return an empty map
            // as the "saved" state.
            Some(FxHashMap::default())
        } else {
            let saved = self.visiting.clone();
            for tp in type_params {
                let tp_id = self.interner.type_param(*tp);
                self.visiting.remove(&tp_id);
            }
            Some(saved)
        };
        self.shadowed.extend(type_params.iter().map(|tp| tp.name));
        (shadowed_len, saved_visiting)
    }

    /// Exit a shadowing scope, restoring the previous state.
    fn exit_shadowing_scope(
        &mut self,
        shadowed_len: usize,
        saved_visiting: Option<FxHashMap<TypeId, TypeId>>,
    ) {
        self.shadowed.truncate(shadowed_len);
        if let Some(saved) = saved_visiting {
            self.visiting = saved;
        }
    }

    fn lookup_local_type_param(&self, name: Atom) -> Option<TypeId> {
        self.local_type_params
            .iter()
            .rev()
            .find_map(|(bound_name, type_id)| (*bound_name == name).then_some(*type_id))
    }

    /// Apply the substitution to a type, returning the instantiated type.
    ///
    /// Wrapped with `stacker::maybe_grow()` to handle deeply nested generic
    /// instantiation chains that would otherwise overflow the stack.
    pub fn instantiate(&mut self, type_id: TypeId) -> TypeId {
        let _span =
            tracing::trace_span!("instantiate", ty = type_id.0, depth = self.depth,).entered();

        // Fast path: intrinsic types don't need instantiation
        if type_id.is_intrinsic() {
            return type_id;
        }

        if self.depth_exceeded {
            return TypeId::ERROR;
        }

        if self.depth >= self.max_depth {
            self.depth_exceeded = true;
            return TypeId::ERROR;
        }

        // Shared cross-operation stack-frame breaker. The per-instance `depth`
        // guard above resets whenever a fresh `TypeInstantiator` is built mid
        // `evaluate -> instantiate -> evaluate` cycle; this thread-local frame
        // budget bounds the combined recursion that no single instance sees
        // (issue #7574). On exhaustion bail like the depth-limit path above.
        // `depth` is adjusted inside the body so it only counts frames we
        // actually descend into, never the exhausted-bail path.
        crate::recursion::with_solver_frame(|| {
            self.depth += 1;
            let result = self.instantiate_inner(type_id);
            self.depth -= 1;
            result
        })
        .unwrap_or_else(|| {
            self.depth_exceeded = true;
            TypeId::ERROR
        })
    }

    fn instantiate_inner(&mut self, type_id: TypeId) -> TypeId {
        // Check if we're already processing this type (cycle detection)
        if let Some(&cached) = self.visiting.get(&type_id) {
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
        }

        // Look up the type structure
        let key = match self.interner.lookup(type_id) {
            Some(k) => k,
            None => return type_id,
        };

        if Self::is_instantiation_leaf(&key) {
            return type_id;
        }

        // Mark as visiting (with original ID as placeholder for cycles)
        self.visiting.insert(type_id, type_id);

        let result = self.instantiate_key(type_id, &key);

        // Update the cache with the actual result
        self.visiting.insert(type_id, result);

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
                if let Some(local_type_param) = self.lookup_local_type_param(info.name) {
                    return local_type_param;
                }
                if self.is_shadowed(info.name) {
                    tracing::trace!(
                        name = ?self.interner.resolve_atom_ref(info.name),
                        shadowed = ?self.shadowed.iter().map(|a| self.interner.resolve_atom_ref(*a)).collect::<Vec<_>>(),
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
                if let Some(substituted) = self.substitution.get(info.name) {
                    tracing::trace!(
                        name = ?self.interner.resolve_atom_ref(info.name),
                        substituted = substituted.0,
                        "instantiate TypeParameter: SUBSTITUTED"
                    );
                    substituted
                } else {
                    if !self.preserve_unsubstituted_type_params
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

            // Intrinsics don't change
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
            | TypeData::ModuleNamespace(_) => self.interner.intern(*key),

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
                    self.interner.intern(*key)
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
                    self.interner.intern(*key)
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
                    let result = self.interner.union(instantiated.clone());
                    self.interner.store_union_origin(result, instantiated);
                    result
                } else {
                    self.interner.intern(*key)
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
                    self.interner.intern(*key)
                }
            }

            // Array: instantiate element type
            TypeData::Array(elem) => {
                let instantiated_elem = self.instantiate(*elem);
                self.interner.array(instantiated_elem)
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
                    return self.interner.intern(*key);
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
                    return self.interner.intern(*key);
                }
                if let Some(instantiated) =
                    self.instantiate_properties_if_changed(&shape.properties)
                {
                    let result = self.interner.object_with_flags_and_symbol(
                        instantiated,
                        shape.flags,
                        shape.symbol,
                    );
                    let original = self.interner.intern(*key);
                    self.propagate_instantiated_display_properties(original, result);
                    self.propagate_instantiated_application_origin(original, result);
                    result
                } else {
                    self.interner.intern(*key)
                }
            }

            // Object with index signatures: instantiate all types
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                if self.shallow_this_only && shape.symbol.is_some() {
                    return self.interner.intern(*key);
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
                if instantiated_props.is_some()
                    || instantiated_string_idx.is_some()
                    || instantiated_number_idx.is_some()
                {
                    let result = self.interner.object_with_index(ObjectShape {
                        flags: shape.flags,
                        properties: instantiated_props.unwrap_or_else(|| shape.properties.clone()),
                        string_index: instantiated_string_idx.or(shape.string_index),
                        number_index: instantiated_number_idx.or(shape.number_index),
                        symbol: shape.symbol,
                    });
                    let original = self.interner.intern(*key);
                    self.propagate_instantiated_display_properties(original, result);
                    self.propagate_instantiated_application_origin(original, result);
                    result
                } else {
                    self.interner.intern(*key)
                }
            }

            // Function: instantiate params and return type
            // Note: Type params in the function create a new scope - don't substitute those
            TypeData::Function(shape_id) => self.instantiate_function(shape_id, key),

            // Callable: instantiate all signatures and properties
            TypeData::Callable(shape_id) => self.instantiate_callable(shape_id, key),

            // Conditional: instantiate all parts
            TypeData::Conditional(cond_id) => self.instantiate_conditional(cond_id),

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
                    self.interner.intern(*key)
                } else {
                    self.interner.readonly_type(inst_operand)
                }
            }

            // NoInfer: preserve wrapper, instantiate inner
            TypeData::NoInfer(inner) => {
                let inst_inner = self.instantiate(*inner);
                if inst_inner == *inner {
                    self.interner.intern(*key)
                } else {
                    self.interner.no_infer(inst_inner)
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
                    && !self.is_shadowed(info.name)
                    && let Some(substituted) = self.substitution.get(info.name)
                {
                    return substituted;
                }
                // Instantiate the constraint if it references type parameters being substituted.
                // e.g., `infer A extends keyof T` when T is being substituted with Obj
                // needs the constraint updated to `keyof Obj`.
                let new_info = if let Some(constraint) = info.constraint {
                    let new_constraint = self.instantiate(constraint);
                    if new_constraint != constraint {
                        TypeParamInfo {
                            constraint: Some(new_constraint),
                            ..*info
                        }
                    } else {
                        *info
                    }
                } else {
                    *info
                };
                self.interner.infer(new_info)
            }
        }
    }
}

mod api;
mod conditional;
mod display_properties;
mod homomorphic;
mod indexed;
mod mapped;
mod signatures;
mod substitution;

pub use self::api::*;
use self::api::{
    conditional_condition_needs_resolver, index_access_operand_needs_resolver,
    mapped_constraint_needs_resolver, template_has_lazy_application_in_composite,
    type_contains_lazy_application,
};
pub use self::substitution::TypeSubstitution;
#[cfg(test)]
#[path = "../../tests/instantiate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/instantiate_readonly_mapped_tests.rs"]
mod readonly_mapped_tests;
