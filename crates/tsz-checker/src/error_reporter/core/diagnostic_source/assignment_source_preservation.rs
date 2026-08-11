//! Source-surface preservation helpers for assignment diagnostics.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Structural display for an assignment/return **source** whose declared
    /// annotation is a non-generic inline call/construct signature (`() => 1`,
    /// `(x: 1) => void`, `new () => 1`).
    ///
    /// Such a signature is rendered through tsc's `typeToString`, which
    /// canonicalizes the author's whitespace (`()=>1` -> `() => 1`) — so the
    /// raw-source-text fallback (which echoes the written spelling verbatim,
    /// leaking `()=>1`) is wrong here. But `normalize_assignability_display_type`
    /// widens a `TypeData::Function` return by default (the fresh
    /// function-expression case, where an inferred `() => 1` widens to
    /// `() => number`), while never touching a `TypeData::Callable`
    /// construct-signature return — so routing a *declared* function source
    /// through the canonical formatter alone would widen its written return
    /// literal, diverging from the constructor path. Because a declared
    /// signature is non-fresh, activate [`PreserveSignatureReturnLiteralsScope`]
    /// so the written return literal is kept, giving both canonical spacing and
    /// literal preservation across the call- and construct-signature forms.
    ///
    /// Generic callables (`<S>() => S[]`, `new <T>(x: T) => T`) are excluded by
    /// [`Self::annotation_is_inline_signature_type`] and keep the established
    /// `declared_identifier_source_display` handling, which owns tsc's
    /// alias-name / `?:`-surface rules for those. Returns `None` for any other
    /// source, leaving the established display path untouched.
    ///
    /// [`PreserveSignatureReturnLiteralsScope`]: crate::error_reporter::core::type_display::PreserveSignatureReturnLiteralsScope
    pub(in crate::error_reporter) fn inline_signature_annotation_source_display(
        &mut self,
        anchor_idx: NodeIndex,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let expr_idx = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))?;
        let is_inline_signature = self
            .declared_type_annotation_node_for_expression(expr_idx)
            .is_some_and(|(arena, annotation_idx)| {
                Self::annotation_is_inline_signature_type(arena, annotation_idx)
            });
        if !is_inline_signature {
            return None;
        }
        let _preserve_signature_returns =
            crate::error_reporter::core::type_display::PreserveSignatureReturnLiteralsScope::enter(
            );
        Some(self.format_assignability_type_for_message(source, target))
    }

    pub(in crate::error_reporter) fn declared_identifier_candidate_preserves_source_surface(
        &self,
        existing: &str,
        candidate: &str,
    ) -> bool {
        if existing == candidate {
            return true;
        }
        if existing.contains("| undefined") && !candidate.contains("| undefined") {
            return false;
        }
        if existing.contains("?:")
            && candidate.contains("?:")
            && existing.contains("| undefined") != candidate.contains("| undefined")
        {
            return false;
        }
        if Self::display_contains_mapped_clause(existing)
            && !Self::display_contains_mapped_clause(candidate)
        {
            return false;
        }
        true
    }

    pub(in crate::error_reporter) fn display_contains_mapped_clause(display: &str) -> bool {
        display
            .match_indices('[')
            .any(|(start, _)| Self::display_slice_starts_mapped_clause(&display[start..]))
    }

    fn display_slice_starts_mapped_clause(display: &str) -> bool {
        let Some(rest) = display.strip_prefix('[') else {
            return false;
        };
        let Some((name, after_name)) = rest.split_once(' ') else {
            return false;
        };
        let mut chars = name.chars();
        if !chars
            .next()
            .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphabetic())
            || !chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
        {
            return false;
        }
        after_name.starts_with("in ")
    }

    pub(in crate::error_reporter) fn direct_type_query_primitive_source_display(
        &mut self,
        expr_idx: NodeIndex,
        display_type: TypeId,
    ) -> Option<String> {
        let annotation_text = self.declared_type_annotation_text_for_expression(expr_idx)?;
        if !annotation_text.trim_start().starts_with("typeof ") {
            return None;
        }

        let evaluated = if let Some(symbol_ref) =
            crate::query_boundaries::common::type_query_symbol(self.ctx.types, display_type)
        {
            let sym_id =
                crate::query_boundaries::definition_identity::symbol_ref_to_symbol_id(symbol_ref);
            let value_decl = self
                .ctx
                .binder
                .get_symbol(sym_id)
                .map(|symbol| symbol.value_declaration)
                .unwrap_or(NodeIndex::NONE);
            self.type_of_value_declaration_for_symbol(sym_id, value_decl)
        } else {
            self.evaluate_type_for_assignability(display_type)
        };
        let widened = self.widen_type_for_display(evaluated);
        if !crate::query_boundaries::common::is_primitive_type(self.ctx.types, widened)
            || crate::query_boundaries::common::is_unique_symbol_type(self.ctx.types, widened)
        {
            return None;
        }

        Some(self.format_type_for_assignability_message(widened))
    }
}
