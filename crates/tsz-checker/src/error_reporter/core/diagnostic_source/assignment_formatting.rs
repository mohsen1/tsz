//! Assignment-source/target type formatting for TS2322-family diagnostics.
//!
//! Extracted from `error_reporter/core/diagnostic_source.rs` to keep that
//! file under the LOC ceiling. Pure file-organization move; no logic changes.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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

        if let Some(display) = self.preferred_evaluated_source_display(source, target) {
            return display;
        }

        let in_arith_compound = self.in_arithmetic_compound_assignment_context(anchor_idx);

        if !in_arith_compound
            && self.is_literal_sensitive_assignment_target(target)
            && let Some(display) = self.literal_expression_display(anchor_idx)
        {
            return display;
        }
        if !in_arith_compound
            && self.is_literal_sensitive_assignment_target(target)
            && crate::query_boundaries::common::literal_value(self.ctx.types, source).is_some()
        {
            return self.format_assignability_type_for_message(source, target);
        }

        if let Some(expr_idx) = self.direct_diagnostic_source_expression(anchor_idx) {
            if !in_arith_compound
                && self.is_literal_sensitive_assignment_target(target)
                && let Some(display) = self.literal_expression_display(expr_idx)
            {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            let expr_display_type = if expr_type == TypeId::UNKNOWN && source != TypeId::UNKNOWN {
                source
            } else {
                expr_type
            };
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
                let preserve_literal_surface = self.target_preserves_literal_surface(target);
                if let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
                    && self.should_prefer_declared_source_annotation_display(
                        expr_idx,
                        expr_display_type,
                        &annotation_text,
                    )
                {
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
                if let Some(display) = self.rebuilt_array_source_display(display_type, target) {
                    return display;
                }
                return self.format_assignability_type_for_message(display_type, target);
            }

            if node_type_matches_source
                && let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
            {
                return display;
            }
        }
        if let Some(expr_idx) = self.assignment_source_expression(anchor_idx) {
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
            {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            let expr_display_type = if expr_type == TypeId::UNKNOWN && source != TypeId::UNKNOWN {
                source
            } else {
                expr_type
            };
            let preserve_literal_surface = self.target_preserves_literal_surface(target);
            if expr_type != TypeId::ERROR
                && let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
                && self.should_prefer_declared_source_annotation_display(
                    expr_idx,
                    expr_display_type,
                    &annotation_text,
                )
            {
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
                if crate::query_boundaries::common::enum_def_id(self.ctx.types, display_type)
                    .is_some()
                {
                    return self.format_assignability_type_for_message(display_type, target);
                }
                return self.format_annotation_like_type(&display);
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

        let target_expr = self
            .assignment_target_expression(anchor_idx)
            .unwrap_or(anchor_idx);
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
            return fallback;
        }

        // When the target is an enum type without annotation text, use the
        // assignability formatter for correct qualified enum name display.
        if crate::query_boundaries::common::enum_def_id(self.ctx.types, display_target).is_some() {
            return self.format_assignability_type_for_message(display_target, source);
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

        if let Some(expr_idx) = self.direct_diagnostic_source_expression(anchor_idx) {
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx) {
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
            // For generic constructor calls without explicit type args (e.g.
            // `new D()` where `class D<T>`), use the type formatter which
            // respects display_alias to show inferred type params like
            // `D<unknown>`. Without this, the expression text "D" would be
            // returned, losing the inferred type arguments.
            if self.ctx.types.get_display_alias(display_type).is_some() {
                return Some(self.format_type_diagnostic_structural(display_type));
            }
            return Some(ctor_display);
        }

        Some(self.format_property_receiver_type_for_diagnostic(display_type))
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
}

/// Whether `target` accepts a literal whose widened primitive kind is
/// `source_primitive` for "literal-of-contextual-type" purposes. Mirrors
/// `isLiteralOfContextualType` from TypeScript: returns true when any
/// literal-typed member of the target shares the same primitive kind, or
/// when the target is a structural literal-shaped type (template/symbol)
/// compatible with the source's primitive kind.
fn target_accepts_literal_primitive_kind(
    db: &dyn tsz_solver::TypeDatabase,
    target: TypeId,
    source_primitive: TypeId,
) -> bool {
    target_accepts_literal_primitive_kind_inner(db, target, source_primitive, 0)
}

fn target_accepts_literal_primitive_kind_inner(
    db: &dyn tsz_solver::TypeDatabase,
    target: TypeId,
    source_primitive: TypeId,
    depth: u32,
) -> bool {
    use crate::query_boundaries::common;
    // Recursion guard for self-referential aliases (e.g. `type T = string |
    // Promise<T>`) and similarly deep unions/intersections. Returning `true`
    // when we bail keeps the literal-display short-circuit's pre-existing
    // behavior intact for unfamiliar shapes.
    if depth > 32 {
        return true;
    }
    if let Some(members) = common::union_members(db, target) {
        return members.iter().any(|&m| {
            target_accepts_literal_primitive_kind_inner(db, m, source_primitive, depth + 1)
        });
    }
    if let Some(members) = common::intersection_members(db, target) {
        return members.iter().any(|&m| {
            target_accepts_literal_primitive_kind_inner(db, m, source_primitive, depth + 1)
        });
    }
    if let Some(value) = common::literal_value(db, target) {
        return value.primitive_type_id() == source_primitive;
    }
    if source_primitive == TypeId::STRING
        && (common::is_template_literal_type(db, target)
            || common::is_string_intrinsic_type(db, target))
    {
        return true;
    }
    if target == TypeId::UNDEFINED || target == TypeId::NULL {
        // tsc preserves the AST literal display for `undefined`/`null` targets
        // (e.g., `var u: typeof undefined = 1` → `Type '1' is not assignable`).
        return true;
    }
    if target == TypeId::NEVER {
        return true;
    }
    // For type parameters, lazy refs, and other non-literal shapes that are
    // still classified as "literal-sensitive" (e.g. unique symbols), keep the
    // AST literal display compatible by default. We only widen when the target
    // is concretely a literal of a different primitive kind.
    true
}
