use super::super::Printer;
use super::{SystemDependencyAction, SystemDependencyPlan};
use crate::emitter::ModuleKind;
use std::collections::{HashMap, HashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

type WrappedValueDeps = Vec<(String, String)>;
type WrappedDependencyGroups = (WrappedValueDeps, Vec<String>, HashMap<String, String>);

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

    pub(in crate::emitter) fn emit_module_wrapper(
        &mut self,
        format: crate::context::transform::ModuleFormat,
        dependencies: &[String],
        source_node: &tsz_parser::parser::node::Node,
        source: &tsz_parser::parser::node::SourceFileData,
        source_idx: NodeIndex,
    ) {
        match format {
            crate::context::transform::ModuleFormat::AMD => {
                self.emit_amd_wrapper(dependencies, source_node, source_idx);
            }
            crate::context::transform::ModuleFormat::UMD => {
                self.emit_umd_wrapper(dependencies, source_node, source_idx);
            }
            crate::context::transform::ModuleFormat::System => {
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

    /// Extract `/// <reference .../>` directives from the source file header
    /// that tsc preserves before the AMD/UMD/System wrapper.
    ///
    /// tsc strips reference directives pointing to local `.d.ts` files that are
    /// part of the same compilation (they're consumed during type checking).
    /// References with absolute paths (like `/.lib/react.d.ts`) are preserved.
    /// We use path shape as a heuristic: only emit references with absolute paths.
    fn extract_reference_directives(&self) -> Vec<String> {
        let Some(text) = self.source_text else {
            return Vec::new();
        };
        let mut refs = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("///") {
                if !trimmed.is_empty() && !trimmed.starts_with("//") {
                    break;
                }
                continue;
            }
            let comment = trimmed.trim_start_matches('/').trim();
            if comment.starts_with("<reference") {
                if let Some(path) = Self::extract_directive_attr(comment, "path") {
                    let references_compilation_dts =
                        self.arena.source_files.iter().any(|source_file| {
                            source_file.is_declaration_file
                                && (source_file.file_name == path
                                    || source_file.file_name.ends_with(&format!("/{path}")))
                        });
                    if (path.starts_with('/') && !references_compilation_dts)
                        || self.should_preserve_bang_module_reference(&path, text)
                    {
                        refs.push(trimmed.to_string());
                    }
                } else {
                    // Non-path references (e.g., `/// <reference lib="dom" />`)
                    refs.push(trimmed.to_string());
                }
            }
        }
        refs
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
        let restore_decorate_helper = self.hoist_decorate_helper_before_wrapper();
        let amd_name = self
            .extract_amd_module_name()
            .or_else(|| self.ctx.options.bundled_module_name.clone());
        let amd_deps = self.extract_amd_dependencies();
        let Some(source) = self.arena.get_source_file(source_node) else {
            return;
        };
        let (value_deps, side_effect_deps, dep_vars) =
            self.collect_amd_dependency_groups(dependencies, source);

        // Emit `/// <reference .../>` directives before `define()` — tsc places
        // these at file top level, outside the AMD wrapper body.
        for directive in &self.extract_reference_directives() {
            self.write(directive);
            self.write_line();
        }
        // Emit `/// <amd-dependency .../>` comments before `define()`.
        for (_, _, original_line) in &amd_deps {
            self.write(original_line);
            self.write_line();
        }
        self.emit_wrapped_import_helpers(source);

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
        for (dep, _) in &value_deps {
            self.write(", \"");
            self.write(dep);
            self.write("\"");
        }
        for (path, _, _) in &unnamed_deps {
            self.write(", \"");
            self.write(path);
            self.write("\"");
        }
        for dep in &side_effect_deps {
            self.write(", \"");
            self.write(dep);
            self.write("\"");
        }

        self.write("], function (require, exports");
        for (_, name, _) in &named_deps {
            if let Some(n) = name {
                self.write(", ");
                self.write(n);
            }
        }
        for (_, name) in &value_deps {
            if !name.is_empty() {
                self.write(", ");
                self.write(name);
            }
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();

        // AMD modules get "use strict" inside the define() callback, matching tsc.
        // Only emit for module files (files with import/export syntax).
        if self.file_is_module(&source.statements) {
            self.write("\"use strict\";");
            self.write_line();
            self.ctx.options.suppress_use_strict = true;
        }

        if self.ctx.options.module != ModuleKind::AMD {
            let empty_system_plan = SystemDependencyPlan::default();
            self.register_system_import_substitutions(source, &dep_vars, &empty_system_plan);
        }

        self.emit_module_wrapper_body(source_node, source_idx);
        self.ctx.options.suppress_use_strict = false;

        self.decrease_indent();
        self.write("});");
        // Add a trailing newline so that when multiple AMD modules are
        // concatenated (outFile mode), each define() block is properly
        // separated on its own line. This matches tsc behavior.
        self.write_line();
        if restore_decorate_helper {
            self.transforms.helpers_mut().decorate = true;
        }
    }

    pub(super) fn emit_umd_wrapper(
        &mut self,
        dependencies: &[String],
        source_node: &tsz_parser::parser::node::Node,
        source_idx: NodeIndex,
    ) {
        let restore_decorate_helper = self.hoist_decorate_helper_before_wrapper();
        let amd_name = self.extract_amd_module_name();
        let amd_deps = self.extract_amd_dependencies();
        let Some(source) = self.arena.get_source_file(source_node) else {
            return;
        };
        let (value_deps, side_effect_deps, _dep_vars) =
            self.collect_amd_dependency_groups(dependencies, source);

        // Emit `/// <reference .../>` directives before the UMD wrapper.
        for directive in &self.extract_reference_directives() {
            self.write(directive);
            self.write_line();
        }
        // Emit `/// <amd-dependency .../>` comments before the UMD wrapper.
        for (_, _, original_line) in &amd_deps {
            self.write(original_line);
            self.write_line();
        }
        self.emit_wrapped_import_helpers(source);

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
        self.write("define(");
        if let Some(name) = &amd_name {
            self.write("\"");
            self.write(name);
            self.write("\", ");
        }
        self.write("[\"require\", \"exports\"");

        // Named AMD deps come first, then unnamed, then import deps —
        // same ordering as the AMD wrapper.
        let named_deps: Vec<_> = amd_deps
            .iter()
            .filter(|(_, name, _)| name.is_some())
            .collect();
        let unnamed_deps: Vec<_> = amd_deps
            .iter()
            .filter(|(_, name, _)| name.is_none())
            .collect();

        // UMD ordering: named amd-deps, unnamed amd-deps, import deps,
        // side-effect deps. This differs from AMD where import deps come
        // before unnamed amd-deps.
        for (path, _, _) in &named_deps {
            self.write(", \"");
            self.write(path);
            self.write("\"");
        }
        for (path, _, _) in &unnamed_deps {
            self.write(", \"");
            self.write(path);
            self.write("\"");
        }
        for (dep, _) in &value_deps {
            self.write(", \"");
            self.write(dep);
            self.write("\"");
        }
        for dep in &side_effect_deps {
            self.write(", \"");
            self.write(dep);
            self.write("\"");
        }

        self.write("], factory);");
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.decrease_indent();

        // Factory function signature: named amd-dependency params appear after
        // `require, exports`.
        self.write("})(function (require, exports");
        for (_, name, _) in &named_deps {
            if let Some(n) = name {
                self.write(", ");
                self.write(n);
            }
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();

        // UMD modules get "use strict" inside the factory callback, matching tsc.
        let source = self.arena.get_source_file(source_node);
        if let Some(source) = source
            && self.file_is_module(&source.statements)
        {
            self.write("\"use strict\";");
            self.write_line();
            if self.source_has_dynamic_import_call(&source.statements) {
                self.write(
                    "var __syncRequire = typeof module === \"object\" && typeof module.exports === \"object\";",
                );
                self.write_line();
            }
            self.ctx.options.suppress_use_strict = true;
        }

        self.emit_module_wrapper_body(source_node, source_idx);
        self.ctx.options.suppress_use_strict = false;

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

        self.write("System.register(");
        if let Some(name) = self.ctx.options.bundled_module_name.clone() {
            self.write("\"");
            self.write(&name);
            self.write("\", ");
        }
        self.write("[");
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
        self.emit_system_decorate_helper_if_needed(source);
        let system_plan = self.collect_system_dependency_plan(dependencies, source);
        let mut system_plan = system_plan;
        self.add_system_jsx_runtime_dependency(dependencies, &mut system_plan);
        let mut dep_vars = self.collect_system_dependency_vars(dependencies, source);
        for (dep, actions) in &system_plan.actions {
            if let Some(SystemDependencyAction::Assign(dep_var)) = actions
                .iter()
                .find(|action| matches!(action, SystemDependencyAction::Assign(_)))
            {
                dep_vars.insert(dep.clone(), dep_var.clone());
            }
        }
        let mut hoisted_names = self.collect_system_hoisted_names(source, &system_plan);
        let func_names_to_exclude = self.collect_system_hoisted_function_names(source);
        hoisted_names.retain(|n| !func_names_to_exclude.contains(n));
        if !hoisted_names.is_empty() {
            self.write("var ");
            self.write(&hoisted_names.join(", "));
            self.write(";");
            self.write_line();
        }
        self.write("var __moduleName = context_1 && context_1.id;");
        self.write_line();

        self.register_system_import_substitutions(source, &dep_vars, &system_plan);

        // Hoist exported function declarations to the outer module scope,
        // before the `return { setters, execute }` block.  TSC does the same:
        // function declarations are syntactically hoisted, so they (and their
        // corresponding `exports_1` calls) live outside `execute`.
        let hoisted_func_stmts = self.emit_system_hoisted_functions(source);

        self.write("return {");
        self.write_line();
        self.increase_indent();
        self.emit_system_setters(dependencies, &dep_vars, &system_plan);
        self.write_line();
        let execute_is_async = source.statements.nodes.iter().any(|&stmt_idx| {
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
        if execute_is_async {
            self.write("execute: async function () {");
        } else {
            self.write("execute: function () {");
        }
        self.write_line();
        self.increase_indent();

        self.emit_system_execute_body(source_node, &dep_vars, &hoisted_func_stmts, &system_plan);

        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.decrease_indent();
        self.write("};");
        self.write_line();
        self.decrease_indent();
        self.write("});");
    }

    fn collect_system_hoisted_function_names(
        &self,
        source: &tsz_parser::parser::node::SourceFileData,
    ) -> HashSet<String> {
        let mut names = HashSet::new();
        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                let Some(export_decl) = self.arena.get_export_decl(stmt_node) else {
                    continue;
                };
                if export_decl.module_specifier.is_some() {
                    continue;
                }
                let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
                    continue;
                };
                if clause_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                    continue;
                }
                if let Some(func_decl) = self.arena.get_function(clause_node) {
                    let func_name = self.get_identifier_text_idx(func_decl.name);
                    if func_name.is_empty() {
                        if export_decl.is_default_export {
                            names.insert("default_1".to_string());
                        }
                    } else {
                        names.insert(func_name);
                    }
                }
            }
            if stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                && let Some(func_decl) = self.arena.get_function(stmt_node)
            {
                let func_name = self.get_identifier_text_idx(func_decl.name);
                if !func_name.is_empty() {
                    names.insert(func_name);
                }
            }
        }
        names
    }

    /// Hoist exported function declarations out of `execute` into the outer
    /// module-wrapper scope.  Returns the set of statement `NodeIndex`es that
    /// were hoisted so they can be skipped inside `emit_system_execute_body`.
    fn emit_system_hoisted_functions(
        &mut self,
        source: &tsz_parser::parser::node::SourceFileData,
    ) -> HashSet<NodeIndex> {
        let mut hoisted = HashSet::new();

        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            // Case 1: `export function foo() {}` or `export default function foo() {}`
            // These appear as EXPORT_DECLARATION with a FUNCTION_DECLARATION export_clause.
            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                let Some(export_decl) = self.arena.get_export_decl(stmt_node) else {
                    continue;
                };
                // Only handle local exports (no module specifier)
                if export_decl.module_specifier.is_some() {
                    continue;
                }
                let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
                    continue;
                };
                if clause_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                    continue;
                }
                let Some(func_decl) = self.arena.get_function(clause_node) else {
                    continue;
                };
                let func_name = self.get_identifier_text_idx(func_decl.name);
                if func_name.is_empty() {
                    // `export default function() {}` — anonymous, needs a generated name
                    // TSC gives it `default_1` and still hoists it.
                    let gen_name = if export_decl.is_default_export {
                        "default_1".to_string()
                    } else {
                        continue;
                    };
                    // Emit `function default_1() { }` at the outer scope
                    self.write("function ");
                    self.write(&gen_name);
                    self.write("() { }");
                    self.write_line();
                    self.write("exports_1(\"default\", ");
                    self.write(&gen_name);
                    self.write(");");
                    self.write_line();
                    hoisted.insert(stmt_idx);
                    continue;
                }

                // Emit `function foo() { <body> }` at the outer scope
                self.emit(export_decl.export_clause);
                self.write_line();

                let export_name = if export_decl.is_default_export {
                    "default"
                } else {
                    &func_name
                };
                self.write("exports_1(\"");
                self.write(export_name);
                self.write("\", ");
                self.write(&func_name);
                self.write(");");
                self.write_line();

                hoisted.insert(stmt_idx);
            }
            // Case 2: Non-exported function declarations — also hoisted in system modules.
            // TSC hoists ALL function declarations to the outer scope.
            if stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                let Some(func_decl) = self.arena.get_function(stmt_node) else {
                    continue;
                };
                let func_name = self.get_identifier_text_idx(func_decl.name);
                if func_name.is_empty() {
                    continue;
                }
                // Emit the function at the outer scope
                self.emit(stmt_idx);
                self.write_line();
                hoisted.insert(stmt_idx);
            }
        }

        // Hoist `export { foo }` / `export { foo as bar }` where foo is a hoisted function
        let hoisted_func_names = self.collect_system_hoisted_function_names(source);
        for &stmt_idx in &source.statements.nodes {
            if hoisted.contains(&stmt_idx) {
                continue;
            }
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
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
            if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                continue;
            }
            let Some(named_exports) = self.arena.get_named_imports(clause_node) else {
                continue;
            };
            let mut all_hoisted = true;
            let mut specs = Vec::new();
            for &spec_idx in &named_exports.elements.nodes {
                let Some(spec) = self.arena.get_specifier_at(spec_idx) else {
                    continue;
                };
                let local_name = if spec.property_name.is_some() {
                    self.get_identifier_text_idx(spec.property_name)
                } else {
                    self.get_identifier_text_idx(spec.name)
                };
                let export_name = self.get_identifier_text_idx(spec.name);
                if !hoisted_func_names.contains(&local_name) {
                    all_hoisted = false;
                    break;
                }
                specs.push((local_name, export_name));
            }
            if all_hoisted && !specs.is_empty() {
                for (local_name, export_name) in &specs {
                    self.write("exports_1(\"");
                    self.write(export_name);
                    self.write("\", ");
                    self.write(local_name);
                    self.write(");");
                    self.write_line();
                }
                hoisted.insert(stmt_idx);
            }
        }

        hoisted
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

    fn collect_system_dependency_plan(
        &mut self,
        dependencies: &[String],
        source: &tsz_parser::parser::node::SourceFileData,
    ) -> SystemDependencyPlan {
        let dependency_set: HashSet<&str> = dependencies.iter().map(String::as_str).collect();
        let mut plan = SystemDependencyPlan::default();

        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                let Some(import_decl) = self.arena.get_import_decl(stmt_node) else {
                    continue;
                };
                if !self.import_decl_has_runtime_value(import_decl) {
                    continue;
                }
                let Some(module_spec) =
                    self.system_module_specifier_text(import_decl.module_specifier)
                else {
                    continue;
                };
                if !dependency_set.contains(module_spec.as_str()) {
                    continue;
                }
                let local_name = self.get_identifier_text_idx(import_decl.import_clause);
                if local_name.is_empty() {
                    continue;
                }

                plan.import_vars.insert(stmt_node.pos, local_name.clone());
                plan.actions
                    .entry(module_spec)
                    .or_default()
                    .push(SystemDependencyAction::Assign(local_name));
                continue;
            }

            if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                let Some(import_decl) = self.arena.get_import_decl(stmt_node) else {
                    continue;
                };
                if !self.import_decl_has_runtime_value(import_decl) {
                    continue;
                }
                let Some(module_spec) =
                    self.system_module_specifier_text(import_decl.module_specifier)
                else {
                    continue;
                };
                if !dependency_set.contains(module_spec.as_str()) {
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

                let mut has_value_binding = clause.name.is_some();
                let mut namespace_name = None;
                if clause.named_bindings.is_some()
                    && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                {
                    if let Some(named_imports) = self.arena.get_named_imports(bindings_node) {
                        if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
                            let local_name = self.get_identifier_text_idx(named_imports.name);
                            if !local_name.is_empty() {
                                namespace_name = Some(local_name);
                            }
                            has_value_binding = true;
                        } else {
                            has_value_binding |= !self
                                .collect_value_specifiers(&named_imports.elements)
                                .is_empty();
                        }
                    } else {
                        has_value_binding = true;
                    }
                }
                if !has_value_binding {
                    continue;
                }

                let dep_var =
                    namespace_name.unwrap_or_else(|| self.next_commonjs_module_var(&module_spec));
                plan.import_vars.insert(stmt_node.pos, dep_var.clone());
                plan.actions
                    .entry(module_spec)
                    .or_default()
                    .push(SystemDependencyAction::Assign(dep_var));
                continue;
            }

            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if !self.export_decl_has_runtime_value(export_decl) {
                continue;
            }
            let Some(module_spec) = self.system_module_specifier_text(export_decl.module_specifier)
            else {
                continue;
            };
            if !dependency_set.contains(module_spec.as_str()) {
                continue;
            }
            let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
                continue;
            };
            if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
                && let Some(named_exports) = self.arena.get_named_imports(clause_node)
            {
                let mut exports = Vec::new();
                for &spec_idx in &self.collect_value_specifiers(&named_exports.elements) {
                    let Some(spec) = self.arena.get_specifier_at(spec_idx) else {
                        continue;
                    };
                    let Some(export_name) = self.get_specifier_name_text(spec.name) else {
                        continue;
                    };
                    let import_name = if spec.property_name.is_some() {
                        self.get_specifier_name_text(spec.property_name)
                            .unwrap_or_else(|| export_name.clone())
                    } else {
                        export_name.clone()
                    };
                    exports.push((export_name, import_name));
                }
                if !exports.is_empty() {
                    plan.actions
                        .entry(module_spec)
                        .or_default()
                        .push(SystemDependencyAction::NamedExports(exports));
                }
                continue;
            }

            if clause_node.kind == SyntaxKind::Identifier as u16
                || clause_node.kind == SyntaxKind::StringLiteral as u16
            {
                let export_name = self
                    .get_specifier_name_text(export_decl.export_clause)
                    .unwrap_or_else(|| self.get_identifier_text_idx(export_decl.export_clause));
                if !export_name.is_empty() {
                    plan.actions
                        .entry(module_spec)
                        .or_default()
                        .push(SystemDependencyAction::NamespaceExport(export_name));
                }
            }
        }

        plan
    }

    fn collect_system_dependency_vars(
        &self,
        dependencies: &[String],
        source: &tsz_parser::parser::node::SourceFileData,
    ) -> HashMap<String, String> {
        let mut dep_vars = HashMap::new();
        for (idx, dep) in dependencies.iter().enumerate() {
            let mut chosen = None;
            for &stmt_idx in &source.statements.nodes {
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                    continue;
                }
                let Some(import_decl) = self.arena.get_import_decl(stmt_node) else {
                    continue;
                };
                if !self.import_decl_has_runtime_value(import_decl) {
                    continue;
                }
                if self
                    .system_module_specifier_text(import_decl.module_specifier)
                    .as_deref()
                    != Some(dep.as_str())
                {
                    continue;
                }
                let local_name = self.get_identifier_text_idx(import_decl.import_clause);
                if !local_name.is_empty() {
                    chosen = Some(local_name);
                    break;
                }
            }

            let dep_var = if let Some(local_name) = chosen {
                local_name
            } else {
                let base = crate::transforms::emit_utils::sanitize_module_name(dep);
                format!("{base}_{}", idx + 1)
            };
            dep_vars.insert(dep.clone(), dep_var);
        }
        dep_vars
    }

    fn register_wrapped_import_substitutions(
        &mut self,
        import_decl: &tsz_parser::parser::node::ImportDeclData,
        module_var: &str,
    ) {
        let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
            return;
        };
        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return;
        };
        if clause.is_type_only {
            return;
        }

        if clause.name.is_some() {
            let local_name = self.get_identifier_text_idx(clause.name);
            if !local_name.is_empty() {
                self.commonjs_named_import_substitutions
                    .insert(local_name, format!("{module_var}.default"));
            }
        }

        let Some(bindings_node) = self.arena.get(clause.named_bindings) else {
            return;
        };
        let Some(named_imports) = self.arena.get_named_imports(bindings_node) else {
            return;
        };

        if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
            let local_name = self.get_identifier_text_idx(named_imports.name);
            if !local_name.is_empty() {
                self.commonjs_named_import_substitutions
                    .insert(local_name, module_var.to_string());
            }
            return;
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
                format!("{module_var}.{import_name}")
            } else {
                format!("{module_var}[\"{import_name}\"]")
            };
            self.commonjs_named_import_substitutions
                .insert(local_name, substitution);
        }
    }

    fn collect_amd_dependency_groups(
        &mut self,
        dependencies: &[String],
        source: &tsz_parser::parser::node::SourceFileData,
    ) -> WrappedDependencyGroups {
        let mut value_deps = Vec::new();
        let mut side_effect_deps = Vec::new();
        let mut dep_vars = HashMap::new();
        let mut seen_value = HashSet::new();
        let mut seen_side_effect = HashSet::new();
        // Track deps that were explicitly rejected (type-only usage) so they
        // don't get re-added from the `dependencies` fallback list.
        let mut rejected_deps: HashSet<String> = HashSet::new();
        let collect_for_amd = self.ctx.options.module == ModuleKind::AMD;
        let collect_for_umd = self.ctx.options.module == ModuleKind::UMD;

        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                let Some(import_decl) = self.arena.get_import_decl(stmt_node) else {
                    continue;
                };
                if !self.import_decl_has_runtime_value(import_decl) {
                    continue;
                }
                // When JSX mode requires a factory, don't elide imports matching
                // the factory name — JSX elements implicitly reference it but the
                // text-based heuristic won't find it in the source.
                let import_name = self.get_identifier_text_idx(import_decl.import_clause);
                let is_jsx_factory = self.is_classic_jsx_factory_root(&import_name);
                // Check value-level usage: `import x = require("m")` where
                // `x` is only used in type positions should not be included in
                // AMD deps (tsc elides these).
                let has_value_usage = self
                    .import_equals_has_value_usage_after_node(stmt_node, import_decl)
                    || self.import_equals_has_value_usage_through_namespace_alias_after_node(
                        stmt_idx,
                        import_decl,
                        source,
                    );
                if !is_jsx_factory && !has_value_usage {
                    if let Some(spec) =
                        self.system_module_specifier_text(import_decl.module_specifier)
                    {
                        rejected_deps.insert(spec);
                    }
                    continue;
                }
                let Some(module_spec) =
                    self.system_module_specifier_text(import_decl.module_specifier)
                else {
                    continue;
                };
                let local_name = self.get_identifier_text_idx(import_decl.import_clause);
                if local_name.is_empty() {
                    continue;
                }
                if collect_for_umd {
                    seen_value.insert(module_spec.clone());
                    value_deps.push((module_spec, String::new()));
                } else if collect_for_amd {
                    seen_value.insert(module_spec.clone());
                    value_deps.push((module_spec, local_name));
                } else if seen_value.insert(module_spec.clone()) {
                    value_deps.push((module_spec.clone(), local_name.clone()));
                    dep_vars.entry(module_spec).or_insert(local_name);
                }
                continue;
            }

            if (collect_for_amd || collect_for_umd)
                && stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_decl) = self.arena.get_export_decl(stmt_node)
            {
                if !self.export_decl_has_runtime_value(export_decl) {
                    continue;
                }
                if let Some(module_spec) =
                    self.system_module_specifier_text(export_decl.module_specifier)
                {
                    seen_value.insert(module_spec.clone());
                    if collect_for_amd {
                        let dep_var = self.next_commonjs_module_var(&module_spec);
                        value_deps.push((module_spec, dep_var.clone()));
                        self.wrapped_export_module_substitutions
                            .insert(stmt_node.pos, dep_var);
                    } else {
                        value_deps.push((module_spec, String::new()));
                    }
                }
                continue;
            }

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
            let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
                if seen_side_effect.insert(module_spec.clone()) {
                    side_effect_deps.push(module_spec);
                }
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                if seen_side_effect.insert(module_spec.clone()) {
                    side_effect_deps.push(module_spec);
                }
                continue;
            };
            if clause.is_type_only {
                continue;
            }

            let mut has_value_binding = clause.name.is_some();
            let mut namespace_name: Option<String> = None;
            if clause.named_bindings.is_some()
                && let Some(bindings_node) = self.arena.get(clause.named_bindings)
            {
                if let Some(named_imports) = self.arena.get_named_imports(bindings_node) {
                    if named_imports.name.is_some() && named_imports.elements.nodes.is_empty() {
                        let local_name = self.get_identifier_text_idx(named_imports.name);
                        if !local_name.is_empty() {
                            namespace_name = Some(local_name);
                        }
                        has_value_binding = true;
                    } else {
                        let value_specs = self.collect_value_specifiers(&named_imports.elements);
                        has_value_binding |= !value_specs.is_empty();
                    }
                } else {
                    has_value_binding = true;
                }
            }

            if !has_value_binding {
                if seen_side_effect.insert(module_spec.clone()) {
                    side_effect_deps.push(module_spec);
                }
                continue;
            }

            // Text-based check: even if the import has value bindings, if
            // none of them are used at the value level, the import should
            // not appear in AMD deps (tsc uses checker info to elide these).
            // Skip this check for JSX factory imports which are implicitly
            // referenced by JSX elements.
            let is_jsx_factory_import = self.is_jsx_factory_import_clause(clause);

            if !is_jsx_factory_import && !self.import_has_value_usage_after_node(stmt_node, clause)
            {
                rejected_deps.insert(module_spec);
                continue;
            }

            if collect_for_umd {
                seen_value.insert(module_spec.clone());
                value_deps.push((module_spec, String::new()));
            } else if collect_for_amd {
                seen_value.insert(module_spec.clone());
                let dep_var = if let Some(ns_name) = namespace_name {
                    ns_name
                } else {
                    self.next_commonjs_module_var(&module_spec)
                };
                self.register_wrapped_import_substitutions(import_decl, &dep_var);
                value_deps.push((module_spec, dep_var));
            } else if seen_value.insert(module_spec.clone()) {
                let dep_var = if let Some(ns_name) = namespace_name {
                    ns_name
                } else {
                    self.next_commonjs_module_var(&module_spec)
                };
                value_deps.push((module_spec.clone(), dep_var.clone()));
                dep_vars.entry(module_spec).or_insert(dep_var);
            }
        }

        for dep in dependencies {
            if seen_value.contains(dep)
                || seen_side_effect.contains(dep)
                || rejected_deps.contains(dep)
            {
                continue;
            }
            if seen_side_effect.insert(dep.clone()) {
                side_effect_deps.push(dep.clone());
            }
        }

        (value_deps, side_effect_deps, dep_vars)
    }
}
