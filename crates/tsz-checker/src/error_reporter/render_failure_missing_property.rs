use super::render_failure_missing_property_base_class::{
    MissingPropertyBaseClassNames, MissingPropertyMessageParts,
};
use super::*;
use crate::query_boundaries::diagnostics::IndexKind;

impl<'a> CheckerState<'a> {
    // Extracted from `render_failure.rs` to keep assignability rendering under the file-size cap.

    pub(super) fn render_missing_property(
        &mut self,
        ctx: &RenderContext,
        property_name: tsz_common::interner::Atom,
        source_type: TypeId,
        target_type: TypeId,
    ) -> Diagnostic {
        let source = ctx.source;
        let target = ctx.target;
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();
        let source_type_is_object = self.is_object_intrinsic_for_missing_properties(source_type);
        // Primitive sources use TS2322 rather than missing-property wording.
        let display_src_str = if depth == 0 && !source_type_is_object {
            // The caller's context may own display policy the renderer cannot
            // recompute here (argument-path fresh-literal widening).
            if let Some(display) = ctx.source_display_override.clone() {
                display
            } else {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            }
        } else {
            self.format_type_diagnostic(source_type)
        };
        // Distinguish "outer source is primitive" (e.g. `let y: Foo = 42`) from
        // "inner source_type is primitive" (e.g. assigning `{ one: number }` to
        // `{ [k: string]: Foo }`, where the solver reports `MissingProperty(foo,
        // src_ty=number, tgt_ty=Foo)` describing the failed nested check). In
        // the first case we want the primitive-vs-target message; in the second
        // we want the OUTER source/target shown, not the inner property types.
        let outer_source_is_primitive =
            crate::query_boundaries::common::is_primitive_type(self.ctx.types, source)
                || is_primitive_type_name(&display_src_str);
        let inner_source_type_is_primitive = !source_type_is_object
            && crate::query_boundaries::common::is_primitive_type(self.ctx.types, source_type);
        let is_source_primitive =
            outer_source_is_primitive || (depth > 0 && inner_source_type_is_primitive);
        if is_source_primitive {
            let tgt_str = self.primitive_source_missing_property_target_display(
                depth,
                target,
                target_type,
                idx,
            );
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&display_src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // Pure function sources against non-callable targets use TS2322; class
        // constructors still keep the missing-property path.
        if self.should_suppress_missing_property_for_callable_source(source, source_type, target) {
            let src_str = if depth == 0 {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            } else {
                self.format_type_diagnostic(source_type)
            };
            let tgt_str = if depth == 0 {
                self.format_assignability_type_for_message(target, source)
            } else {
                self.format_type_diagnostic(target_type)
            };
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 instead of TS2741 when the target has an index signature but the
        // missing property is not a direct named property of the target. In this case, the
        // "missing" property comes from the index signature value type, not from a required
        // named property, so the generic assignability error is more appropriate.
        // Skip this check for array/tuple targets: their properties (like `length`) come
        // from the Array interface and ARE named properties even though the array also has
        // a numeric index signature.
        {
            let target_is_array_or_tuple =
                crate::query_boundaries::common::array_element_type(self.ctx.types, target)
                    .is_some()
                    || crate::query_boundaries::common::is_tuple_type(self.ctx.types, target);
            let target_has_index = !target_is_array_or_tuple
                && crate::query_boundaries::index_signature::has_string_or_number_index_signature(
                    self.ctx.types,
                    target,
                );
            if target_has_index {
                let prop_name_str = self.ctx.types.resolve_atom_ref(property_name);
                let target_has_named_prop = crate::query_boundaries::common::find_property_by_str(
                    self.ctx.types,
                    target,
                    &prop_name_str,
                )
                .is_some();
                if !target_has_named_prop {
                    let src_str = if depth == 0 {
                        self.format_type_for_diagnostic_role(
                            source,
                            DiagnosticTypeDisplayRole::AssignmentSource {
                                target,
                                anchor_idx: idx,
                            },
                        )
                    } else {
                        self.format_type_diagnostic(source_type)
                    };
                    let tgt_str = if depth == 0 {
                        self.format_assignability_type_for_message(target, source)
                    } else {
                        self.format_type_diagnostic(target_type)
                    };
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&src_str, &tgt_str],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }
            }
        }

        // TSC emits TS2322 instead of TS2741 when both source and target have index signatures.
        // For index signature to index signature assignments, the more general assignability error
        // is preferred over specific missing property errors.
        // Skip for array/tuple targets — their numeric index is implicit and missing named
        // properties (like `length`) should still produce TS2741.
        // Check both original and evaluated types (needed for generic class instances)
        let source_evaluated = self.evaluate_type_with_env(source);
        let target_evaluated = self.evaluate_type_with_env(target);
        let target_is_array_or_tuple_for_idx =
            crate::query_boundaries::common::array_element_type(self.ctx.types, target).is_some()
                || crate::query_boundaries::common::is_tuple_type(self.ctx.types, target);
        let source_has_index = [source, source_evaluated].iter().any(|t| {
            crate::query_boundaries::index_signature::has_string_or_number_index_signature(
                self.ctx.types,
                *t,
            )
        });
        let target_has_index = !target_is_array_or_tuple_for_idx
            && [target, target_evaluated].iter().any(|t| {
                crate::query_boundaries::index_signature::has_string_or_number_index_signature(
                    self.ctx.types,
                    *t,
                )
            });
        if source_has_index && target_has_index {
            let src_str = self.format_type_diagnostic(source);
            let tgt_str = self.format_type_diagnostic(target);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // Also emit TS2322 for wrapper-like built-ins (Boolean, Number, String, Object)
        let tgt_str = self.format_type_diagnostic(target_type);
        let original_tgt_str = self.format_type_diagnostic(target);
        if is_builtin_wrapper_name(&tgt_str) || is_builtin_wrapper_name(&original_tgt_str) {
            let src_str = self.format_type_diagnostic(source_type);
            let display_tgt = if is_builtin_wrapper_name(&original_tgt_str) {
                &original_tgt_str
            } else {
                &tgt_str
            };
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, display_tgt],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 instead of TS2741 when the target type is an intersection type.
        // tsc keeps the top-level TS2322 but elaborates which intersection member requires the
        // missing property; returning the bare TS2322 alone hid this root reason
        // ("intersection fallback hides root property mismatch").
        if let Some((intersection, recovered)) =
            self.resolve_intersection_target_for_display_kind(target_type, target, idx)
        {
            // Source display must replicate the path that previously handled
            // each case so no conformance baseline shifts:
            // - recovered (merged) intersections were the flat TS2741 path, which
            //   widens the top-level assigned literal at the anchor
            //   (`{ b: "s" }` -> `{ b: string }`);
            // - genuine intersections were the old intersection path, which keeps
            //   the source as-is so a contextually-literal nested value stays
            //   intact (`{ a: 0 }` -> `{ a: 0 }`, not `{ a: number }`).
            let src_str = if recovered && depth == 0 {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            } else {
                self.format_type_diagnostic(source_type)
            };
            // A recovered (merged) intersection renders its top-level target from
            // the written annotation (see helper); genuine intersections keep the
            // structural display.
            let tgt_str = if recovered {
                self.recovered_intersection_top_level_display(intersection, target, source, idx)
            } else {
                self.format_type_diagnostic(intersection)
            };
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            let mut diag = Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            self.push_intersection_member_elaboration(
                &mut diag,
                intersection,
                &[property_name],
                &src_str,
                start,
                length,
            );
            return diag;
        }

        // An intersection *source* (written directly, via an alias such as
        // `Branded<T> = T & { __brand }`, or via a generic application whose base
        // resolves to an intersection like `LinkedList<T> = T & { next }`) does NOT
        // downgrade a genuine missing required named property to a generic TS2322.
        // tsc reports the property-level miss (TS2741/TS2739) and displays the source
        // as-written — e.g. `Property 'b' is missing in type 'LinkedList<{ a: number; }>'
        // but required in type '{ a: number; b: string; }'`. The intersection-specific
        // TS2322 elaboration applies to intersection *targets* (handled above), where
        // tsc explains which member requires the property; it is not a source concern.
        // Flattened plain-object intersections already reach the TS2741 path here, and
        // the plural `render_missing_properties` path likewise never downgrades for
        // intersection sources, so both singular and plural paths stay consistent.

        // Private brand properties handling
        let prop_name = self.ctx.types.resolve_atom_ref(property_name).to_string();
        if tsz_solver::utils::is_synthetic_private_brand_name(&prop_name) {
            let src_str = if depth == 0 {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            } else {
                self.format_type_for_assignability_message(source_type)
            };
            let tgt_str = if depth == 0 {
                self.format_assignability_type_for_message(target, source)
            } else {
                self.format_type_for_assignability_message(target_type)
            };
            // Try to find the backing private/protected member for a detailed message.
            if depth == 0
                && let Some((member_name, owner_name, visibility)) =
                    self.private_or_protected_member_missing_display(source, target, None)
            {
                let message = self.private_or_protected_assignability_message(
                    &src_str,
                    &tgt_str,
                    &member_name,
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
            // Source HAS the property but with wrong visibility/nominal identity.
            if depth == 0
                && let Some((display_prop, owner_name, visibility)) =
                    self.private_or_protected_brand_backing_member_display(target, None)
            {
                let message = self.private_or_protected_assignability_message(
                    &src_str,
                    &tgt_str,
                    &display_prop,
                    &owner_name,
                    visibility,
                    self.property_info_for_display(
                        source,
                        self.ctx.types.intern_string(&display_prop),
                    )
                    .map(|prop| prop.visibility),
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 instead of TS2741 when the target is an intersection type.
        if self
            .resolve_intersection_target_for_display(target_type, target, idx)
            .is_some()
        {
            let src_str = self.format_type_diagnostic(source);
            let tgt_str_full = self.format_type_diagnostic(target);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str_full],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 when the target's declared type annotation is an intersection type.
        if self.anchor_target_has_intersection_annotation(idx) {
            let src_str = self.format_type_diagnostic(source);
            let tgt_str_full = self.format_type_diagnostic(target);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str_full],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // Object.prototype methods → emit TS2322 instead of TS2741.
        if is_object_prototype_method(&prop_name) {
            let src_str = self.format_type_diagnostic(source_type);
            let tgt_str = self.format_type_diagnostic(target_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // When the source has an index signature, upgrade TS2741 → TS2739 when needed.
        if depth == 0
            && let Some(all_missing) =
                self.missing_required_properties_from_index_signature_source(source, target)
            && all_missing.len() > 1
        {
            // For TS2739 source display, when the source is a non-generic
            // type alias whose body is a generic Application
            // (`type B = A<X1, X2, ...>`), tsc unfolds one level to display
            // the application form `A<X1, X2, ...>` rather than the wrapper
            // alias name `B`. See `compiler/objectTypeWithStringAndNumberIndexSignatureToAny.ts`
            // line 91. Falls through to the role formatter for any other shape.
            let src_str = if let Some(display) =
                self.ts2739_alias_of_application_source_display_text(source)
            {
                display
            } else {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            };
            let tgt_str = self
                .checked_js_global_element_access_fallback_target_display(idx)
                .or_else(|| self.written_alias_reference_target_display(idx, target))
                .unwrap_or_else(|| self.format_assignability_type_for_message(target, source));
            // tsc truncates to "and N more" only ABOVE five missing
            // properties; five or fewer list in full (shared helper rule).
            let (props_joined, more) = self.truncated_missing_property_list(&all_missing, target);
            let (message, code) = if let Some(more_count) = more {
                let more_count = more_count.to_string();
                (
                    format_message(
                        diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                        &[&src_str, &tgt_str, &props_joined, &more_count],
                    ),
                    diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                )
            } else {
                (
                    format_message(
                        diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                        &[&src_str, &tgt_str, &props_joined],
                    ),
                    diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                )
            };
            return Diagnostic::error(file_name, start, length, message, code);
        }

        if depth == 0 {
            let source_resolved = self.resolve_type_for_property_access(source_type);
            let source_evaluated = self.evaluate_type_for_assignability(source_type);
            let target_resolved = self.resolve_type_for_property_access(target_type);
            let target_evaluated = self.evaluate_type_for_assignability(target_type);
            let source_candidates = [source_type, source, source_resolved, source_evaluated];
            let target_candidates = [target_type, target, target_resolved, target_evaluated];
            if let Some((target_symbol, target_display_type, class_own_missing)) = self
                .class_own_missing_properties_for_display(
                    &source_candidates,
                    &target_candidates,
                    property_name,
                    target_type,
                )
            {
                let src_str = self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                );
                let tgt_str = self
                    .ctx
                    .binder
                    .get_symbol(target_symbol)
                    .map(|symbol| symbol.escaped_name.to_string())
                    .unwrap_or_else(|| self.format_type_diagnostic(target_display_type));
                let ordered_names = self.sort_missing_property_names_for_display(
                    target_display_type,
                    &class_own_missing,
                );
                let prop_list: Vec<String> = ordered_names
                    .iter()
                    .take(5)
                    .map(|name| {
                        self.missing_property_list_name_for_display(*name, target_display_type)
                    })
                    .collect();
                let props_joined = prop_list.join(", ");
                let message = format_message(
                    diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                    &[&src_str, &tgt_str, &props_joined],
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                );
            }
        }

        // TS2741: Property 'x' is missing in type 'A' but required in type 'B'.
        let widened_source =
            crate::query_boundaries::widening::widen_type_for_display_preserving_non_fresh(
                self.ctx.types,
                source_type,
            );
        let (mut src_str, mut tgt_str_qualified) = if depth == 0 {
            let src = if source_type == TypeId::OBJECT {
                "{}".to_string()
            } else if let Some(display) = ctx.source_display_override.clone() {
                // The caller's context owns display policy the renderer
                // cannot reproduce here (argument-path fresh-literal
                // widening through the CallArgument role).
                display
            } else if let Some(base_display) =
                self.private_identifier_missing_source_base_display(source, property_name)
            {
                base_display
            } else {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            };
            let widened_target = self.widen_fresh_object_literal_properties_for_display(target);
            // The "required in type '_'" name comes from the alias reference
            // written at this anchor when one resolves: the `FlattenedDiagnostic`
            // role has no anchor, so its reverse type-to-def lookup would
            // answer the first-registered alias of the lowered shape.
            let tgt = self
                .written_alias_reference_target_display(idx, widened_target)
                .unwrap_or_else(|| {
                    self.format_type_for_diagnostic_role(
                        widened_target,
                        DiagnosticTypeDisplayRole::FlattenedDiagnostic,
                    )
                });
            (src, tgt)
        } else if source_type == TypeId::OBJECT {
            ("{}".to_string(), tgt_str)
        } else {
            self.format_type_pair_diagnostic(widened_source, target)
        };
        // When source and target collapse to the same short name (e.g. two
        // same-named classes from different modules), re-qualify them so the
        // reader can tell them apart. The formatter's pair-disambiguation
        // path adds namespace or `import("<specifier>")` prefixes only when
        // the bare names collide.
        //
        // Two cases:
        //   1. `src_str == tgt_str_qualified`: both formatted to the same
        //      short name — disambiguate both sides.
        //   2. `src_str` was already qualified by expression text (e.g.
        //      `N.A` from `new N.A()`) but the underlying source and target
        //      types still share a bare formatted name (e.g. both "A").
        //      Keep the source text as-is and only qualify the target.
        if widened_source != target {
            if src_str == tgt_str_qualified {
                let (da, db) = self.format_type_pair_diagnostic(widened_source, target);
                src_str = da;
                tgt_str_qualified = db;
            } else if crate::query_boundaries::diagnostics::distinct_types_share_nominal_diagnostic_name(
                self.ctx.types,
                self.ctx.binder,
                &self.ctx.definition_store,
                widened_source,
                target,
            ) {
                let (_, db) = self.format_type_pair_diagnostic(widened_source, target);
                if db != tgt_str_qualified {
                    tgt_str_qualified = db;
                }
            }
        }
        if depth == 0
            && let Some(display) =
                self.checked_js_global_element_access_fallback_target_display(idx)
        {
            tgt_str_qualified = display;
        }
        let prop_name_display = self.missing_property_name_for_display(property_name, target);
        // A base class named on either side of the message takes the TS2322
        // head with this line nested beneath it; see
        // `render_failure_missing_property_base_class`. Nested renderings
        // already sit under a head, so only the top level decides.
        let base_class_names = if depth == 0 {
            self.missing_property_base_class_names(source, &[target_type, target], property_name)
        } else {
            MissingPropertyBaseClassNames::default()
        };
        let endpoint_src_str = if base_class_names.source.is_some() {
            self.format_type_for_diagnostic_role(
                source,
                DiagnosticTypeDisplayRole::AssignmentSource {
                    target,
                    anchor_idx: idx,
                },
            )
        } else {
            src_str.clone()
        };
        let mut diagnostic = self.missing_property_diagnostic_with_base_class_head(
            (file_name, start, length),
            &base_class_names,
            MissingPropertyMessageParts {
                property: &prop_name_display,
                endpoint_source: &endpoint_src_str,
                endpoint_target: &tgt_str_qualified,
                nested_source: &src_str,
                nested_target: &tgt_str_qualified,
            },
        );
        // tsc's `reportUnmatchedProperty` pairs the one-missing-property form
        // with a TS2728 pointer at that property's own declaration. The
        // multi-property forms (TS2739/TS2740) above return before this point
        // and carry no pointer, matching tsc.
        // A nested frame reaches the same pointer through the path-aware route:
        // tsc draws no depth distinction here, but the leaf property name alone
        // does not locate a member of the *outer* annotation, so the object
        // literal's own syntax supplies the path first.
        // Only the leaf target is offered to the symbol route on a nested frame:
        // the *outer* target is a different type that legitimately does not
        // declare this property, and asking it would either decline anyway or,
        // for a same-named member, anchor in the wrong declaration.
        let owner_candidates: &[TypeId] = if depth == 0 {
            &[target, target_type]
        } else {
            &[target_type]
        };
        let declared_here = self.missing_property_declared_here_related(
            owner_candidates,
            idx,
            property_name,
            &prop_name_display,
        );
        if let Some(related) = declared_here {
            diagnostic.related_information.push(related);
        }
        diagnostic
    }

    /// Target display for the primitive-source TS2322 downgrade of a
    /// missing-property failure (`let x: T = 42`).
    ///
    /// The solver's `MissingProperty` reason records the *evaluated* member
    /// type it elaborated against (`target_type`), but tsc's
    /// `reportRelationError` renders the original relation target: when the
    /// declared target carries a type-alias surface (a generic alias
    /// application or a bare alias reference), tsc's `reportErrorResults`
    /// restores it, and the alias-retention display policy decides whether the
    /// name survives (`MappedAlias<{ m: string; }>`) or the instantiation
    /// reduced it away (`IdxAlias<{ x: X }>` → `X`). Anonymous targets keep
    /// the recorded evaluated type, preserving the established rendering.
    fn primitive_source_missing_property_target_display(
        &mut self,
        depth: u32,
        target: TypeId,
        target_type: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if depth != 0 {
            return self.recursive_non_generic_alias_body_name(target_type);
        }
        if let Some(display) =
            self.anonymous_composite_annotation_target_display(anchor_idx, target)
        {
            return display;
        }
        // `target` here may already be the strip-rebound union member: the
        // rebind in `render_failure_reason` strips a nullable union to its
        // sole non-nullish member (`Point | null` → `Point`,
        // `x: MaybeRec` → `Rec0`) unless a primitive source restores the
        // alias surface. When the alias-carrying union survived that rebind,
        // the restore must win here too: tsc's `reportErrorResults` renders
        // the original alias-named target whole for the whole-relation
        // failure of a primitive source (`x: MaybeBox = 5` shows `MaybeBox`,
        // `x: OrMissing<{ u: string }> = 5` shows `OrMissing<{ u: string; }>`
        // — a union-bodied alias application is not nullish-stripped). The
        // annotation only *adds* the bare-alias-reference case; a negative
        // verdict must not veto a member's own application surface
        // (`MappedAlias<{ m: string; }> | undefined` strips to the member,
        // which keeps its alias).
        let restores_alias = crate::query_boundaries::diagnostics::type_keeps_alias_symbol_surface(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            target,
        ) || self
            .assignment_target_annotation_alias_reference_verdict(anchor_idx)
            == Some(true);
        if restores_alias {
            // A recursive non-generic alias surface restores as its *name*:
            // the general formatter unrolls the cycle one evaluation step per
            // render (`Box2` → `Box<number | Box<number | Box2>>` for
            // `type Box2 = Box<Box2 | number>`), where tsc keeps `Box2`.
            if let Some(name) = self.recursive_non_generic_alias_body_display_name(target) {
                return name;
            }
            // The alias name must come from the reference *written at this
            // anchor* when one resolves: the formatter's reverse type-to-def
            // lookup is earliest-declaration-wins per interned `TypeId`, so
            // two aliases of one shape would both restore the first (`ObjA`
            // for a target written `: ObjB`).
            if let Some(display) = self.written_alias_reference_target_display(anchor_idx, target) {
                return display;
            }
            // Not the pair formatter: its top-level nullish strip would undo
            // the alias restoration (`MaybeBox` must not strip to its
            // non-nullish member).
            return self.format_type_for_assignability_message(target);
        }
        self.recursive_non_generic_alias_body_name(target_type)
    }

    /// Find the intersection member that requires `property_name`, i.e. declares
    /// it as a non-optional named property. Members are evaluated when a direct
    /// lookup misses so mapped/applied members such as `Map1<{...}>` are
    /// inspected too; the *as-written* member id is returned so its display
    /// keeps the generic/mapped form tsc shows (`Map1<{ ... }>`, not the
    /// expanded object). Returns `None` when no member requires the property
    /// (e.g. the requirement comes from an index signature), leaving the caller
    /// with the bare top-level message.
    /// Whether intersection `member` declares `property_name` as a required
    /// (non-optional) named property. Members are evaluated when a direct lookup
    /// misses so mapped/applied members such as `Map1<{...}>` are inspected too.
    pub(super) fn intersection_member_requires_property(
        &mut self,
        member: TypeId,
        property_name: tsz_common::interner::Atom,
    ) -> bool {
        let prop_str = self.ctx.types.resolve_atom_ref(property_name);
        let found = crate::query_boundaries::common::find_property_by_str(
            self.ctx.types,
            member,
            &prop_str,
        )
        .or_else(|| {
            let evaluated = self.evaluate_type_with_env(member);
            crate::query_boundaries::common::find_property_by_str(
                self.ctx.types,
                evaluated,
                &prop_str,
            )
        });
        found.is_some_and(|prop| !prop.optional)
    }

    /// Append the elaboration tsc emits for a missing-property mismatch against
    /// an intersection target.
    ///
    /// tsc checks intersection members left-to-right and elaborates only the
    /// FIRST member the source fails against, grouping that member's missing
    /// required properties into a single line:
    /// - one miss   -> `Property 'X' is missing in type 'S' but required in type '<member>'`
    /// - several    -> `Type 'S' is missing the following properties from type '<member>': a, b`
    ///
    /// Reproduce that here instead of emitting one line per property, which
    /// diverged from tsc whenever several missing properties belonged to the
    /// same member.
    pub(super) fn push_intersection_member_elaboration(
        &mut self,
        diag: &mut Diagnostic,
        intersection: TypeId,
        property_names: &[tsz_common::interner::Atom],
        src_str: &str,
        start: u32,
        length: u32,
    ) {
        let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, intersection)
        else {
            return;
        };
        let mut failing: Option<(TypeId, Vec<tsz_common::interner::Atom>)> = None;
        for member in members {
            let member_missing: Vec<tsz_common::interner::Atom> = property_names
                .iter()
                .copied()
                .filter(|&prop| self.intersection_member_requires_property(member, prop))
                .collect();
            if !member_missing.is_empty() {
                failing = Some((member, member_missing));
                break;
            }
        }
        let Some((member, member_missing)) = failing else {
            return;
        };

        let member_str = self.format_type_diagnostic(member);
        let ordered = self.sort_missing_property_names_for_display(member, &member_missing);
        let (message, code) = if ordered.len() == 1 {
            let prop_name_display = self.missing_property_name_for_display(ordered[0], member);
            (
                format_message(
                    diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                    &[&prop_name_display, src_str, &member_str],
                ),
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            )
        } else {
            let (props_joined, more) = self.truncated_missing_property_list(&ordered, member);
            if let Some(more_count) = more {
                let more_count = more_count.to_string();
                (
                    format_message(
                        diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                        &[src_str, &member_str, &props_joined, &more_count],
                    ),
                    diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                )
            } else {
                (
                    format_message(
                        diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                        &[src_str, &member_str, &props_joined],
                    ),
                    diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                )
            }
        };
        diag.push_elaboration_in_span(start, length, message, code, 0);
    }

    fn type_or_display_alias_is_intersection(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::diagnostics::is_intersection_type(self.ctx.types, type_id)
            || self
                .ctx
                .types
                .get_display_alias(type_id)
                .is_some_and(|alias| {
                    crate::query_boundaries::diagnostics::is_intersection_type(
                        self.ctx.types,
                        alias,
                    )
                })
    }

    fn common_missing_property_declaring_type_name(
        &self,
        target_type: TypeId,
        property_names: &[tsz_common::interner::Atom],
    ) -> Option<String> {
        let mut common_parent = None;
        for property_name in property_names {
            let parent = self
                .property_info_for_display(target_type, *property_name)?
                .parent_id?;
            if common_parent.is_some_and(|common| common != parent) {
                return None;
            }
            common_parent = Some(parent);
        }
        let symbol = self.ctx.binder.get_symbol(common_parent?)?;
        Some(symbol.escaped_name.clone())
    }

    /// For TS2739 source display, unfold wrapper aliases like
    /// `type B = A<X>` to the body application `A<X>`. Other shapes keep
    /// normal formatting.
    pub(in crate::error_reporter) fn ts2739_alias_of_application_source_display(
        &self,
        source: TypeId,
    ) -> Option<TypeId> {
        // The source can reach this point either as:
        // - `Lazy(DefId)` when an unevaluated alias reference,
        // - the already-evaluated structural form (find_def_for_type points
        //   back at the alias's definition),
        // - or an `Application(Lazy(DefId), [args...])` when generic.
        let source_application =
            crate::query_boundaries::common::application_info(self.ctx.types, source).or_else(
                || {
                    let alias = self.ctx.types.get_display_alias(source)?;
                    crate::query_boundaries::common::application_info(self.ctx.types, alias)
                },
            );

        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, source)
            .or_else(|| self.ctx.definition_store.find_def_for_type(source))
            .or_else(|| {
                // Application path: peek at the application's base to find
                // the alias's def_id.
                let (base, _) = source_application.as_ref()?;
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, *base)
            })?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return None;
        }
        if def.type_params.is_empty() {
            // Recover the as-written application via display_alias for
            // evaluated sources, or via the alias body for lazy references.
            let app_origin = self
                .ctx
                .types
                .get_display_alias(source)
                .filter(|&alias| {
                    crate::query_boundaries::common::application_id(self.ctx.types, alias).is_some()
                })
                .or(def.body)?;
            let app_id =
                crate::query_boundaries::common::application_id(self.ctx.types, app_origin)?;
            let app = self.ctx.types.type_application(app_id);
            if app.args.is_empty() {
                return None;
            }
            let app_base_def_id =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)?;
            if !self
                .ctx
                .definition_store
                .get(app_base_def_id)
                .is_some_and(|def| {
                    matches!(
                        def.kind,
                        tsz_solver::def::DefKind::TypeAlias | tsz_solver::def::DefKind::Interface
                    )
                })
            {
                return None;
            }
            return Some(app_origin);
        }

        // Generic wrapper alias path: `type IndirectArrayish<U extends ...> =
        // Objectish<U>;` — when source is `IndirectArrayish<any>` and the
        // body is itself an `Application` of a different named alias, tsc
        // unfolds one level to display `Objectish<any>` (the body alias's
        // application form with the wrapper's type-args substituted into the
        // body's slots). See `compiler/mappedTypeWithAny.ts` line 47 — tsc
        // displays `Objectish<any>` for `arr = indirectArrayish` rather than
        // the wrapper name `IndirectArrayish<any>`.
        let body = def.body?;
        let body_app_id = crate::query_boundaries::common::application_id(self.ctx.types, body)?;
        let body_app = self.ctx.types.type_application(body_app_id);
        // Body alias must be different from the wrapper itself (avoid loops).
        let body_def_id =
            crate::query_boundaries::common::lazy_def_id(self.ctx.types, body_app.base)?;
        if body_def_id == def_id {
            return None;
        }
        // Substitute the wrapper's type-params with the source application's
        // args so the displayed application reflects the call-site instantiation.
        let (_, source_args) = source_application?;
        if source_args.len() != def.type_params.len() {
            return None;
        }
        let subst = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &def.type_params,
            &source_args,
        );
        let body_args: Vec<TypeId> = body_app
            .args
            .iter()
            .map(|&arg| {
                crate::query_boundaries::common::instantiate_type_preserving_meta(
                    self.ctx.types,
                    arg,
                    &subst,
                )
            })
            .collect();
        Some(
            self.ctx
                .types
                .factory()
                .application(body_app.base, body_args),
        )
    }

    pub(super) fn render_missing_properties(
        &mut self,
        ctx: &RenderContext,
        property_names: &[tsz_common::interner::Atom],
        source_type: TypeId,
        target_type: TypeId,
    ) -> Diagnostic {
        let source = ctx.source;
        let target = ctx.target;
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();
        let source_type_is_object = self.is_object_intrinsic_for_missing_properties(source_type);
        // TSC emits TS2322 instead of TS2739/TS2740 when the source is a primitive type.
        if !source_type_is_object
            && crate::query_boundaries::common::is_primitive_type(self.ctx.types, source_type)
        {
            let src_str = self.format_type_diagnostic(source_type);
            let tgt_str = self.primitive_source_missing_property_target_display(
                depth,
                target,
                target_type,
                idx,
            );
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 instead of TS2739/TS2740/TS2741 when the source has call
        // signatures (pure function type, NOT class constructor) and the target does NOT
        // have call signatures. Class constructors (with construct-only signatures) should
        // still produce TS2741 for missing properties.
        {
            if self.should_suppress_missing_property_for_callable_source(
                source,
                source_type,
                target,
            ) {
                let src_str = if depth == 0 {
                    self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    )
                } else {
                    self.format_type_diagnostic(source_type)
                };
                let tgt_str = if depth == 0 {
                    self.format_assignability_type_for_message(target, source)
                } else {
                    self.format_type_diagnostic(target_type)
                };
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&src_str, &tgt_str],
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
        }

        // Note: TS2696 for `Object` source is handled at the top of render_failure_reason.

        // Emit TS2322 instead of TS2739/TS2740 when the SOURCE is a wrapper-like built-in.
        let src_str_check = self.format_type_diagnostic(source_type);
        let original_src_check = self.format_type_diagnostic(source);
        if is_builtin_wrapper_name(&src_str_check) || is_builtin_wrapper_name(&original_src_check) {
            let display_src = if is_builtin_wrapper_name(&original_src_check) {
                &original_src_check
            } else {
                &src_str_check
            };
            let tgt_str = self.format_type_diagnostic(target_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[display_src, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 instead of TS2739/TS2740 when the target is an intersection type.
        // Evaluated form is checked too: a mapped-type alias like `Mapped<{ a, b }> & { c }`
        // may only resolve to an intersection after evaluation.
        // The jsdoc anchor path also enters here (intersection_for_members = None in that case)
        // and receives bare TS2322 without per-property elaboration, matching tsc's output.
        let intersection_for_members =
            self.resolve_intersection_target_for_display_kind(target_type, target, idx);
        if intersection_for_members.is_some()
            || self.anchor_jsdoc_type_tag_targets_intersection_alias(idx)
        {
            // Source display must replicate the path that previously handled
            // each case so no conformance baseline shifts:
            // - recovered (merged) intersections were the flat TS2739 path, which
            //   widens the top-level assigned literal at the anchor
            //   (`{ b: "s" }` -> `{ b: string }`);
            // - genuine intersections were the old intersection path, which keeps
            //   the source as-is so a contextually-literal nested value stays
            //   intact (`{ a: 0 }` -> `{ a: 0 }`, not `{ a: number }`).
            let recovered = matches!(intersection_for_members, Some((_, true)));
            let src_str = if recovered && depth == 0 {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            } else {
                self.format_type_diagnostic(source)
            };
            // A recovered (merged) intersection renders its top-level target from
            // the written annotation (see helper); genuine intersections keep the
            // structural display.
            let tgt_str = match intersection_for_members {
                Some((intersection, true)) => {
                    self.recovered_intersection_top_level_display(intersection, target, source, idx)
                }
                _ => self.format_type_diagnostic(target),
            };
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            let mut diag = Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            if let Some((intersection, _recovered)) = intersection_for_members {
                self.push_intersection_member_elaboration(
                    &mut diag,
                    intersection,
                    property_names,
                    &src_str,
                    start,
                    length,
                );
            }
            return diag;
        }

        // TSC emits TS2322 instead of TS2739/TS2740 when both source and target have
        // string index signatures. For number index signatures, suppress only when the
        // target has no explicit named properties (i.e., it's purely an index-signature
        // type like `{ [x: number]: T }`). Named interfaces that happen to have number
        // index signatures (like String, Array) should still get TS2739/TS2740.
        // Check both original and evaluated types (needed for generic class instances)
        let source_evaluated = self.evaluate_type_with_env(source);
        let target_evaluated = self.evaluate_type_with_env(target);
        let has_index = |type_id: TypeId, kind: IndexKind| {
            crate::query_boundaries::diagnostics::has_index_signature(self.ctx.types, type_id, kind)
        };
        let source_has_string_index = [source, source_evaluated]
            .iter()
            .any(|t| has_index(*t, IndexKind::String));
        let target_has_string_index = [target, target_evaluated]
            .iter()
            .any(|t| has_index(*t, IndexKind::String));
        let source_has_number_index = [source, source_evaluated]
            .iter()
            .any(|t| has_index(*t, IndexKind::Number));
        let target_has_number_index = [target, target_evaluated]
            .iter()
            .any(|t| has_index(*t, IndexKind::Number));
        // For number index signatures, only suppress when the missing properties are
        // NOT explicitly declared on the target (they came from index value type expansion).
        // We detect this by checking if none of the missing property names match a real
        // named member of the target type's object shape.
        let number_index_suppress =
            source_has_number_index && target_has_number_index && !property_names.is_empty() && {
                let target_shape = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    target_type,
                );
                property_names.iter().all(|name| {
                    // If none of the missing properties are real named members of the
                    // target type, the "missing properties" came from index value type
                    // comparison, not from actual missing named members.
                    match &target_shape {
                        Some(shape) => !shape.properties.iter().any(|p| p.name == *name),
                        None => true,
                    }
                })
            };
        // When the target is an array/tuple type, the missing properties (length, push,
        // pop, etc.) are real named members, not artifacts of index signature comparison.
        // Don't suppress TS2739/TS2740 in that case — tsc correctly emits them.
        let is_array_target = matches!(
            query_utils::classify_array_like(self.ctx.types, target_type),
            query_utils::ArrayLikeKind::Array(_)
                | query_utils::ArrayLikeKind::Tuple
                | query_utils::ArrayLikeKind::Readonly(_)
        );
        if !is_array_target
            && ((source_has_string_index && target_has_string_index) || number_index_suppress)
        {
            let src_str = self.format_type_diagnostic(source);
            let tgt_str = self.format_type_diagnostic(target);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }
        let _has_non_proto_missing = property_names.iter().any(|name| {
            let s = self.ctx.types.resolve_atom_ref(*name);
            !tsz_solver::utils::is_synthetic_private_brand_name(&s)
                && if is_array_target {
                    !is_object_prototype_method_for_array_target(&s)
                } else {
                    !is_object_prototype_method(&s)
                }
        });
        let filtered_names: Vec<_> = property_names
            .iter()
            .filter(|name| {
                let s = self.ctx.types.resolve_atom_ref(**name);
                if tsz_solver::utils::is_synthetic_private_brand_name(&s) {
                    return false;
                }
                if is_array_target {
                    !is_object_prototype_method_for_array_target(&s)
                } else {
                    !is_object_prototype_method(&s)
                }
            })
            .copied()
            .collect();

        // If all missing properties are numeric indices, emit TS2322.
        let all_numeric = !filtered_names.is_empty()
            && filtered_names.iter().all(|name| {
                let s = self.ctx.types.resolve_atom_ref(*name);
                s.parse::<usize>().is_ok()
            });

        if all_numeric {
            let src_str = self.format_type_diagnostic(source_type);
            let tgt_str = self.format_type_diagnostic(target_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // If all missing properties were private brands, emit TS2322 instead.
        if filtered_names.is_empty() {
            if let Some((prop_name, owner_name, visibility)) =
                self.private_or_protected_member_missing_display(source_type, target_type, None)
            {
                let widened_source =
                    crate::query_boundaries::widening::widen_type_for_display_preserving_non_fresh(
                        self.ctx.types,
                        source_type,
                    );
                let src_str = if source_type_is_object {
                    "{}".to_string()
                } else {
                    self.format_type_diagnostic(widened_source)
                };
                let tgt_str = self.format_type_diagnostic(target_type);
                let message = self.private_or_protected_assignability_message(
                    &src_str,
                    &tgt_str,
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
            let src_str = if depth == 0 {
                if source_type_is_object {
                    "{}".to_string()
                } else {
                    let source_display = self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    );
                    self.rewrite_source_display_for_non_literal_target_assignability(
                        source,
                        target,
                        source_display,
                    )
                }
            } else if source_type_is_object {
                "{}".to_string()
            } else {
                let widened_source =
                    crate::query_boundaries::widening::widen_type_for_display_preserving_non_fresh(
                        self.ctx.types,
                        source_type,
                    );
                self.format_type_diagnostic(widened_source)
            };
            let tgt_str = if depth == 0 {
                self.format_assignability_type_for_message(target, source)
            } else {
                self.format_type_diagnostic(target_type)
            };
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        if filtered_names.len() == 1 {
            let source_resolved = self.resolve_type_for_property_access(source_type);
            let source_evaluated = self.evaluate_type_for_assignability(source_type);
            let target_resolved = self.resolve_type_for_property_access(target_type);
            let target_evaluated = self.evaluate_type_for_assignability(target_type);
            let source_candidates = [source_type, source, source_resolved, source_evaluated];
            let target_candidates = [target_type, target, target_resolved, target_evaluated];
            if let Some((target_symbol, target_display_type, class_own_missing)) = self
                .class_own_missing_properties_for_display(
                    &source_candidates,
                    &target_candidates,
                    filtered_names[0],
                    target_type,
                )
            {
                let src_str = if depth == 0 {
                    if source_type_is_object {
                        "{}".to_string()
                    } else {
                        self.format_type_for_diagnostic_role(
                            source,
                            DiagnosticTypeDisplayRole::AssignmentSource {
                                target,
                                anchor_idx: idx,
                            },
                        )
                    }
                } else {
                    self.format_type_diagnostic(source_type)
                };
                let tgt_str = self
                    .ctx
                    .binder
                    .get_symbol(target_symbol)
                    .map(|symbol| symbol.escaped_name.to_string())
                    .unwrap_or_else(|| self.format_type_diagnostic(target_display_type));
                let ordered_names = self.sort_missing_property_names_for_display(
                    target_display_type,
                    &class_own_missing,
                );
                let prop_list: Vec<String> = ordered_names
                    .iter()
                    .take(5)
                    .map(|name| {
                        self.missing_property_list_name_for_display(*name, target_display_type)
                    })
                    .collect();
                let props_joined = prop_list.join(", ");
                let message = format_message(
                    diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                    &[&src_str, &tgt_str, &props_joined],
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                );
            }
        }

        // When filtering removed brand/prototype properties and only 1 remains, emit TS2741.
        if filtered_names.len() == 1 {
            let prop_name = self
                .ctx
                .types
                .resolve_atom_ref(filtered_names[0])
                .to_string();

            // When the source is a function/callable type and the remaining property is
            // private or protected, the function fundamentally can't satisfy the class's
            // nominal brand requirement. TSC emits TS2322 (general mismatch) here, not
            // TS2741 (missing property). For class-to-class assignments, TSC keeps TS2741.
            let source_is_function =
                crate::query_boundaries::common::is_function_type(self.ctx.types, source)
                    || crate::query_boundaries::common::is_function_type(
                        self.ctx.types,
                        source_type,
                    );
            if source_is_function
                && let Some(prop_info) =
                    self.property_info_for_display(target_type, filtered_names[0])
                && prop_info.visibility != tsz_solver::Visibility::Public
            {
                let src_str = if depth == 0 {
                    self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    )
                } else {
                    self.format_type_diagnostic(source_type)
                };
                let tgt_str = if depth == 0 {
                    self.format_assignability_type_for_message(target, source)
                } else {
                    self.format_type_diagnostic(target_type)
                };
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&src_str, &tgt_str],
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }

            let src_str = if depth == 0 {
                if source_type_is_object {
                    "{}".to_string()
                } else if let Some(base_display) =
                    self.private_identifier_missing_source_base_display(source, filtered_names[0])
                {
                    base_display
                } else {
                    self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    )
                }
            } else if source_type_is_object {
                "{}".to_string()
            } else {
                let widened_source =
                    crate::query_boundaries::widening::widen_type_for_display_preserving_non_fresh(
                        self.ctx.types,
                        source_type,
                    );
                self.format_type_diagnostic(widened_source)
            };
            let tgt_str = if depth == 0 {
                self.checked_js_global_element_access_fallback_target_display(idx)
                    // The written-annotation alias is the target's own display
                    // identity when it lowers to exactly this target; the
                    // declaring-type name stays the fallback for inherited /
                    // merged members.
                    .or_else(|| self.written_alias_reference_target_display(idx, target))
                    .or_else(|| self.property_declaring_type_name(target_type, filtered_names[0]))
                    .unwrap_or_else(|| self.format_assignability_type_for_message(target, source))
            } else {
                self.property_declaring_type_name(target_type, filtered_names[0])
                    .unwrap_or_else(|| self.format_type_diagnostic(target_type))
            };
            // Same head rule as the singular path: a base class named on
            // either side demotes this line under a TS2322.
            let base_class_names = if depth == 0 {
                self.missing_property_base_class_names(
                    source,
                    &[target_type, target],
                    filtered_names[0],
                )
            } else {
                MissingPropertyBaseClassNames::default()
            };
            let endpoint_src_str = if base_class_names.source.is_some() {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            } else {
                src_str.clone()
            };
            let endpoint_tgt_str = if base_class_names.target.is_some() {
                self.format_assignability_type_for_message(target, source)
            } else {
                tgt_str.clone()
            };
            return self.missing_property_diagnostic_with_base_class_head(
                (file_name, start, length),
                &base_class_names,
                MissingPropertyMessageParts {
                    property: &prop_name,
                    endpoint_source: &endpoint_src_str,
                    endpoint_target: &endpoint_tgt_str,
                    nested_source: &src_str,
                    nested_target: &tgt_str,
                },
            );
        }

        // TS2739/TS2740: Type 'A' is missing the following properties from type 'B': x, y, z
        let display_source = if self
            .missing_required_properties_from_index_signature_source(source_type, target_type)
            .is_some()
        {
            self.evaluate_type_for_assignability(source_type)
        } else {
            source_type
        };
        let src_str = if depth == 0 {
            // For TS2739, when the source is a non-generic type alias whose
            // body is a generic Application (`type B = A<X1, X2, ...>`),
            // tsc unfolds one level to display the application form
            // `A<X1, X2, ...>` rather than the wrapper alias name `B`.
            // The application form names both the underlying generic and its
            // type arguments, which is the structural information the
            // "is missing the following properties" message is meant to
            // expose. tsc preserves alias names in TS2322 (target context)
            // and TS2339 (receiver), so this unfold is scoped to TS2739
            // source rendering. See
            // `compiler/objectTypeWithStringAndNumberIndexSignatureToAny.ts`
            // line 91, where `type NumberToNumber = NumberTo<number>` is
            // displayed as `NumberTo<number>` in the missing-properties source.
            if source_type_is_object {
                "{}".to_string()
            } else if let Some(display) =
                self.ts2739_alias_of_application_source_display_text(source)
            {
                display
            } else {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            }
        } else {
            // Nested (depth > 0) source: widen the source for display the same
            // way the single-missing-property path does (`{ id: 1 }` ->
            // `{ id: number }`) so the union-target elaboration's
            // missing-properties line matches tsc's widened display. Route
            // through the pair formatter (rather than the bare single-type
            // formatter) so fresh object-literal property types are widened
            // consistently with the `Property 'x' is missing …` rendering.
            let widened_source =
                crate::query_boundaries::widening::widen_type_for_display_preserving_non_fresh(
                    self.ctx.types,
                    display_source,
                );
            let (src, _) = self.format_type_pair_diagnostic(widened_source, target_type);
            src
        };
        let ordered_names =
            self.sort_missing_property_names_for_display(target_type, &filtered_names);
        let tgt_str = if depth == 0 {
            self.checked_js_global_element_access_fallback_target_display(idx)
                .unwrap_or_else(|| {
                    if self.type_or_display_alias_is_intersection(target_type)
                        || self.type_or_display_alias_is_intersection(target)
                    {
                        self.common_missing_property_declaring_type_name(
                            target_type,
                            &ordered_names,
                        )
                        .unwrap_or_else(|| {
                            self.format_assignability_type_for_message(target, source)
                        })
                    } else {
                        // Per-occurrence written-alias identity beats the
                        // formatter's first-registered reverse lookup (same
                        // rule as the TS2322 target display).
                        self.written_alias_reference_target_display(idx, target)
                            .unwrap_or_else(|| {
                                self.format_assignability_type_for_message(target, source)
                            })
                    }
                })
        } else {
            self.format_type_diagnostic(target_type)
        };
        let (props_joined, more) =
            self.truncated_missing_property_list(&ordered_names, target_type);
        if let Some(more_count) = more {
            let more_count = more_count.to_string();
            let message = format_message(
                diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                &[&src_str, &tgt_str, &props_joined, &more_count],
            );
            Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
            )
        } else {
            let message = format_message(
                diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                &[&src_str, &tgt_str, &props_joined],
            );
            Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
            )
        }
    }
}
