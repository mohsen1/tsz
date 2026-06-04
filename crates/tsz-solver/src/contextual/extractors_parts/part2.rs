impl<'a> TypeVisitor for ParameterForCallExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_function(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.function_shape(FunctionShapeId(shape_id));

        if !self.signature_accepts_arg_count(&shape.params, self.arg_count) {
            return None;
        }

        extract_param_type_at_for_call(self.db, &shape.params, self.index, self.arg_count)
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));

        let mut matched = false;
        let mut param_types: Vec<TypeId> = Vec::new();

        let mut matching_call_signatures: Vec<_> = shape
            .call_signatures
            .iter()
            .filter(|sig| self.signature_accepts_arg_count(&sig.params, self.arg_count))
            .collect();
        if matching_call_signatures
            .iter()
            .any(|sig| !sig.params.last().is_some_and(|param| param.rest))
        {
            matching_call_signatures
                .retain(|sig| !sig.params.last().is_some_and(|param| param.rest));
        }

        // tsc's getIntersectedSignatures returns undefined when multiple
        // signatures are present and ANY is generic. This prevents contextual
        // typing when assigning arrow functions to overloaded types that have
        // both generic and non-generic call signatures.
        // Only apply when there's a MIX of generic and non-generic signatures
        // (genuine overloads). When ALL signatures are generic, they likely
        // come from union member merging and should still provide contextual types.
        if matching_call_signatures.len() > 1 {
            let has_generic = matching_call_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty());
            let has_non_generic = matching_call_signatures
                .iter()
                .any(|sig| sig.type_params.is_empty());
            if has_generic && has_non_generic {
                return None;
            }
        }

        for sig in matching_call_signatures {
            matched = true;
            if let Some(param_type) =
                extract_param_type_at_for_call(self.db, &sig.params, self.index, self.arg_count)
            {
                param_types.push(param_type);
            }
        }

        if param_types.is_empty() && !matched {
            param_types = shape
                .call_signatures
                .iter()
                .filter_map(|sig| {
                    extract_param_type_at_for_call(self.db, &sig.params, self.index, self.arg_count)
                })
                .collect();
        }

        // If no call signatures matched, check construct signatures.
        // This handles super() calls and new expressions where the callee
        // is a Callable with construct signatures (not call signatures).
        // NOTE: Generic construct signatures still provide useful contextual
        // types for callback arguments (possibly involving type parameters),
        // and suppressing them causes false TS7006 in constructor calls.
        if param_types.is_empty() {
            matched = false;
            let mut matching_construct_signatures: Vec<_> = shape
                .construct_signatures
                .iter()
                .filter(|sig| self.signature_accepts_arg_count(&sig.params, self.arg_count))
                .collect();
            if matching_construct_signatures
                .iter()
                .any(|sig| !sig.params.last().is_some_and(|param| param.rest))
            {
                matching_construct_signatures
                    .retain(|sig| !sig.params.last().is_some_and(|param| param.rest));
            }
            for sig in matching_construct_signatures {
                matched = true;
                if let Some(param_type) =
                    extract_param_type_at_for_call(self.db, &sig.params, self.index, self.arg_count)
                {
                    param_types.push(param_type);
                }
            }
            if param_types.is_empty() && !matched {
                param_types = shape
                    .construct_signatures
                    .iter()
                    .filter_map(|sig| {
                        extract_param_type_at_for_call(
                            self.db,
                            &sig.params,
                            self.index,
                            self.arg_count,
                        )
                    })
                    .collect();
            }
        }

        // Avoid contextual-type poisoning from catch-all `any` signatures
        // (e.g. implementation signatures like `(...args: any[])` on overloaded
        // constructors). If at least one non-`any` contextual type exists, prefer
        // those and drop `any` contributors.
        if param_types.len() > 1 {
            let has_non_any = param_types.iter().any(|&ty| ty != TypeId::ANY);
            if has_non_any {
                param_types.retain(|&ty| ty != TypeId::ANY);
            }
        }

        collect_single_or_union_no_reduce(self.db, param_types)
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        // For unions, extract parameter types from each member and combine.
        // Use no-reduce union to preserve all callback type variants — see
        // collect_single_or_union_no_reduce doc comment for rationale.
        let members = self.db.type_list(TypeListId(list_id));
        let types: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let mut extractor =
                    ParameterForCallExtractor::new(self.db, self.index, self.arg_count);
                extractor.extract(member)
            })
            .collect();
        collect_single_or_union_no_reduce(self.db, types)
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to extract a type argument at a given index from an Application type.
///
/// Used for `Generator<Y, R, N>` and similar generic types where we need to
/// pull out a specific type parameter by position.
pub(crate) struct ApplicationArgExtractor<'a> {
    db: &'a dyn TypeDatabase,
    arg_index: usize,
}

impl<'a> ApplicationArgExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase, arg_index: usize) -> Self {
        Self { db, arg_index }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }
}

impl<'a> TypeVisitor for ApplicationArgExtractor<'a> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_application(&mut self, app_id: u32) -> Self::Output {
        let app = self.db.type_application(TypeApplicationId(app_id));
        app.args.get(self.arg_index).copied()
    }

    fn default_output() -> Self::Output {
        None
    }
}

