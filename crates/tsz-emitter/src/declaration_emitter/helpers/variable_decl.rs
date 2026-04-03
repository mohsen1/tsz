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
        let const_asserted_enum_member = has_initializer
            .then(|| self.const_asserted_enum_access_member_text(initializer))
            .flatten();
        let widened_enum_type = (has_initializer && keyword != "const")
            .then(|| self.simple_enum_access_base_name_text(initializer))
            .flatten();
        // For JS files with JSDoc @type, named type takes precedence over literal narrowing.
        let js_has_jsdoc_type = self.source_is_js_file
            && self
                .jsdoc_name_like_type_expr_for_pos(stmt_pos)
                .or_else(|| self.jsdoc_name_like_type_expr_for_node(decl_idx))
                .or_else(|| self.jsdoc_name_like_type_expr_for_node(decl_name))
                .is_some();
        let literal_initializer_text = (keyword == "const"
            && !has_type_annotation
            && has_initializer
            && const_asserted_enum_member.is_none()
            && !js_has_jsdoc_type)
            .then(|| self.const_literal_initializer_text_deep(initializer))
            .flatten();

        // Determine if we should emit a literal initializer for const
        if let Some(literal_initializer_text) = literal_initializer_text {
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
            } else if is_unique_symbol {
                self.write(": unique symbol");
            } else if let Some(enum_member_text) = const_asserted_enum_member {
                self.write(": ");
                self.write(&enum_member_text);
            } else if has_initializer && self.is_import_meta_url_expression(initializer) {
                self.write(": string");
            } else if is_const_null_or_undefined
                || (has_initializer && self.invalid_const_enum_object_access(initializer))
                || (has_initializer
                    && self.initializer_uses_inaccessible_class_constructor(initializer))
            {
                self.write(": any");
            } else if let Some(enum_type_text) = widened_enum_type {
                self.write(": ");
                self.write(&enum_type_text);
            } else if self.source_is_js_file
                && let Some(type_text) = self
                    .jsdoc_name_like_type_expr_for_pos(stmt_pos)
                    .or_else(|| self.jsdoc_name_like_type_expr_for_node(decl_idx))
                    .or_else(|| self.jsdoc_name_like_type_expr_for_node(decl_name))
            {
                self.write(": ");
                self.write(&type_text);
            } else if self.source_is_js_file
                && has_initializer
                && let Some(type_text) = self.js_special_initializer_type_text(initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if has_initializer
                && let Some(type_text) = self.explicit_asserted_type_text(initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if has_initializer
                && (self.function_initializer_has_inline_parameter_comments(initializer)
                    || self.function_initializer_is_self_returning(initializer)
                    || self.function_initializer_returns_unique_identifier(initializer))
                && {
                    self.maybe_emit_non_portable_function_return_diagnostic(decl_name, initializer);
                    self.emit_function_initializer_type_annotation(decl_idx, decl_name, initializer)
                }
            {
            } else if let Some(type_id) = self.get_node_type_or_names(&[decl_idx, decl_name]) {
                let printed_type_text = self.print_type_id(type_id);
                let emitted_type_text = has_initializer.then(|| {
                    self.declaration_emittable_type_text(initializer, type_id, &printed_type_text)
                });

                if has_initializer && printed_type_text.contains("any") {
                    self.maybe_emit_non_portable_function_return_diagnostic(decl_name, initializer);
                }

                let directly_nameable_type_text = emitted_type_text
                    .as_deref()
                    .filter(|text| self.type_text_is_directly_nameable_reference(text))
                    .or_else(|| {
                        let printed_is_safe_fallback = printed_type_text.starts_with("import(\"")
                            || printed_type_text.contains('<')
                            || printed_type_text.contains('.');
                        (printed_is_safe_fallback
                            && self.type_text_is_directly_nameable_reference(&printed_type_text))
                        .then_some(printed_type_text.as_str())
                    });
                let preferred_type_is_directly_nameable = directly_nameable_type_text.is_some();

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
                    if self.diagnostics.len() == diagnostics_before
                        && let Some(type_text) = emitted_type_text.as_deref()
                    {
                        let _ = self.emit_serialized_type_text_truncation_diagnostic_if_needed(
                            type_text,
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
                        // through a directly nameable public surface (for example
                        // `StyledComponent<"div">`), tsc does not look through
                        // that alias and emit TS2883 for nested implementation
                        // details like `NonReactStatics`.
                        if !preferred_type_is_directly_nameable {
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
                    if !ran_symbol_check
                        && self.diagnostics.len() == diagnostics_before
                        && has_initializer
                        && directly_nameable_type_text
                            .unwrap_or(&printed_type_text)
                            .starts_with("import(\"")
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
                        self.emit_non_portable_initializer_declaration_diagnostics(
                            initializer,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    if self.diagnostics.len() == diagnostics_before
                        && has_initializer
                        && !preferred_type_is_directly_nameable
                    {
                        self.emit_non_portable_initializer_declaration_diagnostics(
                            initializer,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    if self.diagnostics.len() == diagnostics_before {
                        let _ = self.emit_non_serializable_local_alias_diagnostic(
                            emitted_type_text.as_deref().unwrap_or(&printed_type_text),
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    if self.diagnostics.len() == diagnostics_before {
                        let _ = self.emit_non_serializable_import_type_diagnostic(
                            emitted_type_text.as_deref().unwrap_or(&printed_type_text),
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    if self.diagnostics.len() == diagnostics_before {
                        let _ = self.emit_non_serializable_property_diagnostic(
                            emitted_type_text.as_deref().unwrap_or(&printed_type_text),
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                }

                if keyword == "const"
                    && let Some(interner) = self.type_interner
                {
                    if let Some(lit) = tsz_solver::visitor::literal_value(interner, type_id) {
                        let formatted = Self::format_literal_initializer(&lit, interner);
                        // Infinity/-Infinity must use type annotation syntax (`: Infinity`)
                        // not initializer syntax (`= Infinity`) in DTS, since they are
                        // runtime values, not literal types that can be const initializers.
                        let is_infinity = formatted == "Infinity" || formatted == "-Infinity";
                        if is_infinity || self.source_is_js_file {
                            self.write(": ");
                        } else {
                            self.write(" = ");
                        }
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

                if let Some(type_text) = emitted_type_text.as_deref() {
                    self.write(": ");
                    self.write(type_text);
                } else {
                    self.write(": ");
                    self.write(&printed_type_text);
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
                .infer_fallback_type_text(initializer)
                .or_else(|| self.data_view_new_expression_type_text(initializer))
            {
                self.write(": ");
                self.write(&type_text);
            } else if has_initializer || keyword != "const" {
                // tsc always emits a type annotation in .d.ts output.
                // For var/let without type info, and for const with an
                // initializer but no resolved type, default to `: any`.
                self.write(": any");
            }
        }
    }

    pub(in crate::declaration_emitter) fn data_view_new_expression_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
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

    pub(in crate::declaration_emitter) fn data_view_backing_store_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
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

    pub(in crate::declaration_emitter) fn initializer_is_new_expression(&self, initializer: NodeIndex) -> bool {
        let initializer = self.skip_parenthesized_non_null_and_comma(initializer);
        self.arena
            .get(initializer)
            .is_some_and(|node| node.kind == syntax_kind_ext::NEW_EXPRESSION)
    }

    pub(in crate::declaration_emitter) fn synthetic_class_extends_alias_type_id(
        &self,
        heritage: Option<&NodeList>,
    ) -> Option<tsz_solver::TypeId> {
        let heritage = heritage?;
        let (type_idx, expr_idx) = self.non_nameable_extends_heritage_type(heritage)?;
        self.get_node_type_or_names(&[expr_idx, type_idx])
    }

    pub(crate) fn retain_synthetic_class_extends_alias_dependencies_in_statements(
        &mut self,
        statements: &NodeList,
    ) {
        for &stmt_idx in &statements.nodes {
            self.retain_synthetic_class_extends_alias_dependencies_for_statement(stmt_idx);
        }
    }

    pub(in crate::declaration_emitter) fn retain_synthetic_class_extends_alias_dependencies_for_statement(
        &mut self,
        stmt_idx: NodeIndex,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(stmt_node)
                    && self.statement_has_effective_export(stmt_idx)
                    && let Some(type_id) =
                        self.synthetic_class_extends_alias_type_id(class.heritage_clauses.as_ref())
                {
                    self.retain_direct_type_symbols_for_public_api(type_id);
                }
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export) = self.arena.get_export_decl(stmt_node)
                    && let Some(clause_node) = self.arena.get(export.export_clause)
                {
                    if clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
                        && let Some(class) = self.arena.get_class(clause_node)
                        && let Some(type_id) = self
                            .synthetic_class_extends_alias_type_id(class.heritage_clauses.as_ref())
                    {
                        self.retain_direct_type_symbols_for_public_api(type_id);
                    } else if clause_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                        self.retain_synthetic_module_extends_alias_dependencies(
                            export.export_clause,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.retain_synthetic_module_extends_alias_dependencies(stmt_idx);
            }
            _ => {}
        }
    }

    pub(in crate::declaration_emitter) fn retain_synthetic_module_extends_alias_dependencies(&mut self, module_idx: NodeIndex) {
        let Some(module_node) = self.arena.get(module_idx) else {
            return;
        };
        let Some(module) = self.arena.get_module(module_node) else {
            return;
        };

        let mut current_body = module.body;
        loop {
            let Some(body_node) = self.arena.get(current_body) else {
                return;
            };
            if let Some(nested_mod) = self.arena.get_module(body_node) {
                current_body = nested_mod.body;
                continue;
            }
            if let Some(block) = self.arena.get_module_block(body_node)
                && let Some(ref statements) = block.statements
            {
                self.retain_synthetic_class_extends_alias_dependencies_in_statements(statements);
            }
            return;
        }
    }

    pub(in crate::declaration_emitter) fn retain_direct_type_symbols_for_public_api(&mut self, type_id: tsz_solver::TypeId) {
        let (Some(used_symbols), Some(type_cache), Some(interner)) = (
            self.used_symbols.as_mut(),
            self.type_cache.as_ref(),
            self.type_interner,
        ) else {
            return;
        };

        let mut mark = |sym_id: SymbolId| {
            used_symbols
                .entry(sym_id)
                .and_modify(|kind| *kind |= super::super::usage_analyzer::UsageKind::TYPE)
                .or_insert(super::super::usage_analyzer::UsageKind::TYPE);
        };

        if let Some(def_id) = tsz_solver::visitor::lazy_def_id(interner, type_id)
            && let Some(&sym_id) = type_cache.def_to_symbol.get(&def_id)
        {
            mark(sym_id);
        }

        if let Some((def_id, _)) = tsz_solver::visitor::enum_components(interner, type_id)
            && let Some(&sym_id) = type_cache.def_to_symbol.get(&def_id)
        {
            mark(sym_id);
        }

        if let Some(shape_id) = tsz_solver::visitor::object_shape_id(interner, type_id)
            .or_else(|| tsz_solver::visitor::object_with_index_shape_id(interner, type_id))
            && let Some(sym_id) = interner.object_shape(shape_id).symbol
        {
            mark(sym_id);
        }

        if let Some(shape_id) = tsz_solver::visitor::callable_shape_id(interner, type_id)
            && let Some(sym_id) = interner.callable_shape(shape_id).symbol
        {
            mark(sym_id);
        }
    }

    pub(in crate::declaration_emitter) fn emit_direct_symbol_dependency_for_type(&mut self, type_id: tsz_solver::TypeId) {
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
        self.write(&self.print_synthetic_class_extends_alias_type(type_id));
        self.write(";");
        self.write_line();
        self.emitted_non_exported_declaration = true;

        Some(alias_name)
    }

    pub(in crate::declaration_emitter) fn emit_function_initializer_type_annotation(
        &mut self,
        decl_idx: NodeIndex,
        decl_name: NodeIndex,
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
        let is_self_returning = func.type_annotation.is_none()
            && self.function_initializer_is_self_returning(initializer);

        self.write(": ");
        if is_self_returning {
            self.emit_recursive_function_initializer_type(func, false);
            return true;
        }

        self.emit_function_initializer_signature(func);

        if func.type_annotation.is_some() {
            self.emit_type(func.type_annotation);
            return true;
        }

        if func.body.is_some()
            && let Some(returned_identifier) =
                self.function_body_unique_return_identifier(func.body)
            && let Some(type_text) =
                self.function_return_identifier_type_text(func, returned_identifier)
        {
            self.write(&type_text);
            return true;
        }

        if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            let func_type_id = cache
                .node_types
                .get(&initializer.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[decl_idx, decl_name, initializer]));
            if let Some(func_type_id) = func_type_id
                && let Some(return_type_id) =
                    tsz_solver::type_queries::get_return_type(*interner, func_type_id)
            {
                if return_type_id == tsz_solver::types::TypeId::ANY
                    && func.body.is_some()
                    && self.body_returns_void(func.body)
                {
                    self.write("void");
                } else {
                    self.write(&self.print_type_id(return_type_id));
                }
                return true;
            }
        }

        if func.body.is_some() && self.body_returns_void(func.body) {
            self.write("void");
        } else {
            self.write("any");
        }

        true
    }

    pub(in crate::declaration_emitter) fn maybe_emit_non_portable_function_return_diagnostic(
        &mut self,
        decl_name: NodeIndex,
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
        if func.type_annotation.is_some() {
            return;
        }
        if func.body.is_none() {
            return;
        }
        let body_idx = func.body;
        let Some(return_expr) = self.function_body_single_return_expression(body_idx) else {
            return;
        };
        let Some(name_node) = self.arena.get(decl_name) else {
            return;
        };
        let Some(file_path) = self.current_file_path.clone() else {
            return;
        };
        let Some(return_type_id) = self.get_node_type_or_names(&[return_expr]) else {
            return;
        };
        let Some(name_text) = self.get_identifier_text(decl_name) else {
            return;
        };
        let declared_identifier_idx = self.return_expression_identifier(return_expr);
        let declared_identifier_type_id = declared_identifier_idx.and_then(|identifier_idx| {
            self.function_return_identifier_declared_type_id(func, identifier_idx)
        });
        if let Some(type_name) = self.declared_return_identifier_type_name(func, return_expr)
            && let Some((from_path, _)) = declared_identifier_type_id
                .and_then(|type_id| self.find_non_portable_type_reference(type_id))
                .or_else(|| self.find_non_portable_type_reference(return_type_id))
        {
            self.emit_non_portable_named_reference_diagnostic(
                &name_text,
                &file_path,
                name_node.pos,
                name_node.end - name_node.pos,
                &from_path,
                &type_name,
            );
            return;
        }

        let _ = self.emit_non_portable_type_diagnostic(
            return_type_id,
            &name_text,
            &file_path,
            name_node.pos,
            name_node.end - name_node.pos,
        );
    }

    pub(in crate::declaration_emitter) fn declared_return_identifier_type_name(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        return_expr: NodeIndex,
    ) -> Option<String> {
        let identifier_idx = self.return_expression_identifier(return_expr)?;
        let type_text = self.function_return_identifier_type_text(func, identifier_idx)?;
        Self::simple_type_reference_name(&type_text)
    }

    pub(in crate::declaration_emitter) fn function_return_identifier_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        identifier_idx: NodeIndex,
    ) -> Option<String> {
        self.reference_declared_type_annotation_text(identifier_idx)
            .or_else(|| self.function_parameter_type_text(func, identifier_idx))
    }

    pub(in crate::declaration_emitter) fn function_return_identifier_declared_type_id(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        identifier_idx: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        self.reference_declared_type_id(identifier_idx)
            .or_else(|| self.function_parameter_type_id(func, identifier_idx))
    }

    pub(in crate::declaration_emitter) fn function_parameter_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        identifier_idx: NodeIndex,
    ) -> Option<String> {
        let identifier_name = self.get_identifier_text(identifier_idx)?;

        for param_idx in func.parameters.nodes.iter().copied() {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            let param_name = self.get_identifier_text(param.name)?;
            if param_name != identifier_name {
                continue;
            }
            let type_text = self
                .preferred_annotation_name_text(param.type_annotation)
                .or_else(|| self.emit_type_node_text(param.type_annotation))?;
            let trimmed = type_text.trim_end();
            let trimmed = trimmed.strip_suffix('=').unwrap_or(trimmed).trim_end();
            return Some(trimmed.to_string());
        }

        None
    }

    pub(in crate::declaration_emitter) fn function_parameter_type_id(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        identifier_idx: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let identifier_name = self.get_identifier_text(identifier_idx)?;

        for param_idx in func.parameters.nodes.iter().copied() {
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            let param_name = self.get_identifier_text(param.name)?;
            if param_name != identifier_name {
                continue;
            }
            let type_annotation = param.type_annotation;
            if !type_annotation.is_some() {
                return None;
            }
            return self.get_node_type_or_names(&[type_annotation]);
        }

        None
    }

    pub(in crate::declaration_emitter) fn reference_declared_type_id(&self, expr_idx: NodeIndex) -> Option<tsz_solver::types::TypeId> {
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let decl_node = self.arena.get(decl_idx)?;
            if let Some(var_decl) = self.arena.get_variable_declaration(decl_node)
                && var_decl.type_annotation.is_some()
            {
                let type_annotation = var_decl.type_annotation;
                return self.get_node_type_or_names(&[type_annotation]);
            }
            if let Some(prop_decl) = self.arena.get_property_decl(decl_node)
                && prop_decl.type_annotation.is_some()
            {
                let type_annotation = prop_decl.type_annotation;
                return self.get_node_type_or_names(&[type_annotation]);
            }
            if let Some(param) = self.arena.get_parameter(decl_node)
                && param.type_annotation.is_some()
            {
                let type_annotation = param.type_annotation;
                return self.get_node_type_or_names(&[type_annotation]);
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn simple_type_reference_name(type_text: &str) -> Option<String> {
        let trimmed = type_text.trim();
        if trimmed.is_empty()
            || trimmed.contains("=>")
            || trimmed.contains('{')
            || trimmed.contains('[')
            || trimmed.contains(" & ")
            || trimmed.contains(" | ")
            || trimmed.contains('\n')
        {
            return None;
        }

        let candidate = trimmed.rsplit('.').next()?.trim();
        if candidate.is_empty() {
            return None;
        }

        candidate
            .chars()
            .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
            .then(|| candidate.to_string())
    }

    pub(in crate::declaration_emitter) fn emit_recursive_function_initializer_type(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        elide_return: bool,
    ) {
        self.emit_function_initializer_signature(func);
        if elide_return {
            self.write("/*elided*/ any");
        } else {
            self.emit_recursive_function_initializer_type(func, true);
        }
    }

    pub(in crate::declaration_emitter) fn emit_function_initializer_signature(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
    ) {
        if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }
        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write(") => ");
    }

    pub(in crate::declaration_emitter) fn function_initializer_has_inline_parameter_comments(&self, initializer: NodeIndex) -> bool {
        if self.remove_comments {
            return false;
        }

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

        func.parameters.nodes.iter().any(|&param_idx| {
            self.arena.get(param_idx).is_some_and(|param_node| {
                self.parameter_has_leading_inline_block_comment(param_node.pos)
            })
        })
    }

    pub(in crate::declaration_emitter) fn function_initializer_is_self_returning(&self, initializer: NodeIndex) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION {
            return false;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return false;
        };
        let Some(name) = self.get_identifier_text(func.name) else {
            return false;
        };
        self.function_body_returns_identifier(func.body, &name)
    }

    pub(in crate::declaration_emitter) fn function_initializer_returns_unique_identifier(&self, initializer: NodeIndex) -> bool {
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
        func.type_annotation.is_none()
            && func.body.is_some()
            && self
                .function_body_unique_return_identifier(func.body)
                .is_some()
    }

    pub(in crate::declaration_emitter) fn refine_invokable_return_type_from_identifier(
        &self,
        body_idx: NodeIndex,
        inferred_return_type: tsz_solver::types::TypeId,
    ) -> Option<tsz_solver::types::TypeId> {
        let interner = self.type_interner?;
        if !tsz_solver::type_queries::is_invokable_type(interner, inferred_return_type)
            || self.type_has_visible_declaration_members(inferred_return_type)
        {
            return None;
        }

        let returned_identifier = self.function_body_unique_return_identifier(body_idx)?;
        let returned_identifier_type = self
            .get_node_type_or_names(&[returned_identifier])
            .or_else(|| self.get_type_via_symbol(returned_identifier))?;
        if tsz_solver::type_queries::is_invokable_type(interner, returned_identifier_type)
            && self.type_has_visible_declaration_members(returned_identifier_type)
        {
            return Some(returned_identifier_type);
        }

        None
    }

    pub(in crate::declaration_emitter) fn function_body_returns_identifier(&self, body_idx: NodeIndex, name: &str) -> bool {
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
            .any(|stmt_idx| self.statement_returns_identifier(stmt_idx, name))
    }

    pub(in crate::declaration_emitter) fn emit_js_returned_define_property_function_type(
        &mut self,
        body_idx: NodeIndex,
    ) -> bool {
        let Some((initializer, properties)) =
            self.js_returned_define_property_function_info(body_idx)
        else {
            return false;
        };

        self.write(": ");
        self.write("{");
        self.write_line();
        self.increase_indent();
        self.write_indent();
        if !self.emit_function_initializer_call_signature(initializer) {
            self.decrease_indent();
            return false;
        }
        self.write(";");
        self.write_line();

        for property in properties {
            self.write_indent();
            if property.readonly {
                self.write("readonly ");
            }
            self.write(&self.declaration_property_name_text(&property.name));
            self.write(": ");
            self.write(&property.type_text);
            self.write(";");
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        true
    }

    pub(in crate::declaration_emitter) fn function_body_unique_return_identifier(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        let mut returned_identifier = None;
        if self.collect_unique_return_identifier_from_block(
            &block.statements,
            &mut returned_identifier,
        ) {
            returned_identifier
        } else {
            None
        }
    }

    pub(in crate::declaration_emitter) fn function_body_single_return_expression(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        let stmt_idx = *block.statements.nodes.first()?;
        if block.statements.nodes.len() != 1 {
            return None;
        }
        let stmt_node = self.arena.get(stmt_idx)?;
        let ret = self.arena.get_return_statement(stmt_node)?;
        Some(ret.expression)
    }

    pub(in crate::declaration_emitter) fn collect_unique_return_identifier_from_block(
        &self,
        statements: &NodeList,
        returned_identifier: &mut Option<NodeIndex>,
    ) -> bool {
        statements.nodes.iter().copied().all(|stmt_idx| {
            self.collect_unique_return_identifier_from_statement(stmt_idx, returned_identifier)
        })
    }

    pub(in crate::declaration_emitter) fn collect_unique_return_identifier_from_statement(
        &self,
        stmt_idx: NodeIndex,
        returned_identifier: &mut Option<NodeIndex>,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return true;
        };
        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let Some(ret) = self.arena.get_return_statement(stmt_node) else {
                    return false;
                };
                let Some(identifier_idx) = self.return_expression_identifier(ret.expression) else {
                    return false;
                };

                if let Some(existing_idx) = *returned_identifier {
                    return self
                        .get_identifier_text(existing_idx)
                        .zip(self.get_identifier_text(identifier_idx))
                        .is_some_and(|(existing, current)| existing == current);
                }

                *returned_identifier = Some(identifier_idx);
                true
            }
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    self.collect_unique_return_identifier_from_block(
                        &block.statements,
                        returned_identifier,
                    )
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => self
                .arena
                .get_if_statement(stmt_node)
                .is_some_and(|if_data| {
                    self.collect_unique_return_identifier_from_statement(
                        if_data.then_statement,
                        returned_identifier,
                    ) && if_data.else_statement.is_some()
                        && self.collect_unique_return_identifier_from_statement(
                            if_data.else_statement,
                            returned_identifier,
                        )
                }),
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.arena.get_try(stmt_node).is_some_and(|try_data| {
                    self.collect_unique_return_identifier_from_statement(
                        try_data.try_block,
                        returned_identifier,
                    ) && try_data.catch_clause.is_some()
                        && self.collect_unique_return_identifier_from_statement(
                            try_data.catch_clause,
                            returned_identifier,
                        )
                        && try_data.finally_block.is_some()
                        && self.collect_unique_return_identifier_from_statement(
                            try_data.finally_block,
                            returned_identifier,
                        )
                })
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => self
                .arena
                .get_catch_clause(stmt_node)
                .is_some_and(|catch_data| {
                    self.collect_unique_return_identifier_from_statement(
                        catch_data.block,
                        returned_identifier,
                    )
                }),
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => self
                .arena
                .get_case_clause(stmt_node)
                .is_some_and(|case_data| {
                    self.collect_unique_return_identifier_from_block(
                        &case_data.statements,
                        returned_identifier,
                    )
                }),
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                self.arena.get_switch(stmt_node).is_some_and(|switch_data| {
                    self.arena
                        .get(switch_data.case_block)
                        .and_then(|case_block_node| self.arena.get_block(case_block_node))
                        .is_some_and(|block| {
                            self.collect_unique_return_identifier_from_block(
                                &block.statements,
                                returned_identifier,
                            )
                        })
                })
            }
            k if k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT =>
            {
                self.arena.get_loop(stmt_node).is_some_and(|loop_data| {
                    self.collect_unique_return_identifier_from_statement(
                        loop_data.statement,
                        returned_identifier,
                    )
                })
            }
            _ => true,
        }
    }

}
