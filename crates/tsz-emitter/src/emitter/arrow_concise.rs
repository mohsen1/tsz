use super::{NodeIndex, Printer};

impl<'a> Printer<'a> {
    pub(super) fn arrow_concise_body_needs_temp_prologue(&self, body: NodeIndex) -> bool {
        !self.ctx.options.target.supports_es2020()
            && self.param_initializer_generates_hoisted_temp(body)
    }

    pub(super) fn emit_arrow_concise_body_with_temp_prologue(&mut self, body: NodeIndex) {
        self.open_brace();
        self.write_line();
        self.increase_indent();
        let hoist_anchor = self.capture_hoist_anchor();

        let prev_emitting_function_body_block = self.emitting_function_body_block;
        self.emitting_function_body_block = true;
        self.function_scope_depth += 1;
        self.arrow_function_scope_depth += 1;
        self.write("return ");
        self.emit(body);
        self.write(";");
        self.arrow_function_scope_depth -= 1;
        self.function_scope_depth -= 1;
        self.emitting_function_body_block = prev_emitting_function_body_block;

        if !self.hoisted_assignment_temps.is_empty() {
            let indent = self.writer.indent_string_at(hoist_anchor.indent_level);
            let var_decl = format!(
                "{}var {};",
                indent,
                self.hoisted_assignment_temps.join(", ")
            );
            self.writer
                .insert_line_at(hoist_anchor.byte_offset, hoist_anchor.line_no, &var_decl);
            self.hoisted_assignment_temps.clear();
        }

        self.write_line();
        self.decrease_indent();
        self.close_brace();
    }
}
