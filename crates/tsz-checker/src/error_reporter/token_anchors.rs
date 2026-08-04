//! Token-anchored diagnostic spans.
//!
//! A family of `tsc` grammar diagnostics is anchored with
//! `getSpanOfTokenAtPosition(sourceFile, pos)` rather than with the span of
//! the node that triggered them:
//!
//! ```js
//! function getSpanOfTokenAtPosition(sourceFile, pos) {
//!   const scanner = createScanner(..., sourceFile.text, /*onError*/ undefined, pos);
//!   scanner.scan();
//!   return createTextSpanFromBounds(scanner.getTokenStart(), scanner.getTokenEnd());
//! }
//! ```
//!
//! That is: skip trivia from `pos`, scan exactly one token, and report that
//! token's own bounds. `checkAwaitExpression`'s TS1308/TS1375/TS1309/TS1378
//! all use it, so `tsc` squiggles the five characters of the `await` keyword
//! and never the operand or the statement's trailing `;`.
//!
//! Anchoring on the node instead produces the same `start` (so `--pretty
//! false` output, and therefore the conformance corpus, is byte-identical)
//! but a longer `length` — visible in `--pretty` mode, which is `tsc`'s
//! default, and in every editor surface that draws the squiggle from the
//! diagnostic's length.
//!
//! These helpers own that rule so call sites do not re-derive it with fixed
//! offsets. A hand-computed `node.pos + 4` for the `await` of a `for await`
//! is right only when exactly one space separates the two keywords; the
//! scanner is right for `for /* c */ await` and for a `pos` that still
//! carries leading trivia.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::ScannerState;

impl CheckerState<'_> {
    /// The span of the `skip`-th token (0-based) at or after `pos`, as
    /// `(start, length)`.
    ///
    /// Mirrors `tsc`'s `getSpanOfTokenAtPosition` for `skip == 0`; larger
    /// `skip` values walk forward token by token, which is how a keyword that
    /// `tsc` reaches through a dedicated child node (`ForOfStatement`'s
    /// `awaitModifier`) is located in an arena that does not store one.
    ///
    /// Returns `None` when the scan runs out of tokens or lands on an
    /// unrecognized one, so callers fall back to their node anchor rather
    /// than reporting a zero-width or nonsense span.
    ///
    /// Cost: builds a scanner over the file's text, so this belongs on
    /// diagnostic paths only — never in a checking hot loop.
    pub(crate) fn span_of_token_at_position(&self, pos: u32, skip: usize) -> Option<(u32, u32)> {
        let source_file = self.ctx.arena.source_files.first()?;
        let text: &str = &source_file.text;
        if pos as usize > text.len() {
            return None;
        }

        let mut scanner = ScannerState::new(text.to_string(), true);
        scanner.reset_token_state(pos as usize);

        for _ in 0..=skip {
            match scanner.scan() {
                SyntaxKind::EndOfFileToken | SyntaxKind::Unknown => return None,
                _ => {}
            }
        }

        let start = scanner.get_token_start();
        let end = scanner.get_token_end();
        if end <= start {
            return None;
        }
        u32::try_from(start)
            .ok()
            .zip(u32::try_from(end - start).ok())
    }

    /// The span of `node`'s own first token.
    ///
    /// The direct analogue of `getSpanOfTokenAtPosition(sourceFile, node.pos)`
    /// — the form every `checkAwaitExpression` diagnostic uses.
    pub(crate) fn span_of_first_token_of_node(&self, idx: NodeIndex) -> Option<(u32, u32)> {
        let (pos, _) = self.get_node_span(idx)?;
        self.span_of_token_at_position(pos, 0)
    }

    /// The span of the token that follows `node`'s first token.
    ///
    /// `for await (... of ...)` stores no `awaitModifier` node in this arena,
    /// so the keyword `tsc` anchors on is located as "the token after `for`".
    pub(crate) fn span_of_second_token_of_node(&self, idx: NodeIndex) -> Option<(u32, u32)> {
        let (pos, _) = self.get_node_span(idx)?;
        self.span_of_token_at_position(pos, 1)
    }

    /// Emit `code` anchored on `idx`'s own first token, the way `tsc` anchors
    /// a `getSpanOfTokenAtPosition(sourceFile, node.pos)` diagnostic.
    ///
    /// Falls back to the node anchor when the token cannot be scanned, so a
    /// diagnostic is never lost to a failed anchor lookup. Routes through the
    /// same deduplicating emitter as [`Self::error_at_node`].
    pub(crate) fn error_at_first_token_of_node(
        &mut self,
        idx: NodeIndex,
        message: &str,
        code: u32,
    ) {
        if let Some((start, length)) = self.span_of_first_token_of_node(idx) {
            self.error(start, length, message.to_string(), code);
        } else {
            self.error_at_node(idx, message, code);
        }
    }

    /// [`Self::error_at_first_token_of_node`] with related information already
    /// built — `tsc`'s `checkAwaitExpression` attaches its TS1356 "Did you mean
    /// to mark this function as 'async'?" pointer to exactly this diagnostic.
    pub(crate) fn error_at_first_token_of_node_with_related(
        &mut self,
        idx: NodeIndex,
        message: &str,
        code: u32,
        related: Vec<crate::diagnostics::DiagnosticRelatedInformation>,
    ) {
        if let Some((start, length)) = self.span_of_first_token_of_node(idx) {
            self.error_at_span_with_related(start, length, message, code, related);
        } else {
            self.error_at_node_with_related(idx, message, code, related);
        }
    }
}

#[cfg(test)]
#[path = "token_anchors_tests.rs"]
mod token_anchors_tests;
