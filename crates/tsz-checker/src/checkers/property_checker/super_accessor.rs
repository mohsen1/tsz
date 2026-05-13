use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(super) fn super_accessor_error(
        &mut self,
        object_expr: NodeIndex,
        property_name: &str,
        error_node: NodeIndex,
        class_idx: NodeIndex,
        is_static: bool,
    ) -> bool {
        if !self.is_super_expression(object_expr)
            || self.is_in_static_class_member_context(error_node)
            || !self.class_chain_member_is_accessor(class_idx, property_name, is_static)
            || self.is_property_access_write_context(error_node)
        {
            return false;
        }

        self.error_at_node(
            error_node,
            diagnostic_messages::ONLY_PUBLIC_AND_PROTECTED_METHODS_OF_THE_BASE_CLASS_ARE_ACCESSIBLE_VIA_THE_SUPER,
            diagnostic_codes::ONLY_PUBLIC_AND_PROTECTED_METHODS_OF_THE_BASE_CLASS_ARE_ACCESSIBLE_VIA_THE_SUPER,
        );
        true
    }
}
