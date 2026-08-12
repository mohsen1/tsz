//! Helper methods for assignability error reporting.
//! Extracted from `assignability.rs` for maintainability.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::error_reporter::assignability::is_object_prototype_method;
use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
};
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::{CheckerState, MemberAccessLevel};
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

use crate::query_boundaries::type_checking_utilities as query_utils;

impl<'a> CheckerState<'a> {
    pub(crate) fn recover_unknown_array_source_type_for_display(
        &mut self,
        source: TypeId,
        idx: NodeIndex,
        depth: u32,
    ) -> TypeId {
        if depth != 0 {
            return source;
        }
        let Some(source_element) =
            crate::query_boundaries::diagnostics::array_element_type(self.ctx.types, source)
        else {
            return source;
        };

        let Some(expr_idx) = self.assignment_source_expression(idx) else {
            return source;
        };
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return source;
        };

        if node.kind == tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION
            || node.kind == tsz_parser::parser::syntax_kind_ext::NEW_EXPRESSION
        {
            let Some(call) = self.ctx.arena.get_call_expr(node) else {
                return source;
            };
            let Some(args) = call.arguments.as_ref() else {
                return source;
            };
            let Some(&first_arg) = args.nodes.first() else {
                return source;
            };

            // Display recovery must only *read* already-computed node types.
            // Forcing a fresh computation here can re-enter checking of an
            // expression whose enclosing chain is still mid-resolution (e.g.
            // rendering a nested call's overload failure), typing a callback
            // without its contextual type and leaking its diagnostics.
            let Some(first_arg_type) = self.ctx.node_types.get(&first_arg.0).copied() else {
                return source;
            };
            if matches!(first_arg_type, TypeId::ERROR | TypeId::UNKNOWN) {
                return source;
            }

            let element_type = crate::query_boundaries::diagnostics::array_element_type(
                self.ctx.types,
                first_arg_type,
            )
            .or_else(|| {
                tsz_solver::operations::get_iterator_info(self.ctx.types, first_arg_type, false)
                    .map(|info| info.yield_type)
            });
            let Some(element_type) = element_type else {
                return source;
            };
            if matches!(element_type, TypeId::ERROR | TypeId::UNKNOWN) {
                return source;
            }

            // This recovery exists to make an *unknown-ish* array source more
            // specific (e.g. `unknown[]` -> the argument's real element type).
            // It must never *degrade* an already-concrete element type down to
            // `any`: a generic mapping call such as
            // `stringifyPair<T extends readonly [any, any]>(arr: T): { [K in keyof T]: string }`
            // produces a concrete `string[]` whose display tsc preserves
            // verbatim even though the call argument is `any`. Recovering to
            // `any[]` there would rewrite the `TS2322` headline tsc emits.
            // (#14966, mappedTypeWithAny.ts)
            let widened = self.widen_type_for_display(element_type);
            if widened == TypeId::ANY
                && !matches!(
                    source_element,
                    TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR
                )
            {
                return source;
            }

            let recovered = diagnostic_query::display_array_type(self.ctx.types, widened);
            if recovered != source {
                return recovered;
            }
        }

