use super::{ModuleKind, Printer};

impl<'a> Printer<'a> {
    pub(super) fn emit_module_wrapper(
        &mut self,
        format: &crate::transform_context::ModuleFormat,
        dependencies: &[String],
        source_node: &tsz_parser::parser::node::Node,
        source: &tsz_parser::parser::node::SourceFileData,
    ) {
        match format {
            crate::transform_context::ModuleFormat::AMD => {
                self.emit_amd_wrapper(dependencies, source_node);
            }
            crate::transform_context::ModuleFormat::UMD => {
                self.emit_umd_wrapper(source_node);
            }
            crate::transform_context::ModuleFormat::System => {
                self.emit_system_wrapper(dependencies, source_node);
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

        self.emit_module_wrapper_body(source_node);

        self.decrease_indent();
        self.write("});");
    }

    pub(super) fn emit_umd_wrapper(&mut self, source_node: &tsz_parser::parser::node::Node) {
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

        self.emit_module_wrapper_body(source_node);

        self.decrease_indent();
        self.write("});");
    }

    pub(super) fn emit_system_wrapper(
        &mut self,
        dependencies: &[String],
        source_node: &tsz_parser::parser::node::Node,
    ) {
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
        self.write("return {");
        self.write_line();
        self.increase_indent();
        self.write("setters: [],");
        self.write_line();
        self.write("execute: function () {");
        self.write_line();
        self.increase_indent();

        self.emit_module_wrapper_body(source_node);

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
    ) {
        let prev_module = self.ctx.options.module;
        let prev_auto_detect = self.ctx.auto_detect_module;
        let prev_original = self.ctx.original_module_kind;

        // Remember the actual module kind (AMD/UMD/System) so export assignments
        // can emit `return X` instead of `module.exports = X` in AMD.
        self.ctx.original_module_kind = Some(prev_module);
        self.ctx.options.module = ModuleKind::CommonJS;
        self.ctx.auto_detect_module = false;

        self.emit_source_file(source_node);

        self.ctx.options.module = prev_module;
        self.ctx.auto_detect_module = prev_auto_detect;
        self.ctx.original_module_kind = prev_original;
    }
}
