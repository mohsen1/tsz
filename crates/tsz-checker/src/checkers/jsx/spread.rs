//! JSX spread attribute checking: TS2322 for spread property type mismatches,
//! TS2559 for weak type violations, and TS2783 for attribute overwrite detection.

use crate::context::TypingRequest;
use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_spread_property_types(
        &mut self,
        spread_type: TypeId,
        props_type: TypeId,
        tag_name_idx: NodeIndex,
        overridden_names: &rustc_hash::FxHashSet<&str>,
        display_target: &str,
    ) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;

        // Safety guard: skip when types already contain checker error states.
        if tsz_solver::contains_error_type(self.ctx.types, spread_type) {
            return false;
        }

        let spread_has_type_params =
            tsz_solver::contains_type_parameters(self.ctx.types, spread_type);

        // For concrete spread types, whole-type assignability is the fast path and
        // also prevents false positives from imprecise per-property extraction.
        // For generic spreads, the relation can be too optimistic; keep them on the
        // normalized JSX spread path below so we can classify TS2322 vs TS2741 from
        // the apparent/object shape first.
        if !spread_has_type_params && self.is_assignable_to(spread_type, props_type) {
            return false;
        }

        // TS2559: When the spread type has no properties in common with the target
        // props type (a "weak type" violation), tsc emits TS2559 instead of proceeding
        // with per-property TS2322 checks.
        if !spread_has_type_params {
            let analysis = self.analyze_assignability_failure(spread_type, props_type);
            if matches!(
                &analysis.failure_reason,
                Some(tsz_solver::SubtypeFailureReason::NoCommonProperties { .. })
            ) {
                let resolved_spread = self.evaluate_type_with_env(spread_type);
                let resolved_spread = self.resolve_type_for_property_access(resolved_spread);
                let has_jsx_managed_prop =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, resolved_spread)
                        .map(|shape| {
                            shape.properties.iter().any(|p| {
                                let name = self.ctx.types.resolve_atom(p.name);
                                name == "key"
                                    || name == "ref"
                                    || name == "children"
                                    || name.starts_with("data-")
                                    || name.starts_with("aria-")
                            })
                        })
                        .unwrap_or(false);

                if !has_jsx_managed_prop {
                    let source_str = self.format_type(spread_type);
                    let target_str = if display_target.is_empty() {
                        self.format_type(props_type)
                    } else {
                        display_target.to_string()
                    };
                    let message = format_message(
                        diagnostic_messages::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                        &[&source_str, &target_str],
                    );
                    self.error_at_node(
                        tag_name_idx,
                        &message,
                        diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                    );
                    return true;
                }
            }
        }

        // Resolve the spread type to extract its properties
        let resolved_spread = self.evaluate_type_with_env(spread_type);
        let resolved_spread = self.resolve_type_for_property_access(resolved_spread);

        let props_display = self.format_type(props_type);

        let Some(spread_shape) =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, resolved_spread)
        else {
            let props_has_type_params =
                tsz_solver::contains_type_parameters(self.ctx.types, props_type);
            if (spread_has_type_params && !overridden_names.is_empty()) || props_has_type_params {
                return false;
            }
            if self.is_assignable_to(spread_type, props_type) {
                return false;
            }
            let spread_name = self.format_type(spread_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&spread_name, &props_display],
            );
            self.error_at_node(
                tag_name_idx,
                &message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            return true;
        };

        // Suppress TS2322 when spread also has missing required properties (TS2741 handles those).
        if let Some(props_shape) =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, props_type)
        {
            let spread_prop_names: rustc_hash::FxHashSet<String> = spread_shape
                .properties
                .iter()
                .map(|p| self.ctx.types.resolve_atom(p.name))
                .collect();
            for req_prop in &props_shape.properties {
                if req_prop.optional {
                    continue;
                }
                let req_name = self.ctx.types.resolve_atom(req_prop.name).to_string();
                if req_name == "key" || req_name == "ref" {
                    continue;
                }
                if !spread_prop_names.contains(&req_name)
                    && !overridden_names.contains(req_name.as_str())
                {
                    if spread_has_type_params {
                        if self.is_assignable_to(spread_type, props_type) {
                            return false;
                        }
                        let spread_name = self.format_type(spread_type);
                        let message = format_message(
                            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            &[&spread_name, &props_display],
                        );
                        self.error_at_node(
                            tag_name_idx,
                            &message,
                            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        );
                        return true;
                    }
                    return false;
                }
            }
        }

        // Check per-property type mismatches
        let mut has_type_mismatch = false;
        for prop in &spread_shape.properties {
            let prop_name = self.ctx.types.resolve_atom(prop.name).to_string();

            if overridden_names.contains(prop_name.as_str()) {
                continue;
            }
            if prop_name == "key" || prop_name == "ref" {
                continue;
            }

            let expected_type = match self.resolve_property_access_with_env(props_type, &prop_name)
            {
                PropertyAccessResult::Success { type_id, .. } => {
                    tsz_solver::remove_undefined(self.ctx.types, type_id)
                }
                _ => continue,
            };

            let source_type = if prop.optional {
                tsz_solver::remove_undefined(self.ctx.types, prop.type_id)
            } else {
                prop.type_id
            };

            if !self.is_assignable_to(source_type, expected_type) {
                has_type_mismatch = true;
                break;
            }
        }

        if has_type_mismatch && spread_has_type_params {
            if self.is_assignable_to(spread_type, props_type) {
                has_type_mismatch = false;
            }
        }

        if has_type_mismatch {
            let spread_name = self.format_type(spread_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&spread_name, &props_display],
            );
            self.error_at_node(
                tag_name_idx,
                &message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        false
    }

    /// TS2783: Check if a later spread attribute will overwrite the current attribute.
    pub(crate) fn check_jsx_attr_overwritten_by_spread(
        &mut self,
        attr_name: &str,
        attr_name_idx: NodeIndex,
        attr_nodes: &[NodeIndex],
        current_idx: usize,
    ) -> bool {
        for &later_idx in &attr_nodes[current_idx + 1..] {
            let Some(later_node) = self.ctx.arena.get(later_idx) else {
                continue;
            };
            if later_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(later_node) else {
                    continue;
                };
                let spread_type = self.compute_normalized_jsx_spread_type_with_request(
                    spread_data.expression,
                    &TypingRequest::NONE,
                );

                if spread_type == TypeId::ANY
                    || spread_type == TypeId::ERROR
                    || spread_type == TypeId::UNKNOWN
                {
                    continue;
                }

                if let Some(shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, spread_type)
                {
                    let attr_atom = self.ctx.types.intern_string(attr_name);
                    let has_required_prop = shape
                        .properties
                        .iter()
                        .any(|p| p.name == attr_atom && !p.optional);
                    if has_required_prop {
                        let message = format_message(
                            diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                            &[attr_name],
                        );
                        self.error_at_node(
                            attr_name_idx,
                            &message,
                            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                        );
                        return true;
                    }
                }
            }
        }
        false
    }
}
