//! `super.<member>` legality gates keyed on the member's declaration kind
//! (TS2340 at ES5, TS2855 at ES2015+).
//!
//! Extracted from `property_checker.rs` to keep that module under the
//! 2000-LOC ceiling.

use crate::classes_domain::class_summary::ClassMemberKind;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// tsc's `checkPropertyAccessibilityAtLocation` runs its `isSuper` gates
    /// BEFORE any visibility check, so both fire for private members too
    /// (hence `skip_private: false`):
    ///   1. Under target ES5, a super member backed by a non-*method*
    ///      declaration (plain field, accessor, auto-accessor field —
    ///      instance or static) is TS2340: ES5 emit can only dispatch super
    ///      methods.
    ///   2. Otherwise a parent **instance** field via super is TS2855. From
    ///      within a static member/initializer, `super` is the parent class
    ///      object itself (not its prototype), so `super.x` resolves to the
    ///      parent's *static* member — TS2855 must not fire there.
    ///
    /// A private member that passes both gates (a method at any target; an
    /// accessor or static at ES2015+) falls through to the caller's ordinary
    /// private-member check (TS2341). Returns `false` when a diagnostic was
    /// emitted and the access is denied.
    pub(super) fn check_super_member_kind_gates(
        &mut self,
        object_expr: NodeIndex,
        property_name: &str,
        error_node: NodeIndex,
        class_idx: NodeIndex,
        is_static: bool,
        in_static_context: bool,
    ) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if !self.is_super_expression(object_expr) || self.has_syntax_parse_errors() {
            return true;
        }
        let lookup_is_static = is_static || in_static_context;
        let Some((kind, display_name)) = self.class_chain_member_kind_name_only(
            class_idx,
            property_name,
            lookup_is_static,
            false,
        ) else {
            return true;
        };

        if self.ctx.compiler_options.target.is_es5()
            && matches!(kind, ClassMemberKind::Field | ClassMemberKind::Accessor)
        {
            self.error_at_node(
                error_node,
                diagnostic_messages::ONLY_PUBLIC_AND_PROTECTED_METHODS_OF_THE_BASE_CLASS_ARE_ACCESSIBLE_VIA_THE_SUPER,
                diagnostic_codes::ONLY_PUBLIC_AND_PROTECTED_METHODS_OF_THE_BASE_CLASS_ARE_ACCESSIBLE_VIA_THE_SUPER,
            );
            return false;
        }

        if !is_static && !in_static_context && kind == ClassMemberKind::Field {
            let message = format_message(
                diagnostic_messages::CLASS_FIELD_DEFINED_BY_THE_PARENT_CLASS_IS_NOT_ACCESSIBLE_IN_THE_CHILD_CLASS_VIA,
                &[&display_name],
            );
            self.error_at_node(
                error_node,
                &message,
                diagnostic_codes::CLASS_FIELD_DEFINED_BY_THE_PARENT_CLASS_IS_NOT_ACCESSIBLE_IN_THE_CHILD_CLASS_VIA,
            );
            return false;
        }

        true
    }
}
