use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages, format_message};
use crate::error_reporter::assignability::is_object_prototype_method;
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_type_mismatch(
        &mut self,
        _reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
        start: u32,
        length: u32,
        file_name: String,
    ) -> Diagnostic {
        let declared_numeric_literal_union_source_display = if depth == 0 {
            self.direct_diagnostic_source_expression(idx)
                .or_else(|| self.assignment_source_expression(idx))
                .and_then(|expr_idx| {
                    self.declared_numeric_literal_union_alias_source_display(expr_idx, source)
                })
        } else {
            None
        };
        let mut source_str = if depth == 0 {
            let display = self.format_type_for_diagnostic_role(
                source,
                DiagnosticTypeDisplayRole::AssignmentSource {
                    target,
                    anchor_idx: idx,
                },
            );
            // tsc preserves literal union structure (e.g., `"c" | "d"`) in error
            // messages. If format_assignment_source_type_for_diagnostic widened the
            // union to a primitive (e.g., `string`) or used a type alias name
            // (e.g., `Variants` from a parameter annotation), fall back to the
            // TypeFormatter which correctly displays literal union members.
            // This handles both widening and flow-narrowed type alias display.
            let display_is_declared_identifier_source =
                declared_numeric_literal_union_source_display
                    .as_deref()
                    .is_some_and(|declared_display| declared_display == display.as_str());
            if crate::query_boundaries::common::union_members(self.ctx.types, source).is_some_and(
                |members| {
                    !members.is_empty()
                        && members.iter().all(|&m| {
                            crate::query_boundaries::common::literal_value(self.ctx.types, m)
                                .is_some()
                                || m == TypeId::BOOLEAN_TRUE
                                || m == TypeId::BOOLEAN_FALSE
                        })
                },
            ) && !crate::query_boundaries::common::is_primitive_type(self.ctx.types, source)
                && !display.contains(" | ")
                && !display_is_declared_identifier_source
            {
                self.format_type_diagnostic(source)
            } else {
                display
            }
        } else {
            self.format_nested_assignment_source_type_for_diagnostic(source, target, idx)
        };
        let mut target_str = if depth == 0 {
            self.format_top_level_assignability_message_types_at(source, target, idx)
                .1
        } else if self.should_strip_nullish_for_property_display(target)
            && let Some(stripped) = self.strip_nullish_for_assignability_display(target, source)
        {
            self.format_type_for_assignability_message(stripped)
        } else {
            self.format_assignability_type_for_message(target, source)
        };
        if depth == 0 {
            let source_enum_symbol = self.enum_symbol_from_enumish_type(source);
            let target_enum_symbol = self.enum_symbol_from_enumish_type(target);
            if source_enum_symbol.is_some()
                && target_enum_symbol.is_some()
                && source_enum_symbol != target_enum_symbol
            {
                source_str = self.format_assignability_type_for_message(source, target);
                target_str = self.format_assignability_type_for_message(target, source);
            }
            let source_expr_idx = self
                .assignment_source_expression(idx)
                .or_else(|| self.direct_diagnostic_source_expression(idx));
            let declared_identifier_is_literal_only_alias =
                source_expr_idx.is_some_and(|expr_idx| {
                    self.declared_identifier_has_literal_only_alias_source(expr_idx)
                });
            if !declared_identifier_is_literal_only_alias
                && !self.is_object_rest_assignment_target_anchor(idx)
                && let Some(expr_idx) = source_expr_idx
                && let Some(display) =
                    self.declared_identifier_source_display(expr_idx, target, source)
                && self
                    .declared_identifier_candidate_preserves_source_surface(&source_str, &display)
            {
                source_str = display;
            }
            let source_is_direct_type_query_primitive = self
                .direct_diagnostic_source_expression(idx)
                .or_else(|| self.assignment_source_expression(idx))
                .and_then(|expr_idx| {
                    self.direct_type_query_primitive_source_display(expr_idx, source)
                })
                .is_some_and(|display| display == source_str);
            if !crate::error_reporter::assignability::display_is_literal_value(&source_str)
                && !source_is_direct_type_query_primitive
                && !crate::query_boundaries::common::is_tuple_type(self.ctx.types, source)
                && let Some(display) = self.evaluated_literal_alias_source_display(source)
            {
                source_str = self
                    .canonicalize_assignment_numeric_literal_union_display(source, target, display);
            }
            if let Some(display) = self.evaluated_literal_alias_source_display(target) {
                target_str = self
                    .canonicalize_assignment_numeric_literal_union_display(target, source, display);
            }
            source_str = self.rewrite_source_display_for_non_literal_target_assignability(
                source, target, source_str,
            );
            let has_declared_target_annotation =
                self.assignment_target_expression(idx).is_some_and(|expr| {
                    self.declared_type_annotation_text_for_expression(expr)
                        .is_some()
                        || self
                            .declared_intersection_annotation_display_for_expression(expr)
                            .is_some()
                });
            if !has_declared_target_annotation {
                target_str =
                    self.rewrite_target_display_for_non_literal_assignability(target, target_str);
            }
            if let Some(widened) = self.rewrite_standalone_literal_source_for_keyof_display(
                &source_str,
                &target_str,
                target,
            ) {
                source_str = widened;
            }
        }
        if let Some(display) = self.object_literal_property_literal_union_alias_target_display(
            target,
            &target_str,
            idx,
        ) {
            target_str = display;
        }
        source_str = self.normalize_template_placeholder_spacing_for_display(&source_str);
        target_str = self.normalize_template_placeholder_spacing_for_display(&target_str);
        // When source and target have the same unqualified display name (e.g.,
        // source is "Abcd.E" and target is "E"), disambiguate the target using
        // the annotation text. tsc shows "Type 'Abcd.E' is not assignable to type
        // 'First.E'" when both types are named "E" from different namespaces.
        if depth == 0 {
            let source_unqualified = source_str.rsplit('.').next().unwrap_or(&source_str);
            if (source_unqualified == target_str || source_str == target_str)
                && let Some(target_expr) = self.assignment_target_expression(idx)
                && let Some(annotation) =
                    self.declared_type_annotation_text_for_expression(target_expr)
            {
                let trimmed = annotation.trim();
                if trimmed.contains('.') && !trimmed.contains(' ') && !trimmed.contains('{') {
                    target_str = self.format_annotation_like_type(trimmed);
                }
            }
        }
        if depth == 0 && target_str == "Object" {
            let evaluated = self.evaluate_type_for_assignability(source);
            let widened = crate::query_boundaries::common::widen_type(self.ctx.types, evaluated);
            source_str = self.format_type_for_diagnostic_role(
                widened,
                DiagnosticTypeDisplayRole::WidenedDiagnostic,
            );
        }
        if depth == 0
            && (target_str == "Callable" || target_str == "Applicable")
            && !crate::query_boundaries::common::is_primitive_type(self.ctx.types, source)
        {
            let prop_name = if target_str == "Callable" {
                "call"
            } else {
                "apply"
            };
            let message = format_message(
                diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                &[prop_name, &source_str, &target_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            );
        }

        if depth == 0
            && let Some((prop_name, owner_name, visibility)) =
                self.private_or_protected_member_missing_display(source, target, None)
        {
            let (source_display, target_display) =
                self.finalize_pair_display_for_diagnostic(source, target, source_str, target_str);
            let message = self.private_or_protected_assignability_message(
                &source_display,
                &target_display,
                &prop_name,
                &owner_name,
                visibility,
                None,
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // Skip single-missing-property lookup when the target is an intersection type.
        let target_is_intersection_for_mismatch = {
            let target_eval = self.evaluate_type_with_env(target);
            crate::query_boundaries::common::is_intersection_type(self.ctx.types, target)
                || crate::query_boundaries::common::is_intersection_type(
                    self.ctx.types,
                    target_eval,
                )
        };
        if depth == 0
            && !target_is_intersection_for_mismatch
            && let Some(property_name) = self.missing_single_required_property(source, target)
        {
            let prop_name = self.ctx.types.resolve_atom_ref(property_name);
            if tsz_solver::utils::is_synthetic_private_brand_name(&prop_name) {
                let (source_display, target_display) = self
                    .finalize_pair_display_for_diagnostic(source, target, source_str, target_str);
                let message = self
                    .private_or_protected_brand_backing_member_display(target, None)
                    .map(|(display_prop, owner_name, visibility)| {
                        self.private_or_protected_assignability_message(
                            &source_display,
                            &target_display,
                            &display_prop,
                            &owner_name,
                            visibility,
                            self.property_info_for_display(
                                source,
                                self.ctx.types.intern_string(&display_prop),
                            )
                            .map(|prop| prop.visibility),
                        )
                    })
                    .unwrap_or_else(|| {
                        format_message(
                            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            &[&source_display, &target_display],
                        )
                    });
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
            if let Some(display) =
                self.checked_js_global_element_access_fallback_target_display(idx)
            {
                target_str = display;
            }
            let message = format_message(
                diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                &[&prop_name, &source_str, &target_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            );
        }

        if depth == 0
            && !target_is_intersection_for_mismatch
            && let Some(missing_props) =
                self.missing_required_properties_from_index_signature_source(source, target)
            && missing_props.len() > 1
        {
            // For TS2739 missing-properties source display, when the source is
            // a non-generic type alias whose body is a generic Application
            // (`type B = A<X1, X2, ...>`), tsc unfolds one level to display
            // `A<X1, X2, ...>` rather than the wrapper alias name `B`. See
            // `compiler/objectTypeWithStringAndNumberIndexSignatureToAny.ts`
            // line 91. Falls through to the previous behavior for any other
            // shape so direct Application sources, primitive aliases, etc.
            // continue to format as before.
            let src_str = if let Some(display) =
                self.ts2739_alias_of_application_source_display_text(source)
            {
                display
            } else {
                let evaluated_source = self.evaluate_type_for_assignability(source);
                self.format_type_diagnostic(evaluated_source)
            };
            let tgt_str = self
                .checked_js_global_element_access_fallback_target_display(idx)
                .unwrap_or_else(|| self.format_type_diagnostic(target));
            let prop_list: Vec<String> = missing_props
                .iter()
                .take(4)
                .map(|name| self.ctx.types.resolve_atom_ref(*name).to_string())
                .collect();
            let props_joined = prop_list.join(", ");
            let message = if missing_props.len() > 4 {
                let more_count = (missing_props.len() - 4).to_string();
                format_message(
                    diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                    &[&src_str, &tgt_str, &props_joined, &more_count],
                )
            } else {
                format_message(
                    diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                    &[&src_str, &tgt_str, &props_joined],
                )
            };
            let code = if missing_props.len() > 4 {
                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
            } else {
                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
            };
            return Diagnostic::error(file_name, start, length, message, code);
        }

        if depth == 0
            && !target_is_intersection_for_mismatch
            && source_str == "object"
            && crate::query_boundaries::common::union_members(self.ctx.types, source).is_some_and(
                |members| {
                    members
                        .iter()
                        .any(|member| self.is_object_intrinsic_for_missing_properties(*member))
                },
            )
        {
            let target_candidates = [
                target,
                self.resolve_type_for_property_access(target),
                self.judge_evaluate(target),
                self.evaluate_type_with_env(target),
                self.evaluate_type_for_assignability(target),
            ];
            if let Some(target_with_shape) = target_candidates.into_iter().find(|candidate| {
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, *candidate)
                    .is_some()
            }) {
                let target_shape = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    target_with_shape,
                )
                .expect("target candidate was checked for an object shape");
                let mut missing_with_order: Vec<_> = target_shape
                    .properties
                    .iter()
                    .filter(|prop| !prop.optional)
                    .filter(|prop| {
                        let name = self.ctx.types.resolve_atom_ref(prop.name);
                        !is_object_prototype_method(name)
                    })
                    .map(|prop| (prop.declaration_order, prop.name))
                    .collect();
                missing_with_order.sort_by_key(|(order, _)| *order);
                let missing_props: Vec<_> = missing_with_order
                    .into_iter()
                    .map(|(_, name)| name)
                    .collect();
                if missing_props.len() > 1 {
                    let ordered_names =
                        self.sort_missing_property_names_for_display(target, &missing_props);
                    let is_truncated = ordered_names.len() > 5;
                    let display_count = if is_truncated { 4 } else { 5 };
                    let prop_list: Vec<String> = ordered_names
                        .iter()
                        .take(display_count)
                        .map(|name| self.missing_property_name_for_display(*name, target))
                        .collect();
                    let props_joined = prop_list.join(", ");
                    let message = if is_truncated {
                        let more_count = (ordered_names.len() - display_count).to_string();
                        format_message(
                            diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                            &["{}", &target_str, &props_joined, &more_count],
                        )
                    } else {
                        format_message(
                            diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                            &["{}", &target_str, &props_joined],
                        )
                    };
                    let code = if is_truncated {
                        diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
                    } else {
                        diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                    };
                    return Diagnostic::error(file_name, start, length, message, code);
                }
            }
        }

        let source_from_annotation = declared_numeric_literal_union_source_display
            .as_ref()
            .map(|display| {
                source_str = display.clone();
            })
            .is_some();
        if !source_from_annotation {
            source_str = self
                .canonicalize_assignment_numeric_literal_union_display(source, target, source_str);
        }
        if depth == 0 {
            (source_str, target_str) =
                self.finalize_pair_display_for_diagnostic(source, target, source_str, target_str);
            if !crate::error_reporter::assignability::display_is_literal_value(&source_str)
                && let Some(display) = self.nonmissing_ts2739_alias_source_display_text(source)
            {
                source_str = display;
            }
            if target_str.trim() != "{}"
                && let Some(unfolded) = self.ts2739_alias_target_display(target, &target_str)
            {
                target_str = self.format_type_diagnostic(unfolded);
            }
            if let Some(display) = self.static_schema_array_structural_display(source, target) {
                source_str = display;
            }
            if let Some(display) = self.static_schema_array_structural_display(target, source) {
                target_str = display;
            }
            if let Some(display) = self.type_query_static_array_structural_display(&source_str) {
                source_str = display;
            }
            if let Some((direct_source, direct_target)) =
                self.direct_type_param_alias_application_pair_display(source, target)
            {
                source_str = direct_source;
                target_str = direct_target;
            }
            if !source_from_annotation {
                source_str = self.canonicalize_assignment_numeric_literal_union_display(
                    source, target, source_str,
                );
            }
            target_str = self
                .canonicalize_assignment_numeric_literal_union_display(target, source, target_str);
        }
        let base = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );

        if depth == 0 {
            let nonpublic = self.first_nonpublic_constructor_param_property(target);
            if tracing::enabled!(tracing::Level::TRACE) {
                tracing::trace!(
                    target = %target_str,
                    nonpublic = ?nonpublic,
                    "nonpublic constructor param property probe"
                );
            }
            if nonpublic.is_some() {
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    base,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
        }

        // TS2820: spelling suggestion for string literals.
        // Try both the raw target and its evaluated form (resolves type aliases
        // like `T3 = T1 & ("string" | "boolean")` to their union of string literals).
        let evaluated_target_for_ts2820 = self.evaluate_type_with_env(target);
        let ts2820_suggestion = self
            .find_string_literal_spelling_suggestion(source, target)
            .or_else(|| {
                self.find_string_literal_spelling_suggestion(source, evaluated_target_for_ts2820)
            });
        if let Some(suggestion) = ts2820_suggestion {
            let display_target_str =
                self.format_ts2820_target_display(target, evaluated_target_for_ts2820, &target_str);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                &[&source_str, &display_target_str, &suggestion],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
            );
        }

        // TS2719: when source and target display identically but are different
        // types, emit the more specific "Two different types with this name
        // exist, but they are unrelated" message instead of generic TS2322.
        // Skip when the shared display is a primitive name — primitives have
        // no second declaration that could clash, so the "two different
        // types" framing is wrong. This catches the case where the printer
        // collapsed a complex type (e.g. a deferred conditional whose branch
        // upper bound is `string`) to a primitive spelling that happens to
        // equal the source's primitive display.
        if source_str == target_str
            && !crate::error_reporter::assignability::is_primitive_type_name(&source_str)
            // Literal-value displays (`"foo"`, `42`, `true`, etc.) have no
            // nominal identity. Identical literal displays always mean
            // identical types, so emitting TS2719 with messages like
            // `Type '"foo"' is not assignable to type '"foo"'` is misleading.
            // Fall through to TS2322.
            && !crate::error_reporter::assignability::display_is_literal_value(&source_str)
        {
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
                &[&source_str, &target_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
            );
        }

        Diagnostic::error(
            file_name,
            start,
            length,
            base,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        )
    }

    pub(in crate::error_reporter) fn ts2739_alias_target_display(
        &self,
        target: TypeId,
        target_display: &str,
    ) -> Option<TypeId> {
        if target_display.starts_with('[')
            && crate::query_boundaries::common::is_tuple_type(self.ctx.types, target)
        {
            None
        } else {
            self.ts2739_alias_of_application_source_display(target)
        }
    }
}
