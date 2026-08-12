//! Class member declaration and accessibility validation helpers.
use crate::context::TypingRequest;
use crate::state::{CheckerState, MemberAccessLevel, MemberLookup};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
impl<'a> CheckerState<'a> {
    fn missing_name_type_ref_is_bare_scoped_type_parameter(&self, type_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return false;
        };
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        if type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return false;
        }
        let Some(name_node) = self.ctx.arena.get(type_ref.type_name) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        self.ctx
            .type_parameter_scope
            .contains_key(ident.escaped_text.as_str())
    }

    pub(crate) fn check_async_modifier_on_declaration(
        &mut self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) {
        use crate::diagnostics::diagnostic_codes;
        let Some(async_mod_idx) = self.find_async_modifier(modifiers) else {
            return;
        };
        // tsc's `checkGrammarModifiers` reports at most one diagnostic per
        // modifier list and returns immediately after: for `async export
        // class C {}` (`async` misordered before `export`) it reports only
        // the order violation (TS1029, anchored on `export`) and never goes
        // on to ask whether `async` is legal on this declaration kind at
        // all. tsz's parser reports that same TS1029 eagerly during parsing
        // (`look_ahead_async_before_export_target` /
        // `parse_statement_async_declaration_or_expression`, #16403 slice
        // 3) — before this class/interface/enum/namespace's own modifier
        // list even exists as a checked unit — so this independent checker
        // pass has no AST field to read "did it already fire" back from.
        // Re-derive it from position instead, the same way
        // `check_export_declaration`'s TS1319 dedup does for the sibling
        // `declare export default` shape: any parse-diagnostic position
        // strictly inside this modifier list's own span can only be the
        // order-violation grammar check tsc already reported for this exact
        // node, since the declaration body (where an unrelated parse error
        // could otherwise land) starts only after the last modifier.
        if self.modifier_run_already_has_parse_error(modifiers) {
            return;
        }
        self.error_at_node(
            async_mod_idx,
            "'async' modifier cannot be used here.",
            diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
        );
    }

    /// Whether any parse-time diagnostic landed strictly inside `modifiers`'
    /// own span (first modifier's start through last modifier's end).
    /// `NodeList::pos`/`end` are not populated for a modifier list built from
    /// individually-parsed tokens (`Parser::make_node_list` always zeroes
    /// them), so the span is recomputed from the member nodes' own
    /// positions rather than trusted from the list itself.
    fn modifier_run_already_has_parse_error(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        let Some(modifiers) = modifiers else {
            return false;
        };
        let mut span: Option<(u32, u32)> = None;
        for &mod_idx in &modifiers.nodes {
            let Some(node) = self.ctx.arena.get(mod_idx) else {
                continue;
            };
            span = Some(match span {
                Some((start, end)) => (start.min(node.pos), end.max(node.end)),
                None => (node.pos, node.end),
            });
        }
        let Some((start, end)) = span else {
            return false;
        };
        self.ctx
            .all_parse_error_positions
            .iter()
            .any(|&pos| pos >= start && pos < end)
    }

    /// TS1277: `const` modifier can only appear on function, method, or class type parameters.
    pub(crate) fn check_const_type_parameter_on_non_function(
        &mut self,
        type_params: Option<&tsz_parser::parser::NodeList>,
    ) {
        let Some(type_params) = type_params else {
            return;
        };
        for &param_idx in &type_params.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(tp) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            if let Some(ref modifiers) = tp.modifiers {
                for &mod_idx in &modifiers.nodes {
                    let Some(mod_node) = self.ctx.arena.get(mod_idx) else {
                        continue;
                    };
                    if mod_node.kind == tsz_scanner::SyntaxKind::ConstKeyword as u16 {
                        self.error_at_node_msg(
                            mod_idx,
                            crate::diagnostics::diagnostic_codes::MODIFIER_CAN_ONLY_APPEAR_ON_A_TYPE_PARAMETER_OF_A_FUNCTION_METHOD_OR_CLASS,
                            &["const"],
                        );
                    }
                }
            }
        }
    }

    /// TS1273: modifiers categorically invalid on a type parameter (`public`,
    /// `private`, `protected`, `static`, `readonly`, `async`, `declare`,
    /// `abstract`, `override`, `export`, `default`, `accessor`). TS1274 is
    /// reserved for `in`/`out` in the wrong context.
    pub(crate) fn check_never_valid_type_parameter_modifiers(
        &mut self,
        type_params: Option<&tsz_parser::parser::NodeList>,
    ) {
        let Some(type_params) = type_params else {
            return;
        };
        for &param_idx in &type_params.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(tp) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            if let Some(ref modifiers) = tp.modifiers {
                for &mod_idx in &modifiers.nodes {
                    let Some(mod_node) = self.ctx.arena.get(mod_idx) else {
                        continue;
                    };
                    let kind = mod_node.kind;
                    let is_invalid = matches!(
                        kind,
                        x if x == SyntaxKind::PublicKeyword as u16
                            || x == SyntaxKind::PrivateKeyword as u16
                            || x == SyntaxKind::ProtectedKeyword as u16
                            || x == SyntaxKind::StaticKeyword as u16
                            || x == SyntaxKind::ReadonlyKeyword as u16
                            || x == SyntaxKind::AsyncKeyword as u16
                            || x == SyntaxKind::DeclareKeyword as u16
                            || x == SyntaxKind::AbstractKeyword as u16
                            || x == SyntaxKind::OverrideKeyword as u16
                            || x == SyntaxKind::AccessorKeyword as u16
                            || x == SyntaxKind::ExportKeyword as u16
                            || x == SyntaxKind::DefaultKeyword as u16
                    );
                    if is_invalid {
                        let modifier_text = match kind {
                            x if x == SyntaxKind::PublicKeyword as u16 => "public",
                            x if x == SyntaxKind::PrivateKeyword as u16 => "private",
                            x if x == SyntaxKind::ProtectedKeyword as u16 => "protected",
                            x if x == SyntaxKind::StaticKeyword as u16 => "static",
                            x if x == SyntaxKind::ReadonlyKeyword as u16 => "readonly",
                            x if x == SyntaxKind::AsyncKeyword as u16 => "async",
                            x if x == SyntaxKind::DeclareKeyword as u16 => "declare",
                            x if x == SyntaxKind::AbstractKeyword as u16 => "abstract",
                            x if x == SyntaxKind::OverrideKeyword as u16 => "override",
                            x if x == SyntaxKind::AccessorKeyword as u16 => "accessor",
                            x if x == SyntaxKind::ExportKeyword as u16 => "export",
                            x if x == SyntaxKind::DefaultKeyword as u16 => "default",
                            _ => continue,
                        };
                        self.error_at_node_msg(
                            mod_idx,
                            crate::diagnostics::diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_PARAMETER,
                            &[modifier_text],
                        );
                    }
                }
            }
        }
    }

    /// TS1274: variance modifiers (`in`, `out`) are invalid on function/method type parameters.
    pub(crate) fn check_variance_on_function_type_parameters(
        &mut self,
        type_params: Option<&tsz_parser::parser::NodeList>,
    ) {
        let Some(type_params) = type_params else {
            return;
        };
        for &param_idx in &type_params.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(tp) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            if let Some(ref modifiers) = tp.modifiers {
                for &mod_idx in &modifiers.nodes {
                    let Some(mod_node) = self.ctx.arena.get(mod_idx) else {
                        continue;
                    };
                    let kind = mod_node.kind;
                    if kind == SyntaxKind::InKeyword as u16 || kind == SyntaxKind::OutKeyword as u16
                    {
                        let modifier_text = if kind == SyntaxKind::InKeyword as u16 {
                            "in"
                        } else {
                            "out"
                        };
                        self.error_at_node_msg(
                            mod_idx,
                            crate::diagnostics::diagnostic_codes::MODIFIER_CAN_ONLY_APPEAR_ON_A_TYPE_PARAMETER_OF_A_CLASS_INTERFACE_OR_TYPE_ALIAS,
                            &[modifier_text],
                        );
                    }
                }
            }
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
                            let level = self.member_access_level_from_modifiers(&param.modifiers);
                            return match level {
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

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if self.missing_name_type_ref_is_bare_scoped_type_parameter(type_idx) {
                    return;
                }
                if !self.ctx.symbol_resolution_set.is_empty()
                    && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
                    && let Some(sym_id) = self
                        .resolve_type_symbol_for_lowering(type_ref.type_name)
                        .map(tsz_binder::SymbolId)
                    // Only check direct self-references. Transitive walks through
                    // type_alias_reaches_resolving_alias are too aggressive: cycles
                    // through object/interface members are productive recursion that
                    // tsc allows without emitting TS2577.
                    && self.ctx.symbol_resolution_set.contains(&sym_id)
                {
                    // For circular references, still check type arguments for missing
                    // names. The main resolution is skipped to avoid infinite recursion,
                    // but type arguments may contain unresolvable names that need TS2304.
                    let arg_indices: Vec<NodeIndex> = self
                        .ctx
                        .arena
                        .get_type_ref(node)
                        .and_then(|tr| tr.type_arguments.as_ref())
                        .map(|args| args.nodes.clone())
                        .unwrap_or_default();
                    for arg_idx in arg_indices {
                        self.check_type_for_missing_names(arg_idx);
                    }
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
                    for (pi, &param_idx) in func_type.parameters.nodes.iter().enumerate() {
                        self.check_parameter_type_for_missing_names(param_idx);
                        // Function type literals in type positions (e.g. `(x) => string`)
                        // require explicit parameter types under --noImplicitAny, just like
                        // method signatures.  tsc emits TS7006/TS7019 for untyped params here.
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        {
                            self.maybe_report_implicit_any_parameter(param, false, pi);
                        }
                    }
                    let typeof_param_names =
                        self.push_typeof_params_from_ast_nodes(&func_type.parameters.nodes);
                    if func_type.type_annotation.is_some() {
                        // Check for TS2577: circular return type annotation.
                        // Only emit when the function type is inside a conditional
                        // type's extends clause — that context requires eager
                        // evaluation of the return type pattern, making the
                        // circularity observable.  A self-referential return type
                        // in a plain type alias body (e.g. `type F = () => F`) is
                        // valid productive recursion and must NOT trigger TS2577.
                        if !self.ctx.symbol_resolution_set.is_empty()
                            && self.ctx.in_conditional_extends_depth > 0
                            && self.type_node_contains_circular_reference(func_type.type_annotation)
                        {
                            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                            self.error_at_node(
                                func_type.type_annotation,
                                diagnostic_messages::RETURN_TYPE_ANNOTATION_CIRCULARLY_REFERENCES_ITSELF,
                                diagnostic_codes::RETURN_TYPE_ANNOTATION_CIRCULARLY_REFERENCES_ITSELF,
                            );
                        }
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
                    self.check_unique_symbol_in_conditional_extends(cond.extends_type);

                    // TS2838: Check that duplicate infer type params have identical constraints
                    self.check_infer_constraint_consistency(cond.extends_type);

                    // Collect infer bindings and install them in scope for true_type.
                    let param_bindings = self.push_infer_bindings_from_extends(cond.extends_type);

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
                    // A missing operand (`let v: readonly ;`) already produced TS1110
                    // `Type expected` in the parser; tsc does not also report the
                    // array/tuple grammar error, so skip it for the recovery node.
                    if op.operator == tsz_scanner::SyntaxKind::ReadonlyKeyword as u16
                        && let Some(operand_node) = self.ctx.arena.get(op.type_node)
                        && operand_node.kind != syntax_kind_ext::ARRAY_TYPE
                        && operand_node.kind != syntax_kind_ext::TUPLE_TYPE
                        && !self.ctx.arena.is_missing_recovery_identifier(op.type_node)
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
                    let param_binding =
                        self.push_mapped_type_param_provisional(mapped.type_parameter);
                    let is_direct_self_constraint = param_binding
                        .as_ref()
                        .and_then(|(name, _)| {
                            let param_node = self.ctx.arena.get(mapped.type_parameter)?;
                            let param = self.ctx.arena.get_type_parameter(param_node)?;
                            let constraint_node = self.ctx.arena.get(param.constraint)?;
                            let constraint_ref = self.ctx.arena.get_type_ref(constraint_node)?;
                            if constraint_ref.type_arguments.is_some() {
                                return None;
                            }
                            let constraint_name = self.ctx.arena.get(constraint_ref.type_name)?;
                            let constraint_ident =
                                self.ctx.arena.get_identifier(constraint_name)?;
                            if constraint_ident.escaped_text == name.as_str() {
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
                        self.ctx.report_mapped_type_missing_template(type_idx);
                    }
                    if let Some(ref members) = mapped.members {
                        for &member_idx in &members.nodes {
                            self.check_type_member_for_missing_names(member_idx);
                        }
                    }
                    self.pop_mapped_type_param_provisional(param_binding);
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

        // Two-pass insertion mirrors `push_type_parameters`: the first pass
        // binds every name unconstrained so the second pass can resolve each
        // declared constraint against its sibling type parameters. Without
        // the second pass the early "missing names" scan observes a parameter
        // `K extends keyof T[U]` as if it were unconstrained, which causes
        // downstream mapped-type validity checks (`{ [P in K]: ... }`) to
        // incorrectly fire TS2322/TS2536. `tsc` resolves constraints lazily;
        // tsz uses scope, so the scope must reflect the declared constraints
        // before validation walks the signature.
        struct Entry {
            name: String,
            atom: tsz_common::interner::Atom,
            is_const: bool,
            constraint_node: Option<NodeIndex>,
        }
        let mut entries: Vec<Entry> = Vec::new();
        let mut updates = Vec::new();

        // Pass 1: insert unconstrained provisional bindings so constraint
        // resolution in pass 2 can see all sibling type parameters.
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
            let name = ident.escaped_text.to_string();
            let atom = self.ctx.types.intern_string(&name);
            let is_const = self
                .ctx
                .arena
                .has_modifier(&param.modifiers, SyntaxKind::ConstKeyword);
            let constraint_node = (param.constraint != NodeIndex::NONE).then_some(param.constraint);
            let type_id = self.ctx.types.factory().type_param(TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
                is_const,
                origin: tsz_solver::TypeParamOrigin::User,
            });
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name.clone(), previous, false));
            entries.push(Entry {
                name,
                atom,
                is_const,
                constraint_node,
            });
        }

        // Pass 2: refine each binding with its resolved constraint. Resolution
        // runs against the now-populated scope so transitive references like
        // `<K extends keyof T[TB]>` (where `TB` is a prior type parameter)
        // bind to the constrained type, not the provisional placeholder.
        // Resolution errors fall back to the unconstrained placeholder so
        // TS2304 / cycle handling is preserved.
        for entry in entries {
            let Some(constraint_node_idx) = entry.constraint_node else {
                continue;
            };
            let resolved = self.get_type_from_type_node(constraint_node_idx);
            if resolved == TypeId::ERROR {
                continue;
            }
            let constrained_type_id = self.ctx.types.factory().type_param(TypeParamInfo {
                name: entry.atom,
                constraint: Some(resolved),
                default: None,
                is_const: entry.is_const,
                origin: tsz_solver::TypeParamOrigin::User,
            });
            self.ctx
                .type_parameter_scope
                .insert(entry.name, constrained_type_id);
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
            let name = ident.escaped_text.to_string();
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
            return;
        }

        // Handle get/set accessor members in interfaces and type literals.
        // Without this, type annotations on accessor parameters (e.g.,
        // `set x(value: Fail<string>)`) are never visited for constraint
        // validation, missing TS2344 errors.
        if (member_node.kind == syntax_kind_ext::GET_ACCESSOR
            || member_node.kind == syntax_kind_ext::SET_ACCESSOR)
            && let Some(accessor) = self.ctx.arena.get_accessor(member_node)
        {
            // Check getter return type annotation
            if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                && accessor.type_annotation.is_some()
            {
                self.check_type_for_missing_names(accessor.type_annotation);
            }
            // Check setter parameter type annotations
            if member_node.kind == syntax_kind_ext::SET_ACCESSOR {
                for &param_idx in &accessor.parameters.nodes {
                    self.check_parameter_type_for_missing_names(param_idx);
                }
            }
        }
    }

    /// Check a type literal member for parameter properties (call/construct signatures).
    pub(crate) fn check_type_member_for_parameter_properties(&mut self, member_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return;
        };

        // TS1015/TS1016 also run for method signatures of interfaces and
        // type literals (tsc runs parameter grammar for every function-like
        // signature).
        if node.kind == syntax_kind_ext::METHOD_SIGNATURE
            && let Some(sig) = self.ctx.arena.get_signature(node)
            && let Some(params) = &sig.parameters
        {
            self.check_parameter_ordering(params, Some(member_idx));
        }

        // TS1052/TS1053 likewise run for a `set` accessor member of an
        // interface or type literal: tsc's `checkGrammarAccessor` reads the
        // parameter node alone, so the container never gates the rule. Both
        // callers of this walk (interface members and type-literal members)
        // therefore get it here.
        if node.kind == syntax_kind_ext::SET_ACCESSOR {
            self.check_setter_parameter_grammar(member_idx);
        }

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
                    // TS1015/TS1016: parameter grammar runs for every
                    // function-like signature, call/construct signatures of
                    // interfaces and type literals included.
                    self.check_parameter_ordering(params, Some(member_idx));
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
    /// TS2784/TS2680 for class accessors.
    ///
    /// Accessors do not route through `check_parameter_ordering` — the walks
    /// that do are keyed on methods, constructors and signatures — so the
    /// shared `this`-parameter placement check runs from its own member walk.
    /// This one deliberately runs for ambient classes too: a `declare class`
    /// getter can declare a `this` parameter just as illegally as a concrete
    /// one, and `check_class_member_implementations` is skipped when the class
    /// is ambient.
    pub(crate) fn check_class_accessor_this_parameters(&mut self, members: &[NodeIndex]) {
        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::GET_ACCESSOR
                && node.kind != syntax_kind_ext::SET_ACCESSOR
            {
                continue;
            }
            if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                self.check_this_parameter_placement(&accessor.parameters, Some(member_idx));
            }
        }
    }

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
                // TS1042: 'async' modifier cannot be used on getters/setters.
                // Anchor at the `async` keyword (tsc's `checkGrammarModifiers`
                // points at the offending modifier, not the accessor name, so
                // `public async get x()` still points at `async`). When the
                // accessor also carries `readonly`, defer to that member's
                // TS1024 (`readonly`-on-accessor, reported from `class.rs`):
                // tsc emits a single modifier-grammar diagnostic per member and
                // `readonly` wins over `async` on an accessor in either source
                // order (`readonly async get`, `async readonly get` both report
                // only TS1024). `readonly` on a *property* is legal and does not
                // fire TS1024, so the property arm below keeps its lone TS1042.
                // A `declare` accessor is excluded either way: tsc reports
                // exactly one diagnostic per member, and a `declare`+`async`
                // accessor already gets it from the parser — TS1031 (`declare`
                // illegal on this member kind) when `declare` came first, or
                // TS1040 (ambient conflict) when `async` came first.
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(node)
                        && !self.has_readonly_modifier(&accessor.modifiers)
                        && !self.has_declare_modifier(&accessor.modifiers)
                        && let Some(async_mod_idx) = self.find_async_modifier(&accessor.modifiers)
                    {
                        self.error_at_node(
                            async_mod_idx,
                            "'async' modifier cannot be used here.",
                            diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
                        );
                    }
                }
                // TS1042: 'async' modifier cannot be used on a property
                // declaration either — `checkGrammarAsyncModifier` allows it
                // only on method/function-like nodes. `readonly` on a property
                // is legal, so `readonly async p` is a lone TS1042 with no
                // ordering collision; anchored at the `async` keyword to match
                // tsc's column even when the property carries a decorator. A
                // `declare` property is excluded: the parser already reports
                // tsc's single TS1040 (ambient conflict) for `declare async p`
                // in either source order.
                syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.ctx.arena.get_property_decl(node)
                        && !self.has_declare_modifier(&prop.modifiers)
                        && let Some(async_mod_idx) = self.find_async_modifier(&prop.modifiers)
                    {
                        self.error_at_node(
                            async_mod_idx,
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
                        // Constructor overload signature. Like methods (and like
                        // tsc's `checkFunctionOrConstructorSymbol`), the
                        // missing-implementation diagnostic belongs to the *last*
                        // declaration of the overload set, reported once — not once
                        // per bodyless signature. Advance past all consecutive
                        // bodyless constructor signatures to that last one before
                        // deciding, tracking whether that last signature is
                        // `abstract` as we go (tsc suppresses the diagnostic on an
                        // abstract last declaration — an abstract member needs no
                        // implementation, exactly as the method arm below skips
                        // abstract methods. `abstract` on a constructor is itself a
                        // grammar error, TS1242, but tsc still honours it here, so
                        // `abstract constructor()` reports only TS1242, never
                        // TS2390).
                        let mut last_overload_i = i;
                        let mut last_is_abstract = self.has_abstract_modifier(&ctor.modifiers);
                        let mut j = i + 1;
                        while j < members.len() {
                            let next_idx = members[j];
                            let Some(next_node) = self.ctx.arena.get(next_idx) else {
                                break;
                            };
                            if next_node.kind == syntax_kind_ext::CONSTRUCTOR
                                && let Some(next_ctor) = self.ctx.arena.get_constructor(next_node)
                                && next_ctor.body.is_none()
                            {
                                last_overload_i = j;
                                last_is_abstract = self.has_abstract_modifier(&next_ctor.modifiers);
                                j += 1;
                                continue;
                            }
                            break;
                        }

                        // tsc anchors the missing-implementation error on the last
                        // overload signature.
                        let report_member_idx = members[last_overload_i];
                        let has_impl = self.find_constructor_impl(members, last_overload_i + 1);
                        if !has_impl && !last_is_abstract {
                            self.error_at_node(
                                report_member_idx,
                                "Constructor implementation is missing.",
                                diagnostic_codes::CONSTRUCTOR_IMPLEMENTATION_IS_MISSING,
                            );
                        }

                        // Skip past every overload we just folded into this group.
                        i = last_overload_i + 1;
                        continue;
                    }
                }
                syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(node) {
                        if node.this_node_has_error() || node.this_or_subtree_has_error() {
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
                                // Advance past consecutive bodyless method overloads with the same name,
                                // tracking whether the LAST one is `abstract` as we go. tsc suppresses
                                // the missing-implementation diagnostic when the group's last signature
                                // is abstract — an abstract member needs no implementation, exactly like
                                // the constructor arm above (`abstract m(x: string): void;` after a
                                // non-abstract overload reports only TS2512, never TS2391). The starting
                                // member here is always non-abstract (gated above), so a group with no
                                // abstract siblings keeps reporting as before.
                                let mut last_overload_i = i;
                                let mut last_is_abstract = false;
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
                                            last_is_abstract =
                                                self.has_abstract_modifier(&next_method.modifiers);
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
                                if !has_impl && !last_is_abstract {
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

        let class_jsdoc_template_updates =
            self.push_enclosing_jsdoc_class_template_types(member_idx);

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

        self.check_variance_modifier_not_on_class_member_node(member_idx);

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
                    // The loop/switch/label reset that used to be hand-copied
                    // here now rides along with the member-body boundary, so
                    // every member kind gets it — see `enter_class_member_body`.
                    let saved_member_body_depth = self.ctx.enter_class_member_body();
                    // Check each statement in the block
                    for &stmt_idx in &block.statements.nodes {
                        let body_request = request.read().contextual_opt(None);
                        self.check_statement_with_request(stmt_idx, &body_request);
                        if !self.statement_falls_through(stmt_idx) {
                            self.ctx.is_unreachable = true;
                        }
                    }
                    self.ctx.exit_class_member_body(saved_member_body_depth);
                    self.ctx.is_unreachable = prev_unreachable;
                    self.ctx.has_reported_unreachable = prev_reported;
                }
            }
            syntax_kind_ext::INDEX_SIGNATURE => {
                // Index signatures are metadata used during type resolution, not
                // members with their own types. They're handled separately by
                // get_index_signatures. The only member-declaration check here is
                // the TS1071 modifier grammar (owned by `index_signature_checks`).
                self.check_index_signature_member_modifiers(member_idx);
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

        self.pop_enclosing_jsdoc_class_template_types(class_jsdoc_template_updates);
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

            // Parameter decorators only have semantic work here under
            // `experimentalDecorators` (the valid-position path); without it,
            // the parameter loop bails immediately and TS1206 is owned by
            // `check_parameter_properties`. So a member whose *only* decorators
            // are on its parameters need not enter this function at all in the
            // standard-decorator mode.
            let has_param_decorator = self.ctx.compiler_options.experimental_decorators
                && match node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .ctx
                        .arena
                        .get_method_decl(node)
                        .is_some_and(|d| self.any_parameter_has_decorator(&d.parameters.nodes)),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
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

        // A decorated method with no body is an overload signature, which tsc's
        // checkGrammarDecorators reports as TS1249 ("A decorator can only decorate
        // a method implementation, not an overload") — the same `!nodeCanBeDecorated`
        // path as an abstract method. Ambient methods are legitimately body-less
        // and handled elsewhere, so exclude them.
        let is_overload_method = !is_ambient
            && node.kind == syntax_kind_ext::METHOD_DECLARATION
            && self
                .ctx
                .arena
                .get_method_decl(node)
                .is_some_and(|m| m.body.is_none());

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

        // ES (TC39) decorator first-argument shape per member kind. Computed once
        // before the per-decorator loop because the member kind and modifiers do
        // not vary across decorators on the same declaration.
        //
        // - Plain field: runtime invokes `decorator(undefined, context)`.
        // - Auto-accessor (`accessor x = …`): runtime invokes
        //   `decorator(target, context)` where `target` is a
        //   `ClassAccessorDecoratorTarget<This, Value>` object. We resolve the
        //   global type and instantiate it with `<any, any>`; the decorator's
        //   `This`/`Value` type parameters are inferred from this shape.
        //
        // If `ClassAccessorDecoratorTarget` is unavailable (e.g. `--noLib`) we
        // fall back to `ANY` so the absence of the lib type cannot itself
        // produce a TS1240 false positive.
        let es_member_first_arg: Option<TypeId> =
            if !self.ctx.compiler_options.experimental_decorators
                && node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && !is_ambient_field
            {
                Some(if self.has_accessor_modifier_ref(modifiers) {
                    self.resolve_class_accessor_decorator_target_any()
                        .unwrap_or(TypeId::ANY)
                } else {
                    TypeId::UNDEFINED
                })
            } else {
                None
            };

        if let Some(modifiers) = modifiers {
            for &modifier_idx in &modifiers.nodes {
                let Some(modifier_node) = self.ctx.arena.get(modifier_idx) else {
                    continue;
                };
                if modifier_node.kind != syntax_kind_ext::DECORATOR {
                    continue;
                }

                if is_abstract
                    || is_overload_method
                    || (!self.ctx.compiler_options.experimental_decorators && is_ambient_field)
                    || legacy_decorator_not_valid
                {
                    use crate::diagnostics::diagnostic_codes;
                    if (is_abstract || is_overload_method)
                        && node.kind == syntax_kind_ext::METHOD_DECLARATION
                    {
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
                let actual_this_type =
                    self.call_site_receiver_type(decorator_type, decorator.expression);

                if let Some(first_arg) = es_member_first_arg {
                    self.check_es_member_decorator_call_signature(
                        decorator.expression,
                        modifier_idx,
                        decorator_type,
                        first_arg,
                        actual_this_type,
                    );
                }

                if self.ctx.compiler_options.experimental_decorators
                    && !is_abstract
                    && !legacy_decorator_not_valid
                    && node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                {
                    self.check_legacy_property_decorator_call_signature(
                        modifier_idx,
                        decorator.expression,
                        decorator_type,
                        self.has_accessor_modifier_ref(Some(modifiers)),
                        actual_this_type,
                    );
                }

                if !is_abstract
                    && !legacy_decorator_not_valid
                    && (node.kind == syntax_kind_ext::METHOD_DECLARATION
                        || node.kind == syntax_kind_ext::GET_ACCESSOR
                        || node.kind == syntax_kind_ext::SET_ACCESSOR)
                {
                    self.check_method_or_accessor_decorator_call_signature(
                        decorator.expression,
                        decorator_type,
                        modifier_idx,
                        member_idx,
                        self.ctx.compiler_options.experimental_decorators,
                        actual_this_type,
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

                        // Without `experimentalDecorators` a class-member
                        // parameter decorator is an invalid position. TS1206 is
                        // owned by `check_parameter_properties` (the single,
                        // universal parameter-decorator grammar gate), which
                        // reports it once per parameter. tsc emits nothing else
                        // for an invalidly-placed decorator — in particular it
                        // does not resolve the decorator expression — so skip
                        // the semantic checks below to avoid a spurious TS2304.
                        if !self.ctx.compiler_options.experimental_decorators {
                            continue;
                        }

                        if let Some(decorator) = self.ctx.arena.get_decorator(modifier_node) {
                            // TS1497: Check decorator expression grammar
                            self.check_grammar_decorator(decorator.expression);

                            let decorator_type = self.compute_type_of_node(decorator.expression);

                            // TS1308: Check for await expressions in decorator arguments.
                            // Decorator arguments are evaluated in the enclosing scope,
                            // not the decorated method's scope. An await in a non-async
                            // enclosing function should trigger TS1308.
                            self.check_await_expression(decorator.expression);

                            // TS1239: Validate parameter decorator call signature.
                            // The runtime invokes parameter decorators as
                            // `decorator(target, key, parameterIndex)`. For
                            // constructor parameters tsc passes `undefined` for
                            // `key`; for method/accessor parameters tsc passes a
                            // string (the method name). Decorators whose `key`
                            // parameter type disagrees with the position are
                            // rejected with TS1239. Reached only under
                            // `experimentalDecorators` (the early `continue`
                            // above), the sole configuration where a parameter
                            // decorator is a valid, semantically-checked target.
                            let is_constructor_parameter =
                                node.kind == syntax_kind_ext::CONSTRUCTOR;
                            let actual_this_type =
                                self.call_site_receiver_type(decorator_type, decorator.expression);
                            self.check_parameter_decorator_call_signature(
                                modifier_idx,
                                decorator_type,
                                is_constructor_parameter,
                                actual_this_type,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Quick scan to check if any parameter in a parameter list has a decorator modifier.
    fn any_parameter_has_decorator(&self, params: &[NodeIndex]) -> bool {
        params.iter().any(|&param_idx| {
            self.ctx
                .arena
                .get(param_idx)
                .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                .is_some_and(|param| self.first_parameter_decorator(&param.modifiers).is_some())
        })
    }
}
