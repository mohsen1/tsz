//! Elaboration notes for assignments whose target is a bare type parameter.
//!
//! Mirrors `tsc`'s `isRelatedTo` type-parameter-target handling: when a
//! concrete source fails to relate to a bare type-parameter target, the
//! failure is annotated with one of two notes (`TS5082`/`TS5075`).

use crate::diagnostics::{
    DiagnosticCategory, DiagnosticRelatedInformation, RelatedInformationKind, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Build `tsc`'s type-parameter-target elaboration for a failed assignment
    /// to a bare type-parameter `target`. When a concrete source is assigned to
    /// a bare type-parameter target and the relation fails, `tsc` attaches one
    /// of two notes:
    ///
    ///   * If the parameter has an explicit `extends` constraint (including
    ///     `any`/`unknown`, which `tsc` normalizes to `unknown` in the message)
    ///     that the source *satisfies*, the failure is that the parameter could
    ///     still be instantiated with a narrower type, so `tsc` reports `TS5075`
    ///     ("`'{src}'` is assignable to the constraint of type `'{T}'`, but
    ///     `'{T}'` could be instantiated with a different subtype of constraint
    ///     `'{constraint}'`.").
    ///   * Otherwise — the parameter has no `extends` clause at all, or the
    ///     source does not satisfy the constraint — the parameter could be
    ///     instantiated with something entirely unrelated, so `tsc` reports
    ///     `TS5082` ("`'{T}'` could be instantiated with an arbitrary type which
    ///     could be unrelated to `'{src}'`.").
    pub(in crate::error_reporter) fn unrelated_type_parameter_target_related_info(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_display: &str,
        target_display: &str,
        start: u32,
        length: u32,
        note_depth: u32,
    ) -> Option<DiagnosticRelatedInformation> {
        if !self.target_is_bare_type_parameter(target) {
            return None;
        }
        let file = self.ctx.file_name.clone();
        // `note_depth` is the related-info chain depth the caller wants this note
        // rendered at — directly beneath the failing `Type '{src}' is not
        // assignable to type '{T}'.` line. `tsc` appends the note at *every*
        // nesting level a bare-type-parameter target fails at, not only the
        // top-level mismatch, so the caller threads the failing line's child
        // depth through here.
        let note_depth = u8::try_from(note_depth).unwrap_or(u8::MAX);
        let elaboration = |code, message_text| DiagnosticRelatedInformation {
            category: DiagnosticCategory::Message,
            code,
            file: file.clone(),
            start,
            length,
            message_text,
            depth: note_depth,
            kind: RelatedInformationKind::ChainLink,
        };
        // An explicit `extends any`/`extends unknown` constraint still counts as
        // "has a constraint" for this branch — `tsc` normalizes it to `unknown`
        // in the rendered message (both `T extends any` and `T extends unknown`
        // show `constraint 'unknown'`), it just never *fails* the assignability
        // check below since everything is related to `unknown`. Only a bare
        // type parameter with no `extends` clause at all (`constraint: None`)
        // takes the unconstrained fallback.
        let constraint =
            crate::query_boundaries::diagnostics::type_parameter_constraint(self.ctx.types, target)
                .map(|c| if c == TypeId::ANY { TypeId::UNKNOWN } else { c });
        if let Some(constraint) = constraint
            && self
                .type_parameter_constraint_elaboration_relation_outcome(source, constraint)
                .related
        {
            let constraint_display = self.format_type_diagnostic(constraint);
            return Some(elaboration(
                diagnostic_codes::IS_ASSIGNABLE_TO_THE_CONSTRAINT_OF_TYPE_BUT_COULD_BE_INSTANTIATED_WITH_A_DIFFERE,
                format_message(
                    diagnostic_messages::IS_ASSIGNABLE_TO_THE_CONSTRAINT_OF_TYPE_BUT_COULD_BE_INSTANTIATED_WITH_A_DIFFERE,
                    &[source_display, target_display, &constraint_display],
                ),
            ));
        }
        Some(elaboration(
            diagnostic_codes::COULD_BE_INSTANTIATED_WITH_AN_ARBITRARY_TYPE_WHICH_COULD_BE_UNRELATED_TO,
            format_message(
                diagnostic_messages::COULD_BE_INSTANTIATED_WITH_AN_ARBITRARY_TYPE_WHICH_COULD_BE_UNRELATED_TO,
                &[target_display, source_display],
            ),
        ))
    }
}
