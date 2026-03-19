//! Assignability diagnostic reporting and excess property checking.
//!
//! Contains the "report" side of assignability: methods that call the core
//! `is_assignable_to` entrypoints and emit diagnostics when types are incompatible.

use crate::query_boundaries::assignability::{
    AssignabilityQueryInputs, ExcessPropertiesKind, check_assignable_gate_with_overrides,
    classify_for_excess_properties, get_keyof_type, get_string_literal_value, is_keyof_type,
    is_type_parameter_like, object_shape_for_type,
};
use crate::state::{CheckerOverrideProvider, CheckerState};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

// =============================================================================
// Weak Union, Excess Property, and Diagnostic Reporting Methods
// =============================================================================

impl<'a> CheckerState<'a> {
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
        let Some(node) = self.ctx.arena.get(source_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        // Check for weak union violation first (using scoped borrow)
        if self.is_weak_union_violation(source, target) {
            return true;
        }

        // Check if there are excess properties.
        if !self.object_literal_has_excess_properties(source, target, source_idx) {
            return false;
        }

        // There are excess properties. Check if all matching properties have compatible types.
        let Some(source_shape) = object_shape_for_type(self.ctx.types, source) else {
            return true;
        };

        let resolved_target = self.resolve_type_for_property_access(target);
        let Some(target_shape) = object_shape_for_type(self.ctx.types, resolved_target) else {
            // If we can't extract a simple object shape from the target (e.g., it's
            // an intersection with a deferred conditional type), we should NOT skip
            // the assignability error. The solver already determined the types are
            // incompatible, and inability to extract properties for excess-property
            // analysis doesn't mean the assignment is valid.
            return false;
        };

        let source_props = source_shape.properties.as_slice();
        let target_props = target_shape.properties.as_slice();

        // Check if any source property that exists in target has a wrong type.
        // Also collect the matching properties so we can verify structural assignability.
        let mut matching_props = Vec::new();
        for source_prop in source_props {
            if let Some(target_prop) = target_props.iter().find(|p| p.name == source_prop.name) {
                let source_prop_type = source_prop.type_id;
                let target_prop_type = target_prop.type_id;

                let effective_target_type = if target_prop.optional {
                    self.ctx
                        .types
                        .union(vec![target_prop_type, TypeId::UNDEFINED])
                } else {
                    target_prop_type
                };

                let is_assignable =
                    { self.is_assignable_to(source_prop_type, effective_target_type) };

                if !is_assignable {
                    return false;
                }
                matching_props.push(source_prop.clone());
            }
        }

        // All matching properties are compatible. Verify that the failure is truly
        // caused by excess properties alone by checking if an object with only the
        // matching properties would be assignable. If not, the failure is structural
        // (e.g., target contains a deferred conditional type) and we should NOT
        // suppress TS2322.
        let trimmed_source = self.ctx.types.object(matching_props);
        if !self.is_assignable_to(trimmed_source, target) {
            return false;
        }

        true
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

        if is_keyof_type(self.ctx.types, target)
            && let Some(str_lit) = get_string_literal_value(self.ctx.types, source)
        {
            let keyof_type = get_keyof_type(self.ctx.types, target)
                .expect("is_keyof_type guard ensures this succeeds");
            let allowed_keys = self.get_keyof_type_keys(keyof_type, self.ctx.types);
            if !allowed_keys.contains(&str_lit) {
                self.error_type_does_not_satisfy_the_expected_type(
                    source,
                    target,
                    diag_idx,
                    keyword_pos,
                );
                return false;
            }
        }

        // Track whether excess property checking emits diagnostics.
        // When TS2353 is emitted for excess properties, tsc does NOT also emit TS1360.
        let mut had_excess_property_error = false;
        if let Some(node) = self.ctx.arena.get(source_idx)
            && node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            let diags_before = self.ctx.diagnostics.len();
            self.check_object_literal_excess_properties(source, target, source_idx);
            had_excess_property_error = self.ctx.diagnostics.len() > diags_before;
        }

