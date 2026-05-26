use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::{SyntaxKind, string_to_token, token_is_reserved_word};

use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
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
                .is_some_and(Self::jsdoc_has_function_signature_tags)
        {
            self.suppress_current_statement_jsdoc_comments = true;
        }
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
                        tsz_solver::type_queries::get_return_type(*interner, func_type_id)
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
                } else {
                    self.emit_js_body_return_annotation(func.body);
                }
            } else {
                self.emit_js_body_return_annotation(func.body);
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
                && let Some(return_type_id) =
                    tsz_solver::type_queries::get_return_type(*interner, func_type_id)
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
            } else {
                self.emit_js_body_return_annotation(func.body);
            }
        } else {
            self.emit_js_body_return_annotation(func.body);
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
        if is_exported {
            self.record_js_require_property_import_alias_for_new_expression(initializer);
        }

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

    pub(in crate::declaration_emitter) fn js_reserved_export_local_name(
        &mut self,
        name_idx: NodeIndex,
    ) -> Option<(String, String)> {
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
        if let Some(initializer) = self.js_define_property_function_initializer(property.value) {
            self.emit_js_synthetic_function_declaration_named_text(
                &property.name,
                initializer,
                true,
            );
            return;
        }

        self.write_indent();
        self.write("export const ");
        self.write(&property.name);
        self.write(": ");
        self.write(&property.type_text);
        self.write(";");
        self.write_line();
        self.emitted_module_indicator = true;
    }

    pub(in crate::declaration_emitter) fn emit_js_commonjs_define_property_namespace_member(
        &mut self,
        root_name: &str,
        property: &super::super::helpers::JsDefinedPropertyDecl,
    ) {
        self.write_indent();
        self.write("export namespace ");
        self.write(root_name);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        if let Some(initializer) = self.js_define_property_function_initializer(property.value) {
            self.emit_js_namespace_function_member_named_text(&property.name, initializer);
        } else {
            self.emit_js_namespace_value_member_named_text(&property.name, &property.type_text);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        self.emitted_module_indicator = true;
    }

    pub(in crate::declaration_emitter) fn emit_js_synthetic_function_declaration_named_text(
        &mut self,
        name: &str,
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
        if let Some(jsdoc) = jsdoc.as_deref() {
            self.emit_multiline_jsdoc_comment(jsdoc);
        }
        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("function ");
        self.write(name);
        self.emit_js_function_tail_from_function_data(func, initializer, jsdoc.as_deref());
        self.write(";");
        self.write_line();
        if is_exported {
            self.emitted_module_indicator = true;
        }
    }

    pub(in crate::declaration_emitter) fn emit_js_namespace_function_member_named_text(
        &mut self,
        name: &str,
        initializer: NodeIndex,
    ) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        let Some(func) = self.arena.get_function(init_node) else {
            return;
        };
        let jsdoc = self.function_like_jsdoc_for_node(initializer);
        if let Some(jsdoc) = jsdoc.as_deref() {
            self.emit_multiline_jsdoc_comment(jsdoc);
        }
        self.write_indent();
        self.write("function ");
        self.write(name);
        self.emit_js_function_tail_from_function_data(func, initializer, jsdoc.as_deref());
        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_js_namespace_value_member_named_text(
        &mut self,
        name: &str,
        type_text: &str,
    ) {
        self.write_indent();
        self.write("let ");
        self.write(name);
        self.write(": ");
        self.write(type_text);
        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_js_function_tail_from_function_data(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        initializer: NodeIndex,
        jsdoc: Option<&str>,
    ) {
        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            jsdoc
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

        if !self.emit_js_define_property_function_parameters(func, jsdoc) {
            self.write("(");
            self.emit_parameters_with_body(&func.parameters, func.body);
            self.write(")");
        }

        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = jsdoc.and_then(Self::parse_jsdoc_return_type_text) {
            self.write(": ");
            self.write(&return_type_text);
        } else if let Some(return_type_text) =
            self.js_define_property_jsdoc_body_return_text(func, jsdoc)
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
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            let func_type_id = cache
                .node_types
                .get(&initializer.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[initializer]));
            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) =
                    tsz_solver::type_queries::get_return_type(*interner, func_type_id)
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
            } else {
                self.emit_js_body_return_annotation(func.body);
            }
        } else {
            self.emit_js_body_return_annotation(func.body);
        }
    }

    pub(in crate::declaration_emitter) fn emit_js_define_property_function_parameters(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        jsdoc: Option<&str>,
    ) -> bool {
        let Some(jsdoc) = jsdoc else {
            return false;
        };
        let jsdoc_params = Self::parse_jsdoc_param_decls(jsdoc);
        if jsdoc_params.is_empty() {
            return false;
        }

        self.write("(");
        for (idx, &param_idx) in func.parameters.nodes.iter().enumerate() {
            if idx > 0 {
                self.write(", ");
            }
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            self.emit_node(param.name);
            let Some(jsdoc_param) = jsdoc_params.get(idx) else {
                continue;
            };
            self.write(": ");
            self.emit_define_property_jsdoc_type_text(&jsdoc_param.type_text);
        }
        self.write(")");
        true
    }

    pub(in crate::declaration_emitter) fn emit_define_property_jsdoc_type_text(
        &mut self,
        type_text: &str,
    ) {
        let normalized = self.normalize_define_property_jsdoc_type_text(type_text);
        if let Some(inner) = normalized
            .strip_prefix('{')
            .and_then(|text| text.strip_suffix('}'))
            && let Some((name, ty)) = inner.split_once(':')
        {
            self.write("{");
            self.write_line();
            self.increase_indent();
            self.write_indent();
            self.write(name.trim());
            self.write(": ");
            self.write(ty.trim());
            if !ty.trim_end().ends_with(';') {
                self.write(";");
            }
            self.write_line();
            self.decrease_indent();
            self.write_indent();
            self.write("}");
            return;
        }
        self.write(&normalized);
    }

    pub(in crate::declaration_emitter) fn normalize_define_property_jsdoc_type_text(
        &self,
        type_text: &str,
    ) -> String {
        if let Some(export_name) = type_text.strip_prefix("typeof module.exports.") {
            if self
                .js_define_property_function_initializer_for_export_name(export_name)
                .is_some()
            {
                return "() => void".to_string();
            }
        }
        if let Some(start) = type_text.find("typeof module.exports.") {
            let export_start = start + "typeof module.exports.".len();
            let export_name: String = type_text[export_start..]
                .chars()
                .take_while(|ch| *ch == '_' || *ch == '$' || ch.is_ascii_alphanumeric())
                .collect();
            if !export_name.is_empty()
                && self
                    .js_define_property_function_initializer_for_export_name(&export_name)
                    .is_some()
            {
                return type_text.replace(
                    &format!("typeof module.exports.{export_name}"),
                    "() => void",
                );
            }
        }
        type_text.to_string()
    }

    pub(in crate::declaration_emitter) fn js_define_property_jsdoc_body_return_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        jsdoc: Option<&str>,
    ) -> Option<String> {
        let jsdoc = jsdoc?;
        let jsdoc_params = Self::parse_jsdoc_param_decls(jsdoc);
        for (idx, &param_idx) in func.parameters.nodes.iter().enumerate() {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            let param_name = self.get_identifier_text(param.name)?;
            let jsdoc_param = jsdoc_params.get(idx)?;
            if self.function_body_returns_identifier(func.body, &param_name) {
                return Some(jsdoc_param.type_text.clone());
            }
        }

        if self.js_define_property_body_returns_string_and_call(func.body, &jsdoc_params) {
            return Some("void | \"\"".to_string());
        }
        None
    }

    pub(in crate::declaration_emitter) fn js_define_property_body_returns_string_and_call(
        &self,
        body_idx: NodeIndex,
        jsdoc_params: &[crate::declaration_emitter::helpers::JsdocParamDecl],
    ) -> bool {
        let Some(body_node) = self.arena.get(body_idx) else {
            return false;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return false;
        };
        let [stmt_idx] = block.statements.nodes.as_slice() else {
            return false;
        };
        let Some(stmt_node) = self.arena.get(*stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
            return false;
        }
        let Some(ret) = self.arena.get_return_statement(stmt_node) else {
            return false;
        };
        let expr_idx = ret.expression;
        if expr_idx.is_none() {
            return false;
        }
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(binary) = self.arena.get_binary_expr(expr_node) else {
            return false;
        };
        if binary.operator_token != SyntaxKind::AmpersandAmpersandToken as u16 {
            return false;
        }
        let left = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.left);
        let Some(left_node) = self.arena.get(left) else {
            return false;
        };
        if left_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(left_access) = self.arena.get_access_expr(left_node) else {
            return false;
        };
        let Some(left_param_name) = self.get_identifier_text(left_access.expression) else {
            return false;
        };
        let left_is_string = jsdoc_params
            .iter()
            .any(|param| param.name == left_param_name && param.type_text.contains("string"));
        if !left_is_string {
            return false;
        }
        let right = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        self.arena
            .get(right)
            .is_some_and(|node| node.kind == syntax_kind_ext::CALL_EXPRESSION)
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
                    .is_some_and(|h| {
                        h.token == SyntaxKind::ExtendsKeyword as u16
                            && h.types.nodes.iter().any(|&type_idx| {
                                !(self.source_is_js_file && self.heritage_type_is_null(type_idx))
                            })
                    })
            })
        });
        self.method_names_with_overloads = FxHashSet::default();

        self.emit_parameter_properties(&class.members);
        let delay_private_identifier_marker = self
            .should_delay_private_identifier_marker_for_js_constructor_overloads(&class.members);
        if self.class_has_private_identifier_member(&class.members)
            && !delay_private_identifier_marker
        {
            self.emit_private_identifier_marker();
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
        if self.class_has_private_identifier_member(&class.members)
            && delay_private_identifier_marker
        {
            self.emit_private_identifier_marker();
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
                    .is_some_and(|h| {
                        h.token == SyntaxKind::ExtendsKeyword as u16
                            && h.types.nodes.iter().any(|&type_idx| {
                                !(self.source_is_js_file && self.heritage_type_is_null(type_idx))
                            })
                    })
            })
        });
        self.method_names_with_overloads = FxHashSet::default();

        self.emit_parameter_properties(&class.members);
        let delay_private_identifier_marker = self
            .should_delay_private_identifier_marker_for_js_constructor_overloads(&class.members);
        if self.class_has_private_identifier_member(&class.members)
            && !delay_private_identifier_marker
        {
            self.emit_private_identifier_marker();
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
        if self.class_has_private_identifier_member(&class.members)
            && delay_private_identifier_marker
        {
            self.emit_private_identifier_marker();
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
}
