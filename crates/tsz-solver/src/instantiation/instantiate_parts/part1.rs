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
        for type_param in type_params.as_deref().unwrap_or(&sig.type_params) {
            self.local_type_params
                .push((type_param.name, self.interner.type_param(*type_param)));
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
}
