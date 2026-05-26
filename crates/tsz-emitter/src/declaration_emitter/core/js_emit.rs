use tsz_parser::parser::node::FunctionData;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

use super::DeclarationEmitter;

type JsCommonJsClosureSecondaryMember = (NodeIndex, NodeIndex, NodeIndex);
type JsCommonJsClosureExport = (NodeIndex, NodeIndex, Vec<JsCommonJsClosureSecondaryMember>);

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn emit_js_class_define_property_accessors_for_name(
        &mut self,
        class_name_idx: NodeIndex,
    ) {
        let Some(class_name) = self.get_identifier_text(class_name_idx) else {
            return;
        };
        let Some(accessors) = self
            .js_class_define_property_accessors
            .get(&class_name)
            .cloned()
        else {
            return;
        };

        for accessor in accessors {
            if let Some(setter) = accessor.setter {
                let (param_name, type_text) = self.js_define_property_setter_parameter_text(setter);
                self.write_indent();
                self.write("set ");
                self.write(&self.declaration_property_name_text(&accessor.property_name));
                self.write("(");
                self.write(&param_name);
                self.write(": ");
                self.write(&type_text);
                self.write(");");
                self.write_line();
            }

            if let Some(getter) = accessor.getter {
                let type_text = self
                    .js_define_property_getter_return_type_text(getter)
                    .unwrap_or_else(|| "any".to_string());
                self.write_indent();
                self.write("get ");
                self.write(&self.declaration_property_name_text(&accessor.property_name));
                self.write("(): ");
                self.write(&type_text);
                self.write(";");
                self.write_line();
            }
        }
    }

    pub(in crate::declaration_emitter) fn js_define_property_getter_return_type_text(
        &mut self,
        getter_idx: NodeIndex,
    ) -> Option<String> {
        let getter_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(getter_idx);
        let getter_node = self.arena.get(getter_idx)?;
        if getter_node.kind == syntax_kind_ext::METHOD_DECLARATION {
            let method = self.arena.get_method_decl(getter_node)?;
            if method.type_annotation.is_some() {
                return self.type_node_text(method.type_annotation);
            }
            if let Some(type_text) = self.jsdoc_return_type_text_for_node(getter_idx) {
                return Some(type_text);
            }
            return self
                .function_body_preferred_return_type_text(method.body)
                .or_else(|| {
                    self.get_node_type_or_names(&[getter_idx, method.name])
                        .map(|type_id| self.print_type_id(type_id))
                });
        }

        let func = self.js_define_property_function_data(getter_idx)?.clone();
        if func.type_annotation.is_some() {
            return self.type_node_text(func.type_annotation);
        }
        if let Some(type_text) = self.jsdoc_return_type_text_for_node(getter_idx) {
            return Some(type_text);
        }
        self.function_body_preferred_return_type_text(func.body)
            .or_else(|| {
                self.get_node_type_or_names(&[getter_idx])
                    .map(|type_id| self.print_type_id(type_id))
            })
    }

    pub(in crate::declaration_emitter) fn js_define_property_setter_parameter_text(
        &mut self,
        setter: crate::declaration_emitter::helpers::JsClassDefinePropertySetter,
    ) -> (String, String) {
        let initializer = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(setter.initializer);
        if let Some((name, type_text)) = self.js_define_property_setter_parameter_from_function(
            initializer,
            setter.preserve_param_name,
        ) {
            return (name, type_text);
        }
        if let Some(type_text) =
            self.js_define_property_setter_parameter_type_from_expression(initializer)
        {
            return ("value".to_string(), type_text);
        }
        ("value".to_string(), "any".to_string())
    }

    pub(in crate::declaration_emitter) fn js_define_property_setter_parameter_type_from_expression(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        if let Some(local_name) = self.get_identifier_text(expr_idx)
            && let Some(initializer) = self.js_top_level_variable_initializer(&local_name)
        {
            return self
                .js_define_property_setter_parameter_text(
                    crate::declaration_emitter::helpers::JsClassDefinePropertySetter {
                        initializer,
                        preserve_param_name: false,
                    },
                )
                .1
                .into();
        }

        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION {
            let cond = self.arena.get_conditional_expr(expr_node)?;
            let left =
                self.js_define_property_setter_parameter_type_from_expression(cond.when_true);
            let right =
                self.js_define_property_setter_parameter_type_from_expression(cond.when_false);
            return match (left, right) {
                (Some(left), Some(right)) if left == right => Some(left),
                (Some(_), Some(_)) => Some("any".to_string()),
                _ => None,
            };
        }

        None
    }

    pub(in crate::declaration_emitter) fn js_define_property_setter_parameter_from_function(
        &mut self,
        func_idx: NodeIndex,
        preserve_param_name: bool,
    ) -> Option<(String, String)> {
        let func_node = self.arena.get(func_idx)?;
        if func_node.kind == syntax_kind_ext::METHOD_DECLARATION {
            let method = self.arena.get_method_decl(func_node)?;
            return Some(self.js_define_property_setter_parameter_from_params(
                &method.parameters,
                method.body,
                preserve_param_name,
            ));
        }

        let func = self.js_define_property_function_data(func_idx)?.clone();
        Some(self.js_define_property_setter_parameter_from_params(
            &func.parameters,
            func.body,
            preserve_param_name,
        ))
    }

    pub(in crate::declaration_emitter) fn js_define_property_setter_parameter_from_params(
        &mut self,
        params: &NodeList,
        _body_idx: NodeIndex,
        preserve_param_name: bool,
    ) -> (String, String) {
        let Some(&param_idx) = params.nodes.first() else {
            return ("value".to_string(), "any".to_string());
        };
        let Some(param_node) = self.arena.get(param_idx) else {
            return ("value".to_string(), "any".to_string());
        };
        let Some(param) = self.arena.get_parameter(param_node) else {
            return ("value".to_string(), "any".to_string());
        };
        let name = if preserve_param_name {
            self.get_identifier_text(param.name)
                .unwrap_or_else(|| "value".to_string())
        } else {
            "value".to_string()
        };
        let mut type_text = if param.type_annotation.is_some() {
            self.type_node_text(param.type_annotation)
        } else if let Some(jsdoc_param) = self.jsdoc_param_decl_for_parameter(param_idx, 0) {
            Some(jsdoc_param.type_text)
        } else {
            self.get_node_type_or_names(&[param_idx, param.name])
                .map(|type_id| self.print_type_id(type_id))
        }
        .unwrap_or_else(|| "any".to_string());

        if param.dot_dot_dot_token {
            if let Some(element_type) = type_text.strip_suffix("[]") {
                type_text = element_type.to_string();
            }
        }

        (name, type_text)
    }

    pub(in crate::declaration_emitter) fn js_define_property_function_data(
        &self,
        func_idx: NodeIndex,
    ) -> Option<&FunctionData> {
        let func_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(func_idx);
        let func_node = self.arena.get(func_idx)?;
        if func_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            && func_node.kind != syntax_kind_ext::ARROW_FUNCTION
        {
            return None;
        }
        self.arena.get_function(func_node)
    }

    pub(in crate::declaration_emitter) fn js_top_level_variable_initializer(
        &self,
        name: &str,
    ) -> Option<NodeIndex> {
        self.js_top_level_variable_initializer_info(name)
            .map(|(initializer, _)| initializer)
    }

    pub(in crate::declaration_emitter) fn js_top_level_variable_initializer_info(
        &self,
        name: &str,
    ) -> Option<(NodeIndex, bool)> {
        let root_idx = self.current_source_file_idx?;
        let root_node = self.arena.get(root_idx)?;
        let source_file = self.arena.get_source_file(root_node)?;
        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;
            let (var_node, is_exported) = if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                (stmt_node, false)
            } else if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                let export = self.arena.get_export_decl(stmt_node)?;
                let export_clause_node = self.arena.get(export.export_clause)?;
                if export_clause_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                    continue;
                }
                (export_clause_node, true)
            } else {
                continue;
            };
            let var_stmt = self.arena.get_variable(var_node)?;
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let decl_list_node = self.arena.get(decl_list_idx)?;
                let decl_list = self.arena.get_variable(decl_list_node)?;
                for &decl_idx in &decl_list.declarations.nodes {
                    let decl_node = self.arena.get(decl_idx)?;
                    let decl = self.arena.get_variable_declaration(decl_node)?;
                    if self.get_identifier_text(decl.name).as_deref() == Some(name) {
                        return decl
                            .initializer
                            .into_option()
                            .map(|init| (init, is_exported));
                    }
                }
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn type_node_text(
        &mut self,
        type_idx: NodeIndex,
    ) -> Option<String> {
        if !type_idx.is_some() {
            return None;
        }
        let saved_comment_idx = self.comment_emit_idx;
        let saved_pending_source_pos = self.pending_source_pos;
        let saved_writer = std::mem::take(&mut self.writer);
        self.emit_type(type_idx);
        let type_writer = std::mem::replace(&mut self.writer, saved_writer);
        self.comment_emit_idx = saved_comment_idx;
        self.pending_source_pos = saved_pending_source_pos;
        Some(type_writer.take_output())
    }

    /// Emit a `declare class X { private constructor(); ... }` for a variable
    /// that has `.prototype` member assignments (the JS class-like heuristic).
    pub(in crate::declaration_emitter) fn emit_js_class_like_heuristic_if_needed(
        &mut self,
        name_idx: NodeIndex,
        is_exported: bool,
    ) -> bool {
        let Some(name) = self.get_identifier_text(name_idx) else {
            return false;
        };
        let Some(members) = self.js_class_like_prototype_members.get(&name).cloned() else {
            return false;
        };
        if members.is_empty() {
            return false;
        }

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("class ");
        self.emit_node(name_idx);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        self.write_indent();
        self.write("private constructor();");
        self.write_line();

        for (member_name, initializer) in &members {
            self.emit_js_class_like_member(*member_name, *initializer);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        true
    }

    pub(in crate::declaration_emitter) fn emit_js_class_like_member(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
    ) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if init_node.kind == syntax_kind_ext::ARROW_FUNCTION
            || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
        {
            self.emit_js_synthetic_class_method(name_idx, initializer);
            return;
        }

        self.write_indent();
        self.emit_node(name_idx);
        self.write(": ");

        if let Some(resolved_type) =
            self.resolve_declaration_type_text(&[initializer], Some(initializer))
        {
            self.write(&resolved_type.emitted_type_text);
        } else if let Some(type_text) = self.allowlisted_initializer_type_text(initializer) {
            self.write(&type_text);
        } else {
            self.write("any");
        }
        self.write(";");
        self.write_line();
    }

    /// Emit a function/arrow initializer's type annotation directly from the AST
    /// when it has an explicit return type. This preserves source-level type alias
    /// references in type parameter constraints and binding pattern formatting
    /// (including trailing commas) that the `TypePrinter` would otherwise expand.
    pub(in crate::declaration_emitter) fn emit_arrow_fn_type_from_ast(
        &mut self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return false;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return false;
        };
        if func.type_annotation.is_none() {
            return false;
        }
        self.write(": ");
        // Emit type parameters from AST
        if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }
        // Emit parameters from AST
        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write(") => ");
        // Emit return type from AST
        self.emit_type(func.type_annotation);
        true
    }

    pub(in crate::declaration_emitter) fn is_js_object_literal_namespace_candidate(
        &self,
        decl_name: NodeIndex,
        initializer: NodeIndex,
    ) -> bool {
        if !self.source_is_js_file || !initializer.is_some() {
            return false;
        }

        let Some(name_node) = self.arena.get(decl_name) else {
            return false;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

        self.js_object_literal_initializer_has_namespace_shape(initializer, true)
    }

    pub(in crate::declaration_emitter) fn js_object_literal_initializer_has_namespace_shape(
        &self,
        initializer: NodeIndex,
        allow_property_references: bool,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(object) = self.arena.get_literal_expr(init_node) else {
            return false;
        };
        if object.elements.nodes.is_empty() {
            return false;
        }

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.arena.get_property_assignment(member_node) else {
                        return false;
                    };
                    let Some(prop_name_node) = self.arena.get(prop.name) else {
                        return false;
                    };
                    if prop_name_node.kind != SyntaxKind::Identifier as u16 {
                        return false;
                    }
                    if !allow_property_references
                        && self.arena.get(prop.initializer).is_some_and(|node| {
                            node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        })
                    {
                        return false;
                    }
                    if !self.js_namespace_object_member_initializer_supported(prop.initializer) {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        return false;
                    };
                    let Some(method_name_node) = self.arena.get(method.name) else {
                        return false;
                    };
                    if method_name_node.kind != SyntaxKind::Identifier as u16 {
                        return false;
                    }
                }
                _ => return false,
            }
        }

        true
    }

    pub(in crate::declaration_emitter) fn statement_emits_js_object_literal_namespace(
        &self,
        stmt_idx: NodeIndex,
    ) -> bool {
        if !self.source_is_js_file {
            return false;
        }
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return false;
        }
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return false;
        };

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            if decl_list_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                continue;
            }
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };
            if decl_list.declarations.nodes.len() != 1 {
                continue;
            }
            let Some(&decl_idx) = decl_list.declarations.nodes.first() else {
                continue;
            };
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if self.is_js_export_equals_name(decl.name) {
                continue;
            }
            if self.jsdoc_type_text_for_node(decl_idx).is_some()
                || self.jsdoc_type_text_for_node(decl.name).is_some()
            {
                continue;
            }
            if self.is_js_object_literal_namespace_candidate(decl.name, decl.initializer) {
                return true;
            }
        }

        false
    }

    pub(in crate::declaration_emitter) fn emit_js_object_literal_namespace_if_possible(
        &mut self,
        decl_name: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
    ) -> bool {
        if self.is_js_export_equals_name(decl_name) {
            return false;
        }
        if !self.is_js_object_literal_namespace_candidate(decl_name, initializer) {
            return false;
        }
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        let Some(object) = self.arena.get_literal_expr(init_node) else {
            return false;
        };

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("namespace ");
        self.emit_node(decl_name);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.arena.get_property_assignment(member_node) else {
                        continue;
                    };
                    let Some(init_node) = self.arena.get(prop.initializer) else {
                        continue;
                    };
                    if init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                        || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    {
                        let overload_signatures =
                            self.jsdoc_overload_signatures_for_node(prop.initializer);
                        if self.emit_jsdoc_overload_namespace_function_signatures(
                            prop.name,
                            member_idx,
                            &overload_signatures,
                        ) {
                            continue;
                        }
                        let Some(func) = self.arena.get_function(init_node) else {
                            continue;
                        };
                        self.emit_js_namespace_function_member(
                            prop.name,
                            func.type_parameters.as_ref(),
                            &func.parameters,
                            func.body,
                            func.type_annotation,
                        );
                    } else if let Some(reference_text) =
                        self.js_namespace_property_reference_text(prop.initializer)
                    {
                        self.emit_js_namespace_import_alias_member(prop.name, &reference_text);
                    } else if let Some(type_text) =
                        self.js_namespace_value_member_type_text(prop.initializer)
                    {
                        self.record_js_require_property_import_alias_for_new_expression(
                            prop.initializer,
                        );
                        self.emit_js_namespace_value_member(prop.name, &type_text);
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    self.emit_js_namespace_function_member(
                        method.name,
                        method.type_parameters.as_ref(),
                        &method.parameters,
                        method.body,
                        method.type_annotation,
                    );
                }
                _ => {}
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        self.emit_jsdoc_callback_type_aliases_for_object_literal_namespace(
            initializer,
            is_exported,
        );
        true
    }

    pub(in crate::declaration_emitter) fn has_synthetic_js_expression_declaration(
        &self,
        stmt_idx: NodeIndex,
    ) -> bool {
        self.js_deferred_function_export_statements
            .contains_key(&stmt_idx)
            || self
                .js_deferred_value_export_statements
                .contains_key(&stmt_idx)
            || self
                .js_static_method_augmentation_statements
                .contains_key(&stmt_idx)
            || self
                .js_named_export_equals_class_expression(stmt_idx)
                .is_some()
            || self
                .js_anonymous_export_equals_class_expression_initializer(stmt_idx)
                .is_some()
            || self
                .js_anonymous_module_exports_named_members_initializer(stmt_idx)
                .is_some()
            || self
                .js_anonymous_export_equals_value_initializer(stmt_idx)
                .is_some()
            || self
                .js_commonjs_define_property_export_for_statement(stmt_idx)
                .is_some()
            || self
                .js_commonjs_define_property_namespace_member_for_statement(stmt_idx)
                .is_some()
    }

    pub(in crate::declaration_emitter) fn emit_js_synthetic_expression_statement(
        &mut self,
        stmt_idx: NodeIndex,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        if let Some((name_idx, initializer, is_exported)) = self
            .js_deferred_function_export_statements
            .get(&stmt_idx)
            .copied()
        {
            self.emit_js_synthetic_function_declaration(name_idx, initializer, is_exported);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        if let Some((name_idx, initializer, is_exported)) = self
            .js_deferred_value_export_statements
            .get(&stmt_idx)
            .copied()
        {
            self.emit_js_synthetic_value_declaration(name_idx, initializer, is_exported);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        if let Some(property) = self.js_commonjs_define_property_export_for_statement(stmt_idx) {
            self.emit_js_commonjs_define_property_export(&property);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        if let Some((root_name, property)) =
            self.js_commonjs_define_property_namespace_member_for_statement(stmt_idx)
        {
            self.emit_js_commonjs_define_property_namespace_member(&root_name, &property);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        if let Some(group) = self
            .js_static_method_augmentation_statements
            .get(&stmt_idx)
            .cloned()
        {
            self.emit_js_static_method_augmentation_namespace(&group);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        if let Some((name_idx, initializer)) =
            self.js_named_export_equals_class_expression(stmt_idx)
        {
            self.emit_pending_js_export_equals_for_name(name_idx);
            let _ = self.emit_js_named_class_expression_declaration(name_idx, initializer, false);
            self.emit_js_namespace_export_aliases_for_name(name_idx, false);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        if let Some(initializer) =
            self.js_anonymous_export_equals_class_expression_initializer(stmt_idx)
        {
            self.emit_js_anonymous_export_equals_class_expression_declaration(initializer);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        if let Some(initializer) =
            self.js_anonymous_module_exports_named_members_initializer(stmt_idx)
        {
            self.emit_js_anonymous_module_exports_named_members(initializer);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
            return;
        }

        if let Some(initializer) = self.js_anonymous_export_equals_value_initializer(stmt_idx) {
            self.emit_js_anonymous_export_equals_value_declaration(initializer);
            self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
        }
    }

    pub(in crate::declaration_emitter) fn js_anonymous_export_equals_value_initializer(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        if self.js_module_exports_object_stmts.contains(&stmt_idx) {
            return None;
        }
        if self
            .js_anonymous_export_equals_class_expression_initializer(stmt_idx)
            .is_some()
            || self
                .js_anonymous_module_exports_named_members_initializer(stmt_idx)
                .is_some()
        {
            return None;
        }
        let initializer = self.js_anonymous_module_exports_assignment_initializer(stmt_idx)?;
        if self.is_js_function_initializer(initializer) {
            return Some(initializer);
        }
        self.js_synthetic_export_value_type_text(initializer)
            .map(|_| initializer)
    }

    pub(in crate::declaration_emitter) fn emit_js_anonymous_export_equals_value_declaration(
        &mut self,
        initializer: NodeIndex,
    ) {
        let export_name = if self.reserved_names.contains("_exports") {
            self.generate_unique_name("_exports")
        } else {
            "_exports".to_string()
        };
        self.reserved_names.insert(export_name.clone());

        if self.emit_js_anonymous_export_equals_function_declaration(initializer, &export_name) {
            return;
        }

        let Some(type_text) = self.js_synthetic_export_value_type_text(initializer) else {
            return;
        };
        self.record_js_require_property_import_alias_for_new_expression(initializer);
        let require_alias_import = self
            .js_require_alias_property_access_parts(initializer)
            .map(|(local_name, _, module_name)| (local_name, module_name));

        self.write_indent();
        self.write("declare ");
        self.write(self.js_synthetic_export_value_keyword(initializer));
        self.write(&export_name);
        self.write(": ");
        self.write(&type_text);
        self.write(";");
        self.write_line();

        self.write_indent();
        self.write("export = ");
        self.write(&export_name);
        self.write(";");
        self.write_line();
        if let Some((local_name, module_name)) = require_alias_import {
            self.emit_js_bare_require_alias_import(&local_name, &module_name);
        }
        self.emitted_scope_marker = true;
        self.emitted_module_indicator = true;
    }

    pub(in crate::declaration_emitter) fn emit_js_anonymous_export_equals_function_declaration(
        &mut self,
        initializer: NodeIndex,
        export_name: &str,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if !self.is_js_function_initializer(initializer) {
            return false;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return false;
        };

        let jsdoc_aliases = self.jsdoc_type_alias_decls_before_pos(init_node.pos);
        if !jsdoc_aliases.is_empty() {
            self.write_indent();
            self.write("declare namespace ");
            self.write(export_name);
            self.write(" {");
            self.write_line();
            self.increase_indent();
            self.write_indent();
            self.write("export { ");
            for (idx, alias) in jsdoc_aliases.iter().enumerate() {
                if idx > 0 {
                    self.write(", ");
                }
                self.write(&alias.name);
            }
            self.write(" };");
            self.write_line();
            self.decrease_indent();
            self.write_indent();
            self.write("}");
            self.write_line();
        }

        let jsdoc = self.function_like_jsdoc_for_node(initializer);
        self.write_indent();
        self.write("declare function ");
        self.write(export_name);
        self.write("(");
        let saved_comment_idx = self.comment_emit_idx;
        self.comment_emit_idx = self
            .all_comments
            .iter()
            .position(|comment| comment.end > init_node.pos)
            .unwrap_or(self.all_comments.len());
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.comment_emit_idx = saved_comment_idx;
        self.write(")");

        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = jsdoc
            .as_deref()
            .and_then(Self::parse_jsdoc_return_type_text)
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let (Some(return_type_text), true) =
            self.function_body_return_hint(func, func.body)
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let Some(return_type_text) = self
            .js_function_body_preferred_return_text_for_declaration(
                func.body,
                initializer,
                &func.parameters,
            )
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write(": void");
        } else {
            self.write(": any");
        }
        self.write(";");
        self.write_line();

        self.write_indent();
        self.write("export = ");
        self.write(export_name);
        self.write(";");
        self.write_line();

        for alias in jsdoc_aliases {
            self.emit_rendered_jsdoc_type_alias(alias, false);
        }

        self.emitted_scope_marker = true;
        self.emitted_module_indicator = true;
        true
    }

    pub(in crate::declaration_emitter) fn js_commonjs_export_assignment_closure(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> Option<JsCommonJsClosureExport> {
        if !self.source_file_is_js(source_file) {
            return None;
        }

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                continue;
            }
            let Some(func) = self.arena.get_function(stmt_node) else {
                continue;
            };
            let Some(block_node) = self.arena.get(func.body) else {
                continue;
            };
            let Some(block) = self.arena.get_block(block_node) else {
                continue;
            };
            let Some(root_initializer) =
                block
                    .statements
                    .nodes
                    .iter()
                    .copied()
                    .find_map(|inner_stmt_idx| {
                        self.js_module_exports_exports_assignment_initializer(inner_stmt_idx)
                    })
            else {
                continue;
            };

            let mut secondary_members = Vec::new();
            for &inner_stmt_idx in &block.statements.nodes {
                let Some((export_name_idx, local_name_idx, local_initializer)) =
                    self.js_exports_closure_secondary_member(inner_stmt_idx, &block.statements)
                else {
                    continue;
                };
                secondary_members.push((export_name_idx, local_name_idx, local_initializer));
            }

            return Some((stmt_idx, root_initializer, secondary_members));
        }
        None
    }

    pub(in crate::declaration_emitter) fn js_module_exports_exports_assignment_initializer(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let stmt_node = self.arena.get(stmt_idx)?;
        if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return None;
        }
        let expr_stmt = self.arena.get_expression_statement(stmt_node)?;
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_stmt.expression);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::EqualsToken as u16
            || !self.is_module_exports_reference(binary.left)
        {
            return None;
        }

        let rhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        let rhs_node = self.arena.get(rhs)?;
        if rhs_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let rhs_binary = self.arena.get_binary_expr(rhs_node)?;
        if rhs_binary.operator_token != SyntaxKind::EqualsToken as u16
            || !self.is_exports_identifier_reference(rhs_binary.left)
        {
            return None;
        }
        let initializer = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(rhs_binary.right);
        if self.is_js_function_initializer(initializer) {
            Some(initializer)
        } else {
            None
        }
    }

    pub(in crate::declaration_emitter) fn js_exports_closure_secondary_member(
        &self,
        stmt_idx: NodeIndex,
        statements: &NodeList,
    ) -> Option<(NodeIndex, NodeIndex, NodeIndex)> {
        let (export_name_idx, initializer) =
            self.js_commonjs_named_export_for_statement_with_options(stmt_idx, false)?;
        let local_name = self.get_identifier_text(initializer)?;
        let (local_name_idx, local_initializer) =
            self.js_closure_local_function_initializer(statements, &local_name)?;
        Some((export_name_idx, local_name_idx, local_initializer))
    }

    pub(in crate::declaration_emitter) fn js_closure_local_function_initializer(
        &self,
        statements: &NodeList,
        name: &str,
    ) -> Option<(NodeIndex, NodeIndex)> {
        for &stmt_idx in &statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }
            let var_stmt = self.arena.get_variable(stmt_node)?;
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let decl_list_node = self.arena.get(decl_list_idx)?;
                let decl_list = self.arena.get_variable(decl_list_node)?;
                for &decl_idx in &decl_list.declarations.nodes {
                    let decl_node = self.arena.get(decl_idx)?;
                    let decl = self.arena.get_variable_declaration(decl_node)?;
                    if self.get_identifier_text(decl.name).as_deref() == Some(name)
                        && self.is_js_function_initializer(decl.initializer)
                    {
                        return Some((decl.name, decl.initializer));
                    }
                }
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn emit_js_commonjs_closure_export_assignment(
        &mut self,
        root_initializer: NodeIndex,
        secondary_members: &[JsCommonJsClosureSecondaryMember],
    ) {
        let export_name = if self.reserved_names.contains("_exports") {
            self.generate_unique_name("_exports")
        } else {
            "_exports".to_string()
        };
        self.reserved_names.insert(export_name.clone());

        self.emit_js_commonjs_closure_export_function(root_initializer, &export_name);
        if !secondary_members.is_empty() {
            self.write_indent();
            self.write("declare namespace ");
            self.write(&export_name);
            self.write(" {");
            self.write_line();
            self.increase_indent();
            self.write_indent();
            self.write("export { ");
            for (idx, (export_name_idx, local_name_idx, _)) in secondary_members.iter().enumerate()
            {
                if idx > 0 {
                    self.write(", ");
                }
                self.emit_node(*local_name_idx);
                self.write(" as ");
                self.emit_node(*export_name_idx);
            }
            self.write(" };");
            self.write_line();
            self.decrease_indent();
            self.write_indent();
            self.write("}");
            self.write_line();
        }

        self.write_indent();
        self.write("export = ");
        self.write(&export_name);
        self.write(";");
        self.write_line();

        for (_, local_name_idx, local_initializer) in secondary_members {
            self.emit_js_synthetic_function_declaration(*local_name_idx, *local_initializer, false);
        }
        self.emitted_scope_marker = true;
        self.emitted_module_indicator = true;
    }

    pub(in crate::declaration_emitter) fn emit_js_commonjs_closure_export_function(
        &mut self,
        initializer: NodeIndex,
        export_name: &str,
    ) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        let Some(func) = self.arena.get_function(init_node) else {
            return;
        };

        let jsdoc = self.function_like_jsdoc_for_node(initializer);
        self.write_indent();
        self.write("declare function ");
        self.write(export_name);
        self.write("(");
        let saved_comment_idx = self.comment_emit_idx;
        self.comment_emit_idx = self
            .all_comments
            .iter()
            .position(|comment| comment.end > init_node.pos)
            .unwrap_or(self.all_comments.len());
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.comment_emit_idx = saved_comment_idx;
        self.write(")");

        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = jsdoc
            .as_deref()
            .and_then(Self::parse_jsdoc_return_type_text)
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let (Some(return_type_text), true) =
            self.function_body_return_hint(func, func.body)
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let Some(return_type_text) = self
            .js_function_body_preferred_return_text_for_declaration(
                func.body,
                initializer,
                &func.parameters,
            )
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write(": void");
        } else {
            self.write(": any");
        }
        self.write(";");
        self.write_line();
    }
}
