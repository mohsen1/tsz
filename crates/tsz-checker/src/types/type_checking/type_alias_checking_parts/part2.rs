impl<'a> CheckerState<'a> {
    fn check_type_node_with_literal_context(
        &mut self,
        node_idx: NodeIndex,
        nested_in_type_literal: bool,
    ) {
        if node_idx == NodeIndex::NONE {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };
        let child_nested_in_type_literal =
            nested_in_type_literal || node.kind == syntax_kind_ext::TYPE_LITERAL;
        macro_rules! check_child_type_node {
            ($checker:expr, $child:expr) => {
                $checker.check_type_node_with_literal_context($child, child_nested_in_type_literal)
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
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                    && let Some(type_arguments) = &type_ref.type_arguments
                {
                    for &arg_idx in &type_arguments.nodes {
                        check_child_type_node!(self, arg_idx);
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
                if !self.check_explicit_type_reference_for_alias_body_validation(node_idx) {
                    let _ = if nested_in_type_literal {
                        self.get_type_from_type_node_in_type_literal(node_idx)
                    } else {
                        self.get_type_from_type_node(node_idx)
                    };
                }
                self.check_styled_component_inner_component_constraint(node_idx);
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
                            let (_type_params, type_param_updates) =
                                self.push_type_parameters(&sig.type_parameters);
                            if let Some(params) = &sig.parameters {
                                for &param_idx in &params.nodes {
                                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                                        && let Some(param) =
                                            self.ctx.arena.get_parameter(param_node)
                                        && param.type_annotation != NodeIndex::NONE
                                    {
                                        check_child_type_node!(self, param.type_annotation);
                                    }
                                }
                            }
                            if sig.type_annotation != NodeIndex::NONE {
                                check_child_type_node!(self, sig.type_annotation);
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
                        if let Some(prop) = self.ctx.arena.get_property_decl(member_node)
                            && prop.type_annotation != NodeIndex::NONE
                        {
                            check_child_type_node!(self, prop.type_annotation);
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
                            check_child_type_node!(self, cond.true_type);
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
                        check_child_type_node!(self, mapped.type_node);
                    }
                    // Also recurse into the name_type (the `as` clause) which may
                    // reference the mapped type parameter.
                    if mapped.name_type != NodeIndex::NONE {
                        check_child_type_node!(self, mapped.name_type);
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
                    let type_parameters = func_type.type_parameters.clone();
                    let parameters = func_type.parameters.nodes.clone();
                    let type_annotation = func_type.type_annotation;
                    let (_type_params, tp_updates) = self.push_type_parameters(&type_parameters);
                    for &param_idx in &parameters {
                        let param_type_annotation = (|| {
                            let param_node = self.ctx.arena.get(param_idx)?;
                            let param = self.ctx.arena.get_parameter(param_node)?;
                            param
                                .type_annotation
                                .is_some()
                                .then_some(param.type_annotation)
                        })();
                        if let Some(param_type_annotation) = param_type_annotation {
                            check_child_type_node!(self, param_type_annotation);
                        }
                    }
                    if type_annotation.is_some() {
                        check_child_type_node!(self, type_annotation);
                    }
                    self.check_rest_parameter_types(&parameters);
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
            _ => {}
        }
    }
}
