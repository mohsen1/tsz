//! Return-context substitution helpers for generic call inference.

use super::*;

impl<'a> CheckerState<'a> {
    fn array_or_number_index_element_type(&mut self, type_id: TypeId) -> Option<TypeId> {
        if let Some(elem) = common::array_element_type(self.ctx.types, type_id) {
            return Some(elem);
        }

        let resolved = self.resolve_lazy_type(type_id);
        let resolved = self.evaluate_type_with_env(resolved);
        let resolved = self.resolve_type_for_property_access(resolved);
        let resolver = tsz_solver::objects::IndexSignatureResolver::new(self.ctx.types);
        resolver.resolve_number_index(resolved)
    }

    pub(super) fn return_context_application_bases_match(
        &self,
        left: TypeId,
        right: TypeId,
    ) -> bool {
        use tsz_binder::SymbolId;

        if left == right {
            return true;
        }

        let symbol_for_base = |base: TypeId| {
            common::lazy_def_id(self.ctx.types, base)
                .and_then(|def_id| self.ctx.def_to_symbol_id(def_id))
                .or_else(|| {
                    crate::query_boundaries::common::type_query_symbol(self.ctx.types, base)
                        .map(|symbol_ref| SymbolId(symbol_ref.0))
                })
        };

        let left_symbol = symbol_for_base(left);
        let right_symbol = symbol_for_base(right);
        if left_symbol.is_some() && left_symbol == right_symbol {
            return true;
        }

        let base_name = |symbol_id: Option<SymbolId>| {
            symbol_id
                .and_then(|symbol_id| self.ctx.binder.get_symbol(symbol_id))
                .map(|symbol| symbol.escaped_name.as_str())
        };

        matches!(
            (base_name(left_symbol), base_name(right_symbol)),
            (Some(left_name), Some(right_name)) if left_name == right_name
        )
    }

    fn return_context_type_head(&self, type_id: TypeId) -> Option<String> {
        let display = self.format_type(type_id);
        let trimmed = display.trim();
        if !trimmed.contains('<') {
            return None;
        }

        Some(
            trimmed
                .split('<')
                .next()
                .unwrap_or(trimmed)
                .trim()
                .to_string(),
        )
    }

    fn return_context_types_share_outer_structure(&mut self, left: TypeId, right: TypeId) -> bool {
        let left_application = common::application_info(self.ctx.types, left).or_else(|| {
            let evaluated = self.evaluate_for_return_context_substitution(left);
            (evaluated != left).then(|| common::application_info(self.ctx.types, evaluated))?
        });
        let right_application = common::application_info(self.ctx.types, right).or_else(|| {
            let evaluated = self.evaluate_for_return_context_substitution(right);
            (evaluated != right).then(|| common::application_info(self.ctx.types, evaluated))?
        });
        if let (Some((left_base, _)), Some((right_base, _))) = (left_application, right_application)
            && self.return_context_application_bases_match(left_base, right_base)
        {
            return true;
        }

        let left_eval = self.evaluate_for_return_context_substitution(left);
        let right_eval = self.evaluate_for_return_context_substitution(right);
        matches!(
            (
                common::object_shape_for_type(self.ctx.types, left_eval).is_some(),
                common::object_shape_for_type(self.ctx.types, right_eval).is_some(),
                call_checker::get_contextual_signature(self.ctx.types, left_eval).is_some(),
                call_checker::get_contextual_signature(self.ctx.types, right_eval).is_some(),
            ),
            (true, true, _, _) | (_, _, true, true)
        )
    }

    pub(crate) fn collect_return_context_substitution(
        &mut self,
        source: TypeId,
        target: TypeId,
        tracked_type_params: &FxHashSet<Atom>,
        substitution: &mut crate::query_boundaries::common::TypeSubstitution,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
    ) {
        if !visited.insert((source, target)) {
            return;
        }
        // Depth guard: evaluate_type_with_env can produce fresh TypeIds, defeating
        // the visited set and causing unbounded recursion.
        if !self.ctx.enter_recursion() {
            return;
        }
        self.collect_return_context_substitution_impl(
            source,
            target,
            tracked_type_params,
            substitution,
            visited,
        );
        self.ctx.leave_recursion();
    }

