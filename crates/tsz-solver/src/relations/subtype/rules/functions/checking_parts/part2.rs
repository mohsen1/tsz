impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check if an overloaded callable type is a subtype of a single function type.
    pub(crate) fn check_callable_to_function_subtype(
        &mut self,
        s_callable_id: CallableShapeId,
        t_fn_id: FunctionShapeId,
    ) -> SubtypeResult {
        let s_callable = self.interner.callable_shape(s_callable_id);
        let t_fn = self.interner.function_shape(t_fn_id);

        if t_fn.is_constructor {
            let has_multiple_source_construct_sigs = s_callable.construct_signatures.len() > 1;
            for s_sig in &s_callable.construct_signatures {
                let direct = self
                    .check_call_signature_subtype_to_fn(s_sig, &t_fn)
                    .is_true();
                if direct {
                    return SubtypeResult::True;
                }
            }

            // tsc N×M path: when the source has multiple constructor signatures,
            // retry by erasing type parameters to `any`.
            if has_multiple_source_construct_sigs {
                for s_sig in &s_callable.construct_signatures {
                    let erased = self
                        .check_erased_signature_subtype_to_fn(s_sig, &t_fn)
                        .is_true();
                    if erased {
                        return SubtypeResult::True;
                    }
                }
            }
            return SubtypeResult::False;
        }

        if s_callable.call_signatures.is_empty() {
            return SubtypeResult::False;
        }

        // Check source call signatures against the target function.
        // A single compatible source signature is enough to establish the relation.
        for s_sig in &s_callable.call_signatures {
            if self
                .check_call_signature_subtype_to_fn(s_sig, &t_fn)
                .is_true()
            {
                return SubtypeResult::True;
            }

            if !s_sig.type_params.is_empty()
                && t_fn.type_params.is_empty()
                && self
                    .try_instantiate_generic_callable_to_function(s_sig, &t_fn)
                    .is_true()
            {
                return SubtypeResult::True;
            }
        }

        // tsc N×M path: when a callable has multiple signatures and the direct
        // comparison above fails, try erasing type parameters to `any`
        // comparison above fails, try erasing type parameters to `any`
        // (matching tsc's `getErasedSignature` / `createTypeEraser`). In tsc's
        // `signaturesRelatedTo`, the N×M case (source.length > 1 || target.length > 1)
        // always uses `erase = true`, which maps type params to `any`. This allows
        // overloaded callables with constrained generics (e.g., `{ <T extends A>(x: T): T;
        // <T extends B>(x: T): T }`) to be assignable to unconstrained generic functions
        // (e.g., `<T>(x: T) => T`), because after erasure both become `(x: any) => any`.
        if s_callable.call_signatures.len() > 1 {
            for s_sig in &s_callable.call_signatures {
                if self
                    .check_erased_signature_subtype_to_fn(s_sig, &t_fn)
                    .is_true()
                {
                    return SubtypeResult::True;
                }
            }
        }

        SubtypeResult::False
    }

    /// Try to instantiate a generic callable signature to match a concrete function type.
    /// This handles cases like: `declare function box<V>(x: V): {value: V}; const f: (x: number) => {value: number} = box;`
    fn try_instantiate_generic_callable_to_function(
        &mut self,
        s_sig: &crate::types::CallSignature,
        t_fn: &crate::types::FunctionShape,
    ) -> SubtypeResult {
        use crate::TypeData;
        use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

        // Create a substitution mapping type parameters to the target's parameter types
        // This is a simplified instantiation - we map each source type param to the corresponding target param type
        let mut substitution = TypeSubstitution::new();

        // For a simple case like <V>(x: V) => R vs (x: T) => S, map V to T
        // This handles the common case where type parameters flow through from parameters to return type
        for (s_param, t_param) in s_sig.params.iter().zip(t_fn.params.iter()) {
            // If source param is a type parameter, map it to target param type
            if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(s_param.type_id) {
                substitution.insert(tp.name, t_param.type_id);
            }
        }

        // If we couldn't infer any type parameters, fall back to checking with unknown
        // This handles cases where type params aren't directly in parameters
        if substitution.is_empty() {
            for tp in &s_sig.type_params {
                substitution.insert(tp.name, crate::TypeId::UNKNOWN);
            }
        }

        // Instantiate the source signature
        let instantiated_params: Vec<_> = s_sig
            .params
            .iter()
            .map(|p| crate::types::ParamInfo {
                name: p.name,
                type_id: instantiate_type(self.interner, p.type_id, &substitution),
                optional: p.optional,
                rest: p.rest,
            })
            .collect();

        let instantiated_return = instantiate_type(self.interner, s_sig.return_type, &substitution);

        let instantiated_sig = crate::types::CallSignature {
            type_params: Vec::new(), // No type params after instantiation
            params: instantiated_params,
            this_type: s_sig.this_type,
            return_type: instantiated_return,
            type_predicate: s_sig.type_predicate,
            is_method: s_sig.is_method,
        };

        // Check if instantiated signature is compatible with target
        self.check_call_signature_subtype_to_fn(&instantiated_sig, t_fn)
    }

    /// Check callable subtyping with overloaded signatures.
    pub(crate) fn check_callable_subtype(
        &mut self,
        source: &CallableShape,
        target: &CallableShape,
    ) -> SubtypeResult {
        // For each target call signature, at least one source call signature must match.
        // Unlike call-site overload resolution (which uses only the implementation/last
        // signature), structural subtype checking uses ALL source signatures — matching
        // tsc's signaturesRelatedTo N×M comparison.
        let is_multi_sig = source.call_signatures.len() > 1 || target.call_signatures.len() > 1;
        for t_sig in &target.call_signatures {
            let mut found_match = false;
            if source.call_signatures.len() > 1
                && (t_sig.is_method || source.call_signatures.iter().any(|sig| sig.is_method))
                && self
                    .method_overloads_cover_tuple_union_rest_target(&source.call_signatures, t_sig)
            {
                found_match = true;
            }
            for s_sig in &source.call_signatures {
                if self.check_call_signature_subtype(s_sig, t_sig).is_true() {
                    found_match = true;
                    break;
                }
            }
            // tsc N×M path: when either side has multiple signatures, try erasing
            // type params to `any` (matching tsc's `getErasedSignature` behavior).
            if !found_match && is_multi_sig {
                for s_sig in &source.call_signatures {
                    if self
                        .check_erased_call_signature_subtype(s_sig, t_sig)
                        .is_true()
                        || self
                            .check_erased_call_signature_params_with_matching_return_base(
                                s_sig, t_sig,
                            )
                            .is_true()
                    {
                        found_match = true;
                        break;
                    }
                }
            }
            if !found_match {
                return SubtypeResult::False;
            }
        }

        // For each target construct signature, at least one source signature must match.
        // Callable-object construct signatures come from property values such as
        // `{ ctor: new <T>(x: T) => T }`, not from method syntax, so they should
        // follow the regular property-function relation instead of method-style
        // bivariance. Standalone constructor function types still flow through
        // `check_function_subtype` with `is_constructor = true`.
        for t_sig in &target.construct_signatures {
            let mut found_match = false;
            for s_sig in &source.construct_signatures {
                let result = self.check_call_signature_subtype_as_constructor(s_sig, t_sig);
                if result.is_true() {
                    found_match = true;
                    break;
                }
            }
            if !found_match
                && (source.construct_signatures.len() > 1 || target.construct_signatures.len() > 1)
            {
                for s_sig in &source.construct_signatures {
                    if self
                        .check_erased_call_signature_subtype_as_constructor(s_sig, t_sig)
                        .is_true()
                    {
                        found_match = true;
                        break;
                    }
                }
            }
            if !found_match {
                return SubtypeResult::False;
            }
        }

        // Check properties (if any), excluding private fields.
        // Sort by name (Atom) to match the merge scan's expectation in check_object_subtype.
        //
        // When both callables have construct signatures (class constructors), skip the
        // `prototype` property. Its type is the instance type which is already validated
        // by construct signature compatibility — checking it separately can fail when
        // the target has generic type params that were erased only at the signature level.
        let has_construct_sigs =
            !source.construct_signatures.is_empty() && !target.construct_signatures.is_empty();
        let should_skip_prop = |name| {
            let resolved = self.interner.resolve_atom(name);
            resolved.starts_with('#') || (has_construct_sigs && resolved == "prototype")
        };
        let mut source_props: Vec<_> = source
            .properties
            .iter()
            .filter(|p| !should_skip_prop(p.name))
            .cloned()
            .collect();
        // Function-like sources (with call signatures) are expected to have Function members
        // such as `call` and `apply`, even if those properties are not materialized on the
        // callable shape. Add synthetic members to align assignability behavior.
        if !source.call_signatures.is_empty() {
            for t_prop in &target.properties {
                let prop_name = self.interner.resolve_atom(t_prop.name);
                if (prop_name == "call" || prop_name == "apply")
                    && !source_props.iter().any(|p| p.name == t_prop.name)
                {
                    source_props.push(PropertyInfo {
                        name: t_prop.name,
                        type_id: t_prop.type_id,
                        write_type: t_prop.write_type,
                        optional: false,
                        readonly: false,
                        is_method: true,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: 0,
                        is_string_named: false,
                        is_symbol_named: false,
                        single_quoted_name: false,
                    });
                }
            }
        }
        source_props.sort_by_key(|a| a.name);
        let mut target_props: Vec<_> = target
            .properties
            .iter()
            .filter(|p| !should_skip_prop(p.name))
            .cloned()
            .collect();
        target_props.sort_by_key(|a| a.name);
        // Create temporary ObjectShape instances for the property check
        let source_shape = ObjectShape {
            flags: ObjectFlags::empty(),
            properties: source_props,
            string_index: source.string_index,
            number_index: source.number_index,
            symbol: source.symbol,
        };
        let target_shape = ObjectShape {
            flags: ObjectFlags::empty(),
            properties: target_props,
            string_index: target.string_index,
            number_index: target.number_index,
            symbol: target.symbol,
        };
        if !self
            .check_object_subtype(&source_shape, None, None, &target_shape, None)
            .is_true()
        {
            return SubtypeResult::False;
        }

        SubtypeResult::True
    }

    fn method_overloads_cover_tuple_union_rest_target(
        &mut self,
        source_sigs: &[CallSignature],
        target_sig: &CallSignature,
    ) -> bool {
        use crate::type_queries::data::get_union_members;
        use crate::type_queries::unpack_tuple_rest_parameter;

        let Some(last_target_param) = target_sig.params.last().filter(|param| param.rest) else {
            return false;
        };
        let Some(union_members) = get_union_members(self.interner, last_target_param.type_id)
        else {
            return false;
        };

        let prefix_params = &target_sig.params[..target_sig.params.len().saturating_sub(1)];
        union_members.iter().all(|member_type_id| {
            let member_param = ParamInfo {
                type_id: *member_type_id,
                rest: true,
                ..*last_target_param
            };
            let mut variant_params = prefix_params.to_vec();
            variant_params.extend(unpack_tuple_rest_parameter(self.interner, &member_param));
            source_sigs.iter().any(|source_sig| {
                let source_fn = FunctionShape {
                    type_params: source_sig.type_params.clone(),
                    params: source_sig.params.clone(),
                    this_type: source_sig.this_type,
                    return_type: source_sig.return_type,
                    type_predicate: source_sig.type_predicate,
                    is_constructor: false,
                    is_method: source_sig.is_method,
                };
                let variant_fn = FunctionShape {
                    type_params: target_sig.type_params.clone(),
                    params: variant_params.clone(),
                    this_type: target_sig.this_type,
                    return_type: target_sig.return_type,
                    type_predicate: target_sig.type_predicate,
                    is_constructor: false,
                    is_method: target_sig.is_method,
                };
                self.check_function_subtype(&source_fn, &variant_fn)
                    .is_true()
                    || self.method_overload_prefix_covers_variant(source_sig, &variant_params)
            })
        })
    }

    fn method_overload_prefix_covers_variant(
        &mut self,
        source_sig: &CallSignature,
        variant_params: &[ParamInfo],
    ) -> bool {
        if !source_sig.is_method || variant_params.is_empty() {
            return false;
        }
        if source_sig.params.len() < variant_params.len() {
            return false;
        }
        source_sig
            .params
            .iter()
            .zip(variant_params.iter())
            .take(variant_params.len())
            .all(|(source_param, target_param)| {
                let (source_type, target_type) =
                    self.effective_param_type_pair(source_param, target_param);
                self.are_parameters_compatible_impl(source_type, target_type, true)
            })
    }

    /// Check call signature subtyping.
    pub(crate) fn check_call_signature_subtype(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> SubtypeResult {
        self.check_call_signature_subtype_impl(source, target, false)
    }

    pub(crate) fn callable_modality_flags_for_type(&mut self, type_id: TypeId) -> (bool, bool) {
        let direct = self.callable_modality_flags_for_type_direct(type_id);
        if direct.0 || direct.1 {
            return direct;
        }
        let evaluated = self.evaluate_type(type_id);
        if evaluated == type_id {
            direct
        } else {
            self.callable_modality_flags_for_type_direct(evaluated)
        }
    }

    fn callable_modality_flags_for_type_direct(&self, type_id: TypeId) -> (bool, bool) {
        if let Some(shape_id) = callable_shape_id(self.interner, type_id) {
            let shape = self.interner.callable_shape(shape_id);
            return (
                !shape.call_signatures.is_empty(),
                !shape.construct_signatures.is_empty(),
            );
        }
        if let Some(fn_id) = crate::visitor::function_shape_id(self.interner, type_id) {
            let f = self.interner.function_shape(fn_id);
            return (!f.is_constructor, f.is_constructor);
        }
        (false, false)
    }

    pub(crate) fn check_call_signature_subtype_as_constructor(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
    ) -> SubtypeResult {
        self.check_call_signature_subtype_impl(source, target, true)
    }

    fn check_call_signature_subtype_impl(
        &mut self,
        source: &CallSignature,
        target: &CallSignature,
        is_constructor: bool,
    ) -> SubtypeResult {
        let source_fn = FunctionShape {
            type_params: source.type_params.clone(),
            params: source.params.clone(),
            this_type: source.this_type,
            return_type: source.return_type,
            type_predicate: source.type_predicate,
            is_constructor,
            is_method: source.is_method,
        };
        let target_fn = FunctionShape {
            type_params: target.type_params.clone(),
            params: target.params.clone(),
            this_type: target.this_type,
            return_type: target.return_type,
            type_predicate: target.type_predicate,
            is_constructor,
            is_method: target.is_method,
        };
        self.check_function_subtype(&source_fn, &target_fn)
    }

    fn constructor_signatures_need_strict_params(
        source: &FunctionShape,
        target: &FunctionShape,
    ) -> bool {
        if !(source.is_constructor || target.is_constructor) {
            return false;
        }

        let source_generic = !source.type_params.is_empty();
        let target_generic = !target.type_params.is_empty();
        if !source_generic && !target_generic {
            // Non-generic constructors need strict params when there's an
            // optionality mismatch between corresponding parameters. This
            // matches tsc where property-typed constructor types like
            // `new (x?: number) => number` use strict comparison, not
            // constructor bivariance. Without this, the bivariant check
            // would pass (`number <: number | undefined`) and incorrectly
            // allow `new (x: number) => number` as a subtype.
            let has_optionality_mismatch = source
                .params
                .iter()
                .zip(target.params.iter())
                .any(|(sp, tp)| sp.optional != tp.optional);
            return has_optionality_mismatch;
        }

        if source_generic && !target_generic {
            let has_optionality_mismatch = source
                .params
                .iter()
                .zip(target.params.iter())
                .any(|(sp, tp)| sp.optional != tp.optional);
            return has_optionality_mismatch
                || source.type_params.iter().any(|tp| tp.constraint.is_some());
        }

        source.type_params.len() != target.type_params.len()
            || source
                .type_params
                .iter()
                .chain(target.type_params.iter())
                .any(|tp| tp.constraint.is_some())
            || source.params.len() != 1
            || target.params.len() != 1
    }

    /// Check call signature subtype to function shape.
    pub(crate) fn check_call_signature_subtype_to_fn(
        &mut self,
        source: &CallSignature,
        target: &FunctionShape,
    ) -> SubtypeResult {
        let source_fn = FunctionShape {
            type_params: source.type_params.clone(),
            params: source.params.clone(),
            this_type: source.this_type,
            return_type: source.return_type,
            type_predicate: source.type_predicate,
            is_constructor: target.is_constructor,
            is_method: source.is_method,
        };
        self.check_function_subtype(&source_fn, target)
    }

    /// Check function shape subtype to call signature.
    pub(crate) fn check_call_signature_subtype_fn(
        &mut self,
        source: &FunctionShape,
        target: &CallSignature,
    ) -> SubtypeResult {
        let target_fn = FunctionShape {
            type_params: target.type_params.clone(),
            params: target.params.clone(),
            this_type: target.this_type,
            return_type: target.return_type,
            type_predicate: target.type_predicate,
            is_constructor: source.is_constructor,
            is_method: target.is_method,
        };
        self.check_function_subtype(source, &target_fn)
    }

    /// Evaluate a meta-type (conditional, index access, mapped, keyof, etc.) to its
    /// concrete form. Uses `TypeEvaluator` with the resolver to correctly resolve
    /// Lazy(DefId) types at all nesting levels (e.g., KeyOf(Lazy(DefId))).
    ///
    /// Always uses `TypeEvaluator` with the resolver instead of `query_db.evaluate_type()`
    /// because the checker populates DefId→TypeId mappings in the `TypeEnvironment` that
    /// the `query_db`'s resolver-less evaluator cannot access.
    ///
    /// Results are cached in `eval_cache` to avoid re-evaluating the same type across
    /// multiple subtype checks. This turns O(n²) evaluate calls into O(n).
    pub(crate) fn evaluate_type(&mut self, type_id: TypeId) -> TypeId {
        // Fast path: intrinsic types (number, string, boolean, void, null, etc.)
        // never need evaluation. Skip cache lookup entirely.
        if type_id.is_intrinsic() {
            return type_id;
        }
        // Check local evaluation cache first.
        // Key includes no_unchecked_indexed_access since with that flag evaluation results can vary.
        let cache_key = (type_id, self.no_unchecked_indexed_access);
        if let Some(&cached) = self.eval_cache.get(&cache_key) {
            return cached;
        }
        use crate::evaluation::evaluate::TypeEvaluator;
        let mut evaluator = TypeEvaluator::with_resolver(self.interner, self.resolver);
        evaluator.set_no_unchecked_indexed_access(self.no_unchecked_indexed_access);
        // Pass query_db to share the application evaluation cache across evaluations.
        // This ensures that Application(Lazy(DefId), args) evaluated multiple times produces
        // the same ObjectShapeId, preventing spurious structural subtype failures when two
        // independent evaluations of the same generic type (e.g., AsyncGenerator<string, string, string[]>)
        // produce different shape IDs.
        if let Some(db) = self.query_db {
            evaluator = evaluator.with_query_db(db);
        }
        let result = evaluator.evaluate(type_id);
        self.eval_cache.insert(cache_key, result);
        result
    }
}
