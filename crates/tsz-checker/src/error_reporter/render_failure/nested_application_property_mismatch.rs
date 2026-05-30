use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::error_reporter::render_failure::RenderContext;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn application_base_for_property_mismatch_display(&self, type_id: TypeId) -> Option<TypeId> {
        crate::query_boundaries::common::application_info(self.ctx.types, type_id)
            .or_else(|| {
                let alias = self.ctx.types.get_display_alias(type_id)?;
                crate::query_boundaries::common::application_info(self.ctx.types, alias)
            })
            .map(|(base, _)| base)
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
            | tsz_solver::SubtypeFailureReason::ArrayElementMismatch {
                source_element,
                target_element,
            } => (*source_element, *target_element),
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
        use tsz_solver::SubtypeFailureReason as R;
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
    fn push_property_chain_leaf(
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
            diag.related_information.push(DiagnosticRelatedInformation {
                file: diag.file.clone(),
                start: diag.start,
                length: diag.length,
                message_text: message,
                category: DiagnosticCategory::Message,
                code: diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                depth: depth.min(u8::MAX as u32) as u8,
            });
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
                    diag.related_information.push(DiagnosticRelatedInformation {
                        file: diag.file.clone(),
                        start,
                        length,
                        message_text: detail,
                        category: DiagnosticCategory::Message,
                        code: reason.diagnostic_code(),
                        depth: 0,
                    });
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
            let leaf_is_plain = leaf.is_none_or(Self::nested_reason_is_plain_type_mismatch);
            if path.len() >= 2 && leaf_is_plain {
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
                diag.related_information.push(DiagnosticRelatedInformation {
                    file: diag.file.clone(),
                    start,
                    length,
                    message_text: detail,
                    category: DiagnosticCategory::Message,
                    code: diagnostic_codes::THE_TYPES_OF_ARE_INCOMPATIBLE_BETWEEN_THESE_TYPES,
                    depth: 0,
                });
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
            diag.related_information.push(DiagnosticRelatedInformation {
                file: diag.file.clone(),
                start,
                length,
                message_text: detail,
                category: DiagnosticCategory::Message,
                code: reason.diagnostic_code(),
                depth: 0,
            });
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
            && depth < 5
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
    ) -> Diagnostic {
        let source = ctx.source;
        let target = ctx.target;
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();
        let index_str = index.to_string();

        // TS2626: source and target positions are both the element index for a
        // fixed tuple element mismatch.
        let detail = format_message(
            diagnostic_messages::TYPE_AT_POSITION_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_TARGET,
            &[&index_str, &index_str],
        );

        let mut diag = if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
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
            diag.related_information.push(DiagnosticRelatedInformation {
                file: diag.file.clone(),
                start,
                length,
                message_text: detail,
                category: DiagnosticCategory::Message,
                code: diagnostic_codes::TYPE_AT_POSITION_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_TARGET,
                            depth: 0,
            });
            diag
        } else {
            Diagnostic::error(
                file_name,
                start,
                length,
                detail,
                diagnostic_codes::TYPE_AT_POSITION_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_TARGET,
            )
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
            diag.related_information.push(DiagnosticRelatedInformation {
                file: diag.file.clone(),
                start: diag.start,
                length: diag.length,
                message_text: message,
                category: DiagnosticCategory::Message,
                code: diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                depth: (depth + 1).min(u8::MAX as u32) as u8,
            });
        }
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
    fn push_nested_chain(diag: &mut Diagnostic, nested_diag: Diagnostic, child_depth: u32) {
        diag.related_information.push(DiagnosticRelatedInformation {
            file: nested_diag.file,
            start: nested_diag.start,
            length: nested_diag.length,
            message_text: nested_diag.message_text,
            category: DiagnosticCategory::Message,
            code: nested_diag.code,
            depth: child_depth.min(u8::MAX as u32) as u8,
        });
        diag.related_information
            .extend(nested_diag.related_information);
    }
}
