use super::{NodeIndex, Printer};

impl<'a> Printer<'a> {
    pub(super) fn arrow_concise_body_needs_temp_prologue(&self, body: NodeIndex) -> bool {
        !self.ctx.options.target.supports_es2020()
            && self.param_initializer_generates_hoisted_temp(body)
    }

    pub(super) fn emit_arrow_concise_body_with_temp_prologue(&mut self, body: NodeIndex) {
        // tsc converts a concise-body arrow that needs a hoisted temp (e.g. a
        // class-expression static-state alias) into a *single-line* block body
        // `{ var _a; return <expr>; }`. The synthesized block keeps the brace,
        // `var`, and `return` on the same line; only the emitted expression
        // (a multi-line class body or comma wrapper) expands across lines.
        self.write("{ ");
        let var_insert_pos = self.writer.len();

        let prev_emitting_function_body_block = self.emitting_function_body_block;
        let prev_emitting_concise_arrow_return_argument =
            self.emitting_concise_arrow_return_argument;
        self.emitting_function_body_block = true;
        // The expression is the direct operand of the synthesized `return`, so a
        // class-expression comma wrapper must not be parenthesized.
        self.emitting_concise_arrow_return_argument = true;
        self.function_scope_depth += 1;
        self.arrow_function_scope_depth += 1;
        self.write("return ");
        self.emit(body);
        self.write(";");
        self.arrow_function_scope_depth -= 1;
        self.function_scope_depth -= 1;
        self.emitting_concise_arrow_return_argument = prev_emitting_concise_arrow_return_argument;
        self.emitting_function_body_block = prev_emitting_function_body_block;

        if !self.hoisted_assignment_temps.is_empty() {
            let var_decl = format!("var {}; ", self.hoisted_assignment_temps.join(", "));
            self.writer.insert_at(var_insert_pos, &var_decl);
            self.hoisted_assignment_temps.clear();
        }

        self.write(" }");
    }
}
