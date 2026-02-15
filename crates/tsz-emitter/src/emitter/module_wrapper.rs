use super::Printer;
use crate::emitter::ModuleKind;
use std::collections::HashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
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

    pub(super) fn emit_amd_wrapper(
        &mut self,
        dependencies: &[String],
        source_node: &tsz_parser::parser::node::Node,
        source_idx: NodeIndex,
    ) {
        use crate::transforms::module_commonjs;

        self.write("define([\"require\", \"exports\"");
        for dep in dependencies {
            self.write(", \"");
            self.write(dep);
            self.write("\"");
        }
        self.write("], function (require, exports");
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
    }

    pub(super) fn emit_umd_wrapper(
        &mut self,
        source_node: &tsz_parser::parser::node::Node,
        source_idx: NodeIndex,
    ) {
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
        self.write("define([\"require\", \"exports\"], factory);");
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
        let hoisted_names = self.collect_system_hoisted_names(source);
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
        self.write("setters: [],");
        self.write_line();
        self.write("execute: function () {");
        self.write_line();
        self.increase_indent();

        self.emit_system_execute_body(source_node);

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

    fn emit_system_execute_body(&mut self, source_node: &tsz_parser::parser::node::Node) {
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

        for &stmt_idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let before_len = self.writer.len();

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

    fn emit_system_variable_initializers(&mut self, node: &tsz_parser::parser::node::Node) {
        let Some(var_stmt) = self.arena.get_variable(node) else {
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

                if decl.initializer.is_none() {
                    continue;
                }

                self.emit(decl.name);
                self.write(" = ");
                self.emit_expression(decl.initializer);
                self.write_semicolon();
            }
        }
    }
}
