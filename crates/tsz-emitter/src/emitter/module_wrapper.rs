use super::{JsxEmit, Printer};
use crate::emitter::ModuleKind;
use rustc_hash::FxHashMap;
use std::collections::{HashMap, HashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

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
                self.emit_umd_wrapper(source_node, source_idx);
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
                // Only emit references with absolute paths — relative paths
                // typically resolve to files in the same compilation and tsc
                // strips those from JS output.
                if let Some(path) = Self::extract_directive_attr(comment, "path") {
                    if path.starts_with('/') {
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
        let amd_name = self.extract_amd_module_name();
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
        for dep in &value_deps {
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
        for dep in &value_deps {
            if let Some(name) = dep_vars.get(dep) {
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

        self.register_system_import_substitutions(source, &dep_vars);

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
        source_node: &tsz_parser::parser::node::Node,
        source_idx: NodeIndex,
    ) {
        let restore_decorate_helper = self.hoist_decorate_helper_before_wrapper();
        // Emit `/// <reference .../>` directives before the UMD wrapper.
        for directive in &self.extract_reference_directives() {
            self.write(directive);
            self.write_line();
        }
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

        // UMD modules get "use strict" inside the factory callback, matching tsc.
        if let Some(source) = self.arena.get_source_file(source_node)
            && self.file_is_module(&source.statements)
        {
            self.write("\"use strict\";");
            self.write_line();
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
        let dep_vars = self.collect_system_dependency_vars(dependencies, source);
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

        // Hoist exported function declarations to the outer module scope,
        // before the `return { setters, execute }` block.  TSC does the same:
        // function declarations are syntactically hoisted, so they (and their
        // corresponding `exports_1` calls) live outside `execute`.
        let hoisted_func_stmts = self.emit_system_hoisted_functions(source);

        self.write("return {");
        self.write_line();
        self.increase_indent();
        self.emit_system_setters(dependencies, &dep_vars);
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
                    (decl_list_node.flags as u32 & tsz_parser::parser::node_flags::AWAIT_USING)
                        == tsz_parser::parser::node_flags::AWAIT_USING
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

        self.emit_system_execute_body(source_node, &dep_vars, &hoisted_func_stmts);

        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.decrease_indent();
        self.write("};");
        self.write_line();
        self.decrease_indent();
        self.write("});");
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

    fn collect_system_hoisted_names(
        &mut self,
        source: &tsz_parser::parser::node::SourceFileData,
    ) -> Vec<String> {
        let mut names = Vec::new();
        let mut deferred_named_export_names = Vec::new();
        let mut seen_deferred_named_export_names = HashSet::new();
        let mut seen = HashSet::new();
        let mut seen_top_level_using = false;
        let has_top_level_using = !self.ctx.options.target.supports_es2025()
            && source
                .statements
                .nodes
                .iter()
                .filter_map(|&stmt_idx| self.arena.get(stmt_idx))
                .any(|stmt_node| self.statement_is_top_level_using(stmt_node));

        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if self.statement_is_top_level_using(stmt_node) {
                seen_top_level_using = true;
            }
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                if stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    && let Some(class_decl) = self.arena.get_class(stmt_node)
                {
                    let class_name = self.get_identifier_text_idx(class_decl.name);
                    if !class_name.is_empty() && seen.insert(class_name.clone()) {
                        names.push(class_name);
                    }
                    if has_top_level_using
                        && self
                            .arena
                            .has_modifier(&class_decl.modifiers, SyntaxKind::ExportKeyword)
                        && self
                            .arena
                            .has_modifier(&class_decl.modifiers, SyntaxKind::DefaultKeyword)
                        && class_decl.name.is_some()
                        && seen.insert("_default".to_string())
                    {
                        names.push("_default".to_string());
                    }
                }
                if stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    && let Some(import_decl) = self.arena.get_import_decl(stmt_node)
                    && self.import_decl_has_runtime_value(import_decl)
                {
                    let local_name = self.get_identifier_text_idx(import_decl.import_clause);
                    if !local_name.is_empty() && seen.insert(local_name.clone()) {
                        names.push(local_name);
                    }
                }
                if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && let Some(module_decl) = self.arena.get_module(stmt_node)
                {
                    // Skip ambient/declare module declarations — they don't
                    // produce runtime code and shouldn't be hoisted.
                    // e.g., `declare global { interface ImportMeta {...} }`
                    let is_ambient = self
                        .arena
                        .has_modifier(&module_decl.modifiers, SyntaxKind::DeclareKeyword);
                    if !is_ambient {
                        let module_name = self.get_identifier_text_idx(module_decl.name);
                        if !module_name.is_empty() && seen.insert(module_name.clone()) {
                            names.push(module_name);
                        }
                    }
                }
                if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    && let Some(export_decl) = self.arena.get_export_decl(stmt_node)
                    && export_decl.module_specifier.is_none()
                    && let Some(clause_node) = self.arena.get(export_decl.export_clause)
                {
                    // `export default class {}` or `export default class Foo {}` —
                    // hoist `var default_1;` or `var Foo;` respectively.
                    if export_decl.is_default_export
                        && clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    {
                        if let Some(class_decl) = self.arena.get_class(clause_node) {
                            let class_name = self.get_identifier_text_idx(class_decl.name);
                            if has_top_level_using && class_name.is_empty() {
                                if self.ctx.target_es5 && seen.insert("default_1".to_string()) {
                                    names.push("default_1".to_string());
                                }
                                if seen_top_level_using && seen.insert("_default".to_string()) {
                                    names.push("_default".to_string());
                                }
                                continue;
                            }
                            let name = if class_name.is_empty() {
                                "default_1".to_string()
                            } else {
                                class_name
                            };
                            if seen.insert(name.clone()) {
                                names.push(name);
                            }
                            if has_top_level_using
                                && ((class_decl.name.is_some()) || seen_top_level_using)
                                && seen.insert("_default".to_string())
                            {
                                names.push("_default".to_string());
                            }
                        }
                        continue;
                    }
                    if has_top_level_using
                        && export_decl.is_default_export
                        && clause_node.kind != syntax_kind_ext::FUNCTION_DECLARATION
                        && clause_node.kind != syntax_kind_ext::CLASS_DECLARATION
                        && seen.insert("_default".to_string())
                    {
                        let insert_at = names.len().saturating_sub(1);
                        names.insert(insert_at, "_default".to_string());
                        continue;
                    }
                    if has_top_level_using
                        && export_decl.is_default_export
                        && (clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                            || clause_node.kind == syntax_kind_ext::CLASS_DECLARATION)
                    {
                        let has_local_name =
                            if clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                                self.arena
                                    .get_function(clause_node)
                                    .and_then(|func| self.get_identifier_text_opt(func.name))
                                    .is_some_and(|name| !name.is_empty())
                            } else {
                                self.arena
                                    .get_class(clause_node)
                                    .and_then(|class| self.get_identifier_text_opt(class.name))
                                    .is_some_and(|name| !name.is_empty())
                            };
                        if has_local_name && seen.insert("_default".to_string()) {
                            let insert_at = names.len().saturating_sub(1);
                            names.insert(insert_at, "_default".to_string());
                        }
                    }
                    if export_decl.is_default_export {
                        continue;
                    }
                    if clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                        for name in self.collect_variable_names_from_node(clause_node) {
                            if !name.is_empty() && seen.insert(name.clone()) {
                                names.push(name);
                            }
                        }
                        continue;
                    }
                    if has_top_level_using
                        && clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
                        && let Some(named_exports) = self.arena.get_named_imports(clause_node)
                    {
                        for &spec_idx in &named_exports.elements.nodes {
                            if let Some(spec) = self.arena.get_specifier_at(spec_idx) {
                                let local_name = if spec.property_name.is_some() {
                                    self.get_identifier_text_idx(spec.property_name)
                                } else {
                                    self.get_identifier_text_idx(spec.name)
                                };
                                if !local_name.is_empty()
                                    && seen_deferred_named_export_names.insert(local_name.clone())
                                {
                                    deferred_named_export_names.push(local_name);
                                }
                            }
                        }
                        continue;
                    }
                    if clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        && let Some(import_decl) = self.arena.get_import_decl(clause_node)
                        && self.import_decl_has_runtime_value(import_decl)
                    {
                        let name = self.get_identifier_text_idx(import_decl.import_clause);
                        if !name.is_empty() && seen.insert(name.clone()) {
                            names.push(name);
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
                    if clause_node.kind == syntax_kind_ext::MODULE_DECLARATION
                        && let Some(module_decl) = self.arena.get_module(clause_node)
                    {
                        let is_ambient = self
                            .arena
                            .has_modifier(&module_decl.modifiers, SyntaxKind::DeclareKeyword);
                        if !is_ambient {
                            let name = self.get_identifier_text_idx(module_decl.name);
                            if !name.is_empty() && seen.insert(name.clone()) {
                                names.push(name);
                            }
                        }
                    }
                }
                if has_top_level_using {
                    let needs_default_temp = (stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                        && self
                            .arena
                            .get_export_assignment(stmt_node)
                            .is_some_and(|export_assignment| !export_assignment.is_export_equals))
                        || (stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                            && self
                                .arena
                                .get_export_decl(stmt_node)
                                .is_some_and(|export_decl| {
                                    export_decl.is_default_export
                                        && export_decl.module_specifier.is_none()
                                        && self.arena.get(export_decl.export_clause).is_some_and(
                                            |clause_node| {
                                                clause_node.kind
                                                    != syntax_kind_ext::FUNCTION_DECLARATION
                                                    && clause_node.kind
                                                        != syntax_kind_ext::CLASS_DECLARATION
                                            },
                                        )
                                }));
                    if needs_default_temp && seen.insert("_default".to_string()) {
                        let insert_at = names.len().saturating_sub(1);
                        names.insert(insert_at, "_default".to_string());
                    }
                }
                continue;
            }
            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                continue;
            };
            if self
                .arena
                .has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword)
            {
                continue;
            }

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

        if has_top_level_using && seen.insert("env_1".to_string()) {
            names.push("env_1".to_string());
        }

        for name in deferred_named_export_names {
            if seen.insert(name.clone()) {
                names.push(name);
            }
        }

        names
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

    fn collect_amd_dependency_groups(
        &mut self,
        dependencies: &[String],
        source: &tsz_parser::parser::node::SourceFileData,
    ) -> (Vec<String>, Vec<String>, HashMap<String, String>) {
        let mut value_deps = Vec::new();
        let mut side_effect_deps = Vec::new();
        let mut dep_vars = HashMap::new();
        let mut seen_value = HashSet::new();
        let mut seen_side_effect = HashSet::new();
        // Track deps that were explicitly rejected (type-only usage) so they
        // don't get re-added from the `dependencies` fallback list.
        let mut rejected_deps: HashSet<String> = HashSet::new();

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
                let is_jsx_factory = matches!(
                    self.ctx.options.jsx,
                    JsxEmit::Preserve | JsxEmit::React | JsxEmit::ReactNative
                ) && {
                    let import_name = self.get_identifier_text_idx(import_decl.import_clause);
                    let factory_root = self
                        .ctx
                        .options
                        .jsx_factory
                        .as_deref()
                        .and_then(|f| f.split('.').next())
                        .unwrap_or("React");
                    import_name == factory_root
                };
                // Check value-level usage: `import x = require("m")` where
                // `x` is only used in type positions should not be included in
                // AMD deps (tsc elides these).
                if !is_jsx_factory
                    && !self.import_equals_has_value_usage_after_node(stmt_node, import_decl)
                {
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
                if seen_value.insert(module_spec.clone()) {
                    value_deps.push(module_spec.clone());
                }
                dep_vars.entry(module_spec).or_insert(local_name);
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
            let is_jsx_factory_import = matches!(
                self.ctx.options.jsx,
                JsxEmit::Preserve | JsxEmit::React | JsxEmit::ReactNative
            ) && {
                let factory_root = self
                    .ctx
                    .options
                    .jsx_factory
                    .as_deref()
                    .and_then(|f| f.split('.').next())
                    .unwrap_or("React");
                let mut is_factory = false;
                // Check default import name
                if clause.name.is_some() {
                    let name = self.get_identifier_text_idx(clause.name);
                    if name == factory_root {
                        is_factory = true;
                    }
                }
                // Check namespace import name (`import * as React`)
                if !is_factory
                    && let Some(ns) = &namespace_name
                    && ns == factory_root
                {
                    is_factory = true;
                }
                is_factory
            };

            if !is_jsx_factory_import && !self.import_has_value_usage_after_node(stmt_node, clause)
            {
                rejected_deps.insert(module_spec);
                continue;
            }

            if seen_value.insert(module_spec.clone()) {
                value_deps.push(module_spec.clone());
            }
            let dep_var = if let Some(ns_name) = namespace_name {
                ns_name
            } else {
                self.next_commonjs_module_var(&module_spec)
            };
            dep_vars.entry(module_spec).or_insert(dep_var);
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

    fn emit_wrapped_import_helpers(&mut self, source: &tsz_parser::parser::node::SourceFileData) {
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
        hoisted_func_stmts: &HashSet<NodeIndex>,
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
        self.register_system_import_substitutions(source, dep_vars);

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
                    let s = crate::safe_slice::slice(
                        text,
                        expr_node.pos as usize,
                        expr_node.end as usize,
                    );
                    s == "\"use strict\"" || s == "'use strict'"
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
            } else {
                // For MODULE_DECLARATION (direct or inside EXPORT_DECLARATION),
                // mark the namespace name as declared so the IIFE emitter doesn't
                // emit a duplicate `var` declaration (the var is already hoisted).
                if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && let Some(module_decl) = self.arena.get_module(stmt_node)
                {
                    let module_name = self.get_identifier_text_idx(module_decl.name);
                    if !module_name.is_empty() {
                        self.declared_namespace_names.insert(module_name);
                    }
                }
                if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    && let Some(export_decl) = self.arena.get_export_decl(stmt_node)
                    && let Some(clause_node) = self.arena.get(export_decl.export_clause)
                    && clause_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && let Some(module_decl) = self.arena.get_module(clause_node)
                {
                    let module_name = self.get_identifier_text_idx(module_decl.name);
                    if !module_name.is_empty() {
                        self.declared_namespace_names.insert(module_name);
                    }
                }
                self.emit(stmt_idx);
            }

            if self.writer.len() > before_len
                && stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION
            {
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
                        self.get_identifier_text_idx(spec.property_name)
                    } else {
                        self.get_identifier_text_idx(spec.name)
                    };
                    let export_name = self.get_identifier_text_idx(spec.name);
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
                        (decl_list_node.flags as u32 & tsz_parser::parser::node_flags::AWAIT_USING)
                            == tsz_parser::parser::node_flags::AWAIT_USING
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
                self.emit_top_level_using_class_assignment(stmt_node, stmt_idx, export_name)
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
            let using_async = (flags & tsz_parser::parser::node_flags::AWAIT_USING)
                == tsz_parser::parser::node_flags::AWAIT_USING;

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
            let is_await_using = (flags & tsz_parser::parser::node_flags::AWAIT_USING)
                == tsz_parser::parser::node_flags::AWAIT_USING;

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

#[cfg(test)]
mod tests {
    use crate::emitter::{ModuleKind, Printer, PrinterOptions};
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

    /// AMD wrappers should strip relative-path `/// <reference>` directives.
    #[test]
    fn amd_reference_directive_relative_path_stripped() {
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
            "Relative-path reference should be stripped from AMD output.\nOutput:\n{output}"
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
}
