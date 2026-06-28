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
            crate::query_boundaries::common::application_info(self.ctx.types, type_id)
        {
            (type_id, base)
        } else {
            let alias = self.ctx.types.get_display_alias(type_id)?;
            let (base, _) =
                crate::query_boundaries::common::application_info(self.ctx.types, alias)?;
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
        }

        let source_eval = self.evaluate_type_for_assignability(source);
        let target_eval = self.evaluate_type_for_assignability(target);
        structural_display_type(self.ctx.types, source_eval)
            || structural_display_type(self.ctx.types, target_eval)
    }

    const fn nested_reason_is_plain_type_mismatch(
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
        use crate::query_boundaries::common::SubtypeFailureReason as R;
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
            let (s, t) = Self::nested_failure_display_types(leaf, leaf_src, leaf_tgt);
            let leaf_diag = self.render_failure_reason(leaf, s, t, idx, depth);
            Self::push_nested_chain(diag, leaf_diag, depth);
        } else {
            let s = self.format_type_diagnostic(leaf_src);
            let t = self.format_type_diagnostic(leaf_tgt);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&s, &t],
            );
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

        if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
            let outer_is_structural = {
                let eval_source = self.evaluate_type_for_assignability(source);
                let eval_target = self.evaluate_type_for_assignability(target);
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, eval_source)
                    .is_some()
                    || crate::query_boundaries::common::object_shape_for_type(
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
            // keep that predicate separate here.
            let leaf_is_collapsible = leaf.is_none_or(|reason| {
                Self::nested_reason_is_plain_type_mismatch(reason)
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
            if let Some(nested) = nested_reason {
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

        let index_str = index.to_string();
        // TS2626: source and target positions are both the element index for a
        // fixed tuple element mismatch.
        let detail = format_message(
            diagnostic_messages::TYPE_AT_POSITION_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_TARGET,
            &[&index_str, &index_str],
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
        let element_message = {
            let source_str = self.format_type_diagnostic(source_element);
            let target_str = self.format_type_diagnostic(target_element);
            format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            )
        };
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
                let nested_diag = self.render_failure_reason(
                    nested_reason,
                    nested_source,
                    nested_target,
                    idx,
                    depth + 1,
                );
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

    /// Render a union-source mismatch: a union type that is not assignable to
    /// the target because one of its members fails.
    ///
    /// tsc keeps the root mismatch visible by elaborating the first failing
    /// member directly beneath the union-to-target line:
    ///
    /// ```text
    /// Type 'A | B' is not assignable to type 'T'.
    ///   Type 'B' is not assignable to type 'T'.
    /// ```
    ///
    /// When the member fails for a *structural* reason (a tuple element type
    /// mismatch or an object property-type mismatch) tsc emits the member-type
    /// header explicitly and then drills into the structural detail:
    ///
    /// ```text
    /// Type 'A | B' is not assignable to type 'T'.
    ///   Type 'B' is not assignable to type 'T'.
    ///     Type at position 0 in source is not compatible with type at position 0 in target.
    ///       Type 'number' is not assignable to type 'string'.
    /// ```
    ///
    /// The structural renderers omit that header at depth >= 1 (they lead with
    /// `Type at position N …` / `Types of property 'p' …`), so this path emits
    /// it before recursing. Self-heading members (leaf relations, missing
    /// property summaries) carry the member line themselves and are delegated
    /// to [`Self::render_parent_with_child_relation`].
    pub(super) fn render_union_source_mismatch(
        &mut self,
        ctx: &RenderContext,
        source_type: TypeId,
        target_type: TypeId,
        member_type: TypeId,
        nested_reason: &tsz_solver::SubtypeFailureReason,
    ) -> Diagnostic {
        if !Self::union_member_nested_needs_header(nested_reason) {
            return self.render_parent_with_child_relation(
                ctx,
                source_type,
                target_type,
                member_type,
                target_type,
                nested_reason,
            );
        }

        let depth = ctx.depth;
        // The member keeps its own alias (`C` stays `C`); the target keeps the
        // diagnostic alias the rest of the chain uses (`A`), matching tsc for
        // object/interface targets. (tsc additionally expands a *tuple* target
        // alias to its structural form here, but tsz's shared formatter follows
        // the tuple's lazy display alias; the chain shape, positions, and leaf
        // relation are otherwise identical.) Format the target once; it heads
        // both the (deep) outer union line and the member header below.
        let target_str = self.format_type_diagnostic(target_type);

        // Outer union line. At depth 0 it is the primary diagnostic, which
        // reuses `render_type_mismatch` so the full union/alias surface is
        // preserved; deeper, format the union/target pair structurally.
        let mut diag = if depth == 0 {
            self.render_type_mismatch(ctx)
        } else {
            let source_str = self.format_type_diagnostic(source_type);
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
        };

        if depth >= 5 {
            return diag;
        }

        // The member header sits one indent beneath the union line; the
        // structural drill sits one level beneath the header. At depth 0 the
        // union line is the (un-indented) primary, so its first child is at
        // related-depth 0.
        let header_depth = if depth == 0 { 0 } else { depth + 1 };
        let drill_depth = header_depth + 1;

        let member_str = self.format_type_diagnostic(member_type);
        diag.push_elaboration_in_span(
            ctx.start,
            ctx.length,
            format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&member_str, &target_str],
            ),
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            header_depth,
        );

        // A union member that is a function failing on its *return* type drills
        // straight into the return relation beneath the member header, with no
        // intermediate `Return type 'X' is not assignable to 'Y'.` frame — `tsc`
        // relates the return types directly:
        //
        // ```text
        //   Type '(x: string) => string' is not assignable to type 'Target'.
        //     Type 'string' is not assignable to type 'number'.
        // ```
        //
        // Rendering the `ReturnTypeMismatch` reason itself would emit the
        // `Return type …` frame (a non-`tsc` line at depth >= 1), so recurse into
        // the carried return relation instead.
        if let tsz_solver::SubtypeFailureReason::ReturnTypeMismatch {
            source_return,
            target_return,
            nested_reason: inner,
        } = nested_reason
        {
            // When the return relation has no structured sub-reason, drill it as
            // a plain leaf so both arms render the return types through the same
            // path (`render_failure_reason` → `render_type_mismatch`).
            let leaf;
            let return_reason = match inner.as_deref() {
                Some(inner) => inner,
                None => {
                    leaf = tsz_solver::SubtypeFailureReason::TypeMismatch {
                        source_type: *source_return,
                        target_type: *target_return,
                    };
                    &leaf
                }
            };
            let return_diag = self.render_failure_reason(
                return_reason,
                *source_return,
                *target_return,
                ctx.idx,
                drill_depth,
            );
            Self::push_nested_chain(&mut diag, return_diag, drill_depth);
            return diag;
        }

        let nested_diag = self.render_failure_reason(
            nested_reason,
            member_type,
            target_type,
            ctx.idx,
            drill_depth,
        );
        Self::push_nested_chain(&mut diag, nested_diag, drill_depth);

        diag
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
            let source_str = self.format_type_diagnostic(source_type);
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
                // assignment's RHS expression.
                let source_str = self.format_type_diagnostic(nested_source);
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
        // intersection — is named) one level beneath the headline.
        let frame_source = self.format_type_diagnostic(source_type);
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
            let source_str = self.format_type_diagnostic(source_element);
            let target_str = self.format_type_diagnostic(target_element);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
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
    ) -> Diagnostic {
        let incompat_message = format_message(
            diagnostic_messages::INDEX_SIGNATURES_ARE_INCOMPATIBLE,
            &[index_kind],
        );

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
                diagnostic_codes::INDEX_SIGNATURES_ARE_INCOMPATIBLE,
                0,
            );
            diag
        } else {
            Diagnostic::error(
                ctx.file_name.clone(),
                ctx.start,
                ctx.length,
                incompat_message,
                diagnostic_codes::INDEX_SIGNATURES_ARE_INCOMPATIBLE,
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
        let source_str = self.format_type_diagnostic(source_element);
        let target_str = self.format_type_diagnostic(target_element);
        let (source_str, target_str) = self.finalize_pair_display_for_diagnostic(
            source_element,
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
