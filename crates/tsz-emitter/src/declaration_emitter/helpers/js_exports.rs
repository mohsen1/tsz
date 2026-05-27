//! JS export collection - basic utilities, require aliases, and export-equals/default handling.

#[allow(unused_imports)]
use super::super::{
    DeclarationEmitter, ImportPlan, JsNestedModuleExportNamespaces, PlannedImportModule,
    PlannedImportSymbol,
};
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

use super::{
    JsClassDefinePropertyAccessor, JsClassDefinePropertySetter, JsClassLikePrototypeMembers,
    JsCommonjsExpandoDeclKind, JsCommonjsExpandoDeclarations, JsCommonjsNamedExports,
    JsCommonjsSyntheticStatements, JsNamespaceExportAlias, JsNamespaceExportAliases,
    JsStaticMethodAugmentationEntry, JsStaticMethodAugmentationGroup, JsStaticMethodAugmentations,
    JsStaticMethodInfo, JsStaticMethodKey,
};

pub(in crate::declaration_emitter) struct JsCjsExportAliasCollection {
    pub aliases: Vec<(String, String)>,
    pub value_declarations: Vec<(String, String)>,
    pub skipped_statements: FxHashSet<NodeIndex>,
}

#[derive(Default)]
pub(in crate::declaration_emitter) struct JsLocalNamedExportPlan {
    pub(in crate::declaration_emitter) folded_names: Vec<String>,
    pub(in crate::declaration_emitter) plain_interface_names: Vec<String>,
    pub(in crate::declaration_emitter) folded_target_statements: Vec<NodeIndex>,
    pub(in crate::declaration_emitter) interface_statements: Vec<NodeIndex>,
    pub(in crate::declaration_emitter) alias_specifiers: Vec<NodeIndex>,
}

impl<'a> DeclarationEmitter<'a> {
    fn is_js_commonjs_export_identifier_text(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !matches!(first, 'A'..='Z' | 'a'..='z' | '_' | '$') {
            return false;
        }
        chars.all(|ch| matches!(ch, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '$'))
    }