/// Visitor to check if a given argument index falls at a rest parameter position
/// for a callable type. Used by TS2556 checking: non-tuple array spreads must
/// only land on rest parameter positions.
///
/// For overloaded callables, returns `true` only if ALL matching signatures
/// have the index at a rest position. This is conservative — if any signature
/// treats the position as non-rest, the spread is invalid.
pub(crate) struct RestPositionCheckExtractor<'a> {
    db: &'a dyn TypeDatabase,
    index: usize,
    arg_count: usize,
}

impl<'a> RestPositionCheckExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase, index: usize, arg_count: usize) -> Self {
        Self {
            db,
            index,
            arg_count,
        }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> bool {
        self.visit_type(self.db, type_id).unwrap_or(false)
    }

    fn signature_accepts_arg_count(&self, params: &[ParamInfo], arg_count: usize) -> bool {
        let required_count = params.iter().filter(|p| !p.optional).count();
        let has_rest = params.iter().any(|p| p.rest);
        if has_rest {
            arg_count >= required_count
        } else {
            arg_count >= required_count && arg_count <= params.len()
        }
    }
}

impl<'a> TypeVisitor for RestPositionCheckExtractor<'a> {
    type Output = Option<bool>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_function(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.function_shape(FunctionShapeId(shape_id));
        Some(is_rest_position(&shape.params, self.index))
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));

        // Check both call and construct signatures (super() uses construct sigs)
        let all_sigs: Vec<&[ParamInfo]> = shape
            .call_signatures
            .iter()
            .chain(shape.construct_signatures.iter())
            .map(|sig| sig.params.as_slice())
            .collect();

        if all_sigs.is_empty() {
            return None;
        }

        // Check matching signatures first
        let mut any_matched = false;
        let mut all_rest = true;
        for &params in &all_sigs {
            if self.signature_accepts_arg_count(params, self.arg_count) {
                any_matched = true;
                if !is_rest_position(params, self.index) {
                    all_rest = false;
                }
            }
        }

        if !any_matched {
            // Fall back to all signatures
            for &params in &all_sigs {
                if !is_rest_position(params, self.index) {
                    return Some(false);
                }
            }
            return Some(true);
        }

        Some(all_rest)
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        // If any member says non-rest, the spread is invalid
        for &m in members.iter() {
            let mut extractor =
                RestPositionCheckExtractor::new(self.db, self.index, self.arg_count);
            if !extractor.extract(m) {
                return Some(false);
            }
        }
        Some(true)
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        for &m in members.iter() {
            let mut extractor =
                RestPositionCheckExtractor::new(self.db, self.index, self.arg_count);
            if let Some(result) = extractor.visit_type(self.db, m) {
                return Some(result);
            }
        }
        None
    }

    fn default_output() -> Self::Output {
        None
    }
}

pub(crate) struct RestOrOptionalTailPositionExtractor<'a> {
    db: &'a dyn TypeDatabase,
    index: usize,
    arg_count: usize,
}

impl<'a> RestOrOptionalTailPositionExtractor<'a> {
    pub(crate) fn new(db: &'a dyn TypeDatabase, index: usize, arg_count: usize) -> Self {
        Self {
            db,
            index,
            arg_count,
        }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> bool {
        self.visit_type(self.db, type_id).unwrap_or(false)
    }

    fn signature_accepts_arg_count(&self, params: &[ParamInfo], arg_count: usize) -> bool {
        let required_count = params.iter().filter(|p| !p.optional).count();
        let has_rest = params.iter().any(|p| p.rest);
        if has_rest {
            arg_count >= required_count
        } else {
            arg_count >= required_count && arg_count <= params.len()
        }
    }
}

impl<'a> TypeVisitor for RestOrOptionalTailPositionExtractor<'a> {
    type Output = Option<bool>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_function(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.function_shape(FunctionShapeId(shape_id));
        Some(is_rest_or_optional_tail_position(&shape.params, self.index))
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));
        let all_sigs: Vec<&[ParamInfo]> = shape
            .call_signatures
            .iter()
            .chain(shape.construct_signatures.iter())
            .map(|sig| sig.params.as_slice())
            .collect();

        if all_sigs.is_empty() {
            return None;
        }

        let mut any_matched = false;
        let mut all_allowed = true;
        for &params in &all_sigs {
            if self.signature_accepts_arg_count(params, self.arg_count) {
                any_matched = true;
                if !is_rest_or_optional_tail_position(params, self.index) {
                    all_allowed = false;
                }
            }
        }

        if !any_matched {
            for &params in &all_sigs {
                if !is_rest_or_optional_tail_position(params, self.index) {
                    return Some(false);
                }
            }
            return Some(true);
        }

        Some(all_allowed)
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        for &m in members.iter() {
            let mut extractor =
                RestOrOptionalTailPositionExtractor::new(self.db, self.index, self.arg_count);
            if !extractor.extract(m) {
                return Some(false);
            }
        }
        Some(true)
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));
        for &m in members.iter() {
            let mut extractor =
                RestOrOptionalTailPositionExtractor::new(self.db, self.index, self.arg_count);
            if let Some(result) = extractor.visit_type(self.db, m) {
                return Some(result);
            }
        }
        None
    }

    fn default_output() -> Self::Output {
        None
    }
}
