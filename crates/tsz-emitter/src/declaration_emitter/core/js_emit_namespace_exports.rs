use rustc_hash::FxHashSet;
use tsz_binder::symbol_flags;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::{SyntaxKind, string_to_token, token_is_reserved_word};

use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
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

    pub(in crate::declaration_emitter) fn js_reserved_export_local_text(
        &mut self,
        export_name: &str,
    ) -> Option<(String, String)> {
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
                        let overload_signatures =
                            self.jsdoc_overload_signatures_for_node(prop.initializer);
                        if self.emit_jsdoc_overload_namespace_function_signatures(
                            prop.name,
                            member_idx,
                            &overload_signatures,
                        ) {
                            continue;
                        }
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
            if self
                .class_member_info(member_idx)
                .is_none_or(|info| info.is_static)
            {
                continue;
            }
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if let Some(prop) = self.arena.get_property_decl(member_node) {
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
            if let Some(name_idx) = self.get_member_name_idx(member_idx)
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
                .or_else(|| self.anonymous_module_exports_class_new_expression_type_text(rhs_idx))
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
                let stmt_pos = self.arena.get(stmt_idx).map(|stmt_node| stmt_node.pos);
                let emitted_verbatim = stmt_pos.is_some_and(|pos| {
                    self.emitted_leading_single_line_jsdoc_type_comment_for_pos(pos)
                        && self.emit_jsdoc_comment_verbatim_for_pos(pos, &jsdoc)
                });
                if !emitted_verbatim {
                    self.emit_multiline_jsdoc_comment(&jsdoc);
                }
            }
            self.write_indent();
            self.emit_node(name_idx);
            self.write(": ");
            self.write(&type_text);
            self.write(";");
            self.write_line();
        }
    }

    pub(in crate::declaration_emitter) fn js_constructor_assignment_rhs_is_jsdoc_null_parameter(
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
            let member_has_jsdoc_overload_signatures =
                self.arena.get(member_idx).is_some_and(|member_node| {
                    self.source_is_js_file
                        && (member_node.kind == syntax_kind_ext::METHOD_DECLARATION
                            || member_node.kind == syntax_kind_ext::CONSTRUCTOR)
                        && !self
                            .jsdoc_overload_signatures_for_node(member_idx)
                            .is_empty()
                });
            if let Some(member_node) = self.arena.get(member_idx) {
                if member_has_jsdoc_overload_signatures {
                    // Member overload emitters write one JSDoc block per
                    // structured signature.
                } else if uses_reordered_js_member_comments {
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

    pub(in crate::declaration_emitter) fn should_delay_private_identifier_marker_for_js_constructor_overloads(
        &self,
        members: &NodeList,
    ) -> bool {
        self.source_is_js_file
            && members.nodes.iter().copied().any(|member_idx| {
                self.arena.get(member_idx).is_some_and(|member_node| {
                    member_node.kind == syntax_kind_ext::CONSTRUCTOR
                        && !self
                            .jsdoc_overload_signatures_for_node(member_idx)
                            .is_empty()
                })
            })
    }

    pub(in crate::declaration_emitter) fn emit_private_identifier_marker(&mut self) {
        self.write_indent();
        self.write("#private;");
        self.write_line();
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
        } else if let Some(return_type_text) = self.jsdoc_return_type_text_for_node(body_idx) {
            self.write(": ");
            self.write(&return_type_text);
        } else if body_idx.is_some() && self.body_returns_void(body_idx) {
            self.write(": void");
        } else if !self.source_is_declaration_file {
            self.write(": any");
        }
        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_js_namespace_import_alias_member(
        &mut self,
        name_idx: NodeIndex,
        reference_text: &str,
    ) {
        self.write_indent();
        self.write("import ");
        self.emit_node(name_idx);
        self.write(" = ");
        self.write(reference_text);
        self.write(";");
        self.write_line();

        self.write_indent();
        self.write("export { ");
        self.emit_node(name_idx);
        self.write(" };");
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

    pub(in crate::declaration_emitter) fn js_namespace_property_reference_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let initializer = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(initializer);
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        if !self.js_namespace_property_reference_has_namespace_root(initializer) {
            return None;
        }
        self.js_qualified_value_reference_text(initializer)
    }

    pub(in crate::declaration_emitter) fn js_namespace_property_reference_has_namespace_root(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(root_name) = self.js_qualified_value_reference_root_name(initializer) else {
            return false;
        };
        let Some((root_initializer, is_exported)) =
            self.js_top_level_variable_initializer_info(&root_name)
        else {
            return false;
        };
        if !is_exported {
            return false;
        }
        self.js_object_literal_initializer_has_namespace_shape(root_initializer, false)
    }

    pub(in crate::declaration_emitter) fn js_qualified_value_reference_root_name(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let node = self.arena.get(expr_idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => self.get_identifier_text(expr_idx),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                self.js_qualified_value_reference_root_name(access.expression)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn js_qualified_value_reference_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let node = self.arena.get(expr_idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => self.get_identifier_text(expr_idx),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(node)?;
                let left = self.js_qualified_value_reference_text(access.expression)?;
                let right = self.get_identifier_text(access.name_or_argument)?;
                Some(format!("{left}.{right}"))
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn js_namespace_value_member_type_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let init_node = self.arena.get(initializer)?;
        match init_node.kind {
            k if k == SyntaxKind::Identifier as u16
                && self.get_identifier_text(initializer).as_deref() == Some("undefined") =>
            {
                Some("undefined".to_string())
            }
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
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => self
                .get_node_type_or_names(&[initializer])
                .filter(|type_id| *type_id != tsz_solver::types::TypeId::ANY)
                .map(|type_id| self.print_type_id(type_id))
                .or_else(|| self.js_namespace_property_access_value_type_text(initializer)),
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn js_namespace_property_access_value_type_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(init_node)?;
        let property_name = self.get_identifier_text(access.name_or_argument)?;
        if property_name != "length" {
            return None;
        }

        let receiver = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(access.expression);
        let receiver_node = self.arena.get(receiver)?;
        let receiver_initializer = if receiver_node.kind == SyntaxKind::Identifier as u16 {
            let receiver_name = self.get_identifier_text(receiver)?;
            self.js_top_level_variable_initializer(&receiver_name)
        } else {
            Some(receiver)
        }?;
        let receiver_initializer = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(receiver_initializer);
        let receiver_init_node = self.arena.get(receiver_initializer)?;
        match receiver_init_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                Some("number".to_string())
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

        if let Some(type_text) = self.js_require_alias_property_access_typeof_text(initializer) {
            return Some(type_text);
        }

        if let Some((local_name, _, _)) =
            self.js_require_property_import_alias_for_value_expression(initializer)
        {
            return Some(local_name);
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
                &format!("{method_name}(): {};", crate::ELIDED_ANY),
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
                self.computed_destructuring_property_lookup_text(computed.expression)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn computed_destructuring_property_lookup_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        if let Some(interner) = self.type_interner
            && let Some(type_id) = self.type_cache.as_ref().and_then(|cache| {
                cache
                    .node_types
                    .get(&expr_idx.0)
                    .copied()
                    .or_else(|| self.get_node_type_or_names(&[expr_idx]))
            })
            && let Some(literal) = tsz_solver::visitor::literal_value(interner, type_id)
        {
            return Some(match literal {
                tsz_solver::types::LiteralValue::String(atom) => interner.resolve_atom(atom),
                tsz_solver::types::LiteralValue::Number(n) => Self::format_js_number(n.0),
                tsz_solver::types::LiteralValue::Boolean(value) => value.to_string(),
                tsz_solver::types::LiteralValue::BigInt(atom) => {
                    format!("{}n", interner.resolve_atom(atom))
                }
            });
        }

        if let Some(text) = self.const_value_reference_property_key_text(expr_idx) {
            return Some(text);
        }

        self.destructuring_property_lookup_text(expr_idx)
    }

    pub(in crate::declaration_emitter) fn const_value_reference_property_key_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let binder = self.binder?;
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.all_declarations() {
            if !self.arena.is_const_variable_declaration(decl_idx) {
                continue;
            }
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if let Some(text) = self.literal_property_key_initializer_text(decl.initializer) {
                return Some(text);
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn literal_property_key_initializer_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                self.arena
                    .get_literal(expr_node)
                    .map(|lit| lit.text.clone())
            }
            k if k == SyntaxKind::NumericLiteral as u16 => self
                .arena
                .get_literal(expr_node)
                .map(|lit| Self::normalize_numeric_literal(lit.text.as_ref())),
            k if k == SyntaxKind::TrueKeyword as u16 => Some("true".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => Some("false".to_string()),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(expr_node)?;
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
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn emit_js_function_computed_binding_key_declarations(
        &mut self,
        params: &NodeList,
    ) {
        if !self.source_is_js_file {
            return;
        }
        for &param_idx in &params.nodes {
            let Some(param) = self
                .arena
                .get(param_idx)
                .and_then(|node| self.arena.get_parameter(node))
            else {
                continue;
            };
            self.emit_computed_binding_key_declarations(param.name);
        }
    }

    pub(in crate::declaration_emitter) fn emit_computed_binding_key_declarations(
        &mut self,
        pattern_idx: NodeIndex,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
            && pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
        {
            return;
        }
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return;
        };
        for &element_idx in &pattern.elements.nodes {
            let Some(element) = self
                .arena
                .get(element_idx)
                .and_then(|node| self.arena.get_binding_element(node))
            else {
                continue;
            };
            if element.property_name.is_some()
                && let Some(property_node) = self.arena.get(element.property_name)
                && property_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && let Some(computed) = self.arena.get_computed_property(property_node)
            {
                self.emit_const_value_reference_declaration(computed.expression);
            }
            self.emit_computed_binding_key_declarations(element.name);
        }
    }

    pub(in crate::declaration_emitter) fn emit_const_value_reference_declaration(
        &mut self,
        expr_idx: NodeIndex,
    ) {
        let Some(binder) = self.binder else {
            return;
        };
        let Some(sym_id) = self.value_reference_symbol(expr_idx) else {
            return;
        };
        if !self.emitted_synthetic_dependency_symbols.insert(sym_id) {
            return;
        }
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return;
        };
        if symbol.is_exported || symbol.has_any_flags(symbol_flags::EXPORT_VALUE) {
            return;
        }
        for decl_idx in symbol.all_declarations() {
            if !self.arena.is_const_variable_declaration(decl_idx) {
                continue;
            }
            let Some(decl) = self
                .arena
                .get(decl_idx)
                .and_then(|node| self.arena.get_variable_declaration(node))
            else {
                continue;
            };
            let Some(name) = self.get_identifier_text(decl.name) else {
                continue;
            };
            let Some(type_text) = self.const_literal_initializer_text(decl.initializer) else {
                continue;
            };
            self.write_indent();
            self.write("declare const ");
            self.write(&name);
            self.write(": ");
            self.write(&type_text);
            self.write(";");
            self.write_line();
            self.emitted_non_exported_declaration = true;
            return;
        }
    }
}
