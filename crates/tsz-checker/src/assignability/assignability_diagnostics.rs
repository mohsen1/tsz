use crate::query_boundaries::assignability::{
    AssignabilityQueryInputs, ExcessPropertiesKind, check_assignable_gate_with_overrides,
    classify_for_excess_properties, get_keyof_type, get_string_literal_value, is_keyof_type,
    is_type_parameter_like, object_shape_for_type, suppress_raw_excess_property_failure_if_needed,
};
use crate::query_boundaries::common::type_param_info;
use crate::state::{CheckerOverrideProvider, CheckerState};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

mod argument_reports;
mod display_types;
mod explicit_any_annotations;

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
        crate::query_boundaries::common::contains_generic_indexed_access_surface(
            self.ctx.types,
            type_id,
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
    #[allow(dead_code)]
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
        let built_outcome = self.assign_relation_outcome(source, target);
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
            crate::query_boundaries::common::is_fresh_object_type(self.ctx.types, source);
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

        if self.diagnostic_relation_boolean_guard(source, target) {
            return true;
        }
        if self.is_nested_same_wrapper_application_assignment(source, target) {
            return true;
        }

        // Use the canonical assign relation outcome so the weak-union hint is collected alongside
        // the failure reason, avoiding a redundant solver round-trip in
        // should_skip_weak_union_error's fallback path.
        let outcome = self.assign_relation_outcome(source, target);
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
            crate::query_boundaries::common::readonly_inner_type(self.ctx.types, source)
            && self.diagnostic_relation_boolean_guard(inner, target)
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

        // For array literal sources, drill into element-level errors, matching
        // tsc's `elaborateElementwise` behavior. Check the unwrapped source so
        // equivalent forms like `([10, "20"]) satisfies number[]` and
        // `([10, "20"] as (number | string)[]) satisfies number[]` use the same
        // element elaboration path.
        let unwrapped_source_idx = self.ctx.arena.skip_parenthesized_and_assertions(source_idx);
        if let Some(node) = self.ctx.arena.get(unwrapped_source_idx)
            && node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            && self.try_elaborate_assignment_source_error(source_idx, target)
        {
            return false;
        }

        self.error_type_does_not_satisfy_the_expected_type(source, target, diag_idx, keyword_pos);
        false
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
                if crate::query_boundaries::common::type_is_conditional_type_result_with_unresolved_inference(
                    self.ctx.types,
                    target_prop_type,
                ) || crate::query_boundaries::common::type_is_conditional_type_result_with_unresolved_inference(
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
        if self.diagnostic_relation_boolean_guard(source, target) {
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
        let assignable = self.diagnostic_relation_boolean_guard(source, target);
        // tsc emits TS2859 ("Excessive complexity") for all relation-checker
        // overflows regardless of whether it was depth or iteration that fired.
        // TS2321 ("Excessive stack depth") fires from a separate mechanism.
        if !assignable && self.ctx.relation_overflow.get().has_overflow() {
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
        // Use the canonical assign relation outcome so the weak-union hint
        // can be collected alongside the failure reason.
        let outcome = self.assign_relation_outcome(source, target);

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
            // Recover the literal initializer type for display (e.g. `"A"` not
            // `string`) when the source has been widened upstream. tsc keeps
            // the literal in the TS2559 source slot.
            let display_source = self
                .literal_type_from_initializer(source_idx)
                .unwrap_or(source);
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
            && self.try_elaborate_assignment_source_error(source_idx, target)
        {
            return false;
        }
        self.error_type_not_assignable_with_reason_at(source, target, diag_idx);
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

    fn error_type_not_assignable_at_with_raw_display_types(
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
            crate::query_boundaries::common::tuple_elements(self.ctx.types, ty)
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

    fn numeric_enum_assignment_override_from_source(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> Option<bool> {
        use crate::query_boundaries::common::TypeResolver;
        let target = self.evaluate_type_for_assignability(target);
        let target_def_id = crate::query_boundaries::common::enum_def_id(self.ctx.types, target)?;
        if !self.ctx.is_numeric_enum(target_def_id) {
            return None;
        }

        let source_literal = self.literal_type_from_initializer(source_idx);
        let source_is_number_like = source == TypeId::NUMBER
            || source_literal.is_some_and(|lit| {
                crate::query_boundaries::common::is_number_literal(self.ctx.types, lit)
            });
        if !source_is_number_like {
            return None;
        }

        if self.ctx.is_enum_type(target, self.ctx.types) {
            if let Some(source_literal) = source_literal {
                let structural_target =
                    crate::query_boundaries::common::enum_member_type(self.ctx.types, target)
                        .unwrap_or(target);
                return Some(
                    self.diagnostic_relation_boolean_guard(source_literal, structural_target),
                );
            }
            return None;
        }

        let target_member =
            crate::query_boundaries::common::enum_member_type(self.ctx.types, target);
        let target_literal = target_member.and_then(|member| {
            crate::query_boundaries::common::literal_value(self.ctx.types, member)
        });

        target_member?;

        match source_literal {
            Some(source_literal) => {
                let source_val =
                    crate::query_boundaries::common::literal_value(self.ctx.types, source_literal);
                match (source_val, target_literal) {
                    (
                        Some(tsz_solver::LiteralValue::Number(source_num)),
                        Some(tsz_solver::LiteralValue::Number(target_num)),
                    ) => Some(source_num == target_num),
                    _ => Some(false),
                }
            }
            None => (source == TypeId::NUMBER).then_some(true),
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
        if self.diagnostic_relation_boolean_guard(source, target) {
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

        // Use the canonical assign relation outcome so the weak-union hint is collected alongside
        // the failure reason, avoiding a redundant solver round-trip in
        // should_skip_weak_union_error's fallback path.
        let outcome = self.assign_relation_outcome(source, target);
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

        if self.try_elaborate_assignment_source_error(source_idx, target) {
            return false;
        }
        self.error_type_not_assignable_with_reason_at_anchor(source, target, diag_idx);
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
        if self.diagnostic_relation_boolean_guard(source, target) {
            return true;
        }

        // Use the canonical assign relation outcome so the weak-union hint is collected alongside
        // the failure reason, avoiding a redundant solver round-trip in
        // should_skip_weak_union_error's fallback path.
        let outcome = self.assign_relation_outcome(source, target);
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

        let outcome = self.assign_relation_outcome(source, target);
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

    /// Check if two object types with call/construct signatures are comparable
    /// because at least one has generic type parameters.
    ///
    /// In tsc's Comparable relation, object types with generic call signatures
    /// are considered comparable to concrete call signature objects because the
    /// generic could potentially be instantiated to match. For example:
    /// `{ fn<T, U extends T>(x: T, y: U): T }` is comparable to
    /// `{ fn(x: Base, y: C): Base }` because T=Base, U=C is a valid instantiation.
    ///
    /// This checks both direct callable shapes (for Callable types) and
    /// property-level callable shapes (for Object types with method properties).
    fn objects_with_generic_signatures_are_comparable(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let source_resolved = self.evaluate_type_with_resolution(source);
        let target_resolved = self.evaluate_type_with_resolution(target);

        let src_has_generics = self.type_has_generic_signatures(source_resolved);
        let tgt_has_generics = self.type_has_generic_signatures(target_resolved);

        // At least one side must have generic type parameters
        if !src_has_generics && !tgt_has_generics {
            return false;
        }

        // Both must be object-like types (have callable shape or object shape)
        let src_is_object_like = self.is_object_or_callable_type(source_resolved);
        let tgt_is_object_like = self.is_object_or_callable_type(target_resolved);

        src_is_object_like && tgt_is_object_like
    }

    /// Check if a type has any generic call/construct signatures, either directly
    /// (Callable/Function type) or through object properties.
    fn type_has_generic_signatures(&self, type_id: TypeId) -> bool {
        // Check direct callable shape (CallableShape has call_signatures + construct_signatures)
        if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
        {
            let has_generic_sigs = shape
                .call_signatures
                .iter()
                .chain(shape.construct_signatures.iter())
                .any(|sig| !sig.type_params.is_empty());
            if has_generic_sigs {
                return true;
            }
        }

        // Check direct function shape (FunctionShape has type_params)
        if let Some(func_shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            && !func_shape.type_params.is_empty()
        {
            return true;
        }

        // Check object properties for callable/function types with generics
        if let Some(obj_shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
        {
            for prop in &obj_shape.properties {
                // Check callable property types
                if let Some(callable) = crate::query_boundaries::common::callable_shape_for_type(
                    self.ctx.types,
                    prop.type_id,
                ) {
                    let has_generic_sigs = callable
                        .call_signatures
                        .iter()
                        .chain(callable.construct_signatures.iter())
                        .any(|sig| !sig.type_params.is_empty());
                    if has_generic_sigs {
                        return true;
                    }
                }
                // Check function property types
                if let Some(func_shape) = crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    prop.type_id,
                ) && !func_shape.type_params.is_empty()
                {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a type is object-like (Callable, Object, or Function type).
    fn is_object_or_callable_type(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id).is_some()
            || crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
                .is_some()
            || crate::query_boundaries::common::has_function_shape(self.ctx.types, type_id)
    }

    /// Check if two object types are comparable because their function-typed
    /// properties have overlapping arity.
    ///
    /// In tsc's comparable relation, objects like `{ fn(a?: Base): void }` and
    /// `{ fn(a?: C): void }` are considered comparable because both functions
    /// can be called with 0 arguments (all optional). The comparable relation
    /// threads through object properties and checks function signatures for
    /// arity overlap, not strict assignability.
    fn objects_with_arity_overlapping_functions_are_comparable(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        use crate::query_boundaries::common::{function_shape_for_type, object_shape_for_type};

        let source_resolved = self.evaluate_type_with_resolution(source);
        let target_resolved = self.evaluate_type_with_resolution(target);

        let Some(source_shape) = object_shape_for_type(self.ctx.types, source_resolved) else {
            return false;
        };
        let Some(target_shape) = object_shape_for_type(self.ctx.types, target_resolved) else {
            return false;
        };

        // Need at least one common property that is a function type
        let mut found_function_prop = false;

        for target_prop in &target_shape.properties {
            if let Some(source_prop) = source_shape
                .properties
                .iter()
                .find(|p| p.name == target_prop.name)
            {
                // Check if both properties are function types
                let src_func = function_shape_for_type(self.ctx.types, source_prop.type_id);
                let tgt_func = function_shape_for_type(self.ctx.types, target_prop.type_id);

                match (src_func, tgt_func) {
                    (Some(src_fn), Some(tgt_fn)) => {
                        found_function_prop = true;
                        // Check arity overlap: min arity of one <= max arity of other
                        let src_min = src_fn.params.iter().filter(|p| p.is_required()).count();
                        let tgt_min = tgt_fn.params.iter().filter(|p| p.is_required()).count();
                        let src_has_rest = src_fn.params.iter().any(|p| p.rest);
                        let tgt_has_rest = tgt_fn.params.iter().any(|p| p.rest);
                        let src_max = if src_has_rest {
                            usize::MAX
                        } else {
                            src_fn.params.len()
                        };
                        let tgt_max = if tgt_has_rest {
                            usize::MAX
                        } else {
                            tgt_fn.params.len()
                        };

                        // Arity ranges must overlap: [src_min, src_max] ∩ [tgt_min, tgt_max] ≠ ∅
                        if src_min > tgt_max || tgt_min > src_max {
                            return false;
                        }

                        // Thread through signature: even with overlapping arity, tsc's
                        // comparable relation requires pairwise parameter comparability
                        // and return-type comparability. Two optional params of unrelated
                        // types are still comparable because both admit `undefined`
                        // (e.g., `a?: Base` vs `a?: C`); skip those positions. Rest
                        // params compare by their element type.
                        let min_pairs = src_fn.params.len().min(tgt_fn.params.len());
                        let mut sig_ok = true;
                        for i in 0..min_pairs {
                            let sp = &src_fn.params[i];
                            let tp = &tgt_fn.params[i];
                            if sp.optional && tp.optional && !sp.rest && !tp.rest {
                                continue;
                            }
                            let src_t = if sp.rest {
                                crate::query_boundaries::common::array_element_type(
                                    self.ctx.types,
                                    sp.type_id,
                                )
                                .unwrap_or(sp.type_id)
                            } else {
                                sp.type_id
                            };
                            let tgt_t = if tp.rest {
                                crate::query_boundaries::common::array_element_type(
                                    self.ctx.types,
                                    tp.type_id,
                                )
                                .unwrap_or(tp.type_id)
                            } else {
                                tp.type_id
                            };
                            if !self.is_type_comparable_to(src_t, tgt_t) {
                                sig_ok = false;
                                break;
                            }
                        }
                        if !sig_ok {
                            return false;
                        }
                        if !self.is_type_comparable_to(src_fn.return_type, tgt_fn.return_type) {
                            return false;
                        }
                    }
                    (None, None) => {
                        // Neither is a function type — check normal comparability
                        let prop_comparable = self.diagnostic_relation_boolean_guard(
                            source_prop.type_id,
                            target_prop.type_id,
                        ) || self.diagnostic_relation_boolean_guard(
                            target_prop.type_id,
                            source_prop.type_id,
                        );
                        if !prop_comparable {
                            return false;
                        }
                    }
                    _ => {
                        // One is function, the other is not — not comparable
                        return false;
                    }
                }
            }
        }

        found_function_prop
    }

    /// Check if two types are comparable (overlap).
    ///
    /// Corresponds to TypeScript's `areTypesComparable`: returns true if the types
    /// have any overlap. TSC's comparableRelation differs from assignability:
    /// - For union sources: uses `someTypeRelatedToType` (ANY member suffices)
    /// - For union targets: also checks per-member overlap
    /// - For `TypeParameter` sources: uses apparent type (constraint or `unknown`)
    /// - Special carve-out: two unrelated type params are NOT comparable
    ///
    /// Used for switch/case comparability (TS2678), equality narrowing,
    /// relational operator checks (TS2365), etc.
    pub(crate) fn is_type_comparable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        use crate::query_boundaries::dispatch as query;

        // Identity: any type is trivially comparable to itself
        if source == target {
            return true;
        }

        // Resolve type parameters to their apparent types for comparison.
        // In tsc, `isTypeComparableTo` uses `getReducedApparentType` for TypeParam sources,
        // and has a carve-out when BOTH source and target are type parameters (only comparable
        // if one constrains to the other). See tsc checker.ts:23671-23684.
        let source_is_tp = is_type_parameter_like(self.ctx.types, source);
        let target_is_tp = is_type_parameter_like(self.ctx.types, target);

        if source_is_tp && target_is_tp {
            // Both are type parameters: only comparable if one constrains to the other.
            // Unconstrained T is NOT comparable to unconstrained U.
            return self.type_params_are_comparable(source, target);
        }

        // Resolve type parameter to apparent type (constraint or `unknown`)
        let source_apparent = if source_is_tp {
            self.get_type_param_apparent_type(source)
        } else {
            source
        };
        let target_apparent = if target_is_tp {
            self.get_type_param_apparent_type(target)
        } else {
            target
        };

        let skip_signature_only_fast_path =
            self.are_pure_signature_objects(source_apparent, target_apparent);

        // Fast path: direct bidirectional assignability (with apparent types).
        // Skip this for pure call/construct signature objects because TS overlap
        // checks are stricter than general object assignability there.
        if !skip_signature_only_fast_path
            && (self.diagnostic_relation_boolean_guard(source_apparent, target_apparent)
                || self.diagnostic_relation_boolean_guard(target_apparent, source_apparent))
        {
            return true;
        }

        // TSC's comparable relation decomposes unions and checks if ANY member
        // is related to the other type. This handles cases like:
        // - `User.A | User.B` comparable to `User.A` (User.A member matches)
        // - `string & Brand` comparable to `"a"` (string member of intersection)

        // Decompose source union: check if any member is assignable in either direction
        if let Some(members) = query::union_members(self.ctx.types, source_apparent) {
            for member in &members {
                if self.diagnostic_relation_boolean_guard(*member, target_apparent)
                    || self.diagnostic_relation_boolean_guard(target_apparent, *member)
                {
                    return true;
                }
            }
        }

        // Decompose target union: check if any member is assignable in either direction
        if let Some(members) = query::union_members(self.ctx.types, target_apparent) {
            for member in &members {
                if self.diagnostic_relation_boolean_guard(source_apparent, *member)
                    || self.diagnostic_relation_boolean_guard(*member, source_apparent)
                {
                    return true;
                }
            }
        }

        // Decompose intersection: `"a"` is comparable to `string & Brand` because
        // `"a"` is assignable to `string` (one constituent). tsc's comparable relation
        // treats intersections as comparable if the source overlaps with ANY member.
        if let Some(members) = query::intersection_members(self.ctx.types, source_apparent) {
            for member in &members {
                if self.diagnostic_relation_boolean_guard(*member, target_apparent)
                    || self.diagnostic_relation_boolean_guard(target_apparent, *member)
                {
                    return true;
                }
            }
        }
        if let Some(members) = query::intersection_members(self.ctx.types, target_apparent) {
            for member in &members {
                if self.diagnostic_relation_boolean_guard(source_apparent, *member)
                    || self.diagnostic_relation_boolean_guard(*member, source_apparent)
                {
                    return true;
                }
            }
        }

        // Additional check: Two object types where ALL properties are optional always
        // overlap at `{}`, making them comparable even if property types differ.
        // Example: `{ b?: number }` vs `{ b?: string }` are comparable because both
        // include `{}` as a valid value.
        if self.objects_with_all_optional_common_props_overlap(source_apparent, target_apparent) {
            return true;
        }

        if self.constructor_signature_only_objects_overlap(source_apparent, target_apparent) {
            return true;
        }

        // Two object types where at least one has generic call/construct signatures
        // are considered comparable by tsc's Comparable relation. This is because
        // generic signatures can potentially be instantiated to match the concrete
        // type, so tsc treats them as having structural overlap.
        if self.objects_with_generic_signatures_are_comparable(source_apparent, target_apparent) {
            return true;
        }

        // Two object types where function-typed properties have overlapping arity
        // are comparable. For example, `{ fn(a?: Base): void }` and `{ fn(a?: C): void }`
        // are comparable because both functions can be called with 0 args (all optional).
        // tsc's Comparable relation threads through object properties and considers
        // function signatures comparable when their arity ranges overlap.
        if self.objects_with_arity_overlapping_functions_are_comparable(
            source_apparent,
            target_apparent,
        ) {
            return true;
        }

        false
    }

    /// Check if two object types have comparable properties.
    ///
    /// Resolves both types to their concrete shapes and checks if every common
    /// property's type is comparable (assignable in at least one direction).
    /// This implements the property-level threading of tsc's `comparableRelation`,
    /// handling cases where whole-object bidirectional assignability fails but
    /// individual property types overlap.
    pub(crate) fn object_properties_are_comparable(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        use crate::query_boundaries::assignability::object_shape_for_type;
        use tsz_common::Visibility;

        // Skip when either type involves type parameters. Type parameter
        // constraints overlap structurally with many types, but tsc's
        // comparable relation for generics is stricter than per-property
        // bidirectional assignability.
        if crate::query_boundaries::assignability::contains_type_parameters(self.ctx.types, source)
            || crate::query_boundaries::assignability::contains_type_parameters(
                self.ctx.types,
                target,
            )
        {
            return false;
        }

        let source_resolved = self.evaluate_type_with_resolution(source);
        let target_resolved = self.evaluate_type_with_resolution(target);

        // Tuples are already handled element-wise by the solver's
        // `types_are_comparable_for_assertion` (see flow.rs); the property-bag
        // view here would treat the implicit `length` literal as a shared
        // comparable property and falsely report overlap for casts like
        // `[C, D] as [A, I]` where the elements don't overlap. Defer to the
        // solver's tuple logic.
        let source_is_tuple =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, source_resolved)
                .is_some();
        let target_is_tuple =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, target_resolved)
                .is_some();
        if source_is_tuple && target_is_tuple {
            return false;
        }

        let Some(source_shape) = object_shape_for_type(self.ctx.types, source_resolved) else {
            return false;
        };

        // When target is an intersection, source must overlap with every member.
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, target_resolved)
        {
            return members
                .iter()
                .all(|&member| self.object_properties_are_comparable(source, member));
        }

        let Some(target_shape) = object_shape_for_type(self.ctx.types, target_resolved) else {
            return false;
        };

        // Skip for types with private/protected members. Classes with private
        // properties use nominal checking — the comparable relation requires
        // matching declarations, not just structural overlap.
        let has_non_public = source_shape
            .properties
            .iter()
            .chain(target_shape.properties.iter())
            .any(|p| p.visibility != Visibility::Public);
        if has_non_public {
            return false;
        }

        // Need at least one common property
        let mut found_common = false;

        for target_prop in &target_shape.properties {
            if let Some(source_prop) = source_shape
                .properties
                .iter()
                .find(|p| p.name == target_prop.name)
            {
                found_common = true;
                // Property types must be comparable (assignable in at least one direction)
                let prop_comparable = self
                    .diagnostic_relation_boolean_guard(source_prop.type_id, target_prop.type_id)
                    || self.diagnostic_relation_boolean_guard(
                        target_prop.type_id,
                        source_prop.type_id,
                    );
                if !prop_comparable {
                    return false;
                }
            }
        }

        found_common
    }

    /// Check if source object literal has properties that don't exist in target.
    ///
    pub(crate) fn analyze_assignability_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
        // `S[T1]` vs `S[T2]` where T1/T2 are distinct type parameters
        // collapses to the same evaluated shape (the constraint), so the
        // structural pipeline loses the IndexAccess form before it can
        // produce the TS2322 + TS5075 elaboration chain. Detect the
        // mismatch on the unevaluated inputs and surface it directly.
        if let Some(reason) =
            crate::query_boundaries::assignability::index_access_pair_distinct_type_param_keys_failure_reason(
                self.ctx.types,
                &self.ctx.definition_store,
                source,
                target,
            )
        {
            return crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
                weak_union_violation: false,
                failure_reason: Some(reason),
            };
        }

        // The abstract→non-abstract constructor rejection lives in a checker
        // override (it needs symbol abstractness) and is detected on the raw
        // declared types, so the structural failure walk over the evaluated
        // inputs produces no reason. Surface the structured reason directly so
        // the TS2517 elaboration is rendered after the top-level TS2322/TS2345.
        if let Some(reason) = self.abstract_constructor_assignment_failure_reason(source, target) {
            return crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
                weak_union_violation: false,
                failure_reason: Some(reason),
            };
        }

        // Same-generic application (`C<A..>` vs `C<B..>`): tsc elaborates the
        // differing type arguments directly. The structural pipeline evaluates
        // the applications to object shapes (losing the type-argument identity)
        // and would emit a `Types of property 'x' are incompatible.` wrapper
        // that tsc does not produce, so detect it on the raw operands first.
        if let Some(reason) =
            crate::query_boundaries::assignability::same_generic_application_failure_reason(
                self.ctx.types,
                &self.ctx,
                &self.ctx,
                source,
                target,
            )
        {
            return crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
                weak_union_violation: false,
                failure_reason: Some(reason),
            };
        }

        let (prepared_source, prepared_target) = self.prepare_assignability_inputs(source, target);

        // Keep failure analysis on the same relation boundary as `is_assignable_to`
        // (CheckerContext resolver + checker overrides) so mismatch suppression and
        // diagnostic rendering observe identical compatibility semantics.
        let overrides = CheckerOverrideProvider::new(self, None);
        let inputs = AssignabilityQueryInputs {
            db: self.ctx.types,
            resolver: &self.ctx,
            source: prepared_source,
            target: prepared_target,
            flags: self.ctx.pack_relation_flags(),
            inheritance_graph: &self.ctx.inheritance_graph,
            sound_mode: self.ctx.sound_mode(),
        };
        let gate = check_assignable_gate_with_overrides(&inputs, &overrides, Some(&self.ctx), true);
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

        crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
            weak_union_violation: result.weak_union_violation,
            failure_reason,
        }
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
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(source)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) != 0
            && let Some(parent) = self.ctx.binder.get_symbol(symbol.parent)
        {
            return parent.escaped_name.to_string();
        }
        self.format_type_diagnostic(source)
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
        if let Some(return_type) =
            crate::query_boundaries::common::return_type_for_type(self.ctx.types, resolved_source)
            && return_type != TypeId::VOID
            && return_type != TypeId::UNDEFINED
            && return_type != TypeId::NEVER
            && self.diagnostic_relation_boolean_guard(return_type, target)
        {
            return true;
        }

        // Check construct signatures — use get_construct_signatures directly
        // which handles Callable types and intersections.
        if let Some(sigs) = crate::query_boundaries::common::construct_signatures_for_type(
            self.ctx.types,
            resolved_source,
        ) && let Some(first_sig) = sigs.first()
        {
            let construct_return = first_sig.return_type;
            if construct_return != TypeId::VOID
                && construct_return != TypeId::UNDEFINED
                && construct_return != TypeId::NEVER
                && self.diagnostic_relation_boolean_guard(construct_return, target)
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
        crate::query_boundaries::common::type_may_display_iterator_protocol(self.ctx.types, source)
            && crate::query_boundaries::common::type_may_display_iterator_protocol(
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

        !self.diagnostic_relation_boolean_guard(TypeId::UNDEFINED, value_type)
    }

    fn iterator_result_application_args(&self, type_id: TypeId) -> Option<Vec<TypeId>> {
        let (base, args) = self.application_info_or_display_alias(type_id)?;
        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, base)
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
        let target_display = self.format_type(target);
        let Some(target_return) = function_return_display(&target_display) else {
            return false;
        };
        let Some((target_name, target_args)) = parse_simple_type_application_display(target_return)
        else {
            return false;
        };
        if target_name != "IteratorResult" || target_args.get(1).copied() != Some("unknown") {
            return false;
        }

        let source_display = self.format_type(source);
        let Some(source_return) = function_return_display(&source_display) else {
            return false;
        };
        source_return.contains("done: boolean")
            || (source_return.contains("done: true") && !source_return.contains("value: undefined"))
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
