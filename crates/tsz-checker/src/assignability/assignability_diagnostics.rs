use crate::query_boundaries::assignability::{
    AssignabilityQueryInputs, ExcessPropertiesKind, check_assignable_gate_with_overrides,
    classify_for_excess_properties, get_keyof_type, get_string_literal_value, is_keyof_type,
    is_type_parameter_like, object_shape_for_type, suppress_raw_excess_property_failure_if_needed,
};
use crate::query_boundaries::diagnostics as assignability_diagnostic_common;
use crate::query_boundaries::diagnostics::type_param_info;
use crate::query_boundaries::enum_analysis::{self as enum_query, NumericEnumAssignmentTarget};
use crate::query_boundaries::relation_types::RelationFailure;
use crate::state::{CheckerOverrideProvider, CheckerState};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

mod argument_reports;
mod display_types;
mod explicit_any_annotations;
mod generic_argument_suppression;
mod type_comparability;

impl<'a> CheckerState<'a> {
    pub(crate) fn generic_indexed_access_argument_surface(&self, type_id: TypeId) -> bool {
        self.generic_indexed_access_argument_surface_inner(type_id)
            || self
                .ctx
                .types
                .get_display_alias(type_id)
                .is_some_and(|alias| self.generic_indexed_access_argument_surface_inner(alias))
    }

    fn generic_indexed_access_argument_surface_inner(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::diagnostics::contains_generic_indexed_access_surface(
            self.ctx.types,
            type_id,
        )
    }

    /// The normalized target surfaces (self, contextual, property-access, and
    /// assignability evaluations) that the assignment-diagnostic query boundary
    /// inspects. Shared by `target_prefers_outer_assignment_diagnostic` and
    /// `target_has_deferred_evaluation_surface` so both look at the same set.
    fn target_evaluation_candidates(&mut self, target: TypeId) -> Vec<TypeId> {
        vec![
            target,
            self.evaluate_contextual_type(target),
            self.resolve_type_for_property_access(target),
            self.evaluate_type_for_assignability(target),
        ]
    }

    fn target_prefers_outer_assignment_diagnostic(&mut self, target: TypeId) -> bool {
        let candidates = self.target_evaluation_candidates(target);
        crate::query_boundaries::assignability::target_prefers_outer_assignment_diagnostic(
            self.ctx.types,
            &self.ctx,
            &candidates,
        )
    }

    /// Whether the relation failed at a concrete *member* of the source/target
    /// shape — a missing required property, or a present property whose value
    /// type is incompatible.
    ///
    /// `tsc` always drills into such failures (`Types of property 'x' are
    /// incompatible.` and the nested root reason), even when the target is a
    /// generic application like `A<T>`. The coarse "outer assignment" path
    /// (`target_prefers_outer_assignment_diagnostic`) is meant only for the
    /// type-argument / indexed-access / conditional / mapped surfaces where
    /// drilling into the evaluated shape would be misleading (e.g. same-generic
    /// `C<A>` vs `C<B>`); a genuine member mismatch must keep its elaboration.
    /// The rich `analyze_assignability_failure` path already produces the
    /// correct reason — the structural property chain for a plain-object source
    /// and the direct type-argument reason for a same-generic application.
    const fn should_preserve_structural_property_diagnostic(
        &self,
        outcome: &crate::query_boundaries::assignability::RelationOutcome,
    ) -> bool {
        matches!(
            outcome.failure,
            Some(RelationFailure::MissingProperty { .. })
                | Some(RelationFailure::MissingProperties { .. })
                | Some(RelationFailure::IncompatiblePropertyValue { .. })
                | Some(RelationFailure::IndexAccessTypeParameterMismatch { .. })
        )
    }

