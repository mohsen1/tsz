//! Private-member access error dispatch: TS2340 vs TS2341.
//!
//! Extracted from `property_checker.rs` to keep that module under the 2000-LOC ceiling.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// tsc distinguishes `super.x` (TS2340) from `instance.x` (TS2341) for private members.
    pub(super) fn report_private_member_error(
        &mut self,
        error_node: NodeIndex,
        object_expr: NodeIndex,
        property_name: &str,
        declaring_class_name: &str,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        if self.is_super_expression(object_expr) {
            self.error_at_node(
                error_node,
                diagnostic_messages::ONLY_PUBLIC_AND_PROTECTED_METHODS_OF_THE_BASE_CLASS_ARE_ACCESSIBLE_VIA_THE_SUPER,
                diagnostic_codes::ONLY_PUBLIC_AND_PROTECTED_METHODS_OF_THE_BASE_CLASS_ARE_ACCESSIBLE_VIA_THE_SUPER,
            );
        } else {
            let message = format_message(
                diagnostic_messages::PROPERTY_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS,
                &[property_name, declaring_class_name],
            );
            self.error_at_node(
                error_node,
                &message,
                diagnostic_codes::PROPERTY_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS,
            );
        }
    }
}
