use crate::query_boundaries::type_computation::complex as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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
                let Some(prop_name_str) = self.get_property_name_resolved(method.name) else {
                    continue;
                };
                let prop_name_atom = self.ctx.types.intern_string(&prop_name_str);
                let rhs_type = self.get_type_of_function(elem_idx);
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
            let Some(prop_name_str) = self.get_property_name_resolved(prop.name) else {
                continue;
            };
            let prop_name_atom = self.ctx.types.intern_string(&prop_name_str);
            let rhs_type = self.get_type_of_node(prop.initializer);
            method_bindings.push((
                prop_name_atom,
                tsz_solver::PropertyInfo {
                    name: prop_name_atom,
                    type_id: rhs_type,
                    write_type: rhs_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: tsz_solver::Visibility::Public,
                    parent_id: Some(parent_sym),
                    declaration_order: 0,
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

    /// Scan sibling statements for `FuncName.prototype.X = rhs` patterns.
    /// Returns two sets:
    /// - Prototype method bindings (`method_name` -> `method_type`) to be added as instance properties
    /// - `this.prop` assignments from inside prototype method bodies (typed as T | undefined)
    #[allow(clippy::type_complexity)]
    pub(crate) fn collect_prototype_members_and_this_properties(
        &mut self,
        func_decl_idx: NodeIndex,
        func_name: &str,
        parent_sym: tsz_binder::SymbolId,
    ) -> (
        Vec<(tsz_common::interner::Atom, tsz_solver::PropertyInfo)>,
        Vec<(tsz_common::interner::Atom, tsz_solver::PropertyInfo)>,
        bool,
    ) {
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
            None => return (method_bindings, this_props, has_prototype_evidence),
        };

        // Variable-assigned function expressions sit under
        // VARIABLE_DECLARATION_LIST -> VARIABLE_STATEMENT. Walk up to the enclosing
        // statement container before scanning sibling statements for prototype writes.
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
            return (method_bindings, this_props, has_prototype_evidence);
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
            if let Some(method_name_str) =
                self.get_property_name_resolved(lhs_access.name_or_argument)
                && !is_computed_name
            {
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
            );

            for (name, prop) in method_this_props {
                this_props.push((name, prop));
            }
        }

        (method_bindings, this_props, has_prototype_evidence)
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
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };
        self.ctx.class_instance_type_cache.get(&decl_idx).copied()
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
        {
            if symbol.flags & symbol_flags::TYPE_ALIAS != 0
                && let Some(def) = self.ctx.definition_store.get(def_id)
                && let Some(body_type) = def.body
            {
                return self.type_contains_abstract_class_inner(body_type, visited);
            }
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
}
