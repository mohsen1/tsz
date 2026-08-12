//! Checker-owned type display roles for diagnostic rendering.
//!
//! This adapter keeps checker context out of the solver formatter while making
//! diagnostic display intent explicit at emission sites. Each role delegates to
//! the existing specialized helper for that surface.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticTypeDisplayRole {
    DefaultDiagnostic,
    WidenedDiagnostic,
    FlattenedDiagnostic,
    AssignmentSource {
        target: TypeId,
        anchor_idx: NodeIndex,
    },
    AssignmentTarget {
        source: TypeId,
        anchor_idx: NodeIndex,
    },
    CallArgument {
        parameter: TypeId,
        argument_idx: NodeIndex,
    },
    CallParameter {
        argument: TypeId,
        argument_idx: NodeIndex,
    },
    WeakCallParameter {
        argument: TypeId,
        argument_idx: NodeIndex,
    },
    PropertyReceiver,
}

impl<'a> CheckerState<'a> {
    /// tsc renders the resolved form (not the alias name) for non-generic
    /// type aliases whose body is a single indexed-access type that reduces
    /// to a concrete result. The classic case is
    /// `type WeakKey = WeakKeyTypes[keyof WeakKeyTypes]` — when `WeakKeyTypes`
    /// has only `object: object` (es2022 lib without es2023.collection.d.ts),
    /// the indexed-access reduction collapses to `object`. tsc loses the outer
    /// alias on the resolved type and displays `object`, not `WeakKey`.
    ///
    /// Pre-resolve the type before passing it to the formatter; this needs
    /// the checker's full evaluator (with `TypeEnvironment`) which the solver
    /// formatter cannot reach on its own.
    pub(in crate::error_reporter) fn resolve_indexed_access_alias_for_display(
        &mut self,
        ty: TypeId,
    ) -> TypeId {
        let body = match crate::query_boundaries::diagnostics::indexed_access_alias_body(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            ty,
        ) {
            Some(b) => b,
            None => return ty,
        };
        let resolved = self.evaluate_type_with_env(body);
        if resolved == body
            || crate::query_boundaries::diagnostics::is_unresolved_for_display(
                self.ctx.types.as_type_database(),
                resolved,
            )
        {
            return ty;
        }
        resolved
    }

