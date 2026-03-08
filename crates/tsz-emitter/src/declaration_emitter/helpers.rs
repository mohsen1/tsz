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
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

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
                _ => {}
            }
        }

        has_import || has_export
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

    pub(crate) fn collect_js_named_export_names(
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

            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = self.arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = self.arena.get_specifier(spec_node) else {
                    continue;
                };
                if spec.property_name.is_some() {
                    continue;
                }
                let Some(name_node) = self.arena.get(spec.name) else {
                    continue;
                };
                let Some(name_ident) = self.arena.get_identifier(name_node) else {
                    continue;
                };
                names.insert(name_ident.escaped_text.clone());
            }
        }

        names
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

        names
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

    pub(crate) fn should_fold_js_named_export_clause(&self, export_idx: NodeIndex) -> bool {
        if !self.source_is_js_file {
            return false;
        }

        let Some(export_node) = self.arena.get(export_idx) else {
            return false;
        };
        let Some(export) = self.arena.get_export_decl(export_node) else {
            return false;
        };
        if export.module_specifier.is_some() || export.export_clause.is_none() {
            return false;
        }

        let Some(clause_node) = self.arena.get(export.export_clause) else {
            return false;
        };
        if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
            return false;
        }

        let Some(named) = self.arena.get_named_imports(clause_node) else {
            return false;
        };
        if named.elements.nodes.is_empty() {
            return false;
        }

        named.elements.nodes.iter().all(|&spec_idx| {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                return false;
            };
            let Some(spec) = self.arena.get_specifier(spec_node) else {
                return false;
            };
            spec.property_name.is_none() && self.is_js_named_exported_name(spec.name)
        })
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
    pub(crate) const fn should_emit_public_api_module(&self, is_exported: bool) -> bool {
        if !self.public_api_filter_enabled() {
            return true;
        }

        is_exported
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
                    }
                    self.write(ct);
                    if has_newline {
                        self.write_line();
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
        if let Some(ident) = self.arena.get_identifier(node) {
            return Some(ident.escaped_text.clone());
        }
        if let Some(literal) = self.arena.get_literal(node) {
            let quote = self.original_quote_char(node);
            return Some(format!("{}{}{}", quote, literal.text, quote));
        }
        self.get_source_slice(node.pos, node.end)
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
            let mut printer = TypePrinter::new(interner).with_indent_level(self.indent_level);

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
        decl_idx: NodeIndex,
        decl_name: NodeIndex,
        type_annotation: NodeIndex,
        initializer: NodeIndex,
        has_type_annotation: bool,
        has_initializer: bool,
    ) {
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
                        // Handle simple enum member accesses: E.A, E["key"]
                        // Only allow when left-hand side is a simple identifier (not deep chains)
                        || ((k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                            || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                            && self.is_simple_enum_access(init_node))
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
            } else if is_const_null_or_undefined {
                self.write(": any");
            } else if self.source_is_js_file
                && has_initializer
                && let Some(type_text) = self.js_special_initializer_type_text(initializer)
            {
                self.write(": ");
                self.write(&type_text);
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

                    if has_initializer
                        && let Some(init_node) = self.arena.get(initializer)
                        && init_node.kind == syntax_kind_ext::CALL_EXPRESSION
                        && let Some(call) = self.arena.get_call_expr(init_node)
                        && let Some(args) = &call.arguments
                        && args.nodes.len() == 1
                    {
                        let arg = args.nodes[0];
                        if let Some(arg_node) = self.arena.get(arg) {
                            let arg_kind = arg_node.kind;
                            let is_literal_arg = arg_kind == SyntaxKind::StringLiteral as u16
                                || arg_kind == SyntaxKind::NumericLiteral as u16
                                || arg_kind == SyntaxKind::BigIntLiteral as u16
                                || arg_kind == SyntaxKind::TrueKeyword as u16
                                || arg_kind == SyntaxKind::FalseKeyword as u16
                                || (arg_kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                                    && self.is_negative_literal(arg_node));
                            if is_literal_arg {
                                self.write(" = ");
                                self.emit_expression(arg);
                                return;
                            }
                        }
                    }
                }

                if let Some(typeof_text) =
                    self.typeof_prefix_for_value_entity(initializer, has_initializer, Some(type_id))
                {
                    // Bare identifier referencing an enum/module → emit typeof
                    self.write(": ");
                    self.write(&typeof_text);
                } else {
                    self.write(": ");
                    self.write(&self.print_type_id(type_id));
                }
            } else if let Some(type_text) = self.infer_fallback_type_text(initializer) {
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
        if init_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let identifier_name = self.get_identifier_text(initializer)?;
        let interner = self.type_interner?;

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
    fn get_identifier_text(&self, idx: NodeIndex) -> Option<String> {
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
