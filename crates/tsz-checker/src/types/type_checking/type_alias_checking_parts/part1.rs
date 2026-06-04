impl<'a> CheckerState<'a> {
    fn type_node_is_nested_in_type_literal(&self, node_idx: NodeIndex) -> bool {
        let mut current = self
            .ctx
            .arena
            .get_extended(node_idx)
            .map_or(NodeIndex::NONE, |info| info.parent);

        while current.is_some() {
            let Some(parent) = self.ctx.arena.get(current) else {
                break;
            };
            if parent.kind == syntax_kind_ext::TYPE_LITERAL {
                return true;
            }
            current = self
                .ctx
                .arena
                .get_extended(current)
                .map_or(NodeIndex::NONE, |info| info.parent);
        }

        false
    }

    pub(crate) fn type_alias_reaches_resolving_alias(&self, sym_id: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS == 0 {
            return false;
        }

        if self.ctx.symbol_resolution_set.is_empty() {
            return false;
        }

        let Some(start_def_id) = self.ctx.get_existing_def_id(sym_id) else {
            return false;
        };

        with_alias_defid_visited(|visited| {
            let mut pending = vec![start_def_id];
            let mut steps = 0usize;
            while let Some(def_id) = pending.pop() {
                if !visited.insert(def_id) {
                    continue;
                }
                if self
                    .ctx
                    .symbol_resolution_set
                    .iter()
                    .any(|sid| self.ctx.get_existing_def_id(*sid) == Some(def_id))
                {
                    return true;
                }
                let Some(body) = self.ctx.definition_store.get_body(def_id) else {
                    continue;
                };
                steps += 1;
                if steps > 64 {
                    break;
                }
                pending.extend(crate::query_boundaries::common::collect_lazy_def_ids(
                    self.ctx.types,
                    body,
                ));
            }
            false
        })
    }

    /// Read-and-clear the solver's `tuple_too_large` flag, returning `true`
    /// only when the flag was set AND `body_type`'s outer shape owns the
    /// synthesis. Always clears the flag so the next alias starts clean.
    fn alias_body_owns_too_large_tuple(&self, body_type: TypeId) -> bool {
        self.ctx.types.take_tuple_too_large()
            && crate::query_boundaries::type_checking_utilities::is_fresh_tuple_synthesis_site(
                self.ctx.types,
                body_type,
            )
    }

    /// Check a type alias declaration.
    pub(crate) fn check_type_alias_declaration(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };
        let Some(alias) = self.ctx.arena.get_type_alias(node) else {
            return;
        };
        let alias_timing_enabled = tsz_common::perf_counters::enabled_fast();
        let alias_pos = node.pos;
        let alias_end = node.end;

        // TS1277: 'const' modifier not allowed on type alias type parameters
        self.check_const_type_parameter_on_non_function(alias.type_parameters.as_ref());

        // TS1274: Check for modifiers that can never appear on type parameters
        // (public, private, static, etc.)
        self.check_never_valid_type_parameter_modifiers(alias.type_parameters.as_ref());

        // Check type parameter defaults for ordering (TS2706), forward references (TS2744),
        // and circular defaults (TS2716)
        let alias_name_str = self
            .ctx
            .arena
            .get(alias.name)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|id| id.escaped_text.to_string());
        let parameter_timing_start = alias_timing_enabled.then(web_time::Instant::now);
        // Push type parameters to scope FIRST so that constraints like
        // `type Pair<A extends B, B>` can reference sibling type parameters.
        let updates = self.push_missing_name_type_parameters(&alias.type_parameters);

        if let Some(ref name) = alias_name_str {
            self.check_type_parameters_for_missing_names_with_enclosing(
                &alias.type_parameters,
                name,
            );
        } else {
            self.check_type_parameters_for_missing_names(&alias.type_parameters);
        }
        if let Some(type_params) = &alias.type_parameters {
            let factory = self.ctx.types.factory();
            for &param_idx in &type_params.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(param.name) else {
                    continue;
                };
                let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                    continue;
                };

                let constraint = if param.constraint != NodeIndex::NONE {
                    Some(self.get_type_from_type_node(param.constraint))
                } else {
                    None
                };
                let default = if param.default != NodeIndex::NONE {
                    let default_type = self.get_type_from_type_node(param.default);
                    if default_type == TypeId::ERROR {
                        None
                    } else {
                        Some(default_type)
                    }
                } else {
                    None
                };
                let atom = self.ctx.types.intern_string(&ident.escaped_text);
                let constrained_param = factory.type_param(tsz_solver::TypeParamInfo {
                    name: atom,
                    constraint,
                    default,
                    is_const: false,
                });
                self.ctx
                    .type_parameter_scope
                    .insert(ident.escaped_text.clone(), constrained_param);
            }
        }
        record_type_alias_phase_timing(
            &self.ctx.file_name,
            alias_name_str.as_deref(),
            "parameters",
            alias_pos,
            alias_end,
            parameter_timing_start,
        );
        // Temporarily register this alias in `symbol_resolution_set` before visiting
        // the type body. This is used by TS4110 (tuple type circularity) and other
        // circular-reference detection during type node checking.
        let alias_sym_id = self.ctx.binder.get_node_symbol(node_idx);
        let inserted_for_circular_check = alias_sym_id
            .map(|sid| self.ctx.symbol_resolution_set.insert(sid))
            .unwrap_or(false);

        // A generic alias whose unwrapped body is a self-application cycle
        // collapses to a non-generic error type (TS2456 + TS2315). Its body is
        // a "deferred self-reference", so the normal `check_type_node` pass that
        // would emit the body's TS2315 is skipped below; we emit it explicitly
        // and force the registered alias type to ERROR so use sites do not
        // cascade off a stale generic shape.
        let is_generic_self_circular =
            alias_sym_id.is_some_and(|sid| self.type_alias_is_generic_self_circular(sid));

        let should_check_variance_annotations = self
            .check_variance_annotations_supported_for_type_alias(alias)
            && self.type_alias_has_variance_annotation_to_check(alias.type_parameters.as_ref());

        // Check variance annotations match actual usage (TS2636).
        // Resolve the alias body type directly so the solver can compute variance.
        // This must be done while type parameters are still in scope.
        let has_deferred_self_reference = alias_sym_id.is_some_and(|alias_sid| {
            self.alias_ast_is_deferred(alias_sid)
                && self.ctx.symbol_resolution_set.contains(&alias_sid)
                && self.alias_ast_refs_symbol_or_resolution_chain_alias(alias.type_node, alias_sid)
        });
        let body_timing_start = alias_timing_enabled.then(web_time::Instant::now);
        let body_type = {
            let _ = self.ctx.types.take_union_too_complex();
            // Clear any stale tuple_too_large flag before constructing the body
            // so that flag reads below are attributable to this alias alone.
            let _ = self.ctx.types.take_tuple_too_large();
            let body_type = if has_deferred_self_reference {
                crate::TypeNodeChecker::new(&mut self.ctx).check(alias.type_node)
            } else {
                self.get_type_from_type_node(alias.type_node)
            };
            if should_check_variance_annotations {
                self.check_variance_annotations_with_body(
                    node_idx,
                    &alias.type_parameters,
                    Some(body_type),
                );
            }
            self.check_styled_component_inner_component_constraint(alias.type_node);
            if is_generic_self_circular {
                TypeId::ERROR
            } else {
                body_type
            }
        };
        record_type_alias_phase_timing(
            &self.ctx.file_name,
            alias_name_str.as_deref(),
            "body",
            alias_pos,
            alias_end,
            body_timing_start,
        );
        let body_construction_too_complex = self.ctx.types.take_union_too_complex();
        let mut body_produced_too_large_tuple = self.alias_body_owns_too_large_tuple(body_type);
        let has_type_params = alias
            .type_parameters
            .as_ref()
            .is_some_and(|params| !params.nodes.is_empty());
        // Generic aliases are checked at declaration time, but their bodies are
        // not fully instantiated until concrete type arguments are supplied.
        let body_evaluation_too_complex = if has_deferred_self_reference || has_type_params {
            false
        } else {
            let evaluation_timing_start = alias_timing_enabled.then(web_time::Instant::now);
            let _ = self.evaluate_type_with_env_uncached(body_type);
            record_type_alias_phase_timing(
                &self.ctx.file_name,
                alias_name_str.as_deref(),
                "evaluation",
                alias_pos,
                alias_end,
                evaluation_timing_start,
            );
            body_produced_too_large_tuple =
                body_produced_too_large_tuple || self.alias_body_owns_too_large_tuple(body_type);
            self.ctx.types.take_union_too_complex()
        };
        let registration_timing_start = alias_timing_enabled.then(web_time::Instant::now);
        if body_type != TypeId::ERROR
            && let Some(alias_sid) = alias_sym_id
        {
            let type_params = self.current_alias_type_params(alias.type_parameters.as_ref());
            let can_register_non_generic_conditional = type_params.is_empty()
                && crate::query_boundaries::common::is_conditional_type(self.ctx.types, body_type)
                && !crate::query_boundaries::checkers::generic::contains_named_or_bound_type_parameter(
                    self.ctx.types,
                    body_type,
                )
                && !tsz_solver::type_queries::is_distributive_conditional_with_deferred_check(
                    self.ctx.types,
                    body_type,
                );
            if !type_params.is_empty() || can_register_non_generic_conditional {
                let alias_def_id = self.ctx.get_or_create_def_id(alias_sid);
                let registered_type = if can_register_non_generic_conditional {
                    self.evaluate_type_with_env_uncached(body_type)
                } else {
                    body_type
                };
                self.ctx.symbol_types.insert(alias_sid, registered_type);
                self.ctx
                    .register_resolved_type(alias_sid, registered_type, type_params);
                self.ctx.clear_type_evaluation_caches_for_def(alias_def_id);
            }
        }
        record_type_alias_phase_timing(
            &self.ctx.file_name,
            alias_name_str.as_deref(),
            "registration",
            alias_pos,
            alias_end,
            registration_timing_start,
        );
        if body_produced_too_large_tuple || self.type_node_produces_too_large_tuple(alias.type_node)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                alias.type_node,
                diagnostic_messages::TYPE_PRODUCES_A_TUPLE_TYPE_THAT_IS_TOO_LARGE_TO_REPRESENT,
                diagnostic_codes::TYPE_PRODUCES_A_TUPLE_TYPE_THAT_IS_TOO_LARGE_TO_REPRESENT,
            );
        }
        if body_construction_too_complex || body_evaluation_too_complex {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            let anchor = if body_evaluation_too_complex {
                self.too_complex_union_member_anchor(alias.type_node)
                    .unwrap_or(alias.type_node)
            } else {
                alias.type_node
            };
            self.error_at_node(
                anchor,
                diagnostic_messages::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT,
                diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT,
            );
        }
        if let Some(alias_sid) = alias_sym_id
            && let Some(body_node) = self.ctx.arena.get(alias.type_node)
            && let Some(conditional) = self.ctx.arena.get_conditional_type(body_node)
            && self.type_node_references_defaulted_alias_with_omitted_args(
                conditional.check_type,
                alias_sid,
            )
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                conditional.check_type,
                diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
            );
        }

        // TS2589: detect excessively deep type instantiation at definition time.
        // tsc emits TS2589 for type aliases whose body contains conditional types
        // that self-reference and create infinite expansion (e.g.,
        // `type Foo<T> = T extends unknown ? Foo<T> : unknown`).
        // We check this by:
        // 1. Verifying the body references the alias's own DefId
        // 2. Registering the body temporarily so the evaluator can resolve it
        // 3. Evaluating with a special flag that detects Application cycle = TS2589
        let recursion_timing_start = alias_timing_enabled.then(web_time::Instant::now);
        if let Some(alias_sid) = alias_sym_id {
            let def_id = self.ctx.get_or_create_def_id(alias_sid);
            // Only check when the body is a conditional type — tsc emits TS2589
            // at definition time specifically for recursive conditional types,
            // not indexed access or other patterns.
            let body_is_conditional = crate::query_boundaries::common::is_conditional_type(
                self.ctx.types.as_type_database(),
                body_type,
            );
            let body_refs = if body_is_conditional {
                crate::query_boundaries::common::collect_lazy_def_ids(self.ctx.types, body_type)
            } else {
                Vec::new()
            };
            let has_stable_recursive_ref = body_is_conditional
                && self
                    .conditional_body_has_definite_recursive_alias_ref(alias.type_node, alias_sid);
            let has_unresolved_computed_recursive_ref = body_is_conditional
                && self.conditional_body_has_unresolved_computed_recursive_alias_ref(
                    alias.type_node,
                    alias_sid,
                );
            let has_recursive_wrapper_arg = !body_is_conditional
                && self.type_reference_applies_alias_to_recursive_wrapper_arg(alias.type_node);
            if (has_stable_recursive_ref && body_refs.contains(&def_id))
                || has_unresolved_computed_recursive_ref
                || has_recursive_wrapper_arg
            {
                // Reuse scoped alias params so TS2589 sees constraints/defaults.
                let type_params = self.current_alias_type_params(alias.type_parameters.as_ref());

                // Register body temporarily for evaluation
                self.ctx
                    .register_def_auto_params_in_envs(def_id, body_type, type_params);

                // Evaluate with TS2589 detection flag
                let depth_exceeded = (has_stable_recursive_ref || has_recursive_wrapper_arg)
                    && self.evaluate_type_for_ts2589_check(body_type, def_id);
                // A bare-`infer` self-recursive alias is unconditionally infinite.
                // tsc collapses it to the error type so use sites do not cascade
                // into spurious TS2322. Flag the def as depth-poisoned: the
                // evaluator then resolves every `Alias<...>` application to the
                // error type (assignable both ways). Scoped to this shape so the
                // direct-recursion path, which anchors TS2589 at the use site,
                // keeps its current behavior. Clearing the evaluation caches drops
                // any unexpanded application that was cached before the flag.
                if depth_exceeded
                    && self
                        .conditional_body_has_bare_infer_recursive_ref(alias.type_node, alias_sid)
                {
                    self.ctx.definition_store.mark_depth_poisoned(def_id);
                    self.ctx.clear_type_evaluation_caches_for_def(def_id);
                }
                if depth_exceeded || has_unresolved_computed_recursive_ref {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    // tsc anchors TS2589 at `currentNode` (the inner self-reference
                    // being instantiated when `instantiationDepth === 100` fires).
                    // Conditional-type children are visited in
                    // check→extends→true→false order, so the last self-referential
                    // type reference in source order matches tsc's anchor.
                    let anchor = if has_recursive_wrapper_arg {
                        alias.type_node
                    } else {
                        self.find_last_recursive_alias_ref(alias.type_node, alias_sid)
                            .unwrap_or(alias.type_node)
                    };
                    self.error_at_node(
                        anchor,
                        diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                        diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                    );
                }
            }
        }
        record_type_alias_phase_timing(
            &self.ctx.file_name,
            alias_name_str.as_deref(),
            "recursive_depth",
            alias_pos,
            alias_end,
            recursion_timing_start,
        );

        // TS4109: detect circular type arguments when the alias body is directly
        // a TypeReference (e.g. `type X = Foo<X extends {} ? A : B>`).  In TSC
        // this fires only during `resolveTypeArguments` for the direct body type
        // reference, NOT for nested type references inside unions, mapped types,
        // etc.  We emulate this by checking only when the alias body node itself
        // is a TypeReference whose type arguments reference the resolving alias.
        if let Some(body_node) = self.ctx.arena.get(alias.type_node)
            && body_node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(body_node)
            && let Some(ref type_args) = type_ref.type_arguments
            && let Some(alias_sid) = alias_sym_id
            && self.ctx.symbol_resolution_set.contains(&alias_sid)
        {
            let has_circular_arg = type_args
                .nodes
                .iter()
                .copied()
                .any(|arg_idx| self.type_arg_directly_references_alias(arg_idx, alias_sid));
            if has_circular_arg {
                let name = self
                    .ctx
                    .binder
                    .get_symbol(alias_sid)
                    .map_or_else(|| "<unknown>".to_string(), |s| s.escaped_name.clone());
                // Resolve the target type reference to get the name of the
                // referenced type (e.g. `NumArray`, `Mx`).
                let target_name = self
                    .ctx
                    .arena
                    .get_type_ref(body_node)
                    .and_then(|tr| {
                        self.resolve_type_symbol_for_lowering(tr.type_name)
                            .and_then(|raw| self.ctx.binder.get_symbol(tsz_binder::SymbolId(raw)))
                            .map(|s| s.escaped_name.clone())
                    })
                    .unwrap_or_else(|| name.clone());
                self.error_at_node_msg(
                    alias.type_node,
                    crate::diagnostics::diagnostic_codes::TYPE_ARGUMENTS_FOR_CIRCULARLY_REFERENCE_THEMSELVES,
                    &[&target_name],
                );
            }
        }

        if is_generic_self_circular {
            self.validate_collapsed_alias_body_reference(alias.type_node);
        }
        let validation_timing_start = alias_timing_enabled.then(web_time::Instant::now);
        if has_deferred_self_reference {
            if let Some(owner_name) = alias_name_str.as_deref() {
                self.check_type_literal_self_indexed_property_annotations(
                    alias.type_node,
                    owner_name,
                );
            }
            if self
                .ctx
                .arena
                .get(alias.type_node)
                .is_some_and(|node| node.kind == syntax_kind_ext::TYPE_LITERAL)
                && self.type_literal_has_circular_accessor_reference(alias.type_node)
            {
                let _ = self.get_type_from_type_literal(alias.type_node);
            }
        } else if !self.validate_signature_only_type_literal_alias_body(alias.type_node) {
            self.check_type_node(alias.type_node);
            if !self.type_alias_body_missing_names_covered_by_type_node_checking(alias.type_node) {
                self.check_type_alias_body_for_missing_names_after_type_node_check(alias.type_node);
            }
        }
        record_type_alias_phase_timing(
            &self.ctx.file_name,
            alias_name_str.as_deref(),
            "body_validation",
            alias_pos,
            alias_end,
            validation_timing_start,
        );

        if inserted_for_circular_check && let Some(sid) = alias_sym_id {
            self.ctx.symbol_resolution_set.remove(&sid);
        }
        // Pre-compute flow-narrowed types for `typeof expr` in the type alias body.
        // This allows `typeof c` inside a type alias to pick up narrowing from
        // control flow (e.g., inside an `if (typeof c === 'string')` block).
        // The results are stored in `node_types` and consumed by `TypeLowering`
        // via the `type_query_override` callback during `ensure_type_alias_resolved`.
        let flow_timing_start = alias_timing_enabled.then(web_time::Instant::now);
        self.precompute_type_query_flow_types(alias.type_node);
        record_type_alias_phase_timing(
            &self.ctx.file_name,
            alias_name_str.as_deref(),
            "type_query_flow",
            alias_pos,
            alias_end,
            flow_timing_start,
        );
        self.pop_type_parameters(updates);
    }

    /// Emit the body-position TS2315 for a generic alias that collapsed to a
    /// non-generic error type. The body's direct (parenthesis-unwrapped) type
    /// reference applies type arguments to the now-non-generic alias, so the
    /// argument-bearing form is "Type 'X' is not generic". A bare body
    /// reference (no type arguments) produces no diagnostic, matching tsc. The
    /// validation cache prevents a duplicate when a use site already triggered
    /// this node's validation.
    fn validate_collapsed_alias_body_reference(&mut self, body_node: NodeIndex) {
        let Some(ref_idx) = self.unwrap_parenthesized_type(body_node) else {
            return;
        };
        let Some(node) = self.ctx.arena.get(ref_idx) else {
            return;
        };
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return;
        };
        let Some(args) = type_ref
            .type_arguments
            .clone()
            .filter(|a| !a.nodes.is_empty())
        else {
            return;
        };
        let Some(raw) = self.resolve_type_symbol_for_lowering(type_ref.type_name) else {
            return;
        };
        self.validate_type_reference_type_arguments(tsz_binder::SymbolId(raw), &args, ref_idx);
    }

    fn too_complex_union_member_anchor(&mut self, type_node: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(type_node)?;
        if node.kind != syntax_kind_ext::UNION_TYPE {
            return None;
        }
        let members: Vec<NodeIndex> = self
            .ctx
            .arena
            .get_composite_type(node)?
            .types
            .nodes
            .to_vec();

        for member in members {
            let _ = self.ctx.types.take_union_too_complex();
            let member_type = self.get_type_from_type_node(member);
            let construction_too_complex = self.ctx.types.take_union_too_complex();
            let _ = self.evaluate_type_with_env_uncached(member_type);
            if construction_too_complex || self.ctx.types.take_union_too_complex() {
                return Some(member);
            }
        }

        None
    }

    fn check_variance_annotations_supported_for_type_alias(
        &mut self,
        alias: &tsz_parser::parser::node::TypeAliasData,
    ) -> bool {
        let Some(type_params) = &alias.type_parameters else {
            return true;
        };

        let variance_supported = self.type_alias_body_supports_variance_annotations(alias);
        if variance_supported {
            return true;
        }

        let mut emitted_unsupported_variance_diagnostic = false;
        for param_idx in type_params.nodes.iter().copied() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            if self.node_contains_any_parse_error(param.name)
                || self.type_parameter_name_is_variance_keyword(param.name)
            {
                continue;
            }
            let Some(modifiers) = param.modifiers.as_ref() else {
                continue;
            };
            let Some(variance_modifier_idx) =
                modifiers.nodes.iter().copied().find(|&modifier_idx| {
                    self.ctx
                        .arena
                        .get(modifier_idx)
                        .is_some_and(|modifier_node| {
                            matches!(
                                modifier_node.kind,
                                k if k == SyntaxKind::InKeyword as u16
                                    || k == SyntaxKind::OutKeyword as u16
                            )
                        })
                })
            else {
                continue;
            };

            self.error_at_node(
                variance_modifier_idx,
                crate::diagnostics::diagnostic_messages::VARIANCE_ANNOTATIONS_ARE_ONLY_SUPPORTED_IN_TYPE_ALIASES_FOR_OBJECT_FUNCTION_CONS,
                crate::diagnostics::diagnostic_codes::VARIANCE_ANNOTATIONS_ARE_ONLY_SUPPORTED_IN_TYPE_ALIASES_FOR_OBJECT_FUNCTION_CONS,
            );
            emitted_unsupported_variance_diagnostic = true;
        }

        !emitted_unsupported_variance_diagnostic
    }

    fn type_alias_has_variance_annotation_to_check(
        &self,
        type_parameters: Option<&tsz_parser::parser::base::NodeList>,
    ) -> bool {
        let Some(type_params) = type_parameters else {
            return false;
        };

        type_params.nodes.iter().copied().any(|param_idx| {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                return false;
            };
            let Some(modifiers) = &param.modifiers else {
                return false;
            };

            let mut declared_in = false;
            let mut declared_out = false;
            for modifier_idx in modifiers.nodes.iter().copied() {
                let Some(modifier_node) = self.ctx.arena.get(modifier_idx) else {
                    continue;
                };
                declared_in |= modifier_node.kind == SyntaxKind::InKeyword as u16;
                declared_out |= modifier_node.kind == SyntaxKind::OutKeyword as u16;
            }

            declared_in != declared_out
        })
    }

    fn type_alias_body_supports_variance_annotations(
        &self,
        alias: &tsz_parser::parser::node::TypeAliasData,
    ) -> bool {
        self.ctx.arena.kind_at(alias.type_node).is_some_and(|kind| {
            kind == syntax_kind_ext::TYPE_LITERAL
                || kind == syntax_kind_ext::FUNCTION_TYPE
                || kind == syntax_kind_ext::CONSTRUCTOR_TYPE
                || kind == syntax_kind_ext::MAPPED_TYPE
        })
    }

    fn type_parameter_name_is_variance_keyword(&self, name_idx: NodeIndex) -> bool {
        if matches!(
            self.get_identifier_text_from_idx(name_idx).as_deref(),
            Some("in" | "out")
        ) {
            return true;
        }
        self.ctx.arena.get(name_idx).is_some_and(|node| {
            node.kind == SyntaxKind::InKeyword as u16 || node.kind == SyntaxKind::OutKeyword as u16
        })
    }

    /// Walk the alias body AST and return the AST node of the last
    /// `TypeReference` (in source order) whose name resolves to `alias_sid`.
    ///
    /// Used as the anchor for TS2589 at type-alias definition sites: tsc emits
    /// at `currentNode`, which is the inner self-reference being instantiated
    /// at the time the depth limit fires. `forEachChild` visits conditional
    /// children in check→extends→true→false order, so the last self-reference
    /// in source order is the one tsc reports against.
    fn find_last_recursive_alias_ref(
        &self,
        body_idx: NodeIndex,
        alias_sid: tsz_binder::SymbolId,
    ) -> Option<NodeIndex> {
        let mut best: Option<(u32, NodeIndex)> = None;
        self.collect_recursive_alias_refs(body_idx, alias_sid, &mut best);
        best.map(|(_, idx)| idx)
    }

    fn type_reference_applies_alias_to_recursive_wrapper_arg(&self, body_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(body_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        if !self.type_reference_names_conditional_type_alias(type_ref.type_name) {
            return false;
        }
        let Some(args) = &type_ref.type_arguments else {
            return false;
        };
        args.nodes
            .iter()
            .any(|&arg_idx| self.type_reference_is_recursive_wrapper_alias(arg_idx))
    }

    fn type_reference_names_conditional_type_alias(&self, type_name: NodeIndex) -> bool {
        let Some(sym_ref) = self.resolve_type_symbol_for_lowering(type_name) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(tsz_binder::SymbolId(sym_ref)) else {
            return false;
        };
        symbol.declarations.iter().any(|&decl_idx| {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            if decl_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                return false;
            }
            let Some(alias) = self.ctx.arena.get_type_alias(decl_node) else {
                return false;
            };
            self.ctx
                .arena
                .get(alias.type_node)
                .is_some_and(|body_node| body_node.kind == syntax_kind_ext::CONDITIONAL_TYPE)
        })
    }

    fn type_reference_is_recursive_wrapper_alias(&self, type_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        let Some(sym_ref) = self.resolve_type_symbol_for_lowering(type_ref.type_name) else {
            return false;
        };
        let alias_sid = tsz_binder::SymbolId(sym_ref);
        let Some(symbol) = self.ctx.binder.get_symbol(alias_sid) else {
            return false;
        };
        symbol.declarations.iter().any(|&decl_idx| {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            if decl_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                return false;
            }
            let Some(alias) = self.ctx.arena.get_type_alias(decl_node) else {
                return false;
            };
            let Some(body_node) = self.ctx.arena.get(alias.type_node) else {
                return false;
            };
            if body_node.kind != syntax_kind_ext::TYPE_REFERENCE {
                return false;
            }
            let Some(body_ref) = self.ctx.arena.get_type_ref(body_node) else {
                return false;
            };
            let Some(body_args) = &body_ref.type_arguments else {
                return false;
            };
            body_args.nodes.iter().any(|&arg_idx| {
                let mut best = None;
                self.collect_recursive_alias_refs(arg_idx, alias_sid, &mut best);
                best.is_some()
            })
        })
    }

    fn collect_recursive_alias_refs(
        &self,
        node_idx: NodeIndex,
        alias_sid: tsz_binder::SymbolId,
        best: &mut Option<(u32, NodeIndex)>,
    ) {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(tr) = self.ctx.arena.get_type_ref(node)
        {
            let resolved = self
                .resolve_type_symbol_for_lowering(tr.type_name)
                .map(tsz_binder::SymbolId);
            if resolved == Some(alias_sid) {
                let pos = node.pos;
                if best.is_none_or(|(p, _)| pos >= p) {
                    *best = Some((pos, node_idx));
                }
            }
        }

        for child_idx in self.ctx.arena.get_children(node_idx) {
            self.collect_recursive_alias_refs(child_idx, alias_sid, best);
        }
    }

    fn conditional_body_has_definite_recursive_alias_ref(
        &mut self,
        node_idx: NodeIndex,
        alias_sid: tsz_binder::SymbolId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
        {
            let resolved = self
                .resolve_type_symbol_for_lowering(type_ref.type_name)
                .map(tsz_binder::SymbolId);
            if resolved == Some(alias_sid) {
                let Some(type_args) = &type_ref.type_arguments else {
                    return true;
                };
                if self.type_args_match_alias_params(alias_sid, type_args) {
                    return true;
                }
                if self.type_arg_nodes_all_are_deferred_passthrough_for_depth_check(type_args) {
                    return false;
                }
                if self
                    .type_args_contain_subtractive_alias_guard_for_depth_check(alias_sid, type_args)
                {
                    return false;
                }
                if self
                    .type_args_reset_defaulted_alias_params_with_scoped_transform_for_depth_check(
                        alias_sid, type_args,
                    )
                {
                    return true;
                }
                if type_args.nodes.iter().copied().all(|arg_idx| {
                    self.type_node_is_deferred_passthrough_for_depth_check(arg_idx)
                        || self.type_node_is_bounded_indexed_descent_for_depth_check(
                            alias_sid, arg_idx,
                        )
                }) {
                    return false;
                }
                return !self
                    .type_arg_nodes_contain_scoped_type_parameter_for_depth_check(type_args);
            }
        }

        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| {
                self.conditional_body_has_definite_recursive_alias_ref(child_idx, alias_sid)
            })
    }

    fn conditional_body_has_computed_recursive_alias_ref(
        &mut self,
        node_idx: NodeIndex,
        alias_sid: tsz_binder::SymbolId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
        {
            let resolved = self
                .resolve_type_symbol_for_lowering(type_ref.type_name)
                .map(tsz_binder::SymbolId);
            return resolved == Some(alias_sid)
                && type_ref.type_arguments.as_ref().is_some_and(|type_args| {
                    !self.type_args_match_alias_params(alias_sid, type_args)
                        && !self
                            .type_arg_nodes_all_are_deferred_passthrough_for_depth_check(type_args)
                        && !self.type_args_contain_subtractive_alias_guard_for_depth_check(
                            alias_sid, type_args,
                        )
                        && type_args.nodes.iter().copied().any(|arg_idx| {
                            !self.type_node_is_deferred_passthrough_for_depth_check(arg_idx)
                                && !self.type_node_is_bounded_indexed_descent_for_depth_check(
                                    alias_sid, arg_idx,
                                )
                        })
                });
        }

        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| {
                self.conditional_body_has_computed_recursive_alias_ref(child_idx, alias_sid)
            })
    }

    pub(crate) fn type_alias_has_computed_recursive_conditional_body(
        &mut self,
        alias_sid: tsz_binder::SymbolId,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(alias_sid) else {
            return false;
        };
        let declarations = symbol.declarations.clone();
        declarations.into_iter().any(|decl_idx| {
            self.ctx.arena.get(decl_idx).is_some_and(|decl_node| {
                decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    && self
                        .ctx
                        .arena
                        .get_type_alias(decl_node)
                        .is_some_and(|alias| {
                            self.ctx
                                .arena
                                .kind_at(alias.type_node)
                                .is_some_and(|kind| kind == syntax_kind_ext::CONDITIONAL_TYPE)
                                && self.conditional_body_has_computed_recursive_alias_ref(
                                    alias.type_node,
                                    alias_sid,
                                )
                        })
            })
        })
    }

    /// True when the alias body contains a conditional whose `extends` clause is
    /// a bare `infer X` and whose true branch re-applies the alias to itself
    /// (e.g. `type A<T> = T extends infer X ? A<X & B> : never`).
    ///
    /// A bare `infer X` always matches, so the true branch is taken
    /// unconditionally; if that branch re-applies the alias the instantiation is
    /// infinite. tsc reports TS2589 and collapses the alias to the error type.
    /// We use this to scope the error-type collapse to exactly this shape, so the
    /// existing direct-recursion path (which anchors TS2589 at the use site) is
    /// unaffected.
    fn conditional_body_has_bare_infer_recursive_ref(
        &mut self,
        node_idx: NodeIndex,
        alias_sid: tsz_binder::SymbolId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::CONDITIONAL_TYPE
            && let Some(cond) = self.ctx.arena.get_conditional_type(node)
            && self
                .ctx
                .arena
                .kind_at(cond.extends_type)
                .is_some_and(|kind| kind == syntax_kind_ext::INFER_TYPE)
            && self.conditional_body_has_definite_recursive_alias_ref(cond.true_type, alias_sid)
        {
            return true;
        }

        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| {
                self.conditional_body_has_bare_infer_recursive_ref(child_idx, alias_sid)
            })
    }

    fn conditional_body_has_unresolved_computed_recursive_alias_ref(
        &mut self,
        node_idx: NodeIndex,
        alias_sid: tsz_binder::SymbolId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
        {
            let resolved = self
                .resolve_type_symbol_for_lowering(type_ref.type_name)
                .map(tsz_binder::SymbolId);
            if resolved == Some(alias_sid)
                && let Some(type_args) = &type_ref.type_arguments
                && !self.type_args_match_alias_params(alias_sid, type_args)
                && !self
                    .type_args_contain_subtractive_alias_guard_for_depth_check(alias_sid, type_args)
                && type_args.nodes.iter().copied().any(|arg_idx| {
                    !self.type_node_is_deferred_passthrough_for_depth_check(arg_idx)
                        && self
                            .type_node_contains_unresolved_type_reference_for_depth_check(arg_idx)
                })
            {
                return true;
            }
        }

        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| {
                self.conditional_body_has_unresolved_computed_recursive_alias_ref(
                    child_idx, alias_sid,
                )
            })
    }

    /// Walk `extends_type` collecting every `infer X` binding and push each as a
    /// provisional type parameter into `type_parameter_scope`. Returns save-state
    /// for `pop_infer_bindings`.
    pub(crate) fn push_infer_bindings_from_extends(
        &mut self,
        extends_type: NodeIndex,
    ) -> Vec<(String, Option<TypeId>)> {
        if extends_type.is_none() {
            return Vec::new();
        }
        // Phase 1: collect the names (immutable AST walk).
        let mut infer_names: Vec<String> = Vec::new();
        let mut stack = vec![extends_type];
        while let Some(idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::TEMPLATE_LITERAL_TYPE_SPAN {
                // `get_children` does not currently expose the expression of a
                // template literal type span, but `${infer X}` stores the
                // binding there.
                if let Some(span) = self.ctx.arena.get_template_span(node) {
                    stack.push(span.expression);
                }
                continue;
            } else if node.kind == syntax_kind_ext::INFER_TYPE {
                if let Some(infer_data) = self.ctx.arena.get_infer_type(node) {
                    if let Some(tp_node) = self.ctx.arena.get(infer_data.type_parameter)
                        && let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node)
                        && let Some(name_node) = self.ctx.arena.get(tp_data.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        let name = ident.escaped_text.clone();
                        if !infer_names.contains(&name) {
                            infer_names.push(name);
                        }
                    }
                    // The constraint of `infer X extends Constraint` may itself
                    // contain `infer Y extends C2`; tsc binds those nested names
                    // in the true branch too. Descend into the type-parameter
                    // subtree to pick them up.
                    stack.push(infer_data.type_parameter);
                }
                continue;
            } else if node.kind == syntax_kind_ext::TYPE_PARAMETER {
                if let Some(type_param) = self.ctx.arena.get_type_parameter(node) {
                    if type_param.constraint != NodeIndex::NONE {
                        stack.push(type_param.constraint);
                    }
                    if type_param.default != NodeIndex::NONE {
                        stack.push(type_param.default);
                    }
                }
                continue;
            }
            for child in self.ctx.arena.get_children(idx) {
                stack.push(child);
            }
        }

        // Phase 2: compute each name's implicit constraint from the surrounding
        // pattern (template literal → string, explicit extends → that type, etc.).
        // Must run before borrowing the factory, since it takes &mut self.
        let infer_constraints: Vec<Option<TypeId>> = infer_names
            .iter()
            .map(|name| self.effective_infer_constraint_from_extends_type(extends_type, name))
            .collect();

        // Phase 3: intern provisional `TypeParameter`s and install them in scope.
        let factory = self.ctx.types.factory();
        let mut pushes: Vec<(String, Option<TypeId>)> = Vec::new();
        for (name, &constraint) in infer_names.iter().zip(infer_constraints.iter()) {
            let atom = self.ctx.types.intern_string(name);
            let provisional = factory.type_param(tsz_solver::TypeParamInfo {
                name: atom,
                constraint,
                default: None,
                is_const: false,
            });
            let previous = self
                .ctx
                .type_parameter_scope
                .insert(name.clone(), provisional);
            pushes.push((name.clone(), previous));
        }
        pushes
    }

    fn pop_infer_bindings(&mut self, pushes: Vec<(String, Option<TypeId>)>) {
        for (name, previous) in pushes.into_iter().rev() {
            if let Some(prev) = previous {
                self.ctx.type_parameter_scope.insert(name, prev);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }
    }

    /// Walk a type argument AST node and return true if it contains a reference
    /// to the alias `alias_sid` inside a "computation" context that would cause
    /// a true cycle during type argument resolution.
    ///
    /// TSC's TS4109 fires only when resolving a type argument requires
    /// evaluating the alias (e.g. `X extends {} ? A : B` or `X['prop']`).
    /// A bare reference to the alias (`type T = I<T>`) does NOT trigger TS4109
    /// because TSC resolves it as a simple type lookup (caught by TS2456).
    ///
    /// `inside_computation` tracks whether we are inside a node that requires
    /// type evaluation (conditional type, indexed access, etc.).
    fn type_arg_directly_references_alias(
        &self,
        node_idx: NodeIndex,
        alias_sid: tsz_binder::SymbolId,
    ) -> bool {
        self.type_arg_references_alias_inner(node_idx, alias_sid, false)
    }

    fn type_arg_references_alias_inner(
        &self,
        node_idx: NodeIndex,
        alias_sid: tsz_binder::SymbolId,
        inside_computation: bool,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        // Check identifiers and type references for a direct alias hit.
        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            || node.kind == syntax_kind_ext::TYPE_REFERENCE
        {
            let sym_id = if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                self.ctx.arena.get_type_ref(node).and_then(|tr| {
                    self.resolve_type_symbol_for_lowering(tr.type_name)
                        .map(tsz_binder::SymbolId)
                })
            } else {
                self.resolve_type_symbol_for_lowering(node_idx)
                    .map(tsz_binder::SymbolId)
            };

            if sym_id == Some(alias_sid) {
                // A TypeReference to the alias WITH type arguments creates a
                // new instantiation (e.g. `Recursive<T>`) -- not circular.
                if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                    let has_args = self
                        .ctx
                        .arena
                        .get_type_ref(node)
                        .is_some_and(|tr| tr.type_arguments.is_some());
                    if has_args {
                        return false;
                    }
                }
                // Only flag as circular if we are inside a computation context
                // (conditional, indexed access, etc.).  A bare reference at the
                // top level is handled by TS2456 instead.
                return inside_computation;
            }

            // A TypeReference to a different type creates a new instantiation
            // boundary -- do not recurse into its children.
            if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                return false;
            }
        }

        // Type constructions that create instantiation boundaries break
        // circularity -- do not recurse into them.
        match node.kind {
            syntax_kind_ext::ARRAY_TYPE
            | syntax_kind_ext::TUPLE_TYPE
            | syntax_kind_ext::FUNCTION_TYPE
            | syntax_kind_ext::CONSTRUCTOR_TYPE
            | syntax_kind_ext::TYPE_LITERAL
            | syntax_kind_ext::MAPPED_TYPE
            | syntax_kind_ext::TYPE_QUERY => {
                return false;
            }
            _ => {}
        }

        // Conditional types and indexed access types are "computation"
        // contexts: resolving them requires evaluating the alias.
        let enters_computation = matches!(
            node.kind,
            k if k == syntax_kind_ext::CONDITIONAL_TYPE
                || k == syntax_kind_ext::INDEXED_ACCESS_TYPE
        );
        let child_inside = inside_computation || enters_computation;

        for child_idx in self.ctx.arena.get_children(node_idx) {
            if self.type_arg_references_alias_inner(child_idx, alias_sid, child_inside) {
                return true;
            }
        }

        false
    }

    /// Check an index signature parameter type for TS1337 (literal/generic) vs TS1268.
    /// Called from `check_type_node` for index signatures inside type literals.
    fn check_index_sig_param_type_in_type_literal(
        &mut self,
        parameters: &tsz_parser::parser::base::NodeList,
    ) {
        let param_idx = parameters.nodes.first().copied().unwrap_or(NodeIndex::NONE);
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return;
        };
        let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
            return;
        };
        if param_data.dot_dot_dot_token || param_data.question_token {
            return; // suppress when parameter already has grammar errors
        }
        if param_data.type_annotation.is_none() {
            return;
        }
        let Some(type_node) = self.ctx.arena.get(param_data.type_annotation) else {
            return;
        };

        // Skip check if the type resolves to a valid index signature type
        // (e.g., type alias to string/number/symbol)
        if self.is_valid_index_sig_param_type(type_node.kind, param_data.type_annotation) {
            return;
        }

        // Check AST to detect type parameters and literal types (TS1337).
        let is_generic_or_literal =
            self.is_type_param_or_literal_in_index_sig(type_node.kind, param_data.type_annotation);
        if is_generic_or_literal {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                param_idx,
                diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_CANNOT_BE_A_LITERAL_TYPE_OR_GENERIC_TYPE_CONSI,
                diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_CANNOT_BE_A_LITERAL_TYPE_OR_GENERIC_TYPE_CONSI,
            );
        }
        // Note: TS1268 for non-generic/non-literal invalid types is handled
        // separately in the type literal type resolution paths.
    }

    /// Check a type node for validity (recursive).
    ///
    /// Visits nested type nodes to validate constraints. Handles:
    /// - Indexed access types
    /// - Union/intersection types (recurse into members)
    /// - Array types (recurse into element)
    /// - Conditional types (recurse into branches, respecting narrowing)
    /// - Mapped types (check constraint is valid key type via TS2322, recurse into template)
    pub(crate) fn check_type_node(&mut self, node_idx: NodeIndex) {
        let nested_in_type_literal = self.type_node_is_nested_in_type_literal(node_idx);
        self.check_type_node_with_literal_context(node_idx, nested_in_type_literal);
    }
}
