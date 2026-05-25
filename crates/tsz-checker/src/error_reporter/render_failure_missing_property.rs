use super::*;

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
            let tgt_str = self.recursive_non_generic_alias_body_name(target_type);
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
            use crate::query_boundaries::common::{IndexKind, IndexSignatureResolver};
            let resolver = IndexSignatureResolver::new(self.ctx.types);
            let target_is_array_or_tuple =
                crate::query_boundaries::common::array_element_type(self.ctx.types, target)
                    .is_some()
                    || crate::query_boundaries::common::tuple_list_id(self.ctx.types, target)
                        .is_some();
            let target_has_index = !target_is_array_or_tuple
                && (resolver.has_index_signature(target, IndexKind::String)
                    || resolver.has_index_signature(target, IndexKind::Number));
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
        use crate::query_boundaries::common::{IndexKind, IndexSignatureResolver};
        let resolver = IndexSignatureResolver::new(self.ctx.types);
        // Check both original and evaluated types (needed for generic class instances)
        let source_evaluated = self.evaluate_type_with_env(source);
        let target_evaluated = self.evaluate_type_with_env(target);
        let target_is_array_or_tuple_for_idx =
            crate::query_boundaries::common::array_element_type(self.ctx.types, target).is_some()
                || crate::query_boundaries::common::tuple_list_id(self.ctx.types, target).is_some();
        let source_has_index = [source, source_evaluated].iter().any(|t| {
            resolver.has_index_signature(*t, IndexKind::String)
                || resolver.has_index_signature(*t, IndexKind::Number)
        });
        let target_has_index = !target_is_array_or_tuple_for_idx
            && [target, target_evaluated].iter().any(|t| {
                resolver.has_index_signature(*t, IndexKind::String)
                    || resolver.has_index_signature(*t, IndexKind::Number)
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
        let target_evaluated_for_intersection = self.evaluate_type_with_env(target);
        if crate::query_boundaries::common::is_intersection_type(self.ctx.types, target_type)
            || crate::query_boundaries::common::is_intersection_type(self.ctx.types, target)
            || crate::query_boundaries::common::is_intersection_type(
                self.ctx.types,
                target_evaluated_for_intersection,
            )
        {
            let src_str = self.format_type_diagnostic(source_type);
            let tgt_str = if crate::query_boundaries::common::is_intersection_type(
                self.ctx.types,
                target_evaluated_for_intersection,
            ) {
                self.format_type_diagnostic(target_evaluated_for_intersection)
            } else if crate::query_boundaries::common::is_intersection_type(self.ctx.types, target)
            {
                self.format_type_diagnostic(target)
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

        // TSC emits TS2322 instead of TS2741 when the *source* type is an intersection.
        // This covers type aliases like `LinkedList<T> = T & { next: ... }` that may have
        // been evaluated to an intersection by the time we reach diagnostic rendering.
        // Check both the type data and the source's declaration annotation, since
        // intersections may be flattened into Object types by the solver.
        let source_evaluated_for_intersection = self.evaluate_type_with_env(source);
        if crate::query_boundaries::common::is_intersection_type(self.ctx.types, source)
            || crate::query_boundaries::common::is_intersection_type(
                self.ctx.types,
                source_evaluated_for_intersection,
            )
            || (depth == 0 && self.anchor_source_has_intersection_annotation(idx))
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

        // TSC emits TS2322 instead of TS2741 when the source is a type application
        // (generic type alias) whose base type resolves to an intersection. For example,
        // `LinkedList<Entity>` where `type LinkedList<T> = T & { next: LinkedList<T> }`.
        // Named type aliases expanding to intersections are reported as general
        // assignability failures, not property-level "missing" errors.
        if let Some((base, _args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, source)
        {
            let base_eval = self.evaluate_type_with_env(base);
            let base_is_intersection =
                crate::query_boundaries::common::is_intersection_type(self.ctx.types, base)
                    || crate::query_boundaries::common::is_intersection_type(
                        self.ctx.types,
                        base_eval,
                    );
            if base_is_intersection {
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
        if crate::query_boundaries::common::is_intersection_type(self.ctx.types, target_type)
            || crate::query_boundaries::common::is_intersection_type(self.ctx.types, target)
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
                .unwrap_or_else(|| self.format_assignability_type_for_message(target, source));
            let prop_list: Vec<String> = all_missing
                .iter()
                .take(4)
                .map(|name| self.missing_property_list_name_for_display(*name))
                .collect();
            let props_joined = prop_list.join(", ");
            let (message, code) = if all_missing.len() > 4 {
                let more_count = (all_missing.len() - 4).to_string();
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
                    .map(|name| self.missing_property_list_name_for_display(*name))
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
        let widened_source = self.widen_type_for_display(source_type);
        let (mut src_str, mut tgt_str_qualified) = if depth == 0 {
            let src = if source_type == TypeId::OBJECT {
                "{}".to_string()
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
            (
                src,
                self.format_type_for_diagnostic_role(
                    widened_target,
                    DiagnosticTypeDisplayRole::FlattenedDiagnostic,
                ),
            )
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
            } else {
                let fmt_src_bare = self.format_type_diagnostic(widened_source);
                let fmt_tgt_bare = self.format_type_diagnostic(target);
                if fmt_src_bare == fmt_tgt_bare {
                    let (_, db) = self.format_type_pair_diagnostic(widened_source, target);
                    if db != tgt_str_qualified {
                        tgt_str_qualified = db;
                    }
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
        let message = format_message(
            diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            &[&prop_name_display, &src_str, &tgt_str_qualified],
        );
        Diagnostic::error(
            file_name,
            start,
            length,
            message,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
        )
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
}
