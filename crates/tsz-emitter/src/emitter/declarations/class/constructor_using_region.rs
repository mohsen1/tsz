use super::super::super::Printer;
use tsz_parser::parser::{NodeIndex, NodeList};

pub(in crate::emitter) struct ConstructorUsingRegion {
    env_name: String,
    error_name: String,
    result_name: String,
    using_async: bool,
    prev_block_using_env: Option<(String, bool)>,
}

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn reserve_constructor_using_region(
        &mut self,
        block_idx: NodeIndex,
        statements: &NodeList,
    ) -> ConstructorUsingRegion {
        let using_async = self.block_has_await_using(statements);
        let (env_name, error_name, result_name) = self.disposable_env_names_for_node(block_idx);
        ConstructorUsingRegion {
            env_name,
            error_name,
            result_name,
            using_async,
            prev_block_using_env: None,
        }
    }

    pub(in crate::emitter) fn begin_constructor_using_region(
        &mut self,
        region: &mut ConstructorUsingRegion,
    ) {
        let env_decl_keyword = if self.ctx.target_es5 { "var" } else { "const" };
        region.prev_block_using_env = self
            .block_using_env
            .replace((region.env_name.clone(), region.using_async));

        self.write(env_decl_keyword);
        self.write(" ");
        self.write(&region.env_name);
        self.write(" = { stack: [], error: void 0, hasError: false };");
        self.write_line();
        self.write("try {");
        self.write_line();
        self.increase_indent();
    }

    pub(in crate::emitter) fn end_constructor_using_region(
        &mut self,
        region: ConstructorUsingRegion,
    ) {
        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.write("catch (");
        self.write(&region.error_name);
        self.write(") {");
        self.write_line();
        self.increase_indent();
        self.write(&region.env_name);
        self.write(".error = ");
        self.write(&region.error_name);
        self.write(";");
        self.write_line();
        self.write(&region.env_name);
        self.write(".hasError = true;");
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.write("finally {");
        self.write_line();
        self.increase_indent();
        if region.using_async {
            let await_kw = if self.ctx.emit_await_as_yield || self.ctx.emit_await_as_yield_await {
                "yield"
            } else {
                "await"
            };
            self.write(if self.ctx.target_es5 { "var" } else { "const" });
            self.write(" ");
            self.write(&region.result_name);
            self.write(" = ");
            self.write_helper("__disposeResources");
            self.write("(");
            self.write(&region.env_name);
            self.write(");");
            self.write_line();
            self.write("if (");
            self.write(&region.result_name);
            self.write(")");
            self.write_line();
            self.increase_indent();
            self.write(await_kw);
            self.write(" ");
            if self.ctx.emit_await_as_yield_await {
                self.write_helper("__await");
                self.write("(");
                self.write(&region.result_name);
                self.write(")");
            } else {
                self.write(&region.result_name);
            }
            self.write(";");
            self.write_line();
            self.decrease_indent();
        } else {
            self.write_helper("__disposeResources");
            self.write("(");
            self.write(&region.env_name);
            self.write(");");
            self.write_line();
        }
        self.decrease_indent();
        self.write("}");
        self.write_line();
        self.block_using_env = region.prev_block_using_env;
    }
}