    pub(in crate::declaration_emitter) fn js_commonjs_export_name_text(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        let name_node = self.arena.get(name_idx)?;
        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => self.get_identifier_text(name_idx),
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let literal = self.arena.get_literal(name_node)?;
                if Self::is_js_commonjs_export_identifier_text(&literal.text) {
                    Some(literal.text.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn js_commonjs_export_alias_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        if let Some(name) = self.js_commonjs_export_name_text(name_idx) {
            return Some(name);
        }

        let name_node = self.arena.get(name_idx)?;
        match name_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let literal = self.arena.get_literal(name_node)?;
                let sanitized = crate::transforms::emit_utils::sanitize_module_name(&literal.text);
                Some(format!("_{sanitized}"))
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn is_js_function_initializer(
        &self,
        node_idx: NodeIndex,
    ) -> bool {
        self.arena.get(node_idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::ARROW_FUNCTION
                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
        })
    }

    pub(in crate::declaration_emitter) fn js_require_alias_property_access_typeof_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let (receiver_name, property_name, _) =
            self.js_require_alias_property_access_parts(expr_idx)?;
        Some(format!("typeof {receiver_name}.{property_name}"))
    }

    pub(in crate::declaration_emitter) fn js_require_alias_property_access_parts(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<(String, String, String)> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(expr_node)?;
        let receiver_name = self.get_identifier_text(access.expression)?;
        let module_name = self.js_top_level_bare_require_alias_module(&receiver_name)?;
        let property_name = self.get_identifier_text(access.name_or_argument)?;
        Some((receiver_name, property_name, module_name))
    }

    pub(in crate::declaration_emitter) fn js_require_property_import_alias_for_value_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<(String, String, String)> {
        if self.current_source_file_has_native_esm_syntax() {
            return None;
        }
        self.js_require_property_import_alias_for_value_expression_inner(expr_idx, 0)
    }

    fn current_source_file_has_native_esm_syntax(&self) -> bool {
        self.current_source_file_idx
            .and_then(|root_idx| self.arena.get(root_idx))
            .and_then(|root_node| self.arena.get_source_file(root_node))
            .is_some_and(|source_file| self.source_file_has_native_esm_syntax(source_file))
    }

    fn js_require_property_import_alias_for_value_expression_inner(
        &self,
        expr_idx: NodeIndex,
        depth: u8,
    ) -> Option<(String, String, String)> {
        if depth > 4 {
            return None;
        }

        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let local_name = self.get_identifier_text(expr_idx)?;
                let initializer = self.js_top_level_variable_initializer(&local_name)?;
                self.js_require_property_import_alias_for_value_expression_inner(
                    initializer,
                    depth + 1,
                )
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                let new_expr = self.arena.get_call_expr(expr_node)?;
                let local_name = self.get_identifier_text(new_expr.expression)?;
                if self.js_named_export_names.contains(&local_name) {
                    return None;
                }
                let binder = self.binder?;
                let sym_id = self.resolve_identifier_symbol(new_expr.expression, &local_name)?;
                let symbol = binder.symbols.get(sym_id)?;
                for decl_idx in symbol.declarations.iter().copied() {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some((module_name, export_name)) =
                        self.require_property_initializer_parts(decl_node)
                    else {
                        continue;
                    };
                    return Some((local_name, module_name, export_name));
                }
                None
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn source_file_has_commonjs_export_equals_require_alias_property(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        self.source_file_is_js(source_file)
            && !self.source_file_has_native_esm_syntax(source_file)
            && source_file
                .statements
                .nodes
                .iter()
                .copied()
                .any(|stmt_idx| {
                    self.js_commonjs_export_equals_require_alias_property_type_text(stmt_idx)
                        .is_some()
                })
    }

    pub(in crate::declaration_emitter) fn js_commonjs_export_equals_require_alias_property_type_text(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<String> {
        let initializer = self.js_anonymous_module_exports_assignment_initializer(stmt_idx)?;
        self.js_require_alias_property_access_typeof_text(initializer)
    }

    pub(in crate::declaration_emitter) fn emit_js_bare_require_alias_import(
        &mut self,
        local_name: &str,
        module_name: &str,
    ) {
        self.write_indent();
        self.write("import ");
        self.write(local_name);
        self.write(" = require(\"");
        self.write(module_name);
        self.write("\");");
        self.write_line();
        self.emitted_module_indicator = true;
    }

    fn js_top_level_bare_require_alias_module(&self, local_name: &str) -> Option<String> {
        let source_idx = self.current_source_file_idx?;
        let source_node = self.arena.get(source_idx)?;
        let source_file = self.arena.get_source_file(source_node)?;
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
                    if self.get_identifier_text(decl.name).as_deref() != Some(local_name) {
                        continue;
                    }
                    return self.bare_require_call_module_specifier(decl.initializer);
                }
            }
        }

        None
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

        if !self.source_file_has_native_esm_syntax(source_file) {
            for &stmt_idx in &source_file.statements.nodes {
                let Some(name) = self.js_commonjs_export_equals_name(stmt_idx) else {
                    continue;
                };
                names.insert(name);
            }
        }

        names
    }

