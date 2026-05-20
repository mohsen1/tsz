use crate::state::CheckerState;
use tsz_parser::parser::NodeList;

impl<'a> CheckerState<'a> {
    pub(super) fn validate_jsx_intrinsic_type_arguments(&mut self, type_args: &NodeList) {
        if type_args.has_trailing_comma
            && let Some(comma_pos) = self.jsx_type_argument_trailing_comma_pos(type_args)
        {
            self.error_at_position(
                comma_pos,
                1,
                crate::diagnostics::diagnostic_messages::TRAILING_COMMA_NOT_ALLOWED,
                crate::diagnostics::diagnostic_codes::TRAILING_COMMA_NOT_ALLOWED,
            );
        }

        for &arg_idx in &type_args.nodes {
            self.check_type_node(arg_idx);
            self.get_type_from_type_node(arg_idx);
        }

        let Some(&first_arg_idx) = type_args.nodes.first() else {
            return;
        };
        let got = type_args.nodes.len();
        self.error_at_node_msg(
            first_arg_idx,
            crate::diagnostics::diagnostic_codes::EXPECTED_TYPE_ARGUMENTS_BUT_GOT,
            &["0", &got.to_string()],
        );
    }

    fn jsx_type_argument_trailing_comma_pos(&self, type_args: &NodeList) -> Option<u32> {
        let last_arg = self.ctx.arena.get(*type_args.nodes.last()?)?;
        let source = self.current_jsx_source_text()?;
        let start = last_arg.end as usize;
        let tail = source.get(start..source.len().min(start + 32))?;
        let before_close = tail.split('>').next().unwrap_or(tail);
        before_close
            .find(',')
            .map(|offset| last_arg.end + offset as u32)
    }
}
