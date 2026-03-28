//! Class member declaration and accessibility validation helpers.

use crate::context::TypingRequest;
use crate::state::{CheckerState, MemberAccessInfo, MemberAccessLevel, MemberLookup};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_async_modifier_on_declaration(
        &mut self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) {
        use crate::diagnostics::diagnostic_codes;

        if let Some(async_mod_idx) = self.find_async_modifier(modifiers) {
            self.error_at_node(
                async_mod_idx,
                "'async' modifier cannot be used here.",
                diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
            );
        }
    }

    pub(crate) fn lookup_member_access_in_class(
        &self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> MemberLookup {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return MemberLookup::NotFound;
        };
        let Some(class) = self.ctx.arena.get_class(node) else {
            return MemberLookup::NotFound;
        };

        let mut accessor_access: Option<MemberAccessLevel> = None;

        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&prop.modifiers) != is_static {
                        continue;
                    }
                    let Some(prop_name) = self.get_property_name(prop.name) else {
                        continue;
                    };
                    if prop_name == name {
                        let access_level = if self.is_private_identifier_name(prop.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&prop.modifiers)
                                .or_else(|| self.jsdoc_access_level(member_idx))
                        };
                        return match access_level {
                            Some(level) => MemberLookup::Restricted(level),
                            None => MemberLookup::Public,
                        };
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&method.modifiers) != is_static {
                        continue;
                    }
                    let Some(method_name) = self.get_property_name(method.name) else {
                        continue;
                    };
                    if method_name == name {
                        let access_level = if self.is_private_identifier_name(method.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&method.modifiers)
                                .or_else(|| self.jsdoc_access_level(member_idx))
                        };
                        return match access_level {
                            Some(level) => MemberLookup::Restricted(level),
                            None => MemberLookup::Public,
                        };
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&accessor.modifiers) != is_static {
                        continue;
                    }
                    let Some(accessor_name) = self.get_property_name(accessor.name) else {
                        continue;
                    };
                    if accessor_name == name {
                        let access_level = if self.is_private_identifier_name(accessor.name) {
                            Some(MemberAccessLevel::Private)
                        } else {
                            self.member_access_level_from_modifiers(&accessor.modifiers)
                                .or_else(|| self.jsdoc_access_level(member_idx))
                        };
                        // Don't return immediately - a getter/setter pair may have
                        // different visibility. Track the accessor access level and
                        // use the most permissive level when both are found (tsc
                        // allows reads when getter is public even if setter is private).
                        match access_level {
                            None => {
                                // No explicit modifier = public; any public accessor
                                // makes the pair publicly accessible.
                                return MemberLookup::Public;
                            }
                            Some(level) => {
                                accessor_access = Some(match accessor_access {
                                    // First accessor found, or both found — use the most permissive
                                    None | Some(MemberAccessLevel::Private) => level,
                                    Some(prev) => prev,
                                });
                            }
                        }
                    }
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    if is_static {
                        continue;
                    }
                    let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                        continue;
                    };
                    if ctor.body.is_none() {
                        continue;
                    }
                    for &param_idx in &ctor.parameters.nodes {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        if !self.has_parameter_property_modifier(&param.modifiers) {
                            continue;
                        }
                        let Some(param_name) = self.get_property_name(param.name) else {
                            continue;
                        };
                        if param_name == name {
                            return match self.member_access_level_from_modifiers(&param.modifiers) {
                                Some(level) => MemberLookup::Restricted(level),
                                None => MemberLookup::Public,
                            };
                        }
                    }
                    // In JS files, constructor body `this.x = value` assignments
                    // with JSDoc @private/@protected tags create accessible members.
                    if let Some(access) = self.lookup_ctor_this_assignment_jsdoc(ctor.body, name) {
                        return access;
                    }
                }
                _ => {}
            }
        }

        // If we found accessor(s) but didn't early-return Public, return
        // the most permissive access level across getter/setter pair.
        if let Some(level) = accessor_access {
            return MemberLookup::Restricted(level);
        }

        MemberLookup::NotFound
    }

    /// Scan constructor body for `this.name = ...` assignment statements
    /// with JSDoc `@private` / `@protected` tags (common in JS class patterns).
    ///
    /// Returns `Some(MemberLookup)` if a matching `this.name` assignment is
    /// found, using the JSDoc tag to determine access level.
    fn lookup_ctor_this_assignment_jsdoc(
        &self,
        body: NodeIndex,
        name: &str,
    ) -> Option<MemberLookup> {
        let body_node = self.ctx.arena.get(body)?;
        let block = self.ctx.arena.get_block(body_node)?;

        for &stmt_idx in &block.statements.nodes {
            let stmt_node = self.ctx.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let expr_stmt = self.ctx.arena.get_expression_statement(stmt_node)?;
            let expr_node = self.ctx.arena.get(expr_stmt.expression)?;
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let bin = self.ctx.arena.get_binary_expr(expr_node)?;
            // Must be assignment operator
            if bin.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }
            // LHS must be `this.name`
            let lhs_node = self.ctx.arena.get(bin.left)?;
            if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                continue;
            }
            let access = self.ctx.arena.get_access_expr(lhs_node)?;
            // Check that the object is `this`
            let obj_node = self.ctx.arena.get(access.expression)?;
            if obj_node.kind != SyntaxKind::ThisKeyword as u16 {
                continue;
            }
            // Check property name matches
            let prop_name_node = self.ctx.arena.get(access.name_or_argument)?;
            let prop_name = self.ctx.arena.get_identifier(prop_name_node)?;
            if prop_name.escaped_text != name {
                continue;
            }
            // Found `this.name = ...` — check JSDoc on the enclosing statement
            if let Some(level) = self.jsdoc_access_level(stmt_idx) {
                return Some(MemberLookup::Restricted(level));
            }
            // Has the assignment but no JSDoc accessibility tag → public
            return Some(MemberLookup::Public);
        }

        None
    }

    pub(crate) fn find_member_access_info(
        &self,
        class_idx: NodeIndex,
        name: &str,
        is_static: bool,
    ) -> Option<MemberAccessInfo> {
        use rustc_hash::FxHashSet;

        let mut current = class_idx;
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();

        while visited.insert(current) {
            match self.lookup_member_access_in_class(current, name, is_static) {
                MemberLookup::Restricted(level) => {
                    return Some(MemberAccessInfo {
                        level,
                        declaring_class_idx: current,
                        declaring_class_name: self
                            .get_class_name_with_type_params_from_decl(current),
                    });
                }
                MemberLookup::Public => return None,
                MemberLookup::NotFound => {
                    let base_idx = self.get_base_class_idx(current)?;
                    current = base_idx;
                }
            }
        }

        None
    }

    /// Recursively check a type node for parameter properties in function types.
    /// Function types (like `(x: T) => R` or `new (x: T) => R`) cannot have parameter properties.
    /// Walk a type node and emit TS2304 for unresolved type names inside complex types.
    /// Check type for missing names, but skip top-level `TYPE_REFERENCE` nodes.
    /// This is used when the caller will separately check `TYPE_REFERENCE` nodes
    /// to avoid duplicate error emissions.
    pub(crate) fn check_type_for_missing_names_skip_top_level_ref(&mut self, type_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        use tsz_parser::parser::syntax_kind_ext;

        // Skip TYPE_REFERENCE at top level to avoid duplicates
        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            return;
        }

        // For all other types, use the normal check
        self.check_type_for_missing_names(type_idx);
    }

    pub(crate) fn check_type_for_missing_names(&mut self, type_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };
        let factory = self.ctx.types.factory();

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                    && let Some(sym_id) = self
                        .resolve_type_symbol_for_lowering(type_ref.type_name)
                        .map(tsz_binder::SymbolId)
                    && (self.ctx.symbol_resolution_set.contains(&sym_id)
                        || self.type_alias_reaches_resolving_alias(sym_id))
                {
                    return;
                }
                let _ = self.get_type_from_type_reference(type_idx);
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                let _ = self.get_type_from_type_query(type_idx);
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(type_lit) = self.ctx.arena.get_type_literal(node) {
                    for &member_idx in &type_lit.members.nodes {
                        self.check_type_member_for_missing_names(member_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.ctx.arena.get_function_type(node) {
                    let updates =
                        self.push_missing_name_type_parameters(&func_type.type_parameters);
                    self.check_type_parameters_for_missing_names(&func_type.type_parameters);
                    self.check_duplicate_type_parameters(&func_type.type_parameters);
                    self.check_duplicate_parameters(&func_type.parameters, false);
                    for &param_idx in &func_type.parameters.nodes {
                        self.check_parameter_type_for_missing_names(param_idx);
                    }
                    let typeof_param_names =
                        self.push_typeof_params_from_ast_nodes(&func_type.parameters.nodes);
                    if func_type.type_annotation.is_some() {
                        self.check_type_for_missing_names(func_type.type_annotation);
                    }
                    self.pop_typeof_params_from_ast(typeof_param_names);
                    self.pop_type_parameters(updates);
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.ctx.arena.get_array_type(node) {
                    self.check_type_for_missing_names(arr.element_type);
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
                    for &elem_idx in &tuple.elements.nodes {
                        self.check_tuple_element_for_missing_names(elem_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.check_type_for_missing_names(wrapped.type_node);
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    for &member_idx in &composite.types.nodes {
                        self.check_type_for_missing_names(member_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    // Check check_type (infer NOT allowed here)
                    self.check_type_for_missing_names(cond.check_type);

                    // Check extends_type (infer IS allowed here — TS1338 validation)
                    self.ctx.in_conditional_extends_depth += 1;
                    self.check_type_for_missing_names(cond.extends_type);
                    self.ctx.in_conditional_extends_depth -= 1;

                    // TS2838: Check that duplicate infer type params have identical constraints
                    self.check_infer_constraint_consistency(cond.extends_type);

                    // Collect infer type parameters from extends_type and add them to scope for true_type
                    let infer_params = self.collect_infer_type_parameters(cond.extends_type);
                    let mut param_bindings = Vec::new();
                    for param_name in &infer_params {
                        let atom = self.ctx.types.intern_string(param_name);
                        let type_id = factory.type_param(tsz_solver::TypeParamInfo {
                            name: atom,
                            constraint: None,
                            default: None,
                            is_const: false,
                        });
                        let previous = self
                            .ctx
                            .type_parameter_scope
                            .insert(param_name.clone(), type_id);
                        param_bindings.push((param_name.clone(), previous));
                    }

                    // Check true_type with infer type parameters in scope
                    self.check_type_for_missing_names(cond.true_type);

                    // Remove infer type parameters from scope
                    for (name, previous) in param_bindings.into_iter().rev() {
                        if let Some(prev_type) = previous {
                            self.ctx.type_parameter_scope.insert(name, prev_type);
                        } else {
                            self.ctx.type_parameter_scope.remove(&name);
                        }
                    }

                    // Check false_type (infer type params not in scope)
                    self.check_type_for_missing_names(cond.false_type);
                }
            }
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.ctx.arena.get_infer_type(node) {
                    // TS1338: 'infer' declarations are only permitted in the 'extends'
                    // clause of a conditional type.
                    if self.ctx.in_conditional_extends_depth == 0 {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.error_at_node(
                            type_idx,
                            diagnostic_messages::INFER_DECLARATIONS_ARE_ONLY_PERMITTED_IN_THE_EXTENDS_CLAUSE_OF_A_CONDITIONAL_TYP,
                            diagnostic_codes::INFER_DECLARATIONS_ARE_ONLY_PERMITTED_IN_THE_EXTENDS_CLAUSE_OF_A_CONDITIONAL_TYP,
                        );
                    }
                    self.check_type_parameter_node_for_missing_names(infer.type_parameter);
                }
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(op) = self.ctx.arena.get_type_operator(node) {
                    // TS1354: 'readonly' type modifier is only permitted on array and tuple literal types.
                    if op.operator == tsz_scanner::SyntaxKind::ReadonlyKeyword as u16
                        && let Some(operand_node) = self.ctx.arena.get(op.type_node)
                        && operand_node.kind != syntax_kind_ext::ARRAY_TYPE
                        && operand_node.kind != syntax_kind_ext::TUPLE_TYPE
                    {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.ctx.error(
                            node.pos,
                            node.end.saturating_sub(node.pos),
                            diagnostic_messages::READONLY_TYPE_MODIFIER_IS_ONLY_PERMITTED_ON_ARRAY_AND_TUPLE_LITERAL_TYPES.to_string(),
                            diagnostic_codes::READONLY_TYPE_MODIFIER_IS_ONLY_PERMITTED_ON_ARRAY_AND_TUPLE_LITERAL_TYPES,
                        );
                    }
                    self.check_type_for_missing_names(op.type_node);
                }
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.check_type_for_missing_names(indexed.object_type);
                    self.check_type_for_missing_names(indexed.index_type);
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                    // TS7039: Mapped object type implicitly has an 'any' template type.
                    if self.ctx.no_implicit_any() && mapped.type_node.is_none() {
                        let pos = node.pos;
                        let len = node.end.saturating_sub(node.pos);
                        self.ctx.error(
                            pos,
                            len,
                            "Mapped object type implicitly has an 'any' template type.".to_string(),
                            7039,
                        );
                    }
                    let mut param_binding: Option<(String, Option<TypeId>)> = None;
                    if let Some(param_node) = self.ctx.arena.get(mapped.type_parameter)
                        && let Some(param) = self.ctx.arena.get_type_parameter(param_node)
                        && let Some(name_node) = self.ctx.arena.get(param.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        let name = ident.escaped_text.clone();
                        let atom = self.ctx.types.intern_string(&name);
                        let type_id = factory.type_param(tsz_solver::TypeParamInfo {
                            name: atom,
                            constraint: None,
                            default: None,
                            is_const: false,
                        });
                        let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
                        param_binding = Some((name, previous));
                    }
                    let is_direct_self_constraint = param_binding
                        .as_ref()
                        .and_then(|(name, _)| {
                            let param_node = self.ctx.arena.get(mapped.type_parameter)?;
                            let param = self.ctx.arena.get_type_parameter(param_node)?;
                            let constraint = param.constraint;
                            let constraint_text = self.node_text(constraint)?.trim().to_string();
                            if constraint_text == name.as_str() {
                                Some(())
                            } else {
                                None
                            }
                        })
                        .is_some();

                    // TS2313: `P in P` and equivalent direct self-constraint cases are
                    // reported in mapped-constraint checking, which runs during type-node
                    // validation (`check_type_node`). Skip constraint-name missing-name checks
                    // here to avoid surfacing a secondary TS2304.
                    if !is_direct_self_constraint {
                        self.check_type_parameter_node_for_missing_names(mapped.type_parameter);
                    }
                    if mapped.name_type.is_some() {
                        self.check_type_for_missing_names(mapped.name_type);
                    }
                    if mapped.type_node.is_some() {
                        self.check_type_for_missing_names(mapped.type_node);
                    } else if self.ctx.no_implicit_any() {
                        // TS7039: Mapped object type implicitly has an 'any' template type
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.error_at_node(
                            type_idx,
                            diagnostic_messages::MAPPED_OBJECT_TYPE_IMPLICITLY_HAS_AN_ANY_TEMPLATE_TYPE,
                            diagnostic_codes::MAPPED_OBJECT_TYPE_IMPLICITLY_HAS_AN_ANY_TEMPLATE_TYPE,
                        );
                    }
                    if let Some(ref members) = mapped.members {
                        for &member_idx in &members.nodes {
                            self.check_type_member_for_missing_names(member_idx);
                        }
                    }
                    if let Some((name, previous)) = param_binding {
                        if let Some(prev_type) = previous {
                            self.ctx.type_parameter_scope.insert(name, prev_type);
                        } else {
                            self.ctx.type_parameter_scope.remove(&name);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                if let Some(pred) = self.ctx.arena.get_type_predicate(node)
                    && pred.type_node.is_some()
                {
                    self.check_type_for_missing_names(pred.type_node);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(template) = self.ctx.arena.get_template_literal_type(node) {
                    for &span_idx in &template.template_spans.nodes {
                        let Some(span_node) = self.ctx.arena.get(span_idx) else {
                            continue;
                        };
                        let Some(span) = self.ctx.arena.get_template_span(span_node) else {
                            continue;
                        };
                        self.check_type_for_missing_names(span.expression);
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn push_missing_name_type_parameters(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) -> Vec<(String, Option<TypeId>, bool)> {
        use tsz_solver::TypeParamInfo;

        let Some(list) = type_parameters else {
            return Vec::new();
        };

        let factory = self.ctx.types.factory();
        let mut updates = Vec::new();
        for &param_idx in &list.nodes {
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
            let name = ident.escaped_text.clone();
            let atom = self.ctx.types.intern_string(&name);
            let type_id = factory.type_param(TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
                is_const: false,
            });
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous, false));
        }

        updates
    }

    /// Push parameter names from an AST `Option<NodeList>` (signature parameters) into
    /// `typeof_param_scope` so that `typeof paramName` in return types resolves without TS2304.
    /// Returns the names pushed so they can be popped later.
    fn push_typeof_params_from_ast_params(
        &mut self,
        params: &Option<tsz_parser::parser::NodeList>,
    ) -> Vec<String> {
        let Some(list) = params else {
            return Vec::new();
        };
        self.push_typeof_params_from_ast_nodes(&list.nodes)
    }

    /// Push parameter names from a slice of parameter `NodeIndex` values into `typeof_param_scope`.
    fn push_typeof_params_from_ast_nodes(&mut self, nodes: &[NodeIndex]) -> Vec<String> {
        let mut names = Vec::new();
        for &param_idx in nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            // Only handle simple identifier binding names (covers the common case).
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                continue;
            };
            let name = ident.escaped_text.clone();
            self.ctx
                .typeof_param_scope
                .insert(name.clone(), TypeId::ANY);
            names.push(name);
        }
        names
    }

    /// Pop parameter names previously pushed by `push_typeof_params_from_ast_*`.
    fn pop_typeof_params_from_ast(&mut self, names: Vec<String>) {
        for name in names {
            self.ctx.typeof_param_scope.remove(&name);
        }
    }

    pub(crate) fn check_type_member_for_missing_names(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        if let Some(sig) = self.ctx.arena.get_signature(member_node) {
            self.check_computed_property_name(sig.name);

            let updates = self.push_missing_name_type_parameters(&sig.type_parameters);
            self.check_type_parameters_for_missing_names(&sig.type_parameters);
            self.check_duplicate_type_parameters(&sig.type_parameters);
            if let Some(ref params) = sig.parameters {
                self.check_duplicate_parameters(params, false);
                for &param_idx in &params.nodes {
                    self.check_parameter_type_for_missing_names(param_idx);
                }
            }
            // Push parameter names into typeof_param_scope so that `typeof paramName`
            // in return type annotations can resolve without emitting TS2304.
            let typeof_param_names = self.push_typeof_params_from_ast_params(&sig.parameters);
            if sig.type_annotation.is_some() {
                self.check_type_for_missing_names(sig.type_annotation);
            }
            self.pop_typeof_params_from_ast(typeof_param_names);
            self.pop_type_parameters(updates);
            return;
        }

        if let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) {
            for &param_idx in &index_sig.parameters.nodes {
                self.check_parameter_type_for_missing_names(param_idx);
            }
            if index_sig.type_annotation.is_some() {
                self.check_type_for_missing_names(index_sig.type_annotation);
            }
        }
    }

    /// Check a type literal member for parameter properties (call/construct signatures).
    pub(crate) fn check_type_member_for_parameter_properties(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        // Check call signatures and construct signatures for parameter properties
        if node.kind == syntax_kind_ext::CALL_SIGNATURE
            || node.kind == syntax_kind_ext::CONSTRUCT_SIGNATURE
        {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                if let Some(params) = &sig.parameters {
                    self.check_strict_mode_reserved_parameter_names(
                        &params.nodes,
                        member_idx,
                        false,
                    );
                    self.check_parameter_properties(&params.nodes);
                    // TS2371: Parameter initializers not allowed in call/construct signatures
                    self.check_non_impl_parameter_initializers(&params.nodes, false, false);
                    for (pi, &param_idx) in params.nodes.iter().enumerate() {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            if param.type_annotation.is_some() {
                                self.check_type_for_parameter_properties(param.type_annotation);
                            }
                            self.maybe_report_implicit_any_parameter(param, false, pi);
                        }
                    }
                }
                // Recursively check the return type
                self.check_type_for_parameter_properties(sig.type_annotation);

                // TS7013/TS7020: Check for implicit any return type on construct/call signatures
                if self.ctx.no_implicit_any() && sig.type_annotation.is_none() {
                    use crate::diagnostics::diagnostic_codes;
                    if node.kind == syntax_kind_ext::CONSTRUCT_SIGNATURE {
                        self.error_at_node(
                            member_idx,
                            "Construct signature, which lacks return-type annotation, implicitly has an 'any' return type.",
                            diagnostic_codes::CONSTRUCT_SIGNATURE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_ANY_RET,
                        );
                    } else {
                        self.error_at_node(
                            member_idx,
                            "Call signature, which lacks return-type annotation, implicitly has an 'any' return type.",
                            diagnostic_codes::CALL_SIGNATURE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_ANY_RETURN_T,
                        );
                    }
                }
            }
        }
        // Check method signatures in type literals
        else if node.kind == syntax_kind_ext::METHOD_SIGNATURE {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                // Push method type parameters so they are in scope when
                // resolving return type annotations (e.g., groupBy<T>(): { [k: string]: T[] })
                let (_, type_param_updates) = self.push_type_parameters(&sig.type_parameters);
                if let Some(params) = &sig.parameters {
                    self.check_strict_mode_reserved_parameter_names(
                        &params.nodes,
                        member_idx,
                        false,
                    );
                    self.check_parameter_properties(&params.nodes);
                    // TS2371: Parameter initializers not allowed in method signatures
                    self.check_non_impl_parameter_initializers(&params.nodes, false, false);
                    for (pi, &param_idx) in params.nodes.iter().enumerate() {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            if param.type_annotation.is_some() {
                                self.check_type_for_parameter_properties(param.type_annotation);
                            }
                            self.maybe_report_implicit_any_parameter(param, false, pi);
                        }
                    }
                }
                self.check_type_for_parameter_properties(sig.type_annotation);
                self.pop_type_parameters(type_param_updates);
                if self.ctx.no_implicit_any()
                    && sig.type_annotation.is_none()
                    && let Some(name) = self.property_name_for_error(sig.name)
                {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        sig.name,
                        diagnostic_codes::WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
                        &[&name, "any"],
                    );
                }
            }
        }
        // Check property signatures for implicit any (error 7008)
        else if node.kind == syntax_kind_ext::PROPERTY_SIGNATURE {
            if let Some(sig) = self.ctx.arena.get_signature(node) {
                if sig.type_annotation.is_some() {
                    self.check_type_for_parameter_properties(sig.type_annotation);
                }
                // Property signature without type annotation implicitly has 'any' type
                // Only emit TS7008 when noImplicitAny is enabled
                if self.ctx.no_implicit_any()
                    && sig.type_annotation.is_none()
                    && let Some(member_name) = self.get_member_name_display_text(sig.name)
                {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        sig.name,
                        diagnostic_codes::MEMBER_IMPLICITLY_HAS_AN_TYPE,
                        &[&member_name, "any"],
                    );
                }
            }
        }
        // Check accessors in type literals/interfaces - cannot have body (error 1183)
        else if (node.kind == syntax_kind_ext::GET_ACCESSOR
            || node.kind == syntax_kind_ext::SET_ACCESSOR)
            && let Some(accessor) = self.ctx.arena.get_accessor(node)
        {
            // Accessors in type literals and interfaces cannot have implementations
            if accessor.body.is_some() {
                use crate::diagnostics::diagnostic_codes;
                // Report error on the body
                self.error_at_node(
                    accessor.body,
                    "An implementation cannot be declared in ambient contexts.",
                    diagnostic_codes::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
                );
            }
        }
    }

    /// Check that all method/constructor overload signatures have implementations.
    /// Reports errors 2389, 2390, 2391, 1042.
    pub(crate) fn check_class_member_implementations(&mut self, members: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;

        let mut i = 0;
        while i < members.len() {
            let member_idx = members[i];
            let Some(node) = self.ctx.arena.get(member_idx) else {
                i += 1;
                continue;
            };

            match node.kind {
                // TS1042: 'async' modifier cannot be used on getters/setters
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(node)
                        && self.has_async_modifier(&accessor.modifiers)
                    {
                        self.error_at_node(
                            member_idx,
                            "'async' modifier cannot be used here.",
                            diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
                        );
                    }
                }
                syntax_kind_ext::CONSTRUCTOR => {
                    // Skip constructor overload checks when the file has parse errors.
                    // Malformed constructors (e.g., `constructor` without parentheses)
                    // produce TS1005 from the parser, and tsc does not additionally
                    // emit TS2390 in these cases.
                    if self.has_parse_errors() {
                        i += 1;
                        continue;
                    }
                    if let Some(ctor) = self.ctx.arena.get_constructor(node)
                        && ctor.body.is_none()
                    {
                        // Constructor overload signature - check for implementation
                        let has_impl = self.find_constructor_impl(members, i + 1);
                        if !has_impl {
                            self.error_at_node(
                                member_idx,
                                "Constructor implementation is missing.",
                                diagnostic_codes::CONSTRUCTOR_IMPLEMENTATION_IS_MISSING,
                            );
                        }
                    }
                }
                syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(node) {
                        let flags = u32::from(node.flags);
                        if (flags & node_flags::THIS_NODE_HAS_ERROR) != 0
                            || (flags & node_flags::THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR) != 0
                        {
                            continue;
                        }
                        // Abstract methods don't need implementations (they're meant for derived classes).
                        // Optional methods (g?(): T) also don't need implementations —
                        // they are standalone declarations, not overload signatures.
                        let is_abstract = self.has_abstract_modifier(&method.modifiers);
                        let is_declare = self.has_declare_modifier(&method.modifiers);
                        if method.body.is_none()
                            && !is_abstract
                            && !is_declare
                            && !method.question_token
                        {
                            // Method overload signature - check for implementation.
                            // TSC only reports TS2391 on the LAST overload in a consecutive
                            // group with the same name, so skip ahead to find it.
                            let method_name = self.get_method_name_from_node(member_idx);
                            if let Some(name) = method_name {
                                // Advance past consecutive bodyless method overloads with the same name.
                                let mut last_overload_i = i;
                                let mut j = i + 1;
                                while j < members.len() {
                                    let next_idx = members[j];
                                    let Some(next_node) = self.ctx.arena.get(next_idx) else {
                                        break;
                                    };
                                    if next_node.kind == syntax_kind_ext::METHOD_DECLARATION
                                        && let Some(next_method) =
                                            self.ctx.arena.get_method_decl(next_node)
                                        && next_method.body.is_none()
                                    {
                                        let next_name = self.get_method_name_from_node(next_idx);
                                        if next_name.as_deref() == Some(name.as_str()) {
                                            last_overload_i = j;
                                            j += 1;
                                            continue;
                                        }
                                    }
                                    break;
                                }

                                // Report at the last overload in the group
                                let report_member_idx = members[last_overload_i];
                                let report_error_node = self
                                    .ctx
                                    .arena
                                    .get(report_member_idx)
                                    .and_then(|n| self.ctx.arena.get_method_decl(n))
                                    .map(|m| m.name)
                                    .filter(|n| n.is_some())
                                    .unwrap_or(report_member_idx);

                                let (has_impl, impl_name, impl_idx) =
                                    self.find_method_impl(members, last_overload_i + 1, &name);
                                if !has_impl {
                                    self.error_at_node(
                                        report_error_node,
                                        "Function implementation is missing or not immediately following the declaration.",
                                        diagnostic_codes::FUNCTION_IMPLEMENTATION_IS_MISSING_OR_NOT_IMMEDIATELY_FOLLOWING_THE_DECLARATION
                                    );
                                } else if let Some(actual_name) = impl_name
                                    && actual_name != name
                                {
                                    // Implementation has wrong name — report at the
                                    // implementation's name node, and only on the last
                                    // overload (the one immediately preceding the implementation).
                                    let impl_member_idx = impl_idx.unwrap_or(last_overload_i + 1);
                                    if impl_member_idx == last_overload_i + 1 {
                                        let impl_node_idx = members[impl_member_idx];
                                        let expected_display = self
                                            .get_method_name_for_diagnostic(report_member_idx)
                                            .unwrap_or_else(|| name.clone());
                                        let impl_error_node = self
                                            .ctx
                                            .arena
                                            .get(impl_node_idx)
                                            .and_then(|n| self.ctx.arena.get_method_decl(n))
                                            .map(|m| m.name)
                                            .filter(|n| n.is_some())
                                            .unwrap_or(impl_node_idx);
                                        self.error_at_node(
                                            impl_error_node,
                                            &format!(
                                                "Function implementation name must be '{expected_display}'."
                                            ),
                                            diagnostic_codes::FUNCTION_IMPLEMENTATION_NAME_MUST_BE,
                                        );
                                    }
                                }
                                // Skip past all overloads we already processed
                                i = last_overload_i + 1;
                                continue;
                            }
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// Check that consecutive method declarations with the same name have consistent
    /// static/instance modifiers (TS2387/TS2388).
    ///
    /// TSC rule: for each consecutive pair of same-name method declarations within
    /// an overload group, if their static-ness differs, emit an error on the second:
    /// - TS2387 if it's instance but should be static
    /// - TS2388 if it's static but shouldn't be
    ///
    /// An overload group ends when we encounter an implementation (method with body).
    /// After an implementation, the next declaration starts a new group even if
    /// it has the same name.
    pub(crate) fn check_static_instance_overload_consistency(&mut self, members: &[NodeIndex]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let mut prev_name: Option<String> = None;
        let mut prev_is_static = false;
        let mut prev_had_body = false;

        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                prev_name = None;
                prev_had_body = false;
                continue;
            };

            if node.kind != syntax_kind_ext::METHOD_DECLARATION {
                prev_name = None;
                prev_had_body = false;
                continue;
            }

            let Some(method) = self.ctx.arena.get_method_decl(node) else {
                prev_name = None;
                prev_had_body = false;
                continue;
            };

            let cur_name = self.get_method_name_from_node(member_idx);
            let cur_is_static = self.has_static_modifier(&method.modifiers);
            let cur_has_body = method.body.is_some();

            // Only compare within the same overload group.
            // After an implementation (body), start a new group.
            if !prev_had_body
                && let (Some(prev), Some(cur)) = (&prev_name, &cur_name)
                && prev == cur
                && cur_is_static != prev_is_static
            {
                let error_node = if method.name.is_some() {
                    method.name
                } else {
                    member_idx
                };
                if cur_is_static {
                    self.error_at_node(
                        error_node,
                        diagnostic_messages::FUNCTION_OVERLOAD_MUST_NOT_BE_STATIC,
                        diagnostic_codes::FUNCTION_OVERLOAD_MUST_NOT_BE_STATIC,
                    );
                } else {
                    self.error_at_node(
                        error_node,
                        diagnostic_messages::FUNCTION_OVERLOAD_MUST_BE_STATIC,
                        diagnostic_codes::FUNCTION_OVERLOAD_MUST_BE_STATIC,
                    );
                }
            }

            prev_name = cur_name;
            prev_is_static = cur_is_static;
            prev_had_body = cur_has_body;
        }
    }

    /// Report an error at a specific node.
    /// Check an expression node for TS1359: await outside async function.
    /// Recursively checks the expression tree for await expressions.
    /// Report an error with context about a related symbol.
    /// Check a class member (property, method, constructor, accessor).
    pub(crate) fn check_class_member(&mut self, member_idx: NodeIndex) {
        self.check_class_member_with_request(member_idx, &TypingRequest::NONE);
    }

    pub(crate) fn check_class_member_with_request(
        &mut self,
        member_idx: NodeIndex,
        request: &TypingRequest,
    ) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        let mut pushed_this = false;
        if let Some(this_type) = self.class_member_this_type(member_idx) {
            self.ctx.this_type_stack.push(this_type);
            pushed_this = true;
        }

        let is_static_member = match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .ctx
                .arena
                .get_property_decl(node)
                .is_some_and(|decl| self.has_static_modifier(&decl.modifiers)),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .is_some_and(|decl| self.has_static_modifier(&decl.modifiers)),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .ctx
                .arena
                .get_accessor(node)
                .is_some_and(|decl| self.has_static_modifier(&decl.modifiers)),
            k if k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => true,
            _ => false,
        };

        let prev_in_static_member = self
            .ctx
            .enclosing_class
            .as_ref()
            .map(|c| c.in_static_member)
            .unwrap_or(false);

        if let Some(ref mut class_info) = self.ctx.enclosing_class {
            class_info.in_static_member = is_static_member;
        }

        self.check_class_member_name(member_idx);
        self.check_class_member_decorator_expressions(member_idx);

        // TS2302: Static members cannot reference class type parameters
        self.check_static_member_for_class_type_param_refs(member_idx);

        match node.kind {
            syntax_kind_ext::PROPERTY_DECLARATION => {
                self.check_property_declaration_with_request(member_idx, request);
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                self.check_method_declaration_with_request(member_idx, request);
            }
            syntax_kind_ext::CONSTRUCTOR => {
                self.check_constructor_declaration_with_request(member_idx, request);
            }
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                self.check_accessor_declaration_with_request(member_idx, request);
            }
            syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                // TS2729: Check for use-before-init of static properties in static blocks
                self.check_static_block_initialization_order(member_idx);

                // Static blocks contain statements that must be type-checked
                if let Some(block) = self.ctx.arena.get_block(node) {
                    let prev_unreachable = self.ctx.is_unreachable;
                    let prev_reported = self.ctx.has_reported_unreachable;
                    let saved_cf_context = (
                        self.ctx.iteration_depth,
                        self.ctx.switch_depth,
                        self.ctx.label_stack.len(),
                        self.ctx.had_outer_loop,
                    );
                    if self.ctx.iteration_depth > 0
                        || self.ctx.switch_depth > 0
                        || self.ctx.had_outer_loop
                    {
                        self.ctx.had_outer_loop = true;
                    }
                    self.ctx.iteration_depth = 0;
                    self.ctx.switch_depth = 0;
                    self.ctx.function_depth += 1;
                    // Check each statement in the block
                    for &stmt_idx in &block.statements.nodes {
                        let body_request = request.read().contextual_opt(None);
                        self.check_statement_with_request(stmt_idx, &body_request);
                        if !self.statement_falls_through(stmt_idx) {
                            self.ctx.is_unreachable = true;
                        }
                    }
                    self.ctx.iteration_depth = saved_cf_context.0;
                    self.ctx.switch_depth = saved_cf_context.1;
                    self.ctx.function_depth -= 1;
                    self.ctx.label_stack.truncate(saved_cf_context.2);
                    self.ctx.had_outer_loop = saved_cf_context.3;
                    self.ctx.is_unreachable = prev_unreachable;
                    self.ctx.has_reported_unreachable = prev_reported;
                }
            }
            syntax_kind_ext::INDEX_SIGNATURE => {
                // Index signatures are metadata used during type resolution, not members
                // with their own types. They're handled separately by get_index_signatures.
                // TS1071: Accessibility modifiers cannot appear on index signatures.
                if let Some(index_sig) = self.ctx.arena.get_index_signature(node)
                    && let Some(ref mods) = index_sig.modifiers
                {
                    use crate::diagnostics::diagnostic_codes;
                    use tsz_scanner::SyntaxKind;
                    for &mod_idx in &mods.nodes {
                        if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                            let modifier_name = match mod_node.kind {
                                k if k == SyntaxKind::PublicKeyword as u16 => Some("public"),
                                k if k == SyntaxKind::PrivateKeyword as u16 => Some("private"),
                                k if k == SyntaxKind::ProtectedKeyword as u16 => Some("protected"),
                                k if k == SyntaxKind::ExportKeyword as u16 => Some("export"),
                                _ => None,
                            };
                            if let Some(name) = modifier_name {
                                self.error_at_node(
                                    mod_idx,
                                    &format!(
                                        "'{name}' modifier cannot appear on an index signature."
                                    ),
                                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_AN_INDEX_SIGNATURE,
                                );
                            }
                        }
                    }
                }
            }
            _ => {
                // Other class member types (semicolons, etc.)
                self.get_type_of_node(member_idx);
            }
        }

        if pushed_this {
            self.ctx.this_type_stack.pop();
        }

        if let Some(ref mut class_info) = self.ctx.enclosing_class {
            class_info.in_static_member = prev_in_static_member;
        }
    }

    fn check_class_member_decorator_expressions(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        // Fast path: skip all decorator-related work when the member has no decorators.
        // This avoids expensive AST extraction and modifier analysis for the common case.
        {
            let has_any_decorator = match node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                    .ctx
                    .arena
                    .get_property_decl(node)
                    .and_then(|d| d.modifiers.as_ref())
                    .is_some_and(|m| {
                        m.nodes.iter().any(|&idx| {
                            self.ctx
                                .arena
                                .get(idx)
                                .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                        })
                    }),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(node)
                    .and_then(|d| d.modifiers.as_ref())
                    .is_some_and(|m| {
                        m.nodes.iter().any(|&idx| {
                            self.ctx
                                .arena
                                .get(idx)
                                .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                        })
                    }),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.ctx
                        .arena
                        .get_accessor(node)
                        .and_then(|d| d.modifiers.as_ref())
                        .is_some_and(|m| {
                            m.nodes.iter().any(|&idx| {
                                self.ctx
                                    .arena
                                    .get(idx)
                                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                            })
                        })
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => self
                    .ctx
                    .arena
                    .get_constructor(node)
                    .and_then(|d| d.modifiers.as_ref())
                    .is_some_and(|m| {
                        m.nodes.iter().any(|&idx| {
                            self.ctx
                                .arena
                                .get(idx)
                                .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                        })
                    }),
                _ => false,
            };

            // Also need to check constructor parameter decorators
            let has_param_decorator = match node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(node)
                    .is_some_and(|d| self.any_parameter_has_decorator(&d.parameters.nodes)),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.ctx
                        .arena
                        .get_accessor(node)
                        .is_some_and(|d| self.any_parameter_has_decorator(&d.parameters.nodes))
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => self
                    .ctx
                    .arena
                    .get_constructor(node)
                    .is_some_and(|d| self.any_parameter_has_decorator(&d.parameters.nodes)),
                _ => false,
            };

            if !has_any_decorator && !has_param_decorator {
                return;
            }
        }

        let (modifiers, parameters, member_name_idx) = match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .ctx
                .arena
                .get_property_decl(node)
                .map_or((None, None, NodeIndex::NONE), |decl| {
                    (decl.modifiers.as_ref(), None, decl.name)
                }),
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .map_or((None, None, NodeIndex::NONE), |decl| {
                    (decl.modifiers.as_ref(), Some(&decl.parameters), decl.name)
                }),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .ctx
                .arena
                .get_accessor(node)
                .map_or((None, None, NodeIndex::NONE), |decl| {
                    (decl.modifiers.as_ref(), Some(&decl.parameters), decl.name)
                }),
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                self.ctx
                    .arena
                    .get_constructor(node)
                    .map_or((None, None, NodeIndex::NONE), |decl| {
                        (
                            decl.modifiers.as_ref(),
                            Some(&decl.parameters),
                            NodeIndex::NONE,
                        )
                    })
            }
            _ => (None, None, NodeIndex::NONE),
        };

        let is_abstract = modifiers.is_some_and(|m| {
            m.nodes.iter().any(|&mod_idx| {
                self.ctx
                    .arena
                    .get(mod_idx)
                    .is_some_and(|n| n.kind == SyntaxKind::AbstractKeyword as u16)
            })
        });

        let is_ambient = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| c.is_declared)
            || modifiers.is_some_and(|m| {
                m.nodes.iter().any(|&n| {
                    self.ctx
                        .arena
                        .get(n)
                        .is_some_and(|n| n.kind == SyntaxKind::DeclareKeyword as u16)
                })
            });

        let is_ambient_field = is_ambient && node.kind == syntax_kind_ext::PROPERTY_DECLARATION;

        // With --experimentalDecorators, decorators on private-named members
        // and members of class expressions are not valid (TS1206).
        let is_private_member =
            member_name_idx != NodeIndex::NONE && self.is_private_identifier_name(member_name_idx);
        let is_class_expression_member = self.ctx.enclosing_class.as_ref().is_some_and(|c| {
            self.ctx
                .arena
                .get(c.class_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_EXPRESSION)
        });
        let legacy_decorator_not_valid = self.ctx.compiler_options.experimental_decorators
            && (is_private_member || is_class_expression_member);

        if let Some(modifiers) = modifiers {
            for &modifier_idx in &modifiers.nodes {
                let Some(modifier_node) = self.ctx.arena.get(modifier_idx) else {
                    continue;
                };
                if modifier_node.kind != syntax_kind_ext::DECORATOR {
                    continue;
                }

                if is_abstract
                    || (!self.ctx.compiler_options.experimental_decorators && is_ambient_field)
                    || legacy_decorator_not_valid
                {
                    use crate::diagnostics::diagnostic_codes;
                    if is_abstract && node.kind == syntax_kind_ext::METHOD_DECLARATION {
                        self.error_at_node(
                            modifier_idx,
                            "A decorator can only decorate a method implementation, not an overload.",
                            diagnostic_codes::A_DECORATOR_CAN_ONLY_DECORATE_A_METHOD_IMPLEMENTATION_NOT_AN_OVERLOAD,
                        );
                    } else {
                        self.error_at_node(
                            modifier_idx,
                            "Decorators are not valid here.",
                            diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                        );
                    }
                }

                let Some(decorator) = self.ctx.arena.get_decorator(modifier_node) else {
                    continue;
                };

                // TS1497: Check decorator expression grammar
                self.check_grammar_decorator(decorator.expression);

                let decorator_type = self.compute_type_of_node(decorator.expression);

                // TS1329: Check if the decorator accepts too few arguments for this position.
                // For experimental decorators on methods/accessors, the decorator is called
                // with 3 arguments (target, propertyKey, descriptor). If the decorator has
                // call signatures but none can accept 3 args, it's likely a factory that
                // should be called first: @dec() instead of @dec.
                if self.ctx.compiler_options.experimental_decorators
                    && !is_abstract
                    && !legacy_decorator_not_valid
                    && (node.kind == syntax_kind_ext::METHOD_DECLARATION
                        || node.kind == syntax_kind_ext::GET_ACCESSOR
                        || node.kind == syntax_kind_ext::SET_ACCESSOR)
                {
                    self.check_method_decorator_arity(
                        decorator.expression,
                        decorator_type,
                        modifier_idx,
                    );
                }
            }
        }

        if let Some(parameters) = parameters {
            for &param_idx in &parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if let Some(param_modifiers) = &param.modifiers {
                    for &modifier_idx in &param_modifiers.nodes {
                        let Some(modifier_node) = self.ctx.arena.get(modifier_idx) else {
                            continue;
                        };
                        if modifier_node.kind != syntax_kind_ext::DECORATOR {
                            continue;
                        }

                        if !self.ctx.compiler_options.experimental_decorators {
                            use crate::diagnostics::diagnostic_codes;
                            self.error_at_node(
                                modifier_idx,
                                "Decorators are not valid here.",
                                diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                            );
                        }

                        if let Some(decorator) = self.ctx.arena.get_decorator(modifier_node) {
                            // TS1497: Check decorator expression grammar
                            self.check_grammar_decorator(decorator.expression);

                            self.get_type_of_node(decorator.expression);

                            // TS1308: Check for await expressions in decorator arguments.
                            // Decorator arguments are evaluated in the enclosing scope,
                            // not the decorated method's scope. An await in a non-async
                            // enclosing function should trigger TS1308.
                            self.check_await_expression(decorator.expression);
                        }
                    }
                }
            }
        }
    }

    /// Quick scan to check if any parameter in a parameter list has a decorator modifier.
    fn any_parameter_has_decorator(&self, params: &[NodeIndex]) -> bool {
        for &param_idx in params {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            if let Some(ref mods) = param.modifiers {
                for &mod_idx in &mods.nodes {
                    if self
                        .ctx
                        .arena
                        .get(mod_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// TS1329: Check if a method/accessor decorator accepts too few arguments.
    ///
    /// For experimental decorators, method/accessor decorators are called as
    /// `decorator(target, propertyKey, descriptor)` — 3 arguments.
    /// If the decorator expression has call signatures but none can accept 3 args,
    /// emit TS1329 suggesting to call it first: `@dec()` instead of `@dec`.
    fn check_method_decorator_arity(
        &mut self,
        decorator_expr: NodeIndex,
        decorator_type: TypeId,
        decorator_node: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Skip validation for error/any/unknown types
        if decorator_type == TypeId::ERROR
            || decorator_type == TypeId::ANY
            || decorator_type == TypeId::UNKNOWN
        {
            return;
        }

        // Try multiple approaches to get call signatures:
        // 1. Direct function shape (works for simple function types)
        // 2. Call signatures query (works for overloaded/complex types)
        let has_too_few_args = if let Some(shape) =
            crate::query_boundaries::class_type::function_shape(self.ctx.types, decorator_type)
        {
            shape.params.is_empty()
        } else if let Some(callable) = crate::query_boundaries::class_type::callable_shape_for_type(
            self.ctx.types,
            decorator_type,
        ) {
            // Check if ALL call signatures accept zero args (decorator factory pattern)
            !callable.call_signatures.is_empty()
                && callable
                    .call_signatures
                    .iter()
                    .all(|sig| sig.params.is_empty())
        } else {
            false
        };

        if has_too_few_args {
            let name = self.get_decorator_expression_name(decorator_expr);
            let msg = diagnostic_messages::ACCEPTS_TOO_FEW_ARGUMENTS_TO_BE_USED_AS_A_DECORATOR_HERE_DID_YOU_MEAN_TO_CALL_IT
                .replace("{0}", &name);
            self.error_at_node(
                decorator_node,
                &msg,
                diagnostic_codes::ACCEPTS_TOO_FEW_ARGUMENTS_TO_BE_USED_AS_A_DECORATOR_HERE_DID_YOU_MEAN_TO_CALL_IT,
            );
        }
    }

    /// Get the text name of a decorator expression for error messages.
    /// For simple identifiers like `dec`, returns "dec".
    /// For member access like `a.b`, returns "a.b".
    /// Falls back to "decorator" if the name can't be determined.
    fn get_decorator_expression_name(&self, expr: NodeIndex) -> String {
        if let Some(node) = self.ctx.arena.get(expr)
            && let Some(ident) = self.ctx.arena.get_identifier(node)
        {
            return ident.escaped_text.to_string();
        }
        "decorator".to_string()
    }

    /// TS2838: Check that all `infer X` declarations with the same name in a
    /// conditional type's extends clause have identical constraints.
    ///
    /// For example, `T extends { a: infer U extends string, b: infer U extends number }`
    /// should emit TS2838 because `U` has constraints `string` and `number`.
    fn check_infer_constraint_consistency(&mut self, extends_type: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;
        use std::collections::HashMap;

        let infer_decls = self.collect_infer_type_params_with_constraints(extends_type);
        if infer_decls.len() < 2 {
            return;
        }

        // Group by name: collect all (constraint, type_param_node) for each infer name
        let mut groups: HashMap<String, Vec<[NodeIndex; 2]>> = HashMap::new();
        for (name, constraint, tp_node) in &infer_decls {
            groups
                .entry(name.clone())
                .or_default()
                .push([*constraint, *tp_node]);
        }

        // For each duplicate group, check constraint consistency.
        // Only declarations with EXPLICIT constraints participate — unconstrained
        // `infer U` declarations inherit from the constrained ones (TSC behavior).
        for (name, entries) in &groups {
            if entries.len() < 2 {
                continue;
            }

            // Collect only entries that have an explicit constraint
            let constrained: Vec<(TypeId, NodeIndex)> = entries
                .iter()
                .filter(|pair| pair[0] != NodeIndex::NONE)
                .map(|pair| (self.get_type_from_type_node(pair[0]), pair[1]))
                .collect();

            // Need at least 2 explicitly constrained declarations to have a conflict
            if constrained.len() < 2 {
                continue;
            }

            // Check if all explicit constraints are identical
            let first_type = constrained[0].0;
            let all_identical = constrained
                .iter()
                .all(|(type_id, _)| *type_id == first_type);

            if !all_identical {
                // Emit TS2838 at each explicitly constrained declaration site
                for (_, tp_node) in &constrained {
                    self.error_at_node_msg(
                        *tp_node,
                        diagnostic_codes::ALL_DECLARATIONS_OF_MUST_HAVE_IDENTICAL_CONSTRAINTS,
                        &[name],
                    );
                }
            }
        }
    }

    /// Check if a node is a bare `intrinsic` type reference (no type args, no qualification,
    /// not inside parentheses).
    ///
    /// Returns true when `node_idx` points to a `TYPE_REFERENCE` whose `type_name` is a simple
    /// `IDENTIFIER` with text `"intrinsic"` and no type arguments.
    ///
    /// TSC treats `intrinsic` as a keyword only when it appears directly as the type alias body
    /// (e.g., `type Uppercase<S extends string> = intrinsic`). When parenthesized like
    /// `type TE1 = (intrinsic)`, TSC treats it as a regular identifier reference (TS2304).
    /// Since our parser doesn't create a `PARENTHESIZED_TYPE` wrapper node, we detect
    /// parenthesization by checking if the character before the type reference in the source
    /// is `(`.
    pub(crate) fn is_bare_intrinsic_type_ref(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        // Must have no type arguments
        if type_ref.type_arguments.is_some() {
            return false;
        }
        // type_name must be a simple IDENTIFIER (not QUALIFIED_NAME)
        let Some(name_node) = self.ctx.arena.get(type_ref.type_name) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        if ident.escaped_text != "intrinsic" {
            return false;
        }
        // Check that it's not parenthesized: look at the source text before the
        // type reference's position. If the nearest non-whitespace character is `(`,
        // the reference is parenthesized and should NOT be treated as the keyword.
        if let Some(sf) = self.ctx.arena.source_files.first() {
            let pos = node.pos as usize;
            if pos > 0 {
                let before = &sf.text[..pos];
                let last_non_ws = before
                    .bytes()
                    .rev()
                    .find(|&b| b != b' ' && b != b'\t' && b != b'\n' && b != b'\r');
                if last_non_ws == Some(b'(') {
                    return false;
                }
            }
        }
        true
    }
}
