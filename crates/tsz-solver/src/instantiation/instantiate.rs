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
    CallSignature, CallableShape, ConditionalType, FunctionShape, IndexSignature, IntrinsicKind,
    LiteralValue, MappedType, ObjectShape, ParamInfo, TemplateSpan, TupleElement, TypeData, TypeId,
    TypeParamInfo, TypePredicate,
};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;

/// Maximum depth for recursive type instantiation.
pub const MAX_INSTANTIATION_DEPTH: u32 = 50;
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
                interner
                    .resolve_atom_ref(*key)
                    .as_ref()
                    .starts_with("__infer_")
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
        self.interner
            .resolve_atom_ref(name)
            .as_ref()
            .starts_with("__infer_")
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

    /// Instantiate a call signature.
    fn instantiate_call_signature_if_changed(
        &mut self,
        sig: &CallSignature,
    ) -> Option<CallSignature> {
        let (shadowed_len, saved_visiting) = self.enter_shadowing_scope(&sig.type_params);

        let type_params = self.instantiate_type_params_if_changed(&sig.type_params);
        let local_start = self.local_type_params.len();
        // Redirect occurrences of the signature's own params only when their
        // infos actually changed (constraint/default instantiated). When
        // unchanged, pushing a structural re-intern would rewrite
        // declaration-scoped fresh params to the structural canonical and
        // erase declaration identity; the shadowing scope already preserves
        // them as-is (#13044).
        if let Some(changed_params) = type_params.as_deref() {
            for type_param in changed_params {
                self.local_type_params
                    .push((type_param.name, self.interner.type_param(*type_param)));
            }
        }
        let type_predicate = sig
            .type_predicate
            .as_ref()
            .and_then(|predicate| self.instantiate_type_predicate_if_changed(predicate));
        let this_type = sig.this_type.map(|type_id| self.instantiate(type_id));
        let params = self.instantiate_params_if_changed(&sig.params);
        let return_type = self.instantiate(sig.return_type);
        self.local_type_params.truncate(local_start);

        self.exit_shadowing_scope(shadowed_len, saved_visiting);

        let this_changed = this_type != sig.this_type;
        let return_changed = return_type != sig.return_type;
        if type_params.is_none()
            && params.is_none()
            && type_predicate.is_none()
            && !this_changed
            && !return_changed
        {
            return None;
        }

        Some(CallSignature {
            type_params: type_params.unwrap_or_else(|| sig.type_params.clone()),
            params: params.unwrap_or_else(|| sig.params.clone()),
            this_type,
            return_type,
            type_predicate: type_predicate.or(sig.type_predicate),
            is_method: sig.is_method,
        })
    }

    fn instantiate_call_signatures_if_changed(
        &mut self,
        signatures: &[CallSignature],
    ) -> Option<Vec<CallSignature>> {
        let mut instantiated: Option<Vec<CallSignature>> = None;
        for (index, signature) in signatures.iter().enumerate() {
            let signature = self.instantiate_call_signature_if_changed(signature);
            if let Some(instantiated) = &mut instantiated {
                instantiated.push(signature.unwrap_or_else(|| signatures[index].clone()));
            } else if let Some(signature) = signature {
                let mut changed = Vec::with_capacity(signatures.len());
                changed.extend_from_slice(&signatures[..index]);
                changed.push(signature);
                instantiated = Some(changed);
            }
        }
        instantiated
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
                let args: Vec<TypeId> = app.args.iter().map(|&arg| self.instantiate(arg)).collect();
                self.interner.application(base, args)
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
                let mut changed = false;
                let instantiated: Vec<TypeId> = members
                    .iter()
                    .map(|&m| {
                        let inst = self.instantiate(m);
                        if inst != m {
                            changed = true;
                        }
                        inst
                    })
                    .collect();
                if changed {
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
                let mut changed = false;
                let instantiated: Vec<TypeId> = members
                    .iter()
                    .map(|&m| {
                        let inst = self.instantiate(m);
                        if inst != m {
                            changed = true;
                        }
                        inst
                    })
                    .collect();
                if changed {
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
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(*shape_id);
                // Shallow-this mode: substitute `this:` parameter slot,
                // and substitute params/return_type only when they ARE the
                // top-level `ThisType` (no nesting). Don't recurse into
                // composite types like `this & T` — those carry the
                // polymorphic `this` scope that must stay raw for
                // intersection rebinding (chained `extend({a}).extend({b})`
                // pattern). Top-level `this` substitution is needed for
                // ordinary `(p: this) => this` shapes.
                if self.shallow_this_only {
                    let target_this = self.this_type.unwrap_or(TypeId::ERROR);
                    let sub_top_level = |id: TypeId| -> TypeId {
                        if matches!(self.interner.lookup(id), Some(TypeData::ThisType)) {
                            target_this
                        } else {
                            id
                        }
                    };
                    let new_this_slot = shape.this_type.map(sub_top_level);
                    let new_return_type = sub_top_level(shape.return_type);
                    let mut new_params = Vec::with_capacity(shape.params.len());
                    let mut params_changed = false;
                    for p in shape.params.iter() {
                        let new_t = sub_top_level(p.type_id);
                        if new_t != p.type_id {
                            params_changed = true;
                            let mut np = *p;
                            np.type_id = new_t;
                            new_params.push(np);
                        } else {
                            new_params.push(*p);
                        }
                    }
                    let this_changed = match (shape.this_type, new_this_slot) {
                        (Some(a), Some(b)) => a != b,
                        (None, None) => false,
                        _ => true,
                    };
                    if params_changed || this_changed || new_return_type != shape.return_type {
                        return self.interner.function(FunctionShape {
                            type_params: shape.type_params.clone(),
                            params: new_params,
                            this_type: new_this_slot,
                            return_type: new_return_type,
                            type_predicate: shape.type_predicate,
                            is_constructor: shape.is_constructor,
                            is_method: shape.is_method,
                        });
                    }
                    return self.interner.intern(*key);
                }
                let (shadowed_len, saved_visiting) = self.enter_shadowing_scope(&shape.type_params);

                let instantiated_type_params =
                    self.instantiate_type_params_if_changed(&shape.type_params);
                let local_start = self.local_type_params.len();
                // Redirect own-param occurrences only when the param infos
                // changed; see `instantiate_call_signature_if_changed` for
                // the declaration-identity rationale (#13044).
                if let Some(changed_params) = instantiated_type_params.as_deref() {
                    for type_param in changed_params {
                        self.local_type_params
                            .push((type_param.name, self.interner.type_param(*type_param)));
                    }
                }
                let type_predicate = shape
                    .type_predicate
                    .as_ref()
                    .and_then(|predicate| self.instantiate_type_predicate_if_changed(predicate));
                let this_type = shape.this_type.map(|type_id| self.instantiate(type_id));
                let instantiated_params = self.instantiate_params_if_changed(&shape.params);
                let instantiated_return = self.instantiate(shape.return_type);
                self.local_type_params.truncate(local_start);

                self.exit_shadowing_scope(shadowed_len, saved_visiting);

                let this_changed = this_type != shape.this_type;
                let return_changed = instantiated_return != shape.return_type;
                if instantiated_type_params.is_some()
                    || instantiated_params.is_some()
                    || type_predicate.is_some()
                    || this_changed
                    || return_changed
                {
                    self.interner.function(FunctionShape {
                        type_params: instantiated_type_params
                            .unwrap_or_else(|| shape.type_params.clone()),
                        params: instantiated_params.unwrap_or_else(|| shape.params.clone()),
                        this_type,
                        return_type: instantiated_return,
                        type_predicate: type_predicate.or(shape.type_predicate),
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    })
                } else {
                    self.interner.intern(*key)
                }
            }

            // Callable: instantiate all signatures and properties
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(*shape_id);
                // Shallow-this mode: substitute the `this:` slot and
                // top-level `ThisType` references in params / return_type;
                // leave deeper composite types alone so polymorphic `this`
                // in method bodies stays raw for intersection rebinding.
                if self.shallow_this_only {
                    let target_this = self.this_type.unwrap_or(TypeId::ERROR);
                    let sub_top_level = |id: TypeId| -> TypeId {
                        if matches!(self.interner.lookup(id), Some(TypeData::ThisType)) {
                            target_this
                        } else {
                            id
                        }
                    };
                    let rewrite_sig = |sig: &CallSignature| -> (CallSignature, bool) {
                        let mut changed = false;
                        let new_this_slot = sig.this_type.map(|s| {
                            let n = sub_top_level(s);
                            if n != s {
                                changed = true;
                            }
                            n
                        });
                        let new_return = sub_top_level(sig.return_type);
                        if new_return != sig.return_type {
                            changed = true;
                        }
                        let mut new_params = Vec::with_capacity(sig.params.len());
                        for p in sig.params.iter() {
                            let new_t = sub_top_level(p.type_id);
                            if new_t != p.type_id {
                                changed = true;
                                let mut np = *p;
                                np.type_id = new_t;
                                new_params.push(np);
                            } else {
                                new_params.push(*p);
                            }
                        }
                        let mut new_sig = sig.clone();
                        new_sig.this_type = new_this_slot;
                        new_sig.return_type = new_return;
                        new_sig.params = new_params;
                        (new_sig, changed)
                    };

                    let mut updated_call = Vec::with_capacity(shape.call_signatures.len());
                    let mut any_changed = false;
                    for sig in shape.call_signatures.iter() {
                        let (new_sig, changed) = rewrite_sig(sig);
                        any_changed |= changed;
                        updated_call.push(new_sig);
                    }
                    let mut updated_construct = Vec::with_capacity(shape.construct_signatures.len());
                    for sig in shape.construct_signatures.iter() {
                        let (new_sig, changed) = rewrite_sig(sig);
                        any_changed |= changed;
                        updated_construct.push(new_sig);
                    }
                    if any_changed {
                        return self.interner.callable(CallableShape {
                            call_signatures: updated_call,
                            construct_signatures: updated_construct,
                            properties: shape.properties.clone(),
                            string_index: shape.string_index,
                            number_index: shape.number_index,
                            symbol: shape.symbol,
                            is_abstract: shape.is_abstract,
                        });
                    }
                    return self.interner.intern(*key);
                }
                let instantiated_call =
                    self.instantiate_call_signatures_if_changed(&shape.call_signatures);
                let instantiated_construct =
                    self.instantiate_call_signatures_if_changed(&shape.construct_signatures);
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

                if instantiated_call.is_some()
                    || instantiated_construct.is_some()
                    || instantiated_props.is_some()
                    || instantiated_string_idx.is_some()
                    || instantiated_number_idx.is_some()
                {
                    self.interner.callable(CallableShape {
                        call_signatures: instantiated_call
                            .unwrap_or_else(|| shape.call_signatures.clone()),
                        construct_signatures: instantiated_construct
                            .unwrap_or_else(|| shape.construct_signatures.clone()),
                        properties: instantiated_props.unwrap_or_else(|| shape.properties.clone()),
                        string_index: instantiated_string_idx.or(shape.string_index),
                        number_index: instantiated_number_idx.or(shape.number_index),
                        symbol: shape.symbol,
                        is_abstract: shape.is_abstract,
                    })
                } else {
                    self.interner.intern(*key)
                }
            }

            // Conditional: instantiate all parts
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(*cond_id);
                if cond.is_distributive
                    && let Some(TypeData::TypeParameter(info)) =
                        self.interner.lookup(cond.check_type)
                    && !self.is_shadowed(info.name)
                    && let Some(substituted) = self.substitution.get(info.name)
                {
                    // When substituting with `never`, the result is `never`
                    if substituted == crate::types::TypeId::NEVER {
                        return substituted;
                    }
                    // For `any`, we need to let evaluation handle it properly
                    // so it can distribute to both branches
                    // TypeScript treats `boolean` as `true | false` for distributive conditionals
                    if substituted == TypeId::BOOLEAN {
                        let cond_type = self.interner.conditional(cond);
                        let mut results = Vec::with_capacity(2);
                        for &member in &[TypeId::BOOLEAN_TRUE, TypeId::BOOLEAN_FALSE] {
                            if self.depth_exceeded {
                                return TypeId::ERROR;
                            }
                            let mut member_subst = self.substitution.clone();
                            member_subst.insert(info.name, member);
                            let instantiated = if self.preserve_unsubstituted_type_params {
                                instantiate_type_preserving(self.interner, cond_type, &member_subst)
                            } else {
                                instantiate_type(self.interner, cond_type, &member_subst)
                            };
                            if instantiated == TypeId::ERROR {
                                self.depth_exceeded = true;
                                return TypeId::ERROR;
                            }
                            let evaluated = crate::evaluation::evaluate::evaluate_type(
                                self.interner,
                                instantiated,
                            );
                            if evaluated == TypeId::ERROR {
                                self.depth_exceeded = true;
                                return TypeId::ERROR;
                            }
                            results.push(evaluated);
                        }
                        return self.interner.union(results);
                    }
                    let distribution_source = match self.interner.lookup(substituted) {
                        Some(TypeData::Union(_)) => substituted,
                        _ => crate::evaluation::evaluate::evaluate_type(
                            self.interner,
                            substituted,
                        ),
                    };
                    if let Some(TypeData::Union(members)) =
                        self.interner.lookup(distribution_source)
                    {
                        let members = self.interner.type_list(members);
                        // Limit distribution to prevent OOM with pathologically
                        // large unions (e.g. string-literal unions with thousands
                        // of members). Shares the evaluation-path cap so both
                        // lowering routes agree on what is representable.
                        if members.len()
                            > crate::evaluation::evaluate_rules::conditional::MAX_CONDITIONAL_DISTRIBUTION_SIZE
                        {
                            self.depth_exceeded = true;
                            return TypeId::ERROR;
                        }
                        let cond_type = self.interner.conditional(cond);
                        let mut results = Vec::with_capacity(members.len());
                        // Reuse one substitution map across members: only the
                        // distributed parameter (`info.name`) changes per step, so
                        // overwrite that single key instead of cloning the whole
                        // map for every member (matters now the cap allows up to
                        // `MAX_CONDITIONAL_DISTRIBUTION_SIZE` members).
                        let mut member_subst = self.substitution.clone();
                        for &member in members.iter() {
                            // Check depth before each distribution step
                            if self.depth_exceeded {
                                return TypeId::ERROR;
                            }
                            member_subst.insert(info.name, member);
                            let instantiated = if self.preserve_unsubstituted_type_params {
                                instantiate_type_preserving(self.interner, cond_type, &member_subst)
                            } else {
                                instantiate_type(self.interner, cond_type, &member_subst)
                            };
                            // Check if instantiation hit depth limit
                            if instantiated == TypeId::ERROR {
                                self.depth_exceeded = true;
                                return TypeId::ERROR;
                            }
                            // Don't evaluate here — the instantiator lacks a TypeResolver,
                            // so evaluate_type (with NoopResolver) can't resolve Lazy types
                            // in the conditional's check/extends positions. Instead, return
                            // the unevaluated conditionals and let the caller's evaluator
                            // (which has a proper resolver) handle evaluation.
                            results.push(instantiated);
                        }
                        return self.interner.union(results);
                    }
                }
                let instantiated = ConditionalType {
                    check_type: self.instantiate(cond.check_type),
                    extends_type: self.instantiate(cond.extends_type),
                    true_type: self.instantiate(cond.true_type),
                    false_type: self.instantiate(cond.false_type),
                    is_distributive: cond.is_distributive,
                };
                self.interner.conditional(instantiated)
            }

            // Mapped: instantiate constraint and template
            TypeData::Mapped(mapped_id) => {
                let mut mapped = self.interner.get_mapped(*mapped_id);
                let tp_slice = std::slice::from_ref(&mapped.type_param);
                let (shadowed_len, saved_visiting) = self.enter_shadowing_scope(tp_slice);

                // Restore homomorphic modifier inheritance for a self-indexed
                // mapped type `{ [Q in P]: T[P] }` whose constraint parameter is
                // substituted by a single key (ts-essentials `ReadonlyKeys` /
                // `WritableKeys` substrate). Done before the constraint/template
                // substitution below collapses `T[P]` to `T["k"]`.
                if let Some(rewritten) = self.rewrite_single_key_self_indexed_template(&mapped) {
                    mapped = MappedType {
                        template: rewritten,
                        ..mapped
                    };
                }

                // Homomorphic array/tuple handling must run before standard
                // instantiation collapses `keyof T` to a flat union.

                // HOMOMORPHIC UNION DISTRIBUTION (tsc: instantiateMappedType → mapTypeWithAlias)
                // Excluded: array/tuple-like unions are handled by the blocks below.
                if let Some(TypeData::KeyOf(keyof_source)) = self.interner.lookup(mapped.constraint)
                    && let Some(TypeData::TypeParameter(tp_info)) =
                        self.interner.lookup(keyof_source)
                    && !self.is_shadowed(tp_info.name)
                    && let Some(substituted) = self.substitution.get(tp_info.name)
                {
                    let resolved =
                        crate::evaluation::evaluate::evaluate_type(self.interner, substituted);
                    if let Some(TypeData::Union(list_id)) = self.interner.lookup(resolved)
                        && !Self::is_array_or_tuple_like(self.interner, resolved)
                    {
                        let members: Vec<TypeId> =
                            self.interner.type_list(list_id).to_vec();
                        let mut results = Vec::with_capacity(members.len());
                        // The iteration variable is a fresh local of this
                        // mapped type; without shadowing it during per-member
                        // splicing, the constraint-resolution fallback in
                        // `instantiate_key` rewrites `K` to its instantiated
                        // constraint and produces `<member>[keyof <member>]`
                        // where the source said `<member>[K]`.
                        let iter_var_shadow = [mapped.type_param.name];
                        for &member in &members {
                            if crate::visitors::visitor_predicates::is_primitive_type(
                                self.interner,
                                member,
                            ) {
                                results.push(member);
                                continue;
                            }
                            let mut member_subst = self.substitution.clone();
                            member_subst.insert(tp_info.name, member);
                            let inst = |t| {
                                instantiate_type_with_shadowed(
                                    self.interner,
                                    t,
                                    &member_subst,
                                    &iter_var_shadow,
                                )
                            };
                            results.push(self.interner.mapped(MappedType {
                                constraint: inst(mapped.constraint),
                                template: inst(mapped.template),
                                name_type: mapped.name_type.map(&inst),
                                type_param: TypeParamInfo {
                                    constraint: mapped.type_param.constraint.map(&inst),
                                    default: mapped.type_param.default.map(&inst),
                                    ..mapped.type_param
                                },
                                ..mapped
                            }));
                        }
                        self.exit_shadowing_scope(shadowed_len, saved_visiting);
                        return self.interner.union(results);
                    }
                }

                // tsc's `instantiateMappedType`: when the homomorphic source T
                // resolves to `any` and T is constrained to array/tuple types,
                // the result is an array shape — independent of whether the
                // template references T[K]. Templates that DO reference T[K]
                // are still handled by the main array-preservation block below
                // (which mirrors tsc's full instantiateMappedArrayType path).
                if crate::type_queries::mapped::is_identity_name_mapping(self.interner, &mapped)
                    && let Some(TypeData::KeyOf(keyof_source)) =
                        self.interner.lookup(mapped.constraint)
                    && let Some(TypeData::TypeParameter(tp_info)) =
                        self.interner.lookup(keyof_source)
                    && !self.is_shadowed(tp_info.name)
                    && let Some(substituted) = self.substitution.get(tp_info.name)
                    && !Self::mapped_template_uses_source_index(
                        self.interner,
                        mapped.template,
                        keyof_source,
                        mapped.type_param.name,
                    )
                    && crate::evaluation::evaluate::evaluate_type(self.interner, substituted)
                        == TypeId::ANY
                    && tp_info.constraint.is_some_and(|c| {
                        let ec = crate::evaluation::evaluate::evaluate_type(self.interner, c);
                        Self::is_array_or_tuple_like(self.interner, ec)
                    })
                {
                    // Substitute T → any in the template, then K → number, then
                    // wrap in Array (matching tsc's instantiateMappedArrayType).
                    let new_template = self.instantiate(mapped.template);
                    self.exit_shadowing_scope(shadowed_len, saved_visiting);

                    let subst =
                        TypeSubstitution::single(mapped.type_param.name, TypeId::NUMBER);
                    let mapped_element = crate::evaluation::evaluate::evaluate_type(
                        self.interner,
                        instantiate_type(self.interner, new_template, &subst),
                    );

                    let final_element = if matches!(
                        mapped.optional_modifier,
                        Some(crate::types::MappedModifier::Add)
                    ) {
                        self.interner.union2(mapped_element, TypeId::UNDEFINED)
                    } else {
                        mapped_element
                    };

                    let array_type = self.interner.array(final_element);
                    return if matches!(
                        mapped.readonly_modifier,
                        Some(crate::types::MappedModifier::Add)
                    ) {
                        self.interner.readonly_type(array_type)
                    } else {
                        array_type
                    };
                }

                if crate::type_queries::mapped::is_identity_name_mapping(self.interner, &mapped)
                    && let Some(TypeData::KeyOf(keyof_source)) =
                        self.interner.lookup(mapped.constraint)
                    && let Some(TypeData::TypeParameter(tp_info)) =
                        self.interner.lookup(keyof_source)
                    && !self.is_shadowed(tp_info.name)
                    && let Some(substituted) = self.substitution.get(tp_info.name)
                {
                    let template_uses_source_index = Self::mapped_template_uses_source_index(
                        self.interner, mapped.template, keyof_source, mapped.type_param.name,
                    );
                    let resolved =
                        crate::evaluation::evaluate::evaluate_type(self.interner, substituted);

                    // tsc: When a homomorphic mapped type's source type parameter
                    // is instantiated with `any`, the result depends on the type
                    // parameter's constraint:
                    //   - Array/tuple constraint → produce array result
                    //   - Non-array constraint → fall through to standard mapped
                    //     type instantiation (produces `{ [x: string]: ... }`)
                    // We must NOT unconditionally return TypeId::ANY because that
                    // makes `Objectish<any>` assignable to `any[]`, which is wrong.
                    if resolved == TypeId::ANY {
                        let constraint_is_array_like = tp_info.constraint.is_some_and(|c| {
                            let ec = crate::evaluation::evaluate::evaluate_type(self.interner, c);
                            Self::is_array_or_tuple_like(self.interner, ec)
                        });

                        if constraint_is_array_like {
                            // Array/tuple-constrained T with any: produce array.
                            // Substitute K → number in the template.
                            let new_template = self.instantiate(mapped.template);
                            self.exit_shadowing_scope(shadowed_len, saved_visiting);

                            let subst = TypeSubstitution::single(mapped.type_param.name, TypeId::NUMBER);
                            let mapped_element = crate::evaluation::evaluate::evaluate_type(
                                self.interner,
                                instantiate_type(self.interner, new_template, &subst),
                            );

                            let final_element = if matches!(
                                mapped.optional_modifier,
                                Some(crate::types::MappedModifier::Add)
                            ) {
                                self.interner.union2(mapped_element, TypeId::UNDEFINED)
                            } else {
                                mapped_element
                            };

                            let array_type = self.interner.array(final_element);
                            return if matches!(
                                mapped.readonly_modifier,
                                Some(crate::types::MappedModifier::Add)
                            ) {
                                self.interner.readonly_type(array_type)
                            } else {
                                array_type
                            };
                        }
                        // IDENTITY homomorphic mapped type with `any`: return any.
                        // tsc returns `any` ONLY for identity templates (`T[K]`), not
                        // for non-identity templates like `Box<T[K]>`.
                        // For non-identity templates, we fall through to standard
                        // instantiation which produces `{ [x: string]: Box<any> }`.
                        let is_identity_template =
                            crate::index_access_parts(self.interner, mapped.template).is_some_and(
                                |(obj, key)| {
                                    obj == keyof_source
                                        && matches!(
                                            self.interner.lookup(key),
                                            Some(TypeData::TypeParameter(kp))
                                                if kp.name == mapped.type_param.name
                                        )
                                },
                            );
                        if is_identity_template {
                            self.exit_shadowing_scope(shadowed_len, saved_visiting);
                            return TypeId::ANY;
                        }
                        // Non-identity template: fall through to standard instantiation
                    }

                    // Check for Tuple first (tsc: instantiateMappedTupleType)
                    // Must also handle ReadonlyType wrapping Tuple
                    let tuple_source = if resolved.is_intrinsic() {
                        None
                    } else {
                        match self.interner.lookup(resolved) {
                            Some(TypeData::Tuple(tid)) => Some((tid, false)),
                            Some(TypeData::ReadonlyType(inner)) => {
                                let ir = crate::evaluation::evaluate::evaluate_type(
                                    self.interner,
                                    inner,
                                );
                                if ir.is_intrinsic() {
                                    None
                                } else {
                                    match self.interner.lookup(ir) {
                                        Some(TypeData::Tuple(tid)) => Some((tid, true)),
                                        _ => None,
                                    }
                                }
                            }
                            _ => None,
                        }
                    };
                    if let Some((tuple_id, source_readonly)) = tuple_source {
                        use crate::types::MappedModifier;
                        let elements = self.interner.tuple_list(tuple_id);
                        // Instantiate template first (substitutes T, keeps K shadowed).
                        // After this `new_template` holds the *resolved* source tuple
                        // wherever T appeared.
                        let new_template = self.instantiate(mapped.template);
                        self.exit_shadowing_scope(shadowed_len, saved_visiting);

                        // Per-element rebinding mirrors tsc's
                        // `instantiateMappedTupleType`. The choice of (template, key)
                        // determines whether `T[K]` resolves to this element's own
                        // type or the union of every element type. The naive
                        // pre-fix loop bound K = "i" for every element, which:
                        //   - dropped the Array<> wrapper from a rest element's
                        //     type_id (producing structurally invalid tuples like
                        //     `[string, ...number]`); and
                        //   - silently widened any fixed element after a rest to
                        //     the union of all element types, because `T["i"]` is
                        //     ambiguous when the rest range could be 0 or more
                        //     elements long.
                        //
                        // The four cases below mirror tsc's switch on per-element kind.
                        enum ElemBinding {
                            /// Rest of `Array<E>` / `readonly E[]` — rewrite the
                            /// resolved source to `Array<E>` and bind K = number;
                            /// the result must re-wrap in `Array<>` so the rest's
                            /// `type_id` stays array-shaped.
                            RestArray(TypeId),
                            /// Rest of an opaque type (type parameter, lazy ref,
                            /// etc.) — bind K = number on the existing template
                            /// so the deferred `T[K]` shape is preserved.
                            OpaqueRest,
                            /// Fixed element after at least one rest — the
                            /// numeric index on the full source is ambiguous, so
                            /// rewrite the source to a single-element proxy and
                            /// bind K = "0".
                            SuffixFixed,
                            /// Fixed element before any rest — the literal index
                            /// resolves unambiguously, no rebinding needed.
                            PrefixFixed,
                        }

                        let rebind_source = |new_source: TypeId| {
                            let mut memo: FxHashMap<TypeId, TypeId> = FxHashMap::default();
                            crate::evaluation::evaluate_rules::substitute::substitute_exact_type_db(
                                self.interner,
                                new_template,
                                resolved,
                                new_source,
                                &mut memo,
                            )
                        };

                        let mut seen_rest = false;
                        let mut new_elements = Vec::with_capacity(elements.len());
                        for (i, elem) in elements.iter().enumerate() {
                            let is_suffix = seen_rest && !elem.rest;
                            if elem.rest {
                                seen_rest = true;
                            }

                            let binding = match (elem.rest, self.interner.lookup(elem.type_id)) {
                                (true, Some(TypeData::Array(_))) => {
                                    ElemBinding::RestArray(elem.type_id)
                                }
                                (true, Some(TypeData::ReadonlyType(roi)))
                                    if matches!(
                                        self.interner.lookup(roi),
                                        Some(TypeData::Array(_))
                                    ) =>
                                {
                                    ElemBinding::RestArray(roi)
                                }
                                (true, _) => ElemBinding::OpaqueRest,
                                (false, _) if is_suffix => ElemBinding::SuffixFixed,
                                (false, _) => ElemBinding::PrefixFixed,
                            };

                            let (rebound_template, key_type) = match binding {
                                ElemBinding::RestArray(rest_arr) => {
                                    (rebind_source(rest_arr), TypeId::NUMBER)
                                }
                                ElemBinding::OpaqueRest => (new_template, TypeId::NUMBER),
                                ElemBinding::SuffixFixed => {
                                    let proxy = self.interner.tuple(vec![
                                        crate::types::TupleElement::fixed(elem.type_id),
                                    ]);
                                    (rebind_source(proxy), self.interner.literal_string("0"))
                                }
                                ElemBinding::PrefixFixed => {
                                    (new_template, self.interner.literal_string(&i.to_string()))
                                }
                            };

                            let subst = TypeSubstitution::single(mapped.type_param.name, key_type);
                            let mapped_type = crate::evaluation::evaluate::evaluate_type(
                                self.interner,
                                instantiate_type(self.interner, rebound_template, &subst),
                            );

                            // Re-wrap a `...E[]` rest in `Array<>`; absorb
                            // `Add ?` on a rest as `T | undefined` (a rest can
                            // not syntactically combine with `?`); apply the
                            // optional modifier to fixed slots only.
                            let (type_id, optional) = match (binding, mapped.optional_modifier) {
                                (ElemBinding::RestArray(_), Some(MappedModifier::Add)) => (
                                    self.interner.union2(
                                        self.interner.array(mapped_type),
                                        TypeId::UNDEFINED,
                                    ),
                                    elem.optional,
                                ),
                                (ElemBinding::RestArray(_), _) => {
                                    (self.interner.array(mapped_type), elem.optional)
                                }
                                (
                                    ElemBinding::OpaqueRest,
                                    Some(MappedModifier::Add),
                                ) => (
                                    self.interner.union2(mapped_type, TypeId::UNDEFINED),
                                    elem.optional,
                                ),
                                (ElemBinding::OpaqueRest, _) | (_, None) => {
                                    (mapped_type, elem.optional)
                                }
                                (_, Some(MappedModifier::Add)) => (mapped_type, true),
                                (_, Some(MappedModifier::Remove)) => (mapped_type, false),
                            };

                            new_elements.push(crate::types::TupleElement {
                                type_id,
                                name: elem.name,
                                optional,
                                rest: elem.rest,
                            });
                        }

                        let tuple_type = self.interner.tuple(new_elements);
                        return if mapped.resolve_readonly(source_readonly) {
                            self.interner.readonly_type(tuple_type)
                        } else {
                            tuple_type
                        };
                    }

                    // Then check for Array (tsc: instantiateMappedArrayType)
                    let array_element = Self::extract_array_element(self.interner, resolved);
                    if let Some((_element_type, source_readonly)) = array_element {
                        // Produce array result: substitute K → number in the template
                        let new_template = self.instantiate(mapped.template);
                        self.exit_shadowing_scope(shadowed_len, saved_visiting);

                        let subst = TypeSubstitution::single(mapped.type_param.name, TypeId::NUMBER);
                        let mapped_element = crate::evaluation::evaluate::evaluate_type(
                            self.interner,
                            crate::instantiation::instantiate::instantiate_type(
                                self.interner,
                                new_template,
                                &subst,
                            ),
                        );

                        // Apply mapped type modifiers
                        let final_element = if matches!(
                            mapped.optional_modifier,
                            Some(crate::types::MappedModifier::Add)
                        ) {
                            self.interner.union2(mapped_element, TypeId::UNDEFINED)
                        } else {
                            mapped_element
                        };

                        let array_type = self.interner.array(final_element);
                        return if mapped.resolve_readonly(source_readonly) {
                            self.interner.readonly_type(array_type)
                        } else {
                            array_type
                        };
                    }

                    // Primitive homomorphic sources pass through unchanged.
                    if template_uses_source_index && Self::is_primitive_or_primitive_union(self.interner, resolved) {
                        self.exit_shadowing_scope(shadowed_len, saved_visiting);
                        return resolved;
                    }

                    if let Some(result) = self.try_expand_substituted_homomorphic_object_mapped(
                        &mapped,
                        resolved,
                    ) {
                        self.exit_shadowing_scope(shadowed_len, saved_visiting);
                        return result;
                    }
                }

                tracing::trace!(
                    tp_name = ?self.interner.resolve_atom_ref(mapped.type_param.name),
                    constraint = mapped.constraint.0,
                    constraint_key = ?self.interner.lookup(mapped.constraint),
                    shadowed = ?self.shadowed.iter().map(|a| self.interner.resolve_atom_ref(*a)).collect::<Vec<_>>(),
                    subst = ?self.substitution.map.iter().map(|(k, v)| (self.interner.resolve_atom_ref(*k), v.0)).collect::<Vec<_>>(),
                    "instantiate Mapped: about to instantiate constraint"
                );
                let saved_preserve_unsubstituted = self.preserve_unsubstituted_type_params;
                self.preserve_unsubstituted_type_params = true;

                let new_constraint = self.instantiate(mapped.constraint);
                let new_template = self.instantiate(mapped.template);
                let new_name_type = mapped.name_type.map(|t| self.instantiate(t));
                let new_param_constraint =
                    mapped.type_param.constraint.map(|c| self.instantiate(c));
                let new_param_default = mapped.type_param.default.map(|d| self.instantiate(d));

                self.preserve_unsubstituted_type_params = saved_preserve_unsubstituted;

                self.exit_shadowing_scope(shadowed_len, saved_visiting);

                tracing::trace!(
                    old_constraint = mapped.constraint.0,
                    new_constraint = new_constraint.0,
                    new_constraint_key = ?self.interner.lookup(new_constraint),
                    old_template = mapped.template.0,
                    new_template = new_template.0,
                    "instantiate Mapped: result"
                );

                // If the mapped type is unchanged after substitution (e.g., because
                // the mapped type's own type parameter shadowed the outer substitution),
                // return the original to avoid eager evaluation that would collapse it.
                let unchanged = new_constraint == mapped.constraint
                    && new_template == mapped.template
                    && new_name_type == mapped.name_type
                    && new_param_constraint == mapped.type_param.constraint
                    && new_param_default == mapped.type_param.default;

                if unchanged {
                    tracing::trace!("instantiate Mapped: UNCHANGED, returning original");
                    return self.interner.mapped(mapped);
                }

                let instantiated = MappedType {
                    type_param: TypeParamInfo {
                        is_const: false,
                        name: mapped.type_param.name,
                        constraint: new_param_constraint,
                        default: new_param_default,
                    },
                    constraint: new_constraint,
                    name_type: new_name_type,
                    template: new_template,
                    readonly_modifier: mapped.readonly_modifier,
                    optional_modifier: mapped.optional_modifier,
                };

                // Trigger evaluation immediately for changed mapped types.
                // This converts MappedType { constraint: "host"|"port", ... }
                // into Object { host?: string, port?: number }
                // Without this, the MappedType is returned unevaluated, causing subtype checks to fail.
                //
                // However, skip eager evaluation when the template Conditional's condition
                // references a body the `NoopResolver` cannot expand: a direct `Lazy(DefId)`
                // or a lazy *application* `Application(Lazy, args)` (cross-file
                // `T[K] extends V<any>`). The per-key subtype check then silently fails and
                // a `{ [K in keyof T]: … }[keyof T]` filter collapses to `never`; defer so
                // the resolver-backed outer evaluator decides each key. The lazy-app check is
                // contained-in (reached via the iteration var, e.g. `Conc[K]`); bare `Lazy`
                // stays top-only (keeps `undefined extends T[P]` eager).
                let mapped_type = self.interner.mapped(instantiated);
                let has_lazy_conditional_boundary =
                    conditional_condition_needs_resolver(self.interner, new_template);
                // Also skip eager evaluation when the template contains Application
                // types whose base is a Lazy(DefId) reference (e.g. recursive type
                // aliases like `Spec<T[P]>`).  The instantiator's NoopResolver cannot
                // resolve these references, so the evaluator would silently drop
                // unresolvable union members.  Deferring lets the outer evaluator
                // (which has a proper TypeResolver) handle the full expansion.
                let has_lazy_application =
                    template_has_lazy_application_in_composite(self.interner, new_template);
                // Same hazard for the `as` clause: a Lazy alias application in
                // `name_type` collapses to `never` under NoopResolver eager
                // evaluation, filtering out every key.
                let name_type_has_lazy_application = new_name_type
                    .is_some_and(|nt| type_contains_lazy_application(self.interner, nt));
                let resolver_dependent_constraint =
                    mapped_constraint_needs_resolver(self.interner, new_constraint);
                if self.preserve_meta_types
                    || has_lazy_conditional_boundary
                    || has_lazy_application
                    || name_type_has_lazy_application
                    || resolver_dependent_constraint
                {
                    mapped_type
                } else if crate::visitor::contains_type_parameters(self.interner, new_constraint) {
                    // Don't eagerly evaluate when the constraint still contains type
                    // parameters (e.g., `keyof __infer_0` during generic call inference).
                    // Premature evaluation would resolve `keyof T` through T's constraint
                    // (e.g., `keyof Record<string, string>` → `string`), destroying the
                    // homomorphic `keyof T` pattern needed for reverse-mapped inference.
                    // The constraint collection and post-inference check will evaluate
                    // the mapped type after inference resolves the type parameters.
                    mapped_type
                } else {
                    crate::evaluation::evaluate::evaluate_type(self.interner, mapped_type)
                }
            }

            // Index access: instantiate both parts and evaluate immediately
            // Task #46: Meta-type reduction for O(1) equality
            TypeData::IndexAccess(obj, idx) => {
                // For homomorphic -? mapped type evaluation, T[K] must use the
                // declared property type (without the `| undefined` that
                // `optional_property_type` adds for read access). Check *idx
                // BEFORE instantiation so we can detect the iteration variable
                // (K → key_literal substitution hasn't happened yet).
                let is_iter_var = self.declared_index_type.is_some_and(|(_, iter_var, _)| {
                    matches!(
                        self.interner.lookup(*idx),
                        Some(TypeData::TypeParameter(p)) if p.name == iter_var
                    )
                });
                let inst_obj = self.instantiate(*obj);
                if let Some((override_source, _, replacement)) = self.declared_index_type
                    && is_iter_var
                    && inst_obj == override_source
                {
                    return replacement;
                }
                let inst_idx = self.instantiate(*idx);
                // Don't eagerly evaluate if either part still contains type parameters.
                // This prevents premature evaluation of `T[K]` or `T[keyof T]` where T
                // is an inference placeholder, which would resolve through the constraint
                // instead of waiting for the actual inferred type.
                if crate::visitor::contains_type_parameters(self.interner, inst_obj)
                    || crate::visitor::contains_type_parameters(self.interner, inst_idx)
                {
                    return self.interner.index_access(inst_obj, inst_idx);
                }
                if self.preserve_meta_types
                    || index_access_operand_needs_resolver(self.interner, inst_obj)
                    || index_access_operand_needs_resolver(self.interner, inst_idx)
                {
                    return self.interner.index_access(inst_obj, inst_idx);
                }
                // Evaluate immediately to achieve O(1) equality
                crate::evaluation::evaluate::evaluate_index_access(
                    self.interner,
                    inst_obj,
                    inst_idx,
                )
            }

            // KeyOf: instantiate the operand and evaluate immediately
            // Task #46: Meta-type reduction for O(1) equality
            TypeData::KeyOf(operand) => {
                tracing::trace!(
                    operand = operand.0,
                    operand_key = ?self.interner.lookup(*operand),
                    subst = ?self.substitution.map.iter().map(|(k, v)| (self.interner.resolve_atom_ref(*k), v.0)).collect::<Vec<_>>(),
                    "instantiate KeyOf: about to instantiate operand"
                );
                let inst_operand = self.instantiate(*operand);
                tracing::trace!(
                    operand = operand.0,
                    inst_operand = inst_operand.0,
                    inst_operand_key = ?self.interner.lookup(inst_operand),
                    has_type_params = crate::visitor::contains_type_parameters(self.interner, inst_operand),
                    "instantiate KeyOf: result"
                );
                // Don't eagerly evaluate if the operand still contains type parameters.
                // This prevents premature evaluation of `keyof T` where T is an inference
                // placeholder (e.g. during compute_contextual_types), which would resolve
                // to `keyof <constraint>` instead of waiting for T to be inferred.
                // Without this, mapped types like `{ [P in keyof T]: ... }` collapse to `{}`
                // because `keyof object` = `never`.
                if crate::visitor::contains_type_parameters(self.interner, inst_operand) {
                    return self.interner.keyof(inst_operand);
                }
                if self.preserve_meta_types {
                    return self.interner.keyof(inst_operand);
                }
                if matches!(
                    self.interner.lookup(inst_operand),
                    Some(
                        TypeData::TypeQuery(_)
                            | TypeData::Lazy(_)
                            | TypeData::Application(_)
                            | TypeData::IndexAccess(_, _)
                    )
                ) {
                    return self.interner.keyof(inst_operand);
                }
                // Union/intersection operands whose members are semantic refs
                // (`Lazy(DefId)`), generic applications, or recursive aliases
                // cannot be flattened to a finite key set by the resolver-less
                // `evaluate_keyof` reached from this instantiation path: the
                // member refs stay unresolved, so the keyof collapses to a
                // deferred, structurally-detached form that loses the source's
                // properties (and their optional/readonly modifiers) when the
                // mapped type later expands. Keep the keyof deferred over the
                // instantiated operand so the resolver-aware key extraction in
                // `extract_mapped_keys`/`collect_properties` can resolve the
                // member refs and recover the full key set. Fully concrete
                // unions/intersections (e.g. `keyof ({ a: 1 } & { b: 2 })`)
                // have no such refs and continue to evaluate eagerly below.
                if matches!(
                    self.interner.lookup(inst_operand),
                    Some(TypeData::Union(_) | TypeData::Intersection(_))
                ) && crate::type_queries::contains_lazy_or_recursive_db(
                    self.interner,
                    inst_operand,
                ) {
                    return self.interner.keyof(inst_operand);
                }
                // Evaluate immediately to expand keyof { a: 1 } -> "a"
                let result =
                    crate::evaluation::evaluate::evaluate_keyof(self.interner, inst_operand);

                // Store display alias so the formatter shows "keyof Shape" instead
                // of the expanded union. Only store when the result is non-trivial
                // and the operand is a named type (has a def-store mapping via the
                // Object/Callable shape → def reverse lookup in the formatter).
                if result != TypeId::NEVER && !result.is_intrinsic() {
                    let keyof_type = self.interner.keyof(inst_operand);
                    if result != keyof_type {
                        self.interner.store_display_alias(result, keyof_type);
                    }
                }

                result
            }

            // ReadonlyType: instantiate the operand
            TypeData::ReadonlyType(operand) => {
                let inst_operand = self.instantiate(*operand);
                self.interner.readonly_type(inst_operand)
            }

            // NoInfer: preserve wrapper, instantiate inner
            TypeData::NoInfer(inner) => {
                let inst_inner = self.instantiate(*inner);
                self.interner.no_infer(inst_inner)
            }

            // Template literal: instantiate embedded types
            // After substitution, if any type span becomes a union of string literals,
            // we trigger evaluation to expand the template literal into a union of strings.
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(*spans);
                let mut instantiated: Vec<TemplateSpan> = Vec::with_capacity(spans.len());
                let mut needs_evaluation = false;

                for span in spans.iter() {
                    match span {
                        TemplateSpan::Text(t) => instantiated.push(TemplateSpan::Text(*t)),
                        TemplateSpan::Type(t) => {
                            let inst_type = self.instantiate(*t);
                            // Check if this type became something that can be evaluated:
                            // - A union of string literals
                            // - A single string literal
                            // - The string intrinsic type
                            if let Some(
                                TypeData::Union(_)
                                | TypeData::Literal(
                                    LiteralValue::String(_)
                                    | LiteralValue::Number(_)
                                    | LiteralValue::Boolean(_),
                                )
                                | TypeData::Intrinsic(
                                    IntrinsicKind::String
                                    | IntrinsicKind::Number
                                    | IntrinsicKind::Boolean,
                                ),
                            ) = self.interner.lookup(inst_type)
                            {
                                needs_evaluation = true;
                            }
                            instantiated.push(TemplateSpan::Type(inst_type));
                        }
                    }
                }

                let template_type = self.interner.template_literal(instantiated);

                // If we detected types that can be evaluated, trigger evaluation
                // to potentially expand the template literal to a union of string literals
                if needs_evaluation {
                    crate::evaluation::evaluate::evaluate_type(self.interner, template_type)
                } else {
                    template_type
                }
            }

            // StringIntrinsic: instantiate the type argument
            // After substitution, if the type argument becomes a concrete type that can
            // be evaluated (like a string literal or union), trigger evaluation.
            TypeData::StringIntrinsic { kind, type_arg } => {
                let inst_arg = self.instantiate(*type_arg);
                let string_intrinsic = self.interner.string_intrinsic(*kind, inst_arg);

                // Check if we can evaluate the result
                if let Some(key) = self.interner.lookup(inst_arg) {
                    match key {
                        TypeData::Union(_)
                        | TypeData::Literal(LiteralValue::String(_))
                        | TypeData::TemplateLiteral(_)
                        | TypeData::Intrinsic(IntrinsicKind::String) => {
                            crate::evaluation::evaluate::evaluate_type(
                                self.interner,
                                string_intrinsic,
                            )
                        }
                        _ => string_intrinsic,
                    }
                } else {
                    string_intrinsic
                }
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
mod display_properties;
mod homomorphic;
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
