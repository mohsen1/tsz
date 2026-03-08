//! Declaration emitter - expression/node emission, import management, and utility helpers.
//!
//! Type syntax emission (type references, unions, mapped types, etc.) is in `type_emission.rs`.

use super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
use crate::emitter::type_printer::TypePrinter;
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tracing::debug;
use tsz_binder::{BinderState, SymbolId};
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
use tsz_parser::parser::ParserState;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

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
                // tsc normalizes initializer string literals to double quotes
                if let Some(lit) = self.arena.get_literal(expr_node) {
                    self.write("\"");
                    self.write(&lit.text);
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
                    self.write(quote);
                    // Escape special characters that can't appear raw in string literals
                    // (e.g., template literals produce cooked text with actual newlines)
                    if lit.text.contains('\n')
                        || lit.text.contains('\r')
                        || lit.text.contains('\t')
                        || lit.text.contains('\0')
                    {
                        let escaped = lit
                            .text
                            .replace('\\', "\\\\")
                            .replace('\n', "\\n")
                            .replace('\r', "\\r")
                            .replace('\t', "\\t")
                            .replace('\0', "\\0");
                        self.write(&escaped);
                    } else {
                        self.write(&lit.text);
                    }
                    self.write(quote);
                }
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    self.write(&lit.text);
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

    fn leading_jsdoc_type_expr_for_pos(&self, pos: u32) -> Option<String> {
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
            if next_char.is_none() || !next_char.unwrap().is_alphabetic() {
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

    fn jsdoc_name_like_type_expr_for_pos(&self, pos: u32) -> Option<String> {
        let expr = self.leading_jsdoc_type_expr_for_pos(pos)?;
        if Self::jsdoc_name_like_type_reference(&expr) {
            Some(expr)
        } else {
            None
        }
    }

    fn jsdoc_name_like_type_expr_for_node(&self, idx: NodeIndex) -> Option<String> {
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

        is_exported || self.should_emit_public_api_dependency(name_idx)
    }

    /// Return true if a declaration should be skipped because it's a
    /// non-exported value/type inside a non-ambient namespace.
    /// Namespace and import-alias declarations are NOT filtered here
    /// (they may be needed for name resolution and are filtered recursively).
    pub(crate) fn should_skip_ns_internal_member(&self, modifiers: &Option<NodeList>) -> bool {
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
        // Non-exported member inside non-ambient namespace: skip
        true
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

        // Check if the specifier's NAME symbol is used
        if let Some(&sym_id) = binder.node_symbols.get(&specifier.name.0) {
            used.contains_key(&sym_id)
        } else {
            // No symbol found - emit conservatively
            true
        }
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
                if clause.name.is_some()
                    && let Some(&sym_id) = binder.node_symbols.get(&clause.name.0)
                    && used.contains_key(&sym_id)
                {
                    default_count = 1;
                }

                // Count named imports
                if clause.named_bindings.is_some()
                    && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                    && let Some(bindings) = self.arena.get_named_imports(bindings_node)
                {
                    for &spec_idx in &bindings.elements.nodes {
                        // Get the specifier's name to check its symbol
                        if let Some(spec_node) = self.arena.get(spec_idx)
                            && let Some(specifier) = self.arena.get_specifier(spec_node)
                            && let Some(&sym_id) = binder.node_symbols.get(&specifier.name.0)
                            && used.contains_key(&sym_id)
                        {
                            named_count += 1;
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
            k if k == SyntaxKind::TrueKeyword as u16 || k == SyntaxKind::FalseKeyword as u16 => {
                Some("boolean".to_string())
            }
            k if k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16 =>
            {
                Some("any".to_string())
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.infer_object_literal_type_text_at(node_id, depth)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => Some("any[]".to_string()),
            _ => self
                .get_node_type(node_id)
                .map(|type_id| self.print_type_id(type_id)),
        }
    }

    fn infer_object_literal_type_text_at(
        &self,
        object_expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let object_node = self.arena.get(object_expr_idx)?;
        let object = self.arena.get_literal_expr(object_node)?;
        let mut members = Vec::new();

        for &member_idx in &object.elements.nodes {
            if let Some(member_text) = self.infer_object_member_type_text_at(member_idx, depth + 1)
            {
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

    fn infer_object_member_type_text_at(
        &self,
        member_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let data = self.arena.get_property_assignment(member_node)?;
                let name = self.infer_property_name_text(data.name)?;
                let type_text = self
                    .infer_fallback_type_text_at(data.initializer, depth)
                    .unwrap_or_else(|| "any".to_string());
                Some(format!("{name}: {type_text}"))
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let data = self.arena.get_shorthand_property(member_node)?;
                let name = self.infer_property_name_text(data.name)?;
                let type_text = self
                    .infer_fallback_type_text_at(data.object_assignment_initializer, depth)
                    .unwrap_or_else(|| "any".to_string());
                Some(format!("{name}: {type_text}"))
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let data = self.arena.get_accessor(member_node)?;
                let name = self.infer_property_name_text(data.name)?;
                // Prefer explicit return type annotation, then fall back to any
                let type_text = self
                    .infer_fallback_type_text_at(data.type_annotation, depth)
                    .unwrap_or_else(|| "any".to_string());
                Some(format!("readonly {name}: {type_text}"))
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let data = self.arena.get_method_decl(member_node)?;
                let name = self.infer_property_name_text(data.name)?;
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
                    return self.get_source_slice(node.pos, node.end);
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

        object.elements.nodes.iter().copied().any(|member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
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

            name_idx.is_some_and(|name_idx| {
                self.arena.get(name_idx).is_some_and(|name_node| {
                    name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                })
            })
        })
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

        let mut computed_members = Vec::new();
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
            if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                continue;
            }

            let Some(name_text) = self.infer_property_name_text(name_idx) else {
                continue;
            };
            only_numeric_like &= Self::is_numeric_property_name_text(&name_text);

            let Some(member_text) =
                self.infer_object_member_type_text_at(member_idx, self.indent_level + 1)
            else {
                continue;
            };
            computed_members.push(member_text);
        }

        if computed_members.is_empty() {
            return None;
        }

        let printed = self.print_type_id(type_id);
        let mut lines: Vec<String> = printed.lines().map(str::to_string).collect();
        if lines.len() < 2 {
            return Some(printed);
        }

        if only_numeric_like {
            lines.retain(|line| !line.trim_start().starts_with("[x: string]:"));
        }

        let insert_at = lines.len().saturating_sub(1);
        let indent = "    ".repeat((self.indent_level + 1) as usize);
        for (offset, member_text) in computed_members.into_iter().enumerate() {
            let line = format!("{indent}{member_text};");
            if !lines.iter().any(|existing| existing.trim() == line.trim()) {
                lines.insert(insert_at + offset, line);
            }
        }

        Some(lines.join("\n"))
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

            let Some(node) = self.arena.get(node_id) else {
                continue;
            };

            for related_id in self.get_node_type_related_nodes(node) {
                if let Some(type_id) = self.get_node_type(related_id) {
                    return Some(type_id);
                }
            }
        }
        None
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
            k if k == syntax_kind_ext::TYPE_QUERY => {
                if let Some(query) = self.arena.get_type_query(node) {
                    vec![query.expr_name]
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
            let mut printer = TypePrinter::new(interner)
                .with_indent_level(self.indent_level)
                .with_node_arena(self.arena);

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

    /// Resolve a foreign symbol to its module path.
    ///
    /// Returns the module specifier (e.g., "./utils") for importing the symbol.
    pub(crate) fn resolve_symbol_module_path(&self, sym_id: SymbolId) -> Option<String> {
        let (Some(binder), Some(current_path)) = (&self.binder, &self.current_file_path) else {
            return None;
        };

        // 1. Check for ambient modules (declare module "name")
        if let Some(ambient_path) = self.check_ambient_module(sym_id, binder) {
            return Some(ambient_path);
        }

        // 2. Check import_symbol_map for imported symbols
        // This handles symbols that were imported from other modules
        if let Some(module_specifier) = self.import_symbol_map.get(&sym_id) {
            return Some(module_specifier.clone());
        }

        // 3. Get the source arena for this symbol
        let source_arena = binder.symbol_arenas.get(&sym_id)?;

        // 4. Look up the file path from arena address
        let arena_addr = Arc::as_ptr(source_arena) as usize;
        let source_path = self.arena_to_path.get(&arena_addr)?;

        // 5. Calculate relative path
        let rel_path = self.calculate_relative_path(current_path, source_path);

        // 6. Strip TypeScript extensions
        Some(self.strip_ts_extensions(&rel_path))
    }

    pub(crate) fn resolve_symbol_module_path_cached(&mut self, sym_id: SymbolId) -> Option<String> {
        if let Some(cached) = self.symbol_module_specifier_cache.get(&sym_id) {
            return cached.clone();
        }

        let resolved = self.resolve_symbol_module_path(sym_id);
        self.symbol_module_specifier_cache
            .insert(sym_id, resolved.clone());
        resolved
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

    /// Strip TypeScript file extensions from a path.
    ///
    /// Converts "../utils.ts" -> "../utils"
    pub(crate) fn strip_ts_extensions(&self, path: &str) -> String {
        // Remove .ts, .tsx, .d.ts, .d.tsx extensions
        for ext in [".d.ts", ".d.tsx", ".tsx", ".ts"] {
            if let Some(path) = path.strip_suffix(ext) {
                return path.to_string();
            }
        }
        path.to_string()
    }

    /// Group foreign symbols by their module paths.
    ///
    /// Returns a map of module path -> Vec<SymbolId> for all foreign symbols.
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

        if let Some(binder) = self.binder {
            let module_map = self.group_foreign_symbols_by_module();
            let mut auto_modules: Vec<_> = module_map.into_iter().collect();
            auto_modules.sort_by(|a, b| a.0.cmp(&b.0));

            for (module, symbol_ids) in auto_modules {
                let mut symbol_names: Vec<String> = symbol_ids
                    .into_iter()
                    .filter(|sym_id| self.import_symbol_map.contains_key(sym_id))
                    .filter_map(|sym_id| binder.symbols.get(sym_id).map(|s| s.escaped_name.clone()))
                    .collect();
                symbol_names.sort();
                symbol_names.dedup();

                if symbol_names.is_empty() {
                    continue;
                }

                let symbols = symbol_names
                    .into_iter()
                    .map(|name| PlannedImportSymbol { name, alias: None })
                    .collect();
                plan.auto_generated
                    .push(PlannedImportModule { module, symbols });
            }
        }

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

        // Determine if we should emit a literal initializer for const
        let use_literal_initializer =
            if keyword == "const" && !has_type_annotation && has_initializer {
                // Check if initializer is a primitive literal
                // Note: null is excluded — `const x = null` should emit `: any` in .d.ts
                if let Some(init_node) = self.arena.get(initializer) {
                    let k = init_node.kind;
                    k == SyntaxKind::StringLiteral as u16
                        || k == SyntaxKind::NumericLiteral as u16
                        || k == SyntaxKind::BigIntLiteral as u16
                        || k == SyntaxKind::TrueKeyword as u16
                        || k == SyntaxKind::FalseKeyword as u16
                        // Handle negative numeric/bigint literals: PrefixUnaryExpression(-X)
                        || (k == tsz_parser::parser::syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                            && self.is_negative_literal(init_node))
                        || self.simple_enum_access_member_text(initializer).is_some()
                } else {
                    false
                }
            } else {
                false
            };

        if use_literal_initializer {
            self.write(if self.source_is_js_file { ": " } else { " = " });
            self.emit_expression(initializer);
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
                && (self.function_initializer_has_inline_parameter_comments(initializer)
                    || self.function_initializer_is_self_returning(initializer))
                && self.emit_function_initializer_type_annotation(decl_idx, decl_name, initializer)
            {
            } else if let Some(type_id) = self.get_node_type_or_names(&[decl_idx, decl_name]) {
                if keyword == "const"
                    && let Some(interner) = self.type_interner
                {
                    if let Some(lit) = tsz_solver::visitor::literal_value(interner, type_id) {
                        self.write(if self.source_is_js_file { ": " } else { " = " });
                        self.write(&Self::format_literal_initializer(&lit, interner));
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
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(type_id));
                }
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
            format!("\"{name}\"")
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

        if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(init_node)?;
            let rhs = self.get_identifier_text(access.name_or_argument)?;
            let lhs = self.nameable_constructor_expression_text(access.expression)?;
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
                format!("\"{}\"", interner.resolve_atom(*atom))
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
                .map(|lit| format!("\"{}\"", lit.text)),
            k if k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16 =>
            {
                self.arena
                    .get_literal(expr_node)
                    .map(|lit| lit.text.clone())
            }
            k if k == SyntaxKind::TrueKeyword as u16 => Some("true".to_string()),
            k if k == SyntaxKind::FalseKeyword as u16 => Some("false".to_string()),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                && self.is_negative_literal(expr_node) =>
            {
                self.get_source_slice(expr_node.pos, expr_node.end)
            }
            _ => None,
        }
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
}
