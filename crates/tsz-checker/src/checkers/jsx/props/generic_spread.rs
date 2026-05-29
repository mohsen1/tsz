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

        let spread_source_is_deferred = generic_spread_types
            .iter()
            .any(|&spread_type| self.jsx_relation_operand_defers(spread_type));
        let explicit_attrs_type = self.build_jsx_provided_attrs_object_type(provided_attrs);
        let mut members = generic_spread_types;
        members.push(explicit_attrs_type);
        let attrs_type = self.ctx.types.factory().intersection(members);

        let props_for_access = self.normalize_jsx_required_props_target(props_type);

        let has_explicit_prop_mismatch = provided_attrs.iter().any(|(name, actual_type)| {
            if matches!(*actual_type, TypeId::ANY | TypeId::ERROR) {
                return false;
            }
            let expected_type = self
                .jsx_expected_attribute_write_type(props_for_access, name)
                .or_else(|| {
                    self.jsx_concrete_prop_expected_type(props_for_access, name, &mut Vec::new())
                });
            let Some(expected_type) = expected_type else {
                return false;
            };
            // A deferred conditional on either operand is comparable for `tsc`;
            // the structural relation cannot soundly disprove it, so it is not a
            // genuine mismatch.
            if self.jsx_relation_operand_defers(*actual_type)
                || self.jsx_relation_operand_defers(expected_type)
            {
                return false;
            }
            !self
                .assign_relation_outcome(*actual_type, expected_type)
                .related
        });

        let has_alias_string_prop_mismatch = provided_attrs.iter().any(|(name, actual_type)| {
            *actual_type == TypeId::NUMBER && self.jsx_alias_declares_string_prop(props_type, name)
        });

        if has_excess_property_error
            && !has_explicit_prop_mismatch
            && !has_alias_string_prop_mismatch
        {
            return false;
        }

        // A spread whose source carries a deferred conditional over a type
        // parameter (e.g. `Omit`/`Overwrite` of an unresolved `T`) is an
        // instantiable, comparable type for `tsc`; it does not drive a
        // whole-object TS2322. Without a concrete explicit-attribute mismatch
        // the structural relation cannot soundly disprove assignability, so
        // emitting here is a false positive.
        if !has_explicit_prop_mismatch
            && !has_alias_string_prop_mismatch
            && spread_source_is_deferred
        {
            return false;
        }

        if !has_explicit_prop_mismatch
            && !has_alias_string_prop_mismatch
            && self.assign_relation_outcome(attrs_type, props_type).related
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
