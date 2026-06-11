//! Variadic tuple display helpers for call diagnostics.

use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_solver::{TupleElement, TypeId};

impl<'a> CheckerState<'a> {
    /// Structural display for an *effective rest slice*: a tuple whose first
    /// element is a rest element followed by a fixed tail, e.g.
    /// `[...string[], number]`. tsc derives this parameter surface through
    /// `getEffectiveRestType`/`sliceTupleType`, which always synthesizes an
    /// anonymous tuple — so the displayed parameter never borrows the name of
    /// a structurally identical user alias (`V01`), even though interning
    /// would otherwise share the alias's display.
    pub(crate) fn effective_rest_slice_parameter_display(
        &mut self,
        param_type: TypeId,
    ) -> Option<String> {
        let readonly =
            crate::query_boundaries::common::readonly_inner_type(self.ctx.types, param_type)
                .is_some();
        let unwrapped = query_common::unwrap_readonly(self.ctx.types, param_type);
        let elements = query_common::tuple_elements(self.ctx.types, unwrapped)?;
        let (first, tail) = elements.split_first()?;
        if !first.rest || tail.is_empty() || tail.iter().any(|element| element.rest) {
            return None;
        }
        Some(self.format_tuple_element_display(&elements, readonly))
    }

    /// Whether an argument mapping into this rest parameter is reported
    /// against a per-position element type or sliced remainder rather than
    /// the whole rest tuple — true when the rest parameter's type is a tuple.
    ///
    /// tsc's `getTypeAtPosition`/`getEffectiveRestType` model never relates an
    /// argument against the whole rest tuple, and the solver already computes
    /// that per-position/sliced expected type, so display reconstruction from
    /// the raw rest parameter must stand down for these shapes.
    pub(in crate::error_reporter::call_errors) fn rest_tuple_parameter_reports_per_position(
        &mut self,
        raw_param_type: TypeId,
    ) -> bool {
        let raw_unwrapped = query_common::unwrap_readonly(self.ctx.types, raw_param_type);
        if query_common::tuple_elements(self.ctx.types, raw_unwrapped).is_some() {
            return true;
        }
        // The raw type may hide the tuple behind an alias/application; only
        // then pay for an environment evaluation.
        let instantiated_probe = self.evaluate_type_with_env(raw_param_type);
        let unwrapped = query_common::unwrap_readonly(self.ctx.types, instantiated_probe);
        query_common::tuple_elements(self.ctx.types, unwrapped).is_some()
    }

    pub(crate) fn constrained_variadic_tuple_parameter_display(
        &mut self,
        param_type: TypeId,
        arg_type: TypeId,
    ) -> Option<String> {
        self.constrained_variadic_tuple_parameter_display_structured(param_type, arg_type)
    }

    fn constrained_variadic_tuple_parameter_display_structured(
        &mut self,
        param_type: TypeId,
        arg_type: TypeId,
    ) -> Option<String> {
        let mut resolved = self.evaluate_type_with_env(param_type);
        resolved = self.resolve_type_for_property_access(resolved);
        resolved = self.resolve_lazy_type(resolved);
        resolved = self.evaluate_application_type(resolved);
        resolved = query_common::unwrap_readonly(self.ctx.types, resolved);
        let elements = query_common::tuple_elements(self.ctx.types, resolved)?;
        let rest_index = elements.iter().position(|element| element.rest)?;
        let outer_tail = &elements[rest_index + 1..];
        if outer_tail.is_empty() {
            return None;
        }

        let rest_element = elements.get(rest_index)?;
        let constraint = query_common::type_param_info(self.ctx.types, rest_element.type_id)
            .and_then(|info| info.constraint)
            .unwrap_or(rest_element.type_id);
        let mut constraint = self.evaluate_type_with_env(constraint);
        constraint = self.resolve_type_for_property_access(constraint);
        constraint = self.resolve_lazy_type(constraint);
        constraint = self.evaluate_application_type(constraint);
        constraint = query_common::unwrap_readonly(self.ctx.types, constraint);
        let constraint_elements = query_common::tuple_elements(self.ctx.types, constraint)?;
        let constraint_rest_index = constraint_elements
            .iter()
            .position(|element| element.rest)?;

        let arg_tuple = query_common::tuple_elements(self.ctx.types, arg_type);
        if arg_tuple.is_none() {
            return constraint_elements
                .iter()
                .take(constraint_rest_index)
                .find(|element| !element.optional)
                .map(|element| self.format_type_for_assignability_message(element.type_id));
        }

        let arg_elements = arg_tuple?;
        let mut consumed = 0usize;
        for fixed in constraint_elements.iter().take(constraint_rest_index) {
            let Some(actual) = arg_elements.get(consumed) else {
                break;
            };
            if !self
                .call_arg_relation_outcome_with_env(actual.type_id, fixed.type_id)
                .related
            {
                break;
            }
            consumed += 1;
        }
        if consumed == 0 {
            return None;
        }

        let mut display_elements = Vec::new();
        display_elements.extend(constraint_elements[consumed..].iter().copied());
        display_elements.extend(outer_tail.iter().copied());
        Some(self.format_tuple_element_display(&display_elements, false))
    }

    pub(crate) fn underfilled_generic_variadic_tuple_parameter_display(
        &mut self,
        param_type: TypeId,
        arg_type: TypeId,
    ) -> Option<String> {
        let mut resolved = self.evaluate_type_with_env(param_type);
        resolved = self.resolve_type_for_property_access(resolved);
        resolved = self.resolve_lazy_type(resolved);
        resolved = self.evaluate_application_type(resolved);
        resolved = query_common::unwrap_readonly(self.ctx.types, resolved);
        let elements = query_common::tuple_elements(self.ctx.types, resolved)?;
        let arg_elements = query_common::tuple_elements(self.ctx.types, arg_type)?;
        let required_fixed = elements
            .iter()
            .filter(|element| !element.rest && !element.optional)
            .count();
        if arg_elements.len() >= required_fixed {
            return None;
        }
        let has_unknown_variadic = elements.iter().any(|element| {
            element.rest
                && query_common::array_element_type(self.ctx.types, element.type_id)
                    .is_some_and(|inner| inner == TypeId::UNKNOWN)
        });
        let has_unknown_fixed = elements
            .iter()
            .any(|element| !element.rest && element.type_id == TypeId::UNKNOWN);
        if !has_unknown_variadic || !has_unknown_fixed {
            return None;
        }

        let display_elements: Vec<_> = elements
            .iter()
            .map(|element| TupleElement {
                type_id: if element.rest {
                    element.type_id
                } else {
                    TypeId::UNKNOWN
                },
                name: element.name,
                optional: element.optional,
                rest: element.rest,
            })
            .collect();
        Some(self.format_tuple_element_display(&display_elements, false))
    }
}