        if self.is_assignable_to(source, target)
            || self.should_skip_weak_union_error(source, target, source_idx)
        {
            return true;
        }

        // tsc 6.0: `satisfies` ignores readonly-to-mutable mismatches.
        // `[1,2,3] as const satisfies unknown[]` is accepted because `satisfies`
        // checks structural shape, not mutability. If the source is Readonly<T>,
        // try checking T against the target.
        if let Some(inner) = tsz_solver::readonly_inner_type(self.ctx.types, source)
            && self.is_assignable_to(inner, target)
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
        let resolved_target = self.resolve_type_for_property_access(target);
        let target_shape = match object_shape_for_type(self.ctx.types, resolved_target) {
            Some(shape) => shape,
            None => return false,
        };

        // Get the index signature value type from the target
        let index_value_type = target_shape.string_index.as_ref().map(|sig| sig.value_type);

        let Some(index_value_type) = index_value_type else {
            // No string index signature — try elaborating against named target properties.
            // For targets with named properties (like interfaces), check if there are
            // missing required properties (TS2741 elaboration) — handled elsewhere.
            return false;
        };

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
            if elem_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
                continue;
            }
            let Some(prop_data) = self.ctx.arena.get_property_assignment(elem_node) else {
                continue;
            };

            // Get the type of the property value (the initializer)
            let prop_value_type = self.get_type_of_node(prop_data.initializer);
            self.ensure_relation_input_ready(prop_value_type);
            self.ensure_relation_input_ready(index_value_type);

            // Check nested object literal excess properties FIRST — tsc prioritizes
            // excess property errors (TS2353) over assignability errors (TS2322).
            // e.g., `{ r: 0, g: 0, d: 0 }` vs `Color` reports "d does not exist" (TS2353)
            // rather than "missing b" (TS2322).
            if let Some(val_node) = self.ctx.arena.get(prop_data.initializer)
                && val_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            {
                let diags_before = self.ctx.diagnostics.len();
                self.check_object_literal_excess_properties(
                    prop_value_type,
                    index_value_type,
                    prop_data.initializer,
                );
                if self.ctx.diagnostics.len() > diags_before {
                    // Excess property errors were reported — skip assignability check
                    continue;
                }
            }

            if !self.is_assignable_to(prop_value_type, index_value_type) {
                // Report TS2322 at the property name (use _with_anchor to avoid
                // assignment_diagnostic_anchor_idx walking up to the variable declaration)
                self.error_type_not_assignable_at_with_anchor(
                    prop_value_type,
                    index_value_type,
                    prop_data.name,
                );
            }
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
    pub(crate) fn check_assignable_or_report_at(
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

        if self.is_assignable_to(source, target)
            || self.should_skip_weak_union_error(source, target, source_idx)
        {
            return true;
        }
        if self.try_elaborate_assignment_source_error(source_idx, target) {
            return false;
        }
        self.error_type_not_assignable_with_reason_at(source, target, diag_idx);
        false
    }

    fn numeric_enum_assignment_override_from_source(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> Option<bool> {
        use tsz_solver::TypeResolver;
        let target = self.evaluate_type_for_assignability(target);
        let target_def_id = tsz_solver::type_queries::get_enum_def_id(self.ctx.types, target)?;
        if !self.ctx.is_numeric_enum(target_def_id) {
            return None;
        }

        let source_literal = self.literal_type_from_initializer(source_idx);
        let source_is_number_like = source == TypeId::NUMBER
            || source_literal.is_some_and(|lit| {
                tsz_solver::type_queries::extended::is_number_literal(self.ctx.types, lit)
            });
        if !source_is_number_like {
            return None;
        }

        if self.ctx.is_enum_type(target, self.ctx.types) {
            if let Some(source_literal) = source_literal {
                let structural_target =
                    tsz_solver::type_queries::data::get_enum_member_type(self.ctx.types, target)
                        .unwrap_or(target);
                return Some(self.is_assignable_to(source_literal, structural_target));
            }
            return None;
        }

        let target_member =
            tsz_solver::type_queries::data::get_enum_member_type(self.ctx.types, target);
        let target_literal =
            target_member.and_then(|member| tsz_solver::literal_value(self.ctx.types, member));

        target_member?;

        match source_literal {
            Some(source_literal) => {
                let source_val = tsz_solver::literal_value(self.ctx.types, source_literal);
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
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, diag_idx) {
            return true;
        }
        if self.is_assignable_to(source, target)
            || self.should_skip_weak_union_error(source, target, source_idx)
        {
            return true;
        }
        if self.try_elaborate_assignment_source_error(source_idx, target) {
            return false;
        }
        self.error_type_not_assignable_with_reason_at_anchor(source, target, diag_idx);
        false
    }

    /// Check assignability and emit a generic TS2322 diagnostic at `diag_idx`.
    ///
    /// This is used for call sites that intentionally avoid detailed reason rendering
    /// but still share centralized mismatch/suppression behavior.
    pub(crate) fn check_assignable_or_report_generic_at(
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
        if self.is_assignable_to(source, target)
            || self.should_skip_weak_union_error(source, target, source_idx)
        {
            return true;
        }
        self.error_type_not_assignable_generic_at(source, target, diag_idx);
        false
    }

    /// Check assignability and emit argument-not-assignable diagnostics (TS2345-style).
    ///
    /// Returns true when no diagnostic was emitted (assignable or intentionally skipped),
    /// false when an argument-assignability diagnostic was emitted.
    pub(crate) fn check_argument_assignable_or_report(
        &mut self,
        source: TypeId,
        target: TypeId,
        arg_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(arg_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return true;
        }
        if self.should_suppress_assignability_for_parse_recovery(arg_idx, arg_idx) {
            return true;
        }
        if self.is_assignable_to(source, target) {
            return true;
        }
        if self.should_skip_weak_union_error(source, target, arg_idx) {
            return true;
        }
        // Conditional/generic callback contexts can narrow argument callback parameter
        // types to intersections involving type parameters (e.g. `number & T`).
        // In these cases, strict contravariant checking reports TS2345 even when the
        // concrete expected callback type is assignable to the narrowed callback.
        // tsc defers this mismatch.
        if crate::query_boundaries::assignability::contains_type_parameters(self.ctx.types, source)
            && !crate::query_boundaries::assignability::contains_type_parameters(
                self.ctx.types,
                target,
            )
            && tsz_solver::type_queries::is_callable_type(self.ctx.types, source)
            && tsz_solver::type_queries::is_callable_type(self.ctx.types, target)
            && !self.callable_has_own_generic_signatures(source)
            && self.ctx.types.is_assignable_to(target, source)
        {
            return true;
        }
        self.error_argument_not_assignable_at(source, target, arg_idx);
        false
    }

    /// Returns true when an assignability mismatch should produce a diagnostic.
    ///
    /// This centralizes the standard "not assignable + not weak-union/excess-property
    /// suppression" decision so call sites emitting different diagnostics can share it.
    pub(crate) fn should_report_assignability_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return false;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, source_idx) {
            return false;
        }
        !self.is_assignable_to(source, target)
            && !self.should_skip_weak_union_error(source, target, source_idx)
    }

    /// Returns true when a bivariant-assignability mismatch should produce a diagnostic.
    ///
    /// Mirrors `should_report_assignability_mismatch` but uses the bivariant relation
    /// entrypoint for method-compatibility scenarios.
    pub(crate) fn should_report_assignability_mismatch_bivariant(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_idx: NodeIndex,
    ) -> bool {
        let source = self.narrow_this_from_enclosing_typeof_guard(source_idx, source);
        if self.should_suppress_assignability_diagnostic(source, target) {
            return false;
        }
        if self.should_suppress_assignability_for_parse_recovery(source_idx, source_idx) {
            return false;
        }
        !self.is_assignable_to_bivariant(source, target)
            && !self.should_skip_weak_union_error(source, target, source_idx)
    }

    /// Check bidirectional assignability.
    ///
    /// Useful in checker locations that need type comparability/equivalence-like checks.
    pub(crate) fn are_mutually_assignable(&mut self, left: TypeId, right: TypeId) -> bool {
        self.is_assignable_to(left, right) && self.is_assignable_to(right, left)
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

        // Fast path: direct bidirectional assignability (with apparent types)
        if self.is_assignable_to(source_apparent, target_apparent)
            || self.is_assignable_to(target_apparent, source_apparent)
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
                if self.is_assignable_to(*member, target_apparent)
                    || self.is_assignable_to(target_apparent, *member)
                {
                    return true;
                }
            }
        }

        // Decompose target union: check if any member is assignable in either direction
        if let Some(members) = query::union_members(self.ctx.types, target_apparent) {
            for member in &members {
                if self.is_assignable_to(source_apparent, *member)
                    || self.is_assignable_to(*member, source_apparent)
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
                if self.is_assignable_to(*member, target_apparent)
                    || self.is_assignable_to(target_apparent, *member)
                {
                    return true;
                }
            }
        }
        if let Some(members) = query::intersection_members(self.ctx.types, target_apparent) {
            for member in &members {
                if self.is_assignable_to(source_apparent, *member)
                    || self.is_assignable_to(*member, source_apparent)
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

        false
    }

    /// Check if source object literal has properties that don't exist in target.
    ///
    /// Uses TypeId-based freshness tracking (fresh object literals only).
    pub(crate) fn object_literal_has_excess_properties(
        &mut self,
        source: TypeId,
        target: TypeId,
        _source_idx: NodeIndex,
    ) -> bool {
        use tsz_solver::relations::freshness;
        // Only fresh object literals trigger excess property checking.
        if !freshness::is_fresh_object_type(self.ctx.types, source) {
            return false;
        }

        let Some(source_shape) = object_shape_for_type(self.ctx.types, source) else {
            return false;
        };

        let source_props = source_shape.properties.as_slice();
        if source_props.is_empty() {
            return false;
        }

        let resolved_target = self.resolve_type_for_property_access(target);

        match classify_for_excess_properties(self.ctx.types, resolved_target) {
            ExcessPropertiesKind::Object(shape_id) => {
                let target_shape = self.ctx.types.object_shape(shape_id);
                let target_props = target_shape.properties.as_slice();

                if target_props.is_empty() {
                    return false;
                }

                if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
                    return false;
                }

                source_props
                    .iter()
                    .any(|source_prop| !target_props.iter().any(|p| p.name == source_prop.name))
            }
            ExcessPropertiesKind::ObjectWithIndex(_shape_id) => false,
            ExcessPropertiesKind::Union(members) => {
                let mut target_shapes = Vec::new();
                let mut matched_shapes = Vec::new();

                for member in members {
                    let resolved_member = self.resolve_type_for_property_access(member);
                    let Some(shape) = object_shape_for_type(self.ctx.types, resolved_member) else {
                        // If a union member has no object shape and is a type parameter
                        // or the `object` intrinsic, it accepts any properties, so EPC
                        // should not apply.
                        if is_type_parameter_like(self.ctx.types, resolved_member)
                            || resolved_member == TypeId::OBJECT
                        {
                            return false;
                        }
                        continue;
                    };

                    if shape.properties.is_empty()
                        || shape.string_index.is_some()
                        || shape.number_index.is_some()
                    {
                        return false;
                    }

                    target_shapes.push(shape.clone());

                    if self.is_subtype_of(source, resolved_member) {
                        matched_shapes.push(shape);
                    }
                }

                if target_shapes.is_empty() {
                    return false;
                }

                let effective_shapes = if matched_shapes.is_empty() {
                    target_shapes
                } else {
                    matched_shapes
                };

                source_props.iter().any(|source_prop| {
                    !effective_shapes.iter().any(|shape| {
                        shape
                            .properties
                            .iter()
                            .any(|prop| prop.name == source_prop.name)
                    })
                })
            }
            ExcessPropertiesKind::Intersection(members) => {
                let mut target_shapes = Vec::new();

                for member in members {
                    let resolved_member = self.resolve_type_for_property_access(member);
                    let Some(shape) = object_shape_for_type(self.ctx.types, resolved_member) else {
                        // If an intersection member is a type parameter, it could accept
                        // any properties, so EPC should not apply (same logic as union case).
                        if is_type_parameter_like(self.ctx.types, resolved_member)
                            || resolved_member == TypeId::OBJECT
                        {
                            return false;
                        }
                        continue;
                    };

                    if shape.string_index.is_some() || shape.number_index.is_some() {
                        return false;
                    }

                    target_shapes.push(shape);
                }

                if target_shapes.is_empty() {
                    return false;
                }

                source_props.iter().any(|source_prop| {
                    !target_shapes.iter().any(|shape| {
                        shape
                            .properties
                            .iter()
                            .any(|prop| prop.name == source_prop.name)
                    })
                })
            }
            ExcessPropertiesKind::NotObject => false,
        }
    }

    pub(crate) fn analyze_assignability_failure(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
        // Keep failure analysis on the same relation boundary as `is_assignable_to`
        // (CheckerContext resolver + checker overrides) so mismatch suppression and
        // diagnostic rendering observe identical compatibility semantics.
        let overrides = CheckerOverrideProvider::new(self, None);
        let inputs = AssignabilityQueryInputs {
            db: self.ctx.types,
            resolver: &self.ctx,
            source,
            target,
            flags: self.ctx.pack_relation_flags(),
            inheritance_graph: &self.ctx.inheritance_graph,
            sound_mode: self.ctx.sound_mode(),
        };
        let gate = check_assignable_gate_with_overrides(&inputs, &overrides, Some(&self.ctx), true);
        if gate.related
            && let Some(reason) = self.checker_only_assignability_failure_reason(source, target)
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
        let mut result = gate.analysis.unwrap_or(
            crate::query_boundaries::assignability::AssignabilityFailureAnalysis {
                weak_union_violation: false,
                failure_reason: None,
            },
        );

        // When the failure is ExcessProperty but the target contains a deferred
        // conditional type, the real issue is structural (the deferred conditional
        // makes the assignment incompatible regardless of excess properties).
        // tsc emits TS2322 rather than TS2353 in this case. Evaluate the target
        // to check for conditional members and downgrade to a generic TS2322.
        if matches!(
            &result.failure_reason,
            Some(tsz_solver::SubtypeFailureReason::ExcessProperty { .. })
        ) {
            let evaluated_target = self.evaluate_type_for_assignability(target);
            if tsz_solver::has_deferred_conditional_member(self.ctx.types, evaluated_target) {
                result.failure_reason = None;
            }
            let has_non_epc_intersection_member = [target, evaluated_target]
                .into_iter()
                .filter_map(|candidate| {
                    tsz_solver::type_queries::data::get_intersection_members(
                        self.ctx.types,
                        candidate,
                    )
                })
                .any(|members| {
                    members.iter().any(|member| {
                        let evaluated_member = self.evaluate_type_for_assignability(*member);
                        tsz_solver::is_primitive_type(self.ctx.types, evaluated_member)
                            || tsz_solver::type_queries::is_type_parameter_like(
                                self.ctx.types,
                                evaluated_member,
                            )
                    })
                });
            if has_non_epc_intersection_member {
                result.failure_reason = None;
            }
        }

        result
    }

    pub(crate) fn is_weak_union_violation(&mut self, source: TypeId, target: TypeId) -> bool {
        self.analyze_assignability_failure(source, target)
            .weak_union_violation
    }

    pub(crate) const fn checker_only_assignability_failure_reason(
        &mut self,
        _source: TypeId,
        _target: TypeId,
    ) -> Option<tsz_solver::SubtypeFailureReason> {
        None
    }
}
