impl<'a> CheckerState<'a> {
    /// Merge overflow flags into the checker context (sticky: only ever sets to `true`).
    ///
    /// Callers that need a fresh read must reset the context fields before
    /// invoking the relation.
    #[inline]
    pub(super) fn propagate_overflow_flags(&self, depth_exceeded: bool, iteration_exceeded: bool) {
        let mut overflow = self.ctx.relation_overflow.get();
        overflow.merge(depth_exceeded, iteration_exceeded);
        self.ctx.relation_overflow.set(overflow);
    }

    pub(crate) fn callable_has_own_generic_signatures(&self, type_id: TypeId) -> bool {
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
        {
            return !shape.type_params.is_empty();
        }
        if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
        {
            return shape
                .call_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty())
                || shape
                    .construct_signatures
                    .iter()
                    .any(|sig| !sig.type_params.is_empty());
        }
        false
    }

    /// Check if a callable type's parameters contain type parameters within intersections.
    /// This distinguishes narrowed callback parameters (e.g., `(x: number & T) => void`)
    /// from callbacks with standalone enclosing-scope type parameters (e.g., `(x: T) => void`).
    pub(crate) fn callable_params_contain_type_param_intersection(&self, type_id: TypeId) -> bool {
        let params = if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
        {
            shape.params.iter().map(|p| p.type_id).collect::<Vec<_>>()
        } else if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
        {
            shape
                .call_signatures
                .iter()
                .flat_map(|sig| sig.params.iter().map(|p| p.type_id))
                .collect::<Vec<_>>()
        } else {
            return false;
        };
        params.iter().any(|&param_type| {
            if let Some(members) =
                crate::query_boundaries::common::intersection_members(self.ctx.types, param_type)
            {
                members.iter().any(|&m| {
                    crate::query_boundaries::assignability::contains_type_parameters(
                        self.ctx.types,
                        m,
                    )
                })
            } else {
                false
            }
        })
    }

    /// Check if an argument node is a callback (arrow function or function expression)
    /// with unannotated parameters that rely on contextual typing.
    pub(crate) fn arg_is_callback_with_unannotated_params(&self, arg_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };

        let is_callback = node.kind == syntax_kind_ext::ARROW_FUNCTION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION;

        if !is_callback {
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                return self.arg_is_callback_with_unannotated_params(paren.expression);
            }
            return false;
        }

        let Some(func) = self.ctx.arena.get_function(node) else {
            return false;
        };

        func.parameters.nodes.iter().any(|&param_idx| {
            self.ctx
                .arena
                .get(param_idx)
                .and_then(|pn| self.ctx.arena.get_parameter(pn))
                .is_some_and(|p| {
                    p.type_annotation.is_none()
                        && self.ctx.arena.get(p.name).is_some_and(|name_node| {
                            name_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
                                && name_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
                        })
                })
        })
    }

    /// Returns the parameter count of a callback argument's function expression.
    /// Returns `None` if `arg_idx` is not an arrow/function expression (or a
    /// parenthesized one).
    fn callback_argument_param_count(&self, arg_idx: NodeIndex) -> Option<usize> {
        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.ctx.arena.get_parenthesized(node)?;
            return self.callback_argument_param_count(paren.expression);
        }
        if node.kind != syntax_kind_ext::ARROW_FUNCTION
            && node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.ctx.arena.get_function(node)?;
        Some(func.parameters.nodes.len())
    }

    /// Returns true when `target` exposes at least one callable signature whose
    /// parameter list can supply contextual types for every parameter of the
    /// unannotated callback at `arg_idx`.
    ///
    /// A signature can supply contextual types when it has a rest parameter, or
    /// when its fixed parameter count is at least the source callback's
    /// parameter count. When the target is not a recognizably callable type, we
    /// conservatively answer `true` so that the existing suppression behavior
    /// for non-trivial target shapes (unions, generics, etc.) is preserved —
    /// the bug we are guarding against is the concrete case where the target
    /// has *fewer* parameters than the source.
    pub(crate) fn target_can_contextually_type_callback_params(
        &self,
        arg_idx: NodeIndex,
        target: TypeId,
    ) -> bool {
        let Some(source_param_count) = self.callback_argument_param_count(arg_idx) else {
            return true;
        };
        let db = self.ctx.types;
        if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(db, target) {
            return signature_has_param_capacity(&shape.params, source_param_count);
        }
        if let Some(shape) = crate::query_boundaries::common::callable_shape_for_type(db, target) {
            let any_call_ok = shape
                .call_signatures
                .iter()
                .any(|sig| signature_has_param_capacity(&sig.params, source_param_count));
            let any_construct_ok = shape
                .construct_signatures
                .iter()
                .any(|sig| signature_has_param_capacity(&sig.params, source_param_count));
            return any_call_ok || any_construct_ok;
        }
        true
    }

    /// Returns true when a callback-like function type still has unresolved
    /// `any`/`unknown` parameter types, meaning contextual typing did not
    /// concretely bind its parameters yet.
    pub(crate) fn callback_type_params_are_unresolved(&self, arg_type: TypeId) -> bool {
        if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
            self.ctx.types.as_type_database(),
            arg_type,
        ) {
            shape.params.is_empty()
                || shape
                    .params
                    .iter()
                    .all(|p| matches!(p.type_id, TypeId::ANY | TypeId::UNKNOWN))
        } else {
            false
        }
    }

    fn normalize_nested_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        // Depth guard: prevents stack overflow from mutually recursive types
        // (e.g., Foo<T> ↔ Bar<T>) where each fresh visited set misses cross-function cycles.
        thread_local! { static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) }; }
        let depth = DEPTH.with(|d| {
            let v = d.get();
            d.set(v + 1);
            v
        });
        if depth >= 10 {
            DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            return type_id;
        }
        let mut visited = FxHashSet::default();
        let result = self.normalize_nested_type_for_assignability_inner(type_id, &mut visited);
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        result
    }

    fn normalize_nested_type_for_assignability_inner(
        &mut self,
        type_id: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> TypeId {
        if !visited.insert(type_id) {
            return type_id;
        }

        let resolved = self.resolve_type_query_type(type_id);
        let evaluated = if crate::query_boundaries::common::type_application(
            self.ctx.types,
            resolved,
        )
        .is_some()
        {
            self.evaluate_type_for_assignability(resolved)
        } else {
            self.evaluate_type_with_env(resolved)
        };
        let type_id = if evaluated == TypeId::UNKNOWN && resolved != TypeId::UNKNOWN {
            resolved
        } else if evaluated != resolved {
            evaluated
        } else {
            resolved
        };

        if let Some(inner) =
            crate::query_boundaries::common::get_readonly_inner(self.ctx.types, type_id)
        {
            let normalized = self.normalize_nested_type_for_assignability_inner(inner, visited);
            if normalized != inner {
                self.ctx.types.readonly_type(normalized)
            } else {
                type_id
            }
        } else if let Some(inner) =
            crate::query_boundaries::common::get_noinfer_inner(self.ctx.types, type_id)
        {
            let normalized = self.normalize_nested_type_for_assignability_inner(inner, visited);
            if normalized != inner {
                self.ctx.types.no_infer(normalized)
            } else {
                type_id
            }
        } else if let Some(elem) =
            crate::query_boundaries::common::array_element_type(self.ctx.types, type_id)
        {
            if crate::query_boundaries::common::is_array_type(self.ctx.types, type_id) {
                let normalized = self.normalize_nested_type_for_assignability_inner(elem, visited);
                if normalized != elem {
                    self.ctx.types.array(normalized)
                } else {
                    type_id
                }
            } else {
                type_id
            }
        } else if let Some(elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id)
        {
            if crate::query_boundaries::common::is_tuple_type(self.ctx.types, type_id) {
                let mut changed = false;
                let normalized_elements: Vec<_> = elements
                    .iter()
                    .map(|elem| {
                        let normalized = self
                            .normalize_nested_type_for_assignability_inner(elem.type_id, visited);
                        if normalized != elem.type_id {
                            changed = true;
                        }
                        tsz_solver::TupleElement {
                            type_id: normalized,
                            name: elem.name,
                            optional: elem.optional,
                            rest: elem.rest,
                        }
                    })
                    .collect();
                if changed {
                    self.ctx.types.factory().tuple(normalized_elements)
                } else {
                    type_id
                }
            } else {
                type_id
            }
        } else if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            let mut changed = false;
            let normalized_members: Vec<_> = members
                .iter()
                .map(|&member| {
                    let normalized =
                        self.normalize_nested_type_for_assignability_inner(member, visited);
                    if normalized != member {
                        changed = true;
                    }
                    normalized
                })
                .collect();
            if changed {
                self.ctx.types.factory().union(normalized_members)
            } else {
                type_id
            }
        } else if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        {
            let mut changed = false;
            let normalized_members: Vec<_> = members
                .iter()
                .map(|&member| {
                    let normalized =
                        self.normalize_nested_type_for_assignability_inner(member, visited);
                    if normalized != member {
                        changed = true;
                    }
                    normalized
                })
                .collect();
            if changed {
                self.ctx.types.factory().intersection(normalized_members)
            } else {
                type_id
            }
        } else {
            type_id
        }
    }

    fn normalize_function_shape_for_assignability(
        &mut self,
        shape: &tsz_solver::FunctionShape,
    ) -> Option<tsz_solver::FunctionShape> {
        let own_tp_names: Vec<_> = shape.type_params.iter().map(|tp| tp.name).collect();

        let mut changed = false;
        let params = shape
            .params
            .iter()
            .map(|param| {
                let skip = !own_tp_names.is_empty()
                    && own_tp_names.iter().any(|&name| {
                        crate::query_boundaries::common::contains_type_parameter_named(
                            self.ctx.types,
                            param.type_id,
                            name,
                        )
                    });
                let evaluated = if skip {
                    param.type_id
                } else {
                    self.normalize_nested_type_for_assignability(param.type_id)
                };
                if evaluated != param.type_id {
                    changed = true;
                }
                tsz_solver::ParamInfo {
                    name: param.name,
                    type_id: evaluated,
                    optional: param.optional,
                    rest: param.rest,
                }
            })
            .collect();
        let this_type = shape.this_type.map(|this_type| {
            let evaluated = self.normalize_nested_type_for_assignability(this_type);
            if evaluated != this_type {
                changed = true;
            }
            evaluated
        });
        let return_type = {
            let skip_for_type_params = !own_tp_names.is_empty()
                && own_tp_names.iter().any(|&name| {
                    crate::query_boundaries::common::contains_type_parameter_named(
                        self.ctx.types,
                        shape.return_type,
                        name,
                    )
                });
            let skip_for_type_query = crate::query_boundaries::common::is_type_query_type(
                self.ctx.types,
                shape.return_type,
            );
            let skip_for_conditional = crate::query_boundaries::common::is_conditional_type(
                self.ctx.types,
                shape.return_type,
            );
            let skip = skip_for_type_params || skip_for_type_query || skip_for_conditional;
            let evaluated = if skip {
                shape.return_type
            } else {
                self.normalize_nested_type_for_assignability(shape.return_type)
            };
            if evaluated != shape.return_type {
                changed = true;
            }
            evaluated
        };
        let type_predicate = shape.type_predicate.as_ref().map(|predicate| {
            let type_id = predicate.type_id.map(|type_id| {
                let evaluated = self.normalize_nested_type_for_assignability(type_id);
                if evaluated != type_id {
                    changed = true;
                }
                evaluated
            });
            tsz_solver::TypePredicate {
                asserts: predicate.asserts,
                target: predicate.target,
                type_id,
                parameter_index: predicate.parameter_index,
            }
        });

        changed.then_some(tsz_solver::FunctionShape {
            type_params: shape.type_params.clone(),
            params,
            this_type,
            return_type,
            type_predicate,
            is_constructor: shape.is_constructor,
            is_method: shape.is_method,
        })
    }

    fn normalize_callable_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
        {
            let result = self
                .normalize_function_shape_for_assignability(&shape)
                .map(|shape| self.ctx.types.factory().function(shape))
                .unwrap_or(type_id);
            return result;
        }
        if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
        {
            let mut changed = false;
            let call_signatures: Vec<_> = shape
                .call_signatures
                .iter()
                .map(|sig| {
                    let normalized = self.normalize_function_shape_for_assignability(
                        &tsz_solver::FunctionShape {
                            type_params: sig.type_params.clone(),
                            params: sig.params.clone(),
                            this_type: sig.this_type,
                            return_type: sig.return_type,
                            type_predicate: sig.type_predicate,
                            is_constructor: false,
                            is_method: false,
                        },
                    );
                    if normalized.is_some() {
                        changed = true;
                    }
                    normalized.map_or_else(
                        || sig.clone(),
                        |shape| tsz_solver::CallSignature {
                            type_params: shape.type_params,
                            params: shape.params,
                            this_type: shape.this_type,
                            return_type: shape.return_type,
                            type_predicate: shape.type_predicate,
                            is_method: sig.is_method,
                        },
                    )
                })
                .collect();
            let construct_signatures: Vec<_> = shape
                .construct_signatures
                .iter()
                .map(|sig| {
                    let normalized = self.normalize_function_shape_for_assignability(
                        &tsz_solver::FunctionShape {
                            type_params: sig.type_params.clone(),
                            params: sig.params.clone(),
                            this_type: sig.this_type,
                            return_type: sig.return_type,
                            type_predicate: sig.type_predicate,
                            is_constructor: true,
                            is_method: false,
                        },
                    );
                    if normalized.is_some() {
                        changed = true;
                    }
                    normalized.map_or_else(
                        || sig.clone(),
                        |shape| tsz_solver::CallSignature {
                            type_params: shape.type_params,
                            params: shape.params,
                            this_type: shape.this_type,
                            return_type: shape.return_type,
                            type_predicate: shape.type_predicate,
                            is_method: sig.is_method,
                        },
                    )
                })
                .collect();

            if changed {
                self.ctx
                    .types
                    .factory()
                    .callable(tsz_solver::CallableShape {
                        call_signatures,
                        construct_signatures,
                        properties: shape.properties.clone(),
                        string_index: shape.string_index,
                        number_index: shape.number_index,
                        symbol: shape.symbol,
                        is_abstract: shape.is_abstract,
                    })
            } else {
                type_id
            }
        } else {
            type_id
        }
    }

    pub(crate) fn get_keyof_type_keys(
        &mut self,
        type_id: TypeId,
        db: &dyn tsz_solver::construction::TypeDatabase,
    ) -> FxHashSet<Atom> {
        if let Some(keyof_type) = get_keyof_type(db, type_id)
            && let Some(key_type) = keyof_object_properties(db, keyof_type)
            && let Some(members) = get_union_members(db, key_type)
        {
            return members
                .into_iter()
                .filter_map(|m| {
                    if let Some(str_lit) = get_string_literal_value(db, m) {
                        return Some(str_lit);
                    }
                    None
                })
                .collect();
        }
        FxHashSet::default()
    }

    fn typeof_this_comparison_literal(
        &self,
        left: NodeIndex,
        right: NodeIndex,
        this_ref: NodeIndex,
    ) -> Option<&str> {
        if self.is_typeof_this_target(left, this_ref) {
            return self.string_literal_text(right);
        }
        if self.is_typeof_this_target(right, this_ref) {
            return self.string_literal_text(left);
        }
        None
    }

    fn is_typeof_this_target(&self, expr: NodeIndex, this_ref: NodeIndex) -> bool {
        let expr = self.ctx.arena.skip_parenthesized(expr);
        let Some(node) = self.ctx.arena.get(expr) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return false;
        }
        let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
            return false;
        };
        if unary.operator != SyntaxKind::TypeOfKeyword as u16 {
            return false;
        }
        let operand = self.ctx.arena.skip_parenthesized(unary.operand);
        if operand == this_ref {
            return true;
        }
        self.ctx
            .arena
            .get(operand)
            .is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16)
    }

    fn string_literal_text(&self, idx: NodeIndex) -> Option<&str> {
        let idx = self.ctx.arena.skip_parenthesized(idx);
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return self
                .ctx
                .arena
                .get_literal(node)
                .map(|lit| lit.text.as_str());
        }
        None
    }

    pub(crate) fn narrow_this_from_enclosing_typeof_guard(
        &self,
        source_idx: NodeIndex,
        source: TypeId,
    ) -> TypeId {
        let is_this_source = self
            .ctx
            .arena
            .get(source_idx)
            .is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16);
        if !is_this_source {
            return source;
        }

        let mut current = source_idx;
        let mut depth = 0usize;
        while depth < 256 {
            depth += 1;
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(current) else {
                break;
            };
            if parent_node.kind != syntax_kind_ext::IF_STATEMENT {
                continue;
            }
            let Some(if_stmt) = self.ctx.arena.get_if_statement(parent_node) else {
                continue;
            };
            if !self.is_node_within(source_idx, if_stmt.then_statement) {
                continue;
            }
            let Some(cond_node) = self.ctx.arena.get(if_stmt.expression) else {
                continue;
            };
            if cond_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(bin) = self.ctx.arena.get_binary_expr(cond_node) else {
                continue;
            };
            let is_equality = bin.operator_token == SyntaxKind::EqualsEqualsEqualsToken as u16
                || bin.operator_token == SyntaxKind::EqualsEqualsToken as u16;
            if !is_equality {
                continue;
            }
            if let Some(type_name) =
                self.typeof_this_comparison_literal(bin.left, bin.right, source_idx)
            {
                return NarrowingContext::new(self.ctx.types).narrow_by_typeof(source, type_name);
            }
        }

        source
    }

    /// Ensure relation preconditions (lazy refs + application symbols) for one type.
    pub(crate) fn ensure_relation_input_ready(&mut self, type_id: TypeId) {
        if type_id.is_intrinsic() {
            return;
        }
        // Do NOT gate on global_resolution_fuel_exhausted() here.  The inner
        // guards inside ensure_refs_resolved (and ensure_application_symbols_resolved)
        // already exit the materialization worklist when global fuel is exhausted,
        // bounding total work per call to O(1).  Gating the entire readiness step
        // here caused subsequent DOM/lib relation checks in the same file to skip
        // their input step entirely, silently dropping TS2322/TS2345 diagnostics
        // after the first large-lib type graph was materialized (issue #12144).
        self.ensure_refs_resolved(type_id);
        self.ensure_application_symbols_resolved(type_id);
    }

    /// Ensure relation preconditions (lazy refs + application symbols) for multiple types.
    pub(crate) fn ensure_relation_inputs_ready(&mut self, type_ids: &[TypeId]) {
        for &type_id in type_ids {
            self.ensure_relation_input_ready(type_id);
        }
    }

    /// Centralized suppression for TS2322-style assignability diagnostics.
    pub(crate) fn should_suppress_assignability_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let is_type_alias_application = |type_id: TypeId| {
            crate::query_boundaries::common::type_application(self.ctx.types, type_id)
                .and_then(|app| {
                    crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)
                })
                .and_then(|def_id| self.ctx.definition_store.get(def_id))
                .is_some_and(|def| def.kind == tsz_solver::def::DefKind::TypeAlias)
        };
        if is_type_alias_application(source)
            && is_type_alias_application(target)
            && crate::query_boundaries::assignability::are_types_structurally_identical(
                self.ctx.types,
                &self.ctx,
                source,
                target,
            )
        {
            return true;
        }
        if self.recursive_conditional_path_alias_mismatch_is_tsc_bailout(source, target) {
            return true;
        }

        if crate::query_boundaries::common::keyof_inner_type(self.ctx.types, target).is_some() {
            let resolved_keyof =
                crate::query_boundaries::state::type_environment::evaluate_type_with_resolver(
                    self.ctx.types,
                    &self.ctx,
                    target,
                );
            if resolved_keyof != target
                && self
                    .keyof_diagnostic_suppression_relation_outcome(source, resolved_keyof)
                    .related
            {
                return true;
            }
            if self.keyof_interface_augmentation_literals_cover_source(source, target) {
                return true;
            }
        }

        let evaluated_target_for_invalid_mapped = self.ctx.types.evaluate_type(target);
        if self.type_contains_invalid_mapped_key_type(target)
            || self.type_contains_invalid_mapped_key_type(evaluated_target_for_invalid_mapped)
        {
            return true;
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, target)
        {
            let has_indexed_access = members.iter().any(|&member| {
                crate::query_boundaries::common::is_index_access_type(self.ctx.types, member)
            });
            if has_indexed_access {
                let indexed_access_has_errors = members.iter().any(|&member| {
                    if crate::query_boundaries::common::is_index_access_type(self.ctx.types, member)
                    {
                        Self::type_contains_error_application(self.ctx.types, member)
                    } else {
                        false
                    }
                });
                let union_has_errors =
                    Self::type_contains_error_application(self.ctx.types, target);
                if !indexed_access_has_errors && !union_has_errors {
                    return false;
                }
            }
        }

        // Check if a type contains an error application (e.g., error<any>)
        // This happens when type resolution fails for qualified names like React.ReactElement
        // in function return type positions. Suppress the false positive TS2322.
        let contains_error_application =
            |type_id: TypeId| Self::type_contains_error_application(self.ctx.types, type_id);
        let evaluated_target_for_infer_suppression = self.ctx.types.evaluate_type(target);
        let target_is_conditional_for_infer_suppression =
            crate::query_boundaries::common::is_conditional_type(self.ctx.types, target)
                || crate::query_boundaries::common::is_conditional_type(
                    self.ctx.types,
                    evaluated_target_for_infer_suppression,
                );

        // Suppress TS2322 for callable types with generic type parameters from outer
        // context. Skip the suppression when both sides have their own signature-level
        // type params — the solver handles generic-to-generic comparison correctly.
        let is_callable_or_function = |type_id: TypeId| {
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
                .is_some()
                || crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
                    .is_some()
                || crate::query_boundaries::common::type_application(self.ctx.types, type_id)
                    .is_some_and(|app| {
                        crate::query_boundaries::common::callable_shape_for_type(
                            self.ctx.types,
                            app.base,
                        )
                        .is_some()
                            || crate::query_boundaries::common::function_shape_for_type(
                                self.ctx.types,
                                app.base,
                            )
                            .is_some()
                    })
        };

        let is_constructor_like = |type_id: TypeId| -> bool {
            if crate::query_boundaries::common::has_construct_signatures(self.ctx.types, type_id) {
                return true;
            }
            if let Some(shape) =
                crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
                && shape.is_constructor
            {
                return true;
            }
            if let Some(app) =
                crate::query_boundaries::common::type_application(self.ctx.types, type_id)
            {
                if crate::query_boundaries::common::has_construct_signatures(
                    self.ctx.types,
                    app.base,
                ) {
                    return true;
                }
                if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    app.base,
                ) && shape.is_constructor
                {
                    return true;
                }
            }
            false
        };

        let has_own_signature_type_params = |type_id: TypeId| -> bool {
            if let Some(shape) =
                crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
            {
                return shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                    .any(|sig| !sig.type_params.is_empty());
            }
            if let Some(shape) =
                crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            {
                return !shape.type_params.is_empty();
            }
            false
        };

        let contains_type_parameters = |type_id: TypeId| {
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, type_id)
        };

        let is_structural_target_that_must_not_be_suppressed = |type_id: TypeId| {
            let has_structural_mismatch_shape = |candidate: TypeId| {
                crate::query_boundaries::assignability::has_deferred_conditional_member(
                    self.ctx.types,
                    candidate,
                ) || crate::query_boundaries::common::is_conditional_type(self.ctx.types, candidate)
                    || crate::query_boundaries::common::is_string_intrinsic_type(
                        self.ctx.types,
                        candidate,
                    )
                    || crate::query_boundaries::common::is_mapped_type(self.ctx.types, candidate)
                    || crate::query_boundaries::common::intersection_members(
                        self.ctx.types,
                        candidate,
                    )
                    .is_some()
            };

            let evaluated = self.ctx.types.evaluate_type(type_id);
            let application_evaluated =
                if crate::query_boundaries::state::type_environment::application_info(
                    self.ctx.types,
                    type_id,
                )
                .is_some()
                {
                    crate::query_boundaries::state::type_environment::evaluate_type_with_resolver(
                        self.ctx.types,
                        &self.ctx,
                        type_id,
                    )
                } else {
                    type_id
                };
            has_structural_mismatch_shape(type_id)
                || (evaluated != type_id && has_structural_mismatch_shape(evaluated))
                || (application_evaluated != type_id
                    && has_structural_mismatch_shape(application_evaluated))
        };

        // Suppress TS2322 for types that contain recursive constraints or error conditions
        // that would lead to false positive diagnostics. These include:
        // - Types with type parameters that might cause recursive constraint issues
        let should_suppress_for_complex_type = |type_id: TypeId| -> bool {
            if crate::query_boundaries::common::is_type_parameter(self.ctx.types, type_id)
                || is_callable_or_function(type_id)
                || is_structural_target_that_must_not_be_suppressed(type_id)
            {
                return false;
            }
            // Also check for union types containing indexed access types.
            // For example, `(S & State<T>)["a"] | undefined` is a union where
            // one member is an indexed access type. We should not suppress TS2322
            // for these cases because the indexed access may resolve to a type
            // that is not assignable from the source.
            //
            // However, if the indexed access types contain error applications
            // (e.g., when type resolution fails), we should still allow suppression
            // to avoid false positives on unresolved types.
            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, type_id)
            {
                if members.iter().any(|&member| {
                    crate::query_boundaries::common::is_type_parameter(self.ctx.types, member)
                }) {
                    return false;
                }

                let has_indexed_access = members.iter().any(|&member| {
                    crate::query_boundaries::common::is_index_access_type(self.ctx.types, member)
                });
                if has_indexed_access {
                    // Check if any indexed access type contains error applications
                    let indexed_access_has_errors = members.iter().any(|&member| {
                        if crate::query_boundaries::common::is_index_access_type(
                            self.ctx.types,
                            member,
                        ) {
                            Self::type_contains_error_application(self.ctx.types, member)
                        } else {
                            false
                        }
                    });
                    // Also check if the union itself contains error applications
                    let union_has_errors =
                        Self::type_contains_error_application(self.ctx.types, type_id);
                    // Only prevent suppression if there are indexed access types AND no errors
                    if !indexed_access_has_errors && !union_has_errors {
                        return false; // Don't suppress for unions containing indexed access types without errors
                    }
                }
            }
            // Keep the generic false-positive suppression for genuinely complex
            // generic shapes, but do not suppress plain `T`/`U` relations.
            // tsc reports TS2322 for distinct type parameters even when they
            // share the same constraint.
            crate::query_boundaries::assignability::has_recursive_type_parameter_constraint(
                self.ctx.types,
                type_id,
            ) || (crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                type_id,
            ) && !is_type_parameter_like(self.ctx.types, type_id))
        };

        // Check if both source and target are simple generic Applications with the same base.
        // In this case, don't suppress - let the variance check or structural comparison
        // handle it. This fixes cases like `Foo<T>` vs `Foo<U>` where T and U are different
        // unconstrained type parameters that should produce TS2322.
        let are_simple_generic_applications = |s: TypeId, t: TypeId| -> bool {
            if let (Some(s_app), Some(t_app)) = (
                crate::query_boundaries::common::type_application(self.ctx.types, s),
                crate::query_boundaries::common::type_application(self.ctx.types, t),
            ) {
                // Same base type, both contain type parameters
                return s_app.base == t_app.base
                    && crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        s,
                    )
                    && crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        t,
                    );
            }
            false
        };

        if are_simple_generic_applications(source, target) {
            return false; // Don't suppress - let the actual assignability check run
        }

        // Don't suppress for generic Applications with type parameters.
        // This fixes false TS2769 errors when passing generic return types
        // (e.g., IterableIterator<T> from values()) to overloads.
        let is_generic_application_with_type_params = |ty: TypeId| -> bool {
            if let Some(app) = crate::query_boundaries::common::type_application(self.ctx.types, ty)
                && app.args.iter().any(|&arg| {
                    crate::query_boundaries::common::contains_type_parameters(self.ctx.types, arg)
                })
            {
                return true;
            }
            false
        };

        // Check if target contains indexed access type - these should NOT be suppressed
        // even when source has type parameters, because indexed access may resolve
        // to incompatible types (e.g., (S & State<T>)["a"] may not accept T)
        let target_contains_indexed_access = || -> bool {
            if crate::query_boundaries::common::is_index_access_type(self.ctx.types, target) {
                return true;
            }
            // Check union members for indexed access types
            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, target)
            {
                return members.iter().any(|&member| {
                    crate::query_boundaries::common::is_index_access_type(self.ctx.types, member)
                });
            }
            false
        };

        // Check if target is an index signature type (e.g., { [s: string]: A })
        // These should prefer TS2741 for missing properties over TS2322 suppression
        let target_is_index_signature = || -> bool {
            if let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target)
            {
                return shape.string_index.is_some() || shape.number_index.is_some();
            }
            false
        };

        if is_generic_application_with_type_params(source)
            || is_generic_application_with_type_params(target)
        {
            return false; // Don't suppress - let the actual assignability check run
        }

        let is_single_type_param_spread_tuple = |ty: TypeId| {
            crate::query_boundaries::common::tuple_elements(self.ctx.types, ty).is_some_and(
                |elements| {
                    elements.len() == 1
                        && elements[0].rest
                        && crate::query_boundaries::common::type_param_info(
                            self.ctx.types,
                            elements[0].type_id,
                        )
                        .is_some()
                },
            )
        };
        if is_single_type_param_spread_tuple(source) || is_single_type_param_spread_tuple(target) {
            return false;
        }

        let evaluated_source = self.ctx.types.evaluate_type(source);
        let evaluated_target = self.ctx.types.evaluate_type(target);
        if let (Some(source_elem), Some(target_elem)) = (
            crate::query_boundaries::common::array_element_type(self.ctx.types, evaluated_source),
            crate::query_boundaries::common::array_element_type(self.ctx.types, evaluated_target),
        ) && crate::query_boundaries::common::is_mapped_type(self.ctx.types, source_elem)
            && is_type_parameter_like(self.ctx.types, target_elem)
        {
            return false;
        }

        // Structural targets (mapped/intersection/conditional/string-intrinsic) require
        // property-level checking; they must not take the complex-generic suppression
        // early-exit below — the solver decides those relations directly.
        let target_is_structural = is_structural_target_that_must_not_be_suppressed(target);
        let target_is_template_literal_from_bare_type_param =
            crate::query_boundaries::common::is_template_literal_type(self.ctx.types, target)
                && crate::query_boundaries::common::is_type_parameter(self.ctx.types, source);
        let target_allows_complex_generic_suppression = !target_is_structural
            && should_suppress_for_complex_type(target)
            && contains_type_parameters(source)
            && !is_callable_or_function(target)
            && !target_contains_indexed_access()
            && !target_is_template_literal_from_bare_type_param;

        matches!(source, TypeId::ERROR)
            || matches!(target, TypeId::ERROR | TypeId::ANY)
            || contains_error_application(target)
            // any is assignable to everything except never — tsc reports TS2322 for any→never
            || (source == TypeId::ANY && target != TypeId::NEVER)
            // Inference placeholders are transient solver state. Emitting TS2322/TS2345
            // while they are still present creates contextual false positives.
            || contains_free_infer_types(self.ctx.types, self.ctx.types.evaluate_type(source))
            || (contains_free_infer_types(self.ctx.types, evaluated_target_for_infer_suppression)
                && !target_is_conditional_for_infer_suppression)
            // Suppress TS2322 for non-callable types with type parameters that may
            // cause false positives due to complex generic constraints
            // (e.g., T extends { [P in T]: number }). Callable/generic signature
            // targets have their own suppression rules below, and suppressing them
            // here hides real TS2322s like templateLiteralTypes7.
            // Also keep mainline behavior that only suppresses while the source is
            // still generic/unresolved too; once the source has reduced to a concrete
            // type, tsc surfaces the mismatch even if the target still mentions an
            // outer type parameter (for example Assign<T, U> receiving a concrete U).
            // EXCEPTION: Don't suppress when target contains indexed access types - these
            // may resolve to incompatible concrete types that should produce TS2322.
            // Don't suppress when target is a template-literal pattern and the
            // source is a bare type parameter. The pattern `${T}` is *not*
            // trivially assignable from a bare T: T's instantiation could be
            // a literal subtype ("a") that does not structurally match the
            // template's pattern. tsc emits TS2322 for these cases (see
            // templateLiteralTypes5.ts:14:11 — `const test1: \`${T3}\` = x`).
            // Restrict the carve-out to bare type-parameter sources so that
            // template-vs-template generic comparisons (e.g.
            // `\`...${Uppercase<T>}.4\`` vs `\`...${Uppercase<T>}.3\``) keep
            // their existing suppression — tsc tolerates those under generic
            // constraint relationships.
            || target_allows_complex_generic_suppression
            // Suppress TS2322 for callable types where the source contains generic type
            // parameters that may not have been fully inferred from context. When both
            // source and target contain type parameters that are COMPLETELY disjoint
            // at the signature level (e.g., () => T vs () => U from an outer `<T, U>`
            // scope), the incompatibility is real and must NOT be suppressed.
            // Skip when both sides have their own signature-level type parameters —
            // the solver handles generic-to-generic comparison correctly via alpha-renaming.
            // Also skip when only the source has type parameters and target is concrete —
            // this is a real mismatch (e.g., <T>(x: T) => T vs (x: string) => boolean).
            // Additionally skip when source has outer-context type params and target is concrete
            // (e.g., JSDoc @template types that should emit errors for concrete mismatches).
            || (!self.ctx.skip_callable_type_param_suppression.get()
                && is_callable_or_function(source)
                && is_callable_or_function(target)
                && contains_type_parameters(source)
                && !self.callable_types_have_disjoint_type_parameters(source, target)
                && !(has_own_signature_type_params(source)
                    && has_own_signature_type_params(target))
                && !(has_own_signature_type_params(source)
                    && !has_own_signature_type_params(target)
                    && !contains_type_parameters(target))
                && !(!has_own_signature_type_params(source)
                    && contains_type_parameters(source)
                    && !contains_type_parameters(target))
                && !is_constructor_like(source)
                && !is_constructor_like(target)
                && !target_is_index_signature())
    }

    /// Targeted suppression for member type compatibility checks (TS2416/TS2430).
    ///
    /// Unlike `should_suppress_assignability_diagnostic`, this does NOT suppress
    /// callable types whose source contains type parameters from an outer context.
    /// For implements/extends member checking, class-level type parameters are fully
    /// declared and their constraints must be checked eagerly — suppressing them
    /// causes false negatives where incompatible member/property signatures are accepted.
    pub(crate) fn should_suppress_member_assignability(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let contains_error_application =
            |type_id: TypeId| Self::type_contains_error_application(self.ctx.types, type_id);

        matches!(source, TypeId::ERROR)
            || matches!(target, TypeId::ERROR | TypeId::ANY)
            || contains_error_application(target)
            || (source == TypeId::ANY && target != TypeId::NEVER)
            || contains_free_infer_types(self.ctx.types, self.ctx.types.evaluate_type(source))
            || contains_free_infer_types(self.ctx.types, self.ctx.types.evaluate_type(target))
    }

    /// Check if two callable types have completely disjoint outer type parameters
    /// at their immediate signature level (parameters and return type only).
    ///
    /// Returns true when both source and target function shapes directly reference
    /// type parameters in their parameter/return positions and those type parameters
    /// are entirely different. This is a conservative check that only looks at the
    /// shallow signature level to avoid false positives from type parameters buried
    /// in generic utility types.
    fn callable_types_have_disjoint_type_parameters(&self, source: TypeId, target: TypeId) -> bool {
        let get_direct_type_params = |type_id: TypeId| -> Vec<TypeId> {
            let mut params = Vec::new();
            let mut current = type_id;
            // Walk through nested function return types to find type parameters
            // at any depth (e.g., () => (item: any) => T has T in the nested return)
            for _ in 0..4 {
                if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    current,
                ) {
                    for p in &shape.params {
                        if crate::query_boundaries::common::is_type_parameter(
                            self.ctx.types,
                            p.type_id,
                        ) {
                            params.push(p.type_id);
                        }
                    }
                    if crate::query_boundaries::common::is_type_parameter(
                        self.ctx.types,
                        shape.return_type,
                    ) {
                        params.push(shape.return_type);
                        break;
                    }
                    // If return type is another function, recurse into it
                    current = shape.return_type;
                } else {
                    break;
                }
            }
            params
        };

        let source_params = get_direct_type_params(source);
        let target_params = get_direct_type_params(target);

        // Both must have direct type params for them to be disjoint
        if source_params.is_empty() || target_params.is_empty() {
            return false;
        }

        // Disjoint = no overlap at all
        !source_params.iter().any(|s| target_params.contains(s))
    }

    /// Check if a type contains an error application (recursively).
    fn type_contains_error_application(
        db: &dyn tsz_solver::construction::TypeDatabase,
        type_id: TypeId,
    ) -> bool {
        // Check if it's a direct error application
        if let Some(app) = crate::query_boundaries::common::type_application(db, type_id)
            && app.base == TypeId::ERROR
        {
            return true;
        }

        // Check if it's a union type containing an error application
        if let Some(members) = crate::query_boundaries::common::union_members(db, type_id) {
            for member in members {
                if Self::type_contains_error_application(db, member) {
                    return true;
                }
            }
        }

        // Check if it's an intersection type containing an error application
        if let Some(members) = crate::query_boundaries::common::intersection_members(db, type_id) {
            for member in members {
                if Self::type_contains_error_application(db, member) {
                    return true;
                }
            }
        }

        // Check if it's a function type with error return
        if let Some(fn_shape) =
            crate::query_boundaries::common::function_shape_for_type(db, type_id)
            && Self::type_contains_error_application(db, fn_shape.return_type)
        {
            return true;
        }

        // Check if it's a callable type with error return
        if let Some(callable) =
            crate::query_boundaries::common::callable_shape_for_type(db, type_id)
        {
            for sig in &callable.call_signatures {
                if Self::type_contains_error_application(db, sig.return_type) {
                    return true;
                }
            }
        }

        false
    }

    fn recursive_conditional_path_alias_mismatch_is_tsc_bailout(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some((source_base, source_args)) = self.application_info_or_display_alias(source)
        else {
            return false;
        };
        let Some((target_base, target_args)) = self.application_info_or_display_alias(target)
        else {
            return false;
        };
        if source_base != target_base
            || source_args.len() != target_args.len()
            || source_args == target_args
            || !self.ctx.types.is_conditional_alias_base(source_base)
        {
            return false;
        }
        source_args
            .iter()
            .zip(target_args.iter())
            .any(|(&source_arg, &target_arg)| {
                source_arg == target_arg
                    && crate::query_boundaries::common::string_literal_value(
                        self.ctx.types,
                        source_arg,
                    )
                    .is_some_and(|atom| self.ctx.types.resolve_atom_ref(atom).contains('.'))
            })
    }

    /// Suppress assignability diagnostics for parser-recovery artifacts.
    pub(crate) fn should_suppress_assignability_for_parse_recovery(
        &self,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        if !self.has_syntax_parse_errors() {
            return false;
        }

        if self.ctx.syntax_parse_error_positions.is_empty() {
            return false;
        }

        self.is_parse_recovery_anchor_node(source_idx)
            || self.is_parse_recovery_anchor_node(diag_idx)
    }

    /// Detect nodes that look like parser-recovery artifacts (empty text, near errors).
    fn is_parse_recovery_anchor_node(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        // Missing-expression placeholders used by parser recovery.
        if self
            .ctx
            .arena
            .get_identifier_text(idx)
            .is_some_and(str::is_empty)
        {
            return true;
        }

        // Also suppress diagnostics anchored very near a syntax parse error.
        const DIAG_PARSE_DISTANCE: u32 = 16;
        for &err_pos in &self.ctx.syntax_parse_error_positions {
            let before = err_pos.saturating_sub(DIAG_PARSE_DISTANCE);
            let after = err_pos.saturating_add(DIAG_PARSE_DISTANCE);
            if (node.pos >= before && node.pos <= after)
                || (node.end >= before && node.end <= after)
            {
                return true;
            }
        }

        let mut current = idx;
        let mut walk_guard = 0;
        while current.is_some() {
            walk_guard += 1;
            if walk_guard > 512 {
                break;
            }

            if let Some(current_node) = self.ctx.arena.get(current) {
                if current_node.this_node_has_error() || current_node.this_or_subtree_has_error() {
                    return true;
                }
            } else {
                break;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        false
    }

    /// Ensure all Lazy/Ref types in a type are resolved into the type environment.
    pub(crate) fn ensure_refs_resolved(&mut self, type_id: TypeId) {
        use crate::state_domain::type_environment::lazy::{
            enter_refs_resolution_scope, exit_refs_resolution_scope,
            global_resolution_fuel_exhausted, increment_global_resolution_fuel,
            increment_refs_resolution_fuel, refs_resolution_fuel_exhausted,
        };

        if self.ctx.refs_resolved.contains(&type_id) {
            return;
        }

        let is_outermost = enter_refs_resolution_scope();

        let mut visited_types = FxHashSet::default();
        let mut visited_def_ids = FxHashSet::default();
        let mut worklist = vec![type_id];

        while let Some(current) = worklist.pop() {
            if refs_resolution_fuel_exhausted() {
                break;
            }

            if !visited_types.insert(current) {
                continue;
            }

            for symbol_ref in collect_type_queries(self.ctx.types, current) {
                let sym_id = tsz_binder::SymbolId(symbol_ref.0);
                let _ = self.get_type_of_symbol(sym_id);
                // Populate type_env with the VALUE type (constructor for classes) so that
                // TypeEvaluator::visit_type_query can resolve via TypeEnvironment::resolve_ref.
                // Without this, resolve_ref returns None and the fallback resolve_lazy returns
                // the INSTANCE type for classes, causing false TS2345 on `typeof ClassName` args.
                if let Some(&value_type) = self.ctx.symbol_types.get(&sym_id)
                    && let Ok(mut env) = self.ctx.type_env.try_borrow_mut()
                {
                    env.insert(tsz_solver::SymbolRef(sym_id.0), value_type);
                }
            }

            for def_id in collect_lazy_def_ids(self.ctx.types, current) {
                if refs_resolution_fuel_exhausted() {
                    break;
                }
                if !visited_def_ids.insert(def_id) {
                    continue;
                }
                increment_refs_resolution_fuel();
                increment_global_resolution_fuel();
                let at_fuel_limit = global_resolution_fuel_exhausted();
                // Always call resolve_and_insert_def_type even when global fuel is
                // exhausted: the call is typically a fast cache hit for lib types that
                // were computed during type-environment building, and the resolver needs
                // the TypeEnvironment entry to evaluate a Lazy(def_id) during
                // assignability checks.  Without this, exhausted-fuel calls silently
                // leave subsequent DOM/lib type refs unresolvable, causing the relation
                // checker to treat unresolved Lazy types as compatible (issue #12144).
                // When at the fuel limit we still resolve the direct def_id but skip
                // adding its result to the worklist so transitive work stays bounded.
                if let Some(result) = self.resolve_and_insert_def_type(def_id)
                    && result != TypeId::ERROR
                    && result != TypeId::ANY
                    && !at_fuel_limit
                {
                    worklist.push(result);
                }
                if at_fuel_limit {
                    break;
                }
            }
        }
        self.ctx.refs_resolved.insert(type_id);

        if is_outermost {
            exit_refs_resolution_scope();
        }
    }

    /// Evaluate a type for assignability checking.
    ///
    /// Determines if the type needs evaluation (applications, env-dependent types)
    /// and performs the appropriate evaluation.
    pub(crate) fn evaluate_type_for_assignability(&mut self, type_id: TypeId) -> TypeId {
        if type_id.is_intrinsic() {
            return type_id;
        }

        thread_local! {
            static ASSIGNABILITY_EVAL_VISITING: std::cell::RefCell<FxHashSet<TypeId>> =
                Default::default();
        }

        let entered =
            ASSIGNABILITY_EVAL_VISITING.with(|visiting| visiting.borrow_mut().insert(type_id));
        if !entered {
            return type_id;
        }

        let result = self.evaluate_type_for_assignability_inner(type_id);
        ASSIGNABILITY_EVAL_VISITING.with(|visiting| visiting.borrow_mut().remove(&type_id));
        result
    }
}
