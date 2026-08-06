use crate::state::CheckerState;
use std::collections::HashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn try_union_index_signature_value_check(
        &mut self,
        source_props: &[tsz_solver::PropertyInfo],
        obj_literal_idx: NodeIndex,
        union_shapes: &[std::sync::Arc<tsz_solver::ObjectShape>],
        explicit_property_names: Option<&HashSet<Atom>>,
        union_target: TypeId,
    ) -> bool {
        let diag_count_before = self.ctx.diagnostics.len();

        for source_prop in source_props {
            if explicit_property_names.is_some()
                && !explicit_property_names
                    .as_ref()
                    .is_some_and(|names| names.contains(&source_prop.name))
            {
                continue;
            }

            // Named properties have their own union-member compatibility paths.
            // Keep this check scoped to properties whose only plausible union
            // acceptance is through an index signature.
            if union_shapes.iter().any(|shape| {
                shape
                    .properties
                    .iter()
                    .any(|target_prop| target_prop.name == source_prop.name)
            }) {
                continue;
            }

            let prop_name = self.ctx.types.resolve_atom(source_prop.name);
            let is_numeric_name = tsz_solver::utils::is_numeric_literal_name(&prop_name);

            // tsc `findBestTypeForObjectLiteral`: when the union target has an
            // array-like member, elaboration resolves the key against the first
            // non-array-like member. If that best match does not expose the key
            // (e.g. a leading primitive in a `Json`-style union), tsc reports the
            // outer relation error rather than checking the value against a
            // trailing index-signature member. Skip the index-signature drill-in
            // for those keys so the outer TS2322 with its member chain stands.
            if self.object_literal_union_skips_property_drill_in(
                union_target,
                prop_name.as_ref(),
                is_numeric_name,
                source_prop.is_symbol_named,
            ) {
                continue;
            }
            let mut applicable_index_value_types = Vec::new();
            let mut accepted_by_index = false;
            let mut has_deferred_index_value_type = false;

            for shape in union_shapes {
                // A `symbol`-keyed property is covered only by the target's
                // `[k: symbol]` signature (tsc `getApplicableIndexInfo`); the
                // `[k: string]`/`[k: number]` signatures this loop otherwise
                // selects never apply to it. Route the value check to the symbol
                // signature so a mismatch surfaces (TS2418) and a match is
                // accepted — and, when the target has no symbol signature, the
                // property is left uncovered (not excess), matching `tsc`. The
                // trailing `continue` keeps a symbol key from ever falling
                // through to the string/number branches below.
                if source_prop.is_symbol_named {
                    if let Some(symbol_index) = shape.symbol_index_signature()
                        && self.record_applicable_index_value(
                            source_prop.type_id,
                            symbol_index.value_type,
                            &mut applicable_index_value_types,
                            &mut has_deferred_index_value_type,
                        )
                    {
                        accepted_by_index = true;
                        break;
                    }
                    continue;
                }

                if let Some(string_index) = &shape.string_index {
                    if !self.string_index_key_accepts_property_name(
                        string_index.key_type,
                        prop_name.as_ref(),
                        source_prop.is_symbol_named,
                    ) {
                        continue;
                    }
                    if self.record_applicable_index_value(
                        source_prop.type_id,
                        string_index.value_type,
                        &mut applicable_index_value_types,
                        &mut has_deferred_index_value_type,
                    ) {
                        accepted_by_index = true;
                        break;
                    }
                }

                if is_numeric_name
                    && let Some(number_index) = &shape.number_index
                    && self.record_applicable_index_value(
                        source_prop.type_id,
                        number_index.value_type,
                        &mut applicable_index_value_types,
                        &mut has_deferred_index_value_type,
                    )
                {
                    accepted_by_index = true;
                    break;
                }
            }

            if accepted_by_index
                || applicable_index_value_types.is_empty()
                || has_deferred_index_value_type
            {
                continue;
            }

            let target_value_type =
                tsz_solver::utils::union_or_single(self.ctx.types, applicable_index_value_types);
            let evaluated_target_value_type = self.evaluate_type_with_env(target_value_type);
            if crate::query_boundaries::common::type_is_conditional_type_result_with_unresolved_inference(
                self.ctx.types,
                target_value_type,
            ) || crate::query_boundaries::common::type_is_conditional_type_result_with_unresolved_inference(
                self.ctx.types,
                evaluated_target_value_type,
            ) {
                continue;
            }
            if self
                .index_signature_relation_outcome(source_prop.type_id, target_value_type)
                .related
            {
                continue;
            }

            let report_idx = self
                .find_object_literal_property_element(obj_literal_idx, source_prop.name)
                .unwrap_or(obj_literal_idx);
            if let Some(nested_idx) = self.object_literal_property_initializer(report_idx) {
                let nested_idx = self.ctx.arena.skip_parenthesized(nested_idx);
                if self
                    .ctx
                    .arena
                    .get(nested_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
                {
                    let nested_request =
                        crate::context::TypingRequest::with_contextual_type(target_value_type);
                    let nested_source =
                        self.get_type_of_node_with_request(nested_idx, &nested_request);
                    let before_nested = self.ctx.diagnostics.len();
                    self.check_object_literal_excess_properties(
                        nested_source,
                        target_value_type,
                        nested_idx,
                    );
                    if self.ctx.diagnostics.len() > before_nested {
                        continue;
                    }
                }
            }
            let computed_property = self
                .ctx
                .arena
                .get(report_idx)
                .and_then(|node| self.ctx.arena.get_property_assignment(node))
                .map(|prop| (prop.name, prop.initializer))
                .or_else(|| {
                    self.object_literal_property_name_and_value(obj_literal_idx, source_prop.name)
                })
                .or_else(|| {
                    let obj_node = self.ctx.arena.get(obj_literal_idx)?;
                    let obj_lit = self.ctx.arena.get_literal_expr(obj_node)?;
                    obj_lit.elements.nodes.iter().rev().find_map(|&elem_idx| {
                        let elem_node = self.ctx.arena.get(elem_idx)?;
                        let prop = self.ctx.arena.get_property_assignment(elem_node)?;
                        let resolved = self.get_property_name_resolved(prop.name)?;
                        (self.ctx.types.intern_string(&resolved) == source_prop.name)
                            .then_some((prop.name, prop.initializer))
                    })
                });
            if let Some((prop_name_idx, prop_value_idx)) = computed_property
                && self
                    .ctx
                    .arena
                    .get(prop_name_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

                // This message renders the *widened* value type, so the raw
                // literal recovery (not the literal-preferring display rule)
                // feeds the widen below.
                let source_type = self
                    .literal_type_from_initializer(prop_value_idx)
                    .unwrap_or(source_prop.type_id);
                let source_type = self.widen_literal_type(source_type);
                let source_str = self.format_type_for_assignability_message(source_type);
                let target_str = self.format_type_for_assignability_message(target_value_type);
                let message = format_message(
                    diagnostic_messages::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                self.error_at_node(
                    prop_name_idx,
                    &message,
                    diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
                continue;
            }
            // tsc's `elaborateElementwise` descends into an elaboratable property
            // value matched here only through an index signature and anchors the
            // deepest leaf mismatch, exactly as it does for named-property targets
            // — it does not report the property-level aggregate. So
            // `{ [k: string]: { n: number } } = { foo: { n: "x" } }` reports
            // `string`->`number` at the inner `n`, not a redundant
            // `{ n: string }`->`{ n: number }` at `foo` the way
            // `_without_source_elaboration` would (and a fresh function value
            // anchors at its return expression). This routes through the same
            // `try_elaborate_assignment_source_error` gateway the named-property
            // path uses, with the identical elaboratable-source kinds; a
            // non-elaboratable value (a reference, a call result, ...) keeps the
            // property-level aggregate below, which is what tsc reports for it.
            if let Some(value_idx) = self.object_literal_property_initializer(report_idx) {
                let value_idx = self.ctx.arena.skip_parenthesized(value_idx);
                if self.ctx.arena.get(value_idx).is_some_and(|node| {
                    matches!(
                        node.kind,
                        syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            | syntax_kind_ext::ARROW_FUNCTION
                            | syntax_kind_ext::FUNCTION_EXPRESSION
                    )
                }) && self.try_elaborate_assignment_source_error(value_idx, target_value_type)
                {
                    continue;
                }
            }

            // tsc's `elaborateDidYouMeanToCallOrConstruct` applies to an
            // object-literal property value as well: in
            // `var b: { [x: string]: A } = { a: A }` the failure is reported at
            // the value `A`, not at the property name, because `typeof A` has a
            // construct signature whose return type satisfies `A`. Without this
            // the property name keeps the anchor.
            //
            // The return-type half of the predicate is load-bearing: anchoring
            // on "the value is callable" alone re-anchors values whose call
            // would not have helped, which tsc leaves on the property name.
            let mut anchor_idx = report_idx;
            if let Some(value_idx) = self.object_literal_property_initializer(report_idx)
                && crate::query_boundaries::assignability_did_you_mean::did_you_mean_call_or_construct(
                    self.ctx.types.as_type_database(),
                    source_prop.type_id,
                    target_value_type,
                )
            {
                anchor_idx = value_idx;
            }

            let _ = self.check_assignable_or_report_at_exact_anchor_without_source_elaboration(
                source_prop.type_id,
                target_value_type,
                anchor_idx,
                anchor_idx,
            );
        }

        self.ctx.diagnostics.len() > diag_count_before
    }

    /// Record `value_type` as an applicable index-signature value for the
    /// property under check, returning `true` when the property's type is
    /// assignable to it — i.e. an accepting signature was found and the caller
    /// should stop scanning. A deferred value type is noted in
    /// `has_deferred_index_value_type` and never counts as accepting. Shared by
    /// the symbol / string / number branches of the per-shape scan.
    fn record_applicable_index_value(
        &mut self,
        source_type: TypeId,
        value_type: TypeId,
        applicable_index_value_types: &mut Vec<TypeId>,
        has_deferred_index_value_type: &mut bool,
    ) -> bool {
        if self.index_value_type_is_deferred(value_type) {
            *has_deferred_index_value_type = true;
            return false;
        }
        applicable_index_value_types.push(value_type);
        self.index_signature_relation_outcome(source_type, value_type)
            .related
    }
}
