impl<'a> CheckerState<'a> {
    fn cache_resolved_symbol_type_for_owner(&self, sym_id: SymbolId, type_id: TypeId) {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };
        if symbol.decl_file_idx == u32::MAX {
            return;
        }
        if symbol.decl_file_idx as usize != self.ctx.current_file_idx {
            return;
        }

        self.ctx
            .cache_cross_file_symbol_type(sym_id, symbol.decl_file_idx, type_id, Vec::new());
    }

    fn can_register_evaluated_alias_form(
        &self,
        alias_def_id: tsz_solver::def::DefId,
        type_id: TypeId,
    ) -> bool {
        let mut pending =
            crate::query_boundaries::common::collect_lazy_def_ids(self.ctx.types, type_id);
        if pending.is_empty() {
            return true;
        }

        let mut visited = FxHashSet::default();
        let mut steps = 0usize;
        while let Some(def_id) = pending.pop() {
            if !visited.insert(def_id) {
                continue;
            }
            if def_id == alias_def_id {
                return false;
            }

            let Some(body) = self.ctx.definition_store.get_body(def_id) else {
                // Member body not set yet (e.g., forward-declared interface).
                // Skip instead of rejecting — we can still safely evaluate the
                // alias body; unresolved members will stay as Lazy and won't
                // cause incorrect registrations.  The evaluation machinery has
                // its own recursion limits to prevent infinite loops.
                continue;
            };

            steps += 1;
            if steps > 64 {
                return false;
            }

            pending.extend(crate::query_boundaries::common::collect_lazy_def_ids(
                self.ctx.types,
                body,
            ));
        }

        true
    }

    pub(super) fn type_parameter_default_syntactically_satisfies_constraint(
        &self,
        param_idx: NodeIndex,
    ) -> bool {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return false;
        };
        let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
            return false;
        };
        if param.default == NodeIndex::NONE || param.constraint == NodeIndex::NONE {
            return false;
        }
        self.type_node_syntactically_in_constraint(param.default, param.constraint, 0)
    }

    fn type_node_syntactically_in_constraint(
        &self,
        default_node: NodeIndex,
        constraint_node: NodeIndex,
        depth: u8,
    ) -> bool {
        if depth > 32 {
            return false;
        }
        let Some(node) = self.ctx.arena.get(constraint_node) else {
            return false;
        };
        if node.kind == syntax_kind_ext::UNION_TYPE
            && let Some(union) = self.ctx.arena.get_composite_type(node)
        {
            return union.types.nodes.iter().any(|&member| {
                self.type_nodes_syntactically_equal(default_node, member, depth + 1)
            });
        }
        self.type_nodes_syntactically_equal(default_node, constraint_node, depth + 1)
    }

    fn type_nodes_syntactically_equal(&self, left: NodeIndex, right: NodeIndex, depth: u8) -> bool {
        if depth > 32 || left == NodeIndex::NONE || right == NodeIndex::NONE {
            return false;
        }

        let left = self.unwrap_syntactic_type_node(left, depth);
        let right = self.unwrap_syntactic_type_node(right, depth);
        if left == right {
            return true;
        }

        let Some(left_node) = self.ctx.arena.get(left) else {
            return false;
        };
        let Some(right_node) = self.ctx.arena.get(right) else {
            return false;
        };
        if left_node.kind != right_node.kind {
            return false;
        }

        if let (Some(left_ident), Some(right_ident)) = (
            self.ctx.arena.get_identifier(left_node),
            self.ctx.arena.get_identifier(right_node),
        ) {
            return left_ident.escaped_text == right_ident.escaped_text;
        }

        if let (Some(left_name), Some(right_name)) = (
            self.ctx.arena.get_qualified_name(left_node),
            self.ctx.arena.get_qualified_name(right_node),
        ) {
            return self.type_nodes_syntactically_equal(left_name.left, right_name.left, depth + 1)
                && self.type_nodes_syntactically_equal(
                    left_name.right,
                    right_name.right,
                    depth + 1,
                );
        }

        if let (Some(left_ref), Some(right_ref)) = (
            self.ctx.arena.get_type_ref(left_node),
            self.ctx.arena.get_type_ref(right_node),
        ) {
            if !self.type_nodes_syntactically_equal(
                left_ref.type_name,
                right_ref.type_name,
                depth + 1,
            ) {
                return false;
            }
            return match (&left_ref.type_arguments, &right_ref.type_arguments) {
                (None, None) => true,
                (Some(left_args), Some(right_args)) => {
                    left_args.nodes.len() == right_args.nodes.len()
                        && left_args.nodes.iter().zip(right_args.nodes.iter()).all(
                            |(&left_arg, &right_arg)| {
                                self.type_nodes_syntactically_equal(left_arg, right_arg, depth + 1)
                            },
                        )
                }
                _ => false,
            };
        }

        if let (Some(left_wrapped), Some(right_wrapped)) = (
            self.ctx.arena.get_wrapped_type(left_node),
            self.ctx.arena.get_wrapped_type(right_node),
        ) {
            return self.type_nodes_syntactically_equal(
                left_wrapped.type_node,
                right_wrapped.type_node,
                depth + 1,
            );
        }

        if let (Some(left_literal_type), Some(right_literal_type)) = (
            self.ctx.arena.get_literal_type(left_node),
            self.ctx.arena.get_literal_type(right_node),
        ) {
            return self.type_nodes_syntactically_equal(
                left_literal_type.literal,
                right_literal_type.literal,
                depth + 1,
            );
        }

        if let (Some(left_literal), Some(right_literal)) = (
            self.ctx.arena.get_literal(left_node),
            self.ctx.arena.get_literal(right_node),
        ) {
            return left_literal.text == right_literal.text;
        }

        if let (Some(left_array), Some(right_array)) = (
            self.ctx.arena.get_array_type(left_node),
            self.ctx.arena.get_array_type(right_node),
        ) {
            return self.type_nodes_syntactically_equal(
                left_array.element_type,
                right_array.element_type,
                depth + 1,
            );
        }

        if let (Some(left_tuple), Some(right_tuple)) = (
            self.ctx.arena.get_tuple_type(left_node),
            self.ctx.arena.get_tuple_type(right_node),
        ) {
            return left_tuple.elements.nodes.len() == right_tuple.elements.nodes.len()
                && left_tuple
                    .elements
                    .nodes
                    .iter()
                    .zip(right_tuple.elements.nodes.iter())
                    .all(|(&left_elem, &right_elem)| {
                        self.type_nodes_syntactically_equal(left_elem, right_elem, depth + 1)
                    });
        }

        if let (Some(left_composite), Some(right_composite)) = (
            self.ctx.arena.get_composite_type(left_node),
            self.ctx.arena.get_composite_type(right_node),
        ) {
            return left_composite.types.nodes.len() == right_composite.types.nodes.len()
                && left_composite
                    .types
                    .nodes
                    .iter()
                    .zip(right_composite.types.nodes.iter())
                    .all(|(&left_type, &right_type)| {
                        self.type_nodes_syntactically_equal(left_type, right_type, depth + 1)
                    });
        }

        matches!(
            left_node.kind,
            k if k == SyntaxKind::AnyKeyword as u16
                || k == SyntaxKind::UnknownKeyword as u16
                || k == SyntaxKind::NeverKeyword as u16
                || k == SyntaxKind::StringKeyword as u16
                || k == SyntaxKind::NumberKeyword as u16
                || k == SyntaxKind::BooleanKeyword as u16
                || k == SyntaxKind::BigIntKeyword as u16
                || k == SyntaxKind::SymbolKeyword as u16
                || k == SyntaxKind::VoidKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16
                || k == SyntaxKind::ObjectKeyword as u16
                || k == SyntaxKind::ThisKeyword as u16
        )
    }

    fn unwrap_syntactic_type_node(&self, mut node_idx: NodeIndex, mut depth: u8) -> NodeIndex {
        while depth <= 32 {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                return node_idx;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
                && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
            {
                node_idx = wrapped.type_node;
                depth += 1;
                continue;
            }
            return node_idx;
        }
        node_idx
    }

    fn maybe_push_enclosing_type_parameters(
        &mut self,
        type_parameters: &tsz_parser::parser::NodeList,
    ) -> Vec<(String, Option<TypeId>, bool)> {
        let Some(&first_param_idx) = type_parameters.nodes.first() else {
            return Vec::new();
        };

        let mut current = self
            .ctx
            .arena
            .get_extended(first_param_idx)
            .map_or(NodeIndex::NONE, |ext| ext.parent);

        let mut depth = 0;
        while current.is_some() && depth < 64 {
            depth += 1;
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
            if !current.is_some() {
                break;
            }

            let maybe_enclosing_type_params =
                self.ctx
                    .arena
                    .get(current)
                    .and_then(|parent| match parent.kind {
                        k if k == syntax_kind_ext::INTERFACE_DECLARATION => self
                            .ctx
                            .arena
                            .get_interface(parent)
                            .and_then(|iface| iface.type_parameters.clone()),
                        k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                            .ctx
                            .arena
                            .get_type_alias(parent)
                            .and_then(|type_alias| type_alias.type_parameters.clone()),
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION
                            || k == syntax_kind_ext::FUNCTION_EXPRESSION
                            || k == syntax_kind_ext::ARROW_FUNCTION =>
                        {
                            self.ctx
                                .arena
                                .get_function(parent)
                                .and_then(|func| func.type_parameters.clone())
                        }
                        k if k == syntax_kind_ext::METHOD_DECLARATION => self
                            .ctx
                            .arena
                            .get_method_decl(parent)
                            .and_then(|method| method.type_parameters.clone()),
                        k if k == syntax_kind_ext::METHOD_SIGNATURE
                            || k == syntax_kind_ext::CALL_SIGNATURE
                            || k == syntax_kind_ext::CONSTRUCT_SIGNATURE =>
                        {
                            self.ctx
                                .arena
                                .get_signature(parent)
                                .and_then(|sig| sig.type_parameters.clone())
                        }
                        k if k == syntax_kind_ext::FUNCTION_TYPE
                            || k == syntax_kind_ext::CONSTRUCTOR_TYPE =>
                        {
                            self.ctx
                                .arena
                                .get_function_type(parent)
                                .and_then(|func| func.type_parameters.clone())
                        }
                        _ => None,
                    });

            let Some(enclosing_type_params) = maybe_enclosing_type_params else {
                continue;
            };

            let mut any_missing = false;
            let mut any_present = false;
            for &param_idx in &enclosing_type_params.nodes {
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
                if self
                    .ctx
                    .type_parameter_scope
                    .contains_key(ident.escaped_text.as_str())
                {
                    any_present = true;
                } else {
                    any_missing = true;
                }
            }

            if any_missing && !any_present {
                let (_, updates) = self.push_type_parameters(&Some(enclosing_type_params));
                return updates;
            }
        }

        Vec::new()
    }

    /// Push type parameters from enclosing generic functions/methods for a given
    /// declaration node. Used when computing local type aliases that have no own
    /// type parameters but reference type parameters from an enclosing function.
    ///
    /// For example: `function foo<T>() { type X = T extends string ? T : never; }`
    /// When computing `X`, `T` must be in the type parameter scope.
    pub(crate) fn push_enclosing_type_params_for_node(
        &mut self,
        arena: &tsz_parser::parser::node::NodeArena,
        node_idx: tsz_parser::parser::NodeIndex,
    ) -> Vec<(String, Option<TypeId>, bool)> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = arena
            .get_extended(node_idx)
            .map_or(tsz_parser::parser::NodeIndex::NONE, |ext| ext.parent);

        let mut all_updates = Vec::new();
        let mut depth = 0;
        while current.is_some() && depth < 64 {
            depth += 1;
            let Some(parent) = arena.get(current) else {
                break;
            };

            let maybe_type_params = match parent.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    arena
                        .get_function(parent)
                        .and_then(|func| func.type_parameters.clone())
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => arena
                    .get_method_decl(parent)
                    .and_then(|method| method.type_parameters.clone()),
                k if k == syntax_kind_ext::METHOD_SIGNATURE
                    || k == syntax_kind_ext::CALL_SIGNATURE
                    || k == syntax_kind_ext::CONSTRUCT_SIGNATURE =>
                {
                    arena
                        .get_signature(parent)
                        .and_then(|sig| sig.type_parameters.clone())
                }
                _ => None,
            };

            if let Some(type_params) = maybe_type_params {
                // Only push if these type params are from the SAME arena as we're using
                // and none of them are already in scope.
                let all_missing = type_params.nodes.iter().all(|&param_idx| {
                    arena
                        .get(param_idx)
                        .and_then(|n| arena.get_type_parameter(n))
                        .and_then(|tp| arena.get(tp.name))
                        .and_then(|n| arena.get_identifier(n))
                        .is_none_or(|ident| {
                            !self
                                .ctx
                                .type_parameter_scope
                                .contains_key(ident.escaped_text.as_str())
                        })
                });
                if all_missing && std::ptr::eq(arena, self.ctx.arena) {
                    let (_, updates) = self.push_type_parameters(&Some(type_params));
                    all_updates.extend(updates);
                }
            }

            current = arena
                .get_extended(current)
                .map_or(tsz_parser::parser::NodeIndex::NONE, |ext| ext.parent);
        }

        all_updates
    }

    /// Get type from a union type node (A | B).
    ///
    /// Parses a union type expression and creates a Union type with all members.
    ///
    /// ## Type Normalization:
    /// - Empty union → NEVER (the empty type)
    /// - Single member → the member itself (no union wrapper)
    /// - Multiple members → Union type with all members
    ///
    /// ## Member Resolution:
    /// - Each member is resolved via `get_type_from_type_node`
    /// - This handles nested typeof expressions and type references
    /// - Type arguments are recursively resolved
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// type StringOrNumber = string | number;
    /// // Creates Union(STRING, NUMBER)
    ///
    /// type ThreeTypes = string | number | boolean;
    /// // Creates Union(STRING, NUMBER, BOOLEAN)
    ///
    /// type Nested = (string | number) | boolean;
    /// // Normalized to Union(STRING, NUMBER, BOOLEAN)
    /// ```
    #[allow(dead_code)]
    /// Decide whether a `typeof <name>` query is positioned in the signature
    /// (type-annotation) region of a function and the name resolves only
    /// because of body-local hoisting. tsc treats such references as
    /// unresolved — the signature scope is logically outside the body, even
    /// though we bind parameters, type parameters, and body `var`/function
    /// declarations into a single function scope.
    ///
    /// Returns true when:
    ///   * `idx` is inside a function/method/arrow and its enclosing chain
    ///     stays inside the function's **signature** (i.e. we reach the
    ///     function's `body` edge, or the `type`/parameter/return-type edge,
    ///     before reaching the function itself), AND
    ///   * `name` resolves to a symbol whose declaration is inside that
    ///     function's body.
    pub(super) fn is_typeof_in_function_signature_of_body_local(
        &self,
        idx: NodeIndex,
        name: &str,
    ) -> bool {
        // Walk up to find the enclosing function-like node, tracking whether we
        // ever entered its body subtree. If we entered the body, this typeof is
        // inside the body — body-scope visibility is fine there.
        let mut current = idx;
        let mut enclosing_fn: Option<NodeIndex> = None;
        let mut saw_body = false;
        let mut entered_from: NodeIndex = idx;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };
            match parent_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR =>
                {
                    if let Some(func) = self.ctx.arena.get_function(parent_node)
                        && func.body == entered_from
                    {
                        saw_body = true;
                    }
                    enclosing_fn = Some(parent);
                    break;
                }
                _ => {}
            }
            entered_from = parent;
            current = parent;
        }

        let Some(fn_idx) = enclosing_fn else {
            return false;
        };
        if saw_body {
            return false;
        }

        let Some(fn_node) = self.ctx.arena.get(fn_idx) else {
            return false;
        };
        let Some(func) = self.ctx.arena.get_function(fn_node) else {
            return false;
        };
        if func.body.is_none() {
            return false;
        }
        let Some(body_node) = self.ctx.arena.get(func.body) else {
            return false;
        };
        let (body_pos, body_end) = (body_node.pos, body_node.end);

        // Ask every scope whose container is lexically inside the function
        // body: does it declare `name`? If yes, the symbol is body-only and
        // tsc treats the signature-position `typeof name` as unresolved.
        // We intentionally don't call `resolve_identifier` here — that resolver
        // sees body-hoisted vars from the function scope and would always
        // succeed, hiding the signature/body boundary we're trying to recover.
        for scope in self.ctx.binder.scopes.iter() {
            let Some(cnode) = self.ctx.arena.get(scope.container_node) else {
                continue;
            };
            if cnode.pos < body_pos || cnode.end > body_end {
                continue;
            }
            if scope.table.get(name).is_some() {
                return true;
            }
        }
        false
    }

    pub(crate) fn is_type_query_in_non_flow_sensitive_signature_parameter(
        &self,
        idx: NodeIndex,
    ) -> bool {
        crate::types_domain::type_node_helpers::is_type_query_in_non_flow_sensitive_signature_parameter(
            self.ctx.arena,
            idx,
        )
    }

    /// Get type from a type query node (typeof X).
    ///
    /// Resolves value symbols, emits TS2504 for type-only symbols, handles
    /// unknown identifiers and missing members. Supports type arguments.
    ///
    /// Resolve a qualified name chain as a value property access chain
    /// for `typeof` context. Recurses through nested `QualifiedName` nodes
    /// so that `typeof a.b.c` resolves `a` as a value, then `.b`, then `.c`.
    #[allow(dead_code)]
    pub(crate) fn resolve_typeof_qualified_value_chain(
        &mut self,
        idx: NodeIndex,
        use_flow: bool,
    ) -> TypeId {
        self.resolve_typeof_qualified_value_chain_with_request(idx, &TypingRequest::NONE, use_flow)
    }

    pub(crate) fn resolve_typeof_qualified_value_chain_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
        use_flow: bool,
    ) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
                return TypeId::ERROR;
            };
            let left_type =
                self.resolve_typeof_qualified_value_chain_with_request(qn.left, request, use_flow);
            if let Some(rn) = self.ctx.arena.get(qn.right)
                && let Some(ident) = self.ctx.arena.get_identifier(rn)
            {
                if let Some(global_like_type) = self.resolve_global_like_typeof_member_access(
                    qn.left,
                    &ident.escaped_text,
                    qn.right,
                ) {
                    return if use_flow {
                        self.apply_flow_narrowing(idx, global_like_type)
                    } else {
                        global_like_type
                    };
                }
                if left_type == TypeId::ANY || left_type == TypeId::ERROR {
                    return left_type;
                }
                let object_type = self.resolve_type_for_property_access(left_type);
                if object_type == TypeId::ANY || object_type == TypeId::ERROR {
                    return object_type;
                }
                let (object_type_for_access, nullish_cause) = self.split_nullish_type(object_type);
                let Some(object_type_for_access) = object_type_for_access else {
                    if let Some(cause) = nullish_cause {
                        self.report_nullish_object(qn.left, cause, true);
                    }
                    return TypeId::ERROR;
                };
                if let Some(cause) = nullish_cause {
                    self.report_nullish_object(qn.left, cause, false);
                }
                use crate::query_boundaries::common::PropertyAccessResult;
                match self
                    .resolve_property_access_with_env(object_type_for_access, &ident.escaped_text)
                {
                    PropertyAccessResult::Success { type_id, .. } => {
                        let resolved = self.resolve_type_query_type(type_id);
                        if use_flow {
                            self.apply_flow_narrowing(idx, resolved)
                        } else {
                            resolved
                        }
                    }
                    _ => TypeId::ERROR,
                }
            } else {
                TypeId::ERROR
            }
        } else {
            // Base case: identifier or other expression — resolve as value
            let expr_request = if use_flow {
                request.read().contextual_opt(None)
            } else {
                request.write().contextual_opt(None)
            };
            self.get_type_of_node_with_request(idx, &expr_request)
        }
    }

    pub(super) fn resolve_global_like_typeof_member_access(
        &mut self,
        left_idx: NodeIndex,
        member_name: &str,
        member_node: NodeIndex,
    ) -> Option<TypeId> {
        let is_this_global = self.is_this_resolving_to_global(left_idx);
        if !(self.is_global_this_like_expression(left_idx) || is_this_global) {
            return None;
        }

        let base_display = if self.is_global_this_expression(left_idx) || is_this_global {
            "typeof globalThis"
        } else {
            "Window & typeof globalThis"
        };
        let allow_unknown_property_fallback =
            self.is_global_this_expression(left_idx) || is_this_global;
        let property_type = self.resolve_global_this_property_type(
            member_name,
            member_node,
            allow_unknown_property_fallback,
            base_display,
        );
        if property_type == TypeId::ERROR {
            return Some(TypeId::ERROR);
        }

        let access_targets_global_this = is_this_global || self.is_global_this_expression(left_idx);
        if access_targets_global_this
            && property_type == TypeId::ANY
            && self.ctx.no_implicit_any()
            && !self.is_js_file()
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            self.error_at_node(
                member_node,
                &format_message(
                    diagnostic_messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE,
                    &["typeof globalThis"],
                ),
                diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_TYPE_HAS_NO_INDEX_SIGNATURE,
            );
        }

        Some(self.resolve_type_query_type(property_type))
    }

    #[allow(dead_code)]
    pub(super) fn resolve_type_query_import_type_symbol(&self, idx: NodeIndex) -> Option<u32> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let local_sym_id = self.resolve_identifier_symbol(idx)?;
        if !self.alias_resolves_to_type_only(local_sym_id) {
            return None;
        }

        match self.resolve_identifier_symbol_in_type_position_without_tracking(idx) {
            TypeSymbolResolution::Type(sym_id) | TypeSymbolResolution::ValueOnly(sym_id) => {
                Some(sym_id.0)
            }
            TypeSymbolResolution::NotFound => Some(local_sym_id.0),
        }
    }

    /// Push type parameters into scope for generic type resolution.
    ///
    /// This is a critical function for handling generic types (classes, interfaces,
    /// functions, type aliases). It makes type parameters available for use within
    /// the generic type's body and returns information for later scope restoration.
    ///
    /// ## Two-Pass Algorithm:
    /// 1. **First pass**: Adds all type parameters to scope WITHOUT constraints
    ///    - This allows self-referential constraints like `T extends Box<T>`
    ///    - Creates unconstrained `TypeParameter` entries
    /// 2. **Second pass**: Resolves constraints and defaults with all params in scope
    ///    - Now all type parameters are visible for constraint resolution
    ///    - Updates the scope with constrained `TypeParameter` entries
    ///
    /// ## Returns:
    /// - `Vec<TypeParamInfo>`: Type parameter info with constraints and defaults
    /// - `Vec<(String, Option<TypeId>)>`: Restoration data for `pop_type_parameters`
    ///
    /// ## Constraint Validation:
    /// - Emits TS2315 if constraint type is error
    /// - Emits TS2314 if default doesn't satisfy constraint
    /// - Uses UNKNOWN for invalid constraints
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Simple type parameter
    /// function identity<T>(value: T): T { return value; }
    /// // push_type_parameters adds T to scope
    ///
    /// // Type parameter with constraint
    /// interface Comparable<T> {
    ///   compare(other: T): number;
    /// }
    /// function max<T extends Comparable<T>>(a: T, b: T): T {
    ///   // T is in scope with constraint Comparable<T>
    ///   return a.compare(b) > 0 ? a : b;
    /// }
    ///
    /// // Type parameter with default
    /// interface Box<T = string> {
    ///   value: T;
    /// }
    /// // T has default type string
    ///
    /// // Self-referential constraint (requires two-pass algorithm)
    /// type Box<T extends Box<T>> = T;
    /// // First pass: T added to scope unconstrained
    /// // Second pass: Constraint Box<T> resolved (T now in scope)
    ///
    /// // Multiple type parameters
    /// interface Map<K, V> {
    ///   get(key: K): V | undefined;
    ///   set(key: K, value: V): void;
    /// }
    /// ```
    pub(crate) fn push_type_parameters(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) -> TypeParamPushResult {
        let Some(list) = type_parameters else {
            return (Vec::new(), Vec::new());
        };

        // Recursion depth check: prevent stack overflow from circular type parameter
        // references (e.g. interface I<T extends I<T>> {} or circular generic defaults)
        if !self.ctx.enter_recursion() {
            return (Vec::new(), Vec::new());
        }

        let mut updates = self.maybe_push_enclosing_type_parameters(list);
        let mut params = Vec::new();
        let mut param_indices = Vec::new();
        let mut seen_names = FxHashSet::default();

        // First pass: Add all type parameters to scope WITHOUT resolving constraints
        // This allows self-referential constraints like T extends Box<T>
        for &param_idx in &list.nodes {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };

            let name = self
                .ctx
                .arena
                .get(data.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map_or_else(|| "T".to_string(), |id_data| id_data.escaped_text.clone());

            // Check for duplicate type parameter names (TS2300)
            if !seen_names.insert(name.clone()) {
                self.error_at_node_msg(
                    data.name,
                    crate::diagnostics::diagnostic_codes::DUPLICATE_IDENTIFIER,
                    &[&name],
                );
            }

            // Check for reserved type names (TS2368)
            self.check_type_name_is_reserved(data.name, &name);

            let atom = self.ctx.types.intern_string(&name);

            // Create unconstrained type parameter initially
            let info = tsz_solver::TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
                is_const: false,
            };
            let mut shadowed_class_param = false;
            if let Some(ref mut c) = self.ctx.enclosing_class
                && let Some(pos) = c.type_param_names.iter().position(|x| *x == name)
            {
                c.type_param_names.remove(pos);
                shadowed_class_param = true;
            }

            let type_id = self.intern_type_param_for_decl(data.name, info);
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous, shadowed_class_param));
            param_indices.push(param_idx);
        }

        // Second pass: iteratively refine constraints/defaults against the evolving scope.
        // A single forward pass leaves transitive chains like `T extends U, U extends V`
        // pointing at the original unconstrained placeholders. Re-resolving until the
        // scope stabilizes preserves the full local constraint graph.
        let max_refinement_passes = param_indices.len().max(1);
        for _ in 0..max_refinement_passes {
            let mut changed = false;
            let mut next_params = Vec::with_capacity(param_indices.len());

            for &param_idx in &param_indices {
                let Some(node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                    continue;
                };

                let name = self
                    .ctx
                    .arena
                    .get(data.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .map_or_else(|| "T".to_string(), |id_data| id_data.escaped_text.clone());
                let atom = self.ctx.types.intern_string(&name);

                let constraint = if data.constraint != NodeIndex::NONE {
                    let constraint_type = self.get_type_from_type_node(data.constraint);
                    let is_typeof_constraint =
                        self.ctx.arena.get(data.constraint).is_some_and(|n| {
                            n.kind == tsz_parser::parser::syntax_kind_ext::TYPE_QUERY
                        });
                    let is_direct_mapped_constraint =
                        self.ctx.arena.get(data.constraint).is_some_and(|n| {
                            n.kind == tsz_parser::parser::syntax_kind_ext::MAPPED_TYPE
                        });
                    let is_direct_resolution_path_constraint =
                        self.ctx.arena.get(data.constraint).is_some_and(|n| {
                            matches!(
                                n.kind,
                                k if k == tsz_parser::parser::syntax_kind_ext::MAPPED_TYPE
                                    || k == tsz_parser::parser::syntax_kind_ext::UNION_TYPE
                                    || k == tsz_parser::parser::syntax_kind_ext::INTERSECTION_TYPE
                                    || k == tsz_parser::parser::syntax_kind_ext::INDEXED_ACCESS_TYPE
                            )
                        });
                    // Skip circular constraint check for `typeof` expressions.
                    // `T extends typeof a` where `a: T` resolves to `T extends T` but is
                    // NOT considered circular by tsc — it's a valid pattern for type narrowing.
                    // tsc's getConstraintOfTypeParameter defers typeof resolution.
                    let is_circular = !is_typeof_constraint
                        && if let Some(&param_type_id) = self.ctx.type_parameter_scope.get(&name) {
                            self.is_same_type_parameter(
                                constraint_type,
                                param_type_id,
                                &name,
                                is_direct_mapped_constraint,
                                is_direct_resolution_path_constraint,
                            )
                        } else {
                            false
                        };

                    if is_circular {
                        self.error_at_node_msg(
                            data.constraint,
                            crate::diagnostics::diagnostic_codes::TYPE_PARAMETER_HAS_A_CIRCULAR_CONSTRAINT,
                            &[&name],
                        );
                        Some(TypeId::UNKNOWN)
                    } else {
                        self.ensure_application_symbols_resolved(constraint_type);
                        Some(constraint_type)
                    }
                } else {
                    None
                };

                let default = if data.default != NodeIndex::NONE {
                    let default_type = self.get_type_from_type_node(data.default);
                    self.ensure_application_symbols_resolved(default_type);
                    (default_type != TypeId::ERROR).then_some(default_type)
                } else {
                    None
                };

                let is_const = self
                    .ctx
                    .arena
                    .has_modifier(&data.modifiers, tsz_scanner::SyntaxKind::ConstKeyword);
                let info = tsz_solver::TypeParamInfo {
                    name: atom,
                    constraint,
                    default,
                    is_const,
                };

                let constrained_type_id = self.intern_type_param_for_decl(data.name, info);
                if self.ctx.type_parameter_scope.get(&name).copied() != Some(constrained_type_id) {
                    self.ctx
                        .type_parameter_scope
                        .insert(name.clone(), constrained_type_id);
                    changed = true;
                }
                next_params.push(info);
            }

            params = next_params;
            if !changed {
                break;
            }
        }

        // Third pass: Detect indirect circular constraints (e.g., T extends U, U extends T)
        // Build a constraint graph among type parameters in this list and detect cycles.
        self.check_indirect_circular_constraints(&params, &param_indices);

        self.validate_type_parameter_defaults_against_constraints(&param_indices, &params);

        self.ctx.leave_recursion();
        (params, updates)
    }

    /// Allocate (or reuse) the canonical `TypeId` for one type-parameter
    /// declaration's `TypeParamInfo`.
    ///
    /// Two processings of the same declaration (e.g. `function f<T>` whose
    /// signature is computed once for parameter resolution and once for an
    /// annotation context) must converge on a single `TypeId`. Without
    /// this, `fresh_type_param` mints distinct non-deduped ids each time
    /// and every downstream interner table for types closing over `T`
    /// hashes to a different entry, defeating identity-based fast paths
    /// in the relation engine and producing spurious `TS2859`s on
    /// recursive aliases (`Recur<T>` vs `Recur<T> | undefined`).
    ///
    /// The reuse is guarded on full `TypeParamInfo` equality so the
    /// refinement pass can install a constrained variant when the user
    /// wrote `T extends C`.
    ///
    /// For type parameters that have no `DefId` registration (class, method,
    /// and interface type parameters are not emitted into `semantic_defs` by
    /// the binder), a secondary node-keyed cache (`type_param_node_cache`) is
    /// consulted. Without it, `get_class_instance_type_inner` and the outer
    /// `check_class_declaration` each call `push_type_parameters` independently,
    /// producing different `TypeIds` for the same `T`. That discrepancy causes
    /// `MappedType.constraint = KeyOf(T_id_instance)` to differ from
    /// `K.constraint = KeyOf(T_id_check)`, silently defeating
    /// `type_param_constraint_matches` in the solver's `visit_mapped` and
    /// producing a false TS2349 on `this.map[key]()` patterns.
    fn intern_type_param_for_decl(
        &mut self,
        name_node: tsz_parser::parser::NodeIndex,
        info: tsz_solver::TypeParamInfo,
    ) -> tsz_solver::TypeId {
        let registered_def = self
            .ctx
            .binder
            .node_symbols
            .get(&name_node.0)
            .and_then(|&sym_id| self.ctx.definition_store.find_def_by_symbol(sym_id.0));

        let cached = registered_def.and_then(|def_id| {
            let cached_id = self.ctx.definition_store.find_type_param_for_def(def_id)?;
            if type_param_info(self.ctx.types, cached_id) == Some(info) {
                Some(cached_id)
            } else {
                None
            }
        });

        // Fallback: for type params with no DefId (class/method/interface type params
        // omitted from semantic_defs), use the node-keyed cache so repeated
        // push_type_parameters calls for the same declaration converge on the same TypeId.
        let cached = cached.or_else(|| {
            if registered_def.is_none() {
                self.ctx
                    .type_param_node_cache
                    .get(&(name_node.0, info))
                    .copied()
            } else {
                None
            }
        });

        let type_id = cached.unwrap_or_else(|| self.ctx.types.fresh_type_param(info));

        if let Some(def_id) = registered_def {
            self.ctx
                .definition_store
                .register_type_to_def(type_id, def_id);
            self.ctx
                .definition_store
                .register_type_param_for_def(def_id, type_id);
        } else {
            // Keep the node-keyed cache up to date so the next push_type_parameters
            // call for this same declaration returns the same TypeId.
            self.ctx
                .type_param_node_cache
                .insert((name_node.0, info), type_id);
        }

        type_id
    }

    pub(super) fn empty_type_literal_satisfies_optional_mapped_constraint(
        &mut self,
        param_idx: NodeIndex,
        constraint_type: TypeId,
    ) -> bool {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return false;
        };
        let Some(param_data) = self.ctx.arena.get_type_parameter(param_node) else {
            return false;
        };
        let default_node = param_data.default;
        if !self.is_empty_type_literal_node(default_node) {
            return false;
        }

        if self.constraint_node_is_partial_object(param_data.constraint) {
            return true;
        }

        let Some((base, args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, constraint_type)
        else {
            return false;
        };
        if args.len() != 1 {
            return false;
        }
        let Some(&arg) = args.first() else {
            return false;
        };

        let base = self.resolve_lazy_type(base);
        let Some(mapped) = crate::query_boundaries::common::mapped_type_info(
            self.ctx.types.as_type_database(),
            base,
        ) else {
            return false;
        };
        if !matches!(
            mapped.optional_modifier,
            Some(tsz_solver::MappedModifier::Add)
        ) {
            return false;
        }

        self.is_object_like_for_optional_mapped_type(arg)
    }

    fn constraint_node_is_partial_object(&mut self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(paren) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.constraint_node_is_partial_object(paren.type_node);
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(type_ref.type_name) else {
            return false;
        };
        let Some(identifier) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        if identifier.escaped_text != "Partial" {
            return false;
        }
        let TypeSymbolResolution::Type(partial_sym) =
            self.resolve_identifier_symbol_in_type_position_without_tracking(type_ref.type_name)
        else {
            return false;
        };
        if !self.ctx.symbol_is_from_actual_or_cloned_lib(partial_sym) {
            return false;
        }
        let Some(type_args) = &type_ref.type_arguments else {
            return false;
        };
        if type_args.nodes.len() != 1 {
            return false;
        }
        let Some(&arg_node) = type_args.nodes.first() else {
            return false;
        };
        let arg_type = self.get_type_from_type_node(arg_node);
        self.is_object_like_for_optional_mapped_type(arg_type)
    }

    fn is_empty_type_literal_node(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(paren) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.is_empty_type_literal_node(paren.type_node);
        }
        node.kind == syntax_kind_ext::TYPE_LITERAL
            && self
                .ctx
                .arena
                .get_type_literal(node)
                .is_some_and(|type_lit| type_lit.members.nodes.is_empty())
    }

    fn is_object_like_for_optional_mapped_type(&mut self, type_id: TypeId) -> bool {
        let resolved = self.resolve_lazy_type(type_id);
        if resolved == TypeId::OBJECT {
            return true;
        }

        crate::query_boundaries::common::is_object_like_type(
            self.ctx.types.as_type_database(),
            resolved,
        )
    }

    /// Detect indirect circular constraints among type parameters.
    ///
    /// For each type parameter, if its constraint is another type parameter in the same
    /// list, follow the chain. If we reach the original parameter, emit TS2313.
    /// Direct self-references (T extends T) are already caught in the second pass.
    /// Get type of a symbol with caching and circular reference detection.
    ///
    /// This is the main entry point for resolving the type of symbols (variables,
    /// functions, classes, interfaces, type aliases, etc.). All type resolution
    /// ultimately flows through this function.
    ///
    /// ## Caching:
    /// - Symbol types are cached in `ctx.symbol_types` by symbol ID
    /// - Subsequent calls for the same symbol return the cached type
    /// - Cache is populated on first successful resolution
    ///
    /// ## Fuel Management:
    /// - Consumes fuel on each call to prevent infinite loops
    /// - Returns ERROR if fuel is exhausted (prevents type checker timeout)
    ///
    /// ## Circular Reference Detection:
    /// - Tracks currently resolving symbols in `ctx.symbol_resolution_set`
    /// - Returns ERROR if a circular reference is detected
    /// - Uses a stack to track resolution depth
    ///
    /// ## Type Environment Population:
    /// - After resolution, populates the type environment for generic type expansion
    /// - For classes: Handles instance type with type parameters specially
    /// - For generic types: Stores both the type and its type parameters
    /// - Skips ANY/ERROR types (don't populate environment for errors)
    ///
    /// ## Symbol Dependency Tracking:
    /// - Records symbol dependencies for incremental type checking
    /// - Pushes/pops from dependency stack during resolution
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// let x = 42;              // get_type_of_symbol(x) → number
    /// function foo(): void {}  // get_type_of_symbol(foo) → () => void
    /// class C {}               // get_type_of_symbol(C) → typeof C (constructor)
    /// interface I {}           // get_type_of_symbol(I) → I (interface type)
    /// type T = string;         // get_type_of_symbol(T) → string
    /// ```
    pub fn get_type_of_symbol(&mut self, sym_id: SymbolId) -> TypeId {
        // Hard stack guard: bail with ERROR when the stack overflow breaker
        // has been tripped by a previous deep recursion.
        if crate::checkers_domain::stack_overflow_tripped() {
            self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            return TypeId::ERROR;
        }
        // Periodically probe remaining stack and trip the breaker if low.
        if crate::checkers_domain::should_probe_stack()
            && stacker::remaining_stack().unwrap_or(0) < 1024 * 1024
        {
            crate::checkers_domain::trip_stack_overflow();
            self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
            return TypeId::ERROR;
        }
        // Dynamically grow the stack for deeply recursive symbol resolution
        // chains.
        stacker::maybe_grow(256 * 1024, 2 * 1024 * 1024, || {
            self.get_type_of_symbol_inner(sym_id)
        })
    }
}
