//! Context-sensitive assignability display rewrites.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn contextual_callable_application_target_display(
        &mut self,
        target: TypeId,
        source: TypeId,
        target_display: &str,
    ) -> Option<String> {
        // Reverse-mapped contextual targets can re-expand the pair display back
        // to `Selector<S, T["editable"]>` even after assignability has an
        // evaluated `Selector<any, {}>` application. Repaint only those indexed
        // access displays, preserving ordinary explicit `Selector<any, ...>`
        // annotations.
        if !(target_display.contains('[') && target_display.contains(']')) {
            return None;
        }

        let evaluated_target = self.evaluate_type_for_assignability(target);
        let db = self.ctx.types;
        let display_target =
            if crate::query_boundaries::common::type_application(db, target).is_some() {
                target
            } else {
                self.ctx.types.get_display_alias(target)?
            };
        let app_target =
            if crate::query_boundaries::common::type_application(db, evaluated_target).is_some() {
                evaluated_target
            } else {
                display_target
            };
        let app = crate::query_boundaries::common::type_application(db, app_target)?;
        let target_shape =
            crate::query_boundaries::common::function_shape_for_type(db, display_target)
                .or_else(|| crate::query_boundaries::common::function_shape_for_type(db, target))
                .or_else(|| {
                    crate::query_boundaries::common::function_shape_for_type(db, evaluated_target)
                })?;
        let source_shape = crate::query_boundaries::common::function_shape_for_type(db, source)?;
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
                        || crate::query_boundaries::common::contains_type_by_id(
                            db,
                            target_param.type_id,
                            arg,
                        ))
                    .then(|| {
                        let source_param = crate::query_boundaries::common::widen_type_for_display(
                            db,
                            source_param.type_id,
                        );
                        if source_param == TypeId::ANY {
                            TypeId::UNKNOWN
                        } else {
                            source_param
                        }
                    })
                })
                .or_else(|| {
                    (target_shape.return_type == arg
                        || crate::query_boundaries::common::contains_type_by_id(
                            db,
                            target_shape.return_type,
                            arg,
                        ))
                    .then(|| {
                        crate::query_boundaries::common::widen_type_for_display(
                            db,
                            source_shape.return_type,
                        )
                    })
                })
                .or_else(|| {
                    crate::query_boundaries::common::contains_type_parameters(db, arg)
                        .then_some(TypeId::UNKNOWN)
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
