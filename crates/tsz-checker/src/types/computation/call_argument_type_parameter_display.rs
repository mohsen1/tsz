//! `TS2345` rendering for call arguments whose target carries a *free*
//! (caller-scope) type parameter — the written `T`/`Elem` name must survive
//! in the message rather than being resolved away, so this bypasses the
//! general assignability gateway
//! (`assignability::assignability_diagnostics::check_argument_assignable_or_report`)
//! and builds the display and elaboration directly. Split out of
//! `call_result.rs` to stay under the 2000-line arch-size ratchet.

use crate::query_boundaries::checkers::call as call_checker;
use crate::query_boundaries::common;
use crate::query_boundaries::diagnostics;
use crate::state::CheckerState;
use tsz_common::diagnostics::format_message;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn error_argument_not_assignable_preserving_param_display(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        arg_idx: NodeIndex,
    ) {
        if self.should_suppress_argument_not_assignable_diagnostic(arg_type, param_type) {
            return;
        }
        if self.should_suppress_self_referential_mapped_constraint_arg_mismatch(
            arg_type, param_type, arg_idx,
        ) {
            return;
        }

        // Preserving the parameter display governs only the fallback TS2345
        // rendering — a missing-property failure still promotes to TS2741/2739/
        // 2740, even when the target merely contains a free type parameter (a
        // class merged with a generic-base interface leaks the base's `T`; #17145).
        let analysis = self.analyze_assignability_failure(arg_type, param_type);
        if self.try_promote_missing_property_argument(&analysis, arg_type, param_type, arg_idx) {
            return;
        }

        let display_arg_type =
            diagnostics::widen_argument_type_for_display(self.ctx.types, arg_type);
        // Widen a fresh boolean-literal array element to `boolean` structurally
        // (`true[]`/`false[]` -> `boolean[]`) instead of patching the rendered text.
        let display_arg_type =
            diagnostics::boolean_literal_array_display_type(self.ctx.types, display_arg_type)
                .unwrap_or(display_arg_type);
        let mut actual_display = self.format_type_diagnostic(display_arg_type);
        let mut target_display = self
            .constrained_variadic_tuple_parameter_display(param_type, arg_type)
            .or_else(|| {
                self.underfilled_generic_variadic_tuple_parameter_display(param_type, arg_type)
            })
            .or_else(|| {
                self.finite_mapped_parameter_display_type(param_type)
                    .map(|display_type| self.format_type_for_assignability_message(display_type))
            })
            .or_else(|| self.noinfer_call_parameter_mismatch_display(param_type, arg_type))
            .unwrap_or_else(|| self.format_type_diagnostic(param_type));
        target_display = Self::normalize_array_generic_to_shorthand(&target_display);
        if let Some((generic_actual_display, generic_target_display)) =
            self.generic_direct_primitive_mismatch_display(arg_type, param_type, arg_idx)
        {
            actual_display = generic_actual_display;
            target_display = generic_target_display;
        }
        let (code, msg_template) =
            self.argument_not_assignable_code_and_template(arg_type, param_type);
        let message = format_message(msg_template, &[&actual_display, &target_display]);
        // This path exists precisely because `param_type` carries a free type
        // parameter (`preserve_type_parameter_expected_display`), so it owes
        // the same TS5075/TS5082 bare-type-parameter-target elaboration the
        // direct-assignment TS2322 surface attaches
        // (`unrelated_type_parameter_target_related_info` already gates on
        // the target actually being a bare type parameter, not merely
        // containing one, and is a no-op otherwise).
        let Some((start, end)) = self.get_node_span(arg_idx) else {
            return;
        };
        let raw_length = end.saturating_sub(start);
        let (start, length) = self.normalized_anchor_span(arg_idx, start, raw_length);
        let related = self
            .unrelated_type_parameter_target_related_info(
                arg_type,
                param_type,
                &actual_display,
                &target_display,
                start,
                length,
                0,
            )
            .into_iter()
            .collect();
        self.error_at_node_with_related(arg_idx, &message, code, related);
    }

    fn finite_mapped_parameter_display_type(&mut self, param_type: TypeId) -> Option<TypeId> {
        let mapped_id = common::mapped_type_id(self.ctx.types, param_type)?;
        let mapped = self.ctx.types.mapped_type(mapped_id);
        let names = crate::query_boundaries::state::checking::collect_finite_mapped_property_names(
            self.ctx.types,
            mapped_id,
        )?;
        let mut names: Vec<_> = names.into_iter().collect();
        names.sort_by(|a, b| {
            self.ctx
                .types
                .resolve_atom_ref(*a)
                .cmp(&self.ctx.types.resolve_atom_ref(*b))
        });

        let mut properties = Vec::with_capacity(names.len());
        for name in names {
            let property_name = self.ctx.types.resolve_atom_ref(name).to_string();
            let type_id =
                crate::query_boundaries::state::checking::get_finite_mapped_property_display_type(
                    self.ctx.types,
                    mapped_id,
                    &property_name,
                )?;
            properties.push((name, type_id));
        }

        Some(call_checker::call_result_finite_mapped_display_object(
            self.ctx.types,
            properties,
            mapped.optional_modifier == Some(tsz_solver::MappedModifier::Add),
            mapped.readonly_modifier == Some(tsz_solver::MappedModifier::Add),
        ))
    }
}
