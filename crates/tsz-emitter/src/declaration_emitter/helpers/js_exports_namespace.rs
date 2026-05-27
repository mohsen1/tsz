//! JS export collection - namespace objects, grouped reexports, module-exports properties, and emit helpers.

#[allow(unused_imports)]
use super::super::{
    DeclarationEmitter, ImportPlan, JsNestedModuleExportNamespaces, PlannedImportModule,
    PlannedImportSymbol,
};
#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};
#[allow(unused_imports)]
use tsz_parser::parser::node::Node;
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

use super::{
    JsCommonjsExpandoDeclKind, JsNamespaceExportAlias, JsNamespaceExportAliases,
    JsStaticMethodInfo, JsStaticMethodKey,
};

impl<'a> DeclarationEmitter<'a> {
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

    pub(in crate::declaration_emitter) fn js_namespace_object_stmt_emits_in_source_order(
        &self,
        stmt_idx: NodeIndex,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return false;
        }
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return false;
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
                if self.is_js_named_exported_name(decl.name) {
                    return true;
                }
            }
        }

        false
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
            k if k == SyntaxKind::Identifier as u16 => {
                self.get_identifier_text(initializer).as_deref() == Some("undefined")
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.js_empty_object_literal_initializer(initializer)
                    || self.js_object_literal_initializer_has_namespace_shape(initializer, true)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => true,
            k if k == syntax_kind_ext::NEW_EXPRESSION => true,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => self
                .js_namespace_property_reference_text(initializer)
                .or_else(|| self.js_namespace_value_member_type_text(initializer))
                .or_else(|| self.js_prop_types_validator_member_type_text(initializer))
                .is_some(),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                self.is_negative_literal(init_node)
            }
            _ => false,
        }
    }

    pub(crate) fn js_empty_object_literal_initializer(&self, initializer: NodeIndex) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        self.arena
            .get_literal_expr(init_node)
            .is_some_and(|object| object.elements.nodes.is_empty())
    }

    pub(in crate::declaration_emitter) fn groupable_js_reexport_info(
        &self,
        export_idx: NodeIndex,
    ) -> Option<(String, bool)> {
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

    pub(in crate::declaration_emitter) fn record_js_require_property_import_alias_statement(
        &mut self,
        stmt_idx: NodeIndex,
    ) -> bool {
        if !self.source_is_js_file || self.inside_non_ambient_namespace {
            return false;
        }
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return false;
        }
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return false;
        };

        let mut aliases = Vec::new();
        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                return false;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                return false;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    return false;
                };
                let Some(local_name) = self
                    .arena
                    .get_variable_declaration(decl_node)
                    .and_then(|decl| self.get_identifier_text(decl.name))
                else {
                    return false;
                };
                let Some((module_name, export_name)) =
                    self.require_property_initializer_parts(decl_node)
                else {
                    return false;
                };
                aliases.push((local_name, module_name, export_name));
            }
        }

        if aliases.is_empty() {
            return false;
        }
        for alias in aliases {
            if !self
                .js_require_property_import_aliases
                .iter()
                .any(|existing| existing == &alias)
            {
                self.js_require_property_import_aliases.push(alias);
            }
        }
        true
    }

    pub(in crate::declaration_emitter) fn require_property_initializer_parts(
        &self,
        decl_node: &Node,
    ) -> Option<(String, String)> {
        let decl = self.arena.get_variable_declaration(decl_node)?;
        let init_node = self.arena.get(decl.initializer)?;
        if init_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.arena.get_access_expr(init_node)?;
        let export_name = self.get_identifier_text(access.name_or_argument)?;
        let call_node = self.arena.get(access.expression)?;
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(call_node)?;
        let callee = self.get_identifier_text(call.expression)?;
        let args = call.arguments.as_ref()?;
        if callee != "require" || args.nodes.len() != 1 {
            return None;
        }

        let arg_node = self.arena.get(args.nodes[0])?;
        if arg_node.kind != SyntaxKind::StringLiteral as u16 {
            return None;
        }
        let module_name = self.arena.get_literal(arg_node)?.text.clone();
        Some((module_name, export_name))
    }

    pub(in crate::declaration_emitter) fn js_local_bare_require_alias_without_export_surface(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        if !self.source_is_js_file
            || !self.js_named_export_names.is_empty()
            || !self.js_export_equals_names.is_empty()
            || !self.js_namespace_export_aliases.is_empty()
        {
            return false;
        }

        self.initializer_is_bare_require_call(initializer)
    }

    pub(in crate::declaration_emitter) fn emit_js_require_property_import_aliases(&mut self) {
        let aliases = std::mem::take(&mut self.js_require_property_import_aliases);
        for (local_name, module_name, export_name) in &aliases {
            let module_alias_candidate = format!("{local_name}_1");
            let module_alias = if self.reserved_names.contains(&module_alias_candidate) {
                self.generate_unique_name(local_name)
            } else {
                module_alias_candidate
            };
            self.reserved_names.insert(module_alias.clone());
            self.write_indent();
            self.write("import ");
            self.write(&module_alias);
            self.write(" = require(\"");
            self.write(module_name);
            self.write("\");");
            self.write_line();

            self.write_indent();
            self.write("import ");
            self.write(local_name);
            self.write(" = ");
            self.write(&module_alias);
            self.write(".");
            self.write(export_name);
            self.write(";");
            self.write_line();
            self.emitted_module_indicator = true;
        }
        self.js_require_property_import_aliases = aliases;
    }

    pub(in crate::declaration_emitter) fn record_js_require_property_import_alias_for_new_expression(
        &mut self,
        expr_idx: NodeIndex,
    ) {
        let Some(alias) = self.js_require_property_import_alias_for_value_expression(expr_idx)
        else {
            return;
        };
        if !self
            .js_require_property_import_aliases
            .iter()
            .any(|existing| existing == &alias)
        {
            self.js_require_property_import_aliases.push(alias);
        }
    }

    pub(crate) fn collect_js_module_exports_nested_namespaces(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> (JsNestedModuleExportNamespaces, FxHashSet<NodeIndex>) {
        let mut roots = FxHashMap::<String, NodeIndex>::default();
        let mut nested = FxHashMap::<NodeIndex, Vec<(NodeIndex, NodeIndex)>>::default();
        let mut skipped = FxHashSet::default();
        if !self.source_file_is_js(source_file) {
            return (nested, skipped);
        }

        for &stmt_idx in &source_file.statements.nodes {
            let Some((root_name, initializer)) =
                self.js_module_exports_property_assignment(stmt_idx)
            else {
                continue;
            };
            let Some(init_node) = self.arena.get(initializer) else {
                continue;
            };
            if init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                && self
                    .arena
                    .get_literal_expr(init_node)
                    .is_some_and(|object| object.elements.nodes.is_empty())
            {
                roots.insert(root_name, stmt_idx);
            }
        }

        for &stmt_idx in &source_file.statements.nodes {
            let Some((root_name, member_name, initializer)) =
                self.js_module_exports_nested_property_assignment(stmt_idx)
            else {
                continue;
            };
            let Some(root_stmt) = roots.get(&root_name).copied() else {
                continue;
            };
            let Some(init_node) = self.arena.get(initializer) else {
                continue;
            };
            if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }
            nested
                .entry(root_stmt)
                .or_default()
                .push((member_name, initializer));
            skipped.insert(stmt_idx);
        }

        for root_stmt in nested.keys().copied() {
            skipped.insert(root_stmt);
        }
        (nested, skipped)
    }

    pub(in crate::declaration_emitter) fn js_module_exports_property_assignment(
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
        let root_name = self.module_exports_property_reference_name(lhs)?;
        let rhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        Some((root_name, rhs))
    }

    fn js_module_exports_nested_property_assignment(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<(String, NodeIndex, NodeIndex)> {
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
        let member_name = lhs_access.name_or_argument;
        self.get_identifier_text(member_name)?;
        let receiver = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(lhs_access.expression);
        let root_name = self.module_exports_property_reference_name(receiver)?;
        let rhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        Some((root_name, member_name, rhs))
    }

    pub(crate) const fn should_emit_declare_keyword(&self, is_exported: bool) -> bool {
        !(self.inside_declare_namespace
            || self.source_is_declaration_file
            || (self.source_is_js_file && is_exported))
    }

    /// Whether an exported declaration should emit its `export` keyword.
    ///
    /// Source ambient external modules (`declare module "pkg"`) already expose
    /// their members through the module body, so `tsc` strips member `export`
    /// keywords even when the body mixes exported and non-exported declarations.
    ///
    /// Inside a `declare namespace` (including non-ambient namespaces that
    /// gain `declare` in the .d.ts output), `export` is only emitted when the
    /// namespace body has a mix of exported and non-exported members (i.e., a
    /// "scope marker" is present). Outside a `declare namespace`, `export` is
    /// always emitted.
    ///
    /// String-named ambient modules (`declare module "foo"`) follow the same
    /// rule: `export` on individual members is only preserved when the body
    /// carries a scope marker (`export {}`); without one, all members are
    /// implicitly accessible and the keyword is stripped.
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

    pub(crate) fn emit_js_namespace_export_aliases_for_name(
        &mut self,
        name_idx: NodeIndex,
        is_exported: bool,
    ) {
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
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("namespace ");
        self.emit_node(name_idx);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        let mut import_alias_exports = Vec::new();
        let mut plain_exports = Vec::new();
        let mut renamed_exports = Vec::new();
        for alias in aliases {
            let export_name = alias.export_name;
            let local_name = self
                .js_shadowed_export_equals_local_aliases
                .get(&alias.local_name)
                .cloned()
                .unwrap_or(alias.local_name);
            if alias.use_import_alias {
                import_alias_exports.push((export_name, local_name));
                continue;
            }
            if export_name == local_name {
                plain_exports.push(local_name);
            } else {
                renamed_exports.push((export_name, local_name));
            }
        }

        for (export_name, local_name) in import_alias_exports {
            self.write_indent();
            self.write("import ");
            self.write(&export_name);
            self.write(" = ");
            self.write(&local_name);
            self.write(";");
            self.write_line();

            self.write_indent();
            self.write("export { ");
            self.write(&export_name);
            self.write(" };");
            self.write_line();
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
        self.emit_deferred_js_namespace_alias_declarations_for_name(&name);
    }

    fn emit_deferred_js_namespace_alias_declarations_for_name(&mut self, root_name: &str) {
        let Some(stmt_idxs) = self
            .js_deferred_namespace_alias_declarations
            .get(root_name)
            .cloned()
        else {
            return;
        };

        for stmt_idx in stmt_idxs {
            if self
                .js_deferred_namespace_alias_declaration_stmts
                .remove(&stmt_idx)
            {
                self.emit_statement(stmt_idx);
                self.js_deferred_namespace_alias_declaration_stmts
                    .insert(stmt_idx);
            }
        }
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

    pub(in crate::declaration_emitter) fn js_commonjs_export_equals_name(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<String> {
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

        let rhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        if let Some(name) = self.get_identifier_text(rhs) {
            return Some(name);
        }

        let rhs_node = self.arena.get(rhs)?;
        if rhs_node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            let class = self.arena.get_class(rhs_node)?;
            return self.get_identifier_text(class.name);
        }

        None
    }

    pub(in crate::declaration_emitter) fn js_namespace_class_expando_for_statement(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<(String, NodeIndex, NodeIndex)> {
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
        let member_name = self.get_identifier_text(lhs_access.name_or_argument)?;
        if member_name == "prototype" {
            return None;
        }

        let receiver = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(lhs_access.expression);
        let root_name = self
            .get_identifier_text(receiver)
            .filter(|name| name != "exports" && name != "module")
            .or_else(|| self.module_exports_property_reference_name(receiver))?;

        let rhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        let rhs_node = self.arena.get(rhs)?;
        if rhs_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return None;
        }
        let class = self.arena.get_class(rhs_node)?;
        if let Some(class_name) = self.get_identifier_text(class.name)
            && class_name != member_name
        {
            return None;
        }

        Some((root_name, lhs_access.name_or_argument, rhs))
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
        let receiver = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(lhs_access.expression);
        let (root_name, is_prototype) =
            self.js_commonjs_expando_receiver(receiver, js_export_equals_names)?;

        if is_prototype {
            if self.is_js_function_initializer(rhs) {
                return Some((
                    root_name,
                    lhs_access.name_or_argument,
                    rhs,
                    JsCommonjsExpandoDeclKind::PrototypeMethod,
                ));
            }
            return None;
        }

        if self.is_js_function_initializer(rhs) {
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
        self.js_commonjs_named_export_for_statement_with_options(stmt_idx, true)
    }

    pub(in crate::declaration_emitter) fn module_exports_property_reference_name(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(expr_node)?;
        let receiver = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(access.expression);
        if !self.is_module_exports_reference(receiver) {
            return None;
        }
        self.get_identifier_text(access.name_or_argument)
    }

    pub(in crate::declaration_emitter) fn js_namespace_export_alias_for_statement(
        &self,
        stmt_idx: NodeIndex,
        commonjs_root: Option<&str>,
    ) -> Option<(String, String, String, bool)> {
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

        let rhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.right);
        let module_exports_local_name = self.module_exports_property_reference_name(rhs);
        let local_name = self
            .get_identifier_text(rhs)
            .or_else(|| module_exports_local_name.clone())?;
        let (root_name, export_name) =
            self.js_namespace_export_alias_target(binary.left, commonjs_root)?;
        Some((
            root_name,
            export_name,
            local_name,
            module_exports_local_name.is_some(),
        ))
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

        if let Some(root_name) = self.module_exports_property_reference_name(receiver_idx) {
            return Some((root_name, export_name));
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
        let is_supported = self.is_js_function_initializer(initializer)
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
            if !self.arena.is_static(&method.modifiers) {
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

    pub(in crate::declaration_emitter) fn is_module_exports_reference(
        &self,
        idx: NodeIndex,
    ) -> bool {
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

    pub(in crate::declaration_emitter) fn is_exports_identifier_reference(
        &self,
        idx: NodeIndex,
    ) -> bool {
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
        Self::push_js_namespace_export_alias_with_kind(
            aliases,
            root_name,
            export_name,
            local_name,
            false,
        );
    }

    pub(in crate::declaration_emitter) fn push_js_namespace_export_alias_with_kind(
        aliases: &mut JsNamespaceExportAliases,
        root_name: &str,
        export_name: String,
        local_name: String,
        use_import_alias: bool,
    ) {
        let entry = aliases.entry(root_name.to_string()).or_default();
        if !entry.iter().any(|existing| {
            existing.export_name == export_name
                && existing.local_name == local_name
                && existing.use_import_alias == use_import_alias
        }) {
            entry.push(JsNamespaceExportAlias {
                export_name,
                local_name,
                use_import_alias,
            });
        }
    }
}
