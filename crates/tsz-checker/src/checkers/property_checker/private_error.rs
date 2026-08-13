//! Private-member access error reporting (TS2341).
//!
//! Extracted from `property_checker.rs` to keep that module under the 2000-LOC ceiling.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// A private member accessed outside its declaring class is TS2341 — for
    /// `super.x` receivers too. tsc's `checkPropertyAccessibilityAtLocation`
    /// has no super special-case in its private branch: the super-specific
    /// diagnostics (ES5 non-method TS2340, instance-field TS2855) fire from
    /// earlier gates that run regardless of visibility, so a private member
    /// that reaches the visibility check gets the same TS2341 as an ordinary
    /// `instance.x` access.
    pub(super) fn report_private_member_error(
        &mut self,
        error_node: NodeIndex,
        property_name: &str,
        declaring_class_name: &str,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
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
