//! Alias display helpers for assignability diagnostics.

use crate::diagnostics::{diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    fn generic_alias_name_from_display(display: &str) -> Option<&str> {
        let display = display.trim_start();
        let (name, _) = display.split_once('<')?;
        let name = name.trim();
        (!name.is_empty()
            && name
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
        .then_some(name)
    }

    fn declared_generic_alias_annotation_matches_target_display(
        annotation: &str,
        target_display: &str,
    ) -> bool {
        let Some(annotation_name) = Self::generic_alias_name_from_display(annotation) else {
            return false;
        };
        let Some(target_name) = Self::generic_alias_name_from_display(target_display) else {
            return false;
        };
        annotation_name == target_name
    }

    pub(in crate::error_reporter) fn declared_generic_alias_source_display_for_target_display(
        &self,
        anchor_idx: NodeIndex,
        source_display: &str,
        target_display: &str,
    ) -> Option<String> {
        if !source_display.contains(" extends ") && !source_display.contains("infer ") {
            return None;
        }
        let expr_idx = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))?;
        let annotation_text = self.declared_type_annotation_text_for_expression(expr_idx)?;
        Self::declared_generic_alias_annotation_matches_target_display(
            &annotation_text,
            target_display,
        )
        .then(|| self.format_declared_annotation_for_diagnostic(&annotation_text))
    }

    pub(in crate::error_reporter) fn rewrite_declared_generic_alias_source_in_ts2322_message(
        &self,
        anchor_idx: NodeIndex,
        message: String,
    ) -> String {
        let Some(rest) = message.strip_prefix("Type '") else {
            return message;
        };
        let Some((source_display, target_part)) = rest.split_once("' is not assignable to type '")
        else {
            return message;
        };
        let Some(target_display) = target_part.strip_suffix("'.") else {
            return message;
        };
        let Some(source_display) = self.declared_generic_alias_source_display_for_target_display(
            anchor_idx,
            source_display,
            target_display,
        ) else {
            return message;
        };
        format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_display, target_display],
        )
    }
}