    /// Whether the assignment/return source expression is (after stripping
    /// parentheses and type assertions) a bare object- or array-literal — the
    /// only source shape `tsc`'s `elaborateObjectLiteral`/`elaborateArrayLiteral`
    /// drills into. Used to keep per-property source elaboration scoped to fresh
    /// literals so non-literal sources (identifiers, call results) keep the outer
    /// diagnostic and its relation-reason chain intact.
    fn assignment_source_is_object_or_array_literal(&self, source_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(source_idx);
        self.ctx.arena.get(expr_idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        })
    }

    /// Whether the target carries a deferred evaluation surface (generic indexed
    /// access / mapped / conditional-with-type-params). See
    /// `query_boundaries::assignability::target_has_deferred_evaluation_surface`.
    /// Fresh-literal per-property elaboration is skipped on such targets because
    /// `tsc` cannot resolve their members concretely and keeps the outer
    /// whole-object diagnostic.
    fn target_has_deferred_evaluation_surface(&mut self, target: TypeId) -> bool {
        let candidates = self.target_evaluation_candidates(target);
        crate::query_boundaries::assignability::target_has_deferred_evaluation_surface(
            self.ctx.types,
            &self.ctx,
            &candidates,
        )
    }

    fn excess_property_target_score(&self, type_id: TypeId) -> (u8, usize) {
        match classify_for_excess_properties(self.ctx.types, type_id) {
            ExcessPropertiesKind::NotObject => (0, 0),
            ExcessPropertiesKind::Object(shape_id)
            | ExcessPropertiesKind::ObjectWithIndex(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                let structural_slots = shape.properties.len()
                    + usize::from(shape.string_index.is_some())
                    + usize::from(shape.number_index.is_some());
                let rank = if structural_slots == 0 { 1 } else { 2 };
                (rank, structural_slots)
            }
            ExcessPropertiesKind::Union(members) | ExcessPropertiesKind::Intersection(members) => {
                (3, members.len())
            }
        }
    }

    pub(crate) fn normalized_target_for_excess_properties(&mut self, target: TypeId) -> TypeId {
        let resolved = self.resolve_type_for_property_access(target);
        let evaluated = self.judge_evaluate(resolved);
        let contextual = self.evaluate_contextual_type(target);

        let mut best = resolved;
        let mut best_score = self.excess_property_target_score(resolved);

        for candidate in [evaluated, contextual, target] {
            if candidate == best {
                continue;
            }
            let score = self.excess_property_target_score(candidate);
            if score > best_score {
                best = candidate;
                best_score = score;
            }
        }

        best
    }

    /// Check if we should skip the general assignability error for an object literal.
    /// Returns true if:
    /// 1. It's a weak union violation (TypeScript shows excess property error instead)
    /// 2. OR if the object literal has excess properties (TypeScript prioritizes TS2353 over TS2345/TS2322)
    pub(crate) fn should_skip_weak_union_error(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        self.should_skip_weak_union_error_with_outcome(source, target, source_idx, None)
    }

    /// Alias for `should_skip_weak_union_error_with_outcome` — kept for
    /// architecture contract test compatibility.
    #[expect(dead_code)]
    pub(crate) fn should_skip_weak_union_error_with_hint(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        _weak_union_hint: Option<bool>,
    ) -> bool {
        self.should_skip_weak_union_error_with_outcome(source, target, source_idx, None)
    }

    /// Like `should_skip_weak_union_error`, but uses a pre-computed
    /// `RelationOutcome` from a prior boundary call to avoid redundant
    /// property enumeration and compatibility checks.
    ///
    /// When `outcome` is `Some`, this uses:
    /// - `outcome.weak_union_violation` instead of calling `is_weak_union_violation`
    /// - `outcome.property_classification` instead of re-enumerating source/target
    ///   properties and re-checking assignability
    pub(crate) fn should_skip_weak_union_error_with_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        outcome: Option<&crate::query_boundaries::assignability::RelationOutcome>,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(source_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        // Check for weak union violation — use the outcome when available
        // to avoid an extra solver round-trip.
        let is_weak_union = outcome
            .map(|o| o.weak_union_violation)
            .unwrap_or_else(|| self.is_weak_union_violation(source, target));
        // TS2559 takes priority over TS2353 — do not skip when a weak-type violation applies.
        if is_weak_union {
            return false;
        }

        // Use the canonical property classification from the RelationOutcome
        // to decide if the failure is caused ONLY by excess properties.
        // This replaces the previous checker-local property enumeration and
        // per-property assignability re-checking.
        if let Some(outcome) = outcome
            && let Some(ref cls) = outcome.property_classification
        {
            // No excess properties → don't skip
            if cls.excess_properties.is_empty() {
                return false;
            }
            // Has excess properties AND all matching ones are compatible
            // AND trimmed source is structurally assignable → skip
            if cls.all_matching_compatible && cls.trimmed_source_assignable {
                return true;
            }
            // Has incompatible matching properties → don't skip
            return false;
        }

        // No pre-computed outcome available. Build one through the canonical
        // boundary so we never fall back to checker-local property enumeration.
        let built_outcome = self.assignability_reason_relation_outcome(source, target);
        if let Some(ref cls) = built_outcome.property_classification {
            if cls.excess_properties.is_empty() {
                return false;
            }
            if cls.all_matching_compatible && cls.trimmed_source_assignable {
                return true;
            }
            return false;
        }
        // No property classification available (e.g., non-object types) → don't skip
        false
    }

    /// Run excess property checking when `source` is a fresh object literal or fresh object type.
    /// Returns `true` if an excess-property diagnostic was emitted.
    fn check_excess_properties_for_fresh_source(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        let is_direct_literal = self
            .ctx
            .arena
            .get(source_idx)
            .is_some_and(|n| n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION);
        let is_fresh =
            crate::query_boundaries::diagnostics::is_fresh_object_type(self.ctx.types, source);
        if !(is_direct_literal || is_fresh) {
            return false;
        }
        let node_idx = if is_direct_literal {
            source_idx
        } else {
            // Fresh type from a non-literal expression (e.g. `return obj = { x: 1, y: 2 }`):
            // walk through binary assignment expressions to find the object literal.
            self.find_rhs_object_literal(source_idx)
                .unwrap_or(source_idx)
        };
        let diags_before = self.ctx.diagnostics.len();
        self.check_object_literal_excess_properties(source, target, node_idx);
        self.ctx.diagnostics.len() > diags_before
    }

    /// Check assignability and emit the standard TS2322/TS2345-style diagnostic when needed.
    /// `keyword_pos` is the source position of the `satisfies` keyword for accurate TS1360 spans.
    pub(crate) fn check_satisfies_assignable_or_report(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        keyword_pos: Option<u32>,
    ) -> bool {
        let diag_idx = source_idx;
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }
        let evaluated_target_for_invalid_mapped = self.evaluate_type_for_assignability(target);
        if self.type_contains_invalid_mapped_key_type(target)
            || self.type_contains_invalid_mapped_key_type(evaluated_target_for_invalid_mapped)
        {
            return true;
        }

        if is_keyof_type(self.ctx.types, target)
            && let Some(str_lit) = get_string_literal_value(self.ctx.types, source)
        {
            let keyof_type = get_keyof_type(self.ctx.types, target)
                .expect("is_keyof_type guard ensures this succeeds");
            let allowed_keys = self.get_keyof_type_keys(keyof_type, self.ctx.types);
            // Only use this pre-check when we could determine concrete keys.
            // An empty set means the inner type couldn't be resolved (e.g., ThisType,
            // Application, or Lazy reference). Fall through to the solver check.
            if !allowed_keys.is_empty() && !allowed_keys.contains(&str_lit) {
                self.error_type_does_not_satisfy_the_expected_type(
                    source,
                    target,
                    diag_idx,
                    keyword_pos,
                );
                return false;
            }
        }

        // TS2353 and TS1360 are mutually exclusive; skip TS1360 when EPC fires.
        let had_excess_property_error =
            self.check_excess_properties_for_fresh_source(source, target, source_idx);

        // Use the canonical satisfies relation outcome so the weak-union hint is collected
        // alongside the failure reason, avoiding a redundant solver round-trip in
        // should_skip_weak_union_error's fallback path.
        let outcome = self.satisfies_relation_outcome(source, target);
        if outcome.related {
            return true;
        }
        if self.is_nested_same_wrapper_application_assignment(source, target) {
            return true;
        }

        if self.should_skip_weak_union_error_with_outcome(
            source,
            target,
            source_idx,
            Some(&outcome),
        ) {
            return true;
        }
        if outcome.weak_union_violation {
            self.error_no_common_properties(source, target, diag_idx);
            return false;
        }

        // tsc 6.0: `satisfies` ignores readonly-to-mutable mismatches.
        // `[1,2,3] as const satisfies unknown[]` is accepted because `satisfies`
        // checks structural shape, not mutability. If the source is Readonly<T>,
        // try checking T against the target.
        if let Some(inner) =
            crate::query_boundaries::diagnostics::readonly_inner_type(self.ctx.types, source)
            && self.satisfies_relation_outcome(inner, target).related
        {
            return true;
        }

        // If excess property errors were already emitted, skip the general TS1360.
        // This matches tsc: when TS2353 is reported, the "does not satisfy" error
        // is suppressed to avoid redundant diagnostics.
        if had_excess_property_error {
            return false;
        }

        // Elaborate: for object literal sources, drill into property-level errors
        // instead of reporting the generic TS1360. This matches tsc behavior where
        // `{ s: "false" } satisfies { [key: string]: boolean }` reports TS2322 at
        // the specific mismatching property rather than TS1360 on the whole expression.
        if let Some(node) = self.ctx.arena.get(source_idx)
            && node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            let elaborated =
                self.elaborate_satisfies_object_literal(source, target, source_idx, keyword_pos);
            if elaborated {
                return false;
            }
        }

        // For the other fresh source kinds that tsc's `elaborateError` drills into
        // — array literals and expression-bodied arrow / function expressions —
        // route through the same assignment-source elaboration boundary the
        // direct-assignment path uses. tsc runs the identical `elaborateError` for
        // a `satisfies` operand as for an assignment (only the outer error code and
        // keyword anchor differ), so `[10, "20"] satisfies number[]` reports TS2322
        // at the offending element and `(() => 1) satisfies () => string` reports
        // TS2322 at the arrow's returned expression, instead of the coarse
        // whole-expression TS1360. Block-bodied functions (where tsc keeps the
        // coarse TS1360) and every non-drilling source fall through to the TS1360
        // report below. (Object-literal sources took the dedicated
        // `elaborate_satisfies_object_literal` path above.)
        if self.satisfies_source_drills_to_inner_anchor(source_idx)
            && self.try_elaborate_assignment_source_error(source_idx, target)
        {
            return false;
        }

        self.error_type_does_not_satisfy_the_expected_type(source, target, diag_idx, keyword_pos);
        false
    }

    /// Whether a `satisfies` source expression drills into an inner node under
    /// tsc's `elaborateElementwise` / `elaborateArrowFunction`, producing a
    /// nested `TS2322` anchor (rather than the coarse whole-expression frame).
    ///
    /// Object and array literals always drill (into a property or an element).
    /// A function expression drills only when its body is an *expression* — tsc's
    /// `elaborateArrowFunction` bails on a block body and keeps the
    /// function-level frame, so a block-bodied arrow/function value must fall
    /// through to the coarse report (`TS1360` at the keyword for a direct source,
    /// or the function-level `TS2322` at the property name for an object-literal
    /// member) instead of being drilled here. Parentheses and assertions are
    /// transparent, matching the boundary's own `skip_parenthesized_and_assertions`.
    fn satisfies_source_drills_to_inner_anchor(&self, source_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(source_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        match node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                true
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                self.ctx
                    .arena
                    .get_function(node)
                    .and_then(|func| self.ctx.arena.get(func.body))
                    .is_some_and(|body| body.kind != syntax_kind_ext::BLOCK)
            }
            _ => false,
        }
    }

    /// Elaborate a `satisfies` failure for object literal expressions by checking
    /// each property against the target type's index signature or named properties.
    /// Returns true if elaboration produced property-level diagnostics.
    fn elaborate_satisfies_object_literal(
        &mut self,
        _source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        _keyword_pos: Option<u32>,
    ) -> bool {
        let resolved_target = self.normalized_target_for_excess_properties(target);
        let target_shape = match object_shape_for_type(self.ctx.types, resolved_target) {
            Some(shape) => shape,
            None => return false,
        };

        let index_value_type = target_shape.string_index.as_ref().map(|sig| sig.value_type);

        // Iterate over the object literal's AST properties and check each value
        let Some(lit_data) = self.ctx.arena.get_literal_expr_at(source_idx) else {
            return false;
        };
        let elements: Vec<NodeIndex> = lit_data.elements.nodes.to_vec();

        let diag_count_before = self.ctx.diagnostics.len();

        for &elem_idx in &elements {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            let (prop_name_idx, prop_value_idx) = match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                        continue;
                    };
                    (prop.name, prop.initializer)
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_shorthand_property(elem_node) else {
                        continue;
                    };
                    (prop.name, prop.name)
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(elem_node) else {
                        continue;
                    };
                    (method.name, elem_idx)
                }
                _ => continue,
            };
            let target_prop_type = self
                .get_property_name(prop_name_idx)
                .and_then(|name| {
                    let name_atom = self.ctx.types.intern_string(&name);
                    target_shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == name_atom)
                        .map(|prop| {
                            if prop.write_type == TypeId::NONE {
                                prop.type_id
                            } else {
                                prop.write_type
                            }
                        })
                })
                .or(index_value_type);
            let Some(target_prop_type) = target_prop_type else {
                continue;
            };

            // Get the type of the property value (the initializer)
            let prop_value_type = self.get_type_of_node(prop_value_idx);
            self.ensure_relation_input_ready(prop_value_type);
            self.ensure_relation_input_ready(target_prop_type);

            // Check nested object literal excess properties FIRST — tsc prioritizes
            // excess property errors (TS2353) over assignability errors (TS2322).
            // e.g., `{ r: 0, g: 0, d: 0 }` vs `Color` reports "d does not exist" (TS2353)
            // rather than "missing b" (TS2322).
            if let Some(val_node) = self.ctx.arena.get(prop_value_idx)
                && val_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            {
                let evaluated_target_prop_type = self.evaluate_type_with_env(target_prop_type);
                if crate::query_boundaries::diagnostics::type_is_conditional_type_result_with_unresolved_inference(
                    self.ctx.types,
                    target_prop_type,
                ) || crate::query_boundaries::diagnostics::type_is_conditional_type_result_with_unresolved_inference(
                    self.ctx.types,
                    evaluated_target_prop_type,
                ) {
                    continue;
                }

                let diags_before = self.ctx.diagnostics.len();
                self.check_object_literal_excess_properties(
                    prop_value_type,
                    target_prop_type,
                    prop_value_idx,
                );
                if self.ctx.diagnostics.len() > diags_before {
                    // Excess property errors were reported — skip assignability check
                    continue;
                }
            }

            // tsc's `elaborateElementwise` recurses into a property value that is
            // itself a fresh object/array literal or an expression-bodied arrow,
            // anchoring the mismatch at the innermost node — a nested property, an
            // array element, or the arrow's returned expression — rather than
            // reporting the whole property-value type. Route those values through
            // the same assignment-source elaboration boundary the direct-assignment
            // path uses so `satisfies` and assignment stay in lockstep. It is
            // relation-gated (emits only on a genuine nested mismatch), so a passing
            // value falls through to the leaf report below. Values that do not drill
            // (a leaf primitive, a block-bodied function whose function-level frame
            // tsc keeps at the property name) are excluded so the leaf report owns
            // their anchor and elaboration chain.
            if self.satisfies_source_drills_to_inner_anchor(prop_value_idx)
                && self.try_elaborate_assignment_source_error(prop_value_idx, target_prop_type)
            {
                continue;
            }

            let _ = self.check_assignable_or_report_at_exact_anchor_without_source_elaboration(
                prop_value_type,
                target_prop_type,
                prop_value_idx,
                prop_name_idx,
            );
        }

        self.ctx.diagnostics.len() > diag_count_before
    }

    ///
    /// Returns true when no diagnostic was emitted (assignable or intentionally skipped),
    /// false when an assignability diagnostic was emitted.
    pub(crate) fn check_assignable_or_report(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        self.check_assignable_or_report_at(source, target, source_idx, source_idx)
    }

    /// Check assignability and emit TS2322/TS2345-style diagnostics with independent
    /// source and diagnostic anchors.
    ///
    /// `source_idx` is used for weak-union/excess-property prioritization.
    /// `diag_idx` is where the assignability diagnostic is anchored.
    ///
    /// Uses the canonical `RelationRequest` / `RelationOutcome` boundary path
    /// so that the assignability check and failure analysis happen in a single
    /// solver round-trip rather than separate calls.
    pub(crate) fn check_assignable_or_report_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        self.check_assignable_or_report_at_with_options(source, target, source_idx, diag_idx, false)
    }

    /// Same as `check_assignable_or_report_at`, but skips deep assignment
    /// source elaboration so failures are reported at the enclosing source
    /// context rather than a nested property/element node.
    pub(crate) fn check_assignable_or_report_at_without_source_elaboration(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        self.check_assignable_or_report_at_with_options(source, target, source_idx, diag_idx, true)
    }

    /// For JSX callback props: checks `source` against `target`, anchors the error at
    /// `diag_idx` without source elaboration, and uses `source & target` as the
    /// display target type.
    ///
    /// tsc shows both the inferred callback type (source) and the expected prop type
    /// in an intersection when a JSX function-valued attribute fails the assignability
    /// check. Skipping source elaboration keeps the diagnostic at the attribute name
    /// instead of drilling into the lambda body.
    pub(crate) fn check_assignable_or_report_jsx_callback_prop_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }
        if self.jsx_props_relation_outcome(source, target).related {
            return true;
        }
        let display_target = self.ctx.types.intersect_types_raw2(source, target);
        self.error_type_not_assignable_at_with_display_types_widened(
            source,
            display_target,
            diag_idx,
        );
        false
    }

    fn check_assignable_or_report_at_with_options(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
        skip_source_elaboration: bool,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        let force_nested_error_nullish_report =
            self.should_report_nullish_assignment_through_nested_target_error(source, target);
        let exact_optional_mismatch = self.has_exact_optional_property_mismatch(source, target);
        if let Some(reason) = self.readonly_to_mutable_array_or_tuple_reason(source, target) {
            self.error_type_not_assignable_with_reason_and_display(
                source, target, &reason, diag_idx,
            );
            return false;
        }
        if self.same_base_application_to_constrained_type_param_target(source, target) {
            self.error_type_not_assignable_with_reason_at(source, target, diag_idx);
            return false;
        }
        if self
            .ctx
            .arena
            .get(self.ctx.arena.skip_parenthesized_and_assertions(source_idx))
            .is_some_and(|node| node.kind == tsz_scanner::SyntaxKind::Identifier as u16)
            && self.try_report_concrete_remapped_mapped_missing_property(source, target, diag_idx)
        {
            return false;
        }
        if self.same_base_generic_mapped_application_variance_accepts(source, target) {
            return true;
        }
        {
            let flags = self.ctx.pack_relation_flags();
            let inputs = crate::query_boundaries::assignability::AssignabilityQueryInputs {
                db: self.ctx.types,
                resolver: &self.ctx,
                source,
                target,
                flags,
                inheritance_graph: &self.ctx.inheritance_graph,
                sound_mode: self.ctx.sound_mode(),
                evaluation_session: Some(self.ctx.eval_session.as_ref()),
            };
            if matches!(
                crate::query_boundaries::assignability::check_application_variance_assignability(
                    &inputs,
                ),
                Some(false)
            ) {
                if self.same_base_generic_mapped_application_has_type_param_arg(source, target) {
                    // A negative public variance prepass is not definitive for
                    // generic mapped aliases. Let the ordinary structural path
                    // decide so opposite-direction mapped relations still fail.
                } else if self.same_type_alias_application_uses_conditional_infer(source, target) {
                    let outcome = self.assignability_reason_relation_outcome(source, target);
                    if outcome.related {
                        return true;
                    }
                } else {
                    // The public-variance prepass determined these two
                    // instantiations of the same generic base are not
                    // assignable. tsc elaborates the failing type argument
                    // (`TypeArgumentMismatch`) under the top-line TS2322 — e.g.
                    //   Type 'Box<string>' is not assignable to type 'Box<number>'.
                    //     Type 'string' is not assignable to type 'number'.
                    // Route through the reason-bearing emitter so the nested
                    // relation reason is rendered, matching the return/argument
                    // (TS2345) paths instead of dropping it. If that path
                    // suppresses (the full structural relation disagrees with the
                    // variance prepass), preserve the prepass decision with the
                    // bare top-line diagnostic.
                    let diags_before = self.ctx.diagnostics.len();
                    self.error_type_not_assignable_with_reason_at(source, target, diag_idx);
                    if self.ctx.diagnostics.len() == diags_before {
                        self.error_type_not_assignable_at_with_raw_display_types(
                            source, target, diag_idx,
                        );
                    }
                    return false;
                }
            }
        }
        if !force_nested_error_nullish_report
            && !exact_optional_mismatch
            && self.should_suppress_assignability_diagnostic(source, target)
        {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }
        if force_nested_error_nullish_report {
            self.error_type_not_assignable_with_reason_at(source, target, diag_idx);
            return false;
        }

        if is_keyof_type(self.ctx.types, target)
            && let Some(str_lit) = get_string_literal_value(self.ctx.types, source)
        {
            let keyof_type = get_keyof_type(self.ctx.types, target)
                .expect("is_keyof_type guard ensures this succeeds");
            let allowed_keys = self.get_keyof_type_keys(keyof_type, self.ctx.types);
            // Only use this pre-check when we could determine concrete keys.
            // An empty set means the inner type couldn't be resolved (e.g., it's
            // an Application, Mapped type with as-clause, or Lazy reference).
            // In that case, fall through to the solver's assignability check which
            // correctly evaluates keyof through the full type evaluation pipeline.
            if !allowed_keys.is_empty() && !allowed_keys.contains(&str_lit) {
                self.error_type_not_assignable_with_reason_at(source, target, diag_idx);
                return false;
            }
        }

        if let Some(allowed) =
            self.numeric_enum_assignment_override_from_source(source, target, source_idx)
        {
            if allowed {
                return true;
            }
            if self.try_elaborate_assignment_source_error(source_idx, target) {
                return false;
            }
            self.error_type_not_assignable_with_reason_at(source, target, diag_idx);
            return false;
        }

        // Check excess properties on fresh object types BEFORE the assignability check.
        // Fresh types from chained assignments (e.g. `return obj = { x: 1, y: 2 }`)
        // are structurally assignable but should still trigger TS2353.
        let had_excess_property_error =
            self.check_excess_properties_for_fresh_source(source, target, source_idx);
        if had_excess_property_error {
            return false;
        }

        // Reset overflow flags before the assignability check so we detect fresh
        // exceedance from this particular relation rather than a prior one.
        self.ctx
            .relation_overflow
            .set(crate::context::RelationOverflowFlags::default());
        let outcome = self.assignability_reason_relation_outcome(source, target);
        let assignable = outcome.related;
        // tsc emits TS2859 ("Excessive complexity") for all relation-checker
        // overflows regardless of whether it was depth or iteration that fired.
        // TS2321 ("Excessive stack depth") fires from a separate mechanism.
        if !assignable && self.ctx.relation_overflow.get().has_overflow() {
            if crate::query_boundaries::assignability::intersection_source_contains_target_member(
                self.ctx.types,
                &self.ctx,
                source,
                target,
            ) {
                return true;
            }
            let source_name = self.format_type_diagnostic(source);
            let target_name = self.format_type_diagnostic(target);
            self.error_at_node(
                diag_idx,
                &format!(
                    "Excessive complexity comparing types '{source_name}' and '{target_name}'."
                ),
                crate::diagnostics::diagnostic_codes::EXCESSIVE_COMPLEXITY_COMPARING_TYPES_AND,
            );
            return false;
        }

        if exact_optional_mismatch {
            self.diagnose_assignment_failure(source, target, diag_idx);
            return false;
        }

        if assignable {
            if self.has_explicit_any_generic_variable_annotation(diag_idx)
                && self.emit_polymorphic_this_property_assignment_error(source, target, diag_idx)
            {
                return false;
            }
            if self.emit_polymorphic_this_call_assignment_error(source_idx, target, diag_idx) {
                return false;
            }
            return true;
        }
        // Use the pre-computed RelationOutcome to avoid re-enumerating
        // properties and re-checking assignability inside the skip logic.
        if self.should_skip_weak_union_error_with_outcome(
            source,
            target,
            source_idx,
            Some(&outcome),
        ) {
            return true;
        }
        // Weak union violation for non-object-literal sources → emit TS2559
        // instead of the general TS2322/TS2345 error.
        if outcome.weak_union_violation {
            // tsc keeps the literal (e.g. `"A"` not `string`) in the TS2559
            // source slot even when the source has been widened upstream.
            let display_source =
                self.expression_display_type_preferring_literal(source_idx, source);
            self.error_no_common_properties(display_source, target, diag_idx);
            return false;
        }
        if let Some(display_source) =
            self.parameter_type_param_display_source_for_variadic_tuple(source, target, source_idx)
        {
            self.error_type_not_assignable_at_with_raw_display_types(
                display_source,
                target,
                diag_idx,
            );
            return false;
        }
        if !skip_source_elaboration
            && !self.target_prefers_outer_assignment_diagnostic(target)
            && self.try_elaborate_assignment_source_error(source_idx, target)
        {
            return false;
        }
        if self.target_prefers_outer_assignment_diagnostic(target)
            && !self.should_preserve_structural_property_diagnostic(&outcome)
            && self
                .missing_required_properties_from_index_signature_source(source, target)
                .is_none()
        {
            self.error_type_not_assignable_at_with_display_types(source, target, diag_idx);
        } else {
            self.error_type_not_assignable_with_reason_at(source, target, diag_idx);
        }
        false
    }

    fn parameter_type_annotation_anchor_for_identifier_source(
        &self,
        source_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let source_idx = self.ctx.arena.skip_parenthesized_and_assertions(source_idx);
        let source_node = self.ctx.arena.get(source_idx)?;
        if source_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(source_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut decl_idx = symbol.value_declaration;
        if let Some(decl_node) = self.ctx.arena.get(decl_idx)
            && decl_node.kind == SyntaxKind::Identifier as u16
            && let Some(ext) = self.ctx.arena.get_extended(decl_idx)
            && ext.parent.is_some()
        {
            decl_idx = ext.parent;
        }

        let decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::PARAMETER {
            return None;
        }
        let annotation = self.ctx.arena.get_parameter(decl_node)?.type_annotation;
        annotation.is_some().then_some(annotation)
    }

    pub(crate) fn error_type_not_assignable_at_with_raw_display_types(
        &mut self,
        source_for_display: TypeId,
        target_for_display: TypeId,
        anchor_idx: NodeIndex,
    ) {
        let source_str = self.format_type_diagnostic(source_for_display);
        let target_str = self.format_type_diagnostic(target_for_display);
        let message = crate::diagnostics::format_message(
            crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        self.error_at_node(
            anchor_idx,
            &message,
            crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
    }

    fn parameter_type_param_display_source_for_variadic_tuple(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> Option<TypeId> {
        let evaluated_target = self.evaluate_type_for_assignability(target);
        let target_has_single_rest_tuple = |ty| {
            crate::query_boundaries::diagnostics::tuple_elements(self.ctx.types, ty)
                .is_some_and(|elements| elements.len() == 1 && elements[0].rest)
        };
        if !target_has_single_rest_tuple(target) && !target_has_single_rest_tuple(evaluated_target)
        {
            return None;
        }

        let source_idx = self.ctx.arena.skip_parenthesized_and_assertions(source_idx);
        let source_node = self.ctx.arena.get(source_idx)?;
        if source_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(source_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut decl_idx = symbol.value_declaration;
        if let Some(decl_node) = self.ctx.arena.get(decl_idx)
            && decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(ext) = self.ctx.arena.get_extended(decl_idx)
            && ext.parent.is_some()
        {
            decl_idx = ext.parent;
        }

        let decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::PARAMETER {
            return None;
        }
        let annotation = self.ctx.arena.get_parameter(decl_node)?.type_annotation;
        if annotation.is_none() {
            return None;
        }

        if is_type_parameter_like(self.ctx.types, source) {
            return Some(source);
        }
        let display_source = self.get_type_from_type_node(annotation);
        let param_info = type_param_info(self.ctx.types, display_source)?;
        let constraint = param_info.constraint?;
        let evaluated_constraint = self.evaluate_type_for_assignability(constraint);
        if source == display_source || source == constraint || source == evaluated_constraint {
            Some(display_source)
        } else {
            None
        }
    }

    pub(crate) fn numeric_enum_assignment_override_from_source(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> Option<bool> {
        let target = self.evaluate_type_for_assignability(target);
        let target_fact = enum_query::numeric_enum_assignment_target(&self.ctx, target)?;

        let source_literal = self.literal_type_from_initializer(source_idx);
        let source_is_number_like = source == TypeId::NUMBER
            || source_literal
                .and_then(|lit| enum_query::numeric_literal_value(self.ctx.types, lit))
                .is_some();
        if !source_is_number_like {
            return None;
        }

        match target_fact {
            NumericEnumAssignmentTarget::Enum { structural_target } => {
                if let Some(source_literal) = source_literal {
                    return Some(
                        self.numeric_enum_assignment_relation_outcome(
                            source_literal,
                            structural_target,
                        )
                        .related,
                    );
                }
                None
            }
            NumericEnumAssignmentTarget::Member { target_literal } => match source_literal {
                Some(source_literal) => {
                    let source_val =
                        enum_query::numeric_literal_value(self.ctx.types, source_literal);
                    Some(source_val == Some(target_literal))
                }
                None => (source == TypeId::NUMBER).then_some(true),
            },
        }
    }

    /// Check assignability and emit TS2322/TS2345-style diagnostics anchored
    /// exactly at `diag_idx`, without assignment-anchor rewriting.
    pub(crate) fn check_assignable_or_report_at_exact_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        let force_nested_error_nullish_report =
            self.should_report_nullish_assignment_through_nested_target_error(source, target);
        if !force_nested_error_nullish_report
            && self.should_suppress_assignability_diagnostic(source, target)
        {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }
        if force_nested_error_nullish_report {
            self.error_type_not_assignable_with_reason_at_anchor(source, target, diag_idx);
            return false;
        }
        if let Some(allowed) =
            self.numeric_enum_assignment_override_from_source(source, target, source_idx)
        {
            if allowed {
                return true;
            }
            self.error_type_not_assignable_with_reason_at_anchor(source, target, diag_idx);
            return false;
        }
        let outcome = self.assignability_reason_relation_outcome(source, target);
        if outcome.related {
            return true;
        }
        if self.is_nested_same_wrapper_application_assignment(source, target) {
            return true;
        }

        // TS2589: A homomorphic self-referential mapped-type alias applied to a tuple
        // argument and checked against a tuple target causes infinite instantiation.
        // tsc detects the depth limit during instantiation and emits TS2589 here
        // instead of letting the structural check fall through to TS2322.
        if self.source_is_homomorphic_self_mapped_tuple_arg_vs_tuple_target(source, target) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            let anchor_idx = self
                .parameter_type_annotation_anchor_for_identifier_source(source_idx)
                .unwrap_or(source_idx);
            self.error_at_node(
                anchor_idx,
                diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
            );
            return false;
        }

        if self.should_skip_weak_union_error_with_outcome(
            source,
            target,
            source_idx,
            Some(&outcome),
        ) {
            return true;
        }
        if outcome.weak_union_violation {
            self.error_no_common_properties(source, target, diag_idx);
            return false;
        }

        // `tsc`'s `elaborateObjectLiteral` drills a **fresh object/array-literal**
        // source into per-property errors whenever the failure is a genuine
        // member mismatch against a target whose members resolve concretely —
        // including a *plain* generic interface/object application like `A<T>`.
        // The coarse `target_prefers_outer_assignment_diagnostic` gate lumps such
        // plain applications in with the genuinely-deferred surfaces
        // (generic indexed access, generic mapped, conditional-with-type-params),
        // where `tsc` cannot resolve a member type and so keeps the outer
        // whole-object diagnostic and its nested relation-reason chain. Re-enable
        // the literal-source elaboration only for the plain-application case: the
        // source is a fresh object/array literal, the relation failed at a
        // concrete member (`should_preserve_structural_property_diagnostic`), and
        // the target has no deferred evaluation surface. A non-literal source
        // (e.g. `X[K1]` vs `X[K2]` type-parameter drift) and any deferred target
        // (e.g. `Pick<C<T>, 'k'> & …`) keep the outer diagnostic + chain
        // unchanged, and `try_elaborate_assignment_source_error` still no-ops when
        // no per-property mismatch is present.
        let source_is_fresh_literal_with_member_failure = self
            .assignment_source_is_object_or_array_literal(source_idx)
            && self.should_preserve_structural_property_diagnostic(&outcome)
            && !self.target_has_deferred_evaluation_surface(target);
        let target_prefers_outer = self.target_prefers_outer_assignment_diagnostic(target);
        if (!target_prefers_outer || source_is_fresh_literal_with_member_failure)
            && self.try_elaborate_assignment_source_error(source_idx, target)
        {
            return false;
        }
        if target_prefers_outer
            && !self.should_preserve_structural_property_diagnostic(&outcome)
            && self
                .missing_required_properties_from_index_signature_source(source, target)
                .is_none()
        {
            self.error_type_not_assignable_at_with_display_types(source, target, diag_idx);
        } else {
            self.error_type_not_assignable_with_reason_at_anchor(source, target, diag_idx);
        }
        false
    }

    /// Like `check_assignable_or_report_at_exact_anchor`, but skips
    /// assignment-source elaboration so diagnostics stay on the enclosing
    /// source type shape.
    pub(crate) fn check_assignable_or_report_at_exact_anchor_without_source_elaboration(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }
        if let Some(allowed) =
            self.numeric_enum_assignment_override_from_source(source, target, source_idx)
        {
            if allowed {
                return true;
            }
            self.error_type_not_assignable_with_reason_at_anchor(source, target, diag_idx);
            return false;
        }
        let outcome = self.assignability_reason_relation_outcome(source, target);
        if outcome.related {
            return true;
        }

        if self.should_skip_weak_union_error_with_outcome(
            source,
            target,
            source_idx,
            Some(&outcome),
        ) {
            return true;
        }
        if outcome.weak_union_violation {
            self.error_no_common_properties(source, target, diag_idx);
            return false;
        }

        self.error_type_not_assignable_with_reason_at_anchor(source, target, diag_idx);
        false
    }

    /// Check pre-resolved source/target types and keep those exact types in the
    /// generic TS2322 display.
    pub(crate) fn check_pre_resolved_assignable_or_report_at_exact_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }
        if let Some(allowed) =
            self.numeric_enum_assignment_override_from_source(source, target, source_idx)
        {
            if allowed {
                return true;
            }
            self.error_type_not_assignable_at_with_raw_display_types(source, target, diag_idx);
            return false;
        }

        let outcome = self.assignability_reason_relation_outcome(source, target);
        if outcome.related {
            return true;
        }
        if self.should_skip_weak_union_error_with_outcome(
            source,
            target,
            source_idx,
            Some(&outcome),
        ) {
            return true;
        }
        if outcome.weak_union_violation {
            self.error_no_common_properties(source, target, diag_idx);
            return false;
        }

        self.error_type_not_assignable_at_with_raw_display_types(source, target, diag_idx);
        false
    }

    /// Check if source object literal has properties that don't exist in target.
    ///
    /// Pre-evaluation failure detectors that work on the *raw* (unevaluated)
    /// source/target. These cover failure shapes the structural solver pass
    /// cannot reconstruct after `prepare_assignability_inputs` collapses the
    /// operands to their evaluated form:
    ///
    /// 1. `S[T1]` vs `S[T2]` distinct type-parameter keys — both halves
    ///    evaluate to the same shared constraint, erasing the `T1`/`T2`
    ///    identity needed for the TS2322 + TS5075 elaboration chain.
    /// 2. Abstract → non-abstract constructor — the abstractness flag lives
    ///    on the symbol and the checker override consults it before any
    ///    structural walk; the evaluated shapes are otherwise compatible.
    /// 3. Same-generic application (`C<A..>` vs `C<B..>`) — the applications
    ///    evaluate to object shapes losing the type-argument identity, which
    ///    suppresses tsc's direct-argument elaboration chain.
    ///
    /// Returns `None` when none of the pre-evaluation detectors fires.
    /// Cheap by design — none of these probes runs a fresh `CompatChecker`
    /// solver pass; they only inspect type-data shape and symbol flags.
    /// Used both as `analyze_assignability_failure`'s pre-pass and as the
    /// cheap fallback for callers (like `assign_relation_outcome`) that
    /// already exhausted the solver-side reason and only need to recover the
    /// raw-input cases.
    pub(crate) fn raw_input_failure_reason(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<tsz_solver::SubtypeFailureReason> {
        if let Some(reason) =
            crate::query_boundaries::assignability::index_access_pair_distinct_type_param_keys_failure_reason(
                self.ctx.types,
                &self.ctx.definition_store,
                source,
                target,
            )
        {
            return Some(reason);
        }

        if let Some(reason) = self.abstract_constructor_assignment_failure_reason(source, target) {
            return Some(reason);
        }

        crate::query_boundaries::assignability::same_generic_application_failure_reason(
            self.ctx.types,
            &self.ctx,
            &self.ctx,
            source,
            target,
        )
    }

    pub(crate) fn analyze_assignability_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
        if let Some(reason) = self.raw_input_failure_reason(source, target) {
            return crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
                weak_union_violation: false,
                failure_reason: Some(reason),
            };
        }

        let (prepared_source, prepared_target) = self.prepare_assignability_inputs(source, target);

        // Share one captured reason-collecting solver pass with the
        // `RelationRequest` gateway (`execute_relation_request`): the gateway
        // typically already decided this exact prepared pair under the same
        // flags, so the memo replays its analysis instead of re-running the
        // relation engine (issue #13243). Same key and stamp model on both
        // sides keeps the gate byte-equivalent to a fresh pass.
        let flags = self.ctx.pack_relation_flags();
        let memo_key = (
            prepared_source,
            prepared_target,
            flags,
            self.ctx.sound_mode(),
        );
        let precomputed = self.failure_memo_lookup(memo_key);

        // Keep failure analysis on the same relation boundary as `is_assignable_to`
        // (CheckerContext resolver + checker overrides) so mismatch suppression and
        // diagnostic rendering observe identical compatibility semantics.
        let overrides = CheckerOverrideProvider::new(self, None);
        let inputs = AssignabilityQueryInputs {
            db: self.ctx.types,
            resolver: &self.ctx,
            source: prepared_source,
            target: prepared_target,
            flags,
            inheritance_graph: &self.ctx.inheritance_graph,
            sound_mode: self.ctx.sound_mode(),
            evaluation_session: Some(self.ctx.eval_session.as_ref()),
        };
        // Snapshot the unresolved-`Lazy` counter before the relation so
        // `failure_memo_store` can refuse to persist an analysis that compared
        // against a not-yet-registered def body (issue #12101 backstop).
        let lazy_failures_at_entry = crate::query_boundaries::common::lazy_resolve_failure_count();
        let (gate, capture) =
            check_assignable_gate_with_overrides(&inputs, &overrides, true, precomputed.as_ref());
        if let Some(capture) = capture {
            self.failure_memo_store(memo_key, capture, lazy_failures_at_entry);
        }
        if gate.related
            && let Some(reason) =
                self.checker_only_assignability_failure_reason(prepared_source, prepared_target)
        {
            return crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
                weak_union_violation: false,
                failure_reason: Some(reason),
            };
        }
        if gate.related {
            return crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
                weak_union_violation: false,
                failure_reason: None,
            };
        }
        let result = gate.analysis.unwrap_or(
            crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
                weak_union_violation: false,
                failure_reason: None,
            },
        );

        let evaluated_target = self.evaluate_type_for_assignability(target);
        let result = suppress_raw_excess_property_failure_if_needed(
            result,
            self.ctx.types,
            [target, evaluated_target],
            |member| self.evaluate_type_for_assignability(member),
        );

        // Suppress false TS2559 (NoCommonProperties) for interfaces that extend
        // arrays/tuples. These types inherit non-optional members from Array.prototype
        // (length, push, pop, etc.) that aren't in the ObjectShape's property list,
        // making them appear as weak types when they aren't.
        let failure_reason = if matches!(
            &result.failure_reason,
            Some(tsz_solver::SubtypeFailureReason::NoCommonProperties { .. })
        ) && self.target_extends_array_or_tuple(target)
        {
            None
        } else {
            result.failure_reason
        };

        let failure_reason = failure_reason
            .map(|reason| self.wrap_intersection_target_failure(source, target, reason));

        crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
            weak_union_violation: result.weak_union_violation,
            failure_reason,
        }
    }

    /// Elaborate a target-**intersection** assignment failure with the failing
    /// constituent frame that `tsc` emits.
    ///
    /// `tsc` (`typeRelatedToEachType`) relates a source to each constituent of a
    /// target intersection `C1 & C2 & …` in written order and elaborates the
    /// first constituent the source fails. tsz evaluates the intersection into a
    /// single merged object via `evaluate_type_for_assignability` before the
    /// reason is built, so the merged reason drills straight into the failing
    /// property and drops the constituent context (`Type 'S' is not assignable
    /// to type 'Ci'.`) that explains which member of the intersection requires
    /// the failing shape. Recover it here from the original (pre-evaluation)
    /// target by re-relating against each constituent and nesting the first
    /// failure under an [`IntersectionTargetMismatch`] frame.
    ///
    /// Display-only: the relation decision is unchanged; this restructures the
    /// failure reason chain to match `tsc`. Excess-property / weak-type failures
    /// (which `tsc` does not elaborate per constituent) are left untouched.
    ///
    /// [`IntersectionTargetMismatch`]: tsz_solver::SubtypeFailureReason::IntersectionTargetMismatch
    fn wrap_intersection_target_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
        reason: tsz_solver::SubtypeFailureReason,
    ) -> tsz_solver::SubtypeFailureReason {
        use crate::query_boundaries::common::SubtypeFailureReason as R;
        // Excess-property failures are never a per-constituent elaboration in
        // `tsc` (they only arise from a fresh object literal source), and an
        // already-framed reason must not be re-wrapped.
        if matches!(
            reason,
            R::ExcessProperty { .. } | R::IntersectionTargetMismatch { .. }
        ) {
            return reason;
        }
        // Missing/no-common-property failures against an intersection target are
        // owned by a separate caller-side emission path *for object-like
        // sources* (which anchors and words `TS2741`/`TS2739`, naming the
        // requiring constituent itself). `tsc` reaches that path only when the
        // source has properties to compare; a primitive/non-object source
        // (`number`, `string`, a literal, `symbol`, …) produces no such
        // message, so `tsc` (`typeRelatedToEachType`) instead elaborates the
        // first failing constituent frame — which tsz otherwise dropped. Wrap
        // those here; leave the object-source path untouched. This object/
        // primitive split must stay in sync with the missing-property render
        // path (`render_failure_missing_property.rs`), which likewise emits the
        // `TS2741`/`TS2739` line only for object-like sources.
        //
        // `resolve_lazy_type` is load-bearing: `is_object_like_type` treats a
        // bare `Lazy` alias (e.g. `type N = number`) as object-like, so the
        // source must be resolved to its concrete form before the check.
        if matches!(
            reason,
            R::NoCommonProperties { .. } | R::MissingProperty { .. } | R::MissingProperties { .. }
        ) {
            let resolved_source = self.resolve_lazy_type(source);
            if crate::query_boundaries::common::is_object_like_type(self.ctx.types, resolved_source)
            {
                return reason;
            }
        }
        let members = match self.target_intersection_constituents(target) {
            Some(members) => members,
            None => return reason,
        };
        for constituent in members {
            // The first constituent the source fails is the one tsc elaborates.
            // Route the per-constituent decision through the same gateway
            // (`analyze_assignability_failure`) rather than a raw assignability
            // predicate: a `None` reason means the source satisfies this
            // constituent, and a `Some` reason is exactly the nested chain to
            // frame.
            let Some(inner) = self
                .analyze_assignability_failure(source, constituent)
                .failure_reason
            else {
                continue;
            };
            return R::IntersectionTargetMismatch {
                source_type: source,
                target_type: target,
                constituent_type: constituent,
                nested_reason: Box::new(inner),
                // Preserve the merged-target reason so the headline stays
                // byte-identical to the pre-wrap output (fingerprint-stable).
                original_reason: Box::new(reason),
            };
        }
        reason
    }

    /// The written constituents of an intersection `target`, or `None` if it is
    /// not an intersection.
    ///
    /// Resolves lazy aliases (`type T = A & B`) without evaluating/merging so the
    /// constituents survive. Anonymous object intersections (`{ x } & { y }`) are
    /// eagerly merged into a single object at construction but retain the written
    /// intersection as a display alias — a structural `TypeId` back-reference, not
    /// rendered text — so fall back to that to recover the constituents.
    fn target_intersection_constituents(&mut self, target: TypeId) -> Option<Vec<TypeId>> {
        // A merged multi-declaration interface reference (e.g. the lib's
        // `Map<K, V>`) evaluates to an intersection of its per-declaration
        // shapes, but tsc relates and reports it as ONE named interface
        // surface — the per-constituent elaboration frame applies only to
        // written intersections.
        if crate::query_boundaries::diagnostics::is_interface_reference(
            self.ctx.types,
            &self.ctx.definition_store,
            target,
        ) {
            return None;
        }
        let resolved = self.resolve_lazy_type(target);
        crate::query_boundaries::common::intersection_members(self.ctx.types, resolved)
            .map(|list| list.iter().copied().collect())
            .or_else(|| self.display_alias_intersection_constituents(resolved))
            .or_else(|| self.display_alias_intersection_constituents(target))
    }

    /// The intersection constituents of `ty`'s display alias, if it has one whose
    /// alias is an intersection (the anonymous-object-intersection recovery path).
    fn display_alias_intersection_constituents(&self, ty: TypeId) -> Option<Vec<TypeId>> {
        let alias = self.ctx.types.get_display_alias(ty)?;
        Some(
            crate::query_boundaries::common::intersection_members(self.ctx.types, alias)?
                .iter()
                .copied()
                .collect(),
        )
    }

    /// Check if a target type extends an array or tuple by looking through lazy
    /// and evaluated forms. The `types_extending_array` set stores the interface
    /// merge result TypeId, but the target at assignability-check time may be
    /// a Lazy or evaluated form of the same type.
    fn target_extends_array_or_tuple(&mut self, target: TypeId) -> bool {
        if self.ctx.types_extending_array.contains(&target) {
            return true;
        }
        // The target may be a Lazy(DefId) that evaluates to a tracked type.
        // Resolve it and check again.
        let resolved = self.resolve_lazy_type(target);
        if resolved != target && self.ctx.types_extending_array.contains(&resolved) {
            return true;
        }
        // Also check the evaluated form.
        let evaluated = self.evaluate_type_for_assignability(target);
        if evaluated != target && self.ctx.types_extending_array.contains(&evaluated) {
            return true;
        }
        false
    }

    pub(crate) fn is_weak_union_violation(&mut self, source: TypeId, target: TypeId) -> bool {
        self.analyze_assignability_failure(source, target)
            .weak_union_violation
    }

    /// Emit TS2559 ("Type 'X' has no properties in common with type 'Y'")
    /// or TS2560 ("Value of type 'X' has no properties in common with type 'Y'. Did you mean to call it?")
    /// at the given node. Used for variable assignment and parameter sites
    /// where the solver detected a weak type violation.
    ///
    /// When the source type is callable or constructable and calling/constructing
    /// it would produce a type that is assignable to the target, tsc emits TS2560
    /// instead of TS2559 to suggest calling the value.
    /// Format the source type for TS2559 messages. tsc widens enum-member
    /// literal types to their parent enum name (e.g. `E.A` → `E`) in the
    /// source slot of "has no properties in common" — but only there, not in
    /// the general `format_type_diagnostic` output where `Parent.Member`
    /// remains correct.
    fn ts2559_source_display(&self, source: TypeId) -> String {
        if let Some(parent_name) =
            enum_query::enum_member_like_parent_escaped_name(&self.ctx, source)
        {
            return parent_name;
        }
        // tsc renders the source's *widened* apparent type in the weak-type
        // ("no properties in common") slot: a fresh object literal `{ c: 1 }`
        // displays as `{ c: number }`, matching TS2322/TS2739/TS2741. The
        // shared helper rewrites only a top-level *fresh* object literal, so a
        // non-fresh annotated source (or a declared intersection rendered via a
        // display alias) is returned untouched and keeps its existing display.
        let display_source = self.widen_fresh_object_literal_properties_for_display(source);
        self.format_type_diagnostic(display_source)
    }

    pub(crate) fn error_no_common_properties(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        if source == TypeId::ERROR
            || target == TypeId::ERROR
            || source == TypeId::ANY
            || target == TypeId::ANY
        {
            return;
        }

        // Disambiguate same-short-named nominal pairs (e.g. `M.A` vs `N.A`)
        // so the diagnostic doesn't collapse to `Type 'A' has no properties
        // in common with type 'A'.`. Mirrors the pair-display logic used by
        // the standard TS2322 emitter.
        // For TS2559, tsc widens enum-member literal types to the parent enum
        // name in the source slot (e.g. `E.A` displays as `E`). The default
        // `format_type_diagnostic` returns `Parent.Member`, which is correct
        // elsewhere but mismatches tsc here.
        let source_str = self.ts2559_source_display(source);
        let target_str = self.format_type_diagnostic(target);
        let (source_str, target_str) =
            self.finalize_pair_display_for_diagnostic(source, target, source_str, target_str);

        // Check if the source is callable/constructable and calling/constructing
        // would produce a type assignable to the target (TS2560 instead of TS2559).
        if self.should_suggest_calling_for_weak_type(source, target) {
            self.error_at_node_msg(
                idx,
                crate::diagnostics::diagnostic_codes::VALUE_OF_TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_DID_YOU_MEAN_TO_CALL_IT,
                &[&source_str, &target_str],
            );
            return;
        }

        self.error_at_node_msg(
            idx,
            crate::diagnostics::diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
            &[&source_str, &target_str],
        );
    }

    /// Per-property elaboration helper: when a property value would otherwise
    /// produce TS2322, route to TS2559 if the source has no properties in
    /// common with the property's weak target. Strips strictNullChecks'
    /// implicit `| undefined` from the target and uses the literal source
    /// type for display so the message reads `Type 'false' has no properties
    /// in common with type 'OverridesInput'` instead of `Type 'boolean' is
    /// not assignable to type 'OverridesInput | undefined'`.
    pub(crate) fn try_emit_property_weak_type_violation(
        &mut self,
        source_prop_type: TypeId,
        target_prop_type: TypeId,
        target_prop_type_for_diagnostic: TypeId,
        prop_value_idx: NodeIndex,
        prop_name_idx: NodeIndex,
    ) -> bool {
        let weak_target = match self.split_nullish_type(target_prop_type) {
            (Some(non_nullish), Some(_)) => non_nullish,
            _ => target_prop_type,
        };
        let weak_target_for_display = match self.split_nullish_type(target_prop_type_for_diagnostic)
        {
            (Some(non_nullish), Some(_)) => non_nullish,
            _ => target_prop_type_for_diagnostic,
        };
        let weak_source = self
            .literal_type_from_initializer(prop_value_idx)
            .unwrap_or(source_prop_type);
        if !self.is_weak_union_violation(weak_source, weak_target) {
            return false;
        }
        self.error_no_common_properties(weak_source, weak_target_for_display, prop_name_idx);
        true
    }

    /// Check whether a "did you mean to call it?" suggestion is appropriate
    /// for a weak type violation. Returns true when the source type has call
    /// or construct signatures and the return type would be assignable to
    /// the target (i.e., calling/constructing would fix the type mismatch).
    pub(crate) fn should_suggest_calling_for_weak_type(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        // Evaluate the source type to resolve Lazy(DefId) → concrete type form.
        // This is needed because interfaces like `CtorOnly { new(s: string): T }`
        // start as Lazy types that must be evaluated before signature extraction.
        let resolved_source = self.evaluate_type_for_assignability(source);

        // Check call signatures first
        if let Some(return_type) = crate::query_boundaries::diagnostics::return_type_for_type(
            self.ctx.types,
            resolved_source,
        ) && return_type != TypeId::VOID
            && return_type != TypeId::UNDEFINED
            && return_type != TypeId::NEVER
            && self.return_relation_outcome(return_type, target).related
        {
            return true;
        }

        // Check construct signatures — use get_construct_signatures directly
        // which handles Callable types and intersections.
        if let Some(sigs) = crate::query_boundaries::diagnostics::construct_signatures_for_type(
            self.ctx.types,
            resolved_source,
        ) && let Some(first_sig) = sigs.first()
        {
            let construct_return = first_sig.return_type;
            if construct_return != TypeId::VOID
                && construct_return != TypeId::UNDEFINED
                && construct_return != TypeId::NEVER
                && self
                    .return_relation_outcome(construct_return, target)
                    .related
            {
                return true;
            }
        }

        false
    }

    pub(crate) fn checker_only_assignability_failure_reason(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<tsz_solver::SubtypeFailureReason> {
        if self.iterator_result_required_value_mismatch(source, target) {
            return Some(tsz_solver::SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            });
        }

        if !self.checker_only_assignability_may_apply(source, target) {
            return None;
        }
        if self.iterator_next_type_display_mismatch(source, target) {
            return Some(tsz_solver::SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            });
        }
        if self.iterator_result_return_display_mismatch(source, target) {
            return Some(tsz_solver::SubtypeFailureReason::ReturnTypeMismatch {
                source_return: source,
                target_return: target,
                nested_reason: None,
            });
        }
        None
    }

    /// Produce the structured reason for an abstract-constructor-to-concrete
    /// assignment failure when the checker's abstract-constructor override
    /// rejects the relation. The override is the single source of truth for
    /// the abstractness decision (it resolves symbol flags and unwraps
    /// application/type-query chains), so this stays in sync with the relation
    /// itself rather than re-deriving the shape from the printer.
    pub(crate) fn abstract_constructor_assignment_failure_reason(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<tsz_solver::SubtypeFailureReason> {
        (self.abstract_constructor_assignability_override(source, target, None) == Some(false))
            .then_some(tsz_solver::SubtypeFailureReason::AbstractConstructorAssignment)
    }

    pub(crate) fn checker_only_assignability_may_apply(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        crate::query_boundaries::diagnostics::type_may_display_iterator_protocol(
            self.ctx.types,
            source,
        ) && crate::query_boundaries::diagnostics::type_may_display_iterator_protocol(
            self.ctx.types,
            target,
        )
    }

    fn iterator_result_required_value_mismatch(&mut self, source: TypeId, target: TypeId) -> bool {
        let Some(source_args) = self.iterator_result_application_args(source) else {
            return false;
        };
        if source_args
            .get(1)
            .copied()
            .is_none_or(|return_type| !self.type_evaluates_to(return_type, TypeId::UNDEFINED))
        {
            return false;
        }

        let target = self.evaluate_type_for_assignability(target);
        let Some(target_shape) = object_shape_for_type(self.ctx.types, target) else {
            return false;
        };
        let value_name = self.ctx.types.intern_string("value");
        let Some(value_prop) = target_shape
            .properties
            .iter()
            .find(|prop| prop.name == value_name)
        else {
            return false;
        };
        if value_prop.optional {
            return false;
        }
        let value_type = value_prop.type_id;

        !self
            .iterator_result_value_relation_outcome(TypeId::UNDEFINED, value_type)
            .related
    }

    fn iterator_result_application_args(&self, type_id: TypeId) -> Option<Vec<TypeId>> {
        let (base, args) = self.application_info_or_display_alias(type_id)?;
        let def_id = crate::query_boundaries::diagnostics::lazy_def_id(self.ctx.types, base)
            .or_else(|| self.ctx.definition_store.find_def_for_type(base))?;
        let iterator_result_def =
            self.resolve_entity_name_text_to_def_id_for_lowering("IteratorResult")?;
        (def_id == iterator_result_def).then_some(args)
    }

    fn type_evaluates_to(&mut self, type_id: TypeId, expected: TypeId) -> bool {
        type_id == expected || self.evaluate_type_for_assignability(type_id) == expected
    }

    fn iterator_next_type_display_mismatch(&mut self, source: TypeId, target: TypeId) -> bool {
        let source_display = self.format_type(source);
        let Some(source_next) = iterator_protocol_next_type_arg(&source_display).or_else(|| {
            function_return_display(&source_display).and_then(iterator_protocol_next_type_arg)
        }) else {
            return false;
        };

        let target_display = self.format_type(target);
        let target_protocol = function_return_display(&target_display).unwrap_or(&target_display);
        let mut target_nexts: Vec<&str> = iterator_protocol_next_type_arg(target_protocol)
            .into_iter()
            .collect();
        if target_nexts.is_empty() {
            target_nexts.extend(
                target_protocol
                    .split(" | ")
                    .filter_map(iterator_protocol_next_type_arg),
            );
        }

        !target_nexts.is_empty()
            && target_nexts
                .into_iter()
                .all(|target_next| !iterator_next_type_accepts(source_next, target_next))
    }

    fn iterator_result_return_display_mismatch(&mut self, source: TypeId, target: TypeId) -> bool {
        let Some(target_return) = self.callable_return_type_for_iterator_diagnostic(target) else {
            return false;
        };
        let target_return = self.evaluate_type_for_assignability(target_return);
        let Some(target_args) = self.iterator_result_application_args(target_return) else {
            return false;
        };
        if target_args
            .get(1)
            .copied()
            .is_none_or(|return_type| !self.type_evaluates_to(return_type, TypeId::UNKNOWN))
        {
            return false;
        }

        let Some(source_return) = self.callable_return_type_for_iterator_diagnostic(source) else {
            return false;
        };
        self.iterator_result_return_source_has_broad_done(source_return)
    }

    fn callable_return_type_for_iterator_diagnostic(&mut self, type_id: TypeId) -> Option<TypeId> {
        crate::query_boundaries::diagnostics::return_type_for_type(self.ctx.types, type_id).or_else(
            || {
                let evaluated = self.evaluate_type_for_assignability(type_id);
                crate::query_boundaries::diagnostics::return_type_for_type(
                    self.ctx.types,
                    evaluated,
                )
            },
        )
    }

    fn iterator_result_return_source_has_broad_done(&mut self, type_id: TypeId) -> bool {
        let type_id = self.evaluate_type_for_assignability(type_id);
        if let Some(members) =
            crate::query_boundaries::diagnostics::union_members(self.ctx.types, type_id)
        {
            return members
                .iter()
                .any(|&member| self.iterator_result_return_source_has_broad_done(member));
        }

        let Some(shape) = object_shape_for_type(self.ctx.types, type_id) else {
            return false;
        };
        let value_name = self.ctx.types.intern_string("value");
        let value_prop = shape.properties.iter().find(|prop| prop.name == value_name);
        // An `any`-typed `value` makes the object assignable to *either*
        // `IteratorResult` arm (`IteratorYieldResult<TYield>` /
        // `IteratorReturnResult<TReturn>`) regardless of its `done` discriminant —
        // `any` satisfies both `TYield` and `TReturn`. So a broad `done` is not a
        // genuine override mismatch here: `tsc` accepts a `next()` override whose
        // inferred return is `{ done: boolean; value: any }`, which is exactly the
        // shape produced by an unannotated iterator override under
        // `strictNullChecks: false` (`value: undefined` widens to `any`, #17003).
        // Defer to the general relation for these instead of forcing a TS2416.
        if value_prop.is_some_and(|prop| self.type_evaluates_to(prop.type_id, TypeId::ANY)) {
            return false;
        }
        let done_name = self.ctx.types.intern_string("done");
        let Some(done_prop) = shape.properties.iter().find(|prop| prop.name == done_name) else {
            return false;
        };
        let done_type = self.evaluate_type_for_assignability(done_prop.type_id);
        if done_type == TypeId::BOOLEAN {
            return true;
        }
        if done_type != TypeId::BOOLEAN_TRUE {
            return false;
        }

        !value_prop.is_some_and(|prop| self.type_evaluates_to(prop.type_id, TypeId::UNDEFINED))
    }
}

