//! Variadic tuple display helpers for call diagnostics.

use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_solver::{TupleElement, TypeId};

impl<'a> CheckerState<'a> {
    pub(crate) fn constrained_variadic_tuple_parameter_display(
        &mut self,
        param_type: TypeId,
        arg_type: TypeId,
    ) -> Option<String> {
        self.constrained_variadic_tuple_parameter_display_structured(param_type, arg_type)
            .or_else(|| {
                self.constrained_variadic_tuple_parameter_display_from_surface(param_type, arg_type)
            })
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
            if !self.is_assignable_to_with_env(actual.type_id, fixed.type_id) {
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

    fn constrained_variadic_tuple_parameter_display_from_surface(
        &mut self,
        param_type: TypeId,
        arg_type: TypeId,
    ) -> Option<String> {
        let display = self.format_type_diagnostic(param_type);
        let rest = display
            .strip_prefix("readonly [...readonly [")
            .or_else(|| display.strip_prefix("[...["))?;
        let (constraint, outer_tail) = rest.rsplit_once("], ")?;
        let outer_tail = outer_tail.strip_suffix(']')?;
        let (fixed, variadic) = constraint.split_once(", ...")?;
        if query_common::tuple_elements(self.ctx.types, arg_type).is_some() {
            Some(format!("[...{variadic}, {outer_tail}]"))
        } else {
            Some(fixed.to_string())
        }
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
