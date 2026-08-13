//! Fundamental error emission helpers: node-anchored, position-anchored,
//! and templated diagnostic emitters.

use crate::diagnostics::{Diagnostic, format_message};
use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInfoStrategy, ResolvedDiagnosticAnchor,
    normalize_related_information_blocks,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// Whether a diagnostic anchored at `idx` can be emitted from this
    /// checker.
    ///
    /// The node-anchored emitters below (`error_at_node` and friends) are
    /// span-gated: they silently drop the diagnostic when `idx` is not
    /// addressable in the current arena — the case for nodes from another
    /// arena reached through demand-driven lowering, whose diagnostics
    /// belong to the owning file's own check. Pre-flight gates that want to
    /// skip diagnostic-presentation work (message building, spelling
    /// suggestion scans) for such nodes must use this predicate so they
    /// cannot drift from the emitters' actual drop rule.
    pub(crate) fn can_emit_diagnostic_at(&self, idx: NodeIndex) -> bool {
        self.get_node_span(idx).is_some()
    }

    /// Report an error at a specific node.
    ///
    /// The span is normalized via `normalized_anchor_span` so that, for
    /// example, a `VariableDeclaration` node is trimmed to its leading
    /// identifier — matching the anchor policy used by `emit_render_request`
    /// and keeping diagnostic fingerprints stable.
    pub(crate) fn error_at_node(&mut self, node_idx: NodeIndex, message: &str, code: u32) {
        if let Some((start, end)) = self.get_node_span(node_idx) {
            let raw_length = end.saturating_sub(start);
            let (start, length) = self.normalized_anchor_span(node_idx, start, raw_length);
            // Use the error() function which has deduplication by (start, code)
            self.error(start, length, message.to_string(), code);
        }
    }

    /// Report an error at a node with its `related_information` already attached,
    /// built as one complete `Diagnostic` value before it is pushed.
    ///
    /// This is the build-before-push counterpart to emitting with
    /// [`Self::error_at_node`] and then appending related entries to the
    /// buffer's last diagnostic: the latter silently attaches the elaboration to
    /// the wrong entry whenever `(start, code)` deduplication drops the just
    /// emitted diagnostic (so the previous, unrelated entry becomes `last`).
    /// Routing the full value through [`CheckerContext::push_diagnostic`] keeps
    /// the related-information collision reconciliation authoritative and the
    /// buffer append-only. The main-diagnostic span is normalized exactly as
    /// [`Self::error_at_node`] does; callers supply `related` built against
    /// whatever spans they need.
    pub(crate) fn error_at_node_with_related(
        &mut self,
        node_idx: NodeIndex,
        message: &str,
        code: u32,
        related: Vec<crate::diagnostics::DiagnosticRelatedInformation>,
    ) {
        if let Some((start, end)) = self.get_node_span(node_idx) {
            let raw_length = end.saturating_sub(start);
            let (start, length) = self.normalized_anchor_span(node_idx, start, raw_length);
            let mut diag =
                Diagnostic::error(self.ctx.file_name.clone(), start, length, message, code);
            diag.related_information = related;
            self.ctx.push_diagnostic(diag);
        }
    }

    /// Report an error at a raw span with its `related_information` already
    /// attached, built as one complete `Diagnostic` value before it is pushed.
    ///
    /// The raw-span counterpart to [`Self::error_at_node_with_related`], for the
    /// callers that deliberately bypass `normalized_anchor_span` because tsc
    /// anchors on the whole node rather than its normalized sub-span (a
    /// parameter's leading `...`, for instance). Routing through
    /// [`CheckerContext::push_diagnostic`] keeps the same deduplication and
    /// related-information collision reconciliation as every other emitter, so
    /// this is not a way to smuggle a duplicate diagnostic past the buffer.
    pub(crate) fn error_at_span_with_related(
        &mut self,
        start: u32,
        length: u32,
        message: &str,
        code: u32,
        related: Vec<crate::diagnostics::DiagnosticRelatedInformation>,
    ) {
        let mut diag = Diagnostic::error(self.ctx.file_name.clone(), start, length, message, code);
        diag.related_information = related;
        self.ctx.push_diagnostic(diag);
    }

    /// Report an error using a shared diagnostic anchor policy.
    pub(crate) fn error_at_anchor(
        &mut self,
        node_idx: NodeIndex,
        anchor_kind: DiagnosticAnchorKind,
        message: &str,
        code: u32,
    ) {
        if let Some(anchor) = self.resolve_diagnostic_anchor(node_idx, anchor_kind) {
            self.error(anchor.start, anchor.length, message.to_string(), code);
        }
    }

    /// [`Self::error_at_anchor`] with `related_information` already attached,
    /// built as one complete `Diagnostic` value before it is pushed — the
    /// anchor-resolving counterpart to [`Self::error_at_span_with_related`],
    /// for callers whose primary span comes from the shared anchor policy
    /// rather than a raw node span.
    pub(crate) fn error_at_anchor_with_related(
        &mut self,
        node_idx: NodeIndex,
        anchor_kind: DiagnosticAnchorKind,
        message: &str,
        code: u32,
        related: Vec<crate::diagnostics::DiagnosticRelatedInformation>,
    ) {
        if let Some(anchor) = self.resolve_diagnostic_anchor(node_idx, anchor_kind) {
            self.error_at_span_with_related(anchor.start, anchor.length, message, code, related);
        }
    }

    /// Emit a generator-related error (TS1221/TS1222) at the `*` asterisk token.
    ///
    /// TSC's `grammarErrorOnNode(node.asteriskToken!, ...)` anchors these errors
    /// at the asterisk, not the function/method node. Since our AST stores
    /// `asterisk_token` as a `bool` (not a node), we scan backward from the
    /// name node's position in source text to locate the `*`.
    pub(crate) fn emit_generator_error_at_asterisk(
        &mut self,
        name_idx: NodeIndex,
        fallback_idx: NodeIndex,
        message: &str,
        code: u32,
    ) {
        // Try to find the `*` by scanning backward from the name node's start position
        if let Some(name_node) = self.ctx.arena.get(name_idx)
            && let Some(sf) = self.ctx.arena.source_files.first()
        {
            let text = sf.text.as_bytes();
            let name_pos = name_node.pos as usize;
            // Scan backward from the name position to find `*`
            for i in (0..name_pos).rev() {
                match text.get(i) {
                    Some(b'*') => {
                        self.error_at_position(i as u32, 1, message, code);
                        return;
                    }
                    Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => continue,
                    _ => break, // Hit a non-whitespace, non-asterisk char — give up
                }
            }
        }
        // Fallback: error at the entire node
        self.error_at_node(fallback_idx, message, code);
    }

    /// Emit a templated diagnostic error at a node.
    ///
    /// Looks up the message template for `code` via `get_message_template`,
    /// formats it with `args`, and emits the error at `node_idx`.
    /// Panics in debug mode if the code has no registered template.
    pub(crate) fn error_at_node_msg(&mut self, node_idx: NodeIndex, code: u32, args: &[&str]) {
        use tsz_common::diagnostics::get_message_template;
        let template = get_message_template(code).unwrap_or("Unexpected checker diagnostic code.");
        let message = format_message(template, args);
        self.error_at_node(node_idx, &message, code);
    }

    /// [`Self::error_at_node_msg`], with `related_information` already
    /// attached via [`Self::error_at_node_with_related`].
    pub(crate) fn error_at_node_msg_with_related(
        &mut self,
        node_idx: NodeIndex,
        code: u32,
        args: &[&str],
        related: Vec<crate::diagnostics::DiagnosticRelatedInformation>,
    ) {
        use tsz_common::diagnostics::get_message_template;
        let template = get_message_template(code).unwrap_or("Unexpected checker diagnostic code.");
        let message = format_message(template, args);
        self.error_at_node_with_related(node_idx, &message, code, related);
    }

    /// Get the source text for a node by extracting from the source file text.
    pub(crate) fn get_source_text_for_node(&self, node_idx: NodeIndex) -> String {
        if let Some((start, end)) = self.get_node_span(node_idx)
            && let Some(sf) = self.ctx.arena.source_files.first()
        {
            let text: &str = &sf.text;
            let s = start as usize;
            let e = end as usize;
            if s <= e && e <= text.len() {
                return text[s..e].to_string();
            }
        }
        String::new()
    }

    /// Report a program-level error (no file location).
    ///
    /// Used for diagnostics that tsc emits globally (e.g., TS2468 "Cannot find
    /// global value 'Promise'") rather than anchored to a specific source location.
    pub(crate) fn error_program_level(&mut self, message: String, code: u32) {
        self.ctx
            .push_diagnostic(Diagnostic::error(String::new(), 0, 0, message, code));
    }

    /// Report an error at a raw `start`/`length` in the file currently being
    /// checked, routing through `push_diagnostic` for consistent deduplication.
    ///
    /// This is the shared construction path for the position-anchored emitters
    /// below (`error_expression_expected_at_position`,
    /// `error_declared_but_never_read`, …): it owns the `file_name.clone()` plus
    /// dedup shape so each emitter only supplies its message and code, and a
    /// future change to the dedup path lands in one place instead of five.
    pub(crate) fn push_error_at(
        &mut self,
        start: u32,
        length: u32,
        message: impl Into<String>,
        code: u32,
    ) {
        self.ctx.push_diagnostic(Diagnostic::error(
            self.ctx.file_name.clone(),
            start,
            length,
            message.into(),
            code,
        ));
    }

    /// Report an error at a specific position.
    ///
    /// Unlike [`Self::push_error_at`] and the position emitters that build on
    /// it, this pushes straight to the diagnostic buffer and intentionally does
    /// **not** deduplicate; converging it onto `push_diagnostic` is a separate
    /// semantic change (it can drop or keep duplicate diagnostics) that needs a
    /// conformance run, so it is kept distinct here rather than folded in.
    pub(crate) fn error_at_position(&mut self, start: u32, length: u32, message: &str, code: u32) {
        self.ctx.diagnostics.push(Diagnostic::error(
            self.ctx.file_name.clone(),
            start,
            length,
            message.to_string(),
            code,
        ));
    }

    /// Report an error at a specific position in a different file.
    /// Used for cross-file diagnostics (e.g., imported global augmentation errors).
    pub(crate) fn error_at_position_in_file(
        &mut self,
        file_name: String,
        start: u32,
        length: u32,
        message: &str,
        code: u32,
    ) {
        self.ctx.diagnostics.push(Diagnostic::error(
            file_name,
            start,
            length,
            message.to_string(),
            code,
        ));
    }

    /// Report TS1109: Expression expected, at a raw source position.
    ///
    /// Used when scanning JSDoc comments for `@import` tags that have empty
    /// or malformed import expressions. Routes through `push_diagnostic` for
    /// consistent deduplication.
    pub(crate) fn error_expression_expected_at_position(&mut self, start: u32, length: u32) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.push_error_at(
            start,
            length,
            diagnostic_messages::EXPRESSION_EXPECTED,
            diagnostic_codes::EXPRESSION_EXPECTED,
        );
    }

    /// Report TS6133: '{name}' is declared but its value is never read.
    ///
    /// Used for unused variables, parameters, imports, and type parameters.
    /// Accepts raw position data since callers compute spans from declaration
    /// nodes directly. Routes through `push_diagnostic` for consistent dedup.
    pub(crate) fn error_declared_but_never_read(&mut self, name: &str, start: u32, length: u32) {
        use crate::diagnostics::diagnostic_codes;
        let message = format!("'{name}' is declared but its value is never read.");
        self.push_error_at(
            start,
            length,
            message,
            diagnostic_codes::IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ,
        );
    }

    /// Report TS6205: All type parameters are unused.
    ///
    /// Used when an entire `@template` tag's type parameters are unused.
    /// Routes through `push_diagnostic` for consistent dedup.
    pub(crate) fn error_all_type_parameters_unused(&mut self, start: u32, length: u32) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.push_error_at(
            start,
            length,
            diagnostic_messages::ALL_TYPE_PARAMETERS_ARE_UNUSED,
            diagnostic_codes::ALL_TYPE_PARAMETERS_ARE_UNUSED,
        );
    }

    /// Report TS6138: "Property '{name}' is declared but its value is never read."
    ///
    /// Used for unused constructor parameter properties (parameters with
    /// `public`, `private`, `protected`, or `readonly` modifiers).
    pub(crate) fn error_property_declared_but_never_read(
        &mut self,
        name: &str,
        start: u32,
        length: u32,
    ) {
        use crate::diagnostics::diagnostic_codes;
        let message = format!("Property '{name}' is declared but its value is never read.");
        self.push_error_at(
            start,
            length,
            message,
            diagnostic_codes::PROPERTY_IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ,
        );
    }

    /// Report TS6196: '{name}' is declared but never used.
    ///
    /// Used for unused type-only declarations (classes, interfaces, type aliases,
    /// enums). Routes through `push_diagnostic` for consistent deduplication.
    pub(crate) fn error_declared_but_never_used(&mut self, name: &str, start: u32, length: u32) {
        use crate::diagnostics::diagnostic_codes;
        let message = format!("'{name}' is declared but never used.");
        self.push_error_at(
            start,
            length,
            message,
            diagnostic_codes::IS_DECLARED_BUT_NEVER_USED,
        );
    }

    /// Report an error at the current node being processed (from resolution stack).
    /// Falls back to the start of the file if no node is in the stack.
    pub(crate) fn error_at_current_node(&mut self, message: &str, code: u32) {
        // Try to use the last node in the resolution stack
        if let Some(&node_idx) = self.ctx.node_resolution_stack.last() {
            self.error_at_node(node_idx, message, code);
        } else {
            // No current node - emit at start of file
            self.error_at_position(0, 0, message, code);
        }
    }

    /// Emit a diagnostic through the central render-request policy.
    ///
    /// This is the single entry point for semantic reporters that have
    /// constructed a `DiagnosticRenderRequest`. It handles:
    /// 1. Anchor resolution (via `resolve_diagnostic_anchor`)
    /// 2. Related-info generation (from failure reason or prebuilt)
    /// 3. Related-info normalization (dedup, limit)
    /// 4. Diagnostic push
    ///
    /// Returns `true` if a diagnostic was emitted, `false` if anchor
    /// resolution failed (no source location).
    pub(crate) fn emit_render_request(
        &mut self,
        node_idx: NodeIndex,
        request: DiagnosticRenderRequest,
    ) -> bool {
        let Some(anchor) = self.resolve_diagnostic_anchor(node_idx, request.anchor_kind) else {
            return false;
        };

        let mut diag = Diagnostic::error(
            self.ctx.file_name.clone(),
            anchor.start,
            anchor.length,
            request.message,
            request.code,
        );

        match request.related {
            RelatedInfoStrategy::None => {}
            RelatedInfoStrategy::FromFailureReason {
                reason,
                source,
                target,
            } => {
                if let Some(related) =
                    self.related_from_failure_reason(&reason, source, target, anchor.node_idx)
                {
                    diag.related_information = related;
                }
            }
            RelatedInfoStrategy::Prebuilt(items) => {
                diag.related_information = items;
            }
        }

        if !diag.related_information.is_empty() {
            diag.related_information = normalize_related_information_blocks(
                std::mem::take(&mut diag.related_information),
                request.related_policy,
            );
        }

        self.ctx.push_diagnostic(diag);
        true
    }

    /// Emit a diagnostic at a pre-resolved anchor.
    ///
    /// Use this when the caller has already resolved the anchor (e.g., to
    /// compute related information that depends on the anchor span). This
    /// avoids double-resolution while still centralizing the emission path.
    pub(crate) fn emit_render_request_at_anchor(
        &mut self,
        anchor: ResolvedDiagnosticAnchor,
        request: DiagnosticRenderRequest,
    ) {
        let mut diag = Diagnostic::error(
            self.ctx.file_name.clone(),
            anchor.start,
            anchor.length,
            request.message,
            request.code,
        );

        match request.related {
            RelatedInfoStrategy::None => {}
            RelatedInfoStrategy::FromFailureReason {
                reason,
                source,
                target,
            } => {
                if let Some(related) =
                    self.related_from_failure_reason(&reason, source, target, anchor.node_idx)
                {
                    diag.related_information = related;
                }
            }
            RelatedInfoStrategy::Prebuilt(items) => {
                diag.related_information = items;
            }
        }

        if !diag.related_information.is_empty() {
            diag.related_information = normalize_related_information_blocks(
                std::mem::take(&mut diag.related_information),
                request.related_policy,
            );
        }

        self.ctx.push_diagnostic(diag);
    }
}
