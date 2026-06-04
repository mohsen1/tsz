impl TypeDatabase for QueryCache<'_> {
    fn intern(&self, key: TypeData) -> TypeId {
        self.interner.intern(key)
    }

    fn lookup(&self, id: TypeId) -> Option<TypeData> {
        self.interner.lookup(id)
    }

    fn lookup_alloc_order(&self, id: TypeId) -> Option<u32> {
        self.interner.lookup_alloc_order(id)
    }

    fn intern_string(&self, s: &str) -> Atom {
        self.interner.intern_string(s)
    }

    fn resolve_atom(&self, atom: Atom) -> String {
        self.interner.resolve_atom(atom)
    }

    fn resolve_atom_ref(&self, atom: Atom) -> Arc<str> {
        self.interner.resolve_atom_ref(atom)
    }

    fn type_list(&self, id: TypeListId) -> Arc<[TypeId]> {
        self.interner.type_list(id)
    }

    fn tuple_list(&self, id: TupleListId) -> Arc<[TupleElement]> {
        self.interner.tuple_list(id)
    }

    fn template_list(&self, id: TemplateLiteralId) -> Arc<[TemplateSpan]> {
        self.interner.template_list(id)
    }

    fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape> {
        self.interner.object_shape(id)
    }

    fn object_property_index(&self, shape_id: ObjectShapeId, name: Atom) -> PropertyLookup {
        self.interner.object_property_index(shape_id, name)
    }

    fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape> {
        self.interner.function_shape(id)
    }

    fn callable_shape(&self, id: CallableShapeId) -> Arc<CallableShape> {
        self.interner.callable_shape(id)
    }

    fn conditional_type(&self, id: ConditionalTypeId) -> Arc<ConditionalType> {
        self.interner.conditional_type(id)
    }

    fn mapped_type(&self, id: MappedTypeId) -> Arc<MappedType> {
        self.interner.mapped_type(id)
    }

    fn get_conditional(&self, id: ConditionalTypeId) -> ConditionalType {
        self.interner.get_conditional(id)
    }

    fn get_mapped(&self, id: MappedTypeId) -> MappedType {
        self.interner.get_mapped(id)
    }

    fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication> {
        self.interner.type_application(id)
    }

    fn literal_string(&self, value: &str) -> TypeId {
        self.interner.literal_string(value)
    }

    fn literal_number(&self, value: f64) -> TypeId {
        self.interner.literal_number(value)
    }

    fn literal_boolean(&self, value: bool) -> TypeId {
        self.interner.literal_boolean(value)
    }

    fn literal_bigint(&self, value: &str) -> TypeId {
        self.interner.literal_bigint(value)
    }

    fn literal_bigint_with_sign(&self, negative: bool, digits: &str) -> TypeId {
        self.interner.literal_bigint_with_sign(negative, digits)
    }

    fn union(&self, members: Vec<TypeId>) -> TypeId {
        self.interner.union(members)
    }

    fn union_from_slice(&self, members: &[TypeId]) -> TypeId {
        self.interner.union_from_slice(members)
    }

    fn union_literal_reduce(&self, members: Vec<TypeId>) -> TypeId {
        self.interner.union_literal_reduce(members)
    }

    fn union_from_sorted_vec(&self, flat: Vec<TypeId>) -> TypeId {
        self.interner.union_from_sorted_vec(flat)
    }

    fn union2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.interner.union2(left, right)
    }

    fn union3(&self, first: TypeId, second: TypeId, third: TypeId) -> TypeId {
        self.interner.union3(first, second, third)
    }

    fn intersection(&self, members: Vec<TypeId>) -> TypeId {
        self.interner.intersection(members)
    }

    fn intersection2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.interner.intersection2(left, right)
    }

    fn intersect_types_raw2(&self, left: TypeId, right: TypeId) -> TypeId {
        self.interner.intersect_types_raw2(left, right)
    }

    fn array(&self, element: TypeId) -> TypeId {
        self.interner.array(element)
    }

    fn tuple(&self, elements: Vec<TupleElement>) -> TypeId {
        self.interner.tuple(elements)
    }

    fn object(&self, properties: Vec<PropertyInfo>) -> TypeId {
        self.interner.object(properties)
    }

    fn object_with_flags(&self, properties: Vec<PropertyInfo>, flags: ObjectFlags) -> TypeId {
        self.interner.object_with_flags(properties, flags)
    }

    fn object_with_flags_and_symbol(
        &self,
        properties: Vec<PropertyInfo>,
        flags: ObjectFlags,
        symbol: Option<SymbolId>,
    ) -> TypeId {
        self.interner
            .object_with_flags_and_symbol(properties, flags, symbol)
    }

    fn object_with_index(&self, shape: ObjectShape) -> TypeId {
        self.interner.object_with_index(shape)
    }

    fn object_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId {
        self.interner.object_type_from_shape(shape_id)
    }

    fn object_with_index_type_from_shape(&self, shape_id: ObjectShapeId) -> TypeId {
        self.interner.object_with_index_type_from_shape(shape_id)
    }

    fn function(&self, shape: FunctionShape) -> TypeId {
        self.interner.function(shape)
    }

    fn callable(&self, shape: CallableShape) -> TypeId {
        self.interner.callable(shape)
    }

    fn template_literal(&self, spans: Vec<TemplateSpan>) -> TypeId {
        self.interner.template_literal(spans)
    }

    fn conditional(&self, conditional: ConditionalType) -> TypeId {
        self.interner.conditional(conditional)
    }

    fn mapped(&self, mapped: MappedType) -> TypeId {
        self.interner.mapped(mapped)
    }

    fn reference(&self, symbol: SymbolRef) -> TypeId {
        self.interner.reference(symbol)
    }

    fn lazy(&self, def_id: DefId) -> TypeId {
        self.interner.lazy(def_id)
    }

    fn bound_parameter(&self, index: u32) -> TypeId {
        self.interner.bound_parameter(index)
    }

    fn recursive(&self, depth: u32) -> TypeId {
        self.interner.recursive(depth)
    }

    fn type_param(&self, info: TypeParamInfo) -> TypeId {
        self.interner.type_param(info)
    }

    fn unresolved_type_name(&self, name: Atom) -> TypeId {
        self.interner.unresolved_type_name(name)
    }

    fn type_query(&self, symbol: SymbolRef) -> TypeId {
        self.interner.type_query(symbol)
    }

    fn enum_type(&self, def_id: DefId, structural_type: TypeId) -> TypeId {
        self.interner.enum_type(def_id, structural_type)
    }

    fn application(&self, base: TypeId, args: Vec<TypeId>) -> TypeId {
        self.interner.application(base, args)
    }

    fn literal_string_atom(&self, atom: Atom) -> TypeId {
        self.interner.literal_string_atom(atom)
    }

    fn union_preserve_members(&self, members: Vec<TypeId>) -> TypeId {
        self.interner.union_preserve_members(members)
    }

    fn readonly_type(&self, inner: TypeId) -> TypeId {
        self.interner.readonly_type(inner)
    }

    fn keyof(&self, inner: TypeId) -> TypeId {
        self.interner.keyof(inner)
    }

    fn index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
        self.interner.index_access(object_type, index_type)
    }

    fn this_type(&self) -> TypeId {
        self.interner.this_type()
    }

    fn no_infer(&self, inner: TypeId) -> TypeId {
        self.interner.no_infer(inner)
    }

    fn unique_symbol(&self, symbol: SymbolRef) -> TypeId {
        self.interner.unique_symbol(symbol)
    }

    fn infer(&self, info: TypeParamInfo) -> TypeId {
        self.interner.infer(info)
    }

    fn string_intrinsic(&self, kind: StringIntrinsicKind, type_arg: TypeId) -> TypeId {
        self.interner.string_intrinsic(kind, type_arg)
    }

    fn get_class_base_type(&self, symbol_id: SymbolId) -> Option<TypeId> {
        // Delegate to the interner
        self.interner.get_class_base_type(symbol_id)
    }

    fn is_identity_comparable_type(&self, type_id: TypeId) -> bool {
        self.interner.is_identity_comparable_type(type_id)
    }

    fn get_array_base_type(&self) -> Option<TypeId> {
        self.interner.get_array_base_type()
    }

    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        self.interner.get_array_base_type_params()
    }

    fn get_array_display_base_type(&self) -> Option<TypeId> {
        self.interner.get_array_display_base_type()
    }

    fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        self.interner.get_readonly_array_base_type()
    }

    fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        self.interner.get_boxed_type(kind)
    }

    fn is_boxed_def_id(&self, def_id: DefId, kind: IntrinsicKind) -> bool {
        self.interner.is_boxed_def_id(def_id, kind)
    }

    fn is_this_type_marker_def_id(&self, def_id: DefId) -> bool {
        self.interner.is_this_type_marker_def_id(def_id)
    }

    fn consume_evaluation_fuel(&self, amount: u32) -> bool {
        self.interner.consume_evaluation_fuel(amount)
    }

    fn is_evaluation_fuel_exhausted(&self) -> bool {
        self.interner.is_evaluation_fuel_exhausted()
    }
}

