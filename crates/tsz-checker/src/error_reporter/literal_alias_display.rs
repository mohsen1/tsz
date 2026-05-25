//! Literal-only generic alias display helpers.

use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn declared_identifier_has_literal_only_alias_source(
        &mut self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(sym_id) = self.resolve_identifier_symbol(expr_idx) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if !symbol.has_any_flags(tsz_binder::symbol_flags::VARIABLE) {
            return false;
        }
        let declared_type = self.get_type_of_symbol(sym_id);
        self.evaluated_literal_alias_source_display(declared_type)
            .is_some()
    }

    pub(in crate::error_reporter) fn evaluated_literal_alias_source_display(
        &mut self,
        declared_type: TypeId,
    ) -> Option<String> {
        let (_, args) = diagnostic_query::application_info(self.ctx.types, declared_type)?;
        let has_literal_arg = args
            .iter()
            .copied()
            .any(|arg| self.contains_literal_display_candidate(arg));
        if !has_literal_arg {
            return None;
        }

        let evaluated = self.evaluate_type_for_assignability(declared_type);
        if evaluated == declared_type || matches!(evaluated, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }

        let is_literal =
            |state: &Self, ty| diagnostic_query::literal_value(state.ctx.types, ty).is_some();
        let literal_only = if is_literal(self, evaluated) {
            true
        } else if let Some(members) = diagnostic_query::union_members(self.ctx.types, evaluated) {
            !members.is_empty() && members.into_iter().all(|member| is_literal(self, member))
        } else {
            false
        };

        literal_only.then(|| self.format_type_for_assignability_message(evaluated))
    }

    fn contains_literal_display_candidate(&self, ty: TypeId) -> bool {
        if diagnostic_query::literal_value(self.ctx.types, ty).is_some() {
            return true;
        }
        if let Some(members) = diagnostic_query::union_members(self.ctx.types, ty) {
            return members
                .into_iter()
                .any(|member| self.contains_literal_display_candidate(member));
        }
        false
    }
}
