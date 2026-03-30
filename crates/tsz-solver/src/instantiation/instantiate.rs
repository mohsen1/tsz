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

use crate::TypeDatabase;
#[cfg(test)]
use crate::types::*;
use crate::types::{
    CallSignature, CallableShape, ConditionalType, FunctionShape, IndexSignature, IntrinsicKind,
    LiteralValue, MappedType, ObjectShape, ParamInfo, PropertyInfo, TemplateSpan, TupleElement,
    TypeData, TypeId, TypeParamInfo, TypePredicate,
};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;

/// Maximum depth for recursive type instantiation.
pub const MAX_INSTANTIATION_DEPTH: u32 = 50;

/// A substitution map from type parameter names to concrete types.
#[derive(Clone, Debug, Default)]
pub struct TypeSubstitution {
    /// Maps type parameter names to their substituted types
    map: FxHashMap<Atom, TypeId>,
}

impl TypeSubstitution {
    /// Create an empty substitution.
    pub fn new() -> Self {
        Self {
            map: FxHashMap::default(),
        }
    }

    /// Clear the substitution for reuse, preserving allocated capacity.
    #[inline]
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Create a substitution from type parameters and arguments.
    ///
    /// `type_params` - The declared type parameters (e.g., `<T, U>`)
    /// `type_args` - The provided type arguments (e.g., `<string, number>`)
    ///
    /// When `type_args` has fewer elements than `type_params`, default values
    /// from the type parameters are used for the remaining parameters.
    ///
    /// IMPORTANT: Defaults may reference earlier type parameters, so they need
    /// to be instantiated with the substitution built so far.
    pub fn from_args(
        interner: &dyn TypeDatabase,
        type_params: &[TypeParamInfo],
        type_args: &[TypeId],
    ) -> Self {
        let mut map = FxHashMap::default();
        for (i, param) in type_params.iter().enumerate() {
            let type_id = if i < type_args.len() {
                type_args[i]
            } else {
                // Use default value if type argument not provided
                match param.default {
                    Some(default) => {
                        // Defaults may reference earlier type parameters, so instantiate them
                        let resolved = if i > 0 && !map.is_empty() {
                            let subst = Self { map: map.clone() };
                            instantiate_type(interner, default, &subst)
                        } else {
                            default
                        };
                        // Circular default detection: if the resolved default is (or
                        // contains) the type parameter itself, fall back to `any`.
                        // This matches tsc behavior for `type T<X extends C = X>`.
                        if type_references_param(interner, resolved, param.name) {
                            TypeId::ANY
                        } else {
                            resolved
                        }
                    }
                    None => {
                        // No default and no argument - leave this parameter unsubstituted
                        // It will remain as a TypeParameter in the result
                        continue;
                    }
                }
            };
            map.insert(param.name, type_id);
        }
        Self { map }
    }

    /// Add a single substitution.
    pub fn insert(&mut self, name: Atom, type_id: TypeId) {
        self.map.insert(name, type_id);
    }

    /// Look up a substitution.
    pub fn get(&self, name: Atom) -> Option<TypeId> {
        self.map.get(&name).copied()
    }

    /// Check if substitution is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Number of substitutions.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Check if this substitution is an identity mapping (every type parameter maps to itself).
    ///
    /// **WARNING**: This check only verifies name equality, which is insufficient when
    /// different type parameters share the same name (e.g., alias `T` vs function
    /// `T extends object`). Callers that have access to the original `TypeParamInfo`
    /// (like `instantiate_generic`) should use `is_identity_for` instead.
    pub fn is_identity(&self, interner: &dyn TypeDatabase) -> bool {
        self.map.iter().all(|(&name, &type_id)| {
            // Check that the substitution maps a name to the UNCONSTRAINED
            // TypeParameter with that name. If the target has a constraint or
            // default, it may differ from the TypeParameter in the body being
            // instantiated, so the substitution is NOT identity.
            //
            // This prevents false identity when a type alias's unconstrained `T`
            // (the body) would be substituted with a constrained `T extends C`
            // (from the caller). Without this check, `{ "T" → T_constrained }`
            // would be treated as identity, leaving the body's unconstrained T
            // unsubstituted.
            if let Some(TypeData::TypeParameter(info)) = interner.lookup(type_id) {
                info.name == name && info.constraint.is_none() && info.default.is_none()
            } else {
                false
            }
        })
    }

