use super::super::Printer;
use super::{SystemDependencyAction, SystemDependencyPlan};
use crate::emitter::ModuleKind;
use rustc_hash::FxHashMap;
use std::collections::{HashMap, HashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(super) fn emit_wrapped_import_helpers(
        &mut self,
        source: &tsz_parser::parser::node::SourceFileData,
    ) {
        if self.ctx.options.no_emit_helpers || self.ctx.options.import_helpers {
            return;
        }

        let mut needs_import_default = false;
        let mut needs_import_star = false;

        // Check if lowering pass detected dynamic import() calls needing __importStar
        if self.transforms.helpers_populated() && self.transforms.helpers().import_star {
            needs_import_star = true;
        }

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
            let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };
            if clause.is_type_only {
                continue;
            }
            if !self.ctx.options.verbatim_module_syntax
                && !self.source_is_js_file
                && !self.is_jsx_factory_import_clause(clause)
                && !self.import_has_value_usage_after_node(stmt_node, clause)
            {
                continue;
            }
            if clause.name.is_some() {
                needs_import_default = true;
            }
            if clause.named_bindings.is_some()
                && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(named_imports) = self.arena.get_named_imports(bindings_node)
                && named_imports.name.is_some()
                && named_imports.elements.nodes.is_empty()
            {
                needs_import_star = true;
            }
        }

        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export_decl.is_type_only || export_decl.module_specifier.is_none() {
                continue;
            }
            if let Some(clause_node) = self.arena.get(export_decl.export_clause)
                && clause_node.kind != syntax_kind_ext::NAMED_EXPORTS
            {
                needs_import_star = true;
            }
        }

        if needs_import_star {
            self.write(crate::transforms::helpers::CREATE_BINDING_HELPER);
            self.write_line();
            self.write(crate::transforms::helpers::SET_MODULE_DEFAULT_HELPER);
            self.write_line();
            self.write(crate::transforms::helpers::IMPORT_STAR_HELPER);
            self.write_line();
        }
        if needs_import_default {
            self.write(crate::transforms::helpers::IMPORT_DEFAULT_HELPER);
            self.write_line();
        }
    }

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
            let Some(dep_var) = dep_vars.get(dep) else {
                continue;
            };
            let Some(actions) = system_plan.actions.get(dep) else {
                continue;
            };
            self.write("function (");
            self.write(dep_var);
            self.write("_1) {");
            self.write_line();
            self.increase_indent();
            for action in actions {
                match action {
                    SystemDependencyAction::Assign(local_name) => {
                        self.write(local_name);
                        self.write(" = ");
                        self.write(dep_var);
                        self.write("_1;");
                        self.write_line();
                    }
                    SystemDependencyAction::NamedExports(exports) => {
                        self.write("exports_1({");
                        self.write_line();
                        self.increase_indent();
                        for (export_idx, (export_name, import_name)) in exports.iter().enumerate() {
                            self.write("\"");
                            self.write(export_name);
                            self.write("\": ");
                            let setter_arg = format!("{dep_var}_1");
                            self.write_module_property_access(&setter_arg, import_name);
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
                if !local_name.is_empty() {
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

        self.ctx.original_module_kind = Some(prev_module);
        self.ctx.options.module = ModuleKind::CommonJS;
        self.ctx.auto_detect_module = false;
        self.in_system_execute_body = true;

        let Some(source) = self.arena.get_source_file(source_node) else {
            self.ctx.options.module = prev_module;
            self.ctx.auto_detect_module = prev_auto_detect;
            self.ctx.original_module_kind = prev_original;
            self.in_system_execute_body = false;
            return;
        };
        self.register_system_import_substitutions(source, dep_vars, system_plan);

        let mut reexported_names: FxHashMap<String, String> = FxHashMap::default();
        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                && let Some(var_stmt) = self.arena.get_variable(stmt_node)
                && self
                    .arena
                    .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
            {
                for name in self.collect_variable_names(&var_stmt.declarations) {
                    if !name.is_empty() {
                        reexported_names.entry(name.clone()).or_insert(name);
                    }
                }
                continue;
            }
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export_decl.module_specifier.is_some() || export_decl.is_default_export {
                continue;
            }
            let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
                continue;
            };
            if clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                && let Some(var_stmt) = self.arena.get_variable(clause_node)
            {
                for name in self.collect_variable_names(&var_stmt.declarations) {
                    if !name.is_empty() {
                        reexported_names.entry(name.clone()).or_insert(name);
                    }
                }
                continue;
            }
            if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                continue;
            }
            let Some(named_exports) = self.arena.get_named_imports(clause_node) else {
                continue;
            };
            for &spec_idx in &named_exports.elements.nodes {
                if let Some(spec) = self.arena.get_specifier_at(spec_idx) {
                    let local_name = if spec.property_name.is_some() {
                        self.get_specifier_name_text(spec.property_name)
                    } else {
                        self.get_specifier_name_text(spec.name)
                    }
                    .unwrap_or_default();
                    let export_name = self.get_specifier_name_text(spec.name).unwrap_or_default();
                    if !local_name.is_empty() && !export_name.is_empty() {
                        reexported_names.insert(local_name, export_name);
                    }
                }
            }
        }
        self.system_reexported_names = reexported_names;
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
            self.in_system_execute_body = false;
            return;
        }

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
                    let module_name = self.get_identifier_text_idx(module_decl.name);
                    if !module_name.is_empty() {
                        self.declared_namespace_names.insert(module_name.clone());
                        if let Some(export_name) =
                            self.system_reexported_names.get(&module_name).cloned()
                        {
                            self.emit_system_namespace_with_export_fold(
                                stmt_idx,
                                &module_name,
                                &export_name,
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
                        if let Some(export_name) =
                            self.system_reexported_names.get(&enum_name).cloned()
                        {
                            self.emit_system_enum_with_export_fold(
                                stmt_idx,
                                &enum_name,
                                &export_name,
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

        self.ctx.options.module = prev_module;
        self.ctx.auto_detect_module = prev_auto_detect;
        self.ctx.original_module_kind = prev_original;
        self.in_system_execute_body = false;
    }

    fn emit_system_top_level_using_scope(
        &mut self,
        source: &tsz_parser::parser::node::SourceFileData,
        start_idx: usize,
        dep_vars: &HashMap<String, String>,
        hoisted_func_stmts: &HashSet<NodeIndex>,
    ) {
        let mut deferred_named_exports: FxHashMap<String, String> = FxHashMap::default();
        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export_decl.module_specifier.is_some() {
                continue;
            }
            let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
                continue;
            };
            if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                continue;
            }
            let Some(named_exports) = self.arena.get_named_imports(clause_node) else {
                continue;
            };
            for &spec_idx in &named_exports.elements.nodes {
                if let Some(spec) = self.arena.get_specifier_at(spec_idx) {
                    let local_name = if spec.property_name.is_some() {
                        self.get_specifier_name_text(spec.property_name)
                    } else {
                        self.get_specifier_name_text(spec.name)
                    }
                    .unwrap_or_default();
                    let export_name = self.get_specifier_name_text(spec.name).unwrap_or_default();
                    if !local_name.is_empty() && !export_name.is_empty() {
                        deferred_named_exports.insert(local_name, export_name);
                    }
                }
            }
        }
        for &stmt_idx in &source.statements.nodes[..start_idx] {
            if hoisted_func_stmts.contains(&stmt_idx) {
                continue;
            }
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
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
                && export_decl.module_specifier.is_none()
                && let Some(clause_node) = self.arena.get(export_decl.export_clause)
                && clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
            {
                continue;
            }

            let before_len = self.writer.len();
            if self.emit_system_top_level_using_statement(
                stmt_node,
                stmt_idx,
                dep_vars,
                &deferred_named_exports,
            ) {
                if self.writer.len() > before_len
                    && stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION
                    && !self.writer.is_at_line_start()
                {
                    self.write_line();
                }
                continue;
            } else if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && self.emit_system_export_declaration(stmt_node, dep_vars)
            {
                if self.writer.len() > before_len {
                    self.write_line();
                }
                continue;
            }

            if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                self.emit_system_variable_initializers(stmt_node);
            } else {
                self.emit(stmt_idx);
            }

            if self.writer.len() > before_len
                && stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION
            {
                self.write_line();
            }
        }

        if self.ctx.options.target.supports_es2025() {
            let prev_deferred_local_export_bindings = self
                .deferred_local_export_bindings
                .replace(deferred_named_exports.clone());
            for &stmt_idx in &source.statements.nodes[start_idx..] {
                if hoisted_func_stmts.contains(&stmt_idx) {
                    continue;
                }
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    continue;
                };
                if self.is_erased_statement(stmt_node) {
                    continue;
                }
                if self.emit_system_top_level_using_statement(
                    stmt_node,
                    stmt_idx,
                    dep_vars,
                    &deferred_named_exports,
                ) && !self.writer.is_at_line_start()
                {
                    self.write_line();
                }
            }
            self.deferred_local_export_bindings = prev_deferred_local_export_bindings;
            return;
        }

        let using_async = source.statements.nodes[start_idx..]
            .iter()
            .any(|&stmt_idx| {
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    return false;
                };
                if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                    return false;
                }
                let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                    return false;
                };
                var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
                    self.arena.get(decl_list_idx).is_some_and(|decl_list_node| {
                        tsz_parser::parser::node_flags::is_await_using(decl_list_node.flags as u32)
                    })
                })
            });
        let (env_name, error_name, result_name) = self.next_disposable_env_names();
        self.write(&env_name);
        self.write(" = { stack: [], error: void 0, hasError: false };");
        self.write_line();
        self.write("try {");
        self.write_line();
        self.increase_indent();

        let prev_deferred_local_export_bindings = self
            .deferred_local_export_bindings
            .replace(deferred_named_exports.clone());
        let prev_block_using_env = self
            .block_using_env
            .replace((env_name.clone(), using_async));
        let prev_in_top_level_using_scope = self.in_top_level_using_scope;
        self.in_top_level_using_scope = true;
        for &stmt_idx in &source.statements.nodes[start_idx..] {
            if hoisted_func_stmts.contains(&stmt_idx) {
                continue;
            }
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if self.is_erased_statement(stmt_node) {
                continue;
            }
            if self.emit_system_top_level_using_statement(
                stmt_node,
                stmt_idx,
                dep_vars,
                &deferred_named_exports,
            ) && !self.writer.is_at_line_start()
            {
                self.write_line();
            }
        }
        self.in_top_level_using_scope = prev_in_top_level_using_scope;
        self.block_using_env = prev_block_using_env;
        self.deferred_local_export_bindings = prev_deferred_local_export_bindings;

        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.write("catch (");
        self.write(&error_name);
        self.write(") {");
        self.write_line();
        self.increase_indent();
        self.write(&env_name);
        self.write(".error = ");
        self.write(&error_name);
        self.write(";");
        self.write_line();
        self.write(&env_name);
        self.write(".hasError = true;");
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.write("finally {");
        self.write_line();
        self.increase_indent();
        if using_async {
            self.write("const ");
            self.write(&result_name);
            self.write(" = ");
            self.write_helper("__disposeResources");
            self.write("(");
            self.write(&env_name);
            self.write(");");
            self.write_line();
            self.write("if (");
            self.write(&result_name);
            self.write(")");
            self.write_line();
            self.increase_indent();
            self.write("await ");
            self.write(&result_name);
            self.write(";");
            self.write_line();
            self.decrease_indent();
        } else {
            self.write_helper("__disposeResources");
            self.write("(");
            self.write(&env_name);
            self.write(");");
            self.write_line();
        }
        self.decrease_indent();
        self.write("}");
        self.write_line();
    }

    fn emit_system_top_level_using_statement(
        &mut self,
        stmt_node: &tsz_parser::parser::node::Node,
        stmt_idx: NodeIndex,
        dep_vars: &HashMap<String, String>,
        deferred_named_exports: &FxHashMap<String, String>,
    ) -> bool {
        match stmt_node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => self
                .emit_system_top_level_using_variable_statement(stmt_node, deferred_named_exports),
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                let export_name = self
                    .arena
                    .get_class(stmt_node)
                    .and_then(|class| self.get_identifier_text_opt(class.name))
                    .and_then(|name| deferred_named_exports.get(&name).cloned());
                self.emit_top_level_using_class_assignment(stmt_node, stmt_idx, export_name, false)
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let export_name = self
                    .arena
                    .get_function(stmt_node)
                    .and_then(|func| self.get_identifier_text_opt(func.name))
                    .and_then(|name| deferred_named_exports.get(&name).cloned());
                self.emit_top_level_using_function_assignment(stmt_node, stmt_idx, export_name)
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                let Some(export) = self.arena.get_export_decl(stmt_node) else {
                    return false;
                };
                if export.is_type_only || export.module_specifier.is_some() {
                    return false;
                }
                let Some(clause_node) = self.arena.get(export.export_clause) else {
                    return false;
                };
                match clause_node.kind {
                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => self
                        .emit_system_top_level_using_variable_statement(
                            clause_node,
                            deferred_named_exports,
                        ),
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        let export_name = if export.is_default_export {
                            Some("default".to_string())
                        } else {
                            self.arena
                                .get_class(clause_node)
                                .and_then(|class| self.get_identifier_text_opt(class.name))
                        };
                        if let Some(export_name) = export_name {
                            self.emit_top_level_using_class_assignment(
                                clause_node,
                                export.export_clause,
                                Some(export_name),
                                !export.is_default_export,
                            )
                        } else {
                            false
                        }
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        let export_name = if export.is_default_export {
                            Some("default".to_string())
                        } else {
                            self.arena
                                .get_function(clause_node)
                                .and_then(|func| self.get_identifier_text_opt(func.name))
                        };
                        if let Some(export_name) = export_name {
                            self.emit_top_level_using_function_assignment(
                                clause_node,
                                export.export_clause,
                                Some(export_name),
                            )
                        } else {
                            false
                        }
                    }
                    k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                        self.emit_system_import_equals_declaration(clause_node, dep_vars, true)
                    }
                    k if k == syntax_kind_ext::NAMED_EXPORTS => true,
                    _ if export.is_default_export => {
                        self.write_export_binding_start("default");
                        self.write("_default = ");
                        self.emit(export.export_clause);
                        self.write_export_binding_end();
                        true
                    }
                    _ => false,
                }
            }
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                let Some(export_assignment) = self.arena.get_export_assignment(stmt_node) else {
                    return false;
                };
                if export_assignment.is_export_equals {
                    return false;
                }
                self.write_export_binding_start("default");
                self.write("_default = ");
                self.emit(export_assignment.expression);
                self.write_export_binding_end();
                true
            }
            _ => {
                self.emit(stmt_idx);
                true
            }
        }
    }

    fn emit_system_top_level_using_variable_statement(
        &mut self,
        node: &tsz_parser::parser::node::Node,
        deferred_named_exports: &FxHashMap<String, String>,
    ) -> bool {
        if self.ctx.options.target.supports_es2025() {
            return self.emit_system_native_top_level_using_variable_statement(
                node,
                deferred_named_exports,
            );
        }
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return false;
        };
        let is_exported = self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);
        let mut emitted = false;

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };
            let flags = decl_list_node.flags as u32;
            let is_using = (flags & tsz_parser::parser::node_flags::USING) != 0;
            let using_async = tsz_parser::parser::node_flags::is_await_using(flags);

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                let name = self.get_identifier_text_idx(decl.name);
                if name.is_empty() {
                    continue;
                }

                if emitted {
                    self.write_line();
                }

                if is_using {
                    let env_name = self
                        .block_using_env
                        .as_ref()
                        .map(|(env_name, _)| env_name.clone())
                        .unwrap_or_default();
                    self.write(&name);
                    self.write(" = ");
                    self.write_helper("__addDisposableResource");
                    self.write("(");
                    self.write(&env_name);
                    self.write(", ");
                    if decl.initializer.is_some() {
                        self.emit(decl.initializer);
                    } else {
                        self.write("void 0");
                    }
                    self.write(", ");
                    self.write(if using_async { "true" } else { "false" });
                    self.write(");");
                } else if is_exported {
                    self.write_export_binding_start(&name);
                    self.write(&name);
                    self.write(" = ");
                    if decl.initializer.is_some() {
                        self.emit(decl.initializer);
                    } else {
                        self.write("void 0");
                    }
                    self.write_export_binding_end();
                } else if let Some(export_name) = deferred_named_exports.get(&name) {
                    self.write_export_binding_start(export_name);
                    self.write(&name);
                    self.write(" = ");
                    if decl.initializer.is_some() {
                        self.emit(decl.initializer);
                    } else {
                        self.write("void 0");
                    }
                    self.write_export_binding_end();
                } else if decl.initializer.is_some() {
                    self.write(&name);
                    self.write(" = ");
                    self.emit(decl.initializer);
                    self.write(";");
                }
                emitted = true;
            }
        }

        emitted
    }

    fn emit_system_native_top_level_using_variable_statement(
        &mut self,
        node: &tsz_parser::parser::node::Node,
        deferred_named_exports: &FxHashMap<String, String>,
    ) -> bool {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return false;
        };
        let is_exported = self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);
        let mut emitted = false;

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };
            let flags = decl_list_node.flags as u32;
            let is_using = (flags & tsz_parser::parser::node_flags::USING) != 0;
            let is_await_using = tsz_parser::parser::node_flags::is_await_using(flags);

            if is_using || is_await_using {
                if emitted {
                    self.write_line();
                }
                if is_await_using {
                    self.write("await using ");
                } else {
                    self.write("using ");
                }

                let mut first = true;
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    let name = self.get_identifier_text_idx(decl.name);
                    if name.is_empty() {
                        continue;
                    }
                    if !first {
                        self.write(", ");
                    }
                    let temp_name = self.make_unique_name_from_base(&name);
                    self.write(&temp_name);
                    self.write(" = ");
                    self.write(&name);
                    self.write(" = ");
                    if decl.initializer.is_some() {
                        self.emit_expression(decl.initializer);
                    } else {
                        self.write("void 0");
                    }
                    first = false;
                }
                if !first {
                    self.write(";");
                    emitted = true;
                }
                continue;
            }

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

                let name = self.get_identifier_text_idx(decl.name);
                if name.is_empty() {
                    continue;
                }

                if emitted {
                    self.write_line();
                }

                if is_exported {
                    self.write("exports_1(\"");
                    self.write(&name);
                    self.write("\", ");
                    self.write(&name);
                    self.write(" = ");
                    self.emit_expression(decl.initializer);
                    self.write(");");
                } else if let Some(export_name) = deferred_named_exports.get(&name) {
                    self.write_export_binding_start(export_name);
                    self.write(&name);
                    self.write(" = ");
                    self.emit_expression(decl.initializer);
                    self.write_export_binding_end();
                } else {
                    self.write(&name);
                    self.write(" = ");
                    self.emit_expression(decl.initializer);
                    self.write(";");
                }
                emitted = true;
            }
        }

        emitted
    }

    fn emit_system_import_equals_declaration(
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

    fn emit_system_export_declaration(
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
            self.write(&gen_name);
            self.write(" = ");
            // Emit class as anonymous class expression
            self.anonymous_default_export_name = None;
            self.defer_class_static_blocks = true;
            self.deferred_class_static_blocks.clear();
            self.emit_class_es6(clause_node, export_decl.export_clause);
            self.defer_class_static_blocks = false;
            let deferred = std::mem::take(&mut self.deferred_class_static_blocks);
            if !self.output_ends_with_semicolon() {
                self.write(";");
            }
            self.write_line();
            self.write("exports_1(\"default\", ");
            self.write(&gen_name);
            self.write(");");
            if !deferred.is_empty() {
                self.emit_static_block_iifes(deferred);
            }
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
            let class_name = self.get_identifier_text_idx(class_decl.name);
            if class_name.is_empty() {
                return false;
            }
            self.write(&class_name);
            self.write(" = ");
            // Defer static block IIFEs so we can emit exports_1 before them
            self.defer_class_static_blocks = true;
            self.deferred_class_static_blocks.clear();
            self.emit_class_es6(clause_node, export_decl.export_clause);
            self.defer_class_static_blocks = false;
            let deferred = std::mem::take(&mut self.deferred_class_static_blocks);
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
                    &enum_name,
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

    fn emit_system_enum_with_export_fold(
        &mut self,
        enum_idx: NodeIndex,
        enum_name: &str,
        export_name: &str,
    ) {
        let mut enum_emitter = crate::transforms::EnumES5Emitter::new(self.arena);
        enum_emitter.set_indent_level(self.writer.indent_level());
        enum_emitter.set_preserve_const_enums(self.ctx.options.preserve_const_enums);
        if let Some(text) = self.source_text {
            enum_emitter.set_source_text(text);
        }
        let mut output = enum_emitter.emit_enum(enum_idx);
        let var_prefix = format!("var {enum_name};\n");
        if output.starts_with(&var_prefix) {
            output = output[var_prefix.len()..].to_string();
        }
        let from = format!("({enum_name} || ({enum_name} = {{}}))");
        let to = format!("({enum_name} || (exports_1(\"{export_name}\", {enum_name} = {{}})))");
        output = output.replacen(&from, &to, 1);
        self.write(output.trim_end_matches('\n').trim_start());
    }

    fn emit_system_namespace_with_export_fold(
        &mut self,
        stmt_idx: NodeIndex,
        ns_name: &str,
        export_name: &str,
    ) {
        let start_pos = self.writer.len();
        self.emit(stmt_idx);
        let output = self.writer.get_output()[start_pos..].to_string();
        self.writer.truncate(start_pos);
        let from = format!("({ns_name} || ({ns_name} = {{}}))");
        let to = format!("({ns_name} || (exports_1(\"{export_name}\", {ns_name} = {{}})))");
        let replaced = output.replacen(&from, &to, 1);
        self.write(replaced.trim_end_matches('\n'));
        self.write_line();
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
        let class_name = self.get_identifier_text_idx(class_decl.name);
        if class_name.is_empty() {
            self.emit(idx);
            return;
        }
        self.write(&class_name);
        self.write(" = ");
        self.defer_class_static_blocks = true;
        self.deferred_class_static_blocks.clear();
        self.emit_class_es6(node, idx);
        self.defer_class_static_blocks = false;
        let deferred = std::mem::take(&mut self.deferred_class_static_blocks);
        if !self.output_ends_with_semicolon() {
            self.write(";");
        }
        if !deferred.is_empty() {
            self.emit_static_block_iifes(deferred);
        }
    }

    fn emit_system_variable_initializers(&mut self, node: &tsz_parser::parser::node::Node) {
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
        if name_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return false;
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
        let prop_name = if elem.property_name.is_some() {
            let prop = self.get_identifier_text_idx(elem.property_name);
            if prop.is_empty() {
                export_name.clone()
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
        if self
            .arena
            .get(decl.initializer)
            .is_some_and(|node| node.is_numeric_literal())
        {
            self.write(".");
        }
        self.write(".");
        self.write(&prop_name);
        if is_exported {
            self.write(")");
        }
        self.write_semicolon();
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::emitter::{ModuleKind, Printer, PrinterOptions};
    use tsz_common::ScriptTarget;
    use tsz_parser::ParserState;

    /// `/// <reference .../>` directives should be stripped from JS output.
    /// tsc never emits these in JS — they are only preserved in .d.ts files.
    #[test]
    fn amd_reference_directive_absolute_path_preserved() {
        // References with absolute paths (like JSX lib references) should be
        // emitted before the AMD wrapper, matching tsc behavior.
        let source = r#"/// <reference path="/.lib/react.d.ts" />
import * as React from "react";
export const Foo = () => null;
"#;
        let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::AMD,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.starts_with("/// <reference path=\"/.lib/react.d.ts\" />"),
            "Absolute-path reference should be emitted before AMD wrapper.\nOutput:\n{output}"
        );
        assert!(
            output.contains("define("),
            "Output should still contain the AMD define() call.\nOutput:\n{output}"
        );
    }

    /// AMD wrappers should strip relative declaration-file `/// <reference>` directives.
    #[test]
    fn amd_reference_directive_relative_dts_path_stripped() {
        let source = r#"/// <reference path="file1.d.ts" />
import { x } from "mod";
export const y = x;
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::AMD,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            !output.contains("/// <reference"),
            "Relative .d.ts reference should be stripped from AMD JS output.\nOutput:\n{output}"
        );
        assert!(
            output.contains("define("),
            "Output should still contain the AMD define() call.\nOutput:\n{output}"
        );
    }

    #[test]
    fn amd_reference_directive_for_bang_module_preserved() {
        let declarations = r#"declare module "http" {
}

declare module 'intern/dojo/node!http' {
    import http = require('http');
    export = http;
}
"#;
        let source = r#"/// <reference path="a.d.ts"/>

import * as http from 'intern/dojo/node!http';
"#;
        let mut parser = ParserState::new("a.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut declaration_file = parser.arena.source_files[0].clone();
        declaration_file.file_name = "a.d.ts".to_string();
        declaration_file.text = std::sync::Arc::from(declarations);
        declaration_file.is_declaration_file = true;
        parser.arena.source_files.push(declaration_file);

        let options = PrinterOptions {
            module: ModuleKind::AMD,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.starts_with("/// <reference path=\"a.d.ts\"/>"),
            "Bang module declaration reference should be emitted before AMD wrapper.\nOutput:\n{output}"
        );
        assert!(
            output.contains("define("),
            "Output should still contain the AMD define() call.\nOutput:\n{output}"
        );
    }

    /// UMD wrappers should also strip `/// <reference>` directives from JS output.
    #[test]
    fn umd_reference_directive_stripped_from_output() {
        let source = r#"/// <reference path="lib.d.ts" />
import { x } from "mod";
export const y = x;
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::UMD,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            !output.contains("/// <reference"),
            "Reference directives should be stripped from JS output.\nOutput:\n{output}"
        );
        assert!(
            output.contains("(function (factory)"),
            "Output should still contain the UMD wrapper.\nOutput:\n{output}"
        );
    }

    #[test]
    fn system_top_level_using_named_export_keeps_legacy_decorator_assignment_export() {
        let source = "export {};\ndeclare var dec: any;\n@dec\nclass C {}\nexport { C as D };\nusing after = null;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::with_options(
            &parser.arena,
            PrinterOptions {
                module: ModuleKind::System,
                legacy_decorators: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("exports_1(\"D\", C);"),
            "System named export should preserve the pre-export before __decorate.\nOutput:\n{output}"
        );
        assert!(
            output.contains("exports_1(\"D\", C = __decorate(["),
            "System named export should wrap the legacy decorator reassignment directly.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("exports_1(\"D\", C);\n            C = __decorate(["),
            "System named export should not split the export from the __decorate reassignment.\nOutput:\n{output}"
        );
    }

    #[test]
    fn system_top_level_using_direct_exported_legacy_class_stays_inline() {
        let source =
            "export {};\ndeclare var dec: any;\nusing before = null;\n@dec\nexport class C {}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::with_options(
            &parser.arena,
            PrinterOptions {
                module: ModuleKind::System,
                legacy_decorators: true,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("exports_1(\"C\", C = class C {"),
            "System top-level using should keep direct legacy-decorated class exports inline.\nOutput:\n{output}"
        );
        assert!(
            output.contains("exports_1(\"C\", C = __decorate(["),
            "System top-level using should preserve the exported legacy decorator reassignment.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("});\n                exports_1(\"C\", C);"),
            "System top-level using should not split direct legacy class exports into a trailing export statement.\nOutput:\n{output}"
        );
    }

    #[test]
    fn system_exported_object_binding_initializer_assigns_and_exports_hoisted_name() {
        let source = "export let { toString } = 1;\n{\n    let { toFixed } = 1;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::with_options(
            &parser.arena,
            PrinterOptions {
                module: ModuleKind::System,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("var toString;"),
            "System wrapper should hoist the exported binding name.\nOutput:\n{output}"
        );
        assert!(
            output.contains("exports_1(\"toString\", toString = 1..toString);"),
            "System wrapper should export the destructuring assignment value.\nOutput:\n{output}"
        );
        assert!(
            output.contains("let { toFixed } = 1;"),
            "Nested block-scoped destructuring should remain a declaration.\nOutput:\n{output}"
        );
    }

    #[test]
    fn system_object_binding_initializer_assigns_hoisted_name() {
        let source = "let { toString } = 1;\n{\n    let { toFixed } = 1;\n}\nexport {};\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::with_options(
            &parser.arena,
            PrinterOptions {
                module: ModuleKind::System,
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("var toString;"),
            "System wrapper should hoist the binding name.\nOutput:\n{output}"
        );
        assert!(
            output.contains("toString = 1..toString;"),
            "System wrapper should initialize the hoisted binding from the object property.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("exports_1(\"toString\""),
            "Non-exported binding should not be exported.\nOutput:\n{output}"
        );
        assert!(
            output.contains("let { toFixed } = 1;"),
            "Nested block-scoped destructuring should remain a declaration.\nOutput:\n{output}"
        );
    }

    /// Imports whose only textual references are to a type alias or
    /// interface of the same name must NOT be retained as runtime imports
    /// just because their `PascalCase` name appears as the return type of
    /// an async function under ES5. Mirrors the existing guard in
    /// `extract_awaiter_promise_constructor`.
    /// Devin review: <https://github.com/mohsen1/tsz/pull/2314#discussion_r3176824619>
    #[test]
    fn amd_es5_type_alias_named_like_import_does_not_force_retention() {
        // The source declares a type alias `Foo` AND imports a value named `Foo`.
        // The async function's return type is `Foo`, but `Foo` is a type alias
        // here, so the import should still be elided (no runtime usage).
        let source = r#"import { Foo } from "lib";
type Foo = string;
async function f(): Foo { return "" as any; }
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::AMD,
            target: ScriptTarget::ES5,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        // The AMD dependency list / require call should NOT include "lib"
        // because the only "use" of `Foo` was as a type position. The buggy
        // version falsely treated the type alias as a Promise constructor
        // and kept the import.
        assert!(
            !output.contains("\"lib\""),
            "AMD wrapper should not keep `lib` import when the only use of `Foo` is as a type alias.\nOutput:\n{output}"
        );
    }

    /// JSX factory imports must not be elided by the AMD/System helper-emission
    /// usage check, even when the factory name doesn't textually appear in the
    /// source (JSX elements reference it implicitly).
    /// Devin review: <https://github.com/mohsen1/tsz/pull/2295#discussion_r3176647570>
    #[test]
    fn amd_jsx_factory_default_import_kept_in_helpers_check() {
        use crate::emitter::JsxEmit;
        let source = r#"import React from "react";
export const Foo = () => <div/>;
"#;
        let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::AMD,
            jsx: JsxEmit::React,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        // The default-import factory ("React") has no textual value usage
        // (only JSX), but because it is a JSX factory we must keep the
        // __importDefault helper definition emitted in the AMD wrapper.
        assert!(
            output.contains("__importDefault"),
            "AMD wrapper should still emit __importDefault helper for JSX factory `React` even without textual value usage.\nOutput:\n{output}"
        );
    }
}
