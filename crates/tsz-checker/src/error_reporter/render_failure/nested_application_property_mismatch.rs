use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages, format_message};
use crate::error_reporter::render_failure::RenderContext;
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn application_base_for_property_mismatch_display(&self, type_id: TypeId) -> Option<TypeId> {
        // Resolve to the application form, either directly or through the type's
        // display alias (the structural value may carry an `Id<…>` alias).
        let (app_type, base) = if let Some((base, _)) =
            crate::query_boundaries::diagnostics::application_info(self.ctx.types, type_id)
        {
            (type_id, base)
        } else {
            let alias = self.ctx.types.get_display_alias(type_id)?;
            let (base, _) =
                crate::query_boundaries::diagnostics::application_info(self.ctx.types, alias)?;
            (alias, base)
        };
        // Homomorphic/structural mapped-type aliases (`Partial<X>`, `Readonly<X>`,
        // a user `type F<T> = { [K in keyof T]… }`, or recursive ones like
        // `type Id<T> = { [K in keyof T]: Id<T[K]> }`) are NOT nominal generic
        // references. tsc elaborates their mismatches structurally — drilling
        // into `Types of property 'p'` / `The types of 'a.b'` chains — rather
        // than collapsing to a single covariant type-argument line. Excluding
        // them here routes the four call sites that gate the type-argument fast
        // path into the structural property-chain elaboration instead.
        if crate::query_boundaries::diagnostics::application_base_is_mapped_type(
            self.ctx.types,
            &self.ctx,
            app_type,
        ) {
            return None;
        }
        Some(base)
    }

    fn should_render_nested_application_property_mismatch(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(source_base) = self.application_base_for_property_mismatch_display(source) else {
            return false;
        };
        let Some(target_base) = self.application_base_for_property_mismatch_display(target) else {
            return false;
        };
        source_base == target_base
    }

    fn is_typed_array_application_property_mismatch_display(&self, type_id: TypeId) -> bool {
        let Some(base) = self.application_base_for_property_mismatch_display(type_id) else {
            return false;
        };
        crate::query_boundaries::definition_identity::type_has_well_known_typed_array_name(
            self.ctx.types,
            &self.ctx.definition_store,
            base,
        )
    }

    fn nested_reason_reuses_enclosing_application_source(
        &self,
        nested_source: TypeId,
        enclosing_source: TypeId,
    ) -> bool {
        let Some(nested_base) = self.application_base_for_property_mismatch_display(nested_source)
        else {
            return false;
        };
        let Some(enclosing_base) =
            self.application_base_for_property_mismatch_display(enclosing_source)
        else {
            return false;
        };
        nested_base == enclosing_base
    }

    fn same_generic_mismatch_keeps_application_top_level(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        fn structural_display_type(
            db: &dyn tsz_solver::construction::TypeDatabase,
            ty: TypeId,
        ) -> bool {
            crate::query_boundaries::dispatch::is_object_like_type(db, ty)
                || crate::query_boundaries::dispatch::is_callable_type(db, ty)
                // A UNION-bodied generic alias (`type ValueOrArray<E> =
                // E | ValueOrArray<E>[]`, `type Maybe<T> = T | undefined`)
                // instantiates to a union; tsc keeps BOTH alias applications
                // at the top of the TS2322 (`'ValueOrArray<string>' is not
                // assignable to 'ValueOrArray<number>'`) exactly as for
                // object/callable instantiations. Without this arm the
                // renderer collapsed the target to the failing type ARGUMENT
                // (`'number'`). Transparent aliases (`type Id<T> = T`) never
                // reach this predicate (their evaluation is not an
                // application mismatch), so bare-argument display for those
                // is unaffected.
                || crate::query_boundaries::diagnostics::is_union_type(db, ty)
        }

        let source_eval = self.evaluate_type_for_assignability(source);
        let target_eval = self.evaluate_type_for_assignability(target);
        // A TRANSPARENT alias chain (`type Wrap<V> = Inner<V>; type Inner<V> =
        // V`) evaluates to its own type ARGUMENT; tsc collapses the display to
        // the underlying type (`'1 | 2' is not assignable to '1'`), so the
        // union arm below must not keep such applications at the head merely
        // because the argument happens to be a union.
        let transparent =
            |db: &dyn tsz_solver::construction::TypeDatabase, app: TypeId, eval: TypeId| {
                crate::query_boundaries::diagnostics::application_info(db, app)
                    .is_some_and(|(_, args)| args.contains(&eval))
            };
        if transparent(self.ctx.types, source, source_eval)
            && transparent(self.ctx.types, target, target_eval)
        {
            return false;
        }
        structural_display_type(self.ctx.types, source_eval)
            || structural_display_type(self.ctx.types, target_eval)
    }

    pub(super) const fn nested_reason_is_plain_type_mismatch(
        reason: &tsz_solver::SubtypeFailureReason,
    ) -> bool {
        matches!(
            reason,
            tsz_solver::SubtypeFailureReason::TypeMismatch { .. }
                | tsz_solver::SubtypeFailureReason::IntrinsicTypeMismatch { .. }
                | tsz_solver::SubtypeFailureReason::LiteralTypeMismatch { .. }
                | tsz_solver::SubtypeFailureReason::ErrorType { .. }
        )
    }

    /// Whether a property's nested failure reason needs the explicit
    /// `Type 'S' is not assignable to type 'T'.` relation frame over the
    /// declared property-type pair before it drills.
    ///
    /// tsc's chain under `Types of property 'p' are incompatible.` always
    /// begins with the property-pair relation line; the header-led structural
    /// reasons (tuple element/arity, index-signature) lead with their own
    /// specialized line instead, so the frame must be supplied here — the
    /// same split [`Self::union_member_nested_needs_header`] applies for
    /// union-member frames. Plain leaves double as the pair line themselves,
    /// union reasons self-head with the pair, a nested property link is
    /// path-compressed, and a member return failure collapses to the TS2201
    /// form with no pair frame, so all of those stay outside this set.
    const fn property_pair_frame_needed(reason: &tsz_solver::SubtypeFailureReason) -> bool {
        matches!(
            reason,
            tsz_solver::SubtypeFailureReason::TupleElementTypeMismatch { .. }
                | tsz_solver::SubtypeFailureReason::TupleVariadicPositionMismatch { .. }
                | tsz_solver::SubtypeFailureReason::TupleElementMismatch { .. }
                | tsz_solver::SubtypeFailureReason::TupleArityMismatch(_)
                | tsz_solver::SubtypeFailureReason::IndexSignatureMismatch { .. }
        )
    }

    pub(super) const fn nested_failure_display_types(
        reason: &tsz_solver::SubtypeFailureReason,
        fallback_source: TypeId,
        fallback_target: TypeId,
    ) -> (TypeId, TypeId) {
        match reason {
            tsz_solver::SubtypeFailureReason::MissingProperty {
                source_type,
                target_type,
                ..
            }
            | tsz_solver::SubtypeFailureReason::MissingProperties {
                source_type,
                target_type,
                ..
            }
            | tsz_solver::SubtypeFailureReason::TypeMismatch {
                source_type,
                target_type,
            }
            | tsz_solver::SubtypeFailureReason::IntrinsicTypeMismatch {
                source_type,
                target_type,
            }
            | tsz_solver::SubtypeFailureReason::LiteralTypeMismatch {
                source_type,
                target_type,
            }
            | tsz_solver::SubtypeFailureReason::ErrorType {
                source_type,
                target_type,
            }
            | tsz_solver::SubtypeFailureReason::UnionSourceMismatch {
                source_type,
                target_type,
                ..
            } => (*source_type, *target_type),
            tsz_solver::SubtypeFailureReason::ReturnTypeMismatch {
                source_return,
                target_return,
                ..
            } => (*source_return, *target_return),
            tsz_solver::SubtypeFailureReason::TupleElementTypeMismatch {
                source_element,
                target_element,
                ..
            }
            | tsz_solver::SubtypeFailureReason::TupleVariadicPositionMismatch {
                source_element,
                target_element,
                ..
            } => (*source_element, *target_element),
            // `ArrayElementMismatch` self-heads with the *array* types (`se[]`
            // vs `te[]`), not its element types, so the nested render must keep
            // the parent-supplied array types rather than drilling to the
            // element pair (which would render `Type 'number' …'string'` in
            // place of `Type 'number[]' …'string[]'`). Keep the fallback, which
            // carries the array types. Explicit (not an implicit `_` gap) so it
            // is not mistakenly "fixed" to mirror the tuple arm above.
            tsz_solver::SubtypeFailureReason::ArrayElementMismatch { .. } => {
                (fallback_source, fallback_target)
            }
            tsz_solver::SubtypeFailureReason::IndexSignatureMismatch {
                source_value_type,
                target_value_type,
                ..
            } => (*source_value_type, *target_value_type),
            _ => (fallback_source, fallback_target),
        }
    }

    /// Fold a run of consecutive plain object-property mismatches into a single
    /// dotted property path, mirroring `tsc`'s
    /// `The types of 'a.b.c' are incompatible between these types.` collapse.
    ///
    /// Walking stops at the first link that is not a plain `PropertyTypeMismatch`
    /// or that is a same-base generic application property mismatch (which has
    /// its own dedicated elaboration), so only homogeneous object-property
    /// chains are folded. Returns the accumulated property-name path, the leaf
    /// reason that terminates the chain (if any), and the deepest property's
    /// source/target display types.
    fn peel_plain_property_chain<'r>(
        &self,
        first_name: tsz_common::interner::Atom,
        first_src: TypeId,
        first_tgt: TypeId,
        first_nested: Option<&'r tsz_solver::SubtypeFailureReason>,
    ) -> (
        Vec<std::sync::Arc<str>>,
        Option<&'r tsz_solver::SubtypeFailureReason>,
        TypeId,
        TypeId,
    ) {
        use crate::query_boundaries::diagnostics::SubtypeFailureReason as R;
        let mut names = vec![self.ctx.types.resolve_atom_ref(first_name)];
        let mut cur_src = first_src;
        let mut cur_tgt = first_tgt;
        let mut nested = first_nested;
        loop {
            // Only fold a property whose value types are plain — i.e. neither
            // side is a generic application. tsc keeps a `Box<string>` vs
            // `Box<number>` boundary visible as its own relation line rather
            // than folding the property into the dotted path, so the path must
            // stop at (and not absorb) such a link.
            if self
                .application_base_for_property_mismatch_display(cur_src)
                .is_some()
                || self
                    .application_base_for_property_mismatch_display(cur_tgt)
                    .is_some()
            {
                break;
            }
            match nested {
                Some(R::PropertyTypeMismatch {
                    property_name,
                    source_property_type,
                    target_property_type,
                    nested_reason,
                }) => {
                    names.push(self.ctx.types.resolve_atom_ref(*property_name));
                    cur_src = *source_property_type;
                    cur_tgt = *target_property_type;
                    nested = nested_reason.as_deref();
                }
                _ => break,
            }
        }
        (names, nested, cur_src, cur_tgt)
    }

    /// Append the leaf relation line beneath a collapsed property-path header at
    /// the given elaboration `depth`. Uses the structured leaf reason when
    /// present so intrinsic/literal display stays accurate; otherwise renders a
    /// direct `Type 'S' is not assignable to type 'T'.` line for the deepest
    /// property's types.
    pub(super) fn push_property_chain_leaf(
        &mut self,
        diag: &mut Diagnostic,
        leaf: Option<&tsz_solver::SubtypeFailureReason>,
        leaf_src: TypeId,
        leaf_tgt: TypeId,
        idx: tsz_parser::parser::NodeIndex,
        depth: u32,
    ) {
        if let Some(leaf) = leaf {
            // A header-led leaf (tuple element/arity, index-signature) does
            // not begin with the deepest property pair's relation line, so
            // supply it before drilling — tsc renders
            // `The types of 'a.b' are incompatible between these types.` ->
            // `Type '[boolean]' is not assignable to type '[string]'.` ->
            // the specialized drill.
            if Self::property_pair_frame_needed(leaf) {
                let message = self.element_mismatch_message(leaf_src, leaf_tgt);
                diag.push_elaboration(
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    depth,
                );
                let (s, t) = Self::nested_failure_display_types(leaf, leaf_src, leaf_tgt);
                let leaf_diag = self.render_failure_reason(leaf, s, t, idx, depth + 1);
                Self::push_nested_chain(diag, leaf_diag, depth + 1);
                return;
            }
            let (s, t) = Self::nested_failure_display_types(leaf, leaf_src, leaf_tgt);
            let leaf_diag = self.render_failure_reason(leaf, s, t, idx, depth);
            Self::push_nested_chain(diag, leaf_diag, depth);
        } else {
            let message = self.element_mismatch_message(leaf_src, leaf_tgt);
            diag.push_elaboration(
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                depth,
            );
        }
    }

    pub(super) fn render_property_type_mismatch(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        ctx: &RenderContext,
        property_name: tsz_common::interner::Atom,
        source_property_type: TypeId,
        target_property_type: TypeId,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> Diagnostic {
        let source = ctx.source;
        let target = ctx.target;
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();
        let target_property_type = if self.should_strip_nullish_for_property_display(target) {
            self.strip_nullish_for_assignability_display(target_property_type, source_property_type)
                .unwrap_or(target_property_type)
        } else {
            target_property_type
        };

        // A member whose two call signatures differ only in their RETURN type is
        // elaborated by tsc as the single TS2201 frame `The types returned by
        // '<name>(...)' are incompatible between these types.`, drilling straight
        // into the return relation and never using the historical `Return type
        // 'X' is not assignable to 'Y'.` phrasing (tsc emits zero such lines).
        // tsc collapses this way for BOTH method syntax (`f(): T`) and
        // function-typed-property syntax (`f: () => T`) — the member relation
        // reduces to a call-signature comparison in either case
        // (`reportIncompatibleCallSignatureReturn`). Route both to the dedicated
        // renderer. Same-generic applications keep their type-argument
        // elaboration, so this only fires for the plain structural-member surface.
        if let Some(tsz_solver::SubtypeFailureReason::ReturnTypeMismatch {
            source_return,
            target_return,
            nested_reason: return_inner,
        }) = nested_reason
            && !self.should_render_nested_application_property_mismatch(source, target)
        {
            return self.render_member_return_type_mismatch(
                ctx,
                property_name,
                (source_property_type, target_property_type),
                (*source_return, *target_return, return_inner.as_deref()),
            );
        }

        if depth == 0 {
            let (mut source_str, target_str) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
            // A fresh object-literal source failing a UNION target (the
            // fresh-literal fold's domain) renders its head role-based, like
            // the plain type-mismatch head: each property keeps its literal
            // exactly when the contextual (target) property type carries a
            // literal of the same primitive base, and widens otherwise (tsc
            // renders the checked fresh type, so `{ key: "foo", value: 3 }`
            // against `{ key: "foo"; value: string; } | …` shows
            // `{ key: "foo"; value: number; }`). The raw type format would
            // leak every display-property literal verbatim. Non-union targets
            // keep the existing head pipeline: its literal-surface rewrite
            // already preserves correctly there, and the role-based formatter
            // over-widens intersection-wrapped and satisfies-carried sources
            // (errorMessagesIntersectionTypes02,
            // typeSatisfaction_vacuousIntersectionOfContextualTypes).
            let evaluated_target = self.evaluate_type_for_assignability(target);
            if self.ctx.types.get_display_properties(source).is_some()
                && crate::query_boundaries::diagnostics::union_members(
                    self.ctx.types,
                    evaluated_target,
                )
                .is_some()
            {
                source_str = self.format_type_for_diagnostic_role(
                    source,
                    crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                );
            }
            let outer_is_structural = {
                let eval_source = self.evaluate_type_for_assignability(source);
                let eval_target = self.evaluate_type_for_assignability(target);
                crate::query_boundaries::diagnostics::object_shape_for_type(
                    self.ctx.types,
                    eval_source,
                )
                .is_some()
                    || crate::query_boundaries::diagnostics::object_shape_for_type(
                        self.ctx.types,
                        eval_target,
                    )
                    .is_some()
            };
            if !outer_is_structural
                && let Some(tsz_solver::SubtypeFailureReason::LiteralTypeMismatch { .. }) =
                    nested_reason
                && !(self.is_typed_array_application_property_mismatch_display(source)
                    && self.is_typed_array_application_property_mismatch_display(target))
            {
                return self.render_failure_reason(
                    nested_reason.expect("checked above"),
                    source_property_type,
                    target_property_type,
                    idx,
                    depth,
                );
            }
            let base = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            if self.should_render_nested_application_property_mismatch(source, target)
                && let Some(nested) = nested_reason
            {
                let (nested_source, nested_target) = Self::nested_failure_display_types(
                    nested,
                    source_property_type,
                    target_property_type,
                );
                if Self::nested_reason_is_plain_type_mismatch(nested) {
                    // When source and target are both applications of the same generic
                    // (e.g. `Box<number>` vs `Box<string>`), tsc elaborates via
                    // type-argument comparison rather than structural property traversal.
                    // It emits the outer mismatch followed directly by the inner
                    // type-argument mismatch — no intermediate
                    // "Types of property 'P' are incompatible." line.
                    let nested_diag = self.render_failure_reason(
                        nested,
                        nested_source,
                        nested_target,
                        idx,
                        depth + 1,
                    );
                    let mut diag = Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                    Self::push_nested_chain(&mut diag, nested_diag, depth + 1);
                    return diag;
                }
                if self.nested_reason_reuses_enclosing_application_source(nested_source, source) {
                    let prop_name = self.ctx.types.resolve_atom_ref(property_name);
                    let detail = format_message(
                        diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                        &[&prop_name],
                    );
                    let mut diag = Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                    diag.push_elaboration_in_span(
                        start,
                        length,
                        detail,
                        reason.diagnostic_code(),
                        0,
                    );
                    return diag;
                }
                let nested_diag = self.render_failure_reason(
                    nested,
                    nested_source,
                    nested_target,
                    idx,
                    depth + 1,
                );
                let mut diag = Diagnostic::error(
                    file_name,
                    start,
                    length,
                    base,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
                Self::push_nested_chain(&mut diag, nested_diag, depth + 1);
                return diag;
            }

            // Plain object-property chain. tsc collapses a run of >= 2
            // consecutive property links into a single
            // `The types of 'a.b.c' are incompatible between these types.` line,
            // then renders the leaf relation one level deeper. A single property
            // link keeps the `Types of property 'X' are incompatible.` form
            // handled below.
            let (path, leaf, leaf_src, leaf_tgt) = self.peel_plain_property_chain(
                property_name,
                source_property_type,
                target_property_type,
                nested_reason,
            );
            // A union-source leaf still terminates a collapsible property run:
            // tsc folds `o.a` into one line and renders the union/member chain
            // beneath it. It is not "plain" for the type-argument fast path, so
            // keep that predicate separate here. Header-led leaves (tuple,
            // index-signature) also fold — tsc renders the dotted path, then
            // the deepest pair's relation line, then the specialized drill
            // (`push_property_chain_leaf` supplies that pair frame).
            let leaf_is_collapsible = leaf.is_none_or(|reason| {
                Self::nested_reason_is_plain_type_mismatch(reason)
                    || Self::property_pair_frame_needed(reason)
                    || matches!(
                        reason,
                        tsz_solver::SubtypeFailureReason::UnionSourceMismatch { .. }
                    )
            });
            if path.len() >= 2 && leaf_is_collapsible {
                let dotted = path.join(".");
                let detail = format_message(
                    diagnostic_messages::THE_TYPES_OF_ARE_INCOMPATIBLE_BETWEEN_THESE_TYPES,
                    &[&dotted],
                );
                let mut diag = Diagnostic::error(
                    file_name,
                    start,
                    length,
                    base,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
                diag.push_elaboration_in_span(
                    start,
                    length,
                    detail,
                    diagnostic_codes::THE_TYPES_OF_ARE_INCOMPATIBLE_BETWEEN_THESE_TYPES,
                    0,
                );
                self.push_property_chain_leaf(&mut diag, leaf, leaf_src, leaf_tgt, idx, 1);
                return diag;
            }

            let prop_name = self.ctx.types.resolve_atom_ref(property_name);
            let detail = format_message(
                diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                &[&prop_name],
            );
            let mut diag = Diagnostic::error(
                file_name,
                start,
                length,
                base,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            diag.push_elaboration_in_span(start, length, detail, reason.diagnostic_code(), 0);
            // A property source that defers to its base constraint (a deferred
            // indexed access `T[K]`, a bare `keyof T`, or a conditional) keeps
            // the written operand and the *full* nullable-union target at the
            // leaf pair: tsc renders the as-written relation (`TBox[KKey]` vs
            // `string | undefined`) and then walks the constraint, never the
            // best-matching-member collapse (`... vs string`) that the solver's
            // evaluated nested reason carries. Emit the raw pair and stop — the
            // deeper constraint walk is separate elaboration tsz does not
            // synthesize, and emitting the collapsed member here would be wrong.
            if crate::query_boundaries::common::is_deferred_constraint_relative_operand(
                self.ctx.types.as_type_database(),
                &self.ctx.definition_store,
                source_property_type,
            ) {
                let source_str = self.format_type_for_assignability_message(source_property_type);
                let target_str = self.format_type_for_assignability_message(target_property_type);
                let leaf = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                diag.push_elaboration(
                    leaf,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    depth + 1,
                );
                return diag;
            }
            if let Some(nested) = nested_reason {
                // A header-led nested reason (tuple element/arity,
                // index-signature) leads with its specialized line, not the
                // property-pair relation line tsc keeps beneath the property
                // header — supply the pair frame, then drill one deeper.
                if Self::property_pair_frame_needed(nested) {
                    let frame =
                        self.element_mismatch_message(source_property_type, target_property_type);
                    diag.push_elaboration(
                        frame,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        depth + 1,
                    );
                    let (nested_source, nested_target) = Self::nested_failure_display_types(
                        nested,
                        source_property_type,
                        target_property_type,
                    );
                    let nested_diag = self.render_failure_reason(
                        nested,
                        nested_source,
                        nested_target,
                        idx,
                        depth + 2,
                    );
                    Self::push_nested_chain(&mut diag, nested_diag, depth + 2);
                    return diag;
                }
                let (nested_source, nested_target) = Self::nested_failure_display_types(
                    nested,
                    source_property_type,
                    target_property_type,
                );
                if !self.nested_reason_reuses_enclosing_application_source(nested_source, source) {
                    let nested_diag = self.render_failure_reason(
                        nested,
                        nested_source,
                        nested_target,
                        idx,
                        depth + 1,
                    );
                    Self::push_nested_chain(&mut diag, nested_diag, depth + 1);
                }
            }
            return diag;
        }

        let prop_name = self.ctx.types.resolve_atom_ref(property_name);
        let message = format_message(
            diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
            &[&prop_name],
        );
        let mut diag =
            Diagnostic::error(file_name, start, length, message, reason.diagnostic_code());

        if let Some(nested) = nested_reason
            && depth < super::PROPERTY_MISMATCH_RENDER_DEPTH_CAP
        {
            // Same pair-frame rule as the top-level path above: a header-led
            // nested reason drills beneath the explicit property-pair line.
            if Self::property_pair_frame_needed(nested) {
                let frame =
                    self.element_mismatch_message(source_property_type, target_property_type);
                diag.push_elaboration(
                    frame,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    depth + 1,
                );
                let (nested_source, nested_target) = Self::nested_failure_display_types(
                    nested,
                    source_property_type,
                    target_property_type,
                );
                let nested_diag = self.render_failure_reason(
                    nested,
                    nested_source,
                    nested_target,
                    idx,
                    depth + 2,
                );
                Self::push_nested_chain(&mut diag, nested_diag, depth + 2);
                return diag;
            }
            let (nested_source, nested_target) = Self::nested_failure_display_types(
                nested,
                source_property_type,
                target_property_type,
            );
            let nested_diag =
                self.render_failure_reason(nested, nested_source, nested_target, idx, depth + 1);
            Self::push_nested_chain(&mut diag, nested_diag, depth + 1);
        }
        diag
    }

    /// Render a member whose two call signatures differ only in their RETURN
    /// type.
    ///
    /// tsc collapses this to a single TS2201 frame and never uses tsz's
    /// historical `Return type 'X' is not assignable to 'Y'.` phrasing. The same
    /// shape serves method syntax (`f(): T`) and function-typed-property syntax
    /// (`f: () => T`) — both reduce to a call-signature comparison:
    /// ```text
    /// Type 'A' is not assignable to type 'B'.
    ///   The types returned by 'f()' are incompatible between these types.
    ///     Type 'string' is not assignable to type 'number'.
    /// ```
    ///
    /// The name suffix is `()` only when both signatures take zero parameters,
    /// otherwise `(...)` — mirroring tsc's `reportIncompatibleCallSignatureReturn`
    /// (`Call_signatures_with_no_arguments_have_incompatible_return_types` when
    /// both parameter lists are empty, `Call_signature_return_types_are_incompatible`
    /// otherwise).
    pub(super) fn render_member_return_type_mismatch(
        &mut self,
        ctx: &RenderContext,
        property_name: tsz_common::interner::Atom,
        property_types: (TypeId, TypeId),
        return_relation: (TypeId, TypeId, Option<&tsz_solver::SubtypeFailureReason>),
    ) -> Diagnostic {
        let (source_property_type, target_property_type) = property_types;
        let (source_return, target_return, return_inner) = return_relation;
        let source = ctx.source;
        let target = ctx.target;
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();
        let prop_name = self.ctx.types.resolve_atom_ref(property_name);
        let suffix =
            self.member_return_signature_suffix(source_property_type, target_property_type);
        let header = format_message(
            diagnostic_messages::THE_TYPES_RETURNED_BY_ARE_INCOMPATIBLE_BETWEEN_THESE_TYPES,
            &[&format!("{prop_name}{suffix}")],
        );
        let header_code =
            diagnostic_codes::THE_TYPES_RETURNED_BY_ARE_INCOMPATIBLE_BETWEEN_THESE_TYPES;

        // The header sits at `header_depth`: at the top level it is an
        // elaboration under the base `Type 'S' is not assignable to type 'T'.`
        // line; when nested it is this diagnostic's own message. Deeper lines
        // are authored at absolute depths starting one below the header, the
        // convention every other `render_*` follows when nested.
        let (mut diag, header_depth) = if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
            let mut diag = Diagnostic::error(
                file_name,
                start,
                length,
                format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                ),
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            diag.push_elaboration_in_span(start, length, header, header_code, 0);
            (diag, 0)
        } else {
            (
                Diagnostic::error(file_name, start, length, header, header_code),
                depth,
            )
        };

        // The return relation drills straight in beneath the TS2201 header, with
        // no intermediate function-type line (tsc omits it for both member forms).
        self.push_member_return_inner(
            &mut diag,
            source_return,
            target_return,
            return_inner,
            idx,
            header_depth + 1,
        );
        diag
    }

    /// The `()` / `(...)` suffix for the TS2201 `The types returned by '<name>...'`
    /// header. tsc uses `()` only when *both* the source and target call
    /// signatures take zero parameters, and `(...)` when either carries
    /// parameters (`reportIncompatibleCallSignatureReturn`).
    fn member_return_signature_suffix(
        &mut self,
        source_property_type: TypeId,
        target_property_type: TypeId,
    ) -> &'static str {
        if self.call_signature_param_count(source_property_type) == 0
            && self.call_signature_param_count(target_property_type) == 0
        {
            "()"
        } else {
            "(...)"
        }
    }

    /// Number of parameters on the first signature of `type_id`'s evaluated
    /// form. A function-typed member reaches here as either a bare `Function`
    /// shape (arrow-property syntax, `f: () => T`) or a `Callable` shape (method
    /// syntax, `f(): T`); `callable_shape_for_type_extended` normalizes both.
    /// Zero when the type carries no signature — the safe fallback that keeps the
    /// historical `<name>()` suffix.
    fn call_signature_param_count(&mut self, type_id: TypeId) -> usize {
        let evaluated = self.evaluate_type_for_assignability(type_id);
        crate::query_boundaries::diagnostics::callable_shape_for_type_extended(
            self.ctx.types,
            evaluated,
        )
        .and_then(|shape| shape.call_signatures.first().map(|sig| sig.params.len()))
        .unwrap_or(0)
    }

    /// Append the inner return-type relation beneath the member header at
    /// `leaf_depth`. When the solver carried a structural reason for the return
    /// failure (a missing property, nested chain, …) it is drilled; otherwise a
    /// plain `Type 'S' is not assignable to type 'T'.` leaf is synthesized.
    fn push_member_return_inner(
        &mut self,
        diag: &mut Diagnostic,
        source_return: TypeId,
        target_return: TypeId,
        return_inner: Option<&tsz_solver::SubtypeFailureReason>,
        idx: tsz_parser::parser::NodeIndex,
        leaf_depth: u32,
    ) {
        if let Some(inner) = return_inner {
            let sub =
                self.render_failure_reason(inner, source_return, target_return, idx, leaf_depth);
            Self::push_rebased_subdiagnostic(diag, sub, leaf_depth, leaf_depth);
        } else {
            let message = self.element_mismatch_message(source_return, target_return);
            let (start, length) = (diag.start, diag.length);
            diag.push_elaboration_in_span(
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                leaf_depth,
            );
        }
    }

    /// Render a tuple element type mismatch.
    ///
    /// tsc elaborates a failing tuple element with TS2626
    /// `Type at position <index> in source is not compatible with type at
    /// position <index> in target.` (both positions are the element index for
    /// fixed tuples), nested beneath the outer
    /// `Type 'S' is not assignable to type 'T'.` line, then the inner element
    /// failure. This mirrors the chain shape of
    /// [`Self::render_property_type_mismatch`] but keyed by position instead of
    /// a property name.
    pub(super) fn render_tuple_element_type_mismatch(
        &mut self,
        ctx: &RenderContext,
        index: usize,
        target_index: usize,
        source_element: TypeId,
        target_element: TypeId,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
        multi_element: bool,
    ) -> Diagnostic {
        // A single-element tuple has no position to disambiguate, so tsc omits
        // the TS2626 positional line and relates the element types directly.
        if !multi_element {
            return self.render_single_element_tuple_mismatch(
                ctx,
                source_element,
                target_element,
                nested_reason,
            );
        }

        // TS2626: the source and target positions coincide for a fixed
        // element, but a failing element that trails a rest slot reports its
        // own TARGET position (`[...number[], boolean]` fails at source 0
        // vs target 1).
        let detail = format_message(
            diagnostic_messages::TYPE_AT_POSITION_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_TARGET,
            &[&index.to_string(), &target_index.to_string()],
        );
        self.render_tuple_positional_chain(
            ctx,
            detail,
            diagnostic_codes::TYPE_AT_POSITION_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_TARGET,
            source_element,
            target_element,
            nested_reason,
        )
    }

    /// Build the `(message, code)` for a variadic/rest tuple positional
    /// mismatch: the plural TS2627 `Type at positions <start> through <end> in
    /// source is not compatible with type at position <target> in target.` when
    /// the source span covers more than one element, or the singular TS2626
    /// `Type at position <start> in source ... position <target> in target.` for
    /// a one-element span. The target position is the rest slot index, which
    /// generally differs from the source span. Rendered through the shared
    /// [`Self::render_tuple_positional_chain`].
    pub(super) fn variadic_positional_detail(
        source_start: usize,
        source_end: usize,
        target_position: usize,
    ) -> (String, u32) {
        if source_start == source_end {
            (
                format_message(
                    diagnostic_messages::TYPE_AT_POSITION_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_TARGET,
                    &[&source_start.to_string(), &target_position.to_string()],
                ),
                diagnostic_codes::TYPE_AT_POSITION_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_TARGET,
            )
        } else {
            (
                format_message(
                    diagnostic_messages::TYPE_AT_POSITIONS_THROUGH_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_T,
                    &[
                        &source_start.to_string(),
                        &source_end.to_string(),
                        &target_position.to_string(),
                    ],
                ),
                diagnostic_codes::TYPE_AT_POSITIONS_THROUGH_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_T,
            )
        }
    }

    /// Shared scaffolding for the positional tuple-mismatch chain used by both
    /// the fixed-element ([`Self::render_tuple_element_type_mismatch`]) render and
    /// the variadic-span dispatch (via [`Self::variadic_positional_detail`]).
    ///
    /// At `depth == 0` it heads with the `Type 'S' is not assignable to type
    /// 'T'.` line and attaches `detail` (the TS2626/TS2627 positional line) as a
    /// nested related-information entry; deeper in a chain it emits the
    /// positional line directly. The failing element relation is then drilled
    /// beneath via [`Self::push_tuple_element_inner_failure`].
    pub(super) fn render_tuple_positional_chain(
        &mut self,
        ctx: &RenderContext,
        detail: String,
        detail_code: u32,
        source_element: TypeId,
        target_element: TypeId,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> Diagnostic {
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();

        let mut diag = if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types_at(ctx.source, ctx.target, idx);
            let base = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            let mut diag = Diagnostic::error(
                file_name,
                start,
                length,
                base,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            diag.push_elaboration_in_span(start, length, detail, detail_code, 0);
            diag
        } else {
            Diagnostic::error(file_name, start, length, detail, detail_code)
        };

        if depth < 5 {
            self.push_tuple_element_inner_failure(
                &mut diag,
                idx,
                depth,
                source_element,
                target_element,
                nested_reason,
            );
        }

        diag
    }

    /// Render a single-element tuple element mismatch.
    ///
    /// With only one element there is no position to disambiguate, so tsc skips
    /// the TS2626 positional line and relates the element types directly. The
    /// element relation is rendered exactly like a top-level assignment failure
    /// — `Type 'se' is not assignable to type 'te'.` followed by the element's
    /// own elaboration.
    ///
    /// How the element relation is produced depends on whether the element's
    /// failure reason *self-heads* with that `Type 'se' …'te'` line:
    /// - **Self-heading** reasons (scalar leaves, unions, intersections,
    ///   same-generic applications, …) already emit the `Type 'se' …'te'` line
    ///   as their own top line, so the whole element relation is delegated to
    ///   the nested reason's renderer — emitting our own header would duplicate
    ///   it.
    /// - **Structural-drill** reasons ([`Self::tuple_element_nested_needs_header`]
    ///   — tuple/object/missing-property/index-signature) lead with a
    ///   specialized line (`Types of property 'a' …`, the deeper element pair,
    ///   …), so we emit the element-type header ourselves and then recurse.
    fn render_single_element_tuple_mismatch(
        &mut self,
        ctx: &RenderContext,
        source_element: TypeId,
        target_element: TypeId,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> Diagnostic {
        let depth = ctx.depth;
        let element_message = self.element_mismatch_message(source_element, target_element);
        let needs_header = nested_reason.is_some_and(Self::tuple_element_nested_needs_header);

        // Deeper levels (`depth > 0`) whose element self-heads: the nested
        // reason *is* the element relation, so delegate entirely rather than
        // wrapping it in a duplicate `Type 'se' …'te'` line.
        if depth > 0
            && !needs_header
            && let Some(nested) = nested_reason
        {
            return self.render_failure_reason(
                nested,
                source_element,
                target_element,
                ctx.idx,
                depth,
            );
        }

        let mut diag = if depth == 0 {
            let (source_str, target_str) = self
                .format_top_level_assignability_message_types_at(ctx.source, ctx.target, ctx.idx);
            let base = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            Diagnostic::error(
                ctx.file_name.clone(),
                ctx.start,
                ctx.length,
                base,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            )
        } else {
            Diagnostic::error(
                ctx.file_name.clone(),
                ctx.start,
                ctx.length,
                element_message.clone(),
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            )
        };

        // Depth of the element's own elaboration (the drill beneath the
        // element-type header). At `depth == 0` the header sits at related-depth
        // 0, so the drill is at 1; deeper, this node's header *is* its message
        // (placed by the parent at `depth`), so the drill is at `depth + 1`.
        let drill_depth = if depth == 0 { 1 } else { depth + 1 };
        if depth >= 5 {
            return diag;
        }

        match nested_reason {
            // Structural drill: emit the element-type header, then recurse the
            // element's own elaboration one level deeper.
            Some(nested) if needs_header => {
                if depth == 0 {
                    diag.push_elaboration_in_span(
                        ctx.start,
                        ctx.length,
                        element_message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        0,
                    );
                }
                let nested_diag = self.render_failure_reason(
                    nested,
                    source_element,
                    target_element,
                    ctx.idx,
                    drill_depth,
                );
                Self::push_nested_chain(&mut diag, nested_diag, drill_depth);
            }
            // Self-heading at `depth == 0`: the nested reason emits the
            // `Type 'se' …'te'` element line itself. Render it at depth 1 (so
            // the element types are formatted directly rather than recovered
            // from the assignment anchor) and rebase its chain one level up to
            // sit directly beneath the assignment line.
            Some(nested) => {
                let element_diag =
                    self.render_failure_reason(nested, source_element, target_element, ctx.idx, 1);
                Self::push_rebased_subdiagnostic(&mut diag, element_diag, 1, 0);
            }
            // No further structure: the element-type relation is terminal.
            None => {
                if depth == 0 {
                    diag.push_elaboration_in_span(
                        ctx.start,
                        ctx.length,
                        element_message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        0,
                    );
                }
            }
        }
        diag
    }

    /// Graft a sub-diagnostic (rendered at `sub_render_depth`) beneath `diag`,
    /// placing its own message line at `base_depth` and rebasing every line of
    /// its related chain by the same offset (`r.depth + base_depth -
    /// sub_render_depth`). Used when a tuple element's failure is delegated to a
    /// fresh sub-render that must be re-seated at a different chain level.
    fn push_rebased_subdiagnostic(
        diag: &mut Diagnostic,
        sub: Diagnostic,
        sub_render_depth: u32,
        base_depth: u32,
    ) {
        diag.push_elaboration_at(
            sub.file,
            sub.start,
            sub.length,
            sub.message_text,
            sub.code,
            base_depth,
        );
        let delta = i64::from(base_depth) - i64::from(sub_render_depth);
        for related in sub.related_information {
            diag.related_information
                .push(related.with_depth_shift(delta));
        }
    }

    /// Whether a tuple element's nested failure reason leads with a specialized
    /// elaboration line rather than self-heading with `Type 'se' …'te'`. Such
    /// reasons need an explicit element-type header emitted before them;
    /// everything else already emits that header itself.
    const fn tuple_element_nested_needs_header(reason: &tsz_solver::SubtypeFailureReason) -> bool {
        matches!(
            reason,
            tsz_solver::SubtypeFailureReason::TupleElementTypeMismatch { .. }
                | tsz_solver::SubtypeFailureReason::TupleVariadicPositionMismatch { .. }
                | tsz_solver::SubtypeFailureReason::SourceProvidesNoMatch { .. }
                | tsz_solver::SubtypeFailureReason::PropertyTypeMismatch { .. }
                | tsz_solver::SubtypeFailureReason::MissingProperty { .. }
                | tsz_solver::SubtypeFailureReason::MissingProperties { .. }
                | tsz_solver::SubtypeFailureReason::IndexSignatureMismatch { .. }
                | tsz_solver::SubtypeFailureReason::ReturnTypeMismatch { .. }
                | tsz_solver::SubtypeFailureReason::ParameterTypeMismatch { .. }
        )
    }

    /// Render a same-generic type-argument mismatch (`C<A..>` vs `C<B..>`).
    ///
    /// Emits the top-level `Type 'C<A..>' is not assignable to type 'C<B..>'.`
    /// line followed directly by the failing argument relation, with no
    /// intermediate `Types of property 'x' are incompatible.` wrapper — tsc
    /// elaborates same-generic argument failures straight into the differing
    /// argument. Deeper argument failures (e.g. nested generic arguments) keep
    /// elaborating through the structured `nested_reason`.
    pub(super) fn render_type_argument_mismatch(
        &mut self,
        ctx: &RenderContext,
        source_arg: TypeId,
        target_arg: TypeId,
        nested_reason: &tsz_solver::SubtypeFailureReason,
    ) -> Diagnostic {
        let source = ctx.source;
        let target = ctx.target;
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();

        let (source_str, target_str) = if depth == 0
            && !self.same_generic_mismatch_keeps_application_top_level(source, target)
        {
            let (source_str, _) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
            (
                source_str,
                self.format_type_for_assignability_message_skip_application_alias(target_arg),
            )
        } else if depth == 0 {
            self.format_top_level_assignability_message_types_at(source, target, idx)
        } else {
            (
                self.format_type_diagnostic(source),
                self.format_type_diagnostic(target),
            )
        };
        let base = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        let mut diag = Diagnostic::error(
            file_name,
            start,
            length,
            base,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );

        // The failing argument relation is rendered as the immediate child of
        // this line (no intermediate wrapper), so it sits at the current
        // elaboration depth — one indent level beneath the application line.
        if depth < 5 {
            let (nested_source, nested_target) =
                Self::nested_failure_display_types(nested_reason, source_arg, target_arg);
            if Self::nested_reason_is_plain_type_mismatch(nested_reason) {
                let source_str = self
                    .format_type_for_assignability_message_skip_application_alias(nested_source);
                let target_str = self
                    .format_type_for_assignability_message_skip_application_alias(nested_target);
                diag.push_elaboration_in_span(
                    start,
                    length,
                    format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    ),
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    depth,
                );
            } else {
                let mut nested_diag = self.render_failure_reason(
                    nested_reason,
                    nested_source,
                    nested_target,
                    idx,
                    depth + 1,
                );
                // `push_nested_chain` renumbers only the nested headline to
                // this line's child position (`depth`). The union-source
                // renderer places the headline's own children at `depth + 2`
                // (its member header sits at `ctx.depth + 1`), which would
                // leave a skipped indent level beneath the renumbered
                // headline — pull exactly that shape up one level. A nested
                // same-generic drill (`Wrap` of `Wrap`) and the other arms
                // already place children at `depth + 1`, so shifting them
                // would flatten a genuinely nested chain into siblings.
                if matches!(
                    nested_reason,
                    tsz_solver::SubtypeFailureReason::UnionSourceMismatch { .. }
                ) {
                    nested_diag.related_information = nested_diag
                        .related_information
                        .into_iter()
                        .map(|related| related.with_depth_shift(-1))
                        .collect();
                }
                Self::push_nested_chain(&mut diag, nested_diag, depth);
            }
        }

        diag
    }

    /// Whether a failing union member's nested reason leads its elaboration
    /// with a specialized line (`Type at position 0 …`, `Types of property
    /// 'p' …`, `'string' index signatures are incompatible.`, or — for a
    /// function-return mismatch — the bare return-relation leaf) instead of
    /// self-heading with `Type 'M' is not assignable to type 'T'.`. Such
    /// reasons need an explicit member-type header emitted before the
    /// structural drill; self-heading reasons (the property
    /// `MissingProperty`/`MissingProperties` summaries, `ParameterTypeMismatch`
    /// — whose own first line is the signature relation — and plain leaf
    /// relations) already carry the member line themselves.
    pub(super) const fn union_member_nested_needs_header(
        reason: &tsz_solver::SubtypeFailureReason,
    ) -> bool {
        matches!(
            reason,
            tsz_solver::SubtypeFailureReason::TupleElementTypeMismatch { .. }
                | tsz_solver::SubtypeFailureReason::TupleVariadicPositionMismatch { .. }
                // Tuple fixed-/variadic-arity count leaves (`Source has N
                // element(s) …`) carry no member name, so the union renderer
                // emits the `Type 'M' is not assignable to type 'T'.` header
                // before drilling the arity leaf.
                | tsz_solver::SubtypeFailureReason::TupleElementMismatch { .. }
                | tsz_solver::SubtypeFailureReason::TupleArityMismatch(_)
                | tsz_solver::SubtypeFailureReason::SourceProvidesNoMatch { .. }
                | tsz_solver::SubtypeFailureReason::PropertyTypeMismatch { .. }
                | tsz_solver::SubtypeFailureReason::IndexSignatureMismatch { .. }
                | tsz_solver::SubtypeFailureReason::ReturnTypeMismatch { .. }
        )
    }

    /// Render an outer `Type 'S' is not assignable to type 'T'.` line and
    /// elaborate it with the child relation `child_source <: child_target`,
    /// preserving the nested reason chain.
    ///
    /// Used by [`Self::render_union_source_mismatch`] and the
    /// `ConditionalBranchMismatch` dispatch arm: both shapes layer a child
    /// branch relation one indent beneath the outer pair, with the same
    /// depth handling and the same plain-leaf vs structural-recursion split.
    /// The depth-0 outer line reuses `render_type_mismatch` so the primary
    /// diagnostic keeps the standard source/target display (e.g. preserving
    /// the full union surface). At deeper depths the outer pair is formatted
    /// structurally.
    pub(super) fn render_parent_with_child_relation(
        &mut self,
        ctx: &RenderContext,
        source_type: TypeId,
        target_type: TypeId,
        child_source: TypeId,
        child_target: TypeId,
        nested_reason: &tsz_solver::SubtypeFailureReason,
    ) -> Diagnostic {
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();

        let mut diag = if depth == 0 {
            self.render_type_mismatch(ctx)
        } else {
            // Nested relation line: generalize a literal / all-unit-union
            // source to its base when the target has no singleton capacity
            // (tsc `reportRelationError`).
            let display_source =
                self.generalize_nested_relation_source_for_display(source_type, target_type);
            let source_str = self.format_type_diagnostic(display_source);
            let target_str = self.format_type_diagnostic(target_type);
            let base = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            Diagnostic::error(
                file_name,
                start,
                length,
                base,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            )
        };

        // The failing child relation sits exactly one indent level beneath
        // the outer line. At depth 0 the outer is the (un-indented) primary,
        // so its first child is at related-depth 0; when nested, the outer
        // is at related-depth `depth`, so the child is at `depth + 1`.
        if depth < 5 {
            let child_depth = if depth == 0 { 0 } else { depth + 1 };
            let (nested_source, nested_target) =
                Self::nested_failure_display_types(nested_reason, child_source, child_target);
            if Self::nested_reason_is_plain_type_mismatch(nested_reason) {
                // Plain leaf relation (e.g. `undefined` vs `number`): render
                // the child source/target structurally so the displayed
                // source is the failing branch/member, not the enclosing
                // assignment's RHS expression. The leaf generalizes the same
                // way as the outer line (tsc runs `reportRelationError` on
                // every relation line).
                let display_source = self
                    .generalize_nested_relation_source_for_display(nested_source, nested_target);
                // `source_type` is the union whenever this leaf elaborates a
                // `UnionSourceMismatch` member (see `render_union_source_mismatch`
                // below); for every other caller of this shared renderer the
                // per-union provenance lookup simply misses and falls back to
                // the ordinary diagnostic formatting.
                let source_str =
                    self.format_type_diagnostic_for_union_member(source_type, display_source);
                let target_str = self.format_type_diagnostic(nested_target);
                diag.push_elaboration_in_span(
                    start,
                    length,
                    format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    ),
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    child_depth,
                );
            } else {
                // The nested reason is rendered at `child_depth.max(1)` (a
                // structural renderer needs depth >= 1 to format its child lines
                // structurally rather than as a top-level primary), but its
                // *primary* line lands at `child_depth`. When `child_depth == 0`
                // the two differ by one, so a multi-line nested reason (e.g. an
                // array-element mismatch, whose own header doubles as the member
                // line and which then drills into the element relation) would
                // keep its drill lines one indent too deep. Rebase the whole
                // sub-chain from its render depth down to `child_depth` so the
                // drill lines sit directly beneath the member line. For
                // `child_depth >= 1` the render depth already equals
                // `child_depth`, so this is a no-op and deeper chains are
                // unchanged.
                let nested_diag = self.render_failure_reason(
                    nested_reason,
                    nested_source,
                    nested_target,
                    idx,
                    child_depth.max(1),
                );
                Self::push_rebased_subdiagnostic(
                    &mut diag,
                    nested_diag,
                    child_depth.max(1),
                    child_depth,
                );
            }
        }

        diag
    }

    /// Render a target-intersection failure: the intersection headline, then the
    /// first failing constituent's relation one level deeper.
    ///
    /// `tsc` (`typeRelatedToEachType`) relates the source to each constituent of
    /// a target intersection `C1 & C2 & …` and elaborates the first failing one.
    /// A structural failure self-heads with the constituent frame `Type 'S' is
    /// not assignable to type 'Ci'.` followed by its own drill; a
    /// missing-property leaf is folded (the missing line already names `Ci`); a
    /// plain leaf collapses to the constituent frame itself.
    pub(super) fn render_intersection_target_mismatch(
        &mut self,
        ctx: &RenderContext,
        source_type: TypeId,
        target_type: TypeId,
        constituent_type: TypeId,
        nested_reason: &tsz_solver::SubtypeFailureReason,
        original_reason: &tsz_solver::SubtypeFailureReason,
    ) -> Diagnostic {
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();

        // Top-level intersection headline (`Type 'S' is not assignable to type
        // 'C1 & C2 & …'.`). This line is the only one the conformance harness
        // fingerprints, so it must stay byte-identical to the pre-wrap output:
        // render the merged-target `original_reason` and reuse exactly its
        // headline (its primary `message_text`/`code`), then discard its
        // elaboration — the constituent frame and drill below replace it. This
        // preserves whichever source/target display the unwrapped reason used
        // (e.g. the written intersection order for an `object & string` source,
        // which neither the structural nor the merged formatter reproduces
        // verbatim). Nested, fall back to a plain structural headline.
        let mut diag = if depth == 0 {
            let headline =
                self.render_failure_reason(original_reason, source_type, target_type, idx, 0);
            Diagnostic::error(
                file_name.clone(),
                start,
                length,
                headline.message_text,
                headline.code,
            )
        } else {
            let source_str = self.format_type_diagnostic(source_type);
            let target_str = self.format_type_diagnostic(target_type);
            Diagnostic::error(
                file_name.clone(),
                start,
                length,
                format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                ),
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            )
        };

        if depth >= 5 {
            return diag;
        }
        // The failing constituent's relation sits one indent level beneath the
        // headline. At depth 0 the headline is the (un-indented) primary, so its
        // first child is related-depth 0; when nested, the headline is at
        // related-depth `depth`, so the child is at `depth + 1`.
        let child_depth = if depth == 0 { 0 } else { depth + 1 };

        // A reason that self-heads with a non-frame primary (a missing-property
        // leaf renders `Property 'p' is missing in type 'S' but required in type
        // 'Ci'.`) folds: render it directly beneath the headline with no
        // constituent frame, since its own line already names `Ci`.
        if matches!(
            nested_reason,
            tsz_solver::SubtypeFailureReason::MissingProperty { .. }
                | tsz_solver::SubtypeFailureReason::MissingProperties { .. }
        ) {
            let sub = self.render_failure_reason(
                nested_reason,
                source_type,
                constituent_type,
                idx,
                child_depth,
            );
            Self::push_rebased_subdiagnostic(&mut diag, sub, child_depth, child_depth);
            return diag;
        }

        // Otherwise emit the constituent frame `Type 'S' is not assignable to
        // type 'Ci'.` (structural display, so the constituent — not the merged
        // intersection — is named) one level beneath the headline. The frame is
        // a nested relation line, so its literal source generalizes against
        // the constituent (tsc `reportRelationError`).
        let display_source =
            self.generalize_nested_relation_source_for_display(source_type, constituent_type);
        let frame_source = self.format_type_diagnostic(display_source);
        let frame_target = self.format_type_diagnostic(constituent_type);
        let frame_message = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&frame_source, &frame_target],
        );
        diag.push_elaboration_at(
            file_name,
            start,
            length,
            frame_message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            child_depth,
        );

        // Render the `S <: Ci` relation as a standalone diagnostic (depth 0) so
        // its drill keeps `tsc`'s path-compressed shape (`The types of 'x.p' are
        // incompatible …`, which the property renderer only produces at depth 0),
        // then drop its anchor-derived headline (already expressed by the frame
        // above) and slot the remaining drill one level beneath the frame. A
        // plain leaf carries no drill, so the frame stands alone.
        let sub = self.render_failure_reason(nested_reason, source_type, constituent_type, idx, 0);
        let drill_base = i64::from(child_depth + 1);
        for related in sub.related_information {
            diag.related_information
                .push(related.with_depth_shift(drill_base));
        }

        diag
    }

    /// Append the inner element failure line beneath a tuple element mismatch.
    ///
    /// Uses the structured `nested_reason` when present so deeply nested element
    /// failures keep elaborating; otherwise falls back to a direct
    /// `Type 'S' is not assignable to type 'T'.` line for the element pair so the
    /// chain never stops at the bare `Types of property` header.
    fn push_tuple_element_inner_failure(
        &mut self,
        diag: &mut Diagnostic,
        idx: tsz_parser::parser::NodeIndex,
        depth: u32,
        source_element: TypeId,
        target_element: TypeId,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) {
        // When a positional (multi-element) tuple's failing element is itself a
        // single-element tuple, tsc relates the element types directly with the
        // element-type header `Type 'se' is not assignable to type 'te'.` before
        // drilling — exactly like a top-level single-element tuple relation.
        // Route through the single-element renderer so the header is preserved.
        if let Some(
            nested @ tsz_solver::SubtypeFailureReason::TupleElementTypeMismatch {
                multi_element: false,
                ..
            },
        ) = nested_reason
        {
            let element_ctx = RenderContext {
                source: source_element,
                target: target_element,
                idx,
                depth: depth + 1,
                start: diag.start,
                length: diag.length,
                file_name: diag.file.clone(),
                source_display_override: None,
            };
            let element_diag = self.render_single_element_tuple_mismatch(
                &element_ctx,
                source_element,
                target_element,
                Some(nested),
            );
            Self::push_nested_chain(diag, element_diag, depth + 1);
            return;
        }
        if let Some(nested) = nested_reason {
            let (nested_source, nested_target) =
                Self::nested_failure_display_types(nested, source_element, target_element);
            let nested_diag =
                self.render_failure_reason(nested, nested_source, nested_target, idx, depth + 1);
            Self::push_nested_chain(diag, nested_diag, depth + 1);
        } else {
            let message = self.element_mismatch_message(source_element, target_element);
            diag.push_elaboration(
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                depth + 1,
            );
        }
    }

    /// Render an `IndexSignatureMismatch` failure.
    ///
    /// At `depth == 0` emits the TS2322 top-level message followed by
    /// `"'{kind}' index signatures are incompatible."` and the value-type
    /// nested chain. At deeper depths, emits the incompatibility message
    /// directly and continues the chain.
    pub(super) fn render_index_signature_mismatch(
        &mut self,
        ctx: &RenderContext,
        index_kind: &str,
        source_value_type: TypeId,
        target_value_type: TypeId,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
        property_name: Option<tsz_common::interner::Atom>,
    ) -> Diagnostic {
        // A named source property measured against the target's index signature
        // renders as TS2530 "Property '{name}' is incompatible with index
        // signature."; a source *index signature* vs the target index renders
        // as TS2634 "'{kind}' index signatures are incompatible." `tsc` uses the
        // same head (TS2322/TS2345) for both and only the elaboration differs.
        let (incompat_message, incompat_code) = match property_name {
            Some(name) => (
                format_message(
                    diagnostic_messages::PROPERTY_IS_INCOMPATIBLE_WITH_INDEX_SIGNATURE,
                    &[&self.ctx.types.resolve_atom_ref(name)],
                ),
                diagnostic_codes::PROPERTY_IS_INCOMPATIBLE_WITH_INDEX_SIGNATURE,
            ),
            None => (
                format_message(
                    diagnostic_messages::INDEX_SIGNATURES_ARE_INCOMPATIBLE,
                    &[index_kind],
                ),
                diagnostic_codes::INDEX_SIGNATURES_ARE_INCOMPATIBLE,
            ),
        };

        let mut diag = if ctx.depth == 0 {
            let source_str = self.format_type_for_diagnostic_role(
                ctx.source,
                DiagnosticTypeDisplayRole::AssignmentSource {
                    target: ctx.target,
                    anchor_idx: ctx.idx,
                },
            );
            let target_str = self.format_assignability_type_for_message(ctx.target, ctx.source);
            let base = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            let mut diag = Diagnostic::error(
                ctx.file_name.clone(),
                ctx.start,
                ctx.length,
                base,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            diag.push_elaboration_in_span(
                ctx.start,
                ctx.length,
                incompat_message,
                incompat_code,
                0,
            );
            diag
        } else {
            Diagnostic::error(
                ctx.file_name.clone(),
                ctx.start,
                ctx.length,
                incompat_message,
                incompat_code,
            )
        };

        if ctx.depth < 5 {
            self.push_tuple_element_inner_failure(
                &mut diag,
                ctx.idx,
                ctx.depth,
                source_value_type,
                target_value_type,
                nested_reason,
            );
        }

        diag
    }

    /// Flatten a fully-rendered nested failure into `diag`'s related
    /// information: the nested diagnostic's own message line followed by its
    /// related chain. This is the shared shape every elaboration step uses to
    /// append a child reason.
    ///
    /// `child_depth` is the render depth at which `nested_diag` was produced; it
    /// becomes the nested message line's elaboration depth so the plain reporter
    /// can indent each chain level by 2 more spaces than its parent, matching
    /// `tsc`. The nested diagnostic's own related chain already carries absolute
    /// depths from its render, so it is appended unchanged.
    pub(super) fn push_nested_chain(
        diag: &mut Diagnostic,
        nested_diag: Diagnostic,
        child_depth: u32,
    ) {
        diag.push_elaboration_at(
            nested_diag.file,
            nested_diag.start,
            nested_diag.length,
            nested_diag.message_text,
            nested_diag.code,
            child_depth,
        );
        diag.related_information
            .extend(nested_diag.related_information);
    }

    /// Render an array element-type mismatch (`se[]` vs `te[]`).
    ///
    /// `tsc` elaborates an array relation exactly like a single numerically
    /// keyed element: it leads with the `Type 'se[]' is not assignable to type
    /// 'te[]'.` line (the array types themselves — unlike a single-element
    /// tuple, the array relation never collapses to its element line), then
    /// relates the element types directly beneath it, recursing through the
    /// element's own failure reason. Examples:
    ///
    /// ```text
    /// Type 'number[]' is not assignable to type 'string[]'.
    ///   Type 'number' is not assignable to type 'string'.
    ///
    /// Type 'number[][]' is not assignable to type 'string[][]'.
    ///   Type 'number[]' is not assignable to type 'string[]'.
    ///     Type 'number' is not assignable to type 'string'.
    /// ```
    ///
    /// The element drill is shared with the tuple/index-signature renderers via
    /// [`Self::push_tuple_element_inner_failure`]; only the header (always the
    /// array types) is array-specific.
    pub(super) fn render_array_element_mismatch(
        &mut self,
        ctx: &RenderContext,
        source_element: TypeId,
        target_element: TypeId,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> Diagnostic {
        let depth = ctx.depth;

        // Header: the array types themselves (`se[]` vs `te[]`), at every depth.
        // Unlike a single-element tuple, an array relation never collapses to
        // its element line — `tsc` always shows the array-to-array line first.
        let (source_str, target_str) = if depth == 0 {
            self.format_top_level_assignability_message_types_at(ctx.source, ctx.target, ctx.idx)
        } else {
            (
                self.format_type_diagnostic(ctx.source),
                self.format_type_diagnostic(ctx.target),
            )
        };
        let base = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        let mut diag = Diagnostic::error(
            ctx.file_name.clone(),
            ctx.start,
            ctx.length,
            base,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );

        if depth >= 5 {
            return diag;
        }

        // The element relation `se -> te` sits directly beneath the array
        // header (no intermediate line, exactly like a single-element tuple).
        // It occupies related-depth `elem_depth`; its own drill goes one deeper.
        let elem_depth = if depth == 0 { 0 } else { depth + 1 };

        match nested_reason {
            // Self-heading element (scalar leaf, union, nested array, …): the
            // nested reason emits the `Type 'se' …'te'` element line itself.
            // Render it one level deeper and rebase its message down to sit
            // directly beneath the array header.
            Some(nested) if !Self::tuple_element_nested_needs_header(nested) => {
                let element_diag = self.render_failure_reason(
                    nested,
                    source_element,
                    target_element,
                    ctx.idx,
                    elem_depth + 1,
                );
                Self::push_rebased_subdiagnostic(
                    &mut diag,
                    element_diag,
                    elem_depth + 1,
                    elem_depth,
                );
            }
            // Structural element (object property, index signature, …) or a
            // terminal scalar pair: emit the `Type 'se' …'te'` element header,
            // then drill into the structural reason when one is present.
            other => {
                let element_message = self.element_mismatch_message(source_element, target_element);
                diag.push_elaboration_in_span(
                    ctx.start,
                    ctx.length,
                    element_message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    elem_depth,
                );
                if let Some(nested) = other {
                    let nested_diag = self.render_failure_reason(
                        nested,
                        source_element,
                        target_element,
                        ctx.idx,
                        elem_depth + 1,
                    );
                    Self::push_nested_chain(&mut diag, nested_diag, elem_depth + 1);
                }
            }
        }

        diag
    }

    /// Format the `Type 'se' is not assignable to type 'te'.` line for an
    /// element-type pair, disambiguating same-named types (e.g. `N.Token` vs
    /// `M.Token`) so the line is never the ambiguous
    /// `Type 'Token' is not assignable to type 'Token'.`, matching `tsc`.
    pub(super) fn element_mismatch_message(
        &mut self,
        source_element: TypeId,
        target_element: TypeId,
    ) -> String {
        // Generalize the literal source (tsc `reportRelationError`), format
        // both sides, and disambiguate same-named nominal pairs like the top
        // level does — the disambiguator gets the type actually rendered.
        let display_source =
            self.generalize_nested_relation_source_for_display(source_element, target_element);
        let source_str = self.format_type_diagnostic(display_source);
        let target_str = self.format_type_diagnostic(target_element);
        let (source_str, target_str) = self.finalize_pair_display_for_diagnostic(
            display_source,
            target_element,
            source_str,
            target_str,
        );
        format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        )
    }
}
