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
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn type_node_is_nested_in_type_literal(&self, node_idx: NodeIndex) -> bool {
        let mut current = self
            .ctx
            .arena
            .get_extended(node_idx)
            .map_or(NodeIndex::NONE, |info| info.parent);

        while !current.is_none() {
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

        let mut visited = rustc_hash::FxHashSet::default();
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
            pending.extend(tsz_solver::visitor::collect_lazy_def_ids(
                self.ctx.types,
                body,
            ));
        }

        false
    }

    /// Check a type alias declaration.
    pub(crate) fn check_type_alias_declaration(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };
        let Some(alias) = self.ctx.arena.get_type_alias(node) else {
            return;
        };

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

        self.check_variance_annotations_supported_for_type_alias(alias);

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

        self.check_type_node(alias.type_node);
        self.check_type_for_missing_names(alias.type_node);

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

    fn check_variance_annotations_supported_for_type_alias(
        &mut self,
        alias: &tsz_parser::parser::node::TypeAliasData,
    ) {
        let Some(type_params) = &alias.type_parameters else {
            return;
        };

        let first_variance_modifier = type_params.nodes.iter().copied().find_map(|param_idx| {
            let param_node = self.ctx.arena.get(param_idx)?;
            let param = self.ctx.arena.get_type_parameter(param_node)?;
            if self.node_contains_any_parse_error(param_idx)
                || matches!(
                    self.get_identifier_text_from_idx(param.name).as_deref(),
                    Some("in" | "out")
                )
            {
                return None;
            }
            let modifiers = param.modifiers.as_ref()?;
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
        });

        let Some(variance_modifier_idx) = first_variance_modifier else {
            return;
        };

        let body_kind = self.ctx.arena.get(alias.type_node).map(|n| n.kind);
        let variance_supported = body_kind.is_some_and(|kind| {
            kind == syntax_kind_ext::TYPE_LITERAL
                || kind == syntax_kind_ext::FUNCTION_TYPE
                || kind == syntax_kind_ext::CONSTRUCTOR_TYPE
                || kind == syntax_kind_ext::MAPPED_TYPE
        });

        if variance_supported {
            return;
        }

        self.error_at_node(
            variance_modifier_idx,
            crate::diagnostics::diagnostic_messages::VARIANCE_ANNOTATIONS_ARE_ONLY_SUPPORTED_IN_TYPE_ALIASES_FOR_OBJECT_FUNCTION_CONS,
            crate::diagnostics::diagnostic_codes::VARIANCE_ANNOTATIONS_ARE_ONLY_SUPPORTED_IN_TYPE_ALIASES_FOR_OBJECT_FUNCTION_CONS,
        );
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
                        if let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                            && accessor.type_annotation != NodeIndex::NONE
                        {
                            self.check_type_node(accessor.type_annotation);
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
                            self.check_type_node(cond.true_type);
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
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                // Force tuple element validation (TS1257, TS1265, TS1266)
                // which lives inside get_type_from_tuple_type.
                let _ = self.get_type_from_type_node(node_idx);
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                // Force function/constructor type validation (TS2371 for parameter
                // initializers in type position, including binding element defaults).
                let _ = self.get_type_from_type_node(node_idx);
            }
            _ => {}
        }
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
            _ => {}
        }
    }
}
