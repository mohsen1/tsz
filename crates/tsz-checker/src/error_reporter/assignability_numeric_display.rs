//! Numeric literal union display rewrites for assignment diagnostics.

use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn canonicalize_assignment_numeric_literal_union_display(
        &mut self,
        type_id: TypeId,
        other_type: TypeId,
        display: String,
    ) -> String {
        let Some(display_type) = self.assignment_canonical_numeric_literal_display_type(type_id)
        else {
            return display;
        };
        let other_display_type =
            self.assignment_canonical_numeric_literal_counterpart_type(other_type);

        let mut replacements = Vec::new();
        let mut seen = Vec::new();
        self.collect_numeric_literal_union_display_replacements(
            display_type,
            other_display_type,
            &mut seen,
            &mut replacements,
        );
        let evaluated = self.evaluate_type_for_assignability(type_id);
        let other_evaluated = self.evaluate_type_for_assignability(other_type);
        if evaluated != type_id
            && crate::query_boundaries::common::function_shape_for_type(self.ctx.types, evaluated)
                .is_none()
            && crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, evaluated)
                .is_none()
        {
            self.collect_numeric_literal_union_display_replacements(
                evaluated,
                (other_evaluated != other_type).then_some(other_evaluated),
                &mut seen,
                &mut replacements,
            );
        }
        if replacements.is_empty() {
            return display;
        }

        replacements.sort_by_key(|(source_order, _)| std::cmp::Reverse(source_order.len()));
        replacements
            .into_iter()
            .fold(display, |current, (source_order, canonical_order)| {
                current.replace(&source_order, &canonical_order)
            })
    }

    fn collect_numeric_literal_union_display_replacements(
        &self,
        type_id: TypeId,
        other_type: Option<TypeId>,
        seen: &mut Vec<(TypeId, Option<TypeId>)>,
        replacements: &mut Vec<(String, String)>,
    ) {
        let seen_key = (type_id, other_type);
        if seen.contains(&seen_key) {
            return;
        }
        seen.push(seen_key);

        if self.is_number_literal_union_for_display_order(type_id) {
            if self.numeric_literal_union_origin_preserves_alias(type_id) {
                return;
            }
            let source_order =
                self.format_type_for_assignability_message_with_union_origin_policy(type_id, false);
            let canonical_order =
                self.format_type_for_assignability_message_with_union_origin_policy(type_id, true);
            if let Some(assignment_order) = self.assignment_canonical_number_literal_union_display(
                type_id,
                other_type,
                &source_order,
                &canonical_order,
            ) && !replacements
                .iter()
                .any(|(existing, _)| existing == &source_order)
            {
                replacements.push((source_order, assignment_order));
            }
        }

        match diagnostic_query::assignment_numeric_display_children(self.ctx.types, type_id) {
            diagnostic_query::AssignmentNumericDisplayChildren::Application { base, args } => {
                let other_args = other_type.and_then(|other| {
                    crate::query_boundaries::common::application_info(self.ctx.types, other)
                        .and_then(|(other_base, args)| (other_base == base).then_some(args))
                });
                for (index, &arg) in args.iter().enumerate() {
                    self.collect_numeric_literal_union_display_replacements(
                        arg,
                        other_args
                            .as_ref()
                            .and_then(|args| args.get(index))
                            .copied(),
                        seen,
                        replacements,
                    );
                }
            }
            diagnostic_query::AssignmentNumericDisplayChildren::Members(members) => {
                for member in members {
                    self.collect_numeric_literal_union_display_replacements(
                        member,
                        other_type,
                        seen,
                        replacements,
                    );
                }
            }
            diagnostic_query::AssignmentNumericDisplayChildren::Array(element) => {
                self.collect_numeric_literal_union_display_replacements(
                    element,
                    other_type.and_then(|other| {
                        crate::query_boundaries::common::array_element_type(self.ctx.types, other)
                    }),
                    seen,
                    replacements,
                );
            }
            diagnostic_query::AssignmentNumericDisplayChildren::Tuple(elements) => {
                let other_elements = other_type.and_then(|other| {
                    crate::query_boundaries::common::tuple_elements(self.ctx.types, other)
                });
                for (index, element) in elements.iter().enumerate() {
                    self.collect_numeric_literal_union_display_replacements(
                        element.type_id,
                        other_elements
                            .as_ref()
                            .and_then(|elements| elements.get(index))
                            .map(|element| element.type_id),
                        seen,
                        replacements,
                    );
                }
            }
            diagnostic_query::AssignmentNumericDisplayChildren::Object(shape) => {
                let other_shape = other_type.and_then(|other| {
                    diagnostic_query::object_shape_for_assignment_numeric_display(
                        self.ctx.types,
                        other,
                    )
                });
                for property in &shape.properties {
                    let other_property = other_shape.as_ref().and_then(|shape| {
                        shape
                            .properties
                            .iter()
                            .find(|other| other.name == property.name)
                    });
                    self.collect_numeric_literal_union_display_replacements(
                        property.type_id,
                        other_property.map(|property| property.type_id),
                        seen,
                        replacements,
                    );
                    if property.write_type != property.type_id {
                        self.collect_numeric_literal_union_display_replacements(
                            property.write_type,
                            other_property.map(|property| property.write_type),
                            seen,
                            replacements,
                        );
                    }
                }
                if let Some(index) = &shape.string_index {
                    self.collect_numeric_literal_union_display_replacements(
                        index.value_type,
                        other_shape
                            .as_ref()
                            .and_then(|shape| shape.string_index.as_ref())
                            .map(|index| index.value_type),
                        seen,
                        replacements,
                    );
                }
                if let Some(index) = &shape.number_index {
                    self.collect_numeric_literal_union_display_replacements(
                        index.value_type,
                        other_shape
                            .as_ref()
                            .and_then(|shape| shape.number_index.as_ref())
                            .map(|index| index.value_type),
                        seen,
                        replacements,
                    );
                }
            }
            diagnostic_query::AssignmentNumericDisplayChildren::None => {}
        }
    }

    fn assignment_canonical_numeric_literal_display_type(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        if crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            .is_some()
            || crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
                .is_some()
        {
            return None;
        }

        if self.is_number_literal_union_for_display_order(type_id)
            || crate::query_boundaries::common::application_info(self.ctx.types, type_id).is_some()
        {
            return Some(type_id);
        }

        let evaluated = self.evaluate_type_for_assignability(type_id);
        if evaluated == type_id
            || crate::query_boundaries::common::function_shape_for_type(self.ctx.types, evaluated)
                .is_some()
            || crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, evaluated)
                .is_some()
        {
            return None;
        }

        if self.is_number_literal_union_for_display_order(evaluated)
            || crate::query_boundaries::common::application_info(self.ctx.types, evaluated)
                .is_some()
        {
            Some(evaluated)
        } else {
            None
        }
    }

    fn assignment_canonical_numeric_literal_counterpart_type(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        if crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            .is_some()
            || crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
                .is_some()
        {
            return None;
        }

        let evaluated = self.evaluate_type_for_assignability(type_id);
        Some(evaluated)
    }

    fn format_type_for_assignability_message_with_union_origin_policy(
        &self,
        type_id: TypeId,
        ignore_union_origins: bool,
    ) -> String {
        let mut formatter =
            tsz_solver::TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols)
                .with_def_store(&self.ctx.definition_store)
                .with_diagnostic_mode()
                .with_preserve_optional_parameter_surface_syntax(true)
                .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks)
                .with_exact_optional_property_types(
                    self.ctx.compiler_options.exact_optional_property_types,
                );
        if ignore_union_origins {
            formatter = formatter.with_ignore_union_origins();
        }
        formatter.format(type_id).into_owned()
    }

    fn assignment_canonical_number_literal_union_display(
        &self,
        type_id: TypeId,
        other_type: Option<TypeId>,
        source_order: &str,
        canonical_order: &str,
    ) -> Option<String> {
        if let Some(relation_order) =
            self.numeric_literal_union_display_with_unmatched_members_first(type_id, other_type)
            && relation_order != source_order
        {
            return Some(relation_order);
        }

        if source_order != canonical_order {
            return Some(canonical_order.to_string());
        }

        None
    }

    fn numeric_literal_union_display_with_unmatched_members_first(
        &self,
        type_id: TypeId,
        other_type: Option<TypeId>,
    ) -> Option<String> {
        let other_numbers = self.numeric_literal_values_for_display_order(other_type?)?;
        if other_numbers.is_empty() {
            return None;
        }

        let members = crate::query_boundaries::common::union_members(self.ctx.types, type_id)?;
        let mut unmatched = Vec::new();
        let mut matched = Vec::new();
        for member in members {
            let number = self.numeric_literal_value(member)?;
            if other_numbers.contains(&number) {
                matched.push(member);
            } else {
                unmatched.push(member);
            }
        }
        if unmatched.is_empty() || matched.is_empty() {
            return None;
        }

        let mut ordered = unmatched;
        ordered.extend(matched);
        Some(
            ordered
                .into_iter()
                .map(|member| {
                    self.format_type_for_assignability_message_with_union_origin_policy(
                        member, true,
                    )
                })
                .collect::<Vec<_>>()
                .join(" | "),
        )
    }

    fn numeric_literal_values_for_display_order(&self, type_id: TypeId) -> Option<Vec<u64>> {
        if let Some(number) = self.numeric_literal_value(type_id) {
            return Some(vec![number]);
        }

        let members = crate::query_boundaries::common::union_members(self.ctx.types, type_id)?;
        let mut numbers = Vec::with_capacity(members.len());
        for member in members {
            numbers.push(self.numeric_literal_value(member)?);
        }
        Some(numbers)
    }

    fn numeric_literal_value(&self, type_id: TypeId) -> Option<u64> {
        diagnostic_query::number_literal_bits(self.ctx.types, type_id)
    }

    fn is_number_literal_union_for_display_order(&self, type_id: TypeId) -> bool {
        diagnostic_query::is_number_literal_union(self.ctx.types, type_id)
    }

    fn numeric_literal_union_origin_preserves_alias(&self, type_id: TypeId) -> bool {
        diagnostic_query::numeric_literal_union_origin_preserves_alias(
            self.ctx.types,
            &self.ctx.definition_store,
            type_id,
        )
    }
}
