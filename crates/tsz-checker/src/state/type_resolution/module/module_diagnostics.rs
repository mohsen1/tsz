use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(crate) fn emit_module_has_no_default_export_at(
        &mut self,
        module_specifier: &str,
        decl_node: NodeIndex,
    ) {
        let (start, length) = if decl_node.is_some() {
            if let Some(node) = self.ctx.arena.get(decl_node) {
                (node.pos, node.end - node.pos)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let display_name = self.imported_namespace_display_module_name(module_specifier);
        let quoted_name = format!("\"{display_name}\"");
        let message = format_message(
            diagnostic_messages::MODULE_HAS_NO_DEFAULT_EXPORT,
            &[&quoted_name],
        );
        self.error(
            start,
            length,
            message,
            diagnostic_codes::MODULE_HAS_NO_DEFAULT_EXPORT,
        );
    }
}
