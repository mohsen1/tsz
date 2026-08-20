//! Type alias declaration checking, type node validation, and type query
//! flow-type precomputation.
//!
//! Split from `core.rs` to keep modules under the maintainability threshold.
//! Contains:
//! - `type_alias_reaches_resolving_alias` — circularity detection for type aliases
//! - `check_type_alias_declaration` — validates type alias declarations (TS4109, TS2716, etc.)
//! - `type_arg_directly_references_alias` / `type_arg_references_alias_inner` — recursive alias ref detection
//! - `check_index_sig_param_type_in_type_literal` — TS1337 for index signature params
//! - `check_type_node` — recursive type node validation (mapped types, conditionals, etc.)
//! - `precompute_type_query_flow_types` — pre-computes `typeof` flow-narrowed types

mod type_query_flow;

use super::alias_defid_visited_pool::with_alias_defid_visited;
use crate::query_boundaries::type_checking as type_checking_query;
use crate::state::CheckerState;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_solver::TypeId;

#[inline]
fn record_type_alias_phase_timing(
    file: &str,
    name: Option<&str>,
    phase: &'static str,
    pos: u32,
    end: u32,
    start: Option<web_time::Instant>,
) {
    if let Some(start) = start {
        tsz_common::perf_counters::record_slow_type_alias_check_timing(
            file,
            name,
            phase,
            pos,
            end,
            start.elapsed().as_nanos() as u64,
        );
    }
}

impl<'a> CheckerState<'a> {
    fn conditional_branch_needs_direct_type_ref_validation(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            return true;
        }