    /// tsc resolves a *concrete* indexed-access type (`Obj["m"]` where the
    /// object type and key are fully resolved, with no free type parameters) to
    /// the member type during type construction (`getIndexedAccessType`), so it
    /// never renders the `Obj["m"]` form in a diagnostic — it shows the reduced
    /// member type. tsz keeps the indexed access deferred and rendered the
    /// unreduced `Obj["m"]` form (e.g. `Property 'bar' is missing in type
    /// 'Obj["m"]'` where tsc says `... in type '{ foo: string; }'`).
    ///
    /// Reduce it here for display, but only when concrete: an indexed access
    /// that carries a free type parameter is *legitimately* deferred (tsc keeps
    /// `T["m"]` too), and pre-resolving it risks `TS2589` on deeply-recursive
    /// generic forms — so those are left opaque. This mirrors the deliberate
    /// exclusion of [`Self::resolve_indexed_access_alias_for_display`] from the
    /// assignment roles, but is strictly narrower: only a bare,
    /// type-parameter-free indexed access with a literal key is touched.
    pub(in crate::error_reporter) fn resolve_concrete_indexed_access_for_display(
        &mut self,
        ty: TypeId,
    ) -> TypeId {
        use crate::query_boundaries::common;
        use crate::query_boundaries::diagnostics;
        let db = self.ctx.types.as_type_database();
        let Some(indexed) = common::get_indexed_access_type(db, ty) else {
            return ty;
        };
        // The key must be a shape tsc reduces eagerly (literal, union, unique
        // symbol, typeof query, the bare string/number primitive — the
        // array/tuple element idiom `Arr[number]` — or a `keyof` operator over a
        // concrete operand). A still-deferred key shape (type parameter,
        // conditional, another indexed access) keeps tsc's own indexed access
        // deferred too, and the free-type-parameter guard below rejects a
        // generic `keyof T`.
        //
        // `keyof` is admitted here rather than in the shared
        // `is_display_reducible_index_key` classifier for two reasons. First, it
        // confines the widening to the assignment-display gate and leaves the
        // solver's application-argument / index-object display paths (which share
        // the narrower classifier) untouched. Second, a reduced `keyof` access
        // can intern onto the very `TypeId` that a *sibling* expression already
        // stamped with a `display_alias` — `type Pair<T> = Pairs<T>[keyof T]`
        // reduces `Pairs<FooBar>[keyof FooBar]` onto the same union `Pair<FooBar>`
        // instantiates to, and the members can equally carry their *own* alias
        // (`Q[keyof Q]` where every member is `Partial<X>`). tsz's
        // `TypeId`-keyed `display_alias` table cannot tell tsc's per-reference
        // `aliasSymbol` apart from either, so the reduced result is only used
        // when it carries *no* alias at all (the `keyof`-only guard below).
        // Otherwise the access is left as written — tsc's safe fallback, matching
        // the residual policy #16461 / #16469 already established for the deferred
        // rows — rather than risk repainting it with an unrelated name.
        let index_is_keyof = common::is_keyof_type(db, indexed.index_type);
        if !diagnostics::is_display_reducible_index_key(db, indexed.index_type) && !index_is_keyof {
            return ty;
        }
        // A free type parameter anywhere in the object or index means the
        // access is legitimately deferred (tsc renders `T["m"]`); never
        // force-evaluate those. The index check matters now that the shape
        // gate above admits unions, which can still carry a type parameter
        // member (`K | "a"`).
        if common::contains_free_type_parameters(db, indexed.object_type)
            || common::contains_free_type_parameters(db, indexed.index_type)
        {
            return ty;
        }
        let resolved = self.evaluate_type_with_env(ty);
        if resolved == ty
            || resolved == tsz_solver::TypeId::ERROR
            || crate::query_boundaries::diagnostics::is_unresolved_for_display(
                self.ctx.types.as_type_database(),
                resolved,
            )
        {
            return ty;
        }
        // `keyof`-only conservative guard: only reduce when the result carries no
        // ambiguous `display_alias`. When it does (a sibling alias, or the
        // members' own), leave the access as written so the reduction never
        // repaints it with the wrong name and never pre-empts the target role's
        // union member-splitting path (which already renders these correctly).
        // The literal / union / primitive-key reductions are unaffected — they
        // keep reducing unconditionally, exactly as before.
        if index_is_keyof && self.ctx.types.get_display_alias(resolved).is_some() {
            return ty;
        }
        resolved
    }

    /// Widen literal annotations of `ty` for diagnostic display (#13075),
    /// using the type environment for display-time evaluation when it is
    /// available (so generic applications that evaluate to literals widen
    /// like the literals they render as).
    pub(crate) fn widen_annotation_literals_for_display(
        &self,
        ty: TypeId,
        policy: crate::query_boundaries::diagnostics::AnnotationLiteralWideningPolicy,
    ) -> crate::query_boundaries::diagnostics::AnnotationWideningOutcome {
        match self.ctx.type_env.try_borrow() {
            Ok(env) => {
                crate::query_boundaries::diagnostics::widen_object_property_literals_for_display_resolved(
                    self.ctx.types,
                    &*env,
                    ty,
                    policy,
                )
            }
            Err(_) => {
                crate::query_boundaries::diagnostics::widen_object_property_literals_for_display(
                    self.ctx.types,
                    ty,
                    policy,
                )
            }
        }
    }