fn parse_simple_type_application_display(display: &str) -> Option<(&str, Vec<&str>)> {
    let (name, args) = display.split_once('<')?;
    let args = args.strip_suffix('>')?;
    if name.is_empty() || name.contains([' ', '<', '>']) || args.contains('<') || args.contains('>')
    {
        return None;
    }
    let arg_names: Vec<_> = args
        .split(',')
        .map(str::trim)
        .filter(|arg| !arg.is_empty() && !arg.contains(' '))
        .collect();
    if arg_names.is_empty() {
        return None;
    }
    Some((name, arg_names))
}

fn iterator_protocol_next_type_arg(display: &str) -> Option<&str> {
    if let Some((name, args)) = parse_simple_type_application_display(display)
        && matches!(
            name,
            "Generator" | "Iterator" | "IteratorObject" | "Iterable"
        )
    {
        return args.get(2).copied();
    }

    display
        .split_once("next(..._: [] | [")
        .and_then(|(_, rest)| rest.split_once("])").map(|(next, _)| next.trim()))
}

fn function_return_display(display: &str) -> Option<&str> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut angle_depth = 0usize;
    let mut iter = display.char_indices().peekable();

    while let Some((idx, ch)) = iter.next() {
        if ch == '=' && iter.peek().is_some_and(|(_, next)| *next == '>') {
            if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 && angle_depth == 0 {
                return display.get(idx + 2..).map(str::trim);
            }
            iter.next();
            continue;
        }

        match ch {
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth = bracket_depth.saturating_add(1),
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth = brace_depth.saturating_add(1),
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '<' => angle_depth = angle_depth.saturating_add(1),
            '>' if angle_depth > 0 => angle_depth -= 1,
            _ => {}
        }
    }

    None
}

fn iterator_next_type_accepts(source_next: &str, target_next: &str) -> bool {
    source_next == target_next
        || source_next == "any"
        || target_next == "any"
        || source_next == "unknown"
        || target_next == "never"
}
