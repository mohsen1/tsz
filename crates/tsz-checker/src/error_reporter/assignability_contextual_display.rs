//! Context-sensitive assignability display rewrites.

use crate::query_boundaries::{common as query_common, diagnostics as query_diagnostics};
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn contextual_callable_application_target_display(
        &mut self,
        target: TypeId,
        source: TypeId,
        _target_display: &str,
    ) -> Option<String> {
        // Reverse-mapped contextual targets can re-expand the pair display back
        // to `Selector<S, T["editable"]>` even after assignability has an
        // evaluated `Selector<any, {}>` application. Repaint only those indexed
        // access displays, preserving ordinary explicit `Selector<any, ...>`
        // annotations.
        let evaluated_target = self.evaluate_type_for_assignability(target);
        let db = self.ctx.types;
        let display_target = if query_common::type_application(db, target).is_some() {
            target
        } else {
            self.ctx.types.get_display_alias(target)?
        };
        let app_target = if query_common::type_application(db, evaluated_target).is_some() {
            evaluated_target
        } else {
            display_target
        };
        let app = query_common::type_application(db, app_target)?;
        let original_app = query_common::type_application(db, display_target)
            .or_else(|| query_common::type_application(db, target));
        let target_application_evaluated = evaluated_target != target
            && original_app
                .as_ref()
                .is_some_and(|original_app| original_app.base == app.base);
        let target_shape = query_common::function_shape_for_type(db, display_target)
            .or_else(|| query_common::function_shape_for_type(db, target))
            .or_else(|| query_common::function_shape_for_type(db, evaluated_target))?;
        let application_args_contain_index_access = app.args.iter().any(|&arg| {
            query_diagnostics::is_index_access_type(db, arg)
                || query_diagnostics::contains_index_access_type(db, arg)
        });
        let target_shape_contains_index_access = target_shape
            .params
            .iter()
            .any(|param| query_diagnostics::contains_index_access_type(db, param.type_id))
            || query_diagnostics::contains_index_access_type(db, target_shape.return_type);
        if !target_application_evaluated
            && !application_args_contain_index_access
            && !target_shape_contains_index_access
        {
            return None;
        }
        let source_shape = query_common::function_shape_for_type(db, source)?;
        if source_shape.params.len() <= target_shape.params.len() {
            return None;
        }

        let mut changed = false;
        let mut display_args = Vec::with_capacity(app.args.len());
        for &arg in &app.args {
            let replacement = target_shape
                .params
                .iter()
                .zip(source_shape.params.iter())
                .find_map(|(target_param, source_param)| {
                    (target_param.type_id == arg
                        || query_common::contains_type_by_id(db, target_param.type_id, arg))
                    .then(|| {
                        let source_param =
                            query_common::widen_type_for_display(db, source_param.type_id);
                        if source_param == TypeId::ANY {
                            TypeId::UNKNOWN
                        } else {
                            source_param
                        }
                    })
                })
                .or_else(|| {
                    (target_shape.return_type == arg
                        || query_common::contains_type_by_id(db, target_shape.return_type, arg))
                    .then(|| query_common::widen_type_for_display(db, source_shape.return_type))
                })
                .or_else(|| {
                    query_common::contains_type_parameters(db, arg).then_some(TypeId::UNKNOWN)
                });

            let display_arg = replacement.unwrap_or(arg);
            changed |= display_arg != arg;
            display_args.push(display_arg);
        }

        if !changed {
            return None;
        }

        let display_app = self.ctx.types.factory().application(app.base, display_args);
        Some(self.format_type_for_assignability_message(display_app))
    }
}
