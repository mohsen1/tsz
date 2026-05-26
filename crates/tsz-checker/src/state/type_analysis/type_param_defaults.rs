//! Type-parameter default validation helpers.
//!
//! Extracted from `core.rs` to reduce the oversized type-analysis module while
//! routing default/constraint diagnostic relation checks through the shared
//! boundary.

use crate::query_boundaries::checkers::generic as generic_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn validate_type_parameter_defaults_against_constraints(
        &mut self,
        param_indices: &[NodeIndex],
        params: &[tsz_solver::TypeParamInfo],
    ) {
        for (&param_idx, param) in param_indices.iter().zip(params.iter()) {
            let Some(default_type) = param.default else {
                continue;
            };
            let Some(constraint_type) = param.constraint else {
                continue;
            };
            let constraint_has_type_params =
                generic_query::contains_type_parameters(self.ctx.types, constraint_type);
            let default_has_type_params =
                generic_query::contains_type_parameters(self.ctx.types, default_type);
            if constraint_has_type_params || default_has_type_params {
                let is_other_bare_type_parameter = |type_id| {
                    generic_query::is_bare_type_parameter(self.ctx.types, type_id)
                        && generic_query::type_parameter_name(self.ctx.types, type_id)
                            .is_some_and(|name| name != param.name)
                };
                if !is_other_bare_type_parameter(default_type)
                    && !is_other_bare_type_parameter(constraint_type)
                {
                    continue;
                }
            }
            if self
                .empty_type_literal_satisfies_optional_mapped_constraint(param_idx, constraint_type)
            {
                continue;
            }
            // A default that is syntactically one branch of its constraint union
            // satisfies the constraint by construction. This keeps default
            // validation from depending on an early semantic copy of the same
            // branch that may not have all lazy aliases stabilized yet.
            if self.type_parameter_default_syntactically_satisfies_constraint(param_idx) {
                continue;
            }
            let mut default_satisfies =
                self.diagnostic_relation_boolean_guard(default_type, constraint_type);
            if !default_satisfies {
                self.ensure_refs_resolved(default_type);
                self.ensure_refs_resolved(constraint_type);
                let evaluated_default = self.evaluate_type_for_assignability(default_type);
                let evaluated_constraint = self.evaluate_type_for_assignability(constraint_type);
                if evaluated_default != default_type
                    && !matches!(
                        evaluated_default,
                        TypeId::UNKNOWN | TypeId::ERROR | TypeId::NEVER
                    )
                {
                    default_satisfies = self
                        .diagnostic_relation_boolean_guard(evaluated_default, evaluated_constraint)
                        || self.satisfies_array_like_constraint(
                            evaluated_default,
                            evaluated_constraint,
                        )
                        || self.conditional_result_branches_satisfy_constraint(
                            evaluated_default,
                            evaluated_constraint,
                        );
                }
            }
            if !default_satisfies {
                let Some(node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                    continue;
                };
                if let Some(instantiated_default) =
                    self.instantiate_type_ref_argument_from_syntax(default_type, data.default)
                {
                    let evaluated_default =
                        self.evaluate_type_for_assignability(instantiated_default);
                    default_satisfies = self
                        .diagnostic_relation_boolean_guard(evaluated_default, constraint_type)
                        || self.satisfies_array_like_constraint(evaluated_default, constraint_type)
                        || self.conditional_result_branches_satisfy_constraint(
                            evaluated_default,
                            constraint_type,
                        );
                }
                if default_satisfies {
                    continue;
                }
                let type_str = self.format_type(default_type);
                let constraint_str = self.format_type(constraint_type);
                self.error_at_node_msg(
                    data.default,
                    crate::diagnostics::diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
                    &[&type_str, &constraint_str],
                );
            }
        }
    }
}
