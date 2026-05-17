//! Expression literal helpers for declaration type inference.

use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn array_literal_expression_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let array = self.arena.get_literal_expr(expr_node)?;
        if array.elements.nodes.is_empty() {
            return Some("any[]".to_string());
        }

        let mut element_types = Vec::with_capacity(array.elements.nodes.len());
        for elem_idx in array.elements.nodes.iter().copied() {
            // When strictNullChecks is off, skip null/undefined/void elements
            // so they don't pollute the array element type (tsc widens them away).
            if !self.strict_null_checks {
                if let Some(elem_node) = self.arena.get(elem_idx) {
                    let k = elem_node.kind;
                    if k == SyntaxKind::NullKeyword as u16
                        || k == SyntaxKind::UndefinedKeyword as u16
                    {
                        continue;
                    }
                    // Also skip void expressions (e.g., void 0)
                    if self.is_void_expression(elem_node) {
                        continue;
                    }
                }
                // Skip elements whose inferred type is null/undefined
                if let Some(type_id) = self.get_node_type_or_names(&[elem_idx])
                    && matches!(
                        type_id,
                        tsz_solver::types::TypeId::NULL
                            | tsz_solver::types::TypeId::UNDEFINED
                            | tsz_solver::types::TypeId::VOID
                    )
                {
                    continue;
                }
            }
            let elem_type = self
                .preferred_expression_type_text(elem_idx)
                .or_else(|| {
                    self.get_node_type_or_names(&[elem_idx])
                        .map(|type_id| self.print_type_id(type_id))
                })
                .or_else(|| self.infer_fallback_type_text_at(elem_idx, self.indent_level + 1))?;
            element_types.push(elem_type);
        }

        // If any element type is `any`, the whole union collapses to `any`
        // (matches tsc: T | any = any for all T).
        if element_types.iter().any(|t| t == "any") {
            return Some("any[]".to_string());
        }

        let mut distinct = Vec::new();
        for ty in element_types {
            if !distinct.iter().any(|existing| existing == &ty) {
                distinct.push(ty);
            }
        }
        Self::expand_object_union_arms_from_sibling_properties(&mut distinct);
        Self::drop_optional_param_function_subtypes(&mut distinct);

        // tsc orders union members by `TypeFlags` when printing: for the
        // primitive intrinsics the rank is Any < Unknown < String < Number
        // < Boolean < BigInt < Symbol. Our solver-inferred array-element
        // union was otherwise rendered in construction order, so
        // `var a = [1, "hello"]` printed as `(number | string)[]` instead
        // of tsc's `(string | number)[]`. Apply a stable sort that reorders
        // known primitives while keeping non-primitive members in their
        // original relative order (a comparator that returns Equal for
        // them preserves insertion order under a stable sort).
        fn primitive_rank(name: &str) -> Option<u32> {
            match name {
                "any" => Some(1),
                "unknown" => Some(2),
                "string" => Some(4),
                "number" => Some(8),
                "boolean" => Some(16),
                "bigint" => Some(64),
                "symbol" => Some(4096),
                "object" => Some(33_554_432),
                _ => None,
            }
        }
        distinct.sort_by(|a, b| match (primitive_rank(a), primitive_rank(b)) {
            (Some(ra), Some(rb)) => ra.cmp(&rb),
            _ => std::cmp::Ordering::Equal,
        });

        let elem_text = if distinct.len() == 1 {
            distinct.pop()?
        } else {
            // Each union member that is itself a function/constructor type
            // must be parenthesized so the trailing `=>` does not bind the
            // following `|` into the return type:
            //   `(x: A) => void | (x: B) => void` parses as
            //   `(x: A) => (void | (x: B) => void)`.
            distinct
                .iter()
                .map(|t| Self::parenthesize_type_text_in_union_position(t))
                .collect::<Vec<_>>()
                .join(" | ")
        };
        let needs_parens =
            elem_text.contains("=>") || elem_text.contains('|') || elem_text.contains('&');
        if needs_parens {
            Some(format!("({elem_text})[]"))
        } else {
            Some(format!("{elem_text}[]"))
        }
    }

    pub(in crate::declaration_emitter) fn local_variable_initializer_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.declarations.iter().copied() {
            let decl_node = self.arena.get(decl_idx)?;
            let Some(var_decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if !var_decl.initializer.is_some() {
                continue;
            }
            if let Some(type_text) = self
                .call_expression_reused_type_text(var_decl.initializer)
                .filter(|text| text != "any" && text != "unknown" && !text.contains("any"))
                .or_else(|| {
                    self.call_expression_declared_return_type_text(var_decl.initializer)
                        .filter(|text| text != "any" && text != "unknown" && !text.contains("any"))
                })
                .or_else(|| {
                    self.function_expression_type_text_from_ast(var_decl.initializer)
                        .filter(|text| text != "any" && text != "unknown" && !text.contains("any"))
                })
                .or_else(|| {
                    self.preferred_expression_type_text(var_decl.initializer)
                        .filter(|text| text != "any" && text != "unknown" && !text.contains("any"))
                })
                .or_else(|| self.as_const_assertion_type_text(var_decl.initializer))
                .or_else(|| self.infer_fallback_type_text_at(var_decl.initializer, 0))
            {
                return Some(
                    self.expand_rest_tuple_parameters_in_function_type_text(
                        var_decl.initializer,
                        &type_text,
                    )
                    .unwrap_or(type_text),
                );
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn function_expression_type_text_from_ast(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && expr_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.arena.get_function(expr_node)?;

        let mut scratch = if let (Some(type_cache), Some(type_interner), Some(binder)) =
            (&self.type_cache, self.type_interner, self.binder)
        {
            DeclarationEmitter::with_type_info(
                self.arena,
                type_cache.clone(),
                type_interner,
                binder,
            )
        } else {
            DeclarationEmitter::new(self.arena)
        };
        scratch.source_is_declaration_file = self.source_is_declaration_file;
        scratch.source_is_js_file = self.source_is_js_file;
        scratch.current_source_file_idx = self.current_source_file_idx;
        scratch.source_file_text = self.source_file_text.clone();
        scratch.current_file_path = self.current_file_path.clone();
        scratch.current_arena = self.current_arena.clone();
        scratch.arena_to_path = self.arena_to_path.clone();
        scratch.indent_level = self.indent_level;
        scratch.strict_null_checks = self.strict_null_checks;
        scratch.normalize_string_literal_type_quotes = true;

        if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            scratch.emit_type_parameters(type_params);
        }
        scratch.write("(");
        scratch.emit_parameters_with_body(&func.parameters, func.body);
        scratch.write(") => ");
        if func.type_annotation.is_some() {
            scratch.emit_type(func.type_annotation);
        } else if func.body.is_some() && scratch.body_returns_void(func.body) {
            scratch.write("void");
        } else if let Some(return_type) = scratch.expression_body_parameter_return_type_text(func) {
            scratch.write(&return_type);
        } else if func.body.is_some()
            && let Some(return_type) = scratch
                .preferred_expression_type_text(func.body)
                .or_else(|| scratch.infer_fallback_type_text_at(func.body, 0))
                .filter(|text| !text.is_empty() && text != "any")
        {
            scratch.write(&return_type);
        } else if let Some(return_type) =
            scratch.function_body_preferred_return_type_text(func.body)
        {
            scratch.write(&return_type);
        } else {
            scratch.write("any");
        }
        Some(scratch.writer.take_output())
    }

    fn expression_body_parameter_return_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<String> {
        let body_node = self.arena.get(func.body)?;
        if body_node.kind == syntax_kind_ext::BLOCK {
            return None;
        }
        let body_name = self.get_identifier_text(func.body)?;
        for &param_idx in &func.parameters.nodes {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            if self.get_identifier_text(param.name).as_deref() != Some(body_name.as_str()) {
                continue;
            }
            if param.type_annotation.is_some() {
                return self.emit_type_node_text_normalized(param.type_annotation);
            }
            return self
                .get_node_type(param.name)
                .map(|type_id| self.print_type_id(type_id));
        }
        None
    }

    pub(in crate::declaration_emitter) fn infer_object_literal_type_text_at(
        &self,
        object_expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let object_node = self.arena.get(object_expr_idx)?;
        let object = self.arena.get_literal_expr(object_node)?;

        // Pre-scan: collect setter and getter names for accessor pair handling
        let mut setter_names = rustc_hash::FxHashSet::<String>::default();
        let mut getter_names = rustc_hash::FxHashSet::<String>::default();
        for &idx in &object.elements.nodes {
            if let Some(n) = self.arena.get(idx) {
                if n.kind == syntax_kind_ext::SET_ACCESSOR {
                    if let Some(acc) = self.arena.get_accessor(n)
                        && let Some(name) = self.object_literal_member_name_text(acc.name)
                    {
                        setter_names.insert(name);
                    }
                } else if n.kind == syntax_kind_ext::GET_ACCESSOR
                    && let Some(acc) = self.arena.get_accessor(n)
                    && let Some(name) = self.object_literal_member_name_text(acc.name)
                {
                    getter_names.insert(name);
                }
            }
        }

        let mut members = Vec::new();
        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(name_idx) = self.object_literal_member_name_idx(member_node) else {
                continue;
            };
            let Some(name) = self.object_literal_member_name_text(name_idx) else {
                continue;
            };
            if name.is_empty() || name == ":" {
                continue;
            }

            if let Some(member_text) = self.infer_object_member_type_text_named_at(
                member_idx,
                &name,
                depth + 1,
                getter_names.contains(&name),
                setter_names.contains(&name),
            ) {
                if member_text.trim_start().starts_with(':') {
                    continue;
                }
                if !self.remove_comments {
                    for jsdoc in self.leading_jsdoc_comment_chain_for_pos(member_node.pos) {
                        members.push(Self::format_object_member_jsdoc_text(&jsdoc));
                    }
                }
                members.push(member_text);
            }
        }

        if members.is_empty() {
            Some("{}".to_string())
        } else {
            // Format as multi-line to match tsc's .d.ts output
            let member_indent = "    ".repeat((depth + 1) as usize);
            let closing_indent = "    ".repeat(depth as usize);
            let formatted_members: Vec<String> = members
                .iter()
                .map(|m| Self::format_object_member_entry(&member_indent, m))
                .collect();
            Some(format!(
                "{{\n{}\n{closing_indent}}}",
                formatted_members.join("\n")
            ))
        }
    }

    pub(in crate::declaration_emitter) fn object_literal_value_typeof_type_text(
        &self,
        object_expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let object_node = self.arena.get(object_expr_idx)?;
        let object = self.arena.get_literal_expr(object_node)?;
        let mut saw_typeof = false;
        let mut members = Vec::new();

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let name_idx = self.object_literal_member_name_idx(member_node)?;
            let name_text = self.object_literal_member_name_text(name_idx)?;
            if name_text.is_empty() || name_text == ":" {
                return None;
            }

            let value_idx = if let Some(data) = self.arena.get_shorthand_property(member_node) {
                data.name
            } else {
                self.arena.get_property_assignment(member_node)?.initializer
            };

            let type_text = self
                .local_function_declaration_identifier_type_text(value_idx)
                .or_else(|| self.direct_value_reference_typeof_text(value_idx))
                .or_else(|| {
                    self.preferred_object_member_initializer_type_text(value_idx, depth + 1)
                })?;
            saw_typeof |= type_text.contains("typeof ");
            members.push(Self::format_object_member_type_text(
                &name_text, &type_text, depth,
            ));
        }

        if !saw_typeof || members.is_empty() {
            return None;
        }

        let member_indent = "    ".repeat((depth + 1) as usize);
        let closing_indent = "    ".repeat(depth as usize);
        let formatted_members: Vec<String> = members
            .iter()
            .map(|member| Self::format_object_member_entry(&member_indent, member))
            .collect();
        Some(format!(
            "{{\n{}\n{closing_indent}}}",
            formatted_members.join("\n")
        ))
    }

    pub(in crate::declaration_emitter) fn infer_object_member_type_text_named_at(
        &self,
        member_idx: NodeIndex,
        name: &str,
        depth: u32,
        getter_exists: bool,
        setter_exists: bool,
    ) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let data = self.arena.get_property_assignment(member_node)?;
                let type_text = self
                    .preferred_object_member_initializer_type_text(data.initializer, depth)
                    .unwrap_or_else(|| "any".to_string());
                Some(Self::format_object_member_type_text(
                    name, &type_text, depth,
                ))
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let data = self.arena.get_shorthand_property(member_node)?;
                // For `{ foo }` the value reference is the name identifier itself.
                // For `{ foo = expr }` (CoverInitializedName) the assignment
                // initializer holds the default value.
                let initializer = if data.object_assignment_initializer == NodeIndex::NONE {
                    data.name
                } else {
                    data.object_assignment_initializer
                };
                let type_text = self
                    .preferred_object_member_initializer_type_text(initializer, depth)
                    .unwrap_or_else(|| "any".to_string());
                Some(Self::format_object_member_type_text(
                    name, &type_text, depth,
                ))
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let data = self.arena.get_accessor(member_node)?;
                // Infer return type: explicit annotation > body inference > any
                let type_text = self
                    .infer_fallback_type_text_at(data.type_annotation, depth)
                    .or_else(|| self.function_body_preferred_return_type_text(data.body))
                    .unwrap_or_else(|| "any".to_string());
                let readonly = if setter_exists { "" } else { "readonly " };
                Some(format!("{readonly}{name}: {type_text}"))
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if getter_exists {
                    return None;
                }

                let data = self.arena.get_accessor(member_node)?;
                let type_text = data
                    .parameters
                    .nodes
                    .first()
                    .and_then(|&p_idx| self.arena.get(p_idx))
                    .and_then(|p_node| self.arena.get_parameter(p_node))
                    .and_then(|param| {
                        self.infer_fallback_type_text_at(param.type_annotation, depth)
                    })
                    .unwrap_or_else(|| "any".to_string());
                Some(format!("{name}: {type_text}"))
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let data = self.arena.get_method_decl(member_node)?;
                if self.object_literal_method_uses_property_syntax(data) {
                    self.method_function_type_text(member_idx, data, depth)
                        .map(|type_text| format!("{name}: {type_text}"))
                } else {
                    self.method_signature_type_text_named_at(member_idx, data, name, depth)
                }
            }
            _ => None,
        }
    }

    fn method_signature_type_text_named_at(
        &self,
        method_idx: NodeIndex,
        method: &tsz_parser::parser::node::MethodDeclData,
        name: &str,
        depth: u32,
    ) -> Option<String> {
        let mut scratch = self.scratch_declaration_emitter();
        scratch.indent_level = depth;
        scratch.write(name);
        if method.question_token {
            scratch.write("?");
        }

        let jsdoc_template_params = if method
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            self.jsdoc_template_params_for_node(method_idx)
        } else {
            Vec::new()
        };
        if let Some(ref type_params) = method.type_parameters {
            if !type_params.nodes.is_empty() {
                scratch.emit_type_parameters(type_params);
            } else if !jsdoc_template_params.is_empty() {
                scratch.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        } else if !jsdoc_template_params.is_empty() {
            scratch.emit_jsdoc_template_parameters(&jsdoc_template_params);
        }

        scratch.write("(");
        scratch.emit_parameters_with_body(&method.parameters, method.body);
        scratch.write("): ");
        scratch.emit_method_function_type_return(method_idx, method);
        let type_text = scratch.writer.take_output();
        (!type_text.trim().is_empty()).then_some(type_text)
    }

    fn object_literal_method_uses_property_syntax(
        &self,
        method: &tsz_parser::parser::node::MethodDeclData,
    ) -> bool {
        let Some(name_node) = self.arena.get(method.name) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        if self
            .resolved_computed_property_name_text(method.name)
            .is_some()
            || self.computed_property_name_is_symbol_access(method.name)
            || self.computed_property_name_is_literal_key(method.name)
        {
            return false;
        }

        let computed_key_requires_property_syntax = self
            .arena
            .get_computed_property(name_node)
            .and_then(|computed| self.get_node_type_or_names(&[computed.expression, method.name]))
            .is_none_or(|type_id| {
                type_id == tsz_solver::types::TypeId::ANY
                    || self.type_interner.is_some_and(|interner| {
                        !tsz_solver::type_queries::is_type_usable_as_property_name(
                            interner, type_id,
                        )
                    })
            });

        method.question_token || computed_key_requires_property_syntax
    }

    fn computed_property_name_is_literal_key(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }

        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return false;
        };
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(computed.expression);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        expr_node.kind == SyntaxKind::StringLiteral as u16
            || expr_node.kind == SyntaxKind::NumericLiteral as u16
            || expr_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
    }

    fn computed_property_name_is_symbol_access(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }

        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return false;
        };
        let expr_idx = self.skip_parenthesized_non_null_and_comma(computed.expression);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        self.get_identifier_text(access.expression).as_deref() == Some("Symbol")
    }
}
