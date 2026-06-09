//! Helper methods for assignability error reporting.
//! Extracted from `assignability.rs` for maintainability.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::error_reporter::assignability::is_object_prototype_method;
use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
};
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
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
        if depth != 0
            || crate::query_boundaries::diagnostics::array_element_type(self.ctx.types, source)
                .is_none()
        {
            return source;
        }

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

            let first_arg_type = self.get_type_of_node(first_arg);
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

            let recovered = self
                .ctx
                .types
                .array(self.widen_type_for_display(element_type));
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
        let (mut source_str, target_str) = if source_is_function_like || target_is_function_like {
            (
                self.format_type_diagnostic(source_for_display),
                self.format_type_diagnostic(target_for_display),
            )
        } else {
            (
                self.format_type_for_diagnostic_role(
                    source_for_display,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target: target_for_display,
                        anchor_idx,
                    },
                ),
                self.format_type_for_diagnostic_role(
                    target_for_display,
                    DiagnosticTypeDisplayRole::AssignmentTarget {
                        source: source_for_display,
                        anchor_idx,
                    },
                ),
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
    fn target_annotation_node(&self, anchor_idx: NodeIndex) -> Option<NodeIndex> {
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
            current = self.ctx.arena.get_extended(current).map(|ext| ext.parent)?;
        }
        None
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
    fn enclosing_function_return_annotation_node(
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
        use crate::query_boundaries::diagnostics::{IndexKind, IndexSignatureResolver};

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

        let resolver = IndexSignatureResolver::new(self.ctx.types);
        let source_has_index = source_candidates.iter().copied().any(|candidate| {
            resolver.has_index_signature(candidate, IndexKind::String)
                || resolver.has_index_signature(candidate, IndexKind::Number)
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
                resolver.has_index_signature(candidate, IndexKind::String)
                    || resolver.has_index_signature(candidate, IndexKind::Number)
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

    pub(super) fn nominal_mismatch_detail(
        &self,
        source_type: TypeId,
        target_type: TypeId,
        property_name: tsz_common::interner::Atom,
    ) -> Option<String> {
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
            tsz_solver::Visibility::Protected => Some(format!(
                "Types have separate declarations of a protected property '{prop_name}'."
            )),
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
}
