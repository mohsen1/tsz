//! Declaration emitter - expression/node emission, import management, and utility helpers.
//!
//! Type syntax emission (type references, unions, mapped types, etc.) is in `type_emission.rs`.

use super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
use crate::emitter::type_printer::TypePrinter;
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tracing::debug;
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
use tsz_parser::parser::ParserState;
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

/// Escape a cooked string value for embedding in a double-quoted string literal.
///
/// The scanner stores "cooked" (unescaped) text for string literals. When
/// writing strings back into `.d.ts` output we must re-escape characters
/// that cannot appear raw inside double-quoted string literals.
fn escape_string_for_double_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out
}

type JsFoldedNamedExports = (
    FxHashSet<String>,
    FxHashMap<NodeIndex, Vec<NodeIndex>>,
    FxHashSet<NodeIndex>,
);
type JsNamespaceExportAliases = FxHashMap<String, Vec<(String, String)>>;
type JsCommonjsSyntheticStatements = FxHashMap<NodeIndex, (NodeIndex, NodeIndex)>;
type JsCommonjsNamedExports = (
    FxHashSet<String>,
    JsCommonjsSyntheticStatements,
    JsCommonjsSyntheticStatements,
);

#[derive(Clone, Copy)]
enum JsCommonjsExpandoDeclKind {
    Function,
    Value,
    PrototypeMethod,
}

#[derive(Default)]
pub(crate) struct JsCommonjsExpandoDeclarations {
    pub(crate) function_statements: FxHashMap<NodeIndex, (NodeIndex, NodeIndex)>,
    pub(crate) value_statements: FxHashMap<NodeIndex, (NodeIndex, NodeIndex)>,
    pub(crate) prototype_methods: FxHashMap<String, Vec<(NodeIndex, NodeIndex)>>,
}

#[derive(Clone)]
pub(crate) struct JsStaticMethodAugmentationGroup {
    pub(crate) class_idx: NodeIndex,
    pub(crate) method_idx: NodeIndex,
    pub(crate) class_is_exported: bool,
    pub(crate) properties: Vec<(NodeIndex, NodeIndex)>,
}

#[derive(Default)]
pub(crate) struct JsStaticMethodAugmentations {
    pub(crate) statements: FxHashMap<NodeIndex, JsStaticMethodAugmentationGroup>,
    pub(crate) skipped_statements: FxHashSet<NodeIndex>,
    pub(crate) augmented_method_nodes: FxHashSet<NodeIndex>,
}

/// Collected prototype member assignments for JS class-like heuristic variables.
/// e.g. `let A; A.prototype.b = {};` → variable `A` becomes `declare class A { ... }`.
#[derive(Default)]
pub(crate) struct JsClassLikePrototypeMembers {
    /// Maps variable name → list of (member_name_idx, initializer_idx) pairs.
    pub(crate) members: FxHashMap<String, Vec<(NodeIndex, NodeIndex)>>,
    /// Statement indices consumed by the class-like heuristic (to skip during normal emit).
    pub(crate) consumed_stmts: FxHashSet<NodeIndex>,
}

type JsStaticMethodKey = (String, String);
type JsStaticMethodInfo = (NodeIndex, NodeIndex, bool);
type JsStaticMethodAugmentationEntry = (
    NodeIndex,
    NodeIndex,
    NodeIndex,
    bool,
    Vec<(NodeIndex, NodeIndex)>,
);

struct JsdocTypeAliasDecl {
    name: String,
    type_params: Vec<String>,
    type_text: String,
    description_lines: Vec<String>,
    render_verbatim: bool,
}

struct JsDefinedPropertyDecl {
    name: String,
    type_text: String,
    readonly: bool,
}

