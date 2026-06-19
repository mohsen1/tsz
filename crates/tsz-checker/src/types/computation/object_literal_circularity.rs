use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{CallSignature, CallableShape, PropertyInfo, TypeId, Visibility};

impl<'a> CheckerState<'a> {
    pub(super) fn object_literal_callable_member_names(
        &mut self,
        elements: &[NodeIndex],
    ) -> FxHashMap<Atom, (NodeIndex, u32)> {
        elements
            .iter()
            .enumerate()
            .filter_map(|(pos, &elem_idx)| {
                let elem_node = self.ctx.arena.get(elem_idx)?;

                if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
                    let name = self.get_property_name(method.name)?;
                    return Some((
                        self.ctx.types.intern_string(&name),
                        (elem_idx, (pos + 1) as u32),
                    ));
                }

                let prop = self.ctx.arena.get_property_assignment(elem_node)?;
                let initializer = self
                    .ctx
                    .arena
                    .skip_parenthesized_and_assertions(prop.initializer);
                let init_node = self.ctx.arena.get(initializer)?;
                if !matches!(
                    init_node.kind,
                    syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
                ) {
                    return None;
                }
                let name = self.get_property_name_resolved(prop.name)?;
                Some((
                    self.ctx.types.intern_string(&name),
                    (elem_idx, (pos + 1) as u32),
                ))
            })
            .collect()
    }

    pub(super) fn object_literal_circular_return_method_sites(
        &self,
        obj_all_method_names: &FxHashMap<Atom, (NodeIndex, u32)>,
    ) -> FxHashSet<NodeIndex> {
        let unannotated_methods: FxHashMap<Atom, NodeIndex> = obj_all_method_names
            .iter()
            .filter_map(|(&name, &(elem_idx, _))| {
                self.object_literal_callable_member_has_inferred_return(elem_idx)
                    .then_some((name, elem_idx))
            })
            .collect();
        if unannotated_methods.is_empty() {
            return FxHashSet::default();
        }

        let mut graph: FxHashMap<NodeIndex, Vec<NodeIndex>> = FxHashMap::default();
        for &elem_idx in unannotated_methods.values() {
            let Some(body_idx) = self.object_literal_callable_member_body(elem_idx) else {
                continue;
            };
            let mut callees = FxHashSet::default();
            self.collect_this_member_calls_in_returns(body_idx, &unannotated_methods, &mut callees);
            if !callees.is_empty() {
                graph.insert(elem_idx, callees.into_iter().collect());
            }
        }

        let mut circular_sites = FxHashSet::default();
        let mut visited = FxHashSet::default();
        let mut stack = Vec::new();
        for &elem_idx in unannotated_methods.values() {
            Self::collect_circular_return_graph_sites(
                elem_idx,
                &graph,
                &mut visited,
                &mut stack,
                &mut circular_sites,
            );
        }
        circular_sites
    }

    fn object_literal_callable_member_body(&self, elem_idx: NodeIndex) -> Option<NodeIndex> {
        let elem_node = self.ctx.arena.get(elem_idx)?;
        if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
            return method.body.into_option();
        }

        let prop = self.ctx.arena.get_property_assignment(elem_node)?;
        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(prop.initializer);
        let init_node = self.ctx.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION {
            return None;
        }
        self.ctx.arena.get_function(init_node)?.body.into_option()
    }

    fn object_literal_callable_member_has_inferred_return(&self, elem_idx: NodeIndex) -> bool {
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return false;
        };
        if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
            return method.type_annotation.is_none() && method.body.is_some();
        }

        let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
            return false;
        };
        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(prop.initializer);
        let Some(init_node) = self.ctx.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION {
            return false;
        }
        self.ctx
            .arena
            .get_function(init_node)
            .is_some_and(|func| func.type_annotation.is_none() && func.body.is_some())
    }

    fn collect_circular_return_graph_sites(
        elem_idx: NodeIndex,
        graph: &FxHashMap<NodeIndex, Vec<NodeIndex>>,
        visited: &mut FxHashSet<NodeIndex>,
        stack: &mut Vec<NodeIndex>,
        circular_sites: &mut FxHashSet<NodeIndex>,
    ) {
        if let Some(cycle_start) = stack.iter().position(|&stacked| stacked == elem_idx) {
            circular_sites.extend(stack[cycle_start..].iter().copied());
            return;
        }
        if visited.contains(&elem_idx) {
            return;
        }

        stack.push(elem_idx);
        if let Some(targets) = graph.get(&elem_idx) {
            for &target in targets {
                Self::collect_circular_return_graph_sites(
                    target,
                    graph,
                    visited,
                    stack,
                    circular_sites,
                );
            }
        }
        stack.pop();
        visited.insert(elem_idx);
    }

    fn collect_this_member_calls_in_returns(
        &self,
        body_idx: NodeIndex,
        unannotated_methods: &FxHashMap<Atom, NodeIndex>,
        callees: &mut FxHashSet<NodeIndex>,
    ) {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };
        if body_node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.ctx.arena.get_block(body_node) {
                for &stmt_idx in &block.statements.nodes {
                    self.collect_this_member_calls_in_return_statement(
                        stmt_idx,
                        unannotated_methods,
                        callees,
                    );
                }
            }
        } else {
            self.collect_this_member_calls_in_return_expression(
                body_idx,
                unannotated_methods,
                callees,
            );
        }
    }

    fn collect_this_member_calls_in_return_statement(
        &self,
        stmt_idx: NodeIndex,
        unannotated_methods: &FxHashMap<Atom, NodeIndex>,
        callees: &mut FxHashSet<NodeIndex>,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        match node.kind {
            syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.ctx.arena.get_return_statement(node)
                    && ret.expression.is_some()
                {
                    self.collect_this_member_calls_in_return_expression(
                        ret.expression,
                        unannotated_methods,
                        callees,
                    );
                }
            }
            syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.collect_this_member_calls_in_return_statement(
                            stmt,
                            unannotated_methods,
                            callees,
                        );
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.ctx.arena.get_if_statement(node) {
                    self.collect_this_member_calls_in_return_statement(
                        if_data.then_statement,
                        unannotated_methods,
                        callees,
                    );
                    if if_data.else_statement.is_some() {
                        self.collect_this_member_calls_in_return_statement(
                            if_data.else_statement,
                            unannotated_methods,
                            callees,
                        );
                    }
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node)
                    && let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block)
                    && let Some(case_block) = self.ctx.arena.get_block(case_block_node)
                {
                    for &clause_idx in &case_block.statements.nodes {
                        if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                            && let Some(clause) = self.ctx.arena.get_case_clause(clause_node)
                        {
                            for &stmt in &clause.statements.nodes {
                                self.collect_this_member_calls_in_return_statement(
                                    stmt,
                                    unannotated_methods,
                                    callees,
                                );
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_this_member_calls_in_return_statement(
                        try_data.try_block,
                        unannotated_methods,
                        callees,
                    );
                    if try_data.catch_clause.is_some() {
                        self.collect_this_member_calls_in_return_statement(
                            try_data.catch_clause,
                            unannotated_methods,
                            callees,
                        );
                    }
                    if try_data.finally_block.is_some() {
                        self.collect_this_member_calls_in_return_statement(
                            try_data.finally_block,
                            unannotated_methods,
                            callees,
                        );
                    }
                }
            }
            syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.ctx.arena.get_catch_clause(node) {
                    self.collect_this_member_calls_in_return_statement(
                        catch_data.block,
                        unannotated_methods,
                        callees,
                    );
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_this_member_calls_in_return_statement(
                        loop_data.statement,
                        unannotated_methods,
                        callees,
                    );
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_this_member_calls_in_return_statement(
                        loop_data.statement,
                        unannotated_methods,
                        callees,
                    );
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_this_member_calls_in_return_statement(
                        labeled.statement,
                        unannotated_methods,
                        callees,
                    );
                }
            }
            _ => {}
        }
    }

    fn collect_this_member_calls_in_return_expression(
        &self,
        expr_idx: NodeIndex,
        unannotated_methods: &FxHashMap<Atom, NodeIndex>,
        callees: &mut FxHashSet<NodeIndex>,
    ) {
        if self.object_literal_expression_is_void_prefix_unary(expr_idx) {
            return;
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return;
        };

        if matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION
        ) {
            return;
        }

        if node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.ctx.arena.get_call_expr(node)
        {
            let callee = self
                .ctx
                .arena
                .skip_parenthesized_and_assertions(call.expression);
            if let Some(callee_node) = self.ctx.arena.get(callee)
                && callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.ctx.arena.get_access_expr(callee_node)
            {
                let receiver = self
                    .ctx
                    .arena
                    .skip_parenthesized_and_assertions(access.expression);
                if self.ctx.arena.get(receiver).is_some_and(|receiver_node| {
                    receiver_node.kind == SyntaxKind::ThisKeyword as u16
                }) && let Some(name) = self
                    .ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.as_str())
                {
                    let atom = self.ctx.types.intern_string(name);
                    if let Some(&target_idx) = unannotated_methods.get(&atom) {
                        callees.insert(target_idx);
                    }
                }
            }
        }

        for child_idx in self.ctx.arena.get_children(expr_idx) {
            self.collect_this_member_calls_in_return_expression(
                child_idx,
                unannotated_methods,
                callees,
            );
        }
    }

    fn object_literal_expression_is_void_prefix_unary(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        self.ctx.arena.get(expr_idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && self
                    .ctx
                    .arena
                    .get_unary_expr(node)
                    .is_some_and(|unary| unary.operator == SyntaxKind::VoidKeyword as u16)
        })
    }

    pub(super) fn build_object_literal_method_synthetic_this_type(
        &mut self,
        properties: &rustc_hash::FxHashMap<tsz_common::interner::Atom, tsz_solver::PropertyInfo>,
        obj_all_method_names: &rustc_hash::FxHashMap<tsz_common::interner::Atom, (NodeIndex, u32)>,
        member_previews: &rustc_hash::FxHashMap<
            tsz_common::interner::Atom,
            tsz_solver::PropertyInfo,
        >,
        current_method_idx: NodeIndex,
        current_method_name: &str,
        current_method_type_override: Option<TypeId>,
    ) -> TypeId {
        let mut this_props: Vec<tsz_solver::PropertyInfo> = properties.values().cloned().collect();

        if self.ctx.in_const_assertion {
            for prop in &mut this_props {
                prop.readonly = true;
            }
        }

        let current_method_name_atom = self.ctx.types.intern_string(current_method_name);
        for (&method_name_atom, &(other_elem_idx, decl_order)) in obj_all_method_names {
            if this_props.iter().any(|p| p.name == method_name_atom) {
                continue;
            }

            let method_type = if method_name_atom == current_method_name_atom {
                if let Some(override_type) = current_method_type_override {
                    override_type
                } else {
                    let Some(current_method_node) = self.ctx.arena.get(current_method_idx) else {
                        continue;
                    };
                    let Some(current_method) = self.ctx.arena.get_method_decl(current_method_node)
                    else {
                        continue;
                    };
                    let (_, tp_updates) =
                        self.push_type_parameters(&current_method.type_parameters);
                    let params = current_method
                        .parameters
                        .nodes
                        .iter()
                        .filter_map(|&param_idx| {
                            let param =
                                self.ctx.arena.get(param_idx).and_then(|param_node| {
                                    self.ctx.arena.get_parameter(param_node)
                                })?;
                            Some(tsz_solver::ParamInfo {
                                name: self
                                    .ctx
                                    .arena
                                    .get(param.name)
                                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                                    .map(|ident| self.ctx.types.intern_string(&ident.escaped_text)),
                                type_id: if param.type_annotation.is_some() {
                                    self.get_type_from_type_node(param.type_annotation)
                                } else {
                                    TypeId::ANY
                                },
                                optional: param.question_token || param.initializer.is_some(),
                                rest: param.dot_dot_dot_token,
                            })
                        })
                        .collect();
                    let placeholder = self.ctx.types.factory().callable(CallableShape {
                        call_signatures: vec![CallSignature {
                            type_params: Vec::new(),
                            params,
                            this_type: None,
                            return_type: TypeId::VOID,
                            type_predicate: None,
                            is_method: true,
                        }],
                        construct_signatures: Vec::new(),
                        properties: Vec::new(),
                        string_index: None,
                        number_index: None,
                        symbol: None,
                        is_abstract: false,
                    });
                    self.pop_type_parameters(tp_updates);
                    placeholder
                }
            } else {
                let (other_params, other_return_type) = self
                    .ctx
                    .arena
                    .get(other_elem_idx)
                    .and_then(|n| self.ctx.arena.get_method_decl(n))
                    .map(|other_method| {
                        let params: Vec<tsz_solver::ParamInfo> = other_method
                            .parameters
                            .nodes
                            .iter()
                            .filter_map(|&param_idx| {
                                let param = self
                                    .ctx
                                    .arena
                                    .get(param_idx)
                                    .and_then(|pn| self.ctx.arena.get_parameter(pn))?;
                                if let Some(name_node) = self.ctx.arena.get(param.name)
                                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                    && ident.escaped_text == "this"
                                {
                                    return None;
                                }
                                Some(tsz_solver::ParamInfo {
                                    name: self
                                        .ctx
                                        .arena
                                        .get(param.name)
                                        .and_then(|name_node| {
                                            self.ctx.arena.get_identifier(name_node)
                                        })
                                        .map(|ident| {
                                            self.ctx.types.intern_string(&ident.escaped_text)
                                        }),
                                    type_id: if param.type_annotation.is_some() {
                                        self.get_type_from_type_node(param.type_annotation)
                                    } else {
                                        TypeId::ANY
                                    },
                                    optional: param.question_token || param.initializer.is_some(),
                                    rest: param.dot_dot_dot_token,
                                })
                            })
                            .collect();
                        let return_type = if other_method.type_annotation.is_some() {
                            self.get_type_from_type_node(other_method.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                        (params, return_type)
                    })
                    .unwrap_or_else(|| {
                        (
                            vec![tsz_solver::ParamInfo {
                                name: None,
                                type_id: TypeId::ANY,
                                optional: false,
                                rest: true,
                            }],
                            TypeId::ANY,
                        )
                    });

                self.ctx.types.factory().callable(CallableShape {
                    call_signatures: vec![CallSignature {
                        type_params: Vec::new(),
                        params: other_params,
                        this_type: None,
                        return_type: other_return_type,
                        type_predicate: None,
                        is_method: true,
                    }],
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                    is_abstract: false,
                })
            };

            this_props.push(tsz_solver::PropertyInfo {
                name: method_name_atom,
                type_id: method_type,
                write_type: method_type,
                optional: false,
                readonly: self.ctx.in_const_assertion,
                is_method: true,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: decl_order,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
            });
        }

        self.extend_this_props_with_member_previews(&mut this_props, member_previews);
        self.ctx.types.factory().object(this_props)
    }

    /// Build a synthetic `this` type for a function expression that is a property
    /// initializer in an object literal. Similar to `build_object_literal_method_synthetic_this_type`
    /// but for property assignments like `{ prop: function() { this.n } }`.
    ///
    /// The synthetic type includes:
    /// - All already-processed properties from the object literal
    /// - Placeholder signatures for pre-scanned method declarations
    /// - Previews for later data properties and accessors
    pub(super) fn build_object_literal_fn_property_synthetic_this_type(
        &mut self,
        properties: &rustc_hash::FxHashMap<tsz_common::interner::Atom, tsz_solver::PropertyInfo>,
        obj_all_method_names: &rustc_hash::FxHashMap<tsz_common::interner::Atom, (NodeIndex, u32)>,
        member_previews: &rustc_hash::FxHashMap<
            tsz_common::interner::Atom,
            tsz_solver::PropertyInfo,
        >,
        _current_property_name: &str,
    ) -> TypeId {
        let mut this_props: Vec<tsz_solver::PropertyInfo> = properties.values().cloned().collect();

        if self.ctx.in_const_assertion {
            for prop in &mut this_props {
                prop.readonly = true;
            }
        }

        // Add placeholder callable types for pre-scanned method declarations
        for (&method_name_atom, &(other_elem_idx, decl_order)) in obj_all_method_names {
            if this_props.iter().any(|p| p.name == method_name_atom) {
                continue;
            }

            let (other_params, other_return_type) = self
                .ctx
                .arena
                .get(other_elem_idx)
                .and_then(|n| self.ctx.arena.get_method_decl(n))
                .map(|other_method| {
                    let params: Vec<tsz_solver::ParamInfo> = other_method
                        .parameters
                        .nodes
                        .iter()
                        .filter_map(|&param_idx| {
                            let param = self
                                .ctx
                                .arena
                                .get(param_idx)
                                .and_then(|pn| self.ctx.arena.get_parameter(pn))?;
                            if let Some(name_node) = self.ctx.arena.get(param.name)
                                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                && ident.escaped_text == "this"
                            {
                                return None;
                            }
                            Some(tsz_solver::ParamInfo {
                                name: self
                                    .ctx
                                    .arena
                                    .get(param.name)
                                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                                    .map(|ident| self.ctx.types.intern_string(&ident.escaped_text)),
                                type_id: if param.type_annotation.is_some() {
                                    self.get_type_from_type_node(param.type_annotation)
                                } else {
                                    TypeId::ANY
                                },
                                optional: param.question_token || param.initializer.is_some(),
                                rest: param.dot_dot_dot_token,
                            })
                        })
                        .collect();
                    let return_type = if other_method.type_annotation.is_some() {
                        self.get_type_from_type_node(other_method.type_annotation)
                    } else {
                        TypeId::ANY
                    };
                    (params, return_type)
                })
                .unwrap_or_else(|| {
                    (
                        vec![tsz_solver::ParamInfo {
                            name: None,
                            type_id: TypeId::ANY,
                            optional: false,
                            rest: true,
                        }],
                        TypeId::ANY,
                    )
                });

            this_props.push(tsz_solver::PropertyInfo {
                name: method_name_atom,
                type_id: self.ctx.types.factory().callable(CallableShape {
                    call_signatures: vec![CallSignature {
                        type_params: Vec::new(),
                        params: other_params,
                        this_type: None,
                        return_type: other_return_type,
                        type_predicate: None,
                        is_method: true,
                    }],
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                    is_abstract: false,
                }),
                write_type: TypeId::ANY,
                optional: false,
                readonly: self.ctx.in_const_assertion,
                is_method: true,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: decl_order,
                is_string_named: false,
                is_symbol_named: false,
                single_quoted_name: false,
            });
        }

        self.extend_this_props_with_member_previews(&mut this_props, member_previews);
        self.ctx.types.factory().object(this_props)
    }

    /// Append placeholders for object-literal **data properties** and
    /// **accessors** (computed once via [`object_literal_member_previews`]) to a
    /// synthetic `this` member list, skipping any name already present.
    ///
    /// `tsc` types `this` inside an object-literal method / accessor /
    /// `function`-expression property as the *complete* object literal type,
    /// independent of member declaration order. The incremental builders above
    /// only see members processed *before* the current one plus the callable
    /// pre-scan, so later non-function data properties and accessors would
    /// otherwise be invisible (spurious `TS2339` / `TS7023`). The precise types
    /// of already-processed members (in `this_props`) take precedence over the
    /// previews.
    pub(super) fn extend_this_props_with_member_previews(
        &self,
        this_props: &mut Vec<PropertyInfo>,
        member_previews: &FxHashMap<Atom, PropertyInfo>,
    ) {
        if member_previews.is_empty() {
            return;
        }
        let existing: FxHashSet<Atom> = this_props.iter().map(|p| p.name).collect();
        for (&name_atom, preview) in member_previews {
            if existing.contains(&name_atom) {
                continue;
            }
            let mut preview = preview.clone();
            if self.ctx.in_const_assertion {
                preview.readonly = true;
            }
            this_props.push(preview);
        }
    }

    /// Forward-scan an object literal's elements and build placeholder
    /// [`PropertyInfo`]s for every statically-named **data property** and
    /// **accessor**, so a member's synthetic `this` can see members declared
    /// *after* it (matching `tsc`'s order-independent object-literal `this`).
    ///
    /// Methods and `function`/arrow-valued data properties are intentionally
    /// excluded — they are already represented through the callable pre-scan
    /// (`object_literal_callable_member_names`).
    ///
    /// Data-property previews carry the (widened) initializer type so a real
    /// `this.value` mismatch still surfaces (e.g. `TS2322`). To keep early
    /// preview evaluation from diverging from in-order resolution, a data
    /// property whose initializer references the enclosing object's own
    /// variable binding (`const o = { m() { this.v }, v: o.x }`) falls back to
    /// `any` rather than being evaluated out of order.
    ///
    /// Returns an empty map unless a synthetic `this` will actually be built —
    /// i.e. the object literal has no `ThisType<T>` marker, no contextual type,
    /// and no enclosing `this`. Restricting eager evaluation to that case keeps
    /// the data-property initializer previews off the contextual path (where
    /// the in-order computation uses a different, contextual cache) and avoids
    /// needless work.
    pub(super) fn object_literal_synthetic_this_member_previews(
        &mut self,
        obj_idx: NodeIndex,
        elements: &[NodeIndex],
        marker_this_type: Option<TypeId>,
        contextual_type: Option<TypeId>,
    ) -> FxHashMap<Atom, PropertyInfo> {
        if marker_this_type.is_some()
            || contextual_type.is_some()
            || self.current_this_type().is_some()
        {
            return FxHashMap::default();
        }
        self.object_literal_member_previews(obj_idx, elements)
    }

    fn object_literal_member_previews(
        &mut self,
        obj_idx: NodeIndex,
        elements: &[NodeIndex],
    ) -> FxHashMap<Atom, PropertyInfo> {
        let mut previews: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();

        // Name of the enclosing object's own variable binding, if any. Used to
        // guard against self-referential initializers (see doc comment).
        let self_ref_name = self.object_literal_variable_initializer_name(obj_idx);

        // Pre-collect setter names so getter-only accessors are marked readonly.
        let mut setter_names: FxHashSet<Atom> = FxHashSet::default();
        for &elem_idx in elements {
            let Some(node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::SET_ACCESSOR
                && let Some(accessor) = self.ctx.arena.get_accessor(node)
                && let Some(name) = self.get_property_name_resolved(accessor.name)
            {
                setter_names.insert(self.ctx.types.intern_string(&name));
            }
        }

        for (pos, &elem_idx) in elements.iter().enumerate() {
            let Some(node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let decl_order = (pos + 1) as u32;

            // Accessor: { get x() {} } / { set x(v) {} }
            if matches!(
                node.kind,
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR
            ) {
                let Some(accessor) = self.ctx.arena.get_accessor(node) else {
                    continue;
                };
                if self.object_literal_computed_key_is_wide_symbol(accessor.name) {
                    continue;
                }
                let Some(name) = self.get_property_name_resolved(accessor.name) else {
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);
                let is_getter = node.kind == syntax_kind_ext::GET_ACCESSOR;
                // A setter that shares its name with a getter is subordinate to
                // the getter's read type; only fill in a setter-only entry.
                if !is_getter && previews.contains_key(&name_atom) {
                    continue;
                }
                let member_type = if is_getter {
                    if accessor.type_annotation.is_some() {
                        self.get_type_from_type_node(accessor.type_annotation)
                    } else {
                        TypeId::ANY
                    }
                } else {
                    self.object_literal_setter_parameter_type(accessor)
                };
                let mut preview =
                    Self::object_literal_preview_data_property(name_atom, member_type, decl_order);
                // A getter with no paired setter is readonly on the object type.
                preview.readonly = is_getter && !setter_names.contains(&name_atom);
                previews.insert(name_atom, preview);
                continue;
            }

            // Shorthand data property: { x }
            if node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                let Some(shorthand) = self.ctx.arena.get_shorthand_property(node) else {
                    continue;
                };
                let Some(name) = self.get_property_name_resolved(shorthand.name) else {
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);
                if previews.contains_key(&name_atom) {
                    continue;
                }
                let member_type =
                    self.object_literal_preview_data_type(shorthand.name, self_ref_name.as_deref());
                previews.insert(
                    name_atom,
                    Self::object_literal_preview_data_property(name_atom, member_type, decl_order),
                );
                continue;
            }

            // Regular data property: { x: <expr> }
            let Some(prop) = self.ctx.arena.get_property_assignment(node) else {
                continue;
            };
            // Function/arrow initializers are covered by the callable pre-scan.
            let initializer = self
                .ctx
                .arena
                .skip_parenthesized_and_assertions(prop.initializer);
            if self.ctx.arena.get(initializer).is_some_and(|init_node| {
                matches!(
                    init_node.kind,
                    syntax_kind_ext::ARROW_FUNCTION | syntax_kind_ext::FUNCTION_EXPRESSION
                )
            }) {
                continue;
            }
            // Skip computed / unresolved keys.
            let Some(name) = self.get_property_name_resolved(prop.name) else {
                continue;
            };
            let name_atom = self.ctx.types.intern_string(&name);
            if previews.contains_key(&name_atom) {
                continue;
            }
            // `prop.initializer == prop.name` is the parser's error-recovery
            // sentinel for a missing value expression.
            let member_type = if prop.initializer == prop.name {
                TypeId::ANY
            } else {
                self.object_literal_preview_data_type(prop.initializer, self_ref_name.as_deref())
            };
            previews.insert(
                name_atom,
                Self::object_literal_preview_data_property(name_atom, member_type, decl_order),
            );
        }

        previews
    }

    /// Build the (widened) preview type of a data-property initializer for the
    /// synthetic `this`, falling back to `any` for self-referential
    /// initializers (see [`object_literal_member_previews`]).
    fn object_literal_preview_data_type(
        &mut self,
        initializer: NodeIndex,
        self_ref_name: Option<&str>,
    ) -> TypeId {
        if let Some(name) = self_ref_name
            && self.object_literal_initializer_references_name(initializer, name)
        {
            return TypeId::ANY;
        }
        let raw = self.get_type_of_node(initializer);
        if self.ctx.in_const_assertion {
            raw
        } else {
            self.widen_literal_type(raw)
        }
    }

    const fn object_literal_preview_data_property(
        name_atom: Atom,
        member_type: TypeId,
        decl_order: u32,
    ) -> PropertyInfo {
        PropertyInfo {
            declaration_order: decl_order,
            ..PropertyInfo::new(name_atom, member_type)
        }
    }

    fn object_literal_setter_parameter_type(
        &mut self,
        accessor: &tsz_parser::parser::node::AccessorData,
    ) -> TypeId {
        accessor
            .parameters
            .nodes
            .first()
            .and_then(|&param_idx| {
                let param = self
                    .ctx
                    .arena
                    .get(param_idx)
                    .and_then(|pn| self.ctx.arena.get_parameter(pn))?;
                param
                    .type_annotation
                    .is_some()
                    .then(|| self.get_type_from_type_node(param.type_annotation))
            })
            .unwrap_or(TypeId::ANY)
    }

    /// Name of the variable binding an object literal initializes, if it is the
    /// initializer of a `const`/`let`/`var` declaration (`const o = { … }`).
    fn object_literal_variable_initializer_name(&self, obj_idx: NodeIndex) -> Option<String> {
        let parent_idx = self.ctx.arena.get_extended(obj_idx)?.parent;
        let parent_node = self.ctx.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.ctx.arena.get_variable_declaration(parent_node)?;
        if var_decl.initializer != obj_idx {
            return None;
        }
        let name_node = self.ctx.arena.get(var_decl.name)?;
        let ident = self.ctx.arena.get_identifier(name_node)?;
        Some(ident.escaped_text.to_string())
    }

    /// Whether the expression subtree contains an identifier with the given
    /// name (used to detect self-referential data-property initializers).
    fn object_literal_initializer_references_name(&self, idx: NodeIndex, name: &str) -> bool {
        if idx.is_none() {
            return false;
        }
        if let Some(ident) = self.ctx.arena.get_identifier_at(idx)
            && ident.escaped_text == name
        {
            return true;
        }
        self.ctx
            .arena
            .get_children(idx)
            .into_iter()
            .any(|child| self.object_literal_initializer_references_name(child, name))
    }
}
