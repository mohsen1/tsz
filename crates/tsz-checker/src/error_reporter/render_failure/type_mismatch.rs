use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages, format_message};
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
        let mut source_str = if depth == 0 {
            let display = self.format_assignment_source_type_for_diagnostic(source, target, idx);
            // tsc preserves literal union structure (e.g., `"c" | "d"`) in error
            // messages. If format_assignment_source_type_for_diagnostic widened the
            // union to a primitive (e.g., `string`) or used a type alias name
            // (e.g., `Variants` from a parameter annotation), fall back to the
            // TypeFormatter which correctly displays literal union members.
            // This handles both widening and flow-narrowed type alias display.
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
            if let Some(display) = self.evaluated_literal_alias_source_display(source) {
                source_str = self.canonicalize_assignment_numeric_literal_union_display(display);
            }
            if let Some(display) = self.evaluated_literal_alias_source_display(target) {
                target_str = self.canonicalize_assignment_numeric_literal_union_display(display);
            }
            source_str = self.rewrite_source_display_for_non_literal_target_assignability(
                source, target, source_str,
            );
            let has_declared_target_annotation = self
                .assignment_target_expression(idx)
                .and_then(|expr| self.declared_type_annotation_text_for_expression(expr))
                .is_some();
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
            source_str = self.format_type_diagnostic_widened(widened);
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
            let message = self.private_or_protected_assignability_message(
                &source_str,
                &target_str,
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
            if prop_name.starts_with("__private_brand") {
                let message = self
                    .private_or_protected_brand_backing_member_display(target, None)
                    .map(|(display_prop, owner_name, visibility)| {
                        self.private_or_protected_assignability_message(
                            &source_str,
                            &target_str,
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
                            &[&source_str, &target_str],
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
            let evaluated_source = self.evaluate_type_for_assignability(source);
            let src_str = self.format_type_diagnostic(evaluated_source);
            let tgt_str = self.format_type_diagnostic(target);
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

        source_str = self.canonicalize_assignment_numeric_literal_union_display(source_str);

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
            // TSC uses the expanded union form (not the alias name) when emitting TS2820.
            let expanded_target_str = self.format_type_diagnostic(evaluated_target_for_ts2820);
            let display_target_str = if expanded_target_str != target_str {
                &expanded_target_str
            } else {
                &target_str
            };
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                &[&source_str, display_target_str, &suggestion],
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
        if source_str == target_str {
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
}
