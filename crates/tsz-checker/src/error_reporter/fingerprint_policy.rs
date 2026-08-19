//! Shared diagnostic fingerprint normalization policy for checker reporters.
//!
//! This centralizes:
//! - primary anchor resolution
//! - exact vs rewritten anchor behavior
//! - node-vs-position span selection
//! - related-information normalization

use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, RelatedInformationKind,
    diagnostic_codes, diagnostic_messages, format_message,
};
use crate::error_reporter::assignability::is_object_prototype_method;
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
use crate::query_boundaries::common as query_common;
use crate::query_boundaries::diagnostics;
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

    /// Demote a diagnostic's primary message into the related chain, keeping any
    /// existing deeper entries. Used when a specific assignability failure (e.g.
    /// TS2741 missing property) must be nested beneath a wrapping head message.
    pub(crate) const WRAPPED_DIAGNOSTIC: Self = Self {
        include_primary: true,
        dedupe: true,
        limit: None,
    };

    /// Flat overload-failure list (fewer than 2 argument-error candidates, or
    /// candidate sets whose failures carry no declared signature, e.g.
    /// callback-body sets): one related line per failure, deduped and sorted
    /// as before.
    pub(crate) const OVERLOAD_FAILURES: Self = Self {
        include_primary: false,
        dedupe: false,
        limit: None,
    };

    /// Per-overload elaboration for 2+ argument-error candidates: with 2 or 3,
    /// each candidate contributes a depth-0 `Overload N of M, '...', gave the
    /// following error.` (`TS2772`) header immediately followed by its
    /// applicability error at depth 1+; with four or more, a single `The last
    /// overload gave the following error.` (`TS2770`) header wraps only the
    /// last candidate. Every candidate's header shares one anchor (the call
    /// site), so this is a set of sibling elaboration chains at one anchor:
    /// block-aware normalization keeps each candidate's chain contiguous and
    /// orders the candidates by their (shared) head anchor with a stable sort,
    /// which preserves declaration order. Dedupe is off so two overloads that
    /// fail identically still each keep their own nested body under their
    /// distinct header.
    pub(crate) const OVERLOAD_CHAINS: Self = Self {
        include_primary: false,
        dedupe: false,
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
    /// Extra pre-built related lines appended *after* whatever `related`
    /// produces, before normalization. This is how a caller layers an
    /// independent elaboration (e.g. the bare-type-parameter-target
    /// `TS5075`/`TS5082` note) on top of a `FromFailureReason` strategy
    /// without discarding the structural failure lines.
    pub extra_related: Vec<DiagnosticRelatedInformation>,
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
            extra_related: Vec::new(),
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
            extra_related: Vec::new(),
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
            extra_related: Vec::new(),
            related_policy: policy,
        }
    }

    /// Append extra pre-built related lines that render *after* whatever the
    /// `related` strategy produces. Used to layer an independent elaboration
    /// (e.g. the bare-type-parameter-target note) onto a request that already
    /// carries a failure-reason or prebuilt related chain.
    pub(crate) fn with_extra_related(mut self, extra: Vec<DiagnosticRelatedInformation>) -> Self {
        self.extra_related = extra;
        self
    }
}

/// Normalize a diagnostic's flat related-information list one elaboration
/// *block* at a time.
///
/// A block is a single root-anchored elaboration chain: it opens on a
/// `depth == 0` line and runs through every deeper (`depth > 0`) line up to the
/// next `depth == 0` line. `tsc` keeps each chain contiguous, in construction
/// order, and never dedupes across chains — chains live inside `messageText`,
/// not in `relatedInformation`. A flat per-line pass cannot represent that: at
/// one `(file, start)` anchor a `(file, start, depth, message)` sort interleaves
/// sibling chains by depth (all depth-0 headers, then all depth-1 leaves, …),
/// and a global dedup merges an identical leaf line that legitimately sits under
/// two *different* headers. Both failures vanish once normalization is scoped to
/// a block:
///
/// * **Blocks stay whole and ordered by their head anchor.** The block ordering
///   sort keys on the head line's `(file, start)` and is *stable*, so
///   different-anchor chains keep their former positional order while sibling
///   chains sharing one anchor — which the global sort used to interleave — now
///   render in the order they were built. That build order is the order `tsc`
///   emits them (e.g. overload candidates in declaration order), which
///   generalizes the former per-overload `preserve_order` special case into the
///   default mechanism.
/// * **Dedup is per block.** A block-local `seen` set drops exact duplicate
///   lines within a chain (as before) but never merges a shared leaf across
///   chains, so both leaves survive under their respective headers.
///
/// Within a block the former depth-major sort is retained, so a header stays
/// above its leaves and single-chain output is byte-identical to the previous
/// per-line normalization.
pub(crate) fn normalize_related_information_blocks(
    items: Vec<DiagnosticRelatedInformation>,
    policy: RelatedInformationPolicy,
) -> Vec<DiagnosticRelatedInformation> {
    // Partition into elaboration blocks. A depth-0 line is a chain head, so it
    // opens a new block — but only once the block currently open already holds a
    // head. That guard keeps the partition insensitive to intra-chain ordering:
    // any leading deeper (`depth > 0`) lines that arrive before the first head
    // attach to that head's block rather than orphaning into their own, so a
    // chain whose lines are not appended head-first still lands in one block
    // (the depth-major sort below then seats the head above them). Genuine
    // sibling chains — a second head after a block already has one — still split.
    let mut blocks: Vec<Vec<DiagnosticRelatedInformation>> = Vec::new();
    let mut open_block_has_head = false;
    for item in items {
        let is_head = item.depth == 0;
        if blocks.is_empty() || (is_head && open_block_has_head) {
            blocks.push(Vec::new());
            open_block_has_head = false;
        }
        open_block_has_head |= is_head;
        blocks
            .last_mut()
            .expect("a block was just ensured to exist")
            .push(item);
    }

    for block in &mut blocks {
        if policy.dedupe {
            let mut seen = FxHashSet::default();
            block.retain(|item| {
                seen.insert((
                    item.category as u8,
                    item.code,
                    item.file.clone(),
                    item.start,
                    item.length,
                    item.message_text.clone(),
                ))
            });
        }

        // Keep the header above its leaves within the chain. `depth` precedes
        // the textual tiebreaker so a depth-0 header (e.g. "Types of property
        // 'p' are incompatible.") always precedes its depth-1+ leaves (e.g.
        // "Type 'X' is not assignable to type 'Y'."). Without the depth key the
        // alphabetic compare reverses chains because "Type " (trailing space)
        // sorts before "Types". Within the same depth the message-text
        // tiebreaker still gives deterministic order when distinct code paths
        // append items in different sequences.
        block.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.start.cmp(&b.start))
                .then(a.depth.cmp(&b.depth))
                .then(a.message_text.cmp(&b.message_text))
        });
    }

    // Order the chains by their head anchor. The sort is stable, so chains that
    // share a `(file, start)` head keep construction order — sibling
    // elaboration chains at one anchor render in the order they were built
    // (which is the order `tsc` emits them), while different-anchor chains keep
    // their former positional ordering.
    blocks.sort_by(|a, b| {
        let (a_head, b_head) = (&a[0], &b[0]);
        a_head
            .file
            .cmp(&b_head.file)
            .then(a_head.start.cmp(&b_head.start))
    });

    let mut normalized: Vec<DiagnosticRelatedInformation> = blocks.into_iter().flatten().collect();
    if let Some(limit) = policy.limit {
        normalized.truncate(limit);
    }
    normalized
}

