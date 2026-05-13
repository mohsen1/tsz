//! JSX SFC factory arity validation helpers.

use super::runtime;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn effective_jsx_factory_name(&self) -> String {
        let pragma_factory = self
            .current_jsx_source_text()
            .and_then(runtime::extract_jsx_pragma);
        pragma_factory.unwrap_or_else(|| self.ctx.compiler_options.jsx_factory.clone())
    }

    fn report_jsx_factory_arity_mismatch(
        &mut self,
        tag_name_idx: NodeIndex,
        required_arg_count: usize,
    ) {
        let tag_text = self.get_jsx_tag_name_text(tag_name_idx);
        let factory_name = self.effective_jsx_factory_name();
        self.error_at_node_msg(
            tag_name_idx,
            crate::diagnostics::diagnostic_codes::TAG_EXPECTS_AT_LEAST_ARGUMENTS_BUT_THE_JSX_FACTORY_PROVIDES_AT_MOST,
            &[&tag_text, &required_arg_count.to_string(), &factory_name, "2"],
        );
    }

    fn jsx_sfc_required_param_count(params: &[tsz_solver::ParamInfo]) -> usize {
        params
            .iter()
            .take_while(|param| param.is_required())
            .count()
    }

    pub(super) fn check_jsx_sfc_factory_arity(
        &mut self,
        component_type: TypeId,
        tag_name_idx: NodeIndex,
    ) -> bool {
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, component_type)
            && !shape.is_constructor
            && shape.type_params.is_empty()
        {
            let required_arg_count = Self::jsx_sfc_required_param_count(&shape.params);
            if required_arg_count > 2 {
                self.report_jsx_factory_arity_mismatch(tag_name_idx, required_arg_count);
                return true;
            }
            return false;
        }

        if let Some(sigs) = crate::query_boundaries::common::call_signatures_for_type(
            self.ctx.types,
            component_type,
        ) {
            let non_generic: Vec<_> = sigs
                .iter()
                .filter(|sig| sig.type_params.is_empty())
                .collect();
            if non_generic.len() == 1 {
                let required_arg_count = Self::jsx_sfc_required_param_count(&non_generic[0].params);
                if required_arg_count > 2 {
                    self.report_jsx_factory_arity_mismatch(tag_name_idx, required_arg_count);
                    return true;
                }
            }
        }

        false
    }
}
