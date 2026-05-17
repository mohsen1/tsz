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

use crate::state::CheckerState;
use std::cell::RefCell;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

// Reusable scratch `FxHashSet<DefId>` for the alias-resolution DFS in this
// module. Mirrors the pool pattern from #4722 / #4790 and follow-up PRs.
thread_local! {
    static ALIAS_DEFID_VISITED_POOL: RefCell<Option<rustc_hash::FxHashSet<DefId>>> =
        const { RefCell::new(None) };
}

#[inline]
fn with_alias_defid_visited<R>(f: impl FnOnce(&mut rustc_hash::FxHashSet<DefId>) -> R) -> R {
    let mut visited = ALIAS_DEFID_VISITED_POOL
        .with(|p| p.borrow_mut().take())
        .unwrap_or_default();
    visited.clear();
    let r = f(&mut visited);
    ALIAS_DEFID_VISITED_POOL.with(|p| {
        let mut slot = p.borrow_mut();
        let keep = match &*slot {
            None => true,
            Some(existing) => visited.capacity() >= existing.capacity(),
        };
        if keep {
            *slot = Some(visited);
        }
    });
    r
}

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

        let resolving_defs: rustc_hash::FxHashSet<_> = self
            .ctx
            .symbol_resolution_set
            .iter()
            .filter_map(|sid| self.ctx.get_existing_def_id(*sid))
            .collect();
        if resolving_defs.is_empty() {
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
                if resolving_defs.contains(&def_id) {
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

    /// Check a type alias declaration.
    pub(crate) fn check_type_alias_declaration(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };
        let Some(alias) = self.ctx.arena.get_type_alias(node) else {
            return;
        };

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
        // Temporarily register this alias in `symbol_resolution_set` before visiting
        // the type body. This is used by TS4110 (tuple type circularity) and other
        // circular-reference detection during type node checking.
        let alias_sym_id = self.ctx.binder.get_node_symbol(node_idx);
        let inserted_for_circular_check = alias_sym_id
            .map(|sid| self.ctx.symbol_resolution_set.insert(sid))
            .unwrap_or(false);

        let variance_annotations_supported =
            self.check_variance_annotations_supported_for_type_alias(alias);

        // Check variance annotations match actual usage (TS2636).
        // Resolve the alias body type directly so the solver can compute variance.
        // This must be done while type parameters are still in scope.
        let has_deferred_self_reference = alias_sym_id.is_some_and(|alias_sid| {
            self.alias_ast_is_deferred(alias_sid)
                && self.ctx.symbol_resolution_set.contains(&alias_sid)
                && self.alias_ast_refs_symbol_or_resolution_chain_alias(alias.type_node, alias_sid)
        });
        let body_type = {
            let _ = self.ctx.types.take_union_too_complex();
            let body_type = if has_deferred_self_reference {
                crate::TypeNodeChecker::new(&mut self.ctx).check(alias.type_node)
            } else {
                self.get_type_from_type_node(alias.type_node)
            };
            if variance_annotations_supported {
                self.check_variance_annotations_with_body(
                    node_idx,
                    &alias.type_parameters,
                    Some(body_type),
                );
            }
            self.check_styled_component_inner_component_constraint(alias.type_node);
            body_type
        };
        let body_construction_too_complex = self.ctx.types.take_union_too_complex();
        let has_type_params = alias
            .type_parameters
            .as_ref()
            .is_some_and(|params| !params.nodes.is_empty());
        // Generic aliases are checked at declaration time, but their bodies are
        // not fully instantiated until concrete type arguments are supplied.
        let body_evaluation_too_complex = if has_deferred_self_reference || has_type_params {
            false
        } else {
            let _ = self.evaluate_type_with_env_uncached(body_type);
            self.ctx.types.take_union_too_complex()
        };
        if body_type != TypeId::ERROR
            && let Some(alias_sid) = alias_sym_id
        {
            let type_params = self.current_alias_type_params(alias.type_parameters.as_ref());
            let can_register_non_generic_conditional = type_params.is_empty()
                && crate::query_boundaries::common::is_conditional_type(self.ctx.types, body_type)
                && !crate::query_boundaries::checkers::generic::contains_named_or_bound_type_parameter(
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
        if self.type_node_produces_too_large_tuple(alias.type_node) {
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

        // TS2589: detect excessively deep type instantiation at definition time.
        // tsc emits TS2589 for type aliases whose body contains conditional types
        // that self-reference and create infinite expansion (e.g.,
        // `type Foo<T> = T extends unknown ? Foo<T> : unknown`).
        // We check this by:
        // 1. Verifying the body references the alias's own DefId
        // 2. Registering the body temporarily so the evaluator can resolve it
        // 3. Evaluating with a special flag that detects Application cycle = TS2589
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
                // Collect type params that were pushed into scope above
                let type_params: Vec<tsz_solver::TypeParamInfo> = alias
                    .type_parameters
                    .as_ref()
                    .map(|tps| {
                        tps.nodes
                            .iter()
                            .filter_map(|&param_idx| {
                                let param_node = self.ctx.arena.get(param_idx)?;
                                let param = self.ctx.arena.get_type_parameter(param_node)?;
                                let name_node = self.ctx.arena.get(param.name)?;
                                let ident = self.ctx.arena.get_identifier(name_node)?;
                                let atom = self.ctx.types.intern_string(&ident.escaped_text);
                                Some(tsz_solver::TypeParamInfo {
                                    name: atom,
                                    constraint: None,
                                    default: None,
                                    is_const: false,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                // Register body temporarily for evaluation
                self.ctx
                    .register_def_auto_params_in_envs(def_id, body_type, type_params);

                // Evaluate with TS2589 detection flag
                let depth_exceeded = (has_stable_recursive_ref || has_recursive_wrapper_arg)
                    && self.evaluate_type_for_ts2589_check(body_type, def_id);
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
        } else {
            self.check_type_node(alias.type_node);
            self.check_type_for_missing_names(alias.type_node);
        }

        if inserted_for_circular_check && let Some(sid) = alias_sym_id {
            self.ctx.symbol_resolution_set.remove(&sid);
        }
        // Pre-compute flow-narrowed types for `typeof expr` in the type alias body.
        // This allows `typeof c` inside a type alias to pick up narrowing from
        // control flow (e.g., inside an `if (typeof c === 'string')` block).
        // The results are stored in `node_types` and consumed by `TypeLowering`
        // via the `type_query_override` callback during `ensure_type_alias_resolved`.
        self.precompute_type_query_flow_types(alias.type_node);
        self.pop_type_parameters(updates);
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

    fn type_arg_nodes_all_are_deferred_passthrough_for_depth_check(
        &mut self,
        type_args: &tsz_parser::parser::NodeList,
    ) -> bool {
        !type_args.nodes.is_empty()
            && type_args
                .nodes
                .iter()
                .copied()
                .all(|node_idx| self.type_node_is_deferred_passthrough_for_depth_check(node_idx))
    }

    fn type_node_is_deferred_passthrough_for_depth_check(&mut self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if let Some(identifier) = self.ctx.arena.get_identifier(node)
            && self
                .ctx
                .type_parameter_scope
                .contains_key(&identifier.escaped_text)
        {
            return true;
        }
        if let Some(identifier) = self.ctx.arena.get_identifier(node)
            && self
                .identifier_references_enclosing_infer_binding(node_idx, &identifier.escaped_text)
        {
            return true;
        }

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
        {
            if type_ref
                .type_arguments
                .as_ref()
                .is_some_and(|type_args| !type_args.nodes.is_empty())
            {
                return false;
            }

            return self.type_name_is_deferred_passthrough_for_depth_check(type_ref.type_name);
        }

        false
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

    fn type_node_contains_unresolved_type_reference_for_depth_check(
        &mut self,
        node_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
            && self
                .resolve_type_symbol_for_lowering(type_ref.type_name)
                .is_none()
            && self
                .ctx
                .arena
                .kind_at(type_ref.type_name)
                .is_some_and(|kind| kind == syntax_kind_ext::QUALIFIED_NAME)
            && !self.type_name_is_deferred_passthrough_for_depth_check(type_ref.type_name)
        {
            return true;
        }

        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| {
                self.type_node_contains_unresolved_type_reference_for_depth_check(child_idx)
            })
    }

    fn type_name_is_deferred_passthrough_for_depth_check(&mut self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        let Some(identifier) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };

        self.ctx
            .type_parameter_scope
            .contains_key(&identifier.escaped_text)
            || self
                .identifier_references_enclosing_infer_binding(name_idx, &identifier.escaped_text)
    }

    fn identifier_references_enclosing_infer_binding(
        &self,
        node_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let mut current = node_idx;
        for _ in 0..50 {
            let parent = self
                .ctx
                .arena
                .get_extended(current)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if let Some(conditional) = self.ctx.arena.get_conditional_type(parent_node)
                && self.type_node_contains_infer_binding_named(conditional.extends_type, name)
            {
                return true;
            }
            current = parent;
        }
        false
    }

    fn type_node_contains_infer_binding_named(&self, node_idx: NodeIndex, name: &str) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::INFER_TYPE
            && let Some(infer_data) = self.ctx.arena.get_infer_type(node)
            && let Some(type_param_node) = self.ctx.arena.get(infer_data.type_parameter)
            && let Some(type_param) = self.ctx.arena.get_type_parameter(type_param_node)
            && let Some(name_node) = self.ctx.arena.get(type_param.name)
            && let Some(identifier) = self.ctx.arena.get_identifier(name_node)
        {
            return identifier.escaped_text == name;
        }

        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| self.type_node_contains_infer_binding_named(child_idx, name))
    }

    /// Walk `extends_type` collecting every `infer X` binding and push each as a
    /// provisional type parameter into `type_parameter_scope`. Returns save-state
    /// for `pop_infer_bindings`.
    fn push_infer_bindings_from_extends(
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
            if node.kind == syntax_kind_ext::INFER_TYPE {
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
        if node_idx == NodeIndex::NONE {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                self.check_indexed_access_type(node_idx);
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.check_type_node(indexed.object_type);
                    self.check_type_node(indexed.index_type);
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &child in &composite.types.nodes {
                        self.check_type_node(child);
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.check_type_node(arr.element_type);
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                    && let Some(type_arguments) = &type_ref.type_arguments
                {
                    for &arg_idx in &type_arguments.nodes {
                        self.check_type_node(arg_idx);
                    }
                }
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                    && let Some(sym_id) = self
                        .resolve_type_symbol_for_lowering(type_ref.type_name)
                        .map(tsz_binder::SymbolId)
                    && (self.ctx.symbol_resolution_set.contains(&sym_id)
                        || self.type_alias_reaches_resolving_alias(sym_id))
                {
                    return;
                }
                let _ = if self.type_node_is_nested_in_type_literal(node_idx) {
                    self.get_type_from_type_node_in_type_literal(node_idx)
                } else {
                    self.get_type_from_type_node(node_idx)
                };
                self.check_styled_component_inner_component_constraint(node_idx);
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        let Some(member_node) = self.ctx.arena.get(member_idx) else {
                            continue;
                        };
                        if member_node.kind == syntax_kind_ext::MAPPED_TYPE {
                            self.check_type_node(member_idx);
                            continue;
                        }
                        if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                            let (_type_params, type_param_updates) =
                                self.push_type_parameters(&sig.type_parameters);
                            if let Some(params) = &sig.parameters {
                                for &param_idx in &params.nodes {
                                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                                        && let Some(param) =
                                            self.ctx.arena.get_parameter(param_node)
                                        && param.type_annotation != NodeIndex::NONE
                                    {
                                        self.check_type_node(param.type_annotation);
                                    }
                                }
                            }
                            if sig.type_annotation != NodeIndex::NONE {
                                self.check_type_node(sig.type_annotation);
                            }
                            self.pop_type_parameters(type_param_updates);
                            continue;
                        }
                        if let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) {
                            if index_sig.type_annotation != NodeIndex::NONE {
                                self.check_type_node(index_sig.type_annotation);
                            }
                            // TS1337: Check index signature parameter type for
                            // generic type parameters or literal types.
                            self.check_index_sig_param_type_in_type_literal(&index_sig.parameters);
                            continue;
                        }
                        if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                            if accessor.type_annotation != NodeIndex::NONE {
                                self.check_type_node(accessor.type_annotation);
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
                                        self.check_type_node(param.type_annotation);
                                    }
                                }
                            }
                            continue;
                        }
                        // Property signatures/declarations: recurse into type
                        // annotations to validate nested type references.
                        if let Some(prop) = self.ctx.arena.get_property_decl(member_node)
                            && prop.type_annotation != NodeIndex::NONE
                        {
                            self.check_type_node(prop.type_annotation);
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
                    if true_is_mapped {
                        // Check if the check type resolves to a type parameter.
                        // If so, the true branch benefits from narrowing and we
                        // skip it. Use get_type_from_type_node which is safe here
                        // because we only call it on the check type (not the
                        // branches), and only when a mapped type is present.
                        let check_type = self.get_type_from_type_node(cond.check_type);
                        let check_is_type_param =
                            crate::query_boundaries::common::is_type_parameter_like(
                                self.ctx.types,
                                check_type,
                            );
                        if !check_is_type_param {
                            let infer_pushes =
                                self.push_infer_bindings_from_extends(cond.extends_type);
                            self.check_type_node(cond.true_type);
                            self.pop_infer_bindings(infer_pushes);
                        }
                    }
                    let false_is_mapped = self
                        .ctx
                        .arena
                        .get(cond.false_type)
                        .is_some_and(|n| n.kind == syntax_kind_ext::MAPPED_TYPE);
                    if false_is_mapped {
                        self.check_type_node(cond.false_type);
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
                        let name = ident.escaped_text.clone();
                        let atom = self.ctx.types.intern_string(&name);
                        let mut constraint_type = TypeId::UNKNOWN;
                        if tp_data.constraint != tsz_parser::parser::NodeIndex::NONE {
                            let resolved = self.get_type_from_type_node(tp_data.constraint);
                            if resolved != TypeId::ERROR {
                                constraint_type = resolved;
                            }
                        }
                        let provisional =
                            self.ctx
                                .types
                                .factory()
                                .type_param(tsz_solver::TypeParamInfo {
                                    name: atom,
                                    constraint: Some(constraint_type),
                                    default: None,
                                    is_const: false,
                                });
                        let previous = self
                            .ctx
                            .type_parameter_scope
                            .insert(name.clone(), provisional);
                        pushed_name = Some((name, previous));
                    }
                    if mapped.type_node != NodeIndex::NONE {
                        self.check_type_node(mapped.type_node);
                    }
                    // Also recurse into the name_type (the `as` clause) which may
                    // reference the mapped type parameter.
                    if mapped.name_type != NodeIndex::NONE {
                        self.check_type_node(mapped.name_type);
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
                    let elements = tuple.elements.nodes.clone();
                    for &element_idx in &elements {
                        self.check_type_node(element_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                // Force function/constructor type validation (TS2371 for parameter
                // initializers in type position, including binding element defaults).
                let _ = self.get_type_from_type_node(node_idx);

                // Clone before &mut self calls so the arena borrow is released.
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    let type_parameters = func_type.type_parameters.clone();
                    let param_nodes = func_type.parameters.nodes.clone();
                    let return_type = func_type.type_annotation;
                    let tp_updates = self.push_missing_name_type_parameters(&type_parameters);
                    // TS2370: Check that rest parameters have array types.
                    self.check_rest_parameter_types(&param_nodes);
                    for &param_idx in &param_nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                            && param.type_annotation != NodeIndex::NONE
                        {
                            self.check_type_node(param.type_annotation);
                        }
                    }
                    if return_type != NodeIndex::NONE {
                        self.check_type_node(return_type);
                    }
                    self.pop_type_parameters(tp_updates);
                }
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                // `typeof expr<Args>` — validate instantiation expression type args.
                if let Some(type_query) = self.ctx.arena.get_type_query(node)
                    && let Some(args) = &type_query.type_arguments
                {
                    let args_nodes = args.nodes.clone();
                    for &arg_idx in &args_nodes {
                        self.check_type_node(arg_idx);
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
                    let num_type_args = args_nodes.len();
                    self.check_instantiation_expression_type_args(
                        expr_type,
                        num_type_args,
                        node_idx,
                        &args_nodes,
                    );
                }
            }
            _ => {}
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
                let type_id = self.ctx.type_parameter_scope.get(&ident.escaped_text)?;
                crate::query_boundaries::checkers::generic::named_type_param_info(
                    self.ctx.types,
                    *type_id,
                )
            })
            .collect()
    }

    /// Walk a type node AST subtree to find `TYPE_QUERY` nodes (`typeof expr`)
    /// and pre-compute the flow-narrowed type of each expression.
    ///
    /// This is called during `check_type_alias_declaration` so that when the
    /// type alias body is later lowered by `ensure_type_alias_resolved`, the
    /// `TypeLowering` can use these pre-computed types instead of creating
    /// deferred `TypeQuery` types that would lose flow narrowing information.
    fn precompute_type_query_flow_types(&mut self, node_idx: NodeIndex) {
        if node_idx == NodeIndex::NONE {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::TYPE_QUERY {
            // Found a `typeof expr` in type position — compute the flow-narrowed
            // type of the expression and store it in node_types.
            if let Some(type_query) = self.ctx.arena.get_type_query(node) {
                let expr_name = type_query.expr_name;
                if expr_name != NodeIndex::NONE && !self.ctx.node_types.contains_key(&expr_name.0) {
                    let narrowed = self.get_type_of_identifier(expr_name);
                    if narrowed != TypeId::ERROR {
                        self.ctx.node_types.insert(expr_name.0, narrowed);
                    }
                }
            }
            return;
        }

        // Recurse into child type nodes to find nested TYPE_QUERY nodes
        match node.kind {
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        let Some(member) = self.ctx.arena.get(member_idx) else {
                            continue;
                        };
                        if let Some(sig) = self.ctx.arena.get_signature(member) {
                            if let Some(params) = &sig.parameters {
                                for &p in &params.nodes {
                                    if let Some(pn) = self.ctx.arena.get(p)
                                        && let Some(pd) = self.ctx.arena.get_parameter(pn)
                                    {
                                        self.precompute_type_query_flow_types(pd.type_annotation);
                                    }
                                }
                            }
                            self.precompute_type_query_flow_types(sig.type_annotation);
                        } else if let Some(prop) = self.ctx.arena.get_property_decl(member) {
                            self.precompute_type_query_flow_types(prop.type_annotation);
                        } else if let Some(idx_sig) = self.ctx.arena.get_index_signature(member) {
                            self.precompute_type_query_flow_types(idx_sig.type_annotation);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &child in &composite.types.nodes {
                        self.precompute_type_query_flow_types(child);
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.precompute_type_query_flow_types(arr.element_type);
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
                    for &elem in &tuple.elements.nodes {
                        self.precompute_type_query_flow_types(elem);
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.precompute_type_query_flow_types(wrapped.type_node);
                }
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.precompute_type_query_flow_types(indexed.object_type);
                    self.precompute_type_query_flow_types(indexed.index_type);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    self.precompute_type_query_flow_types(cond.check_type);
                    self.precompute_type_query_flow_types(cond.extends_type);
                    self.precompute_type_query_flow_types(cond.true_type);
                    self.precompute_type_query_flow_types(cond.false_type);
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                    self.precompute_type_query_flow_types(mapped.type_node);
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                    && let Some(args) = &type_ref.type_arguments
                {
                    for &arg in &args.nodes {
                        self.precompute_type_query_flow_types(arg);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    let param_nodes = func_type.parameters.nodes.clone();
                    let return_type = func_type.type_annotation;
                    for &param_idx in &param_nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                            && param.type_annotation != NodeIndex::NONE
                        {
                            self.precompute_type_query_flow_types(param.type_annotation);
                        }
                    }
                    if return_type != NodeIndex::NONE {
                        self.precompute_type_query_flow_types(return_type);
                    }
                }
            }
            _ => {}
        }
    }
}
