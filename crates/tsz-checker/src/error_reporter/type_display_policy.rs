//! Checker-owned type display roles for diagnostic rendering.
//!
//! This adapter keeps checker context out of the solver formatter while making
//! diagnostic display intent explicit at emission sites. Each role delegates to
//! the existing specialized helper for that surface.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(in crate::error_reporter) enum DiagnosticTypeDisplayRole {
    DefaultDiagnostic,
    WidenedDiagnostic,
    FlattenedDiagnostic,
    Assignability,
    AssignmentSource {
        target: TypeId,
        anchor_idx: NodeIndex,
    },
    AssignmentTarget {
        source: TypeId,
        anchor_idx: NodeIndex,
    },
    CallArgument {
        parameter: TypeId,
        argument_idx: NodeIndex,
    },
    CallParameter {
        argument: TypeId,
        argument_idx: NodeIndex,
    },
    WeakCallParameter {
        argument: TypeId,
        argument_idx: NodeIndex,
    },
    PropertyReceiver,
}

impl<'a> CheckerState<'a> {
    /// tsc renders the resolved form (not the alias name) for non-generic
    /// type aliases whose body is a single indexed-access type that reduces
    /// to a concrete result. The classic case is
    /// `type WeakKey = WeakKeyTypes[keyof WeakKeyTypes]` — when `WeakKeyTypes`
    /// has only `object: object` (es2022 lib without es2023.collection.d.ts),
    /// the indexed-access reduction collapses to `object`. tsc loses the outer
    /// alias on the resolved type and displays `object`, not `WeakKey`.
    ///
    /// Pre-resolve the type before passing it to the formatter; this needs
    /// the checker's full evaluator (with `TypeEnvironment`) which the solver
    /// formatter cannot reach on its own.
    pub(in crate::error_reporter) fn resolve_indexed_access_alias_for_display(
        &mut self,
        ty: TypeId,
    ) -> TypeId {
        let body = match crate::query_boundaries::diagnostics::indexed_access_alias_body(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            ty,
        ) {
            Some(b) => b,
            None => return ty,
        };
        let resolved = self.evaluate_type_with_env(body);
        if resolved == body
            || crate::query_boundaries::diagnostics::is_unresolved_for_display(
                self.ctx.types.as_type_database(),
                resolved,
            )
        {
            return ty;
        }
        resolved
    }

    pub(in crate::error_reporter) fn format_type_for_diagnostic_role(
        &mut self,
        ty: TypeId,
        role: DiagnosticTypeDisplayRole,
    ) -> String {
        // Discarded-diagnostics children never surface their messages: skip
        // the role-specific display policy and the type formatter entirely.
        // The placeholder embeds the type id so different types still format
        // to different strings (message-hash dedup keys stay distinct).
        if self.ctx.diagnostics_discarded {
            return format!("[discarded #{}]", ty.0);
        }
        // One rendered type, one work budget (issue #13040).
        let _budget_scope = crate::error_reporter::display_budget::DisplayBudgetScope::enter();
        // Only apply the indexed-access alias resolution for roles where the
        // alias would otherwise leak through unresolved (call parameters /
        // arguments / direct assignability). For declaration-emit-adjacent
        // roles, type-display roles, and property-receiver displays, keep the
        // original alias name — pre-resolving there can trigger TS2589 on
        // legitimately-deferred indexed-access aliases that the checker
        // intentionally leaves opaque.
        let ty = match role {
            DiagnosticTypeDisplayRole::CallArgument { .. }
            | DiagnosticTypeDisplayRole::CallParameter { .. }
            | DiagnosticTypeDisplayRole::WeakCallParameter { .. }
            | DiagnosticTypeDisplayRole::Assignability => {
                self.resolve_indexed_access_alias_for_display(ty)
            }
            _ => ty,
        };
        // Recursive tuple aliases (e.g. `T2<U>` for
        // `type T2<T> = [42, T2<{ x: T }>]`) cannot be expanded structurally
        // without producing an unbounded tuple display. Preserve those alias
        // applications before the role-specific tuple fallbacks evaluate the
        // application into its self-referential body. Recursive object/mapped
        // aliases still flow through the normal role-specific formatters: those
        // paths can reduce property values through mapped type substitutions
        // where tsc displays the concrete value type instead of the alias.
        if matches!(
            role,
            DiagnosticTypeDisplayRole::Assignability
                | DiagnosticTypeDisplayRole::AssignmentSource { .. }
                | DiagnosticTypeDisplayRole::AssignmentTarget { .. }
                | DiagnosticTypeDisplayRole::CallArgument { .. }
                | DiagnosticTypeDisplayRole::CallParameter { .. }
                | DiagnosticTypeDisplayRole::WeakCallParameter { .. }
        ) && self.is_recursive_tuple_alias_application_for_diagnostic(ty)
        {
            return self.format_type_diagnostic(ty);
        }
        match role {
            DiagnosticTypeDisplayRole::DefaultDiagnostic => self.format_type_diagnostic(ty),
            DiagnosticTypeDisplayRole::WidenedDiagnostic => self.format_type_diagnostic_widened(ty),
            DiagnosticTypeDisplayRole::FlattenedDiagnostic => {
                self.format_type_diagnostic_flattened(ty)
            }
            DiagnosticTypeDisplayRole::Assignability => {
                self.format_type_for_assignability_message(ty)
            }
            DiagnosticTypeDisplayRole::AssignmentSource { target, anchor_idx } => {
                self.format_assignment_source_type_for_diagnostic(ty, target, anchor_idx)
            }
            DiagnosticTypeDisplayRole::AssignmentTarget { source, anchor_idx } => {
                self.format_assignment_target_type_for_diagnostic(ty, source, anchor_idx)
            }
            DiagnosticTypeDisplayRole::CallArgument {
                parameter,
                argument_idx,
            } => self.format_call_argument_type_for_diagnostic(ty, parameter, argument_idx),
            DiagnosticTypeDisplayRole::CallParameter {
                argument,
                argument_idx,
            } => {
                let display = self.format_call_parameter_type_for_diagnostic(
                    ty,
                    argument,
                    argument_idx,
                    true,
                );
                if crate::query_boundaries::common::array_element_type(self.ctx.types, ty).is_some()
                {
                    Self::normalize_array_generic_to_shorthand(&display)
                } else {
                    display
                }
            }
            DiagnosticTypeDisplayRole::WeakCallParameter {
                argument,
                argument_idx,
            } => {
                let display = self.format_call_parameter_type_for_diagnostic(
                    ty,
                    argument,
                    argument_idx,
                    false,
                );
                if crate::query_boundaries::common::array_element_type(self.ctx.types, ty).is_some()
                {
                    Self::normalize_array_generic_to_shorthand(&display)
                } else {
                    display
                }
            }
            DiagnosticTypeDisplayRole::PropertyReceiver => {
                self.format_property_receiver_type_for_diagnostic(ty)
            }
        }
    }

    fn is_recursive_tuple_alias_application_for_diagnostic(&mut self, ty: TypeId) -> bool {
        let evaluated = self.evaluate_type_with_env(ty);
        crate::query_boundaries::recursive_alias::is_recursive_tuple_type_alias_application(
            self.ctx.types,
            &self.ctx.definition_store,
            ty,
            evaluated,
        )
    }
}
