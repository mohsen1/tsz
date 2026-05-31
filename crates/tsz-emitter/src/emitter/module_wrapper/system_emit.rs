use super::super::Printer;
use super::system_legacy_class_decorators::split_system_class_static_tail;
use super::{SystemDependencyAction, SystemDependencyPlan};
use crate::emitter::{JsxEmit, ModuleKind};
use std::collections::{HashMap, HashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(super) fn emit_system_setters(
        &mut self,
        dependencies: &[String],
        dep_vars: &HashMap<String, String>,
        system_plan: &SystemDependencyPlan,
    ) {
        if dependencies.is_empty() {
            self.write("setters: [],");
            return;
        }

        self.write("setters: [");
        self.write_line();
        self.increase_indent();
        for (idx, dep) in dependencies.iter().enumerate() {
            let actions = system_plan.actions.get(dep);
            let dep_var = actions.and_then(|_| dep_vars.get(dep));
            if actions.is_some() && dep_var.is_none() {
                continue;
            }
            self.write("function (");
            if let Some(dep_var) = dep_var {
                self.write(dep_var);
                self.write("_1");
            } else {
                self.write("_1");
            }
            self.write(") {");
            self.write_line();
            self.increase_indent();
            if let Some(actions) = actions {
                let Some(dep_var) = dep_var else {
                    continue;
                };
                for action in actions {
                    match action {
                        SystemDependencyAction::Assign(local_name) => {
                            self.write(local_name);
                            self.write(" = ");
                            self.write(dep_var);
                            self.write("_1;");
                            self.write_line();
                        }
                        SystemDependencyAction::ExportStar => {
                            self.write("exportStar_1(");
                            self.write(dep_var);
                            self.write("_1);");
                            self.write_line();
                        }
                        SystemDependencyAction::NamedExports(exports) => {
                            self.write("exports_1({");
                            self.write_line();
                            self.increase_indent();
                            for (export_idx, (export_name, import_name)) in
                                exports.iter().enumerate()
                            {
                                self.write("\"");
                                self.write(export_name);
                                self.write("\": ");
                                let setter_arg = format!("{dep_var}_1");
                                self.write(&setter_arg);
                                self.write("[\"");
                                self.write(import_name);
                                self.write("\"]");
                                if export_idx + 1 != exports.len() {
                                    self.write(",");
                                }
                                self.write_line();
                            }
                            self.decrease_indent();
                            self.write("});");
                            self.write_line();
                        }
                        SystemDependencyAction::NamespaceExport(export_name) => {
                            self.write("exports_1(\"");
                            self.write(export_name);
                            self.write("\", ");
                            self.write(dep_var);
                            self.write("_1);");
                            self.write_line();
                        }
                    }
                }
            }
            self.decrease_indent();
            self.write("}");
            if idx + 1 != dependencies.len() {
                self.write(",");
            }
            self.write_line();
        }
        self.decrease_indent();
        self.write("],");
    }

    pub(super) fn emit_system_export_star_helpers_if_needed(
        &mut self,
        source: &tsz_parser::parser::node::SourceFileData,
        system_plan: &SystemDependencyPlan,
    ) {
        let has_export_star = system_plan.actions.values().any(|actions| {
            actions
                .iter()
                .any(|action| matches!(action, SystemDependencyAction::ExportStar))
        });
        if !has_export_star {
            return;
        }

        let (excluded_names, emit_exclusion_map) =
            self.collect_system_export_star_excluded_names(source);
        if emit_exclusion_map {
            if excluded_names.is_empty() {
                self.write("var exportedNames_1 = {};");
                self.write_line();
            } else {
                self.write("var exportedNames_1 = {");
                self.write_line();
                self.increase_indent();
                for (idx, export_name) in excluded_names.iter().enumerate() {
                    self.write("\"");
                    self.emit_escaped_string(export_name, '"');
                    self.write("\": true");
                    if idx + 1 != excluded_names.len() {
                        self.write(",");
                    }
                    self.write_line();
                }
                self.decrease_indent();
                self.write("};");
                self.write_line();
            }
        }

        self.write("function exportStar_1(m) {");
        self.write_line();
        self.increase_indent();
        self.write("var exports = {};");
        self.write_line();
        self.write("for (var n in m) {");
        self.write_line();
        self.increase_indent();
        if emit_exclusion_map {
            self.write(
                "if (n !== \"default\" && !exportedNames_1.hasOwnProperty(n)) exports[n] = m[n];",
            );
        } else {
            self.write("if (n !== \"default\") exports[n] = m[n];");
        }
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.write("exports_1(exports);");
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();
    }

    pub(super) fn register_system_import_substitutions(
        &mut self,
        source: &tsz_parser::parser::node::SourceFileData,
        dep_vars: &HashMap<String, String>,
        system_plan: &SystemDependencyPlan,
    ) {
        self.commonjs_named_import_substitutions.clear();

        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }
            let Some(import_decl) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            if !self.import_decl_has_runtime_value(import_decl) {
                continue;
            }
            let Some(module_spec) = self.system_module_specifier_text(import_decl.module_specifier)
            else {
                continue;
            };
            let dep_var = if let Some(dep_var) = system_plan.import_vars.get(&stmt_node.pos) {
                dep_var
            } else if let Some(dep_var) = dep_vars.get(&module_spec) {
                dep_var
            } else {
                continue;
            };
            let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };
            if clause.is_type_only {
                continue;
            }

            if clause.name.is_some() {
                let local_name = self.get_identifier_text_idx(clause.name);
                if !local_name.is_empty() {
                    self.commonjs_named_import_substitutions
                        .insert(local_name, format!("{dep_var}.default"));
                }
            }

            if clause.named_bindings.is_none() {
                continue;
            }
            let Some(bindings_node) = self.arena.get(clause.named_bindings) else {
                continue;
            };
            let Some(named_imports) = self.arena.get_named_imports(bindings_node) else {
                continue;
            };

            if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
                let local_name = self.get_identifier_text_idx(named_imports.name);
                if !local_name.is_empty() && clause.name.is_none() {
                    self.commonjs_named_import_substitutions
                        .insert(local_name, dep_var.clone());
                }
                continue;
            }

            for &spec_idx in &named_imports.elements.nodes {
                let Some(spec_node) = self.arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = self.arena.get_specifier(spec_node) else {
                    continue;
                };
                if spec.is_type_only {
                    continue;
                }
                let local_name = self.get_identifier_text_idx(spec.name);
                if local_name.is_empty() {
                    continue;
                }
                let import_name = if spec.property_name.is_some() {
                    self.get_specifier_name_text(spec.property_name)
                        .unwrap_or_else(|| local_name.clone())
                } else {
                    local_name.clone()
                };
                let substitution = if super::super::is_valid_identifier_name(&import_name) {
                    format!("{dep_var}.{import_name}")
                } else {
                    format!("{dep_var}[\"{import_name}\"]")
                };
                self.commonjs_named_import_substitutions
                    .insert(local_name, substitution);
            }
        }
    }

    pub(super) fn emit_system_execute_body(
        &mut self,
        source_node: &tsz_parser::parser::node::Node,
        dep_vars: &HashMap<String, String>,
        hoisted_func_stmts: &HashSet<NodeIndex>,
        system_plan: &SystemDependencyPlan,
    ) {
        let prev_module = self.ctx.options.module;
        let prev_auto_detect = self.ctx.auto_detect_module;
        let prev_original = self.ctx.original_module_kind;
        let prev_jsx_dev_file_name = self.jsx_dev_file_name.clone();

        self.ctx.original_module_kind = Some(prev_module);
        self.ctx.options.module = ModuleKind::CommonJS;
        self.ctx.auto_detect_module = false;
        self.in_system_execute_body = true;

        let Some(source) = self.arena.get_source_file(source_node) else {
            self.ctx.options.module = prev_module;
            self.ctx.auto_detect_module = prev_auto_detect;
            self.ctx.original_module_kind = prev_original;
            self.jsx_dev_file_name = prev_jsx_dev_file_name;
            self.in_system_execute_body = false;
            return;
        };
        if matches!(self.ctx.options.jsx, JsxEmit::ReactJsxDev) {
            self.jsx_dev_file_name = Some(system_jsx_dev_file_name(&source.file_name));
        }
        self.register_system_import_substitutions(source, dep_vars, system_plan);

        self.install_system_local_export_bindings(source);
        self.system_folded_export_names.clear();

        if let Some(first_using_idx) = source.statements.nodes.iter().position(|&stmt_idx| {
            self.arena
                .get(stmt_idx)
                .is_some_and(|stmt_node| self.statement_is_top_level_using(stmt_node))
        }) {
            self.emit_system_top_level_using_scope(
                source,
                first_using_idx,
                dep_vars,
                hoisted_func_stmts,
            );
            self.ctx.options.module = prev_module;
            self.ctx.auto_detect_module = prev_auto_detect;
            self.ctx.original_module_kind = prev_original;
            self.jsx_dev_file_name = prev_jsx_dev_file_name;
            self.in_system_execute_body = false;
            return;
        }

        let prev_deferred_local_export_bindings = self
            .deferred_local_export_bindings
            .replace(self.system_reexported_names.clone());

        if matches!(self.ctx.options.jsx, JsxEmit::ReactJsxDev) {
            if let Some(file_name_text) = self.jsx_dev_file_name_text() {
                let assignment = file_name_text
                    .trim()
                    .strip_prefix("const ")
                    .unwrap_or(file_name_text.trim());
                self.write(assignment);
                self.write_line();
            }
        }
        self.emit_system_import_binding_exports(source, system_plan);

        for &stmt_idx in &source.statements.nodes {
            // Skip function declarations that were already hoisted to the outer scope
            if hoisted_func_stmts.contains(&stmt_idx) {
                continue;
            }
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            // Skip "use strict" prologue directives — the System.register callback
            // already emits "use strict" at the module scope (line 315 above), so
            // re-emitting the source's own directive inside execute() would duplicate it.
            if stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                && let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node)
                && let Some(expr_node) = self.arena.get(expr_stmt.expression)
                && expr_node.kind == SyntaxKind::StringLiteral as u16
            {
                let is_use_strict = if let Some(lit) = self.arena.get_literal(expr_node) {
                    lit.text == "use strict"
                } else if let Some(text) = self.source_text {
                    // safe_slice: B (decision-affecting). On a bad span we
                    // treat the directive as not "use strict", which makes us
                    // emit a duplicate rather than silently drop a directive
                    // — the safer of the two failure modes. The fallible API
                    // surfaces span errors via tracing::debug! for diagnosis.
                    match crate::safe_slice::slice(
                        text,
                        expr_node.pos as usize,
                        expr_node.end as usize,
                    ) {
                        Ok(s) => s == "\"use strict\"" || s == "'use strict'",
                        Err(_) => false,
                    }
                } else {
                    false
                };
                if is_use_strict {
                    continue;
                }
            }
            if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }
            if stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                let before_len = self.writer.len();
                if self.emit_system_import_equals_declaration(stmt_node, dep_vars, false)
                    && self.writer.len() > before_len
                {
                    self.write_line();
                }
                continue;
            }
            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_decl) = self.arena.get_export_decl(stmt_node)
                && export_decl.module_specifier.is_some()
            {
                continue;
            }
            if self.is_erased_statement(stmt_node) {
                continue;
            }
            let before_len = self.writer.len();

            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && self.emit_system_export_declaration(stmt_node, dep_vars)
            {
                if self.writer.len() > before_len {
                    self.write_line();
                }
                continue;
            }

            if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                self.emit_system_variable_initializers(stmt_node);
            } else if stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION {
                // Non-exported class declarations: var is hoisted, emit as assignment
                self.emit_system_class_as_expression(stmt_node, stmt_idx);
            } else {
                if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && let Some(module_decl) = self.arena.get_module(stmt_node)
                {
                    if !self.is_instantiated_module(module_decl.body) {
                        continue;
                    }
                    let module_name = self.get_identifier_text_idx(module_decl.name);
                    if !module_name.is_empty() {
                        self.declared_namespace_names.insert(module_name.clone());
                        let export_names = self.system_export_names_for_local(&module_name);
                        if !export_names.is_empty() {
                            self.emit_system_namespace_with_export_fold(
                                stmt_idx,
                                &module_name,
                                export_names,
                            );
                            self.system_folded_export_names.insert(module_name);
                            continue;
                        }
                    }
                }
                if stmt_node.kind == syntax_kind_ext::ENUM_DECLARATION
                    && let Some(enum_decl) = self.arena.get_enum(stmt_node)
                {
                    let enum_name = self.get_identifier_text_idx(enum_decl.name);
                    if !enum_name.is_empty() {
                        self.declared_namespace_names.insert(enum_name.clone());
                        let export_names = self.system_export_names_for_local(&enum_name);
                        if !export_names.is_empty() {
                            self.emit_system_enum_with_export_fold(
                                stmt_idx,
                                &enum_name,
                                export_names,
                            );
                            self.write_line();
                            self.system_folded_export_names.insert(enum_name);
                            continue;
                        }
                    }
                }
                if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    && let Some(export_decl) = self.arena.get_export_decl(stmt_node)
                    && let Some(clause_node) = self.arena.get(export_decl.export_clause)
                {
                    if clause_node.kind == syntax_kind_ext::MODULE_DECLARATION
                        && let Some(module_decl) = self.arena.get_module(clause_node)
                    {
                        let module_name = self.get_identifier_text_idx(module_decl.name);
                        if !module_name.is_empty() {
                            self.declared_namespace_names.insert(module_name);
                        }
                    }
                    if clause_node.kind == syntax_kind_ext::ENUM_DECLARATION
                        && let Some(enum_decl) = self.arena.get_enum(clause_node)
                    {
                        let enum_name = self.get_identifier_text_idx(enum_decl.name);
                        if !enum_name.is_empty() {
                            self.declared_namespace_names.insert(enum_name);
                        }
                    }
                }
                self.emit(stmt_idx);
            }

            let skip_newline = stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION
                || (stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    && self
                        .arena
                        .get_export_decl(stmt_node)
                        .and_then(|ed| self.arena.get(ed.export_clause))
                        .is_some_and(|cn| cn.kind == syntax_kind_ext::MODULE_DECLARATION));
            if self.writer.len() > before_len && !skip_newline {
                self.write_line();
            }
        }

        self.deferred_local_export_bindings = prev_deferred_local_export_bindings;
        self.ctx.options.module = prev_module;
        self.ctx.auto_detect_module = prev_auto_detect;
        self.ctx.original_module_kind = prev_original;
        self.jsx_dev_file_name = prev_jsx_dev_file_name;
        self.in_system_execute_body = false;
    }

    pub(super) fn emit_system_import_equals_declaration(
        &mut self,
        node: &tsz_parser::parser::node::Node,
        dep_vars: &HashMap<String, String>,
        force_exported: bool,
    ) -> bool {
        let Some(import_decl) = self.arena.get_import_decl(node) else {
            return false;
        };
        if !self.import_decl_has_runtime_value(import_decl) || import_decl.import_clause.is_none() {
            return true;
        }

        let local_name = self.get_identifier_text_idx(import_decl.import_clause);
        if local_name.is_empty() {
            return true;
        }

        let is_exported = force_exported
            || self
                .arena
                .has_modifier(&import_decl.modifiers, SyntaxKind::ExportKeyword);
        let is_external = self
            .arena
            .get(import_decl.module_specifier)
            .is_some_and(|module_node| {
                module_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
                    || module_node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
            });

        if is_external
            && let Some(dep) = self.system_module_specifier_text(import_decl.module_specifier)
            && dep_vars.contains_key(&dep)
        {
            if is_exported {
                self.write("exports_1(\"");
                self.write(&local_name);
                self.write("\", ");
                self.write(&local_name);
                self.write(");");
            }
            return true;
        }

        if is_exported {
            self.write("exports_1(\"");
            self.write(&local_name);
            self.write("\", ");
        }
        self.write(&local_name);
        self.write(" = ");
        if is_external {
            if let Some(module_name) =
                self.system_module_specifier_text(import_decl.module_specifier)
            {
                self.write("require(\"");
                self.write(&module_name);
                self.write("\")");
            } else {
                self.emit_entity_name(import_decl.module_specifier);
            }
        } else {
            self.emit_entity_name(import_decl.module_specifier);
        }
        if is_exported {
            self.write(")");
        }
        self.write(";");
        true
    }

    pub(super) fn emit_system_export_declaration(
        &mut self,
        node: &tsz_parser::parser::node::Node,
        dep_vars: &HashMap<String, String>,
    ) -> bool {
        let Some(export_decl) = self.arena.get_export_decl(node) else {
            return false;
        };
        if export_decl.module_specifier.is_some() {
            return false;
        }
        let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
            return false;
        };

        // Handle `export default class {}` — emit `default_1 = class {}; exports_1("default", default_1);`
        if export_decl.is_default_export
            && clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
            && let Some(class_decl) = self.arena.get_class(clause_node)
        {
            let class_name = self.get_identifier_text_idx(class_decl.name);
            let gen_name = if class_name.is_empty() {
                "default_1".to_string()
            } else {
                class_name
            };
            let legacy_class_decorators = self.collect_class_decorators(&class_decl.modifiers);
            if self.ctx.options.legacy_decorators
                && (!legacy_class_decorators.is_empty()
                    || !self
                        .collect_constructor_param_decorators(&class_decl.members.nodes)
                        .is_empty())
            {
                let alias_name = self.system_legacy_decorated_class_alias(
                    export_decl.export_clause,
                    &gen_name,
                    &class_decl.members.nodes,
                );
                let emitted = self.capture_system_class_assignment(
                    clause_node,
                    export_decl.export_clause,
                    &gen_name,
                    alias_name.as_deref(),
                );
                let (class_part, static_tail) = split_system_class_static_tail(&emitted);
                self.write(class_part.trim_start().trim_end());
                if !self.output_ends_with_semicolon() {
                    self.write(";");
                }
                self.write_line();
                if !static_tail.trim().is_empty() {
                    self.write(static_tail.trim());
                    if !self.output_ends_with_semicolon() {
                        self.write(";");
                    }
                    self.write_line();
                }
                let assignment = self.capture_system_legacy_class_decorator_assignment(
                    &gen_name,
                    &legacy_class_decorators,
                    &class_decl.members.nodes,
                    alias_name.as_deref(),
                );
                self.write(&assignment);
                if !self.output_ends_with_semicolon() {
                    self.write(";");
                }
                self.write_line();
                self.write("exports_1(\"default\", ");
                self.write(&gen_name);
                self.write(");");
                return true;
            }
            self.write(&gen_name);
            self.write(" = ");
            self.anonymous_default_export_name = None;
            let deferred = self.emit_system_class_expression_value(
                clause_node,
                export_decl.export_clause,
                true,
            );
            if !self.output_ends_with_semicolon() {
                self.write(";");
            }
            self.write_line();
            // tsc emits static block IIFEs before the default export registration.
            if !deferred.is_empty() {
                self.emit_static_block_iifes(deferred);
            }
            self.write("exports_1(\"default\", ");
            self.write(&gen_name);
            self.write(");");
            return true;
        }

        if export_decl.is_default_export {
            return false;
        }

        if clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
            self.emit_system_variable_initializers(clause_node);
            return true;
        }

        if clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return self.emit_system_import_equals_declaration(clause_node, dep_vars, true);
        }

        if clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
            && let Some(class_decl) = self.arena.get_class(clause_node)
        {
            if self
                .arena
                .has_modifier(&class_decl.modifiers, SyntaxKind::DeclareKeyword)
            {
                return true;
            }
            let class_name = self.get_identifier_text_idx(class_decl.name);
            if class_name.is_empty() {
                return false;
            }
            let legacy_class_decorators = self.collect_class_decorators(&class_decl.modifiers);
            let needs_legacy_class_decorate = self.ctx.options.legacy_decorators
                && (!legacy_class_decorators.is_empty()
                    || !self
                        .collect_constructor_param_decorators(&class_decl.members.nodes)
                        .is_empty());
            let alias_name = needs_legacy_class_decorate
                .then(|| {
                    self.system_legacy_decorated_class_alias(
                        export_decl.export_clause,
                        &class_name,
                        &class_decl.members.nodes,
                    )
                })
                .flatten();
            if needs_legacy_class_decorate {
                // Defer static block IIFEs so we can emit exports_1 before them
                self.defer_class_static_blocks = true;
                self.deferred_class_static_blocks.clear();
                let emitted = self.capture_system_class_assignment(
                    clause_node,
                    export_decl.export_clause,
                    &class_name,
                    alias_name.as_deref(),
                );
                self.defer_class_static_blocks = false;
                let deferred = std::mem::take(&mut self.deferred_class_static_blocks);
                let (class_part, static_tail) = split_system_class_static_tail(&emitted);
                self.write(class_part.trim_start().trim_end());
                if !self.output_ends_with_semicolon() {
                    self.write(";");
                }
                self.write_line();
                self.write("exports_1(\"");
                self.write(&class_name);
                self.write("\", ");
                self.write(&class_name);
                self.write(");");
                if !static_tail.trim().is_empty() {
                    self.write_line();
                    self.write(static_tail.trim());
                    if !self.output_ends_with_semicolon() {
                        self.write(";");
                    }
                }
                self.write_line();
                self.emit_system_legacy_class_decorator_export(
                    &class_name,
                    &class_name,
                    &legacy_class_decorators,
                    &class_decl.members.nodes,
                    alias_name.as_deref(),
                );
                if !deferred.is_empty() {
                    self.emit_static_block_iifes(deferred);
                }
                return true;
            }

            if !self.ctx.target_es5 {
                let emitted = self.capture_system_class_assignment(
                    clause_node,
                    export_decl.export_clause,
                    &class_name,
                    None,
                );
                let (class_part, static_tail) = split_system_class_static_tail(&emitted);
                self.write(class_part.trim_start().trim_end());
                if !self.output_ends_with_semicolon() {
                    self.write(";");
                }
                self.write_line();
                self.write("exports_1(\"");
                self.write(&class_name);
                self.write("\", ");
                self.write(&class_name);
                self.write(");");
                if !static_tail.trim().is_empty() {
                    self.write_line();
                    self.write(static_tail.trim());
                    if !self.output_ends_with_semicolon() {
                        self.write(";");
                    }
                }
                return true;
            }

            self.write(&class_name);
            self.write(" = ");
            let deferred = self.emit_system_class_expression_value(
                clause_node,
                export_decl.export_clause,
                true,
            );
            if !self.output_ends_with_semicolon() {
                self.write(";");
            }
            self.write_line();
            self.write("exports_1(\"");
            self.write(&class_name);
            self.write("\", ");
            self.write(&class_name);
            self.write(");");
            if !deferred.is_empty() {
                self.emit_static_block_iifes(deferred);
            }
            return true;
        }

        if clause_node.kind == syntax_kind_ext::ENUM_DECLARATION
            && let Some(enum_decl) = self.arena.get_enum(clause_node)
        {
            let enum_name = self.get_identifier_text_idx(enum_decl.name);
            if !enum_name.is_empty() {
                self.emit_system_enum_with_export_fold(
                    export_decl.export_clause,
                    &enum_name,
                    vec![enum_name.clone()],
                );
                return true;
            }
        }

        // Exported function declarations: emit the function then register with exports_1
        if clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            && let Some(func_decl) = self.arena.get_function(clause_node)
        {
            let func_name = self.get_identifier_text_idx(func_decl.name);
            if func_name.is_empty() {
                return false;
            }
            self.emit(export_decl.export_clause);
            self.write_line();
            self.write("exports_1(\"");
            let export_name = if export_decl.is_default_export {
                "default"
            } else {
                &func_name
            };
            self.write(export_name);
            self.write("\", ");
            self.write(&func_name);
            self.write(");");
            return true;
        }

        // `export { x, x as y }` — emit `exports_1("x", <value>); exports_1("y", <value>);`
        // where `<value>` is either the import substitution or the local name.
        if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
            && let Some(named_exports) = self.arena.get_named_imports(clause_node)
        {
            let mut emitted_any = false;
            let value_specs = self.collect_local_export_value_specifiers(&named_exports.elements);
            for &spec_idx in &value_specs {
                let Some(spec) = self.arena.get_specifier_at(spec_idx) else {
                    continue;
                };
                let local_name = if spec.property_name.is_some() {
                    self.get_specifier_name_text(spec.property_name)
                } else {
                    self.get_specifier_name_text(spec.name)
                }
                .unwrap_or_default();
                let export_name = self.get_specifier_name_text(spec.name).unwrap_or_default();
                if local_name.is_empty() || export_name.is_empty() {
                    continue;
                }
                if self.system_folded_export_names.contains(&local_name) {
                    continue;
                }
                // Check if the local name has an import substitution
                let value = if let Some(subst) =
                    self.commonjs_named_import_substitutions.get(&local_name)
                {
                    subst.clone()
                } else {
                    if self.system_local_var_export_has_initializer(&local_name) == Some(false) {
                        continue;
                    }
                    local_name
                };
                if emitted_any {
                    self.write_line();
                }
                self.write("exports_1(\"");
                self.write(&export_name);
                self.write("\", ");
                self.write(&value);
                self.write(");");
                emitted_any = true;
            }
            return true;
        }

        false
    }

    fn system_local_var_export_has_initializer(&self, local_name: &str) -> Option<bool> {
        let mut found_uninitialized = false;
        for stmt_idx in self.scope_statements_for_runtime_lookup(None) {
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
                    let mut names = Vec::new();
                    self.collect_binding_names(decl.name, &mut names);
                    if names.iter().any(|name| name == local_name) {
                        if decl.initializer.is_some() {
                            return Some(true);
                        }
                        found_uninitialized = true;
                    }
                }
            }
        }
        found_uninitialized.then_some(false)
    }

    pub(super) fn emit_system_enum_with_export_fold(
        &mut self,
        enum_idx: NodeIndex,
        _enum_name: &str,
        export_names: Vec<String>,
    ) {
        let mut enum_emitter = crate::transforms::EnumES5Emitter::new(self.arena);
        enum_emitter.set_indent_level(self.writer.indent_level());
        enum_emitter.set_preserve_const_enums(self.ctx.options.preserve_const_enums);
        if let Some(text) = self.source_text {
            enum_emitter.set_source_text(text);
        }
        enum_emitter.set_emit_var_declaration(false);
        enum_emitter.set_system_export_folds(export_names.iter().map(String::as_str));
        let output = enum_emitter.emit_enum(enum_idx);
        self.write(output.trim_end_matches('\n').trim_start());
    }

    pub(super) fn emit_system_namespace_with_export_fold(
        &mut self,
        stmt_idx: NodeIndex,
        _ns_name: &str,
        export_names: Vec<String>,
    ) {
        let before_len = self.writer.len();
        let prev_system_fold = self
            .pending_system_namespace_export_fold
            .replace(export_names);
        self.emit(stmt_idx);
        self.pending_system_namespace_export_fold = prev_system_fold;
        if self.writer.len() > before_len && !self.writer.is_at_line_start() {
            self.write_line();
        }
    }

    pub(super) fn system_module_specifier_text(&self, specifier: NodeIndex) -> Option<String> {
        if specifier.is_none() {
            return None;
        }
        let node = self.arena.get(specifier)?;
        let literal = self.arena.get_literal(node)?;
        Some(literal.text.clone())
    }

    /// Emit a non-exported class declaration as a class expression assignment
    /// in the system execute body: Name = class Name { };
    fn emit_system_class_as_expression(
        &mut self,
        node: &tsz_parser::parser::node::Node,
        idx: NodeIndex,
    ) {
        let Some(class_decl) = self.arena.get_class(node) else {
            self.emit(idx);
            return;
        };
        if self
            .arena
            .has_modifier(&class_decl.modifiers, SyntaxKind::DeclareKeyword)
        {
            return;
        }
        let class_name = self.get_identifier_text_idx(class_decl.name);
        if class_name.is_empty() {
            self.emit(idx);
            return;
        }
        self.write(&class_name);
        self.write(" = ");
        let deferred = self.emit_system_class_expression_value(node, idx, true);
        if !self.output_ends_with_semicolon() {
            self.write(";");
        }
        let legacy_class_decorators = self.collect_class_decorators(&class_decl.modifiers);
        if self.ctx.options.legacy_decorators
            && (!legacy_class_decorators.is_empty()
                || !self
                    .collect_constructor_param_decorators(&class_decl.members.nodes)
                    .is_empty())
        {
            self.write_line();
            self.emit_legacy_class_decorator_assignment(
                &class_name,
                &legacy_class_decorators,
                false,
                false,
                false,
                None,
                &class_decl.members.nodes,
            );
        }
        if !deferred.is_empty() {
            self.emit_static_block_iifes(deferred);
        }
    }

    fn emit_system_class_expression_value(
        &mut self,
        node: &tsz_parser::parser::node::Node,
        idx: NodeIndex,
        defer_es5_static_block_tail: bool,
    ) -> Vec<(NodeIndex, usize)> {
        if self.ctx.target_es5 {
            if defer_es5_static_block_tail {
                self.defer_class_static_blocks = true;
                self.deferred_class_static_blocks.clear();
            }
            self.emit_class_expression_es5(idx);
            if defer_es5_static_block_tail {
                self.defer_class_static_blocks = false;
                return std::mem::take(&mut self.deferred_class_static_blocks);
            }
            return Vec::new();
        }

        self.defer_class_static_blocks = true;
        self.deferred_class_static_blocks.clear();
        self.emit_class_es6(node, idx);
        self.defer_class_static_blocks = false;
        std::mem::take(&mut self.deferred_class_static_blocks)
    }

    pub(in crate::emitter) fn emit_system_variable_initializers(
        &mut self,
        node: &tsz_parser::parser::node::Node,
    ) {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return;
        };
        let is_exported = self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);

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

                if decl.initializer.is_none() {
                    continue;
                }

                if self.emit_system_binding_pattern_initializer(decl, is_exported) {
                    continue;
                }

                if is_exported {
                    let export_name = self.get_identifier_text_idx(decl.name);
                    if !export_name.is_empty() {
                        self.write("exports_1(\"");
                        self.write(&export_name);
                        self.write("\", ");
                        self.write(&export_name);
                        self.write(" = ");
                        self.emit_expression(decl.initializer);
                        self.write(");");
                        continue;
                    }
                }
                self.emit(decl.name);
                self.write(" = ");
                self.emit_expression(decl.initializer);
                self.write_semicolon();
            }
        }
    }

    fn emit_system_binding_pattern_initializer(
        &mut self,
        decl: &tsz_parser::parser::node::VariableDeclarationData,
        is_exported: bool,
    ) -> bool {
        let Some(name_node) = self.arena.get(decl.name) else {
            return false;
        };
        if self.binding_pattern_is_empty(decl.name) {
            let (source_temp, export_temp) = self
                .arena
                .get(decl.name)
                .and_then(|name| self.system_empty_binding_temps.get(&name.pos).cloned())
                .or_else(|| {
                    let temp = self.make_unique_name();
                    Some((temp, None))
                })
                .unwrap_or_default();
            if is_exported
                && self.ctx.target_es5
                && let Some(export_temp) = export_temp
            {
                self.write("exports_1(\"");
                self.write(&export_temp);
                self.write("\", ");
                self.write(&export_temp);
                self.write(" = ");
                self.write(&source_temp);
                self.write(" = ");
                self.emit_expression(decl.initializer);
                self.write(");");
            } else {
                self.write(&source_temp);
                self.write(" = ");
                self.emit_expression(decl.initializer);
                self.write_semicolon();
            }
            return true;
        }
        // Non-empty array patterns and multi-element object patterns use a
        // planned source so each bound name can be published individually.
        if let Some(source_temp) = self
            .arena
            .get(decl.name)
            .and_then(|n| self.system_binding_pattern_temps.get(&n.pos).cloned())
        {
            self.emit_system_destructuring_with_source(decl, source_temp.as_deref(), is_exported);
            return true;
        }
        if name_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return false;
        }
        if is_exported && self.emit_system_object_rest_export_initializer(decl) {
            return true;
        }
        let Some(pattern) = self.arena.get_binding_pattern(name_node) else {
            return false;
        };
        if pattern.elements.nodes.len() != 1 {
            return false;
        }
        let Some(elem_node) = self.arena.get(pattern.elements.nodes[0]) else {
            return false;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return false;
        };
        if elem.dot_dot_dot_token {
            return false;
        }
        if elem.initializer.is_some() {
            return false;
        }

        let export_name = self.get_identifier_text(elem.name);
        if export_name.is_empty() {
            return false;
        }
        let mut prop_element = NodeIndex::NONE;
        let prop_name = if elem.property_name.is_some() {
            let Some(prop_node) = self.arena.get(elem.property_name) else {
                return false;
            };
            let prop = self.get_identifier_text_idx(elem.property_name);
            if prop.is_empty() {
                if prop_node.kind == SyntaxKind::StringLiteral as u16
                    || prop_node.kind == SyntaxKind::NumericLiteral as u16
                {
                    prop_element = elem.property_name;
                    String::new()
                } else {
                    return false;
                }
            } else {
                prop
            }
        } else {
            export_name.clone()
        };

        if is_exported {
            self.write("exports_1(\"");
            self.write(&export_name);
            self.write("\", ");
        }
        self.write(&export_name);
        self.write(" = ");
        self.emit_expression(decl.initializer);
        if prop_element.is_some() {
            self.write("[");
            self.emit_expression(prop_element);
            self.write("]");
        } else {
            if self
                .arena
                .get(decl.initializer)
                .is_some_and(|node| node.is_numeric_literal())
            {
                self.write(".");
            }
            self.write(".");
            self.write(&prop_name);
        }
        if is_exported {
            self.write(")");
        }
        self.write_semicolon();
        true
    }

    /// Emits a destructuring variable initializer for an exported array or
    /// multi-element object binding pattern using either a preallocated source
    /// temp or a reusable identifier initializer.
    ///
    /// Structural rule: when a System-module exported variable declaration has
    /// a no-default array or multi-element object binding pattern as its name,
    /// tsc publishes each bound name via `exports_1("n", n = source.path)`,
    /// first assigning complex/non-reusable sources to a temp when needed.
    fn emit_system_destructuring_with_source(
        &mut self,
        decl: &tsz_parser::parser::node::VariableDeclarationData,
        source_temp: Option<&str>,
        is_exported: bool,
    ) {
        let mut name_paths: Vec<(String, String)> = Vec::new();
        self.collect_bound_names_with_paths(decl.name, String::new(), &mut name_paths);
        let reusable_source = self.reusable_object_rest_export_source(decl.initializer);
        let source_name = source_temp
            .or(reusable_source.as_deref())
            .unwrap_or_default();

        if let Some(source_temp) = source_temp {
            self.write(source_temp);
            self.write(" = ");
            self.emit_expression(decl.initializer);
        }

        for (index, (name, path)) in name_paths.iter().enumerate() {
            if source_temp.is_some() || index > 0 {
                self.write(", ");
            }
            if is_exported {
                self.write("exports_1(\"");
                self.write(name);
                self.write("\", ");
            }
            self.write(name);
            self.write(" = ");
            if source_name.is_empty() && name_paths.len() == 1 {
                self.emit_expression(decl.initializer);
            } else {
                self.write(source_name);
            }
            self.write(path);
            if is_exported {
                self.write(")");
            }
        }
        self.write_semicolon();
    }

    /// Recursively collects `(bound_name, access_path)` pairs from a binding
    /// pattern, where `access_path` is appended to a temp variable name to
    /// form the full access expression (e.g. `[0]`, `.a`, `.b.c`).
    ///
    /// `prefix` accumulates the path from the containing pattern root and is
    /// empty at the top-level call.  Elided array positions and elements with
    /// rest tokens or default initializers are skipped for now.
    fn collect_bound_names_with_paths(
        &self,
        name_idx: NodeIndex,
        prefix: String,
        out: &mut Vec<(String, String)>,
    ) {
        let Some(node) = self.arena.get(name_idx) else {
            return;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            if let Some(id) = self.arena.get_identifier(node) {
                let text = id
                    .original_text
                    .as_deref()
                    .unwrap_or(&id.escaped_text)
                    .to_string();
                if !text.is_empty() {
                    out.push((text, prefix));
                }
            }
            return;
        }

        if node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            if let Some(pattern) = self.arena.get_binding_pattern(node) {
                for (i, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
                    if elem_idx.is_none() {
                        continue; // elided element
                    }
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        continue;
                    };
                    let Some(elem) = self.arena.get_binding_element(elem_node) else {
                        continue;
                    };
                    if elem.dot_dot_dot_token || elem.initializer.is_some() {
                        continue;
                    }
                    let elem_path = format!("{prefix}[{i}]");
                    self.collect_bound_names_with_paths(elem.name, elem_path, out);
                }
            }
            return;
        }

        if node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            if let Some(pattern) = self.arena.get_binding_pattern(node) {
                for &elem_idx in &pattern.elements.nodes {
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        continue;
                    };
                    let Some(elem) = self.arena.get_binding_element(elem_node) else {
                        continue;
                    };
                    if elem.dot_dot_dot_token || elem.initializer.is_some() {
                        continue;
                    }
                    // The property key: use `property_name` when present,
                    // otherwise the element name itself is the key.
                    let key = if elem.property_name.is_some() {
                        self.get_identifier_text_idx(elem.property_name)
                    } else {
                        self.get_identifier_text_idx(elem.name)
                    };
                    if key.is_empty() {
                        continue;
                    }
                    let elem_path = format!("{prefix}.{key}");
                    self.collect_bound_names_with_paths(elem.name, elem_path, out);
                }
            }
        }
    }
}

fn system_jsx_dev_file_name(file_name: &str) -> String {
    let normalized = file_name.replace('\\', "/");
    if let Some(src_start) = normalized.find("/.src/") {
        return normalized[src_start..].to_string();
    }
    if let Some(stripped) = normalized.strip_prefix(".src/") {
        return format!("/.src/{stripped}");
    }
    normalized
        .rsplit('/')
        .next()
        .unwrap_or(normalized.as_str())
        .to_string()
}
