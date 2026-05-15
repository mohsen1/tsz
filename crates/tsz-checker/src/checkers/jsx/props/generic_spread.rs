//! JSX generic spread whole-object assignability diagnostics.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

pub(in crate::checkers_domain::jsx) struct GenericSpreadAssignabilityReport<'a> {
    pub generic_spread_types: Vec<TypeId>,
    pub provided_attrs: &'a [(String, TypeId)],
    pub props_type: TypeId,
    pub display_target: &'a str,
    pub tag_name_idx: NodeIndex,
    pub has_excess_property_error: bool,
    pub skip_prop_checks: bool,
    pub has_explicit_jsx_attrs: bool,
}

impl<'a> CheckerState<'a> {
    pub(in crate::checkers_domain::jsx) fn report_invalid_generic_jsx_spread_assignability(
        &mut self,
        report: GenericSpreadAssignabilityReport<'_>,
    ) -> bool {
        let GenericSpreadAssignabilityReport {
            generic_spread_types,
            provided_attrs,
            props_type,
            display_target,
            tag_name_idx,
            has_excess_property_error,
            skip_prop_checks,
            has_explicit_jsx_attrs,
        } = report;

        if generic_spread_types.is_empty() || skip_prop_checks {
            return false;
        }
        // With no explicit attributes, `provided_attrs` may still contain
        // properties enumerated from spread shapes. The per-spread checker owns
        // that whole-object diagnostic because it has the spread expression's
        // raw type-parameter surface (`T`) and the precise tag anchor.
        // Rebuilding an aggregate attrs object here normalizes that source to
        // `T & Constraint`, producing a duplicate fingerprint.
        if !has_explicit_jsx_attrs {
            return false;
        }

        let explicit_attrs_type = self.build_jsx_provided_attrs_object_type(provided_attrs);
        let mut members = generic_spread_types;
        members.push(explicit_attrs_type);
        let attrs_type = self.ctx.types.factory().intersection(members);

        let props_for_access = self.normalize_jsx_required_props_target(props_type);

        let has_explicit_prop_mismatch = provided_attrs.iter().any(|(name, actual_type)| {
            use crate::query_boundaries::common::PropertyAccessResult;

            if matches!(*actual_type, TypeId::ANY | TypeId::ERROR) {
                return false;
            }
            let expected_type = match self.resolve_property_access_with_env(props_for_access, name)
            {
                PropertyAccessResult::Success { type_id, .. }
                | PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(type_id),
                    ..
                } => Some(type_id),
                _ => crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    props_for_access,
                )
                .and_then(|shape| {
                    shape.properties.iter().find_map(|prop| {
                        (self.ctx.types.resolve_atom(prop.name) == name.as_str())
                            .then_some(prop.type_id)
                    })
                }),
            };
            let expected_type = expected_type.or_else(|| {
                self.jsx_concrete_prop_expected_type(props_for_access, name, &mut Vec::new())
            });
            let Some(expected_type) = expected_type else {
                return false;
            };
            let expected_type =
                crate::query_boundaries::common::remove_undefined(self.ctx.types, expected_type);
            !self.is_assignable_to(*actual_type, expected_type)
        });

        let target_display = self.format_type(props_type);
        let has_displayed_string_prop_mismatch =
            provided_attrs.iter().any(|(name, actual_type)| {
                *actual_type == TypeId::NUMBER
                    && target_display.contains(&format!("{name}: string"))
            });
        let evaluated_props_display = {
            let evaluated = self.evaluate_type_with_env(props_type);
            self.format_type(evaluated)
        };
        let has_alias_string_prop_mismatch = provided_attrs.iter().any(|(name, actual_type)| {
            *actual_type == TypeId::NUMBER
                && evaluated_props_display.contains(&format!("{name}: string"))
        });
        let has_alias_source_string_prop_mismatch =
            provided_attrs.iter().any(|(name, actual_type)| {
                *actual_type == TypeId::NUMBER
                    && self.jsx_alias_source_has_prop_text(props_type, name, "string")
            });

        if has_excess_property_error
            && !has_explicit_prop_mismatch
            && !has_displayed_string_prop_mismatch
            && !has_alias_string_prop_mismatch
            && !has_alias_source_string_prop_mismatch
        {
            return false;
        }

        if !has_explicit_prop_mismatch
            && !has_displayed_string_prop_mismatch
            && !has_alias_string_prop_mismatch
            && !has_alias_source_string_prop_mismatch
            && self.is_assignable_to(attrs_type, props_type)
        {
            return false;
        }

        self.report_jsx_synthesized_props_assignability_error(
            attrs_type,
            display_target,
            tag_name_idx,
        );
        true
    }
}