impl<'a> CheckerState<'a> {
    fn widen_display_property_literals_for_related_info(&mut self, type_id: TypeId) -> TypeId {
        diagnostics::display_property_literals_widened_for_related_info(self.ctx.types, type_id)
    }

    /// Re-render an anonymous object display with its literal annotations
    /// widened at the type level (#13075).
    ///
    /// The historical rewrite only applied to displays rendered as a bare
    /// anonymous object (`{ ... }`); keep that scope so named, union, and
    /// array surfaces preserve their literal annotations.
    fn rerender_anonymous_object_with_widened_literals(
        &mut self,
        type_id: TypeId,
        display: String,
    ) -> String {
        if !display.starts_with("{ ") || !display.ends_with(" }") {
            return display;
        }
        let widened = self.widen_annotation_literals_for_display(
            type_id,
            diagnostics::AnnotationLiteralWideningPolicy::ALL,
        );
        if widened.display_residue {
            // Literal spellings live only in fresh-object-literal display
            // provenance; render the canonical (display-property-free) form.
            return self.format_type_diagnostic_widened(widened.type_id);
        }
        if widened.type_id == type_id {
            return display;
        }
        self.format_type_for_diagnostic_role(
            widened.type_id,
            DiagnosticTypeDisplayRole::DefaultDiagnostic,
        )
    }

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

