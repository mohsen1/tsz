//! Source-surface preservation helpers for assignment diagnostics.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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
        if (existing.contains("[K in ") || existing.contains("[P in "))
            && !(candidate.contains("[K in ") || candidate.contains("[P in "))
        {
            return false;
        }
        true
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
            let sym_id = tsz_binder::SymbolId(symbol_ref.0);
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
