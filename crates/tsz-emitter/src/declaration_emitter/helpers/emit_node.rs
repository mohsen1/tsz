//! Expression/node emission and source file analysis

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

use super::JsFoldedNamedExports;

impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn emit_expression(&mut self, expr_idx: NodeIndex) {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return;
        };
        let before_len = self.writer.len();
        self.queue_source_mapping(expr_node);

        match expr_node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(expr_node) {
                    // Normalize large numeric literals through f64 representation
                    // to match tsc's behavior (round-trips numbers through JS).
                    self.write(&Self::normalize_numeric_literal(&lit.text));
                }
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                // tsc normalizes initializer string literals to double quotes.
                // The scanner stores cooked text, so we must re-escape
                // backslashes, double quotes, and control characters.
                if let Some(lit) = self.arena.get_literal(expr_node) {
                    self.write("\"");
                    let needs_escape = lit.text.contains('\\')
                        || lit.text.contains('"')
                        || lit.text.contains('\n')
                        || lit.text.contains('\r')
                        || lit.text.contains('\t')
                        || lit.text.contains('\0');
                    if needs_escape {
                        let mut escaped = String::with_capacity(lit.text.len() + 4);
                        for ch in lit.text.chars() {
                            match ch {
                                '\\' => escaped.push_str("\\\\"),
                                '"' => escaped.push_str("\\\""),
                                '\n' => escaped.push_str("\\n"),
                                '\r' => escaped.push_str("\\r"),
                                '\t' => escaped.push_str("\\t"),
                                '\0' => escaped.push_str("\\0"),
                                c => escaped.push(c),
                            }
                        }
                        self.write(&escaped);
                    } else {
                        self.write(&lit.text);
                    }
                    self.write("\"");
                }
            }
            k if k == SyntaxKind::NullKeyword as u16 => {
                self.write("null");
            }
            k if k == SyntaxKind::TrueKeyword as u16 => {
                self.write("true");
            }
            k if k == SyntaxKind::FalseKeyword as u16 => {
                self.write("false");
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(expr_node) {
                    self.write(&lit.text);
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(expr_node) {
                    if unary.operator == SyntaxKind::MinusToken as u16 {
                        self.write("-");
                    } else if unary.operator == SyntaxKind::PlusToken as u16 {
                        self.write("+");
                    }
                    self.emit_expression(unary.operand);
                }
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                // Array literal in default parameter: emit as []
                self.write("[]");
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                // Object literal in default parameter: emit as {}
                self.write("{}");
            }
            _ => self.emit_node(expr_idx),
        }

        if self.writer.len() == before_len {
            self.pending_source_pos = None;
        }
    }

    pub(crate) fn emit_node(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };
        let before_len = self.writer.len();
        self.queue_source_mapping(node);

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(node) {
                    self.write(&ident.escaped_text);
                }
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                // Type parameter node - emit its name
                if let Some(param) = self.arena.get_type_parameter(node) {
                    self.emit_node(param.name);
                }
            }
            k if k == syntax_kind_ext::QUALIFIED_NAME
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || k == SyntaxKind::ThisKeyword as u16
                || k == SyntaxKind::SuperKeyword as u16 =>
            {
                self.emit_entity_name(node_idx);
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    let quote = self.original_quote_char(node);
                    let quote_char = if quote == "'" { '\'' } else { '"' };
                    self.write(quote);
                    // The scanner stores cooked (unescaped) text, so we must
                    // re-escape characters that cannot appear raw in string
                    // literals: backslashes, the surrounding quote character,
                    // newlines, carriage returns, tabs, and null bytes.
                    let needs_escape = lit.text.contains('\\')
                        || lit.text.contains(quote_char)
                        || lit.text.contains('\n')
                        || lit.text.contains('\r')
                        || lit.text.contains('\t')
                        || lit.text.contains('\0');
                    if needs_escape {
                        let mut escaped = String::with_capacity(lit.text.len() + 4);
                        for ch in lit.text.chars() {
                            match ch {
                                '\\' => escaped.push_str("\\\\"),
                                '\n' => escaped.push_str("\\n"),
                                '\r' => escaped.push_str("\\r"),
                                '\t' => escaped.push_str("\\t"),
                                '\0' => escaped.push_str("\\0"),
                                c if c == quote_char => {
                                    escaped.push('\\');
                                    escaped.push(c);
                                }
                                c => escaped.push(c),
                            }
                        }
                        self.write(&escaped);
                    } else {
                        self.write(&lit.text);
                    }
                    self.write(quote);
                }
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    // Strip numeric separators (tsc strips them in .d.ts output)
                    if lit.text.contains('_') {
                        if let Some(v) = lit.value {
                            if v.fract() == 0.0 && v.abs() < 1e20 {
                                self.write(&format!("{}", v as i64));
                            } else {
                                self.write(&v.to_string());
                            }
                        } else {
                            self.write(&lit.text.replace('_', ""));
                        }
                    } else {
                        self.write(&lit.text);
                    }
                }
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    if lit.text.contains('_') {
                        self.write(&lit.text.replace('_', ""));
                    } else {
                        self.write(&lit.text);
                    }
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                if let Some(computed) = self.arena.get_computed_property(node) {
                    self.write("[");
                    self.emit_node(computed.expression);
                    self.write("]");
                }
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    self.write("[");
                    let mut first = true;
                    for &elem_idx in &pattern.elements.nodes {
                        if !first {
                            self.write(", ");
                        }
                        first = false;
                        if let Some(elem_node) = self.arena.get(elem_idx) {
                            if elem_node.kind == syntax_kind_ext::BINDING_ELEMENT {
                                if let Some(elem) = self.arena.get_binding_element(elem_node) {
                                    if elem.dot_dot_dot_token {
                                        self.write("...");
                                    }
                                    self.emit_node(elem.name);
                                }
                            } else if elem_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                                // Empty slot in array pattern: [, x] → skip (comma already emitted)
                            }
                        }
                    }
                    self.write("]");
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr(node) {
                    if unary.operator == SyntaxKind::MinusToken as u16 {
                        self.write("-");
                    } else if unary.operator == SyntaxKind::PlusToken as u16 {
                        self.write("+");
                    }
                    self.emit_node(unary.operand);
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    if pattern.elements.nodes.is_empty() {
                        self.write("{}");
                    } else {
                        self.write("{ ");
                        let mut first = true;
                        for &elem_idx in &pattern.elements.nodes {
                            if !first {
                                self.write(", ");
                            }
                            first = false;
                            if let Some(elem_node) = self.arena.get(elem_idx)
                                && elem_node.kind == syntax_kind_ext::BINDING_ELEMENT
                                && let Some(elem) = self.arena.get_binding_element(elem_node)
                            {
                                if elem.dot_dot_dot_token {
                                    self.write("...");
                                }
                                if elem.property_name.is_some() {
                                    self.emit_node(elem.property_name);
                                    self.write(": ");
                                }
                                self.emit_node(elem.name);
                            }
                        }
                        if pattern.elements.has_trailing_comma {
                            self.write(", }");
                        } else {
                            self.write(" }");
                        }
                    }
                }
            }
            // Fallback for contextual keywords and other unhandled node kinds used as names.
            _ if self.source_file_text.is_some() => {
                if let Some(text) = self.get_source_slice(node.pos, node.end) {
                    self.write(&text);
                }
            }
            _ => {}
        }

        if self.writer.len() == before_len {
            self.pending_source_pos = None;
        }
    }

    pub(crate) fn has_public_api_exports(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        if source_file.is_declaration_file {
            return false;
        }

        let mut has_import = false;
        let mut has_export = false;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION
                || stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                has_import = true;
            }

            if self.stmt_has_export_modifier(stmt_node)
                || stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                || stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                || (stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                    && self
                        .js_supported_commonjs_named_export_for_statement(stmt_idx)
                        .is_some())
            {
                has_export = true;
            }
        }

        has_import || has_export
    }

    pub(crate) fn source_file_has_module_syntax(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        source_file.statements.nodes.iter().any(|&stmt_idx| {
            self.arena.get(stmt_idx).is_some_and(|stmt_node| {
                let k = stmt_node.kind;
                k == syntax_kind_ext::IMPORT_DECLARATION
                    || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    || k == syntax_kind_ext::EXPORT_DECLARATION
                    || k == syntax_kind_ext::EXPORT_ASSIGNMENT
                    || k == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                    || self.stmt_has_export_modifier(stmt_node)
                    || self
                        .js_supported_commonjs_named_export_for_statement(stmt_idx)
                        .is_some()
            })
        })
    }

    pub(crate) fn source_file_is_js(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        if source_file.is_declaration_file {
            return false;
        }

        let lower = source_file.file_name.to_ascii_lowercase();
        lower.ends_with(".js")
            || lower.ends_with(".jsx")
            || lower.ends_with(".mjs")
            || lower.ends_with(".cjs")
    }

    pub(crate) fn collect_js_folded_named_exports(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> JsFoldedNamedExports {
        let mut names = FxHashSet::default();
        let mut folded_exports = FxHashMap::default();
        let mut deferred_statements = FxHashSet::default();
        if !self.source_file_is_js(source_file) {
            return (names, folded_exports, deferred_statements);
        }

        let export_targets = self.collect_js_named_export_targets(source_file);

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }

            let Some(export) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export.module_specifier.is_some() || export.export_clause.is_none() {
                continue;
            }

            let Some(clause_node) = self.arena.get(export.export_clause) else {
                continue;
            };
            if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                continue;
            }

            let Some(named) = self.arena.get_named_imports(clause_node) else {
                continue;
            };
            if named.elements.nodes.is_empty() {
                continue;
            }

            let mut foldable_names = Vec::new();
            let mut target_statements = Vec::new();
            let mut seen_target_statements = FxHashSet::default();

            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = self.arena.get(spec_idx) else {
                    foldable_names.clear();
                    target_statements.clear();
                    break;
                };
                let Some(spec) = self.arena.get_specifier(spec_node) else {
                    foldable_names.clear();
                    target_statements.clear();
                    break;
                };
                if spec.property_name.is_some() {
                    foldable_names.clear();
                    target_statements.clear();
                    break;
                }
                let Some(name_node) = self.arena.get(spec.name) else {
                    foldable_names.clear();
                    target_statements.clear();
                    break;
                };
                let Some(name_ident) = self.arena.get_identifier(name_node) else {
                    foldable_names.clear();
                    target_statements.clear();
                    break;
                };
                let name = name_ident.escaped_text.clone();
                let Some(&target_stmt_idx) = export_targets.get(&name) else {
                    foldable_names.clear();
                    target_statements.clear();
                    break;
                };
                foldable_names.push(name);
                if seen_target_statements.insert(target_stmt_idx) {
                    target_statements.push(target_stmt_idx);
                }
            }

            if foldable_names.is_empty() {
                continue;
            }

            for name in foldable_names {
                names.insert(name);
            }

            let mut deferred_targets = Vec::new();
            for target_stmt_idx in target_statements {
                let Some(target_stmt_node) = self.arena.get(target_stmt_idx) else {
                    continue;
                };
                if self.stmt_has_export_modifier(target_stmt_node)
                    || self.statement_has_attached_jsdoc(source_file, target_stmt_node)
                {
                    continue;
                }
                if deferred_statements.insert(target_stmt_idx) {
                    deferred_targets.push(target_stmt_idx);
                }
            }

            folded_exports.insert(stmt_idx, deferred_targets);
        }

        (names, folded_exports, deferred_statements)
    }

    pub(in crate::declaration_emitter) fn collect_js_named_export_targets(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> FxHashMap<String, NodeIndex> {
        let mut targets = FxHashMap::default();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            match stmt_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    let Some(func) = self.arena.get_function(stmt_node) else {
                        continue;
                    };
                    let Some(name_node) = self.arena.get(func.name) else {
                        continue;
                    };
                    let Some(name_ident) = self.arena.get_identifier(name_node) else {
                        continue;
                    };
                    targets.insert(name_ident.escaped_text.clone(), stmt_idx);
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    let Some(class) = self.arena.get_class(stmt_node) else {
                        continue;
                    };
                    let Some(name_node) = self.arena.get(class.name) else {
                        continue;
                    };
                    let Some(name_ident) = self.arena.get_identifier(name_node) else {
                        continue;
                    };
                    targets.insert(name_ident.escaped_text.clone(), stmt_idx);
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                        continue;
                    };

                    let mut declaration_names = Vec::new();
                    let mut supported = true;
                    for &decl_list_idx in &var_stmt.declarations.nodes {
                        let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                            supported = false;
                            break;
                        };
                        let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                            supported = false;
                            break;
                        };
                        for &decl_idx in &decl_list.declarations.nodes {
                            let Some(decl_node) = self.arena.get(decl_idx) else {
                                supported = false;
                                break;
                            };
                            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                                supported = false;
                                break;
                            };
                            let Some(name_node) = self.arena.get(decl.name) else {
                                supported = false;
                                break;
                            };
                            if name_node.kind != SyntaxKind::Identifier as u16 {
                                supported = false;
                                break;
                            }
                            let Some(name_ident) = self.arena.get_identifier(name_node) else {
                                supported = false;
                                break;
                            };
                            declaration_names.push(name_ident.escaped_text.clone());
                        }
                        if !supported {
                            break;
                        }
                    }

                    if supported && declaration_names.len() == 1 {
                        targets.insert(declaration_names.pop().unwrap_or_default(), stmt_idx);
                    }
                }
                _ => {}
            }
        }

        targets
    }
}
