//! Shared diagnostic fingerprint normalization policy for checker reporters.
//!
//! This centralizes:
//! - primary anchor resolution
//! - exact vs rewritten anchor behavior
//! - node-vs-position span selection
//! - related-information normalization

use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::error_reporter::assignability::is_object_prototype_method;
use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticAnchorKind {
    Exact,
    RewriteAssignment,
    CallPrimary,
    OverloadPrimary,
    PropertyToken,
    ElementAccessExpr,
    ElementIndexArg,
    TypeAssertionOverlap { target_type: TypeId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedDiagnosticAnchor {
    pub node_idx: NodeIndex,
    pub start: u32,
    pub length: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RelatedInformationPolicy {
    include_primary: bool,
    dedupe: bool,
    limit: Option<usize>,
}

impl RelatedInformationPolicy {
    pub(crate) const ELABORATION: Self = Self {
        include_primary: true,
        dedupe: true,
        limit: None,
    };

    pub(crate) const WRAPPED_DIAGNOSTIC: Self = Self {
        include_primary: true,
        dedupe: true,
        limit: None,
    };

    pub(crate) const OVERLOAD_FAILURES: Self = Self {
        include_primary: false,
        dedupe: true,
        limit: None,
    };
}

// =========================================================================
// DiagnosticRenderRequest — explicit render-policy input
// =========================================================================

/// Strategy for generating related diagnostic information.
///
/// Each variant captures the inputs needed so that `emit_render_request`
/// can produce the related info through the central policy surface.
pub(crate) enum RelatedInfoStrategy {
    /// No related information.
    None,
    /// Generate from a solver failure reason using `related_from_failure_reason`.
    FromFailureReason {
        reason: tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
    },
    /// Use pre-built related items (already constructed by the reporter).
    Prebuilt(Vec<DiagnosticRelatedInformation>),
}

/// An explicit render-policy object that captures all decisions for emitting
/// a semantic diagnostic.
///
/// Reporters construct this to describe *what* to report (anchor kind, code,
/// message, related-info strategy). The central `emit_render_request` method
/// handles *how*: anchor resolution, related-info generation, normalization,
/// and emission. This prevents open-coded anchor/related-info decisions from
/// spreading across reporter modules.
pub(crate) struct DiagnosticRenderRequest {
    /// How to resolve the diagnostic anchor from the AST node.
    pub anchor_kind: DiagnosticAnchorKind,
    /// The diagnostic error code.
    pub code: u32,
    /// The formatted message text.
    pub message: String,
    /// Strategy for related-information generation.
    pub related: RelatedInfoStrategy,
    /// Policy for normalizing related information.
    pub related_policy: RelatedInformationPolicy,
}

impl DiagnosticRenderRequest {
    /// Create a simple render request with no related information.
    pub(crate) const fn simple(
        anchor_kind: DiagnosticAnchorKind,
        code: u32,
        message: String,
    ) -> Self {
        Self {
            anchor_kind,
            code,
            message,
            related: RelatedInfoStrategy::None,
            related_policy: RelatedInformationPolicy::ELABORATION,
        }
    }

    /// Create a simple render request from a diagnostic code and message arguments.
    ///
    /// Looks up the message template for `code`, formats it with `args`, and
    /// uses `DiagnosticAnchorKind::Exact` anchoring with no related information.
    /// This is the render-request equivalent of `error_at_node_msg`.
    pub(crate) fn simple_msg(code: u32, args: &[&str]) -> Self {
        use tsz_common::diagnostics::get_message_template;
        let template = get_message_template(code).unwrap_or("Unexpected checker diagnostic code.");
        let message = format_message(template, args);
        Self::simple(DiagnosticAnchorKind::Exact, code, message)
    }

    /// Create a render request that generates related info from a failure reason.
    pub(crate) const fn with_failure_reason(
        anchor_kind: DiagnosticAnchorKind,
        code: u32,
        message: String,
        reason: tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
    ) -> Self {
        Self {
            anchor_kind,
            code,
            message,
            related: RelatedInfoStrategy::FromFailureReason {
                reason,
                source,
                target,
            },
            related_policy: RelatedInformationPolicy::ELABORATION,
        }
    }

    /// Create a render request with pre-built related information.
    pub(crate) const fn with_related(
        anchor_kind: DiagnosticAnchorKind,
        code: u32,
        message: String,
        related: Vec<DiagnosticRelatedInformation>,
        policy: RelatedInformationPolicy,
    ) -> Self {
        Self {
            anchor_kind,
            code,
            message,
            related: RelatedInfoStrategy::Prebuilt(related),
            related_policy: policy,
        }
    }
}

impl<'a> CheckerState<'a> {
    pub(crate) fn resolve_diagnostic_anchor(
        &self,
        idx: NodeIndex,
        kind: DiagnosticAnchorKind,
    ) -> Option<ResolvedDiagnosticAnchor> {
        let node_idx = self.resolve_diagnostic_anchor_node(idx, kind);
        let loc = self.get_source_location(node_idx)?;
        let (start, length) = self.normalized_anchor_span(node_idx, loc.start, loc.length());
        Some(ResolvedDiagnosticAnchor {
            node_idx,
            start,
            length,
        })
    }

    pub(crate) fn resolve_diagnostic_anchor_node(
        &self,
        idx: NodeIndex,
        kind: DiagnosticAnchorKind,
    ) -> NodeIndex {
        match kind {
            DiagnosticAnchorKind::Exact => idx,
            DiagnosticAnchorKind::RewriteAssignment => self.assignment_anchor_node(idx),
            DiagnosticAnchorKind::CallPrimary => self.call_primary_anchor_node(idx),
            DiagnosticAnchorKind::OverloadPrimary => self.overload_primary_anchor_node(idx),
            DiagnosticAnchorKind::PropertyToken => self.property_token_anchor_node(idx),
            DiagnosticAnchorKind::ElementAccessExpr => self.element_access_expr_anchor_node(idx),
            DiagnosticAnchorKind::ElementIndexArg => self.element_index_arg_anchor_node(idx),
            DiagnosticAnchorKind::TypeAssertionOverlap { target_type } => {
                self.type_assertion_overlap_anchor_node(idx, target_type)
            }
        }
    }

    pub(crate) fn resolve_excess_argument_span(
        &self,
        args: &[NodeIndex],
        expected_max: usize,
    ) -> Option<(u32, u32)> {
        if expected_max >= args.len() {
            return None;
        }

        let first_excess = args[expected_max];
        let last_arg = *args.last()?;
        let start_loc = self.get_source_location(first_excess)?;
        let end_loc = self.get_source_location(last_arg)?;
        Some((start_loc.start, end_loc.end.saturating_sub(start_loc.start)))
    }

    pub(crate) fn related_from_failure_reason(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> Option<Vec<DiagnosticRelatedInformation>> {
        use tsz_solver::SubtypeFailureReason;

        let anchor = self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)?;
        let start = anchor.start;
        let length = anchor.length;

        let related = match reason {
            SubtypeFailureReason::MissingProperty {
                property_name,
                source_type,
                target_type,
            } => {
                if tsz_solver::is_primitive_type(self.ctx.types, *source_type) {
                    return None;
                }
                let tgt_str = self.format_type_diagnostic(*target_type);
                if matches!(tgt_str.as_str(), "Boolean" | "Number" | "String" | "Object") {
                    return None;
                }
                if tsz_solver::type_queries::is_intersection_type(self.ctx.types, *target_type) {
                    return None;
                }
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                if prop_name.starts_with("__private_brand") {
                    return None;
                }
                let widened = self.widen_type_for_display(*source_type);
                let src_str = self.format_type_diagnostic(widened);
                vec![DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Error,
                    code: diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                    file: self.ctx.file_name.clone(),
                    start,
                    length,
                    message_text: format_message(
                        diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                        &[&prop_name, &src_str, &tgt_str],
                    ),
                }]
            }
            SubtypeFailureReason::MissingProperties {
                property_names,
                source_type,
                target_type,
            } => {
                if tsz_solver::is_primitive_type(self.ctx.types, *source_type) {
                    return None;
                }
                let tgt_str = self.format_type_diagnostic(*target_type);
                if matches!(tgt_str.as_str(), "Boolean" | "Number" | "String" | "Object") {
                    return None;
                }
                if tsz_solver::type_queries::is_intersection_type(self.ctx.types, *target_type) {
                    return None;
                }
                let src_str = self.format_type_diagnostic(*source_type);
                let names: Vec<String> = property_names
                    .iter()
                    .filter(|a| !is_object_prototype_method(self.ctx.types.resolve_atom_ref(**a)))
                    .map(|a| self.ctx.types.resolve_atom_ref(*a).to_string())
                    .collect();
                if names.is_empty() {
                    return None;
                }
                if names.len() <= 4 {
                    vec![DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Error,
                        code: diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                        file: self.ctx.file_name.clone(),
                        start,
                        length,
                        message_text: format_message(
                            diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                            &[&src_str, &tgt_str, &names.join(", ")],
                        ),
                    }]
                } else {
                    let shown: Vec<&str> = names.iter().take(4).map(|s| s.as_str()).collect();
                    let more = names.len() - 4;
                    vec![DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Error,
                        code: diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                        file: self.ctx.file_name.clone(),
                        start,
                        length,
                        message_text: format_message(
                            diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                            &[&src_str, &tgt_str, &shown.join(", "), &more.to_string()],
                        ),
                    }]
                }
            }
            SubtypeFailureReason::PropertyTypeMismatch {
                property_name,
                source_property_type,
                target_property_type,
                ..
            } => vec![
                DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Error,
                    code: diagnostic_codes::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                    file: self.ctx.file_name.clone(),
                    start,
                    length,
                    message_text: format_message(
                        diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                        &[&self.ctx.types.resolve_atom_ref(*property_name)],
                    ),
                },
                DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Message,
                    code: diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    file: self.ctx.file_name.clone(),
                    start,
                    length,
                    message_text: format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[
                            &self.format_type_diagnostic(*source_property_type),
                            &self.format_type_diagnostic(*target_property_type),
                        ],
                    ),
                },
            ],
            SubtypeFailureReason::OptionalPropertyRequired { property_name } => {
                vec![DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Error,
                    code: diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                    file: self.ctx.file_name.clone(),
                    start,
                    length,
                    message_text: format_message(
                        diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                        &[
                            &self.ctx.types.resolve_atom_ref(*property_name),
                            &self.format_type_diagnostic(source),
                            &self.format_type_diagnostic(target),
                        ],
                    ),
                }]
            }
            SubtypeFailureReason::ReturnTypeMismatch {
                source_return,
                target_return,
                nested_reason,
            } => {
                let source_str = self.format_type_diagnostic(*source_return);
                let target_str = self.format_type_diagnostic(*target_return);
                let mut items = vec![
                    DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Error,
                        code: reason.diagnostic_code(),
                        file: self.ctx.file_name.clone(),
                        start,
                        length,
                        message_text: format!(
                            "Return type '{source_str}' is not assignable to '{target_str}'."
                        ),
                    },
                    DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Message,
                        code: diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        file: self.ctx.file_name.clone(),
                        start,
                        length,
                        message_text: format_message(
                            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            &[&source_str, &target_str],
                        ),
                    },
                ];
                // Drill into nested reason to produce elaboration diagnostics
                // (e.g. TS2741 "Property 'x' is missing..." when the return type
                // mismatch is due to a missing property).
                if let Some(nested) = nested_reason
                    && let Some(nested_related) = self.related_from_failure_reason(
                        nested,
                        *source_return,
                        *target_return,
                        anchor_idx,
                    )
                {
                    items.extend(nested_related);
                }
                items
            }
            SubtypeFailureReason::IndexSignatureMismatch {
                index_kind,
                source_value_type,
                target_value_type,
            } => {
                let source_str = self.format_type_diagnostic(*source_value_type);
                let target_str = self.format_type_diagnostic(*target_value_type);
                vec![
                    DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Error,
                        code: reason.diagnostic_code(),
                        file: self.ctx.file_name.clone(),
                        start,
                        length,
                        message_text: format!(
                            "{index_kind} index signature is incompatible: '{source_str}' is not assignable to '{target_str}'."
                        ),
                    },
                    DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Message,
                        code: diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        file: self.ctx.file_name.clone(),
                        start,
                        length,
                        message_text: format_message(
                            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            &[&source_str, &target_str],
                        ),
                    },
                ]
            }
            SubtypeFailureReason::ArrayElementMismatch {
                source_element,
                target_element,
            } => {
                let source_str = self.format_type_diagnostic(*source_element);
                let target_str = self.format_type_diagnostic(*target_element);
                vec![
                    DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Error,
                        code: reason.diagnostic_code(),
                        file: self.ctx.file_name.clone(),
                        start,
                        length,
                        message_text: format!(
                            "Array element type '{source_str}' is not assignable to '{target_str}'."
                        ),
                    },
                    DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Message,
                        code: diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        file: self.ctx.file_name.clone(),
                        start,
                        length,
                        message_text: format_message(
                            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            &[&source_str, &target_str],
                        ),
                    },
                ]
            }
            SubtypeFailureReason::MissingIndexSignature { index_kind } => {
                vec![DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Error,
                    code: diagnostic_codes::INDEX_SIGNATURE_FOR_TYPE_IS_MISSING_IN_TYPE,
                    file: self.ctx.file_name.clone(),
                    start,
                    length,
                    message_text: format_message(
                        diagnostic_messages::INDEX_SIGNATURE_FOR_TYPE_IS_MISSING_IN_TYPE,
                        &[index_kind, &self.format_type_diagnostic(source)],
                    ),
                }]
            }
            _ => return None,
        };

        Some(self.normalize_related_information(related, RelatedInformationPolicy::ELABORATION))
    }

    pub(crate) fn related_from_diagnostic(
        &self,
        diag: &Diagnostic,
        policy: RelatedInformationPolicy,
    ) -> Vec<DiagnosticRelatedInformation> {
        let mut items = Vec::new();

        if policy.include_primary {
            items.push(DiagnosticRelatedInformation {
                category: diag.category,
                code: diag.code,
                file: diag.file.clone(),
                start: diag.start,
                length: diag.length,
                message_text: diag.message_text.clone(),
            });
        }

        items.extend(diag.related_information.iter().cloned());
        self.normalize_related_information(items, policy)
    }

    pub(crate) fn normalize_related_information(
        &self,
        items: Vec<DiagnosticRelatedInformation>,
        policy: RelatedInformationPolicy,
    ) -> Vec<DiagnosticRelatedInformation> {
        let mut normalized = Vec::new();
        let mut seen = FxHashSet::default();

        for item in items {
            if policy.dedupe {
                let key = (
                    item.category as u8,
                    item.code,
                    item.file.clone(),
                    item.start,
                    item.length,
                    item.message_text.clone(),
                );
                if !seen.insert(key) {
                    continue;
                }
            }
            normalized.push(item);
            if let Some(limit) = policy.limit
                && normalized.len() >= limit
            {
                break;
            }
        }

        normalized
    }

    /// Returns true when a contextual object-literal call mismatch is only caused by
    /// Object.prototype members such as `toString` or `valueOf`.
    ///
    /// Those members are implicitly present on ordinary objects, so the call-level
    /// TS2345 should be suppressed instead of surfacing a bogus missing-property error.
    pub(crate) fn should_suppress_object_literal_call_mismatch(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
    ) -> bool {
        use tsz_solver::SubtypeFailureReason;

        let analysis = self.analyze_assignability_failure(source_type, target_type);
        let Some(reason) = analysis.failure_reason else {
            return false;
        };

        match reason {
            SubtypeFailureReason::MissingProperty { property_name, .. } => {
                let prop_name = self.ctx.types.resolve_atom_ref(property_name);
                is_object_prototype_method(&prop_name)
            }
            SubtypeFailureReason::MissingProperties { property_names, .. } => {
                !property_names.is_empty()
                    && property_names.iter().all(|property_name| {
                        let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                        is_object_prototype_method(&prop_name)
                    })
            }
            _ => false,
        }
    }

    fn parent_index(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let ext = self.ctx.arena.get_extended(idx)?;
        ext.parent.is_some().then_some(ext.parent)
    }

    pub(crate) fn normalized_anchor_span(
        &self,
        node_idx: NodeIndex,
        start: u32,
        length: u32,
    ) -> (u32, u32) {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return (start, length);
        };

        if node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(node)
        {
            return (start, ident.escaped_text.len() as u32);
        }

        // For declarations that always start with a name token (no
        // modifiers), normalize the diagnostic span to just the leading
        // identifier.  This matches tsc which anchors on the name, not
        // the full declaration span.
        if matches!(
            node.kind,
            k if k == syntax_kind_ext::VARIABLE_DECLARATION
                || k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                || k == syntax_kind_ext::PROPERTY_SIGNATURE
                || k == syntax_kind_ext::BINDING_ELEMENT
        ) && let Some(identifier_len) = self.leading_identifier_len(start)
        {
            return (start, identifier_len);
        }

        // For declarations that may have leading modifiers (private,
        // readonly, etc.) or keywords (dot-dot-dot), resolve via the
        // explicit `name` child node so modifiers are excluded from the
        // diagnostic span.
        if node.kind == syntax_kind_ext::PROPERTY_DECLARATION
            && let Some(prop) = self.ctx.arena.get_property_decl(node)
            && prop.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(prop.name)
        {
            let name_start = name_node.pos;
            let name_len = name_node.end.saturating_sub(name_start);
            return self.normalized_anchor_span(prop.name, name_start, name_len);
        }

        if node.kind == syntax_kind_ext::PARAMETER
            && let Some(param) = self.ctx.arena.get_parameter(node)
            && param.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(param.name)
        {
            let name_start = name_node.pos;
            let name_len = name_node.end.saturating_sub(name_start);
            return self.normalized_anchor_span(param.name, name_start, name_len);
        }

        (start, length)
    }

    fn leading_identifier_len(&self, start: u32) -> Option<u32> {
        let sf = self.ctx.arena.source_files.first()?;
        let text = sf.text.get(start as usize..)?;
        let mut chars = text.chars();
        let first = chars.next()?;
        if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
            return None;
        }

        let mut len = first.len_utf8() as u32;
        for ch in chars {
            if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                len += ch.len_utf8() as u32;
            } else {
                break;
            }
        }
        Some(len)
    }

    fn property_token_anchor_node(&self, idx: NodeIndex) -> NodeIndex {
        let Some(node) = self.ctx.arena.get(idx) else {
            return idx;
        };

        if matches!(
            node.kind,
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        ) && let Some(access) = self.ctx.arena.get_access_expr(node)
        {
            return access.name_or_argument;
        }

        if let Some(parent_idx) = self.parent_index(idx)
            && let Some(parent_node) = self.ctx.arena.get(parent_idx)
            && matches!(
                parent_node.kind,
                k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            )
            && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
            && access.name_or_argument == idx
        {
            return idx;
        }

        idx
    }

    fn element_access_expr_anchor_node(&self, idx: NodeIndex) -> NodeIndex {
        let Some(node) = self.ctx.arena.get(idx) else {
            return idx;
        };
        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return idx;
        }
        if let Some(parent_idx) = self.parent_index(idx)
            && let Some(parent_node) = self.ctx.arena.get(parent_idx)
            && parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
            && access.name_or_argument == idx
        {
            return parent_idx;
        }
        idx
    }

    fn element_index_arg_anchor_node(&self, idx: NodeIndex) -> NodeIndex {
        let Some(node) = self.ctx.arena.get(idx) else {
            return idx;
        };
        if node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
        {
            return access.name_or_argument;
        }
        idx
    }

    fn assignment_anchor_node(&self, idx: NodeIndex) -> NodeIndex {
        let mut current = idx;
        let mut saw_assignment_binary = false;
        let mut var_decl: Option<NodeIndex> = None;

        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }

            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };

            if matches!(
                parent_node.kind,
                syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::METHOD_DECLARATION
                    | syntax_kind_ext::CONSTRUCTOR
                    | syntax_kind_ext::GET_ACCESSOR
                    | syntax_kind_ext::SET_ACCESSOR
                    | syntax_kind_ext::CLASS_EXPRESSION
                    | syntax_kind_ext::CLASS_DECLARATION
            ) {
                break;
            }

            if matches!(
                parent_node.kind,
                syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT | syntax_kind_ext::JSX_OPENING_ELEMENT
            ) {
                break;
            }

            if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.ctx.arena.get_binary_expr(parent_node)
                && self.is_assignment_operator(binary.operator_token)
            {
                if saw_assignment_binary {
                    return idx;
                }
                saw_assignment_binary = true;
            }

            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                if saw_assignment_binary {
                    return idx;
                }
                var_decl = Some(parent);
            }

            if parent_node.kind == syntax_kind_ext::VARIABLE_STATEMENT && var_decl.is_some() {
                if let Some(vd_idx) = var_decl
                    && let Some(vd) = self.ctx.arena.get_variable_declaration_at(vd_idx)
                    && vd.name.is_some()
                {
                    return vd.name;
                }
                return parent;
            }

            if parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT && saw_assignment_binary {
                return parent;
            }

            current = parent;
        }

        if let Some(vd_idx) = var_decl {
            if let Some(vd) = self.ctx.arena.get_variable_declaration_at(vd_idx)
                && vd.name.is_some()
            {
                return vd.name;
            }
            return vd_idx;
        }

        idx
    }

    fn call_primary_anchor_node(&self, idx: NodeIndex) -> NodeIndex {
        let Some(node) = self.ctx.arena.get(idx) else {
            return idx;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return idx;
        }

        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return idx;
        };
        let Some(callee_node) = self.ctx.arena.get(call.expression) else {
            return idx;
        };

        if callee_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(callee_node)
        {
            return access.name_or_argument;
        }

        call.expression
    }

    fn overload_primary_anchor_node(&self, call_idx: NodeIndex) -> NodeIndex {
        let Some(node) = self.ctx.arena.get(call_idx) else {
            return call_idx;
        };
        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return call_idx;
        };
        if let Some(args) = &call.arguments
            && let Some(&first) = args.nodes.first()
            && let Some(arg_node) = self.ctx.arena.get(first)
            && arg_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            && self.is_concat_call(call.expression)
            && let Some(array) = self.ctx.arena.get_literal_expr(arg_node)
            && let Some(&first_elem) = array.elements.nodes.first()
        {
            return first_elem;
        }
        self.call_primary_anchor_node(call_idx)
    }

    fn type_assertion_overlap_anchor_in_expression(
        &self,
        expr_idx: NodeIndex,
        target_type: TypeId,
    ) -> Option<NodeIndex> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;

        if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let array = self.ctx.arena.get_literal_expr(node)?;
            let element_target = query_common::array_element_type(self.ctx.types, target_type)?;
            for &element_idx in &array.elements.nodes {
                if let Some(anchor) =
                    self.type_assertion_overlap_anchor_in_expression(element_idx, element_target)
                {
                    return Some(anchor);
                }
            }
            return None;
        }

        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let target_shape = query_common::object_shape_for_type(self.ctx.types, target_type)?;
        let object = self.ctx.arena.get_literal_expr(node)?;

        for &element_idx in &object.elements.nodes {
            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };

            let (prop_name, report_idx) = match element_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_property_assignment(element_node)?;
                    let name = self.get_property_name(prop.name)?;
                    (self.ctx.types.intern_string(&name), prop.name)
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_shorthand_property(element_node)?;
                    let name = self.get_identifier_text_from_idx(prop.name)?;
                    (self.ctx.types.intern_string(&name), prop.name)
                }
                _ => continue,
            };

            let exists = target_shape
                .properties
                .iter()
                .any(|prop| prop.name == prop_name);
            if !exists {
                return Some(report_idx);
            }
        }

        None
    }

    fn type_assertion_overlap_anchor_node(&self, idx: NodeIndex, target_type: TypeId) -> NodeIndex {
        let Some(node) = self.ctx.arena.get(idx) else {
            return idx;
        };
        let Some(assertion) = self.ctx.arena.get_type_assertion(node) else {
            return idx;
        };
        self.type_assertion_overlap_anchor_in_expression(assertion.expression, target_type)
            .unwrap_or(idx)
    }
}