        if (node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            || node.kind == syntax_kind_ext::OPTIONAL_TYPE
            || node.kind == syntax_kind_ext::REST_TYPE)
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.conditional_branch_needs_direct_type_ref_validation(wrapped.type_node);
        }

        false
    }

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

    pub(crate) fn type_alias_reaches_resolving_alias(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
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

        let single_active_def_id = if self.ctx.symbol_resolution_set.len() == 1 {
            self.ctx
                .symbol_resolution_set
                .iter()
                .next()
                .and_then(|sid| self.ctx.get_existing_def_id(*sid))
        } else {
            None
        };
        if let Some(active_def_id) = single_active_def_id
            && let Some(&cached) = self
                .ctx
                .type_reference_validation_caches
                .alias_reaches_single_resolving_alias
                .get(&(sym_id, active_def_id))
        {
            return cached;
        }

        let reaches = with_alias_defid_visited(|visited| {
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
        });
        if let Some(active_def_id) = single_active_def_id {
            self.ctx
                .type_reference_validation_caches
                .alias_reaches_single_resolving_alias
                .insert((sym_id, active_def_id), reaches);
        }
        reaches
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

        // TS1273/TS1277: type-alias type parameters allow variance (`in`/`out`)
        // but not `const`; all other modifiers are never valid. First grammar
        // error wins per parameter.
        self.check_type_parameter_modifier_grammar(
            alias.type_parameters.as_ref(),
            /* const_allowed */ false,
            /* variance_allowed */ true,
        );

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
                let constrained_param = type_checking_query::user_type_param(
                    self.ctx.types,
                    atom,
                    constraint,
                    default,
                    false,
                );
                self.ctx
                    .type_parameter_scope
                    .insert(ident.escaped_text.to_string(), constrained_param);
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
        let has_type_params = alias
            .type_parameters
            .as_ref()
            .is_some_and(|params| !params.nodes.is_empty());

        // Check variance annotations match actual usage (TS2636).
        // Resolve the alias body type directly so the solver can compute variance.
        // This must be done while type parameters are still in scope.
        let has_deferred_self_reference = alias_sym_id.is_some_and(|alias_sid| {
            self.alias_ast_is_deferred(alias_sid)
                && self.ctx.symbol_resolution_set.contains(&alias_sid)
                && self.alias_ast_refs_symbol_or_resolution_chain_alias(alias.type_node, alias_sid)
        });
        // Generic aliases still get the syntax/diagnostic body walk below.
        // Defer semantic body construction until a use site supplies type args.
        let skip_eager_generic_alias_body = has_type_params
            && !should_check_variance_annotations
            && !is_generic_self_circular
            && !has_deferred_self_reference
            && self.type_alias_body_allows_lazy_generic_semantic_body(alias.type_node);
        // Register value-space types for `typeof <merged interface+value>` query
        // operands BEFORE eager body resolution. Body resolution evaluates a
        // nested indexed-access / conditional over a deferred `TypeQuery`, and
        // the resolver's `resolve_type_query` must already see the value side at
        // that point (otherwise it falls back to the instance type and reports
        // phantom TS2339s). This only populates the dedicated `typeof_value_types`
        // map; flow narrowing for `typeof` in alias bodies is still computed by
        // `precompute_type_query_flow_types` after validation.
        self.register_type_query_value_types(alias.type_node);

        let body_timing_start = alias_timing_enabled.then(web_time::Instant::now);
        let body_type = {
            let _ = self.ctx.types.take_union_too_complex();
            // Clear any stale tuple_too_large flag before constructing the body
            // so that flag reads below are attributable to this alias alone.
            let _ = self.ctx.types.take_tuple_too_large();
            let body_type = if skip_eager_generic_alias_body {
                TypeId::UNKNOWN
            } else if has_deferred_self_reference {
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
        // Generic aliases are checked at declaration time, but their bodies are
        // not fully instantiated until concrete type arguments are supplied.
        let body_evaluation_too_complex =
            if has_deferred_self_reference || has_type_params || skip_eager_generic_alias_body {
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
                body_produced_too_large_tuple = body_produced_too_large_tuple
                    || self.alias_body_owns_too_large_tuple(body_type);
                self.ctx.types.take_union_too_complex()
            };
        let registration_timing_start = alias_timing_enabled.then(web_time::Instant::now);
        if !skip_eager_generic_alias_body
            && body_type != TypeId::ERROR
            && let Some(alias_sid) = alias_sym_id
        {
            let type_params = self.current_alias_type_params(alias.type_parameters.as_ref());
            let can_register_non_generic_conditional = type_params.is_empty()
                && crate::query_boundaries::common::is_conditional_type(self.ctx.types, body_type)
                && !crate::query_boundaries::checkers::generic::contains_named_or_bound_type_parameter(
                    self.ctx.types,
                    body_type,
                )
                && !crate::query_boundaries::common::is_distributive_conditional_with_deferred_check(
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
        if !skip_eager_generic_alias_body
            && (body_construction_too_complex || body_evaluation_too_complex)
        {
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
        if !skip_eager_generic_alias_body
            && let Some(alias_sid) = alias_sym_id
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
        if !skip_eager_generic_alias_body && let Some(alias_sid) = alias_sym_id {
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
        // A template literal body collapses through its eagerly-evaluated
        // interpolation spans, so the argument-bearing self-application that
        // earns the body-position TS2315 lives inside a `${...}` span rather than
        // at the top of the body (`type Str<T> = \`${T}${Str<...>}\``).
        if node.kind == syntax_kind_ext::TEMPLATE_LITERAL_TYPE {
            for span_expr in self.template_literal_type_span_expressions(ref_idx) {
                self.validate_collapsed_alias_body_reference(span_expr);
            }
            return;
        }
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
                        let name = ident.escaped_text.to_string();
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
        let mut pushes: Vec<(String, Option<TypeId>)> = Vec::new();
        for (name, &constraint) in infer_names.iter().zip(infer_constraints.iter()) {
            let atom = self.ctx.types.intern_string(name);
            let provisional =
                type_checking_query::user_type_param(self.ctx.types, atom, constraint, None, false);
            let previous = self
                .ctx
                .type_parameter_scope
                .insert(name.clone(), provisional);
            pushes.push((name.clone(), previous));
        }
        pushes
    }

    pub(crate) fn pop_infer_bindings(&mut self, pushes: Vec<(String, Option<TypeId>)>) {
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

        // Check AST to detect type parameters and literal types (TS1337). The
        // AST walk over-reports "generic" for an instantiated generic-alias
        // application (e.g. `Brand<string, 'event'>`); drop the spurious TS1337
        // when the resolved key is a concrete valid index key. See
        // `resolved_index_key_is_concrete_valid` for the full rationale.
        let key_type = self.get_type_from_type_node_in_type_literal(param_data.type_annotation);
        let is_generic_or_literal = self
            .is_type_param_or_literal_in_index_sig(type_node.kind, param_data.type_annotation)
            && !self.resolved_index_key_is_concrete_valid(key_type);
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
        let scope_key = self.type_reference_arg_validation_scope_key();
        let active_alias_key = self.active_resolving_alias_set_key();
        self.check_type_node_with_literal_context(
            node_idx,
            nested_in_type_literal,
            scope_key,
            active_alias_key,
        );
    }

    fn check_type_node_with_literal_context(
        &mut self,
        node_idx: NodeIndex,
        nested_in_type_literal: bool,
        scope_key: u64,
        active_alias_key: u64,
    ) {
        if node_idx == NodeIndex::NONE {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };
        let validation_cache_key = (
            node_idx.0,
            nested_in_type_literal,
            scope_key,
            active_alias_key,
        );
        if self
            .ctx
            .type_reference_validation_caches
            .type_node_validation
            .contains(&validation_cache_key)
        {
            return;
        }
        let diagnostics_before = self.ctx.diagnostics.len();
        let child_nested_in_type_literal =
            nested_in_type_literal || node.kind == syntax_kind_ext::TYPE_LITERAL;
        macro_rules! check_child_type_node {
            ($checker:expr, $child:expr) => {
                $checker.check_type_node_with_literal_context(
                    $child,
                    child_nested_in_type_literal,
                    scope_key,
                    active_alias_key,
                )
            };
        }
        macro_rules! check_child_type_node_in_current_scope {
            ($checker:expr, $child:expr) => {
                $checker.check_type_node_with_literal_context(
                    $child,
                    child_nested_in_type_literal,
                    $checker.type_reference_arg_validation_scope_key(),
                    $checker.active_resolving_alias_set_key(),
                )
            };
        }

        match node.kind {
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                self.check_indexed_access_type(node_idx);
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    check_child_type_node!(self, indexed.object_type);
                    check_child_type_node!(self, indexed.index_type);
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &child in &composite.types.nodes {
                        check_child_type_node!(self, child);
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    check_child_type_node!(self, arr.element_type);
                }
            }
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    check_child_type_node!(self, wrapped.type_node);
                }
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                // `keyof`/`readonly`/`unique` operators wrap a regular operand type
                // node. Recurse into it so nested errors (e.g. an invalid indexed
                // access `keyof A[T]` → TS2536, or an unresolved name) are reported
                // just as tsc validates them — the operand was previously dropped by
                // the catch-all arm, silently swallowing those diagnostics.
                if let Some(type_op) = self.ctx.arena.get_type_operator(node) {
                    check_child_type_node!(self, type_op.type_node);
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node) {
                    let is_bare_scoped_type_parameter = self
                        .type_ref_is_bare_scoped_type_parameter(
                            type_ref.type_name,
                            type_ref.type_arguments.as_ref(),
                        );
                    if !is_bare_scoped_type_parameter {
                        if let Some(sym_id) = self
                            .resolve_type_symbol_for_lowering(type_ref.type_name)
                            .map(tsz_binder::SymbolId)
                            && (self.ctx.symbol_resolution_set.contains(&sym_id)
                                || self.type_alias_reaches_resolving_alias(sym_id))
                        {
                            return;
                        }
                        let explicit_validation_done =
                            self.check_explicit_type_reference_for_alias_body_validation(node_idx);
                        let type_arguments_checked_by_validation = explicit_validation_done
                            && type_ref
                                .type_arguments
                                .as_ref()
                                .is_some_and(|type_arguments| {
                                    type_arguments.nodes.iter().all(|arg_idx| {
                                        self.ctx
                                            .type_reference_validation_caches
                                            .type_node_validation
                                            .contains(&(
                                                arg_idx.0,
                                                child_nested_in_type_literal,
                                                scope_key,
                                                active_alias_key,
                                            ))
                                    })
                                });
                        if !type_arguments_checked_by_validation
                            && let Some(type_arguments) = &type_ref.type_arguments
                        {
                            for &arg_idx in &type_arguments.nodes {
                                check_child_type_node!(self, arg_idx);
                            }
                        }
                        if !explicit_validation_done {
                            let _ = if nested_in_type_literal {
                                self.get_type_from_type_node_in_type_literal(node_idx)
                            } else {
                                self.get_type_from_type_node(node_idx)
                            };
                        }
                        self.check_styled_component_inner_component_constraint(node_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        let Some(member_node) = self.ctx.arena.get(member_idx) else {
                            continue;
                        };
                        if member_node.kind == syntax_kind_ext::MAPPED_TYPE {
                            check_child_type_node!(self, member_idx);
                            continue;
                        }
                        if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                            {
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                // TS1170 (syntactic form) and check_computed_property_name's
                                // diagnostics (TS2464 property-key-type, TS1212/1213 reserved
                                // words, await-grammar) are independent checks in tsc — it
                                // emits both when both conditions hold. Verified against
                                // tsc@7.0.2: `type U = { [b as unknown as boolean]: number }`
                                // (`b: boolean`) reports both TS1170 and TS2464 at the same
                                // position, so the literal-form check must never gate the
                                // shared funnel down to the await-only half.
                                let _ = self.check_computed_property_requires_literal(
                                    sig.name,
                                    diagnostic_messages::A_COMPUTED_PROPERTY_NAME_IN_A_TYPE_LITERAL_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYP,
                                    diagnostic_codes::A_COMPUTED_PROPERTY_NAME_IN_A_TYPE_LITERAL_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYP,
                                );
                                self.check_computed_property_name(sig.name);
                            }
                            let (_type_params, type_param_updates) =
                                self.push_type_parameters(&sig.type_parameters);
                            if let Some(params) = &sig.parameters {
                                for &param_idx in &params.nodes {
                                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                                        && let Some(param) =
                                            self.ctx.arena.get_parameter(param_node)
                                        && param.type_annotation != NodeIndex::NONE
                                    {
                                        check_child_type_node_in_current_scope!(
                                            self,
                                            param.type_annotation
                                        );
                                    }
                                }
                            }
                            // TS2370: a rest parameter must be of an array type.
                            // Method signatures in a type literal reach this general
                            // walk (the call/construct-only fast path is handled by
                            // `validate_signature_only_type_literal_alias_body`), so
                            // the rest check belongs here for parity with interfaces.
                            self.check_rest_parameter_types(
                                sig.parameters.as_ref().map_or(&[][..], |p| &p.nodes),
                            );
                            if sig.type_annotation != NodeIndex::NONE {
                                check_child_type_node_in_current_scope!(self, sig.type_annotation);
                            }
                            self.pop_type_parameters(type_param_updates);
                            continue;
                        }
                        if let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) {
                            if index_sig.type_annotation != NodeIndex::NONE {
                                check_child_type_node!(self, index_sig.type_annotation);
                            }
                            // TS1337: Check index signature parameter type for
                            // generic type parameters or literal types.
                            self.check_index_sig_param_type_in_type_literal(&index_sig.parameters);
                            continue;
                        }
                        if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                            self.check_computed_property_name(accessor.name);
                            if accessor.type_annotation != NodeIndex::NONE {
                                check_child_type_node!(self, accessor.type_annotation);
                            }
                            // Also check set accessor parameter type annotations
                            // for constraint validation (TS2344).
                            if member_node.kind == syntax_kind_ext::SET_ACCESSOR {
                                for &param_idx in &accessor.parameters.nodes {
                                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                                        && let Some(param) =
                                            self.ctx.arena.get_parameter(param_node)
                                        && param.type_annotation != NodeIndex::NONE
                                    {
                                        check_child_type_node!(self, param.type_annotation);
                                    }
                                }
                            }
                            continue;
                        }
                        // Property signatures/declarations: recurse into type
                        // annotations to validate nested type references.
                        if let Some(prop) = self.ctx.arena.get_property_decl(member_node) {
                            {
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                // See the signature-member arm above: TS1170 and
                                // check_computed_property_name's diagnostics are
                                // independent checks, so the literal-form check must
                                // never gate the shared funnel down to the await-only half.
                                let _ = self.check_computed_property_requires_literal(
                                    prop.name,
                                    diagnostic_messages::A_COMPUTED_PROPERTY_NAME_IN_A_TYPE_LITERAL_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYP,
                                    diagnostic_codes::A_COMPUTED_PROPERTY_NAME_IN_A_TYPE_LITERAL_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYP,
                                );
                                self.check_computed_property_name(prop.name);
                            }
                            if prop.type_annotation != NodeIndex::NONE {
                                check_child_type_node!(self, prop.type_annotation);
                            }
                        }
                    }

                    let is_type_alias_body = self
                        .ctx
                        .arena
                        .get_extended(node_idx)
                        .and_then(|ext| ext.parent.is_some().then_some(ext.parent))
                        .and_then(|parent_idx| self.ctx.arena.get(parent_idx))
                        .is_some_and(|parent| {
                            parent.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                        });
                    if is_type_alias_body
                        && self.type_literal_has_circular_accessor_reference(node_idx)
                    {
                        let _ = self.get_type_from_type_literal(node_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                // Recurse into conditional type branches to validate nested
                // mapped type constraints (e.g., `string extends T ? { [P in T]: V } : T`).
                //
                // Scoping subtlety: in `CheckType extends ExtendsType ? TrueType : FalseType`,
                // the true branch narrows CheckType to `CheckType & ExtendsType` when
                // CheckType is a type parameter. This means mapped types in the true branch
                // may be valid even if the unconstrained type parameter isn't a valid key.
                // (e.g., `T extends string ? { [P in T]: void } : T` — T is narrowed to string)
                //
                // Only visit a branch when:
                // 1. It IS a mapped type (direct child), AND
                // 2. For the true branch: the check type is NOT a type parameter reference
                //    (no narrowing applies, so the mapped type key isn't silently valid).
                //
                // This minimizes side effects from type resolution while still catching
                // invalid mapped type keys inside conditional types.
                //
                // Infer-binding scope: `infer X` declarations in ExtendsType bind `X` in
                // TrueType only. Push them as provisional type parameters only while
                // recursing into TrueType so references to `X` inside FalseType still
                // report TS2304 like `tsc`.
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    let true_is_mapped = self
                        .ctx
                        .arena
                        .get(cond.true_type)
                        .is_some_and(|n| n.kind == syntax_kind_ext::MAPPED_TYPE);
                    let true_needs_direct_type_ref_validation =
                        self.conditional_branch_needs_direct_type_ref_validation(cond.true_type);
                    if true_needs_direct_type_ref_validation {
                        let infer_pushes = self.push_infer_bindings_from_extends(cond.extends_type);
                        check_child_type_node_in_current_scope!(self, cond.true_type);
                        self.pop_infer_bindings(infer_pushes);
                    } else if true_is_mapped {
                        // Check if the check type resolves to a type parameter.
                        // If so, mapped true branches benefit from narrowing and
                        // we skip them. Direct type-reference branches still need
                        // their generic constraints checked under the conditional
                        // `infer` bindings, but they do not need this potentially
                        // expensive check-type resolution.
                        let check_type = self.get_type_from_type_node(cond.check_type);
                        let check_is_type_param =
                            crate::query_boundaries::common::is_type_parameter_like(
                                self.ctx.types,
                                check_type,
                            );
                        if !check_is_type_param {
                            let infer_pushes =
                                self.push_infer_bindings_from_extends(cond.extends_type);
                            check_child_type_node_in_current_scope!(self, cond.true_type);
                            self.pop_infer_bindings(infer_pushes);
                        }
                    }
                    let false_is_mapped = self
                        .ctx
                        .arena
                        .get(cond.false_type)
                        .is_some_and(|n| n.kind == syntax_kind_ext::MAPPED_TYPE);
                    if false_is_mapped {
                        check_child_type_node!(self, cond.false_type);
                    }
                    if self.ctx.compiler_options.no_unused_parameters {
                        self.check_unused_infer_type_params_in_conditional(cond);
                    }
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                self.check_mapped_type_constraint(node_idx);
                // Recurse into mapped type template to validate nested types.
                // Push the mapped type parameter into scope so references like `K`
                // in `{ [K in keyof T]: { src: K } }` resolve correctly and don't
                // produce false TS2304 errors.
                if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                    let mut pushed_name: Option<(String, Option<TypeId>)> = None;
                    if let Some(tp_node) = self.ctx.arena.get(mapped.type_parameter)
                        && let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node)
                        && let Some(name_node) = self.ctx.arena.get(tp_data.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        let name = ident.escaped_text.to_string();
                        let atom = self.ctx.types.intern_string(&name);
                        let mut constraint_type = TypeId::UNKNOWN;
                        if tp_data.constraint != tsz_parser::parser::NodeIndex::NONE {
                            check_child_type_node_in_current_scope!(self, tp_data.constraint);
                            let resolved = self.get_type_from_type_node(tp_data.constraint);
                            if resolved != TypeId::ERROR {
                                constraint_type = resolved;
                            }
                        }
                        let provisional = type_checking_query::user_type_param(
                            self.ctx.types,
                            atom,
                            Some(constraint_type),
                            None,
                            false,
                        );
                        let previous = self
                            .ctx
                            .type_parameter_scope
                            .insert(name.clone(), provisional);
                        pushed_name = Some((name, previous));
                    }
                    if mapped.type_node != NodeIndex::NONE {
                        check_child_type_node_in_current_scope!(self, mapped.type_node);
                    }
                    // Also recurse into the name_type (the `as` clause) which may
                    // reference the mapped type parameter.
                    if mapped.name_type != NodeIndex::NONE {
                        check_child_type_node_in_current_scope!(self, mapped.name_type);
                    }
                    if let Some((name, previous)) = pushed_name {
                        if let Some(prev_type) = previous {
                            self.ctx.type_parameter_scope.insert(name, prev_type);
                        } else {
                            self.ctx.type_parameter_scope.remove(&name);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                // Force tuple element validation (TS1257, TS1265, TS1266)
                // which lives inside get_type_from_tuple_type.
                let _ = self.get_type_from_type_node(node_idx);
                // Recurse into tuple elements to validate nested type nodes
                // (e.g., indexed access types inside tuples need TS2536/TS4105 checks).
                if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
                    // Arena-backed `&'a` borrow (never mutated during checking):
                    // iterate in place instead of cloning a throwaway `Vec` per node (#11617).
                    for &element_idx in &tuple.elements.nodes {
                        check_child_type_node!(self, element_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                // Force function/constructor type validation (TS2371 for parameter
                // initializers in type position, including binding element defaults).
                let _ = self.get_type_from_type_node(node_idx);

                // TS2370: Check that rest parameters have array types.
                // This is needed because function/constructor types in type aliases
                // don't go through the normal function declaration checking path.
                //
                // Push the function type's own type parameters into scope so that
                // rest parameter annotations referencing them (e.g. `<L>(...args: L)`)
                // resolve correctly instead of emitting a spurious TS2304.
                // `get_type_from_function_type` pushes/pops these internally, so by
                // the time we reach this sibling check the scope no longer contains
                // the inner signature's type parameters.
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    // Arena-backed `&'a` borrow: `func_type`'s type-parameter and
                    // parameter lists outlive the `&mut self` checks below, so reference
                    // them in place instead of cloning two throwaway `Vec`s per node (#11617).
                    let (_type_params, tp_updates) =
                        self.push_type_parameters(&func_type.type_parameters);
                    for &param_idx in &func_type.parameters.nodes {
                        let param_type_annotation = (|| {
                            let param_node = self.ctx.arena.get(param_idx)?;
                            let param = self.ctx.arena.get_parameter(param_node)?;
                            param
                                .type_annotation
                                .is_some()
                                .then_some(param.type_annotation)
                        })();
                        if let Some(param_type_annotation) = param_type_annotation {
                            check_child_type_node_in_current_scope!(self, param_type_annotation);
                        }
                    }
                    if func_type.type_annotation.is_some() {
                        check_child_type_node_in_current_scope!(self, func_type.type_annotation);
                    }
                    self.check_rest_parameter_types(&func_type.parameters.nodes);
                    self.pop_type_parameters(tp_updates);
                }
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                // `typeof expr<Args>` — validate instantiation expression type args.
                if let Some(type_query) = self.ctx.arena.get_type_query(node)
                    && let Some(args) = &type_query.type_arguments
                {
                    // Arena-backed `&'a` borrow: the type-argument list outlives the
                    // `&mut self` checks, so iterate and reuse it in place instead of
                    // cloning a throwaway `Vec` per node (#11617).
                    for &arg_idx in &args.nodes {
                        check_child_type_node!(self, arg_idx);
                    }
                    let expr_name = type_query.expr_name;
                    let expr_type = if self
                        .ctx
                        .arena
                        .get(expr_name)
                        .is_some_and(|expr| expr.kind == syntax_kind_ext::QUALIFIED_NAME)
                    {
                        self.resolve_typeof_qualified_value_chain(expr_name, true)
                    } else {
                        self.get_type_of_node(expr_name)
                    };
                    let num_type_args = args.nodes.len();
                    self.check_instantiation_expression_type_args(
                        expr_type,
                        num_type_args,
                        node_idx,
                        &args.nodes,
                    );
                }
            }
            _ => {}
        }

        if self.ctx.diagnostics.len() == diagnostics_before {
            self.ctx
                .type_reference_validation_caches
                .type_node_validation
                .insert(validation_cache_key);
        }
    }

    /// Check TS2635/TS2344 for instantiation expression type arguments.
    fn check_instantiation_expression_type_args(
        &mut self,
        expr_type: TypeId,
        num_type_args: usize,
        type_query_idx: NodeIndex,
        type_arg_nodes: &[NodeIndex],
    ) {
        if expr_type == TypeId::ERROR || expr_type == TypeId::ANY {
            return;
        }

        if let Some(error_type) =
            self.instantiation_expression_applicability_error_type(expr_type, num_type_args)
        {
            // Skip TS2635 if any type argument node contains parse errors (e.g. JSDoc
            // syntax like `?string` outside documentation comments). tsc reports the
            // syntax errors but does not validate type argument applicability in that case.
            if type_arg_nodes
                .iter()
                .any(|&node| self.node_contains_any_parse_error(node))
            {
                return;
            }
            if let Some(error_node) = type_arg_nodes.first().copied() {
                let base_expr = self
                    .ctx
                    .arena
                    .get(type_query_idx)
                    .and_then(|node| self.ctx.arena.get_type_query(node))
                    .map(|type_query| type_query.expr_name)
                    .unwrap_or(type_query_idx);
                self.error_no_applicable_signatures_for_type_args_with_base(
                    error_type, error_node, base_expr,
                );
            }
            return;
        }

        self.validate_instantiation_expression_type_arg_constraints(expr_type, type_arg_nodes);
    }

    fn validate_instantiation_expression_type_arg_constraints(
        &mut self,
        expr_type: TypeId,
        type_arg_nodes: &[NodeIndex],
    ) {
        if type_arg_nodes.is_empty() {
            return;
        }

        let type_args_list = NodeList {
            nodes: type_arg_nodes.to_vec(),
            pos: 0,
            end: 0,
            has_trailing_comma: false,
        };
        let expr_type = self.resolve_lazy_type(expr_type);

        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, expr_type)
            && shape.type_params.len() == type_arg_nodes.len()
        {
            let type_params = shape.type_params.clone();
            self.validate_type_args_against_params(&type_params, &type_args_list);
        }

        if let Some(sigs) =
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, expr_type)
        {
            let matching: Vec<Vec<tsz_solver::TypeParamInfo>> = sigs
                .iter()
                .filter(|sig| sig.type_params.len() == type_arg_nodes.len())
                .map(|sig| sig.type_params.clone())
                .collect();
            for type_params in matching {
                self.validate_type_args_against_params(&type_params, &type_args_list);
            }
        }

        if let Some(sigs) = crate::query_boundaries::common::construct_signatures_for_type(
            self.ctx.types,
            expr_type,
        ) {
            let matching: Vec<Vec<tsz_solver::TypeParamInfo>> = sigs
                .iter()
                .filter(|sig| sig.type_params.len() == type_arg_nodes.len())
                .map(|sig| sig.type_params.clone())
                .collect();
            for type_params in matching {
                self.validate_type_args_against_params(&type_params, &type_args_list);
            }
        }
    }

    fn type_query_targets_generic_function_like_with_arity(
        &self,
        type_query_idx: NodeIndex,
        num_type_args: usize,
    ) -> bool {
        let Some(type_query_node) = self.ctx.arena.get(type_query_idx) else {
            return false;
        };
        let Some(type_query) = self.ctx.arena.get_type_query(type_query_node) else {
            return false;
        };
        let Some(sym_u32) = self.resolve_value_symbol_for_lowering(type_query.expr_name) else {
            return false;
        };
        let sym_id = tsz_binder::SymbolId(sym_u32);
        let value_decl = self
            .get_cross_file_symbol(sym_id)
            .map(|symbol| symbol.value_declaration)
            .or_else(|| {
                self.ctx
                    .binder
                    .get_symbol(sym_id)
                    .map(|symbol| symbol.value_declaration)
            })
            .unwrap_or(NodeIndex::NONE);
        if value_decl.is_none() {
            return false;
        }
        let Some(decl_node) = self.ctx.arena.get(value_decl) else {
            return false;
        };
        if let Some(func) = self.ctx.arena.get_function(decl_node) {
            return func
                .type_parameters
                .as_ref()
                .map_or(0, |tps| tps.nodes.len())
                == num_type_args;
        }
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
            && var_decl.initializer.is_some()
            && let Some(init_node) = self.ctx.arena.get(var_decl.initializer)
            && let Some(func) = self.ctx.arena.get_function(init_node)
        {
            return func
                .type_parameters
                .as_ref()
                .map_or(0, |tps| tps.nodes.len())
                == num_type_args;
        }
        false
    }

    fn current_alias_type_params(
        &self,
        type_parameters: Option<&NodeList>,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        let Some(type_parameters) = type_parameters else {
            return Vec::new();
        };

        type_parameters
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let param_node = self.ctx.arena.get(param_idx)?;
                let param = self.ctx.arena.get_type_parameter(param_node)?;
                let name_node = self.ctx.arena.get(param.name)?;
                let ident = self.ctx.arena.get_identifier(name_node)?;
                let type_id = self
                    .ctx
                    .type_parameter_scope
                    .get(ident.escaped_text.as_str())?;
                crate::query_boundaries::checkers::generic::named_type_param_info(
                    self.ctx.types,
                    *type_id,
                )
            })
            .collect()
    }
}
