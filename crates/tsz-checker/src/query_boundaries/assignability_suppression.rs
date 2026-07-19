//! Centralized assignability-diagnostic suppression for `TS2322`/`TS2416`
//! family checks.
//!
//! Extracted verbatim from `assignability_checker.rs` (the "Diagnostic
//! Suppression" cluster) to keep that module under the 2000-LOC architecture
//! cap and route its solver-shape probes through the query-boundary tree;
//! behavior is unchanged. These are the `should_suppress_*` entry points plus
//! their private helpers (`type_contains_error_application`,
//! `callable_types_have_disjoint_type_parameters`,
//! `recursive_conditional_path_alias_mismatch_is_tsc_bailout`,
//! `is_parse_recovery_anchor_node`).

use crate::query_boundaries::assignability::{contains_free_infer_types, is_type_parameter_like};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Centralized suppression for TS2322-style assignability diagnostics.
    pub(crate) fn should_suppress_assignability_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let is_type_alias_application = |type_id: TypeId| {
            crate::query_boundaries::common::type_application(self.ctx.types, type_id)
                .and_then(|app| {
                    crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)
                })
                .and_then(|def_id| self.ctx.definition_store.get(def_id))
                .is_some_and(|def| def.kind == tsz_solver::def::DefKind::TypeAlias)
        };
        if is_type_alias_application(source)
            && is_type_alias_application(target)
            && crate::query_boundaries::assignability::are_types_structurally_identical(
                self.ctx.types,
                &self.ctx,
                source,
                target,
            )
        {
            return true;
        }
        if self.recursive_conditional_path_alias_mismatch_is_tsc_bailout(source, target) {
            return true;
        }

        if crate::query_boundaries::common::keyof_inner_type(self.ctx.types, target).is_some() {
            let resolved_keyof =
                crate::query_boundaries::state::type_environment::evaluate_type_with_resolver(
                    self.ctx.types,
                    &self.ctx,
                    target,
                );
            if resolved_keyof != target
                && self
                    .keyof_diagnostic_suppression_relation_outcome(source, resolved_keyof)
                    .related
            {
                return true;
            }
            if self.keyof_interface_augmentation_literals_cover_source(source, target) {
                return true;
            }
        }

        let evaluated_target_for_invalid_mapped = self.ctx.types.evaluate_type(target);
        if self.type_contains_invalid_mapped_key_type(target)
            || self.type_contains_invalid_mapped_key_type(evaluated_target_for_invalid_mapped)
        {
            return true;
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, target)
        {
            let has_indexed_access = members.iter().any(|&member| {
                crate::query_boundaries::common::is_index_access_type(self.ctx.types, member)
            });
            if has_indexed_access {
                let indexed_access_has_errors = members.iter().any(|&member| {
                    if crate::query_boundaries::common::is_index_access_type(self.ctx.types, member)
                    {
                        Self::type_contains_error_application(self.ctx.types, member)
                    } else {
                        false
                    }
                });
                let union_has_errors =
                    Self::type_contains_error_application(self.ctx.types, target);
                if !indexed_access_has_errors && !union_has_errors {
                    return false;
                }
            }
        }

        // Check if a type contains an error application (e.g., error<any>)
        // This happens when type resolution fails for qualified names like React.ReactElement
        // in function return type positions. Suppress the false positive TS2322.
        let contains_error_application =
            |type_id: TypeId| Self::type_contains_error_application(self.ctx.types, type_id);
        let evaluated_target_for_infer_suppression = self.ctx.types.evaluate_type(target);
        let target_is_conditional_for_infer_suppression =
            crate::query_boundaries::common::is_conditional_type(self.ctx.types, target)
                || crate::query_boundaries::common::is_conditional_type(
                    self.ctx.types,
                    evaluated_target_for_infer_suppression,
                );

        let callable_pair_has_opaque_return_mismatch =
            if crate::query_boundaries::assignability::callable_pair_contains_type_parameters(
                self.ctx.types,
                source,
                target,
            ) {
                let callable_return_type = |type_id: TypeId| -> Option<TypeId> {
                    if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                        self.ctx.types,
                        type_id,
                    ) {
                        return Some(shape.return_type);
                    }
                    if let Some(shape) = crate::query_boundaries::common::callable_shape_for_type(
                        self.ctx.types,
                        type_id,
                    ) {
                        return shape.call_signatures.last().map(|sig| sig.return_type);
                    }
                    if let Some(app) =
                        crate::query_boundaries::common::type_application(self.ctx.types, type_id)
                    {
                        if let Some(shape) =
                            crate::query_boundaries::common::function_shape_for_type(
                                self.ctx.types,
                                app.base,
                            )
                        {
                            return Some(shape.return_type);
                        }
                        if let Some(shape) =
                            crate::query_boundaries::common::callable_shape_for_type(
                                self.ctx.types,
                                app.base,
                            )
                        {
                            return shape.call_signatures.last().map(|sig| sig.return_type);
                        }
                    }
                    None
                };
                match (callable_return_type(source), callable_return_type(target)) {
                    (Some(source_return), Some(target_return)) => {
                        !self
                            .no_erase_generics_relation_outcome(source_return, target_return)
                            .related
                    }
                    _ => false,
                }
            } else {
                false
            };

        // Suppress TS2322 for callable types with generic type parameters from outer
        // context. Skip the suppression when both sides have their own signature-level
        // type params — the solver handles generic-to-generic comparison correctly.
        let is_callable_or_function = |type_id: TypeId| {
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
                .is_some()
                || crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
                    .is_some()
                || crate::query_boundaries::common::type_application(self.ctx.types, type_id)
                    .is_some_and(|app| {
                        crate::query_boundaries::common::callable_shape_for_type(
                            self.ctx.types,
                            app.base,
                        )
                        .is_some()
                            || crate::query_boundaries::common::function_shape_for_type(
                                self.ctx.types,
                                app.base,
                            )
                            .is_some()
                    })
        };

        let is_constructor_like = |type_id: TypeId| -> bool {
            if crate::query_boundaries::common::has_construct_signatures(self.ctx.types, type_id) {
                return true;
            }
            if let Some(shape) =
                crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
                && shape.is_constructor
            {
                return true;
            }
            if let Some(app) =
                crate::query_boundaries::common::type_application(self.ctx.types, type_id)
            {
                if crate::query_boundaries::common::has_construct_signatures(
                    self.ctx.types,
                    app.base,
                ) {
                    return true;
                }
                if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    app.base,
                ) && shape.is_constructor
                {
                    return true;
                }
            }
            false
        };

        let has_own_signature_type_params = |type_id: TypeId| -> bool {
            if let Some(shape) =
                crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, type_id)
            {
                return shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                    .any(|sig| !sig.type_params.is_empty());
            }
            if let Some(shape) =
                crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            {
                return !shape.type_params.is_empty();
            }
            false
        };

        let contains_type_parameters = |type_id: TypeId| {
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, type_id)
        };

        let is_structural_target_that_must_not_be_suppressed = |type_id: TypeId| {
            let has_structural_mismatch_shape = |candidate: TypeId| {
                crate::query_boundaries::assignability::has_deferred_conditional_member(
                    self.ctx.types,
                    candidate,
                ) || crate::query_boundaries::common::is_conditional_type(self.ctx.types, candidate)
                    || crate::query_boundaries::common::is_string_intrinsic_type(
                        self.ctx.types,
                        candidate,
                    )
                    || crate::query_boundaries::common::is_mapped_type(self.ctx.types, candidate)
                    // A deferred `keyof T` target is a structural key-space
                    // relation the solver decides directly (contravariantly:
                    // `keyof S <: keyof T` iff `T <: S`), and a concrete-key
                    // `keyof` target is already handled by the literal-membership
                    // suppression above. Either way it must not take the
                    // complex-generic suppression, which would hide a legitimate
                    // TS2322 (e.g. `keyof X` assigned to `keyof A` for distinct
                    // type parameters, which `tsc` reports).
                    || crate::query_boundaries::common::is_keyof_type(self.ctx.types, candidate)
                    || crate::query_boundaries::common::intersection_members(
                        self.ctx.types,
                        candidate,
                    )
                    .is_some()
            };

            let evaluated = self.ctx.types.evaluate_type(type_id);
            let application_evaluated =
                if crate::query_boundaries::state::type_environment::application_info(
                    self.ctx.types,
                    type_id,
                )
                .is_some()
                {
                    crate::query_boundaries::state::type_environment::evaluate_type_with_resolver(
                        self.ctx.types,
                        &self.ctx,
                        type_id,
                    )
                } else {
                    type_id
                };
            has_structural_mismatch_shape(type_id)
                || (evaluated != type_id && has_structural_mismatch_shape(evaluated))
                || (application_evaluated != type_id
                    && has_structural_mismatch_shape(application_evaluated))
        };

        // Suppress TS2322 for types that contain recursive constraints or error conditions
        // that would lead to false positive diagnostics. These include:
        // - Types with type parameters that might cause recursive constraint issues
        let should_suppress_for_complex_type = |type_id: TypeId| -> bool {
            if crate::query_boundaries::common::is_type_parameter(self.ctx.types, type_id)
                || is_callable_or_function(type_id)
                || is_structural_target_that_must_not_be_suppressed(type_id)
            {
                return false;
            }
            // Also check for union types containing indexed access types.
            // For example, `(S & State<T>)["a"] | undefined` is a union where
            // one member is an indexed access type. We should not suppress TS2322
            // for these cases because the indexed access may resolve to a type
            // that is not assignable from the source.
            //
            // However, if the indexed access types contain error applications
            // (e.g., when type resolution fails), we should still allow suppression
            // to avoid false positives on unresolved types.
            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, type_id)
            {
                if members.iter().any(|&member| {
                    crate::query_boundaries::common::is_type_parameter(self.ctx.types, member)
                }) {
                    return false;
                }

                let has_indexed_access = members.iter().any(|&member| {
                    crate::query_boundaries::common::is_index_access_type(self.ctx.types, member)
                });
                if has_indexed_access {
                    // Check if any indexed access type contains error applications
                    let indexed_access_has_errors = members.iter().any(|&member| {
                        if crate::query_boundaries::common::is_index_access_type(
                            self.ctx.types,
                            member,
                        ) {
                            Self::type_contains_error_application(self.ctx.types, member)
                        } else {
                            false
                        }
                    });
                    // Also check if the union itself contains error applications
                    let union_has_errors =
                        Self::type_contains_error_application(self.ctx.types, type_id);
                    // Only prevent suppression if there are indexed access types AND no errors
                    if !indexed_access_has_errors && !union_has_errors {
                        return false; // Don't suppress for unions containing indexed access types without errors
                    }
                }
            }
            // Keep the generic false-positive suppression for genuinely complex
            // generic shapes, but do not suppress plain `T`/`U` relations.
            // tsc reports TS2322 for distinct type parameters even when they
            // share the same constraint.
            crate::query_boundaries::assignability::has_recursive_type_parameter_constraint(
                self.ctx.types,
                type_id,
            ) || (crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                type_id,
            ) && !is_type_parameter_like(self.ctx.types, type_id))
        };

        // Check if both source and target are simple generic Applications with the same base.
        // In this case, don't suppress - let the variance check or structural comparison
        // handle it. This fixes cases like `Foo<T>` vs `Foo<U>` where T and U are different
        // unconstrained type parameters that should produce TS2322.
        let are_simple_generic_applications = |s: TypeId, t: TypeId| -> bool {
            if let (Some(s_app), Some(t_app)) = (
                crate::query_boundaries::common::type_application(self.ctx.types, s),
                crate::query_boundaries::common::type_application(self.ctx.types, t),
            ) {
                // Same base type, both contain type parameters
                return s_app.base == t_app.base
                    && crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        s,
                    )
                    && crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        t,
                    );
            }
            false
        };

        if are_simple_generic_applications(source, target) {
            return false; // Don't suppress - let the actual assignability check run
        }

        // Don't suppress for generic Applications with type parameters.
        // This fixes false TS2769 errors when passing generic return types
        // (e.g., IterableIterator<T> from values()) to overloads.
        let is_generic_application_with_type_params = |ty: TypeId| -> bool {
            if let Some(app) = crate::query_boundaries::common::type_application(self.ctx.types, ty)
                && app.args.iter().any(|&arg| {
                    crate::query_boundaries::common::contains_type_parameters(self.ctx.types, arg)
                })
            {
                return true;
            }
            false
        };

        // Check if target contains indexed access type - these should NOT be suppressed
        // even when source has type parameters, because indexed access may resolve
        // to incompatible types (e.g., (S & State<T>)["a"] may not accept T)
        let target_contains_indexed_access = || -> bool {
            if crate::query_boundaries::common::is_index_access_type(self.ctx.types, target) {
                return true;
            }
            // Check union members for indexed access types
            if let Some(members) =
                crate::query_boundaries::common::union_members(self.ctx.types, target)
            {
                return members.iter().any(|&member| {
                    crate::query_boundaries::common::is_index_access_type(self.ctx.types, member)
                });
            }
            false
        };

        // Check if target is an index signature type (e.g., { [s: string]: A })
        // These should prefer TS2741 for missing properties over TS2322 suppression
        let target_is_index_signature = || -> bool {
            if let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target)
            {
                return shape.string_index.is_some() || shape.number_index.is_some();
            }
            false
        };

        if is_generic_application_with_type_params(source)
            || is_generic_application_with_type_params(target)
        {
            return false; // Don't suppress - let the actual assignability check run
        }

        let is_type_param_spread_tuple = |ty: TypeId| {
            crate::query_boundaries::common::tuple_elements(self.ctx.types, ty).is_some_and(
                |elements| {
                    elements.iter().any(|element| {
                        element.rest
                            && crate::query_boundaries::common::type_param_info(
                                self.ctx.types,
                                element.type_id,
                            )
                            .is_some()
                    })
                },
            )
        };
        if is_type_param_spread_tuple(source) || is_type_param_spread_tuple(target) {
            return false;
        }

        let evaluated_source = self.ctx.types.evaluate_type(source);
        let evaluated_target = self.ctx.types.evaluate_type(target);
        if let (Some(source_elem), Some(target_elem)) = (
            crate::query_boundaries::common::array_element_type(self.ctx.types, evaluated_source),
            crate::query_boundaries::common::array_element_type(self.ctx.types, evaluated_target),
        ) && crate::query_boundaries::common::is_mapped_type(self.ctx.types, source_elem)
            && is_type_parameter_like(self.ctx.types, target_elem)
        {
            return false;
        }

        // Structural targets (mapped/intersection/conditional/string-intrinsic) require
        // property-level checking; they must not take the complex-generic suppression
        // early-exit below — the solver decides those relations directly.
        let target_is_structural = is_structural_target_that_must_not_be_suppressed(target);
        let target_is_template_literal_from_bare_type_param =
            crate::query_boundaries::common::is_template_literal_type(self.ctx.types, target)
                && crate::query_boundaries::common::is_type_parameter(self.ctx.types, source);
        let target_allows_complex_generic_suppression = !target_is_structural
            && should_suppress_for_complex_type(target)
            && contains_type_parameters(source)
            && !is_callable_or_function(target)
            && !target_contains_indexed_access()
            && !target_is_template_literal_from_bare_type_param;
        // A free declaration-scoped parameter is an ordinary value-level type
        // variable here, not a binder introduced by this relation. Distinct
        // declaration sets therefore remain unrelated even when alias
        // reduction leaves the same outer object shape. Generic callables are
        // already excluded above and establish their own alpha-equivalence
        // scope in the solver.
        let distinct_decl_scoped_free_params = target_allows_complex_generic_suppression
            && crate::query_boundaries::assignability::have_distinct_decl_scoped_free_type_parameters(
                self.ctx.types,
                source,
                target,
            );

        matches!(source, TypeId::ERROR)
            || matches!(target, TypeId::ERROR | TypeId::ANY)
            || contains_error_application(target)
            // any is assignable to everything except never — tsc reports TS2322 for any→never
            || (source == TypeId::ANY && target != TypeId::NEVER)
            // Inference placeholders are transient solver state. Emitting TS2322/TS2345
            // while they are still present creates contextual false positives.
            || contains_free_infer_types(self.ctx.types, self.ctx.types.evaluate_type(source))
            || (contains_free_infer_types(self.ctx.types, evaluated_target_for_infer_suppression)
                && !target_is_conditional_for_infer_suppression)
            // Suppress TS2322 for non-callable types with type parameters that may
            // cause false positives due to complex generic constraints
            // (e.g., T extends { [P in T]: number }). Callable/generic signature
            // targets have their own suppression rules below, and suppressing them
            // here hides real TS2322s like templateLiteralTypes7.
            // Also keep mainline behavior that only suppresses while the source is
            // still generic/unresolved too; once the source has reduced to a concrete
            // type, tsc surfaces the mismatch even if the target still mentions an
            // outer type parameter (for example Assign<T, U> receiving a concrete U).
            // EXCEPTION: Don't suppress when target contains indexed access types - these
            // may resolve to incompatible concrete types that should produce TS2322.
            // Don't suppress when target is a template-literal pattern and the
            // source is a bare type parameter. The pattern `${T}` is *not*
            // trivially assignable from a bare T: T's instantiation could be
            // a literal subtype ("a") that does not structurally match the
            // template's pattern. tsc emits TS2322 for these cases (see
            // templateLiteralTypes5.ts:14:11 — `const test1: \`${T3}\` = x`).
            // Restrict the carve-out to bare type-parameter sources so that
            // template-vs-template generic comparisons (e.g.
            // `\`...${Uppercase<T>}.4\`` vs `\`...${Uppercase<T>}.3\``) keep
            // their existing suppression — tsc tolerates those under generic
            // constraint relationships.
            || (target_allows_complex_generic_suppression && !distinct_decl_scoped_free_params)
            // Suppress TS2322 for callable types where the source contains generic type
            // parameters that may not have been fully inferred from context. When both
            // source and target contain type parameters that are COMPLETELY disjoint
            // at the signature level (e.g., () => T vs () => U from an outer `<T, U>`
            // scope), the incompatibility is real and must NOT be suppressed.
            // Skip when both sides have their own signature-level type parameters —
            // the solver handles generic-to-generic comparison correctly via alpha-renaming.
            // Also skip when only the source has type parameters and target is concrete —
            // this is a real mismatch (e.g., <T>(x: T) => T vs (x: string) => boolean).
            // Additionally skip when source has outer-context type params and target is concrete
            // (e.g., JSDoc @template types that should emit errors for concrete mismatches).
            || (!self.ctx.skip_callable_type_param_suppression.get()
                && is_callable_or_function(source)
                && is_callable_or_function(target)
                && contains_type_parameters(source)
                && !self.callable_types_have_disjoint_type_parameters(source, target)
                // A genuine return mismatch confirmed by the solver while holding
                // shared/outer type parameters opaque (no-erase-generics) must not be
                // suppressed. Keep this return-scoped: tsc still accepts generic rest
                // parameter comparisons whose return types agree, even when an opaque
                // whole-callable probe cannot relate the parameter tuples.
                && !callable_pair_has_opaque_return_mismatch
                && !(has_own_signature_type_params(source)
                    && has_own_signature_type_params(target))
                && !(has_own_signature_type_params(source)
                    && !has_own_signature_type_params(target)
                    && !contains_type_parameters(target))
                && !(!has_own_signature_type_params(source)
                    && contains_type_parameters(source)
                    && !contains_type_parameters(target))
                && !is_constructor_like(source)
                && !is_constructor_like(target)
                && !target_is_index_signature())
    }

    /// Targeted suppression for member type compatibility checks (TS2416/TS2430).
    ///
    /// Unlike `should_suppress_assignability_diagnostic`, this does NOT suppress
    /// callable types whose source contains type parameters from an outer context.
    /// For implements/extends member checking, class-level type parameters are fully
    /// declared and their constraints must be checked eagerly — suppressing them
    /// causes false negatives where incompatible member/property signatures are accepted.
    pub(crate) fn should_suppress_member_assignability(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let contains_error_application =
            |type_id: TypeId| Self::type_contains_error_application(self.ctx.types, type_id);

        matches!(source, TypeId::ERROR)
            || matches!(target, TypeId::ERROR | TypeId::ANY)
            || contains_error_application(target)
            || (source == TypeId::ANY && target != TypeId::NEVER)
            || contains_free_infer_types(self.ctx.types, self.ctx.types.evaluate_type(source))
            || contains_free_infer_types(self.ctx.types, self.ctx.types.evaluate_type(target))
    }

    /// Check if two callable types have completely disjoint outer type parameters
    /// at their immediate signature level (parameters and return type only).
    ///
    /// Returns true when both source and target function shapes directly reference
    /// type parameters in their parameter/return positions and those type parameters
    /// are entirely different. This is a conservative check that only looks at the
    /// shallow signature level to avoid false positives from type parameters buried
    /// in generic utility types.
    fn callable_types_have_disjoint_type_parameters(&self, source: TypeId, target: TypeId) -> bool {
        let get_direct_type_params = |type_id: TypeId| -> Vec<TypeId> {
            let mut params = Vec::new();
            let mut current = type_id;
            // Walk through nested function return types to find type parameters
            // at any depth (e.g., () => (item: any) => T has T in the nested return)
            for _ in 0..4 {
                if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    current,
                ) {
                    for p in &shape.params {
                        if crate::query_boundaries::common::is_type_parameter(
                            self.ctx.types,
                            p.type_id,
                        ) {
                            params.push(p.type_id);
                        }
                    }
                    if crate::query_boundaries::common::is_type_parameter(
                        self.ctx.types,
                        shape.return_type,
                    ) {
                        params.push(shape.return_type);
                        break;
                    }
                    // If return type is another function, recurse into it
                    current = shape.return_type;
                } else {
                    break;
                }
            }
            params
        };

        let source_params = get_direct_type_params(source);
        let target_params = get_direct_type_params(target);

        // Both must have direct type params for them to be disjoint
        if source_params.is_empty() || target_params.is_empty() {
            return false;
        }

        // Disjoint = no overlap at all
        !source_params.iter().any(|s| target_params.contains(s))
    }

    /// Check if a type contains an error application (recursively).
    fn type_contains_error_application(
        db: &dyn tsz_solver::construction::TypeDatabase,
        type_id: TypeId,
    ) -> bool {
        // Check if it's a direct error application
        if let Some(app) = crate::query_boundaries::common::type_application(db, type_id)
            && app.base == TypeId::ERROR
        {
            return true;
        }

        // Check if it's a union type containing an error application
        if let Some(members) = crate::query_boundaries::common::union_members(db, type_id) {
            for member in members {
                if Self::type_contains_error_application(db, member) {
                    return true;
                }
            }
        }

        // Check if it's an intersection type containing an error application
        if let Some(members) = crate::query_boundaries::common::intersection_members(db, type_id) {
            for member in members {
                if Self::type_contains_error_application(db, member) {
                    return true;
                }
            }
        }

        // Check if it's a function type with error return
        if let Some(fn_shape) =
            crate::query_boundaries::common::function_shape_for_type(db, type_id)
            && Self::type_contains_error_application(db, fn_shape.return_type)
        {
            return true;
        }

        // Check if it's a callable type with error return
        if let Some(callable) =
            crate::query_boundaries::common::callable_shape_for_type(db, type_id)
        {
            for sig in &callable.call_signatures {
                if Self::type_contains_error_application(db, sig.return_type) {
                    return true;
                }
            }
        }

        false
    }

    fn recursive_conditional_path_alias_mismatch_is_tsc_bailout(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some((source_base, source_args)) = self.application_info_or_display_alias(source)
        else {
            return false;
        };
        let Some((target_base, target_args)) = self.application_info_or_display_alias(target)
        else {
            return false;
        };
        if source_base != target_base
            || source_args.len() != target_args.len()
            || source_args == target_args
            || !self.ctx.types.is_conditional_alias_base(source_base)
        {
            return false;
        }
        source_args
            .iter()
            .zip(target_args.iter())
            .any(|(&source_arg, &target_arg)| {
                source_arg == target_arg
                    && crate::query_boundaries::common::string_literal_value(
                        self.ctx.types,
                        source_arg,
                    )
                    .is_some_and(|atom| {
                        let path = self.ctx.types.resolve_atom_ref(atom);
                        // Each dot is one recursive nesting level in a
                        // path-splitting conditional alias.  tsc's
                        // `getRecursionIdentity` mechanism assumes compatible
                        // (`Ternary.Maybe`) at depth ≥ 4 path segments (3 dots).
                        // Suppress only when the path is deep enough for that
                        // bailout; shallower paths reach the leaf and must
                        // produce TS2322 on a genuine mismatch.
                        path.chars().filter(|&c| c == '.').count() >= 3
                    })
            })
    }

    /// Suppress assignability diagnostics for parser-recovery artifacts.
    pub(crate) fn should_suppress_assignability_for_parse_recovery(
        &self,
        source_idx: NodeIndex,
        diag_idx: NodeIndex,
    ) -> bool {
        if !self.has_syntax_parse_errors() {
            return false;
        }

        if self.ctx.syntax_parse_error_positions.is_empty() {
            return false;
        }

        self.is_parse_recovery_anchor_node(source_idx)
            || self.is_parse_recovery_anchor_node(diag_idx)
    }

    /// Detect nodes that look like parser-recovery artifacts (empty text, near errors).
    fn is_parse_recovery_anchor_node(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        // Missing-expression placeholders used by parser recovery.
        if self
            .ctx
            .arena
            .get_identifier_text(idx)
            .is_some_and(str::is_empty)
        {
            return true;
        }

        // Also suppress diagnostics anchored very near a syntax parse error.
        const DIAG_PARSE_DISTANCE: u32 = 16;
        for &err_pos in &self.ctx.syntax_parse_error_positions {
            let before = err_pos.saturating_sub(DIAG_PARSE_DISTANCE);
            let after = err_pos.saturating_add(DIAG_PARSE_DISTANCE);
            if (node.pos >= before && node.pos <= after)
                || (node.end >= before && node.end <= after)
            {
                return true;
            }
        }

        let mut current = idx;
        let mut walk_guard = 0;
        while current.is_some() {
            walk_guard += 1;
            if walk_guard > 512 {
                break;
            }

            if let Some(current_node) = self.ctx.arena.get(current) {
                if current_node.this_node_has_error() || current_node.this_or_subtree_has_error() {
                    return true;
                }
            } else {
                break;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        false
    }
}
