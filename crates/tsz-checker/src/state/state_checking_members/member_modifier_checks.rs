//! Class/member modifier validation helpers.

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_async_modifier_on_declaration(
        &mut self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) {
        use crate::diagnostics::diagnostic_codes;

        if let Some(async_mod_idx) = self.find_async_modifier(modifiers) {
            self.error_at_node(
                async_mod_idx,
                "'async' modifier cannot be used here.",
                diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
            );
        }
    }
}