    /// Collect identifiers used in `export default <Identifier>` statements when the
    /// source file is JS *and* the identifier resolves to a top-level local
    /// declaration. tsc emits these default exports at the export statement's
    /// source position and moves the referenced local declaration after it when
    /// the local declaration is otherwise unexported.
    pub(crate) fn collect_js_export_default_names(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        if !self.source_file_is_js(source_file) {
            return names;
        }

        // Build the set of top-level local declaration names so we only hoist
        // exports of same-file locals. Re-exports of imported identifiers do not
        // need hoisting (tsc emits them in source order, after the import line).
        let mut top_level_names: FxHashSet<String> = FxHashSet::default();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if let Some(name) = self.extract_declaration_name(stmt_idx) {
                top_level_names.insert(name);
                continue;
            }
            if let Some(var_stmt) = self.arena.get_variable(stmt_node) {
                for &decl_list_idx in &var_stmt.declarations.nodes {
                    let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                        continue;
                    };
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
                        if let Some(name) = self.get_identifier_text(decl.name) {
                            top_level_names.insert(name);
                        }
                    }
                }
            }
        }

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            // tsz parses `export default <Identifier>;` as `EXPORT_DECLARATION`
            // with `is_default_export: true` (NOT as `EXPORT_ASSIGNMENT`, which
            // is reserved for `export = X;`).
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if !export.is_default_export {
                continue;
            }
            if export.export_clause.is_none() {
                continue;
            }
            let Some(expr_node) = self.arena.get(export.export_clause) else {
                continue;
            };
            if expr_node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            let Some(ident) = self.arena.get_identifier(expr_node) else {
                continue;
            };
            if !top_level_names.contains(&ident.escaped_text) {
                continue;
            }
            names.insert(ident.escaped_text.clone());
        }

        names
    }

    pub(crate) fn js_default_export_declaration_should_defer_until_export(
        &self,
        stmt_idx: NodeIndex,
    ) -> bool {
        if !self.source_is_js_file
            || self.js_export_default_names.is_empty()
            || self.statement_has_effective_export(stmt_idx)
        {
            return false;
        }

        self.js_default_export_declaration_names_for_statement(stmt_idx)
            .into_iter()
            .any(|name| self.js_export_default_names.contains(&name))
    }

    pub(crate) fn emit_js_default_export_deferred_declaration_for_name(&mut self, name: &str) {
        let Some(root_idx) = self.current_source_file_idx else {
            return;
        };
        let Some(root_node) = self.arena.get(root_idx) else {
            return;
        };
        let Some(source_file) = self.arena.get_source_file(root_node) else {
            return;
        };
        let stmt_idx = source_file
            .statements
            .nodes
            .iter()
            .copied()
            .find(|&stmt_idx| {
                self.js_default_export_declaration_names_for_statement(stmt_idx)
                    .iter()
                    .any(|declared| declared == name)
                    && self.js_default_export_declaration_should_defer_until_export(stmt_idx)
            });

        let Some(stmt_idx) = stmt_idx else {
            return;
        };

        let deferred_jsdoc = self
            .arena
            .get(stmt_idx)
            .map(|stmt_node| {
                (
                    stmt_node.pos,
                    self.leading_jsdoc_comment_chain_for_pos(stmt_node.pos),
                )
            })
            .filter(|(_, chain)| !chain.is_empty());

        if let Some((pos, chain)) = deferred_jsdoc
            && !self.emit_jsdoc_comment_chain_preserving_source_for_pos(pos, &chain)
        {
            self.emit_jsdoc_comment_chain(&chain);
        }

        let previous = self.emitting_js_default_export_declaration;
        self.emitting_js_default_export_declaration = true;
        self.emit_statement(stmt_idx);
        self.emitting_js_default_export_declaration = previous;
    }

    fn js_default_export_declaration_names_for_statement(
        &self,
        stmt_idx: NodeIndex,
    ) -> Vec<String> {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return Vec::new();
        };

        if let Some(name) = self.extract_declaration_name(stmt_idx) {
            return vec![name];
        }

        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return Vec::new();
        };

        let mut names = Vec::new();
        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
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
                if let Some(name) = self.get_identifier_text(decl.name) {
                    names.push(name);
                }
            }
        }
        names
    }

    pub(in crate::declaration_emitter) fn js_module_exports_assignment_initializer(
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

        Some(
            self.arena
                .skip_parenthesized_and_assertions_and_comma(binary.right),
        )
    }

    pub(in crate::declaration_emitter) fn js_anonymous_module_exports_assignment_initializer(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        if !self.source_is_js_file {
            return None;
        }
        if self.js_commonjs_export_equals_name(stmt_idx).is_some() {
            return None;
        }
        self.js_module_exports_assignment_initializer(stmt_idx)
    }

    pub(in crate::declaration_emitter) fn emit_js_cross_file_commonjs_merge_diagnostic(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        if !self.source_file_is_js(source_file)
            || self.source_file_has_native_esm_syntax(source_file)
        {
            return false;
        }

        let mut require_aliases = FxHashSet::default();
        for &stmt_idx in &source_file.statements.nodes {
            self.collect_js_require_aliases_for_statement(stmt_idx, &mut require_aliases);
        }

        let mut has_active_cross_file_export = false;
        let mut augmenting_export = None;
        for &stmt_idx in &source_file.statements.nodes {
            if let Some(initializer) = self.js_module_exports_assignment_initializer(stmt_idx) {
                has_active_cross_file_export = self
                    .js_initializer_references_required_module_export(
                        initializer,
                        &require_aliases,
                    );
                continue;
            }

            if has_active_cross_file_export
                && self
                    .js_commonjs_named_export_for_statement_with_options(stmt_idx, true)
                    .is_some()
            {
                augmenting_export = Some(stmt_idx);
                break;
            }
        }

        if !has_active_cross_file_export {
            return false;
        }
        let Some(augmenting_export) = augmenting_export else {
            return false;
        };
        let Some(stmt_node) = self.arena.get(augmenting_export) else {
            return false;
        };

        self.diagnostics.push(tsz_common::diagnostics::Diagnostic::from_code(
            tsz_common::diagnostics::diagnostic_codes::DECLARATION_AUGMENTS_DECLARATION_IN_ANOTHER_FILE_THIS_CANNOT_BE_SERIALIZED,
            &source_file.file_name,
            stmt_node.pos,
            stmt_node.end.saturating_sub(stmt_node.pos),
            &[],
        ));
        true
    }

    fn collect_js_require_aliases_for_statement(
        &self,
        stmt_idx: NodeIndex,
        aliases: &mut FxHashSet<String>,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return;
        }
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
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
                if self.initializer_is_bare_require_call(decl.initializer)
                    && let Some(name) = self.get_identifier_text(decl.name)
                {
                    aliases.insert(name);
                }
            }
        }
    }

    fn js_initializer_references_required_module_export(
        &self,
        initializer: NodeIndex,
        require_aliases: &FxHashSet<String>,
    ) -> bool {
        let initializer = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(initializer);
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && init_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(init_node) else {
            return false;
        };
        if self
            .js_commonjs_export_name_text(access.name_or_argument)
            .as_deref()
            == Some("default")
        {
            return false;
        }

        let receiver = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(access.expression);
        if self.initializer_is_bare_require_call(receiver) {
            return true;
        }

        self.get_identifier_text(receiver)
            .is_some_and(|name| require_aliases.contains(&name))
    }

    pub(in crate::declaration_emitter) fn js_anonymous_export_equals_class_expression_initializer(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let initializer = self.js_anonymous_module_exports_assignment_initializer(stmt_idx)?;
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return None;
        }
        let class = self.arena.get_class(init_node)?;
        if class.name.is_some() {
            return None;
        }
        Some(initializer)
    }

    pub(in crate::declaration_emitter) fn js_named_export_equals_class_expression(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<(NodeIndex, NodeIndex)> {
        let initializer = self.js_module_exports_assignment_initializer(stmt_idx)?;
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return None;
        }
        let class = self.arena.get_class(init_node)?;
        let name_idx = class.name;
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        Some((name_idx, initializer))
    }

    pub(in crate::declaration_emitter) fn current_file_has_named_export_equals_class_root_name(
        &self,
        name: &str,
    ) -> bool {
        let Some(source_file_idx) = self.current_source_file_idx else {
            return false;
        };
        let Some(source_file_node) = self.arena.get(source_file_idx) else {
            return false;
        };
        let Some(source_file) = self.arena.get_source_file(source_file_node) else {
            return false;
        };

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .filter_map(|stmt_idx| self.js_named_export_equals_class_expression(stmt_idx))
            .any(|(name_idx, _)| self.get_identifier_text(name_idx).as_deref() == Some(name))
    }

    pub(in crate::declaration_emitter) fn js_shadowed_export_equals_local_alias(
        &mut self,
        name: &str,
    ) -> Option<String> {
        if let Some(alias) = self.js_shadowed_export_equals_local_aliases.get(name) {
            return Some(alias.clone());
        }
        if !self.current_file_has_named_export_equals_class_root_name(name) {
            return None;
        }

        let source_file_idx = self.current_source_file_idx?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;

        let has_shadowed_local_class =
            source_file
                .statements
                .nodes
                .iter()
                .copied()
                .any(|stmt_idx| {
                    self.arena
                        .get(stmt_idx)
                        .and_then(|stmt_node| self.arena.get_class(stmt_node))
                        .and_then(|class| self.get_identifier_text(class.name))
                        .as_deref()
                        == Some(name)
                });
        if !has_shadowed_local_class {
            return None;
        }

        let alias = self.generate_unique_name(name);
        self.reserved_names.insert(alias.clone());
        self.js_shadowed_export_equals_local_aliases
            .insert(name.to_string(), alias.clone());
        Some(alias)
    }

    pub(in crate::declaration_emitter) fn js_commonjs_named_export_for_statement_with_options(
        &self,
        stmt_idx: NodeIndex,
        allow_module_exports_receiver: bool,
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
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && lhs_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let lhs_access = self.arena.get_access_expr(lhs_node)?;
        let receiver = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(lhs_access.expression);
        let receiver_is_supported = self.is_exports_identifier_reference(receiver)
            || (allow_module_exports_receiver && self.is_module_exports_reference(receiver));
        if !receiver_is_supported {
            return None;
        }
        self.js_commonjs_export_name_text(lhs_access.name_or_argument)?;

        let rhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        Some((lhs_access.name_or_argument, rhs))
    }

    fn js_commonjs_named_export_alias_for_statement(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<(String, NodeIndex)> {
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
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && lhs_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let lhs_access = self.arena.get_access_expr(lhs_node)?;
        let receiver = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(lhs_access.expression);
        let receiver_is_supported = self.is_exports_identifier_reference(receiver)
            || self.is_module_exports_reference(receiver);
        if !receiver_is_supported {
            return None;
        }
        let export_name = self.js_commonjs_export_alias_name_text(lhs_access.name_or_argument)?;
        let rhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        Some((export_name, rhs))
    }

    pub(in crate::declaration_emitter) fn js_anonymous_module_exports_named_members_initializer(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let initializer = self.js_anonymous_module_exports_assignment_initializer(stmt_idx)?;
        let init_node = self.arena.get(initializer)?;
        if init_node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            return None;
        }

        let source_file_idx = self.current_source_file_idx?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;

        let has_secondary_exports = source_file
            .statements
            .nodes
            .iter()
            .copied()
            .filter(|other_stmt_idx| *other_stmt_idx != stmt_idx)
            .any(|other_stmt_idx| {
                self.js_commonjs_named_export_for_statement_with_options(other_stmt_idx, true)
                    .is_some()
            });
        if !has_secondary_exports {
            return None;
        }

        if init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return Some(initializer);
        }
        if self
            .js_new_expression_class_declaration(initializer)
            .is_some()
        {
            return Some(initializer);
        }

        let type_id = self.get_node_type_or_names(&[initializer])?;
        if matches!(
            type_id,
            tsz_solver::types::TypeId::ANY
                | tsz_solver::types::TypeId::UNKNOWN
                | tsz_solver::types::TypeId::ERROR
                | tsz_solver::types::TypeId::NEVER
        ) {
            return None;
        }
        let interner = self.type_interner?;
        let properties = interner.get_display_properties(type_id)?;
        if properties.is_empty() {
            return None;
        }

        Some(initializer)
    }

    pub(crate) fn source_file_export_equals_names(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> FxHashSet<String> {
        let mut names = FxHashSet::default();

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
            let Some((root_name, export_name, local_name, use_import_alias)) =
                self.js_namespace_export_alias_for_statement(stmt_idx, commonjs_root.as_deref())
            else {
                if let Some((root_name, member_name, _initializer)) =
                    self.js_namespace_class_expando_for_statement(stmt_idx)
                    && let Some(member_text) = self.get_identifier_text(member_name)
                {
                    Self::push_js_namespace_export_alias(
                        &mut aliases,
                        &root_name,
                        member_text.clone(),
                        member_text,
                    );
                    continue;
                }
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

            Self::push_js_namespace_export_alias_with_kind(
                &mut aliases,
                &root_name,
                export_name,
                local_name,
                use_import_alias,
            );
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

    pub(crate) fn collect_js_namespace_alias_declaration_statements(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
        js_export_equals_names: &FxHashSet<String>,
    ) -> FxHashMap<String, Vec<NodeIndex>> {
        if !self.source_file_is_js(source_file) {
            return FxHashMap::default();
        }

        let commonjs_root = if js_export_equals_names.len() == 1 {
            js_export_equals_names.iter().next().map(String::as_str)
        } else {
            None
        };

        let mut declarations_by_local_name: FxHashMap<String, Vec<NodeIndex>> =
            FxHashMap::default();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let local_name = match stmt_node.kind {
                k if k == syntax_kind_ext::CLASS_DECLARATION => self
                    .arena
                    .get_class(stmt_node)
                    .and_then(|class| self.get_identifier_text(class.name)),
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => self
                    .arena
                    .get_function(stmt_node)
                    .and_then(|func| self.get_identifier_text(func.name)),
                _ => None,
            };
            let Some(local_name) = local_name else {
                continue;
            };
            declarations_by_local_name
                .entry(local_name)
                .or_default()
                .push(stmt_idx);
        }
        if declarations_by_local_name.is_empty() {
            return FxHashMap::default();
        }

        let mut declarations: FxHashMap<String, Vec<NodeIndex>> = FxHashMap::default();
        for &stmt_idx in &source_file.statements.nodes {
            let Some((root_name, _export_name, local_name, _use_import_alias)) =
                self.js_namespace_export_alias_for_statement(stmt_idx, commonjs_root)
            else {
                continue;
            };
            if local_name == root_name {
                continue;
            }
            let Some(local_declaration_stmts) = declarations_by_local_name.get(&local_name) else {
                continue;
            };

            let root_declarations = declarations.entry(root_name).or_default();
            for &local_declaration_stmt in local_declaration_stmts {
                if !root_declarations.contains(&local_declaration_stmt) {
                    root_declarations.push(local_declaration_stmt);
                }
            }
        }

        declarations
    }

    pub(crate) fn collect_js_namespace_class_expando_declarations(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> JsCommonjsSyntheticStatements {
        let mut declarations = FxHashMap::default();
        if !self.source_file_is_js(source_file) {
            return declarations;
        }

        let mut commonjs_exported_property_refs: FxHashSet<(String, String)> = FxHashSet::default();
        for &stmt_idx in &source_file.statements.nodes {
            let Some((_export_name_idx, initializer)) =
                self.js_commonjs_named_export_for_statement(stmt_idx)
            else {
                continue;
            };
            let initializer = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(initializer);
            let Some(init_node) = self.arena.get(initializer) else {
                continue;
            };
            if init_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                continue;
            }
            let Some(access) = self.arena.get_access_expr(init_node) else {
                continue;
            };
            let receiver = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(access.expression);
            if let (Some(root_name), Some(member_name)) = (
                self.get_identifier_text(receiver),
                self.get_identifier_text(access.name_or_argument),
            ) {
                commonjs_exported_property_refs.insert((root_name, member_name));
            }
        }

        for &stmt_idx in &source_file.statements.nodes {
            let Some((root_name, member_name, initializer)) =
                self.js_namespace_class_expando_for_statement(stmt_idx)
            else {
                continue;
            };
            if let Some(member_text) = self.get_identifier_text(member_name)
                && commonjs_exported_property_refs.contains(&(root_name, member_text))
            {
                continue;
            }
            declarations.insert(stmt_idx, (member_name, initializer));
        }

        declarations
    }

    /// Collect CJS export aliases for `exports.X = Y` / `module.exports.X = Y`.
    pub(in crate::declaration_emitter) fn collect_js_cjs_export_aliases(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> JsCjsExportAliasCollection {
        let empty = JsCjsExportAliasCollection {
            aliases: Vec::new(),
            value_declarations: Vec::new(),
            skipped_statements: FxHashSet::default(),
        };
        if !self.source_file_is_js(source_file) {
            return empty;
        }
        if self.source_file_has_native_esm_syntax(source_file) {
            return empty;
        }
        if !self.js_export_equals_names.is_empty() {
            return empty;
        }
        let export_targets = self.collect_js_named_export_targets(source_file);
        let mut alias_map: FxHashMap<String, (String, Vec<NodeIndex>, usize)> =
            FxHashMap::default();
        for (order, &stmt_idx) in source_file.statements.nodes.iter().enumerate() {
            if let Some((export_name, rhs_idx)) =
                self.js_commonjs_named_export_alias_for_statement(stmt_idx)
            {
                let rhs_idx = self
                    .arena
                    .skip_parenthesized_and_assertions_and_comma(rhs_idx);
                let local_name = self
                    .get_identifier_text(rhs_idx)
                    .or_else(|| self.module_exports_property_reference_name(rhs_idx));
                let entry = alias_map
                    .entry(export_name.clone())
                    .or_insert_with(|| (String::new(), Vec::new(), order));
                entry.1.push(stmt_idx);
                if let Some(ref ln) = local_name
                    && *ln != export_name
                {
                    entry.0 = ln.clone();
                }
                continue;
            }
            if let Some((export_name, local_name, stmt)) =
                self.js_module_exports_property_alias(stmt_idx)
            {
                let entry = alias_map
                    .entry(export_name.clone())
                    .or_insert_with(|| (String::new(), Vec::new(), order));
                entry.1.push(stmt);
                if export_name != local_name {
                    entry.0 = local_name;
                }
            }
        }
        let mut aliases = Vec::new();
        let mut value_declarations = Vec::new();
        let mut skipped = FxHashSet::default();
        let mut seen = FxHashSet::default();
        let mut ordered_aliases: Vec<_> = alias_map.iter().collect();
        ordered_aliases.sort_by_key(|(_, (_, _, order))| *order);
        for (export_name, (local_name, stmts, _)) in ordered_aliases {
            if local_name.is_empty() {
                continue;
            }
            if !export_targets.contains_key(local_name) && !alias_map.contains_key(local_name) {
                continue;
            }
            for &s in stmts {
                skipped.insert(s);
            }
            if let Some(type_text) =
                self.js_commonjs_export_alias_value_type_text(source_file, export_name, local_name)
            {
                value_declarations.push((export_name.clone(), type_text));
            }
            if seen.insert((export_name.clone(), local_name.clone())) {
                aliases.push((export_name.clone(), local_name.clone()));
            }
        }
        JsCjsExportAliasCollection {
            aliases,
            value_declarations,
            skipped_statements: skipped,
        }
    }

    fn js_commonjs_export_alias_value_type_text(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
        export_name: &str,
        alias_local_name: &str,
    ) -> Option<String> {
        let mut alias_type = false;
        let mut value_types = Vec::new();
        let mut has_undefined = false;
        let mut seen = FxHashSet::default();
        for &stmt_idx in &source_file.statements.nodes {
            let Some((name_idx, initializer)) =
                self.js_commonjs_named_export_for_statement_with_options(stmt_idx, true)
            else {
                continue;
            };
            if self.js_commonjs_export_name_text(name_idx).as_deref() != Some(export_name) {
                continue;
            }
            if self.get_identifier_text(initializer).as_deref() == Some(alias_local_name) {
                alias_type = true;
                continue;
            }
            let Some(type_text) = self.js_commonjs_export_alias_assignment_type_text(initializer)
            else {
                continue;
            };
            if type_text == "undefined" {
                has_undefined = true;
                continue;
            }
            if seen.insert(type_text.clone()) {
                value_types.push(type_text);
            }
        }
        if value_types.is_empty() {
            return None;
        }

        let mut parts = Vec::new();
        if alias_type {
            parts.push(format!("typeof {alias_local_name}"));
        }
        parts.extend(value_types);
        if has_undefined {
            parts.push("undefined".to_string());
        }
        if parts.len() <= 1 {
            return None;
        }
        Some(parts.join(" | "))
    }

    fn js_commonjs_export_alias_assignment_type_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let initializer = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(initializer);
        let init_node = self.arena.get(initializer)?;
        if self.get_identifier_text(initializer).as_deref() == Some("undefined")
            || init_node.kind == SyntaxKind::UndefinedKeyword as u16
            || self.is_void_expression(init_node)
        {
            return Some("undefined".to_string());
        }
        if init_node.kind == SyntaxKind::StringLiteral as u16
            || init_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            let literal = self.arena.get_literal(init_node)?;
            return Some(format!("{:?}", literal.text));
        }
        self.js_synthetic_export_value_type_text(initializer)
    }
}
