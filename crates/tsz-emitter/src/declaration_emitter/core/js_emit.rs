use rustc_hash::FxHashSet;
use tsz_binder::{BinderState, SymbolId};
use tsz_parser::parser::node::FunctionData;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::{SyntaxKind, string_to_token, token_is_reserved_word};
use tsz_solver::type_queries;

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

    fn js_define_property_getter_return_type_text(
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

    fn js_define_property_setter_parameter_text(
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

    fn js_define_property_setter_parameter_type_from_expression(
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

    fn js_define_property_setter_parameter_from_function(
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

    fn js_define_property_setter_parameter_from_params(
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

    fn js_define_property_function_data(&self, func_idx: NodeIndex) -> Option<&FunctionData> {
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

    fn js_top_level_variable_initializer(&self, name: &str) -> Option<NodeIndex> {
        let root_idx = self.current_source_file_idx?;
        let root_node = self.arena.get(root_idx)?;
        let source_file = self.arena.get_source_file(root_node)?;
        for &stmt_idx in &source_file.statements.nodes {
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
                    if self.get_identifier_text(decl.name).as_deref() == Some(name) {
                        return decl.initializer.into_option();
                    }
                }
            }
        }
        None
    }

    fn type_node_text(&mut self, type_idx: NodeIndex) -> Option<String> {
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
        self.emitted_scope_marker = true;
        self.emitted_module_indicator = true;
    }

    fn emit_js_anonymous_export_equals_function_declaration(
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

    fn js_module_exports_exports_assignment_initializer(
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

    fn js_exports_closure_secondary_member(
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

    fn js_closure_local_function_initializer(
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

    fn emit_js_commonjs_closure_export_function(
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

    pub(in crate::declaration_emitter) fn emit_js_synthetic_function_declaration(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
    ) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if !self.is_js_function_initializer(initializer) {
            return;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return;
        };

        let jsdoc = self.function_like_jsdoc_for_node(initializer);
        if is_exported
            && jsdoc
                .as_deref()
                .is_some_and(|jsdoc| jsdoc.contains("@constructor"))
            && self.emit_js_commonjs_constructor_prototype_class(name_idx)
        {
            return;
        }
        let reserved_export_alias = if is_exported {
            self.js_reserved_export_local_name(name_idx)
        } else {
            None
        };

        if let Some((export_name, local_name)) = reserved_export_alias {
            self.write_indent();
            self.write("declare ");
            self.write("function ");
            self.write(&local_name);
            self.write("(");
            self.emit_parameters_with_body(&func.parameters, func.body);
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
            } else if let Some(return_type_text) = self
                .js_function_body_preferred_return_text_for_declaration(
                    func.body,
                    name_idx,
                    &func.parameters,
                )
            {
                self.write(": ");
                self.write(&return_type_text);
            } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
                let func_type_id = cache
                    .node_types
                    .get(&initializer.0)
                    .copied()
                    .or_else(|| self.get_node_type_or_names(&[name_idx, initializer]));
                if let Some(func_type_id) = func_type_id
                    && let Some(return_type_id) =
                        type_queries::get_return_type(*interner, func_type_id)
                {
                    if return_type_id == tsz_solver::types::TypeId::ANY
                        && func.body.is_some()
                        && self.body_returns_void(func.body)
                    {
                        self.write(": void");
                    } else {
                        self.write(": ");
                        self.write(&self.print_type_id(return_type_id));
                    }
                } else if func.body.is_some() && self.body_returns_void(func.body) {
                    self.write(": void");
                }
            } else if func.body.is_some() && self.body_returns_void(func.body) {
                self.write(": void");
            }

            self.write(";");
            self.write_line();

            self.write_indent();
            if export_name == "default" {
                self.write("export default ");
                self.write(&local_name);
            } else {
                self.write("export { ");
                self.write(&local_name);
                self.write(" as ");
                self.write(&export_name);
                self.write(" }");
            }
            self.write(";");
            self.write_line();
            self.emitted_module_indicator = true;
            return;
        }

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("function ");
        self.emit_node(name_idx);

        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            jsdoc
                .as_deref()
                .map(Self::parse_jsdoc_template_params)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if let Some(ref type_params) = func.type_parameters {
            if !type_params.nodes.is_empty() {
                self.emit_type_parameters(type_params);
            } else if !jsdoc_template_params.is_empty() {
                self.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        } else if !jsdoc_template_params.is_empty() {
            self.emit_jsdoc_template_parameters(&jsdoc_template_params);
        }

        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
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
        } else if let Some(return_type_text) = self
            .js_function_body_preferred_return_text_for_declaration(
                func.body,
                name_idx,
                &func.parameters,
            )
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            let func_type_id = cache
                .node_types
                .get(&initializer.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[name_idx, initializer]));
            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) = type_queries::get_return_type(*interner, func_type_id)
            {
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && func.body.is_some()
                    && self.body_returns_void(func.body)
                {
                    self.write(": void");
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(return_type_id));
                }
            } else if func.body.is_some() && self.body_returns_void(func.body) {
                self.write(": void");
            }
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write(": void");
        }

        self.write(";");
        self.write_line();
        let late_bound_members = self.collect_ts_late_bound_assignment_members(name_idx);
        self.emit_ts_late_bound_function_namespace_from_members(
            name_idx,
            is_exported,
            &late_bound_members,
        );
        self.emit_js_function_like_class_if_needed(
            name_idx,
            &func.parameters,
            func.body,
            is_exported,
            initializer,
        );
        if is_exported {
            self.emit_js_namespace_export_aliases_for_name(name_idx, true);
            self.emitted_module_indicator = true;
        }
    }

    pub(in crate::declaration_emitter) fn emit_js_synthetic_value_declaration(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
    ) {
        if is_exported {
            let object_initializer = self
                .get_identifier_text(initializer)
                .and_then(|local_name| self.js_top_level_variable_initializer(&local_name))
                .unwrap_or(initializer);
            if self.emit_js_object_literal_namespace(name_idx, object_initializer, true, false) {
                return;
            }
        }

        if self.emit_js_synthetic_class_expression_declaration(name_idx, initializer, is_exported) {
            return;
        }

        let type_text = if is_exported {
            self.js_synthetic_export_value_type_text(initializer)
        } else {
            self.js_namespace_value_member_type_text(initializer)
        };
        let Some(type_text) = type_text else {
            return;
        };

        self.write_indent();
        let reserved_export_alias = if is_exported {
            self.js_reserved_export_local_name(name_idx)
        } else {
            None
        };
        if is_exported && reserved_export_alias.is_none() {
            self.write("export ");
            self.write(self.js_synthetic_export_value_keyword(initializer));
        } else {
            if self.should_emit_declare_keyword(false) {
                self.write("declare ");
            }
            if is_exported {
                self.write(self.js_synthetic_export_value_keyword(initializer));
            } else {
                self.write("var ");
            }
        }
        if let Some((export_name, local_name)) = reserved_export_alias {
            self.write(&local_name);
            self.js_cjs_export_aliases.push((export_name, local_name));
        } else if let Some(export_name) = self.js_commonjs_export_name_text(name_idx) {
            self.write(&export_name);
        } else {
            self.emit_node(name_idx);
        }
        self.write(": ");
        self.write(&type_text);
        self.write(";");
        self.write_line();
        if is_exported {
            self.emitted_module_indicator = true;
        }
    }

    fn js_reserved_export_local_name(&mut self, name_idx: NodeIndex) -> Option<(String, String)> {
        let export_name = self.get_identifier_text(name_idx)?;
        if !token_is_reserved_word(string_to_token(&export_name)) {
            return None;
        }

        let base = format!("_{export_name}");
        let local_name = if self.reserved_names.contains(&base) {
            self.generate_unique_name(&base)
        } else {
            base
        };
        self.reserved_names.insert(local_name.clone());
        Some((export_name, local_name))
    }

    pub(in crate::declaration_emitter) fn emit_js_commonjs_define_property_export(
        &mut self,
        property: &super::super::helpers::JsDefinedPropertyDecl,
    ) {
        self.write_indent();
        self.write("export const ");
        self.write(&property.name);
        self.write(": ");
        self.write(&property.type_text);
        self.write(";");
        self.write_line();
        self.emitted_module_indicator = true;
    }

    pub(in crate::declaration_emitter) fn emit_js_named_class_expression_declaration(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return false;
        }
        let Some(class) = self.arena.get_class(init_node) else {
            return false;
        };

        let Some(export_name) = self.get_identifier_text(name_idx) else {
            return false;
        };
        if class.name.is_some()
            && let Some(class_name) = self.get_identifier_text(class.name)
            && class_name != export_name
        {
            return false;
        }

        let is_abstract = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword);

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        if is_abstract {
            self.write("abstract ");
        }
        self.write("class ");
        self.write(&export_name);

        if let Some(ref type_params) = class.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }
        if let Some(ref heritage) = class.heritage_clauses {
            self.emit_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        self.class_has_constructor_overloads = false;
        self.class_extends_another = class.heritage_clauses.as_ref().is_some_and(|hc| {
            hc.nodes.iter().any(|&clause_idx| {
                self.arena
                    .get_heritage_clause_at(clause_idx)
                    .is_some_and(|h| h.token == SyntaxKind::ExtendsKeyword as u16)
            })
        });
        self.method_names_with_overloads = FxHashSet::default();

        self.emit_parameter_properties(&class.members);
        if self.class_has_private_identifier_member(&class.members) {
            self.write_indent();
            self.write("#private;");
            self.write_line();
        }

        self.emit_js_array_subclass_constructor_overloads_if_needed(
            &class.members,
            class.heritage_clauses.as_ref(),
        );
        for &member_idx in &class.members.nodes {
            let before_jsdoc_len = self.writer.len();
            let saved_comment_idx = self.comment_emit_idx;
            if let Some(member_node) = self.arena.get(member_idx) {
                self.emit_leading_jsdoc_comments(member_node.pos);
            }
            let before_member_len = self.writer.len();
            self.emit_class_member(member_idx);
            if self.writer.len() == before_member_len {
                self.writer.truncate(before_jsdoc_len);
                self.comment_emit_idx = saved_comment_idx;
                if let Some(member_node) = self.arena.get(member_idx) {
                    self.skip_comments_in_node(member_node.pos, member_node.end);
                }
            }
        }
        self.emit_js_inferred_constructor_assignment_properties(&class.members);

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        if is_exported {
            self.emitted_module_indicator = true;
        }
        true
    }

    pub(in crate::declaration_emitter) fn emit_js_anonymous_export_equals_class_expression_declaration(
        &mut self,
        initializer: NodeIndex,
    ) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return;
        }
        let Some(class) = self.arena.get_class(init_node) else {
            return;
        };

        self.write_indent();
        self.write("export = exports;");
        self.write_line();
        self.emitted_scope_marker = true;
        self.emitted_module_indicator = true;

        let is_abstract = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword);

        self.write_indent();
        if self.should_emit_declare_keyword(false) {
            self.write("declare ");
        }
        if is_abstract {
            self.write("abstract ");
        }
        self.write("class exports");

        if let Some(ref type_params) = class.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }
        if let Some(ref heritage) = class.heritage_clauses {
            self.emit_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        self.class_has_constructor_overloads = false;
        self.class_extends_another = class.heritage_clauses.as_ref().is_some_and(|hc| {
            hc.nodes.iter().any(|&clause_idx| {
                self.arena
                    .get_heritage_clause_at(clause_idx)
                    .is_some_and(|h| h.token == SyntaxKind::ExtendsKeyword as u16)
            })
        });
        self.method_names_with_overloads = FxHashSet::default();

        self.emit_parameter_properties(&class.members);
        if self.class_has_private_identifier_member(&class.members) {
            self.write_indent();
            self.write("#private;");
            self.write_line();
        }

        self.emit_js_array_subclass_constructor_overloads_if_needed(
            &class.members,
            class.heritage_clauses.as_ref(),
        );
        for &member_idx in &class.members.nodes {
            let before_jsdoc_len = self.writer.len();
            let saved_comment_idx = self.comment_emit_idx;
            if let Some(member_node) = self.arena.get(member_idx) {
                self.emit_leading_jsdoc_comments(member_node.pos);
            }
            let before_member_len = self.writer.len();
            self.emit_class_member(member_idx);
            if self.writer.len() == before_member_len {
                self.writer.truncate(before_jsdoc_len);
                self.comment_emit_idx = saved_comment_idx;
                if let Some(member_node) = self.arena.get(member_idx) {
                    self.skip_comments_in_node(member_node.pos, member_node.end);
                }
            }
        }
        self.emit_js_inferred_constructor_assignment_properties(&class.members);

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        self.emit_js_anonymous_export_equals_class_secondary_members(initializer);
    }

    pub(in crate::declaration_emitter) fn emit_js_synthetic_class_expression_declaration(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
    ) -> bool {
        self.emit_js_named_class_expression_declaration(name_idx, initializer, is_exported)
    }

    pub(in crate::declaration_emitter) fn emit_js_anonymous_module_exports_named_members(
        &mut self,
        initializer: NodeIndex,
    ) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };

        match init_node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.emit_js_anonymous_module_exports_object_members(initializer);
            }
            _ => {
                self.emit_js_anonymous_module_exports_typed_members(initializer);
            }
        }

        self.emit_js_anonymous_module_exports_secondary_members(initializer);
    }

    pub(in crate::declaration_emitter) fn emit_js_anonymous_module_exports_object_members(
        &mut self,
        initializer: NodeIndex,
    ) {
        let Some(object_node) = self.arena.get(initializer) else {
            return;
        };
        let Some(object) = self.arena.get_literal_expr(object_node) else {
            return;
        };

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.arena.get_property_assignment(member_node) else {
                        continue;
                    };
                    if let Some(prop_name) = self.get_identifier_text(prop.name)
                        && let Some(init_name) = self.get_identifier_text(prop.initializer)
                        && prop_name == init_name
                        && self.js_named_export_names.contains(&prop_name)
                    {
                        continue;
                    }
                    if self
                        .emit_js_named_export_object_literal_namespace(prop.name, prop.initializer)
                    {
                        continue;
                    }
                    let Some(prop_init_node) = self.arena.get(prop.initializer) else {
                        continue;
                    };
                    if prop_init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                        || prop_init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    {
                        self.emit_js_synthetic_function_declaration(
                            prop.name,
                            prop.initializer,
                            true,
                        );
                    } else if let Some(type_text) =
                        self.js_namespace_value_member_type_text(prop.initializer)
                    {
                        self.emit_js_named_export_value_member(
                            prop.name,
                            &type_text,
                            "declare let ",
                        );
                    } else {
                        self.emit_js_synthetic_value_declaration(prop.name, prop.initializer, true);
                    }
                }
                _ => {}
            }
        }
    }

    pub(in crate::declaration_emitter) fn emit_js_anonymous_module_exports_typed_members(
        &mut self,
        initializer: NodeIndex,
    ) {
        let Some(type_id) = self.get_node_type_or_names(&[initializer]) else {
            if !self.emit_js_new_expression_class_instance_members(initializer) {
                return;
            }
            return;
        };
        let Some(interner) = self.type_interner else {
            if !self.emit_js_new_expression_class_instance_members(initializer) {
                return;
            }
            return;
        };
        let instance_type =
            tsz_solver::type_queries::instance_type_from_constructor(interner, type_id)
                .unwrap_or(type_id);
        let Some(display_props) = interner.get_display_properties(instance_type) else {
            let _ = self.emit_js_new_expression_class_instance_members(initializer);
            return;
        };
        if display_props.is_empty() {
            let _ = self.emit_js_new_expression_class_instance_members(initializer);
            return;
        }

        let mut props: Vec<_> = display_props.iter().cloned().collect();
        if props.iter().any(|prop| prop.declaration_order > 0) {
            props.sort_by_key(|prop| prop.declaration_order);
        }

        // Skip properties that will also be emitted via secondary members
        // (i.e. `module.exports.X = ...` assignments later in the file). The
        // checker may augment the initializer's instance type with those
        // expando properties — e.g. for `module.exports = new Foo();
        // module.exports.additional = 20;` the instance type carries both
        // `member` (from Foo) and `additional` (from the later assignment).
        // Without this guard `emit_js_anonymous_module_exports_named_members`
        // emits `additional` from the typed pass AND from the secondary pass,
        // producing a duplicate `export const additional` declaration in the
        // .d.ts.
        for prop in props {
            if prop.is_class_prototype || prop.name == interner.intern_string("constructor") {
                continue;
            }

            let prop_name = interner.resolve_atom(prop.name);
            if !Self::is_unquoted_property_name(&prop_name) {
                continue;
            }

            let type_text = self.print_type_id(prop.type_id);
            self.emit_js_named_export_value_text(&prop_name, &type_text, "var ");
        }
    }

    pub(in crate::declaration_emitter) fn emit_js_anonymous_module_exports_secondary_members(
        &mut self,
        root_initializer: NodeIndex,
    ) {
        let root_is_object_literal = self
            .arena
            .get(root_initializer)
            .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION);
        // Each `module.exports.X = Y` secondary statement is also registered
        // in `js_deferred_value_export_statements` (via the CJS-named-export
        // collector) and would be emitted a second time when the statement
        // visitor reaches it. Remove those stmt indices from the deferred
        // maps before emitting here, so the statement visitor skips them.
        for (stmt_idx, _, _) in
            self.js_module_exports_secondary_member_stmt_assignments(root_initializer)
        {
            self.js_deferred_value_export_statements.remove(&stmt_idx);
            self.js_deferred_function_export_statements
                .remove(&stmt_idx);
        }
        for (name_idx, initializer) in
            self.js_module_exports_secondary_member_assignments(root_initializer)
        {
            if self.get_identifier_text(initializer).is_some() {
                continue;
            }

            let Some(init_node) = self.arena.get(initializer) else {
                continue;
            };
            if init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                self.emit_js_synthetic_function_declaration(name_idx, initializer, true);
            } else if self.emit_js_named_export_object_literal_namespace(name_idx, initializer) {
                continue;
            } else if root_is_object_literal
                && let Some(type_text) = self.js_namespace_value_member_type_text(initializer)
            {
                self.emit_js_named_export_value_member(
                    name_idx,
                    &type_text,
                    self.js_synthetic_export_value_keyword(initializer),
                );
            } else if !root_is_object_literal
                && let Some(type_text) = self.js_synthetic_export_value_type_text(initializer)
            {
                // When the root export is a class instance / nameable value
                // (`module.exports = new Foo()`), secondary `.X = Y` members
                // share its CommonJS "structurally mutable" semantics. tsc
                // emits these as `let`; `js_synthetic_export_value_keyword`
                // would otherwise pick `const` for primitive-literal
                // initializers and produce a spurious
                // `export const additional: 20;` divergence.
                self.emit_js_named_export_value_member(name_idx, &type_text, "let ");
            } else {
                self.emit_js_synthetic_value_declaration(name_idx, initializer, true);
            }
        }
    }

    /// Like `js_module_exports_secondary_member_assignments` but also
    /// returns each containing statement's `NodeIndex`. Used by the
    /// secondary-member emitter to suppress the duplicate emission that
    /// would otherwise occur when the statement visitor later reaches
    /// these `module.exports.X = Y` statements with their own deferred
    /// value export.
    pub(in crate::declaration_emitter) fn js_module_exports_secondary_member_stmt_assignments(
        &self,
        root_initializer: NodeIndex,
    ) -> Vec<(NodeIndex, NodeIndex, NodeIndex)> {
        let Some(source_file_idx) = self.current_source_file_idx else {
            return Vec::new();
        };
        let Some(source_file_node) = self.arena.get(source_file_idx) else {
            return Vec::new();
        };
        let Some(source_file) = self.arena.get_source_file(source_file_node) else {
            return Vec::new();
        };

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .filter(|stmt_idx| {
                self.js_module_exports_assignment_initializer(*stmt_idx) != Some(root_initializer)
            })
            .filter_map(|stmt_idx| {
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
                let lhs = self
                    .arena
                    .skip_parenthesized_and_assertions_and_comma(binary.left);
                let lhs_node = self.arena.get(lhs)?;
                if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    return None;
                }
                let lhs_access = self.arena.get_access_expr(lhs_node)?;
                let receiver = self
                    .arena
                    .skip_parenthesized_and_assertions_and_comma(lhs_access.expression);
                if !self.is_module_exports_reference(receiver) {
                    return None;
                }
                let (name_idx, initializer) =
                    self.js_commonjs_named_export_for_statement_with_options(stmt_idx, true)?;
                Some((stmt_idx, name_idx, initializer))
            })
            .collect()
    }

    pub(in crate::declaration_emitter) fn js_module_exports_secondary_member_assignments(
        &self,
        root_initializer: NodeIndex,
    ) -> Vec<(NodeIndex, NodeIndex)> {
        let Some(source_file_idx) = self.current_source_file_idx else {
            return Vec::new();
        };
        let Some(source_file_node) = self.arena.get(source_file_idx) else {
            return Vec::new();
        };
        let Some(source_file) = self.arena.get_source_file(source_file_node) else {
            return Vec::new();
        };

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .filter(|stmt_idx| {
                self.js_module_exports_assignment_initializer(*stmt_idx) != Some(root_initializer)
            })
            .filter_map(|stmt_idx| {
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
                let lhs = self
                    .arena
                    .skip_parenthesized_and_assertions_and_comma(binary.left);
                let lhs_node = self.arena.get(lhs)?;
                if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    return None;
                }
                let lhs_access = self.arena.get_access_expr(lhs_node)?;
                let receiver = self
                    .arena
                    .skip_parenthesized_and_assertions_and_comma(lhs_access.expression);
                if !self.is_module_exports_reference(receiver) {
                    return None;
                }

                self.js_commonjs_named_export_for_statement_with_options(stmt_idx, true)
            })
            .collect()
    }

    pub(in crate::declaration_emitter) fn emit_js_anonymous_export_equals_class_secondary_members(
        &mut self,
        root_initializer: NodeIndex,
    ) {
        let secondary_class_stmts: Vec<_> = self
            .js_module_exports_secondary_member_stmt_assignments(root_initializer)
            .into_iter()
            .filter(|(_, _, initializer)| {
                self.arena
                    .get(*initializer)
                    .is_some_and(|node| node.kind == syntax_kind_ext::CLASS_EXPRESSION)
            })
            .collect();
        for (stmt_idx, _, _) in &secondary_class_stmts {
            self.js_deferred_value_export_statements.remove(stmt_idx);
            self.js_deferred_function_export_statements.remove(stmt_idx);
        }
        let secondary_classes: Vec<_> = secondary_class_stmts
            .into_iter()
            .map(|(_, name_idx, initializer)| (name_idx, initializer))
            .collect();
        if secondary_classes.is_empty() {
            return;
        }

        self.write_indent();
        if self.should_emit_declare_keyword(false) {
            self.write("declare ");
        }
        self.write("namespace exports {");
        self.write_line();
        self.increase_indent();
        self.write_indent();
        self.write("export { ");
        for (idx, (name_idx, _)) in secondary_classes.iter().enumerate() {
            if idx > 0 {
                self.write(", ");
            }
            self.emit_node(*name_idx);
        }
        self.write(" };");
        self.write_line();
        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();

        for (name_idx, initializer) in secondary_classes {
            let _ = self.emit_js_named_class_expression_declaration(name_idx, initializer, false);
        }
    }

    pub(in crate::declaration_emitter) fn emit_js_named_export_value_member(
        &mut self,
        name_idx: NodeIndex,
        type_text: &str,
        keyword: &'static str,
    ) {
        let Some(name) = self.get_identifier_text(name_idx) else {
            return;
        };
        self.emit_js_named_export_value_text(&name, type_text, keyword);
    }

    pub(in crate::declaration_emitter) fn emit_js_named_export_value_text(
        &mut self,
        name: &str,
        type_text: &str,
        keyword: &'static str,
    ) {
        if let Some((export_name, local_name)) = self.js_reserved_export_local_text(name) {
            self.write_indent();
            self.write(keyword);
            self.write(&local_name);
            self.write(": ");
            self.write(type_text);
            self.write(";");
            self.write_line();
            self.js_cjs_export_aliases.push((export_name, local_name));
            self.emitted_module_indicator = true;
            return;
        }

        self.write_indent();
        self.write("export ");
        self.write(keyword);
        self.write(name);
        self.write(": ");
        self.write(type_text);
        self.write(";");
        self.write_line();
        self.emitted_module_indicator = true;
    }

    fn js_reserved_export_local_text(&mut self, export_name: &str) -> Option<(String, String)> {
        if !token_is_reserved_word(string_to_token(export_name)) {
            return None;
        }

        let base = format!("_{export_name}");
        let local_name = if self.reserved_names.contains(&base) {
            self.generate_unique_name(&base)
        } else {
            base
        };
        self.reserved_names.insert(local_name.clone());
        Some((export_name.to_string(), local_name))
    }

    pub(in crate::declaration_emitter) fn emit_js_named_export_object_literal_namespace(
        &mut self,
        decl_name: NodeIndex,
        initializer: NodeIndex,
    ) -> bool {
        self.emit_js_object_literal_namespace(decl_name, initializer, true, true)
    }

    pub(in crate::declaration_emitter) fn emit_js_object_literal_namespace(
        &mut self,
        decl_name: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
        is_declare: bool,
    ) -> bool {
        if !self.source_is_js_file {
            return false;
        }

        let Some(name_node) = self.arena.get(decl_name) else {
            return false;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }

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

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if is_declare {
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
                    let Some(member_init_node) = self.arena.get(prop.initializer) else {
                        continue;
                    };
                    if member_init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                        || member_init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    {
                        let Some(func) = self.arena.get_function(member_init_node) else {
                            continue;
                        };
                        self.emit_js_namespace_function_member(
                            prop.name,
                            func.type_parameters.as_ref(),
                            &func.parameters,
                            func.body,
                            func.type_annotation,
                        );
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
        self.emitted_module_indicator = true;
        true
    }

    pub(in crate::declaration_emitter) fn emit_js_new_expression_class_instance_members(
        &mut self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(class_idx) = self.js_new_expression_class_declaration(initializer) else {
            return false;
        };
        let Some(class_node) = self.arena.get(class_idx) else {
            return false;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return false;
        };

        let mut emitted_any = false;
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if let Some(prop) = self.arena.get_property_decl(member_node) {
                if self.arena.is_static(&prop.modifiers) {
                    continue;
                }
                let Some(prop_name_node) = self.arena.get(prop.name) else {
                    continue;
                };
                if prop_name_node.kind != SyntaxKind::Identifier as u16 {
                    continue;
                }

                let type_text = if prop.type_annotation.is_some() {
                    let before = self.writer.len();
                    self.emit_type(prop.type_annotation);
                    let emitted = self.writer.get_output()[before..].to_string();
                    self.writer.truncate(before);
                    emitted
                } else if prop.initializer.is_some() {
                    self.js_namespace_value_member_type_text(prop.initializer)
                        .or_else(|| self.allowlisted_initializer_type_text(prop.initializer))
                        .unwrap_or_else(|| "any".to_string())
                } else {
                    "any".to_string()
                };
                // tsc emits class-instance fields exported through CommonJS
                // (`module.exports = new Foo()`) as `let` — the binding is
                // structurally mutable in CommonJS. Use `let ` regardless of
                // initializer kind here (rather than routing through
                // `js_synthetic_export_value_keyword`, which would return
                // `const ` for primitive literals and produce
                // `export const member: number;` where tsc emits
                // `export let member: number;`).
                self.emit_js_named_export_value_member(prop.name, &type_text, "let ");
                emitted_any = true;
            }
        }

        emitted_any
    }

    pub(in crate::declaration_emitter) fn js_new_expression_class_declaration(
        &self,
        initializer: NodeIndex,
    ) -> Option<NodeIndex> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }
        let new_expr = self.arena.get_call_expr(init_node)?;
        let class_name = self.nameable_constructor_expression_text(new_expr.expression)?;
        let source_file_idx = self.current_source_file_idx?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .find(|stmt_idx| {
                self.arena
                    .get(*stmt_idx)
                    .and_then(|stmt_node| self.arena.get_class(stmt_node))
                    .and_then(|class| self.get_identifier_text(class.name))
                    .is_some_and(|name| name == class_name)
            })
    }

    pub(in crate::declaration_emitter) fn emit_js_inferred_constructor_assignment_properties(
        &mut self,
        members: &NodeList,
    ) {
        let ctor_idx = members.nodes.iter().find(|&&idx| {
            self.arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::CONSTRUCTOR)
        });
        let Some(&ctor_idx) = ctor_idx else {
            return;
        };
        let Some(ctor_node) = self.arena.get(ctor_idx) else {
            return;
        };
        let Some(ctor) = self.arena.get_constructor(ctor_node) else {
            return;
        };
        let Some(body_node) = self.arena.get(ctor.body) else {
            return;
        };
        let Some(body) = self.arena.get_block(body_node) else {
            return;
        };

        let mut declared_names = FxHashSet::default();
        for &member_idx in &members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let member_name = if let Some(prop) = self.arena.get_property_decl(member_node) {
                Some(prop.name)
            } else if let Some(method) = self.arena.get_method_decl(member_node) {
                Some(method.name)
            } else {
                self.arena
                    .get_accessor(member_node)
                    .map(|accessor| accessor.name)
            };
            if let Some(name_idx) = member_name
                && let Some(name) = self.get_identifier_text(name_idx)
            {
                declared_names.insert(name);
            }
        }

        for &stmt_idx in &body.statements.nodes {
            let Some((name_idx, rhs_idx)) = self.js_this_property_assignment(stmt_idx) else {
                continue;
            };
            let Some(name) = self.get_identifier_text(name_idx) else {
                continue;
            };
            if !declared_names.insert(name) {
                continue;
            }

            let jsdoc = self.arena.get(stmt_idx).and_then(|stmt_node| {
                self.leading_jsdoc_comment_chain_for_pos(stmt_node.pos)
                    .last()
                    .cloned()
            });
            let resolved_type = self.resolve_declaration_type_text(&[rhs_idx], Some(rhs_idx));
            let type_text = self
                .jsdoc_type_text_for_node(stmt_idx)
                .or_else(|| {
                    if self.js_constructor_assignment_rhs_is_jsdoc_null_parameter(
                        rhs_idx,
                        &ctor.parameters,
                    ) {
                        Some("any".to_string())
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    resolved_type
                        .as_ref()
                        .filter(|resolved| {
                            resolved.type_id != tsz_solver::types::TypeId::ANY
                                && resolved.emitted_type_text != "any"
                        })
                        .map(|resolved| resolved.emitted_type_text.clone())
                })
                .or_else(|| {
                    self.get_node_type_or_names(&[rhs_idx])
                        .filter(|type_id| *type_id != tsz_solver::types::TypeId::ANY)
                        .map(|type_id| self.print_type_id(type_id))
                })
                .or_else(|| {
                    self.js_constructor_assignment_expression_type_text(
                        rhs_idx,
                        &ctor.parameters,
                        0,
                    )
                })
                .or_else(|| self.infer_fallback_type_text(rhs_idx))
                .or_else(|| self.allowlisted_initializer_type_text(rhs_idx))
                .or_else(|| self.js_namespace_value_member_type_text(rhs_idx))
                .or_else(|| resolved_type.map(|resolved| resolved.emitted_type_text))
                .unwrap_or_else(|| "any".to_string());

            if let Some(jsdoc) = jsdoc {
                self.emit_multiline_jsdoc_comment(&jsdoc);
            }
            self.write_indent();
            self.emit_node(name_idx);
            self.write(": ");
            self.write(&type_text);
            self.write(";");
            self.write_line();
        }
    }

    fn js_constructor_assignment_rhs_is_jsdoc_null_parameter(
        &self,
        rhs_idx: NodeIndex,
        params: &NodeList,
    ) -> bool {
        let rhs_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(rhs_idx);
        let Some(rhs_node) = self.arena.get(rhs_idx) else {
            return false;
        };
        if rhs_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(rhs_name) = self.get_identifier_text(rhs_idx) else {
            return false;
        };

        for (position, param_idx) in params.nodes.iter().copied().enumerate() {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if self.get_identifier_text(param.name).as_deref() != Some(rhs_name.as_str()) {
                continue;
            }
            return self
                .jsdoc_param_decl_for_parameter(param_idx, position)
                .is_some_and(|decl| decl.type_text.trim() == "null");
        }

        false
    }

    pub(in crate::declaration_emitter) fn emit_ordered_class_members_with_js_constructor_assignment_properties(
        &mut self,
        members: &NodeList,
    ) {
        let mut emitted_js_constructor_assignment_properties = false;
        let member_order = self.class_member_emit_order(members);
        let uses_reordered_js_member_comments =
            self.source_is_js_file && member_order != members.nodes;
        for member_idx in member_order {
            let before_jsdoc_len = self.writer.len();
            let saved_comment_idx = self.comment_emit_idx;
            if let Some(member_node) = self.arena.get(member_idx) {
                if uses_reordered_js_member_comments {
                    self.emit_leading_jsdoc_comment_chain_preserving_source(member_node.pos);
                } else {
                    self.emit_leading_jsdoc_comments(member_node.pos);
                }
            }
            let before_member_len = self.writer.len();
            self.emit_class_member(member_idx);
            if self.writer.len() == before_member_len {
                self.writer.truncate(before_jsdoc_len);
                self.comment_emit_idx = saved_comment_idx;
                if let Some(member_node) = self.arena.get(member_idx) {
                    self.skip_comments_in_node(member_node.pos, member_node.end);
                }
            } else if uses_reordered_js_member_comments
                && let Some(member_node) = self.arena.get(member_idx)
            {
                self.skip_comments_in_node(member_node.pos, member_node.end);
            }
            if !emitted_js_constructor_assignment_properties
                && self.source_is_js_file
                && self
                    .arena
                    .get(member_idx)
                    .is_some_and(|member_node| member_node.kind == syntax_kind_ext::CONSTRUCTOR)
            {
                self.emit_js_inferred_constructor_assignment_properties(members);
                emitted_js_constructor_assignment_properties = true;
            }
        }
    }

    pub(in crate::declaration_emitter) fn js_constructor_assignment_expression_type_text(
        &self,
        expr_idx: NodeIndex,
        params: &NodeList,
        depth: u32,
    ) -> Option<String> {
        if depth > 8 {
            return None;
        }

        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => Some("number".to_string()),
            k if k == SyntaxKind::StringLiteral as u16 => Some("string".to_string()),
            k if k == SyntaxKind::RegularExpressionLiteral as u16 => Some("RegExp".to_string()),
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
            {
                Some("string".to_string())
            }
            k if k == SyntaxKind::TrueKeyword as u16 || k == SyntaxKind::FalseKeyword as u16 => {
                Some("boolean".to_string())
            }
            k if k == SyntaxKind::Identifier as u16 => self
                .js_parameter_type_text(params, expr_idx)
                .or_else(|| self.preferred_expression_type_text(expr_idx))
                .or_else(|| self.infer_fallback_type_text_at(expr_idx, depth)),
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || k == syntax_kind_ext::AWAIT_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION =>
            {
                let inner = self.arena.get_unary_expr_ex(expr_node)?.expression;
                self.js_constructor_assignment_expression_type_text(inner, params, depth + 1)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let binary = self.arena.get_binary_expr(expr_node)?;
                let op = binary.operator_token;
                let is_numeric_op = op == SyntaxKind::MinusToken as u16
                    || op == SyntaxKind::AsteriskToken as u16
                    || op == SyntaxKind::AsteriskAsteriskToken as u16
                    || op == SyntaxKind::SlashToken as u16
                    || op == SyntaxKind::PercentToken as u16
                    || op == SyntaxKind::LessThanLessThanToken as u16
                    || op == SyntaxKind::GreaterThanGreaterThanToken as u16
                    || op == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16
                    || op == SyntaxKind::AmpersandToken as u16
                    || op == SyntaxKind::BarToken as u16
                    || op == SyntaxKind::CaretToken as u16;
                let is_plus = op == SyntaxKind::PlusToken as u16;

                if is_numeric_op {
                    return Some("number".to_string());
                }
                if !is_plus {
                    return self
                        .preferred_expression_type_text(expr_idx)
                        .or_else(|| self.infer_fallback_type_text_at(expr_idx, depth));
                }

                let left_type = self.js_constructor_assignment_expression_type_text(
                    binary.left,
                    params,
                    depth + 1,
                )?;
                let right_type = self.js_constructor_assignment_expression_type_text(
                    binary.right,
                    params,
                    depth + 1,
                )?;
                if left_type == "string" || right_type == "string" {
                    Some("string".to_string())
                } else if left_type == "number" && right_type == "number" {
                    Some("number".to_string())
                } else {
                    None
                }
            }
            _ => self
                .preferred_expression_type_text(expr_idx)
                .or_else(|| self.infer_fallback_type_text_at(expr_idx, depth)),
        }
    }

    pub(in crate::declaration_emitter) fn js_parameter_type_text(
        &self,
        params: &NodeList,
        identifier_idx: NodeIndex,
    ) -> Option<String> {
        let identifier_name = self.get_identifier_text(identifier_idx)?;
        for (position, param_idx) in params.nodes.iter().copied().enumerate() {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            let param_name = self.get_identifier_text(param.name)?;
            if param_name != identifier_name {
                continue;
            }

            if let Some(type_text) = self
                .preferred_annotation_name_text(param.type_annotation)
                .or_else(|| self.emit_type_node_text(param.type_annotation))
            {
                let trimmed = type_text.trim_end();
                let trimmed = trimmed.strip_suffix('=').unwrap_or(trimmed).trim_end();
                return Some(trimmed.to_string());
            }

            if self.source_is_js_file
                && let Some(jsdoc_param) = self.jsdoc_param_decl_for_parameter(param_idx, position)
            {
                return Some(jsdoc_param.type_text);
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn js_this_property_assignment(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<(NodeIndex, NodeIndex)> {
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
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }

        let lhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.left);
        let lhs_node = self.arena.get(lhs)?;
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let lhs_access = self.arena.get_access_expr(lhs_node)?;
        if self
            .arena
            .get(lhs_access.expression)
            .is_none_or(|receiver| receiver.kind != SyntaxKind::ThisKeyword as u16)
        {
            return None;
        }
        if self
            .arena
            .get(lhs_access.name_or_argument)
            .is_none_or(|name| name.kind != SyntaxKind::Identifier as u16)
        {
            return None;
        }

        Some((
            lhs_access.name_or_argument,
            self.arena
                .skip_parenthesized_and_assertions_and_comma(binary.right),
        ))
    }

    pub(in crate::declaration_emitter) fn emit_js_static_method_augmentation_namespace(
        &mut self,
        group: &crate::declaration_emitter::helpers::JsStaticMethodAugmentationGroup,
    ) {
        let Some(class_node) = self.arena.get(group.class_idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return;
        };
        let Some(method_node) = self.arena.get(group.method_idx) else {
            return;
        };
        let Some(method) = self.arena.get_method_decl(method_node) else {
            return;
        };

        self.write_indent();
        if group.class_is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(group.class_is_exported) {
            self.write("declare ");
        }
        self.write("namespace ");
        self.emit_node(class.name);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        self.emit_js_namespace_function_member(
            method.name,
            method.type_parameters.as_ref(),
            &method.parameters,
            method.body,
            method.type_annotation,
        );

        self.write_indent();
        self.write("namespace ");
        self.emit_node(method.name);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        for &(prop_name, initializer) in &group.properties {
            let Some(init_node) = self.arena.get(initializer) else {
                continue;
            };
            if init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                let Some(func) = self.arena.get_function(init_node) else {
                    continue;
                };
                self.emit_js_namespace_function_member(
                    prop_name,
                    func.type_parameters.as_ref(),
                    &func.parameters,
                    func.body,
                    func.type_annotation,
                );
            } else if let Some(type_text) = self.js_namespace_value_member_type_text(initializer) {
                self.emit_js_namespace_value_member(prop_name, &type_text);
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_js_synthetic_prototype_class_if_needed(
        &mut self,
        name_idx: NodeIndex,
        is_exported: bool,
    ) {
        let Some(name) = self.get_identifier_text(name_idx) else {
            return;
        };
        let Some(methods) = self
            .js_deferred_prototype_method_statements
            .get(&name)
            .cloned()
        else {
            return;
        };
        if methods.is_empty() {
            return;
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

        for (method_name, initializer) in methods {
            self.emit_js_synthetic_class_method(method_name, initializer);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_js_synthetic_class_method(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
    ) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return;
        };

        let jsdoc = self.function_like_jsdoc_for_node(initializer);

        self.write_indent();
        self.emit_node(name_idx);

        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            jsdoc
                .as_deref()
                .map(Self::parse_jsdoc_template_params)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if let Some(ref type_params) = func.type_parameters {
            if !type_params.nodes.is_empty() {
                self.emit_type_parameters(type_params);
            } else if !jsdoc_template_params.is_empty() {
                self.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        } else if !jsdoc_template_params.is_empty() {
            self.emit_jsdoc_template_parameters(&jsdoc_template_params);
        }

        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
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
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            let func_type_id = cache
                .node_types
                .get(&initializer.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[name_idx, initializer]));
            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) = type_queries::get_return_type(*interner, func_type_id)
            {
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && func.body.is_some()
                    && self.body_returns_void(func.body)
                {
                    self.write(": void");
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(return_type_id));
                }
            } else if func.body.is_some() && self.body_returns_void(func.body) {
                self.write(": void");
            }
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write(": void");
        }

        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_js_namespace_function_member(
        &mut self,
        name_idx: NodeIndex,
        type_params: Option<&NodeList>,
        parameters: &NodeList,
        body_idx: NodeIndex,
        type_annotation: NodeIndex,
    ) {
        self.write_indent();
        self.write("function ");
        self.emit_node(name_idx);
        if let Some(type_params) = type_params
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }
        self.write("(");
        self.emit_parameters_with_body(parameters, body_idx);
        self.write(")");
        if type_annotation.is_some() {
            self.write(": ");
            self.emit_type(type_annotation);
        } else if body_idx.is_some() && self.body_returns_void(body_idx) {
            self.write(": void");
        } else if !self.source_is_declaration_file {
            self.write(": any");
        }
        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_js_namespace_value_member(
        &mut self,
        name_idx: NodeIndex,
        type_text: &str,
    ) {
        self.write_indent();
        self.write("let ");
        self.emit_node(name_idx);
        self.write(": ");
        self.write(type_text);
        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn js_namespace_value_member_type_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let init_node = self.arena.get(initializer)?;
        match init_node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => Some("string".to_string()),
            k if k == SyntaxKind::NumericLiteral as u16 => Some("number".to_string()),
            k if k == SyntaxKind::BigIntLiteral as u16 => Some("bigint".to_string()),
            k if k == SyntaxKind::TrueKeyword as u16 => Some("boolean".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => Some("boolean".to_string()),
            k if k == SyntaxKind::NullKeyword as u16 => Some("null".to_string()),
            k if k == SyntaxKind::UndefinedKeyword as u16 => Some("undefined".to_string()),
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                && self.js_empty_object_literal_initializer(initializer) =>
            {
                Some("{}".to_string())
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.infer_fallback_type_text(initializer)
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                self.nameable_new_expression_type_text(initializer)
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if self.is_negative_literal(init_node) {
                    Some("number".to_string())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub(crate) fn js_synthetic_export_value_type_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        self.js_synthetic_export_value_type_text_inner(initializer, 0)
    }

    pub(in crate::declaration_emitter) fn js_synthetic_export_value_type_text_inner(
        &self,
        initializer: NodeIndex,
        depth: u8,
    ) -> Option<String> {
        if depth > 4 {
            return None;
        }

        let init_node = self.arena.get(initializer)?;
        if init_node.kind == SyntaxKind::UndefinedKeyword as u16
            || self.is_void_expression(init_node)
        {
            return None;
        }

        match init_node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => {
                return self.get_source_slice_no_semi(init_node.pos, init_node.end);
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                return self.get_source_slice_no_semi(init_node.pos, init_node.end);
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                return self.get_source_slice_no_semi(init_node.pos, init_node.end);
            }
            k if k == SyntaxKind::TrueKeyword as u16 => return Some("true".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => return Some("false".to_string()),
            k if k == SyntaxKind::NullKeyword as u16 => return Some("null".to_string()),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && self.is_negative_literal(init_node) =>
            {
                return self.get_source_slice_no_semi(init_node.pos, init_node.end);
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                if let Some(new_expr) = self.arena.get_call_expr(init_node)
                    && let Some(reference_text) =
                        self.nameable_constructor_expression_text(new_expr.expression)
                {
                    return Some(reference_text);
                }
            }
            _ => {}
        }

        if let Some(type_id) = self.get_node_type_or_names(&[initializer])
            && type_id != tsz_solver::types::TypeId::ANY
        {
            return Some(self.print_type_id(type_id));
        }

        if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(reference_text) = self.nameable_constructor_expression_text(initializer)
            && let Some(assigned_initializer) =
                self.js_assigned_initializer_for_value_reference(initializer)
        {
            let printed =
                self.js_synthetic_export_value_type_text_inner(assigned_initializer, depth + 1)?;
            return Some(self.rewrite_recursive_js_class_expression_type(
                assigned_initializer,
                &reference_text,
                printed,
            ));
        }

        match init_node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                self.infer_fallback_type_text(initializer)
            }
            _ => self.js_namespace_value_member_type_text(initializer),
        }
    }

    pub(crate) fn is_void_expression(&self, node: &Node) -> bool {
        node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && self
                .arena
                .get_unary_expr(node)
                .is_some_and(|unary| unary.operator == SyntaxKind::VoidKeyword as u16)
    }

    pub(in crate::declaration_emitter) fn js_synthetic_export_value_keyword(
        &self,
        initializer: NodeIndex,
    ) -> &'static str {
        let resolved_initializer = self
            .js_assigned_initializer_for_value_reference(initializer)
            .unwrap_or(initializer);
        let Some(init_node) = self.arena.get(resolved_initializer) else {
            return "const ";
        };
        if init_node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            "var "
        } else {
            "const "
        }
    }

    pub(in crate::declaration_emitter) fn rewrite_recursive_js_class_expression_type(
        &self,
        initializer: NodeIndex,
        reference_text: &str,
        printed: String,
    ) -> String {
        let Some(class_expr) = self.arena.get_class_at(initializer) else {
            return printed;
        };

        let mut rewritten = printed;
        for &member_idx in &class_expr.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::METHOD_DECLARATION {
                continue;
            }
            let Some(method) = self.arena.get_method_decl(member_node) else {
                continue;
            };
            if method.type_annotation.is_some()
                || !method.body.is_some()
                || !self.function_body_returns_new_reference(method.body, reference_text)
            {
                continue;
            }
            let Some(method_name) = self.get_identifier_text(method.name) else {
                continue;
            };
            rewritten = rewritten.replacen(
                &format!("{method_name}(): any;"),
                &format!("{method_name}(): /*elided*/ any;"),
                1,
            );
        }

        rewritten
    }

    pub(in crate::declaration_emitter) fn function_body_returns_new_reference(
        &self,
        body_idx: NodeIndex,
        reference_text: &str,
    ) -> bool {
        let Some(body_node) = self.arena.get(body_idx) else {
            return false;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return false;
        };

        block
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| self.statement_returns_new_reference(stmt_idx, reference_text))
    }

    pub(in crate::declaration_emitter) fn statement_returns_new_reference(
        &self,
        stmt_idx: NodeIndex,
        reference_text: &str,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => self
                .arena
                .get_return_statement(stmt_node)
                .is_some_and(|ret| {
                    self.expression_is_new_reference(ret.expression, reference_text)
                }),
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    block
                        .statements
                        .nodes
                        .iter()
                        .copied()
                        .any(|nested| self.statement_returns_new_reference(nested, reference_text))
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => self
                .arena
                .get_if_statement(stmt_node)
                .is_some_and(|if_data| {
                    self.statement_returns_new_reference(if_data.then_statement, reference_text)
                        || (if_data.else_statement.is_some()
                            && self.statement_returns_new_reference(
                                if_data.else_statement,
                                reference_text,
                            ))
                }),
            _ => false,
        }
    }

    pub(in crate::declaration_emitter) fn expression_is_new_reference(
        &self,
        expr_idx: NodeIndex,
        reference_text: &str,
    ) -> bool {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return false;
        }
        let Some(new_expr) = self.arena.get_call_expr(expr_node) else {
            return false;
        };
        self.nameable_constructor_expression_text(new_expr.expression)
            .as_deref()
            == Some(reference_text)
    }

    pub(in crate::declaration_emitter) fn preferred_binding_source_type(
        &self,
        type_annotation: NodeIndex,
        initializer: NodeIndex,
        related_nodes: &[NodeIndex],
    ) -> Option<tsz_solver::types::TypeId> {
        if type_annotation.is_some()
            && let Some(type_id) = self.get_node_type(type_annotation)
        {
            return Some(type_id);
        }
        if initializer.is_some()
            && let Some(type_id) = self.get_node_type(initializer)
        {
            return Some(type_id);
        }
        self.get_node_type_or_names(related_nodes)
    }

    pub(in crate::declaration_emitter) fn destructuring_property_lookup_text(
        &self,
        node_idx: NodeIndex,
    ) -> Option<String> {
        let node = self.arena.get(node_idx)?;

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => self.get_identifier_text(node_idx),
            k if k == SyntaxKind::StringLiteral as u16 => {
                self.arena.get_literal(node).map(|lit| lit.text.clone())
            }
            k if k == SyntaxKind::NumericLiteral as u16 => self
                .arena
                .get_literal(node)
                .map(|lit| Self::normalize_numeric_literal(lit.text.as_ref())),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(node)?;
                let operand_idx = self
                    .arena
                    .skip_parenthesized_and_assertions_and_comma(unary.operand);
                let operand_node = self.arena.get(operand_idx)?;
                if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                    return None;
                }
                let literal = self.arena.get_literal(operand_node)?;
                let normalized = Self::normalize_numeric_literal(literal.text.as_ref());
                match unary.operator {
                    k if k == SyntaxKind::MinusToken as u16 => Some(format!("-{normalized}")),
                    k if k == SyntaxKind::PlusToken as u16 => Some(normalized),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                let computed = self.arena.get_computed_property(node)?;
                self.destructuring_property_lookup_text(computed.expression)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn array_binding_element_type(
        &self,
        tuple_elements: Option<&[tsz_solver::types::TupleElement]>,
        tuple_index: usize,
        array_element_type: Option<tsz_solver::types::TypeId>,
    ) -> Option<tsz_solver::types::TypeId> {
        if let Some(tuple_elements) = tuple_elements
            && let Some(tuple_element) = tuple_elements.get(tuple_index)
        {
            let mut type_id = if tuple_element.rest {
                self.type_interner.and_then(|interner| {
                    type_queries::get_array_element_type(interner, tuple_element.type_id)
                        .or(Some(tuple_element.type_id))
                })?
            } else {
                tuple_element.type_id
            };
            if tuple_element.optional
                && let Some(interner) = self.type_interner
            {
                type_id = interner.union(vec![type_id, tsz_solver::types::TypeId::UNDEFINED]);
            }
            return Some(type_id);
        }

        if array_element_type == Some(tsz_solver::types::TypeId::NEVER) {
            return Some(tsz_solver::types::TypeId::UNDEFINED);
        }

        array_element_type
    }

    pub(in crate::declaration_emitter) fn array_rest_binding_type(
        &self,
        source_type: Option<tsz_solver::types::TypeId>,
        tuple_elements: Option<&[tsz_solver::types::TupleElement]>,
        tuple_index: usize,
        array_element_type: Option<tsz_solver::types::TypeId>,
    ) -> Option<tsz_solver::types::TypeId> {
        let interner = self.type_interner?;

        if tuple_index == 0
            && let Some(source_type) = source_type
            && let Some(union_type) =
                type_queries::get_tuple_element_type_union(interner, source_type)
        {
            return Some(interner.array(union_type));
        }

        if let Some(tuple_elements) = tuple_elements {
            let remaining = tuple_elements
                .get(tuple_index..)
                .map_or_else(Vec::new, ToOwned::to_owned);
            return Some(interner.tuple(remaining));
        }

        array_element_type.map(|element_type| interner.array(element_type))
    }

    pub(in crate::declaration_emitter) fn object_binding_element_type(
        &self,
        source_type: Option<tsz_solver::types::TypeId>,
        element: &tsz_parser::parser::node::BindingElementData,
    ) -> Option<tsz_solver::types::TypeId> {
        let interner = self.type_interner?;
        let source_type = type_queries::unwrap_readonly(interner, source_type?);
        let property_name_idx = if element.property_name.is_some() {
            element.property_name
        } else {
            element.name
        };
        let property_name = self.destructuring_property_lookup_text(property_name_idx)?;
        let property =
            type_queries::find_property_in_type_by_str(interner, source_type, &property_name)?;
        if property.optional {
            Some(interner.union(vec![property.type_id, tsz_solver::types::TypeId::UNDEFINED]))
        } else {
            Some(property.type_id)
        }
    }

    pub(in crate::declaration_emitter) fn collect_typed_bindings_recursive(
        &self,
        node_idx: NodeIndex,
        source_type: Option<tsz_solver::types::TypeId>,
        bindings: &mut Vec<(NodeIndex, Option<tsz_solver::types::TypeId>)>,
    ) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let type_id = source_type
                    .filter(|type_id| *type_id != tsz_solver::types::TypeId::ANY)
                    .or_else(|| self.get_symbol_cached_type(node_idx))
                    .or_else(|| self.get_node_type(node_idx))
                    .or_else(|| self.get_type_via_symbol(node_idx));
                bindings.push((node_idx, type_id));
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(element) = self.arena.get_binding_element(node) {
                    let effective_type = source_type
                        .filter(|type_id| *type_id != tsz_solver::types::TypeId::ANY)
                        .or_else(|| self.get_symbol_cached_type(node_idx))
                        .or_else(|| self.get_symbol_cached_type(element.name))
                        .or_else(|| {
                            if element.initializer.is_some() {
                                self.get_node_type(element.initializer)
                            } else {
                                None
                            }
                        })
                        .or_else(|| {
                            self.get_node_type_or_names(&[
                                node_idx,
                                element.name,
                                element.initializer,
                            ])
                        });
                    self.collect_typed_bindings_recursive(element.name, effective_type, bindings);
                }
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                let tuple_elements = self.type_interner.and_then(|interner| {
                    source_type
                        .and_then(|type_id| type_queries::get_tuple_elements(interner, type_id))
                });
                let array_element_type = self.type_interner.and_then(|interner| {
                    source_type.and_then(|type_id| {
                        type_queries::get_array_element_type(interner, type_id).or_else(|| {
                            type_queries::get_tuple_element_type_union(interner, type_id)
                        })
                    })
                });

                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    let mut tuple_index = 0usize;
                    for &element_idx in &pattern.elements.nodes {
                        let Some(element_node) = self.arena.get(element_idx) else {
                            continue;
                        };
                        if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                            tuple_index += 1;
                            continue;
                        }
                        if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                            continue;
                        }
                        let Some(element) = self.arena.get_binding_element(element_node) else {
                            continue;
                        };
                        let element_type = if element.dot_dot_dot_token {
                            self.array_rest_binding_type(
                                source_type,
                                tuple_elements.as_deref(),
                                tuple_index,
                                array_element_type,
                            )
                        } else {
                            self.array_binding_element_type(
                                tuple_elements.as_deref(),
                                tuple_index,
                                array_element_type,
                            )
                        };
                        self.collect_typed_bindings_recursive(element_idx, element_type, bindings);
                        if !element.dot_dot_dot_token {
                            tuple_index += 1;
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    for &element_idx in &pattern.elements.nodes {
                        let Some(element_node) = self.arena.get(element_idx) else {
                            continue;
                        };
                        if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                            continue;
                        }
                        let Some(element) = self.arena.get_binding_element(element_node) else {
                            continue;
                        };
                        let element_type = if element.dot_dot_dot_token {
                            source_type
                        } else {
                            self.object_binding_element_type(source_type, element)
                        };
                        self.collect_typed_bindings_recursive(element_idx, element_type, bindings);
                    }
                }
            }
            _ => {}
        }
    }

    pub(in crate::declaration_emitter) fn collect_flattened_binding_entries(
        &self,
        pattern_idx: NodeIndex,
        source_type: Option<tsz_solver::types::TypeId>,
    ) -> Vec<(NodeIndex, Option<tsz_solver::types::TypeId>)> {
        let mut bindings = Vec::new();
        self.collect_typed_bindings_recursive(pattern_idx, source_type, &mut bindings);
        bindings
    }

    fn collect_flattened_binding_type_texts_from_annotation(
        &mut self,
        pattern_idx: NodeIndex,
        type_annotation: NodeIndex,
    ) -> Vec<(NodeIndex, String)> {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return Vec::new();
        };
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return Vec::new();
        }
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return Vec::new();
        };
        let pattern_elements = pattern.elements.nodes.clone();

        let Some(type_node) = self.arena.get(type_annotation) else {
            return Vec::new();
        };
        let Some(tuple) = self.arena.get_tuple_type(type_node) else {
            return Vec::new();
        };
        let tuple_elements = tuple.elements.nodes.clone();

        let mut type_texts = Vec::new();
        let mut tuple_index = 0usize;
        for element_idx in pattern_elements {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                tuple_index += 1;
                continue;
            }
            if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                continue;
            }
            let Some(element) = self.arena.get_binding_element(element_node) else {
                continue;
            };
            let Some(name_node) = self.arena.get(element.name) else {
                continue;
            };
            if name_node.kind == SyntaxKind::Identifier as u16
                && let Some(&tuple_element_idx) = tuple_elements.get(tuple_index)
                && let Some(type_text) = self.type_node_text(tuple_element_idx)
            {
                type_texts.push((element.name, type_text));
            }
            if !element.dot_dot_dot_token {
                tuple_index += 1;
            }
        }
        type_texts
    }

    pub(in crate::declaration_emitter) fn emit_flattened_binding_type_annotation(
        &mut self,
        ident_idx: NodeIndex,
        type_id: Option<tsz_solver::types::TypeId>,
    ) {
        let type_id = type_id
            .or_else(|| self.get_symbol_cached_type(ident_idx))
            .or_else(|| self.get_node_type(ident_idx))
            .or_else(|| self.get_type_via_symbol(ident_idx));
        self.write(": ");
        if let Some(type_id) = type_id {
            self.write(&self.print_type_id(type_id));
        } else {
            self.write("any");
        }
    }

    /// Emits flattened variable declarations for destructuring patterns.
    ///
    /// In .d.ts files, destructuring like `export const { a, b } = obj;`
    /// must be flattened into individual declarations:
    /// `export declare const a: Type;`
    /// `export declare const b: Type;`
    pub(in crate::declaration_emitter) fn emit_flattened_variable_declaration(
        &mut self,
        decl_idx: NodeIndex,
        keyword: &str,
        is_exported: bool,
    ) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };
        let bindings = self.collect_flattened_binding_entries(
            decl.name,
            self.preferred_binding_source_type(
                decl.type_annotation,
                decl.initializer,
                &[decl_idx, decl.name, decl.initializer],
            ),
        );
        let annotation_type_texts = self
            .collect_flattened_binding_type_texts_from_annotation(decl.name, decl.type_annotation);
        if bindings.is_empty() {
            return;
        }

        self.write_indent();
        if is_exported && (!self.inside_declare_namespace || self.ambient_module_has_scope_marker) {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write(keyword);
        self.write(" ");

        for (index, (ident_idx, type_id)) in bindings.into_iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            let has_leading_jsdoc = self.arena.get(ident_idx).is_some_and(|node| {
                !self
                    .leading_jsdoc_comment_chain_for_pos(node.pos)
                    .is_empty()
            });
            if has_leading_jsdoc && let Some(node) = self.arena.get(ident_idx) {
                self.write_line();
                self.emit_leading_jsdoc_comments(node.pos);
            }
            self.emit_node(ident_idx);
            if let Some(type_text) = annotation_type_texts
                .iter()
                .find_map(|(idx, text)| (*idx == ident_idx).then(|| text.clone()))
            {
                self.write(": ");
                self.write(&type_text);
            } else {
                self.emit_flattened_binding_type_annotation(ident_idx, type_id);
            }
        }
        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_parameter_property_modifiers(
        &mut self,
        modifiers: &Option<NodeList>,
    ) -> bool {
        let mut is_private = false;
        if let Some(modifiers) = modifiers {
            for &mod_idx in &modifiers.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        k if k == SyntaxKind::PrivateKeyword as u16 => {
                            self.write("private ");
                            is_private = true;
                        }
                        k if k == SyntaxKind::ProtectedKeyword as u16 => {
                            self.write("protected ");
                        }
                        k if k == SyntaxKind::ReadonlyKeyword as u16 => {
                            self.write("readonly ");
                        }
                        k if k == SyntaxKind::OverrideKeyword as u16 => {
                            // tsc strips `override` in .d.ts output.
                        }
                        _ => {}
                    }
                }
            }
        }
        is_private
    }

    /// Check if an initializer is a simple reference (identifier or qualified name)
    /// to a local import-equals alias (e.g. `import b = a.foo`).
    /// Returns the text to use after `typeof` if so (e.g. `"b"`).
    ///
    /// tsc emits `typeof <alias>` for variables initialized with an import-equals
    /// alias target rather than expanding the resolved type. This preserves the
    /// declarative reference in the .d.ts output.
    pub(in crate::declaration_emitter) fn initializer_import_alias_typeof_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let binder = self.binder?;
        let init_node = self.arena.get(initializer)?;

        // Only handle simple identifier references.
        if init_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let ident = self.arena.get_identifier(init_node)?;
        let name = &ident.escaped_text;

        // Resolve the identifier by walking the scope chain from the enclosing scope.
        // The binder's node_symbols map only contains declaration-site mappings, not
        // usage-site references. We need to walk scopes to find the symbol for `b`.
        let scope_id = binder.find_enclosing_scope(self.arena, initializer)?;
        let sym_id = self.resolve_name_in_scope_chain(binder, scope_id, name)?;
        let sym = binder.symbols.get(sym_id)?;

        // Check if this symbol is an alias (import-equals creates ALIAS symbols)
        if sym.flags & tsz_binder::symbol_flags::ALIAS == 0 {
            return None;
        }

        // Verify that at least one declaration is an import-equals declaration
        let has_import_equals_decl = sym.declarations.iter().any(|&decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
        });

        if !has_import_equals_decl {
            return None;
        }

        // tsc only emits `typeof alias` when the alias target is a function, class,
        // enum, or module — NOT when it targets a plain variable. For plain variables
        // (e.g. `import b = a.x` where `x` is `var x = 10`), tsc resolves and emits
        // the actual type (e.g. `number`).
        if self.import_alias_targets_plain_variable(binder, sym) {
            return None;
        }

        Some(name.clone())
    }

    /// Check whether an import-equals alias resolves to a plain variable.
    /// Returns `true` when the alias target's symbol has only VARIABLE flags
    /// (not FUNCTION, CLASS, ENUM, or MODULE).
    pub(in crate::declaration_emitter) fn import_alias_targets_plain_variable(
        &self,
        binder: &BinderState,
        alias_sym: &tsz_binder::Symbol,
    ) -> bool {
        use tsz_binder::symbol_flags;

        // Find the import-equals declaration to get the entity name reference.
        let import_decl_idx = alias_sym.declarations.iter().copied().find(|&decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
        });
        let import_decl_idx = match import_decl_idx {
            Some(idx) => idx,
            None => return false,
        };
        let import_node = match self.arena.get(import_decl_idx) {
            Some(n) => n,
            None => return false,
        };
        let import_data = match self.arena.get_import_decl(import_node) {
            Some(d) => d,
            None => return false,
        };

        // module_specifier is the entity name (e.g. `a.x` or just `a`).
        // Resolve it to find the target symbol.
        let target_sym_id =
            self.resolve_entity_name_to_symbol(binder, import_data.module_specifier);
        let target_sym_id = match target_sym_id {
            Some(id) => id,
            None => return false,
        };
        let target_sym = match binder.symbols.get(target_sym_id) {
            Some(s) => s,
            None => return false,
        };

        // A "plain variable" has VARIABLE flags but not FUNCTION, CLASS, ENUM, or MODULE.
        let non_variable_value = symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::REGULAR_ENUM
            | symbol_flags::CONST_ENUM
            | symbol_flags::VALUE_MODULE
            | symbol_flags::NAMESPACE_MODULE;
        target_sym.flags & symbol_flags::VARIABLE != 0 && target_sym.flags & non_variable_value == 0
    }

    /// Resolve a qualified entity name (e.g. `a.x`) to its final symbol by walking
    /// through namespace exports. For a simple identifier, resolve via scope chain.
    pub(in crate::declaration_emitter) fn resolve_entity_name_to_symbol(
        &self,
        binder: &BinderState,
        entity_name: NodeIndex,
    ) -> Option<SymbolId> {
        let node = self.arena.get(entity_name)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            // Simple identifier — resolve via scope chain from the entity name's location.
            let ident = self.arena.get_identifier(node)?;
            let scope_id = binder.find_enclosing_scope(self.arena, entity_name)?;
            self.resolve_name_in_scope_chain(binder, scope_id, &ident.escaped_text)
        } else if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            // Qualified name (e.g. `a.x`) — resolve left side first, then look up right
            // in the left symbol's exports.
            let qn = self.arena.get_qualified_name(node)?;
            let left_sym_id = self.resolve_entity_name_to_symbol(binder, qn.left)?;
            let left_sym = binder.symbols.get(left_sym_id)?;
            let right_node = self.arena.get(qn.right)?;
            let right_ident = self.arena.get_identifier(right_node)?;
            let right_name = &right_ident.escaped_text;

            // Look up in exports table of the left symbol.
            if let Some(exports) = &left_sym.exports
                && let Some(sym_id) = exports.get(right_name)
            {
                return Some(sym_id);
            }
            None
        } else {
            None
        }
    }

    /// Walk the scope chain from `scope_id` upward, looking for a symbol with the given name.
    pub(in crate::declaration_emitter) fn resolve_name_in_scope_chain(
        &self,
        binder: &BinderState,
        start_scope: tsz_binder::scopes::ScopeId,
        name: &str,
    ) -> Option<SymbolId> {
        let mut scope_id = start_scope;
        let mut iterations = 0;
        while scope_id.is_some() {
            iterations += 1;
            if iterations > 100 {
                break;
            }
            let scope = binder.scopes.get(scope_id.0 as usize)?;
            if let Some(sym_id) = scope.table.get(name) {
                return Some(sym_id);
            }
            scope_id = scope.parent;
        }
        None
    }

    pub(in crate::declaration_emitter) fn js_function_body_preferred_return_text_for_declaration(
        &self,
        body_idx: NodeIndex,
        name_idx: NodeIndex,
        params: &NodeList,
    ) -> Option<String> {
        if !self.source_is_js_file || !body_idx.is_some() {
            return None;
        }
        let name = self.get_identifier_text(name_idx)?;
        if self.js_function_body_returns_new_named(body_idx, &name) {
            return Some(name);
        }
        if !self
            .js_function_body_this_property_assignments(body_idx)
            .is_empty()
        {
            return Some("void".to_string());
        }
        if let Some(returned_identifier) = self.function_body_unique_return_identifier(body_idx)
            && let Some(type_text) = self.js_parameter_type_text(params, returned_identifier)
        {
            return Some(type_text);
        }

        self.function_body_single_return_expression(body_idx)
            .and_then(|expr_idx| {
                self.js_constructor_assignment_expression_type_text(expr_idx, params, 0)
            })
            .filter(|type_text| !type_text.is_empty() && type_text != "any")
    }

    pub(in crate::declaration_emitter) fn emit_js_function_like_class_if_needed(
        &mut self,
        name_idx: NodeIndex,
        params: &NodeList,
        body_idx: NodeIndex,
        is_exported: bool,
        jsdoc_anchor: NodeIndex,
    ) -> bool {
        if !self.source_is_js_file || !body_idx.is_some() {
            return false;
        }
        let this_assignments = self.js_function_body_this_property_assignments(body_idx);
        let prototype_members = self.js_prototype_object_members_for_name(name_idx);
        if this_assignments.is_empty() && prototype_members.is_empty() {
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

        if let Some(jsdoc) = self.function_like_jsdoc_for_node(jsdoc_anchor) {
            self.emit_multiline_jsdoc_comment(&jsdoc);
        }
        self.write_indent();
        self.write("constructor(");
        self.emit_parameters_with_body(params, body_idx);
        self.write(");");
        self.write_line();

        let returns_new = self
            .get_identifier_text(name_idx)
            .is_some_and(|name| self.js_function_body_returns_new_named(body_idx, &name));
        let mut declared_names = FxHashSet::default();
        for &member_idx in &prototype_members {
            if let Some(name_idx) = self.get_member_name_idx(member_idx)
                && let Some(name) = self.get_identifier_text(name_idx)
            {
                declared_names.insert(name);
            }
        }
        for (stmt_idx, prop_name_idx, rhs_idx) in this_assignments {
            let Some(prop_name) = self.get_identifier_text(prop_name_idx) else {
                continue;
            };
            if !declared_names.insert(prop_name) {
                continue;
            }
            if let Some(jsdoc_type) = self.jsdoc_type_text_for_node(stmt_idx) {
                if let Some(jsdoc) = self.function_like_jsdoc_for_node(stmt_idx) {
                    self.emit_multiline_jsdoc_comment(&jsdoc);
                }
                self.write_indent();
                self.emit_node(prop_name_idx);
                self.write(": ");
                self.write(&jsdoc_type);
                self.write(";");
                self.write_line();
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
                }
                continue;
            }
            let type_text = self
                .js_constructor_assignment_expression_type_text(rhs_idx, params, 0)
                .or_else(|| {
                    self.resolve_declaration_type_text(&[rhs_idx], Some(rhs_idx))
                        .map(|resolved| resolved.emitted_type_text)
                })
                .or_else(|| self.allowlisted_initializer_type_text(rhs_idx))
                .unwrap_or_else(|| "any".to_string());
            self.write_indent();
            self.emit_node(prop_name_idx);
            self.write(": ");
            self.write(&type_text);
            if returns_new && !type_text.contains("undefined") {
                self.write(" | undefined");
            }
            self.write(";");
            self.write_line();
        }

        let mut proto_type = None;
        let mut emitted_getters = FxHashSet::default();
        for &member_idx in &prototype_members {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                && self
                    .arena
                    .get_property_assignment(member_node)
                    .and_then(|prop| self.get_identifier_text(prop.name))
                    .as_deref()
                    == Some("__proto__")
            {
                if let Some(type_text) = self.js_proto_property_assignment_type_text(member_idx) {
                    proto_type = Some(type_text);
                }
                continue;
            }
            if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                && let Some(name_idx) = self.get_member_name_idx(member_idx)
                && let Some(name) = self.get_identifier_text(name_idx)
                && self.prototype_members_have_setter_named(&prototype_members, &name)
            {
                continue;
            }
            let before_jsdoc_len = self.writer.len();
            let saved_comment_idx = self.comment_emit_idx;
            self.emit_leading_jsdoc_comments(member_node.pos);
            let before_member_len = self.writer.len();
            self.emit_class_member(member_idx);
            if member_node.kind == syntax_kind_ext::SET_ACCESSOR
                && let Some(name_idx) = self.get_member_name_idx(member_idx)
                && let Some(name) = self.get_identifier_text(name_idx)
                && emitted_getters.insert(name.clone())
                && let Some(getter_idx) =
                    self.prototype_members_getter_named(&prototype_members, &name)
            {
                self.emit_class_member(getter_idx);
            }
            if self.writer.len() == before_member_len {
                self.writer.truncate(before_jsdoc_len);
                self.comment_emit_idx = saved_comment_idx;
                self.skip_comments_in_node(member_node.pos, member_node.end);
            }
        }
        if let Some(proto_type) = proto_type {
            self.write_indent();
            self.write("__proto__: ");
            self.write(&proto_type);
            self.write(";");
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        true
    }

    fn js_proto_property_assignment_type_text(&self, member_idx: NodeIndex) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        let prop = self.arena.get_property_assignment(member_node)?;
        let initializer = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(prop.initializer);
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return self
                .get_identifier_text(initializer)
                .map(|base_name| format!("typeof {base_name}"));
        }
        let access = self.arena.get_access_expr(init_node)?;
        if self.get_identifier_text(access.name_or_argument).as_deref() != Some("prototype") {
            return self
                .get_identifier_text(initializer)
                .map(|base_name| format!("typeof {base_name}"));
        }

        let base_name = self.get_identifier_text(access.expression)?;
        Some(format!("typeof {base_name}"))
    }

    fn prototype_members_have_setter_named(&self, members: &[NodeIndex], name: &str) -> bool {
        self.prototype_members_getter_or_setter_named(members, name, syntax_kind_ext::SET_ACCESSOR)
            .is_some()
    }

    fn prototype_members_getter_named(
        &self,
        members: &[NodeIndex],
        name: &str,
    ) -> Option<NodeIndex> {
        self.prototype_members_getter_or_setter_named(members, name, syntax_kind_ext::GET_ACCESSOR)
    }

    fn prototype_members_getter_or_setter_named(
        &self,
        members: &[NodeIndex],
        name: &str,
        kind: u16,
    ) -> Option<NodeIndex> {
        members.iter().copied().find(|&member_idx| {
            self.arena.get(member_idx).is_some_and(|node| {
                node.kind == kind
                    && self
                        .get_member_name_idx(member_idx)
                        .and_then(|name_idx| self.get_identifier_text(name_idx))
                        .as_deref()
                        == Some(name)
            })
        })
    }

    fn js_function_body_this_property_assignments(
        &self,
        body_idx: NodeIndex,
    ) -> Vec<(NodeIndex, NodeIndex, NodeIndex)> {
        let Some(body_node) = self.arena.get(body_idx) else {
            return Vec::new();
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return Vec::new();
        };
        block
            .statements
            .nodes
            .iter()
            .copied()
            .filter_map(|stmt_idx| {
                self.js_this_property_assignment(stmt_idx)
                    .map(|(name_idx, rhs_idx)| (stmt_idx, name_idx, rhs_idx))
            })
            .collect()
    }

    fn js_prototype_object_members_for_name(&self, name_idx: NodeIndex) -> Vec<NodeIndex> {
        let Some(name) = self.get_identifier_text(name_idx) else {
            return Vec::new();
        };
        self.js_prototype_object_members_for_export_name(&name)
    }

    fn js_function_body_returns_new_named(&self, body_idx: NodeIndex, name: &str) -> bool {
        let Some(body_node) = self.arena.get(body_idx) else {
            return false;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return false;
        };
        block
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| self.js_statement_returns_new_named(stmt_idx, name))
    }

    fn js_statement_returns_new_named(&self, stmt_idx: NodeIndex, name: &str) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => self
                .arena
                .get_return_statement(stmt_node)
                .is_some_and(|ret| self.js_expression_is_new_named(ret.expression, name)),
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    block
                        .statements
                        .nodes
                        .iter()
                        .copied()
                        .any(|stmt_idx| self.js_statement_returns_new_named(stmt_idx, name))
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => self
                .arena
                .get_if_statement(stmt_node)
                .is_some_and(|if_data| {
                    self.js_statement_returns_new_named(if_data.then_statement, name)
                        || (if_data.else_statement.is_some()
                            && self.js_statement_returns_new_named(if_data.else_statement, name))
                }),
            _ => false,
        }
    }

    fn js_expression_is_new_named(&self, expr_idx: NodeIndex, name: &str) -> bool {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return false;
        }
        let Some(new_expr) = self.arena.get_call_expr(expr_node) else {
            return false;
        };
        self.get_identifier_text(new_expr.expression).as_deref() == Some(name)
    }

    // Export/import emission → exports.rs
    // Type emission and utility helpers → helpers.rs
}
