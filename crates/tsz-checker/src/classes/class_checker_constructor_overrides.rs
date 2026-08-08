//! Constructor parameter-property override checks, split out of
//! `class_checker.rs` to keep it under the 2000-line architecture cap.

use crate::classes_domain::class_summary::ClassChainSummary;
use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// Report explicit/implicit override errors for constructor parameter properties.
    pub(crate) fn check_constructor_parameter_property_overrides(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
        base_class_idx: Option<NodeIndex>,
        base_chain_summary: Option<&ClassChainSummary>,
        base_class_name: &str,
        derived_class_name: &str,
        base_instance_member_names: &rustc_hash::FxHashSet<String>,
        no_implicit_override: bool,
    ) {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }

            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                continue;
            };
            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if !self.has_parameter_property_modifier(&param.modifiers) {
                    continue;
                }
                let Some(param_name) = self.get_property_name(param.name) else {
                    continue;
                };

                let has_override = self.has_override_modifier(&param.modifiers)
                    || self.has_jsdoc_override_tag(param_idx);
                let base_member = match (base_class_idx, base_chain_summary) {
                    (Some(base_idx), Some(summary)) => {
                        let _ = base_idx;
                        summary.lookup(&param_name, false, true).cloned()
                    }
                    (Some(base_idx), None) => {
                        self.find_member_in_class_chain(base_idx, &param_name, false, 0, true)
                    }
                    (None, _) => None,
                };

                if has_override {
                    if base_class_idx.is_none() {
                        // tsc points at the parameter declaration (starting at the
                        // first modifier like 'public'), not just the identifier name.
                        // Use ctx.error() directly to bypass normalized_anchor_span
                        // which would strip modifiers and point at just the name.
                        self.ctx.error(
                            param_node.pos,
                            param_node.end - param_node.pos,
                            crate::diagnostics::format_message(
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N,
                                &[base_class_name],
                            ),
                            diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N,
                        );
                        continue;
                    }

                    if base_member.is_none() {
                        // tsc points at the parameter declaration (starting at the
                        // first modifier like 'public'), not just the identifier name.
                        if let Some(suggestion) = self
                            .find_override_name_suggestion(base_instance_member_names, &param_name)
                        {
                            self.ctx.error(
                                param_node.pos,
                                param_node.end - param_node.pos,
                                crate::diagnostics::format_message(
                                    diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                                    &[base_class_name, &suggestion],
                                ),
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                            );
                        } else {
                            self.ctx.error(
                                param_node.pos,
                                param_node.end - param_node.pos,
                                crate::diagnostics::format_message(
                                    diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                                    &[base_class_name],
                                ),
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                            );
                        }
                    }
                } else if no_implicit_override && base_member.is_some() {
                    // tsc points TS4115 at the parameter declaration (starting at the
                    // first modifier like 'public'), not just the identifier name.
                    self.ctx.error(
                        param_node.pos,
                        param_node.end - param_node.pos,
                        crate::diagnostics::format_message(
                            diagnostic_messages::THIS_PARAMETER_PROPERTY_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_ME,
                            &[base_class_name],
                        ),
                        diagnostic_codes::THIS_PARAMETER_PROPERTY_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_ME,
                    );
                }

                // TS2610: constructor parameter property overrides a base accessor
                // A parameter property like `constructor(public p: string)` acts as an
                // instance property. If the base class defines the same name as an
                // accessor (get/set), this is an accessor/property kind mismatch.
                if let Some(ref base_info) = base_member
                    && base_info.is_accessor
                    && !base_info.is_abstract
                {
                    self.error_at_node(
                            param.name,
                            &format!(
                                "'{param_name}' is defined as an accessor in class '{base_class_name}', but is overridden here in '{derived_class_name}' as an instance property."
                            ),
                            diagnostic_codes::IS_DEFINED_AS_AN_ACCESSOR_IN_CLASS_BUT_IS_OVERRIDDEN_HERE_IN_AS_AN_INSTANCE_PROP,
                        );
                }
            }
        }
    }
}
