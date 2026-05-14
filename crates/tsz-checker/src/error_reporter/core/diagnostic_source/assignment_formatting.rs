//! Assignment-source/target type formatting for TS2322-family diagnostics.
//!
//! Extracted from `error_reporter/core/diagnostic_source.rs` to keep that
//! file under the LOC ceiling. Pure file-organization move; no logic changes.

use super::literal_widening_helpers::{
    literal_display_appropriate_for_undefined_null_target, simple_or_namespace_member_name,
    target_accepts_literal_primitive_kind,
};
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn source_type_contains_number_literal_only_union(
        &self,
        ty: TypeId,
    ) -> bool {
        let mut stack = vec![ty];
        let mut seen = FxHashSet::default();

        while let Some(current) = stack.pop() {
            if !seen.insert(current) {
                continue;
            }

            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, current)
            {
                if self.union_members_are_number_literals_or_common_intersections(&members) {
                    return true;
                }
                stack.extend(members);
                continue;
            }

            if let Some(members) =
                crate::query_boundaries::common::intersection_members(self.ctx.types, current)
            {
                stack.extend(members);
            }
        }

        false
    }

    fn union_members_are_number_literals_or_common_intersections(
        &self,
        members: &[TypeId],
    ) -> bool {
        if members.len() < 2 {
            return false;
        }

        let mut expected_non_numeric_parts: Option<FxHashSet<TypeId>> = None;
        for &member in members {
            let Some(non_numeric_parts) =
                self.number_literal_union_member_non_numeric_intersection_parts(member)
            else {
                return false;
            };

            if let Some(expected) = &expected_non_numeric_parts {
                if *expected != non_numeric_parts {
                    return false;
                }
            } else {
                expected_non_numeric_parts = Some(non_numeric_parts);
            }
        }

        true
    }

    fn number_literal_union_member_non_numeric_intersection_parts(
        &self,
        member: TypeId,
    ) -> Option<FxHashSet<TypeId>> {
        if matches!(
            crate::query_boundaries::common::literal_value(self.ctx.types, member),
            Some(crate::query_boundaries::common::LiteralValue::Number(_))
        ) {
            return Some(FxHashSet::default());
        }

        let intersection_members =
            crate::query_boundaries::common::intersection_members(self.ctx.types, member)?;
        let mut saw_number_literal = false;
        let mut non_numeric_parts = FxHashSet::default();
        for part in intersection_members {
            if matches!(
                crate::query_boundaries::common::literal_value(self.ctx.types, part),
                Some(crate::query_boundaries::common::LiteralValue::Number(_))
            ) {
                if saw_number_literal {
                    return None;
                }
                saw_number_literal = true;
            } else {
                non_numeric_parts.insert(part);
            }
        }

        saw_number_literal.then_some(non_numeric_parts)
    }

    pub(in crate::error_reporter) fn format_assignment_source_type_for_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        // For property-access source expressions whose underlying value type is
        // a `unique symbol` (e.g. `Symbol.toPrimitive`), tsc displays the source
        // as `typeof <expr>` rather than widening to `symbol`. Match that here
        // before any widening below collapses the source to its primitive.
        if let Some(display) = self.typeof_unique_symbol_source_display(anchor_idx) {
            return display;
        }
        if let Some(display) =
            self.js_constructor_instance_assignment_source_display(source, anchor_idx)
        {
            return display;
        }

        let has_optional_callable_param =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, source)
                .is_some_and(|shape| shape.params.iter().any(|param| param.optional))
                || crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, source)
                    .is_some_and(|shape| {
                        shape
                            .call_signatures
                            .iter()
                            .chain(shape.construct_signatures.iter())
                            .any(|sig| sig.params.iter().any(|param| param.optional))
                    });
        if has_optional_callable_param {
            return self.format_assignability_type_for_message(source, target);
        }

        if let Some(display) = self.tuple_structural_source_display(source, target) {
            return display;
        }

        if let Some(expr_idx) = self.assignment_source_expression(anchor_idx)
            && let Some(display) =
                self.array_literal_tuple_source_type_display(expr_idx, source, target)
        {
            return display;
        }

        if source == TypeId::UNDEFINED
            && self.ctx.arena.get(anchor_idx).is_some_and(|node| {
                node.kind == tsz_parser::parser::syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            })
        {
            return self.format_assignability_type_for_message(source, target);
        }

        // Generic intersection source reduction: when the source is an intersection
        // containing type parameters (e.g., `T & U`), tsc displays the reduced base
        // constraint instead of the raw generic intersection.  For example,
        // `T extends string | number | undefined` and `U extends string | null | undefined`
        // display as `string | undefined` rather than `T & U`.
        //
        // This matches tsc's `getBaseConstraintOfType` behavior for intersection types
        // in error messages.
        if let Some(reduced) = self.generic_intersection_source_display_substitution(source) {
            return self.format_type_for_assignability_message(reduced);
        }

        // For Lazy(DefId) source types representing named interfaces (non-generic),
        // return the interface name directly. This prevents get_type_of_node from
        // resolving the Lazy to its structural form, losing the name (e.g., showing
        // "{ constraint: Constraint<this>; ... }" instead of "Num").
        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, source)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind == tsz_solver::def::DefKind::Interface
            && def.type_params.is_empty()
        {
            let name = self.ctx.types.resolve_atom_ref(def.name);
            return name.to_string();
        }

        if let Some(display) = self.jsdoc_annotated_expression_display(anchor_idx, target) {
            return display;
        }

        if crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some()
            && crate::query_boundaries::common::string_intrinsic_components(self.ctx.types, target)
                .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }
        if crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some()
            && self.keyof_type_alias_body_display(target).is_some()
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }
        if let Some(target_expr) = self.assignment_target_expression(anchor_idx)
            && self
                .keyof_type_alias_annotation_display_for_expression(target_expr)
                .is_some()
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }
        if let Some(annotation) = self.direct_assignment_target_annotation_text(anchor_idx)
            && self
                .keyof_type_alias_annotation_display(&annotation)
                .is_some()
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }

        if let Some(expr_idx) = self.assignment_source_expression(anchor_idx)
            && let Some(display) =
                self.array_literal_tuple_source_type_display(expr_idx, source, target)
        {
            return display;
        }

        let in_arith_compound = self.in_arithmetic_compound_assignment_context(anchor_idx);
        if let Some(display) = self.literal_assignment_source_display_for_target(target, anchor_idx)
        {
            return display;
        }

        if let Some(display) = self.preferred_evaluated_source_display(source, target) {
            return display;
        }

        if let Some(display_type) = self.string_covered_template_union_source_display(source) {
            return self.format_assignability_type_for_message(display_type, target);
        }

        if let Some(display) = self.related_generic_indexed_access_source_display(source, target) {
            return display;
        }

        if !in_arith_compound
            && self.array_literal_element_source_widening_required_for_display(
                anchor_idx, source, target,
            )
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }

        if !in_arith_compound
            && self.is_literal_sensitive_assignment_target(target)
            && let Some(display) = self.literal_expression_display(anchor_idx)
            && literal_display_appropriate_for_undefined_null_target(
                self.ctx.types,
                target,
                &display,
            )
        {
            return display;
        }
        if !in_arith_compound
            && self.is_literal_sensitive_assignment_target(target)
            && crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some()
        {
            return self.format_assignability_type_for_message(source, target);
        }

        if self.is_object_rest_assignment_target_anchor(anchor_idx) {
            return self.format_type_for_assignability_message(source);
        }

        if let Some(expr_idx) = self.direct_diagnostic_source_expression(anchor_idx) {
            if !in_arith_compound
                && self.is_literal_sensitive_assignment_target(target)
                && let Some(display) = self.literal_expression_display(expr_idx)
                && literal_display_appropriate_for_undefined_null_target(
                    self.ctx.types,
                    target,
                    &display,
                )
            {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) =
                self.array_literal_tuple_source_type_display(expr_idx, source, target)
            {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            if source != TypeId::UNKNOWN
                && (expr_type == TypeId::UNKNOWN || expr_type == source)
                && crate::query_boundaries::common::is_empty_object_type(self.ctx.types, source)
            {
                return self.format_assignability_type_for_message(source, target);
            }
            let expr_display_type = if expr_type == TypeId::UNKNOWN && source != TypeId::UNKNOWN {
                source
            } else {
                expr_type
            };
            if self.should_preserve_nuia_source_undefined_display(
                source,
                target,
                expr_idx,
                expr_display_type,
            ) {
                return self.format_type_for_assignability_message(source);
            }
            let node_is_array_of_source = crate::query_boundaries::common::array_element_type(
                self.ctx.types,
                expr_display_type,
            )
            .is_some_and(|elem| elem == source);
            if node_is_array_of_source {
                return self.format_assignability_type_for_message(source, target);
            }
            let node_is_target_not_source =
                expr_display_type == target && expr_display_type != source;
            let node_type_matches_source =
                expr_display_type != TypeId::ERROR && !node_is_target_not_source;
            if node_type_matches_source {
                if !in_arith_compound
                    && crate::query_boundaries::common::is_template_literal_type(
                        self.ctx.types,
                        target,
                    )
                    && let Some(display) = self.literal_expression_display(expr_idx)
                    && literal_display_appropriate_for_undefined_null_target(
                        self.ctx.types,
                        target,
                        &display,
                    )
                {
                    return display;
                }
                let preserve_literal_surface = self.target_preserves_literal_surface(target);
                if let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
                    && self.should_prefer_declared_source_annotation_display(
                        expr_idx,
                        expr_display_type,
                        &annotation_text,
                    )
                {
                    if let Some(display) =
                        self.declared_intersection_annotation_display_for_expression(expr_idx)
                    {
                        return display;
                    }
                    return self.format_declared_annotation_for_diagnostic(&annotation_text);
                }
                let display_type =
                    if self.should_widen_enum_member_assignment_source(expr_display_type, target) {
                        self.widen_enum_member_type(expr_display_type)
                    } else {
                        expr_display_type
                    };
                let display_type = self.widen_function_like_display_type(display_type);
                let display_type = if self.is_literal_sensitive_assignment_target(target)
                    || preserve_literal_surface
                {
                    display_type
                } else if crate::query_boundaries::common::keyof_inner_type(
                    self.ctx.types,
                    display_type,
                )
                .is_some()
                {
                    let evaluated = self.evaluate_type_for_assignability(display_type);
                    crate::query_boundaries::common::widen_type(self.ctx.types, evaluated)
                } else {
                    crate::query_boundaries::common::widen_type(self.ctx.types, display_type)
                };
                if let Some(display) =
                    self.new_expression_nominal_source_display(expr_idx, display_type)
                {
                    return display;
                }
                if crate::query_boundaries::common::array_element_type(self.ctx.types, display_type)
                    == Some(TypeId::UNKNOWN)
                    && let Some(display) = self.call_unknown_array_source_display(expr_idx, target)
                {
                    return display;
                }
                if let Some(display) =
                    self.declared_identifier_source_display(expr_idx, target, expr_display_type)
                {
                    return display;
                }
                if let Some(display) =
                    self.direct_type_query_primitive_source_display(expr_idx, display_type)
                {
                    return display;
                }
                if let Some(display) = self.rebuilt_array_source_display(display_type, target) {
                    return display;
                }
                // When widening rebuilt the type into a structurally-equivalent but
                // distinct `TypeId`, the new id does not carry the original
                // `TypeAlias` registration (`find_def_for_type`). The diagnostic
                // formatter relies on that registration to render the alias name
                // (`SimpleType`) instead of the expanded body
                // (`string | Promise<SimpleType>`). When the original is a
                // registered `TypeAlias`, format the original `TypeId` so the
                // printer recovers the alias name.
                let formatting_type = if display_type != expr_display_type
                    && self.is_registered_type_alias_for_display(expr_display_type)
                {
                    expr_display_type
                } else {
                    display_type
                };
                return self.format_assignability_type_for_message(formatting_type, target);
            }

            if node_type_matches_source
                && let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
            {
                if let Some(intersection_display) =
                    self.declared_intersection_annotation_display_for_expression(expr_idx)
                {
                    return intersection_display;
                }
                return display;
            }
        }
        if let Some(expr_idx) = self.assignment_source_expression(anchor_idx) {
            if let Some(display) = self.type_assertion_mapped_alias_source_display(expr_idx) {
                return display;
            }
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
                && display.contains("=>")
            {
                return self.format_annotation_like_type(&display);
            }
            if let Some(display) = self.literal_expression_display(expr_idx)
                && !self.in_arithmetic_compound_assignment_context(anchor_idx)
                && (self.is_literal_sensitive_assignment_target(target)
                    || (self.assignment_source_is_return_expression(anchor_idx)
                        && crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            target,
                        )
                        && !self.is_property_assignment_initializer(expr_idx)
                        // When the target is a bare type parameter (e.g. T),
                        // tsc widens literals in error messages: "Type 'string'
                        // is not assignable to type 'T'" rather than "Type '\"\"'
                        // is not assignable to type 'T'". Preserve literals only
                        // for complex generic targets like indexed access types.
                        && !self.target_is_bare_type_parameter(target)))
                // For pre-widened property-elaboration sources, mirror tsc's
                // `getWidenedLiteralLikeTypeForContextualType`: only resurrect
                // the AST literal display when the source's primitive kind has
                // a matching literal kind in the target. Cross-primitive cases
                // (e.g. numeric literal `1` against boolean literal `true`)
                // widen the source so the diagnostic shows
                // `Type 'number' is not assignable to type 'true'.` instead of
                // `Type '1' ...`. Direct same-primitive mismatches like
                // `"bar"` vs `"foo"` keep the literal display.
                && !self.property_elaboration_widening_required_for_display(
                    expr_idx, source, target,
                )
                && literal_display_appropriate_for_undefined_null_target(
                    self.ctx.types,
                    target,
                    &display,
                )
            {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) =
                self.array_literal_tuple_source_type_display(expr_idx, source, target)
            {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            if source != TypeId::UNKNOWN
                && (expr_type == TypeId::UNKNOWN || expr_type == source)
                && crate::query_boundaries::common::is_empty_object_type(self.ctx.types, source)
            {
                return self.format_assignability_type_for_message(source, target);
            }
            let expr_display_type = if expr_type == TypeId::UNKNOWN && source != TypeId::UNKNOWN {
                source
            } else {
                expr_type
            };
            if self.should_preserve_nuia_source_undefined_display(
                source,
                target,
                expr_idx,
                expr_display_type,
            ) {
                return self.format_type_for_assignability_message(source);
            }
            let preserve_literal_surface = self.target_preserves_literal_surface(target);
            if expr_type != TypeId::ERROR
                && let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
            {
                let expr_enum_symbol = self
                    .enum_symbol_from_enumish_type(expr_display_type)
                    .or_else(|| self.enum_symbol_from_enumish_type(source));
                let target_enum_symbol = self.enum_symbol_from_enumish_type(target);
                if expr_enum_symbol.is_some()
                    && expr_enum_symbol == target_enum_symbol
                    && !annotation_text.contains(" | ")
                    && !annotation_text.contains(" & ")
                    && !annotation_text.contains('<')
                {
                    return self.format_declared_annotation_for_diagnostic(&annotation_text);
                }
            }
            if expr_type != TypeId::ERROR
                && let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
                && self.should_prefer_declared_source_annotation_display(
                    expr_idx,
                    expr_display_type,
                    &annotation_text,
                )
            {
                if let Some(display) =
                    self.declared_intersection_annotation_display_for_expression(expr_idx)
                {
                    return display;
                }
                return self.format_declared_annotation_for_diagnostic(&annotation_text);
            }
            let display_type = if expr_display_type != TypeId::ERROR {
                let widened_expr_type = if preserve_literal_surface {
                    expr_display_type
                } else {
                    self.widen_type_for_display(expr_display_type)
                };
                if self.should_widen_enum_member_assignment_source(widened_expr_type, target) {
                    self.widen_enum_member_type(widened_expr_type)
                } else {
                    widened_expr_type
                }
            } else {
                self.widen_type_for_display(source)
            };
            let display_type = self.widen_function_like_display_type(display_type);
            if let Some(display) =
                self.new_expression_nominal_source_display(expr_idx, display_type)
            {
                return display;
            }
            if crate::query_boundaries::common::array_element_type(self.ctx.types, display_type)
                == Some(TypeId::UNKNOWN)
                && let Some(display) = self.call_unknown_array_source_display(expr_idx, target)
            {
                return display;
            }
            if let Some(display) =
                self.declared_identifier_source_display(expr_idx, target, expr_display_type)
            {
                return display;
            }
            if let Some(display) =
                self.direct_type_query_primitive_source_display(expr_idx, display_type)
            {
                return display;
            }
            if let Some(display) = self.rebuilt_array_source_display(display_type, target) {
                return display;
            }

            if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && symbol.has_any_flags(tsz_binder::symbol_flags::ENUM)
                && !symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
            {
                return self.format_assignability_type_for_message(display_type, target);
            }

            if expr_type == TypeId::ERROR
                && let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
            {
                if let Some(intersection_display) =
                    self.declared_intersection_annotation_display_for_expression(expr_idx)
                {
                    return intersection_display;
                }
                return display;
            }

            let display_type =
                if crate::query_boundaries::common::keyof_inner_type(self.ctx.types, display_type)
                    .is_some()
                {
                    let evaluated = self.evaluate_type_for_assignability(display_type);
                    crate::query_boundaries::common::widen_type(self.ctx.types, evaluated)
                } else {
                    display_type
                };
            let source_enum_symbol = self.enum_symbol_from_enumish_type(display_type);
            let target_enum_symbol = self.enum_symbol_from_enumish_type(target);
            if source_enum_symbol.is_some()
                && target_enum_symbol.is_some()
                && source_enum_symbol != target_enum_symbol
            {
                return self.format_assignability_type_for_message(display_type, target);
            }
            let formatted = self.format_type_for_assignability_message(display_type);
            let resolved_for_access = self.resolve_type_for_property_access(display_type);
            let resolved = self.judge_evaluate(resolved_for_access);
            let resolver =
                tsz_solver::objects::index_signatures::IndexSignatureResolver::new(self.ctx.types);
            if !formatted.contains('{')
                && !formatted.contains('[')
                && !formatted.contains('|')
                && !formatted.contains('&')
                && !formatted.contains('<')
                && !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    display_type,
                )
                && (resolver.has_index_signature(
                    resolved,
                    tsz_solver::objects::index_signatures::IndexKind::String,
                ) || resolver.has_index_signature(
                    resolved,
                    tsz_solver::objects::index_signatures::IndexKind::Number,
                ))
            {
                if let Some(structural) = self.format_structural_indexed_object_type(resolved) {
                    return structural;
                }
                return self.format_type(resolved);
            }
            // For generic type aliases whose conditional body is ambiguous
            // (e.g. `IsArray<T>` where T extends `object`), skip annotation text.
            let eval_for_ambiguous = self.evaluate_type_for_assignability(display_type);
            let is_ambiguous_conditional_alias = self
                .compute_ambiguous_conditional_display(eval_for_ambiguous)
                .is_some();
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
                && self.should_prefer_declared_source_annotation_display(
                    expr_idx,
                    expr_display_type,
                    &display,
                )
                && !is_ambiguous_conditional_alias
                && !display.starts_with("keyof ")
                && !display.starts_with("typeof ")
                && !display.contains("[P in ")
                && !display.contains("[K in ")
                // Don't use annotation text for union types — the TypeFormatter
                // reorders null/undefined to the end to match tsc's display.
                // Annotation text preserves the user's original order which
                // differs from tsc's canonical display.
                && (!display.contains(" | ")
                    || Self::display_has_member_literals_assignability(&display))
                // Don't use annotation text when the formatted type includes
                // `| undefined` (added by strictNullChecks for optional params)
                // that the raw annotation text doesn't have. The annotation text
                // reflects the source code literally and misses the semantic
                // `| undefined` injection.
                && (!formatted.contains("| undefined") || display.contains("| undefined"))
                // Don't use annotation text for string intrinsic types when it
                // differs from the formatted type. tsc collapses idempotent
                // nesting (e.g. Uppercase<Uppercase<string>> → Uppercase<string>)
                // at type creation time, so the annotation text may be stale.
                && (!crate::query_boundaries::common::is_string_intrinsic_type(
                    self.ctx.types,
                    display_type,
                ) || display.trim() == formatted)
            {
                if let Some(intersection_display) =
                    self.declared_intersection_annotation_display_for_expression(expr_idx)
                {
                    return intersection_display;
                }
                return self.format_annotation_like_type(&display);
            }
            if let Some(display) =
                self.direct_type_query_primitive_source_display(expr_idx, display_type)
            {
                return display;
            }
            return formatted;
        }

        // Check if source is a single-call-signature callable that tsc displays in
        // arrow syntax. For these, use the TypeFormatter instead of annotation text.
        let source_uses_arrow_syntax =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, source)
                .is_some_and(|shape| {
                    shape.call_signatures.len() == 1
                        && shape.construct_signatures.is_empty()
                        && shape.properties.is_empty()
                        && shape.string_index.is_none()
                        && shape.number_index.is_none()
                });
        if !source_uses_arrow_syntax {
            if let Some(annotation_text) =
                self.declared_type_annotation_text_for_symbol_type(source, true)
            {
                let display = self.format_declared_annotation_for_diagnostic(&annotation_text);
                return self.canonicalize_assignment_numeric_literal_union_display(
                    source, target, display,
                );
            }
            let evaluated_source = self.evaluate_type_with_env(source);
            if evaluated_source != source
                && let Some(annotation_text) =
                    self.declared_type_annotation_text_for_symbol_type(evaluated_source, true)
            {
                let display = self.format_declared_annotation_for_diagnostic(&annotation_text);
                return self.canonicalize_assignment_numeric_literal_union_display(
                    evaluated_source,
                    target,
                    display,
                );
            }
        }

        self.format_assignability_type_for_message(source, target)
    }

    fn should_preserve_nuia_source_undefined_display(
        &self,
        source: TypeId,
        target: TypeId,
        expr_idx: NodeIndex,
        expr_display_type: TypeId,
    ) -> bool {
        if !self.ctx.compiler_options.no_unchecked_indexed_access
            || expr_display_type == TypeId::ERROR
        {
            return false;
        }
        if !crate::query_boundaries::class_type::type_includes_undefined(self.ctx.types, source)
            || crate::query_boundaries::class_type::type_includes_undefined(self.ctx.types, target)
        {
            return false;
        }
        self.ctx.arena.get(expr_idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        })
    }

    fn string_covered_template_union_source_display(&self, source: TypeId) -> Option<TypeId> {
        let members = crate::query_boundaries::common::union_members(self.ctx.types, source)?;
        if !members.contains(&TypeId::STRING) {
            return None;
        }
        members
            .iter()
            .all(|&member| self.is_string_covered_template_union_member(member))
            .then_some(TypeId::STRING)
    }

    fn is_string_covered_template_union_member(&self, type_id: TypeId) -> bool {
        type_id == TypeId::STRING
            || crate::query_boundaries::common::is_template_literal_type(self.ctx.types, type_id)
            || crate::query_boundaries::common::is_string_intrinsic_type(self.ctx.types, type_id)
            || crate::query_boundaries::common::literal_value(self.ctx.types, type_id)
                .is_some_and(|value| value.primitive_type_id() == TypeId::STRING)
    }

    pub(in crate::error_reporter) fn format_assignment_target_type_for_diagnostic(
        &mut self,
        target: TypeId,
        source: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if let Some(contextual_target) =
            self.object_literal_property_contextual_target_for_diagnostic(anchor_idx, target)
        {
            return self.format_object_literal_property_diag_target(contextual_target);
        }

        // When the target is a nullable union (e.g., `T | null | undefined`)
        // and the source is non-nullable, strip null/undefined from the
        // top-level display to match tsc's behavior.
        let display_target = self
            .strip_nullish_for_assignability_display(target, source)
            .unwrap_or(target);
        if crate::query_boundaries::common::is_index_access_type(self.ctx.types, display_target)
            && crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                display_target,
            )
        {
            return self.format_type_for_assignability_message(display_target);
        }
        if let Some(display) = self.keyof_type_alias_body_display(display_target) {
            return display;
        }
        if let Some(display) = self.static_schema_array_structural_display(display_target, source) {
            return display;
        }

        let target_expr = self
            .assignment_target_expression(anchor_idx)
            .unwrap_or(anchor_idx);
        if display_target == target
            && let Some(display) =
                self.keyof_type_alias_annotation_display_for_expression(target_expr)
        {
            return display;
        }
        if display_target == target
            && let Some(annotation) = self.direct_assignment_target_annotation_text(anchor_idx)
            && let Some(display) = self.keyof_type_alias_annotation_display(&annotation)
        {
            return display;
        }
        if display_target == target
            && let Some(display) = self.direct_assignment_target_annotation_text(anchor_idx)
            && display.trim() == "{}"
        {
            return self.format_annotation_like_type(&display);
        }
        if display_target == target
            && let Some(display) =
                self.declared_intersection_annotation_display_for_expression(target_expr)
        {
            return display;
        }
        if display_target == target
            && let Some(display) = self.declared_type_annotation_text_for_expression(target_expr)
            && display.contains('&')
            && display.contains('{')
            && !display.trim_start().starts_with("keyof ")
        {
            return self.format_annotation_like_type(&display);
        }
        if let Some(display) = self.declared_type_annotation_text_for_expression(target_expr)
            && (display.starts_with("keyof ")
                || display.contains("[P in ")
                || display.contains("[K in "))
        {
            // For `typeof EnumName.Member`, tsc evaluates to the enum member type
            // and displays as `EnumName.Member` (without `typeof` prefix). Skip the
            // raw annotation text when the target resolves to an enum member type.
            if display.starts_with("typeof ")
                && crate::query_boundaries::common::enum_def_id(self.ctx.types, target).is_some()
            {
                // Fall through to use the TypeFormatter, which correctly displays
                // `TypeData::Enum` as qualified `W.a` style names.
            } else if display.starts_with("keyof ") && display_target == target {
                // For `keyof (A | B)` / `keyof (A & B)`, use the TypeFormatter so
                // that distribution rules apply (→ `keyof A & keyof B`).
                // For plain `keyof SomeName`, the annotation text is already correct
                // (tsc shows `keyof A`, not the expanded literal union). Only route
                // through TypeFormatter when the operand contains a union/intersection.
                let operand_text = display.trim_start_matches("keyof ").trim();
                let needs_distribution = operand_text.contains('|')
                    || (operand_text.contains('&') && operand_text.starts_with('('));
                if needs_distribution {
                    return self.format_type_for_assignability_message(display_target);
                }
                return self.format_annotation_like_type(&display);
            } else if display_target == target {
                // Only use annotation text when we didn't strip nullable members;
                // otherwise the annotation includes null/undefined that tsc omits.
                return self.format_annotation_like_type(&display);
            }
        }

        if display_target == target
            && let Some(display) = self.declared_type_annotation_text_for_expression(target_expr)
        {
            if display.trim() == "{}" {
                return self.format_annotation_like_type(&display);
            }
            if let Some(intersection_display) =
                self.declared_intersection_annotation_display_for_expression(target_expr)
            {
                return intersection_display;
            }
            if crate::query_boundaries::common::is_index_access_type(self.ctx.types, display_target)
                && Self::should_evaluate_indexed_access_annotation_for_assignment(&display)
            {
                let evaluated = self.evaluate_type_for_assignability(display_target);
                if evaluated != display_target && evaluated != TypeId::ERROR {
                    if let Some(members) =
                        crate::query_boundaries::common::union_members(self.ctx.types, evaluated)
                    {
                        let mut formatter = self
                            .ctx
                            .create_diagnostic_type_formatter()
                            .with_display_properties()
                            .with_preserve_optional_parameter_surface_syntax(true);
                        return members
                            .iter()
                            .map(|&member| formatter.format(member).into_owned())
                            .collect::<Vec<_>>()
                            .join(" | ");
                    }
                    let mut formatter = self
                        .ctx
                        .create_diagnostic_type_formatter()
                        .with_display_properties()
                        .with_skip_application_alias_names()
                        .with_preserve_optional_parameter_surface_syntax(true);
                    return formatter.format(evaluated).into_owned();
                }
            }
            let preserve_literal_surface = self.target_preserves_literal_surface(source);
            let fallback = if preserve_literal_surface {
                self.format_type_diagnostic(target)
            } else {
                // Use diagnostic mode to avoid synthetic `?: undefined` in unions
                self.format_type_diagnostic_widened(
                    self.widen_fresh_object_literal_properties_for_display(target),
                )
            };
            let assignability_display =
                self.format_assignability_type_for_message(display_target, source);
            if let Some((annotation_base, _)) = display.trim().split_once('<')
                && assignability_display.trim() == annotation_base.trim()
            {
                return self.format_annotation_like_type(&display);
            }
            if assignability_display.starts_with('"')
                || assignability_display.starts_with('`')
                || assignability_display == "true"
                || assignability_display == "false"
                || (crate::query_boundaries::common::string_intrinsic_components(
                    self.ctx.types,
                    display_target,
                )
                .is_some()
                    && assignability_display != fallback)
            {
                return assignability_display;
            }
            // Generic callable targets preserve type alias names from annotations
            let target_is_generic_callable =
                crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, target)
                    .is_some_and(|shape| {
                        shape
                            .call_signatures
                            .iter()
                            .chain(shape.construct_signatures.iter())
                            .any(|sig| !sig.type_params.is_empty())
                    })
                    || crate::query_boundaries::common::function_shape_for_type(
                        self.ctx.types,
                        target,
                    )
                    .is_some_and(|shape| !shape.type_params.is_empty());
            if target_is_generic_callable {
                return self.format_annotation_like_type(&display);
            }
            let display_trimmed = display.trim_start();
            if display_trimmed.starts_with('[') || display_trimmed.starts_with("readonly [") {
                return self.format_annotation_like_type(&display);
            }
            if self.tuple_target_has_application_display_alias(display_target)
                && let Some(tuple_display) =
                    self.raw_tuple_assignment_target_display_without_alias(display_target, true)
            {
                return tuple_display;
            }
            if !display_trimmed.contains('<')
                && assignability_display.contains('<')
                && let Some(tuple_display) =
                    self.raw_tuple_assignment_target_display_without_alias(display_target, true)
            {
                return tuple_display;
            }
            // When the target is a generic Application of a Lazy type alias
            // whose body recursively references the same alias (e.g.
            // `T2<U>` for `type T2<T> = [42, T2<{ x: T }>]`), expanding to
            // the alias body produces an unbounded `[42, [42, [..., ...]]]`
            // cascade because the printer flattens every nested Application
            // when alias names are skipped. tsc keeps the alias annotation
            // (`T2<U>`) in this case; preserve it here too.
            if self.is_recursive_type_alias_application_for_display(target) {
                return self.format_annotation_like_type(&display);
            }
            let preserve_tuple_alias_display = !display_trimmed.starts_with('{')
                && !display_trimmed.starts_with('(')
                && !display_trimmed.contains('[')
                && !display_trimmed.contains("=>")
                && !display_trimmed.contains(" | ")
                && !display_trimmed.contains(" & ")
                && !crate::query_boundaries::common::is_generic_application(self.ctx.types, target)
                && !self.type_alias_body_is_generic_application(target);
            if !preserve_tuple_alias_display
                && let Some(tuple_display) =
                    self.raw_tuple_assignment_target_display_without_alias(target, true)
            {
                return tuple_display;
            }
            if Self::display_has_member_literals_assignability(&display) {
                return self.format_annotation_like_type(&display);
            }
            if Self::display_has_member_literals_assignability(&fallback)
                && !Self::display_has_member_literals_assignability(&display)
            {
                return self.format_annotation_like_type(&display);
            }
            // When the fallback produces duplicate names in a union or tuple
            // (e.g., `Yep | Yep` or `[Yep, Yep]`) but the annotation text preserves
            // namespace-qualified names (e.g., `Foo.Yep | Bar.Yep` or
            // `[Foo.Yep, Bar.Yep]`), prefer the annotation text. This matches tsc's
            // behavior of qualifying types when they'd otherwise be ambiguous.
            if Self::has_duplicate_union_member_names(&fallback)
                && !Self::has_duplicate_union_member_names(&display)
            {
                return self.format_annotation_like_type(&display);
            }
            // When the target is an enum type, format_type() may resolve to
            // an unrelated type name (e.g., a DOM interface that shares the
            // same structural shape). Use the assignability formatter which
            // correctly produces namespace-qualified enum names.
            if crate::query_boundaries::common::enum_def_id(self.ctx.types, target).is_some() {
                return self.format_assignability_type_for_message(target, source);
            }
            // When the evaluated display contains our internal "error" sentinel
            // (from unresolved type names like `() => SymbolScope` where `SymbolScope`
            // is not defined), prefer the declared annotation text. tsc shows the
            // original annotation, not the evaluated error type.
            // Only applies when the annotation itself does not contain "error" (which
            // would indicate the user actually wrote a type named `error`).
            if fallback.contains("error") && !display.contains("error") {
                return self.format_annotation_like_type(&display);
            }
            // For Application types whose alias body is an IndexedAccess or
            // Conditional type (e.g. `type Cb<T> = {noAlias: () => T}["noAlias"]`),
            // tsc does not preserve the alias name in error messages — it shows the
            // structurally-evaluated form. The assignability_display computed above
            // already contains the correct expanded form.
            if assignability_display != fallback {
                let evaluated_for_display = self.evaluate_type_for_assignability(display_target);
                if self.should_use_evaluated_assignability_display(
                    display_target,
                    evaluated_for_display,
                ) {
                    return assignability_display;
                }
            }
            return fallback;
        }

        // When the target is an enum type without annotation text, use the
        // assignability formatter for correct qualified enum name display.
        if crate::query_boundaries::common::enum_def_id(self.ctx.types, display_target).is_some() {
            return self.format_assignability_type_for_message(display_target, source);
        }
        if let Some(tuple_display) =
            self.raw_tuple_assignment_target_display_without_alias(display_target, true)
        {
            return tuple_display;
        }

        if self.target_preserves_literal_surface(source) {
            let assignability_display =
                self.format_assignability_type_for_message(display_target, source);
            let fallback = self.format_type_diagnostic(display_target);
            if assignability_display.starts_with('"')
                || assignability_display.starts_with('`')
                || assignability_display == "true"
                || assignability_display == "false"
                || (crate::query_boundaries::common::string_intrinsic_components(
                    self.ctx.types,
                    display_target,
                )
                .is_some()
                    && assignability_display != fallback)
            {
                assignability_display
            } else {
                fallback
            }
        } else {
            // Use diagnostic mode to avoid synthetic `?: undefined` in unions
            let assignability_display =
                self.format_assignability_type_for_message(display_target, source);
            if self
                .lookup_type_alias_name_for_display(display_target)
                .is_some_and(|alias| alias == assignability_display)
            {
                return assignability_display;
            }
            let fallback = self.format_type_diagnostic_widened(
                self.widen_fresh_object_literal_properties_for_display(display_target),
            );
            if assignability_display.starts_with('"')
                || assignability_display.starts_with('`')
                || assignability_display == "true"
                || assignability_display == "false"
                || (crate::query_boundaries::common::string_intrinsic_components(
                    self.ctx.types,
                    display_target,
                )
                .is_some()
                    && assignability_display != fallback)
            {
                assignability_display
            } else {
                fallback
            }
        }
    }

    fn direct_assignment_target_annotation_text(&self, anchor_idx: NodeIndex) -> Option<String> {
        let mut current = anchor_idx;
        let mut guard = 0;
        let source_is_return = self.assignment_source_is_return_expression(anchor_idx);

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }

            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node)
                && var_decl.type_annotation.is_some()
            {
                return self.node_text(var_decl.type_annotation).and_then(|text| {
                    self.sanitize_type_annotation_text_for_diagnostic(text, true)
                });
            }
            if let Some(param) = self.ctx.arena.get_parameter(node)
                && param.type_annotation.is_some()
            {
                return self.node_text(param.type_annotation).and_then(|text| {
                    self.sanitize_type_annotation_text_for_diagnostic(text, true)
                });
            }
            if source_is_return
                && let Some(function) = self.ctx.arena.get_function(node)
                && function.type_annotation.is_some()
            {
                return self.node_text(function.type_annotation).and_then(|text| {
                    self.sanitize_type_annotation_text_for_diagnostic(text, true)
                });
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        self.source_assignment_target_annotation_text(anchor_idx)
    }

    fn source_assignment_target_annotation_text(&self, anchor_idx: NodeIndex) -> Option<String> {
        let (start, end) = self.get_node_span(anchor_idx)?;
        let source = self.ctx.arena.source_files.first()?.text.as_ref();
        let start = start as usize;
        let end = end as usize;
        if start >= end || end > source.len() {
            return None;
        }
        let line_end = source[end..]
            .find('\n')
            .map_or(source.len(), |offset| end + offset);
        if let Some(text) = self.annotation_text_from_colon_fragment(&source[end..line_end]) {
            return Some(text);
        }

        let anchor_text = source[start..end].trim_start();
        if !anchor_text.starts_with("return") {
            return None;
        }
        let body_start = source[..start].rfind('{')?;
        let close_paren = source[..body_start].rfind(')')?;
        self.annotation_text_from_colon_fragment(&source[close_paren + 1..body_start])
    }

    fn annotation_text_from_colon_fragment(&self, fragment: &str) -> Option<String> {
        let colon = fragment.find(':')?;
        if !fragment[..colon].trim().is_empty() {
            return None;
        }
        let type_fragment = &fragment[colon + 1..];
        let type_start = type_fragment
            .char_indices()
            .find_map(|(idx, ch)| (!ch.is_whitespace()).then_some(idx))?;
        let mut depth = 0u32;
        let mut end = type_fragment.len();
        for (idx, ch) in type_fragment[type_start..].char_indices() {
            let absolute_idx = type_start + idx;
            if depth == 0 && absolute_idx > type_start && matches!(ch, '=' | ';' | ',' | ')' | '{')
            {
                end = absolute_idx;
                break;
            }
            match ch {
                '<' | '(' | '[' | '{' => depth = depth.saturating_add(1),
                '>' | ')' | ']' | '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        let text = type_fragment[type_start..end].trim().to_string();
        self.sanitize_type_annotation_text_for_diagnostic(text, true)
    }

    fn tuple_target_has_application_display_alias(&self, target: TypeId) -> bool {
        crate::query_boundaries::common::is_tuple_type(self.ctx.types, target)
            && self
                .ctx
                .types
                .get_display_alias(target)
                .is_some_and(|alias| {
                    crate::query_boundaries::common::application_info(self.ctx.types, alias)
                        .is_some()
                })
    }

    /// True when `target` is `Application(Lazy(D), args)` and the alias body
    /// of `D` reaches another reference to `D` (directly or transitively).
    /// Used to suppress the `raw_tuple_…_without_alias` expansion path: a
    /// recursive alias rendered with alias names skipped collapses to an
    /// unbounded `[..., ...]` cascade rather than a useful structural form.
    fn is_recursive_type_alias_application_for_display(&self, target: TypeId) -> bool {
        crate::query_boundaries::recursive_alias::is_recursive_type_alias_application(
            self.ctx.types,
            &self.ctx.definition_store,
            target,
        )
    }

    fn raw_tuple_assignment_target_display_without_alias(
        &mut self,
        target: TypeId,
        allow_direct_tuple: bool,
    ) -> Option<String> {
        let mut current = target;
        let mut saw_generic_application = false;
        let display_target = 'resolved: {
            for _ in 0..4 {
                if crate::query_boundaries::common::is_tuple_type(self.ctx.types, current) {
                    if !allow_direct_tuple
                        && (crate::query_boundaries::common::is_generic_application(
                            self.ctx.types,
                            current,
                        ) || self.type_alias_body_is_generic_application(current))
                    {
                        saw_generic_application = true;
                        if crate::query_boundaries::common::is_generic_application(
                            self.ctx.types,
                            current,
                        ) {
                            let evaluated = self.evaluate_tuple_display_candidate(current);
                            if evaluated != current && evaluated != TypeId::ERROR {
                                current = evaluated;
                                continue;
                            }
                        }
                    }
                    break 'resolved current;
                }
                if current == TypeId::ERROR {
                    return None;
                }
                saw_generic_application |= crate::query_boundaries::common::is_generic_application(
                    self.ctx.types,
                    current,
                ) || self
                    .type_alias_body_is_generic_application(current);
                let evaluated = self.evaluate_tuple_display_candidate(current);
                if evaluated == current {
                    return None;
                }
                current = evaluated;
            }
            if crate::query_boundaries::common::is_tuple_type(self.ctx.types, current) {
                current
            } else {
                return None;
            }
        };
        if !allow_direct_tuple && !saw_generic_application {
            return None;
        }
        if crate::query_boundaries::common::lazy_def_id(self.ctx.types, display_target).is_some() {
            return None;
        }
        self.format_raw_tuple_assignment_target(display_target)
    }

    fn evaluate_tuple_display_candidate(&mut self, type_id: TypeId) -> TypeId {
        let evaluated = self.evaluate_application_type(type_id);
        if evaluated != type_id && evaluated != TypeId::ERROR {
            return evaluated;
        }
        let evaluated = self.evaluate_type_for_assignability(type_id);
        if evaluated != type_id && evaluated != TypeId::ERROR {
            return evaluated;
        }
        let evaluated = self.evaluate_type_with_env(type_id);
        if evaluated != type_id && evaluated != TypeId::ERROR {
            return evaluated;
        }
        type_id
    }

    fn format_raw_tuple_assignment_target(&mut self, type_id: TypeId) -> Option<String> {
        let elements = crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id)?;
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties()
            .with_skip_application_alias_names()
            .with_preserve_optional_parameter_surface_syntax(true);
        Some(formatter.format_tuple_elements_for_diagnostic(&elements))
    }

    fn type_alias_body_is_generic_application(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)
            .or_else(|| self.ctx.definition_store.find_def_for_type(type_id))
            .and_then(|def_id| self.ctx.definition_store.get(def_id))
            .is_some_and(|def| {
                def.kind == tsz_solver::def::DefKind::TypeAlias
                    && def.body.is_some_and(|body| {
                        crate::query_boundaries::common::is_generic_application(
                            self.ctx.types,
                            body,
                        )
                    })
            })
    }

    fn should_evaluate_indexed_access_annotation_for_assignment(display: &str) -> bool {
        display.contains("[keyof ")
    }

    pub(in crate::error_reporter) fn related_generic_indexed_access_source_display(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let (source_object, source_index) =
            crate::query_boundaries::common::index_access_types(self.ctx.types, source)?;
        let (target_object, target_index) =
            crate::query_boundaries::common::index_access_types(self.ctx.types, target)?;

        let source_index_info =
            crate::query_boundaries::common::type_param_info(self.ctx.types, source_index)?;
        crate::query_boundaries::common::type_param_info(self.ctx.types, target_index)?;
        if source_index == target_index {
            return None;
        }

        let source_object_display = self.format_type_for_assignability_message(source_object);
        let target_object_display = self.format_type_for_assignability_message(target_object);
        let source_short = simple_or_namespace_member_name(&source_object_display)?;
        let target_short = simple_or_namespace_member_name(&target_object_display)?;
        if source_short != target_short {
            return None;
        }

        let source_index_display = self.ctx.types.resolve_atom_ref(source_index_info.name);
        Some(format!("{source_short}[{source_index_display}]"))
    }

    pub(in crate::error_reporter) fn format_nested_assignment_source_type_for_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some()
            && crate::query_boundaries::common::string_intrinsic_components(self.ctx.types, target)
                .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }

        if let Some(display) = self.preferred_evaluated_source_display(source, target) {
            return display;
        }

        // Skip the anchor-expression-derived path when `source` does not
        // correspond to the anchor's expression type. This happens during
        // nested elaboration of a structural failure (e.g. function-return
        // mismatch elaborates with the inner return types as `source`/
        // `target`, but the anchor still points at the outer assignment
        // expression). In that case the expression's type is the OUTER
        // value, which produces the bogus
        // "Type '(x: Object) => Object' is not assignable to type 'string'."
        // message — a category-error claim that the outer function is not
        // assignable to the inner return type.
        let anchor_expr_type = self
            .direct_diagnostic_source_expression(anchor_idx)
            .map(|expr_idx| self.get_type_of_node(expr_idx));
        let source_matches_anchor_expr = anchor_expr_type
            .is_some_and(|expr_type| expr_type == source || expr_type == TypeId::ERROR);
        if !source_matches_anchor_expr {
            return self.format_assignability_type_for_message(source, target);
        }

        if let Some(expr_idx) = self.direct_diagnostic_source_expression(anchor_idx) {
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx) {
                if let Some(intersection_display) =
                    self.declared_intersection_annotation_display_for_expression(expr_idx)
                {
                    return intersection_display;
                }
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            if expr_type != TypeId::ERROR {
                let widened_expr_type = if self.target_preserves_literal_surface(target) {
                    expr_type
                } else {
                    self.widen_type_for_display(expr_type)
                };
                let display_type =
                    if self.should_widen_enum_member_assignment_source(widened_expr_type, target) {
                        self.widen_enum_member_type(widened_expr_type)
                    } else {
                        widened_expr_type
                    };
                let display_type = self.widen_function_like_display_type(display_type);
                if let Some(display) =
                    self.new_expression_nominal_source_display(expr_idx, display_type)
                {
                    return display;
                }
                return self.format_assignability_type_for_message(display_type, target);
            }
        }

        if let Some(expr_idx) = self.assignment_source_expression(anchor_idx) {
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx) {
                if let Some(intersection_display) =
                    self.declared_intersection_annotation_display_for_expression(expr_idx)
                {
                    return intersection_display;
                }
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            if self.is_literal_sensitive_assignment_target(target)
                && let Some(display) =
                    self.call_object_literal_intersection_source_display(expr_idx, source, target)
            {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            let display_type = if expr_type != TypeId::ERROR {
                let widened_expr_type = if self.target_preserves_literal_surface(target) {
                    expr_type
                } else {
                    self.widen_type_for_display(expr_type)
                };
                if self.should_widen_enum_member_assignment_source(widened_expr_type, target) {
                    self.widen_enum_member_type(widened_expr_type)
                } else {
                    widened_expr_type
                }
            } else {
                self.widen_type_for_display(source)
            };
            let display_type = self.widen_function_like_display_type(display_type);
            if let Some(display) =
                self.new_expression_nominal_source_display(expr_idx, display_type)
            {
                return display;
            }
            return self.format_assignability_type_for_message(display_type, target);
        }

        // Check if source is a single-call-signature callable that tsc displays in
        // arrow syntax. For these, use the TypeFormatter instead of annotation text.
        let source_uses_arrow_syntax =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, source)
                .is_some_and(|shape| {
                    shape.call_signatures.len() == 1
                        && shape.construct_signatures.is_empty()
                        && shape.properties.is_empty()
                        && shape.string_index.is_none()
                        && shape.number_index.is_none()
                });
        if !source_uses_arrow_syntax {
            if let Some(annotation_text) =
                self.declared_type_annotation_text_for_symbol_type(source, true)
            {
                return self.format_declared_annotation_for_diagnostic(&annotation_text);
            }
            let evaluated_source = self.evaluate_type_with_env(source);
            if evaluated_source != source
                && let Some(annotation_text) =
                    self.declared_type_annotation_text_for_symbol_type(evaluated_source, true)
            {
                return self.format_declared_annotation_for_diagnostic(&annotation_text);
            }
        }

        self.format_assignability_type_for_message(source, target)
    }

    fn new_expression_nominal_source_display(
        &mut self,
        expr_idx: NodeIndex,
        display_type: TypeId,
    ) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        // When the result type is a union (e.g., `number | Date` from
        // `new unionOfDifferentReturnType(10)` where unionOfDifferentReturnType
        // is `{ new (a: number): number } | { new (a: number): Date }`),
        // TSC shows the actual result type, not the constructor variable name.
        // Return None to let the fallback formatting handle it.
        if crate::query_boundaries::common::union_members(self.ctx.types, display_type).is_some() {
            return None;
        }

        if let Some(new_expr) = self.ctx.arena.get_call_expr(node)
            && let Some(mut ctor_display) = self.expression_text(new_expr.expression)
        {
            if let Some(type_args) = &new_expr.type_arguments
                && !type_args.nodes.is_empty()
            {
                let rendered_args: Vec<String> = type_args
                    .nodes
                    .iter()
                    .map(|&arg| self.get_source_text_for_node(arg))
                    .collect();
                ctor_display.push('<');
                ctor_display.push_str(&rendered_args.join(", "));
                ctor_display.push('>');
                return Some(ctor_display);
            }
            // With display alias: show the named type (e.g. `D<unknown>` not `D`).
            // Without: variable name is not a type name; let caller format the actual type.
            if self.ctx.types.get_display_alias(display_type).is_some() {
                return Some(self.format_type_diagnostic_structural(display_type));
            }
            return None;
        }

        Some(self.format_property_receiver_type_for_diagnostic(display_type))
    }

    fn js_constructor_instance_assignment_source_display(
        &mut self,
        source: TypeId,
        anchor_idx: NodeIndex,
    ) -> Option<String> {
        crate::query_boundaries::common::object_shape_for_type(self.ctx.types, source)?;
        let expr_idx = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))?;
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let expr_node = self.ctx.arena.get(expr_idx)?;
        if expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let source_sym = self.resolve_identifier_symbol(expr_idx)?;
        let source_symbol = self
            .get_cross_file_symbol(source_sym)
            .or_else(|| self.ctx.binder.get_symbol(source_sym))?;
        if (source_symbol.flags & tsz_binder::symbol_flags::VARIABLE) == 0 {
            return None;
        }

        let current_file_idx = self.ctx.current_file_idx as u32;
        let this_pos = expr_node.pos;
        source_symbol
            .stable_declarations
            .iter()
            .copied()
            .chain(std::iter::once(source_symbol.stable_value_declaration))
            .filter_map(|stable_loc| {
                if !stable_loc.is_known() {
                    return None;
                }
                let file_idx = if stable_loc.has_file_idx() {
                    stable_loc.file_idx
                } else {
                    current_file_idx
                };
                let (decl_idx, arena) = self.ctx.node_at_stable_location(stable_loc)?;
                let decl_node = arena.get(decl_idx)?;
                let declaration = arena.get_variable_declaration(decl_node)?;
                if file_idx == current_file_idx && decl_node.pos > this_pos {
                    return None;
                }

                let init_idx = arena.skip_parenthesized_and_assertions(declaration.initializer);
                let init_node = arena.get(init_idx)?;
                if init_node.kind != syntax_kind_ext::NEW_EXPRESSION {
                    return None;
                }
                let new_expr = arena.get_call_expr(init_node)?;
                let ctor_idx = arena.skip_parenthesized_and_assertions(new_expr.expression);
                let ctor_node = arena.get(ctor_idx)?;
                let ident = arena.get_identifier(ctor_node)?;
                Some((
                    file_idx == current_file_idx,
                    decl_node.pos,
                    ident.escaped_text.clone(),
                ))
            })
            .max_by_key(|(same_file, decl_pos, _)| (*same_file, *decl_pos))
            .map(|(_, _, display)| display)
    }

    fn call_unknown_array_source_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        let call = self.ctx.arena.get_call_expr(node)?;

        let first_arg = *call.arguments.as_ref()?.nodes.first()?;
        let first_arg_type = self.get_type_of_node(first_arg);
        if matches!(first_arg_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }

        let element_type =
            crate::query_boundaries::common::array_element_type(self.ctx.types, first_arg_type)
                .or_else(|| {
                    tsz_solver::operations::get_iterator_info(self.ctx.types, first_arg_type, false)
                        .map(|info| info.yield_type)
                })?;
        if matches!(element_type, TypeId::ERROR | TypeId::UNKNOWN) {
            return None;
        }

        let recovered = self
            .ctx
            .types
            .array(self.widen_type_for_display(element_type));
        Some(self.format_assignability_type_for_message(recovered, target))
    }

    fn preferred_evaluated_source_display(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        let preserve_literal_surface = self.target_preserves_literal_surface(target);
        if crate::query_boundaries::common::is_template_literal_type(self.ctx.types, source) {
            return Some(self.format_type_diagnostic_structural(source));
        }

        let evaluated = self.evaluate_type_for_assignability(source);
        if evaluated == source || evaluated == TypeId::ERROR {
            return None;
        }

        if crate::query_boundaries::common::literal_value(self.ctx.types, evaluated).is_some()
            || crate::query_boundaries::common::is_template_literal_type(self.ctx.types, evaluated)
            || crate::query_boundaries::common::string_intrinsic_components(
                self.ctx.types,
                evaluated,
            )
            .is_some()
        {
            return Some(if preserve_literal_surface {
                self.format_type_diagnostic(evaluated)
            } else {
                self.format_type_diagnostic_structural(evaluated)
            });
        }

        None
    }

    fn type_assertion_mapped_alias_source_display(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;
        if !matches!(
            node.kind,
            syntax_kind_ext::AS_EXPRESSION | syntax_kind_ext::TYPE_ASSERTION
        ) {
            return None;
        }
        let assertion = self.ctx.arena.get_type_assertion(node)?;
        let assertion_type = self.get_type_from_type_node(assertion.type_node);
        let is_generic_mapped_alias =
            crate::query_boundaries::common::is_generic_application(self.ctx.types, assertion_type)
                && crate::query_boundaries::common::get_application_lazy_def_id(
                    self.ctx.types,
                    assertion_type,
                )
                .and_then(|def_id| self.ctx.definition_store.get(def_id))
                .is_some_and(|def| {
                    def.kind == tsz_solver::def::DefKind::TypeAlias
                        && def.body.is_some_and(|body| {
                            crate::query_boundaries::common::is_mapped_type(self.ctx.types, body)
                        })
                });
        if !is_generic_mapped_alias {
            return None;
        }
        self.node_text(assertion.type_node)
            .and_then(|text| self.sanitize_type_annotation_text_for_diagnostic(text, false))
            .map(|text| self.format_annotation_like_type(&text))
    }

    /// Whether to suppress the AST-literal short-circuit for an
    /// object-literal-property elaboration when the property elaboration has
    /// already widened the source (e.g. `1` → `number`). Mirrors tsc's
    /// `getWidenedLiteralLikeTypeForContextualType`: keep the literal display
    /// when the source's primitive kind appears as a literal kind somewhere in
    /// the target, otherwise widen.
    pub(in crate::error_reporter) fn property_elaboration_widening_required_for_display(
        &self,
        expr_idx: NodeIndex,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if !self.is_property_assignment_initializer(expr_idx) {
            return false;
        }
        // Only fire when the caller passed in a non-literal primitive source
        // (i.e. the property elaboration already widened the literal). For
        // direct `let x: 1 = "abc"` style mismatches the source is still the
        // literal type, so this guard short-circuits.
        if !crate::query_boundaries::common::is_primitive_type(self.ctx.types, source) {
            return false;
        }
        if crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some() {
            return false;
        }
        let primitive_kind = source;
        !target_accepts_literal_primitive_kind(self.ctx.types, target, primitive_kind)
    }

    pub(in crate::error_reporter) fn array_elaboration_widening_required_for_display(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        use crate::query_boundaries::common;

        let source_primitive = if let Some(value) = common::literal_value(self.ctx.types, source) {
            value.primitive_type_id()
        } else if matches!(
            source,
            TypeId::STRING | TypeId::NUMBER | TypeId::BIGINT | TypeId::BOOLEAN
        ) {
            source
        } else {
            return false;
        };
        let target = common::evaluate_type(self.ctx.types, target);
        if target == TypeId::UNDEFINED || target == TypeId::NULL {
            return source_primitive != TypeId::BOOLEAN;
        }

        !target_accepts_literal_primitive_kind(self.ctx.types, target, source_primitive)
    }

    pub(in crate::error_reporter) fn array_literal_element_source_widening_required_for_display(
        &self,
        anchor_idx: NodeIndex,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if !self.array_elaboration_widening_required_for_display(source, target) {
            return false;
        }

        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(anchor_idx);
        self.ctx
            .arena
            .parent_of(expr_idx)
            .and_then(|parent_idx| self.ctx.arena.get(parent_idx))
            .is_some_and(|parent| parent.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION)
    }

    pub(in crate::error_reporter) fn is_object_rest_assignment_target_anchor(
        &self,
        anchor_idx: NodeIndex,
    ) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(anchor_idx);
        let mut current = expr_idx;
        let mut saw_spread_wrapper = false;
        let mut object_idx = None;

        while let Some(parent_idx) = self.ctx.arena.parent_of(current) {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                || parent_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
            {
                saw_spread_wrapper = true;
                current = parent_idx;
                continue;
            }
            if parent_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION && saw_spread_wrapper
            {
                object_idx = Some(parent_idx);
                break;
            }
            if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                || parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                || parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
            {
                break;
            }
            current = parent_idx;
        }
        let Some(object_idx) = object_idx else {
            return false;
        };

        self.assignment_target_expression(anchor_idx)
            .is_some_and(|target_idx| {
                self.ctx.arena.skip_parenthesized_and_assertions(target_idx) == object_idx
            })
    }

    pub(in crate::error_reporter) fn computed_index_signature_object_literal_source_display(
        &mut self,
        expr_idx: NodeIndex,
        target: Option<TypeId>,
    ) -> Option<String> {
        let target = target?;
        let shape = crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target)?;
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let literal = self.ctx.arena.get_literal_expr(node)?;
        let mut computed_key_kind = None;
        let mut computed_value_types = Vec::new();

        for child_idx in literal.elements.nodes.iter().copied() {
            let child = self.ctx.arena.get(child_idx)?;
            let prop = self.ctx.arena.get_property_assignment(child)?;
            let name_node = self.ctx.arena.get(prop.name)?;
            if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                return None;
            }
            let computed = self.ctx.arena.get_computed_property(name_node)?;
            let raw_key_type = self.get_type_of_node(computed.expression);
            let key_type = self.widen_type_for_display(raw_key_type);
            let key_kind = if key_type == TypeId::STRING {
                "string"
            } else if key_type == TypeId::NUMBER {
                "number"
            } else {
                return None;
            };
            if computed_key_kind.is_some_and(|existing| existing != key_kind) {
                return None;
            }
            computed_key_kind = Some(key_kind);

            let value_type = self.get_type_of_node(prop.initializer);
            if value_type == TypeId::ERROR {
                return None;
            }
            computed_value_types.push(self.widen_type_for_display(value_type));
        }

        let key_kind = computed_key_kind?;
        if computed_value_types.is_empty()
            || !((key_kind == "string" && shape.string_index.is_some())
                || (key_kind == "number" && shape.number_index.is_some()))
        {
            return None;
        }

        let value_type = if computed_value_types.len() == 1 {
            computed_value_types[0]
        } else {
            self.ctx.types.factory().union(computed_value_types)
        };
        let value_display = self.format_type_for_assignability_message(value_type);
        Some(format!("{{ [x: {key_kind}]: {value_display}; }}"))
    }

    pub(in crate::error_reporter) fn literal_assignment_source_display_for_target(
        &mut self,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> Option<String> {
        if self.in_arithmetic_compound_assignment_context(anchor_idx)
            || !crate::query_boundaries::common::is_template_literal_type(self.ctx.types, target)
        {
            return None;
        }
        let expr_idx = self
            .assignment_source_expression(anchor_idx)
            .or_else(|| self.direct_diagnostic_source_expression(anchor_idx))?;
        let display = self.literal_expression_display(expr_idx)?;
        literal_display_appropriate_for_undefined_null_target(self.ctx.types, target, &display)
            .then_some(display)
    }
}
