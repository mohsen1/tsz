//! JS export collection - local export aliases, enums, interfaces, and class members.

#[allow(unused_imports)]
use super::super::{
    DeclarationEmitter, ImportPlan, JsNestedModuleExportNamespaces, PlannedImportModule,
    PlannedImportSymbol,
};
#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

use super::js_exports::JsLocalNamedExportPlan;
use super::{
    JsClassDefinePropertyAccessor, JsClassDefinePropertySetter, JsClassLikePrototypeMembers,
    JsClassStaticMembers, JsCommonjsExpandoDeclKind, JsCommonjsExpandoDeclarations,
    JsCommonjsNamedExports, JsStaticMethodAugmentationEntry, JsStaticMethodAugmentationGroup,
    JsStaticMethodAugmentations, JsStaticMethodInfo, JsStaticMethodKey,
};

impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn collect_js_local_export_aliases(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> (Vec<NodeIndex>, FxHashSet<NodeIndex>) {
        let mut aliases = Vec::new();
        let mut skipped = FxHashSet::default();
        if !self.source_file_is_js(source_file) {
            return (aliases, skipped);
        }
        let export_targets = self.collect_js_named_export_targets(source_file);
        let enum_targets = self.js_local_enum_targets_by_name(source_file);
        let interface_targets = self.js_local_interface_targets_by_name(source_file);

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
            if export.is_default_export || export.is_type_only || export.module_specifier.is_some()
            {
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
            if named.name.is_some() || named.elements.nodes.is_empty() {
                continue;
            }
            if let Some(plan) = self.js_local_named_export_plan(
                named,
                &export_targets,
                &enum_targets,
                &interface_targets,
            ) {
                aliases.extend(plan.alias_specifiers.iter().copied());
                if !plan.alias_specifiers.is_empty() && plan.folded_names.is_empty() {
                    skipped.insert(stmt_idx);
                }
            }
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
            if export.is_default_export || export.is_type_only || export.module_specifier.is_some()
            {
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
            let all_plain_enum_exports = named.elements.nodes.iter().copied().all(|spec_idx| {
                self.arena
                    .get(spec_idx)
                    .and_then(|spec_node| self.arena.get_specifier(spec_node))
                    .and_then(|spec| {
                        if spec.property_name.is_some() || spec.is_type_only {
                            return None;
                        }
                        self.get_identifier_text(spec.name)
                    })
                    .is_some_and(|name| enum_targets.contains_key(&name))
            });
            if all_plain_enum_exports {
                skipped.insert(stmt_idx);
            }
        }

        (aliases, skipped)
    }

    pub(crate) fn collect_js_deferred_local_export_alias_function_statements(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> FxHashSet<NodeIndex> {
        let mut deferred = FxHashSet::default();
        if !self.source_file_is_js(source_file) {
            return deferred;
        }

        let export_targets = self.collect_js_named_export_targets(source_file);
        if !self.source_file_has_native_esm_syntax(source_file)
            && self.js_export_equals_names.is_empty()
        {
            for &stmt_idx in &source_file.statements.nodes {
                let Some((name_idx, initializer)) =
                    self.js_commonjs_named_export_for_statement_with_options(stmt_idx, true)
                else {
                    continue;
                };
                if self.js_commonjs_export_name_text(name_idx).is_none() {
                    continue;
                }
                let Some(local_name) = self.get_identifier_text(initializer) else {
                    continue;
                };
                let Some(&target_stmt_idx) = export_targets.get(&local_name) else {
                    continue;
                };
                if self.js_function_declaration_has_signature_jsdoc(target_stmt_idx) {
                    deferred.insert(target_stmt_idx);
                }
            }
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
            if export.is_default_export || export.is_type_only || export.module_specifier.is_some()
            {
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
            for &spec_idx in &named.elements.nodes {
                let Some(spec) = self
                    .arena
                    .get(spec_idx)
                    .and_then(|spec_node| self.arena.get_specifier(spec_node))
                else {
                    continue;
                };
                if spec.is_type_only {
                    continue;
                }
                let local_name_idx = if spec.property_name.is_some() {
                    spec.property_name
                } else {
                    spec.name
                };
                let Some(local_name) = self.get_identifier_text(local_name_idx) else {
                    continue;
                };
                let Some(&target_stmt_idx) = export_targets.get(&local_name) else {
                    continue;
                };
                if self.js_function_declaration_has_signature_jsdoc(target_stmt_idx)
                    || self.js_unexported_class_declaration_statement(target_stmt_idx)
                {
                    deferred.insert(target_stmt_idx);
                }
            }
        }

        deferred
    }

    fn js_unexported_class_declaration_statement(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION
            && !self.stmt_has_export_modifier(stmt_node)
    }

    fn js_function_declaration_has_signature_jsdoc(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
            return false;
        }
        self.leading_jsdoc_comment_chain_for_pos(stmt_node.pos)
            .iter()
            .any(|jsdoc| Self::jsdoc_has_function_signature_tags(jsdoc))
    }

    pub(crate) fn collect_js_local_export_enum_statements(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> FxHashSet<NodeIndex> {
        let mut deferred = FxHashSet::default();
        if !self.source_file_is_js(source_file) {
            return deferred;
        }
        let enum_targets = self.js_local_enum_targets_by_name(source_file);
        if enum_targets.is_empty() {
            return deferred;
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
            if export.is_default_export || export.is_type_only || export.module_specifier.is_some()
            {
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
            for &spec_idx in &named.elements.nodes {
                let Some(spec) = self
                    .arena
                    .get(spec_idx)
                    .and_then(|spec_node| self.arena.get_specifier(spec_node))
                else {
                    continue;
                };
                if spec.is_type_only {
                    continue;
                }
                let local_name_idx = if spec.property_name.is_some() {
                    spec.property_name
                } else {
                    spec.name
                };
                let Some(local_name) = self.get_identifier_text(local_name_idx) else {
                    continue;
                };
                if let Some(&enum_stmt) = enum_targets.get(&local_name) {
                    deferred.insert(enum_stmt);
                }
            }
        }

        deferred
    }

    pub(crate) fn collect_js_local_export_interface_statements(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> (FxHashSet<NodeIndex>, FxHashSet<NodeIndex>) {
        let mut deferred = FxHashSet::default();
        let mut skipped_exports = FxHashSet::default();
        if !self.source_file_is_js(source_file) {
            return (deferred, skipped_exports);
        }
        let interface_targets = self.js_local_interface_targets_by_name(source_file);
        if interface_targets.is_empty() {
            return (deferred, skipped_exports);
        }
        let export_targets = self.collect_js_named_export_targets(source_file);
        let enum_targets = self.js_local_enum_targets_by_name(source_file);

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
            if export.is_default_export || export.is_type_only || export.module_specifier.is_some()
            {
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
            let Some(plan) = self.js_local_named_export_plan(
                named,
                &export_targets,
                &enum_targets,
                &interface_targets,
            ) else {
                continue;
            };
            if plan.interface_statements.is_empty() {
                continue;
            }
            if !plan.folded_names.is_empty() {
                continue;
            }
            deferred.extend(plan.interface_statements.iter().copied());
            if plan.folded_names.is_empty() && plan.alias_specifiers.is_empty() {
                skipped_exports.insert(stmt_idx);
            }
        }

        (deferred, skipped_exports)
    }

    pub(crate) fn emit_deferred_js_local_export_enum_statements(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if self.js_deferred_local_export_enum_statements.is_empty() {
            return;
        }
        for &stmt_idx in &source_file.statements.nodes {
            if !self
                .js_deferred_local_export_enum_statements
                .contains(&stmt_idx)
            {
                continue;
            }
            if self.js_local_enum_has_plain_export(stmt_idx, source_file) {
                self.emit_exported_enum(stmt_idx);
                self.emitted_module_indicator = true;
            } else {
                self.emit_enum_declaration(stmt_idx);
            }
        }
    }

    pub(crate) fn emit_deferred_js_local_export_interface_statements(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if self
            .js_deferred_local_export_interface_statements
            .is_empty()
        {
            return;
        }
        for &stmt_idx in &source_file.statements.nodes {
            if !self
                .js_deferred_local_export_interface_statements
                .contains(&stmt_idx)
            {
                continue;
            }
            if self.js_local_interface_has_plain_export(stmt_idx, source_file) {
                self.emit_exported_interface(stmt_idx);
                self.emitted_module_indicator = true;
            } else {
                self.emit_interface_declaration(stmt_idx);
            }
        }
    }

    pub(crate) fn emit_deferred_js_local_export_alias_function_statements(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if self
            .js_deferred_local_export_alias_function_statements
            .is_empty()
        {
            return;
        }
        for &stmt_idx in &source_file.statements.nodes {
            if !self
                .js_deferred_local_export_alias_function_statements
                .contains(&stmt_idx)
            {
                continue;
            }
            if self
                .arena
                .get(stmt_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::FUNCTION_DECLARATION)
            {
                self.emit_hoisted_js_function_statement(stmt_idx);
            } else {
                self.emit_deferred_js_named_export_statement(stmt_idx);
            }
        }
    }

    pub(in crate::declaration_emitter) fn js_local_enum_targets_by_name(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> FxHashMap<String, NodeIndex> {
        let mut targets = FxHashMap::default();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::ENUM_DECLARATION {
                continue;
            }
            let Some(enum_data) = self.arena.get_enum(stmt_node) else {
                continue;
            };
            if self
                .arena
                .has_modifier(&enum_data.modifiers, SyntaxKind::ExportKeyword)
            {
                continue;
            }
            if let Some(name) = self.get_identifier_text(enum_data.name) {
                targets.insert(name, stmt_idx);
            }
        }
        targets
    }

    pub(in crate::declaration_emitter) fn js_local_interface_targets_by_name(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> FxHashMap<String, NodeIndex> {
        let mut targets = FxHashMap::default();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::INTERFACE_DECLARATION {
                continue;
            }
            let Some(iface) = self.arena.get_interface(stmt_node) else {
                continue;
            };
            if self
                .arena
                .has_modifier(&iface.modifiers, SyntaxKind::ExportKeyword)
            {
                continue;
            }
            if let Some(name) = self.get_identifier_text(iface.name) {
                targets.insert(name, stmt_idx);
            }
        }
        targets
    }

    fn js_local_enum_has_plain_export(
        &self,
        enum_stmt: NodeIndex,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        let Some(enum_name) = self
            .arena
            .get(enum_stmt)
            .and_then(|node| self.arena.get_enum(node))
            .and_then(|enum_data| self.get_identifier_text(enum_data.name))
        else {
            return false;
        };

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
            if export.is_default_export || export.is_type_only || export.module_specifier.is_some()
            {
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
            for &spec_idx in &named.elements.nodes {
                let Some(spec) = self
                    .arena
                    .get(spec_idx)
                    .and_then(|spec_node| self.arena.get_specifier(spec_node))
                else {
                    continue;
                };
                if spec.is_type_only || spec.property_name.is_some() {
                    continue;
                }
                if self.get_identifier_text(spec.name).as_deref() == Some(enum_name.as_str()) {
                    return true;
                }
            }
        }
        false
    }

    fn js_local_interface_has_plain_export(
        &self,
        interface_stmt: NodeIndex,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        let Some(interface_name) = self
            .arena
            .get(interface_stmt)
            .and_then(|node| self.arena.get_interface(node))
            .and_then(|iface| self.get_identifier_text(iface.name))
        else {
            return false;
        };

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
            if export.is_default_export || export.is_type_only || export.module_specifier.is_some()
            {
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
            for &spec_idx in &named.elements.nodes {
                let Some(spec) = self
                    .arena
                    .get(spec_idx)
                    .and_then(|spec_node| self.arena.get_specifier(spec_node))
                else {
                    continue;
                };
                if spec.is_type_only || spec.property_name.is_some() {
                    continue;
                }
                if self.get_identifier_text(spec.name).as_deref() == Some(interface_name.as_str()) {
                    return true;
                }
            }
        }
        false
    }

    pub(in crate::declaration_emitter) fn js_local_named_export_plan(
        &self,
        named: &tsz_parser::parser::node::NamedImportsData,
        export_targets: &FxHashMap<String, NodeIndex>,
        enum_targets: &FxHashMap<String, NodeIndex>,
        interface_targets: &FxHashMap<String, NodeIndex>,
    ) -> Option<JsLocalNamedExportPlan> {
        if named.name.is_some() || named.elements.nodes.is_empty() {
            return None;
        }

        let mut plan = JsLocalNamedExportPlan::default();
        let mut seen_folded_targets = FxHashSet::default();
        let mut seen_interface_targets = FxHashSet::default();

        for &spec_idx in &named.elements.nodes {
            let spec = self
                .arena
                .get(spec_idx)
                .and_then(|spec_node| self.arena.get_specifier(spec_node))?;
            if spec.is_type_only {
                return None;
            }

            let local_name_idx = if spec.property_name.is_some() {
                spec.property_name
            } else {
                spec.name
            };
            let local_name = self.get_identifier_text(local_name_idx)?;

            if let Some(&interface_stmt) = interface_targets.get(&local_name) {
                if seen_interface_targets.insert(interface_stmt) {
                    plan.interface_statements.push(interface_stmt);
                }
                if spec.property_name.is_some() {
                    plan.alias_specifiers.push(spec_idx);
                } else {
                    plan.plain_interface_names.push(local_name);
                }
                continue;
            }

            if spec.property_name.is_some() {
                plan.alias_specifiers.push(spec_idx);
                continue;
            }

            if let Some(&target_stmt_idx) = export_targets.get(&local_name) {
                plan.folded_names.push(local_name);
                if seen_folded_targets.insert(target_stmt_idx) {
                    plan.folded_target_statements.push(target_stmt_idx);
                }
                continue;
            }

            if enum_targets.contains_key(&local_name) {
                continue;
            }

            return None;
        }

        Some(plan)
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
        let local_name = self
            .get_identifier_text(rhs)
            .or_else(|| self.module_exports_property_reference_name(rhs))?;
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
        if self.source_file_has_native_esm_syntax(source_file) {
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

        // First, collect all top-level names that may acquire a prototype
        // surface through `Name.prototype.member = ...` assignments.
        let mut top_level_names: FxHashSet<String> = FxHashSet::default();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            match stmt_node.kind {
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
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
                                && !js_export_equals_names.contains(&name)
                            {
                                top_level_names.insert(name);
                            }
                        }
                    }
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    if let Some(func) = self.arena.get_function(stmt_node)
                        && let Some(name) = self.get_identifier_text(func.name)
                        && !js_export_equals_names.contains(&name)
                    {
                        top_level_names.insert(name);
                    }
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(class) = self.arena.get_class(stmt_node)
                        && let Some(name) = self.get_identifier_text(class.name)
                        && !js_export_equals_names.contains(&name)
                    {
                        top_level_names.insert(name);
                    }
                }
                _ => {}
            }
        }

        if top_level_names.is_empty() {
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
            if !top_level_names.contains(&root_name) {
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

    pub(crate) fn collect_js_class_static_members(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> crate::declaration_emitter::helpers::JsClassStaticMembers {
        let mut result = crate::declaration_emitter::helpers::JsClassStaticMembers::default();
        if !self.source_file_is_js(source_file) {
            return result;
        }

        let mut top_level_names = FxHashSet::default();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            match stmt_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    if let Some(func) = self.arena.get_function(stmt_node)
                        && let Some(name) = self.get_identifier_text(func.name)
                    {
                        top_level_names.insert(name);
                    }
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(class) = self.arena.get_class(stmt_node)
                        && let Some(name) = self.get_identifier_text(class.name)
                    {
                        top_level_names.insert(name);
                    }
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
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
                            if self.is_js_function_initializer(decl.initializer)
                                && let Some(name) = self.get_identifier_text(decl.name)
                            {
                                top_level_names.insert(name);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if top_level_names.is_empty() {
            return result;
        }

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
            let Some(lhs_node) = self.arena.get(lhs) else {
                continue;
            };
            if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                continue;
            }
            let Some(lhs_access) = self.arena.get_access_expr(lhs_node) else {
                continue;
            };
            if self
                .get_identifier_text(lhs_access.name_or_argument)
                .as_deref()
                == Some("prototype")
            {
                continue;
            }
            let Some(root_name) = self.get_identifier_text(lhs_access.expression) else {
                continue;
            };
            if self.js_export_equals_names.contains(&root_name) {
                continue;
            }
            if !top_level_names.contains(&root_name) {
                continue;
            }
            let rhs = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(binary.right);
            if !(self.is_js_function_initializer(rhs)
                || self.js_namespace_object_member_initializer_supported(rhs))
            {
                continue;
            }
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
        if self.source_file_has_native_esm_syntax(source_file) {
            return (exported_names, function_statements, value_statements);
        }
        if !self.js_export_equals_names.is_empty() {
            return (exported_names, function_statements, value_statements);
        }
        if self.source_file_has_commonjs_export_equals_require_alias_property(source_file) {
            return (exported_names, function_statements, value_statements);
        }

        let export_targets = self.collect_js_named_export_targets(source_file);

        for &stmt_idx in &source_file.statements.nodes {
            let Some((name_idx, initializer)) =
                self.js_supported_commonjs_named_export_for_statement(stmt_idx)
            else {
                continue;
            };
            let Some(export_name) = self.js_commonjs_export_name_text(name_idx) else {
                continue;
            };

            if let Some(local_name) = self.get_identifier_text(initializer)
                && local_name == export_name
                && export_targets.contains_key(&local_name)
            {
                exported_names.insert(local_name);
                continue;
            }

            if self.is_js_function_initializer(initializer) {
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
        if self.source_file_has_native_esm_syntax(source_file) {
            return empty;
        }

        let mut top_level_names = FxHashSet::default();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if let Some(name) = self.extract_declaration_name(stmt_idx) {
                top_level_names.insert(name);
                continue;
            }
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
                    if let Some(name) = self.get_identifier_text(decl.name) {
                        top_level_names.insert(name);
                    }
                }
            }
        }

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
                    if let Some(data) = self.arena.get_shorthand_property(member_node)
                        && let Some(name) = self.get_identifier_text(data.name)
                        && top_level_names.contains(&name)
                    {
                        names.insert(name);
                        found_any = true;
                    }
                }
                // Handle property assignments: `{ FancyError: FancyError }`
                else if member_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    && let Some(prop) = self.arena.get_property_assignment(member_node)
                    && let Some(prop_name) = self.get_identifier_text(prop.name)
                    && let Some(init_name) = self.get_identifier_text(prop.initializer)
                    && prop_name == init_name
                    && top_level_names.contains(&prop_name)
                {
                    names.insert(prop_name);
                    found_any = true;
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

        if let Some(export_name) = self.js_commonjs_export_name_text(name_idx)
            && let Some(local_name) = self.get_identifier_text(initializer)
            && local_name == export_name
        {
            return Some((name_idx, initializer));
        }

        if self.js_commonjs_void_zero_export_init(initializer) {
            return None;
        }

        if self.is_js_function_initializer(initializer) {
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

    pub(in crate::declaration_emitter) fn js_commonjs_named_export_value_initializer_supported(
        &self,
        initializer: NodeIndex,
    ) -> bool {
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
        let Some(export_name) = self.js_commonjs_export_name_text(export_name_idx) else {
            return false;
        };
        self.get_identifier_text(class.name)
            .is_some_and(|class_name| class_name == export_name)
    }

    pub(in crate::declaration_emitter) fn js_commonjs_void_zero_export_init(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
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
            if let Some((class_name, method_name, _, _)) =
                self.js_class_static_method_augmentation_for_statement(stmt_idx)
                && static_methods.contains_key(&(class_name, method_name))
                && !augmentations.statements.contains_key(&stmt_idx)
            {
                augmentations.skipped_statements.insert(stmt_idx);
            }
        }

        augmentations
    }

    pub(crate) fn collect_js_class_define_property_accessors(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> (
        FxHashMap<String, Vec<JsClassDefinePropertyAccessor>>,
        FxHashSet<NodeIndex>,
    ) {
        let mut accessors: FxHashMap<String, Vec<JsClassDefinePropertyAccessor>> =
            FxHashMap::default();
        let mut consumed = FxHashSet::default();
        if !self.source_file_is_js(source_file) {
            return (accessors, consumed);
        }

        let class_names = self.js_top_level_class_names(source_file);
        if class_names.is_empty() {
            return (accessors, consumed);
        }

        for &stmt_idx in &source_file.statements.nodes {
            let Some((class_name, accessor)) =
                self.js_class_define_property_accessor_for_statement(stmt_idx, &class_names)
            else {
                continue;
            };
            accessors.entry(class_name).or_default().push(accessor);
            consumed.insert(stmt_idx);
        }

        (accessors, consumed)
    }

    fn js_top_level_class_names(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> FxHashSet<String> {
        let mut names = FxHashSet::default();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION {
                if let Some(class) = self.arena.get_class(stmt_node)
                    && let Some(name) = self.get_identifier_text(class.name)
                {
                    names.insert(name);
                }
                continue;
            }
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            let Some(clause_node) = self.arena.get(export.export_clause) else {
                continue;
            };
            if clause_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                continue;
            }
            if let Some(class) = self.arena.get_class(clause_node)
                && let Some(name) = self.get_identifier_text(class.name)
            {
                names.insert(name);
            }
        }
        names
    }

    fn js_class_define_property_accessor_for_statement(
        &self,
        stmt_idx: NodeIndex,
        class_names: &FxHashSet<String>,
    ) -> Option<(String, JsClassDefinePropertyAccessor)> {
        let stmt_node = self.arena.get(stmt_idx)?;
        if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return None;
        }
        let expr_stmt = self.arena.get_expression_statement(stmt_node)?;
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_stmt.expression);
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(expr_node)?;
        let args = call.arguments.as_ref()?;
        if !self.is_object_define_property_call(call.expression) || args.nodes.len() < 3 {
            return None;
        }

        let target = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(args.nodes[0]);
        let target_node = self.arena.get(target)?;
        if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let target_access = self.arena.get_access_expr(target_node)?;
        if self
            .get_identifier_text(target_access.name_or_argument)
            .as_deref()
            != Some("prototype")
        {
            return None;
        }
        let class_name = self.get_identifier_text(target_access.expression)?;
        if !class_names.contains(&class_name) {
            return None;
        }

        let property_name = self.js_define_property_name(args.nodes[1])?;
        let descriptor_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(args.nodes[2]);
        let descriptor_node = self.arena.get(descriptor_idx)?;
        if descriptor_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let descriptor = self.arena.get_literal_expr(descriptor_node)?;
        let mut getter = None;
        let mut setter = None;

        for &member_idx in &descriptor.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let Some(method) = self.arena.get_method_decl(member_node) else {
                    continue;
                };
                match self.get_identifier_text(method.name).as_deref() {
                    Some("get") => getter = Some(member_idx),
                    Some("set") => {
                        setter = Some(JsClassDefinePropertySetter {
                            initializer: member_idx,
                            preserve_param_name: true,
                        });
                    }
                    _ => {}
                }
                continue;
            }
            if member_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                continue;
            }
            let Some(prop) = self.arena.get_property_assignment(member_node) else {
                continue;
            };
            match self.get_identifier_text(prop.name).as_deref() {
                Some("get") => getter = Some(prop.initializer),
                Some("set") => {
                    setter = Some(JsClassDefinePropertySetter {
                        initializer: prop.initializer,
                        preserve_param_name: false,
                    });
                }
                _ => {}
            }
        }

        if getter.is_none() && setter.is_none() {
            return None;
        }

        Some((
            class_name,
            JsClassDefinePropertyAccessor {
                property_name,
                getter,
                setter,
            },
        ))
    }
}
