use crate::context::TypingRequest;

use crate::diagnostics::{
    DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes, diagnostic_messages,
    format_message,
};

use crate::error_reporter::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
    ResolvedDiagnosticAnchor,
};

use crate::query_boundaries::checkers::jsx as jsx_queries;

use crate::state::CheckerState;

use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

use tsz_solver::computation::TypeSubstitution;

use tsz_solver::{ObjectShape, TypeId};

include!("validation_parts/part1.rs");
include!("validation_parts/part2.rs");

#[cfg(test)]
mod tests {
    #[test]
    fn jsx_props_target_selection_avoids_anonymous_display_prefix_decision() {
        let source = include_str!("validation.rs");
        let formatted_member_call = ["format_type", "(member)"].join("");
        let starts_with_object = [".starts_with", "('{')"].join("");
        let inline_forbidden = format!("{formatted_member_call}{starts_with_object}");
        for forbidden in [
            inline_forbidden,
            [
                "let display = self.format_type(member);",
                "let is_anonymous = display.starts_with('{');",
            ]
            .join("\n"),
        ] {
            assert!(
                !source.contains(&forbidden),
                "JSX props target selection must use TypeId/query facts, \
                 not formatted anonymous-object display prefixes: found {forbidden}"
            );
        }
    }
}
