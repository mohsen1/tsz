//! Accessor accessibility parity checks (a get accessor must be at least as
//! accessible as its paired setter).
//!
//! Split out of [`super::ambient_signature_checks`] to keep that module within
//! the checker file-size budget.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// Check if a setter has a paired getter with the same name in the class.
    ///
    /// TSC infers setter parameter types from the getter return type, so a setter
    /// with a paired getter has contextually typed parameters (no TS7006).
    pub(crate) fn setter_has_paired_getter(
        &self,
        _setter_idx: NodeIndex,
        setter_accessor: &tsz_parser::parser::node::AccessorData,
    ) -> bool {
        self.paired_getter_member_for_setter(setter_accessor)
            .is_some()
    }

    pub(crate) fn check_getter_setter_accessibility(
        &mut self,
        getter: &tsz_parser::parser::node::AccessorData,
    ) {
        let getter_name = match self.get_property_name(getter.name) {
            Some(n) => n,
            None => return,
        };

        let should_error = {
            let Some(ref class_info) = self.ctx.enclosing_class else {
                return;
            };
            let mut should_error = false;
            for &member_idx in &class_info.member_nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::SET_ACCESSOR {
                    continue;
                }
                let Some(setter) = self.ctx.arena.get_accessor(member_node) else {
                    continue;
                };
                let Some(setter_name) = self.get_property_name(setter.name) else {
                    continue;
                };
                if setter_name != getter_name {
                    continue;
                }

                let getter_level = self.accessibility_level(&getter.modifiers);
                let setter_level = self.accessibility_level(&setter.modifiers);
                should_error = getter_level < setter_level;
                break;
            }
            should_error
        };

        if should_error {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                getter.name,
                diagnostic_messages::A_GET_ACCESSOR_MUST_BE_AT_LEAST_AS_ACCESSIBLE_AS_THE_SETTER,
                diagnostic_codes::A_GET_ACCESSOR_MUST_BE_AT_LEAST_AS_ACCESSIBLE_AS_THE_SETTER,
            );
        }
    }

    fn accessibility_level(&self, modifiers: &Option<tsz_parser::parser::NodeList>) -> u8 {
        if self.has_private_modifier(modifiers) {
            1
        } else if self.has_protected_modifier(modifiers) {
            2
        } else {
            3 // public (explicit or implicit)
        }
    }

    pub(crate) fn check_setter_getter_accessibility(
        &mut self,
        setter: &tsz_parser::parser::node::AccessorData,
    ) {
        let setter_name = match self.get_property_name(setter.name) {
            Some(n) => n,
            None => return,
        };

        let should_error = {
            let Some(ref class_info) = self.ctx.enclosing_class else {
                return;
            };
            let mut should_error = false;
            for &member_idx in &class_info.member_nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::GET_ACCESSOR {
                    continue;
                }
                let Some(getter) = self.ctx.arena.get_accessor(member_node) else {
                    continue;
                };
                let Some(getter_name) = self.get_property_name(getter.name) else {
                    continue;
                };
                if getter_name != setter_name {
                    continue;
                }

                let getter_level = self.accessibility_level(&getter.modifiers);
                let setter_level = self.accessibility_level(&setter.modifiers);
                should_error = getter_level < setter_level;
                break;
            }
            should_error
        };

        if should_error {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                setter.name,
                diagnostic_messages::A_GET_ACCESSOR_MUST_BE_AT_LEAST_AS_ACCESSIBLE_AS_THE_SETTER,
                diagnostic_codes::A_GET_ACCESSOR_MUST_BE_AT_LEAST_AS_ACCESSIBLE_AS_THE_SETTER,
            );
        }
    }
}