    fn collect_return_context_substitution_impl(
        &mut self,
        source: TypeId,
        target: TypeId,
        tracked_type_params: &FxHashSet<Atom>,
        substitution: &mut crate::query_boundaries::common::TypeSubstitution,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
    ) {
        if let Some(tp) = common::type_param_info(self.ctx.types, source)
            && tracked_type_params.contains(&tp.name)
            && target != TypeId::UNKNOWN
            && target != TypeId::ERROR
            && !self
                .target_contains_blocking_return_context_type_params(target, tracked_type_params)
        {
            if substitution.get(tp.name).is_none() {
                substitution.insert(tp.name, target);
            }
            return;
        }

        if self.collect_awaited_return_context_substitution_by_shape(
            source,
            target,
            tracked_type_params,
            substitution,
            0,
        ) {
            return;
        }

        let awaited_source = self.evaluate_awaited_application_for_assignability(source);
        if awaited_source != source {
            self.collect_return_context_substitution(
                awaited_source,
                target,
                tracked_type_params,
                substitution,
                visited,
            );
            if !substitution.is_empty() {
                return;
            }
        }

        // When target (expected return type) is a type param and source (actual return type)
        // is a concrete type, infer the type param from the source. This handles JSX
        // intra-expression inference like:
        //   <Foo a={(x) => 10} b={(arg) => arg.toString()} />
        // where Props<T> has a: (x: string) => T and b: (arg: T) => void.
        // The actual return type of `(x) => 10` is `number`, and the expected return
        // type is `T`, so we infer T = number.
        if let Some(tp) = common::type_param_info(self.ctx.types, target)
            && tracked_type_params.contains(&tp.name)
            && source != TypeId::UNKNOWN
            && source != TypeId::ERROR
            && !common::references_any_type_param_named(self.ctx.types, source, tracked_type_params)
        {
            if substitution.get(tp.name).is_none() {
                substitution.insert(tp.name, source);
            }
            return;
        }

        if let (Some(source_members), Some(target_members)) = (
            common::union_members(self.ctx.types, source),
            common::union_members(self.ctx.types, target),
        ) {
            let source_members: Vec<_> = source_members
                .into_iter()
                .filter(|member| *member != TypeId::NULL && *member != TypeId::UNDEFINED)
                .collect();
            let target_members: Vec<_> = target_members
                .into_iter()
                .filter(|member| *member != TypeId::NULL && *member != TypeId::UNDEFINED)
                .collect();
            let all_source_members_are_tracked_params = !source_members.is_empty()
                && source_members.iter().all(|member| {
                    common::type_param_info(self.ctx.types, *member)
                        .is_some_and(|tp| tracked_type_params.contains(&tp.name))
                });
            if all_source_members_are_tracked_params && source_members.len() == target_members.len()
            {
                for (source_member, target_member) in source_members
                    .iter()
                    .copied()
                    .zip(target_members.iter().copied())
                {
                    if let Some(tp) = common::type_param_info(self.ctx.types, source_member)
                        && substitution.get(tp.name).is_none()
                        && target_member != TypeId::UNKNOWN
                        && target_member != TypeId::ERROR
                        && !self.target_contains_blocking_return_context_type_params(
                            target_member,
                            tracked_type_params,
                        )
                    {
                        substitution.insert(tp.name, target_member);
                    }
                }
                if !substitution.is_empty() {
                    return;
                }
            }
            let mut matched_structured_member = false;
            for source_member in source_members.iter().copied() {
                if common::type_param_info(self.ctx.types, source_member)
                    .is_some_and(|tp| tracked_type_params.contains(&tp.name))
                {
                    continue;
                }
                for target_member in target_members.iter().copied() {
                    if self.return_context_types_share_outer_structure(source_member, target_member)
                    {
                        matched_structured_member = true;
                        self.collect_return_context_substitution(
                            source_member,
                            target_member,
                            tracked_type_params,
                            substitution,
                            visited,
                        );
                    }
                }
            }
            if matched_structured_member {
                return;
            }
        }

        // When source (return type) is a union like `E | null`, decompose it
        // and try each non-nullish member against the target contextual type.
        // This handles the common pattern `querySelector<E>(...): E | null`
        // where the contextual type `SVGRectElement` should infer E = SVGRectElement.
        if let Some(source_members) = common::union_members(self.ctx.types, source) {
            for member in source_members
                .into_iter()
                .filter(|member| *member != TypeId::NULL && *member != TypeId::UNDEFINED)
            {
                self.collect_return_context_substitution(
                    member,
                    target,
                    tracked_type_params,
                    substitution,
                    visited,
                );
            }
            if !substitution.is_empty() {
                return;
            }
        }

        if let Some(target_members) = common::union_members(self.ctx.types, target) {
            let before_len = substitution.len();
            for member in target_members
                .into_iter()
                .filter(|member| *member != TypeId::NULL && *member != TypeId::UNDEFINED)
            {
                self.collect_return_context_substitution(
                    source,
                    member,
                    tracked_type_params,
                    substitution,
                    visited,
                );
                if substitution.len() > before_len {
                    return;
                }
            }
        }

        if let Some(inner) = common::unwrap_readonly_or_noinfer(self.ctx.types, target) {
            self.collect_return_context_substitution(
                source,
                inner,
                tracked_type_params,
                substitution,
                visited,
            );
            if !substitution.is_empty() {
                return;
            }
        }

        if let Some(inner) = common::unwrap_readonly_or_noinfer(self.ctx.types, source) {
            self.collect_return_context_substitution(
                inner,
                target,
                tracked_type_params,
                substitution,
                visited,
            );
            if !substitution.is_empty() {
                return;
            }
        }
        let source_evaluated_for_wrapper = self.evaluate_for_return_context_substitution(source);
        if source_evaluated_for_wrapper != source
            && let Some(inner) =
                common::unwrap_readonly_or_noinfer(self.ctx.types, source_evaluated_for_wrapper)
        {
            self.collect_return_context_substitution(
                inner,
                target,
                tracked_type_params,
                substitution,
                visited,
            );
            if !substitution.is_empty() {
                return;
            }
        }

        let source_application = common::application_info(self.ctx.types, source).or_else(|| {
            let evaluated = self.evaluate_for_return_context_substitution(source);
            (evaluated != source).then(|| common::application_info(self.ctx.types, evaluated))?
        });
        let target_application = common::application_info(self.ctx.types, target).or_else(|| {
            let evaluated = self.evaluate_for_return_context_substitution(target);
            (evaluated != target).then(|| common::application_info(self.ctx.types, evaluated))?
        });

        // Handle Application types like Readonly<T>, Promise<T>, etc.
        // When source is Application(Base, [args...]) and target is NOT
        // a matching Application, decompose the source Application's type
        // arguments and recursively match each against the target. This
        // handles cases like Readonly<T> where T needs to be inferred from
        // the contextual type (e.g., readonly [string, number][]).
        if let Some((source_base, source_args)) = source_application.as_ref() {
            // Only try if target is not already matched as Application(same_base)
            // (that case is handled later at the Application-Application matching).
            let target_same_base =
                target_application
                    .as_ref()
                    .is_some_and(|(target_base, target_args)| {
                        self.return_context_application_bases_match(*source_base, *target_base)
                            && target_args.len() == source_args.len()
                    })
                    || (target_application.is_none()
                        && self.return_context_type_head(source)
                            == self.return_context_type_head(target));
            if !target_same_base {
                // When the source Application evaluates to a callable type
                // (e.g., Mapper<T, U> = (x: T) => U) and the target is also
                // a callable type (e.g., (x: string) => number), skip the
                // naive decomposition that would map each type arg to the
                // whole target. The function matching below (via
                // get_contextual_signature) will correctly decompose the
                // evaluated callable's parameters and return type.
                let source_eval_for_guard = self.evaluate_for_return_context_substitution(source);
                let target_eval_for_guard = self.evaluate_for_return_context_substitution(target);
                let source_base_is_callable =
                    call_checker::get_contextual_signature(self.ctx.types, *source_base).is_some();
                let source_base_has_callable_shape =
                    common::callable_shape_for_type(self.ctx.types, *source_base).is_some()
                        || common::function_shape_for_type(self.ctx.types, *source_base).is_some();
                let source_evals_to_callable =
                    call_checker::get_contextual_signature(self.ctx.types, source).is_some()
                        || source_base_is_callable
                        || source_base_has_callable_shape
                        || (source_eval_for_guard != source
                            && call_checker::get_contextual_signature(
                                self.ctx.types,
                                source_eval_for_guard,
                            )
                            .is_some());
                let target_is_callable =
                    call_checker::get_contextual_signature(self.ctx.types, target).is_some();
                let both_evaluate_to_structural_objects =
                    common::object_shape_for_type(self.ctx.types, source_eval_for_guard).is_some()
                        && common::object_shape_for_type(self.ctx.types, target_eval_for_guard)
                            .is_some();
                if target_application.is_some() {
                    // When both sides are Applications of different bases, mapping each
                    // source type arg directly to the whole target wrapper is almost
                    // always wrong (e.g. AssignAction<T> vs ActionFunction<U> would infer
                    // T = ActionFunction<U>). Let the later structural/application-aware
                    // matching determine whether the wrappers reveal a meaningful mapping.
                } else if source_evals_to_callable && target_is_callable {
                    // Don't decompose — let function matching below handle it
                } else if both_evaluate_to_structural_objects {
                    // Differing application wrappers like AssignAction<T> and
                    // ActionFunction<U> often carry the tracked type parameter on
                    // marker/object properties after evaluation. Decomposing their
                    // type arguments directly would bind T to the whole target
                    // wrapper instead of letting structural matching infer from the
                    // evaluated property shapes.
                } else {
                    // Special case: when the source Application evaluates to an
                    // iterable-like interface (e.g., Iterable<T>) and the target
                    // is an Array or Tuple, skip the naive decomposition that would
                    // map T to the full array type. The solver's constraint
                    // collection has proper iterable matching that extracts the
                    // element type correctly. Without this guard, `Iterable<T>`
                    // matched against `number[]` infers T = number[] instead of
                    // letting the solver infer T = number.
                    let target_is_array_like =
                        self.array_or_number_index_element_type(target).is_some();
                    let source_is_iterable_like = target_is_array_like
                        && !source_args.is_empty()
                        && self.source_is_iterable_like_for_substitution(source);
                    if source_is_iterable_like {
                        // Extract the array element type and widen it (e.g., 0|2|8 → number)
                        // before mapping against the source type args. This prevents the
                        // contextual substitution from using unwidened literal types that
                        // would cause false TS2345 mismatches.
                        let elem = self
                            .array_or_number_index_element_type(target)
                            .expect("array target should have element type");
                        let widened_elem = tsz_solver::operations::widening::widen_literal_type(
                            self.ctx.types,
                            elem,
                        );
                        for &source_arg in source_args {
                            self.collect_return_context_substitution(
                                source_arg,
                                widened_elem,
                                tracked_type_params,
                                substitution,
                                visited,
                            );
                        }
                        if !substitution.is_empty() {
                            return;
                        }
                    } else {
                        for &source_arg in source_args {
                            self.collect_return_context_substitution(
                                source_arg,
                                target,
                                tracked_type_params,
                                substitution,
                                visited,
                            );
                        }
                        if !substitution.is_empty() {
                            return;
                        }
                    }
                }
            }
        }

        let source_eval = self.evaluate_for_return_context_substitution(source);
        let target_eval = self.evaluate_for_return_context_substitution(target);

        if let (Some((source_base, source_args)), Some((target_base, target_args))) =
            (source_application.as_ref(), target_application.as_ref())
            && self.return_context_application_bases_match(*source_base, *target_base)
            && source_args.len() == target_args.len()
        {
            for (source_arg, target_arg) in source_args.iter().zip(target_args.iter()) {
                self.collect_return_context_substitution(
                    *source_arg,
                    *target_arg,
                    tracked_type_params,
                    substitution,
                    visited,
                );
            }
            return;
        }

        let function_info = match (
            call_checker::get_contextual_signature(self.ctx.types, source),
            call_checker::get_contextual_signature(self.ctx.types, target),
        ) {
            (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
            _ => match (
                call_checker::get_contextual_signature(self.ctx.types, source_eval),
                call_checker::get_contextual_signature(self.ctx.types, target_eval),
            ) {
                (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
                _ => None,
            },
        };

        if let Some((source_fn, target_fn)) = function_info
            && source_fn.params.len() <= target_fn.params.len()
        {
            let substitution_len_before_callable = substitution.len();
            let target_fn =
                instantiate_contextual_target_shape_for_return_context(self.ctx.types, &target_fn);
            let mut target_index = 0usize;
            for source_param in &source_fn.params {
                let target_type = if source_param.rest {
                    let remaining = &target_fn.params[target_index..];
                    if remaining.len() == 1 && remaining[0].rest {
                        remaining[0].type_id
                    } else {
                        self.ctx
                            .types
                            .factory()
                            .tuple(common::params_to_tuple_elements(remaining))
                    }
                } else {
                    let Some(target_param) = target_fn.params.get(target_index) else {
                        break;
                    };
                    target_index += 1;
                    target_param.type_id
                };
                // When the source param is a type alias Application (e.g.,
                // Either<E, A>) and the target param is its evaluated form
                // (e.g., Left<string> | Right<number>), evaluate the source
                // param first so both sides are at the same level. Without
                // this, the Application vs union mismatch causes incorrect
                // decomposition (e.g., A → Left<string> instead of A → number).
                let source_param_type =
                    if common::application_info(self.ctx.types, source_param.type_id).is_some()
                        && common::union_members(self.ctx.types, target_type).is_some()
                    {
                        let evaluated = self.evaluate_type_with_env(source_param.type_id);
                        if evaluated != source_param.type_id {
                            evaluated
                        } else {
                            source_param.type_id
                        }
                    } else {
                        source_param.type_id
                    };
                self.collect_return_context_substitution(
                    source_param_type,
                    target_type,
                    tracked_type_params,
                    substitution,
                    visited,
                );
                if source_param.rest {
                    break;
                }
            }
            self.collect_return_context_substitution(
                source_fn.return_type,
                target_fn.return_type,
                tracked_type_params,
                substitution,
                visited,
            );
            if substitution.len() > substitution_len_before_callable
                || (source_application.is_none() && target_application.is_none())
            {
                return;
            }
        }

        if let (Some(source_elems), Some(target_elems)) = (
            common::tuple_elements(self.ctx.types, source),
            common::tuple_elements(self.ctx.types, target),
        ) {
            for (source_elem, target_elem) in source_elems.iter().zip(target_elems.iter()) {
                self.collect_return_context_substitution(
                    source_elem.type_id,
                    target_elem.type_id,
                    tracked_type_params,
                    substitution,
                    visited,
                );
            }
            return;
        }

        let source_array_elem = common::array_element_type(self.ctx.types, source);
        let target_array_elem = common::array_element_type(self.ctx.types, target);
        if let (Some(source_elem), Some(target_elem)) = (source_array_elem, target_array_elem) {
            self.collect_return_context_substitution(
                source_elem,
                target_elem,
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        if let Some(source_elem) = source_array_elem
            && let Some((_target_base, target_args)) =
                common::application_info(self.ctx.types, target)
            && target_args.len() == 1
        {
            self.collect_return_context_substitution(
                source_elem,
                target_args[0],
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        if let Some(source_elem) = source_array_elem
            && let Some(iterator_info) = common::get_iterator_info(self.ctx.types, target, false)
        {
            self.collect_return_context_substitution(
                source_elem,
                iterator_info.yield_type,
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        // Structural property matching: when either side evaluates to a callable/object
        // wrapper with marker properties (for example ActionFunction<T> carrying
        // `_out_TActor?: T`), recurse through matching properties to recover the
        // underlying type-parameter mapping.
        let source_properties =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, source_eval)
                .map(|shape| shape.properties.clone())
                .or_else(|| {
                    crate::query_boundaries::common::callable_shape_for_type(
                        self.ctx.types,
                        source_eval,
                    )
                    .map(|shape| shape.properties.clone())
                });
        let target_properties =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target_eval)
                .map(|shape| shape.properties.clone())
                .or_else(|| {
                    crate::query_boundaries::common::callable_shape_for_type(
                        self.ctx.types,
                        target_eval,
                    )
                    .map(|shape| shape.properties.clone())
                });
        if let (Some(source_properties), Some(target_properties)) =
            (source_properties.as_ref(), target_properties.as_ref())
        {
            for source_prop in source_properties.iter() {
                if let Some(target_prop) =
                    common::find_matching_property(target_properties, source_prop.name)
                {
                    self.collect_return_context_substitution(
                        source_prop.type_id,
                        target_prop.type_id,
                        tracked_type_params,
                        substitution,
                        visited,
                    );
                }
            }
        }
    }

    pub(crate) fn compute_return_context_substitution_from_shape(
        &mut self,
        shape: &tsz_solver::FunctionShape,
        contextual_type: Option<TypeId>,
    ) -> crate::query_boundaries::common::TypeSubstitution {
        let Some(contextual_type) = contextual_type else {
            return crate::query_boundaries::common::TypeSubstitution::new();
        };
        let tracked_type_params: FxHashSet<_> =
            shape.type_params.iter().map(|tp| tp.name).collect();
        if tracked_type_params.is_empty() {
            return crate::query_boundaries::common::TypeSubstitution::new();
        }

        let mut substitution = crate::query_boundaries::common::TypeSubstitution::new();
        let mut visited = FxHashSet::default();
        self.collect_return_context_substitution(
            shape.return_type,
            contextual_type,
            &tracked_type_params,
            &mut substitution,
            &mut visited,
        );
        substitution
    }
}
