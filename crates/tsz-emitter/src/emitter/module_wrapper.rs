use super::Printer;
use crate::emitter::ModuleKind;
use std::collections::{HashMap, HashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    fn hoist_decorate_helper_before_wrapper(&mut self) -> bool {
        if self.ctx.options.no_emit_helpers
            || !self.transforms.helpers_populated()
            || !self.transforms.helpers().decorate
        {
            return false;
        }

        self.write(crate::transforms::helpers::DECORATE_HELPER);
        self.write_line();
        self.transforms.helpers_mut().decorate = false;
        true
    }

    pub(super) fn emit_module_wrapper(
        &mut self,
        format: crate::transform_context::ModuleFormat,
        dependencies: &[String],
        source_node: &tsz_parser::parser::node::Node,
        source: &tsz_parser::parser::node::SourceFileData,
        source_idx: NodeIndex,
    ) {
        match format {
            crate::transform_context::ModuleFormat::AMD => {
                self.emit_amd_wrapper(dependencies, source_node, source_idx);
            }
            crate::transform_context::ModuleFormat::UMD => {
                self.emit_umd_wrapper(source_node, source_idx);
            }
            crate::transform_context::ModuleFormat::System => {
                self.emit_system_wrapper(dependencies, source_node, source_idx);
            }
            _ => {
                for &stmt_idx in &source.statements.nodes {
                    self.emit(stmt_idx);
                    self.write_line();
                }
            }
        }
    }

    /// Extract the last `/// <amd-module name='...' />` directive name from source text.
    /// Extract a quoted attribute value from a triple-slash directive.
    fn extract_directive_attr(content: &str, attr: &str) -> Option<String> {
        let needle = format!("{attr}=");
        let pos = content.find(&needle)?;
        let after = &content[pos + needle.len()..];
        let quote = after.as_bytes().first().copied()?;
        if !matches!(quote, b'\'' | b'"') {
            return None;
        }
        let q = quote as char;
        let end = after[1..].find(q)?;
        Some(after[1..1 + end].to_string())
    }

    /// Extract the last `/// <amd-module name='...' />` directive name from source text.
    fn extract_amd_module_name(&self) -> Option<String> {
        let text = self.source_text?;
        let mut last_name = None;
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("///") {
                if !trimmed.is_empty() && !trimmed.starts_with("//") {
                    break;
                }
                continue;
            }
            let comment = trimmed.trim_start_matches('/').trim();
            if !comment.contains("<amd-module") || !comment.contains("name=") {
                continue;
            }
            if let Some(name) = Self::extract_directive_attr(comment, "name") {
                last_name = Some(name);
            }
        }
        last_name
    }

    /// Extract `/// <amd-dependency path='...' name='...'/>` directives.
    /// Returns (path, `optional_name`, `original_line`) tuples.
    fn extract_amd_dependencies(&self) -> Vec<(String, Option<String>, String)> {
        let Some(text) = self.source_text else {
            return Vec::new();
        };
        let mut deps = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("///") {
                if !trimmed.is_empty() && !trimmed.starts_with("//") {
                    break;
                }
                continue;
            }
            let comment = trimmed.trim_start_matches('/').trim();
            if !comment.contains("<amd-dependency") || !comment.contains("path=") {
                continue;
            }
            if let Some(path) = Self::extract_directive_attr(comment, "path") {
                let name = Self::extract_directive_attr(comment, "name");
                deps.push((path, name, trimmed.to_string()));
            }
        }
        deps
    }

    pub(super) fn emit_amd_wrapper(
        &mut self,
        dependencies: &[String],
        source_node: &tsz_parser::parser::node::Node,
        source_idx: NodeIndex,
    ) {
        use crate::transforms::module_commonjs;

        let restore_decorate_helper = self.hoist_decorate_helper_before_wrapper();
        let amd_name = self.extract_amd_module_name();
        let amd_deps = self.extract_amd_dependencies();

        // Emit `/// <amd-dependency .../>` comments before `define()`.
        for (_, _, original_line) in &amd_deps {
            self.write(original_line);
            self.write_line();
        }

        self.write("define(");
        if let Some(name) = &amd_name {
            self.write("\"");
            self.write(name);
            self.write("\", ");
        }
        self.write("[\"require\", \"exports\"");

        // Named AMD deps come first, then unnamed, then import deps.
        let named_deps: Vec<_> = amd_deps
            .iter()
            .filter(|(_, name, _)| name.is_some())
            .collect();
        let unnamed_deps: Vec<_> = amd_deps
            .iter()
            .filter(|(_, name, _)| name.is_none())
            .collect();

        for (path, _, _) in &named_deps {
            self.write(", \"");
            self.write(path);
            self.write("\"");
        }
        for dep in dependencies {
            self.write(", \"");
            self.write(dep);
            self.write("\"");
        }
        for (path, _, _) in &unnamed_deps {
            self.write(", \"");
            self.write(path);
            self.write("\"");
        }

        self.write("], function (require, exports");
        for (_, name, _) in &named_deps {
            if let Some(n) = name {
                self.write(", ");
                self.write(n);
            }
        }
        for dep in dependencies {
            let name = module_commonjs::sanitize_module_name(dep);
            self.write(", ");
            self.write(&name);
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();

        self.emit_module_wrapper_body(source_node, source_idx);

        self.decrease_indent();
        self.write("});");
        if restore_decorate_helper {
            self.transforms.helpers_mut().decorate = true;
        }
    }

    pub(super) fn emit_umd_wrapper(
        &mut self,
        source_node: &tsz_parser::parser::node::Node,
        source_idx: NodeIndex,
    ) {
        let restore_decorate_helper = self.hoist_decorate_helper_before_wrapper();
        self.write("(function (factory) {");
        self.write_line();
        self.increase_indent();
        self.write("if (typeof module === \"object\" && typeof module.exports === \"object\") {");
        self.write_line();
        self.increase_indent();
        self.write("var v = factory(require, exports);");
        self.write_line();
        self.write("if (v !== undefined) module.exports = v;");
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.write("else if (typeof define === \"function\" && define.amd) {");
        self.write_line();
        self.increase_indent();
        let amd_name = self.extract_amd_module_name();
        self.write("define(");
        if let Some(name) = &amd_name {
            self.write("\"");
            self.write(name);
            self.write("\", ");
        }
        self.write("[\"require\", \"exports\"], factory);");
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.decrease_indent();
        self.write("})(function (require, exports) {");
        self.write_line();
        self.increase_indent();

        self.emit_module_wrapper_body(source_node, source_idx);

        self.decrease_indent();
        self.write("});");
        if restore_decorate_helper {
            self.transforms.helpers_mut().decorate = true;
        }
    }

    pub(super) fn emit_system_wrapper(
        &mut self,
        dependencies: &[String],
        source_node: &tsz_parser::parser::node::Node,
        _source_idx: NodeIndex,
    ) {
        let Some(source) = self.arena.get_source_file(source_node) else {
            return;
        };

        self.write("System.register([");
        for (i, dep) in dependencies.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.write("\"");
            self.write(dep);
            self.write("\"");
        }
        self.write("], function (exports_1, context_1) {");
        self.write_line();
        self.increase_indent();
        self.write("\"use strict\";");
        self.write_line();
        let dep_vars = self.collect_system_dependency_vars(dependencies);
        let mut hoisted_names = self.collect_system_hoisted_names(source);
        for dep in dependencies {
            if let Some(dep_var) = dep_vars.get(dep)
                && !hoisted_names.iter().any(|n| n == dep_var)
            {
                hoisted_names.insert(0, dep_var.clone());
            }
        }
        if !hoisted_names.is_empty() {
            self.write("var ");
            self.write(&hoisted_names.join(", "));
            self.write(";");
            self.write_line();
        }
        self.write("var __moduleName = context_1 && context_1.id;");
        self.write_line();
        self.write("return {");
        self.write_line();
        self.increase_indent();
        self.emit_system_setters(dependencies, &dep_vars);
        self.write_line();
        self.write("execute: function () {");
        self.write_line();
        self.increase_indent();

        self.emit_system_execute_body(source_node, &dep_vars);

        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.decrease_indent();
        self.write("};");
        self.write_line();
        self.decrease_indent();
        self.write("});");
    }

    pub(super) fn emit_module_wrapper_body(
        &mut self,
        source_node: &tsz_parser::parser::node::Node,
        source_idx: NodeIndex,
    ) {
        let prev_module = self.ctx.options.module;
        let prev_auto_detect = self.ctx.auto_detect_module;
        let prev_original = self.ctx.original_module_kind;

        // Remember the actual module kind (AMD/UMD/System) so export assignments
        // can emit `return X` instead of `module.exports = X` in AMD.
        self.ctx.original_module_kind = Some(prev_module);
        self.ctx.options.module = ModuleKind::CommonJS;
        self.ctx.auto_detect_module = false;

        self.emit_source_file(source_node, source_idx);

        self.ctx.options.module = prev_module;
        self.ctx.auto_detect_module = prev_auto_detect;
        self.ctx.original_module_kind = prev_original;
    }

    fn collect_system_hoisted_names(
        &mut self,
        source: &tsz_parser::parser::node::SourceFileData,
    ) -> Vec<String> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();

        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    && let Some(export_decl) = self.arena.get_export_decl(stmt_node)
                    && export_decl.module_specifier.is_none()
                    && !export_decl.is_default_export
                    && let Some(clause_node) = self.arena.get(export_decl.export_clause)
                {
                    if clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                        for name in self.collect_variable_names_from_node(clause_node) {
                            if !name.is_empty() && seen.insert(name.clone()) {
                                names.push(name);
                            }
                        }
                        continue;
                    }
                    if clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
                        && let Some(class_decl) = self.arena.get_class(clause_node)
                    {
                        let name = self.get_identifier_text_idx(class_decl.name);
                        if !name.is_empty() && seen.insert(name.clone()) {
                            names.push(name);
                        }
                    }
                }
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

                    let mut binding_names = Vec::new();
                    self.collect_binding_names(decl.name, &mut binding_names);
                    for name in binding_names {
                        if !name.is_empty() && seen.insert(name.clone()) {
                            names.push(name);
                        }
                    }
                }
            }
        }

        names
    }

    fn collect_system_dependency_vars(&self, dependencies: &[String]) -> HashMap<String, String> {
        let mut dep_vars = HashMap::new();
        for (idx, dep) in dependencies.iter().enumerate() {
            let base = crate::transforms::module_commonjs::sanitize_module_name(dep);
            dep_vars.insert(dep.clone(), format!("{base}_{}", idx + 1));
        }
        dep_vars
    }

    fn emit_system_setters(&mut self, dependencies: &[String], dep_vars: &HashMap<String, String>) {
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
            self.write("function (");
            self.write(dep_var);
            self.write("_1) {");
            self.write_line();
            self.increase_indent();
            self.write(dep_var);
            self.write(" = ");
            self.write(dep_var);
            self.write("_1;");
            self.write_line();
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

    fn register_system_import_substitutions(
        &mut self,
        source: &tsz_parser::parser::node::SourceFileData,
        dep_vars: &HashMap<String, String>,
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
            let Some(dep_var) = dep_vars.get(&module_spec) else {
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
                    self.get_identifier_text_idx(spec.property_name)
                } else {
                    local_name.clone()
                };
                self.commonjs_named_import_substitutions
                    .insert(local_name, format!("{dep_var}.{import_name}"));
            }
        }
    }

    fn emit_system_execute_body(
        &mut self,
        source_node: &tsz_parser::parser::node::Node,
        dep_vars: &HashMap<String, String>,
    ) {
        let prev_module = self.ctx.options.module;
        let prev_auto_detect = self.ctx.auto_detect_module;
        let prev_original = self.ctx.original_module_kind;

        self.ctx.original_module_kind = Some(prev_module);
        self.ctx.options.module = ModuleKind::CommonJS;
        self.ctx.auto_detect_module = false;

        let Some(source) = self.arena.get_source_file(source_node) else {
            self.ctx.options.module = prev_module;
            self.ctx.auto_detect_module = prev_auto_detect;
            self.ctx.original_module_kind = prev_original;
            return;
        };
        self.register_system_import_substitutions(source, dep_vars);

        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }
            let before_len = self.writer.len();

            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && self.emit_system_export_declaration(stmt_node)
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

            if self.writer.len() > before_len {
                self.write_line();
            }
        }

        self.ctx.options.module = prev_module;
        self.ctx.auto_detect_module = prev_auto_detect;
        self.ctx.original_module_kind = prev_original;
    }

    fn emit_system_export_declaration(&mut self, node: &tsz_parser::parser::node::Node) -> bool {
        let Some(export_decl) = self.arena.get_export_decl(node) else {
            return false;
        };
        if export_decl.is_default_export || export_decl.module_specifier.is_some() {
            return false;
        }
        let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
            return false;
        };

        if clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
            self.emit_system_variable_initializers(clause_node);
            return true;
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
            self.emit_class_es6(clause_node, export_decl.export_clause);
            self.write(";");
            self.write_line();
            self.write("exports_1(\"");
            self.write(&class_name);
            self.write("\", ");
            self.write(&class_name);
            self.write(");");
            return true;
        }

        false
    }

    fn system_module_specifier_text(&self, specifier: NodeIndex) -> Option<String> {
        if specifier.is_none() {
            return None;
        }
        let node = self.arena.get(specifier)?;
        let literal = self.arena.get_literal(node)?;
        Some(literal.text.clone())
    }

    fn emit_system_variable_initializers(&mut self, node: &tsz_parser::parser::node::Node) {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return;
        };
        let is_exported = self.has_export_modifier(&var_stmt.modifiers);

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
}
