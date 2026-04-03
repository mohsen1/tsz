//! JS export collection (CJS, namespace, expando, prototype)

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

use super::{JsFoldedNamedExports, JsNamespaceExportAliases, JsCommonjsSyntheticStatements, JsCommonjsNamedExports, JsCommonjsExpandoDeclKind, JsCommonjsExpandoDeclarations, JsStaticMethodAugmentationGroup, JsStaticMethodAugmentations, JsClassLikePrototypeMembers, JsStaticMethodKey, JsStaticMethodInfo, JsStaticMethodAugmentationEntry, JsDefinedPropertyDecl, JsdocParamDecl};

impl<'a> DeclarationEmitter<'a> {
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
    pub(in crate::declaration_emitter) fn js_module_exports_property_alias(
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

    pub(in crate::declaration_emitter) fn js_commonjs_named_export_value_initializer_supported(&self, initializer: NodeIndex) -> bool {
        self.js_synthetic_export_value_type_text(initializer)
            .is_some()
    }

    pub(in crate::declaration_emitter) fn js_named_class_expression_matches_export(
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

    pub(in crate::declaration_emitter) fn js_commonjs_void_zero_export_init(&self, expr_idx: NodeIndex) -> bool {
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

    pub(in crate::declaration_emitter) fn groupable_js_reexport_info(&self, export_idx: NodeIndex) -> Option<(String, bool)> {
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

    pub(in crate::declaration_emitter) fn js_commonjs_export_equals_name(&self, stmt_idx: NodeIndex) -> Option<String> {
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

    pub(in crate::declaration_emitter) fn js_commonjs_expando_decl_for_statement(
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

    pub(in crate::declaration_emitter) fn js_commonjs_named_export_for_statement(
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

    pub(in crate::declaration_emitter) fn js_namespace_export_alias_for_statement(
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

    pub(in crate::declaration_emitter) fn js_namespace_export_alias_target(
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

    pub(in crate::declaration_emitter) fn js_commonjs_expando_receiver(
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

    pub(in crate::declaration_emitter) fn js_class_static_method_augmentation_for_statement(
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

    pub(in crate::declaration_emitter) fn collect_js_static_class_methods_for_statement(
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

    pub(in crate::declaration_emitter) fn collect_js_static_class_methods(
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

    pub(in crate::declaration_emitter) fn is_module_exports_reference(&self, idx: NodeIndex) -> bool {
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

    pub(in crate::declaration_emitter) fn is_exports_identifier_reference(&self, idx: NodeIndex) -> bool {
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

    pub(in crate::declaration_emitter) fn push_js_namespace_export_alias(
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

}
