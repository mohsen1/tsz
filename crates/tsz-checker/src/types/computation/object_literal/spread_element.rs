//! Spread element handling for object literal type computation.

use super::computation_support::{
    SPREAD_DISPLAY_ORDER_STRIDE, rebase_spread_display_property_order,
    remove_synthetic_missing_union_spread_props,
};
use crate::context::TypingRequest;
use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{IndexSignature, PropertyInfo, TypeId};

impl<'a> CheckerState<'a> {
    pub(super) fn process_object_literal_spread_element(
        &mut self,
        elem_idx: NodeIndex,
        obj_element_count: usize,
        base_request: &TypingRequest,
        contextual_type: Option<TypeId>,
        marker_this_type: Option<TypeId>,
        partial_initializer_stack_index: Option<usize>,
        properties: &mut FxHashMap<Atom, PropertyInfo>,
        named_property_nodes: &mut FxHashMap<Atom, (NodeIndex, String)>,
        union_spread_branches: &mut Vec<FxHashMap<Atom, PropertyInfo>>,
        spread_string_index_signatures: &mut Vec<IndexSignature>,
        spread_number_index_signatures: &mut Vec<IndexSignature>,
        generic_spread_types: &mut Vec<TypeId>,
        has_spread: &mut bool,
        has_any_spread: &mut bool,
        has_union_spread: &mut bool,
        spread_display_order_base: &mut u32,
    ) -> Option<TypeId> {
        let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
            return None;
        };
        *has_spread = true;
        let spread_expr = self
            .ctx
            .arena
            .get_spread(elem_node)
            .map(|spread| spread.expression)
            .or_else(|| {
                self.ctx
                    .arena
                    .get_unary_expr_ex(elem_node)
                    .map(|unary| unary.expression)
            });
        if let Some(spread_expr) = spread_expr {
            let mut invalid_rest_target = false;
            if self.ctx.in_destructuring_target {
                // TS2701: The target of an object rest assignment must be
                // a variable or a property access.
                // E.g. `{ ...expr + expr } = source` is invalid.
                if !self.is_valid_rest_assignment_target(spread_expr) {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                                spread_expr,
                                diagnostic_messages::THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                                diagnostic_codes::THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MUST_BE_A_VARIABLE_OR_A_PROPERTY_ACCESS,
                            );
                    invalid_rest_target = true;
                }
                // TS2778: The target of an object rest assignment may not be
                // an optional property access. E.g. `{ ...obj?.a } = source`
                else if self.is_optional_chain_access(spread_expr) {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                                spread_expr,
                                diagnostic_messages::THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
                                diagnostic_codes::THE_TARGET_OF_AN_OBJECT_REST_ASSIGNMENT_MAY_NOT_BE_AN_OPTIONAL_PROPERTY_ACCESS,
                            );
                }
            }
            // Clear contextual type for call-like spread expressions.
            // The outer contextual type (e.g., from a destructuring pattern)
            // should not propagate into call expression return types —
            // otherwise IIFEs in spreads get false contextual return types,
            // producing spurious TS2741/TS2322 errors.
            // But direct object literals in spreads (e.g., `{ ...{ a: "a" } }`)
            // SHOULD keep the contextual type so literals stay narrow.
            let unwrapped_spread = self
                .ctx
                .arena
                .skip_parenthesized_and_assertions(spread_expr);
            let spread_is_call_like = self.ctx.arena.get(unwrapped_spread).is_some_and(|node| {
                node.kind == syntax_kind_ext::CALL_EXPRESSION
                    || node.kind == syntax_kind_ext::NEW_EXPRESSION
                    || node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
            });
            let spread_request = if spread_is_call_like {
                base_request.contextual_opt(None)
            } else {
                *base_request
            };
            let spread_type = self.get_type_of_node_with_request(spread_expr, &spread_request);
            let this_options_receiver_type = self
                .this_options_property_access_receiver(spread_expr)
                .map(|receiver_idx| self.get_type_of_node(receiver_idx));
            let is_contextual_this_options_any_spread = spread_type == TypeId::ANY
                && this_options_receiver_type
                    .is_some_and(|receiver_type| receiver_type != TypeId::ANY);
            if !self.ctx.in_destructuring_target
                && spread_type == TypeId::ANY
                && !is_contextual_this_options_any_spread
            {
                *has_any_spread = true;
            }
            // TS2698: Spread types may only be created from object types.
            // Only check in expression context — in destructuring targets,
            // `{ ...x }` is a rest binding (x receives remaining properties),
            // not a spread creation (reading x's properties), so the spread
            // validity check does not apply.
            // Also skip when TS2701 was already emitted (invalid rest target).
            let resolved_spread = self.resolve_type_for_property_access(spread_type);
            let resolved_spread = self.resolve_lazy_type(resolved_spread);
            let is_valid_spread = if self.ctx.in_destructuring_target || invalid_rest_target {
                true // rest binding in destructuring or already reported TS2701
            } else {
                crate::query_boundaries::type_computation::access::is_valid_spread_type(
                    self.ctx.types,
                    resolved_spread,
                )
            };
            if !is_valid_spread {
                self.report_spread_not_object_type(elem_idx);
            }

            // Short-circuit: when the object literal is a single spread
            // of a type parameter (e.g., `{ ...item }` where `item: T`),
            // preserve the type parameter as the result type. Expanding
            // to the constraint's properties would lose generic type
            // information, causing false TS2322 errors like
            // `Type '{ name: string }' is not assignable to type 'T'`.
            // Only when the spread is valid (no TS2698) — invalid spreads
            // like `T extends undefined` must not short-circuit.
            if is_valid_spread
                && obj_element_count == 1
                && properties.is_empty()
                && (crate::query_boundaries::common::type_param_info(self.ctx.types, spread_type)
                    .is_some()
                    || crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        spread_type,
                    ))
            {
                self.pop_object_literal_contexts(marker_this_type, partial_initializer_stack_index);
                return Some(spread_type);
            }

            // Check if the spread type is a union — if so, distribute
            // the spread over each union member: { ...A|B } → { ...A } | { ...B }
            let union_members_opt =
                crate::query_boundaries::common::union_members(self.ctx.types, resolved_spread);

            // Guard against exponential blowup: if the cross-product
            // of branches would exceed a limit, skip distribution.
            let branch_count = if union_spread_branches.is_empty() {
                1
            } else {
                union_spread_branches.len()
            };
            let union_members_opt = union_members_opt.filter(|members| {
                // Only distribute when all members are object-like (not
                // false/null/undefined). Spreading primitives just
                // contributes {} which isn't useful to distribute.
                let all_object_like = members.iter().all(|m| {
                    !self
                        .ctx
                        .types
                        .collect_object_spread_properties(*m)
                        .is_empty()
                });
                all_object_like && branch_count.saturating_mul(members.len()) <= 16
            });

            if let Some(members) = union_members_opt {
                // TS2783: Check if any earlier named properties will be
                // overwritten by required properties from this union spread.
                // A property triggers TS2783 when it is required (non-optional)
                // in ALL non-nullish members of the union.
                if self.ctx.strict_null_checks() {
                    let non_nullish_members: Vec<TypeId> = members
                        .iter()
                        .copied()
                        .filter(|m| !m.is_nullable())
                        .collect();
                    if !non_nullish_members.is_empty() {
                        // Collect properties per member
                        let all_member_props: Vec<Vec<_>> = non_nullish_members
                            .iter()
                            .map(|m| self.collect_object_spread_properties(*m))
                            .collect();
                        // Find properties that are required in ALL members
                        if let Some(first) = all_member_props.first() {
                            for prop in first {
                                if prop.optional {
                                    continue;
                                }
                                let in_all = all_member_props[1..].iter().all(|member_props| {
                                    member_props
                                        .iter()
                                        .any(|p| p.name == prop.name && !p.optional)
                                });
                                if in_all
                                    && let Some((prop_node, prop_name)) =
                                        named_property_nodes.get(&prop.name)
                                {
                                    let message = format_message(
                                                    diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                                    &[prop_name],
                                                );
                                    self.error_at_node(
                                                    *prop_node,
                                                    &message,
                                                    diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                                );
                                }
                            }
                        }
                    }
                }

                // Union spread distribution: fork current property set
                // into N branches, one per union member.
                *has_union_spread = true;
                let mut new_branches: Vec<FxHashMap<Atom, PropertyInfo>> = Vec::new();

                // Collect properties from each union member for TS2783
                // and branching.
                let mut all_member_props: Vec<Vec<PropertyInfo>> = members
                    .iter()
                    .map(|m| self.collect_object_spread_properties(*m))
                    .collect();
                remove_synthetic_missing_union_spread_props(&mut all_member_props);

                // TS2783: When a property is required (non-optional)
                // in ALL members of the union spread, it will always
                // overwrite any earlier named property.
                if self.ctx.strict_null_checks() && !named_property_nodes.is_empty() {
                    // Find property names that are required in every member.
                    let mut always_required: FxHashMap<Atom, bool> = FxHashMap::default();
                    for (i, member_props) in all_member_props.iter().enumerate() {
                        if i == 0 {
                            for prop in member_props {
                                always_required.insert(prop.name, !prop.optional);
                            }
                        } else {
                            // Remove names not present in this member
                            always_required.retain(|name, required| {
                                if let Some(prop) = member_props.iter().find(|p| p.name == *name) {
                                    if prop.optional {
                                        *required = false;
                                    }
                                    true
                                } else {
                                    false
                                }
                            });
                        }
                    }
                    for (name, required) in &always_required {
                        if *required
                            && let Some((prop_node, prop_name)) = named_property_nodes.get(name)
                        {
                            let message = format_message(
                                            diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                            &[prop_name],
                                        );
                            self.error_at_node(
                                            *prop_node,
                                            &message,
                                            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                        );
                        }
                    }
                    // Clear named-property tracking for overwritten props
                    for name in always_required.keys() {
                        named_property_nodes.remove(name);
                    }
                }

                let spread_order_base = *spread_display_order_base;
                *spread_display_order_base =
                    (*spread_display_order_base).saturating_sub(SPREAD_DISPLAY_ORDER_STRIDE);
                for member_props in all_member_props {
                    let member_props =
                        rebase_spread_display_property_order(&member_props, spread_order_base);
                    if union_spread_branches.is_empty() {
                        // First union spread: fork from the main properties
                        let mut branch = properties.clone();
                        for prop in member_props {
                            self.merge_spread_property(&mut branch, &prop);
                        }
                        new_branches.push(branch);
                    } else {
                        // Subsequent union spread: cross-product with existing branches
                        for existing in union_spread_branches.iter() {
                            let mut branch = existing.clone();
                            for prop in &member_props {
                                self.merge_spread_property(&mut branch, prop);
                            }
                            new_branches.push(branch);
                        }
                    }
                }
                *union_spread_branches = new_branches;
                // Clear main properties so post-union properties
                // don't include pre-union ones when applied at the end
                properties.clear();
            } else {
                // When the spread type is/contains a type parameter,
                // track it for intersection creation at the end.
                // This preserves generic identity so that return types
                // of generic functions are properly instantiated at
                // call sites. Without this, spreading a type parameter
                // resolves to constraint properties, losing the generic
                // information and causing false TS2741/TS2322 errors.
                let is_generic_spread = is_valid_spread
                    && (crate::query_boundaries::common::type_param_info(
                        self.ctx.types,
                        spread_type,
                    )
                    .is_some()
                        || crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            spread_type,
                        ));

                if is_generic_spread {
                    generic_spread_types.push(spread_type);
                }

                let resolved_spread = self.resolve_lazy_type(spread_type);
                let resolved_spread = self.evaluate_type_with_env(resolved_spread);
                let resolved_spread = self.resolve_type_for_property_access(resolved_spread);
                let spread_props = self.collect_object_spread_properties(resolved_spread);
                // In thisless generic option patterns, `this.options.foo` can
                // temporarily resolve to `any` even though the containing call
                // gives this literal a concrete contextual target. Use that
                // target only for TS2783 overwrite diagnostics; keep type
                // construction based on the actual spread source.
                let spread_props_for_overwrite = if spread_props.is_empty()
                    && spread_type == TypeId::ANY
                    && is_contextual_this_options_any_spread
                    && let Some(ctx_type) = contextual_type
                {
                    self.collect_object_spread_properties(ctx_type)
                } else {
                    spread_props.clone()
                };
                // Propagate index signatures from spread source.
                // When spreading an object with index signatures (e.g.,
                // `{ ...roindex }` where `roindex: { readonly [x: string]: number }`),
                // the result should inherit the index signatures (with readonly removed).
                // These are collected separately and only included in the final type
                // when the literal has no explicit (non-spread) properties, matching tsc.
                if (spread_props.is_empty()
                            || !self.spread_source_is_unannotated_object_literal_binding(spread_expr))
                            && !crate::query_boundaries::type_computation::core::is_fresh_literal_indexed_object(
                                self.ctx.types,
                                resolved_spread,
                            )
                        {
                            use crate::query_boundaries::common::IndexSignatureResolver;
                            let resolver = IndexSignatureResolver::new(self.ctx.types);
                            let index_info = resolver.get_index_info(resolved_spread);
                            if let Some(string_index) = index_info.string_index.or_else(|| {
                                resolver.resolve_string_index(resolved_spread).map(|value_type| {
                                    IndexSignature {
                                        key_type: TypeId::STRING,
                                        value_type,
                                        readonly: false,
                                        param_name: None,
                                    }
                                })
                            }) {
                                spread_string_index_signatures.push(string_index);
                            }
                            if let Some(number_index) = index_info.number_index.or_else(|| {
                                resolver.resolve_number_index(resolved_spread).map(|value_type| {
                                    IndexSignature {
                                        key_type: TypeId::NUMBER,
                                        value_type,
                                        readonly: false,
                                        param_name: None,
                                    }
                                })
                            }) {
                                spread_number_index_signatures.push(number_index);
                            }
                        }

                // TS2783: Check if any earlier named properties will be
                // overwritten by required properties from this spread.
                // Only when strict null checks are enabled.
                // TSC checks constraint properties even for generic spreads,
                // so we do too (unlike type construction, approximations are fine here).
                if self.ctx.strict_null_checks() {
                    for sp in &spread_props_for_overwrite {
                        if !sp.optional
                            && let Some((prop_node, prop_name)) = named_property_nodes.get(&sp.name)
                        {
                            let message = format_message(
                                        diagnostic_messages::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                        &[prop_name],
                                    );
                            self.error_at_node(
                                        *prop_node,
                                        &message,
                                        diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN,
                                    );
                        }
                    }
                }

                // After TS2783 check, clear the named-property tracking
                // for properties that the spread overwrites (so only the
                // first occurrence can trigger the diagnostic, not later
                // spreads which are spread-vs-spread and exempt).
                for prop in &spread_props_for_overwrite {
                    named_property_nodes.remove(&prop.name);
                }

                let spread_props_for_display =
                    rebase_spread_display_property_order(&spread_props, *spread_display_order_base);
                *spread_display_order_base =
                    (*spread_display_order_base).saturating_sub(SPREAD_DISPLAY_ORDER_STRIDE);
                for prop in &spread_props_for_display {
                    self.merge_spread_property(properties, prop);
                }

                // Also apply non-union spread to any existing union branches
                for branch in union_spread_branches.iter_mut() {
                    for prop in &spread_props_for_display {
                        self.merge_spread_property(branch, prop);
                    }
                }
            }
        }
        None
    }
}
