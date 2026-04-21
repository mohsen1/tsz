use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(super) fn check_function_expression_name_diagnostics(
        &mut self,
        idx: NodeIndex,
        name_node: Option<NodeIndex>,
        function_is_async: bool,
        function_is_generator: bool,
    ) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        if node.kind != syntax_kind_ext::FUNCTION_EXPRESSION {
            return;
        }

        let Some(name_idx) = name_node else {
            return;
        };
        let Some(name_n) = self.ctx.arena.get(name_idx) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_n) else {
            return;
        };
        let name = &ident.escaped_text;

        if self.is_strict_mode_for_node(name_idx)
            && crate::state_checking::is_eval_or_arguments(name)
            && !(self.ctx.enclosing_class.is_some() && name.as_str() == "arguments")
        {
            self.emit_eval_or_arguments_strict_mode_error(name_idx, name);
        }

        if self.is_strict_mode_for_node(name_idx)
            && !self.ctx.is_ambient_declaration(idx)
            && crate::state_checking::is_strict_mode_reserved_name(name)
        {
            self.emit_strict_mode_reserved_word_error(name_idx, name, true);
        }

        if function_is_async && !function_is_generator && name == "await" {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                name_idx,
                "Identifier expected. 'await' is a reserved word that cannot be used here.",
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
            );
        }
    }
}
