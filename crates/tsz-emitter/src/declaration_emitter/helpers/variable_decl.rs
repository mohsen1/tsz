//! Variable declaration emission and function initializers

#[allow(unused_imports)]
use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
#[allow(unused_imports)]
use crate::emitter::type_printer::TypePrinter;
#[allow(unused_imports)]
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};
#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use tracing::debug;
#[allow(unused_imports)]
use tsz_binder::{BinderState, SymbolId, symbol_flags};
#[allow(unused_imports)]
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
#[allow(unused_imports)]
use tsz_parser::parser::ParserState;
#[allow(unused_imports)]
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn parse_jsdoc_enum_type_text(jsdoc: &str) -> Option<String> {
        let tag_pos = jsdoc.find("@enum")?;
        let rest = jsdoc[tag_pos + "@enum".len()..].trim();
        let raw_type = if rest.starts_with('{') {
            let (type_expr, _) = Self::parse_jsdoc_braced_type_and_name(rest)?;
            type_expr.to_string()
        } else {
            rest.lines()
                .next()
                .unwrap_or_default()
                .trim()
                .trim_start_matches('*')
                .trim()
                .to_string()
        };
        let raw_type = raw_type.trim();
        if raw_type.is_empty() {
            return None;
        }
        Some(Self::normalize_jsdoc_enum_type_text(raw_type))
    }

    fn normalize_jsdoc_enum_type_text(type_text: &str) -> String {
        let trimmed = type_text.trim();
        Self::convert_jsdoc_function_type(trimmed)
            .unwrap_or_else(|| Self::normalize_jsdoc_type_expr(trimmed))
    }

    pub(crate) fn emit_jsdoc_enum_variable_declaration_if_possible(
        &mut self,
        decl_idx: NodeIndex,
        decl_name: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
    ) -> bool {
        if !self.source_is_js_file || !initializer.is_some() {
            return false;
        }
        if self
            .arena
            .get(decl_name)
            .is_none_or(|node| node.kind != SyntaxKind::Identifier as u16)
        {
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
        let Some(jsdoc) = self.function_like_jsdoc_for_node(decl_idx) else {
            return false;
        };
        let Some(enum_type) = Self::parse_jsdoc_enum_type_text(&jsdoc) else {
            return false;
        };

        self.suppress_current_statement_jsdoc_comments = true;
        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        self.write("type ");
        self.emit_node(decl_name);
        self.write(" = ");
        self.write(&enum_type);
        self.write(";");
        self.write_line();

        self.write_indent();
        if is_exported {
            self.write("export ");
        } else if self.should_emit_declare_keyword(false) {
            self.write("declare ");
        }
        self.write("namespace ");
        self.emit_node(decl_name);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        let enum_is_function = enum_type.contains("=>");
        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if let Some(prop) = self.arena.get_property_assignment(member_node) {
                if enum_is_function
                    && self.emit_jsdoc_enum_function_member(prop.name, prop.initializer)
                {
                    continue;
                }
                let type_text = self
                    .jsdoc_type_text_for_node(member_idx)
                    .unwrap_or_else(|| enum_type.clone());
                self.emit_js_namespace_value_member(prop.name, &type_text);
            } else if let Some(method) = self.arena.get_method_decl(member_node) {
                self.emit_js_namespace_function_member(
                    method.name,
                    method.type_parameters.as_ref(),
                    &method.parameters,
                    method.body,
                    method.type_annotation,
                );
            } else if let Some(shorthand) = self.arena.get_shorthand_property(member_node) {
                self.emit_js_namespace_value_member(shorthand.name, &enum_type);
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        true
    }

    fn emit_jsdoc_enum_function_member(
        &mut self,
        name_idx: NodeIndex,
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

        self.write_indent();
        self.write("function ");
        self.emit_node(name_idx);
        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write(")");
        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if self.jsdoc_enum_function_member_return_is_any(func.body) {
            self.write(": any");
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache)
            && let Some(func_type_id) = cache
                .node_types
                .get(&initializer.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[name_idx, initializer]))
            && let Some(return_type_id) =
                tsz_solver::type_queries::get_return_type(*interner, func_type_id)
        {
            self.write(": ");
            self.write(&self.print_type_id(return_type_id));
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write(": void");
        } else {
            self.write(": any");
        }
        self.write(";");
        self.write_line();
        true
    }

    fn jsdoc_enum_function_member_return_is_any(&self, body_idx: NodeIndex) -> bool {
        let Some(body_node) = self.arena.get(body_idx) else {
            return false;
        };
        let return_expr = if body_node.kind == syntax_kind_ext::BLOCK {
            let Some(return_expr) = self.function_body_single_return_expression(body_idx) else {
                return false;
            };
            return_expr
        } else {
            body_idx
        };
        let Some(return_node) = self.arena.get(return_expr) else {
            return false;
        };
        if return_node.kind == SyntaxKind::Identifier as u16 {
            return true;
        }
        if return_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.arena.get_binary_expr(return_node)
        {
            return binary.operator_token == SyntaxKind::PlusToken as u16;
        }
        false
    }

    /// Emit type annotation (or literal initializer) for a single variable declaration.
    ///
    /// Handles: literal const initializers, explicit type annotations, unique symbol,
    /// null/undefined → `any`, inferred type from cache, and fallback type inference.
    ///
    /// Used by both `emit_exported_variable` and `emit_variable_declaration_statement`
    /// to avoid duplicated type emission logic.
    pub(crate) fn emit_variable_decl_type_or_initializer(
        &mut self,
        keyword: &str,
        stmt_pos: u32,
        decl_idx: NodeIndex,
        decl_name: NodeIndex,
        type_annotation: NodeIndex,
        initializer: NodeIndex,
    ) {
        let has_type_annotation = type_annotation.is_some();
        let has_initializer = initializer.is_some();
        let jsdoc_type_text = self
            .source_is_js_file
            .then(|| {
                self.jsdoc_name_like_type_expr_for_pos(stmt_pos)
                    .or_else(|| self.jsdoc_name_like_type_expr_for_node(decl_idx))
                    .or_else(|| self.jsdoc_name_like_type_expr_for_node(decl_name))
                    .or_else(|| self.jsdoc_type_text_for_node(decl_idx))
                    .or_else(|| self.jsdoc_type_text_for_node(decl_name))
            })
            .flatten();
        let const_asserted_enum_member = has_initializer
            .then(|| self.const_asserted_enum_access_member_text(initializer))
            .flatten();
        let const_enum_member_initializer = (keyword == "const" && !has_type_annotation)
            .then(|| self.simple_const_enum_access_member_text(initializer))
            .flatten();
        let widened_enum_type = (has_initializer && keyword != "const")
            .then(|| self.simple_enum_access_base_name_text(initializer))
            .flatten();
        // For JS files with JSDoc @type, named type takes precedence over literal narrowing.
        let js_has_jsdoc_type = jsdoc_type_text.is_some();
        let exported_call_initializer = self.variable_declaration_has_effective_export(decl_idx)
            && self
                .arena
                .get(initializer)
                .is_some_and(|node| node.kind == syntax_kind_ext::CALL_EXPRESSION);
        let literal_initializer_text = (keyword == "const"
            && !has_type_annotation
            && has_initializer
            && !exported_call_initializer
            && const_asserted_enum_member.is_none()
            && !js_has_jsdoc_type)
            .then(|| self.const_literal_initializer_text_deep(initializer))
            .flatten();

        // Determine if we should emit a literal initializer for const
        if let Some(enum_member_text) = const_enum_member_initializer {
            self.write(if self.source_is_js_file { ": " } else { " = " });
            self.write(&enum_member_text);
        } else if let Some(literal_initializer_text) = literal_initializer_text {
            self.write(if self.source_is_js_file { ": " } else { " = " });
            self.write(&literal_initializer_text);
        } else {
            let is_unique_symbol =
                keyword == "const" && has_initializer && self.is_symbol_call(initializer);

            // For `const x = null` / `const x = undefined`, tsc always emits `: any`.
            // For `let`/`var`, tsc preserves the solver's type (e.g., `let x: null`).
            let is_const_null_or_undefined = keyword == "const"
                && has_initializer
                && self.arena.get(initializer).is_some_and(|n| {
                    let k = n.kind;
                    k == SyntaxKind::NullKeyword as u16 || k == SyntaxKind::UndefinedKeyword as u16
                });

            if has_type_annotation {
                self.write(": ");
                self.emit_type(type_annotation);
            } else if keyword == "const"
                && has_initializer
                && let Some(template_index_type) =
                    self.template_index_signature_element_access_type_text(initializer)
            {
                self.write(": ");
                self.write(&template_index_type);
            } else if self.inside_non_ambient_namespace
                && !has_initializer
                && self.get_identifier_text(decl_name).as_deref() == Some("__proto__")
            {
                self.write(": any");
            } else if is_unique_symbol {
                self.write(": unique symbol");
            } else if let Some(enum_member_text) = const_asserted_enum_member {
                self.write(": ");
                self.write(&enum_member_text);
            } else if has_initializer && self.is_import_meta_url_expression(initializer) {
                self.write(": string");
            } else if has_initializer && self.initializer_is_global_this_identifier(initializer) {
                // `const x = globalThis` — tsc emits `: typeof globalThis`.
                // The solver otherwise resolves `globalThis` to `any` in the
                // emit boundary, producing a less-informative annotation.
                self.write(": typeof globalThis");
            } else if keyword != "const"
                && has_initializer
                && self
                    .arena
                    .get(initializer)
                    .is_some_and(|node| node.kind == SyntaxKind::NullKeyword as u16)
            {
                self.write(": null");
            } else if self.source_is_js_file
                && keyword != "const"
                && has_initializer
                && !js_has_jsdoc_type
                && self.arena.get(initializer).is_some_and(|node| {
                    let kind = node.kind;
                    kind == SyntaxKind::NumericLiteral as u16
                        || kind == SyntaxKind::StringLiteral as u16
                        || kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                        || kind == SyntaxKind::TrueKeyword as u16
                        || kind == SyntaxKind::FalseKeyword as u16
                        || kind == SyntaxKind::BigIntLiteral as u16
                })
                && let Some(type_text) = self.infer_fallback_type_text(initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if has_initializer
                && let Some(type_text) =
                    self.previous_duplicate_variable_declaration_type_text(decl_idx, decl_name)
            {
                self.write(": ");
                self.write(&type_text);
            } else if (is_const_null_or_undefined && !js_has_jsdoc_type)
                || (has_initializer && self.invalid_const_enum_object_access(initializer))
                || (has_initializer
                    && self.initializer_uses_inaccessible_class_constructor(initializer))
            {
                self.write(": any");
            } else if let Some(enum_type_text) = widened_enum_type {
                self.write(": ");
                self.write(&enum_type_text);
            } else if self.source_is_js_file
                && let Some(type_text) = jsdoc_type_text.as_deref()
            {
                self.write(": ");
                self.write(&Self::format_jsdoc_type_text_for_declaration(type_text));
            } else if self.source_is_js_file
                && has_initializer
                && let Some(type_text) = self.js_special_initializer_type_text(initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if has_initializer
                && let Some(type_text) = self.as_const_single_spread_array_type_text(initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if has_initializer
                && self.declaration_type_is_uninformative(&[decl_idx, decl_name, initializer])
                && let Some(type_text) = self.as_const_assertion_type_text(initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if has_initializer
                && let Some(type_text) = self.angle_bracket_const_assertion_type_text(initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if has_initializer
                && let Some(type_text) = self.explicit_asserted_type_text(initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if exported_call_initializer
                && self.preferred_expression_type_text(initializer).is_none()
                && let Some(type_text) = self.const_literal_initializer_text_deep(initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if has_initializer
                && self.initializer_is_new_expression(initializer)
                && let Some(type_text) = self.data_view_new_expression_type_text(initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if has_initializer
                && self.initializer_is_new_expression(initializer)
                && self.new_expression_constructor_is_class_like(initializer)
                && !(self.source_is_js_file && self.inside_non_ambient_namespace)
                && let Some(type_text) = self.nameable_new_expression_type_text(initializer)
            {
                self.write(": ");
                let type_text = Self::expand_parameters_utility_tuple_type_text(&type_text)
                    .unwrap_or(type_text);
                self.write(&type_text);
            } else if has_initializer
                && self
                    .arena
                    .get(initializer)
                    .is_some_and(|node| node.kind == syntax_kind_ext::CALL_EXPRESSION)
                && let Some(type_text) =
                    self.call_expression_reused_type_text(initializer)
                        .filter(|text| {
                            text.contains("=>")
                                && !text.contains("any")
                                && !text.contains("unknown")
                        })
            {
                self.write(": ");
                self.write(&type_text);
            } else if has_initializer
                && self
                    .arena
                    .get(initializer)
                    .is_some_and(|node| node.kind == syntax_kind_ext::CALL_EXPRESSION)
                && let Some(type_text) = self.preferred_expression_type_text(initializer)
            {
                let reused_type_text = self.call_expression_reused_type_text(initializer);
                let type_text = reused_type_text
                    .as_ref()
                    .filter(|text| {
                        !text.contains("unknown")
                            && (text.contains("=>")
                                || text.starts_with('[')
                                || type_text.contains("unknown")
                                || (keyword == "const"
                                    && Self::is_literal_type_text_for_const_call(text)))
                    })
                    .cloned()
                    .unwrap_or(type_text);
                let has_public_import_type = Self::type_text_starts_with_import_type(&type_text)
                    && !self.import_type_uses_private_package_subpath(&type_text);
                let type_text = self.qualify_current_namespace_self_type_text(&type_text);
                let type_text = Self::strip_synthetic_anonymous_object_members(&type_text);
                let type_text = self
                    .expand_portable_mapped_object_text_in_current_context(&type_text)
                    .unwrap_or(type_text);
                let type_text = self
                    .expand_imported_indexed_access_type_text(&type_text)
                    .unwrap_or(type_text);
                let type_text = Self::expand_tuple_item_lookup_mapped_type_text(&type_text)
                    .unwrap_or(type_text);
                let type_text = Self::normalize_inferred_array_any_text(&type_text);
                let mut type_text = self
                    .expand_rest_tuple_parameters_in_function_type_text(initializer, &type_text)
                    .unwrap_or(type_text);
                type_text = self
                    .preserve_call_argument_single_rest_parameter_text(initializer, &type_text)
                    .unwrap_or(type_text);
                type_text = Self::expand_parameters_utility_tuple_type_text(&type_text)
                    .unwrap_or(type_text);
                if let Some(labelled_type_text) = self
                    .preserve_spread_argument_tuple_labels_in_call_return_type(
                        initializer,
                        &type_text,
                    )
                {
                    type_text = labelled_type_text;
                }
                let preserves_reused_literal_call_type = reused_type_text
                    .as_deref()
                    .is_some_and(|reused| reused == type_text && reused.contains('"'));
                if keyword != "const"
                    && !preserves_reused_literal_call_type
                    && let Some(widened_type_text) =
                        self.widen_mutable_call_initializer_literal_type_text(initializer)
                {
                    type_text = widened_type_text;
                }
                if keyword == "const"
                    && self
                        .const_literal_initializer_text_deep(initializer)
                        .is_none()
                    && self.call_contains_unannotated_function_expression(initializer)
                    && let Some(widened_type_text) =
                        self.widen_literal_initializer_result_type_text(initializer)
                {
                    type_text = widened_type_text;
                }
                if keyword == "const"
                    && type_text == "string"
                    && let Some(template_index_type) =
                        self.template_index_signature_element_access_type_text(initializer)
                {
                    type_text = template_index_type;
                }
                let has_reusable_surface_type = self
                    .type_text_is_directly_nameable_reference(&type_text)
                    && (Self::type_text_starts_with_import_type(&type_text)
                        || type_text.contains(['<', '.']));
                self.write(": ");
                if keyword == "const"
                    && let Some(formatted) =
                        self.call_initializer_unexported_alias_literal_text(initializer)
                {
                    self.write(&formatted);
                } else {
                    self.write(&type_text);
                }
                if !has_public_import_type
                    && !has_reusable_surface_type
                    && let Some(name_text) = self.get_identifier_text(decl_name)
                    && let Some(name_node) = self.arena.get(decl_name)
                    && let Some(file_path) = self.current_file_path.clone()
                {
                    self.check_call_expression_return_type_portability(
                        initializer,
                        &name_text,
                        &file_path,
                        name_node.pos,
                        name_node.end - name_node.pos,
                    );
                }
            } else if has_initializer
                && (self.emit_ts_late_bound_function_initializer_type_annotation(
                    decl_name,
                    initializer,
                ) || ((self.function_initializer_has_type_predicate(
                    decl_idx,
                    decl_name,
                    initializer,
                ) || self.function_initializer_needs_source_signature(initializer)
                    || self.function_initializer_has_inline_parameter_comments(initializer)
                    || self.function_initializer_is_self_returning_for(initializer, decl_name)
                    || self.function_initializer_returns_unique_identifier(initializer)
                    || self.function_initializer_has_typeof_in_param_annotations(initializer)
                    || self.function_initializer_has_destructured_parameters(initializer))
                    && {
                        self.maybe_emit_non_portable_function_return_diagnostic(
                            decl_name,
                            initializer,
                        );
                        self.emit_function_initializer_type_annotation(
                            decl_idx,
                            decl_name,
                            initializer,
                        )
                    }))
            {
            } else if has_initializer
                && self
                    .arena
                    .get(initializer)
                    .is_some_and(|node| node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && let Some(type_text) = self.preferred_expression_type_text(initializer)
                && type_text != "any"
            {
                self.write(": ");
                if keyword == "const"
                    && let Some(template_index_type) =
                        self.template_index_signature_element_access_type_text(initializer)
                {
                    self.write(&template_index_type);
                } else {
                    self.write(&type_text);
                }
            } else if has_initializer
                && self
                    .arena
                    .get(initializer)
                    .is_some_and(|node| node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION)
                && let Some(type_text) = self.property_access_source_accessor_type_text(initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if has_initializer
                && self
                    .arena
                    .get(initializer)
                    .is_some_and(|node| node.kind == syntax_kind_ext::BINARY_EXPRESSION)
                && let Some(type_text) = self.preferred_expression_type_text(initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if !has_initializer
                && keyword == "let"
                && self
                    .previous_duplicate_variable_declaration_type_text(decl_idx, decl_name)
                    .is_some()
            {
                self.write(": any");
            } else if let Some(resolved_type) = self.resolve_declaration_type_text(
                &[decl_idx, decl_name],
                has_initializer.then_some(initializer),
            ) {
                let type_id = resolved_type.type_id;
                let printed_type_text = resolved_type.canonical_type_text;
                let emitted_type_text = has_initializer.then_some(resolved_type.emitted_type_text);
                let selected_type_text = if has_initializer
                    && let Some(emitted_text) = emitted_type_text.as_deref()
                    && self
                        .find_unexported_import_type_reference_in_printed_type(emitted_text)
                        .is_some()
                    && self
                        .find_unexported_import_type_reference_in_printed_type(&printed_type_text)
                        .is_none()
                {
                    printed_type_text.as_str()
                } else {
                    emitted_type_text.as_deref().unwrap_or(&printed_type_text)
                };

                if has_initializer && printed_type_text.contains("any") {
                    self.maybe_emit_non_portable_function_return_diagnostic(decl_name, initializer);
                }

                let initializer_preferred_type_text = has_initializer
                    .then_some(initializer)
                    .and_then(|initializer| self.preferred_expression_type_text(initializer));
                let directly_nameable_type_text = Some(selected_type_text)
                    .filter(|text| self.type_text_is_directly_nameable_reference(text))
                    .or_else(|| {
                        let printed_is_safe_fallback =
                            Self::type_text_starts_with_import_type(&printed_type_text)
                                || printed_type_text.contains('<')
                                || printed_type_text.contains('.');
                        (printed_is_safe_fallback
                            && self.type_text_is_directly_nameable_reference(&printed_type_text))
                        .then_some(printed_type_text.as_str())
                    })
                    .or_else(|| {
                        initializer_preferred_type_text
                            .as_deref()
                            .filter(|text| self.type_text_is_directly_nameable_reference(text))
                    });
                let preferred_type_is_directly_nameable = directly_nameable_type_text.is_some();
                let has_reusable_tagged_template_surface = has_initializer
                    && self.arena.get(initializer).is_some_and(|node| {
                        node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
                    });
                // TS2883: Check for non-portable inferred type references
                if let Some(name_text) = self.get_identifier_text(decl_name)
                    && let Some(name_node) = self.arena.get(decl_name)
                    && let Some(file_path) = self.current_file_path.clone()
                {
                    let diagnostics_before = self.diagnostics.len();
                    if self.diagnostics.len() == diagnostics_before && has_initializer {
                        let _ = self.emit_truncation_diagnostic_if_needed(
                            initializer,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    if self.diagnostics.len() == diagnostics_before {
                        let _ = self.emit_serialized_type_text_truncation_diagnostic_if_needed(
                            selected_type_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    if has_initializer && self.diagnostics.len() == diagnostics_before {
                        self.maybe_emit_non_portable_function_return_diagnostic(
                            decl_name,
                            initializer,
                        );
                    }
                    let mut ran_symbol_check = false;
                    if self.diagnostics.len() == diagnostics_before {
                        // If declaration emit can already spell the inferred type
                        // through an explicit import-type reference with a public
                        // package path (e.g. `import("styled-components").StyledComponent<"div">`),
                        // tsc does not look through that alias and emit TS2883 for
                        // nested implementation details like `NonReactStatics`.
                        //
                        // However, a plain identifier like "MySpecialType" is NOT safe
                        // to skip: the name alone doesn't prove the symbol is reachable
                        // from a public path. We must run the portability check whenever
                        // the type text is not an explicit `import("…")` form.
                        let is_safe_import_type = directly_nameable_type_text
                            .is_some_and(Self::type_text_starts_with_import_type);
                        let is_safe_reused_surface_type =
                            directly_nameable_type_text.is_some_and(|t| {
                                is_safe_import_type || t.contains('<') || t.contains('.')
                            });
                        if (!preferred_type_is_directly_nameable || !is_safe_reused_surface_type)
                            && !has_reusable_tagged_template_surface
                        {
                            ran_symbol_check = true;
                            self.check_non_portable_type_references(
                                type_id,
                                &name_text,
                                &file_path,
                                name_node.pos,
                                name_node.end - name_node.pos,
                            );
                        }
                    }
                    let has_safe_reused_surface_type =
                        directly_nameable_type_text.is_some_and(|text| {
                            (Self::type_text_starts_with_import_type(text)
                                && !self.import_type_uses_private_package_subpath(text))
                                || text.contains(['<', '.'])
                        }) || has_reusable_tagged_template_surface;
                    if has_initializer && !has_safe_reused_surface_type {
                        self.check_call_expression_return_type_portability(
                            initializer,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    if !ran_symbol_check
                        && self.diagnostics.len() == diagnostics_before
                        && has_initializer
                        && Self::type_text_starts_with_import_type(
                            directly_nameable_type_text.unwrap_or(&printed_type_text),
                        )
                        && self.import_type_uses_private_package_subpath(
                            directly_nameable_type_text.unwrap_or(&printed_type_text),
                        )
                    {
                        let _ = self.emit_non_portable_import_type_text_diagnostics(
                            directly_nameable_type_text.unwrap_or(&printed_type_text),
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    if self.diagnostics.len() == diagnostics_before {
                        let _ = self.emit_non_serializable_local_alias_diagnostic(
                            selected_type_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    if self.diagnostics.len() == diagnostics_before {
                        let trimmed_type_text = selected_type_text.trim_start();
                        if Self::type_text_starts_with_import_type(trimmed_type_text)
                            && !trimmed_type_text.contains('<')
                        {
                            let _ = self.emit_non_serializable_import_type_diagnostic(
                                trimmed_type_text,
                                &file_path,
                                name_node.pos,
                                name_node.end - name_node.pos,
                            );
                        }
                    }
                    if self.diagnostics.len() == diagnostics_before {
                        let _ = self.emit_non_serializable_property_diagnostic(
                            selected_type_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                }

                if keyword == "const"
                    && let Some(interner) = self.type_interner
                {
                    let has_literal_initializer_surface = !has_initializer
                        || self
                            .const_literal_initializer_text_deep(initializer)
                            .is_some();
                    if has_initializer
                        && let Some(formatted) =
                            self.call_initializer_unexported_alias_literal_text(initializer)
                    {
                        self.write(": ");
                        self.write(&formatted);
                        return;
                    }

                    if has_initializer
                        && let Some(template_index_type) =
                            self.template_index_signature_element_access_type_text(initializer)
                    {
                        self.write(": ");
                        self.write(&template_index_type);
                        return;
                    }

                    if let Some(lit) =
                        Self::enum_member_literal_initializer_value(interner, type_id)
                    {
                        let formatted = Self::format_literal_initializer(&lit, interner);
                        self.write(": ");
                        self.write(&formatted);
                        return;
                    }

                    if let Some(lit) = tsz_solver::visitor::literal_value(interner, type_id) {
                        if has_initializer
                            && let Some(type_text) =
                                self.call_expression_single_literal_type_argument_text(initializer)
                        {
                            let formatted_lit = Self::format_literal_initializer(&lit, interner);
                            if type_text == formatted_lit {
                                self.write(": ");
                                self.write(&type_text);
                                return;
                            }
                        }
                        let formatted = if has_literal_initializer_surface {
                            Self::format_literal_initializer(&lit, interner)
                        } else if let Some(kind) = Self::literal_primitive_kind_text(&lit) {
                            kind.to_string()
                        } else {
                            Self::format_literal_initializer(&lit, interner)
                        };
                        self.write(": ");
                        self.write(&formatted);
                        return;
                    }

                    if let Some(union_id) = tsz_solver::visitor::union_list_id(interner, type_id) {
                        let members = interner.type_list(union_id);
                        let mut saw_member = false;
                        let mut kind: Option<&'static str> = None;
                        let mut mixed = false;
                        for &member in members.iter() {
                            let member_kind =
                                match tsz_solver::visitor::literal_value(interner, member) {
                                    Some(tsz_solver::types::LiteralValue::String(_)) => "string",
                                    Some(tsz_solver::types::LiteralValue::Number(_)) => "number",
                                    Some(tsz_solver::types::LiteralValue::Boolean(_)) => "boolean",
                                    Some(tsz_solver::types::LiteralValue::BigInt(_)) => "bigint",
                                    None => {
                                        mixed = true;
                                        break;
                                    }
                                };
                            saw_member = true;
                            if let Some(existing) = kind {
                                if existing != member_kind {
                                    mixed = true;
                                    break;
                                }
                            } else {
                                kind = Some(member_kind);
                            }
                        }
                        if saw_member
                            && !mixed
                            && let Some(k) = kind
                        {
                            self.write(": ");
                            self.write(k);
                            return;
                        }
                    }
                }

                let selected_type_text =
                    self.qualify_current_namespace_self_type_text(selected_type_text);
                let selected_type_text = if has_initializer {
                    self.imported_call_public_type_text(initializer, &selected_type_text)
                } else {
                    selected_type_text
                };
                let selected_type_text = self
                    .expand_imported_indexed_access_type_text(&selected_type_text)
                    .unwrap_or(selected_type_text);
                let selected_type_text = if has_initializer {
                    self.add_returned_object_member_comments_to_type_text(
                        initializer,
                        &selected_type_text,
                    )
                } else {
                    selected_type_text
                };
                let selected_type_text = if has_initializer {
                    self.add_initializer_object_member_comments_to_type_text(
                        initializer,
                        &selected_type_text,
                    )
                } else {
                    selected_type_text
                };
                let selected_type_text =
                    Self::normalize_inferred_array_any_text(&selected_type_text);
                let selected_type_text = if has_initializer {
                    self.expand_rest_tuple_parameters_in_function_type_text(
                        initializer,
                        &selected_type_text,
                    )
                    .unwrap_or(selected_type_text)
                } else {
                    selected_type_text
                };
                if has_initializer {
                    self.insert_import_for_reused_static_call_type(
                        initializer,
                        &selected_type_text,
                    );
                }
                self.insert_import_for_unqualified_imported_type(&selected_type_text);
                self.write(": ");
                if keyword == "const"
                    && has_initializer
                    && let Some(template_index_type) =
                        self.template_index_signature_element_access_type_text(initializer)
                {
                    self.write(&template_index_type);
                } else {
                    self.write(&Self::strip_synthetic_anonymous_object_members(
                        &selected_type_text,
                    ));
                }
            } else if let Some(typeof_text) =
                self.typeof_prefix_for_value_entity(initializer, has_initializer, None)
            {
                self.write(": ");
                self.write(&typeof_text);
            } else if keyword == "const"
                && has_initializer
                && let Some(lit_text) = self.const_literal_initializer_text_deep(initializer)
            {
                // For const declarations where the type cache missed,
                // preserve the literal value: `declare const X = 123;`
                self.write(if self.source_is_js_file { ": " } else { " = " });
                self.write(&lit_text);
            } else if let Some(type_text) = self
                .allowlisted_initializer_type_text(initializer)
                .or_else(|| self.data_view_new_expression_type_text(initializer))
            {
                self.write(": ");
                self.write(&Self::strip_synthetic_anonymous_object_members(&type_text));
            } else if has_initializer || keyword != "const" {
                // tsc always emits a type annotation in .d.ts output.
                // For var/let without type info, and for const with an
                // initializer but no resolved type, default to `: any`.
                self.write(": any");
            }
        }

        if has_initializer
            && let Some(init_node) = self.arena.get(initializer)
            && (init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
            && let Some(func) = self.arena.get_function(init_node)
            && func.body.is_some()
        {
            let skip_end = self
                .arena
                .get(decl_idx)
                .map_or(init_node.end, |node| node.end);
            self.skip_comments_before_raw(skip_end);
        }
        if has_initializer
            && let Some(init_node) = self.arena.get(initializer)
            && (init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || init_node.kind == syntax_kind_ext::AS_EXPRESSION
                || init_node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
        {
            self.skip_comments_in_node(init_node.pos, init_node.end);
        }
    }

    pub(in crate::declaration_emitter) fn previous_duplicate_variable_declaration_type_text(
        &self,
        decl_idx: NodeIndex,
        decl_name: NodeIndex,
    ) -> Option<String> {
        let binder = self.binder?;
        let current_name = self.get_identifier_text(decl_name)?;
        let sym_id = binder.get_node_symbol(decl_name)?;
        let symbol = binder.symbols.get(sym_id)?;
        let current_node = self.arena.get(decl_idx)?;

        for prior_decl_idx in symbol.declarations.iter().copied() {
            if prior_decl_idx == decl_idx {
                return None;
            }
            let Some(prior_node) = self.arena.get(prior_decl_idx) else {
                continue;
            };
            if prior_node.pos >= current_node.pos {
                continue;
            }
            let Some(prior_decl) = self.arena.get_variable_declaration(prior_node) else {
                continue;
            };
            if self.get_identifier_text(prior_decl.name).as_deref() != Some(current_name.as_str()) {
                continue;
            }
            if prior_decl.type_annotation.is_some() {
                return self
                    .preferred_annotation_name_text(prior_decl.type_annotation)
                    .or_else(|| self.emit_type_node_text(prior_decl.type_annotation));
            }
            if prior_decl.initializer.is_some() {
                if let Some(alias_text) =
                    self.initializer_import_alias_typeof_text(prior_decl.initializer)
                {
                    return Some(format!("typeof {alias_text}"));
                }
                if let Some(typeof_text) =
                    self.typeof_prefix_for_value_entity(prior_decl.initializer, true, None)
                {
                    return Some(typeof_text);
                }
                if let Some(resolved) = self.resolve_declaration_type_text(
                    &[prior_decl_idx, prior_decl.name],
                    Some(prior_decl.initializer),
                ) {
                    return Some(resolved.emitted_type_text);
                }
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn data_view_new_expression_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        let new_expr = self.arena.get_call_expr(expr_node)?;
        let callee_text = self.nameable_constructor_expression_text(new_expr.expression)?;
        if callee_text != "DataView" {
            return None;
        }

        let args = new_expr.arguments.as_ref()?;
        let &arg0 = args.nodes.first()?;
        let backing_type = self.data_view_backing_store_type_text(arg0)?;
        Some(format!("DataView<{backing_type}>"))
    }

    pub(in crate::declaration_emitter) fn data_view_backing_store_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        if let Some(type_id) = self.get_node_type_or_names(&[expr_idx])
            && type_id != tsz_solver::types::TypeId::ANY
        {
            return Some(self.print_type_id(type_id));
        }

        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        let new_expr = self.arena.get_call_expr(expr_node)?;
        self.nameable_constructor_expression_text(new_expr.expression)
    }

    pub(crate) fn nameable_constructor_expression_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => self.get_identifier_text(expr_idx),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(expr_node)?;
                let lhs = self.nameable_constructor_expression_text(access.expression)?;
                let rhs = self.get_identifier_text(access.name_or_argument)?;
                Some(format!("{lhs}.{rhs}"))
            }
            _ => None,
        }
    }

    pub(crate) fn non_nameable_extends_heritage_type(
        &self,
        clauses: &NodeList,
    ) -> Option<(NodeIndex, NodeIndex)> {
        for &clause_idx in &clauses.nodes {
            let clause_node = self.arena.get(clause_idx)?;
            let heritage = self.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            let &type_idx = heritage.types.nodes.first()?;
            if self.is_entity_name_heritage(type_idx) {
                return None;
            }

            let expr_idx = self
                .arena
                .get(type_idx)
                .and_then(|type_node| self.arena.get_expr_type_args(type_node))
                .map(|eta| eta.expression)
                .unwrap_or(type_idx);
            return Some((type_idx, expr_idx));
        }

        None
    }

    pub(in crate::declaration_emitter) fn initializer_is_new_expression(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        let initializer = self.skip_parenthesized_non_null_and_comma(initializer);
        self.arena
            .get(initializer)
            .is_some_and(|node| node.kind == syntax_kind_ext::NEW_EXPRESSION)
    }

    pub(in crate::declaration_emitter) fn new_expression_constructor_is_class_like(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        let initializer = self.skip_parenthesized_non_null_and_comma(initializer);
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return false;
        }
        let Some(new_expr) = self.arena.get_call_expr(init_node) else {
            return false;
        };
        let Some(expr_idx) = self.skip_parenthesized_expression(new_expr.expression) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(ident) = self.get_identifier_text(expr_idx) else {
            return false;
        };
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(sym_id) = self.resolve_identifier_symbol(expr_idx, &ident) else {
            return false;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };
        (symbol.flags & symbol_flags::CLASS) != 0
            || symbol.declarations.iter().copied().any(|decl_idx| {
                self.arena.get(decl_idx).is_some_and(|decl_node| {
                    decl_node.kind == syntax_kind_ext::CLASS_DECLARATION
                        || decl_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                })
            })
    }

    pub(in crate::declaration_emitter) fn emit_direct_symbol_dependency_for_type(
        &mut self,
        type_id: tsz_solver::TypeId,
    ) {
        let Some(binder) = self.binder else {
            return;
        };
        let Some(interner) = self.type_interner else {
            return;
        };
        let Some(type_cache) = self.type_cache.as_ref() else {
            return;
        };

        let symbol_id = tsz_solver::visitor::lazy_def_id(interner, type_id)
            .and_then(|def_id| type_cache.def_to_symbol.get(&def_id).copied())
            .or_else(|| {
                tsz_solver::visitor::object_shape_id(interner, type_id)
                    .or_else(|| tsz_solver::visitor::object_with_index_shape_id(interner, type_id))
                    .and_then(|shape_id| interner.object_shape(shape_id).symbol)
            })
            .or_else(|| {
                tsz_solver::visitor::callable_shape_id(interner, type_id)
                    .and_then(|shape_id| interner.callable_shape(shape_id).symbol)
            });
        let Some(symbol_id) = symbol_id else {
            return;
        };
        if !self.emitted_synthetic_dependency_symbols.insert(symbol_id) {
            return;
        }

        let Some(symbol) = binder.symbols.get(symbol_id) else {
            return;
        };
        let Some(decl_idx) = symbol.declarations.first().copied() else {
            return;
        };
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let wrapped_export = self.arena.nodes.iter().any(|node| {
            self.arena
                .get_export_decl(node)
                .is_some_and(|export| export.export_clause == decl_idx)
        });
        let has_effective_export = self.statement_has_effective_export(decl_idx)
            || self
                .arena
                .get_extended(decl_idx)
                .map(|ext| ext.parent)
                .is_some_and(|parent| self.statement_has_effective_export(parent))
            || wrapped_export;

        let saved_emit_public_api_only = self.emit_public_api_only;
        match decl_node.kind {
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if has_effective_export {
                    self.emitted_synthetic_dependency_symbols.remove(&symbol_id);
                } else {
                    self.emit_public_api_only = false;
                    self.emit_interface_declaration(decl_idx);
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                let should_emit = saved_emit_public_api_only
                    && !has_effective_export
                    && self.arena.get_class(decl_node).is_some_and(|class| {
                        !self
                            .arena
                            .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                    });
                if should_emit {
                    self.emit_public_api_only = false;
                    self.emit_class_declaration(decl_idx);
                } else {
                    self.emitted_synthetic_dependency_symbols.remove(&symbol_id);
                }
            }
            _ => {
                self.emitted_synthetic_dependency_symbols.remove(&symbol_id);
            }
        }
        self.emit_public_api_only = saved_emit_public_api_only;
    }

    pub(crate) fn emit_synthetic_class_extends_alias_if_needed(
        &mut self,
        class_name: NodeIndex,
        heritage: Option<&NodeList>,
        is_default_export: bool,
    ) -> Option<String> {
        let type_id = self.synthetic_class_extends_alias_type_id(heritage)?;
        self.retain_direct_type_symbols_for_public_api(type_id);
        if self.used_symbols.is_none() {
            self.emit_direct_symbol_dependency_for_type(type_id);
        }
        let alias_name = if is_default_export {
            "default_base".to_string()
        } else {
            let class_name = self.get_identifier_text(class_name)?;
            format!("{class_name}_base")
        };

        self.write_indent();
        if self.should_emit_declare_keyword(false) {
            self.write("declare ");
        }
        self.write("const ");
        self.write(&alias_name);
        self.write(": ");
        let type_text = self.print_synthetic_class_extends_alias_type(type_id);
        let source_type_text = self.synthetic_class_extends_alias_source_type_text(heritage);
        let prefer_source_text = type_text == "never"
            || source_type_text.as_ref().is_some_and(|source_text| {
                source_text.contains(" & ")
                    || (Self::is_constructor_object_type_text(source_text)
                        && Self::type_text_has_conditional_infer_surface(&type_text))
            });
        let type_text = if prefer_source_text {
            source_type_text.unwrap_or(type_text)
        } else {
            type_text
        };
        self.write(&type_text);
        self.write(";");
        self.write_line();
        self.emitted_non_exported_declaration = true;

        Some(alias_name)
    }

    fn enum_member_literal_initializer_value(
        interner: &tsz_solver::TypeInterner,
        type_id: tsz_solver::types::TypeId,
    ) -> Option<tsz_solver::types::LiteralValue> {
        let (_def_id, member_type) = tsz_solver::visitor::enum_components(interner, type_id)?;
        tsz_solver::visitor::literal_value(interner, member_type)
    }
}
