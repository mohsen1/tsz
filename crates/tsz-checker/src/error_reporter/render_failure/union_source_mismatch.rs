//! Union-**source** mismatch rendering: the failing constituent's line and
//! drill beneath the union-to-target pair.
//!
//! Extracted from `nested_application_property_mismatch.rs` for the checker
//! arch-size ceiling. Owns [`CheckerState::render_union_source_mismatch`]
//! (the `UnionSourceMismatch` reason renderer: plain-leaf members at depth 0,
//! self-heading members through `render_parent_with_child_relation`, and
//! header-led members through the member-header + drill shape) plus its
//! member-display helpers.

use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages, format_message};
use crate::error_reporter::render_failure::RenderContext;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Display a single failing union constituent honoring the source
    /// annotation's provenance: an inline `{ ... }` constituent (an anonymous
    /// composite the user wrote directly) shows its structural shape, while a
    /// named reference keeps its name. Falls back to the ordinary diagnostic
    /// display when the source expression carries no anonymous-composite
    /// annotation. See issue #16513.
    fn union_source_member_display(
        &mut self,
        anchor_idx: tsz_parser::parser::NodeIndex,
        member: TypeId,
    ) -> String {
        self.anonymous_composite_annotation_source_display(anchor_idx, member)
            .unwrap_or_else(|| self.format_type_diagnostic(member))
    }

    /// Render a depth-0 plain-leaf union-source mismatch (`Type 'A | B' is not
    /// assignable to type 'T'.` -> `Type '<failing member>' is not assignable to
    /// type 'T'.`), displaying the failing constituent with the source
    /// annotation's provenance so an inline `{ ... }` constituent shows its
    /// structural shape rather than a coincidentally same-shaped alias reached
    /// through the reverse type-to-def lookup. See issue #16513.
    ///
    /// Only invoked at chain depth 0 (the primary assignment diagnostic). The
    /// `member_type` is the solver's selected failing constituent; the solver
    /// already walks the union in source order (#16523 reorders enum slots by
    /// declaration), so this path corrects only the member *display*, not the
    /// selection.
    ///
    /// The outer-line + generalize + `push_elaboration_in_span` scaffolding
    /// mirrors the depth-0 plain-leaf branch of
    /// [`Self::render_parent_with_child_relation`] — the two must stay in
    /// lockstep. The sole divergence is `union_source_member_display` (annotation
    /// provenance) in place of that renderer's plain `format_type_diagnostic`.
    fn render_union_source_plain_leaf_member(
        &mut self,
        ctx: &RenderContext,
        target_type: TypeId,
        member_type: TypeId,
        nested_reason: &tsz_solver::SubtypeFailureReason,
    ) -> Diagnostic {
        let mut diag = self.render_type_mismatch(ctx);
        // The member line renders the pair the solver actually related: for a
        // sole-real-member nullable target the member's failure was explained
        // against the reduced member (tsc `getBestMatchingType` re-relates
        // there), so the leaf shows `Type 'boolean' is not assignable to type
        // 'string'.` under the full-union head. Every other producer explains
        // the member against the whole target, so the leaf target equals
        // `target_type` and the display is unchanged.
        let (_, leaf_target) =
            Self::nested_failure_display_types(nested_reason, member_type, target_type);
        // The leaf generalizes the same way as the outer line (tsc runs
        // `reportRelationError` on every relation line): an all-unit member
        // widens to its base against a non-singleton target.
        let display_member =
            self.generalize_nested_relation_source_for_display(member_type, leaf_target);
        let member_str = self.union_source_member_display(ctx.idx, display_member);
        let target_str = self.format_type_diagnostic(leaf_target);
        diag.push_elaboration_in_span(
            ctx.start,
            ctx.length,
            format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&member_str, &target_str],
            ),
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            0,
        );
        diag
    }

    /// Render a union-source mismatch: a union type that is not assignable to
    /// the target because one of its members fails.
    ///
    /// tsc keeps the root mismatch visible by elaborating the first failing
    /// member directly beneath the union-to-target line:
    ///
    /// ```text
    /// Type 'A | B' is not assignable to type 'T'.
    ///   Type 'B' is not assignable to type 'T'.
    /// ```
    ///
    /// When the member fails for a *structural* reason (a tuple element type
    /// mismatch or an object property-type mismatch) tsc emits the member-type
    /// header explicitly and then drills into the structural detail:
    ///
    /// ```text
    /// Type 'A | B' is not assignable to type 'T'.
    ///   Type 'B' is not assignable to type 'T'.
    ///     Type at position 0 in source is not compatible with type at position 0 in target.
    ///       Type 'number' is not assignable to type 'string'.
    /// ```
    ///
    /// The structural renderers omit that header at depth >= 1 (they lead with
    /// `Type at position N …` / `Types of property 'p' …`), so this path emits
    /// it before recursing. Self-heading members (leaf relations, missing
    /// property summaries) carry the member line themselves and are delegated
    /// to [`Self::render_parent_with_child_relation`].
    pub(super) fn render_union_source_mismatch(
        &mut self,
        ctx: &RenderContext,
        source_type: TypeId,
        target_type: TypeId,
        member_type: TypeId,
        nested_reason: &tsz_solver::SubtypeFailureReason,
    ) -> Diagnostic {
        // A whole-constituent (plain-leaf) rejection carries no member-specific
        // structure — its nested reason is just `member <: target`. An inline
        // `{ ... }` constituent has no `aliasSymbol`, but the reverse
        // type-to-def lookup repaints it with a coincidentally same-shaped alias
        // declared elsewhere in the file (`{ m: number }` -> `U`). Render the
        // failing member honoring the source annotation's provenance instead.
        // Scoped to depth-0 plain-leaf: a structural or nested member failure
        // binds its `nested_reason` to a specific member and self-heads its own
        // line, so it keeps the established rendering. See issue #16513.
        if ctx.depth == 0 && Self::nested_reason_is_plain_type_mismatch(nested_reason) {
            return self.render_union_source_plain_leaf_member(
                ctx,
                target_type,
                member_type,
                nested_reason,
            );
        }
        if !Self::union_member_nested_needs_header(nested_reason) {
            return self.render_parent_with_child_relation(
                ctx,
                source_type,
                target_type,
                member_type,
                target_type,
                nested_reason,
            );
        }

        let depth = ctx.depth;
        // The member keeps its own alias (`C` stays `C`); the target keeps the
        // diagnostic alias the rest of the chain uses (`A`), matching tsc for
        // object/interface targets. (tsc additionally expands a *tuple* target
        // alias to its structural form here, but tsz's shared formatter follows
        // the tuple's lazy display alias; the chain shape, positions, and leaf
        // relation are otherwise identical.) Format the target once; it heads
        // both the (deep) outer union line and the member header below.
        let target_str = self.format_type_diagnostic(target_type);

        // Outer union line. At depth 0 it is the primary diagnostic, which
        // reuses `render_type_mismatch` so the full union/alias surface is
        // preserved; deeper, format the union/target pair structurally.
        let mut diag = if depth == 0 {
            self.render_type_mismatch(ctx)
        } else {
            // Nested union line: generalize an all-unit union source to its
            // base (tsc `reportRelationError` / `getBaseTypeOfLiteralTypeUnion`).
            let display_source =
                self.generalize_nested_relation_source_for_display(source_type, target_type);
            let source_str = self.format_type_diagnostic(display_source);
            let base = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            Diagnostic::error(
                ctx.file_name.clone(),
                ctx.start,
                ctx.length,
                base,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            )
        };

        if depth >= 5 {
            return diag;
        }

        // The member header sits one indent beneath the union line; the
        // structural drill sits one level beneath the header. At depth 0 the
        // union line is the (un-indented) primary, so its first child is at
        // related-depth 0.
        let header_depth = super::first_child_depth(depth);
        let drill_depth = header_depth + 1;

        let display_member =
            self.generalize_nested_relation_source_for_display(member_type, target_type);
        let member_str = self.format_type_diagnostic_for_union_member(source_type, display_member);
        // For a sole-real-member nullable union target the member's failure
        // was explained against the reduced member (tsc `getBestMatchingType`
        // re-relates there), so the member header names that member:
        // `Type '{ a: boolean; }' is not assignable to type '{ a: string; }'.`
        // beneath the full-union pair line. Multi-real-member unions strip to
        // more than one survivor and keep the whole target; non-union targets
        // never strip.
        let member_target_str = self
            .strip_nullish_for_assignability_display(target_type, member_type)
            .map_or_else(
                || target_str.clone(),
                |stripped| self.format_type_diagnostic(stripped),
            );
        diag.push_elaboration_in_span(
            ctx.start,
            ctx.length,
            format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&member_str, &member_target_str],
            ),
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            header_depth,
        );

        // A union member that is a function failing on its *return* type drills
        // straight into the return relation beneath the member header, with no
        // intermediate `Return type 'X' is not assignable to 'Y'.` frame — `tsc`
        // relates the return types directly:
        //
        // ```text
        //   Type '(x: string) => string' is not assignable to type 'Target'.
        //     Type 'string' is not assignable to type 'number'.
        // ```
        //
        // Rendering the `ReturnTypeMismatch` reason itself would emit the
        // `Return type …` frame (a non-`tsc` line at depth >= 1), so recurse into
        // the carried return relation instead.
        if let tsz_solver::SubtypeFailureReason::ReturnTypeMismatch {
            source_return,
            target_return,
            nested_reason: inner,
        } = nested_reason
        {
            // When the return relation has no structured sub-reason, drill it as
            // a plain leaf so both arms render the return types through the same
            // path (`render_failure_reason` → `render_type_mismatch`).
            let leaf;
            let return_reason = match inner.as_deref() {
                Some(inner) => inner,
                None => {
                    leaf = tsz_solver::SubtypeFailureReason::TypeMismatch {
                        source_type: *source_return,
                        target_type: *target_return,
                    };
                    &leaf
                }
            };
            let return_diag = self.render_failure_reason(
                return_reason,
                *source_return,
                *target_return,
                ctx.idx,
                drill_depth,
            );
            Self::push_nested_chain(&mut diag, return_diag, drill_depth);
            return diag;
        }

        let nested_diag = self.render_failure_reason(
            nested_reason,
            member_type,
            target_type,
            ctx.idx,
            drill_depth,
        );
        Self::push_nested_chain(&mut diag, nested_diag, drill_depth);

        diag
    }
}
