use crate::state::CheckerState;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn format_type_diagnostic_for_assignability_display_skipping_type_alias(
        &mut self,
        type_id: TypeId,
        def_id: tsz_solver::def::DefId,
    ) -> String {
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties()
            .with_expand_scalar_mapped_alias_applications()
            .with_preserve_optional_parameter_surface_syntax(true)
            .with_skip_type_alias_def_id(def_id)
            .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks)
            .with_exact_optional_property_types(
                self.ctx.compiler_options.exact_optional_property_types,
            );
        formatter.format(type_id).into_owned()
    }

    pub(in crate::error_reporter) fn type_alias_definition_body_is_type_query(
        &self,
        def: &tsz_solver::def::DefinitionInfo,
    ) -> bool {
        let Some(sym_id) = def.symbol_id.map(tsz_binder::SymbolId) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        symbol.declarations.iter().any(|&decl_idx| {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            let Some(alias) = self.ctx.arena.get_type_alias(node) else {
                return false;
            };
            self.ctx
                .arena
                .get(alias.type_node)
                .is_some_and(|body| body.kind == syntax_kind_ext::TYPE_QUERY)
        })
    }
}