    pub(in crate::error_reporter) fn format_type_for_diagnostic_role(
        &mut self,
        ty: TypeId,
        role: DiagnosticTypeDisplayRole,
    ) -> String {
        // Discarded-diagnostics children never surface their messages: skip
        // the role-specific display policy and the type formatter entirely.
        // The placeholder embeds the type id so different types still format
        // to different strings (message-hash dedup keys stay distinct).
        if self.ctx.diagnostics_discarded {
            return format!("[discarded #{}]", ty.0);
        }
        // One rendered type, one work budget (issue #13040).
        let _budget_scope = crate::error_reporter::display_budget::DisplayBudgetScope::enter();
        // Only apply the indexed-access alias resolution for roles where the
        // alias would otherwise leak through unresolved (call parameters /
        // arguments). For declaration-emit-adjacent roles, type-display roles,
        // and property-receiver displays, keep the original alias name —
        // pre-resolving there can trigger TS2589 on legitimately-deferred
        // indexed-access aliases that the checker intentionally leaves opaque.
        let ty = match role {
            DiagnosticTypeDisplayRole::CallArgument { .. }
            | DiagnosticTypeDisplayRole::CallParameter { .. }
            | DiagnosticTypeDisplayRole::WeakCallParameter { .. } => {
                self.resolve_indexed_access_alias_for_display(ty)
            }
            // A bare, concrete (type-parameter-free) indexed access is reduced
            // to its member type, matching tsc's eager `getIndexedAccessType`.
            // Generic/deferred accesses stay opaque (see the helper's docs).
            // `FlattenedDiagnostic` is the TS2741 single-missing-property
            // target: it is a distinct role from `AssignmentTarget`, but tsc
            // draws no such distinction — the target it quotes in "but
            // required in type '_'" is the same annotated type either
            // message reaches, so it needs the identical pre-resolution or a
            // concrete `keyof` access reaches this role's message still
            // unreduced (the shared formatter's own gate excludes `keyof`
            // unconditionally; see `is_display_reducible_index_key`).
            DiagnosticTypeDisplayRole::AssignmentSource { .. }
            | DiagnosticTypeDisplayRole::AssignmentTarget { .. }
            | DiagnosticTypeDisplayRole::FlattenedDiagnostic => {
                self.resolve_concrete_indexed_access_for_display(ty)
            }
            _ => ty,
        };
        // Recursive tuple aliases (e.g. `T2<U>` for
        // `type T2<T> = [42, T2<{ x: T }>]`) cannot be expanded structurally
        // without producing an unbounded tuple display. Preserve those alias
        // applications before the role-specific tuple fallbacks evaluate the
        // application into its self-referential body. Recursive object/mapped
        // aliases still flow through the normal role-specific formatters: those
        // paths can reduce property values through mapped type substitutions
        // where tsc displays the concrete value type instead of the alias.
        if matches!(
            role,
            DiagnosticTypeDisplayRole::AssignmentSource { .. }
                | DiagnosticTypeDisplayRole::AssignmentTarget { .. }
                | DiagnosticTypeDisplayRole::CallArgument { .. }
                | DiagnosticTypeDisplayRole::CallParameter { .. }
                | DiagnosticTypeDisplayRole::WeakCallParameter { .. }
        ) && self.is_recursive_tuple_alias_application_for_diagnostic(ty)
        {
            return self.format_type_diagnostic(ty);
        }
        if let Some(display) = self.format_type_for_basic_diagnostic_role(ty, role) {
            return display;
        }
        match role {
            DiagnosticTypeDisplayRole::DefaultDiagnostic
            | DiagnosticTypeDisplayRole::WidenedDiagnostic
            | DiagnosticTypeDisplayRole::FlattenedDiagnostic => {
                unreachable!("basic diagnostic roles are handled by the checker formatting factory")
            }
            DiagnosticTypeDisplayRole::AssignmentSource { target, anchor_idx } => {
                self.format_assignment_source_type_for_diagnostic(ty, target, anchor_idx)
            }
            DiagnosticTypeDisplayRole::AssignmentTarget { source, anchor_idx } => {
                self.format_assignment_target_type_for_diagnostic(ty, source, anchor_idx)
            }
            DiagnosticTypeDisplayRole::CallArgument {
                parameter,
                argument_idx,
            } => self.format_call_argument_type_for_diagnostic(ty, parameter, argument_idx),
            DiagnosticTypeDisplayRole::CallParameter {
                argument,
                argument_idx,
            } => {
                let display = self.format_call_parameter_type_for_diagnostic(
                    ty,
                    argument,
                    argument_idx,
                    true,
                );
                if crate::query_boundaries::common::array_element_type(self.ctx.types, ty).is_some()
                {
                    Self::normalize_array_generic_to_shorthand(&display)
                } else {
                    display
                }
            }
            DiagnosticTypeDisplayRole::WeakCallParameter {
                argument,
                argument_idx,
            } => {
                let display = self.format_call_parameter_type_for_diagnostic(
                    ty,
                    argument,
                    argument_idx,
                    false,
                );
                if crate::query_boundaries::common::array_element_type(self.ctx.types, ty).is_some()
                {
                    Self::normalize_array_generic_to_shorthand(&display)
                } else {
                    display
                }
            }
            DiagnosticTypeDisplayRole::PropertyReceiver => {
                self.format_property_receiver_type_for_diagnostic(ty)
            }
        }
    }

    fn is_recursive_tuple_alias_application_for_diagnostic(&mut self, ty: TypeId) -> bool {
        let evaluated = self.evaluate_type_with_env(ty);
        crate::query_boundaries::recursive_alias::is_recursive_tuple_type_alias_application(
            self.ctx.types,
            &self.ctx.definition_store,
            ty,
            evaluated,
        )
    }
}