        source
    }

    /// Report a type not assignable error with detailed elaboration.
    ///
    /// This method uses the solver's "explain" API to determine WHY the types
    /// are incompatible (e.g., missing property, incompatible property types,
    /// etc.) and produces a richer diagnostic with that information.
    ///
    /// **Architecture Note**: This follows the "Check Fast, Explain Slow" pattern.
    /// The `is_assignable_to` check is fast (boolean). This explain call is slower
    /// but produces better error messages. Only call this after a failed check.
    pub fn error_type_not_assignable_with_reason_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        if self
            .assignability_reason_relation_outcome(source, target)
            .related
            || self.is_nested_same_wrapper_application_assignment(source, target)
            || self.type_contains_invalid_mapped_key_type(target)
            || crate::query_boundaries::assignability::optional_mapped_type_adds_implicit_undefined(
                self.ctx.types,
                &self.ctx,
                target,
            )
        {
            return;
        }
        self.diagnose_assignment_failure(source, target, idx);
    }

    /// Report a type not assignable error with detailed elaboration, preserving
    /// the provided anchor exactly instead of walking to an assignment anchor.
    pub fn error_type_not_assignable_with_reason_at_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        if self
            .assignability_reason_relation_outcome(source, target)
            .related
            || self.is_nested_same_wrapper_application_assignment(source, target)
            || self.type_contains_invalid_mapped_key_type(target)
            || crate::query_boundaries::assignability::optional_mapped_type_adds_implicit_undefined(
                self.ctx.types,
                &self.ctx,
                target,
            )
        {
            return;
        }
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
    }

    /// Report a type not assignable error using pre-computed display types.
    /// This is used for callback return type errors where we want to show the full
    /// function types in the error message (e.g., "Type '() => string' is not assignable
    /// to type '{ (): number; (i: number): number; }'") instead of just the return types.
    pub(crate) fn error_type_not_assignable_at_with_display_types(
        &mut self,
        source_for_display: TypeId,
        target_for_display: TypeId,
        anchor_idx: NodeIndex,
    ) {
        self.error_type_not_assignable_at_with_display_types_impl(
            source_for_display,
            target_for_display,
            anchor_idx,
            false,
        );
    }

    /// Like `error_type_not_assignable_at_with_display_types`, but renders the
    /// target VERBATIM via `format_type_diagnostic` — preserving any type-alias
    /// name and `| null | undefined` members instead of routing through the
    /// `AssignmentTarget` role, which strips nullish and expands aliases.
    ///
    /// `tsc` applies that stripping only to ordinary write-position props; for the
    /// JSX framework special attributes (`key`/`ref`) it elaborates the full
    /// declared apparent type from the merged `IntrinsicAttributes` /
    /// `IntrinsicClassAttributes` object (`Key | null | undefined`,
    /// `LegacyRef<HTMLDivElement> | undefined`). The source keeps the same
    /// role-based rendering as the standard emitter so its widening is unchanged.
    pub(crate) fn error_type_not_assignable_at_with_verbatim_target(
        &mut self,
        source_for_display: TypeId,
        target_for_display: TypeId,
        anchor_idx: NodeIndex,
    ) {
        self.error_type_not_assignable_at_with_display_types_impl(
            source_for_display,
            target_for_display,
            anchor_idx,
            true,
        );
    }

    /// Shared body for the `..._with_display_types` and `..._with_verbatim_target`
    /// emitters. They differ only in how the TARGET is rendered: the role-based
    /// path strips nullish / expands aliases (tsc's ordinary write-position
    /// display), while `verbatim_target` keeps the declared form via
    /// `format_type_diagnostic` (tsc's framework special-attribute display).
    fn error_type_not_assignable_at_with_display_types_impl(
        &mut self,
        source_for_display: TypeId,
        target_for_display: TypeId,
        anchor_idx: NodeIndex,
        verbatim_target: bool,
    ) {
        let (start, length) = self
            .resolve_diagnostic_anchor(
                anchor_idx,
                super::fingerprint_policy::DiagnosticAnchorKind::Exact,
            )
            .map(|anchor| (anchor.start, anchor.length))
            .unwrap_or_else(|| {
                let (pos, end) = self.get_node_span(anchor_idx).unwrap_or((0, 0));
                self.normalized_anchor_span(anchor_idx, pos, end.saturating_sub(pos))
            });
        let source_is_function_like = crate::query_boundaries::diagnostics::callable_shape_for_type(
            self.ctx.types,
            source_for_display,
        )
        .is_some()
            || crate::query_boundaries::diagnostics::function_shape_for_type(
                self.ctx.types,
                source_for_display,
            )
            .is_some();
        let target_is_function_like = crate::query_boundaries::diagnostics::callable_shape_for_type(
            self.ctx.types,
            target_for_display,
        )
        .is_some()
            || crate::query_boundaries::diagnostics::function_shape_for_type(
                self.ctx.types,
                target_for_display,
            )
            .is_some();
        let function_like = source_is_function_like || target_is_function_like;
        let mut source_str = if function_like {
            self.format_type_diagnostic(source_for_display)
        } else {
            self.format_type_for_diagnostic_role(
                source_for_display,
                DiagnosticTypeDisplayRole::AssignmentSource {
                    target: target_for_display,
                    anchor_idx,
                },
            )
        };
        let target_str = if verbatim_target || function_like {
            self.format_type_diagnostic(target_for_display)
        } else {
            self.format_type_for_diagnostic_role(
                target_for_display,
                DiagnosticTypeDisplayRole::AssignmentTarget {
                    source: source_for_display,
                    anchor_idx,
                },
            )
        };
        if let Some(display) = self.declared_generic_alias_source_display_for_target_display(
            anchor_idx,
            source_for_display,
            &source_str,
            &target_str,
        ) {
            source_str = display;
        }
        // TS2820: tsc upgrades a bare TS2322 to a "Did you mean" spelling
        // suggestion when the source is a near-miss string literal for one of
        // the target's string-literal members. The simple `TypeMismatch` /
        // `LiteralTypeMismatch` / `IntrinsicTypeMismatch` reasons route here
        // (not through `render_failure_reason`), so this emitter must run the
        // same suggestion scan or the suggestion is silently dropped — e.g.
        // when the target reduces to a literal union from a type alias,
        // distributive conditional, or template-`infer` capture. The source and
        // target displays are identical to the TS2322 form; only the suggestion
        // clause is appended, matching tsc's TS2820 wording.
        let spelling_suggestion = self.find_string_literal_spelling_suggestion_reduced(
            source_for_display,
            target_for_display,
        );
        let same_surface_distinct_decl_binders = || {
            crate::query_boundaries::assignability::
                have_same_surface_distinct_decl_scoped_free_type_parameters(
                    self.ctx.types,
                    &self.ctx,
                    source_for_display,
                    target_for_display,
                )
        };
        let (message, code) = match spelling_suggestion {
            Some(suggestion) => (
                crate::diagnostics::format_message(
                    crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                    &[&source_str, &target_str, &suggestion],
                ),
                crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
            ),
            None if source_str == target_str && same_surface_distinct_decl_binders() => (
                crate::diagnostics::format_message(
                    crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
                    &[&source_str, &target_str],
                ),
                crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
            ),
            None => (
                crate::diagnostics::format_message(
                    crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                ),
                crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            ),
        };
        self.ctx
            .push_diagnostic(crate::diagnostics::Diagnostic::error(
                self.ctx.file_name.clone(),
                start,
                length,
                message,
                code,
            ));
    }

    /// Like `error_type_not_assignable_at_with_display_types` but always uses
    /// `format_type_diagnostic_widened` so that tsc-compatible widened types
    /// (e.g. `{ x: string }` not `{ x: "y" }`) appear in the message text.
    /// Used for JSX callback-prop mismatches where tsc shows widened forms.
    pub(crate) fn error_type_not_assignable_at_with_display_types_widened(
        &mut self,
        source_for_display: TypeId,
        target_for_display: TypeId,
        anchor_idx: NodeIndex,
    ) {
        let (start, length) = self
            .resolve_diagnostic_anchor(
                anchor_idx,
                super::fingerprint_policy::DiagnosticAnchorKind::Exact,
            )
            .map(|anchor| (anchor.start, anchor.length))
            .unwrap_or_else(|| {
                let (pos, end) = self.get_node_span(anchor_idx).unwrap_or((0, 0));
                self.normalized_anchor_span(anchor_idx, pos, end.saturating_sub(pos))
            });
        let source_str = self.format_type_diagnostic_widened(source_for_display);
        let target_str = self.format_type_diagnostic_widened(target_for_display);
        let message = crate::diagnostics::format_message(
            crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        self.ctx
            .push_diagnostic(crate::diagnostics::Diagnostic::error(
                self.ctx.file_name.clone(),
                start,
                length,
                message,
                crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            ));
    }

    pub(crate) fn error_type_not_assignable_at_with_widened_source_display(
        &mut self,
        source_for_display: TypeId,
        target_for_display: TypeId,
        anchor_idx: NodeIndex,
    ) {
        let (start, length) = self
            .resolve_diagnostic_anchor(
                anchor_idx,
                super::fingerprint_policy::DiagnosticAnchorKind::Exact,
            )
            .map(|anchor| (anchor.start, anchor.length))
            .unwrap_or_else(|| {
                let (pos, end) = self.get_node_span(anchor_idx).unwrap_or((0, 0));
                self.normalized_anchor_span(anchor_idx, pos, end.saturating_sub(pos))
            });
        let source_str = self.format_type_diagnostic_widened(source_for_display);
        let target_str = self.format_type_for_diagnostic_role(
            target_for_display,
            DiagnosticTypeDisplayRole::AssignmentTarget {
                source: source_for_display,
                anchor_idx,
            },
        );
        let message = crate::diagnostics::format_message(
            crate::diagnostics::diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        self.ctx
            .push_diagnostic(crate::diagnostics::Diagnostic::error(
                self.ctx.file_name.clone(),
                start,
                length,
                message,
                crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            ));
    }

    /// Report a type not assignable error using a pre-computed failure reason.
    /// This renders the failure reason with the provided display types and pushes the diagnostic.
    pub(crate) fn error_type_not_assignable_with_reason_and_display(
        &mut self,
        source_for_display: TypeId,
        target_for_display: TypeId,
        reason: &tsz_solver::SubtypeFailureReason,
        anchor_idx: NodeIndex,
    ) {
        let diag = self.render_failure_reason(
            reason,
            source_for_display,
            target_for_display,
            anchor_idx,
            0,
        );
        self.ctx.push_diagnostic(diag);
    }

    /// Report constructor accessibility mismatch error.
    pub(crate) fn error_constructor_accessibility_not_assignable(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_level: Option<MemberAccessLevel>,
        target_level: Option<MemberAccessLevel>,
        idx: NodeIndex,
    ) {
        let source_type = self.format_type_diagnostic(source);
        let target_type = self.format_type_diagnostic(target);
        let message = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_type, &target_type],
        );
        let detail = format!(
            "Cannot assign a '{}' constructor type to a '{}' constructor type.",
            Self::constructor_access_name(source_level),
            Self::constructor_access_name(target_level),
        );

        // Build related info referencing the anchor span — since we don't know
        // the span yet, use a placeholder (0, 0) and let emit_render_request
        // fill it in via the anchor. Actually, the related info needs the span.
        // Resolve anchor first to get the span for the related item.
        let Some(anchor) = self.resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::Exact) else {
            return;
        };

        let related = vec![crate::diagnostics::DiagnosticRelatedInformation {
            category: crate::diagnostics::DiagnosticCategory::Error,
            code: diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            file: self.ctx.file_name.clone(),
            start: anchor.start,
            length: anchor.length,
            message_text: detail,
            depth: 0,
            kind: crate::diagnostics::RelatedInformationKind::ChainLink,
        }];

        self.emit_render_request_at_anchor(
            anchor,
            DiagnosticRenderRequest::with_related(
                DiagnosticAnchorKind::Exact,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                message,
                related,
                RelatedInformationPolicy::ELABORATION,
            ),
        );
    }

    pub(super) fn anchor_target_has_intersection_annotation(&self, anchor_idx: NodeIndex) -> bool {
        self.anchor_target_intersection_check_inner(anchor_idx)
            .unwrap_or(false)
    }

    pub(super) fn anchor_jsdoc_type_tag_targets_intersection_alias(
        &self,
        anchor_idx: NodeIndex,
    ) -> bool {
        self.anchor_jsdoc_type_tag_targets_intersection_alias_inner(anchor_idx)
            .unwrap_or(false)
    }

    fn anchor_jsdoc_type_tag_targets_intersection_alias_inner(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<bool> {
        let sf = self.source_file_data_for_node(anchor_idx)?;
        let source_text = sf.text.to_string();
        let comments = sf.comments.clone();
        let jsdoc = self.try_jsdoc_with_ancestor_walk(anchor_idx, &comments, &source_text)?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?;
        let base_name = if let Some(angle_idx) = Self::find_top_level_char(type_expr, '<') {
            type_expr[..angle_idx].trim()
        } else {
            type_expr.trim()
        };
        if base_name.is_empty() {
            return Some(false);
        }

        for comment in &comments {
            if !tsz_common::comments::is_jsdoc_comment(comment, &source_text) {
                continue;
            }
            let content = tsz_common::comments::get_jsdoc_content(comment, &source_text);
            for (name, typedef_info) in Self::parse_jsdoc_typedefs(&content) {
                if name != base_name {
                    continue;
                }
                let Some(base_type) = typedef_info.base_type.as_deref() else {
                    continue;
                };
                if Self::split_top_level_binary(base_type, '&').is_some() {
                    return Some(true);
                }
            }
        }

        Some(false)
    }

    /// Inner helper returning `Option` so we can use `?` for early returns.
    fn anchor_target_intersection_check_inner(&self, anchor_idx: NodeIndex) -> Option<bool> {
        use tsz_parser::parser::syntax_kind_ext;

        let anchor_node = self.ctx.arena.get(anchor_idx)?;

        // Walk from anchor to the assignment target identifier
        let target_ident_idx = if anchor_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
            let expr_stmt = self.ctx.arena.get_expression_statement(anchor_node)?;
            let expr_node = self.ctx.arena.get(expr_stmt.expression)?;
            if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                let binary = self.ctx.arena.get_binary_expr(expr_node)?;
                binary.left
            } else {
                return Some(false);
            }
        } else {
            return Some(false);
        };

        // Check if the target is an identifier
        let ident_node = self.ctx.arena.get(target_ident_idx)?;
        if ident_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return Some(false);
        }

        // Resolve identifier to symbol
        let sym_id = self.resolve_identifier_symbol(target_ident_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        // Get value declaration
        let decl_node = self.ctx.arena.get(symbol.value_declaration)?;

        // Check if it's a variable declaration with an intersection type annotation
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
            if var_decl.type_annotation.is_some() {
                let type_node = self.ctx.arena.get(var_decl.type_annotation)?;
                return Some(type_node.kind == syntax_kind_ext::INTERSECTION_TYPE);
            }
        }

        Some(false)
    }

    /// Whether the assignment target's *written* type annotation denotes an
    /// intersection — directly (`A & B`), through parentheses, or through a
    /// type-alias reference whose body resolves (recursively) to an
    /// intersection (`type W = A & B; const x: W = ...`).
    ///
    /// This is a purely syntactic check over annotation NODES. tsz eagerly
    /// merges concrete object-intersections into a single object that interns
    /// identically to a plain object literal of the same shape, so the merged
    /// type cannot be distinguished from a plain object by its `TypeId` (or its
    /// display alias) alone. The annotation's syntactic provenance is the only
    /// reliable signal that tsc would report the mismatch member-by-member.
    pub(super) fn target_annotation_denotes_intersection(&self, anchor_idx: NodeIndex) -> bool {
        self.target_annotation_node(anchor_idx)
            .is_some_and(|node| self.type_node_denotes_intersection(node, 0))
    }

    /// Whether the target annotation is *written* as an intersection literal
    /// (`A & B`, possibly parenthesized) rather than a type-alias reference that
    /// resolves to one. tsc echoes the inline `&` form for the former
    /// (`{ g: number; } & { h: string; }`) but the alias name for the latter
    /// (`PlainWrap`), so the recovered-intersection display chooses accordingly.
    pub(super) fn target_annotation_is_intersection_literal(&self, anchor_idx: NodeIndex) -> bool {
        self.target_annotation_node(anchor_idx)
            .is_some_and(|node| self.type_node_is_intersection_literal(node, 0))
    }

    fn type_node_is_intersection_literal(&self, type_node_idx: NodeIndex, depth: u32) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        if depth > 16 {
            return false;
        }
        let Some(node) = self.ctx.arena.get(type_node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::INTERSECTION_TYPE {
            return true;
        }
        if let Some(inner) = self.parenthesized_inner_type_node(node) {
            return self.type_node_is_intersection_literal(inner, depth + 1);
        }
        false
    }

    /// Inner type node of a `PARENTHESIZED_TYPE`, if `node` is one.
    fn parenthesized_inner_type_node(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            self.ctx.arena.get_wrapped_type(node).map(|w| w.type_node)
        } else {
            None
        }
    }

    /// Type-annotation node of a binding declaration (variable / parameter /
    /// property), if it carries one.
    fn declaration_type_annotation_node(
        &self,
        decl: &tsz_parser::parser::node::Node,
    ) -> Option<NodeIndex> {
        let annotation = if let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl) {
            var_decl.type_annotation
        } else if let Some(param) = self.ctx.arena.get_parameter(decl) {
            param.type_annotation
        } else {
            let prop = self.ctx.arena.get_property_decl(decl)?;
            prop.type_annotation
        };
        annotation.is_some().then_some(annotation)
    }

    /// Resolve the assignment target's type-annotation node from an anchor.
    pub(super) fn target_annotation_node(&self, anchor_idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        // The anchor may be the annotated declaration itself or, more commonly, a
        // descendant such as the initializer expression. Walk up the ancestor
        // chain (bounded) until an annotated binding or assignment is found,
        // stopping at function/statement boundaries so an unrelated outer
        // annotation is never attributed to this assignment.
        let mut current = anchor_idx;
        let mut guard = 0u32;
        while current.is_some() {
            guard += 1;
            if guard > 32 {
                break;
            }
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                || node.kind == syntax_kind_ext::PARAMETER
                || node.kind == syntax_kind_ext::PROPERTY_DECLARATION
            {
                return self.declaration_type_annotation_node(node);
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                let binary = self.ctx.arena.get_binary_expr(node)?;
                return self.declared_assignment_type_annotation_node(binary.left);
            }
            if node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
                let expr_stmt = self.ctx.arena.get_expression_statement(node)?;
                let expr_node = self.ctx.arena.get(expr_stmt.expression)?;
                if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                    let binary = self.ctx.arena.get_binary_expr(expr_node)?;
                    return self.declared_assignment_type_annotation_node(binary.left);
                }
                return None;
            }
            // A `return <expr>` is checked against the enclosing function's
            // declared return type. When that type is written as an
            // intersection, the recovered-intersection display must fire here
            // exactly as it does for an annotated variable/parameter/property —
            // otherwise the value-position mismatch falls back to tsz's
            // eagerly-merged single-object shape (a type the user never wrote).
            if node.kind == syntax_kind_ext::RETURN_STATEMENT {
                return self.enclosing_function_return_annotation_node(current);
            }
            if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || node.kind == syntax_kind_ext::ARROW_FUNCTION
                || node.kind == syntax_kind_ext::METHOD_DECLARATION
                || node.kind == syntax_kind_ext::BLOCK
                || node.kind == syntax_kind_ext::SOURCE_FILE
            {
                return None;
            }
            let parent = self.ctx.arena.get_extended(current).map(|ext| ext.parent)?;
            if let Some(annotation) = self.call_argument_annotation_node(current, parent) {
                return Some(annotation);
            }
            current = parent;
        }
        None
    }

    /// Type-annotation node of the declared parameter a call argument is
    /// checked against, when `current` is one of `parent`'s own arguments and
    /// `parent` is a call/new expression.
    ///
    /// Deliberately narrow: the callee must be a plain identifier resolving to
    /// a symbol with exactly one declaration (no overloads — which specific
    /// overload matched is a resolved-signature fact this syntactic walk does
    /// not have), that declaration must be an ordinary `function` declared in
    /// the file being checked (not an arrow/method/imported/ambient binding),
    /// and the matched parameter must not be a rest parameter (a positional
    /// match against `...args` would name the wrong element type). Any of
    /// those declines rather than guessing, leaving output exactly as it was.
    fn call_argument_annotation_node(
        &self,
        current: NodeIndex,
        parent: NodeIndex,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;

        let parent_node = self.ctx.arena.get(parent)?;
        if parent_node.kind != syntax_kind_ext::CALL_EXPRESSION
            && parent_node.kind != syntax_kind_ext::NEW_EXPRESSION
        {
            return None;
        }
        let call_data = self.ctx.arena.get_call_expr(parent_node)?;
        let args = call_data.arguments.as_ref()?;
        let position = args.nodes.iter().position(|&arg| arg == current)?;

        let callee_symbol = self.resolve_identifier_symbol(call_data.expression)?;
        let symbol = self.ctx.binder.get_symbol(callee_symbol)?;
        let locations: Vec<tsz_binder::StableLocation> =
            std::iter::once(symbol.stable_value_declaration)
                .chain(symbol.stable_declarations.iter().copied())
                .filter(tsz_binder::StableLocation::is_known)
                .collect();
        let mut decl_idx = None;
        for location in locations {
            let (idx, arena) = self.ctx.node_at_stable_location(location)?;
            if !std::ptr::eq(arena, self.ctx.arena) {
                continue;
            }
            match decl_idx {
                None => decl_idx = Some(idx),
                // A second distinct declaration means an overloaded function;
                // which overload resolved the call is not derivable here.
                Some(existing) if existing != idx => return None,
                Some(_) => {}
            }
        }
        let decl_idx = decl_idx?;
        let decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind != syntax_kind_ext::FUNCTION_DECLARATION {
            return None;
        }
        let func = self.ctx.arena.get_function(decl_node)?;
        let param_idx = *func.parameters.nodes.get(position)?;
        let param_node = self.ctx.arena.get(param_idx)?;
        let param_data = self.ctx.arena.get_parameter(param_node)?;
        if param_data.dot_dot_dot_token {
            return None;
        }
        self.declaration_type_annotation_node(param_node)
    }

    /// Return-type annotation node of the function-like that directly encloses
    /// `return_stmt_idx` (a `RETURN_STATEMENT`), or `None` when the enclosing
    /// function has no written return type.
    ///
    /// A `return` always belongs to its innermost enclosing function, so the
    /// nearest function-like ancestor (via the shared `find_enclosing_function`
    /// walk) owns the contextual return type. Constructors/accessors carry no
    /// `FunctionData`/`MethodDeclData` return annotation, so they fall through to
    /// `None` (no recovery), matching the pre-existing behavior for those
    /// positions. Used so the recovered-intersection diagnostic display fires
    /// for `return` positions the same way it already does for annotated
    /// bindings.
    pub(crate) fn enclosing_function_return_annotation_node(
        &self,
        return_stmt_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        let function_idx = self.find_enclosing_function(return_stmt_idx)?;
        let node = self.ctx.arena.get(function_idx)?;
        let annotation = if node.kind == syntax_kind_ext::METHOD_DECLARATION {
            self.ctx.arena.get_method_decl(node)?.type_annotation
        } else {
            self.ctx.arena.get_function(node)?.type_annotation
        };
        annotation.into_option()
    }

    /// Recursively decide whether a type-annotation node denotes an
    /// intersection, unwrapping parentheses and following type-alias references.
    fn type_node_denotes_intersection(&self, type_node_idx: NodeIndex, depth: u32) -> bool {
        use crate::symbol_resolver::TypeSymbolResolution;
        use tsz_parser::parser::syntax_kind_ext;
        if depth > 16 {
            return false;
        }
        let Some(node) = self.ctx.arena.get(type_node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::INTERSECTION_TYPE {
            return true;
        }
        if let Some(inner) = self.parenthesized_inner_type_node(node) {
            return self.type_node_denotes_intersection(inner, depth + 1);
        }
        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
                return false;
            };
            let type_name = type_ref.type_name;
            let sym_id = match self.resolve_qualified_symbol_in_type_position(type_name) {
                TypeSymbolResolution::Type(sym_id) | TypeSymbolResolution::ValueOnly(sym_id) => {
                    sym_id
                }
                TypeSymbolResolution::NotFound => return false,
            };
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                return false;
            };
            let mut declarations = Vec::new();
            if symbol.value_declaration.is_some() {
                declarations.push(symbol.value_declaration);
            }
            declarations.extend(symbol.declarations.iter().copied());
            return declarations.into_iter().any(|decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|decl| self.ctx.arena.get_type_alias(decl))
                    .is_some_and(|alias| {
                        self.type_node_denotes_intersection(alias.type_node, depth + 1)
                    })
            });
        }
        false
    }

    pub(crate) fn missing_required_properties_from_index_signature_source(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<Vec<tsz_common::interner::Atom>> {
        if crate::query_boundaries::diagnostics::is_type_parameter_like(self.ctx.types, source) {
            return None;
        }

        let source_env_evaluated = self.evaluate_type_with_env(source);
        let source_evaluated = self.evaluate_type_for_assignability(source);
        let target_env_evaluated = self.evaluate_type_with_env(target);
        let target_evaluated = self.evaluate_type_for_assignability(target);
        let source_resolved = self.resolve_type_for_property_access(source);
        let source_judged = self.judge_evaluate(source_resolved);
        let source_candidates =
            crate::query_boundaries::assignability::alias_application_surface_candidates(
                self.ctx.types,
                &self.ctx,
                &[
                    source,
                    source_resolved,
                    source_judged,
                    source_env_evaluated,
                    source_evaluated,
                ],
            );

        let source_has_index = source_candidates.iter().copied().any(|candidate| {
            crate::query_boundaries::index_signature::has_string_or_number_index_signature(
                self.ctx.types,
                candidate,
            )
        });
        if !source_has_index {
            return None;
        }

        let target_with_shape = {
            let direct = target;
            let resolved = self.resolve_type_for_property_access(direct);
            let judged = self.judge_evaluate(resolved);
            [
                direct,
                resolved,
                judged,
                target_env_evaluated,
                target_evaluated,
            ]
            .into_iter()
            .find(|candidate| {
                crate::query_boundaries::diagnostics::object_shape_for_type(
                    self.ctx.types,
                    *candidate,
                )
                .is_some()
            })?
        };

        let source_shape = {
            source_candidates.iter().copied().find_map(|candidate| {
                crate::query_boundaries::diagnostics::object_shape_for_type(
                    self.ctx.types,
                    candidate,
                )
            })
        };
        let target_shape = crate::query_boundaries::diagnostics::object_shape_for_type(
            self.ctx.types,
            target_with_shape,
        )?;

        // Check if target has index signature using the resolver (more reliable than shape check)
        let target_has_index = [target, target_env_evaluated, target_evaluated]
            .into_iter()
            .any(|candidate| {
                crate::query_boundaries::index_signature::has_string_or_number_index_signature(
                    self.ctx.types,
                    candidate,
                )
            });

        if target_has_index
            || target_shape.string_index.is_some()
            || target_shape.number_index.is_some()
        {
            return None;
        }

        // tsc lists missing properties in source declaration order. The
        // target_shape's `properties` Vec is sorted by Atom name during
        // shape interning (for hash stability), so we cannot rely on Vec
        // position. Each PropertyInfo carries `declaration_order` (1-based,
        // synthesized members get 0 from the interner fixup), so collect
        // (declaration_order, name) and sort by it.
        let mut missing_with_order: Vec<(u32, tsz_common::interner::Atom)> = target_shape
            .properties
            .iter()
            .filter(|prop| !prop.optional)
            .filter(|prop| {
                !source_shape.as_ref().is_some_and(|shape| {
                    shape
                        .properties
                        .iter()
                        .any(|source_prop| source_prop.name == prop.name)
                })
            })
            .map(|prop| (prop.declaration_order, prop.name))
            .collect();
        missing_with_order.sort_by_key(|(order, _)| *order);
        let missing: Vec<_> = missing_with_order
            .into_iter()
            .map(|(_, name)| name)
            .collect();

        (!missing.is_empty()).then_some(missing)
    }

    pub(super) fn private_or_protected_brand_backing_member_display(
        &self,
        target_type: TypeId,
        required_property_name: Option<tsz_common::interner::Atom>,
    ) -> Option<(String, String, tsz_solver::Visibility)> {
        let find_member = |props: &[tsz_solver::PropertyInfo]| {
            props.iter().find_map(|prop| {
                let prop_name = self.ctx.types.resolve_atom(prop.name);
                if tsz_solver::utils::is_synthetic_private_brand_name(&prop_name)
                    || required_property_name.is_some_and(|required| prop.name != required)
                    || prop.visibility == tsz_solver::Visibility::Public
                {
                    return None;
                }
                let owner_name = prop
                    .parent_id
                    .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                    .map(|sym| sym.escaped_name.clone())
                    .unwrap_or_else(|| self.format_type_diagnostic(target_type));
                Some((prop_name, owner_name, prop.visibility))
            })
        };

        crate::query_boundaries::diagnostics::object_shape_for_type(self.ctx.types, target_type)
            .and_then(|shape| find_member(&shape.properties))
            .or_else(|| {
                crate::query_boundaries::diagnostics::callable_shape_for_type(
                    self.ctx.types,
                    target_type,
                )
                .and_then(|shape| find_member(&shape.properties))
            })
    }

    /// Resolve the *declaring class* name of `property_name` on `ty`.
    ///
    /// Evaluates through instantiated (`Application`) forms first, then reads
    /// the member's owning class symbol, so an instantiated generic
    /// (`G<number>`) reports its bare class name `G`. That is the spelling
    /// `tsc` uses inside a nominal-member elaboration — the top-level
    /// assignability line still shows the instantiated `G<number>`, but the
    /// `refers to a different member` detail names the uninstantiated class.
    pub(super) fn member_declaring_class_name(
        &mut self,
        ty: TypeId,
        property_name: tsz_common::interner::Atom,
    ) -> Option<String> {
        let evaluated = self.evaluate_type_with_env(ty);
        let info = self.property_info_for_display(evaluated, property_name)?;
        let sym = self.ctx.binder.get_symbol(info.parent_id?)?;
        Some(sym.escaped_name.clone())
    }

    /// Build the TS18015 elaboration for an ES private identifier (`#name`)
    /// nominal mismatch, naming each side's declaring class (uninstantiated).
    ///
    /// `fallback_source` / `fallback_target` supply the top-level type display
    /// for the rare case a side has no resolvable owning class symbol (e.g. an
    /// anonymous structural source), preserving the historical spelling there.
    pub(super) fn private_identifier_mismatch_detail(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
        property_name: tsz_common::interner::Atom,
        fallback_source: &str,
        fallback_target: &str,
    ) -> String {
        let prop_name = self.ctx.types.resolve_atom_ref(property_name).to_string();
        let source_owner = self
            .member_declaring_class_name(source_type, property_name)
            .unwrap_or_else(|| fallback_source.to_string());
        let target_owner = self
            .member_declaring_class_name(target_type, property_name)
            .unwrap_or_else(|| fallback_target.to_string());
        format_message(
            diagnostic_messages::PROPERTY_IN_TYPE_REFERS_TO_A_DIFFERENT_MEMBER_THAT_CANNOT_BE_ACCESSED_FROM_WITHI,
            &[&prop_name, &source_owner, &target_owner],
        )
    }

    pub(super) fn nominal_mismatch_detail(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
        property_name: tsz_common::interner::Atom,
    ) -> Option<String> {
        // Evaluate through instantiated (`Application`) forms so a generic
        // modifier-`private` pair (`class A<T> { private s } … : B<U> = new A()`)
        // resolves the member and its visibility rather than dropping the
        // elaboration.
        let source_type = self.evaluate_type_with_env(source_type);
        let target_type = self.evaluate_type_with_env(target_type);
        let source_prop = self.property_info_for_display(source_type, property_name)?;
        let target_prop = self.property_info_for_display(target_type, property_name)?;
        if source_prop.visibility != target_prop.visibility
            || target_prop.visibility == tsz_solver::Visibility::Public
        {
            return None;
        }
        let prop_name = self.ctx.types.resolve_atom_ref(property_name);
        match target_prop.visibility {
            tsz_solver::Visibility::Private => Some(format_message(
                diagnostic_messages::TYPES_HAVE_SEPARATE_DECLARATIONS_OF_A_PRIVATE_PROPERTY,
                &[&prop_name],
            )),
            tsz_solver::Visibility::Protected => self.protected_brand_mismatch_error(
                &prop_name,
                source_type,
                target_type,
                source_prop.parent_id,
                target_prop.parent_id,
            ),
            tsz_solver::Visibility::Public => None,
        }
    }

    /// Report TS2720 for a class that `implements` another class carrying
    /// private/protected (nominal) members, driven by the whole-type relation.
    ///
    /// `tsc` runs the structural relation here rather than a member-by-member
    /// walk: the nominal brand is satisfiable only by a subclass that inherits
    /// the declaration, so `class C extends A implements A` relates and stays
    /// silent, while any independent `implements` fails and reports TS2720 with
    /// the specific member elaboration that named the break (missing /
    /// separate-declaration / visibility). The old branch fired an
    /// elaboration-less TS2720 unconditionally, dropping the member line and
    /// over-reporting the assignable extends-the-same-base case (#17216).
    pub(crate) fn report_class_implements_nominal_failure(
        &mut self,
        class_instance_type: TypeId,
        interface_type: TypeId,
        class_this_type: Option<TypeId>,
        class_error_idx: NodeIndex,
        class_name: &str,
        interface_name: &str,
        interface_display_name: &str,
    ) {
        // Substitute the implementing class's `this` into the target before the
        // relation, matching the per-property substitution the member walk does.
        let target_type = crate::query_boundaries::class::maybe_substitute_this_type(
            self.ctx.types,
            interface_type,
            class_this_type,
        );
        let Some(reason) = self
            .analyze_assignability_failure(class_instance_type, target_type)
            .failure_reason
        else {
            return;
        };
        let primary = format!(
            "Class '{class_name}' incorrectly implements class '{interface_name}'. Did you mean to extend '{interface_name}' and inherit its members as a subclass?"
        );
        let full = match self.class_target_nominal_elaboration(
            &reason,
            class_instance_type,
            target_type,
            class_name,
            interface_display_name,
        ) {
            Some(line) => format!("{primary}\n  {line}"),
            None => primary,
        };
        self.error_at_node(
            class_error_idx,
            &full,
            diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER,
        );
    }

    /// The elaboration line tsc nests under a class-target TS2720, naming the
    /// specific member whose absence or nominal (private/protected) identity
    /// broke the whole-type relation between the implementing class and the
    /// class it `implements`. Returns `None` when the structural failure carries
    /// no such per-member line, leaving the bare TS2720 to stand alone.
    ///
    /// The mapping mirrors tsc's own `propertiesRelatedTo` reporting order:
    /// truly-absent members (`MissingProperty`/`MissingProperties`) win over a
    /// present-but-nominally-mismatched one, and a present private/protected
    /// member is rendered through the same helpers every other assignability
    /// site uses (`nominal_mismatch_detail` for the separate-declaration forms,
    /// the shadowed-visibility fallback for a public member masking a non-public
    /// slot).
    pub(crate) fn class_target_nominal_elaboration(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        class_display: &str,
        target_display: &str,
    ) -> Option<String> {
        use tsz_solver::SubtypeFailureReason as Reason;
        match reason {
            Reason::MissingProperty { property_name, .. } => {
                let prop = self.ctx.types.resolve_atom(*property_name);
                Some(format_message(
                    diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                    &[&prop, class_display, target_display],
                ))
            }
            Reason::MissingProperties { property_names, .. } => {
                // Reuse the crate's canonical truncation + message so the
                // >5-only "and N more" rule and enum-key rendering match every
                // other TS2739/TS2740 site (not a hand-rolled >4 threshold).
                let ordered = self.sort_missing_property_names_for_display(target, property_names);
                let (list, more) = self.truncated_missing_property_list(&ordered, target);
                Some(match more {
                    Some(more_count) => format_message(
                        diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                        &[class_display, target_display, &list, &more_count.to_string()],
                    ),
                    None => format_message(
                        diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                        &[class_display, target_display, &list],
                    ),
                })
            }
            Reason::PropertyNominalMismatch { property_name } => self
                .nominal_mismatch_detail(source, target, *property_name)
                .or_else(|| self.class_target_shadowed_member_line(source, target, *property_name)),
            Reason::PropertyVisibilityMismatch { property_name, .. } => {
                self.class_target_shadowed_member_line(source, target, *property_name)
            }
            _ => None,
        }
    }

    /// The `Property 'x' is private in type 'A' but not in type 'C'` (TS2325) or
    /// the protected-brand `Property 'x' is protected but type 'C' is not a class
    /// derived from 'A'` (TS2443) line for a class target whose non-public member
    /// is shadowed by a *differently-visible* same-named member on the
    /// implementing class. `nominal_mismatch_detail` declines this shape because
    /// the two sides disagree on visibility, so there is no shared
    /// separate-declaration story to tell.
    fn class_target_shadowed_member_line(
        &mut self,
        source: TypeId,
        target: TypeId,
        property_name: tsz_common::interner::Atom,
    ) -> Option<String> {
        let target_prop = self.property_info_for_display(target, property_name)?;
        let prop = self.ctx.types.resolve_atom(property_name);
        match target_prop.visibility {
            tsz_solver::Visibility::Private => {
                let owner = target_prop
                    .parent_id
                    .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                    .map(|sym| sym.escaped_name.clone())
                    .unwrap_or_else(|| self.format_type_diagnostic(target));
                let widened = self.widen_type_for_display(source);
                let source_display = self.format_type_diagnostic(widened);
                Some(format_message(
                    diagnostic_messages::PROPERTY_IS_PRIVATE_IN_TYPE_BUT_NOT_IN_TYPE,
                    &[&prop, &owner, &source_display],
                ))
            }
            tsz_solver::Visibility::Protected => {
                let source_parent = self
                    .property_info_for_display(source, property_name)
                    .and_then(|sp| sp.parent_id);
                self.protected_brand_mismatch_error(
                    &prop,
                    source,
                    target,
                    source_parent,
                    target_prop.parent_id,
                )
            }
            tsz_solver::Visibility::Public => None,
        }
    }

    pub(super) fn canonical_array_display_rank(name: &str) -> Option<usize> {
        match name {
            "length" => Some(0),
            "pop" => Some(1),
            "push" => Some(2),
            "concat" => Some(3),
            "join" => Some(4),
            "reverse" => Some(5),
            "shift" => Some(6),
            "slice" => Some(7),
            "sort" => Some(8),
            "splice" => Some(9),
            "unshift" => Some(10),
            "indexOf" => Some(11),
            "lastIndexOf" => Some(12),
            "every" => Some(13),
            "some" => Some(14),
            "forEach" => Some(15),
            "map" => Some(16),
            "filter" => Some(17),
            "reduce" => Some(18),
            "reduceRight" => Some(19),
            _ => None,
        }
    }

    pub(super) fn private_or_protected_assignability_message(
        &self,
        source_str: &str,
        target_str: &str,
        prop_name: &str,
        owner_name: &str,
        visibility: tsz_solver::Visibility,
        source_visibility: Option<tsz_solver::Visibility>,
    ) -> String {
        let source_side = source_visibility
            .filter(|_| !source_str.trim_start().starts_with('{'))
            .map(Self::visibility_name)
            .map(|visibility| format!("{visibility} in type '{source_str}'"))
            .unwrap_or_else(|| format!("not in type '{source_str}'"));
        let detail = match visibility {
            tsz_solver::Visibility::Private => {
                format!(
                    "Property '{prop_name}' is private in type '{owner_name}' but {source_side}."
                )
            }
            tsz_solver::Visibility::Protected => {
                format!(
                    "Property '{prop_name}' is protected in type '{owner_name}' but {source_side}."
                )
            }
            _ => format!(
                "Property '{prop_name}' is not accessible in type '{owner_name}' from type '{source_str}'."
            ),
        };
        format!("Type '{source_str}' is not assignable to type '{target_str}'.\n  {detail}")
    }

    pub(super) const fn visibility_name(visibility: tsz_solver::Visibility) -> &'static str {
        match visibility {
            tsz_solver::Visibility::Private => "private",
            tsz_solver::Visibility::Protected => "protected",
            tsz_solver::Visibility::Public => "public",
        }
    }

    pub(super) fn property_visibility_assignability_message(
        &self,
        source_str: &str,
        target_str: &str,
        prop_name: &str,
        source_visibility: tsz_solver::Visibility,
        target_visibility: tsz_solver::Visibility,
    ) -> String {
        let source_visibility = Self::visibility_name(source_visibility);
        let target_visibility = Self::visibility_name(target_visibility);
        format!(
            "Type '{source_str}' is not assignable to type '{target_str}'.\n  Property '{prop_name}' is {target_visibility} in type '{target_str}' but {source_visibility} in type '{source_str}'."
        )
    }

    pub(super) fn sort_missing_property_names_for_display(
        &mut self,
        target_type: TypeId,
        property_names: &[tsz_common::interner::Atom],
    ) -> Vec<tsz_common::interner::Atom> {
        // Track (declaration_order, shape_index, is_own_property) for each property.
        // `is_own_property` = true when the property's parent_id matches the target
        // type's symbol, meaning it was declared directly on the target type (not
        // inherited). tsc lists own properties before inherited ones in TS2739/TS2741.
        let target_symbol =
            crate::query_boundaries::diagnostics::get_object_symbol(self.ctx.types, target_type);
        let mut property_ranks: FxHashMap<tsz_common::interner::Atom, (u32, usize, bool)> =
            FxHashMap::default();

        let mut collect_ranks = |ty: TypeId, tgt_sym: Option<tsz_binder::SymbolId>| {
            if let Some(shape) =
                crate::query_boundaries::diagnostics::object_shape_for_type(self.ctx.types, ty)
            {
                for (index, prop) in shape.properties.iter().enumerate() {
                    let is_own = tgt_sym.is_some() && prop.parent_id == tgt_sym;
                    property_ranks.entry(prop.name).or_insert((
                        prop.declaration_order,
                        index,
                        is_own,
                    ));
                }
            }
            if let Some(shape) =
                crate::query_boundaries::diagnostics::callable_shape_for_type(self.ctx.types, ty)
            {
                for (index, prop) in shape.properties.iter().enumerate() {
                    let is_own = tgt_sym.is_some() && prop.parent_id == tgt_sym;
                    property_ranks.entry(prop.name).or_insert((
                        prop.declaration_order,
                        index,
                        is_own,
                    ));
                }
            }
        };

        collect_ranks(target_type, target_symbol);
        let resolved = self.resolve_type_for_property_access(target_type);
        if resolved != target_type {
            collect_ranks(resolved, target_symbol);
        }
        let evaluated = self.evaluate_type_for_assignability(target_type);
        if evaluated != target_type && evaluated != resolved {
            collect_ranks(evaluated, target_symbol);
        }
        let array_like_target = matches!(
            query_utils::classify_array_like(self.ctx.types, target_type),
            query_utils::ArrayLikeKind::Array(_)
                | query_utils::ArrayLikeKind::Tuple
                | query_utils::ArrayLikeKind::Readonly(_)
        ) || matches!(
            query_utils::classify_array_like(self.ctx.types, resolved),
            query_utils::ArrayLikeKind::Array(_)
                | query_utils::ArrayLikeKind::Tuple
                | query_utils::ArrayLikeKind::Readonly(_)
        ) || matches!(
            query_utils::classify_array_like(self.ctx.types, evaluated),
            query_utils::ArrayLikeKind::Array(_)
                | query_utils::ArrayLikeKind::Tuple
                | query_utils::ArrayLikeKind::Readonly(_)
        );

        let mut ordered: Vec<(usize, tsz_common::interner::Atom)> =
            property_names.iter().copied().enumerate().collect();
        let named_target = self.named_type_display_name(target_type).is_some();
        let date_target = self.named_type_display_name(target_type).as_deref() == Some("Date");
        ordered.sort_by(|(left_index, left_name), (right_index, right_name)| {
            if array_like_target {
                let left_text = self.ctx.types.resolve_atom_ref(*left_name);
                let right_text = self.ctx.types.resolve_atom_ref(*right_name);
                match (
                    Self::canonical_array_display_rank(&left_text),
                    Self::canonical_array_display_rank(&right_text),
                ) {
                    (Some(left_rank), Some(right_rank)) => {
                        let rank_ord = left_rank.cmp(&right_rank);
                        if rank_ord != std::cmp::Ordering::Equal {
                            return rank_ord;
                        }
                    }
                    (Some(_), None) => return std::cmp::Ordering::Less,
                    (None, Some(_)) => return std::cmp::Ordering::Greater,
                    (None, None) => {}
                }
            }

            if date_target {
                let date_rank = |name: &str| match name {
                    "toDateString" => Some(0_u8),
                    "toTimeString" => Some(1),
                    "toLocaleDateString" => Some(2),
                    "toLocaleTimeString" => Some(3),
                    _ => None,
                };
                let left_text = self.ctx.types.resolve_atom_ref(*left_name);
                let right_text = self.ctx.types.resolve_atom_ref(*right_name);
                match (date_rank(&left_text), date_rank(&right_text)) {
                    (Some(left_rank), Some(right_rank)) => {
                        let rank_ord = left_rank.cmp(&right_rank);
                        if rank_ord != std::cmp::Ordering::Equal {
                            return rank_ord;
                        }
                    }
                    (Some(_), None) => return std::cmp::Ordering::Less,
                    (None, Some(_)) => return std::cmp::Ordering::Greater,
                    (None, None) => {}
                }
            }

            if named_target {
                let left_text = self.ctx.types.resolve_atom_ref(*left_name);
                let right_text = self.ctx.types.resolve_atom_ref(*right_name);
                match (
                    is_object_prototype_method(&left_text),
                    is_object_prototype_method(&right_text),
                ) {
                    (false, true) => return std::cmp::Ordering::Less,
                    (true, false) => return std::cmp::Ordering::Greater,
                    _ => {}
                }
            }

            let left_rank = property_ranks.get(left_name).copied();
            let right_rank = property_ranks.get(right_name).copied();
            match (left_rank, right_rank) {
                (
                    Some((left_order, left_pos, left_own)),
                    Some((right_order, right_pos, right_own)),
                ) => {
                    // Own properties (declared directly on the target type) come
                    // before inherited ones, matching tsc behavior for TS2739/TS2741.
                    match (left_own, right_own) {
                        (true, false) => return std::cmp::Ordering::Less,
                        (false, true) => return std::cmp::Ordering::Greater,
                        // When both are own, tsc lists in source declaration
                        // order. Each PropertyInfo carries a 1-based
                        // `declaration_order` (with synthesized members
                        // assigned a positional fixup at interning time).
                        // The shape's Vec position (`*_pos`) is NOT useful
                        // as the primary key here because the type interner
                        // sorts shape properties by Atom name for hash
                        // stability — see
                        // `tsz_solver::intern::core::constructors::object_with_index`.
                        // Sort by `declaration_order` first, falling back
                        // to `*_pos` then atom name then original index
                        // when declaration_order ties (e.g. two synthesized
                        // members both at order 0).
                        (true, true) => {
                            return left_order
                                .cmp(&right_order)
                                .then_with(|| left_pos.cmp(&right_pos))
                                .then_with(|| left_name.cmp(right_name))
                                .then_with(|| left_index.cmp(right_index));
                        }
                        (false, false) => {}
                    }
                    match (
                        left_order > 0,
                        right_order > 0,
                        left_order.cmp(&right_order),
                        left_pos.cmp(&right_pos),
                    ) {
                        (true, true, std::cmp::Ordering::Equal, pos_ord)
                            if pos_ord != std::cmp::Ordering::Equal =>
                        {
                            pos_ord
                        }
                        (true, true, ord, _) if ord != std::cmp::Ordering::Equal => ord,
                        (true, false, _, _) => std::cmp::Ordering::Less,
                        (false, true, _, _) => std::cmp::Ordering::Greater,
                        _ => left_index
                            .cmp(right_index)
                            .then_with(|| left_name.cmp(right_name)),
                    }
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => left_index
                    .cmp(right_index)
                    .then_with(|| left_name.cmp(right_name)),
            }
        });

        ordered.into_iter().map(|(_, name)| name).collect()
    }

    pub(crate) fn try_report_concrete_remapped_mapped_missing_property(
        &mut self,
        source: TypeId,
        target: TypeId,
        diag_idx: NodeIndex,
    ) -> bool {
        let resolved = self.resolve_lazy_type(source);
        if !crate::query_boundaries::assignability::remapped_mapped_type_has_no_outer_type_params(
            self.ctx.types,
            resolved,
        ) {
            return false;
        }
        let evaluated = self.evaluate_concrete_remapped_mapped_type_with_resolution(resolved);
        if evaluated == resolved
            || self
                .concrete_remapped_mapped_missing_property_relation_outcome(evaluated, target)
                .related
        {
            return false;
        }
        let analysis = self.analyze_assignability_failure(evaluated, target);
        if let Some(reason) = analysis.failure_reason {
            match reason {
                tsz_solver::SubtypeFailureReason::MissingProperty { property_name, .. } => {
                    self.report_concrete_remapped_mapped_missing_property(
                        evaluated,
                        target,
                        property_name,
                        diag_idx,
                    );
                }
                other => {
                    self.error_type_not_assignable_with_reason_and_display(
                        evaluated, target, &other, diag_idx,
                    );
                }
            }
        } else {
            self.error_type_not_assignable_with_reason_at(evaluated, target, diag_idx);
        }
        true
    }

    fn report_concrete_remapped_mapped_missing_property(
        &mut self,
        evaluated_source: TypeId,
        target: TypeId,
        property_name: tsz_common::interner::Atom,
        diag_idx: NodeIndex,
    ) {
        let Some(anchor) = self.resolve_diagnostic_anchor(diag_idx, DiagnosticAnchorKind::Exact)
        else {
            self.error_type_not_assignable_with_reason_at(evaluated_source, target, diag_idx);
            return;
        };
        let prop_name = self.ctx.types.resolve_atom_ref(property_name).to_string();
        let source_str = self.format_type_for_assignability_message(evaluated_source);
        let target_str = self.format_type_for_diagnostic_role(
            target,
            DiagnosticTypeDisplayRole::AssignmentTarget {
                source: evaluated_source,
                anchor_idx: diag_idx,
            },
        );
        let message = format_message(
            diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            &[&prop_name, &source_str, &target_str],
        );
        self.ctx
            .push_diagnostic(crate::diagnostics::Diagnostic::error(
                self.ctx.file_name.clone(),
                anchor.start,
                anchor.length,
                message,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            ));
    }
    /// Whether a top-level missing-property failure may be PROMOTED to the
    /// primary diagnostic (TS2741/TS2739/TS2740) in place of a context head
    /// (TS2345/TS2344/TS1360). Owns the `reportUnmatchedProperty`
    /// preconditions the renderer does not: tuple targets fail through the
    /// arity machinery in tsc (the generic head stays — `FooIterator` vs
    /// `[any, ...any[]]` keeps TS2345), and error-poisoned sources (e.g. a
    /// class whose heritage failed TS2507) never elaborate their members.
    pub(crate) fn missing_property_head_promotion_applies(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        crate::query_boundaries::diagnostics::tuple_elements(self.ctx.types, target).is_none()
            && !self.target_is_multi_member_object_union(target)
            && !self.target_keeps_generic_head_for_promotion(target)
            && !crate::query_boundaries::diagnostics::contains_error_type(self.ctx.types, source)
            && !self.source_class_heritage_chain_errored(source)
    }

    /// Targets for which tsc keeps the generic TS2345/TS2344 head even when
    /// the failure is a sole missing property: a polymorphic `this` type and
    /// an intersection (including a type parameter whose constraint is an
    /// intersection — `T extends Named & Aged` nests the missing-property
    /// line instead of promoting it).
    fn target_keeps_generic_head_for_promotion(&mut self, target: TypeId) -> bool {
        if crate::query_boundaries::diagnostics::is_this_type(self.ctx.types, target) {
            return true;
        }
        let constrained =
            crate::query_boundaries::diagnostics::type_parameter_constraint(self.ctx.types, target)
                .unwrap_or(target);
        // The constraint may already be flattened into a single object that
        // carries only a display alias back to the written intersection
        // (`Named & Aged`); judge the promotion by the written form.
        let named = self
            .ctx
            .types
            .get_display_alias(constrained)
            .unwrap_or(constrained);
        crate::query_boundaries::diagnostics::intersection_members(self.ctx.types, constrained)
            .is_some()
            || crate::query_boundaries::diagnostics::intersection_members(self.ctx.types, named)
                .is_some()
    }

    /// True when `target` is a union that still has two or more members after
    /// stripping nullish members. tsc licenses the missing-property head
    /// promotion (TS2741/2739/2740) only when the target reduces to a SINGLE
    /// object type — a nullable union that nullish-strips to one member
    /// (`Opts | null`) still promotes, but a genuine multi-member object union
    /// (`Subscribable<any> | Subscribable<never>`) keeps the generic
    /// TS2345/TS2344 head even when the property is missing in every member.
    fn target_is_multi_member_object_union(&mut self, target: TypeId) -> bool {
        let evaluated = self.evaluate_type_for_assignability(target);
        let members = diagnostic_query::union_members(self.ctx.types, evaluated).or_else(|| {
            // The union may have been merged into a single evaluated
            // object that carries only a display alias back to the alias
            // APPLICATION (`ObservableInput<any>`); the alias def's BODY
            // union carries the member arity tsc judges the promotion by.
            let named = self
                .ctx
                .types
                .get_display_alias(evaluated)
                .unwrap_or(evaluated);
            let (base, _) = diagnostic_query::application_info(self.ctx.types, named)?;
            let def_id = diagnostic_query::lazy_def_id(self.ctx.types, base)?;
            let def = self.ctx.definition_store.get(def_id)?;
            if def.kind != tsz_solver::def::DefKind::TypeAlias {
                return None;
            }
            diagnostic_query::union_members(self.ctx.types, def.body?)
        });
        let Some(members) = members else {
            return false;
        };
        members
            .iter()
            .filter(|&&m| m != TypeId::NULL && m != TypeId::UNDEFINED)
            .count()
            > 1
    }

    /// True when `source` is a class-instance type whose declared `extends`
    /// chain contains a base expression that failed constructor validation
    /// (TS2507). tsc then reduces the source through its deepest resolvable
    /// base, so the missing-property failure lands NESTED under the generic
    /// relation head instead of being promoted (recursiveComplicatedClasses:
    /// `Argument of type 'TypeSymbol' is not assignable ...` with the
    /// missing-property line as a chain entry sourced at the base class).
    fn source_class_heritage_chain_errored(&mut self, source: TypeId) -> bool {
        use tsz_scanner::SyntaxKind;
        let Some(def_id) =
            crate::query_boundaries::diagnostics::lazy_def_id(self.ctx.types, source)
                .or_else(|| self.ctx.definition_store.find_def_for_type(source))
        else {
            return false;
        };
        let Some((sym_id, _)) = self.ctx.def_symbol_identity(def_id) else {
            return false;
        };
        let mut current = Some(sym_id);
        for _ in 0..16 {
            let Some(sym_id) = current.take() else {
                return false;
            };
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                return false;
            };
            let mut decl_idx = symbol.value_declaration;
            let Some(mut decl_node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            // `value_declaration` may point at the class NAME identifier;
            // hop to the enclosing class declaration node in that case.
            if self.ctx.arena.get_class(decl_node).is_none()
                && let Some(ext) = self.ctx.arena.get_extended(decl_idx)
                && ext.parent.is_some()
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            {
                decl_idx = ext.parent;
                decl_node = parent_node;
            }
            let Some(class) = self.ctx.arena.get_class(decl_node) else {
                return false;
            };
            let _ = decl_idx;
            let Some(clauses) = &class.heritage_clauses else {
                return false;
            };
            let mut next_base = None;
            for &clause_idx in &clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                let Some(&type_idx) = heritage.types.nodes.first() else {
                    continue;
                };
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };
                let expr_idx = self
                    .ctx
                    .arena
                    .get_expr_type_args(type_node)
                    .map_or(type_idx, |ewta| ewta.expression);
                // Recompute constructor validity instead of sniffing for an
                // already-emitted TS2507: statement order means the heritage
                // check for a class declared after the failing expression has
                // not run yet. Only READ the node-type cache — forcing a full
                // expression check mid-render re-enters checking with flow
                // side effects; fall back to the base symbol's declared type,
                // which is the same lazy path the heritage check itself uses.
                let base_type = self
                    .ctx
                    .node_types
                    .get(&expr_idx.0)
                    .copied()
                    .or_else(|| {
                        self.resolve_identifier_symbol(expr_idx)
                            .map(|sym| self.get_type_of_symbol(sym))
                    })
                    .unwrap_or(TypeId::ERROR);
                if base_type != TypeId::ERROR && base_type != TypeId::ANY {
                    let evaluated = self.evaluate_type_for_assignability(base_type);
                    // Strict constructor check matching tsc's isConstructorType
                    // (and the heritage TS2507 rule): only construct signatures
                    // count — a prototype property does not (SymbolConstructor
                    // has `prototype: Symbol` but no construct signatures).
                    let has_construct_sigs =
                        crate::query_boundaries::class_type::construct_signatures_for_type(
                            self.ctx.types,
                            evaluated,
                        )
                        .is_some_and(|sigs| !sigs.is_empty());
                    if !has_construct_sigs {
                        return true;
                    }
                }
                next_base = self.resolve_identifier_symbol(expr_idx);
            }
            current = next_base;
        }
        false
    }
}
