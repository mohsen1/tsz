//! TS8038/TS1486: decorators split across `export`/`export default`.
//!
//! Split out of `state_declarations_exports.rs` to stay under the parser's
//! per-file line cap; this is a single self-contained diagnostic concern
//! (`@dec export @dec class` / `@dec export default @dec class`), not a
//! shared dependency of the rest of that file.

use super::state::ParserState;
use crate::parser::NodeIndex;
use tsz_common::diagnostics::diagnostic_codes;

impl ParserState {
    /// TS8038 (`Decorators may not appear after 'export' or 'export default' if
    /// they also appear before 'export'.`) with its TS1486 (`Decorator used
    /// before 'export' here.`) related-info pointer, for `@dec export @dec
    /// class` / `@dec export default @dec class`. `tsc` reports this exactly
    /// once per declaration regardless of how many leading or trailing
    /// decorators are present: anchored at the *first* trailing decorator,
    /// pointing back at the *first* leading decorator (oracle-verified,
    /// `typescript@7.0.2`).
    pub(super) fn report_decorator_used_before_export(
        &mut self,
        leading: &Option<crate::parser::NodeList>,
        trailing: &Option<crate::parser::NodeList>,
    ) {
        let (Some(leading), Some(trailing)) = (leading, trailing) else {
            return;
        };
        let (Some(&first_leading), Some(&first_trailing)) =
            (leading.nodes.first(), trailing.nodes.first())
        else {
            return;
        };
        // A decorator node's own `end` is the end of the token *following* it
        // (used elsewhere as a trailing-trivia boundary), not the end of its
        // own expression — anchoring the diagnostic there would swallow
        // whatever comes after the decorator (e.g. `export`/`class`). Read
        // the wrapped expression's own end instead, which is unaffected by
        // that convention.
        let (Some(lead_span), Some(trail_span)) = (
            self.decorator_span(first_leading),
            self.decorator_span(first_trailing),
        ) else {
            return;
        };
        let (lead_pos, lead_end) = lead_span;
        let (trail_pos, trail_end) = trail_span;
        self.parse_error_at_with_related(
            trail_pos,
            trail_end - trail_pos,
            "Decorators may not appear after 'export' or 'export default' if they also appear before 'export'.",
            diagnostic_codes::DECORATORS_MAY_NOT_APPEAR_AFTER_EXPORT_OR_EXPORT_DEFAULT_IF_THEY_ALSO_APPEAR_BEF,
            crate::parser::state::ParseDiagnosticRelated {
                start: lead_pos,
                length: lead_end - lead_pos,
                message: "Decorator used before 'export' here.".to_string(),
                code: diagnostic_codes::DECORATOR_USED_BEFORE_EXPORT_HERE,
            },
        );
    }

    /// A decorator node's true `(start, end)` span — the `@` token through
    /// the end of its wrapped expression — rather than the decorator node's
    /// own `end` field, which points past the token that follows it.
    fn decorator_span(&self, decorator: NodeIndex) -> Option<(u32, u32)> {
        let decorator_node = self.arena.get(decorator)?;
        let data = self.arena.get_decorator_at(decorator)?;
        let expr_end = self.arena.get(data.expression)?.end;
        Some((decorator_node.pos, expr_end))
    }
}
