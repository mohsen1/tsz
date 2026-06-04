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
        match self.lookup(type_id) {
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.object_shape(shape_id);
                IndexInfo {
                    string_index: shape.string_index,
                    number_index: shape.number_index,
                }
            }
            Some(TypeData::Array(element)) => {
                // Arrays have number index signature with element type
                IndexInfo {
                    string_index: None,
                    number_index: Some(crate::types::IndexSignature {
                        key_type: TypeId::NUMBER,
                        value_type: element,
                        readonly: false,
                        param_name: None,
                    }),
                }
            }
            Some(TypeData::Tuple(elements_id)) => {
                // Tuples have number index signature with union of element types
                let elements = self.tuple_list(elements_id);
                let element_types: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
                let value_type = if element_types.is_empty() {
                    TypeId::UNDEFINED
                } else if element_types.len() == 1 {
                    element_types[0]
                } else {
                    self.union(element_types)
                };
                IndexInfo {
                    string_index: None,
                    number_index: Some(crate::types::IndexSignature {
                        key_type: TypeId::NUMBER,
                        value_type,
                        readonly: false,
                        param_name: None,
                    }),
                }
            }
            Some(TypeData::Union(members_id)) => {
                // For unions, collect index signatures from all members
                let members = self.type_list(members_id);
                let mut string_indices = Vec::with_capacity(members.len());
                let mut number_indices = Vec::with_capacity(members.len());

                for &member in members.iter() {
                    let info = self.get_index_signatures(member);
                    if let Some(sig) = info.string_index {
                        string_indices.push(sig);
                    }
                    if let Some(sig) = info.number_index {
                        number_indices.push(sig);
                    }
                }

                // Union of the value types
                let string_index = if string_indices.is_empty() {
                    None
                } else {
                    Some(crate::types::IndexSignature {
                        key_type: TypeId::STRING,
                        value_type: self
                            .union(string_indices.iter().map(|s| s.value_type).collect()),
                        readonly: string_indices.iter().all(|s| s.readonly),
                        param_name: None,
                    })
                };

                let number_index = if number_indices.is_empty() {
                    None
                } else {
                    Some(crate::types::IndexSignature {
                        key_type: TypeId::NUMBER,
                        value_type: self
                            .union(number_indices.iter().map(|s| s.value_type).collect()),
                        readonly: number_indices.iter().all(|s| s.readonly),
                        param_name: None,
                    })
                };

                IndexInfo {
                    string_index,
                    number_index,
                }
            }
            Some(TypeData::Intersection(members_id)) => {
                // For intersections, combine index signatures
                let members = self.type_list(members_id);
                let mut string_index = None;
                let mut number_index = None;

                for &member in members.iter() {
                    let info = self.get_index_signatures(member);
                    if let Some(sig) = info.string_index {
                        string_index = Some(sig);
                    }
                    if let Some(sig) = info.number_index {
                        number_index = Some(sig);
                    }
                }

                IndexInfo {
                    string_index,
                    number_index,
                }
            }
            _ => IndexInfo::default(),
        }
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
