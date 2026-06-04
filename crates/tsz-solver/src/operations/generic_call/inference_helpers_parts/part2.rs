impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(crate) fn instantiate_generic_function_argument_against_target(
        &mut self,
        source_ty: TypeId,
        target_ty: TypeId,
    ) -> TypeId {
        // Class constructor Callable types (e.g., `Promise`) must not be
        // decomposed into a Function type, because that loses static members and
        // the construct-signature wrapper. However, ordinary declared generic
        // functions and generic constructor callbacks represented as Callable
        // types do need contextual instantiation against the target callback
        // signature. Distinguish those cases by checking for a single generic
        // call or construct signature.
        if let Some(TypeData::Callable(shape_id)) = self.interner.lookup(source_ty) {
            let shape = self.interner.callable_shape(shape_id);
            let has_generic_call_sig = shape
                .call_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty());
            let has_generic_construct_sig = shape.call_signatures.is_empty()
                && shape.construct_signatures.len() == 1
                && !shape.construct_signatures[0].type_params.is_empty();
            let has_overloaded_call_sigs = shape.call_signatures.len() > 1;
            if !has_generic_call_sig && !has_generic_construct_sig && !has_overloaded_call_sigs {
                return source_ty;
            }
            // When the Callable has construct signatures AND properties (static
            // members), it represents a class constructor type (e.g., `typeof
            // MyClass`). Decomposing it into a Function type would lose static
            // members and the construct signature, causing false TS2345/TS2769
            // errors when the class is passed as an argument to a `typeof MyClass`
            // parameter in generic overload resolution.
            if !shape.construct_signatures.is_empty() && !shape.properties.is_empty() {
                return source_ty;
            }
        }
        let evaluated_source_ty = self.interner.evaluate_type(source_ty);
        let evaluated_target_ty = self.interner.evaluate_type(target_ty);
        let function_info = Self::get_source_signature_for_target(
            self.interner.as_type_database(),
            source_ty,
            target_ty,
        )
        .or_else(|| {
            Self::get_source_signature_for_target(
                self.interner.as_type_database(),
                evaluated_source_ty,
                evaluated_target_ty,
            )
        })
        .or_else(|| {
            // When the target is an Application with a Lazy base (interface-defined
            // callback like `Callback<T, R>`), the solver's evaluate_type can't resolve
            // the Lazy DefId. Use the checker's evaluate_type which has access to the
            // type environment for DefId → Callable resolution.
            let checker_target = self.checker.evaluate_type(target_ty);
            if checker_target != target_ty && checker_target != evaluated_target_ty {
                Self::get_source_signature_for_target(
                    self.interner.as_type_database(),
                    source_ty,
                    checker_target,
                )
            } else {
                None
            }
        });

        let Some((source_fn, target_fn)) = function_info else {
            return source_ty;
        };
        let source_fn = self.normalize_function_shape_params_for_context(&source_fn);
        let target_fn = self.normalize_function_shape_params_for_context(&target_fn);
        if !source_fn.type_params.is_empty() && !target_fn.type_params.is_empty() {
            return source_ty;
        }

        // Keep generic callbacks intact for `(...args: I)` targets so the
        // constraint walker can infer `I` as a tuple from the source parameters.
        // Contextual instantiation would ask for an element type of unresolved
        // `I` and collapse to its array constraint.
        let target_rest_is_outer_inference_placeholder =
            target_fn.params.last().is_some_and(|param| {
                if !param.rest {
                    return false;
                }
                matches!(
                    self.interner.lookup(param.type_id),
                    Some(TypeData::TypeParameter(info))
                        if self
                            .interner
                            .resolve_atom(info.name)
                            .as_str()
                            .starts_with("__infer_")
                )
            });
        if !source_fn.type_params.is_empty() && target_rest_is_outer_inference_placeholder {
            return source_ty;
        }

        if source_fn.type_params.is_empty() {
            let source_has_calls = crate::type_queries::get_call_signatures(
                self.interner.as_type_database(),
                source_ty,
            )
            .is_some_and(|sigs| !sigs.is_empty());
            let source_has_constructs = crate::type_queries::get_construct_signatures(
                self.interner.as_type_database(),
                source_ty,
            )
            .is_some_and(|sigs| !sigs.is_empty());
            let target_has_calls = crate::type_queries::get_call_signatures(
                self.interner.as_type_database(),
                target_ty,
            )
            .is_some_and(|sigs| !sigs.is_empty());
            let target_has_constructs = crate::type_queries::get_construct_signatures(
                self.interner.as_type_database(),
                target_ty,
            )
            .is_some_and(|sigs| !sigs.is_empty());
            if !source_has_calls
                && source_has_constructs
                && !target_has_calls
                && target_has_constructs
            {
                return source_ty;
            }
            return self.interner.function(source_fn);
        }

        let mut target_param_types = Vec::with_capacity(source_fn.params.len());
        for index in 0..source_fn.params.len() {
            let Some(param_type) =
                self.param_type_for_arg_index(&target_fn.params, index, source_fn.params.len())
            else {
                return source_ty;
            };
            target_param_types.push(param_type);
        }

        if target_param_types.is_empty() {
            return source_ty;
        }
        if target_param_types.iter().any(|&param_type| {
            Self::contains_tuple_like_parameter_target(self.interner.as_type_database(), param_type)
        }) {
            return source_ty;
        }

        let source_type_params_fully_determined_by_params =
            source_fn.type_params.iter().all(|tp| {
                source_fn.params.iter().any(|param| {
                    crate::visitor::collect_referenced_types(
                        self.interner.as_type_database(),
                        param.type_id,
                    )
                    .into_iter()
                    .any(|ty| {
                        crate::type_param_info(self.interner.as_type_database(), ty)
                            .is_some_and(|info| info.name == tp.name)
                    })
                })
            });

        // Handle generic function arguments when target params are inference
        // placeholders from an outer generic call. Three cases:
        //
        // 1. Naked type params (e.g., `list<T>(a: T)`): Skip erasure, let
        //    instantiation proceed. The params match 1:1 against target placeholders.
        //
        // 2. Non-naked type params (e.g., `unbox<W>(x: Box<W>)`) WITH a generic
        //    contextual type: Return source_ty unchanged so `constrain_types_impl`'s
        //    generic function branch creates fresh inference variables in the shared
        //    context, enabling proper higher-order inference (e.g., compose(unbox, unlist)).
        //
        // 3. Non-naked type params WITHOUT a generic contextual type: Erase source
        //    type params to constraints/unknown (old behavior). Without a generic
        //    contextual type, the fresh inference variables would leak unresolved.
        let any_target_param_is_type_param = target_param_types.iter().any(|&param_type| {
            matches!(
                self.interner.lookup(param_type),
                Some(TypeData::TypeParameter(_))
            )
        });
        let any_target_param_contains_infer_placeholder =
            target_param_types.iter().any(|&param_type| {
                crate::type_queries::contains_infer_types_db(
                    self.interner.as_type_database(),
                    param_type,
                )
            });
        let target_params_need_hofi =
            any_target_param_is_type_param || any_target_param_contains_infer_placeholder;

        // Conflicting-candidate substitution applies only when target params
        // are concrete (post-inference) types. When *any* target param is
        // still an inference placeholder from an outer generic call (e.g.,
        // `apply<A,B,C>(fn: (a: A, b: B) => C, ...)` invoking `g<T>(x:T,y:T)`),
        // the existing Case 1/2/3 placeholder-aware logic below is the
        // correct path: two distinct unconstrained TypeParameters mapped to
        // the same source param look "conflicting" by `is_assignable_to`,
        // which would short-circuit erasure and produce a partially-
        // instantiated function.
        if !target_params_need_hofi
            && let Some(substitution) =
                self.conflicting_contextual_param_candidate_substitution(&source_fn, &target_fn)
        {
            return self.interner.function(FunctionShape {
                params: source_fn
                    .params
                    .iter()
                    .map(|param| ParamInfo {
                        name: param.name,
                        type_id: instantiate_type(self.interner, param.type_id, &substitution),
                        optional: param.optional,
                        rest: param.rest,
                    })
                    .collect(),
                return_type: instantiate_type(self.interner, source_fn.return_type, &substitution),
                this_type: source_fn
                    .this_type
                    .map(|this_type| instantiate_type(self.interner, this_type, &substitution)),
                type_params: vec![],
                type_predicate: source_fn
                    .type_predicate
                    .as_ref()
                    .map(|predicate| TypePredicate {
                        asserts: predicate.asserts,
                        target: predicate.target,
                        type_id: predicate
                            .type_id
                            .map(|tid| instantiate_type(self.interner, tid, &substitution)),
                        parameter_index: predicate.parameter_index,
                    }),
                is_constructor: source_fn.is_constructor,
                is_method: source_fn.is_method,
            });
        }

        let source_type_params_are_naked = source_fn.type_params.iter().all(|tp| {
            source_fn.params.iter().any(|param| {
                matches!(
                    self.interner.lookup(param.type_id),
                    Some(TypeData::TypeParameter(info)) if info.name == tp.name
                )
            })
        });
        let source_type_params_have_constraints = source_fn
            .type_params
            .iter()
            .any(|tp| tp.constraint.is_some());
        if source_type_params_are_naked
            && source_type_params_have_constraints
            && target_params_need_hofi
        {
            return source_ty;
        }
        if source_type_params_fully_determined_by_params
            && target_params_need_hofi
            && !source_type_params_are_naked
        {
            let has_generic_contextual_type = self.contextual_type.is_some_and(|ctx| {
                crate::type_queries::get_function_shape(self.interner.as_type_database(), ctx)
                    .is_some_and(|shape| {
                        !shape.type_params.is_empty()
                            && shape.params.iter().any(|param| {
                                crate::type_queries::get_function_shape(
                                    self.interner.as_type_database(),
                                    param.type_id,
                                )
                                .is_some_and(|inner| !inner.type_params.is_empty())
                                    || crate::type_queries::get_call_signatures(
                                        self.interner.as_type_database(),
                                        param.type_id,
                                    )
                                    .is_some_and(|sigs| {
                                        sigs.iter().any(|sig| !sig.type_params.is_empty())
                                    })
                            })
                    })
            });
            let target_is_pure_placeholder = target_fn.type_params.is_empty()
                && target_param_types.iter().all(|&pt| {
                    matches!(self.interner.lookup(pt), Some(TypeData::TypeParameter(_)))
                })
                && matches!(
                    self.interner.lookup(target_fn.return_type),
                    Some(TypeData::TypeParameter(_))
                );
            if has_generic_contextual_type
                || target_is_pure_placeholder
                || any_target_param_contains_infer_placeholder
            {
                // Case 2: let constrain_types handle it with fresh variables
                return source_ty;
            }
            let preserve_callable_alias = source_fn.type_params.len() == 1
                && source_fn.params.len() == 1
                && matches!(
                    self.interner.lookup(source_fn.params[0].type_id),
                    Some(TypeData::TypeParameter(param_tp))
                        if param_tp.name == source_fn.type_params[0].name
                )
                && matches!(
                    self.interner.lookup(source_fn.return_type),
                    Some(TypeData::TypeParameter(ret_tp))
                        if ret_tp.name == source_fn.type_params[0].name
                );
            if preserve_callable_alias {
                return source_ty;
            }
            // Case 3: erase to constraints/unknown
            let mut erasure_sub = TypeSubstitution::new();
            for tp in &source_fn.type_params {
                erasure_sub.insert(tp.name, tp.constraint.unwrap_or(TypeId::UNKNOWN));
            }
            let erased = FunctionShape {
                params: source_fn
                    .params
                    .iter()
                    .map(|p| ParamInfo {
                        name: p.name,
                        type_id: instantiate_type(self.interner, p.type_id, &erasure_sub),
                        optional: p.optional,
                        rest: p.rest,
                    })
                    .collect(),
                return_type: instantiate_type(self.interner, source_fn.return_type, &erasure_sub),
                this_type: source_fn
                    .this_type
                    .map(|t| instantiate_type(self.interner, t, &erasure_sub)),
                type_params: vec![],
                type_predicate: source_fn.type_predicate.as_ref().map(|pred| TypePredicate {
                    asserts: pred.asserts,
                    target: pred.target,
                    type_id: pred
                        .type_id
                        .map(|tid| instantiate_type(self.interner, tid, &erasure_sub)),
                    parameter_index: pred.parameter_index,
                }),
                is_constructor: source_fn.is_constructor,
                is_method: source_fn.is_method,
            };
            return self.interner.function(erased);
        }
        // Case 1: naked type params — fall through to instantiation

        let prev_contextual_type = self.contextual_type;
        // Suppress contextual type when source type params are fully determined by params.
        // This prevents return type from incorrectly constraining T when T already comes
        // from param positions (e.g., `identity<T>(v:T)=>T` vs `Iterator<S, boolean>`).
        //
        // When source type params are NOT fully determined by params, use the target
        // function's RETURN TYPE as the contextual type — not the whole target function.
        // compute_contextual_types (step 2.5) constrains the source function's return
        // type against the contextual type. If the contextual type is the whole target
        // function, return-only type params get incorrectly matched against the target's
        // parameter types instead of its return type. For example:
        //   pair: <T, S>(x: T) => (y: S) => { x: T; y: S }
        //   target: (x: T_zw) => (y: S_zw) => U_zw
        // Without this fix, pair's return `(y: S) => ...` would be matched against
        // the whole target `(x: T_zw) => ...`, causing S to be inferred from T_zw
        // instead of S_zw.
        self.contextual_type = if source_type_params_fully_determined_by_params {
            None
        } else {
            Some(target_fn.return_type)
        };
        let instantiated =
            self.instantiate_function_shape_from_argument_types(&source_fn, &target_param_types);
        self.contextual_type = prev_contextual_type;
        let result = self.interner.function(instantiated);

        // If the instantiation produced a function with unresolved inference
        // placeholders (e.g., because the target parameter was a Union that
        // couldn't be structurally matched against the source's Application
        // type), fall back to erasure.  This prevents leaking `__infer_*`
        // placeholders into argument types and diagnostic messages.
        //
        // Skip this fallback when the target params are inference placeholders
        // from an outer generic call. In that case, the result is expected to
        // contain those placeholders — they represent proper higher-order
        // generic relationships (e.g., compose(list, box)) and will be resolved
        // by the outer inference context.
        if source_type_params_fully_determined_by_params
            && !any_target_param_is_type_param
            && crate::type_queries::contains_infer_types_db(
                self.interner.as_type_database(),
                result,
            )
        {
            let mut erasure_sub = TypeSubstitution::new();
            for tp in &source_fn.type_params {
                erasure_sub.insert(tp.name, tp.constraint.unwrap_or(TypeId::UNKNOWN));
            }
            let erased = FunctionShape {
                params: source_fn
                    .params
                    .iter()
                    .map(|p| ParamInfo {
                        name: p.name,
                        type_id: instantiate_type(self.interner, p.type_id, &erasure_sub),
                        optional: p.optional,
                        rest: p.rest,
                    })
                    .collect(),
                return_type: instantiate_type(self.interner, source_fn.return_type, &erasure_sub),
                this_type: source_fn
                    .this_type
                    .map(|t| instantiate_type(self.interner, t, &erasure_sub)),
                type_params: vec![],
                type_predicate: source_fn.type_predicate.as_ref().map(|pred| TypePredicate {
                    asserts: pred.asserts,
                    target: pred.target,
                    type_id: pred
                        .type_id
                        .map(|tid| instantiate_type(self.interner, tid, &erasure_sub)),
                    parameter_index: pred.parameter_index,
                }),
                is_constructor: source_fn.is_constructor,
                is_method: source_fn.is_method,
            };
            return self.interner.function(erased);
        }

        result
    }

    pub(super) fn single_concrete_upper_bound(
        &self,
        infer_ctx: &mut InferenceContext<'_>,
        var: InferenceVar,
    ) -> Option<TypeId> {
        let constraints = infer_ctx.get_constraints(var)?;
        let mut concrete_upper_bounds = constraints
            .upper_bounds
            .iter()
            .copied()
            .filter(|upper| {
                !upper.is_any_unknown_or_error()
                    && !crate::visitor::contains_type_parameters(
                        self.interner.as_type_database(),
                        *upper,
                    )
                    && !crate::type_queries::contains_infer_types_db(
                        self.interner.as_type_database(),
                        *upper,
                    )
            })
            .collect::<Vec<_>>();
        concrete_upper_bounds.dedup();
        if concrete_upper_bounds.len() == 1 {
            concrete_upper_bounds.pop()
        } else {
            None
        }
    }

    fn is_mergeable_direct_inference_candidate(&self, ty: TypeId) -> bool {
        let evaluated_ty = self.interner.evaluate_type(ty);
        // Primitives (null, undefined, string, number, boolean, void, never, etc.)
        // are always safe to merge into a union — they don't indicate structural
        // ambiguity. Without this, `equal(B, D | undefined)` would discard the
        // union and use only the first candidate, causing false TS2345 errors.
        if ty.is_nullish() || ty.is_any_or_unknown() || ty == TypeId::NEVER || ty == TypeId::VOID {
            return true;
        }
        // Primitive base types are safe to merge — they're just as unambiguous as
        // null/undefined. Literal types (string/number/boolean/bigint literals)
        // are also safe since they widen to their base primitive during resolution.
        if matches!(
            ty,
            TypeId::STRING
                | TypeId::NUMBER
                | TypeId::BOOLEAN
                | TypeId::BIGINT
                | TypeId::SYMBOL
                | TypeId::OBJECT
                | TypeId::BOOLEAN_TRUE
                | TypeId::BOOLEAN_FALSE
        ) {
            return true;
        }
        // Nominal private brands should never be merged into a union during
        // direct argument inference. TypeScript fixes `T` to the first such
        // candidate and reports the later mismatch (`C` vs `D`) instead of
        // inferring `C | D`.
        if crate::type_queries::get_private_brand_name(self.interner.as_type_database(), ty)
            .is_some()
            || crate::type_queries::get_private_field_name(self.interner.as_type_database(), ty)
                .is_some()
            || crate::type_queries::get_private_brand_name(
                self.interner.as_type_database(),
                evaluated_ty,
            )
            .is_some()
            || crate::type_queries::get_private_field_name(
                self.interner.as_type_database(),
                evaluated_ty,
            )
            .is_some()
        {
            return false;
        }
        match self.interner.lookup(ty) {
            Some(
                TypeData::Literal(_)
                | TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Function(_)
                | TypeData::Callable(_)
                | TypeData::Intersection(_)
                | TypeData::Enum(..)
                | TypeData::Lazy(_)
                | TypeData::Application(_)
                | TypeData::Conditional(_)
                | TypeData::IndexAccess(..)
                | TypeData::TemplateLiteral(_)
                | TypeData::ReadonlyType(_)
                | TypeData::KeyOf(_),
            ) => true,
            Some(TypeData::Union(members)) => {
                let members = self.interner.type_list(members);
                !members.is_empty()
                    && members
                        .iter()
                        .all(|member| self.is_mergeable_direct_inference_candidate(*member))
            }
            _ => false,
        }
    }

    pub(super) fn inference_type_contains_fresh_object_or_array(&self, ty: TypeId) -> bool {
        if ty.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(ty) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => self
                .interner
                .object_shape(shape_id)
                .flags
                .contains(ObjectFlags::FRESH_LITERAL),
            Some(TypeData::Array(_) | TypeData::Tuple(_)) => true,
            Some(TypeData::Union(members)) => self
                .interner
                .type_list(members)
                .iter()
                .any(|&member| self.inference_type_contains_fresh_object_or_array(member)),
            _ => false,
        }
    }

    fn is_structural_return_inference_candidate(&self, ty: TypeId) -> bool {
        if ty.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(ty) {
            Some(
                TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Function(_)
                | TypeData::Callable(_)
                | TypeData::Intersection(_),
            ) => true,
            Some(TypeData::Union(members)) => {
                let members = self.interner.type_list(members);
                !members.is_empty()
                    && members
                        .iter()
                        .all(|member| self.is_structural_return_inference_candidate(*member))
            }
            _ => false,
        }
    }

    /// Returns `true` when the lower bounds contain literal types from different
    /// primitive families (e.g., a string literal and a number literal). This indicates
    /// heterogeneous candidates that tsc would NOT merge into a union.
    fn has_conflicting_literal_bases(&self, lower_bounds: &[TypeId]) -> bool {
        // Direct-parameter inference should keep the leftmost candidate when
        // fresh candidates disagree on primitive base. That preserves TypeScript's
        // first-wins behavior for cases like `bar<T>(x: T, y: T); bar(1, "")`,
        // where `T` should settle on `number` and the second argument should
        // still produce TS2345 instead of broadening the call to `number | string`.
        let mut seen_base: Option<TypeId> = None;
        for &ty in lower_bounds {
            let base = self.primitive_base_of(ty);
            if let Some(b) = base {
                match seen_base {
                    None => seen_base = Some(b),
                    Some(prev) if prev != b => return true,
                    _ => {}
                }
            }
        }
        false
    }

    /// Returns the primitive base TypeId for a type if it's a literal or primitive,
    /// or `None` for non-primitive types (objects, arrays, etc.).
    pub(super) fn primitive_base_of(&self, ty: TypeId) -> Option<TypeId> {
        // Check well-known primitive TypeIds first
        if matches!(
            ty,
            TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT | TypeId::SYMBOL
        ) {
            return Some(ty);
        }
        if matches!(ty, TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE) {
            return Some(TypeId::BOOLEAN);
        }
        match self.interner.lookup(ty) {
            Some(TypeData::Literal(lit)) => Some(lit.primitive_type_id()),
            _ => None,
        }
    }
}
