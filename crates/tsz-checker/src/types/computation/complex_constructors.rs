use crate::query_boundaries::type_computation::complex as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

/// Prototype-derived members collected from sibling statements surrounding a
/// constructor function. `method_bindings` are `FuncName.prototype.X = ...`
/// assignments; `this_props` are `this.X = ...` assignments inside prototype
/// method bodies; `has_evidence` is `true` when any prototype pattern was
/// observed (used to decide whether to synthesize a JS class type).
pub(crate) struct PrototypeMembers {
    pub method_bindings: Vec<(tsz_common::interner::Atom, tsz_solver::PropertyInfo)>,
    pub this_props: Vec<(tsz_common::interner::Atom, tsz_solver::PropertyInfo)>,
    pub has_evidence: bool,
}

impl<'a> CheckerState<'a> {
    fn shallow_object_literal_callable_type(&mut self, callable_idx: NodeIndex) -> TypeId {
        use tsz_solver::{CallSignature, CallableShape, ParamInfo};

        let Some(callable_node) = self.ctx.arena.get(callable_idx) else {
            return TypeId::ANY;
        };
        let (parameters, type_parameters, return_type_node) =
            if let Some(method) = self.ctx.arena.get_method_decl(callable_node) {
                (
                    method.parameters.clone(),
                    method.type_parameters.clone(),
                    method.type_annotation,
                )
            } else if let Some(func) = self.ctx.arena.get_function(callable_node) {
                (
                    func.parameters.clone(),
                    func.type_parameters.clone(),
                    func.type_annotation,
                )
            } else {
                return TypeId::ANY;
            };

        let mut jsdoc_type_param_updates = Vec::new();
        let mut func_jsdoc = None;
        let mut jsdoc_param_names = Vec::new();
        let mut comment_start = None;
        let mut type_params = Vec::new();

        if self.is_js_file() {
            func_jsdoc = self.get_jsdoc_for_function(callable_idx);
            comment_start = self.get_jsdoc_comment_pos_for_function(callable_idx);
            jsdoc_param_names = func_jsdoc
                .as_ref()
                .map(|jsdoc| {
                    Self::extract_jsdoc_param_names(jsdoc)
                        .into_iter()
                        .map(|(name, _)| name)
                        .collect()
                })
                .unwrap_or_default();

            if type_parameters.is_none() {
                let factory = self.ctx.types.factory();
                for (name, is_const) in func_jsdoc
                    .as_ref()
                    .map(|jsdoc| Self::jsdoc_template_type_params(jsdoc))
                    .unwrap_or_default()
                {
                    let atom = self.ctx.types.intern_string(&name);
                    let info = tsz_solver::TypeParamInfo {
                        name: atom,
                        constraint: None,
                        default: None,
                        is_const,
                    };
                    let ty = factory.type_param(info);
                    let previous = self.ctx.type_parameter_scope.insert(name.clone(), ty);
                    jsdoc_type_param_updates.push((name, previous, false));
                    type_params.push(info);
                }
            }
        }

        let (declared_type_params, type_param_updates) =
            self.push_type_parameters(&type_parameters);
        if type_params.is_empty() {
            type_params = declared_type_params;
        }
        let params = parameters
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(param_pos, &param_idx)| {
                let param = self
                    .ctx
                    .arena
                    .get(param_idx)
                    .and_then(|param_node| self.ctx.arena.get_parameter(param_node))?;
                let jsdoc_type = if let (Some(jsdoc), Some(comment_start)) =
                    (func_jsdoc.as_ref(), comment_start)
                {
                    let pname =
                        self.effective_jsdoc_param_name(param.name, &jsdoc_param_names, param_pos);
                    self.resolve_jsdoc_param_type_with_pos(jsdoc, &pname, Some(comment_start))
                } else {
                    None
                };
                Some(ParamInfo {
                    name: self
                        .ctx
                        .arena
                        .get(param.name)
                        .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                        .map(|ident| self.ctx.types.intern_string(&ident.escaped_text)),
                    type_id: if param.type_annotation.is_some() {
                        self.get_type_from_type_node(param.type_annotation)
                    } else if let Some(jsdoc_type) = jsdoc_type {
                        jsdoc_type
                    } else {
                        TypeId::ANY
                    },
                    optional: param.question_token || param.initializer.is_some(),
                    rest: param.dot_dot_dot_token,
                })
            })
            .collect();
        let return_type = if return_type_node.is_some() {
            self.get_type_from_type_node(return_type_node)
        } else if let Some(jsdoc) = func_jsdoc.as_ref() {
            self.resolve_jsdoc_return_type(jsdoc).unwrap_or(TypeId::ANY)
        } else {
            TypeId::ANY
        };
        self.pop_type_parameters(type_param_updates);
        for (name, previous, shadowed_class_param) in jsdoc_type_param_updates.into_iter().rev() {
            if let Some(prev) = previous {
                self.ctx.type_parameter_scope.insert(name.clone(), prev);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
            if shadowed_class_param && let Some(ref mut c) = self.ctx.enclosing_class {
                c.type_param_names.push(name);
            }
        }

        self.ctx.types.factory().callable(CallableShape {
            call_signatures: vec![CallSignature {
                type_params,
                params,
                this_type: None,
                return_type,
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
    }

    fn assignment_chain_terminal_object_literal(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let mut current = expr_idx;
        loop {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return Some(current);
            }
            if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                return None;
            }
            let binary = self.ctx.arena.get_binary_expr(node)?;
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                return None;
            }
            current = binary.right;
        }
    }

    fn collect_object_literal_prototype_bindings(
        &mut self,
        object_idx: NodeIndex,
        parent_sym: tsz_binder::SymbolId,
        method_bindings: &mut Vec<(tsz_common::interner::Atom, tsz_solver::PropertyInfo)>,
    ) {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(rhs_node) = self.ctx.arena.get(object_idx) else {
            return;
        };
        if rhs_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return;
        }
        let Some(obj) = self.ctx.arena.get_literal_expr(rhs_node) else {
            return;
        };
        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            if elem_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method) = self.ctx.arena.get_method_decl(elem_node) else {
                    continue;
                };
                let is_computed_name = self
                    .ctx
                    .arena
                    .get(method.name)
                    .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);
                if is_computed_name {
                    continue;
                }
                if self
                    .ctx
                    .arena
                    .get(method.name)
                    .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16)
                {
                    continue;
                }
                let Some(prop_name_str) = self.get_property_name_resolved(method.name) else {
                    continue;
                };
                let prop_name_atom = self.ctx.types.intern_string(&prop_name_str);
                // Avoid re-entering full object-literal method typing here. Prototype
                // object literal methods are harvested while synthesizing the instance
                // type for the same constructor, so contextual `this` reconstruction
                // can recurse back into this collector and overflow the stack.
                let rhs_type = self.shallow_object_literal_callable_type(elem_idx);
                method_bindings.push((
                    prop_name_atom,
                    tsz_solver::PropertyInfo {
                        name: prop_name_atom,
                        type_id: rhs_type,
                        write_type: rhs_type,
                        optional: false,
                        readonly: false,
                        is_method: true,
                        is_class_prototype: false,
                        visibility: tsz_solver::Visibility::Public,
                        parent_id: Some(parent_sym),
                        declaration_order: 0,
                        is_string_named: false,
                    },
                ));
                continue;
            }

            let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                continue;
            };
            let is_computed_name = self
                .ctx
                .arena
                .get(prop.name)
                .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);
            if is_computed_name {
                continue;
            }
            if self
                .ctx
                .arena
                .get(prop.name)
                .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16)
            {
                continue;
            }
            let Some(prop_name_str) = self.get_property_name_resolved(prop.name) else {
                continue;
            };
            let prop_name_atom = self.ctx.types.intern_string(&prop_name_str);
            let initializer_is_function_expression = self
                .ctx
                .arena
                .get(prop.initializer)
                .is_some_and(|node| node.kind == syntax_kind_ext::FUNCTION_EXPRESSION);
            let (rhs_type, is_method) = if initializer_is_function_expression {
                (
                    self.shallow_object_literal_callable_type(prop.initializer),
                    true,
                )
            } else {
                (self.get_type_of_node(prop.initializer), false)
            };
            method_bindings.push((
                prop_name_atom,
                tsz_solver::PropertyInfo {
                    name: prop_name_atom,
                    type_id: rhs_type,
                    write_type: rhs_type,
                    optional: false,
                    readonly: false,
                    is_method,
                    is_class_prototype: false,
                    visibility: tsz_solver::Visibility::Public,
                    parent_id: Some(parent_sym),
                    declaration_order: 0,
                    is_string_named: false,
                },
            ));
        }
    }

    fn collect_chained_prototype_object_literal_bindings(
        &mut self,
        expr_idx: NodeIndex,
        func_name: &str,
        parent_sym: tsz_binder::SymbolId,
        method_bindings: &mut Vec<(tsz_common::interner::Atom, tsz_solver::PropertyInfo)>,
        has_prototype_evidence: &mut bool,
    ) {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return;
        };
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return;
        }
        let Some(binary) = self.ctx.arena.get_binary_expr(expr_node) else {
            return;
        };
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return;
        }

        if self.access_matches_function_prototype(binary.left, func_name) {
            *has_prototype_evidence = true;
            if let Some(object_idx) = self.assignment_chain_terminal_object_literal(binary.right) {
                self.collect_object_literal_prototype_bindings(
                    object_idx,
                    parent_sym,
                    method_bindings,
                );
            }
        }

        self.collect_chained_prototype_object_literal_bindings(
            binary.right,
            func_name,
            parent_sym,
            method_bindings,
            has_prototype_evidence,
        );
    }
    const fn is_property_like_access_kind(kind: u16) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
    }

    fn access_matches_function_prototype(&self, access_idx: NodeIndex, func_name: &str) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(access_node) = self.ctx.arena.get(access_idx) else {
            return false;
        };
        if !Self::is_property_like_access_kind(access_node.kind) {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(access_node) else {
            return false;
        };
        let Some(base_node) = self.ctx.arena.get(access.expression) else {
            return false;
        };
        if base_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(base_ident) = self.ctx.arena.get_identifier(base_node) else {
            return false;
        };
        if base_ident.escaped_text != func_name {
            return false;
        }
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let Some(name_ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        name_ident.escaped_text == "prototype"
    }

    fn js_prototype_binding_literal_name(&self, name_idx: NodeIndex) -> Option<String> {
        use tsz_scanner::SyntaxKind;

        let name_node = self.ctx.arena.get(name_idx)?;
        match name_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                self.ctx
                    .arena
                    .get_literal(name_node)
                    .map(|lit| lit.text.clone())
            }
            _ => None,
        }
    }

    fn js_prototype_binding_resolved_name(&mut self, name_idx: NodeIndex) -> Option<String> {
        if let Some(name) = self.js_prototype_binding_literal_name(name_idx) {
            return Some(name);
        }

        if let Some(symbol_name) = self.get_symbol_property_name_from_expr(name_idx) {
            return Some(symbol_name);
        }

        let prev_preserve_literals = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let key_type = self.get_type_of_node(name_idx);
        self.ctx.preserve_literal_types = prev_preserve_literals;

        let evaluated_key_type = self.evaluate_type_with_env(key_type);
        let resolved_key_type = self.resolve_lazy_type(evaluated_key_type);

        for candidate in [key_type, evaluated_key_type, resolved_key_type] {
            if let Some(sym_ref) =
                crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, candidate)
            {
                return Some(format!("__unique_{}", sym_ref.0));
            }
            if let Some(atom) =
                crate::query_boundaries::type_computation::access::literal_property_name(
                    self.ctx.types,
                    candidate,
                )
            {
                return Some(self.ctx.types.resolve_atom(atom));
            }
        }

        None
    }

    /// Scan sibling statements for `FuncName.prototype.X = rhs` patterns.
    /// Returns two sets:
    /// - Prototype method bindings (`method_name` -> `method_type`) to be added as instance properties
    /// - `this.prop` assignments from inside prototype method bodies (typed as T | undefined)
    pub(crate) fn collect_prototype_members_and_this_properties(
        &mut self,
        func_decl_idx: NodeIndex,
        func_name: &str,
        parent_sym: tsz_binder::SymbolId,
    ) -> PrototypeMembers {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let mut method_bindings = Vec::new();
        let mut this_props = Vec::new();
        let mut has_prototype_evidence = false;

        let mut parent_idx = self
            .ctx
            .arena
            .get_extended(func_decl_idx)
            .map_or(NodeIndex::NONE, |e| e.parent);
        let mut parent_node = match self.ctx.arena.get(parent_idx) {
            Some(node) => node,
            None => {
                return PrototypeMembers {
                    method_bindings,
                    this_props,
                    has_evidence: has_prototype_evidence,
                };
            }
        };

        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            && let Some(grandparent_idx) = self.ctx.arena.get_extended(parent_idx).map(|e| e.parent)
            && let Some(grandparent) = self.ctx.arena.get(grandparent_idx)
        {
            parent_idx = grandparent_idx;
            parent_node = grandparent;
        }
        if parent_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            && let Some(grandparent_idx) = self.ctx.arena.get_extended(parent_idx).map(|e| e.parent)
            && let Some(grandparent) = self.ctx.arena.get(grandparent_idx)
        {
            parent_node = grandparent;
        }

        let siblings: Vec<NodeIndex> = if let Some(block) = self.ctx.arena.get_block(parent_node) {
            block.statements.nodes.clone()
        } else if let Some(source) = self.ctx.arena.get_source_file(parent_node) {
            source.statements.nodes.clone()
        } else {
            return PrototypeMembers {
                method_bindings,
                this_props,
                has_evidence: has_prototype_evidence,
            };
        };

        for &stmt_idx in &siblings {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(expr_node) = self.ctx.arena.get(expr_stmt.expression) else {
                continue;
            };
            if expr_node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some(call_expr) = self.ctx.arena.get_call_expr(expr_node)
                && let Some(arguments) = call_expr.arguments.as_ref()
                && self.is_object_define_property_on_function_prototype(
                    call_expr.expression,
                    &arguments.nodes,
                    func_name,
                )
            {
                has_prototype_evidence = true;
                let current_file_idx = self.ctx.current_file_idx;
                if arguments.nodes.len() >= 3
                    && let Some(name) = self.js_prototype_binding_literal_name(arguments.nodes[1])
                    && let Some(mut prop) = self.define_property_info_from_descriptor(
                        current_file_idx,
                        self.ctx.arena,
                        &name,
                        arguments.nodes[2],
                        method_bindings.len() as u32,
                    )
                {
                    prop.parent_id = Some(parent_sym);
                    let name_atom = prop.name;
                    method_bindings.push((name_atom, prop));
                }
                continue;
            }
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(expr_node) else {
                continue;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }

            self.collect_chained_prototype_object_literal_bindings(
                expr_stmt.expression,
                func_name,
                parent_sym,
                &mut method_bindings,
                &mut has_prototype_evidence,
            );

            let Some(lhs_node) = self.ctx.arena.get(binary.left) else {
                continue;
            };
            if !Self::is_property_like_access_kind(lhs_node.kind) {
                continue;
            }
            let Some(lhs_access) = self.ctx.arena.get_access_expr(lhs_node) else {
                continue;
            };

            if self.access_matches_function_prototype(binary.left, func_name) {
                has_prototype_evidence = true;
                continue;
            }

            let Some(proto_node) = self.ctx.arena.get(lhs_access.expression) else {
                continue;
            };
            if !Self::is_property_like_access_kind(proto_node.kind) {
                continue;
            }
            if !self.access_matches_function_prototype(lhs_access.expression, func_name) {
                continue;
            }
            has_prototype_evidence = true;

            if lhs_access.name_or_argument.is_none() {
                continue;
            }

            let is_computed_name = lhs_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || self
                    .ctx
                    .arena
                    .get(lhs_access.name_or_argument)
                    .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);
            let resolved_property_name = if is_computed_name {
                self.js_prototype_binding_resolved_name(lhs_access.name_or_argument)
            } else {
                self.get_property_name_resolved(lhs_access.name_or_argument)
            };
            if let Some(method_name_str) = resolved_property_name {
                let method_name_atom = self.ctx.types.intern_string(&method_name_str);
                let rhs_type = self.get_type_of_node(binary.right);
                let is_method_like = self
                    .ctx
                    .arena
                    .get(binary.right)
                    .is_some_and(|rhs| rhs.kind == syntax_kind_ext::FUNCTION_EXPRESSION);
                method_bindings.push((
                    method_name_atom,
                    tsz_solver::PropertyInfo {
                        name: method_name_atom,
                        type_id: rhs_type,
                        write_type: rhs_type,
                        optional: false,
                        readonly: false,
                        is_method: is_method_like,
                        is_class_prototype: false,
                        visibility: tsz_solver::Visibility::Public,
                        parent_id: Some(parent_sym),
                        declaration_order: 0,
                        is_string_named: false,
                    },
                ));
            }

            let Some(rhs_node) = self.ctx.arena.get(binary.right) else {
                continue;
            };
            if rhs_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION {
                continue;
            }
            let Some(rhs_func) = self.ctx.arena.get_function(rhs_node) else {
                continue;
            };
            let method_body = rhs_func.body;
            if method_body.is_none() {
                continue;
            }

            let mut method_this_props: rustc_hash::FxHashMap<
                tsz_common::interner::Atom,
                tsz_solver::PropertyInfo,
            > = rustc_hash::FxHashMap::default();
            self.collect_js_constructor_this_properties(
                method_body,
                &mut method_this_props,
                Some(parent_sym),
                false,
            );
            self.collect_nested_arrow_this_properties(
                method_body,
                &mut method_this_props,
                Some(parent_sym),
            );

            for (name, prop) in method_this_props {
                this_props.push((name, prop));
            }
        }

        PrototypeMembers {
            method_bindings,
            this_props,
            has_evidence: has_prototype_evidence,
        }
    }

    pub(crate) fn collect_define_property_bindings_on_function_prototype(
        &mut self,
        func_decl_idx: NodeIndex,
        func_name: &str,
        parent_sym: tsz_binder::SymbolId,
    ) -> Vec<(tsz_common::interner::Atom, tsz_solver::PropertyInfo)> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut parent_idx = self
            .ctx
            .arena
            .get_extended(func_decl_idx)
            .map_or(NodeIndex::NONE, |e| e.parent);
        let mut parent_node = match self.ctx.arena.get(parent_idx) {
            Some(node) => node,
            None => return Vec::new(),
        };

        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            && let Some(grandparent_idx) = self.ctx.arena.get_extended(parent_idx).map(|e| e.parent)
            && let Some(grandparent) = self.ctx.arena.get(grandparent_idx)
        {
            parent_idx = grandparent_idx;
            parent_node = grandparent;
        }
        if parent_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            && let Some(grandparent_idx) = self.ctx.arena.get_extended(parent_idx).map(|e| e.parent)
            && let Some(grandparent) = self.ctx.arena.get(grandparent_idx)
        {
            parent_node = grandparent;
        }

        let siblings: Vec<NodeIndex> = if let Some(block) = self.ctx.arena.get_block(parent_node) {
            block.statements.nodes.clone()
        } else if let Some(source) = self.ctx.arena.get_source_file(parent_node) {
            source.statements.nodes.clone()
        } else {
            return Vec::new();
        };

        let mut props = Vec::new();
        for &stmt_idx in &siblings {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(call_node) = self.ctx.arena.get(expr_stmt.expression) else {
                continue;
            };
            if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
                continue;
            }
            let Some(call) = self.ctx.arena.get_call_expr(call_node) else {
                continue;
            };
            let Some(args) = call.arguments.as_ref() else {
                continue;
            };
            if !self.is_object_define_property_on_function_prototype(
                call.expression,
                &args.nodes,
                func_name,
            ) {
                continue;
            }

            let current_file_idx = self.ctx.current_file_idx;
            let Some(name) = self.js_prototype_binding_literal_name(args.nodes[1]) else {
                continue;
            };
            let Some(mut prop) = self.define_property_info_from_descriptor(
                current_file_idx,
                self.ctx.arena,
                &name,
                args.nodes[2],
                props.len() as u32,
            ) else {
                continue;
            };
            prop.parent_id = Some(parent_sym);
            let name_atom = prop.name;
            props.push((name_atom, prop));
        }

        props
    }

    fn collect_nested_arrow_this_properties(
        &mut self,
        body_idx: NodeIndex,
        properties: &mut rustc_hash::FxHashMap<
            tsz_common::interner::Atom,
            tsz_solver::PropertyInfo,
        >,
        parent_sym: Option<tsz_binder::SymbolId>,
    ) {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };
        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return;
        };

        for &stmt_idx in &block.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(expr_node) = self.ctx.arena.get(expr_stmt.expression) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::ARROW_FUNCTION {
                continue;
            }
            let Some(arrow) = self.ctx.arena.get_function(expr_node) else {
                continue;
            };
            if arrow.body.is_none() {
                continue;
            }
            self.collect_js_constructor_this_properties(arrow.body, properties, parent_sym, false);
        }
    }

    fn is_object_define_property_on_function_prototype(
        &self,
        callee_idx: NodeIndex,
        arg_nodes: &[NodeIndex],
        func_name: &str,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        if arg_nodes.len() < 3 {
            return false;
        }

        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(callee_access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(object_node) = self.ctx.arena.get(callee_access.expression) else {
            return false;
        };
        if object_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(object_ident) = self.ctx.arena.get_identifier(object_node) else {
            return false;
        };
        if object_ident.escaped_text != "Object" {
            return false;
        }
        let Some(name_node) = self.ctx.arena.get(callee_access.name_or_argument) else {
            return false;
        };
        let Some(name_ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        if name_ident.escaped_text != "defineProperty" {
            return false;
        }

        let Some(target_node) = self.ctx.arena.get(arg_nodes[0]) else {
            return false;
        };
        if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(target_access) = self.ctx.arena.get_access_expr(target_node) else {
            return false;
        };
        let Some(base_node) = self.ctx.arena.get(target_access.expression) else {
            return false;
        };
        if base_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(base_ident) = self.ctx.arena.get_identifier(base_node) else {
            return false;
        };
        if base_ident.escaped_text != func_name {
            return false;
        }
        let Some(proto_name_node) = self.ctx.arena.get(target_access.name_or_argument) else {
            return false;
        };
        let Some(proto_ident) = self.ctx.arena.get_identifier(proto_name_node) else {
            return false;
        };
        proto_ident.escaped_text == "prototype"
    }

    /// Resolve a self-referencing class constructor in a static initializer.
    pub(crate) fn resolve_self_referencing_constructor(
        &self,
        constructor_type: TypeId,
        expr_idx: NodeIndex,
    ) -> Option<TypeId> {
        use tsz_binder::symbol_flags;

        query::lazy_def_id(self.ctx.types, constructor_type)?;
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expr_idx)
            .or_else(|| self.ctx.binder.get_node_symbol(expr_idx))?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::CLASS == 0 {
            return None;
        }
        if let Some(&instance_type) = self.ctx.symbol_instance_types.get(&sym_id) {
            return Some(instance_type);
        }
        let decl_idx = symbol.primary_declaration().unwrap_or(NodeIndex::NONE);
        self.ctx.class_instance_type_cache.get(&decl_idx).copied()
    }

    /// Check if the target of a `new` expression is a class with circular
    /// inheritance (TS2506 or TS2310 was emitted for this symbol).
    pub(crate) fn is_circular_class_new(&self, expr_idx: NodeIndex) -> bool {
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expr_idx)
            .or_else(|| self.ctx.binder.get_node_symbol(expr_idx));
        match sym_id {
            Some(id) => self.ctx.circular_class_symbols.contains(&id),
            None => false,
        }
    }

    /// Get the instance type for a class targeted by `new` when the constructor
    /// has no construct signatures (circular inheritance). Returns the cached
    /// instance type with type parameters substituted to `unknown`, matching tsc
    /// behavior where `(new C).blah` on a circular class yields TS2339 on `C<unknown>`.
    pub(crate) fn class_instance_type_for_circular_new(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<TypeId> {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expr_idx)
            .or_else(|| self.ctx.binder.get_node_symbol(expr_idx))
            .or_else(|| self.resolve_qualified_symbol(expr_idx))?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::CLASS == 0 {
            return None;
        }

        // Find the class declaration to get its instance type and type parameters.
        let decl_idx = if symbol.value_declaration.is_some()
            && self
                .ctx
                .arena
                .get(symbol.value_declaration)
                .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_DECLARATION)
        {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .iter()
                .copied()
                .find(|&idx| {
                    self.ctx
                        .arena
                        .get(idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_DECLARATION)
                })
                .unwrap_or(NodeIndex::NONE)
        };

        // Look up the cached instance type.
        let instance_type = self
            .ctx
            .symbol_instance_types
            .get(&sym_id)
            .copied()
            .or_else(|| self.ctx.class_instance_type_cache.get(&decl_idx).copied())?;

        if instance_type == TypeId::ERROR {
            return None;
        }

        // Count the class's type parameters. For generic classes, create an
        // Application type `C<unknown, ...>` so the type formatter displays
        // `C<unknown>` in diagnostics, matching tsc behavior.
        let type_param_count = self
            .ctx
            .arena
            .get(decl_idx)
            .and_then(|n| self.ctx.arena.get_class(n))
            .and_then(|c| c.type_parameters.as_ref())
            .map_or(0, |list| list.nodes.len());

        if type_param_count > 0 {
            // Build Application(Lazy(DefId(C)), [unknown, ...]) so the formatter
            // renders "C<unknown>" and property access checks the instance shape.
            let def_id = self.ctx.get_or_create_def_id(sym_id);
            let factory = self.ctx.types.factory();
            let base = factory.lazy(def_id);
            let unknown_args: Vec<TypeId> =
                (0..type_param_count).map(|_| TypeId::UNKNOWN).collect();
            return Some(factory.application(base, unknown_args));
        }

        Some(instance_type)
    }

    /// Check if a type contains any abstract class constructors.
    pub(crate) fn type_contains_abstract_class(&self, type_id: TypeId) -> bool {
        self.type_contains_abstract_class_inner(type_id, &mut rustc_hash::FxHashSet::default())
    }

    fn type_contains_abstract_class_inner(
        &self,
        type_id: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> bool {
        use tsz_binder::SymbolId;
        use tsz_binder::symbol_flags;

        if !visited.insert(type_id) {
            return false;
        }

        if let Some(callable_shape) = query::callable_shape_for_type(self.ctx.types, type_id) {
            if callable_shape.is_abstract {
                return true;
            }
            if let Some(sym_id) = callable_shape.symbol
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && (symbol.flags & symbol_flags::ABSTRACT) != 0
            {
                return true;
            }
        }

        if let Some(def_id) = query::lazy_def_id(self.ctx.types, type_id)
            && let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.flags & symbol_flags::TYPE_ALIAS != 0
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && let Some(body_type) = def.body
        {
            return self.type_contains_abstract_class_inner(body_type, visited);
        }

        match query::classify_for_abstract_check(self.ctx.types, type_id) {
            query::AbstractClassCheckKind::TypeQuery(sym_ref) => {
                if let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(sym_ref.0))
                    && symbol.flags & symbol_flags::ABSTRACT != 0
                {
                    return true;
                }
                false
            }
            query::AbstractClassCheckKind::Union(members) => members
                .iter()
                .any(|&member| self.type_contains_abstract_class_inner(member, visited)),
            query::AbstractClassCheckKind::Intersection(members) => members
                .iter()
                .any(|&member| self.type_contains_abstract_class_inner(member, visited)),
            query::AbstractClassCheckKind::NotAbstract => false,
        }
    }

    /// Get the construct type from a `TypeId`, used for new expressions.
    pub(crate) fn resolve_ref_type(&mut self, type_id: TypeId) -> TypeId {
        match query::classify_for_lazy_resolution(self.ctx.types, type_id) {
            query::LazyTypeKind::Lazy(def_id) => {
                if let Some(symbol_id) = self.ctx.def_to_symbol_id(def_id) {
                    let symbol_type = self.get_type_of_symbol(symbol_id);
                    if symbol_type == type_id {
                        if let Ok(env) = self.ctx.type_env.try_borrow()
                            && let Some(env_type) = env.get_def(def_id)
                            && env_type != type_id
                        {
                            return env_type;
                        }
                        type_id
                    } else {
                        symbol_type
                    }
                } else {
                    type_id
                }
            }
            _ => type_id,
        }
    }

    /// Resolve type parameter constraints for construct expressions.
    pub(crate) fn resolve_type_param_for_construct(&mut self, type_id: TypeId) -> TypeId {
        let factory = self.ctx.types.factory();
        let Some(info) = query::type_parameter_info(self.ctx.types, type_id) else {
            return type_id;
        };

        let Some(constraint) = info.constraint else {
            return type_id;
        };

        let resolved_constraint = self.resolve_lazy_type(constraint);
        if resolved_constraint == constraint {
            return type_id;
        }

        let new_info = tsz_solver::TypeParamInfo {
            constraint: Some(resolved_constraint),
            ..info
        };
        factory.type_param(new_info)
    }

    /// Check if a `new` expression target has a declared type with generic
    /// construct signatures.
    ///
    /// For `new this.Map_<K, V>()` in a property initializer, `this.Map_` may
    /// resolve to `any` because the class is being constructed. But the member's
    /// DECLARED type (`{ new<K, V>(): any }`) has type parameters on its
    /// construct signature, so TS2347 should NOT fire.
    pub(crate) fn new_target_has_declared_generic_construct(
        &mut self,
        expr_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        // Must be a property access expression (e.g., this.Map_)
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
            return false;
        };

        // The object must be `this`
        let Some(obj_node) = self.ctx.arena.get(access.expression) else {
            return false;
        };
        if obj_node.kind != SyntaxKind::ThisKeyword as u16 {
            return false;
        }

        // Get the property name
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        let prop_name = &ident.escaped_text;

        // Find the enclosing class and check if the member has a declared
        // type with generic construct signatures
        // Walk up parents to find enclosing class
        let class_idx = {
            let mut current = expr_idx;
            let mut found = None;
            for _ in 0..100 {
                let Some(ext) = self.ctx.arena.get_extended(current) else {
                    break;
                };
                if ext.parent.is_none() {
                    break;
                }
                let Some(parent) = self.ctx.arena.get(ext.parent) else {
                    break;
                };
                if parent.kind == syntax_kind_ext::CLASS_DECLARATION
                    || parent.kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    found = Some(ext.parent);
                    break;
                }
                current = ext.parent;
            }
            found
        };
        let Some(class_idx) = class_idx else {
            return false;
        };
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let Some(class_data) = self.ctx.arena.get_class(class_node) else {
            return false;
        };

        // Check constructor parameters (parameter properties)
        if let Some(ctor_idx) = class_data.members.nodes.iter().find(|&&m| {
            self.ctx
                .arena
                .get(m)
                .is_some_and(|n| n.kind == syntax_kind_ext::CONSTRUCTOR)
        }) && let Some(ctor_node) = self.ctx.arena.get(*ctor_idx)
            && let Some(ctor) = self.ctx.arena.get_constructor(ctor_node)
        {
            for &param_idx in &ctor.parameters.nodes {
                if let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    // Check if this parameter has the matching name
                    let param_name = self
                        .ctx
                        .arena
                        .get(param.name)
                        .and_then(|n| self.ctx.arena.get_identifier(n))
                        .map(|id| id.escaped_text.as_str());
                    if param_name == Some(prop_name.as_str()) {
                        // Check if the type annotation has generic construct sigs
                        if param.type_annotation.is_some() {
                            let param_type = self.get_type_from_type_node(param.type_annotation);
                            return self.type_has_generic_construct_signatures(param_type);
                        }
                    }
                }
            }
        }

        // Check regular property declarations
        for &member_idx in &class_data.members.nodes {
            if let Some(member_node) = self.ctx.arena.get(member_idx)
                && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(prop) = self.ctx.arena.get_property_decl(member_node)
                && let Some(name) = self
                    .ctx
                    .arena
                    .get(prop.name)
                    .and_then(|n| self.ctx.arena.get_identifier(n))
                && name.escaped_text == *prop_name
                && prop.type_annotation.is_some()
            {
                let prop_type = self.get_type_from_type_node(prop.type_annotation);
                return self.type_has_generic_construct_signatures(prop_type);
            }
        }

        false
    }

    /// Check if a type has construct signatures with type parameters.
    fn type_has_generic_construct_signatures(&self, type_id: TypeId) -> bool {
        if let Some(sigs) =
            crate::query_boundaries::common::construct_signatures_for_type(self.ctx.types, type_id)
        {
            return sigs.iter().any(|sig| !sig.type_params.is_empty());
        }
        // Also check callable shapes
        if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
        {
            return shape
                .construct_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty());
        }
        false
    }
}