#[derive(Clone)]
pub(crate) struct JsdocParamDecl {
    pub(crate) name: String,
    pub(crate) type_text: String,
    pub(crate) optional: bool,
    pub(crate) rest: bool,
}

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
                        self.write(" }");
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

            match stmt_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    if let Some(func) = self.arena.get_function(stmt_node)
                        && self
                            .arena
                            .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(class) = self.arena.get_class(stmt_node)
                        && self
                            .arena
                            .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    if let Some(iface) = self.arena.get_interface(stmt_node)
                        && self
                            .arena
                            .has_modifier(&iface.modifiers, SyntaxKind::ExportKeyword)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    if let Some(alias) = self.arena.get_type_alias(stmt_node)
                        && self
                            .arena
                            .has_modifier(&alias.modifiers, SyntaxKind::ExportKeyword)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    if let Some(enum_data) = self.arena.get_enum(stmt_node)
                        && self
                            .arena
                            .has_modifier(&enum_data.modifiers, SyntaxKind::ExportKeyword)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    if let Some(var_stmt) = self.arena.get_variable(stmt_node)
                        && self
                            .arena
                            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    if let Some(module) = self.arena.get_module(stmt_node)
                        && self
                            .arena
                            .has_modifier(&module.modifiers, SyntaxKind::ExportKeyword)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION
                    || k == syntax_kind_ext::EXPORT_ASSIGNMENT =>
                {
                    has_export = true;
                }
                k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                    if self
                        .js_supported_commonjs_named_export_for_statement(stmt_idx)
                        .is_some()
                    {
                        has_export = true;
                    }
                }
                _ => {}
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

    fn collect_js_named_export_targets(
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

    fn statement_has_attached_jsdoc(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
        stmt_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let text = source_file.text.as_ref();
        let bytes = text.as_bytes();
        let mut actual_start = stmt_node.pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }

        let mut scan_start = actual_start;
        for comment in source_file.comments.iter().rev() {
            if comment.end as usize > scan_start {
                continue;
            }

            let between = &text[comment.end as usize..scan_start];
            if !between
                .bytes()
                .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            {
                break;
            }

            let comment_text = &text[comment.pos as usize..comment.end as usize];
            if comment_text.starts_with("/**") && comment_text != "/**/" {
                return true;
            }

            scan_start = comment.pos as usize;
        }

        false
    }

    pub(crate) fn leading_jsdoc_type_expr_for_pos(&self, pos: u32) -> Option<String> {
        let text = self.source_file_text.as_deref()?;
        let bytes = text.as_bytes();
        let mut actual_start = pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }

        let nearest = self
            .all_comments
            .iter()
            .filter(|comment| comment.end as usize <= actual_start)
            .filter(|comment| is_jsdoc_comment(comment, text))
            .filter(|comment| {
                Self::jsdoc_attaches_through_var_prefix(&text[comment.end as usize..actual_start])
            })
            .max_by_key(|comment| comment.end)?;

        let jsdoc = get_jsdoc_content(nearest, text);
        if let Some(expr) = Self::extract_jsdoc_type_expression(&jsdoc) {
            return Some(expr.trim().to_string());
        }

        None
    }

    fn jsdoc_attaches_through_var_prefix(between: &str) -> bool {
        let trimmed = between.trim();
        if trimmed.is_empty() {
            return true;
        }

        trimmed.split_whitespace().all(|word| {
            matches!(
                word,
                "export" | "declare" | "const" | "let" | "var" | "using" | "await"
            )
        })
    }

    fn extract_jsdoc_type_expression(jsdoc: &str) -> Option<&str> {
        let typedef_pos = jsdoc.find("@typedef");
        let mut tag_pos = jsdoc.find("@type");

        while let Some(pos) = tag_pos {
            let next_char = jsdoc[pos + "@type".len()..].chars().next();
            if !next_char.is_some_and(|c| c.is_alphabetic()) {
                if let Some(td_pos) = typedef_pos
                    && td_pos < pos
                {
                    let typedef_rest = &jsdoc[td_pos + "@typedef".len()..pos];
                    let mut has_non_object_base = false;
                    if let Some(open) = typedef_rest.find('{')
                        && let Some(close) = typedef_rest[open..].find('}')
                    {
                        let base = typedef_rest[open + 1..open + close].trim();
                        if base != "Object" && base != "object" && !base.is_empty() {
                            has_non_object_base = true;
                        }
                    }
                    if !has_non_object_base {
                        return None;
                    }
                }
                break;
            }
            tag_pos = jsdoc[pos + 1..].find("@type").map(|p| p + pos + 1);
        }
        let tag_pos = tag_pos?;
        let rest = &jsdoc[tag_pos + "@type".len()..];

        if let Some(open) = rest.find('{') {
            let after_open = &rest[open + 1..];
            let mut depth = 1usize;
            let mut end_idx = None;
            for (i, ch) in after_open.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end_idx = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(end_idx) = end_idx {
                return Some(after_open[..end_idx].trim());
            }
        }

        let rest = rest.trim_start();
        if rest.is_empty() || rest.starts_with('@') || rest.starts_with('*') {
            return None;
        }
        let end = rest
            .find('\n')
            .or_else(|| rest.find("*/"))
            .unwrap_or(rest.len());
        let expr = rest[..end].trim().trim_end_matches('*').trim();
        if expr.is_empty() { None } else { Some(expr) }
    }

    fn jsdoc_name_like_type_reference(expr: &str) -> bool {
        let expr = expr.trim();
        if expr.is_empty() {
            return false;
        }

        if expr
            .chars()
            .all(|ch| ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric())
        {
            return true;
        }

        let Some(rest) = expr
            .strip_prefix("import(\"")
            .or_else(|| expr.strip_prefix("import('"))
        else {
            return false;
        };

        let quote = if expr.starts_with("import(\"") {
            '"'
        } else {
            '\''
        };
        let Some(close) = rest.find(&format!("{quote})")) else {
            return false;
        };
        let suffix = &rest[close + 2..];
        let Some(member_path) = suffix.strip_prefix('.') else {
            return false;
        };
        !member_path.is_empty()
            && member_path
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric())
    }

    pub(crate) fn jsdoc_name_like_type_expr_for_pos(&self, pos: u32) -> Option<String> {
        let expr = self.leading_jsdoc_type_expr_for_pos(pos)?;
        if Self::jsdoc_name_like_type_reference(&expr) {
            Some(expr)
        } else {
            None
        }
    }

    pub(crate) fn jsdoc_name_like_type_expr_for_node(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        self.jsdoc_name_like_type_expr_for_pos(node.pos)
    }

    fn leading_jsdoc_comment_chain_for_pos(&self, pos: u32) -> Vec<String> {
        let Some(text) = self.source_file_text.as_deref() else {
            return Vec::new();
        };
        let bytes = text.as_bytes();
        let mut actual_start = pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }

        let nearest = self
            .all_comments
            .iter()
            .filter(|comment| comment.end as usize <= actual_start)
            .filter(|comment| is_jsdoc_comment(comment, text))
            .filter(|comment| {
                Self::jsdoc_attaches_through_var_prefix(&text[comment.end as usize..actual_start])
            })
            .max_by_key(|comment| comment.end);

        let Some(nearest) = nearest else {
            return Vec::new();
        };

        let mut chain = vec![get_jsdoc_content(nearest, text)];
        let mut current_start = nearest.pos as usize;
        for comment in self
            .all_comments
            .iter()
            .filter(|comment| comment.end <= nearest.pos)
            .filter(|comment| is_jsdoc_comment(comment, text))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            let between = &text[comment.end as usize..current_start];
            if !between
                .bytes()
                .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            {
                break;
            }
            chain.push(get_jsdoc_content(comment, text));
            current_start = comment.pos as usize;
        }
        chain.reverse();
        chain
    }

    fn leading_jsdoc_comment_chain_for_node_or_ancestors(&self, idx: NodeIndex) -> Vec<String> {
        let mut current = idx;
        for _ in 0..5 {
            let Some(node) = self.arena.get(current) else {
                break;
            };
            let chain = self.leading_jsdoc_comment_chain_for_pos(node.pos);
            if !chain.is_empty() {
                return chain;
            }
            let Some(ext) = self.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        Vec::new()
    }

    pub(crate) fn function_like_jsdoc_for_node(&self, idx: NodeIndex) -> Option<String> {
        let chain = self.leading_jsdoc_comment_chain_for_node_or_ancestors(idx);
        if chain.is_empty() {
            None
        } else {
            Some(chain.join("\n"))
        }
    }

    fn normalize_jsdoc_block(jsdoc: &str) -> String {
        jsdoc
            .lines()
            .map(|line| line.trim_start_matches('*').trim())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn normalize_jsdoc_param_name(text: &str) -> (String, bool) {
        let text = text.trim();
        if let Some(inner) = text
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            let name = inner.split('=').next().unwrap_or(inner).trim();
            return (name.to_string(), true);
        }
        (text.to_string(), false)
    }

    fn normalize_jsdoc_type_text(type_expr: &str, rest: bool) -> String {
        let trimmed = type_expr.trim();
        let normalized = if trimmed == "*" { "any" } else { trimmed };
        if rest {
            format!("{normalized}[]")
        } else {
            normalized.to_string()
        }
    }

    fn parse_jsdoc_param_decl(line: &str) -> Option<JsdocParamDecl> {
        let rest = line.strip_prefix("@param")?.trim();
        let (raw_type_expr, raw_name) = Self::parse_jsdoc_braced_type_and_name(rest)?;
        let raw_name = raw_name
            .split_whitespace()
            .next()
            .filter(|name| !name.is_empty())?;
        let (name, optional_name) = Self::normalize_jsdoc_param_name(raw_name);

        let mut type_expr = raw_type_expr.trim();
        let optional_type = type_expr.ends_with('=');
        if optional_type {
            type_expr = type_expr[..type_expr.len() - 1].trim();
        }

        let (rest_param, base_type) = if let Some(stripped) = type_expr.strip_prefix("...") {
            (true, stripped.trim())
        } else {
            (false, type_expr)
        };

        Some(JsdocParamDecl {
            name,
            type_text: Self::normalize_jsdoc_type_text(base_type, rest_param),
            optional: optional_name || optional_type,
            rest: rest_param,
        })
    }

    fn parse_jsdoc_param_decls(jsdoc: &str) -> Vec<JsdocParamDecl> {
        jsdoc
            .lines()
            .map(|raw_line| raw_line.trim_start_matches('*').trim())
            .filter_map(Self::parse_jsdoc_param_decl)
            .collect()
    }

    pub(crate) fn jsdoc_param_decl_for_parameter(
        &self,
        param_idx: NodeIndex,
        position: usize,
    ) -> Option<JsdocParamDecl> {
        let jsdoc = self.function_like_jsdoc_for_node(param_idx)?;
        let params = Self::parse_jsdoc_param_decls(&jsdoc);
        if params.is_empty() {
            return None;
        }

        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        if let Some(name) = self.get_identifier_text(param.name)
            && let Some(found) = params.iter().find(|decl| decl.name == name)
        {
            return Some(found.clone());
        }

        params.into_iter().nth(position)
    }

    pub(crate) fn parse_jsdoc_return_type_text(jsdoc: &str) -> Option<String> {
        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            let Some(rest) = line
                .strip_prefix("@returns")
                .or_else(|| line.strip_prefix("@return"))
            else {
                continue;
            };
            let rest = rest.trim();
            let (type_expr, _) = Self::parse_jsdoc_braced_type_and_name(rest)?;
            return Some(Self::normalize_jsdoc_type_text(type_expr, false));
        }
        None
    }

    pub(crate) fn jsdoc_return_type_text_for_node(&self, idx: NodeIndex) -> Option<String> {
        let jsdoc = self.function_like_jsdoc_for_node(idx)?;
        Self::parse_jsdoc_return_type_text(&jsdoc)
    }

    pub(crate) fn jsdoc_template_params_for_node(&self, idx: NodeIndex) -> Vec<String> {
        self.function_like_jsdoc_for_node(idx)
            .map(|jsdoc| Self::parse_jsdoc_template_params(&jsdoc))
            .unwrap_or_default()
    }

    pub(crate) fn emit_jsdoc_template_parameters(&mut self, type_params: &[String]) {
        if type_params.is_empty() {
            return;
        }

        self.write("<");
        for (i, param) in type_params.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.write(param);
        }
        self.write(">");
    }

    fn jsdoc_has_function_signature_tags(jsdoc: &str) -> bool {
        jsdoc.lines().any(|raw_line| {
            let line = raw_line.trim_start_matches('*').trim();
            line.starts_with("@param")
                || line.starts_with("@returns")
                || line.starts_with("@return")
                || line.starts_with("@template")
        })
    }

    pub(crate) fn emit_js_function_variable_declaration_if_possible(
        &mut self,
        decl_idx: NodeIndex,
        decl_name: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
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
        if self
            .leading_jsdoc_type_expr_for_pos(name_node.pos)
            .is_some()
        {
            return false;
        }
        let is_export_equals_root = self.is_js_export_equals_name(decl_name);

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

        let jsdoc = self.function_like_jsdoc_for_node(initializer);
        if !jsdoc
            .as_deref()
            .is_some_and(Self::jsdoc_has_function_signature_tags)
            && !is_export_equals_root
        {
            return false;
        }

        self.emit_pending_js_export_equals_for_name(decl_name);
        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("function ");
        self.emit_node(decl_name);

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
                .or_else(|| self.get_node_type_or_names(&[decl_idx, decl_name, initializer]));
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
            } else if func.body.is_some() && self.body_returns_void(func.body) {
                self.write(": void");
            }
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write(": void");
        }

        self.write(";");
        self.write_line();
        self.emit_js_namespace_export_aliases_for_name(decl_name);
        true
    }

    fn parse_jsdoc_callback_alias(jsdoc: &str) -> Option<(String, String)> {
        let mut name = None;
        let mut params = Vec::new();
        let mut return_type = None;

        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            if line.is_empty() {
                continue;
            }

            if let Some(rest) = line.strip_prefix("@callback") {
                let callback_name = rest.trim();
                if !callback_name.is_empty() {
                    name = Some(callback_name.to_string());
                }
                continue;
            }

            if let Some(rest) = line.strip_prefix("@param") {
                let rest = rest.trim();
                if rest.starts_with('{')
                    && let Some(end) = rest[1..].find('}')
                {
                    let type_expr = rest[1..1 + end].trim();
                    let param_name = rest[2 + end..]
                        .split_whitespace()
                        .next()
                        .filter(|name| !name.is_empty())
                        .unwrap_or("arg");
                    let (rest_param, base_type) =
                        if let Some(stripped) = type_expr.strip_prefix("...") {
                            (true, stripped.trim())
                        } else {
                            (false, type_expr)
                        };
                    let ts_type = if base_type == "*" {
                        "any".to_string()
                    } else if rest_param {
                        format!("{base_type}[]")
                    } else {
                        base_type.to_string()
                    };
                    if rest_param {
                        params.push(format!("...{param_name}: {ts_type}"));
                    } else {
                        params.push(format!("{param_name}: {ts_type}"));
                    }
                }
                continue;
            }

            if let Some(rest) = line
                .strip_prefix("@returns")
                .or_else(|| line.strip_prefix("@return"))
            {
                let rest = rest.trim();
                if rest.starts_with('{')
                    && let Some(end) = rest[1..].find('}')
                {
                    let type_expr = rest[1..1 + end].trim();
                    return_type = Some(if type_expr == "*" {
                        "any".to_string()
                    } else {
                        type_expr.to_string()
                    });
                }
            }
        }

        let name = name?;
        let return_type = return_type.unwrap_or_else(|| "any".to_string());
        Some((name, format!("({}) => {return_type}", params.join(", "))))
    }

    pub(crate) fn parse_jsdoc_template_params(jsdoc: &str) -> Vec<String> {
        let mut params = Vec::new();
        let mut seen = FxHashSet::default();

        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            let Some(rest) = line.strip_prefix("@template") else {
                continue;
            };

            for name in rest
                .split([',', ' ', '\t'])
                .map(str::trim)
                .filter(|name| !name.is_empty())
            {
                if seen.insert(name.to_string()) {
                    params.push(name.to_string());
                }
            }
        }

        params
    }

    fn parse_jsdoc_typedef_alias(jsdoc: &str) -> Option<(String, String)> {
        let normalized = Self::normalize_jsdoc_block(jsdoc);
        let tag_pos = normalized.find("@typedef")?;
        let rest = normalized[tag_pos + "@typedef".len()..].trim();
        let (type_expr, name_rest) = Self::parse_jsdoc_braced_type_and_name(rest)?;
        let name = name_rest
            .split_whitespace()
            .next()
            .filter(|name| !name.is_empty())?;
        if type_expr.is_empty() {
            return None;
        }
        Some((name.to_string(), type_expr.to_string()))
    }

    fn parse_jsdoc_braced_type_and_name(text: &str) -> Option<(&str, &str)> {
        let text = text.trim();
        if !text.starts_with('{') {
            return None;
        }

        let mut depth = 0usize;
        for (idx, ch) in text.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let ty = text[1..idx].trim();
                        let rest = text[idx + 1..].trim();
                        return Some((ty, rest));
                    }
                }
                _ => {}
            }
        }

        None
    }

    fn jsdoc_description_lines(jsdoc: &str) -> Vec<String> {
        let mut lines = Vec::new();
        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            if line.starts_with('@') {
                break;
            }
            if !line.is_empty() {
                lines.push(line.to_string());
            }
        }
        lines
    }

    fn jsdoc_has_property_tags(jsdoc: &str) -> bool {
        jsdoc.lines().any(|raw_line| {
            let line = raw_line.trim_start_matches('*').trim();
            line.starts_with("@property") || line.starts_with("@prop")
        })
    }

    fn parse_jsdoc_property_type_alias(jsdoc: &str) -> Option<(String, String)> {
        let (name, base_type) = Self::parse_jsdoc_typedef_alias(jsdoc)?;
        if name == "default" || !matches!(base_type.as_str(), "Object" | "object") {
            return None;
        }

        let mut properties = Vec::new();
        let mut current_property: Option<(String, bool, String, Vec<String>)> = None;

        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            if line.is_empty() {
                continue;
            }

            if let Some(rest) = line
                .strip_prefix("@property")
                .or_else(|| line.strip_prefix("@prop"))
            {
                if let Some(property) = current_property.take() {
                    properties.push(property);
                }

                let rest = rest.trim();
                let (type_expr, name_rest) = Self::parse_jsdoc_braced_type_and_name(rest)?;
                let mut parts = name_rest.split_whitespace();
                let property_name = parts.next()?.trim();
                if property_name.is_empty() {
                    return None;
                }

                let (property_name, optional) =
                    if property_name.starts_with('[') && property_name.ends_with(']') {
                        let trimmed = property_name
                            .trim_start_matches('[')
                            .trim_end_matches(']')
                            .trim_end_matches('=')
                            .to_string();
                        (trimmed, true)
                    } else {
                        (property_name.to_string(), false)
                    };

                let inline_description = parts.collect::<Vec<_>>().join(" ");
                let mut description_lines = Vec::new();
                if !inline_description.is_empty() {
                    description_lines.push(inline_description);
                }

                current_property = Some((
                    property_name,
                    optional,
                    Self::normalize_jsdoc_primitive_type_name(type_expr),
                    description_lines,
                ));
                continue;
            }

            if line.starts_with('@') {
                if let Some(property) = current_property.take() {
                    properties.push(property);
                }
                continue;
            }

            if let Some((_, _, _, description_lines)) = current_property.as_mut() {
                description_lines.push(line.to_string());
            }
        }

        if let Some(property) = current_property.take() {
            properties.push(property);
        }
        if properties.is_empty() {
            return None;
        }

        let mut type_text = String::from("{\n");
        for (property_name, optional, property_type, description_lines) in properties {
            if !description_lines.is_empty() {
                type_text.push_str("    /**\n");
                for line in description_lines {
                    type_text.push_str("     * ");
                    type_text.push_str(&line);
                    type_text.push('\n');
                }
                type_text.push_str("     */\n");
            }
            type_text.push_str("    ");
            type_text.push_str(&property_name);
            if optional {
                type_text.push('?');
            }
            type_text.push_str(": ");
            type_text.push_str(&property_type);
            type_text.push_str(";\n");
        }
        type_text.push('}');

        Some((name, type_text))
    }

    fn normalize_jsdoc_primitive_type_name(type_name: &str) -> String {
        match type_name.trim() {
            "String" => "string".to_string(),
            "Number" => "number".to_string(),
            "Boolean" => "boolean".to_string(),
            "Symbol" => "symbol".to_string(),
            "BigInt" => "bigint".to_string(),
            "Undefined" => "undefined".to_string(),
            "Null" => "null".to_string(),
            "Object" => "object".to_string(),
            other => other.to_string(),
        }
    }

    fn parse_jsdoc_type_alias_decl(jsdoc: &str) -> Option<JsdocTypeAliasDecl> {
        let type_params = Self::parse_jsdoc_template_params(jsdoc);
        let description_lines = Self::jsdoc_description_lines(jsdoc);

        if let Some((name, type_text)) = Self::parse_jsdoc_typedef_alias(jsdoc) {
            if name == "default" {
                return None;
            }
            if Self::jsdoc_has_property_tags(jsdoc) {
                let (name, type_text) = Self::parse_jsdoc_property_type_alias(jsdoc)?;
                return Some(JsdocTypeAliasDecl {
                    name,
                    type_params,
                    type_text,
                    description_lines: Vec::new(),
                    render_verbatim: true,
                });
            }
            return Some(JsdocTypeAliasDecl {
                name,
                type_params,
                type_text,
                description_lines: Vec::new(),
                render_verbatim: false,
            });
        }

        if let Some((name, type_text)) = Self::parse_jsdoc_callback_alias(jsdoc) {
            return Some(JsdocTypeAliasDecl {
                name,
                type_params,
                type_text,
                description_lines,
                render_verbatim: false,
            });
        }

        None
    }

    fn render_jsdoc_type_alias_decl(decl: &JsdocTypeAliasDecl, exported: bool) -> Option<String> {
        let mut source = String::new();
        if !decl.description_lines.is_empty() {
            source.push_str("/**\n");
            for line in &decl.description_lines {
                source.push_str(" * ");
                source.push_str(line);
                source.push('\n');
            }
            source.push_str(" */\n");
        }
        source.push_str(if exported { "export type " } else { "type " });
        source.push_str(&decl.name);
        if !decl.type_params.is_empty() {
            source.push('<');
            source.push_str(&decl.type_params.join(", "));
            source.push('>');
        }
        source.push_str(" = ");
        source.push_str(&decl.type_text);
        source.push_str(";\n");

        if decl.render_verbatim {
            return Some(source);
        }

        let mut parser = ParserState::new("jsdoc-alias.ts".to_string(), source);
        let root = parser.parse_source_file();
        let mut emitter = DeclarationEmitter::new(&parser.arena);
        let rendered = emitter.emit(root);
        if rendered.trim().is_empty() {
            None
        } else {
            Some(rendered)
        }
    }

    fn emit_rendered_jsdoc_type_alias(&mut self, decl: JsdocTypeAliasDecl, exported: bool) {
        if !self.emitted_jsdoc_type_aliases.insert(decl.name.clone()) {
            return;
        }
        let Some(rendered) = Self::render_jsdoc_type_alias_decl(&decl, exported) else {
            return;
        };
        self.write(&rendered);
        if exported {
            self.emitted_module_indicator = true;
        }
    }

    pub(crate) fn emit_leading_jsdoc_type_aliases_for_pos(&mut self, pos: u32) {
        if !self.source_is_js_file {
            return;
        }
        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(pos) {
            if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                self.emit_rendered_jsdoc_type_alias(decl, true);
            }
        }
    }

    pub(crate) fn emit_jsdoc_callback_type_aliases_for_variable_statement(
        &mut self,
        stmt_idx: NodeIndex,
        force_exported: bool,
    ) {
        if !self.source_is_js_file {
            return;
        }
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };

        let callback_chain = self.leading_jsdoc_comment_chain_for_pos(stmt_node.pos);
        if callback_chain.is_empty() {
            return;
        }

        let callback_aliases = callback_chain
            .iter()
            .filter_map(|jsdoc| Self::parse_jsdoc_callback_alias(jsdoc))
            .collect::<FxHashMap<_, _>>();
        if callback_aliases.is_empty() {
            return;
        }

        let has_export_modifier = self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);

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

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                let is_exported = force_exported
                    || has_export_modifier
                    || self.is_js_named_exported_name(decl.name);
                if !is_exported {
                    continue;
                }

                let Some(type_name) = self
                    .jsdoc_name_like_type_expr_for_pos(stmt_node.pos)
                    .or_else(|| self.jsdoc_name_like_type_expr_for_node(decl_idx))
                    .or_else(|| self.jsdoc_name_like_type_expr_for_node(decl.name))
                else {
                    continue;
                };

                let Some(type_text) = callback_aliases.get(&type_name) else {
                    continue;
                };
                if !self.emitted_jsdoc_type_aliases.insert(type_name.clone()) {
                    continue;
                }

                self.write_indent();
                self.write("export type ");
                self.write(&type_name);
                self.write(" = ");
                self.write(type_text);
                self.write(";");
                self.write_line();
            }
        }
    }

    pub(crate) fn emit_pending_jsdoc_callback_type_aliases(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if !self.source_is_js_file {
            return;
        }

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            match stmt_node.kind {
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    self.emit_jsdoc_callback_type_aliases_for_variable_statement(stmt_idx, false);
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    let Some(export) = self.arena.get_export_decl(stmt_node) else {
                        continue;
                    };
                    let Some(clause_node) = self.arena.get(export.export_clause) else {
                        continue;
                    };
                    if clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                        self.emit_jsdoc_callback_type_aliases_for_variable_statement(
                            export.export_clause,
                            true,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    pub(crate) fn emit_trailing_top_level_jsdoc_type_aliases(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if !self.source_is_js_file {
            return;
        }

        let Ok(eof_pos) = u32::try_from(source_file.text.len()) else {
            return;
        };

        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(eof_pos) {
            if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                self.emit_rendered_jsdoc_type_alias(decl, true);
            }
        }
    }

    pub(crate) fn emit_pending_top_level_jsdoc_type_aliases(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if !self.source_is_js_file || self.source_file_has_module_syntax(source_file) {
            return;
        }

        let mut decls = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            for jsdoc in self.leading_jsdoc_comment_chain_for_pos(stmt_node.pos) {
                if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                    decls.push(decl);
                }
            }
        }

        let Ok(eof_pos) = u32::try_from(source_file.text.len()) else {
            return;
        };
        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(eof_pos) {
            if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                decls.push(decl);
            }
        }

        for decl in decls {
            self.emit_rendered_jsdoc_type_alias(decl, false);
        }
    }

    pub(crate) fn collect_js_export_equals_names(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        if !self.source_file_is_js(source_file) {
            return names;
        }

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_ASSIGNMENT {
                continue;
            }
            let Some(assign) = self.arena.get_export_assignment(stmt_node) else {
                continue;
            };
            if !assign.is_export_equals {
                continue;
            }

            let Some(expr_node) = self.arena.get(assign.expression) else {
                continue;
            };
            if expr_node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let Some(ident) = self.arena.get_identifier(expr_node) else {
                continue;
            };
            names.insert(ident.escaped_text.clone());
        }

        for &stmt_idx in &source_file.statements.nodes {
            let Some(name) = self.js_commonjs_export_equals_name(stmt_idx) else {
                continue;
            };
            names.insert(name);
        }

        names
    }

    pub(crate) fn collect_js_namespace_export_aliases(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
        js_export_equals_names: &FxHashSet<String>,
    ) -> JsNamespaceExportAliases {
        let mut aliases = FxHashMap::default();
        if !self.source_file_is_js(source_file) {
            return aliases;
        }

        let commonjs_root = if js_export_equals_names.len() == 1 {
            js_export_equals_names.iter().next().cloned()
        } else {
            None
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some((root_name, export_name, local_name)) =
                self.js_namespace_export_alias_for_statement(stmt_idx, commonjs_root.as_deref())
            else {
                if let Some((root_name, member_name, _initializer, kind)) =
                    self.js_commonjs_expando_decl_for_statement(stmt_idx, js_export_equals_names)
                    && matches!(
                        kind,
                        JsCommonjsExpandoDeclKind::Function | JsCommonjsExpandoDeclKind::Value
                    )
                    && let Some(member_text) = self.get_identifier_text(member_name)
                {
                    Self::push_js_namespace_export_alias(
                        &mut aliases,
                        &root_name,
                        member_text.clone(),
                        member_text,
                    );
                }
                continue;
            };

            Self::push_js_namespace_export_alias(&mut aliases, &root_name, export_name, local_name);
        }

        if let Some(root_name) = commonjs_root.as_deref() {
            for alias_name in self.collect_top_level_jsdoc_alias_names(source_file) {
                Self::push_js_namespace_export_alias(
                    &mut aliases,
                    root_name,
                    alias_name.clone(),
                    alias_name,
                );
            }
        }

        aliases
    }

    /// Collect CJS export aliases for `exports.X = Y` / `module.exports.X = Y`.
    pub(crate) fn collect_js_cjs_export_aliases(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> (Vec<(String, String)>, FxHashSet<NodeIndex>) {
        let empty = (Vec::new(), FxHashSet::default());
        if !self.source_file_is_js(source_file) {
            return empty;
        }
        if !self.js_export_equals_names.is_empty() {
            return empty;
        }
        let export_targets = self.collect_js_named_export_targets(source_file);
        let mut alias_map: FxHashMap<String, (String, Vec<NodeIndex>)> = FxHashMap::default();
        for &stmt_idx in &source_file.statements.nodes {
            if let Some((export_name_idx, rhs_idx)) =
                self.js_commonjs_named_export_for_statement(stmt_idx)
            {
                let Some(export_name) = self.get_identifier_text(export_name_idx) else {
                    continue;
                };
                let rhs_idx = self
                    .arena
                    .skip_parenthesized_and_assertions_and_comma(rhs_idx);
                let local_name = self.get_identifier_text(rhs_idx);
                let entry = alias_map
                    .entry(export_name.clone())
                    .or_insert_with(|| (String::new(), Vec::new()));
                entry.1.push(stmt_idx);
                if let Some(ref ln) = local_name {
                    if *ln != export_name && export_targets.contains_key(ln) {
                        entry.0 = ln.clone();
                    }
                }
                continue;
            }
            if let Some((export_name, local_name, stmt)) =
                self.js_module_exports_property_alias(stmt_idx)
            {
                let entry = alias_map
                    .entry(export_name.clone())
                    .or_insert_with(|| (String::new(), Vec::new()));
                entry.1.push(stmt);
                if export_name != local_name && export_targets.contains_key(&local_name) {
                    entry.0 = local_name;
                }
            }
        }
        let mut aliases = Vec::new();
        let mut skipped = FxHashSet::default();
        let mut seen = FxHashSet::default();
        for (export_name, (local_name, stmts)) in &alias_map {
            if local_name.is_empty() {
                continue;
            }
            for &s in stmts {
                skipped.insert(s);
            }
            if seen.insert((export_name.clone(), local_name.clone())) {
                aliases.push((export_name.clone(), local_name.clone()));
            }
        }
        (aliases, skipped)
    }

    /// Parse `module.exports.X = Y` and return `(export_name, local_name, stmt_idx)`.
    fn js_module_exports_property_alias(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<(String, String, NodeIndex)> {
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
        let access = self.arena.get_access_expr(lhs_node)?;
        let export_name = self.get_identifier_text(access.name_or_argument)?;
        let receiver = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(access.expression);
        if !self.is_module_exports_reference(receiver) {
            return None;
        }
        let rhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        let local_name = self.get_identifier_text(rhs)?;
        Some((export_name, local_name, stmt_idx))
    }

    pub(crate) fn collect_js_commonjs_expando_declarations(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
        js_export_equals_names: &FxHashSet<String>,
    ) -> JsCommonjsExpandoDeclarations {
        let mut declarations = JsCommonjsExpandoDeclarations::default();
        if !self.source_file_is_js(source_file) || js_export_equals_names.is_empty() {
            return declarations;
        }

        for &stmt_idx in &source_file.statements.nodes {
            let Some((root_name, member_name, initializer, kind)) =
                self.js_commonjs_expando_decl_for_statement(stmt_idx, js_export_equals_names)
            else {
                continue;
            };

            match kind {
                JsCommonjsExpandoDeclKind::Function => {
                    declarations
                        .function_statements
                        .insert(stmt_idx, (member_name, initializer));
                }
                JsCommonjsExpandoDeclKind::Value => {
                    declarations
                        .value_statements
                        .insert(stmt_idx, (member_name, initializer));
                }
                JsCommonjsExpandoDeclKind::PrototypeMethod => {
                    let entry = declarations
                        .prototype_methods
                        .entry(root_name)
                        .or_insert_with(Vec::new);
                    if !entry.iter().any(|(existing_name, existing_initializer)| {
                        *existing_name == member_name && *existing_initializer == initializer
                    }) {
                        entry.push((member_name, initializer));
                    }
                }
            }
        }

        declarations
    }

    /// Collect `X.prototype.Y = expr` assignments for top-level variables that are
    /// NOT already handled by the CJS expando machinery.  tsc uses a "class-like
    /// heuristic": any variable whose name appears in a `Name.prototype.prop = ...`
    /// statement is emitted as `declare class Name { private constructor(); ... }`
    /// instead of `declare let Name: any;`.
    pub(crate) fn collect_js_class_like_prototype_members(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
        js_export_equals_names: &FxHashSet<String>,
    ) -> JsClassLikePrototypeMembers {
        let mut result = JsClassLikePrototypeMembers::default();
        if !self.source_file_is_js(source_file) {
            return result;
        }

        // First, collect all top-level variable names (let/var/const declarations).
        let mut top_level_var_names: FxHashSet<String> = FxHashSet::default();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }
            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                continue;
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
                for &decl_idx in &decl_list.declarations.nodes {
                    if let Some(decl_node) = self.arena.get(decl_idx)
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        && let Some(name) = self.get_identifier_text(decl.name)
                    {
                        // Skip names already handled by CJS expando
                        if !js_export_equals_names.contains(&name) {
                            top_level_var_names.insert(name);
                        }
                    }
                }
            }
        }

        if top_level_var_names.is_empty() {
            return result;
        }

        // Now scan for `X.prototype.Y = expr` expression statements.
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let expr_idx = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(expr_stmt.expression);
            let Some(expr_node) = self.arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.arena.get_binary_expr(expr_node) else {
                continue;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }

            // LHS must be `X.prototype.Y`
            let lhs = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(binary.left);
            let Some(lhs_node) = self.arena.get(lhs) else {
                continue;
            };
            if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                continue;
            }
            let Some(lhs_access) = self.arena.get_access_expr(lhs_node) else {
                continue;
            };
            let Some(_member_name) = self.get_identifier_text(lhs_access.name_or_argument) else {
                continue;
            };

            // Receiver must be `X.prototype` where X is a top-level variable
            let receiver = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(lhs_access.expression);
            let Some(receiver_node) = self.arena.get(receiver) else {
                continue;
            };
            if receiver_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                continue;
            }
            let Some(receiver_access) = self.arena.get_access_expr(receiver_node) else {
                continue;
            };
            if self
                .get_identifier_text(receiver_access.name_or_argument)
                .as_deref()
                != Some("prototype")
            {
                continue;
            }
            let Some(root_name) = self.get_identifier_text(receiver_access.expression) else {
                continue;
            };
            if !top_level_var_names.contains(&root_name) {
                continue;
            }

            let rhs = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(binary.right);
            let entry = result.members.entry(root_name).or_default();
            if !entry.iter().any(|(existing_name, existing_init)| {
                *existing_name == lhs_access.name_or_argument && *existing_init == rhs
            }) {
                entry.push((lhs_access.name_or_argument, rhs));
            }
            result.consumed_stmts.insert(stmt_idx);
        }

        result
    }

    pub(crate) fn collect_js_commonjs_named_exports(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> JsCommonjsNamedExports {
        let mut exported_names = FxHashSet::default();
        let mut function_statements = FxHashMap::default();
        let mut value_statements = FxHashMap::default();
        if !self.source_file_is_js(source_file) {
            return (exported_names, function_statements, value_statements);
        }

        let export_targets = self.collect_js_named_export_targets(source_file);

        for &stmt_idx in &source_file.statements.nodes {
            let Some((name_idx, initializer)) =
                self.js_supported_commonjs_named_export_for_statement(stmt_idx)
            else {
                continue;
            };
            let Some(export_name) = self.get_identifier_text(name_idx) else {
                continue;
            };

            if let Some(local_name) = self.get_identifier_text(initializer)
                && local_name == export_name
                && export_targets.contains_key(&local_name)
            {
                exported_names.insert(local_name);
                continue;
            }

            if self.arena.get(initializer).is_some_and(|init_node| {
                init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            }) {
                function_statements.insert(stmt_idx, (name_idx, initializer));
                continue;
            }

            value_statements.insert(stmt_idx, (name_idx, initializer));
        }

        (exported_names, function_statements, value_statements)
    }

    /// Collect named export names from `module.exports = { Name1, Name2 }` patterns.
    ///
    /// When a JS file has `module.exports = { Foo, Bar }` where the shorthand
    /// property names refer to top-level declarations, tsc treats those names
    /// as named exports (emitting `export class Foo ...` rather than
    /// `declare class Foo ...`).
    pub(crate) fn collect_js_module_exports_object_names(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> (FxHashSet<String>, FxHashSet<NodeIndex>) {
        let empty = (FxHashSet::default(), FxHashSet::default());
        if !self.source_file_is_js(source_file) {
            return empty;
        }

        let export_targets = self.collect_js_named_export_targets(source_file);
        let mut names = FxHashSet::default();
        let mut skipped_stmts = FxHashSet::default();

        for &stmt_idx in &source_file.statements.nodes {
            // Look for expression statements: `module.exports = { ... }`
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let expr_idx = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(expr_stmt.expression);
            let Some(expr_node) = self.arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.arena.get_binary_expr(expr_node) else {
                continue;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }
            if !self.is_module_exports_reference(binary.left) {
                continue;
            }

            let rhs = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(binary.right);
            let Some(rhs_node) = self.arena.get(rhs) else {
                continue;
            };
            if rhs_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }
            let Some(obj) = self.arena.get_literal_expr(rhs_node) else {
                continue;
            };

            let mut found_any = false;
            for &member_idx in &obj.elements.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                // Handle shorthand properties: `{ FancyError }` -> name is `FancyError`
                if member_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                    if let Some(data) = self.arena.get_shorthand_property(member_node) {
                        if let Some(name) = self.get_identifier_text(data.name) {
                            if export_targets.contains_key(&name) {
                                names.insert(name);
                                found_any = true;
                            }
                        }
                    }
                }
                // Handle property assignments: `{ FancyError: FancyError }`
                else if member_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                    if let Some(prop) = self.arena.get_property_assignment(member_node) {
                        if let Some(prop_name) = self.get_identifier_text(prop.name) {
                            if let Some(init_name) = self.get_identifier_text(prop.initializer) {
                                if prop_name == init_name && export_targets.contains_key(&prop_name)
                                {
                                    names.insert(prop_name);
                                    found_any = true;
                                }
                            }
                        }
                    }
                }
            }
            if found_any {
                skipped_stmts.insert(stmt_idx);
            }
        }

        (names, skipped_stmts)
    }

    pub(crate) fn js_supported_commonjs_named_export_for_statement(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<(NodeIndex, NodeIndex)> {
        let (name_idx, initializer) = self.js_commonjs_named_export_for_statement(stmt_idx)?;

        if let Some(export_name) = self.get_identifier_text(name_idx)
            && let Some(local_name) = self.get_identifier_text(initializer)
            && local_name == export_name
        {
            return Some((name_idx, initializer));
        }

        if self.js_commonjs_void_zero_export_init(initializer) {
            return None;
        }

        let init_node = self.arena.get(initializer)?;
        if init_node.kind == syntax_kind_ext::ARROW_FUNCTION
            || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return Some((name_idx, initializer));
        }

        if self.js_named_class_expression_matches_export(initializer, name_idx) {
            return Some((name_idx, initializer));
        }

        if self.js_commonjs_named_export_value_initializer_supported(initializer) {
            return Some((name_idx, initializer));
        }

        None
    }

    fn js_commonjs_named_export_value_initializer_supported(&self, initializer: NodeIndex) -> bool {
        self.js_synthetic_export_value_type_text(initializer)
            .is_some()
    }

    fn js_named_class_expression_matches_export(
        &self,
        initializer: NodeIndex,
        export_name_idx: NodeIndex,
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
        let Some(export_name) = self.get_identifier_text(export_name_idx) else {
            return false;
        };
        self.get_identifier_text(class.name)
            .is_some_and(|class_name| class_name == export_name)
    }

    fn js_commonjs_void_zero_export_init(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if self.is_void_expression(expr_node)
            || expr_node.kind == SyntaxKind::UndefinedKeyword as u16
        {
            return true;
        }
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return false;
        }
        let Some(binary) = self.arena.get_binary_expr(expr_node) else {
            return false;
        };
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return false;
        }
        let lhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.left);
        let Some(lhs_node) = self.arena.get(lhs) else {
            return false;
        };
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(lhs_access) = self.arena.get_access_expr(lhs_node) else {
            return false;
        };
        if !self.is_exports_identifier_reference(lhs_access.expression) {
            return false;
        }

        self.js_commonjs_void_zero_export_init(binary.right)
    }

    pub(crate) fn js_assigned_initializer_for_value_reference(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let target_text = self.nameable_constructor_expression_text(expr_idx)?;
        let source_file_idx = self.current_source_file_idx?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let expr_idx = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(expr_stmt.expression);
            let Some(expr_node) = self.arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.arena.get_binary_expr(expr_node) else {
                continue;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }

            let lhs = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(binary.left);
            if self.nameable_constructor_expression_text(lhs).as_deref() != Some(&target_text) {
                continue;
            }

            return Some(
                self.arena
                    .skip_parenthesized_and_assertions_and_comma(binary.right),
            );
        }

        None
    }

    pub(crate) fn collect_js_class_static_method_augmentations(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> JsStaticMethodAugmentations {
        let mut augmentations = JsStaticMethodAugmentations::default();
        if !self.source_file_is_js(source_file) {
            return augmentations;
        }

        let mut static_methods: FxHashMap<JsStaticMethodKey, JsStaticMethodInfo> =
            FxHashMap::default();
        for &stmt_idx in &source_file.statements.nodes {
            self.collect_js_static_class_methods_for_statement(stmt_idx, &mut static_methods);
        }

        let mut grouped: FxHashMap<JsStaticMethodKey, JsStaticMethodAugmentationEntry> =
            FxHashMap::default();
        for &stmt_idx in &source_file.statements.nodes {
            let Some((class_name, method_name, member_name, initializer)) =
                self.js_class_static_method_augmentation_for_statement(stmt_idx)
            else {
                continue;
            };
            let Some(&(class_idx, method_idx, class_is_exported)) =
                static_methods.get(&(class_name.clone(), method_name.clone()))
            else {
                continue;
            };

            let entry = grouped.entry((class_name, method_name)).or_insert_with(|| {
                (
                    stmt_idx,
                    class_idx,
                    method_idx,
                    class_is_exported,
                    Vec::new(),
                )
            });
            if !entry.4.iter().any(|(existing_name, existing_initializer)| {
                *existing_name == member_name && *existing_initializer == initializer
            }) {
                entry.4.push((member_name, initializer));
            }
        }

        for (_key, (first_stmt_idx, class_idx, method_idx, class_is_exported, properties)) in
            grouped
        {
            augmentations.augmented_method_nodes.insert(method_idx);
            augmentations.statements.insert(
                first_stmt_idx,
                JsStaticMethodAugmentationGroup {
                    class_idx,
                    method_idx,
                    class_is_exported,
                    properties,
                },
            );
        }

        for &stmt_idx in &source_file.statements.nodes {
            if self
                .js_class_static_method_augmentation_for_statement(stmt_idx)
                .is_some()
                && !augmentations.statements.contains_key(&stmt_idx)
            {
                augmentations.skipped_statements.insert(stmt_idx);
            }
        }

        augmentations
    }

    pub(crate) fn collect_js_grouped_reexports(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> (FxHashMap<NodeIndex, Vec<NodeIndex>>, FxHashSet<NodeIndex>) {
        let mut groups = FxHashMap::default();
        let mut skipped = FxHashSet::default();
        if !self.source_file_is_js(source_file) {
            return (groups, skipped);
        }

        let statements = &source_file.statements.nodes;
        let mut i = 0;
        while i < statements.len() {
            let stmt_idx = statements[i];
            let Some((module_specifier, is_type_only)) = self.groupable_js_reexport_info(stmt_idx)
            else {
                i += 1;
                continue;
            };
            if is_type_only {
                i += 1;
                continue;
            }

            let mut group = vec![stmt_idx];
            let mut j = i + 1;
            while j < statements.len() {
                let candidate_idx = statements[j];
                let Some((candidate_module, candidate_type_only)) =
                    self.groupable_js_reexport_info(candidate_idx)
                else {
                    break;
                };
                if candidate_type_only || candidate_module != module_specifier {
                    break;
                }
                group.push(candidate_idx);
                j += 1;
            }

            if group.len() > 1 {
                for &member in group.iter().skip(1) {
                    skipped.insert(member);
                }
                groups.insert(stmt_idx, group);
            }

            i = j.max(i + 1);
        }

        (groups, skipped)
    }

    pub(crate) fn collect_js_namespace_object_statements(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> FxHashSet<NodeIndex> {
        let mut deferred = FxHashSet::default();
        if !self.source_file_is_js(source_file) {
            return deferred;
        }

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT
                || self.statement_has_attached_jsdoc(source_file, stmt_node)
            {
                continue;
            }

            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                continue;
            };
            if self
                .arena
                .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
            {
                continue;
            }

            let mut candidate_decl: Option<NodeIndex> = None;
            let mut initializer: Option<NodeIndex> = None;
            let mut valid = true;

            for &decl_list_idx in &var_stmt.declarations.nodes {
                let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                    valid = false;
                    break;
                };
                let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                    valid = false;
                    break;
                };
                if decl_list.declarations.nodes.len() != 1 {
                    valid = false;
                    break;
                }
                let decl_idx = decl_list.declarations.nodes[0];
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    valid = false;
                    break;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    valid = false;
                    break;
                };
                let Some(name_node) = self.arena.get(decl.name) else {
                    valid = false;
                    break;
                };
                if name_node.kind != SyntaxKind::Identifier as u16 || !decl.initializer.is_some() {
                    valid = false;
                    break;
                }
                let Some(init_node) = self.arena.get(decl.initializer) else {
                    valid = false;
                    break;
                };
                if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                    valid = false;
                    break;
                }
                let Some(object) = self.arena.get_literal_expr(init_node) else {
                    valid = false;
                    break;
                };
                if object.elements.nodes.is_empty() {
                    valid = false;
                    break;
                }

                for &member_idx in &object.elements.nodes {
                    let Some(member_node) = self.arena.get(member_idx) else {
                        valid = false;
                        break;
                    };
                    match member_node.kind {
                        k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                            let Some(prop) = self.arena.get_property_assignment(member_node) else {
                                valid = false;
                                break;
                            };
                            if self
                                .arena
                                .get(prop.name)
                                .is_none_or(|name| name.kind != SyntaxKind::Identifier as u16)
                            {
                                valid = false;
                                break;
                            }
                            let Some(_) = self.arena.get(prop.initializer) else {
                                valid = false;
                                break;
                            };
                            if !self
                                .js_namespace_object_member_initializer_supported(prop.initializer)
                            {
                                valid = false;
                                break;
                            }
                        }
                        k if k == syntax_kind_ext::METHOD_DECLARATION => {
                            let Some(method) = self.arena.get_method_decl(member_node) else {
                                valid = false;
                                break;
                            };
                            if self
                                .arena
                                .get(method.name)
                                .is_none_or(|name| name.kind != SyntaxKind::Identifier as u16)
                            {
                                valid = false;
                                break;
                            }
                        }
                        _ => {
                            valid = false;
                            break;
                        }
                    }
                }

                if !valid {
                    break;
                }

                candidate_decl = Some(decl.name);
                initializer = Some(decl.initializer);
            }

            if valid && candidate_decl.is_some() && initializer.is_some() {
                deferred.insert(stmt_idx);
            }
        }

        deferred
    }

    pub(crate) fn js_namespace_object_member_initializer_supported(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };

        match init_node.kind {
            k if k == syntax_kind_ext::ARROW_FUNCTION => true,
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => true,
            k if k == SyntaxKind::StringLiteral as u16 => true,
            k if k == SyntaxKind::NumericLiteral as u16 => true,
            k if k == SyntaxKind::BigIntLiteral as u16 => true,
            k if k == SyntaxKind::TrueKeyword as u16 => true,
            k if k == SyntaxKind::FalseKeyword as u16 => true,
            k if k == SyntaxKind::NullKeyword as u16 => true,
            k if k == SyntaxKind::UndefinedKeyword as u16 => true,
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                self.is_negative_literal(init_node)
            }
            _ => false,
        }
    }

    fn groupable_js_reexport_info(&self, export_idx: NodeIndex) -> Option<(String, bool)> {
        let export_node = self.arena.get(export_idx)?;
        if export_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
            return None;
        }
        let export = self.arena.get_export_decl(export_node)?;
        let clause_node = self.arena.get(export.export_clause)?;
        if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS || export.module_specifier.is_none() {
            return None;
        }
        let module_node = self.arena.get(export.module_specifier)?;
        let module_specifier = self.arena.get_literal(module_node)?.text.clone();
        Some((module_specifier, export.is_type_only))
    }

    pub(crate) fn is_js_named_exported_name(&self, name_idx: NodeIndex) -> bool {
        if !self.source_is_js_file || self.js_named_export_names.is_empty() {
            return false;
        }

        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        let Some(name_ident) = self.arena.get_identifier(name_node) else {
            return false;
        };
        self.js_named_export_names
            .contains(&name_ident.escaped_text)
    }

    pub(crate) const fn should_emit_declare_keyword(&self, is_exported: bool) -> bool {
        !(self.inside_declare_namespace
            || self.source_is_declaration_file
            || (self.source_is_js_file && is_exported))
    }

    /// Whether an exported declaration should emit its `export` keyword.
    ///
    /// Inside a `declare namespace` (including non-ambient namespaces that
    /// gain `declare` in the .d.ts output), `export` is only emitted when
    /// the namespace body has a mix of exported and non-exported members
    /// (i.e., a "scope marker" is present).  Outside a `declare namespace`,
    /// `export` is always emitted.
    pub(crate) const fn should_emit_export_keyword(&self) -> bool {
        !self.inside_declare_namespace || self.ambient_module_has_scope_marker
    }

    pub(crate) fn is_js_export_equals_name(&self, name_idx: NodeIndex) -> bool {
        if !self.source_is_js_file || self.js_export_equals_names.is_empty() {
            return false;
        }

        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        let Some(name_ident) = self.arena.get_identifier(name_node) else {
            return false;
        };
        self.js_export_equals_names
            .contains(&name_ident.escaped_text)
    }

    pub(crate) fn emit_js_namespace_export_aliases_for_name(&mut self, name_idx: NodeIndex) {
        if !self.source_is_js_file || self.js_namespace_export_aliases.is_empty() {
            return;
        }

        let Some(name) = self.get_identifier_text(name_idx) else {
            return;
        };
        let Some(aliases) = self.js_namespace_export_aliases.get(&name).cloned() else {
            return;
        };
        if aliases.is_empty() {
            return;
        }

        self.write_indent();
        if self.should_emit_declare_keyword(false) {
            self.write("declare ");
        }
        self.write("namespace ");
        self.emit_node(name_idx);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        let mut plain_exports = Vec::new();
        let mut renamed_exports = Vec::new();
        for (export_name, local_name) in aliases {
            if export_name == local_name {
                plain_exports.push(local_name);
            } else {
                renamed_exports.push((export_name, local_name));
            }
        }

        if !plain_exports.is_empty() {
            self.write_indent();
            self.write("export { ");
            for (idx, local_name) in plain_exports.iter().enumerate() {
                if idx > 0 {
                    self.write(", ");
                }
                self.write(local_name);
            }
            self.write(" };");
            self.write_line();
        }

        for (export_name, local_name) in renamed_exports {
            self.write_indent();
            self.write("export { ");
            self.write(&local_name);
            if export_name != local_name {
                self.write(" as ");
                self.write(&export_name);
            }
            self.write(" };");
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(crate) fn emit_pending_js_export_equals_for_name(&mut self, name_idx: NodeIndex) {
        if !self.is_js_export_equals_name(name_idx) {
            return;
        }

        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };
        let Some(name_ident) = self.arena.get_identifier(name_node) else {
            return;
        };
        if !self
            .emitted_js_export_equals_names
            .insert(name_ident.escaped_text.clone())
        {
            return;
        }

        self.write_indent();
        self.write("export = ");
        self.emit_node(name_idx);
        self.write(";");
        self.write_line();
        self.emitted_scope_marker = true;
        self.emitted_module_indicator = true;
    }

    fn js_commonjs_export_equals_name(&self, stmt_idx: NodeIndex) -> Option<String> {
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
        if !self.is_module_exports_reference(binary.left) {
            return None;
        }

        self.get_identifier_text(
            self.arena
                .skip_parenthesized_and_assertions_and_comma(binary.right),
        )
    }

    fn js_commonjs_expando_decl_for_statement(
        &self,
        stmt_idx: NodeIndex,
        js_export_equals_names: &FxHashSet<String>,
    ) -> Option<(String, NodeIndex, NodeIndex, JsCommonjsExpandoDeclKind)> {
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
        self.get_identifier_text(lhs_access.name_or_argument)?;

        let rhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        let rhs_node = self.arena.get(rhs)?;
        let receiver = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(lhs_access.expression);
        let (root_name, is_prototype) =
            self.js_commonjs_expando_receiver(receiver, js_export_equals_names)?;

        if is_prototype {
            if rhs_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || rhs_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                return Some((
                    root_name,
                    lhs_access.name_or_argument,
                    rhs,
                    JsCommonjsExpandoDeclKind::PrototypeMethod,
                ));
            }
            return None;
        }

        if rhs_node.kind == syntax_kind_ext::ARROW_FUNCTION
            || rhs_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return Some((
                root_name,
                lhs_access.name_or_argument,
                rhs,
                JsCommonjsExpandoDeclKind::Function,
            ));
        }

        if self.js_namespace_object_member_initializer_supported(rhs) {
            return Some((
                root_name,
                lhs_access.name_or_argument,
                rhs,
                JsCommonjsExpandoDeclKind::Value,
            ));
        }

        None
    }

    fn js_commonjs_named_export_for_statement(
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
        let receiver = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(lhs_access.expression);
        if !self.is_exports_identifier_reference(receiver) {
            return None;
        }
        if self
            .arena
            .get(lhs_access.name_or_argument)
            .is_none_or(|name| name.kind != SyntaxKind::Identifier as u16)
        {
            return None;
        }

        let rhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        Some((lhs_access.name_or_argument, rhs))
    }

    fn js_namespace_export_alias_for_statement(
        &self,
        stmt_idx: NodeIndex,
        commonjs_root: Option<&str>,
    ) -> Option<(String, String, String)> {
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

        let local_name = self.get_identifier_text(
            self.arena
                .skip_parenthesized_and_assertions_and_comma(binary.right),
        )?;
        let (root_name, export_name) =
            self.js_namespace_export_alias_target(binary.left, commonjs_root)?;
        Some((root_name, export_name, local_name))
    }

    fn js_namespace_export_alias_target(
        &self,
        lhs: NodeIndex,
        commonjs_root: Option<&str>,
    ) -> Option<(String, String)> {
        let lhs = self.arena.skip_parenthesized_and_assertions_and_comma(lhs);
        let lhs_node = self.arena.get(lhs)?;
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(lhs_node)?;
        let export_name = self.get_identifier_text(access.name_or_argument)?;

        let receiver_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(access.expression);
        if let Some(receiver_name) = self.get_identifier_text(receiver_idx)
            && receiver_name != "exports"
            && receiver_name != "module"
        {
            return Some((receiver_name, export_name));
        }

        if self.is_module_exports_reference(receiver_idx)
            && let Some(root_name) = commonjs_root
        {
            return Some((root_name.to_string(), export_name));
        }

        None
    }

    fn js_commonjs_expando_receiver(
        &self,
        receiver_idx: NodeIndex,
        js_export_equals_names: &FxHashSet<String>,
    ) -> Option<(String, bool)> {
        let receiver_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(receiver_idx);

        if let Some(root_name) = self.get_identifier_text(receiver_idx)
            && js_export_equals_names.contains(&root_name)
        {
            return Some((root_name, false));
        }

        let receiver_node = self.arena.get(receiver_idx)?;
        if receiver_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let receiver_access = self.arena.get_access_expr(receiver_node)?;
        if self
            .get_identifier_text(receiver_access.name_or_argument)
            .as_deref()
            != Some("prototype")
        {
            return None;
        }

        let root_name = self.get_identifier_text(receiver_access.expression)?;
        if !js_export_equals_names.contains(&root_name) {
            return None;
        }

        Some((root_name, true))
    }

    fn js_class_static_method_augmentation_for_statement(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<(String, String, NodeIndex, NodeIndex)> {
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
        let prop_name = lhs_access.name_or_argument;
        self.get_identifier_text(prop_name)?;

        let receiver = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(lhs_access.expression);
        let receiver_node = self.arena.get(receiver)?;
        if receiver_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let receiver_access = self.arena.get_access_expr(receiver_node)?;

        let class_name = self.get_identifier_text(receiver_access.expression)?;
        let method_name = self.get_identifier_text(receiver_access.name_or_argument)?;
        let initializer = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        let init_node = self.arena.get(initializer)?;
        let is_supported = init_node.kind == syntax_kind_ext::ARROW_FUNCTION
            || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || self.js_namespace_object_member_initializer_supported(initializer);
        if !is_supported {
            return None;
        }

        Some((class_name, method_name, prop_name, initializer))
    }

    fn collect_js_static_class_methods_for_statement(
        &self,
        stmt_idx: NodeIndex,
        methods: &mut FxHashMap<JsStaticMethodKey, JsStaticMethodInfo>,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                let Some(class) = self.arena.get_class(stmt_node) else {
                    return;
                };
                let class_is_exported = self
                    .arena
                    .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                    || self.is_js_named_exported_name(class.name);
                self.collect_js_static_class_methods(stmt_idx, class, class_is_exported, methods);
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                let Some(export) = self.arena.get_export_decl(stmt_node) else {
                    return;
                };
                let Some(clause_node) = self.arena.get(export.export_clause) else {
                    return;
                };
                if clause_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                    return;
                }
                let Some(class) = self.arena.get_class(clause_node) else {
                    return;
                };
                self.collect_js_static_class_methods(export.export_clause, class, true, methods);
            }
            _ => {}
        }
    }

    fn collect_js_static_class_methods(
        &self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
        class_is_exported: bool,
        methods: &mut FxHashMap<JsStaticMethodKey, JsStaticMethodInfo>,
    ) {
        let Some(class_name) = self.get_identifier_text(class.name) else {
            return;
        };

        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::METHOD_DECLARATION {
                continue;
            }
            let Some(method) = self.arena.get_method_decl(member_node) else {
                continue;
            };
            if !self
                .arena
                .has_modifier(&method.modifiers, SyntaxKind::StaticKeyword)
            {
                continue;
            }
            let Some(method_name) = self.get_identifier_text(method.name) else {
                continue;
            };
            methods.insert(
                (class_name.clone(), method_name),
                (class_idx, member_idx, class_is_exported),
            );
        }
    }

    fn is_module_exports_reference(&self, idx: NodeIndex) -> bool {
        let idx = self.arena.skip_parenthesized_and_assertions_and_comma(idx);
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(node) else {
            return false;
        };

        self.get_identifier_text(access.expression).as_deref() == Some("module")
            && self.get_identifier_text(access.name_or_argument).as_deref() == Some("exports")
    }

    fn is_exports_identifier_reference(&self, idx: NodeIndex) -> bool {
        self.get_identifier_text(idx).as_deref() == Some("exports")
    }

    pub(crate) fn collect_top_level_jsdoc_alias_names(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> Vec<String> {
        let mut names = Vec::new();
        let mut seen = FxHashSet::default();
        if !self.source_file_is_js(source_file) {
            return names;
        }

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            for jsdoc in self.leading_jsdoc_comment_chain_for_pos(stmt_node.pos) {
                if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc)
                    && seen.insert(decl.name.clone())
                {
                    names.push(decl.name);
                }
            }
        }

        let Ok(eof_pos) = u32::try_from(source_file.text.len()) else {
            return names;
        };
        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(eof_pos) {
            if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc)
                && seen.insert(decl.name.clone())
            {
                names.push(decl.name);
            }
        }

        names
    }

    fn push_js_namespace_export_alias(
        aliases: &mut JsNamespaceExportAliases,
        root_name: &str,
        export_name: String,
        local_name: String,
    ) {
        let entry = aliases.entry(root_name.to_string()).or_default();
        if !entry.iter().any(|(existing_export, existing_local)| {
            existing_export == &export_name && existing_local == &local_name
        }) {
            entry.push((export_name, local_name));
        }
    }

    pub(crate) fn statement_has_effective_export(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if self.stmt_has_export_modifier(stmt_node) {
            return true;
        }
        if !self.source_is_js_file {
            return false;
        }

        match stmt_node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                .arena
                .get_function(stmt_node)
                .is_some_and(|func| self.is_js_named_exported_name(func.name)),
            k if k == syntax_kind_ext::CLASS_DECLARATION => self
                .arena
                .get_class(stmt_node)
                .is_some_and(|class| self.is_js_named_exported_name(class.name)),
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.arena.get_variable(stmt_node).is_some_and(|var_stmt| {
                    var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
                        self.arena
                            .get(decl_list_idx)
                            .and_then(|decl_list_node| self.arena.get_variable(decl_list_node))
                            .is_some_and(|decl_list| {
                                decl_list.declarations.nodes.iter().any(|&decl_idx| {
                                    self.arena
                                        .get(decl_idx)
                                        .and_then(|decl_node| {
                                            self.arena.get_variable_declaration(decl_node)
                                        })
                                        .is_some_and(|decl| {
                                            self.is_js_named_exported_name(decl.name)
                                        })
                                })
                            })
                    })
                })
            }
            _ => false,
        }
    }

    /// Return true when declarations are filtered to public API members.
    pub(crate) const fn public_api_filter_enabled(&self) -> bool {
        self.emit_public_api_only && self.public_api_scope_depth == 0
    }

    /// Return true if a top-level declaration should be emitted when API filtering is enabled.
    pub(crate) fn should_emit_public_api_member(&self, modifiers: &Option<NodeList>) -> bool {
        if !self.public_api_filter_enabled() {
            return true;
        }

        self.arena
            .has_modifier(modifiers, SyntaxKind::ExportKeyword)
    }

    /// Return true if a module declaration should be emitted when API filtering is enabled.
    pub(crate) fn should_emit_public_api_module(
        &self,
        is_exported: bool,
        name_idx: NodeIndex,
    ) -> bool {
        if !self.public_api_filter_enabled() {
            return true;
        }

        // Module augmentations (`declare module "foo"` and `declare global`)
        // must always be emitted regardless of the public API filter.
        // They augment external module or global scope and are always part
        // of the declaration output.
        if let Some(name_node) = self.arena.get(name_idx) {
            // String-literal module name: `declare module "some-module" { ... }`
            if name_node.kind == SyntaxKind::StringLiteral as u16 {
                return true;
            }
            // `declare global { ... }` — the parser represents `global` as
            // an Identifier node with escaped_text "global".
            if let Some(ident) = self.arena.get_identifier(name_node)
                && ident.escaped_text == "global"
            {
                return true;
            }
        }

        is_exported || self.should_emit_public_api_dependency(name_idx)
    }

    /// Return true if a declaration should be skipped because it's a
    /// non-exported value/type inside a non-ambient namespace.
    /// Namespace and import-alias declarations are NOT filtered here
    /// (they may be needed for name resolution and are filtered recursively).
    ///
    /// When `decl_idx` is provided, non-exported members that are referenced
    /// by the exported API surface (via `used_symbols`) are preserved —
    /// TSC emits these so that exported declarations can reference them.
    pub(crate) fn should_skip_ns_internal_member(
        &self,
        modifiers: &Option<NodeList>,
        decl_idx: Option<NodeIndex>,
    ) -> bool {
        if !self.inside_non_ambient_namespace {
            return false;
        }
        // If the member has an `export` keyword, keep it
        if self
            .arena
            .has_modifier(modifiers, SyntaxKind::ExportKeyword)
        {
            return false;
        }
        // If the member is referenced by the exported API surface, keep it
        if let Some(idx) = decl_idx
            && self.is_ns_member_used_by_exports(idx)
        {
            return false;
        }
        // Non-exported member inside non-ambient namespace: skip
        true
    }

    /// Check if a non-exported namespace member's symbol appears in `used_symbols`.
    /// Unlike `should_emit_public_api_dependency`, this does NOT short-circuit
    /// when `public_api_filter_enabled()` is false — it always checks the usage set.
    pub(crate) fn is_ns_member_used_by_exports(&self, decl_idx: NodeIndex) -> bool {
        let Some(used) = &self.used_symbols else {
            return false;
        };
        let Some(binder) = self.binder else {
            return false;
        };
        // Direct node-to-symbol lookup (works when decl_idx is the declaration node)
        if let Some(&sym_id) = binder.node_symbols.get(&decl_idx.0) {
            if used.contains_key(&sym_id) {
                return true;
            }
        }
        // Fallback: resolve by identifier text via scope tables.
        // For import-equals declarations, extract the name from the import clause
        // since the declaration node itself is not an identifier.
        let Some(name_node) = self.arena.get(decl_idx) else {
            return false;
        };
        let import_clause_idx = if let Some(import_eq) = self.arena.get_import_decl(name_node) {
            // Also check the import clause's node_symbols (the name identifier
            // may have a different SymbolId than the declaration node).
            if let Some(&clause_sym) = binder.node_symbols.get(&import_eq.import_clause.0) {
                if used.contains_key(&clause_sym) {
                    return true;
                }
            }
            Some(import_eq.import_clause)
        } else {
            None
        };
        let name_text = if let Some(ident) = self.arena.get_identifier(name_node) {
            Some(ident.escaped_text.clone())
        } else if let Some(clause_idx) = import_clause_idx {
            self.arena
                .get(clause_idx)
                .and_then(|n| self.arena.get_identifier(n))
                .map(|ident| ident.escaped_text.clone())
        } else {
            None
        };
        let Some(name) = name_text else {
            return false;
        };
        // Check all scope tables (not just file_locals) since the symbol
        // may be in a namespace scope
        for scope in &binder.scopes {
            if let Some(sym_id) = scope.table.get(&name)
                && used.contains_key(&sym_id)
            {
                return true;
            }
        }
        if let Some(sym_id) = binder.file_locals.get(&name) {
            return used.contains_key(&sym_id);
        }
        false
    }

    /// Check if a statement node has the `export` keyword modifier.
    pub(crate) fn stmt_has_export_modifier(
        &self,
        stmt_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let k = stmt_node.kind;
        if k == syntax_kind_ext::FUNCTION_DECLARATION {
            if let Some(func) = self.arena.get_function(stmt_node) {
                return self
                    .arena
                    .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword);
            }
        } else if k == syntax_kind_ext::CLASS_DECLARATION {
            if let Some(class) = self.arena.get_class(stmt_node) {
                return self
                    .arena
                    .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword);
            }
        } else if k == syntax_kind_ext::INTERFACE_DECLARATION {
            if let Some(iface) = self.arena.get_interface(stmt_node) {
                return self
                    .arena
                    .has_modifier(&iface.modifiers, SyntaxKind::ExportKeyword);
            }
        } else if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            if let Some(alias) = self.arena.get_type_alias(stmt_node) {
                return self
                    .arena
                    .has_modifier(&alias.modifiers, SyntaxKind::ExportKeyword);
            }
        } else if k == syntax_kind_ext::ENUM_DECLARATION {
            if let Some(enum_data) = self.arena.get_enum(stmt_node) {
                return self
                    .arena
                    .has_modifier(&enum_data.modifiers, SyntaxKind::ExportKeyword);
            }
        } else if k == syntax_kind_ext::VARIABLE_STATEMENT {
            if let Some(var_stmt) = self.arena.get_variable(stmt_node) {
                return self
                    .arena
                    .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);
            }
        } else if k == syntax_kind_ext::MODULE_DECLARATION
            && let Some(module) = self.arena.get_module(stmt_node)
        {
            return self
                .arena
                .has_modifier(&module.modifiers, SyntaxKind::ExportKeyword);
        }
        false
    }

    /// Check whether the leading comments before `pos` contain `@internal`.
    /// Used when `--stripInternal` is enabled to elide internal declarations.
    pub(crate) fn has_internal_annotation(&self, pos: u32) -> bool {
        if !self.strip_internal {
            return false;
        }
        let Some(ref text) = self.source_file_text else {
            return false;
        };
        // Search backwards from `pos` through any comments that precede this node.
        // The `@internal` annotation can appear in `/** @internal */` or `// @internal`.
        for comment in self.all_comments.iter().rev() {
            if comment.end > pos {
                continue;
            }
            // Only consider comments immediately before this position
            // (allow only whitespace between comment end and pos)
            let between = &text[comment.end as usize..pos as usize];
            if !between
                .bytes()
                .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            {
                break;
            }
            let ct = &text[comment.pos as usize..comment.end as usize];
            if ct.contains("@internal") {
                return true;
            }
            // If this comment doesn't have @internal, don't look further back
            break;
        }
        false
    }

    /// Return true when a declaration symbol is referenced by the exported API surface.
    pub(crate) fn should_emit_public_api_dependency(&self, name_idx: NodeIndex) -> bool {
        if !self.public_api_filter_enabled() {
            return true;
        }

        let Some(used) = &self.used_symbols else {
            // Usage analysis unavailable: preserve dependent declarations
            // rather than over-pruning and producing unresolved names.
            return true;
        };
        let Some(binder) = self.binder else {
            return true;
        };
        let Some(&sym_id) = binder.node_symbols.get(&name_idx.0) else {
            // Some declaration name nodes are not mapped directly; fall back
            // to root-scope lookup by identifier text.
            let Some(name_node) = self.arena.get(name_idx) else {
                return false;
            };
            let Some(name_ident) = self.arena.get_identifier(name_node) else {
                return false;
            };
            // Check file_locals first (matches UsageAnalyzer's lookup path)
            if let Some(sym_id) = binder.file_locals.get(&name_ident.escaped_text) {
                return used.contains_key(&sym_id);
            }
            // Fall back to root scope table
            let Some(root_scope) = binder.scopes.first() else {
                return false;
            };
            let Some(scope_sym_id) = root_scope.table.get(&name_ident.escaped_text) else {
                return false;
            };
            return used.contains_key(&scope_sym_id);
        };

        used.contains_key(&sym_id)
    }

    /// Check if the target of a namespace-path import-equals resolves to a type-level entity.
    ///
    /// For `import x = a.b.c`, resolves the rightmost identifier (`c`) to a symbol and checks
    /// if it has type-level flags (class, interface, enum, namespace, type alias, function).
    /// When the target is a type/namespace entity, the emitted .d.ts type annotations may
    /// reference the alias name, so the import must be preserved. When the target is a plain
    /// variable, the type resolves to a primitive/literal and the alias is not needed.
    pub(crate) fn import_alias_targets_type_entity(&self, module_spec_idx: NodeIndex) -> bool {
        let Some(binder) = self.binder else {
            return true; // conservative: preserve when we can't resolve
        };

        // Try node_symbols on the full module specifier and rightmost name.
        // The binder may map these nodes to the resolved target symbol.
        let rightmost_idx = self.get_rightmost_name(module_spec_idx);
        for &idx in &[module_spec_idx, rightmost_idx] {
            if let Some(&sym_id) = binder.node_symbols.get(&idx.0)
                && let Some(symbol) = binder.symbols.get(sym_id)
            {
                return !self.symbol_is_value_only(symbol);
            }
        }

        // Fall back to name-based lookup on the rightmost identifier.
        let Some(name_node) = self.arena.get(rightmost_idx) else {
            return true; // conservative
        };
        let Some(name_ident) = self.arena.get_identifier(name_node) else {
            return true; // conservative
        };
        let name = &name_ident.escaped_text;

        // Search all resolution paths for a non-ALIAS symbol.
        // The import's own ALIAS symbol shares the same name, so skip it
        // to find the actual target entity.
        for scope in &binder.scopes {
            if let Some(sym_id) = scope.table.get(name)
                && let Some(symbol) = binder.symbols.get(sym_id)
                && !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
            {
                return !self.symbol_is_value_only(symbol);
            }
        }
        if let Some(sym_id) = binder.file_locals.get(name)
            && let Some(symbol) = binder.symbols.get(sym_id)
            && !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
        {
            return !self.symbol_is_value_only(symbol);
        }

        // All symbols found were aliases — can't determine target.
        // Preserve conservatively.
        true
    }

    /// Get the rightmost identifier from a qualified name or property access.
    fn get_rightmost_name(&self, idx: NodeIndex) -> NodeIndex {
        let Some(node) = self.arena.get(idx) else {
            return idx;
        };
        if let Some(qn) = self.arena.get_qualified_name(node) {
            return self.get_rightmost_name(qn.right);
        }
        if let Some(access) = self.arena.get_access_expr(node) {
            return self.get_rightmost_name(access.name_or_argument);
        }
        idx
    }

    /// Check if a symbol is value-only (plain variable, no type/namespace/class flags).
    /// Value-only entities resolve to primitive types in .d.ts and don't need aliases.
    const fn symbol_is_value_only(&self, symbol: &tsz_binder::Symbol) -> bool {
        const VALUE_ONLY_FLAGS: u32 = tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE
            | tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE
            | tsz_binder::symbol_flags::PROPERTY;
        const NON_VALUE_ONLY_FLAGS: u32 = tsz_binder::symbol_flags::CLASS
            | tsz_binder::symbol_flags::INTERFACE
            | tsz_binder::symbol_flags::ENUM
            | tsz_binder::symbol_flags::TYPE_ALIAS
            | tsz_binder::symbol_flags::VALUE_MODULE
            | tsz_binder::symbol_flags::NAMESPACE_MODULE
            | tsz_binder::symbol_flags::FUNCTION
            | tsz_binder::symbol_flags::METHOD
            | tsz_binder::symbol_flags::ENUM_MEMBER;
        let flags = symbol.flags;
        (flags & VALUE_ONLY_FLAGS) != 0 && (flags & NON_VALUE_ONLY_FLAGS) == 0
    }

    /// Get the function/method name as a string for overload tracking
    pub(crate) fn get_function_name(&self, func_idx: NodeIndex) -> Option<String> {
        let func_node = self.arena.get(func_idx)?;

        // Try to get as function first
        let name_node = if let Some(func) = self.arena.get_function(func_node) {
            self.arena.get(func.name)?
        // Try to get as method
        } else if let Some(method) = self.arena.get_method_decl(func_node) {
            self.arena.get(method.name)?
        } else {
            return None;
        };

        // Extract identifier names directly
        if name_node.kind == SyntaxKind::Identifier as u16 {
            let ident = self.arena.get_identifier(name_node)?;
            Some(ident.escaped_text.clone())
        } else {
            // For computed property names and other non-identifier names,
            // use the source text span as a key for overload tracking
            self.get_source_slice(name_node.pos, name_node.end)
        }
    }

    /// Check if an import specifier should be emitted based on usage analysis.
    ///
    /// Returns true if:
    /// - No usage tracking is enabled (`used_symbols` is None)
    /// - The specifier's symbol is in the `used_symbols` set
    pub(crate) fn should_emit_import_specifier(&self, specifier_idx: NodeIndex) -> bool {
        // If no usage tracking, emit everything
        let Some(used) = &self.used_symbols else {
            return true;
        };

        // If no binder, we can't check symbols - emit conservatively
        let Some(binder) = &self.binder else {
            return true;
        };

        // Get the specifier node to extract its name
        let Some(spec_node) = self.arena.get(specifier_idx) else {
            return true;
        };

        // Only ImportSpecifier/ExportSpecifier nodes have symbols (on their name field)
        // For other node types, emit conservatively
        if spec_node.kind != tsz_parser::parser::syntax_kind_ext::IMPORT_SPECIFIER
            && spec_node.kind != tsz_parser::parser::syntax_kind_ext::EXPORT_SPECIFIER
        {
            return true;
        }

        let Some(specifier) = self.arena.get_specifier(spec_node) else {
            return true;
        };

        self.imported_name_is_used(binder, used, specifier.name)
    }

    fn imported_name_is_used(
        &self,
        binder: &BinderState,
        used: &rustc_hash::FxHashMap<SymbolId, super::usage_analyzer::UsageKind>,
        name_idx: NodeIndex,
    ) -> bool {
        let sym_id = if let Some(&sym_id) = binder.node_symbols.get(&name_idx.0) {
            sym_id
        } else {
            let Some(name_node) = self.arena.get(name_idx) else {
                return true;
            };
            let Some(ident) = self.arena.get_identifier(name_node) else {
                return true;
            };
            let Some(sym_id) = binder.file_locals.get(&ident.escaped_text) else {
                return true;
            };
            sym_id
        };

        self.import_symbol_is_used(binder, used, sym_id)
    }

    fn import_symbol_is_used(
        &self,
        binder: &BinderState,
        used: &rustc_hash::FxHashMap<SymbolId, super::usage_analyzer::UsageKind>,
        sym_id: SymbolId,
    ) -> bool {
        if used.contains_key(&sym_id) {
            return true;
        }

        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };
        let Some(import_module) = symbol.import_module.as_deref() else {
            return false;
        };
        let local_name = symbol.escaped_name.as_str();
        let import_name = symbol
            .import_name
            .as_deref()
            .unwrap_or(&symbol.escaped_name);

        if local_name != import_name {
            return false;
        }

        used.iter().any(|(&used_sym_id, _)| {
            let Some(used_symbol) = binder.symbols.get(used_sym_id) else {
                return false;
            };

            if used_symbol.escaped_name != import_name
                && used_symbol.import_name.as_deref() != Some(import_name)
            {
                return false;
            }

            used_symbol.import_module.as_deref() == Some(import_module)
                || self.resolve_symbol_module_path(used_sym_id).as_deref() == Some(import_module)
        })
    }

    pub(crate) fn can_reference_local_import_alias_by_name(&self, sym_id: SymbolId) -> bool {
        let (Some(binder), Some(used)) = (self.binder, self.used_symbols.as_ref()) else {
            return true;
        };

        self.import_symbol_is_used(binder, used, sym_id)
    }

    /// Count how many import specifiers in an `ImportClause` should be emitted.
    ///
    /// Returns (`default_count`, `named_count`) where:
    /// - `default_count`: 1 if default import is used, 0 otherwise
    /// - `named_count`: number of used named import specifiers
    pub(crate) fn count_used_imports(
        &self,
        import: &tsz_parser::parser::node::ImportDeclData,
    ) -> (usize, usize) {
        let mut default_count = 0;
        let mut named_count = 0;

        if let Some(used) = &self.used_symbols
            && let Some(binder) = &self.binder
        {
            // Check default import
            if import.import_clause.is_some()
                && let Some(clause_node) = self.arena.get(import.import_clause)
                && let Some(clause) = self.arena.get_import_clause(clause_node)
            {
                if clause.name.is_some() && self.imported_name_is_used(binder, used, clause.name) {
                    default_count = 1;
                }

                // Count named imports
                if clause.named_bindings.is_some()
                    && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                    && let Some(bindings) = self.arena.get_named_imports(bindings_node)
                {
                    if bindings.name.is_some() && bindings.elements.nodes.is_empty() {
                        if self.imported_name_is_used(binder, used, bindings.name) {
                            named_count = 1;
                        }
                    } else {
                        for &spec_idx in &bindings.elements.nodes {
                            if let Some(spec_node) = self.arena.get(spec_idx)
                                && let Some(specifier) = self.arena.get_specifier(spec_node)
                                && self.imported_name_is_used(binder, used, specifier.name)
                            {
                                named_count += 1;
                            }
                        }
                    }
                }
            }
        } else {
            // No usage tracking available (e.g., --noCheck --noLib mode).
            // In this mode, tsc would have type info to decide which imports are needed,
            // but we don't. Apply conservative heuristics:
            // - Type-only imports: keep (likely needed for type references)
            // - Named imports with specifiers: keep (may reference types)
            // - Namespace imports (import * as ns): skip (almost always value-level)
            // - Empty imports (import {}): skip
            if import.import_clause.is_some()
                && let Some(clause_node) = self.arena.get(import.import_clause)
                && let Some(clause) = self.arena.get_import_clause(clause_node)
            {
                // Type-only imports are likely needed for type references
                let is_type_only = clause.is_type_only;

                // Default import - keep for type-only, skip otherwise without tracking
                default_count = if is_type_only {
                    usize::from(clause.name.is_some())
                } else {
                    0
                };

                // Named bindings: check if there are actually any specifiers
                if clause.named_bindings.is_some() {
                    if let Some(bindings_node) = self.arena.get(clause.named_bindings)
                        && let Some(bindings) = self.arena.get_named_imports(bindings_node)
                    {
                        if bindings.name.is_some() && bindings.elements.nodes.is_empty() {
                            // Namespace import (import * as ns): skip in fallback mode
                            // These are almost exclusively for value-level code (ns.method())
                            // and rarely needed in .d.ts output
                            named_count = 0;
                        } else if is_type_only {
                            // Type-only named imports - keep all
                            named_count = bindings.elements.nodes.len();
                        } else {
                            // Regular named imports - keep (may be type references)
                            named_count = bindings.elements.nodes.len();
                        }
                    } else {
                        named_count = if is_type_only { 1 } else { 0 };
                    }
                }
            } else {
                // No import clause - side-effect import handled elsewhere
                default_count = 0;
                named_count = 0;
            }
        }

        (default_count, named_count)
    }

    /// Phase 4: Prepare import aliases before emitting anything.
    ///
    /// This detects name collisions and generates aliases for conflicting imports.
    pub(crate) fn prepare_import_aliases(&mut self, root_idx: NodeIndex) {
        // 1. Collect all top-level local declarations into reserved_names
        self.collect_local_declarations(root_idx);

        // 2. Process required_imports (String-based)
        // We clone keys to avoid borrow checker issues during iteration
        let modules: Vec<String> = self.required_imports.keys().cloned().collect();
        for module in modules {
            // Collect names into a separate vector to release the borrow
            let names: Vec<String> = self
                .required_imports
                .get(&module)
                .map(|v| v.to_vec())
                .unwrap_or_default();
            for name in names {
                self.resolve_import_name(&module, &name);
            }
        }

        // 3. Process foreign_symbols (SymbolId-based) - skip for now
        // This requires grouping by module which needs arena_to_path mapping
    }

    /// Collect local top-level names into `reserved_names`.
    pub(crate) fn collect_local_declarations(&mut self, root_idx: NodeIndex) {
        let Some(root_node) = self.arena.get(root_idx) else {
            return;
        };
        let Some(source_file) = self.arena.get_source_file(root_node) else {
            return;
        };

        // If we have a binder, use it to get top-level symbols
        if let Some(binder) = self.binder {
            // Get the root scope (scopes is a Vec, not a HashMap)
            if let Some(root_scope) = binder.scopes.first() {
                // Iterate through all symbols in root scope table
                for (name, _sym_id) in root_scope.table.iter() {
                    self.reserved_names.insert(name.clone());
                }
            }
        } else {
            // Fallback: Walk AST statements for top-level declarations
            for &stmt_idx in &source_file.statements.nodes {
                if stmt_idx.is_none() {
                    continue;
                }
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    continue;
                };

                let kind = stmt_node.kind;
                // Collect names from various declaration types
                if kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
                    || kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION
                    || kind == tsz_parser::parser::syntax_kind_ext::INTERFACE_DECLARATION
                    || kind == tsz_parser::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || kind == tsz_parser::parser::syntax_kind_ext::ENUM_DECLARATION
                {
                    // Try to get the name
                    if let Some(name) = self.extract_declaration_name(stmt_idx) {
                        self.reserved_names.insert(name);
                    }
                }
            }
        }
    }

    /// Extract the name from a declaration node.
    pub(crate) fn extract_declaration_name(&self, decl_idx: NodeIndex) -> Option<String> {
        let decl_node = self.arena.get(decl_idx)?;

        // Try identifier first
        if let Some(ident) = self.arena.get_identifier(decl_node) {
            return Some(ident.escaped_text.clone());
        }

        // For class/function/interface, the name is in a specific field
        if let Some(func) = self.arena.get_function(decl_node)
            && let Some(name_node) = self.arena.get(func.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }
        if let Some(class) = self.arena.get_class(decl_node)
            && let Some(name_node) = self.arena.get(class.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }
        if let Some(iface) = self.arena.get_interface(decl_node)
            && let Some(name_node) = self.arena.get(iface.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }
        if let Some(alias) = self.arena.get_type_alias(decl_node)
            && let Some(name_node) = self.arena.get(alias.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }
        if let Some(enum_data) = self.arena.get_enum(decl_node)
            && let Some(name_node) = self.arena.get(enum_data.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }

        None
    }

    /// Resolve name for string imports, generating alias if needed.
    pub(crate) fn resolve_import_name(&mut self, module: &str, name: &str) {
        if self.reserved_names.contains(name) {
            // Collision! Generate alias
            let alias = self.generate_unique_name(name);
            self.import_string_aliases
                .insert((module.to_string(), name.to_string()), alias.clone());
            self.reserved_names.insert(alias);
        } else {
            // No collision, reserve the name
            self.reserved_names.insert(name.to_string());
        }
    }

    /// Generate unique name (e.g., "`TypeA_1`").
    pub(crate) fn generate_unique_name(&self, base: &str) -> String {
        let mut i = 1;
        loop {
            let candidate = format!("{base}_{i}");
            if !self.reserved_names.contains(&candidate) {
                return candidate;
            }
            i += 1;
        }
    }

    pub(crate) fn reset_writer(&mut self) {
        self.writer = SourceWriter::with_capacity(4096);
        self.pending_source_pos = None;
        self.public_api_scope_depth = 0;
        if let Some(state) = &self.source_map_state {
            self.writer.enable_source_map(state.output_name.clone());
            let content = self.source_map_text.map(std::string::ToString::to_string);
            self.writer.add_source(state.source_name.clone(), content);
        }
    }

    pub(crate) fn emit_leading_jsdoc_comments(&mut self, pos: u32) {
        if self.remove_comments {
            return;
        }
        let Some(ref text) = self.source_file_text else {
            return;
        };
        let text = text.clone();
        let bytes = text.as_bytes();
        let mut actual_start = pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }
        let actual_start = actual_start as u32;
        while self.comment_emit_idx < self.all_comments.len() {
            let c_pos = self.all_comments[self.comment_emit_idx].pos;
            let c_end = self.all_comments[self.comment_emit_idx].end;
            if c_end > actual_start {
                break;
            }
            let ct = &text[c_pos as usize..c_end as usize];
            // Skip empty block comments like /**/
            if ct.starts_with("/**") && ct != "/**/" {
                let si = {
                    let cp = c_pos as usize;
                    let mut ls = cp;
                    if ls > 0 {
                        let mut i = ls;
                        while i > 0 {
                            i -= 1;
                            if bytes[i] == b'\n' || bytes[i] == b'\r' {
                                ls = i + 1;
                                break;
                            }
                            if i == 0 {
                                ls = 0;
                            }
                        }
                    }
                    let mut w = 0usize;
                    for &b in &bytes[ls..cp] {
                        if b == b' ' {
                            w += 1;
                        } else if b == b'\t' {
                            w = (w / 4 + 1) * 4;
                        } else {
                            break;
                        }
                    }
                    w
                };
                // Check if the next comment is a JSDoc comment on the same
                // source line — if so, emit a space instead of a newline to
                // keep consecutive JSDoc comments on one line (matching tsc).
                let next_idx = self.comment_emit_idx + 1;
                let next_on_same_line = next_idx < self.all_comments.len() && {
                    let n_pos = self.all_comments[next_idx].pos;
                    let n_end = self.all_comments[next_idx].end;
                    n_end <= actual_start && {
                        let between = &text[c_end as usize..n_pos as usize];
                        let next_ct = &text[n_pos as usize..n_end as usize];
                        next_ct.starts_with("/**") && next_ct != "/**/" && !between.contains('\n')
                    }
                };
                self.write_indent();
                if ct.contains('\n') {
                    let mut first = true;
                    for line in ct.split('\n') {
                        if first {
                            self.write(line.trim_end());
                            first = false;
                        } else {
                            self.write_line();
                            let line_bytes = line.as_bytes();
                            // Count leading whitespace visual width
                            // (tabs expand to next multiple of 4)
                            let mut line_ws = 0usize;
                            let mut char_ws = 0usize;
                            for &b in line_bytes.iter() {
                                if b == b' ' {
                                    line_ws += 1;
                                    char_ws += 1;
                                } else if b == b'\t' {
                                    line_ws = (line_ws / 4 + 1) * 4;
                                    char_ws += 1;
                                } else {
                                    break;
                                }
                            }
                            let content = line[char_ws..].trim_end();
                            // Compute output indent: apply the relative offset
                            // from the source /** indent to the output indent.
                            let output_indent = (self.indent_level as usize) * 4;
                            let out_ws = if line_ws >= si {
                                output_indent + (line_ws - si)
                            } else {
                                output_indent.saturating_sub(si - line_ws)
                            };
                            for _ in 0..out_ws {
                                self.write_raw(" ");
                            }
                            self.write(content);
                        }
                    }
                } else {
                    self.write(ct);
                }
                if next_on_same_line {
                    self.write(" ");
                } else {
                    self.write_line();
                }
            }
            self.comment_emit_idx += 1;
        }
    }

    /// Emit all inline block comments (both `/*...*/` and `/**...*/`) that appear
    /// before `name_pos`. Used for variable declarations where tsc preserves
    /// comments between the keyword and the variable name (e.g. `var /*4*/ point`).
    pub(crate) fn emit_inline_block_comments(&mut self, name_pos: u32) {
        if self.remove_comments {
            return;
        }
        let Some(ref text) = self.source_file_text else {
            return;
        };
        let text = text.clone();
        let bytes = text.as_bytes();
        let mut actual_start = name_pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }
        let actual_start = actual_start as u32;
        while self.comment_emit_idx < self.all_comments.len() {
            let comment = &self.all_comments[self.comment_emit_idx];
            if comment.end > actual_start {
                break;
            }
            let ct = &text[comment.pos as usize..comment.end as usize];
            if ct.starts_with("/*") {
                self.write(ct);
                self.write(" ");
            }
            self.comment_emit_idx += 1;
        }
    }

    pub(crate) fn emit_inline_parameter_comment(&mut self, param_pos: u32) {
        if self.remove_comments {
            return;
        }
        let Some(ref text) = self.source_file_text else {
            return;
        };
        let text = text.clone();
        let bytes = text.as_bytes();
        let mut actual_start = param_pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }
        let actual_start = actual_start as u32;
        while self.comment_emit_idx < self.all_comments.len() {
            let comment = &self.all_comments[self.comment_emit_idx];
            if comment.end > actual_start {
                break;
            }
            let c_pos = comment.pos as usize;
            let c_end = comment.end as usize;
            let ct = &text[c_pos..c_end];
            if ct.starts_with("/*") {
                // Determine if this is a "leading" comment (before a parameter name)
                // or a "trailing" comment (after a parameter's type annotation).
                // Leading: preceded by `(`, `,`, `[`, `<`, whitespace, or another comment.
                // Trailing: preceded by identifier chars, `)`, type annotation, etc.
                let is_leading = {
                    let mut p = c_pos;
                    let mut leading = true;
                    while p > 0 {
                        p -= 1;
                        match bytes[p] {
                            b' ' | b'\t' | b'\r' | b'\n' => continue,
                            b'(' | b',' | b'[' | b'<' => break,
                            b'/' if p > 0 && bytes[p - 1] == b'*' => break, // end of another comment
                            _ => {
                                leading = false;
                                break;
                            }
                        }
                    }
                    leading
                };

                if is_leading {
                    // Check if the comment was on a new line in the source.
                    let has_newline = {
                        let mut pos = c_pos;
                        let mut found = false;
                        while pos > 0 {
                            pos -= 1;
                            match bytes[pos] {
                                b'\n' => {
                                    found = true;
                                    break;
                                }
                                b' ' | b'\t' | b'\r' => continue,
                                _ => break,
                            }
                        }
                        found
                    };
                    if has_newline {
                        self.write_line();
                        self.write_indent();
                    }
                    self.write(ct);
                    if has_newline {
                        self.write_line();
                        self.write_indent();
                    } else {
                        self.write(" ");
                    }
                }
            }
            self.comment_emit_idx += 1;
        }
    }

    /// Check if there is a trailing block comment on the same source line as `node_end`,
    /// and if so, emit it (space-separated) before the caller emits a newline.
    /// Returns true if a trailing comment was emitted.
    pub(crate) fn emit_trailing_comment(&mut self, node_end: u32) -> bool {
        if self.remove_comments {
            return false;
        }
        let Some(ref text) = self.source_file_text else {
            return false;
        };
        let text = text.clone();
        let bytes = text.as_bytes();
        if self.comment_emit_idx >= self.all_comments.len() {
            return false;
        }
        let c_pos = self.all_comments[self.comment_emit_idx].pos;
        let c_end = self.all_comments[self.comment_emit_idx].end;
        // The comment must start after the node end
        if c_pos < node_end {
            return false;
        }
        let ct = &text[c_pos as usize..c_end as usize];
        // Only handle block comments (/* ... */), not line comments
        if !ct.starts_with("/*") {
            return false;
        }
        // Check that there's no newline between node_end and the comment start
        let between = &bytes[node_end as usize..c_pos as usize];
        if between.contains(&b'\n') || between.contains(&b'\r') {
            return false;
        }
        // Emit as trailing comment
        self.write(" ");
        self.write(ct);
        self.comment_emit_idx += 1;
        true
    }

    /// Advance the comment index past any comments that end before `pos`,
    /// without emitting them. Used to skip comments that belong to a parent
    /// context (e.g. comments between `:` and the type's opening paren).
    pub(crate) fn skip_comments_before(&mut self, pos: u32) {
        while self.comment_emit_idx < self.all_comments.len() {
            if self.all_comments[self.comment_emit_idx].end <= pos {
                self.comment_emit_idx += 1;
            } else {
                break;
            }
        }
    }

    pub(crate) fn skip_comments_in_node(&mut self, pos: u32, end: u32) {
        let ae = self.find_node_code_end(pos, end);
        while self.comment_emit_idx < self.all_comments.len() {
            if self.all_comments[self.comment_emit_idx].pos < ae {
                self.comment_emit_idx += 1;
            } else {
                break;
            }
        }
    }

    fn find_node_code_end(&self, pos: u32, end: u32) -> u32 {
        let Some(ref text) = self.source_file_text else {
            return end;
        };
        let bytes = text.as_bytes();
        let s = pos as usize;
        let e = std::cmp::min(end as usize, bytes.len());
        if s >= e {
            return end;
        }
        let mut d: i32 = 0;
        let mut lt: Option<usize> = None;
        let mut i = s;
        while i < e {
            match bytes[i] {
                b'{' => {
                    d += 1;
                    i += 1;
                }
                b'}' => {
                    d -= 1;
                    if d == 0 {
                        lt = Some(i + 1);
                    }
                    i += 1;
                }
                b';' => {
                    if d == 0 {
                        lt = Some(i + 1);
                    }
                    i += 1;
                }
                b'\'' | b'"' | b'`' => {
                    let q = bytes[i];
                    i += 1;
                    while i < e {
                        if bytes[i] == b'\\' {
                            i += 2;
                        } else if bytes[i] == q {
                            i += 1;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                }
                b'/' if i + 1 < e && bytes[i + 1] == b'/' => {
                    i += 2;
                    while i < e && bytes[i] != b'\n' && bytes[i] != b'\r' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < e && bytes[i + 1] == b'*' => {
                    i += 2;
                    while i + 1 < e {
                        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }
        lt.map_or(end, |x| x as u32)
    }

    pub(crate) fn queue_source_mapping(&mut self, node: &Node) {
        if !self.writer.has_source_map() {
            self.pending_source_pos = None;
            return;
        }

        let Some(text) = self.source_map_text else {
            self.pending_source_pos = None;
            return;
        };

        self.pending_source_pos = Some(source_position_from_offset(text, node.pos));
    }

    pub(crate) const fn take_pending_source_pos(&mut self) -> Option<SourcePosition> {
        self.pending_source_pos.take()
    }

    /// Returns the quote character used for a string literal in the original source.
    /// Falls back to double quote if source text is unavailable.
    pub(crate) fn original_quote_char(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> &'static str {
        if let Some(text) = self.source_file_text.as_ref() {
            let pos = node.pos as usize;
            if pos < text.len() {
                let ch = text.as_bytes()[pos];
                if ch == b'\'' {
                    return "'";
                }
            }
        }
        "\""
    }

    pub(crate) fn get_source_slice(&self, start: u32, end: u32) -> Option<String> {
        let text = self.source_file_text.as_ref()?;
        let start = start as usize;
        let end = end as usize;
        if start > end || end > text.len() {
            return None;
        }

        let slice = text[start..end].trim().to_string();
        if slice.is_empty() { None } else { Some(slice) }
    }

    /// Like `get_source_slice` but also strips a trailing `;` if present.
    /// Use this when extracting type/value text from source that will be
    /// embedded in a statement where the caller adds its own `;`.
    pub(crate) fn get_source_slice_no_semi(&self, start: u32, end: u32) -> Option<String> {
        let mut s = self.get_source_slice(start, end)?;
        if s.ends_with(';') {
            s.pop();
            let trimmed = s.trim_end().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        } else {
            Some(s)
        }
    }

    pub(crate) fn write_raw(&mut self, s: &str) {
        self.writer.write(s);
    }

    pub(crate) fn write(&mut self, s: &str) {
        if let Some(source_pos) = self.take_pending_source_pos() {
            self.writer.write_node(s, source_pos);
        } else {
            self.writer.write(s);
        }
    }

    pub(crate) fn write_line(&mut self) {
        self.writer.write_line();
    }

    pub(crate) fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.write_raw("    ");
        }
    }

    pub(crate) const fn increase_indent(&mut self) {
        self.indent_level += 1;
    }

    pub(crate) const fn decrease_indent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    /// Get the type of a node from the type cache, if available.
    pub(crate) fn get_node_type(&self, node_id: NodeIndex) -> Option<tsz_solver::types::TypeId> {
        if let (Some(cache), _) = (&self.type_cache, &self.type_interner) {
            cache.node_types.get(&node_id.0).copied()
        } else {
            None
        }
    }

    /// Try to find type for a function by looking up both the declaration node and name node.
    /// The binder may map the function declaration node rather than the name identifier,
    /// so we try both.
    pub(crate) fn get_type_via_symbol_for_func(
        &self,
        func_idx: NodeIndex,
        name_node: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let cache = self.type_cache.as_ref()?;
        let binder = self.binder?;
        // Try the name node first, then the function declaration node itself
        let symbol_id = binder
            .get_node_symbol(name_node)
            .or_else(|| binder.get_node_symbol(func_idx))?;
        cache.symbol_types.get(&symbol_id).copied()
    }

    pub(crate) fn get_type_via_symbol(
        &self,
        node_id: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let binder = self.binder?;
        let symbol_id = binder.get_node_symbol(node_id)?;
        let symbol = binder.symbols.get(symbol_id)?;
        symbol
            .declarations
            .iter()
            .copied()
            .find_map(|decl_idx| self.get_node_type_or_names(&[decl_idx]))
    }

    pub(crate) fn infer_fallback_type_text(&self, node_id: NodeIndex) -> Option<String> {
        self.infer_fallback_type_text_at(node_id, self.indent_level)
    }

    fn infer_fallback_type_text_at(&self, node_id: NodeIndex, depth: u32) -> Option<String> {
        if !node_id.is_some() {
            return None;
        }

        let node = self.arena.get(node_id)?;
        match node.kind {
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
            k if k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16 =>
            {
                Some("any".to_string())
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                self.preferred_expression_type_text(node_id)
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.infer_object_literal_type_text_at(node_id, depth)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => self
                .preferred_expression_type_text(node_id)
                .or_else(|| Some("any[]".to_string())),
            k if k == syntax_kind_ext::BINARY_EXPRESSION => self
                .infer_arithmetic_binary_type_text(node_id, depth)
                .or_else(|| {
                    self.get_node_type(node_id)
                        .map(|type_id| self.print_type_id(type_id))
                }),
            _ => self
                .get_node_type(node_id)
                .map(|type_id| self.print_type_id(type_id)),
        }
    }

    /// Infer the type of an arithmetic binary expression for declaration emit.
    /// For numeric operators (`+`, `-`, `*`, `/`, `%`, `**`, bitwise), if both
    /// operands resolve to `number`, the result is `number`.
    /// For `+` specifically, if either operand is `string`, the result is `string`.
    fn infer_arithmetic_binary_type_text(&self, node_id: NodeIndex, depth: u32) -> Option<String> {
        if depth > 8 {
            return None;
        }
        let node = self.arena.get(node_id)?;
        let binary = self.arena.get_binary_expr(node)?;
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

        if !is_numeric_op && !is_plus {
            return None;
        }

        // Purely numeric operators always produce number
        if is_numeric_op {
            return Some("number".to_string());
        }

        // For `+`, resolve both operands
        let left_type = self.infer_operand_type_text(binary.left, depth + 1)?;
        let right_type = self.infer_operand_type_text(binary.right, depth + 1)?;

        if left_type == "string" || right_type == "string" {
            Some("string".to_string())
        } else if left_type == "number" && right_type == "number" {
            Some("number".to_string())
        } else {
            None
        }
    }

    /// Resolve the primitive type of an operand for arithmetic type inference.
    fn infer_operand_type_text(&self, node_id: NodeIndex, depth: u32) -> Option<String> {
        // Try preferred expression first (finds declared types)
        if let Some(text) = self.preferred_expression_type_text(node_id) {
            return Some(text);
        }
        // Then try structural fallback
        self.infer_fallback_type_text_at(node_id, depth)
    }

    pub(crate) fn preferred_expression_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        if let Some(asserted_type_text) = self.explicit_asserted_type_text(expr_idx) {
            return Some(asserted_type_text);
        }

        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
            {
                self.reference_declared_type_annotation_text(expr_idx)
                    .or_else(|| self.undefined_identifier_type_text(expr_idx))
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                self.call_expression_declared_return_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                self.nameable_new_expression_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.array_literal_expression_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.short_circuit_expression_type_text(expr_idx)
            }
            _ => None,
        }
    }

    fn explicit_asserted_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let mut current = expr_idx;

        for _ in 0..100 {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = self.arena.get_unary_expr_ex(node)
            {
                current = unary.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::CommaToken as u16
            {
                current = binary.right;
                continue;
            }

            let assertion = self.arena.get_type_assertion(node)?;
            let asserted_type = self.arena.get(assertion.type_node)?;
            if asserted_type.kind == SyntaxKind::ConstKeyword as u16 {
                return None;
            }
            return self.emit_type_node_text(assertion.type_node);
        }

        None
    }

    fn skip_parenthesized_expression(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = expr_idx;
        loop {
            let node = self.arena.get(current)?;
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return Some(current);
            }
            current = self.arena.get_unary_expr_ex(node)?.expression;
        }
    }

    fn arena_source_file<'arena>(
        &self,
        arena: &'arena tsz_parser::parser::node::NodeArena,
    ) -> Option<&'arena tsz_parser::parser::node::SourceFileData> {
        arena
            .nodes
            .iter()
            .rev()
            .find_map(|node| arena.get_source_file(node))
    }

    fn source_slice_from_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        node_idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(node_idx)?;
        let source_file = self.arena_source_file(arena)?;
        let text = source_file.text.as_ref();
        let start = usize::try_from(node.pos).ok()?;
        let end = usize::try_from(node.end).ok()?;
        text.get(start..end).map(str::to_string)
    }

    pub(crate) fn rescued_asserts_parameter_type_text(
        &self,
        param_idx: NodeIndex,
    ) -> Option<String> {
        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        let type_node = self.arena.get(param.type_annotation)?;
        let type_ref = self.arena.get_type_ref(type_node)?;
        if type_ref.type_arguments.is_some() {
            return None;
        }
        let type_name = self.arena.get(type_ref.type_name)?;
        let ident = self.arena.get_identifier(type_name)?;
        if ident.escaped_text != "asserts" {
            return None;
        }

        let rescued = self.scan_asserts_parameter_type_text(type_node.pos)?;
        let normalized = rescued.split_whitespace().collect::<Vec<_>>().join(" ");
        (normalized != "asserts").then_some(normalized)
    }

    fn scan_asserts_parameter_type_text(&self, start: u32) -> Option<String> {
        let text = self.source_file_text.as_deref()?;
        let bytes = text.as_bytes();
        let start = usize::try_from(start).ok()?;
        if start >= bytes.len() {
            return None;
        }

        let mut i = start;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut angle_depth = 0usize;

        while i < bytes.len() {
            match bytes[i] {
                b'(' => paren_depth += 1,
                b')' => {
                    if paren_depth == 0
                        && bracket_depth == 0
                        && brace_depth == 0
                        && angle_depth == 0
                    {
                        break;
                    }
                    paren_depth = paren_depth.saturating_sub(1);
                }
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                b',' | b'=' | b';'
                    if paren_depth == 0
                        && bracket_depth == 0
                        && brace_depth == 0
                        && angle_depth == 0 =>
                {
                    break;
                }
                _ => {}
            }
            i += 1;
        }

        let rescued = text.get(start..i)?.trim().to_string();
        (!rescued.is_empty()).then_some(rescued)
    }

    fn undefined_identifier_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        (self.get_identifier_text(expr_idx).as_deref() == Some("undefined"))
            .then(|| "any".to_string())
    }

    fn reference_declared_type_annotation_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let decl_node = self.arena.get(decl_idx)?;
            // Variable declarations (var/let/const)
            if let Some(var_decl) = self.arena.get_variable_declaration(decl_node)
                && let Some(type_text) = self
                    .preferred_annotation_name_text(var_decl.type_annotation)
                    .or_else(|| self.emit_type_node_text(var_decl.type_annotation))
            {
                let trimmed = type_text.trim_end();
                let trimmed = trimmed.strip_suffix('=').unwrap_or(trimmed).trim_end();
                return Some(trimmed.to_string());
            }
            // Property declarations (class members)
            if let Some(prop_decl) = self.arena.get_property_decl(decl_node)
                && let Some(type_text) = self
                    .preferred_annotation_name_text(prop_decl.type_annotation)
                    .or_else(|| self.emit_type_node_text(prop_decl.type_annotation))
            {
                let trimmed = type_text.trim_end();
                let trimmed = trimmed.strip_suffix('=').unwrap_or(trimmed).trim_end();
                return Some(trimmed.to_string());
            }
            // Parameters (function/method parameters)
            if let Some(param) = self.arena.get_parameter(decl_node)
                && let Some(type_text) = self
                    .preferred_annotation_name_text(param.type_annotation)
                    .or_else(|| self.emit_type_node_text(param.type_annotation))
            {
                let trimmed = type_text.trim_end();
                let trimmed = trimmed.strip_suffix('=').unwrap_or(trimmed).trim_end();
                return Some(trimmed.to_string());
            }
        }

        None
    }

    fn local_type_annotation_text(&self, type_idx: NodeIndex) -> Option<String> {
        let text = self.source_file_text.as_deref()?;
        let node = self.arena.get(type_idx)?;
        let start = usize::try_from(node.pos).ok()?;
        let end = usize::try_from(node.end).ok()?;
        let slice = text.get(start..end)?.trim();
        (!slice.is_empty()).then(|| slice.to_string())
    }

    fn preferred_annotation_name_text(&self, type_idx: NodeIndex) -> Option<String> {
        let raw = self.local_type_annotation_text(type_idx)?;
        Self::simple_type_reference_name(&raw).map(|_| raw)
    }

    fn call_expression_declared_return_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        let sym_id = self.value_reference_symbol(call.expression)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        let source_arena = binder.symbol_arenas.get(&sym_id)?;
        let source_file = self.arena_source_file(source_arena.as_ref())?;
        if !source_file.is_declaration_file {
            return None;
        }

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = source_arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = source_arena.get_function(decl_node) else {
                continue;
            };
            if func.type_annotation.is_none() {
                continue;
            }
            if let Some(type_text) =
                self.source_slice_from_arena(source_arena.as_ref(), func.type_annotation)
            {
                return Some(
                    type_text
                        .trim_end()
                        .trim_end_matches(';')
                        .trim_end()
                        .to_string(),
                );
            }
        }

        None
    }

    fn nameable_new_expression_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        let new_expr = self.arena.get_call_expr(expr_node)?;
        let base_text = self.declaration_constructor_expression_text(new_expr.expression)?;
        let type_args = self.type_argument_list_source_text(new_expr.type_arguments.as_ref());
        if type_args.is_empty() {
            Some(base_text)
        } else {
            Some(format!("{base_text}<{}>", type_args.join(", ")))
        }
    }

    fn type_argument_list_source_text(&self, type_args: Option<&NodeList>) -> Vec<String> {
        let Some(list) = type_args else {
            return Vec::new();
        };

        list.nodes
            .iter()
            .enumerate()
            .filter_map(|(index, &arg)| {
                let node = self.arena.get(arg)?;
                let mut text = self.get_source_slice_no_semi(node.pos, node.end)?;
                if self.first_type_argument_needs_parentheses(arg, index == 0) {
                    text = format!("({text})");
                }
                Some(text)
            })
            .collect()
    }

    pub(crate) fn first_type_argument_needs_parentheses(
        &self,
        type_arg_idx: NodeIndex,
        is_first: bool,
    ) -> bool {
        if !is_first {
            return false;
        }

        self.arena
            .get(type_arg_idx)
            .and_then(|node| self.arena.get_function_type(node))
            .is_some_and(|func| {
                !func
                    .type_parameters
                    .as_ref()
                    .is_none_or(|params| params.nodes.is_empty())
            })
    }

    fn declaration_constructor_expression_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                self.identifier_constructor_reference_text(expr_idx)
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(expr_node)?;
                let lhs = self.declaration_constructor_expression_text(access.expression)?;
                let rhs = self.get_identifier_text(access.name_or_argument)?;
                Some(format!("{lhs}.{rhs}"))
            }
            _ => None,
        }
    }

    fn identifier_constructor_reference_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let ident = self.get_identifier_text(expr_idx)?;
        let binder = self.binder?;
        let sym_id = self.resolve_identifier_symbol(expr_idx, &ident)?;
        let symbol = binder.symbols.get(sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let decl_node = self.arena.get(decl_idx)?;
            if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                continue;
            }
            let import_eq = self.arena.get_import_decl(decl_node)?;
            let target_node = self.arena.get(import_eq.module_specifier)?;
            if target_node.kind == SyntaxKind::StringLiteral as u16 {
                return Some(ident);
            }
            return Some(ident);
        }

        Some(ident)
    }

    fn resolve_identifier_symbol(&self, expr_idx: NodeIndex, ident: &str) -> Option<SymbolId> {
        let binder = self.binder?;
        let no_libs: &[Arc<BinderState>] = &[];
        binder
            .get_node_symbol(expr_idx)
            .or_else(|| {
                binder.resolve_name_with_filter(ident, self.arena, expr_idx, no_libs, |_| true)
            })
            .or_else(|| binder.file_locals.get(ident))
    }

    fn array_literal_expression_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
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
            let elem_type = self.preferred_expression_type_text(elem_idx).or_else(|| {
                self.get_node_type_or_names(&[elem_idx])
                    .map(|type_id| self.print_type_id(type_id))
            })?;
            element_types.push(elem_type);
        }

        let mut distinct = Vec::new();
        for ty in element_types {
            if !distinct.iter().any(|existing| existing == &ty) {
                distinct.push(ty);
            }
        }

        let elem_text = if distinct.len() == 1 {
            distinct.pop()?
        } else {
            distinct.join(" | ")
        };
        let needs_parens =
            elem_text.contains("=>") || elem_text.contains('|') || elem_text.contains('&');
        if needs_parens {
            Some(format!("({elem_text})[]"))
        } else {
            Some(format!("{elem_text}[]"))
        }
    }

    fn short_circuit_expression_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let binary = self.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::BarBarToken as u16 {
            return None;
        }
        if !self.expression_is_always_truthy_for_decl_emit(binary.left) {
            return None;
        }

        self.preferred_expression_type_text(binary.left)
            .or_else(|| {
                self.get_node_type_or_names(&[binary.left])
                    .map(|type_id| self.print_type_id(type_id))
            })
    }

    fn emit_type_node_text(&self, type_idx: NodeIndex) -> Option<String> {
        self.arena.get(type_idx)?;

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
        scratch.emit_type(type_idx);
        Some(scratch.writer.take_output())
    }

    fn expression_is_always_truthy_for_decl_emit(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        match expr_node.kind {
            k if k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                true
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.arena.get_binary_expr(expr_node).is_some_and(|binary| {
                    binary.operator_token == SyntaxKind::BarBarToken as u16
                        && self.expression_is_always_truthy_for_decl_emit(binary.left)
                })
            }
            _ => false,
        }
    }

    pub(super) fn function_body_preferred_return_type_text(
        &self,
        body_idx: NodeIndex,
    ) -> Option<String> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        let mut preferred = None;
        if self.collect_unique_return_type_text_from_block(&block.statements, &mut preferred) {
            preferred
        } else {
            None
        }
    }

    fn collect_unique_return_type_text_from_block(
        &self,
        statements: &NodeList,
        preferred: &mut Option<String>,
    ) -> bool {
        statements.nodes.iter().copied().all(|stmt_idx| {
            self.collect_unique_return_type_text_from_statement(stmt_idx, preferred)
        })
    }

    fn collect_unique_return_type_text_from_statement(
        &self,
        stmt_idx: NodeIndex,
        preferred: &mut Option<String>,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return true;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let Some(ret) = self.arena.get_return_statement(stmt_node) else {
                    return false;
                };
                let type_text = if let Some(text) = self
                    .preferred_expression_type_text(ret.expression)
                    .filter(|text| !text.is_empty())
                {
                    text
                } else if let Some(text) = self
                    .infer_fallback_type_text_at(ret.expression, 0)
                    .filter(|text| !text.is_empty())
                {
                    text
                } else {
                    return false;
                };
                if let Some(existing) = preferred.as_ref() {
                    existing == &type_text
                } else {
                    *preferred = Some(type_text);
                    true
                }
            }
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    self.collect_unique_return_type_text_from_block(&block.statements, preferred)
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => self
                .arena
                .get_if_statement(stmt_node)
                .is_some_and(|if_data| {
                    self.collect_unique_return_type_text_from_statement(
                        if_data.then_statement,
                        preferred,
                    ) && !if_data.else_statement.is_none()
                        && self.collect_unique_return_type_text_from_statement(
                            if_data.else_statement,
                            preferred,
                        )
                }),
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.arena.get_try(stmt_node).is_some_and(|try_data| {
                    self.collect_unique_return_type_text_from_statement(
                        try_data.try_block,
                        preferred,
                    ) && !try_data.catch_clause.is_none()
                        && self.collect_unique_return_type_text_from_statement(
                            try_data.catch_clause,
                            preferred,
                        )
                        && !try_data.finally_block.is_none()
                        && self.collect_unique_return_type_text_from_statement(
                            try_data.finally_block,
                            preferred,
                        )
                })
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => self
                .arena
                .get_catch_clause(stmt_node)
                .is_some_and(|catch_data| {
                    self.collect_unique_return_type_text_from_statement(catch_data.block, preferred)
                }),
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                self.arena.get_case_clause(stmt_node).is_some_and(|clause| {
                    self.collect_unique_return_type_text_from_block(&clause.statements, preferred)
                })
            }
            _ => true,
        }
    }

    fn infer_object_literal_type_text_at(
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

            if let Some(member_text) = self.infer_object_member_type_text_named_at(
                member_idx,
                &name,
                depth + 1,
                getter_names.contains(&name),
                setter_names.contains(&name),
            ) {
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
                .map(|m| format!("{member_indent}{m};"))
                .collect();
            Some(format!(
                "{{\n{}\n{closing_indent}}}",
                formatted_members.join("\n")
            ))
        }
    }

    fn infer_object_member_type_text_named_at(
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
                Some(format!("{name}: {type_text}"))
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let data = self.arena.get_shorthand_property(member_node)?;
                let type_text = self
                    .preferred_object_member_initializer_type_text(
                        data.object_assignment_initializer,
                        depth,
                    )
                    .unwrap_or_else(|| "any".to_string());
                Some(format!("{name}: {type_text}"))
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
                let type_text = if data.parameters.nodes.is_empty() {
                    "readonly ".to_string()
                } else {
                    String::new()
                };
                Some(format!("{type_text}{name}: any"))
            }
            _ => None,
        }
    }

    fn object_literal_member_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        self.resolved_computed_property_name_text(name_idx)
            .or_else(|| self.infer_property_name_text(name_idx))
    }

    pub(super) fn resolved_computed_property_name_text(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }

        let computed = self.arena.get_computed_property(name_node)?;
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(computed.expression);
        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
            {
                let interner = self.type_interner?;
                let type_id = self.get_node_type_or_names(&[expr_idx])?;
                let literal = tsz_solver::visitor::literal_value(interner, type_id)?;
                Some(Self::format_property_name_literal_value(&literal, interner))
            }
            _ => None,
        }
    }

    fn format_property_name_literal_value(
        literal: &tsz_solver::types::LiteralValue,
        interner: &tsz_solver::TypeInterner,
    ) -> String {
        match literal {
            tsz_solver::types::LiteralValue::String(atom) => {
                Self::format_property_name_literal_text(&interner.resolve_atom(*atom))
            }
            tsz_solver::types::LiteralValue::Number(n) => Self::format_js_number(n.0),
            tsz_solver::types::LiteralValue::Boolean(b) => b.to_string(),
            tsz_solver::types::LiteralValue::BigInt(atom) => {
                format!("{}n", interner.resolve_atom(*atom))
            }
        }
    }

    fn format_property_name_literal_text(text: &str) -> String {
        if Self::is_unquoted_property_name(text) {
            text.to_string()
        } else {
            format!("\"{}\"", escape_string_for_double_quote(text))
        }
    }

    fn is_unquoted_property_name(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };

        if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
            return false;
        }

        chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    fn preferred_object_member_initializer_type_text(
        &self,
        initializer: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let type_id = self.get_node_type_or_names(&[initializer]);
        if let Some(typeof_text) = self.typeof_prefix_for_value_entity(initializer, true, type_id) {
            return Some(typeof_text);
        }
        if let Some(enum_type_text) = self.enum_member_widened_type_text(initializer) {
            return Some(enum_type_text);
        }
        self.preferred_expression_type_text(initializer)
            .or_else(|| self.infer_fallback_type_text_at(initializer, depth))
    }

    fn enum_member_widened_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;
        let binder = self.binder?;

        let member_sym_id = self.value_reference_symbol(expr_idx)?;
        let member_symbol = binder.symbols.get(member_sym_id)?;
        if !member_symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
            return None;
        }

        let enum_expr = self.skip_parenthesized_non_null_and_comma(access.expression);
        let enum_sym_id = self.value_reference_symbol(enum_expr)?;
        let enum_symbol = binder.symbols.get(enum_sym_id)?;
        if !enum_symbol.has_any_flags(symbol_flags::ENUM)
            || enum_symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
        {
            return None;
        }

        self.nameable_constructor_expression_text(enum_expr)
    }

    fn infer_property_name_text(&self, node_id: NodeIndex) -> Option<String> {
        let node = self.arena.get(node_id)?;
        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(node)?;
            let expr_idx = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(computed.expression);
            let expr_node = self.arena.get(expr_idx)?;
            match expr_node.kind {
                k if k == SyntaxKind::StringLiteral as u16 => {
                    let literal = self.arena.get_literal(expr_node)?;
                    let quote = self.original_quote_char(expr_node);
                    return Some(format!("{}{}{}", quote, literal.text, quote));
                }
                k if k == SyntaxKind::NumericLiteral as u16 => {
                    let literal = self.arena.get_literal(expr_node)?;
                    return Some(Self::normalize_numeric_literal(literal.text.as_ref()));
                }
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
                    return match unary.operator {
                        k if k == SyntaxKind::MinusToken as u16 => Some(format!("[-{normalized}]")),
                        k if k == SyntaxKind::PlusToken as u16 => Some(normalized),
                        _ => None,
                    };
                }
                k if k == SyntaxKind::Identifier as u16
                    || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
                {
                    // Use the COMPUTED_PROPERTY_NAME node's source slice.
                    // The node.end may extend past `]` into trailing `:`
                    // (property colon) or `(` (getter/method params), so
                    // trim to the closing `]` to avoid `::` or `(` leaking.
                    if let Some(mut s) = self.get_source_slice(node.pos, node.end) {
                        // Find the last `]` and truncate after it
                        if let Some(bracket_pos) = s.rfind(']') {
                            s.truncate(bracket_pos + 1);
                        } else {
                            // No brackets — trim trailing punctuation
                            while s.ends_with(':') || s.ends_with('(') {
                                s.pop();
                                s = s.trim_end().to_string();
                            }
                        }
                        if !s.is_empty() {
                            return Some(s);
                        }
                    }
                    return None;
                }
                _ => return None,
            }
        }
        if let Some(ident) = self.arena.get_identifier(node) {
            return Some(ident.escaped_text.clone());
        }
        if let Some(literal) = self.arena.get_literal(node) {
            let quote = self.original_quote_char(node);
            return Some(format!("{}{}{}", quote, literal.text, quote));
        }
        self.get_source_slice(node.pos, node.end)
    }

    fn skip_parenthesized_non_null_and_comma(&self, mut idx: NodeIndex) -> NodeIndex {
        for _ in 0..100 {
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(node)
            {
                idx = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = self.arena.get_unary_expr_ex(node)
            {
                idx = unary.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::CommaToken as u16
            {
                idx = binary.right;
                continue;
            }
            return idx;
        }
        idx
    }

    fn semantic_simple_enum_access(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if !self.is_simple_enum_access(expr_node) {
            return None;
        }

        let access = self.arena.get_access_expr(expr_node)?;
        let base_name = self.get_identifier_text(access.expression)?;

        if let Some(binder) = self.binder
            && let Some(symbol_id) = binder.get_node_symbol(access.expression)
            && let Some(symbol) = binder.symbols.get(symbol_id)
            && symbol.flags & tsz_binder::symbol_flags::ENUM != 0
            && symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER == 0
        {
            return Some(expr_idx);
        }

        let source_file_idx = self.current_source_file_idx?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::ENUM_DECLARATION {
                continue;
            }
            if let Some(enum_data) = self.arena.get_enum(stmt_node)
                && self.get_identifier_text(enum_data.name).as_deref() == Some(base_name.as_str())
            {
                return Some(expr_idx);
            }
        }
        None
    }

    pub(crate) fn simple_enum_access_member_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.semantic_simple_enum_access(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;
        let base_name = self.get_identifier_text(access.expression)?;
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let member_name = self.get_identifier_text(access.name_or_argument)?;
            return Some(format!("{base_name}.{member_name}"));
        }

        if expr_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let member_node = self.arena.get(access.name_or_argument)?;
            let member_text = self.get_source_slice(member_node.pos, member_node.end)?;
            return Some(format!("{base_name}[{member_text}]"));
        }

        None
    }

    pub(crate) fn simple_enum_access_base_name_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.semantic_simple_enum_access(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;
        let base_node = self.arena.get(access.expression)?;
        self.get_source_slice(base_node.pos, base_node.end)
    }

    pub(crate) fn const_asserted_enum_access_member_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        let assertion = self.arena.get_type_assertion(expr_node)?;
        let type_node = self.arena.get(assertion.type_node)?;
        let type_text = self.get_source_slice(type_node.pos, type_node.end)?;
        if type_text != "const" {
            return None;
        }

        self.simple_enum_access_member_text(assertion.expression)
    }

    fn invalid_const_enum_object_access(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(base_name) = self.get_identifier_text(access.expression) else {
            return false;
        };

        let is_const_enum = if let Some(binder) = self.binder
            && let Some(symbol_id) = binder.get_node_symbol(access.expression)
            && let Some(symbol) = binder.symbols.get(symbol_id)
        {
            symbol.flags & tsz_binder::symbol_flags::CONST_ENUM != 0
        } else if let Some(source_file_idx) = self.current_source_file_idx
            && let Some(source_file_node) = self.arena.get(source_file_idx)
            && let Some(source_file) = self.arena.get_source_file(source_file_node)
        {
            source_file
                .statements
                .nodes
                .iter()
                .copied()
                .any(|stmt_idx| {
                    let Some(stmt_node) = self.arena.get(stmt_idx) else {
                        return false;
                    };
                    if stmt_node.kind != syntax_kind_ext::ENUM_DECLARATION {
                        return false;
                    }
                    let Some(enum_data) = self.arena.get_enum(stmt_node) else {
                        return false;
                    };
                    self.get_identifier_text(enum_data.name).as_deref() == Some(base_name.as_str())
                        && self
                            .arena
                            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword)
                })
        } else {
            false
        };
        if !is_const_enum {
            return false;
        }

        let argument_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(access.name_or_argument);
        self.arena
            .get(argument_idx)
            .is_some_and(|arg| arg.kind != SyntaxKind::StringLiteral as u16)
    }

    fn object_literal_prefers_syntax_type_text(&self, initializer: NodeIndex) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(object) = self.arena.get_literal_expr(init_node) else {
            return false;
        };

        object
            .elements
            .nodes
            .iter()
            .copied()
            .any(|member_idx| self.object_literal_member_needs_syntax_override(member_idx))
    }

    fn rewrite_object_literal_computed_member_type_text(
        &self,
        initializer: NodeIndex,
        type_id: tsz_solver::types::TypeId,
    ) -> Option<String> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.arena.get_literal_expr(init_node)?;

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

        let mut computed_members = Vec::new();
        let mut overridden_members = Vec::new();
        let mut only_numeric_like = true;

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let name_idx = if let Some(data) = self.arena.get_property_assignment(member_node) {
                Some(data.name)
            } else if let Some(data) = self.arena.get_shorthand_property(member_node) {
                Some(data.name)
            } else if let Some(data) = self.arena.get_accessor(member_node) {
                Some(data.name)
            } else {
                self.arena
                    .get_method_decl(member_node)
                    .map(|data| data.name)
            };
            let Some(name_idx) = name_idx else {
                continue;
            };
            let Some(name_node) = self.arena.get(name_idx) else {
                continue;
            };
            if !self.object_literal_member_needs_syntax_override(member_idx) {
                continue;
            }

            let Some(name_text) = self.object_literal_member_name_text(name_idx) else {
                continue;
            };
            let preserve_computed_syntax = name_node.kind
                == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && self
                    .resolved_computed_property_name_text(name_idx)
                    .is_none();
            let Some(member_text) = self.infer_object_member_type_text_named_at(
                member_idx,
                &name_text,
                self.indent_level + 1,
                getter_names.contains(&name_text),
                setter_names.contains(&name_text),
            ) else {
                continue;
            };
            if preserve_computed_syntax {
                // Skip methods with computed names — the solver already produces correct
                // method signatures (e.g., `"new"(x: number): number`). Overriding them
                // would emit a wrong property form like `"new": any`.
                if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                    continue;
                }
                only_numeric_like &= Self::is_numeric_property_name_text(&name_text);
                computed_members.push((name_text, member_text));
            } else {
                overridden_members.push((name_text, member_text));
            }
        }

        if computed_members.is_empty() && overridden_members.is_empty() {
            return None;
        }

        if overridden_members
            .iter()
            .any(|(_, member_text)| member_text.contains('\n'))
        {
            return self.infer_object_literal_type_text_at(initializer, self.indent_level);
        }

        let printed = self.print_type_id(type_id);
        let mut lines: Vec<String> = printed.lines().map(str::to_string).collect();
        if lines.len() < 2 {
            return Some(printed);
        }

        if only_numeric_like {
            lines.retain(|line| !line.trim_start().starts_with("[x: string]:"));
        }

        let indent = "    ".repeat((self.indent_level + 1) as usize);
        for (name_text, member_text) in overridden_members {
            let replacement = format!("{indent}{member_text};");
            if let Some(existing_idx) = lines.iter().position(|line| {
                Self::object_literal_property_line_matches(line, &name_text, &replacement)
            }) {
                lines[existing_idx] = replacement;
            } else {
                let insert_at = lines.len().saturating_sub(1);
                lines.insert(insert_at, replacement);
            }
        }

        let insert_at = lines.len().saturating_sub(1);
        let mut actual_insertions = 0usize;
        for (name_text, member_text) in computed_members {
            let line = format!("{indent}{member_text};");
            if let Some(existing_idx) = lines.iter().position(|existing| {
                Self::object_literal_property_line_matches(existing, &name_text, &line)
            }) {
                lines[existing_idx] = line;
            } else {
                let line_trimmed = line.trim();
                if !lines.iter().any(|existing| existing.trim() == line_trimmed) {
                    lines.insert(insert_at + actual_insertions, line);
                    actual_insertions += 1;
                }
            }
        }

        Some(lines.join("\n"))
    }

    fn object_literal_property_line_matches(
        existing: &str,
        name_text: &str,
        replacement: &str,
    ) -> bool {
        let trimmed = existing.trim();
        if trimmed == replacement.trim() {
            return true;
        }

        for prefix in Self::object_literal_property_name_prefixes(name_text) {
            if trimmed.starts_with(&prefix) || trimmed.starts_with(&format!("readonly {prefix}")) {
                return true;
            }
        }

        false
    }

    fn object_literal_property_name_prefixes(name_text: &str) -> Vec<String> {
        let mut prefixes = vec![format!("{name_text}:")];

        if let Some(unquoted) = name_text
            .strip_prefix('"')
            .and_then(|name| name.strip_suffix('"'))
            .or_else(|| {
                name_text
                    .strip_prefix('\'')
                    .and_then(|name| name.strip_suffix('\''))
            })
        {
            prefixes.push(format!("\"{unquoted}\":"));
            prefixes.push(format!("'{unquoted}':"));
        }

        if let Some(negative_numeric) = name_text
            .strip_prefix("[-")
            .and_then(|name| name.strip_suffix(']'))
        {
            prefixes.push(format!("\"-{negative_numeric}\":"));
            prefixes.push(format!("'-{negative_numeric}':"));
            prefixes.push(format!("-{negative_numeric}:"));
        }

        prefixes
    }

    fn object_literal_member_needs_syntax_override(&self, member_idx: NodeIndex) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return false;
        };
        let Some(name_idx) = self.object_literal_member_name_idx(member_node) else {
            return false;
        };
        if self
            .arena
            .get(name_idx)
            .is_some_and(|name_node| name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
        {
            return true;
        }

        let Some(initializer) = self.object_literal_member_initializer(member_node) else {
            return false;
        };
        if self
            .arena
            .get(initializer)
            .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
            && self.object_literal_prefers_syntax_type_text(initializer)
        {
            return true;
        }
        let type_id = self.get_node_type_or_names(&[initializer]);
        self.typeof_prefix_for_value_entity(initializer, true, type_id)
            .is_some()
            || self.enum_member_widened_type_text(initializer).is_some()
    }

    fn object_literal_member_name_idx(&self, member_node: &Node) -> Option<NodeIndex> {
        if let Some(data) = self.arena.get_property_assignment(member_node) {
            return Some(data.name);
        }
        if let Some(data) = self.arena.get_shorthand_property(member_node) {
            return Some(data.name);
        }
        if let Some(data) = self.arena.get_accessor(member_node) {
            return Some(data.name);
        }
        self.arena
            .get_method_decl(member_node)
            .map(|data| data.name)
    }

    fn object_literal_member_initializer(&self, member_node: &Node) -> Option<NodeIndex> {
        if let Some(data) = self.arena.get_property_assignment(member_node) {
            return Some(data.initializer);
        }
        self.arena
            .get_shorthand_property(member_node)
            .map(|data| data.object_assignment_initializer)
    }

    fn is_numeric_property_name_text(name: &str) -> bool {
        name.parse::<f64>().is_ok()
            || (name.starts_with("[-")
                && name.ends_with(']')
                && name[2..name.len().saturating_sub(1)].parse::<f64>().is_ok())
    }

    pub(crate) fn get_node_type_or_names(
        &self,
        node_ids: &[NodeIndex],
    ) -> Option<tsz_solver::types::TypeId> {
        for &node_id in node_ids {
            if let Some(type_id) = self.get_node_type(node_id) {
                return Some(type_id);
            }

            if let Some(type_id) = self.recover_expression_type_from_structure(node_id) {
                return Some(type_id);
            }

            let Some(node) = self.arena.get(node_id) else {
                continue;
            };

            for related_id in self.get_node_type_related_nodes(node) {
                if let Some(type_id) = self.get_node_type(related_id) {
                    return Some(type_id);
                }

                if let Some(type_id) = self.recover_expression_type_from_structure(related_id) {
                    return Some(type_id);
                }
            }
        }
        None
    }

    fn recover_expression_type_from_structure(
        &self,
        node_id: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let node = self.arena.get(node_id)?;
        let interner = self.type_interner?;

        match node.kind {
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                let call = self.arena.get_call_expr(node)?;
                let callee_type = self
                    .get_node_type_or_names(&[call.expression])
                    .or_else(|| self.get_type_via_symbol(call.expression))?;
                tsz_solver::type_queries::get_return_type(interner, callee_type)
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || k == syntax_kind_ext::AWAIT_EXPRESSION =>
            {
                let inner = self.arena.get_unary_expr_ex(node)?.expression;
                self.get_node_type_or_names(&[inner])
                    .or_else(|| self.get_type_via_symbol(inner))
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let inner = self.arena.get_unary_expr_ex(node)?.expression;
                self.get_node_type_or_names(&[inner])
                    .or_else(|| self.get_type_via_symbol(inner))
            }
            _ => None,
        }
    }

    pub(crate) fn get_node_type_related_nodes(&self, node: &Node) -> Vec<NodeIndex> {
        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.arena.get_variable_declaration(node) {
                    let mut related = Vec::with_capacity(1);
                    if decl.initializer.is_some() {
                        related.push(decl.initializer);
                    }
                    related.push(decl.type_annotation);
                    related
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(decl) = self.arena.get_property_decl(node) {
                    let mut related = Vec::with_capacity(2);
                    if decl.initializer.is_some() {
                        related.push(decl.initializer);
                    }
                    related.push(decl.type_annotation);
                    related
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::PARAMETER => {
                if let Some(param) = self.arena.get_parameter(node) {
                    if param.initializer.is_some() {
                        vec![param.initializer]
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access_expr) = self.arena.get_access_expr(node) {
                    vec![access_expr.expression, access_expr.name_or_argument]
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access_expr) = self.arena.get_access_expr(node) {
                    vec![access_expr.expression, access_expr.name_or_argument]
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                if let Some(query) = self.arena.get_type_query(node) {
                    vec![query.expr_name]
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                if let Some(unary) = self.arena.get_unary_expr_ex(node) {
                    vec![unary.expression]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    /// Print a `TypeId` as TypeScript syntax using `TypePrinter`.
    pub(crate) fn print_type_id(&self, type_id: tsz_solver::types::TypeId) -> String {
        if let Some(interner) = self.type_interner {
            // Evaluate the type before printing to expand mapped types over
            // literal union constraints (e.g., `{[k in "ar"|"bg"]?: T}` becomes
            // `{ar?: T; bg?: T}`).  This matches tsc's behavior in declaration
            // emit where mapped types are fully resolved.
            let type_id = tsz_solver::evaluate_type(interner, type_id);

            let module_path_resolver = |sym_id| self.resolve_symbol_module_path(sym_id);
            let namespace_alias_resolver = |sym_id| self.resolve_namespace_import_alias(sym_id);
            let local_import_alias_name_resolver =
                |sym_id| self.can_reference_local_import_alias_by_name(sym_id);
            let has_local_import_alias_resolver = |sym_id| {
                if let Some(binder) = self.binder {
                    self.symbol_has_local_import_alias(binder, sym_id)
                } else {
                    false
                }
            };
            let mut printer = TypePrinter::new(interner)
                .with_indent_level(self.indent_level)
                .with_node_arena(self.arena)
                .with_module_path_resolver(&module_path_resolver)
                .with_namespace_alias_resolver(&namespace_alias_resolver)
                .with_local_import_alias_name_resolver(&local_import_alias_name_resolver)
                .with_has_local_import_alias_resolver(&has_local_import_alias_resolver)
                .with_strict_null_checks(self.strict_null_checks);

            // Add symbol arena if available for visibility checking
            if let Some(binder) = self.binder {
                printer = printer.with_symbols(&binder.symbols);
            }

            // Add type cache if available for resolving Lazy(DefId) types
            if let Some(cache) = &self.type_cache {
                printer = printer.with_type_cache(cache);
            }

            // Set enclosing namespace for context-relative qualified names
            if let Some(enc_sym) = self.enclosing_namespace_symbol {
                printer = printer.with_enclosing_symbol(enc_sym);
            }

            printer.print_type(type_id)
        } else {
            // Fallback if no interner available
            "any".to_string()
        }
    }

    fn print_synthetic_class_extends_alias_type(
        &self,
        type_id: tsz_solver::types::TypeId,
    ) -> String {
        let Some(interner) = self.type_interner else {
            return self.print_type_id(type_id);
        };
        let Some(callable_id) = tsz_solver::visitor::callable_shape_id(interner, type_id) else {
            return self.print_type_id(type_id);
        };
        let callable = interner.callable_shape(callable_id);
        let has_properties = callable.properties.iter().any(|prop| {
            let name = interner.resolve_atom(prop.name);
            name != "prototype" && !name.starts_with("__private_brand_")
        });

        if callable.symbol.is_none()
            && callable.call_signatures.is_empty()
            && callable.construct_signatures.len() == 1
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
            && callable.construct_signatures[0].type_predicate.is_none()
        {
            return self.print_construct_signature_arrow_text(
                &callable.construct_signatures[0],
                callable.is_abstract,
            );
        }

        self.print_type_id(type_id)
    }

    fn print_construct_signature_arrow_text(
        &self,
        sig: &tsz_solver::types::CallSignature,
        is_abstract: bool,
    ) -> String {
        let Some(interner) = self.type_interner else {
            return self.print_type_id(sig.return_type);
        };

        let type_params = if sig.type_params.is_empty() {
            String::new()
        } else {
            let params = sig
                .type_params
                .iter()
                .map(|tp| {
                    let mut text = String::new();
                    if tp.is_const {
                        text.push_str("const ");
                    }
                    text.push_str(&interner.resolve_atom(tp.name));
                    if let Some(constraint) = tp.constraint {
                        text.push_str(" extends ");
                        text.push_str(&self.print_type_id(constraint));
                    }
                    if let Some(default) = tp.default {
                        text.push_str(" = ");
                        text.push_str(&self.print_type_id(default));
                    }
                    text
                })
                .collect::<Vec<_>>();
            format!("<{}>", params.join(", "))
        };

        let params = sig
            .params
            .iter()
            .map(|param| {
                let mut text = String::new();
                if param.rest {
                    text.push_str("...");
                }
                if let Some(name) = param.name {
                    text.push_str(&interner.resolve_atom(name));
                    if param.optional {
                        text.push('?');
                    }
                    text.push_str(": ");
                }
                text.push_str(&self.print_type_id(param.type_id));
                text
            })
            .collect::<Vec<_>>();

        let prefix = if is_abstract { "abstract new " } else { "new " };
        format!(
            "{prefix}{}({}) => {}",
            type_params,
            params.join(", "),
            self.print_type_id(sig.return_type)
        )
    }

    /// Resolve a foreign symbol to its module path.
    ///
    /// Returns the module specifier (e.g., "./utils") for importing the symbol.
    pub(crate) fn resolve_symbol_module_path(&self, sym_id: SymbolId) -> Option<String> {
        let (Some(binder), Some(current_path)) = (&self.binder, &self.current_file_path) else {
            return None;
        };

        // Determine the "original" symbol (following import aliases).
        let original_sym_id = binder
            .resolve_import_symbol(sym_id)
            .filter(|resolved| *resolved != sym_id)
            .unwrap_or(sym_id);

        if let Some(path) =
            self.resolve_symbol_module_path_from_source(original_sym_id, binder, current_path)
        {
            // If the symbol is globally accessible (e.g. from a non-module .d.ts
            // or a triple-slash referenced global), suppress the import qualifier.
            if self.symbol_is_globally_accessible(binder, sym_id, original_sym_id) {
                return None;
            }
            return Some(path);
        }

        // Try the non-resolved symbol if it differs.
        if original_sym_id != sym_id {
            if let Some(path) =
                self.resolve_symbol_module_path_from_source(sym_id, binder, current_path)
            {
                if self.symbol_is_globally_accessible(binder, sym_id, original_sym_id) {
                    return None;
                }
                return Some(path);
            }
        }

        // Fall back to the raw import text for imported symbols when we
        // don't have a source file mapping for the originating declaration.
        if let Some(module_specifier) = self.import_symbol_map.get(&sym_id) {
            return Some(module_specifier.clone());
        }

        binder.symbols.get(sym_id)?.import_module.clone()
    }

    /// Check whether a foreign symbol has a local import alias in this file
    /// that will be emitted, making it referenceable by name.
    fn symbol_has_local_import_alias(
        &self,
        binder: &BinderState,
        original_sym_id: SymbolId,
    ) -> bool {
        let symbol = match binder.symbols.get(original_sym_id) {
            Some(s) => s,
            None => return false,
        };
        let target_name = &symbol.escaped_name;

        // Check import_symbol_map: each entry is (alias_sym_id, module_specifier).
        // If an alias resolves to the same original symbol, the name is in scope.
        for &alias_sym_id in self.import_symbol_map.keys() {
            if let Some(resolved) = binder.resolve_import_symbol(alias_sym_id)
                && resolved == original_sym_id
            {
                return true;
            }
            // Also match by name + module when resolve_import_symbol doesn't
            // link them (e.g. cross-file merges).
            if let Some(alias_symbol) = binder.symbols.get(alias_sym_id) {
                let alias_import_name = alias_symbol
                    .import_name
                    .as_deref()
                    .unwrap_or(&alias_symbol.escaped_name);
                if alias_import_name == target_name && alias_symbol.import_module.is_some() {
                    // Verify the alias points to the same foreign module.
                    if let Some(current_path) = &self.current_file_path {
                        if let Some(source_arena) = binder.symbol_arenas.get(&original_sym_id) {
                            let arena_addr = std::sync::Arc::as_ptr(source_arena) as usize;
                            if let Some(source_path) = self.arena_to_path.get(&arena_addr) {
                                let rel = self.calculate_relative_path(current_path, source_path);
                                let stripped = self.strip_ts_extensions(&rel);
                                if alias_symbol.import_module.as_deref() == Some(&stripped)
                                    || alias_symbol.import_module.as_deref()
                                        == Some(source_path.as_str())
                                {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }

        false
    }

    /// Check whether a symbol is globally accessible (from a non-module .d.ts,
    /// triple-slash reference, or ambient global declaration) so it doesn't
    /// need an import("...") qualifier.
    fn symbol_is_globally_accessible(
        &self,
        binder: &BinderState,
        sym_id: SymbolId,
        original_sym_id: SymbolId,
    ) -> bool {
        let check_sym_id = if original_sym_id != sym_id {
            original_sym_id
        } else {
            sym_id
        };
        let symbol = match binder.symbols.get(check_sym_id) {
            Some(s) => s,
            None => return false,
        };

        // Import aliases are never "global" in this sense.
        if symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS) && symbol.import_module.is_some() {
            return false;
        }

        // Walk up to the root parent symbol to find the top-level name.
        // For `M.C`, the root is `M`; for top-level `X`, the root is `X` itself.
        let mut root_id = check_sym_id;
        let mut root_name = &symbol.escaped_name;
        let mut cur_id = check_sym_id;
        // Walk up parent chain (max 20 levels to avoid infinite loops)
        for _ in 0..20 {
            let Some(cur_sym) = binder.symbols.get(cur_id) else {
                break;
            };
            if !cur_sym.parent.is_some() {
                root_id = cur_id;
                root_name = &cur_sym.escaped_name;
                break;
            }
            let parent_id = cur_sym.parent;
            match binder.symbols.get(parent_id) {
                Some(parent_sym) => {
                    // Symbols inside `declare module "..."` are module-scoped,
                    // not globally accessible. Return false immediately.
                    // Check: string-literal module names (starts with `"`) or
                    // MODULE-flagged parents that come from ambient module
                    // declarations (like @types/node's `declare module "url"`).
                    if parent_sym.escaped_name.starts_with('"') {
                        return false;
                    }
                    // A parent with MODULE flags whose name appears in
                    // module_exports indicates an ambient external module
                    // (e.g. `declare module "url"`). Its children are
                    // module-scoped, not globally accessible.
                    if parent_sym.has_any_flags(tsz_binder::symbol_flags::MODULE)
                        && binder.module_exports.contains_key(&parent_sym.escaped_name)
                    {
                        return false;
                    }
                    // Stop at source-file-like internal parents.
                    if parent_sym.escaped_name.starts_with("__") {
                        root_id = cur_id;
                        root_name = &cur_sym.escaped_name;
                        break;
                    }
                    cur_id = parent_id;
                }
                None => {
                    root_id = cur_id;
                    root_name = &cur_sym.escaped_name;
                    break;
                }
            }
        }

        // Check if the root symbol is accessible from file_locals or current_scope.
        self.symbol_name_is_locally_accessible(binder, root_id, root_name)
    }

    /// Check whether a symbol with the given name/id is reachable in the
    /// local scope (`file_locals` or `current_scope`) without an import qualifier.
    fn symbol_name_is_locally_accessible(
        &self,
        binder: &BinderState,
        sym_id: SymbolId,
        name: &str,
    ) -> bool {
        if let Some(local_sym_id) = binder.file_locals.get(name) {
            if local_sym_id == sym_id {
                return true;
            }
            if let Some(resolved) = binder.resolve_import_symbol(local_sym_id)
                && resolved == sym_id
            {
                return true;
            }
        }
        if let Some(scope_sym_id) = binder.current_scope.get(name) {
            if scope_sym_id == sym_id {
                return true;
            }
            if let Some(resolved) = binder.resolve_import_symbol(scope_sym_id)
                && resolved == sym_id
            {
                return true;
            }
        }
        false
    }

    fn resolve_symbol_module_path_from_source(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
        current_path: &str,
    ) -> Option<String> {
        if let Some(ambient_path) = self.check_ambient_module(sym_id, binder) {
            return Some(ambient_path);
        }

        if let Some(source_arena) = binder.symbol_arenas.get(&sym_id) {
            let arena_addr = Arc::as_ptr(source_arena) as usize;
            if let Some(source_path) = self.arena_to_path.get(&arena_addr) {
                if self.paths_refer_to_same_source_file(current_path, source_path) {
                    return None;
                }

                // Symbols sourced from node_modules should retain the package
                // export subpath that tsc would print in declaration emit rather
                // than the raw source import text or a relative filesystem path.
                if let Some(package_specifier) =
                    self.package_specifier_for_node_modules_path(current_path, source_path)
                {
                    return Some(package_specifier);
                }

                let rel_path = self.calculate_relative_path(current_path, source_path);
                return Some(self.strip_ts_extensions(&rel_path));
            }
        }

        None
    }

    #[allow(dead_code)]
    pub(crate) fn resolve_symbol_module_path_cached(&mut self, sym_id: SymbolId) -> Option<String> {
        if let Some(cached) = self.symbol_module_specifier_cache.get(&sym_id) {
            return cached.clone();
        }

        let resolved = self.resolve_symbol_module_path(sym_id);
        self.symbol_module_specifier_cache
            .insert(sym_id, resolved.clone());
        resolved
    }

    fn is_namespace_import_alias_symbol(&self, sym_id: SymbolId) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };

        symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
            && symbol.import_module.is_some()
            && (symbol.import_name.is_none() || symbol.import_name.as_deref() == Some("*"))
    }

    pub(crate) fn resolve_namespace_import_alias(&self, sym_id: SymbolId) -> Option<String> {
        let binder = self.binder?;

        if self.is_namespace_import_alias_symbol(sym_id) {
            return binder
                .symbols
                .get(sym_id)
                .map(|symbol| symbol.escaped_name.clone());
        }

        let module_path = self.resolve_symbol_module_path(sym_id)?;

        let mut local_imports: Vec<SymbolId> = self.import_symbol_map.keys().copied().collect();
        local_imports.sort();

        for import_sym_id in local_imports {
            let Some(symbol) = binder.symbols.get(import_sym_id) else {
                continue;
            };
            if !self.is_namespace_import_alias_symbol(import_sym_id) {
                continue;
            }
            if symbol.import_module.as_deref() == Some(module_path.as_str()) {
                return Some(symbol.escaped_name.clone());
            }
        }

        None
    }

    /// Check if a symbol is from an ambient module declaration.
    ///
    /// Returns the module name if the symbol is declared inside `declare module "name"`.
    pub(crate) fn check_ambient_module(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<String> {
        let symbol = binder.symbols.get(sym_id)?;

        // Walk up the parent chain
        let mut current_sym = symbol;
        let mut parent_id = current_sym.parent;
        while parent_id.is_some() {
            let parent_sym = binder.symbols.get(parent_id)?;

            // Check if parent is a module declaration
            if parent_sym.flags & tsz_binder::symbol_flags::MODULE != 0 {
                // Check if this module is in declared_modules
                let module_name = &parent_sym.escaped_name;
                if binder.declared_modules.contains(module_name) {
                    return Some(module_name.clone());
                }
            }

            current_sym = parent_sym;
            parent_id = current_sym.parent;
        }

        None
    }

    /// Calculate relative path from current file to source file.
    ///
    /// Returns a path like "../utils" or "./helper"
    pub(crate) fn calculate_relative_path(&self, current: &str, source: &str) -> String {
        use std::path::{Component, Path};

        let current_path = Path::new(current);
        let source_path = Path::new(source);

        // Get parent directories
        let current_dir = current_path.parent().unwrap_or(current_path);

        // Find common prefix and build relative path
        let current_components: Vec<_> = current_dir.components().collect();
        let source_components: Vec<_> = source_path.components().collect();

        // Find common prefix length
        let common_len = current_components
            .iter()
            .zip(source_components.iter())
            .take_while(|(a, b)| a == b)
            .count();

        // Build relative path: go up from current_dir, then down to source
        let ups = current_components.len() - common_len;
        let mut result = String::new();

        if ups == 0 {
            result.push_str("./");
        } else {
            for _ in 0..ups {
                result.push_str("../");
            }
        }

        // Append remaining source path components
        let remaining: Vec<_> = source_components[common_len..]
            .iter()
            .filter_map(|c| match c {
                Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect();
        result.push_str(&remaining.join("/"));

        // Normalize separators
        result.replace('\\', "/")
    }

    fn package_specifier_for_node_modules_path(
        &self,
        current_path: &str,
        source_path: &str,
    ) -> Option<String> {
        let (source_root, source_specifier) = self.node_modules_package_info(source_path)?;
        let current_root = self
            .node_modules_package_info(current_path)
            .map(|(root, _)| root);

        if current_root.as_deref() == Some(source_root.as_str()) {
            return None;
        }

        Some(source_specifier)
    }

    fn node_modules_package_info(&self, path: &str) -> Option<(String, String)> {
        use std::path::{Component, Path};

        let components: Vec<_> = Path::new(path).components().collect();
        let node_modules_idx = components.iter().rposition(|component| {
            matches!(
                component,
                Component::Normal(part) if part.to_str() == Some("node_modules")
            )
        })?;

        let trailing_parts: Vec<String> = components[node_modules_idx + 1..]
            .iter()
            .filter_map(|component| match component {
                Component::Normal(part) => part.to_str().map(str::to_string),
                _ => None,
            })
            .collect();
        if trailing_parts.is_empty() {
            return None;
        }

        let package_len = if trailing_parts.first()?.starts_with('@') {
            2
        } else {
            1
        };
        if trailing_parts.len() < package_len {
            return None;
        }

        let package_root_components = &components[..node_modules_idx + 1 + package_len];
        let root_key = package_root_components
            .iter()
            .filter_map(|component| match component {
                Component::Prefix(prefix) => prefix.as_os_str().to_str().map(str::to_string),
                Component::RootDir => Some(String::new()),
                Component::Normal(part) => part.to_str().map(str::to_string),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");

        let package_root = Path::new(path)
            .components()
            .take(node_modules_idx + 1 + package_len)
            .collect::<std::path::PathBuf>();
        let package_name = trailing_parts[..package_len].join("/");
        let package_relative_parts = trailing_parts[package_len..].to_vec();
        let relative_path = package_relative_parts.join("/");
        let runtime_relative_path = self.declaration_runtime_relative_path(&relative_path)?;

        let subpath = self
            .reverse_export_specifier_for_runtime_path(&package_root, &runtime_relative_path)
            .or_else(|| {
                let mut specifier_parts = package_relative_parts;
                if let Some(last) = specifier_parts.last_mut() {
                    *last = self.strip_ts_extensions(last);
                }
                if specifier_parts.last().is_some_and(|part| part == "index") {
                    specifier_parts.pop();
                }
                Some(specifier_parts.join("/"))
            })?;

        let specifier = if subpath.is_empty() {
            package_name
        } else {
            format!("{package_name}/{subpath}")
        };

        Some((root_key, specifier))
    }

    fn declaration_runtime_relative_path(&self, relative_path: &str) -> Option<String> {
        let relative_path = relative_path.replace('\\', "/");

        for (decl_ext, runtime_ext) in [
            (".d.ts", ".js"),
            (".d.tsx", ".jsx"),
            (".d.mts", ".mjs"),
            (".d.cts", ".cjs"),
            (".ts", ".js"),
            (".tsx", ".jsx"),
            (".mts", ".mjs"),
            (".cts", ".cjs"),
        ] {
            if let Some(prefix) = relative_path.strip_suffix(decl_ext) {
                return Some(format!("{prefix}{runtime_ext}"));
            }
        }

        Some(relative_path)
    }

    fn reverse_export_specifier_for_runtime_path(
        &self,
        package_root: &std::path::Path,
        runtime_relative_path: &str,
    ) -> Option<String> {
        let package_json_path = package_root.join("package.json");
        let package_json = std::fs::read_to_string(package_json_path).ok()?;
        let package_json: serde_json::Value = serde_json::from_str(&package_json).ok()?;
        let exports = package_json.get("exports")?;
        let runtime_relative_path = format!("./{}", runtime_relative_path.trim_start_matches("./"));
        self.reverse_match_exports_subpath(exports, &runtime_relative_path)
    }

    fn reverse_match_exports_subpath(
        &self,
        exports: &serde_json::Value,
        runtime_path: &str,
    ) -> Option<String> {
        match exports {
            serde_json::Value::String(target) => {
                self.match_export_target(".", target, runtime_path)
            }
            serde_json::Value::Array(entries) => entries
                .iter()
                .find_map(|entry| self.reverse_match_exports_subpath(entry, runtime_path)),
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if key == "." || key.starts_with("./") {
                        if let Some(specifier) =
                            self.reverse_match_export_entry(key, value, runtime_path)
                        {
                            return Some(specifier);
                        }
                        continue;
                    }

                    if let Some(specifier) = self.reverse_match_exports_subpath(value, runtime_path)
                    {
                        return Some(specifier);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn reverse_match_export_entry(
        &self,
        subpath_key: &str,
        value: &serde_json::Value,
        runtime_path: &str,
    ) -> Option<String> {
        match value {
            serde_json::Value::String(target) => {
                self.match_export_target(subpath_key, target, runtime_path)
            }
            serde_json::Value::Array(entries) => entries.iter().find_map(|entry| {
                self.reverse_match_export_entry(subpath_key, entry, runtime_path)
            }),
            serde_json::Value::Object(map) => map.values().find_map(|entry| {
                self.reverse_match_export_entry(subpath_key, entry, runtime_path)
            }),
            _ => None,
        }
    }

    fn match_export_target(
        &self,
        subpath_key: &str,
        target: &str,
        runtime_path: &str,
    ) -> Option<String> {
        let target = target.trim();
        let runtime_path = runtime_path.trim();

        if target.contains('*') {
            let wildcard = self.match_exports_wildcard(target, runtime_path)?;
            return Some(self.apply_exports_wildcard(subpath_key, &wildcard));
        }

        if target.ends_with('/') && subpath_key.ends_with('/') {
            let remainder = runtime_path.strip_prefix(target)?;
            return Some(format!(
                "{}{}",
                subpath_key.trim_start_matches("./"),
                remainder
            ));
        }

        if target != runtime_path {
            return None;
        }

        if subpath_key == "." {
            return Some(String::new());
        }

        Some(subpath_key.trim_start_matches("./").to_string())
    }

    fn match_exports_wildcard(&self, pattern: &str, value: &str) -> Option<String> {
        let star_idx = pattern.find('*')?;
        let prefix = &pattern[..star_idx];
        let suffix = &pattern[star_idx + 1..];
        let middle = value.strip_prefix(prefix)?.strip_suffix(suffix)?;
        Some(middle.to_string())
    }

    fn apply_exports_wildcard(&self, pattern: &str, wildcard: &str) -> String {
        pattern
            .replace('*', wildcard)
            .trim_start_matches("./")
            .to_string()
    }

    /// Strip TypeScript file extensions from a path.
    ///
    /// Converts "../utils.ts" -> "../utils"
    pub(crate) fn strip_ts_extensions(&self, path: &str) -> String {
        // Remove TypeScript and JavaScript source/declaration extensions.
        for ext in [
            ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".tsx", ".ts", ".mts", ".cts", ".jsx", ".js",
            ".mjs", ".cjs",
        ] {
            if let Some(path) = path.strip_suffix(ext) {
                return path.to_string();
            }
        }
        path.to_string()
    }

    fn normalized_source_path(&self, path: &str) -> std::path::PathBuf {
        use std::path::Component;

        std::path::Path::new(&self.strip_ts_extensions(path))
            .components()
            .filter(|component| !matches!(component, Component::CurDir))
            .collect()
    }

    fn paths_refer_to_same_source_file(&self, current_path: &str, source_path: &str) -> bool {
        let current = self.normalized_source_path(current_path);
        let source = self.normalized_source_path(source_path);

        if current == source || current.ends_with(&source) || source.ends_with(&current) {
            return true;
        }

        let canonical_current = std::fs::canonicalize(current_path)
            .ok()
            .map(|path| self.normalized_source_path(path.to_string_lossy().as_ref()));
        let canonical_source = std::fs::canonicalize(source_path)
            .ok()
            .map(|path| self.normalized_source_path(path.to_string_lossy().as_ref()));

        canonical_current
            .zip(canonical_source)
            .is_some_and(|(a, b)| a == b)
    }

    /// Group foreign symbols by their module paths.
    ///
    /// Returns a map of module path -> Vec<SymbolId> for all foreign symbols.
    #[allow(dead_code)]
    pub(crate) fn group_foreign_symbols_by_module(&mut self) -> FxHashMap<String, Vec<SymbolId>> {
        let mut module_map: FxHashMap<String, Vec<SymbolId>> = FxHashMap::default();

        debug!(
            "[DEBUG] group_foreign_symbols_by_module: foreign_symbols = {:?}",
            self.foreign_symbols
        );

        let foreign_symbols: Vec<SymbolId> = self
            .foreign_symbols
            .as_ref()
            .map(|symbols| symbols.iter().copied().collect())
            .unwrap_or_default();

        for sym_id in foreign_symbols {
            debug!(
                "[DEBUG] group_foreign_symbols_by_module: resolving symbol {:?}",
                sym_id
            );
            if let Some(module_path) = self.resolve_symbol_module_path_cached(sym_id) {
                debug!(
                    "[DEBUG] group_foreign_symbols_by_module: symbol {:?} -> module '{}'",
                    sym_id, module_path
                );
                module_map.entry(module_path).or_default().push(sym_id);
            } else {
                debug!(
                    "[DEBUG] group_foreign_symbols_by_module: symbol {:?} -> no module path",
                    sym_id
                );
            }
        }

        debug!(
            "[DEBUG] group_foreign_symbols_by_module: returning {} modules",
            module_map.len()
        );
        module_map
    }

    pub(crate) fn prepare_import_plan(&mut self) {
        let mut plan = ImportPlan::default();

        let mut required_modules: Vec<String> = self.required_imports.keys().cloned().collect();
        required_modules.sort();
        for module in required_modules {
            let Some(symbol_names) = self.required_imports.get(&module) else {
                continue;
            };
            if symbol_names.is_empty() {
                continue;
            }

            let mut deduped = symbol_names.clone();
            deduped.sort();
            deduped.dedup();

            let symbols = deduped
                .into_iter()
                .map(|name| {
                    let alias = self
                        .import_string_aliases
                        .get(&(module.clone(), name.clone()))
                        .cloned();
                    PlannedImportSymbol { name, alias }
                })
                .collect();

            plan.required.push(PlannedImportModule { module, symbols });
        }

        // NOTE: Auto-generated imports for foreign symbols are intentionally
        // disabled. Source import declarations are now emitted faithfully
        // (preserving `type` modifiers, `with` attributes, aliases, etc.)
        // through `emit_import_declaration`, making auto-imports redundant
        // for symbols that have source imports. Symbols referenced only via
        // inline `import("pkg").Foo` type syntax don't need import
        // declarations at all. This avoids duplicate import lines that were
        // previously generated for resolution-mode imports.

        self.import_plan = plan;
    }

    fn emit_import_modules(&mut self, modules: &[PlannedImportModule]) {
        for module in modules {
            self.write_indent();
            self.write("import { ");

            let mut first = true;
            for symbol in &module.symbols {
                if !first {
                    self.write(", ");
                }
                first = false;

                self.write(&symbol.name);
                if let Some(alias) = &symbol.alias {
                    self.write(" as ");
                    self.write(alias);
                }
            }

            self.write(" } from \"");
            self.write(&module.module);
            self.write("\";");
            self.write_line();
        }
    }

    /// Emit auto-generated imports for foreign symbols.
    ///
    /// This should be called before emitting other declarations to ensure
    /// imports appear at the top of the .d.ts file.
    pub(crate) fn emit_auto_imports(&mut self) {
        let modules = std::mem::take(&mut self.import_plan.auto_generated);
        self.emit_import_modules(&modules);
        self.import_plan.auto_generated = modules;
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

                if has_initializer && printed_type_text.contains("any") {
                    self.maybe_emit_non_portable_function_return_diagnostic(decl_name, initializer);
                }

                // TS2883: Check for non-portable inferred type references
                if let Some(name_text) = self.get_identifier_text(decl_name)
                    && let Some(name_node) = self.arena.get(decl_name)
                    && let Some(file_path) = self.current_file_path.clone()
                {
                    let diagnostics_before = self.diagnostics.len();
                    if has_initializer {
                        self.maybe_emit_non_portable_function_return_diagnostic(
                            decl_name,
                            initializer,
                        );
                    }
                    let mut ran_symbol_check = false;
                    if self.diagnostics.len() == diagnostics_before
                        && !self.type_text_is_directly_nameable_reference(&printed_type_text)
                    {
                        // Run the structural type check unconditionally —
                        // the printed type text may use local names instead
                        // of import("...") syntax, so the text-based guard
                        // can miss non-portable references that the deep
                        // type walker will find.
                        ran_symbol_check = true;
                        self.check_non_portable_type_references(
                            type_id,
                            &name_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        );
                    }
                    if !ran_symbol_check
                        && self.diagnostics.len() == diagnostics_before
                        && has_initializer
                        && printed_type_text.starts_with("import(\"")
                        && self.import_type_uses_private_package_subpath(&printed_type_text)
                    {
                        let _ = self.emit_non_portable_import_type_text_diagnostics(
                            &printed_type_text,
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

                if type_id == tsz_solver::types::TypeId::ANY
                    && has_initializer
                    && let Some(type_text) = self.data_view_new_expression_type_text(initializer)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else if has_initializer
                    && self.object_literal_prefers_syntax_type_text(initializer)
                    && let Some(type_text) =
                        self.rewrite_object_literal_computed_member_type_text(initializer, type_id)
                {
                    self.write(": ");
                    self.write(&type_text);
                } else if let Some(typeof_text) =
                    self.typeof_prefix_for_value_entity(initializer, has_initializer, Some(type_id))
                {
                    // Bare identifier referencing an enum/module → emit typeof
                    self.write(": ");
                    self.write(&typeof_text);
                } else if (type_id != tsz_solver::types::TypeId::ANY
                    || !self.initializer_is_new_expression(initializer))
                    && let Some(type_text) = self.preferred_expression_type_text(initializer)
                {
                    self.write(": ");
                    self.write(&type_text);
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

    fn data_view_new_expression_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
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

    fn data_view_backing_store_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
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

    fn initializer_is_new_expression(&self, initializer: NodeIndex) -> bool {
        let initializer = self.skip_parenthesized_non_null_and_comma(initializer);
        self.arena
            .get(initializer)
            .is_some_and(|node| node.kind == syntax_kind_ext::NEW_EXPRESSION)
    }

    fn synthetic_class_extends_alias_type_id(
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

    fn retain_synthetic_class_extends_alias_dependencies_for_statement(
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

    fn retain_synthetic_module_extends_alias_dependencies(&mut self, module_idx: NodeIndex) {
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

    fn retain_direct_type_symbols_for_public_api(&mut self, type_id: tsz_solver::TypeId) {
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
                .and_modify(|kind| *kind |= super::usage_analyzer::UsageKind::TYPE)
                .or_insert(super::usage_analyzer::UsageKind::TYPE);
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

    fn emit_direct_symbol_dependency_for_type(&mut self, type_id: tsz_solver::TypeId) {
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

    fn emit_function_initializer_type_annotation(
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

    fn maybe_emit_non_portable_function_return_diagnostic(
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

    fn declared_return_identifier_type_name(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        return_expr: NodeIndex,
    ) -> Option<String> {
        let identifier_idx = self.return_expression_identifier(return_expr)?;
        let type_text = self.function_return_identifier_type_text(func, identifier_idx)?;
        Self::simple_type_reference_name(&type_text)
    }

    fn function_return_identifier_type_text(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        identifier_idx: NodeIndex,
    ) -> Option<String> {
        self.reference_declared_type_annotation_text(identifier_idx)
            .or_else(|| self.function_parameter_type_text(func, identifier_idx))
    }

    fn function_return_identifier_declared_type_id(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
        identifier_idx: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        self.reference_declared_type_id(identifier_idx)
            .or_else(|| self.function_parameter_type_id(func, identifier_idx))
    }

    fn function_parameter_type_text(
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

    fn function_parameter_type_id(
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

    fn reference_declared_type_id(&self, expr_idx: NodeIndex) -> Option<tsz_solver::types::TypeId> {
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

    fn simple_type_reference_name(type_text: &str) -> Option<String> {
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

    fn emit_recursive_function_initializer_type(
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

    fn emit_function_initializer_signature(
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

    fn function_initializer_has_inline_parameter_comments(&self, initializer: NodeIndex) -> bool {
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

    fn function_initializer_is_self_returning(&self, initializer: NodeIndex) -> bool {
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

    fn function_initializer_returns_unique_identifier(&self, initializer: NodeIndex) -> bool {
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

    pub(super) fn refine_invokable_return_type_from_identifier(
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

    pub(super) fn function_body_returns_identifier(&self, body_idx: NodeIndex, name: &str) -> bool {
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

    pub(super) fn emit_js_returned_define_property_function_type(
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

    fn function_body_unique_return_identifier(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
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

    fn function_body_single_return_expression(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
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

    fn collect_unique_return_identifier_from_block(
        &self,
        statements: &NodeList,
        returned_identifier: &mut Option<NodeIndex>,
    ) -> bool {
        statements.nodes.iter().copied().all(|stmt_idx| {
            self.collect_unique_return_identifier_from_statement(stmt_idx, returned_identifier)
        })
    }

    fn collect_unique_return_identifier_from_statement(
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
                    ) && !if_data.else_statement.is_none()
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
                    ) && !try_data.catch_clause.is_none()
                        && self.collect_unique_return_identifier_from_statement(
                            try_data.catch_clause,
                            returned_identifier,
                        )
                        && !try_data.finally_block.is_none()
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

    fn js_returned_define_property_function_info(
        &self,
        body_idx: NodeIndex,
    ) -> Option<(NodeIndex, Vec<JsDefinedPropertyDecl>)> {
        if !self.source_is_js_file {
            return None;
        }

        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        let returned_identifier = self.function_body_unique_return_identifier(body_idx)?;
        let returned_name = self.get_identifier_text(returned_identifier)?;

        let mut initializer = None;
        let mut properties = Vec::new();

        for stmt_idx in block.statements.nodes.iter().copied() {
            if initializer.is_none() {
                initializer = self.js_function_initializer_for_statement(stmt_idx, &returned_name);
            }
            if let Some(property) =
                self.js_define_property_decl_for_statement(stmt_idx, &returned_name)
            {
                properties.push(property);
            }
        }

        initializer
            .filter(|_| !properties.is_empty())
            .map(|init| (init, properties))
    }

    fn js_function_initializer_for_statement(
        &self,
        stmt_idx: NodeIndex,
        returned_name: &str,
    ) -> Option<NodeIndex> {
        let stmt_node = self.arena.get(stmt_idx)?;
        let variable = self.arena.get_variable(stmt_node)?;
        let decl_list_node = self.arena.get(variable.declarations.nodes[0])?;
        let decl_list = self.arena.get_variable(decl_list_node)?;

        decl_list
            .declarations
            .nodes
            .iter()
            .copied()
            .find_map(|decl_idx| {
                let decl_node = self.arena.get(decl_idx)?;
                let decl = self.arena.get_variable_declaration(decl_node)?;
                if self.get_identifier_text(decl.name).as_deref() != Some(returned_name) {
                    return None;
                }
                let init_node = self.arena.get(decl.initializer)?;
                if init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                {
                    Some(decl.initializer)
                } else {
                    None
                }
            })
    }

    fn js_define_property_decl_for_statement(
        &self,
        stmt_idx: NodeIndex,
        returned_name: &str,
    ) -> Option<JsDefinedPropertyDecl> {
        let stmt_node = self.arena.get(stmt_idx)?;
        let expr_stmt = self.arena.get_expression_statement(stmt_node)?;
        let expr_node = self.arena.get(expr_stmt.expression)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        if !self.is_object_define_property_call(call.expression) {
            return None;
        }
        let args = call.arguments.as_ref()?;
        if args.nodes.len() != 3 {
            return None;
        }
        if self.get_identifier_text(args.nodes[0]).as_deref() != Some(returned_name) {
            return None;
        }

        let name = self.js_define_property_name(args.nodes[1])?;
        let (mut type_text, readonly) = self.js_define_property_descriptor(args.nodes[2])?;
        if name == "name" && type_text == "any" {
            type_text = "string".to_string();
        }
        Some(JsDefinedPropertyDecl {
            name,
            type_text,
            readonly,
        })
    }

    fn is_object_define_property_call(&self, expr_idx: NodeIndex) -> bool {
        let expr_node = match self.arena.get(expr_idx) {
            Some(node) => node,
            None => return false,
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let access = match self.arena.get_access_expr(expr_node) {
            Some(access) => access,
            None => return false,
        };
        if self.get_identifier_text(access.name_or_argument).as_deref() != Some("defineProperty") {
            return false;
        }
        self.get_identifier_text(access.expression).as_deref() == Some("Object")
    }

    pub(crate) fn is_object_assign_call(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        if self.get_identifier_text(access.name_or_argument).as_deref() != Some("assign") {
            return false;
        }
        self.get_identifier_text(access.expression).as_deref() == Some("Object")
    }

    fn js_define_property_name(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if let Some(identifier) = self.arena.get_identifier(expr_node) {
            return Some(identifier.escaped_text.clone());
        }
        self.arena
            .get_literal(expr_node)
            .map(|literal| literal.text.clone())
    }

    fn js_define_property_descriptor(&self, expr_idx: NodeIndex) -> Option<(String, bool)> {
        let expr_node = self.arena.get(expr_idx)?;
        let object = self.arena.get_literal_expr(expr_node)?;
        let mut value_expr = None;
        let mut writable = false;

        for member_idx in object.elements.nodes.iter().copied() {
            let member_node = self.arena.get(member_idx)?;
            let assignment = self.arena.get_property_assignment(member_node)?;
            let name = self.js_define_property_name(assignment.name)?;
            match name.as_str() {
                "value" => value_expr = Some(assignment.initializer),
                "writable" => {
                    writable = self
                        .arena
                        .get(assignment.initializer)
                        .is_some_and(|init_node| init_node.kind == SyntaxKind::TrueKeyword as u16);
                }
                _ => {}
            }
        }

        let value_expr = value_expr?;
        let type_text = self
            .get_node_type_or_names(&[value_expr])
            .filter(|type_id| *type_id != tsz_solver::types::TypeId::ANY)
            .map(|type_id| self.print_type_id(type_id))
            .or_else(|| self.js_string_concatenation_type_text(value_expr))
            .or_else(|| self.infer_fallback_type_text(value_expr))
            .unwrap_or_else(|| "any".to_string());
        Some((type_text, !writable))
    }

    fn js_string_concatenation_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::PlusToken as u16 {
            return None;
        }
        if self.js_expression_is_string_like(binary.left)
            || self.js_expression_is_string_like(binary.right)
        {
            Some("string".to_string())
        } else {
            None
        }
    }

    fn js_expression_is_string_like(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind == SyntaxKind::StringLiteral as u16
            || expr_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            || expr_node.kind == syntax_kind_ext::TEMPLATE_EXPRESSION
        {
            return true;
        }
        self.js_string_concatenation_type_text(expr_idx).is_some()
    }

    fn emit_function_initializer_call_signature(&mut self, initializer: NodeIndex) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
            && init_node.kind != syntax_kind_ext::ARROW_FUNCTION
        {
            return false;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return false;
        };

        if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }
        self.write("(");
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.write("): ");

        if func.type_annotation.is_some() {
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = self.jsdoc_return_type_text_for_node(initializer) {
            self.write(&return_type_text);
        } else if let Some(interner) = self.type_interner
            && let Some(type_id) = self.get_node_type_or_names(&[initializer])
            && let Some(return_type_id) =
                tsz_solver::type_queries::get_return_type(interner, type_id)
        {
            self.write(&self.print_type_id(return_type_id));
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write("void");
        } else {
            self.write("any");
        }

        true
    }

    fn declaration_property_name_text(&self, name: &str) -> String {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return "\"\"".to_string();
        };
        let needs_quotes = !(first == '_' || first == '$' || first.is_ascii_alphabetic())
            || chars.any(|ch| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()));
        if needs_quotes {
            format!("\"{}\"", escape_string_for_double_quote(name))
        } else {
            name.to_string()
        }
    }

    fn statement_returns_identifier(&self, stmt_idx: NodeIndex, name: &str) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => self
                .arena
                .get_return_statement(stmt_node)
                .and_then(|ret| self.return_expression_is_identifier(ret.expression, name))
                .unwrap_or(false),
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    block
                        .statements
                        .nodes
                        .iter()
                        .copied()
                        .any(|child| self.statement_returns_identifier(child, name))
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => self
                .arena
                .get_if_statement(stmt_node)
                .is_some_and(|if_data| {
                    self.statement_returns_identifier(if_data.then_statement, name)
                        || (!if_data.else_statement.is_none()
                            && self.statement_returns_identifier(if_data.else_statement, name))
                }),
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.arena.get_try(stmt_node).is_some_and(|try_data| {
                    self.statement_returns_identifier(try_data.try_block, name)
                        || (!try_data.catch_clause.is_none()
                            && self.statement_returns_identifier(try_data.catch_clause, name))
                        || (!try_data.finally_block.is_none()
                            && self.statement_returns_identifier(try_data.finally_block, name))
                })
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => self
                .arena
                .get_catch_clause(stmt_node)
                .is_some_and(|catch_data| {
                    self.statement_returns_identifier(catch_data.block, name)
                }),
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => self
                .arena
                .get_case_clause(stmt_node)
                .is_some_and(|case_data| {
                    case_data
                        .statements
                        .nodes
                        .iter()
                        .copied()
                        .any(|child| self.statement_returns_identifier(child, name))
                }),
            _ => false,
        }
    }

    fn return_expression_identifier(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            return Some(expr_idx);
        }
        if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return self
                .arena
                .get_parenthesized(expr_node)
                .and_then(|paren| self.return_expression_identifier(paren.expression));
        }
        None
    }

    fn return_expression_is_identifier(&self, expr_idx: NodeIndex, name: &str) -> Option<bool> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            return Some(
                self.get_identifier_text(expr_idx)
                    .is_some_and(|text| text == name),
            );
        }
        if expr_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return self
                .arena
                .get_parenthesized(expr_node)
                .and_then(|paren| self.return_expression_is_identifier(paren.expression, name));
        }
        Some(false)
    }

    fn type_has_visible_declaration_members(&self, type_id: tsz_solver::types::TypeId) -> bool {
        let Some(interner) = self.type_interner else {
            return false;
        };

        if let Some(shape_id) = tsz_solver::visitor::object_shape_id(interner, type_id)
            .or_else(|| tsz_solver::visitor::object_with_index_shape_id(interner, type_id))
        {
            let shape = interner.object_shape(shape_id);
            return shape.string_index.is_some()
                || shape.number_index.is_some()
                || shape.properties.iter().any(|property| {
                    let name = interner.resolve_atom(property.name);
                    name != "prototype" && !name.starts_with("__private_brand_")
                });
        }

        if let Some(shape_id) = tsz_solver::visitor::callable_shape_id(interner, type_id) {
            let shape = interner.callable_shape(shape_id);
            return shape.string_index.is_some()
                || shape.number_index.is_some()
                || shape.properties.iter().any(|property| {
                    let name = interner.resolve_atom(property.name);
                    name != "prototype" && !name.starts_with("__private_brand_")
                });
        }

        if let Some(list_id) = tsz_solver::visitor::intersection_list_id(interner, type_id) {
            return interner
                .type_list(list_id)
                .iter()
                .copied()
                .any(|member| self.type_has_visible_declaration_members(member));
        }

        if let Some(inner) = tsz_solver::visitor::readonly_inner_type(interner, type_id)
            .or_else(|| tsz_solver::visitor::no_infer_inner_type(interner, type_id))
        {
            return self.type_has_visible_declaration_members(inner);
        }

        false
    }

    fn parameter_has_leading_inline_block_comment(&self, param_pos: u32) -> bool {
        let Some(ref text) = self.source_file_text else {
            return false;
        };
        let bytes = text.as_bytes();
        let mut actual_start = param_pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }
        let actual_start = actual_start as u32;

        for comment in &self.all_comments {
            if comment.end > actual_start {
                break;
            }
            let c_pos = comment.pos as usize;
            let c_end = comment.end as usize;
            let ct = &text[c_pos..c_end];
            if !ct.starts_with("/*") {
                continue;
            }

            let mut p = c_pos;
            let mut leading = true;
            while p > 0 {
                p -= 1;
                match bytes[p] {
                    b' ' | b'\t' | b'\r' | b'\n' => continue,
                    b'(' | b',' | b'[' | b'<' => break,
                    b'/' if p > 0 && bytes[p - 1] == b'*' => break,
                    _ => {
                        leading = false;
                        break;
                    }
                }
            }

            if leading {
                return true;
            }
        }

        false
    }

    /// Check if a type should be printed with a `typeof` prefix because the
    /// initializer is a bare identifier referencing a value-space entity (enum,
    /// module, function). Returns `Some("typeof Name")` if so, `None` otherwise.
    ///
    /// In tsc, `var x = E` (where E is an enum) emits `declare var x: typeof E;`
    /// because the variable holds the enum's runtime VALUE, not its TYPE meaning.
    pub(crate) fn typeof_prefix_for_value_entity(
        &self,
        initializer: NodeIndex,
        has_initializer: bool,
        type_id: Option<tsz_solver::types::TypeId>,
    ) -> Option<String> {
        if !has_initializer {
            return None;
        }
        let init_node = self.arena.get(initializer)?;
        let interner = self.type_interner?;

        if let Some(typeof_text) = self.direct_value_reference_typeof_text(initializer) {
            return Some(typeof_text);
        }

        if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(init_node)?;
            let rhs = self.get_identifier_text(access.name_or_argument)?;
            let lhs = self.nameable_constructor_expression_text(access.expression)?;
            if self
                .value_reference_symbol_needs_typeof(access.name_or_argument)
                .or_else(|| self.value_reference_symbol_needs_typeof(initializer))
                .unwrap_or(false)
            {
                return Some(format!("typeof {lhs}.{rhs}"));
            }
            let tid = type_id?;
            let is_callable = tsz_solver::visitor::function_shape_id(interner, tid).is_some()
                || tsz_solver::visitor::callable_shape_id(interner, tid).is_some();
            if !is_callable {
                return None;
            }
            let base_type = self.get_node_type_or_names(&[access.expression]);
            let is_constructor_like = base_type.is_some_and(|base_type| {
                if let Some(shape_id) = tsz_solver::visitor::callable_shape_id(interner, base_type)
                {
                    return !interner
                        .callable_shape(shape_id)
                        .construct_signatures
                        .is_empty();
                }
                tsz_solver::visitor::function_shape_id(interner, base_type)
                    .is_some_and(|shape_id| interner.function_shape(shape_id).is_constructor)
            });
            if is_constructor_like {
                return Some(format!("typeof {lhs}.{rhs}"));
            }
            let binder = self.binder?;
            let base_sym_id = binder.get_node_symbol(access.expression)?;
            let symbol = binder.symbols.get(base_sym_id)?;
            if symbol.flags
                & (tsz_binder::symbol_flags::ENUM | tsz_binder::symbol_flags::VALUE_MODULE)
                != 0
            {
                return Some(format!("typeof {lhs}.{rhs}"));
            }
            return None;
        }

        if init_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let identifier_name = self.get_identifier_text(initializer)?;

        if self
            .value_reference_symbol_needs_typeof(initializer)
            .unwrap_or(false)
        {
            return Some(format!("typeof {identifier_name}"));
        }

        // Check if the type is an Enum type — this means the initializer is
        // referencing the enum value directly (e.g., `var x = E`)
        if let Some(tid) = type_id
            && let Some((def_id, _members_id)) = tsz_solver::visitor::enum_components(interner, tid)
        {
            // Verify the enum name matches the identifier to avoid
            // false positives with enum member types
            if let Some(cache) = &self.type_cache
                && let Some(&sym_id) = cache.def_to_symbol.get(&def_id)
                && let Some(binder) = self.binder
                && let Some(symbol) = binder.symbols.get(sym_id)
                && symbol.escaped_name == identifier_name
                && symbol.flags & tsz_binder::symbol_flags::ENUM != 0
                && symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER == 0
            {
                return Some(format!("typeof {identifier_name}"));
            }
        }

        // For Lazy(DefId) types pointing to VALUE_MODULE/FUNCTION, the printer
        // already handles the typeof prefix in print_lazy_type.
        None
    }

    fn direct_value_reference_typeof_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let binder = self.binder?;
        let sym_id = binder
            .get_node_symbol(expr_idx)
            .or_else(|| self.value_reference_symbol(expr_idx))?;
        let symbol = binder.symbols.get(sym_id)?;
        if !(symbol.has_any_flags(
            symbol_flags::FUNCTION
                | symbol_flags::CLASS
                | symbol_flags::ENUM
                | symbol_flags::VALUE_MODULE
                | symbol_flags::METHOD,
        ) || self.is_namespace_import_alias_symbol(sym_id))
            || symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
        {
            return None;
        }

        let reference_text = self
            .nameable_constructor_expression_text(expr_idx)
            .or_else(|| self.get_identifier_text(expr_idx))?;
        Some(format!("typeof {reference_text}"))
    }

    fn value_reference_symbol_needs_typeof(&self, expr_idx: NodeIndex) -> Option<bool> {
        let binder = self.binder?;
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let symbol = binder.symbols.get(sym_id)?;
        Some(
            (symbol.has_any_flags(
                symbol_flags::FUNCTION
                    | symbol_flags::CLASS
                    | symbol_flags::ENUM
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::METHOD,
            ) || self.is_namespace_import_alias_symbol(sym_id))
                && !symbol.has_any_flags(symbol_flags::ENUM_MEMBER),
        )
    }

    fn value_reference_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let binder = self.binder?;
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            let ident = self.get_identifier_text(expr_idx)?;
            return self.resolve_identifier_symbol(expr_idx, &ident);
        }
        if expr_node.kind == SyntaxKind::ThisKeyword as u16 {
            return self.resolve_enclosing_class_symbol(expr_idx);
        }
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(expr_node)?;
            if let Some(sym_id) = binder.get_node_symbol(access.name_or_argument) {
                return Some(sym_id);
            }
            let base_sym_id = self.value_reference_symbol(access.expression)?;
            let resolved_base_sym_id = self.resolve_portability_symbol(base_sym_id, binder);
            let base_symbol = binder.symbols.get(resolved_base_sym_id)?;
            let member_name = self.get_identifier_text(access.name_or_argument)?;
            // Try exports first (for namespaces, static class members via class name)
            if let Some(sym_id) = base_symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(&member_name))
            {
                return Some(sym_id);
            }
            // Also try members (for class instance members via `this`)
            if let Some(sym_id) = base_symbol
                .members
                .as_ref()
                .and_then(|members| members.get(&member_name))
            {
                return Some(sym_id);
            }
            if let Some(sym_id) = binder.get_node_symbol(expr_idx) {
                return Some(sym_id);
            }
            return None;
        }
        binder.get_node_symbol(expr_idx)
    }

    /// Resolve `this` to the innermost enclosing class symbol by position.
    fn resolve_enclosing_class_symbol(&self, this_idx: NodeIndex) -> Option<SymbolId> {
        let binder = self.binder?;
        let this_node = self.arena.get(this_idx)?;
        let this_pos = this_node.pos;

        let mut best: Option<(SymbolId, u32)> = None; // (sym_id, span_size)
        for sym in binder.symbols.iter() {
            if (sym.flags & tsz_binder::symbol_flags::CLASS) == 0 {
                continue;
            }
            for &decl_idx in &sym.declarations {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                if this_pos >= decl_node.pos && this_pos < decl_node.end {
                    let span = decl_node.end - decl_node.pos;
                    if best.is_none_or(|(_, best_span)| span < best_span) {
                        best = Some((sym.id, span));
                    }
                }
            }
        }
        best.map(|(id, _)| id)
    }

    /// Get the text of an identifier node.
    pub(crate) fn get_identifier_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        self.arena
            .get_identifier(node)
            .map(|id| id.escaped_text.clone())
    }

    /// Format a literal value as an initializer string for `const` declarations in .d.ts.
    ///
    /// Produces the value form used in `declare const x = "abc"` style declarations.
    pub(crate) fn format_literal_initializer(
        lit: &tsz_solver::types::LiteralValue,
        interner: &tsz_solver::TypeInterner,
    ) -> String {
        match lit {
            tsz_solver::types::LiteralValue::String(atom) => {
                format!(
                    "\"{}\"",
                    escape_string_for_double_quote(&interner.resolve_atom(*atom))
                )
            }
            tsz_solver::types::LiteralValue::Number(n) => Self::format_js_number(n.0),
            tsz_solver::types::LiteralValue::Boolean(b) => b.to_string(),
            tsz_solver::types::LiteralValue::BigInt(atom) => {
                format!("{}n", interner.resolve_atom(*atom))
            }
        }
    }

    fn js_special_initializer_type_text(&self, initializer: NodeIndex) -> Option<String> {
        let init_node = self.arena.get(initializer)?;

        if self.is_import_meta_url_expression(initializer) {
            return Some("string".to_string());
        }

        if init_node.kind == syntax_kind_ext::AWAIT_EXPRESSION {
            let await_expr = self.arena.get_unary_expr_ex(init_node)?;
            return self.js_literal_type_text(await_expr.expression);
        }

        None
    }

    fn initializer_uses_inaccessible_class_constructor(&self, initializer: NodeIndex) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return false;
        }

        let Some(new_expr) = self.arena.get_call_expr(init_node) else {
            return false;
        };
        let Some(sym_id) = self.new_expression_target_symbol(new_expr.expression) else {
            return false;
        };

        self.symbol_has_inaccessible_constructor(sym_id)
    }

    fn new_expression_target_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let binder = self.binder?;
        if let Some(sym_id) = binder.get_node_symbol(expr_idx) {
            return Some(sym_id);
        }

        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .get_identifier_text(expr_idx)
                .and_then(|name| binder.file_locals.get(&name)),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                let access = self.arena.get_access_expr(expr_node)?;
                binder.get_node_symbol(access.name_or_argument)
            }
            _ => None,
        }
    }

    fn symbol_has_inaccessible_constructor(&self, sym_id: SymbolId) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };

        symbol.declarations.iter().copied().any(|decl_idx| {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                return false;
            };
            let Some(class_decl) = self.arena.get_class(decl_node) else {
                return false;
            };

            class_decl.members.nodes.iter().copied().any(|member_idx| {
                let Some(member_node) = self.arena.get(member_idx) else {
                    return false;
                };
                if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                    return false;
                }
                let Some(ctor) = self.arena.get_constructor(member_node) else {
                    return false;
                };
                self.arena
                    .has_modifier(&ctor.modifiers, SyntaxKind::PrivateKeyword)
                    || self
                        .arena
                        .has_modifier(&ctor.modifiers, SyntaxKind::ProtectedKeyword)
            })
        })
    }

    fn js_literal_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == SyntaxKind::StringLiteral as u16 => self
                .arena
                .get_literal(expr_node)
                .map(|lit| format!("\"{}\"", escape_string_for_double_quote(&lit.text))),
            k if k == SyntaxKind::NumericLiteral as u16 => {
                self.arena.get_literal(expr_node).map(|lit| {
                    let text = &lit.text;
                    // Strip numeric separators (tsc strips them in .d.ts output)
                    if text.contains('_') {
                        if let Some(v) = lit.value {
                            if v.fract() == 0.0 && v.abs() < 1e20 {
                                return format!("{}", v as i64);
                            }
                            return v.to_string();
                        }
                        return text.replace('_', "");
                    }
                    // For large numbers (21+ digits), parse as f64 and format
                    // using JS Number.toString() semantics (scientific notation).
                    let digits = text.chars().filter(|c| c.is_ascii_digit()).count();
                    if digits >= 21
                        && let Ok(n) = text.parse::<f64>()
                    {
                        return Self::format_js_number(n);
                    }
                    text.clone()
                })
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                self.arena.get_literal(expr_node).map(|lit| {
                    // Strip numeric separators from bigint literals
                    if lit.text.contains('_') {
                        lit.text.replace('_', "")
                    } else {
                        lit.text.clone()
                    }
                })
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some("true".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => Some("false".to_string()),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && self.is_negative_literal(expr_node) =>
            {
                let raw = self.get_source_slice_no_semi(expr_node.pos, expr_node.end)?;
                // Strip numeric separators from negative literals (e.g., -1_000 → -1000)
                if raw.contains('_') {
                    Some(raw.replace('_', ""))
                } else {
                    Some(raw)
                }
            }
            _ => None,
        }
    }

    fn const_literal_initializer_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => self
                .arena
                .get_unary_expr_ex(expr_node)
                .and_then(|await_expr| self.const_literal_initializer_text(await_expr.expression)),
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => self
                .arena
                .get_parenthesized(expr_node)
                .and_then(|paren| self.const_literal_initializer_text(paren.expression)),
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16 =>
            {
                self.js_literal_type_text(expr_idx)
            }
            // Template literal without substitutions: `hello` → "hello"
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(expr_node) {
                    let escaped = escape_string_for_double_quote(&lit.text);
                    Some(format!("\"{escaped}\""))
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && self.is_negative_literal(expr_node) =>
            {
                let raw = self.get_source_slice_no_semi(expr_node.pos, expr_node.end)?;
                if raw.contains('_') {
                    Some(raw.replace('_', ""))
                } else {
                    Some(raw)
                }
            }
            _ => self.simple_enum_access_member_text(expr_idx),
        }
    }

    /// Like `const_literal_initializer_text` but also unwraps `as` and
    /// `satisfies` expressions to find the underlying literal.
    pub(super) fn const_literal_initializer_text_deep(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        // Try the normal path first
        if let Some(text) = self.const_literal_initializer_text(expr_idx) {
            return Some(text);
        }
        if let Some(text) = self.const_literal_identity_call_text(expr_idx) {
            return Some(text);
        }
        // Unwrap as/satisfies expressions
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind == syntax_kind_ext::AS_EXPRESSION
            || expr_node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(expr_node)?;
            return self.const_literal_initializer_text_deep(assertion.expression);
        }
        None
    }

    fn const_literal_identity_call_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        let args = call.arguments.as_ref()?;
        if args.nodes.len() != 1 {
            return None;
        }

        let func = self.identity_returning_function(call.expression)?;
        let callee_body = func.body;
        let returned_identifier = self.function_body_unique_return_identifier(callee_body)?;
        let return_name = self.get_identifier_text(returned_identifier)?;
        let first_param_name = func
            .parameters
            .nodes
            .first()
            .copied()
            .and_then(|param_idx| self.arena.get(param_idx))
            .and_then(|param_node| self.arena.get_parameter(param_node))
            .and_then(|param| self.get_identifier_text(param.name))?;

        if first_param_name != return_name {
            return None;
        }

        let mut text = self.const_literal_initializer_text_deep(args.nodes[0])?;
        if text.starts_with('-') {
            while text.ends_with(')') {
                text.pop();
            }
        }
        Some(text)
    }

    fn identity_returning_function(
        &self,
        callee_idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::FunctionData> {
        let callee_name = self.get_identifier_text(callee_idx)?;
        let source_file_idx = self.current_source_file_idx?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .find_map(|decl_idx| {
                let decl_node = self.arena.get(decl_idx)?;
                let func = self.arena.get_function(decl_node)?;
                let same_name = self
                    .get_identifier_text(func.name)
                    .is_some_and(|name| name == callee_name);
                (same_name && func.body.is_some() && func.parameters.nodes.len() == 1)
                    .then_some(func)
            })
    }

    fn is_import_meta_url_expression(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        if self.get_identifier_text(access.name_or_argument).as_deref() != Some("url") {
            return false;
        }

        let Some(base_node) = self.arena.get(access.expression) else {
            return false;
        };
        if base_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(base_access) = self.arena.get_access_expr(base_node) else {
            return false;
        };
        if self
            .get_identifier_text(base_access.name_or_argument)
            .as_deref()
            != Some("meta")
        {
            return false;
        }

        self.arena
            .get(base_access.expression)
            .is_some_and(|node| node.kind == SyntaxKind::ImportKeyword as u16)
    }

    /// Format a f64 value as JavaScript would display it.
    ///
    /// Matches JS `Number.prototype.toString()` behavior:
    /// - Infinity/NaN → "Infinity"/"NaN"
    /// - Uses scientific notation for numbers with >= 21 integer digits
    /// - Uses scientific notation for very small numbers
    pub(crate) fn format_js_number(n: f64) -> String {
        if n.is_infinite() {
            if n.is_sign_positive() {
                "Infinity".to_string()
            } else {
                "-Infinity".to_string()
            }
        } else if n.is_nan() {
            "NaN".to_string()
        } else {
            let s = n.to_string();
            // Rust's default formatter doesn't use scientific notation for large
            // integers. JS switches to scientific notation when the integer part
            // has 21+ digits. Detect and convert.
            let abs_s = s.strip_prefix('-').unwrap_or(&s);
            let needs_scientific = if let Some(dot_pos) = abs_s.find('.') {
                dot_pos >= 21
            } else {
                abs_s.len() >= 21
            };
            if needs_scientific {
                Self::format_js_scientific(n)
            } else {
                s
            }
        }
    }

    /// Format a number in JavaScript-style scientific notation (e.g., `1.2345678912345678e+53`).
    fn format_js_scientific(n: f64) -> String {
        let neg = n < 0.0;
        let abs_n = n.abs();
        // Use Rust's {:e} format which gives e.g. "1.2345678912345678e53"
        let s = format!("{abs_n:e}");
        // JS uses e+N for positive exponents, e-N for negative
        let result = if let Some(pos) = s.find('e') {
            let (mantissa, exp_part) = s.split_at(pos);
            let exp_str = &exp_part[1..]; // skip 'e'
            if exp_str.starts_with('-') {
                format!("{mantissa}e{exp_str}")
            } else {
                format!("{mantissa}e+{exp_str}")
            }
        } else {
            s
        };
        if neg { format!("-{result}") } else { result }
    }

    /// Normalize a numeric literal string through f64, matching tsc's JS round-trip behavior.
    /// E.g., `123456789123456789123456789123456789123456789123456789` → `1.2345678912345678e+53`
    pub(crate) fn normalize_numeric_literal(text: &str) -> String {
        if let Ok(val) = text.parse::<f64>() {
            let normalized = Self::format_js_number(val);
            if normalized != text {
                return normalized;
            }
        }
        text.to_string()
    }

    /// Emit required imports at the beginning of the .d.ts file.
    ///
    /// This should be called before emitting other declarations.
    pub(crate) fn emit_required_imports(&mut self) {
        if self.import_plan.required.is_empty() {
            debug!("[DEBUG] emit_required_imports: no required imports");
            return;
        }

        let modules = std::mem::take(&mut self.import_plan.required);
        self.emit_import_modules(&modules);
        self.import_plan.required = modules;
    }

    // =========================================================================
    // TS2883: Non-portable inferred type references
    // =========================================================================

    /// Check if an inferred type references symbols from non-portable module paths
    /// (e.g., nested `node_modules` or private package subpaths).
    ///
    /// If non-portable references are found, emits TS2883 diagnostics.
    ///
    /// - `type_id`: the inferred type to check
    /// - `decl_name`: the declaration name (e.g., "x", "default", "special")
    /// - `file`: the source file path for the diagnostic
    /// - `pos`: the position of the declaration name in source
    /// - `length`: the length of the declaration name in source
    pub(crate) fn check_non_portable_type_references(
        &mut self,
        type_id: tsz_solver::types::TypeId,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) {
        if self.skip_portability_check {
            return;
        }

        // First, detect non-portable references (immutable borrow of self)
        let _ = self.emit_non_portable_type_diagnostic(type_id, decl_name, file, pos, length);
    }

    pub(crate) fn emit_non_portable_type_diagnostic(
        &mut self,
        type_id: tsz_solver::types::TypeId,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        use tsz_common::diagnostics::Diagnostic;

        let Some((from_path, type_name)) = self.find_non_portable_type_reference(type_id) else {
            return false;
        };

        self.diagnostics.push(Diagnostic::from_code(
            2883,
            file,
            pos,
            length,
            &[decl_name, &from_path, &type_name],
        ));
        true
    }

    pub(crate) fn emit_non_portable_expression_symbol_diagnostic(
        &mut self,
        expr_idx: NodeIndex,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if let Some(sym_id) = self.value_reference_symbol(expr_idx)
            && self.emit_non_portable_symbol_diagnostic(sym_id, decl_name, file, pos, length)
        {
            return true;
        }

        if let Some(sym_id) = self.value_reference_symbol(expr_idx)
            && self.emit_non_portable_symbol_initializer_diagnostic(
                sym_id, decl_name, file, pos, length,
            )
        {
            return true;
        }

        if let Some(sym_id) = self.value_reference_symbol(expr_idx)
            && self.emit_non_portable_symbol_declaration_diagnostic(
                sym_id, decl_name, file, pos, length,
            )
        {
            return true;
        }

        if expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && let Some(object) = self.arena.get_literal_expr(expr_node)
        {
            for &member_idx in &object.elements.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                        let Some(prop) = self.arena.get_property_assignment(member_node) else {
                            continue;
                        };
                        if self.emit_non_portable_expression_symbol_diagnostic(
                            prop.initializer,
                            decl_name,
                            file,
                            pos,
                            length,
                        ) {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                        let Some(prop) = self.arena.get_shorthand_property(member_node) else {
                            continue;
                        };
                        if self.emit_non_portable_expression_symbol_diagnostic(
                            prop.name, decl_name, file, pos, length,
                        ) || (prop.object_assignment_initializer.is_some()
                            && self.emit_non_portable_expression_symbol_diagnostic(
                                prop.object_assignment_initializer,
                                decl_name,
                                file,
                                pos,
                                length,
                            ))
                        {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }

        if expr_node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.arena.get_call_expr(expr_node)
            && self.emit_non_portable_expression_symbol_diagnostic(
                call.expression,
                decl_name,
                file,
                pos,
                length,
            )
        {
            return true;
        }

        if expr_node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
            && let Some(tagged) = self.arena.get_tagged_template(expr_node)
            && self.emit_non_portable_expression_symbol_diagnostic(
                tagged.tag, decl_name, file, pos, length,
            )
        {
            return true;
        }

        false
    }

    fn emit_non_portable_symbol_initializer_diagnostic(
        &mut self,
        sym_id: SymbolId,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };
        let Some(source_arena) = binder.symbol_arenas.get(&sym_id) else {
            return false;
        };

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = source_arena.get(decl_idx) else {
                continue;
            };
            if let Some(var_decl) = source_arena.get_variable_declaration(decl_node)
                && var_decl.initializer.is_some()
            {
                if let Some(type_id) = self
                    .get_node_type_or_names(&[var_decl.initializer])
                    .or_else(|| self.get_type_via_symbol(var_decl.initializer))
                    && self.emit_non_portable_type_diagnostic(type_id, decl_name, file, pos, length)
                {
                    return true;
                }
                if self.emit_non_portable_expression_symbol_diagnostic(
                    var_decl.initializer,
                    decl_name,
                    file,
                    pos,
                    length,
                ) {
                    return true;
                }
            }
        }

        false
    }

    fn emit_non_portable_symbol_declaration_diagnostic(
        &mut self,
        sym_id: SymbolId,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        let references = self.collect_non_portable_references_in_symbol_declaration(sym_id);
        if references.is_empty() {
            return false;
        }

        for (from_path, type_name) in references {
            self.emit_non_portable_named_reference_diagnostic(
                decl_name, file, pos, length, &from_path, &type_name,
            );
        }
        true
    }

    fn collect_non_portable_references_in_symbol_declaration(
        &self,
        sym_id: SymbolId,
    ) -> Vec<(String, String)> {
        let resolved_sym = if let Some(binder) = self.binder {
            self.resolve_portability_import_alias(sym_id, binder)
                .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder))
        } else {
            sym_id
        };
        let mut visited_symbols = rustc_hash::FxHashSet::default();
        let mut visited_declaration_symbols = rustc_hash::FxHashSet::default();
        let mut visited_nodes = rustc_hash::FxHashSet::default();
        let mut visited_types = rustc_hash::FxHashSet::default();
        let mut seen = rustc_hash::FxHashSet::default();
        let mut results = Vec::new();
        self.collect_non_portable_references_in_symbol_declaration_inner(
            resolved_sym,
            &mut results,
            &mut seen,
            &mut visited_types,
            &mut visited_symbols,
            &mut visited_declaration_symbols,
            &mut visited_nodes,
        );
        results
    }

    fn resolve_portability_import_alias(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<SymbolId> {
        let symbol = binder.symbols.get(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS) {
            return None;
        }

        let module_specifier = symbol.import_module.as_deref()?;
        let export_name = symbol
            .import_name
            .as_deref()
            .unwrap_or(symbol.escaped_name.as_str());
        let current_path = self.current_file_path.as_deref()?;

        for (module_path, exports) in &binder.module_exports {
            let candidate =
                if module_specifier.starts_with('.') || module_specifier.starts_with('/') {
                    Some(self.strip_ts_extensions(
                        &self.calculate_relative_path(current_path, module_path),
                    ))
                } else {
                    self.package_specifier_for_node_modules_path(current_path, module_path)
                };
            if candidate.as_deref() == Some(module_specifier)
                && let Some(resolved) = exports.get(export_name)
                && resolved != sym_id
            {
                return Some(resolved);
            }
        }

        if !module_specifier.starts_with('.') && !module_specifier.starts_with('/') {
            return binder.symbols.iter().find_map(|candidate| {
                if candidate.id == sym_id || candidate.escaped_name != export_name {
                    return None;
                }
                let source_path = self.get_symbol_source_path(candidate.id, binder)?;
                let package_specifier =
                    self.package_specifier_for_node_modules_path(current_path, &source_path)?;
                (package_specifier == module_specifier
                    || package_specifier.starts_with(&format!("{module_specifier}/")))
                .then_some(candidate.id)
            });
        }

        None
    }

    fn collect_non_portable_references_in_symbol_declaration_inner(
        &self,
        sym_id: SymbolId,
        results: &mut Vec<(String, String)>,
        seen: &mut rustc_hash::FxHashSet<(String, String)>,
        visited_types: &mut rustc_hash::FxHashSet<tsz_solver::types::TypeId>,
        visited_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_declaration_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_nodes: &mut rustc_hash::FxHashSet<(usize, u32)>,
    ) {
        let Some(binder) = self.binder else {
            return;
        };
        let resolved_sym = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        if !visited_declaration_symbols.insert(resolved_sym) {
            return;
        }
        let Some(symbol) = binder.symbols.get(resolved_sym) else {
            return;
        };
        let Some(source_arena) = binder.symbol_arenas.get(&resolved_sym) else {
            return;
        };
        let Some(source_path) = self.get_symbol_source_path(resolved_sym, binder) else {
            return;
        };

        if let Some(current_file_path) = self.current_file_path.as_deref()
            && let Some(result) = self.check_symbol_portability(
                resolved_sym,
                binder,
                current_file_path,
                visited_types,
                visited_symbols,
            )
            && seen.insert(result.clone())
        {
            results.push(result);
        }

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = source_arena.get(decl_idx) else {
                continue;
            };

            if let Some(alias) = source_arena.get_type_alias(decl_node) {
                self.collect_non_portable_references_in_type_node(
                    source_arena.as_ref(),
                    alias.type_node,
                    &source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }

            if let Some(function) = source_arena.get_function(decl_node) {
                self.collect_non_portable_references_in_type_node(
                    source_arena.as_ref(),
                    function.type_annotation,
                    &source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
                for &param_idx in &function.parameters.nodes {
                    self.collect_non_portable_references_in_type_node(
                        source_arena.as_ref(),
                        param_idx,
                        &source_path,
                        results,
                        seen,
                        visited_types,
                        visited_symbols,
                        visited_declaration_symbols,
                        visited_nodes,
                    );
                }
            }

            if let Some(interface) = source_arena.get_interface(decl_node) {
                if let Some(heritage) = &interface.heritage_clauses {
                    for &clause_idx in &heritage.nodes {
                        self.collect_non_portable_references_in_type_node(
                            source_arena.as_ref(),
                            clause_idx,
                            &source_path,
                            results,
                            seen,
                            visited_types,
                            visited_symbols,
                            visited_declaration_symbols,
                            visited_nodes,
                        );
                    }
                }
                for &member_idx in &interface.members.nodes {
                    self.collect_non_portable_references_in_type_node(
                        source_arena.as_ref(),
                        member_idx,
                        &source_path,
                        results,
                        seen,
                        visited_types,
                        visited_symbols,
                        visited_declaration_symbols,
                        visited_nodes,
                    );
                }
            }

            if let Some(sig) = source_arena.get_signature(decl_node) {
                self.collect_non_portable_references_in_type_node(
                    source_arena.as_ref(),
                    sig.type_annotation,
                    &source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }

            if let Some(func_type) = source_arena.get_function_type(decl_node) {
                self.collect_non_portable_references_in_type_node(
                    source_arena.as_ref(),
                    func_type.type_annotation,
                    &source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }

            if let Some(var_decl) = source_arena.get_variable_declaration(decl_node) {
                self.collect_non_portable_references_in_type_node(
                    source_arena.as_ref(),
                    var_decl.type_annotation,
                    &source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }

            if let Some(prop_decl) = source_arena.get_property_decl(decl_node) {
                self.collect_non_portable_references_in_type_node(
                    source_arena.as_ref(),
                    prop_decl.type_annotation,
                    &source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }

            if let Some(param) = source_arena.get_parameter(decl_node) {
                self.collect_non_portable_references_in_type_node(
                    source_arena.as_ref(),
                    param.type_annotation,
                    &source_path,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_non_portable_references_in_type_node(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        source_path: &str,
        results: &mut Vec<(String, String)>,
        seen: &mut rustc_hash::FxHashSet<(String, String)>,
        visited_types: &mut rustc_hash::FxHashSet<tsz_solver::types::TypeId>,
        visited_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_declaration_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_nodes: &mut rustc_hash::FxHashSet<(usize, u32)>,
    ) {
        let arena_addr = arena as *const NodeArena as usize;
        if !node_idx.is_some() || !visited_nodes.insert((arena_addr, node_idx.0)) {
            return;
        }
        let Some(node) = arena.get(node_idx) else {
            return;
        };

        if let Some(result) =
            self.non_portable_namespace_member_reference(arena, node_idx, source_path)
            && seen.insert(result.clone())
        {
            results.push(result);
        }

        if let Some(identifier) = arena.get_identifier(node) {
            let sym_id = self
                .binder
                .and_then(|binder| binder.get_node_symbol(node_idx))
                .or_else(|| self.find_symbol_in_arena_by_name(arena, &identifier.escaped_text));
            if let Some(sym_id) = sym_id {
                if let Some(binder) = self.binder
                    && let Some(current_file_path) = self.current_file_path.as_deref()
                    && let Some(result) = self.check_symbol_portability(
                        sym_id,
                        binder,
                        current_file_path,
                        visited_types,
                        visited_symbols,
                    )
                    && seen.insert(result.clone())
                {
                    results.push(result);
                }

                self.collect_non_portable_references_in_symbol_declaration_inner(
                    sym_id,
                    results,
                    seen,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                );
            }
        }

        for child_idx in arena.get_children(node_idx) {
            self.collect_non_portable_references_in_type_node(
                arena,
                child_idx,
                source_path,
                results,
                seen,
                visited_types,
                visited_symbols,
                visited_declaration_symbols,
                visited_nodes,
            );
        }
    }

    pub(crate) fn emit_non_portable_initializer_declaration_diagnostics(
        &mut self,
        expr_idx: NodeIndex,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        let Some(root_expr) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let mut current = root_expr;
        loop {
            let Some(node) = self.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::CALL_EXPRESSION {
                let Some(call) = self.arena.get_call_expr(node) else {
                    return false;
                };
                current = call.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
                let Some(tagged) = self.arena.get_tagged_template(node) else {
                    return false;
                };
                current = tagged.tag;
                continue;
            }
            break;
        }

        let Some(sym_id) = self.value_reference_symbol(current) else {
            return false;
        };
        self.emit_non_portable_symbol_declaration_diagnostic(sym_id, decl_name, file, pos, length)
    }

    fn emit_non_portable_import_type_text_diagnostics(
        &mut self,
        printed_type_text: &str,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        let Some(sym_id) = self.find_symbol_for_import_type_text(printed_type_text) else {
            return false;
        };
        let mut references = self.collect_non_portable_references_in_symbol_declaration(sym_id);
        if let Some(parsed_reference) = self.parse_import_type_text(printed_type_text)
            && !references.contains(&parsed_reference)
        {
            references.insert(0, parsed_reference);
        }
        if let Some(root_reference) =
            self.private_import_type_package_root_reference(printed_type_text)
            && !references.contains(&root_reference)
        {
            references.push(root_reference);
        }
        if references.is_empty() {
            return false;
        }
        for (from_path, type_name) in references {
            self.emit_non_portable_named_reference_diagnostic(
                decl_name, file, pos, length, &from_path, &type_name,
            );
        }
        true
    }

    fn find_symbol_for_import_type_text(&self, printed: &str) -> Option<SymbolId> {
        let (module_specifier, first_name) = self.parse_import_type_text(printed)?;
        let binder = self.binder?;
        let current_path = self.current_file_path.as_deref()?;

        binder.symbols.iter().find_map(|symbol| {
            if symbol.escaped_name != first_name {
                return None;
            }
            let source_arena = binder.symbol_arenas.get(&symbol.id)?;
            let arena_addr = Arc::as_ptr(source_arena) as usize;
            let source_path = self.arena_to_path.get(&arena_addr)?;
            let candidate = if module_specifier.starts_with('.')
                || module_specifier.starts_with('/')
            {
                self.strip_ts_extensions(&self.calculate_relative_path(current_path, source_path))
            } else {
                self.package_specifier_for_node_modules_path(current_path, source_path)?
            };
            (candidate == module_specifier).then_some(symbol.id)
        })
    }

    fn parse_import_type_text(&self, printed: &str) -> Option<(String, String)> {
        let rest = printed.strip_prefix("import(\"")?;
        let (module_specifier, tail) = rest.split_once("\")")?;
        let tail = tail.strip_prefix('.')?;
        let first_name = tail
            .split(['.', '<', '[', ' ', '&', '|'])
            .find(|part| !part.is_empty())?;
        Some((module_specifier.to_string(), first_name.to_string()))
    }

    fn private_import_type_package_root_reference(
        &self,
        printed: &str,
    ) -> Option<(String, String)> {
        let (module_specifier, type_name) = self.parse_import_type_text(printed)?;
        if module_specifier.starts_with('.') || module_specifier.starts_with('/') {
            return None;
        }

        let mut parts = module_specifier.split('/');
        let first = parts.next()?;
        if first.is_empty() {
            return None;
        }

        let package_name = if first.starts_with('@') {
            format!("{}/{}", first, parts.next()?)
        } else {
            first.to_string()
        };

        if package_name == module_specifier {
            return None;
        }

        Some((format!("./node_modules/{package_name}"), type_name))
    }

    fn non_portable_namespace_member_reference(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        source_path: &str,
    ) -> Option<(String, String)> {
        let node = arena.get(node_idx)?;
        let (left_idx, right_idx) = if let Some(access) = arena.get_access_expr(node) {
            (access.expression, access.name_or_argument)
        } else if let Some(qn) = arena.get_qualified_name(node) {
            (qn.left, qn.right)
        } else {
            return None;
        };

        let left_name = self.rightmost_name_text_in_arena(arena, left_idx)?;
        let type_name = self.rightmost_name_text_in_arena(arena, right_idx)?;
        if let Some(sym_id) = self.find_symbol_in_arena_by_name(arena, &left_name) {
            let binder = self.binder?;
            let symbol = binder.symbols.get(sym_id)?;
            if let Some(import_module) = symbol.import_module.as_deref() {
                if import_module.starts_with('.') || import_module.starts_with('/') {
                    return None;
                }
                let from_path =
                    self.transitive_dependency_from_import(source_path, import_module)?;
                return Some((from_path, type_name));
            }
        }

        let source_text = std::fs::read_to_string(source_path).ok()?;
        if let Some(import_module) =
            self.namespace_import_module_from_text(&source_text, &left_name)
        {
            if !import_module.starts_with('.') && !import_module.starts_with('/') {
                let from_path =
                    self.transitive_dependency_from_import(source_path, &import_module)?;
                return Some((from_path, type_name));
            }
        }

        self.reference_types_namespace_member_reference_from_text(
            &source_text,
            &left_name,
            &type_name,
        )
    }

    fn rightmost_name_text_in_arena(&self, arena: &NodeArena, idx: NodeIndex) -> Option<String> {
        let node = arena.get(idx)?;
        if let Some(ident) = arena.get_identifier(node) {
            return Some(ident.escaped_text.clone());
        }
        if let Some(qn) = arena.get_qualified_name(node) {
            return self.rightmost_name_text_in_arena(arena, qn.right);
        }
        if let Some(access) = arena.get_access_expr(node) {
            return self.rightmost_name_text_in_arena(arena, access.name_or_argument);
        }
        None
    }

    fn find_symbol_in_arena_by_name(&self, arena: &NodeArena, name: &str) -> Option<SymbolId> {
        let binder = self.binder?;
        let arena_addr = arena as *const NodeArena as usize;

        binder.symbols.iter().find_map(|symbol| {
            if symbol.escaped_name != name {
                return None;
            }
            let sym_arena = binder.symbol_arenas.get(&symbol.id)?;
            ((Arc::as_ptr(sym_arena) as usize) == arena_addr).then_some(symbol.id)
        })
    }

    fn transitive_dependency_from_import(
        &self,
        source_path: &str,
        import_module: &str,
    ) -> Option<String> {
        use std::path::{Component, Path};

        let components: Vec<_> = Path::new(source_path).components().collect();
        let nm_positions: Vec<usize> = components
            .iter()
            .enumerate()
            .filter_map(|(i, c)| match c {
                Component::Normal(part) if part.to_str() == Some("node_modules") => Some(i),
                _ => None,
            })
            .collect();
        let last_nm = *nm_positions.last()?;
        let pkg_start = last_nm + 1;
        let pkg_len = if components.get(pkg_start).is_some_and(
            |c| matches!(c, Component::Normal(p) if p.to_str().is_some_and(|s| s.starts_with('@'))),
        ) {
            2
        } else {
            1
        };
        let parent_package: Vec<String> = components[pkg_start..pkg_start + pkg_len]
            .iter()
            .filter_map(|c| match c {
                Component::Normal(part) => part.to_str().map(str::to_string),
                _ => None,
            })
            .collect();
        (!parent_package.is_empty()).then(|| {
            format!(
                "{}/node_modules/{}",
                parent_package.join("/"),
                import_module
            )
        })
    }

    fn reference_types_namespace_member_reference_from_text(
        &self,
        source_text: &str,
        left_name: &str,
        type_name: &str,
    ) -> Option<(String, String)> {
        let current_file_path = self.current_file_path.as_deref()?;

        for types_ref in self.extract_reference_types_from_text(&source_text) {
            if !types_ref.eq_ignore_ascii_case(left_name) {
                continue;
            }

            let binder = self.binder?;
            for module_path in binder.module_exports.keys() {
                let specifier =
                    self.package_specifier_for_node_modules_path(current_file_path, module_path)?;
                if specifier != types_ref {
                    continue;
                }

                let mut from_path = self.strip_ts_extensions(
                    &self.calculate_relative_path(current_file_path, module_path),
                );
                if from_path.ends_with("/index") {
                    from_path.truncate(from_path.len() - "/index".len());
                }
                return Some((from_path, type_name.to_string()));
            }
        }

        None
    }

    fn namespace_import_module_from_text(
        &self,
        source_text: &str,
        alias_name: &str,
    ) -> Option<String> {
        for line in source_text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("import * as ") {
                let (alias, rest) = rest.split_once(" from ")?;
                if alias.trim() != alias_name {
                    continue;
                }
                let module = rest.trim().trim_end_matches(';');
                return Self::quoted_string_text(module);
            }
            if let Some(rest) = trimmed.strip_prefix("import ")
                && let Some((alias, rhs)) = rest.split_once(" = require(")
            {
                if alias.trim() != alias_name {
                    continue;
                }
                let module = rhs.trim().trim_end_matches(");").trim_end_matches(')');
                return Self::quoted_string_text(module);
            }
        }
        None
    }

    fn quoted_string_text(text: &str) -> Option<String> {
        let trimmed = text.trim();
        let quote = trimmed.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let rest = &trimmed[quote.len_utf8()..];
        let end = rest.find(quote)?;
        Some(rest[..end].to_string())
    }

    fn extract_reference_types_from_text(&self, source_text: &str) -> Vec<String> {
        source_text
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if !trimmed.starts_with("///")
                    || !trimmed.contains("<reference")
                    || !trimmed.contains("types=")
                {
                    return None;
                }

                let attr_start = trimmed.find("types=")?;
                let after = &trimmed[attr_start + "types=".len()..];
                let quote = after.chars().next()?;
                if quote != '"' && quote != '\'' {
                    return None;
                }
                let rest = &after[quote.len_utf8()..];
                let end = rest.find(quote)?;
                Some(rest[..end].to_string())
            })
            .collect()
    }

    fn emit_non_portable_symbol_diagnostic(
        &mut self,
        sym_id: SymbolId,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        use tsz_common::diagnostics::Diagnostic;

        if self.skip_portability_check {
            return false;
        }

        let Some(binder) = self.binder else {
            return false;
        };
        let Some(current_file_path) = self.current_file_path.as_deref() else {
            return false;
        };
        let mut visited_types = rustc_hash::FxHashSet::default();
        let mut visited_symbols = rustc_hash::FxHashSet::default();
        let Some((from_path, type_name)) = self.check_symbol_portability(
            sym_id,
            binder,
            current_file_path,
            &mut visited_types,
            &mut visited_symbols,
        ) else {
            return false;
        };

        self.diagnostics.push(Diagnostic::from_code(
            2883,
            file,
            pos,
            length,
            &[decl_name, &from_path, &type_name],
        ));
        true
    }

    fn emit_non_portable_named_reference_diagnostic(
        &mut self,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
        from_path: &str,
        type_name: &str,
    ) {
        use tsz_common::diagnostics::Diagnostic;

        self.diagnostics.push(Diagnostic::from_code(
            2883,
            file,
            pos,
            length,
            &[decl_name, from_path, type_name],
        ));
    }

    fn type_text_is_directly_nameable_reference(&self, printed: &str) -> bool {
        if printed == "any" || printed.is_empty() {
            return false;
        }

        if printed.starts_with("import(\"") {
            return printed.contains("\").")
                && !self.import_type_uses_private_package_subpath(printed)
                && !printed.contains(" & ")
                && !printed.contains(" | ")
                && !printed.contains("{ ")
                && !printed.contains('[')
                && !printed.contains('\n');
        }

        let bytes = printed.as_bytes();
        let Some(&first) = bytes.first() else {
            return false;
        };
        if !matches!(first, b'A'..=b'Z' | b'a'..=b'z' | b'_') {
            return false;
        }

        !printed.contains(" & ")
            && !printed.contains(" | ")
            && !printed.contains("{ ")
            && !printed.contains('[')
            && !printed.contains('(')
            && !printed.contains('\n')
    }

    /// Check whether the printed type text contains any `import("...")` reference
    /// whose module specifier is a private package subpath (has a `/` after the
    /// bare package name).  This scans all `import("...")` occurrences in the
    /// text, not just the leading one.
    ///
    /// When the printed type text has NO such non-portable import references,
    /// the type is already nameable from the consumer's perspective and the
    /// deeper type-graph portability walk can be skipped.
    #[allow(dead_code)]
    fn printed_type_contains_non_portable_import(&self, printed: &str) -> bool {
        let mut remaining = printed;
        while let Some(start) = remaining.find("import(\"") {
            let after_prefix = &remaining[start + 8..]; // skip `import("`
            if let Some((specifier, rest)) = after_prefix.split_once("\")") {
                if !specifier.starts_with('.') && !specifier.starts_with('/') {
                    let mut parts = specifier.split('/');
                    if let Some(first) = parts.next() {
                        if !first.is_empty() {
                            let has_subpath = if first.starts_with('@') {
                                let _scope_pkg = parts.next();
                                parts.next().is_some()
                            } else {
                                parts.next().is_some()
                            };
                            if has_subpath
                                && !self.is_bare_specifier_subpath_publicly_accessible(specifier)
                            {
                                return true;
                            }
                        }
                    }
                }
                remaining = rest;
            } else {
                break;
            }
        }
        false
    }

    fn import_type_uses_private_package_subpath(&self, printed: &str) -> bool {
        let Some(rest) = printed.strip_prefix("import(\"") else {
            return false;
        };
        let Some((specifier, _)) = rest.split_once("\")") else {
            return false;
        };

        if specifier.starts_with('.') || specifier.starts_with('/') {
            return false;
        }

        let mut parts = specifier.split('/');
        let Some(first) = parts.next() else {
            return false;
        };
        if first.is_empty() {
            return false;
        }

        let has_subpath = if first.starts_with('@') {
            let _package = parts.next();
            parts.next().is_some()
        } else {
            parts.next().is_some()
        };

        has_subpath && !self.is_bare_specifier_subpath_publicly_accessible(specifier)
    }

    /// Check whether a bare package specifier with a subpath is publicly accessible.
    /// Returns `true` when the package has no `exports` field (all subpaths accessible)
    /// or the exports map explicitly maps the subpath.
    fn is_bare_specifier_subpath_publicly_accessible(&self, specifier: &str) -> bool {
        use std::path::Path;

        let mut parts = specifier.split('/');
        let Some(first) = parts.next() else {
            return false;
        };
        let (package_name, subpath) = if first.starts_with('@') {
            let scope_pkg = parts.next().unwrap_or("");
            let pkg_name = format!("{first}/{scope_pkg}");
            let rest: Vec<&str> = parts.collect();
            if rest.is_empty() {
                return false;
            }
            (pkg_name, rest.join("/"))
        } else {
            let rest: Vec<&str> = parts.collect();
            if rest.is_empty() {
                return false;
            }
            (first.to_string(), rest.join("/"))
        };

        let package_root = self.find_package_root_for_name(&package_name);
        let Some(package_root) = package_root else {
            return false;
        };

        let pkg_json_path = Path::new(&package_root).join("package.json");
        let Ok(pkg_content) = std::fs::read_to_string(&pkg_json_path) else {
            return false;
        };
        let Ok(pkg_json) = serde_json::from_str::<serde_json::Value>(&pkg_content) else {
            return false;
        };

        let Some(exports) = pkg_json.get("exports") else {
            // No exports field: all subpaths accessible.
            return true;
        };

        let export_subpath = format!("./{subpath}");
        self.exports_map_allows_subpath(exports, &export_subpath)
    }

    /// Find the filesystem path of a package root directory.
    fn find_package_root_for_name(&self, package_name: &str) -> Option<String> {
        let needle = format!("node_modules/{package_name}/");
        for source_path in self.arena_to_path.values() {
            if let Some(idx) = source_path.find(&needle) {
                return Some(source_path[..idx + needle.len() - 1].to_string());
            }
        }
        if let Some(binder) = self.binder {
            for module_path in binder.module_exports.keys() {
                if let Some(idx) = module_path.find(&needle) {
                    return Some(module_path[..idx + needle.len() - 1].to_string());
                }
            }
        }
        None
    }

    /// Check whether a package's exports map allows a given subpath.
    fn exports_map_allows_subpath(&self, exports: &serde_json::Value, subpath: &str) -> bool {
        match exports {
            serde_json::Value::String(target) => {
                subpath == "." || self.match_export_target(".", target, subpath).is_some()
            }
            serde_json::Value::Array(entries) => entries
                .iter()
                .any(|entry| self.exports_map_allows_subpath(entry, subpath)),
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if key == "." || key.starts_with("./") {
                        if self.export_entry_matches_subpath(key, value, subpath) {
                            return true;
                        }
                    } else if self.exports_map_allows_subpath(value, subpath) {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn export_entry_matches_subpath(
        &self,
        key: &str,
        value: &serde_json::Value,
        subpath: &str,
    ) -> bool {
        if key == subpath {
            return true;
        }
        if key.contains('*') && self.match_exports_wildcard(key, subpath).is_some() {
            return true;
        }
        if key.ends_with('/') && subpath.starts_with(key) {
            return true;
        }
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    if !k.starts_with("./") && k != "." {
                        // Condition key: recurse to check if any branch has a target
                        if self.export_entry_matches_subpath(key, v, subpath) {
                            return true;
                        }
                    }
                }
                false
            }
            serde_json::Value::Array(entries) => entries
                .iter()
                .any(|entry| self.export_entry_matches_subpath(key, entry, subpath)),
            _ => false,
        }
    }

    /// Scan a type for non-portable symbol references by checking all
    /// referenced types for symbols from nested `node_modules`.
    ///
    /// Returns `Some((from_path, type_name))` for the first non-portable reference found.
    fn find_non_portable_type_reference(
        &self,
        type_id: tsz_solver::types::TypeId,
    ) -> Option<(String, String)> {
        let mut visited_types = rustc_hash::FxHashSet::default();
        let mut visited_symbols = rustc_hash::FxHashSet::default();
        self.find_non_portable_type_reference_inner(
            type_id,
            &mut visited_types,
            &mut visited_symbols,
        )
    }

    fn find_non_portable_type_reference_inner(
        &self,
        type_id: tsz_solver::types::TypeId,
        visited_types: &mut rustc_hash::FxHashSet<tsz_solver::types::TypeId>,
        visited_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
    ) -> Option<(String, String)> {
        let interner = self.type_interner?;
        let binder = self.binder?;
        let current_file_path = self.current_file_path.as_deref()?;
        let cache = self.type_cache.as_ref()?;

        if !visited_types.insert(type_id) {
            return None;
        }

        // Collect all types referenced by this type (deeply walks into
        // objects, tuples, unions, intersections, etc.)
        let referenced_types = tsz_solver::visitor::collect_referenced_types(interner, type_id);
        for &ref_type_id in &referenced_types {
            // Check Lazy(DefId) types - these are named type references
            if let Some(def_id) = tsz_solver::lazy_def_id(interner, ref_type_id)
                && let Some(&sym_id) = cache.def_to_symbol.get(&def_id)
                && let Some(result) = self.check_symbol_portability(
                    sym_id,
                    binder,
                    current_file_path,
                    visited_types,
                    visited_symbols,
                )
            {
                return Some(result);
            }

            // Check object shapes with symbols - these are structural types
            // that may reference foreign symbols through their shape.symbol field
            if let Some(shape_id) = tsz_solver::object_shape_id(interner, ref_type_id) {
                let shape = interner.object_shape(shape_id);
                if let Some(sym_id) = shape.symbol
                    && let Some(result) = self.check_symbol_portability(
                        sym_id,
                        binder,
                        current_file_path,
                        visited_types,
                        visited_symbols,
                    )
                {
                    return Some(result);
                }
            }
        }

        None
    }

    /// Check if a symbol comes from a non-portable module path.
    ///
    /// Returns `Some((from_path, type_name))` if the symbol is non-portable, where:
    /// - `from_path` is the problematic module path for the diagnostic message
    /// - `type_name` is the symbol name that can't be referenced
    #[allow(clippy::too_many_arguments)]
    fn check_symbol_portability(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
        current_file_path: &str,
        visited_types: &mut rustc_hash::FxHashSet<tsz_solver::types::TypeId>,
        visited_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
    ) -> Option<(String, String)> {
        use std::path::{Component, Path};

        let sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        if !visited_symbols.insert(sym_id) {
            return None;
        }
        let symbol = binder.symbols.get(sym_id)?;
        let type_name = symbol.escaped_name.clone();
        let source_path = self.get_symbol_source_path(sym_id, binder)?;

        // If the symbol is re-exported from a module accessible via a bare
        // package specifier (no subpath), the type IS portable -- consumers
        // can reference it through the package root.  tsc does not emit
        // TS2883 in this situation.
        if self
            .package_root_export_reference_path(sym_id, &type_name, binder, current_file_path)
            .is_some()
        {
            return None;
        }

        // Parse node_modules segments from the source path
        let components: Vec<_> = Path::new(&source_path).components().collect();
        let nm_positions: Vec<usize> = components
            .iter()
            .enumerate()
            .filter_map(|(i, c)| match c {
                Component::Normal(part) if part.to_str() == Some("node_modules") => Some(i),
                _ => None,
            })
            .collect();

        // Case 1: Symbol is an import alias from a package in node_modules,
        // and the import specifier is a bare package name (not relative).
        // This means it's importing from a transitive dependency.
        //
        // Example: foo/index.d.ts has `import { NestedProps } from "nested"`
        // where foo is in node_modules and nested is in foo/node_modules/nested.
        // The "from" path is "foo/node_modules/nested".
        if !nm_positions.is_empty()
            && symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
            && let Some(import_module) = &symbol.import_module
            && !import_module.starts_with('.')
            && !import_module.starts_with('/')
        {
            // The symbol is an import alias that imports from a bare module specifier.
            // Its source file is in a node_modules package. This means it's importing
            // from a transitive dependency.

            // Get the parent package name from the source path
            let last_nm = *nm_positions.last().unwrap();
            let pkg_start = last_nm + 1;
            let pkg_len = if components.get(pkg_start).is_some_and(|c| {
                matches!(c, Component::Normal(p) if p.to_str().is_some_and(|s| s.starts_with('@')))
            }) {
                2
            } else {
                1
            };

            let parent_package: Vec<String> = components[pkg_start..pkg_start + pkg_len]
                .iter()
                .filter_map(|c| match c {
                    Component::Normal(part) => part.to_str().map(str::to_string),
                    _ => None,
                })
                .collect();

            if !parent_package.is_empty() {
                let from_path = format!(
                    "{}/node_modules/{}",
                    parent_package.join("/"),
                    import_module
                );
                return Some((from_path, type_name));
            }
        }

        // Case 2: Source path has nested node_modules
        // (the resolved original symbol lives in a deeply nested path)
        if nm_positions.len() >= 2 {
            let first_nm = nm_positions[0];
            let second_nm = nm_positions[1];

            let parent_parts: Vec<String> = components[first_nm + 1..second_nm]
                .iter()
                .filter_map(|c| match c {
                    Component::Normal(part) => part.to_str().map(str::to_string),
                    _ => None,
                })
                .collect();

            let nested_start = second_nm + 1;
            let nested_len = if components.get(nested_start).is_some_and(|c| {
                matches!(c, Component::Normal(p) if p.to_str().is_some_and(|s| s.starts_with('@')))
            }) {
                2
            } else {
                1
            };

            let nested_parts: Vec<String> = components[nested_start..nested_start + nested_len]
                .iter()
                .filter_map(|c| match c {
                    Component::Normal(part) => part.to_str().map(str::to_string),
                    _ => None,
                })
                .collect();

            if !parent_parts.is_empty() && !nested_parts.is_empty() {
                let from_path = format!(
                    "{}/node_modules/{}",
                    parent_parts.join("/"),
                    nested_parts.join("/")
                );
                return Some((from_path, type_name));
            }
        }

        // Case 3: Source is in node_modules and the subpath isn't in the
        // package's exports map (private module)
        if nm_positions.len() == 1 {
            let nm_idx = nm_positions[0];
            let pkg_start = nm_idx + 1;
            let pkg_len = if components.get(pkg_start).is_some_and(|c| {
                matches!(c, Component::Normal(p) if p.to_str().is_some_and(|s| s.starts_with('@')))
            }) {
                2
            } else {
                1
            };

            let subpath_start = pkg_start + pkg_len;
            if subpath_start < components.len() {
                let package_root = Path::new(&source_path)
                    .components()
                    .take(nm_idx + 1 + pkg_len)
                    .collect::<std::path::PathBuf>();

                let subpath_parts: Vec<String> = components[subpath_start..]
                    .iter()
                    .filter_map(|c| match c {
                        Component::Normal(part) => part.to_str().map(str::to_string),
                        _ => None,
                    })
                    .collect();

                let relative_path = subpath_parts.join("/");
                if let Some(runtime_path) = self.declaration_runtime_relative_path(&relative_path)
                    && self
                        .reverse_export_specifier_for_runtime_path(&package_root, &runtime_path)
                        .is_none()
                {
                    let pkg_json_path = package_root.join("package.json");
                    if let Ok(pkg_content) = std::fs::read_to_string(&pkg_json_path)
                        && let Ok(pkg_json) =
                            serde_json::from_str::<serde_json::Value>(&pkg_content)
                        && pkg_json.get("exports").is_some()
                    {
                        // Before flagging as non-portable, check whether the
                        // symbol is re-exported from a module that IS accessible
                        // through the package's exports map.  If so, the type
                        // can be referenced via the public API and TS2883
                        // should not fire.
                        if self.symbol_is_reexported_from_public_module(
                            sym_id,
                            &type_name,
                            binder,
                            &package_root,
                        ) {
                            return None;
                        }

                        // Also check whether ANY accessible module in this
                        // package re-exports from the same source file.
                        if self.source_file_is_reexported_from_public_module(
                            &source_path,
                            binder,
                            &package_root,
                        ) {
                            return None;
                        }

                        let mut from_path = self.strip_ts_extensions(
                            &self.calculate_relative_path(current_file_path, &source_path),
                        );
                        if from_path.ends_with("/index") {
                            from_path.truncate(from_path.len() - "/index".len());
                        }
                        return Some((from_path, type_name));
                    }
                }
            }
        }

        if let Some(cache) = &self.type_cache
            && let Some(&symbol_type_id) = cache.symbol_types.get(&sym_id)
            && let Some(result) = self.find_non_portable_type_reference_inner(
                symbol_type_id,
                visited_types,
                visited_symbols,
            )
        {
            return Some(result);
        }

        None
    }

    /// Check whether the symbol is re-exported from a module within the same
    /// package whose runtime path IS accessible through the package's exports
    /// map.  Returns `true` when the type can be reached through the public
    /// API, meaning TS2883 should be suppressed.
    fn symbol_is_reexported_from_public_module(
        &self,
        sym_id: SymbolId,
        type_name: &str,
        binder: &BinderState,
        package_root: &std::path::Path,
    ) -> bool {
        let package_root_str = package_root.to_string_lossy();

        for (module_path, exports) in &binder.module_exports {
            // Only consider modules inside the same package.
            if !module_path.starts_with(package_root_str.as_ref()) {
                continue;
            }
            // Check if this module exports the symbol under the same name.
            let Some(exported_sym_id) = exports.get(type_name) else {
                continue;
            };
            let resolved = self
                .resolve_portability_import_alias(exported_sym_id, binder)
                .unwrap_or_else(|| self.resolve_portability_symbol(exported_sym_id, binder));
            if resolved != sym_id {
                continue;
            }
            // The module re-exports the same symbol.  Check whether that
            // module's own path is accessible through the exports map.
            let module_relative = module_path.strip_prefix(package_root_str.as_ref());
            let module_relative = module_relative.map(|p| p.trim_start_matches('/'));
            if let Some(rel) = module_relative
                && !rel.is_empty()
            {
                if let Some(runtime) = self.declaration_runtime_relative_path(rel)
                    && self
                        .reverse_export_specifier_for_runtime_path(package_root, &runtime)
                        .is_some()
                {
                    return true;
                }
            } else {
                // Module IS the package root (index file).
                return true;
            }
        }

        false
    }

    /// Check whether ANY accessible module in the package re-exports from
    /// the source file.  When a public entry point does
    /// `export { x } from "./other.js"`, types from `other.d.ts` are
    /// indirectly reachable and TS2883 should be suppressed.
    fn source_file_is_reexported_from_public_module(
        &self,
        source_path: &str,
        binder: &BinderState,
        package_root: &std::path::Path,
    ) -> bool {
        use std::path::Path;

        let package_root_str = package_root.to_string_lossy();

        let source_relative = source_path
            .strip_prefix(package_root_str.as_ref())
            .map(|p| p.trim_start_matches('/'));
        let Some(source_relative) = source_relative else {
            return false;
        };
        let source_relative_stripped = self.strip_ts_extensions(source_relative);

        for (module_path, exports) in &binder.module_exports {
            if module_path == source_path || !module_path.starts_with(package_root_str.as_ref()) {
                continue;
            }
            let module_relative = module_path.strip_prefix(package_root_str.as_ref());
            let module_relative = module_relative.map(|p| p.trim_start_matches('/'));
            let is_accessible = if let Some(rel) = module_relative
                && !rel.is_empty()
            {
                self.declaration_runtime_relative_path(rel)
                    .and_then(|runtime| {
                        self.reverse_export_specifier_for_runtime_path(package_root, &runtime)
                    })
                    .is_some()
            } else {
                true
            };
            if !is_accessible {
                continue;
            }

            let module_rel_dir = module_relative
                .and_then(|r| Path::new(r).parent())
                .unwrap_or_else(|| Path::new(""));

            for (_, &exported_sym_id) in exports.iter() {
                if let Some(symbol) = binder.symbols.get(exported_sym_id)
                    && symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
                    && let Some(import_module) = &symbol.import_module
                    && import_module.starts_with('.')
                {
                    let resolved = module_rel_dir.join(import_module);
                    let resolved_str = resolved.to_string_lossy();
                    let resolved_stripped = self.strip_ts_extensions(&resolved_str);
                    let resolved_stripped = resolved_stripped
                        .strip_prefix("./")
                        .unwrap_or(&resolved_stripped);
                    let source_cmp = source_relative_stripped
                        .strip_prefix("./")
                        .unwrap_or(&source_relative_stripped);
                    if resolved_stripped == source_cmp {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn package_root_export_reference_path(
        &self,
        sym_id: SymbolId,
        type_name: &str,
        binder: &BinderState,
        current_file_path: &str,
    ) -> Option<String> {
        let source_path = self.get_symbol_source_path(sym_id, binder)?;

        binder
            .module_exports
            .iter()
            .find_map(|(module_path, exports)| {
                let exported = exports.get(type_name)?;
                let exported = self
                    .resolve_portability_import_alias(exported, binder)
                    .unwrap_or_else(|| self.resolve_portability_symbol(exported, binder));
                if module_path == &source_path || exported != sym_id {
                    return None;
                }

                let specifier =
                    self.package_specifier_for_node_modules_path(current_file_path, module_path)?;
                if specifier.contains('/') {
                    return None;
                }

                let mut from_path = self.strip_ts_extensions(
                    &self.calculate_relative_path(current_file_path, module_path),
                );
                if from_path.ends_with("/index") {
                    from_path.truncate(from_path.len() - "/index".len());
                }
                Some(from_path)
            })
    }

    fn resolve_portability_symbol(&self, sym_id: SymbolId, binder: &BinderState) -> SymbolId {
        let mut current = sym_id;
        let mut seen = rustc_hash::FxHashSet::default();

        while seen.insert(current) {
            let Some(symbol) = binder.symbols.get(current) else {
                break;
            };
            if !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS) {
                break;
            }

            let Some(next) = binder
                .resolve_import_symbol(current)
                .filter(|resolved| *resolved != current)
                .or_else(|| self.resolve_import_symbol_from_module_exports(current, binder))
            else {
                break;
            };
            current = next;
        }

        current
    }

    fn resolve_import_symbol_from_module_exports(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<SymbolId> {
        let symbol = binder.symbols.get(sym_id)?;
        let module_specifier = symbol.import_module.as_deref()?;
        let export_name = symbol
            .import_name
            .as_deref()
            .unwrap_or(symbol.escaped_name.as_str());
        let current_path = self.current_file_path.as_deref()?;

        for (module_path, exports) in &binder.module_exports {
            let candidate =
                if module_specifier.starts_with('.') || module_specifier.starts_with('/') {
                    Some(self.strip_ts_extensions(
                        &self.calculate_relative_path(current_path, module_path),
                    ))
                } else {
                    self.package_specifier_for_node_modules_path(current_path, module_path)
                };
            let matches = candidate.as_deref() == Some(module_specifier);
            if matches && let Some(resolved) = exports.get(export_name) {
                return Some(resolved);
            }
        }

        None
    }

    /// Get the source file path for a symbol via the binder's `symbol_arenas` and `arena_to_path`.
    fn get_symbol_source_path(&self, sym_id: SymbolId, binder: &BinderState) -> Option<String> {
        let source_arena = binder.symbol_arenas.get(&sym_id)?;
        let arena_addr = Arc::as_ptr(source_arena) as usize;
        self.arena_to_path.get(&arena_addr).cloned()
    }
}
