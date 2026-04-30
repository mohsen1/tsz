//! JSX spread attribute checking: TS2322 for spread property type mismatches,
//! TS2559 for weak type violations, and TS2783 for attribute overwrite detection.

use crate::context::TypingRequest;
use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check spread property types against the expected props type.
    ///
    /// When there are multiple spreads, we don't emit TS2739/TS2740 for missing
    /// properties here because later spreads might provide them. Instead, we let
    /// the final combined prop validation handle missing property checks.
    ///
    /// `earlier_explicit_attrs` maps earlier explicit attribute names (i.e.
    /// attributes appearing BEFORE this spread) to their name-node indices.
    /// When the spread's property mismatches the expected prop type AND there
    /// is an earlier explicit attribute with the same name, tsc anchors the
    /// per-property TS2322 at that earlier attribute (matching where TS2783
    /// "specified more than once" is emitted), with the per-property message
    /// ("Type 'X' is not assignable to type 'Y'") rather than the whole-type
    /// message at the JSX tag name.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_spread_property_types(
        &mut self,
        spread_type: TypeId,
        props_type: TypeId,
        tag_name_idx: NodeIndex,
        overridden_names: &rustc_hash::FxHashSet<&str>,
        overridden_for_missing: &rustc_hash::FxHashSet<&str>,
        earlier_explicit_attrs: &rustc_hash::FxHashMap<String, NodeIndex>,
        has_later_spreads: bool,
        suppress_missing_props: bool,
        display_target: &str,
    ) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;

        // Safety guard: skip when types already contain checker error states.
        if crate::query_boundaries::common::contains_error_type(self.ctx.types, spread_type) {
            return false;
        }

        let spread_has_type_params =
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, spread_type);

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
                let has_jsx_managed_prop = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    resolved_spread,
                )
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
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, resolved_spread)
        else {
            // For generic spreads without a resolvable object shape, emit TS2322
            // if the spread type is not assignable to the props type.
            // This handles cases like `T extends { y: string }` being spread into
            // an element that requires `{ x: string }` - T doesn't satisfy the requirement.
            if spread_has_type_params && !overridden_names.is_empty() {
                // A later explicit attribute overrides this generic spread,
                // so the spread's type issues are masked.
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

        // When there are multiple spreads, we don't emit TS2739 for missing properties
        // from individual spreads. Later spreads might provide the missing properties,
        // and the final combined prop validation will catch truly missing properties.
        // Also suppress when TS2710 (children specified twice) will be emitted.
        // Only check for missing properties when this is the ONLY spread (no later spreads)
        // and we're not suppressing missing props.
        // For generic spread types (has_type_params), emit TS2322 instead of TS2741
        // to match tsc's behavior for intrinsic element type mismatches.
        if !has_later_spreads
            && !suppress_missing_props
            && !spread_has_type_params
            && let Some(props_shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, props_type)
        {
            let spread_prop_names: rustc_hash::FxHashSet<String> = spread_shape
                .properties
                .iter()
                .map(|p| self.ctx.types.resolve_atom(p.name))
                .collect();
            let mut missing_props: Vec<String> = Vec::new();
            for req_prop in &props_shape.properties {
                if req_prop.optional {
                    continue;
                }
                let req_name = self.ctx.types.resolve_atom(req_prop.name).to_string();
                if req_name == "key" || req_name == "ref" {
                    continue;
                }
                if !spread_prop_names.contains(&req_name)
                    && !overridden_for_missing.contains(req_name.as_str())
                {
                    missing_props.push(req_name);
                }
            }

            if !missing_props.is_empty() {
                // Format as a fresh structural type in declaration order — tsc shows the
                // object shape, not the type alias name, in JSX spread missing-property
                // diagnostics.
                let spread_name = {
                    let mut props: Vec<_> = spread_shape.properties.to_vec();
                    crate::query_boundaries::common::normalize_display_property_order(&mut props);
                    let fields: Vec<String> = props
                        .iter()
                        .map(|p| {
                            let prop_name = self.ctx.types.resolve_atom(p.name);
                            let type_str = self.format_type(p.type_id);
                            format!("{prop_name}: {type_str}")
                        })
                        .collect();
                    if fields.is_empty() {
                        "{}".to_string()
                    } else {
                        format!("{{ {}; }}", fields.join("; "))
                    }
                };
                if missing_props.len() == 1 {
                    // TS2741: Property 'x' is missing in type 'A' but required in type 'B'.
                    let message = format_message(
                        diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                        &[&missing_props[0], &spread_name, &props_display],
                    );
                    self.error_at_node(
                        tag_name_idx,
                        &message,
                        diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                    );
                } else {
                    // TS2739 (≤4 missing props) or TS2740 (>4 missing props)
                    let is_truncated = missing_props.len() > 4;
                    let display_count = if is_truncated { 4 } else { missing_props.len() };
                    let props_list = missing_props[..display_count].join(", ");

                    let (message, code) = if is_truncated {
                        let more_count = missing_props.len() - display_count;
                        (
                                format_message(
                                    diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                                    &[&spread_name, &props_display, &props_list, &more_count.to_string()],
                                ),
                                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                            )
                    } else {
                        (
                                format_message(
                                    diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                                    &[&spread_name, &props_display, &props_list],
                                ),
                                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                            )
                    };
                    self.error_at_node(tag_name_idx, &message, code);
                }
                return true;
            }
        }

        // Check per-property type mismatches
        // Track mismatches that will NOT be fixed by later explicit attributes.
        // A mismatch is "fixable" if a later explicit attr will overwrite the property.
        // A mismatch is "unfixable" if the spread's value will actually be used.
        //
        // When an unfixable mismatch corresponds to an EARLIER explicit attribute
        // (i.e. the spread overrides that attribute via TS2783), tsc anchors the
        // per-property TS2322 at that earlier attribute with the per-property
        // message. We collect those mismatches and emit them inline; remaining
        // unfixable mismatches fall back to a whole-type TS2322 at the tag name.
        let mut has_unfixable_mismatch = false;
        let mut anchored_per_property_emitted = false;
        for prop in &spread_shape.properties {
            let prop_name = self.ctx.types.resolve_atom(prop.name).to_string();

            // Skip key/ref as they're handled specially by JSX
            if prop_name == "key" || prop_name == "ref" {
                continue;
            }

            let expected_type = match self.resolve_property_access_with_env(props_type, &prop_name)
            {
                PropertyAccessResult::Success { type_id, .. } => {
                    crate::query_boundaries::common::remove_undefined(self.ctx.types, type_id)
                }
                // Property doesn't exist in target - this will be caught as excess
                // property or missing property elsewhere
                _ => continue,
            };

            let source_type = if prop.optional {
                crate::query_boundaries::common::remove_undefined(self.ctx.types, prop.type_id)
            } else {
                prop.type_id
            };

            if !self.is_assignable_to(source_type, expected_type) {
                // This property has a type mismatch.
                // Check if it will be overwritten by a later explicit attribute.
                if overridden_names.contains(prop_name.as_str()) {
                    // If the property is in overridden_names, a later explicit attr
                    // will provide the value instead, so this mismatch is fixable.
                    continue;
                }

                // If an EARLIER explicit attribute has the same name, the spread
                // overrides it (TS2783). tsc anchors the per-property TS2322 at
                // that earlier attribute with the per-property message.
                if let Some(&attr_name_idx) = earlier_explicit_attrs.get(&prop_name) {
                    let source_str = self.format_type(source_type);
                    let target_str = self.format_type(expected_type);
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    self.error_at_node(
                        attr_name_idx,
                        &message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                    anchored_per_property_emitted = true;
                    continue;
                }

                // No later explicit attr will overwrite this property AND no
                // earlier explicit attr to anchor at, so the spread's wrong
                // value will be used and we report a whole-type TS2322 below.
                has_unfixable_mismatch = true;
                break;
            }
        }

        // For generic spreads, also check whole-type assignability to catch
        // missing required properties that aren't covered by per-property checks.
        let mut has_type_mismatch = has_unfixable_mismatch;
        if !has_type_mismatch
            && spread_has_type_params
            && !self.is_assignable_to(resolved_spread, props_type)
        {
            has_type_mismatch = true;
        }

        // For generic spreads with type mismatches, only suppress TS2322 if the
        // resolved spread type is assignable to the props type.
        if has_type_mismatch
            && spread_has_type_params
            && self.is_assignable_to(resolved_spread, props_type)
        {
            has_type_mismatch = false;
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
        } else if anchored_per_property_emitted {
            // We emitted per-property TS2322 at earlier explicit attributes
            // (matching the TS2783 anchor). Treat this as "had error" so the
            // caller can suppress redundant TS2741 for the same spread.
            return true;
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

                if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    spread_type,
                ) {
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
