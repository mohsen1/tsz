//! Numeric literal union display rewrites for assignment diagnostics.

use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

struct NumericLiteralUnionDisplayOrder {
    members: Vec<String>,
    canonical: String,
}

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
            false,
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
                false,
                &mut seen,
                &mut replacements,
            );
        }
        if replacements.is_empty() {
            return self.canonicalize_displayed_numeric_literal_union_segments(type_id, display);
        }
        replacements.sort_by_key(|(source_order, _)| std::cmp::Reverse(source_order.len()));
        replacements
            .into_iter()
            .fold(display, |current, (source_order, canonical_order)| {
                current.replace(&source_order, &canonical_order)
            })
    }

    fn canonicalize_displayed_numeric_literal_union_segments(
        &self,
        type_id: TypeId,
        display: String,
    ) -> String {
        let mut orders = Vec::new();
        let mut seen = Vec::new();
        self.collect_numeric_literal_union_display_orders(type_id, &mut seen, &mut orders);
        orders.into_iter().fold(display, |current, order| {
            replace_matching_numeric_union_segments(&current, &order.members, &order.canonical)
        })
    }

    fn collect_numeric_literal_union_display_orders(
        &self,
        type_id: TypeId,
        seen: &mut Vec<TypeId>,
        orders: &mut Vec<NumericLiteralUnionDisplayOrder>,
    ) {
        if seen.contains(&type_id) {
            return;
        }
        seen.push(type_id);

        if self.is_number_literal_union_for_display_order(type_id)
            && let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, type_id)
        {
            let member_displays = members
                .iter()
                .map(|&member| {
                    self.format_type_for_assignability_message_with_union_origin_policy(
                        member, true,
                    )
                })
                .collect::<Vec<_>>();
            let canonical =
                self.format_type_for_assignability_message_with_union_origin_policy(type_id, true);
            if member_displays.len() > 1
                && member_displays.iter().all(|member| !member.contains(" | "))
                && !orders
                    .iter()
                    .any(|order| same_numeric_union_members(&order.members, &member_displays))
            {
                orders.push(NumericLiteralUnionDisplayOrder {
                    members: member_displays,
                    canonical,
                });
            }
        }

        match diagnostic_query::assignment_numeric_display_children(self.ctx.types, type_id) {
            diagnostic_query::AssignmentNumericDisplayChildren::Application { args, .. } => {
                for arg in args {
                    self.collect_numeric_literal_union_display_orders(arg, seen, orders);
                }
            }
            diagnostic_query::AssignmentNumericDisplayChildren::Members(members) => {
                for member in members {
                    self.collect_numeric_literal_union_display_orders(member, seen, orders);
                }
            }
            diagnostic_query::AssignmentNumericDisplayChildren::Array(element) => {
                self.collect_numeric_literal_union_display_orders(element, seen, orders);
            }
            diagnostic_query::AssignmentNumericDisplayChildren::Tuple(elements) => {
                for element in elements {
                    self.collect_numeric_literal_union_display_orders(
                        element.type_id,
                        seen,
                        orders,
                    );
                }
            }
            diagnostic_query::AssignmentNumericDisplayChildren::Object(shape) => {
                for property in &shape.properties {
                    self.collect_numeric_literal_union_display_orders(
                        property.type_id,
                        seen,
                        orders,
                    );
                    if property.write_type != TypeId::NONE {
                        self.collect_numeric_literal_union_display_orders(
                            property.write_type,
                            seen,
                            orders,
                        );
                    }
                }
                if let Some(index) = &shape.string_index {
                    self.collect_numeric_literal_union_display_orders(
                        index.value_type,
                        seen,
                        orders,
                    );
                }
                if let Some(index) = &shape.number_index {
                    self.collect_numeric_literal_union_display_orders(
                        index.value_type,
                        seen,
                        orders,
                    );
                }
            }
            diagnostic_query::AssignmentNumericDisplayChildren::None => {}
        }
    }

    fn collect_numeric_literal_union_display_replacements(
        &self,
        type_id: TypeId,
        other_type: Option<TypeId>,
        rewrite_alias_origin: bool,
        seen: &mut Vec<(TypeId, Option<TypeId>)>,
        replacements: &mut Vec<(String, String)>,
    ) {
        let seen_key = (type_id, other_type);
        if seen.contains(&seen_key) {
            return;
        }
        seen.push(seen_key);

        if self.source_type_contains_number_literal_only_union(type_id) {
            let source_order =
                self.format_type_for_assignability_message_with_union_origin_policy(type_id, false);
            if !source_order.contains(" | ")
                && self.is_number_literal_union_for_display_order(type_id)
                && !rewrite_alias_origin
                && self.numeric_literal_union_origin_preserves_alias(type_id)
            {
                return;
            }
            let canonical_order =
                self.format_type_for_assignability_message_with_union_origin_policy(type_id, true);
            let assignment_order = if !source_order.contains(" | ") {
                None
            } else if self.is_number_literal_union_for_display_order(type_id) {
                self.assignment_canonical_number_literal_union_display(
                    type_id,
                    other_type,
                    &source_order,
                    &canonical_order,
                )
            } else if source_order != canonical_order {
                Some(canonical_order)
            } else {
                None
            };
            if let Some(assignment_order) = assignment_order
                && !replacements
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
                        true,
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
                        rewrite_alias_origin,
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
                    rewrite_alias_origin,
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
                        rewrite_alias_origin,
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
                        rewrite_alias_origin,
                        seen,
                        replacements,
                    );
                    if property.write_type != property.type_id {
                        self.collect_numeric_literal_union_display_replacements(
                            property.write_type,
                            other_property.map(|property| property.write_type),
                            rewrite_alias_origin,
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
                        rewrite_alias_origin,
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
                        rewrite_alias_origin,
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
            || self.source_type_contains_number_literal_only_union(type_id)
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

fn replace_matching_numeric_union_segments(
    display: &str,
    members: &[String],
    canonical: &str,
) -> String {
    let mut output = String::with_capacity(display.len());
    let mut cursor = 0;
    while cursor < display.len() {
        let Some(open_offset) = display[cursor..].find('(') else {
            output.push_str(&display[cursor..]);
            break;
        };
        let open = cursor + open_offset;
        output.push_str(&display[cursor..open]);
        let Some(close_offset) = display[open + 1..].find(')') else {
            output.push_str(&display[open..]);
            break;
        };
        let close = open + 1 + close_offset;
        let inner = &display[open + 1..close];
        if numeric_union_segment_matches(inner, members) {
            output.push('(');
            output.push_str(canonical);
            output.push(')');
        } else {
            output.push_str(&display[open..=close]);
        }
        cursor = close + 1;
    }

    if numeric_union_segment_matches(output.as_str(), members) {
        canonical.to_string()
    } else {
        output
    }
}

fn numeric_union_segment_matches(segment: &str, members: &[String]) -> bool {
    let parts = segment.split(" | ").map(str::trim).collect::<Vec<_>>();
    if parts.len() != members.len() || parts.len() < 2 {
        return false;
    }
    let mut matched = vec![false; members.len()];
    'parts: for part in parts {
        for (index, member) in members.iter().enumerate() {
            if !matched[index] && part == member {
                matched[index] = true;
                continue 'parts;
            }
        }
        return false;
    }
    true
}

fn same_numeric_union_members(left: &[String], right: &[String]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut matched = vec![false; right.len()];
    'left: for member in left {
        for (index, candidate) in right.iter().enumerate() {
            if !matched[index] && member == candidate {
                matched[index] = true;
                continue 'left;
            }
        }
        return false;
    }
    true
}
