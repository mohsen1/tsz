//! Annotation-gated structural display for TS2322-family assignability source
//! and target types.
//!
//! An inline / anonymous type annotation (`{ a: number }`, `[number, string]`,
//! `(a: number) => void`, `string | number`) carries no `aliasSymbol`, so tsc
//! renders its structural shape rather than repainting it with a
//! coincidentally-shaped non-generic type-alias name reached through the
//! reverse type-to-def lookup. These helpers gate that structural render on the
//! written annotation node.
//!
//! Relocated from `assignment_formatting.rs` to keep that file under the LOC
//! ceiling, and extended for #17119: the inline tuple/function/constructor
//! family (`inline_structural_type_annotation_{source,target}_display`) and the
//! shared `annotation_gated_structural_target_display` (mirroring the source
//! side) join the existing anonymous-composite and longhand-union helpers.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Structural display for an assignment **target** whose type was written as
    /// an inline / anonymous composite annotation (`{ a: number }`,
    /// `{ a: number } | { b: string }`, `{ a: number } & { b: string }`).
    ///
    /// Such an annotation carries no `aliasSymbol`, so tsc renders the structural
    /// shape rather than a coincidentally-shaped non-generic type-alias name
    /// reached through the reverse type-to-def lookup. Returns `None` when the
    /// target was not written as an anonymous composite (a named reference, a
    /// mixed union/intersection, or a non-composite type), leaving the
    /// established display path untouched. Shared by every renderer that prints a
    /// top-level assignability target so they cannot drift on alias display.
    pub(in crate::error_reporter) fn anonymous_composite_annotation_target_display(
        &mut self,
        anchor_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        self.annotation_gated_structural_target_display(
            anchor_idx,
            target,
            Self::annotation_is_anonymous_structural_composite,
        )
    }

    /// Structural display for an assignment **target** whose declared type
    /// annotation node satisfies `annotation_matches` — an inline shape that
    /// carries no `aliasSymbol` and so should render by its structure rather than
    /// a coincidentally-shaped alias reached through the reverse type-to-def
    /// lookup. The target mirror of
    /// [`Self::annotation_gated_structural_source_display`].
    pub(super) fn annotation_gated_structural_target_display(
        &mut self,
        anchor_idx: NodeIndex,
        target: TypeId,
        annotation_matches: fn(&tsz_parser::NodeArena, NodeIndex) -> bool,
    ) -> Option<String> {
        let target_expr = self
            .assignment_target_expression(anchor_idx)
            .unwrap_or(anchor_idx);
        let matches = self
            .declared_type_annotation_node_for_expression(target_expr)
            .is_some_and(|(arena, annotation_idx)| annotation_matches(arena, annotation_idx));
        if !matches {
            return None;
        }
        // A non-generic alias reference reaches the formatter as a `Lazy(DefId)`
        // whose name path bypasses the composite-structural gate; resolve it to
        // the structural body first so the inline shape renders even when the
        // checker canonicalized the annotation type.
        let resolved = self.resolve_lazy_type(target);
        Some(self.format_type_for_assignability_message_anonymous_composite_structural(resolved))
    }

    /// Structural display for an assignment **target** written as an inline
    /// tuple / function / constructor type annotation. The target mirror of
    /// [`Self::inline_structural_type_annotation_source_display`] (#17119).
    pub(in crate::error_reporter) fn inline_structural_type_annotation_target_display(
        &mut self,
        anchor_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        self.annotation_gated_structural_target_display(
            anchor_idx,
            target,
            Self::annotation_is_canonicalized_structural_type,
        )
    }

    /// Structural display for an assignment **target** whose declared type was
    /// written as a longhand primitive-keyword union (`string | number`,
    /// `string | number | symbol`). The target mirror of
    /// [`Self::longhand_primitive_union_source_display`]; the two differ only in
    /// which side of the assignment supplies the annotation node.
    ///
    /// Returns `None` for a written-through alias reference (`: Zed`), which is
    /// a `TYPE_REFERENCE` rather than a longhand union, so an annotation that
    /// really did name an alias keeps that name.
    pub(in crate::error_reporter) fn longhand_primitive_union_target_display(
        &mut self,
        anchor_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        self.annotation_gated_structural_target_display(
            anchor_idx,
            target,
            Self::annotation_is_longhand_primitive_keyword_union,
        )
    }

    /// Structural display for an assignment **source** whose declared type
    /// annotation node satisfies `annotation_matches` — an inline shape that
    /// carries no `aliasSymbol` and so should render by its structure rather than
    /// a coincidentally-shaped alias reached through the reverse type-to-def
    /// lookup. Shared by the anonymous-composite and longhand-primitive-union
    /// source paths, which differ only in that annotation predicate.
    pub(super) fn annotation_gated_structural_source_display(
        &mut self,
        anchor_idx: NodeIndex,
        source: TypeId,
        annotation_matches: fn(&tsz_parser::NodeArena, NodeIndex) -> bool,
    ) -> Option<String> {
        let expr_idx = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))?;
        let matches = self
            .declared_type_annotation_node_for_expression(expr_idx)
            .is_some_and(|(arena, annotation_idx)| annotation_matches(arena, annotation_idx));
        if !matches {
            return None;
        }
        let resolved = self.resolve_lazy_type(source);
        Some(self.format_type_for_assignability_message_anonymous_composite_structural(resolved))
    }

    /// Structural display for an assignment **source** written as an inline /
    /// anonymous composite annotation. The source mirror of
    /// [`Self::anonymous_composite_annotation_target_display`].
    pub(in crate::error_reporter) fn anonymous_composite_annotation_source_display(
        &mut self,
        anchor_idx: NodeIndex,
        source: TypeId,
    ) -> Option<String> {
        self.annotation_gated_structural_source_display(
            anchor_idx,
            source,
            Self::annotation_is_anonymous_structural_composite,
        )
    }

    /// Structural display for an assignment **source** whose declared type was
    /// written as a longhand primitive-keyword union (`string | number | symbol`,
    /// `string | number`). Such an inline union carries no `aliasSymbol`, so tsc
    /// renders it by its members rather than repainting it with a
    /// coincidentally-shaped non-generic alias (`PropertyKey`, a user `type`)
    /// reached through the reverse type-to-def lookup (#16610). Returns `None`
    /// for any other source shape — including a written-through alias reference
    /// (`: Zed`), which is a `TYPE_REFERENCE`, not a longhand union — leaving the
    /// established display path untouched.
    pub(in crate::error_reporter) fn longhand_primitive_union_source_display(
        &mut self,
        anchor_idx: NodeIndex,
        source: TypeId,
    ) -> Option<String> {
        self.annotation_gated_structural_source_display(
            anchor_idx,
            source,
            Self::annotation_is_longhand_primitive_keyword_union,
        )
    }

    /// Structural display for an assignment **source** written as an inline
    /// tuple / function / constructor type annotation (`[number, string]`,
    /// `(a: number) => void`, `new () => T`). Such an inline structural type
    /// carries no `aliasSymbol`, so tsc renders its expanded structural form
    /// rather than a coincidentally-shaped non-generic alias reached through the
    /// reverse type-to-def lookup (#17119). Returns `None` for any other source
    /// shape — including a written-through alias reference (`: Fn`), which is a
    /// `TYPE_REFERENCE`, not an inline structural type — leaving the established
    /// display path untouched. The source mirror of
    /// [`Self::inline_structural_type_annotation_target_display`].
    pub(in crate::error_reporter) fn inline_structural_type_annotation_source_display(
        &mut self,
        anchor_idx: NodeIndex,
        source: TypeId,
    ) -> Option<String> {
        self.annotation_gated_structural_source_display(
            anchor_idx,
            source,
            Self::annotation_is_canonicalized_structural_type,
        )
    }
}