    /// Generalize the literal source (tsc `reportRelationError`, via
    /// `generalize_nested_relation_source_for_display`) and produce the
    /// finalized `(source, target)` display pair with the `DefaultDiagnostic`
    /// role — the shared shape of the hand-rolled TS2345 related-info arms
    /// below. The finalizer receives the generalized source so same-name
    /// disambiguation sees the pair actually rendered.
    fn generalized_default_role_pair_display(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> (String, String) {
        let display_source = self.generalize_nested_relation_source_for_display(source, target);
        let source_str = self.format_type_for_diagnostic_role(
            display_source,
            DiagnosticTypeDisplayRole::DefaultDiagnostic,
        );
        let target_str = self
            .format_type_for_diagnostic_role(target, DiagnosticTypeDisplayRole::DefaultDiagnostic);
        self.finalize_pair_display_for_diagnostic(display_source, target, source_str, target_str)
    }

    pub(crate) fn related_from_failure_reason(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> Option<Vec<DiagnosticRelatedInformation>> {
        use crate::query_boundaries::common::SubtypeFailureReason;

        let anchor = self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)?;
        let start = anchor.start;
        let length = anchor.length;

        let related = match reason {
            SubtypeFailureReason::MissingProperty {
                property_name,
                source_type,
                target_type,
            } => {
                if self.should_suppress_missing_property_for_callable_source(
                    source,
                    *source_type,
                    target,
                ) {
                    return None;
                }
                if crate::query_boundaries::common::is_primitive_type(self.ctx.types, *source_type)
                {
                    return None;
                }
                let tgt_str = self.format_type_for_diagnostic_role(
                    *target_type,
                    DiagnosticTypeDisplayRole::DefaultDiagnostic,
                );
                if matches!(tgt_str.as_str(), "Boolean" | "Number" | "String" | "Object") {
                    return None;
                }
                if crate::query_boundaries::common::is_intersection_type(
                    self.ctx.types,
                    *target_type,
                ) {
                    return None;
                }
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                if tsz_solver::utils::is_synthetic_private_brand_name(&prop_name) {
                    return None;
                }
                let source_display_type =
                    self.widen_display_property_literals_for_related_info(source);
                let source_display_type = diagnostics::widen_argument_type_for_display(
                    self.ctx.types,
                    source_display_type,
                );
                let target_display_type =
                    self.widen_display_property_literals_for_related_info(target);
                let src_str = self.format_type_for_diagnostic_role(
                    source_display_type,
                    DiagnosticTypeDisplayRole::DefaultDiagnostic,
                );
                let tgt_str = self.format_type_for_diagnostic_role(
                    target_display_type,
                    DiagnosticTypeDisplayRole::DefaultDiagnostic,
                );
                let src_str = self
                    .rerender_anonymous_object_with_widened_literals(source_display_type, src_str);
                let tgt_str = self
                    .rerender_anonymous_object_with_widened_literals(target_display_type, tgt_str);
                let (src_str, tgt_str) = self.finalize_pair_display_for_diagnostic(
                    source_display_type,
                    target_display_type,
                    src_str,
                    tgt_str,
                );
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
                    depth: 0,
                    kind: RelatedInformationKind::ChainLink,
                }]
            }
            SubtypeFailureReason::MissingProperties {
                property_names,
                source_type,
                target_type,
            } => {
                if self.should_suppress_missing_property_for_callable_source(
                    source,
                    *source_type,
                    target,
                ) {
                    return None;
                }
                if crate::query_boundaries::common::is_primitive_type(self.ctx.types, *source_type)
                {
                    return None;
                }
                let tgt_str = self.format_type_for_diagnostic_role(
                    *target_type,
                    DiagnosticTypeDisplayRole::DefaultDiagnostic,
                );
                if matches!(tgt_str.as_str(), "Boolean" | "Number" | "String" | "Object") {
                    return None;
                }
                if crate::query_boundaries::common::is_intersection_type(
                    self.ctx.types,
                    *target_type,
                ) {
                    return None;
                }
                let src_str = self.format_type_for_diagnostic_role(
                    *source_type,
                    DiagnosticTypeDisplayRole::DefaultDiagnostic,
                );
                let (src_str, tgt_str) = self.finalize_pair_display_for_diagnostic(
                    *source_type,
                    *target_type,
                    src_str,
                    tgt_str,
                );
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
                        depth: 0,
                        kind: RelatedInformationKind::ChainLink,
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
                                            depth: 0,
                    kind: RelatedInformationKind::ChainLink,
                    }]
                }
            }
            SubtypeFailureReason::PropertyTypeMismatch {
                property_name,
                source_property_type,
                target_property_type,
                nested_reason,
            } => {
                // When the property relation fails through its own *structural*
                // drill (a nested tuple position, array element, deeper-property
                // chain, index signature, missing property, …), the hand-rolled
                // two-line `Types of property 'p' … / Type 'sp' … 'tp'.` shape
                // below truncates the chain: it stops at the property leaf and
                // never surfaces the inner cause, diverging from the
                // direct-assignment (TS2322) elaboration tsc uses for both
                // surfaces. Delegate the whole reason to that single source of
                // truth (`render_failure_reason`) so the call-argument (TS2345)
                // chain carries the same dotted-path collapse, tuple positions,
                // and array/index drill. Scalar and union-member property
                // failures keep the established hand-rolled shape below (no
                // structural drill to recover), so those high-traffic chains are
                // byte-identical to today.
                if nested_reason
                    .as_deref()
                    .is_some_and(Self::property_nested_reason_needs_full_drill)
                {
                    return Some(self.reanchored_container_related(
                        reason, source, target, anchor_idx, start, length,
                    ));
                }
                let target_property_type = if self.should_strip_nullish_for_property_display(target)
                {
                    self.strip_nullish_for_assignability_display(
                        *target_property_type,
                        *source_property_type,
                    )
                    .unwrap_or(*target_property_type)
                } else {
                    *target_property_type
                };
                let (source_str, target_str) = self.generalized_default_role_pair_display(
                    *source_property_type,
                    target_property_type,
                );

                let mut items = vec![
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
                        depth: 0,
                        kind: RelatedInformationKind::ChainLink,
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
                        // Leaf sits one level beneath the `Types of property`
                        // header. See the sort-key comment in
                        // `normalize_related_information_blocks` for why the
                        // chain order depends on this.
                        depth: 1,
                        kind: RelatedInformationKind::ChainLink,
                    },
                ];
                // When the property type fails because a union member is not
                // assignable (the common `T | undefined` vs `T` case), surface
                // the failing member at depth 2 so the chain reads
                //   Types of property 'p' are incompatible.
                //     Type 'A | undefined' is not assignable to type 'A'.
                //       Type 'undefined' is not assignable to type 'A'.
                if let Some(line) =
                    self.union_member_related_line(nested_reason.as_deref(), start, length, 2)
                {
                    items.push(line);
                }
                items
            }
            SubtypeFailureReason::OptionalPropertyRequired { property_name } => {
                // Present-but-optional source property assigned to a required
                // target: tsc reports TS2327 ("is optional ... but required"),
                // not the absent-property message TS2741.
                let src_str = self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::DefaultDiagnostic,
                );
                let tgt_str = self.format_type_for_diagnostic_role(
                    target,
                    DiagnosticTypeDisplayRole::DefaultDiagnostic,
                );
                let (src_str, tgt_str) =
                    self.finalize_pair_display_for_diagnostic(source, target, src_str, tgt_str);
                vec![DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Error,
                    code: diagnostic_codes::PROPERTY_IS_OPTIONAL_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                    file: self.ctx.file_name.clone(),
                    start,
                    length,
                    message_text: format_message(
                        diagnostic_messages::PROPERTY_IS_OPTIONAL_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                        &[
                            &self.ctx.types.resolve_atom_ref(*property_name),
                            &src_str,
                            &tgt_str,
                        ],
                    ),
                    depth: 0,
                    kind: RelatedInformationKind::ChainLink,
                }]
            }
            SubtypeFailureReason::PropertyNominalMismatch { property_name } => {
                // Two unrelated classes each declare a same-spelled modifier
                // `private`/`protected` member. tsc elaborates the argument
                // mismatch (TS2345) with the same `Types have separate
                // declarations of a … property 'x'.` line it attaches to the
                // assignment (TS2322) surface; route it through the shared
                // `nominal_mismatch_detail` builder so both surfaces agree.
                // `nominal_mismatch_detail` evaluates through instantiated forms
                // itself, so pass the raw types (matching the sibling arm below).
                let detail = self.nominal_mismatch_detail(source, target, *property_name)?;
                vec![DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Error,
                    code: reason.diagnostic_code(),
                    file: self.ctx.file_name.clone(),
                    start,
                    length,
                    message_text: detail,
                    depth: 0,
                    kind: RelatedInformationKind::ChainLink,
                }]
            }
            SubtypeFailureReason::PrivateIdentifierMemberMismatch { property_name } => {
                // ES private identifier (`#name`) counterpart of the arm above:
                // tsc's TS18015 `Property '#x' in type 'A' refers to a different
                // member …` line, naming each side's declaring class. Shared with
                // the assignment renderer (`render_private_identifier_member_mismatch`).
                let (source_str, target_str) = self
                    .format_top_level_assignability_message_types_at(source, target, anchor_idx);
                let detail = self.private_identifier_mismatch_detail(
                    source,
                    target,
                    *property_name,
                    &source_str,
                    &target_str,
                );
                vec![DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Error,
                    code: reason.diagnostic_code(),
                    file: self.ctx.file_name.clone(),
                    start,
                    length,
                    message_text: detail,
                    depth: 0,
                    kind: RelatedInformationKind::ChainLink,
                }]
            }
            SubtypeFailureReason::ReturnTypeMismatch {
                source_return,
                target_return,
                nested_reason,
            } => {
                let (source_str, target_str) =
                    self.generalized_default_role_pair_display(*source_return, *target_return);
                // tsc's elaboration shape for TS2345 function-return-type
                // mismatches goes straight from the top-level message into
                // the inner mismatch line:
                //
                //   error TS2345: Argument of type '(a) => string' is not
                //                 assignable to parameter of type '(a) => 1'.
                //     Type 'string' is not assignable to type '1'.
                //
                // The intermediate "Return type 'X' is not assignable to 'Y'."
                // framing is never emitted by tsc (verified: zero matches in
                // tsc baselines). Emit only the inner mismatch line, then
                // drill into any nested reason for further elaboration. This
                // mirrors the same fix applied to TS2322's
                // `render_return_type_mismatch` in iter 51.
                let mut items = vec![DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Message,
                    code: diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    file: self.ctx.file_name.clone(),
                    start,
                    length,
                    message_text: format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    ),
                    depth: 0,
                    kind: RelatedInformationKind::ChainLink,
                }];
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
                nested_reason: _,
                property_name,
            } => {
                let source_str = self.format_type_for_diagnostic_role(
                    *source_value_type,
                    DiagnosticTypeDisplayRole::DefaultDiagnostic,
                );
                let target_str = self.format_type_for_diagnostic_role(
                    *target_value_type,
                    DiagnosticTypeDisplayRole::DefaultDiagnostic,
                );
                let (source_str, target_str) = self.finalize_pair_display_for_diagnostic(
                    *source_value_type,
                    *target_value_type,
                    source_str,
                    target_str,
                );
                // A named source property vs the target index renders as the
                // TS2530 head "Property '{name}' is incompatible with index
                // signature." (matching the assignment path); a source index
                // signature vs the target index keeps the "'{kind}' index
                // signature is incompatible" head.
                let head_message = match property_name {
                    Some(name) => format_message(
                        diagnostic_messages::PROPERTY_IS_INCOMPATIBLE_WITH_INDEX_SIGNATURE,
                        &[&self.ctx.types.resolve_atom_ref(*name)],
                    ),
                    None => format!(
                        "{index_kind} index signature is incompatible: '{source_str}' is not assignable to '{target_str}'."
                    ),
                };
                let head_code = match property_name {
                    Some(_) => diagnostic_codes::PROPERTY_IS_INCOMPATIBLE_WITH_INDEX_SIGNATURE,
                    None => reason.diagnostic_code(),
                };
                vec![
                    DiagnosticRelatedInformation {
                        category: DiagnosticCategory::Error,
                        code: head_code,
                        file: self.ctx.file_name.clone(),
                        start,
                        length,
                        message_text: head_message,
                        depth: 0,
                        kind: RelatedInformationKind::ChainLink,
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
                        depth: 1,
                        kind: RelatedInformationKind::ChainLink,
                    },
                ]
            }
            SubtypeFailureReason::ArrayElementMismatch { .. }
            | SubtypeFailureReason::TupleVariadicPositionMismatch { .. }
            | SubtypeFailureReason::TypeArgumentMismatch { .. }
            | SubtypeFailureReason::TupleElementTypeMismatch { .. }
            | SubtypeFailureReason::TupleElementMismatch { .. }
            | SubtypeFailureReason::SourceProvidesNoMatch { .. }
            | SubtypeFailureReason::TupleArityMismatch(_) => {
                // These reasons relate same-shaped containers whose differing
                // *component* is the cause (array element types, same-generic type
                // arguments, a fixed tuple slot, a variadic span, or a tuple
                // arity/length gap). tsc names both containers in the head — which
                // the call's TS2345 line already does — then relates the failing
                // component directly beneath it (for tuples: the positional `Type
                // at position N …` disambiguator, variadic position range, or the
                // `Source has N element(s) …` length line).
                // Reuse the TS2322 elaboration (`render_failure_reason`) as the
                // single source of truth: its child lines already carry the right
                // `code`, `message_text`, `depth`, and `file`, so only the category
                // and the call-site anchor (`start`/`length`) need rewriting for
                // the TS2345 surface. Without this arm, tuple-argument mismatches
                // fall through to `_ => return None` and drop the entire
                // elaboration chain, unlike the object/array argument paths.
                self.reanchored_container_related(reason, source, target, anchor_idx, start, length)
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
                        &[
                            index_kind,
                            &self.format_type_for_diagnostic_role(
                                source,
                                DiagnosticTypeDisplayRole::DefaultDiagnostic,
                            ),
                        ],
                    ),
                    depth: 0,
                    kind: RelatedInformationKind::ChainLink,
                }]
            }
            SubtypeFailureReason::AbstractConstructorAssignment => {
                vec![DiagnosticRelatedInformation {
                    category: DiagnosticCategory::Message,
                    code: diagnostic_codes::CANNOT_ASSIGN_AN_ABSTRACT_CONSTRUCTOR_TYPE_TO_A_NON_ABSTRACT_CONSTRUCTOR_TYPE,
                    file: self.ctx.file_name.clone(),
                    start,
                    length,
                    message_text: diagnostic_messages::CANNOT_ASSIGN_AN_ABSTRACT_CONSTRUCTOR_TYPE_TO_A_NON_ABSTRACT_CONSTRUCTOR_TYPE
                        .to_string(),
                                    depth: 0,
                kind: RelatedInformationKind::ChainLink,
                }]
            }
            SubtypeFailureReason::UnionSourceMismatch { .. }
            | SubtypeFailureReason::ConditionalBranchMismatch { .. }
            | SubtypeFailureReason::TypeParameterConstraintMismatch { .. } => {
                vec![self.union_member_related_line(Some(reason), start, length, 0)?]
            }
            SubtypeFailureReason::UnionTargetMismatch { .. } => {
                // A source assigned to a union target fails through the
                // best-matching member's missing required property. The nested
                // line is a `Property 'x' is missing … but required in type
                // '<member>'.` elaboration, not a plain `not assignable` member
                // line, so reuse the TS2322 elaboration (`render_failure_reason`)
                // as the single source of truth and re-anchor its child lines
                // onto the call's TS2345 surface (category + start/length).
                self.reanchored_container_related(reason, source, target, anchor_idx, start, length)
            }
            SubtypeFailureReason::ParameterTypeMismatch { .. } => {
                // A function/callback argument that fails because one of its
                // parameters is contravariantly incompatible. tsc explains the
                // signature line (already supplied by the TS2345 head) with a
                // `Types of parameters 'a' and 'b' are incompatible.` frame, the
                // contravariant leaf, and any nested chain — exactly the
                // elaboration the direct-assignment (TS2322) path renders via
                // `render_failure_reason` -> `push_parameter_mismatch_elaboration`.
                // When the offending parameter is itself callable that helper
                // intentionally emits no frame, leaving an empty list; preserve
                // the conservative signature-line-only rendering by returning
                // `None` (rather than an empty related list) in that case.
                let related = self.reanchored_container_related(
                    reason, source, target, anchor_idx, start, length,
                );
                if related.is_empty() {
                    return None;
                }
                related
            }
            SubtypeFailureReason::IntersectionTargetMismatch { .. } => {
                // A source assigned to an intersection target fails through one of
                // its constituents. The nested chain is the constituent frame
                // `Type 'S' is not assignable to type 'Ci'.` (alone for a plain
                // leaf, or followed by the constituent's structural drill / a
                // folded missing-property line) — a multi-line shape the
                // hand-rolled arms cannot represent. Reuse the TS2322 elaboration
                // (`render_failure_reason`) as the single source of truth and
                // re-anchor its child lines onto the call's TS2345 surface
                // (category + start/length), exactly like the union-target arm.
                self.reanchored_container_related(reason, source, target, anchor_idx, start, length)
            }
            SubtypeFailureReason::TooManyParameters { .. }
            | SubtypeFailureReason::TypePredicateMismatch { .. } => {
                // Function-signature relation failures whose cause is the
                // signature *shape* rather than a named member: a source that
                // declares more required parameters than the target provides
                // arguments for (`Target signature provides too few arguments.
                // Expected N or more, but got M.`, TS2849), or an incompatible
                // type-predicate return (`Type predicate 'x is A' is not
                // assignable to 'x is B'.` plus its nested leaf). tsc
                // elaborates both beneath the call's TS2345 head exactly as it
                // does beneath the direct-assignment TS2322 head; without this
                // arm they fall through to `_ => return None` and the
                // elaboration is dropped on the argument surface only (the
                // assignment surface renders it via `render_failure_reason`).
                // Reuse that same TS2322 elaboration as the single source of
                // truth and re-anchor its child lines onto the call site.
                let related = self.reanchored_container_related(
                    reason, source, target, anchor_idx, start, length,
                );
                if related.is_empty() {
                    return None;
                }
                related
            }
            _ => return None,
        };

        // The two callers (`emit_render_request`,
        // `emit_render_request_at_anchor`) normalize again under the request's
        // policy. Skipping the intermediate pass keeps the depth-aware sort
        // running exactly once on the final list.
        Some(related)
    }

    /// Re-anchor the child elaboration lines of [`Self::render_failure_reason`]
    /// (the `TS2322` single source of truth) onto the call-argument (`TS2345`)
    /// surface. The call already supplies the signature/container headline, so
    /// only the reason's `related_information` is carried over — with its
    /// category reset to `Message` and its anchor rewritten to the call site
    /// (`start`/`length`). Reason variants whose `TS2322` elaboration is reused
    /// verbatim (array element, type-argument, tuple element/arity, union-target,
    /// intersection-target, function parameter) share this transform.
    fn reanchored_container_related(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
        start: u32,
        length: u32,
    ) -> Vec<DiagnosticRelatedInformation> {
        Self::reanchor_chain_lines(
            self.render_failure_reason(reason, source, target, anchor_idx, 0)
                .related_information,
            start,
            length,
        )
    }

    /// Re-anchor elaboration chain lines onto a primary diagnostic surface:
    /// category reset to `Message`, anchor rewritten to (`start`, `length`).
    /// Chain lines are message-chain text, not cross-location pointers, so
    /// they always carry the primary diagnostic's position.
    pub(crate) fn reanchor_chain_lines(
        lines: Vec<DiagnosticRelatedInformation>,
        start: u32,
        length: u32,
    ) -> Vec<DiagnosticRelatedInformation> {
        lines
            .into_iter()
            .map(|mut rel| {
                rel.category = DiagnosticCategory::Message;
                rel.start = start;
                rel.length = length;
                rel
            })
            .collect()
    }

    /// Whether a [`PropertyTypeMismatch`]'s nested reason carries a structural
    /// drill that the hand-rolled `Types of property 'p' … / Type 'sp' … 'tp'.`
    /// pair cannot represent, so the call-argument (`TS2345`) elaboration must
    /// fall back to the direct-assignment (`TS2322`) renderer to stay faithful
    /// to tsc.
    ///
    /// Returns `true` for reasons whose `TS2322` rendering produces a chain
    /// *deeper or differently shaped* than a single property leaf — nested
    /// property chains (dotted-path collapse), tuple positions, array/index
    /// drill, and missing-property frames. Returns `false` for self-heading
    /// leaves (scalar/literal/intrinsic mismatches) and for union/conditional
    /// members, which the surrounding arm already surfaces via
    /// [`Self::union_member_related_line`]; those keep their established shape.
    const fn property_nested_reason_needs_full_drill(
        reason: &tsz_solver::SubtypeFailureReason,
    ) -> bool {
        use crate::query_boundaries::common::SubtypeFailureReason as R;
        matches!(
            reason,
            R::PropertyTypeMismatch { .. }
                | R::MissingProperty { .. }
                | R::MissingProperties { .. }
                | R::OptionalPropertyRequired { .. }
                | R::TupleElementTypeMismatch { .. }
                | R::TupleElementMismatch { .. }
                | R::TupleArityMismatch(_)
                | R::SourceProvidesNoMatch { .. }
                | R::ArrayElementMismatch { .. }
                | R::IndexSignatureMismatch { .. }
                | R::ReturnTypeMismatch { .. }
                | R::ParameterTypeMismatch { .. }
        )
        // Scalar/intrinsic/literal leaves self-head with the same
        // `Type 'sp' … 'tp'.` line the hand-rolled pair already emits, and
        // union/conditional members are handled by the surrounding arm's
        // `union_member_related_line`; those stay on the hand-rolled path.
    }

    /// Build the child-relation elaboration line (`Type 'C' is not assignable
    /// to type 'T'.`) for a [`UnionSourceMismatch`] or
    /// [`ConditionalBranchMismatch`] reason. Used to surface the root mismatch
    /// `depth` levels beneath a union-typed or conditional-typed failure —
    /// `0` for a top-level union mismatch (`Type 'A | B' is not assignable
    /// to T.` -> `Type 'B' is not assignable to T.`) and `2` when nested
    /// under a `Types of property` header plus its leaf.
    fn union_member_related_line(
        &mut self,
        reason: Option<&tsz_solver::SubtypeFailureReason>,
        start: u32,
        length: u32,
        depth: u8,
    ) -> Option<DiagnosticRelatedInformation> {
        let (child_source, child_target) = match reason? {
            tsz_solver::SubtypeFailureReason::UnionSourceMismatch {
                member_type,
                target_type,
                nested_reason,
                ..
            } => {
                // The member line renders the pair the solver actually
                // related: a sole-real-member nullable target explains the
                // member against the reduced member (tsc `getBestMatchingType`
                // re-relates there), recorded in the nested leaf's own types
                // (`Type 'boolean' is not assignable to type 'string'.` under
                // a `string | undefined` target). Every other producer
                // explains the member against the whole target, so the leaf
                // target equals `target_type` and the display is unchanged.
                match nested_reason.as_ref() {
                    tsz_solver::SubtypeFailureReason::TypeMismatch {
                        source_type: leaf_source,
                        target_type: leaf_target,
                    }
                    | tsz_solver::SubtypeFailureReason::IntrinsicTypeMismatch {
                        source_type: leaf_source,
                        target_type: leaf_target,
                    }
                    | tsz_solver::SubtypeFailureReason::LiteralTypeMismatch {
                        source_type: leaf_source,
                        target_type: leaf_target,
                    } => (*leaf_source, *leaf_target),
                    _ => (*member_type, *target_type),
                }
            }
            tsz_solver::SubtypeFailureReason::ConditionalBranchMismatch {
                branch_source,
                branch_target,
                ..
            } => (*branch_source, *branch_target),
            tsz_solver::SubtypeFailureReason::TypeParameterConstraintMismatch {
                constraint_type,
                target_type,
                ..
            } => (*constraint_type, *target_type),
            _ => return None,
        };
        let member_str = self.format_type_for_diagnostic_role(
            child_source,
            DiagnosticTypeDisplayRole::DefaultDiagnostic,
        );
        let target_str = self.format_type_for_diagnostic_role(
            child_target,
            DiagnosticTypeDisplayRole::DefaultDiagnostic,
        );
        Some(DiagnosticRelatedInformation {
            category: DiagnosticCategory::Message,
            code: diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            file: self.ctx.file_name.clone(),
            start,
            length,
            message_text: format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&member_str, &target_str],
            ),
            depth,
            kind: RelatedInformationKind::ChainLink,
        })
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
                depth: 0,
                kind: RelatedInformationKind::ChainLink,
            });
        }

        items.extend(diag.related_information.iter().cloned());
        normalize_related_information_blocks(items, policy)
    }

    /// Returns true when a contextual object-literal call mismatch is only caused by
    /// Object.prototype members such as `toString` or `valueOf`.
    ///
    /// Those members are implicitly present on ordinary objects, so the call-level
    /// TS2345 should be suppressed instead of surfacing a bogus missing-property error.
    ///
    /// Note: This suppression is intentionally NOT applied for variable declarations
    /// (see `try_elaborate_object_literal_properties_for_var_init`), because it can
    /// silence real errors like `var b: Boolean = {}` where the `Object.prototype`
    /// `valueOf()` return type is incompatible with the target's requirements.
    pub(crate) fn should_suppress_object_literal_call_mismatch(
        &mut self,
        source_type: TypeId,
        target_type: TypeId,
    ) -> bool {
        use crate::query_boundaries::common::SubtypeFailureReason;

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

        // tsc's `getErrorSpanForNode` has no `SyntaxKind.Parameter` case, so a
        // parameter keeps its own span rather than narrowing to its name. For a
        // plain parameter the two coincide (the node starts at the name), but a
        // **parameter property** starts at its accessibility modifier, and tsc
        // anchors there: `constructor(public x: string = 1)` reports TS2322 at
        // `public`, not at `x`. Narrowing unconditionally lost that.
        if node.kind == syntax_kind_ext::PARAMETER
            && let Some(param) = self.ctx.arena.get_parameter(node)
            && param.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(param.name)
        {
            let has_modifiers = param
                .modifiers
                .as_ref()
                .is_some_and(|modifiers| !modifiers.nodes.is_empty());
            if !has_modifiers {
                let name_start = name_node.pos;
                let name_len = name_node.end.saturating_sub(name_start);
                return self.normalized_anchor_span(param.name, name_start, name_len);
            }
        }

        // tsc's `getErrorSpanForNode` narrows a *named* function or class
        // expression to its name (an anonymous one keeps the `function`/`class`
        // keyword span). e.g. `function named() {}` as the always-truthy operand
        // of `||` anchors TS2872 at `named`, not at `function`.
        if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            && let Some(func) = self.ctx.arena.get_function(node)
            && func.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(func.name)
        {
            let name_start = name_node.pos;
            let name_len = name_node.end.saturating_sub(name_start);
            return self.normalized_anchor_span(func.name, name_start, name_len);
        }

        if node.kind == syntax_kind_ext::CLASS_EXPRESSION
            && let Some(class) = self.ctx.arena.get_class(node)
            && class.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(class.name)
        {
            let name_start = name_node.pos;
            let name_len = name_node.end.saturating_sub(name_start);
            return self.normalized_anchor_span(class.name, name_start, name_len);
        }

        (start, length)
    }

    fn leading_identifier_len(&self, start: u32) -> Option<u32> {
        let sf = self.ctx.arena.source_files.first()?;
        let text = sf.text.get(start as usize..)?;
        let mut chars = text.chars();
        let first = chars.next()?;
        if !tsz_common::text_scan::is_ascii_identifier_start_char(first) {
            return None;
        }

        let mut len = first.len_utf8() as u32;
        for ch in chars {
            if tsz_common::text_scan::is_ascii_identifier_continue_char(ch) {
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

        // If the starting node is itself a Parameter, that IS the assignment
        // site (parameter name = default-value initializer). Walking up would
        // land at the enclosing function expression — which starts at `(` —
        // and anchor TS2322 on the open paren instead of the parameter name.
        // Return the parameter so `normalized_anchor_span` can pick up the
        // `param.name` span and tsc's column reporting matches.
        if self
            .ctx
            .arena
            .get(current)
            .is_some_and(|n| n.kind == syntax_kind_ext::PARAMETER)
        {
            return current;
        }

        // If the starting node is itself a VariableDeclaration, capture it
        // immediately. This handles the common case where the diagnostic
        // index is the variable declaration node.
        if self
            .ctx
            .arena
            .get(current)
            .is_some_and(|n| n.kind == syntax_kind_ext::VARIABLE_DECLARATION)
        {
            var_decl = Some(current);
        }

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
                    | syntax_kind_ext::METHOD_DECLARATION
                    | syntax_kind_ext::CONSTRUCTOR
                    | syntax_kind_ext::GET_ACCESSOR
                    | syntax_kind_ext::SET_ACCESSOR
                    | syntax_kind_ext::CLASS_EXPRESSION
                    | syntax_kind_ext::CLASS_DECLARATION
            ) {
                break;
            }

            // When traversing into a function expression from within, return the function
            // expression itself as the anchor. This matches tsc's behavior for contextual
            // typing failures where the error should point at the entire function rather
            // than the inner expression that triggered the type mismatch.
            //
            // EXCEPTION: When the function expression is the RHS of an assignment (e.g.,
            // `A.prototype.foo = function() {}`), continue walking up to the assignment
            // level so the error is anchored at the expression statement, matching tsc.
            if matches!(
                parent_node.kind,
                syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION
            ) {
                // Check if this function is the RHS of an assignment
                let is_rhs_of_assignment =
                    if let Some(parent_ext) = self.ctx.arena.get_extended(parent) {
                        let grandparent = parent_ext.parent;
                        if let Some(gp_node) = self.ctx.arena.get(grandparent) {
                            if gp_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                                if let Some(binary) = self.ctx.arena.get_binary_expr(gp_node) {
                                    self.is_assignment_operator(binary.operator_token)
                                        && binary.right == parent
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                if !is_rhs_of_assignment {
                    return parent;
                }
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
                {
                    return self.variable_declaration_anchor(vd);
                }
                return parent;
            }

            if parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT && saw_assignment_binary {
                return parent;
            }

            current = parent;
        }

        if let Some(vd_idx) = var_decl {
            if let Some(vd) = self.ctx.arena.get_variable_declaration_at(vd_idx) {
                return self.variable_declaration_anchor(vd);
            }
            return vd_idx;
        }

        idx
    }

    /// Find the `VariableStatement` parent of a `VariableDeclaration`.
    fn find_variable_statement_parent(&self, vd_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = Some(vd_idx);
        while let Some(idx) = current {
            let ext = self.ctx.arena.get_extended(idx)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent)
                && parent_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            {
                return Some(parent);
            }
            current = Some(parent);
        }
        None
    }

    /// Choose the anchor for a variable declaration assignment error.
    ///
    /// For `var` declarations with property access initializers where the
    /// initializer type is callable, tsc points at the initializer
    /// (e.g., `var x: T = obj.prop;` -> points at `obj.prop`).
    /// For `let`/`const` or non-callable initializers, tsc points at the variable name.
    fn variable_declaration_anchor(
        &self,
        vd: &tsz_parser::parser::node::VariableDeclarationData,
    ) -> NodeIndex {
        // tsc's `elaborateDidYouMeanToCallOrConstruct` re-reports on the
        // *initializer expression* when calling (or `new`-ing) it would have
        // produced something assignable to the declared type, and only
        // otherwise anchors at the declaration name.
        //
        // Neither the declaration keyword nor the initializer's syntactic form
        // is part of that rule, so the previous `var` + property-access gates
        // made `export let x: Dog = getRover` anchor at `x` where tsc anchors at
        // `getRover`. The return-type check is the load-bearing half: gating on
        // "is callable" alone re-anchors every callable initializer and
        // regresses 23 tests (assignmentCompatability44, classSideInheritance3,
        // constructorAsType, ...), because tsc stays on the declaration name
        // when calling the source would not have helped.
        //
        // The declared type is read from the variable's own cached type, which
        // keeps this on the `&self` anchor path.
        if vd.initializer.is_some()
            && let Some(&init_type) = self.ctx.node_types.get(&vd.initializer.0)
            && let Some(&declared_type) = self
                .ctx
                .node_types
                .get(&vd.type_annotation.0)
                .or_else(|| self.ctx.node_types.get(&vd.name.0))
            && crate::query_boundaries::assignability_did_you_mean::did_you_mean_call_or_construct(
                self.ctx.types.as_type_database(),
                init_type,
                declared_type,
            )
        {
            return vd.initializer;
        }
        if vd.name.is_some() {
            return vd.name;
        }
        vd.initializer
    }

    /// Check if a type is callable (function, method, constructor, etc.).
    fn is_callable_type(&self, ty: TypeId) -> bool {
        use crate::query_boundaries::diagnostics::{callable_shape_for_type, function_shape};

        if function_shape(self.ctx.types, ty).is_some() {
            return true;
        }
        if callable_shape_for_type(self.ctx.types, ty).is_some() {
            return true;
        }
        false
    }

    fn call_primary_anchor_node(&self, idx: NodeIndex) -> NodeIndex {
        let Some(node) = self.ctx.arena.get(idx) else {
            return idx;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION
            && node.kind != syntax_kind_ext::NEW_EXPRESSION
        {
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