    /// Check if this substitution is an identity mapping against specific type parameters.
    ///
    /// Unlike `is_identity`, this correctly handles same-name type parameters by
    /// comparing the interned TypeId of each declared type parameter against the
    /// substituted value. This is the only correct identity check when the body
    /// being instantiated may use different type parameters than the substitution source.
    pub fn is_identity_for(
        &self,
        interner: &dyn TypeDatabase,
        type_params: &[TypeParamInfo],
    ) -> bool {
        type_params.iter().all(|param| {
            match self.map.get(&param.name) {
                Some(&type_id) => interner.type_param(*param) == type_id,
                None => true, // unmapped params don't change anything
            }
        })
    }

    /// Get a reference to the internal substitution map.
    ///
    /// This is useful for building new substitutions based on existing ones.
    pub const fn map(&self) -> &FxHashMap<Atom, TypeId> {
        &self.map
    }
}

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
    depth: u32,
    max_depth: u32,
    depth_exceeded: bool,
}

impl<'a> TypeInstantiator<'a> {
    /// Create a new instantiator.
    pub fn new(interner: &'a dyn TypeDatabase, substitution: &'a TypeSubstitution) -> Self {
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
            depth: 0,
            max_depth: MAX_INSTANTIATION_DEPTH,
            depth_exceeded: false,
        }
    }

    fn is_shadowed(&self, name: Atom) -> bool {
        self.shadowed.contains(&name)
    }

    /// Extract the element type from an array-like type (Array, ReadonlyType(Array),
    /// or ReadonlyArray as `ObjectWithIndex`). Returns None if not array-like.
    fn extract_array_element(interner: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
        match interner.lookup(type_id) {
            Some(TypeData::Array(element_type)) => Some(element_type),
            Some(TypeData::ReadonlyType(inner)) => {
                let inner_resolved = crate::evaluation::evaluate::evaluate_type(interner, inner);
                if let Some(TypeData::Array(element_type)) = interner.lookup(inner_resolved) {
                    Some(element_type)
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
                    .map(|idx| idx.value_type)
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
            .any(|candidate| match interner.lookup(candidate) {
                Some(TypeData::IndexAccess(obj, idx)) if obj == source_obj => {
                    matches!(
                        interner.lookup(idx),
                        Some(TypeData::TypeParameter(info)) if info.name == param_name
                    )
                }
                _ => false,
            })
    }

    /// Instantiate a slice of properties by substituting type IDs.
    fn instantiate_properties(&mut self, properties: &[PropertyInfo]) -> Vec<PropertyInfo> {
        properties
            .iter()
            .map(|p| PropertyInfo {
                name: p.name,
                type_id: self.instantiate(p.type_id),
                write_type: self.instantiate(p.write_type),
                optional: p.optional,
                readonly: p.readonly,
                is_method: p.is_method,
                is_class_prototype: p.is_class_prototype,
                visibility: p.visibility,
                parent_id: p.parent_id,
                declaration_order: p.declaration_order,
                is_string_named: p.is_string_named,
            })
            .collect()
    }

    /// Instantiate an optional index signature.
    fn instantiate_index_signature(
        &mut self,
        idx: Option<&IndexSignature>,
    ) -> Option<IndexSignature> {
        idx.map(|idx| IndexSignature {
            key_type: self.instantiate(idx.key_type),
            value_type: self.instantiate(idx.value_type),
            readonly: idx.readonly,
            param_name: idx.param_name,
        })
    }

    /// Instantiate type parameter constraints and defaults.
    fn instantiate_type_params(&mut self, type_params: &[TypeParamInfo]) -> Vec<TypeParamInfo> {
        let saved_preserve_unsubstituted = self.preserve_unsubstituted_type_params;
        self.preserve_unsubstituted_type_params = true;
        let instantiated = type_params
            .iter()
            .map(|tp| TypeParamInfo {
                is_const: false,
                name: tp.name,
                constraint: tp.constraint.map(|c| self.instantiate(c)),
                default: tp.default.map(|d| self.instantiate(d)),
            })
            .collect();
        self.preserve_unsubstituted_type_params = saved_preserve_unsubstituted;
        instantiated
    }

    /// Instantiate function/signature parameters.
    fn instantiate_params(&mut self, params: &[ParamInfo]) -> Vec<ParamInfo> {
        params
            .iter()
            .map(|p| ParamInfo {
                name: p.name,
                type_id: self.instantiate(p.type_id),
                optional: p.optional,
                rest: p.rest,
            })
            .collect()
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

        self.depth += 1;
        let result = stacker::maybe_grow(256 * 1024, 2 * 1024 * 1024, || {
            self.instantiate_inner(type_id)
        });
        self.depth -= 1;
        result
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

        // Mark as visiting (with original ID as placeholder for cycles)
        self.visiting.insert(type_id, type_id);

        let result = self.instantiate_key(&key);

        // Update the cache with the actual result
        self.visiting.insert(type_id, result);

        result
    }

    /// Instantiate a call signature.
    fn instantiate_call_signature(&mut self, sig: &CallSignature) -> CallSignature {
        let (shadowed_len, saved_visiting) = self.enter_shadowing_scope(&sig.type_params);

        let type_params = self.instantiate_type_params(&sig.type_params);
        let local_start = self.local_type_params.len();
        for type_param in &type_params {
            self.local_type_params
                .push((type_param.name, self.interner.type_param(*type_param)));
        }
        let type_predicate = sig
            .type_predicate
            .as_ref()
            .map(|predicate| self.instantiate_type_predicate(predicate));
        let this_type = sig.this_type.map(|type_id| self.instantiate(type_id));
        let params = self.instantiate_params(&sig.params);
        let return_type = self.instantiate(sig.return_type);
        self.local_type_params.truncate(local_start);

        self.exit_shadowing_scope(shadowed_len, saved_visiting);

        CallSignature {
            type_params,
            params,
            this_type,
            return_type,
            type_predicate,
            is_method: sig.is_method,
        }
    }

    fn instantiate_type_predicate(&mut self, predicate: &TypePredicate) -> TypePredicate {
        TypePredicate {
            asserts: predicate.asserts,
            target: predicate.target,
            type_id: predicate.type_id.map(|type_id| self.instantiate(type_id)),
            parameter_index: predicate.parameter_index,
        }
    }

    /// Instantiate a `TypeData`.
    fn instantiate_key(&mut self, key: &TypeData) -> TypeId {
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
                    return self.interner.intern(*key);
                }
                if let Some(substituted) = self.substitution.get(info.name) {
                    tracing::trace!(
                        name = ?self.interner.resolve_atom_ref(info.name),
                        substituted = substituted.0,
                        "instantiate TypeParameter: SUBSTITUTED"
                    );
                    substituted
                } else {
                    if !self.preserve_unsubstituted_type_params {
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
                    // No substitution and no instantiated constraint, return original
                    self.interner.intern(*key)
                }
            }

            // Intrinsics don't change
            TypeData::Intrinsic(_) | TypeData::Literal(_) | TypeData::Error => {
                self.interner.intern(*key)
            }

            // Lazy types might resolve to something that needs substitution
            TypeData::Lazy(_)
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
                    self.interner.union(instantiated)
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
                    self.interner.intersection(instantiated)
                } else {
                    self.interner.intern(*key)
                }
            }

            // Array: instantiate element type
            TypeData::Array(elem) => {
                let instantiated_elem = self.instantiate(*elem);
                self.interner.array(instantiated_elem)
            }

            // Tuple: instantiate all elements, flattening variadic spreads.
            // When a rest element `...T` is instantiated and T resolves to a
            // tuple type `[A, B, C]`, the spread is flattened into individual
            // elements `A, B, C` (matching tsc's instantiateMappedTupleType
            // behavior for variadic tuple types).
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(*elements);
                let mut instantiated: Vec<TupleElement> = Vec::with_capacity(elements.len());
                for e in elements.iter() {
                    let inst_type = self.instantiate(e.type_id);
                    if e.rest {
                        // Check if the instantiated type is a tuple — if so,
                        // flatten its elements into the parent tuple.
                        if let Some(TypeData::Tuple(inner_elems)) = self.interner.lookup(inst_type)
                        {
                            let inner = self.interner.tuple_list(inner_elems);
                            for ie in inner.iter() {
                                instantiated.push(TupleElement {
                                    type_id: ie.type_id,
                                    name: ie.name,
                                    optional: ie.optional,
                                    rest: ie.rest,
                                });
                            }
                        } else {
                            instantiated.push(TupleElement {
                                type_id: inst_type,
                                name: e.name,
                                optional: e.optional,
                                rest: true,
                            });
                        }
                    } else {
                        instantiated.push(TupleElement {
                            type_id: inst_type,
                            name: e.name,
                            optional: e.optional,
                            rest: false,
                        });
                    }
                }
                self.interner.tuple(instantiated)
            }

            // Object: instantiate all property types
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                let instantiated = self.instantiate_properties(&shape.properties);
                self.interner
                    .object_with_flags_and_symbol(instantiated, shape.flags, shape.symbol)
            }

            // Object with index signatures: instantiate all types
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                let instantiated_props = self.instantiate_properties(&shape.properties);
                let instantiated_string_idx =
                    self.instantiate_index_signature(shape.string_index.as_ref());
                let instantiated_number_idx =
                    self.instantiate_index_signature(shape.number_index.as_ref());
                self.interner.object_with_index(ObjectShape {
                    flags: shape.flags,
                    properties: instantiated_props,
                    string_index: instantiated_string_idx,
                    number_index: instantiated_number_idx,
                    symbol: shape.symbol,
                })
            }

            // Function: instantiate params and return type
            // Note: Type params in the function create a new scope - don't substitute those
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(*shape_id);
                let (shadowed_len, saved_visiting) = self.enter_shadowing_scope(&shape.type_params);

                let instantiated_type_params = self.instantiate_type_params(&shape.type_params);
                let local_start = self.local_type_params.len();
                for type_param in &instantiated_type_params {
                    self.local_type_params
                        .push((type_param.name, self.interner.type_param(*type_param)));
                }
                let type_predicate = shape
                    .type_predicate
                    .as_ref()
                    .map(|predicate| self.instantiate_type_predicate(predicate));
                let this_type = shape.this_type.map(|type_id| self.instantiate(type_id));
                let instantiated_params = self.instantiate_params(&shape.params);
                let instantiated_return = self.instantiate(shape.return_type);
                self.local_type_params.truncate(local_start);

                self.exit_shadowing_scope(shadowed_len, saved_visiting);

                self.interner.function(FunctionShape {
                    type_params: instantiated_type_params,
                    params: instantiated_params,
                    this_type,
                    return_type: instantiated_return,
                    type_predicate,
                    is_constructor: shape.is_constructor,
                    is_method: shape.is_method,
                })
            }

            // Callable: instantiate all signatures and properties
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(*shape_id);
                let instantiated_call: Vec<CallSignature> = shape
                    .call_signatures
                    .iter()
                    .map(|sig| self.instantiate_call_signature(sig))
                    .collect();
                let instantiated_construct: Vec<CallSignature> = shape
                    .construct_signatures
                    .iter()
                    .map(|sig| self.instantiate_call_signature(sig))
                    .collect();
                let instantiated_props = self.instantiate_properties(&shape.properties);

                self.interner.callable(CallableShape {
                    call_signatures: instantiated_call,
                    construct_signatures: instantiated_construct,
                    properties: instantiated_props,
                    string_index: shape.string_index,
                    number_index: shape.number_index,
                    symbol: shape.symbol,
                    is_abstract: shape.is_abstract,
                })
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
                            let instantiated =
                                instantiate_type(self.interner, cond_type, &member_subst);
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
                    if let Some(TypeData::Union(members)) = self.interner.lookup(substituted) {
                        let members = self.interner.type_list(members);
                        // Limit distribution to prevent OOM with large unions
                        // (e.g., string literal unions with thousands of members)
                        const MAX_DISTRIBUTION_SIZE: usize = 100;
                        if members.len() > MAX_DISTRIBUTION_SIZE {
                            self.depth_exceeded = true;
                            return TypeId::ERROR;
                        }
                        let cond_type = self.interner.conditional(cond);
                        let mut results = Vec::with_capacity(members.len());
                        for &member in members.iter() {
                            // Check depth before each distribution step
                            if self.depth_exceeded {
                                return TypeId::ERROR;
                            }
                            let mut member_subst = self.substitution.clone();
                            member_subst.insert(info.name, member);
                            let instantiated =
                                instantiate_type(self.interner, cond_type, &member_subst);
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
                let mapped = self.interner.get_mapped(*mapped_id);
                let tp_slice = std::slice::from_ref(&mapped.type_param);
                let (shadowed_len, saved_visiting) = self.enter_shadowing_scope(tp_slice);

                // HOMOMORPHIC ARRAY/TUPLE: Check if this is `{ [K in keyof T]: Template }`
                // where T is being substituted with an array-like type. If so, produce
                // an array result directly, matching tsc's instantiateMappedArrayType.
                // This must be done BEFORE standard instantiation because instantiating
                // `keyof T` eagerly evaluates it to a flat union, losing the homomorphic
                // structure needed for array/tuple preservation.
                let has_identity_name_type = mapped.name_type.is_some_and(|name_type| {
                    matches!(
                        self.interner.lookup(name_type),
                        Some(TypeData::TypeParameter(info)) if info.name == mapped.type_param.name
                    )
                });
                if (mapped.name_type.is_none() || has_identity_name_type)
                    && let Some(TypeData::KeyOf(keyof_source)) =
                        self.interner.lookup(mapped.constraint)
                    && let Some(TypeData::TypeParameter(tp_info)) =
                        self.interner.lookup(keyof_source)
                    && Self::mapped_template_uses_source_index(
                        self.interner,
                        mapped.template,
                        keyof_source,
                        mapped.type_param.name,
                    )
                    && !self.is_shadowed(tp_info.name)
                    && let Some(substituted) = self.substitution.get(tp_info.name)
                {
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

                            let mut subst = TypeSubstitution::new();
                            subst.insert(mapped.type_param.name, TypeId::NUMBER);
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
                        // Non-array or unknown constraint: return any.
                        // tsc returns any for all homomorphic mapped types over any.
                        self.exit_shadowing_scope(shadowed_len, saved_visiting);
                        return TypeId::ANY;
                    }

                    // Check for Tuple first (tsc: instantiateMappedTupleType)
                    // Must also handle ReadonlyType wrapping Tuple
                    let tuple_source = match self.interner.lookup(resolved) {
                        Some(TypeData::Tuple(tid)) => Some(tid),
                        Some(TypeData::ReadonlyType(inner)) => {
                            let ir =
                                crate::evaluation::evaluate::evaluate_type(self.interner, inner);
                            match self.interner.lookup(ir) {
                                Some(TypeData::Tuple(tid)) => Some(tid),
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    if let Some(tuple_id) = tuple_source {
                        let elements = self.interner.tuple_list(tuple_id);
                        // Instantiate template first (substitutes T, keeps K shadowed)
                        let new_template = self.instantiate(mapped.template);
                        self.exit_shadowing_scope(shadowed_len, saved_visiting);

                        let mut new_elements = Vec::with_capacity(elements.len());
                        for (i, elem) in elements.iter().enumerate() {
                            let key_type = self.interner.literal_string(&i.to_string());
                            let mut subst = TypeSubstitution::new();
                            subst.insert(mapped.type_param.name, key_type);
                            let mapped_type = crate::evaluation::evaluate::evaluate_type(
                                self.interner,
                                instantiate_type(self.interner, new_template, &subst),
                            );

                            let final_type = if matches!(
                                mapped.optional_modifier,
                                Some(crate::types::MappedModifier::Add)
                            ) {
                                self.interner.union2(mapped_type, TypeId::UNDEFINED)
                            } else {
                                mapped_type
                            };

                            new_elements.push(crate::types::TupleElement {
                                type_id: final_type,
                                name: elem.name,
                                optional: elem.optional,
                                rest: elem.rest,
                            });
                        }

                        let tuple_type = self.interner.tuple(new_elements);
                        return if matches!(
                            mapped.readonly_modifier,
                            Some(crate::types::MappedModifier::Add)
                        ) {
                            self.interner.readonly_type(tuple_type)
                        } else {
                            tuple_type
                        };
                    }

                    // Then check for Array (tsc: instantiateMappedArrayType)
                    let array_element = Self::extract_array_element(self.interner, resolved);
                    if let Some(_element_type) = array_element {
                        // Produce array result: substitute K → number in the template
                        let new_template = self.instantiate(mapped.template);
                        self.exit_shadowing_scope(shadowed_len, saved_visiting);

                        let mut subst = TypeSubstitution::new();
                        subst.insert(mapped.type_param.name, TypeId::NUMBER);
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
                        return if matches!(
                            mapped.readonly_modifier,
                            Some(crate::types::MappedModifier::Add)
                        ) {
                            self.interner.readonly_type(array_type)
                        } else {
                            array_type
                        };
                    }

                    // PRIMITIVE PASSTHROUGH: tsc's instantiateMappedType returns the
                    // source type unchanged when it is a primitive (null, undefined,
                    // string, number, boolean, etc.). This matches the evaluate_application
                    // passthrough but also covers mapped types that appear nested inside
                    // unions or other compound types (not just at the top level of a type
                    // alias body). Without this, `{ [K in keyof null]: null[K] }` inside
                    // a union evaluates to `{}` instead of `null`.
                    if crate::visitors::visitor_predicates::is_primitive_type(
                        self.interner,
                        resolved,
                    ) {
                        self.exit_shadowing_scope(shadowed_len, saved_visiting);
                        return resolved;
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
                let new_constraint = self.instantiate(mapped.constraint);
                let new_template = self.instantiate(mapped.template);
                let new_name_type = mapped.name_type.map(|t| self.instantiate(t));
                let new_param_constraint =
                    mapped.type_param.constraint.map(|c| self.instantiate(c));
                let new_param_default = mapped.type_param.default.map(|d| self.instantiate(d));

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
                // However, skip eager evaluation when the template is a Conditional whose
                // extends_type is directly a Lazy(DefId) reference. This pattern occurs in
                // mapped type key filters like `T[K] extends Function ? never : K`, where
                // `Function` is a global lib interface referenced as Lazy(DefId). The
                // instantiator's NoopResolver can't resolve these references, causing the
                // conditional's subtype check to always fail and incorrectly accept all keys.
                // The checker will re-evaluate later with a proper resolver.
                //
                // We check for direct Lazy (not contained-in) to avoid matching cases like
                // `undefined extends T[P] ? never : P` where the extends_type is an
                // IndexAccess that may contain Lazy internally but can be evaluated.
                let mapped_type = self.interner.mapped(instantiated);
                let has_lazy_extends = if let Some(cond) =
                    crate::type_queries::get_conditional_type(self.interner, new_template)
                {
                    matches!(
                        self.interner.lookup(cond.extends_type),
                        Some(crate::types::TypeData::Lazy(_))
                    )
                } else {
                    false
                };
                // Also skip eager evaluation when the template contains Application
                // types whose base is a Lazy(DefId) reference (e.g. recursive type
                // aliases like `Spec<T[P]>`).  The instantiator's NoopResolver cannot
                // resolve these references, so the evaluator would silently drop
                // unresolvable union members.  Deferring lets the outer evaluator
                // (which has a proper TypeResolver) handle the full expansion.
                let has_lazy_application =
                    template_has_lazy_application_in_composite(self.interner, new_template);
                let resolver_dependent_constraint =
                    mapped_constraint_needs_resolver(self.interner, new_constraint);
                if self.preserve_meta_types
                    || has_lazy_extends
                    || has_lazy_application
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
                let inst_obj = self.instantiate(*obj);
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
                if self.preserve_meta_types {
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
                // Evaluate immediately to expand keyof { a: 1 } -> "a"
                crate::evaluation::evaluate::evaluate_keyof(self.interner, inst_operand)
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
                self.interner.infer(*info)
            }
        }
    }
}

/// Convenience function for instantiating a type with a substitution.
#[inline]
pub fn instantiate_type(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    // Fast path: intrinsic types never need instantiation
    if type_id.is_intrinsic() {
        return type_id;
    }
    match interner.lookup(type_id) {
        // Fast path: TypeParameter directly in the substitution — return immediately.
        // This is the most common leaf case in mapped type template instantiation.
        #[allow(clippy::collapsible_if)]
        Some(TypeData::TypeParameter(info)) => {
            if let Some(result) = substitution.get(info.name) {
                return result;
            }
        }
        // Fast path: IndexAccess(T, P) — the most common mapped type template pattern.
        // Recursively instantiate obj and idx without creating a TypeInstantiator.
        Some(TypeData::IndexAccess(obj, idx)) => {
            let new_obj = instantiate_type(interner, obj, substitution);
            let new_idx = instantiate_type(interner, idx, substitution);
            if new_obj == obj && new_idx == idx {
                return type_id;
            }
            return interner.index_access(new_obj, new_idx);
        }
        _ => {}
    }
    instantiate_type_with_depth_status(interner, type_id, substitution).0
}

/// Instantiate a type while preserving unsubstituted type parameters.
///
/// Unlike `instantiate_type`, this does NOT fall back to replacing type
/// parameters with their instantiated constraints when they are not in the
/// substitution map. This is needed when instantiating mapped type bodies
/// (constraint + template) with the outer type arguments, so that the mapped
/// key parameter (e.g., `P` from `[P in keyof T]: T[P]`) stays as a type
/// parameter instead of being collapsed to its constraint.
pub fn instantiate_type_preserving(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    if type_id.is_intrinsic() {
        return type_id;
    }
    if substitution.is_empty() || substitution.is_identity(interner) {
        return type_id;
    }
    let mut instantiator = TypeInstantiator::new(interner, substitution);
    instantiator.preserve_unsubstituted_type_params = true;
    let result = instantiator.instantiate(type_id);
    if instantiator.depth_exceeded {
        TypeId::ERROR
    } else {
        result
    }
}

/// Instantiate a type and report whether instantiation depth overflowed.
pub fn instantiate_type_with_depth_status(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> (TypeId, bool) {
    if substitution.is_empty() || substitution.is_identity(interner) {
        return (type_id, false);
    }
    let mut instantiator = TypeInstantiator::new(interner, substitution);
    let result = instantiator.instantiate(type_id);
    if instantiator.depth_exceeded {
        (TypeId::ERROR, true)
    } else {
        (result, false)
    }
}

/// Convenience function for instantiating a type while preserving meta-type
/// structure such as `keyof`, index access, and mapped types.
///
/// This is used when callers need to inspect whether an instantiated type still
/// structurally depends on a nominal symbol before a later evaluation pass can
/// safely reduce it.
pub fn instantiate_type_preserving_meta(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    if substitution.is_empty() || substitution.is_identity(interner) {
        return type_id;
    }
    let mut instantiator = TypeInstantiator::new(interner, substitution);
    instantiator.preserve_meta_types = true;
    let result = instantiator.instantiate(type_id);
    if instantiator.depth_exceeded {
        TypeId::ERROR
    } else {
        result
    }
}

/// Convenience function for instantiating a type while substituting infer variables.
pub fn instantiate_type_with_infer(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    if substitution.is_empty() || substitution.is_identity(interner) {
        return type_id;
    }
    let mut instantiator = TypeInstantiator::new(interner, substitution);
    instantiator.substitute_infer = true;
    let result = instantiator.instantiate(type_id);
    if instantiator.depth_exceeded {
        TypeId::ERROR
    } else {
        result
    }
}

/// Convenience function for instantiating a generic type with type arguments.
///
/// Fill in type parameter defaults for an application's args when fewer args
/// are provided than parameters exist. Returns `None` if any missing arg has
/// no default. Defaults that reference earlier type parameters are properly
/// instantiated via `TypeSubstitution::from_args`.
///
/// Example: `Generator<T>` with params `[T, TReturn=any, TNext=unknown]`
/// returns `Some([T, any, unknown])`.
pub fn fill_application_defaults(
    interner: &dyn TypeDatabase,
    args: &[TypeId],
    type_params: &[TypeParamInfo],
) -> Option<Vec<TypeId>> {
    if args.len() >= type_params.len() {
        return Some(args[..type_params.len()].to_vec());
    }
    let subst = TypeSubstitution::from_args(interner, type_params, args);
    let mut result = Vec::with_capacity(type_params.len());
    for (i, param) in type_params.iter().enumerate() {
        if i < args.len() {
            result.push(args[i]);
        } else if let Some(resolved) = subst.get(param.name) {
            result.push(resolved);
        } else {
            return None;
        }
    }
    Some(result)
}

/// Uses `is_identity_for` instead of the name-only `is_identity` check to
/// correctly handle same-name type parameters from different scopes (e.g.,
/// alias `T` vs function `T extends object`).
pub fn instantiate_generic(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    type_params: &[TypeParamInfo],
    type_args: &[TypeId],
) -> TypeId {
    if type_params.is_empty() || type_args.is_empty() {
        return type_id;
    }
    let substitution = TypeSubstitution::from_args(interner, type_params, type_args);
    if substitution.is_empty() || substitution.is_identity_for(interner, type_params) {
        return type_id;
    }
    let mut instantiator = TypeInstantiator::new(interner, &substitution);
    let result = instantiator.instantiate(type_id);
    if instantiator.depth_exceeded {
        TypeId::ERROR
    } else {
        result
    }
}

/// Substitute `ThisType` with a concrete type throughout a type.
///
/// Used for method call return types where `this` refers to the receiver's type.
/// For example, in a fluent builder pattern:
/// ```typescript
/// class Builder { setName(n: string): this { ... } }
/// const b: Builder = new Builder().setName("foo"); // this → Builder
/// ```
pub fn substitute_this_type(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    this_type: TypeId,
) -> TypeId {
    // Quick check: if the type is intrinsic, no substitution needed
    if type_id.is_intrinsic() {
        return type_id;
    }
    let empty_subst = TypeSubstitution::new();
    let mut instantiator = TypeInstantiator::new(interner, &empty_subst);
    instantiator.this_type = Some(this_type);
    // Preserve type parameters during this-only substitution. Without this,
    // the instantiator's constraint fallback would collapse type parameters
    // to their constraints when the constraint contains ThisType.
    // Example: T extends A where A has self(): this — substitute_this_type(T, T)
    // would walk into T's constraint A, find ThisType, replace it, and return
    // the modified constraint instead of T.
    instantiator.preserve_unsubstituted_type_params = true;
    let result = instantiator.instantiate(type_id);
    if instantiator.depth_exceeded {
        TypeId::ERROR
    } else {
        result
    }
}

/// Check whether a mapped-type template is a **union or intersection** that
/// contains an `Application` type whose base is a `Lazy(DefId)` reference.
///
/// This pattern occurs in recursive mapped types like:
///   `Spec<T> = { [P in keyof T]: Func<T[P]> | Spec<T[P]> }`
/// where the template union includes a self-referential type alias application.
///
/// The instantiator's eager `evaluate_type` uses `NoopResolver`, which cannot
/// resolve `Lazy` references.  When a union member is an unresolvable
/// application, the mapped type evaluator produces an incomplete object that
/// silently drops that member.  Deferring lets the outer evaluator (which has
/// a proper `TypeResolver`) handle the full expansion.
///
/// We intentionally do NOT match a top-level Application (e.g. `Selector<S, T[K]>`)
/// because the evaluator correctly passes those through as-is.  Only unions/
/// intersections are at risk of member loss.
fn template_has_lazy_application_in_composite(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    let Some(data) = interner.lookup(type_id) else {
        return false;
    };
    match data {
        TypeData::Union(members) | TypeData::Intersection(members) => {
            let list = interner.type_list(members);
            list.iter().any(|&m| {
                if let Some(TypeData::Application(app_id)) = interner.lookup(m) {
                    let app = interner.type_application(app_id);
                    matches!(interner.lookup(app.base), Some(TypeData::Lazy(..)))
                } else {
                    false
                }
            })
        }
        TypeData::Conditional(cond_id) => {
            let cond = interner.get_conditional(cond_id);
            template_has_lazy_application_in_composite(interner, cond.true_type)
                || template_has_lazy_application_in_composite(interner, cond.false_type)
        }
        _ => false,
    }
}

/// Check whether a mapped constraint needs a real resolver before it can be
/// evaluated without losing key information.
///
/// The instantiator runs with `NoopResolver`, so eagerly evaluating
/// `keyof Application(...)` here can collapse a mapped type before the actual
/// alias/application body is available. Deferring lets the outer evaluator,
/// which has a real `TypeResolver`, materialize the correct key set later.
fn mapped_constraint_needs_resolver(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let key = match interner.lookup(type_id) {
        Some(key) => key,
        None => return false,
    };

    match key {
        TypeData::KeyOf(operand) => matches!(
            interner.lookup(operand),
            Some(TypeData::Application(_) | TypeData::Lazy(_) | TypeData::TypeQuery(_))
        ),
        TypeData::Application(_) | TypeData::Lazy(_) | TypeData::TypeQuery(_) => true,
        _ => false,
    }
}

/// Check whether `type_id` references a type parameter with the given `name`.
///
/// Used to detect circular type parameter defaults. When a default resolves
/// to (or contains) the parameter it is defaulting, tsc falls back to `any`.
/// This is a shallow check: it handles the direct self-reference case
/// (`type T<X extends C = X>`) and union/intersection wrappers.
fn type_references_param(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    param_name: tsz_common::interner::Atom,
) -> bool {
    match interner.lookup(type_id) {
        Some(TypeData::TypeParameter(info)) => info.name == param_name,
        Some(TypeData::Union(members_id)) | Some(TypeData::Intersection(members_id)) => {
            let members = interner.type_list(members_id);
            members
                .iter()
                .any(|&m| type_references_param(interner, m, param_name))
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "../../tests/instantiate_tests.rs"]
mod tests;