/// Implement `TypeResolver` for `QueryCache` with noop resolution.
///
/// `QueryCache` doesn't have access to the Binder or type environment,
/// so it cannot resolve symbol references or `DefIds`. Only `resolve_ref`
/// (required) is explicitly implemented; all other resolution methods
/// inherit the trait's default `None`/`false` behavior. The three boxed/array
/// methods delegate to the underlying interner.
impl TypeResolver for QueryCache<'_> {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        self.interner.get_boxed_type(kind)
    }

    fn get_array_base_type(&self) -> Option<TypeId> {
        self.interner.get_array_base_type()
    }

    fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        self.interner.get_array_base_type_params()
    }

    fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        self.interner.get_readonly_array_base_type()
    }
}

impl QueryDatabase for QueryCache<'_> {
    fn as_type_database(&self) -> &dyn TypeDatabase {
        self
    }

    fn as_type_resolver(&self) -> &dyn TypeResolver {
        self
    }

    fn fresh_type_param(&self, info: TypeParamInfo) -> TypeId {
        self.interner.fresh_type_param(info)
    }

    fn register_array_base_type(&self, type_id: TypeId, type_params: Vec<TypeParamInfo>) {
        self.interner.set_array_base_type(type_id, type_params);
    }

    fn register_array_display_base_type(&self, type_id: TypeId) {
        self.interner.set_array_display_base_type(type_id);
    }

    fn register_readonly_array_base_type(&self, type_id: TypeId) {
        self.interner.set_readonly_array_base_type(type_id);
    }

    fn register_boxed_type(&self, kind: IntrinsicKind, type_id: TypeId) {
        self.interner.set_boxed_type(kind, type_id);
    }

    fn register_boxed_def_id(&self, kind: IntrinsicKind, def_id: DefId) {
        self.interner.register_boxed_def_id(kind, def_id);
    }

    fn register_this_type_def_id(&self, def_id: DefId) {
        self.interner.register_this_type_def_id(def_id);
    }

    fn evaluate_type(&self, type_id: TypeId) -> TypeId {
        self.evaluate_type_with_options(type_id, self.no_unchecked_indexed_access())
    }

    fn evaluate_type_with_options(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
        // Fast path: intrinsic types never need evaluation
        if type_id.is_intrinsic() {
            return type_id;
        }

        let request = EvaluationRequest::new(type_id)
            .with_no_unchecked_indexed_access(no_unchecked_indexed_access);
        let key = request.cache_key();
        let cached = self.eval_cache.borrow().get(&key).copied();

        if let Some(result) = cached {
            return result;
        }

        // L2: Check shared cross-file cache before doing expensive evaluation.
        if let Some(shared) = self.shared
            && let Some(result) = shared.eval_cache.get(&key).map(|r| *r)
        {
            self.eval_cache.borrow_mut().insert(key, result);
            return result;
        }

        // Fast path: leaf types that never change during evaluation.
        // Skip TypeEvaluator creation for types where visit_type_key returns type_id unchanged.
        if let Some(
            TypeData::Literal(_)
            | TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::Array(_)
            | TypeData::Function(_)
            | TypeData::Callable(_)
            | TypeData::TypeParameter(_)
            | TypeData::UnresolvedTypeName(_)
            | TypeData::Infer(_)
            | TypeData::Enum(_, _)
            | TypeData::BoundParameter(_)
            | TypeData::Recursive(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::ReadonlyType(_)
            | TypeData::Error,
        ) = self.interner.lookup(type_id)
        {
            self.eval_cache.borrow_mut().insert(key, type_id);
            return type_id;
        }

        let trace_enabled = query_trace::enabled();
        let trace_query_id = trace_enabled.then(|| {
            let query_id = query_trace::next_query_id();
            query_trace::unary_start(
                query_id,
                "evaluate_type_with_options",
                type_id,
                no_unchecked_indexed_access,
            );
            query_id
        });

        let mut evaluator =
            crate::evaluation::evaluate::TypeEvaluator::new(self.as_type_database());
        evaluator = evaluator.with_query_db(self);
        let result = evaluator.evaluate_request_result(request).into_type_id();

        // PERF: Persist intermediate evaluation results from this session into
        // the long-lived eval_cache. During recursive mapped type expansion
        // (e.g., DeepPartial<T>), the evaluator computes many sub-results
        // that would otherwise be recomputed in subsequent top-level evaluate
        // calls. Only persist entries where the result differs from the input
        // (identity mappings are free to recompute) and skip intrinsics.
        {
            let mut cache = self.eval_cache.borrow_mut();
            cache.insert(key, result);
            // Also write to shared cache for cross-file benefit.
            if let Some(shared) = self.shared {
                shared.eval_cache.insert(key, result);
            }
            for (intermediate_id, intermediate_result) in evaluator.drain_cache() {
                if intermediate_id != intermediate_result && !intermediate_id.is_intrinsic() {
                    let ikey = request.with_type_id(intermediate_id).cache_key();
                    cache.entry(ikey).or_insert(intermediate_result);
                    if let Some(shared) = self.shared {
                        shared.eval_cache.entry(ikey).or_insert(intermediate_result);
                    }
                }
            }
        }

        if let Some(query_id) = trace_query_id {
            query_trace::unary_end(query_id, "evaluate_type_with_options", result, false);
        }
        result
    }

    /// Look up a cross-call `instantiate_type` result.
    ///
    /// Hit/miss counters mirror the subtype counters and feed
    /// `QueryCacheStatistics`.
    fn lookup_instantiation_cache(&self, key: &InstantiationCacheKey) -> Option<TypeId> {
        match self.instantiation_cache.lookup(key) {
            Some(result) => {
                self.instantiation_cache_hits
                    .set(self.instantiation_cache_hits.get() + 1);
                Some(result)
            }
            None => {
                self.instantiation_cache_misses
                    .set(self.instantiation_cache_misses.get() + 1);
                None
            }
        }
    }

    /// Store an `instantiate_type` result in the cross-call cache.
    fn insert_instantiation_cache(&self, key: InstantiationCacheKey, result: TypeId) {
        self.instantiation_cache.insert(key, result);
    }

    /// Look up a cached `remove_subtypes_for_bct` result. Hit/miss counters
    /// mirror the instantiation-cache counters and feed
    /// `QueryCacheStatistics`.
    fn lookup_subtype_reduction_cache(
        &self,
        key: &SubtypeReductionKey,
    ) -> Option<std::sync::Arc<[TypeId]>> {
        match self.subtype_reduction_cache.lookup(key) {
            Some(result) => {
                self.subtype_reduction_cache_hits
                    .set(self.subtype_reduction_cache_hits.get() + 1);
                Some(result)
            }
            None => {
                self.subtype_reduction_cache_misses
                    .set(self.subtype_reduction_cache_misses.get() + 1);
                None
            }
        }
    }

    /// Store a `remove_subtypes_for_bct` result in the cross-call cache.
    fn insert_subtype_reduction_cache(
        &self,
        key: SubtypeReductionKey,
        result: std::sync::Arc<[TypeId]>,
    ) {
        self.subtype_reduction_cache.insert(key, result);
    }

    fn is_subtype_of_with_policy(
        &self,
        source: TypeId,
        target: TypeId,
        policy: RelationPolicy,
    ) -> bool {
        self.is_cached_policy_relation(CachedPolicyRelation::Subtype, source, target, policy)
    }

    fn is_assignable_to_with_policy(
        &self,
        source: TypeId,
        target: TypeId,
        policy: RelationPolicy,
    ) -> bool {
        self.is_cached_policy_relation(CachedPolicyRelation::Assignability, source, target, policy)
    }

    /// Convenience wrapper for `is_subtype_of` with default flags.
    fn is_subtype_of(&self, source: TypeId, target: TypeId) -> bool {
        self.is_subtype_of_with_policy(source, target, RelationPolicy::unflagged_compatibility())
    }

    /// Convenience wrapper for `is_assignable_to` with default flags.
    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        self.is_assignable_to_with_policy(source, target, RelationPolicy::unflagged_compatibility())
    }

    fn lookup_subtype_cache(&self, key: RelationCacheKey) -> Option<bool> {
        self.lookup_policy_relation_cache(CachedPolicyRelation::Subtype, key)
    }

    fn insert_subtype_cache(&self, key: RelationCacheKey, result: bool) {
        self.insert_policy_relation_cache(CachedPolicyRelation::Subtype, key, result);
    }

    fn lookup_assignability_cache(&self, key: RelationCacheKey) -> Option<bool> {
        self.lookup_policy_relation_cache(CachedPolicyRelation::Assignability, key)
    }

    fn insert_assignability_cache(&self, key: RelationCacheKey, result: bool) {
        self.insert_policy_relation_cache(CachedPolicyRelation::Assignability, key, result);
    }

    fn lookup_intersection_merge(&self, intersection_id: TypeId) -> Option<Option<TypeId>> {
        let result = self
            .intersection_merge_cache
            .borrow()
            .get(&intersection_id)
            .copied();
        if result.is_some() {
            self.intersection_merge_cache_hits
                .set(self.intersection_merge_cache_hits.get() + 1);
        } else {
            self.intersection_merge_cache_misses
                .set(self.intersection_merge_cache_misses.get() + 1);
        }
        result
    }

    fn insert_intersection_merge(&self, intersection_id: TypeId, result: Option<TypeId>) {
        self.intersection_merge_cache
            .borrow_mut()
            .insert(intersection_id, result);
    }

    fn get_index_signatures(&self, type_id: TypeId) -> IndexInfo {
        // Delegate to the interner - caching could be added later if needed
        self.interner.get_index_signatures(type_id)
    }

    fn is_nullish_type(&self, type_id: TypeId) -> bool {
        // Delegate to the interner
        self.interner.is_nullish_type(type_id)
    }

    fn remove_nullish(&self, type_id: TypeId) -> TypeId {
        crate::narrowing::remove_nullish_query(self, type_id)
    }

    fn resolve_property_access(
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

    fn resolve_property_access_with_options(
        &self,
        object_type: TypeId,
        prop_name: &str,
        no_unchecked_indexed_access: bool,
    ) -> crate::operations::property::PropertyAccessResult {
        // QueryCache doesn't have full TypeResolver capability, so use PropertyAccessEvaluator
        // with the current QueryDatabase.
        let prop_atom = self.interner.intern_string(prop_name);
        let exact_optional_property_types =
            crate::caches::db::TypeCompilerOptions::exact_optional_property_types(self);
        let key = (
            object_type,
            prop_atom,
            no_unchecked_indexed_access,
            exact_optional_property_types,
        );
        if let Some(result) = self.check_property_cache(key) {
            return result;
        }

        let mut evaluator = crate::operations::property::PropertyAccessEvaluator::new(self);
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator.set_exact_optional_property_types(exact_optional_property_types);
        let result = evaluator.resolve_property_access(object_type, prop_name);
        self.insert_property_cache(key, result);
        result
    }

    fn resolve_any_index_access(
        &self,
        object_type: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> Option<crate::operations::property::PropertyAccessResult> {
        let exact_optional_property_types =
            crate::caches::db::TypeCompilerOptions::exact_optional_property_types(self);
        let mut evaluator = crate::operations::property::PropertyAccessEvaluator::new(self);
        evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
        evaluator.set_exact_optional_property_types(exact_optional_property_types);
        evaluator.resolve_any_index_access(object_type)
    }

    fn resolve_element_access_type(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> TypeId {
        let key = (
            object_type,
            index_type,
            literal_index.map(|idx| idx as u32),
            self.no_unchecked_indexed_access(),
        );
        if let Some(result) = self.check_element_access_cache(key) {
            return result;
        }

        let result = match self.resolve_element_access(object_type, index_type, literal_index) {
            ElementAccessResult::Success(type_id) => type_id,
            _ => TypeId::ERROR,
        };

        self.insert_element_access_cache(key, result);
        result
    }

    fn collect_object_spread_properties(&self, spread_type: TypeId) -> Vec<PropertyInfo> {
        if let Some(cached) = self.check_object_spread_properties_cache(spread_type) {
            return cached;
        }

        let mut visited: FxHashSet<TypeId> = FxHashSet::default();
        let result = self.collect_object_spread_properties_inner(spread_type, &mut visited);
        self.insert_object_spread_properties_cache(spread_type, result.clone());
        result
    }

    fn set_no_unchecked_indexed_access(&self, enabled: bool) {
        self.no_unchecked_indexed_access.set(enabled);
    }

    fn set_exact_optional_property_types(&self, enabled: bool) {
        self.exact_optional_property_types.set(enabled);
    }

    fn get_type_param_variance(&self, def_id: DefId) -> Option<Arc<[Variance]>> {
        // Session cache first (shared with the resolver-aware cached boundary).
        if let Some(cached) = self.variance_cache.borrow().get(&def_id) {
            return Some(Arc::clone(cached));
        }
        // Compute via the type's body. `self` is both db and resolver here.
        let params = self.get_lazy_type_params(def_id)?;
        if params.is_empty() {
            return None;
        }
        let body = self.resolve_lazy(def_id, self.as_type_database())?;
        let result: Arc<[Variance]> = params
            .iter()
            .map(|param| crate::relations::variance::compute_variance(self, body, param.name))
            .collect();
        self.variance_cache
            .borrow_mut()
            .insert(def_id, Arc::clone(&result));
        Some(result)
    }

    fn get_cached_type_param_variance(&self, def_id: DefId) -> Option<Arc<[Variance]>> {
        self.variance_cache.borrow().get(&def_id).map(Arc::clone)
    }

    fn insert_type_param_variance(&self, def_id: DefId, variance: Arc<[Variance]>) {
        self.variance_cache.borrow_mut().insert(def_id, variance);
    }

    fn canonical_id(&self, type_id: TypeId) -> TypeId {
        // Check cache first
        let cached = self.canonical_cache.borrow().get(&type_id).copied();

        if let Some(canonical) = cached {
            return canonical;
        }

        // Compute canonical form using a fresh Canonicalizer
        // CRITICAL: Always start with empty stacks for absolute De Bruijn indices
        // This ensures the cached TypeId represents the absolute structural form
        use crate::canonicalize::Canonicalizer;
        let mut canon = Canonicalizer::new(self.as_type_database(), self);
        let canonical = canon.canonicalize(type_id);

        // Cache the result
        self.canonical_cache.borrow_mut().insert(type_id, canonical);

        canonical
    }
}
