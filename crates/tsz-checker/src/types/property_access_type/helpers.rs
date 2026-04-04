//! Property access type resolution helpers: CommonJS detection, JSDoc annotation,
//! finalization, interface recovery, and enum/namespace utilities.

use crate::context::TypingRequest;
use crate::query_boundaries::common::PropertyAccessResult;
use crate::query_boundaries::property_access as access_query;
use crate::state::{CheckerState, MAX_INSTANTIATION_DEPTH};
use tsz_binder::symbol_flags;
use tsz_common::common::Visibility;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn current_file_commonjs_module_identifier_is_unshadowed(
        &self,
        idx: NodeIndex,
    ) -> bool {
        !self
            .resolve_identifier_symbol_without_tracking(idx)
            .is_some_and(|sym_id| {
                self.ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|symbol| symbol.decl_file_idx == self.ctx.current_file_idx as u32)
            })
    }

    pub(crate) fn current_file_commonjs_exports_target_is_unshadowed(
        &self,
        idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text == "exports")
                && !self
                    .resolve_identifier_symbol_without_tracking(idx)
                    .is_some_and(|sym_id| {
                        self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                            symbol.decl_file_idx == self.ctx.current_file_idx as u32
                        })
                    });
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        self.ctx
            .arena
            .get_identifier_at(access.expression)
            .is_some_and(|ident| ident.escaped_text == "module")
            && self.current_file_commonjs_module_identifier_is_unshadowed(access.expression)
            && self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports")
    }

    pub(crate) fn current_file_commonjs_direct_write_rhs(
        &self,
        property_access_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let prop_ext = self.ctx.arena.get_extended(property_access_idx)?;
        let parent_idx = prop_ext.parent;
        let parent_node = self.ctx.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.ctx.arena.get_binary_expr(parent_node)?;
        (binary.left == property_access_idx && self.is_assignment_operator(binary.operator_token))
            .then_some(binary.right)
    }

    pub(crate) fn current_file_commonjs_write_rhs_is_undefined_like(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text == "undefined");
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && self.is_assignment_operator(binary.operator_token)
        {
            return self.current_file_commonjs_write_rhs_is_undefined_like(binary.right);
        }

        if node.kind != syntax_kind_ext::VOID_EXPRESSION
            && node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION
        {
            return false;
        }

        let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
            return false;
        };
        if unary.operator != SyntaxKind::VoidKeyword as u16 {
            return false;
        }
        let Some(expr) = self.ctx.arena.get(unary.operand) else {
            return false;
        };

        matches!(expr.kind, k if k == SyntaxKind::NumericLiteral as u16)
            && self
                .ctx
                .arena
                .get_literal(expr)
                .is_some_and(|lit| lit.text == "0")
    }

    pub(crate) fn is_jsdoc_annotated_this_member_declaration(&mut self, idx: NodeIndex) -> bool {
        if !self.is_js_file() {
            return false;
        }

        let mut current = idx;
        for _ in 0..4 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
                if self.jsdoc_type_annotation_for_node(ext.parent).is_none() {
                    return false;
                }
                let Some(stmt) = self.ctx.arena.get_expression_statement(parent_node) else {
                    return false;
                };
                let Some(expr_node) = self.ctx.arena.get(stmt.expression) else {
                    return false;
                };
                if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && expr_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                {
                    return false;
                }
                let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
                    return false;
                };
                let Some(base_node) = self.ctx.arena.get(access.expression) else {
                    return false;
                };
                return base_node.kind == SyntaxKind::ThisKeyword as u16
                    && self.this_has_contextual_owner(access.expression).is_some();
            }
            current = ext.parent;
        }

        false
    }

    pub(crate) fn finalize_property_access_result(
        &self,
        idx: NodeIndex,
        result_type: TypeId,
        skip_flow_narrowing: bool,
        skip_result_flow_for_result: bool,
    ) -> TypeId {
        if skip_flow_narrowing || skip_result_flow_for_result {
            result_type
        } else {
            self.apply_flow_narrowing(idx, result_type)
        }
    }

    pub(crate) fn union_write_requires_existing_named_member(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> bool {
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, object_type)
        else {
            return false;
        };

        let mut saw_present_member = false;
        let mut saw_fresh_empty_missing_member = false;

        for member in members {
            if member.is_nullable() {
                continue;
            }

            let evaluated_member = self.evaluate_application_type(member);
            let resolved_member = self.resolve_type_for_property_access(evaluated_member);
            match self.resolve_property_access_with_env(resolved_member, property_name) {
                PropertyAccessResult::Success { .. }
                | PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(_),
                    ..
                } => {
                    saw_present_member = true;
                }
                PropertyAccessResult::PropertyNotFound { .. } => {
                    if crate::query_boundaries::common::is_empty_object_type(
                        self.ctx.types,
                        resolved_member,
                    ) && crate::query_boundaries::common::is_fresh_object_type(
                        self.ctx.types,
                        resolved_member,
                    ) {
                        saw_fresh_empty_missing_member = true;
                    } else {
                        return false;
                    }
                }
                PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: None,
                    ..
                }
                | PropertyAccessResult::IsUnknown => {}
            }
        }

        saw_present_member && saw_fresh_empty_missing_member
    }

    pub(crate) fn recover_property_from_implemented_interfaces(
        &mut self,
        class_idx: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        let class_node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(class_node)?;
        let heritage_clauses = class.heritage_clauses.as_ref()?;

        for &clause_idx in &heritage_clauses.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ImplementsKeyword as u16 {
                continue;
            }

            for &type_idx in &heritage.types.nodes {
                // Heritage clause types are ExpressionWithTypeArguments nodes.
                // Resolve the symbol from the expression, then get its instance type
                // via type_reference_symbol_type (which returns the instance type for
                // classes, not the constructor type).
                let expr_idx = if let Some(type_node) = self.ctx.arena.get(type_idx)
                    && let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
                {
                    expr_type_args.expression
                } else {
                    type_idx
                };

                let Some(sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                    continue;
                };
                let interface_type = self.type_reference_symbol_type(sym_id);
                if interface_type == TypeId::ERROR {
                    continue;
                }
                let interface_type_eval = self.evaluate_application_type(interface_type);
                // Resolve Lazy(DefId) types through the checker's TypeEnvironment so the
                // solver can inspect the interface's actual members. Without this step the
                // solver falls back to TypeId::ANY (its "couldn't resolve" sentinel) which
                // would incorrectly suppress TS2339 for properties that don't exist at all.
                let interface_type_resolved =
                    self.resolve_type_for_property_access(interface_type_eval);
                match self.resolve_property_access_with_env(interface_type_resolved, property_name)
                {
                    PropertyAccessResult::Success { type_id, .. }
                    | PropertyAccessResult::PossiblyNullOrUndefined {
                        property_type: Some(type_id),
                        ..
                    } => {
                        // Don't recover private or protected members from implemented
                        // interfaces. When an interface extends a class with private
                        // members, those members should only be accessible on classes
                        // that actually extend that base class, not on any class that
                        // merely implements the interface.
                        if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                            self.ctx.types,
                            interface_type_resolved,
                        ) {
                            let prop_atom = self.ctx.types.intern_string(property_name);
                            if let Some(prop_info) =
                                shape.properties.iter().find(|p| p.name == prop_atom)
                                && prop_info.visibility != Visibility::Public
                            {
                                continue;
                            }
                        }
                        return Some(type_id);
                    }
                    _ => {}
                }
            }
        }

        None
    }

    /// Check if a const enum symbol is "ambient" — declared with `declare` keyword
    /// or originating from a `.d.ts` file. Ambient const enums have no runtime
    /// representation and cannot be accessed under `isolatedModules`.
    pub(crate) fn is_const_enum_ambient(&self, sym: &tsz_binder::Symbol) -> bool {
        // If the file itself is a .d.ts, everything in it is ambient.
        if self.ctx.is_declaration_file() {
            return true;
        }
        // Check if all declarations are in ambient context (e.g., `declare const enum`).
        if sym.declarations.is_empty() {
            return false;
        }
        for &decl_idx in &sym.declarations {
            if !self.ctx.arena.is_in_ambient_context(decl_idx) {
                return false;
            }
        }
        true
    }

    /// Check if a node is in a type-only position (e.g., computed property name
    /// inside a type literal, interface, or type alias). In such positions,
    /// const enum values are resolved at compile time and don't need runtime access.
    pub(crate) fn is_in_type_only_position(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..20 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            // If we hit a type node or type alias/interface declaration, we're in type context
            if parent_node.is_type_node()
                || parent_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
            {
                return true;
            }
            // If we hit a statement, class member, or function-like, we're in value context
            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                || parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                || parent_node.kind == syntax_kind_ext::RETURN_STATEMENT
                || parent_node.kind == syntax_kind_ext::CALL_EXPRESSION
                || parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            {
                return false;
            }
            current = parent;
        }
        false
    }

    pub(crate) fn resolve_shadowed_global_value_member(
        &mut self,
        expr_idx: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        let ident = self.ctx.arena.get_identifier_at(expr_idx)?;
        let sym_id = self.resolve_identifier_symbol_without_tracking(expr_idx)?;
        let symbol = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| self.get_cross_file_symbol(sym_id))?;

        let is_namespace = (symbol.flags & symbol_flags::NAMESPACE_MODULE) != 0;
        let value_flags_except_module = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;
        let has_other_value = (symbol.flags & value_flags_except_module) != 0;
        if !is_namespace || has_other_value {
            return None;
        }

        let is_instantiated = symbol
            .declarations
            .iter()
            .any(|&decl_idx| self.is_namespace_declaration_instantiated(decl_idx));
        if is_instantiated {
            return None;
        }

        let value_type = self.type_of_value_symbol_by_name(&ident.escaped_text);
        if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
            return None;
        }

        match self.resolve_property_access_with_env(value_type, property_name) {
            PropertyAccessResult::Success { type_id, .. }
            | PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(type_id),
                ..
            } => Some(type_id),
            _ => None,
        }
    }

    /// Get type of property access expression.
    #[allow(dead_code)]
    pub(crate) fn get_type_of_property_access(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_property_access_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_property_access_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        if self.ctx.instantiation_depth.get() >= MAX_INSTANTIATION_DEPTH {
            self.ctx.depth_exceeded.set(true);
            return TypeId::ERROR; // Max instantiation depth exceeded - propagate error
        }

        self.ctx
            .instantiation_depth
            .set(self.ctx.instantiation_depth.get() + 1);
        let result = self.get_type_of_property_access_inner(idx, request);
        self.ctx
            .instantiation_depth
            .set(self.ctx.instantiation_depth.get() - 1);
        self.instantiate_callable_result_from_request(idx, result, request)
    }

    pub(crate) fn missing_typescript_lib_dom_global_alias(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        let ident = self.ctx.arena.get_identifier(node)?;
        let name = ident.escaped_text.as_str();
        if !matches!(name, "window" | "self") {
            return None;
        }
        if !self.ctx.typescript_dom_replacement_loaded {
            return None;
        }
        match name {
            "window" if !self.ctx.typescript_dom_replacement_has_window => Some(name.to_string()),
            "self" if !self.ctx.typescript_dom_replacement_has_self => Some(name.to_string()),
            _ => None,
        }
    }

    pub(crate) fn enum_member_initializer_display_type(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<TypeId> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol.declarations.first().copied()?
        };

        let var_decl = self.ctx.arena.get_variable_declaration_at(decl_idx)?;
        if var_decl.initializer.is_none() {
            return None;
        }

        let init_type = self.get_type_of_node(var_decl.initializer);
        self.is_enum_member_type_for_widening(init_type)
            .then_some(init_type)
    }

    /// Resolve the base constraint of an IndexAccess type for display purposes.
    ///
    /// For `T[K]` where `T extends C` and `K extends D`, resolves through the
    /// constraint chain to produce the concrete type (e.g., `C[D]` evaluated).
    /// This matches tsc's behavior of showing the apparent type in error messages.
    pub(crate) fn resolve_index_access_base_constraint(&mut self, type_id: TypeId) -> TypeId {
        // First try standard evaluation (resolves T to its constraint)
        let evaluated = self.evaluate_type_with_env(type_id);

        // If fully resolved (no longer an IndexAccess), use it
        if !tsz_solver::is_index_access_type(self.ctx.types, evaluated) {
            return evaluated;
        }

        // Still an IndexAccess — try resolving the index type parameter's constraint.
        // E.g., {[s:string]:V}[K] where K extends keyof T => evaluate {[s:string]:V}[keyof T] => V
        if let Some((ia_obj, ia_idx)) = tsz_solver::index_access_parts(self.ctx.types, evaluated)
            && let Some(constraint) =
                access_query::type_parameter_constraint(self.ctx.types, ia_idx)
        {
            let resolved = self
                .ctx
                .types
                .evaluate_index_access_with_options(ia_obj, constraint, false);
            if !tsz_solver::is_index_access_type(self.ctx.types, resolved) {
                return resolved;
            }
        }

        type_id
    }
}
