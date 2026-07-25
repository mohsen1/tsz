//! TS1005 for Closure-style `function(...)` JSDoc types.
//!
//! TypeScript 7 does not accept the Closure function-type form. For
//! `@type {function(string): void}` it reports TS1005 `'}' expected.` anchored
//! at the open paren and gives the annotated symbol an error type, rather than
//! reconstructing a signature the way TypeScript 6 did. Across the corpus, 43
//! of 47 JSDoc `function(` sites carry TS1005 or TS1003 in the oracle; the
//! exceptions are the `@enum` tag, which TS7 does not implement at all (so it
//! never parses the tag's type), and `.ts`/`.tsx` files, where JSDoc types are
//! not used as types.

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    /// Report TS1005 for a Closure `function(...)` type written after any of
    /// `tags` on a single `JSDoc` line.
    ///
    /// `check_jsdoc_param_tag_syntax` already walks the comment line by line
    /// with an absolute `line_start`, so tags other than `@param` reuse that
    /// walk rather than re-deriving comment positions.
    pub(super) fn check_closure_function_type_on_tag_line(
        &mut self,
        raw_line: &str,
        line_start: usize,
        tags: &[&str],
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        for tag in tags {
            let Some(at_tag) = Self::jsdoc_tag_offset(raw_line, tag) else {
                continue;
            };
            let after_tag_start = at_tag + Self::jsdoc_tag_source_len(tag);
            let Some(after_tag) = raw_line.get(after_tag_start..) else {
                continue;
            };
            let trimmed = after_tag.trim_start();
            let leading_ws = after_tag.len() - trimmed.len();
            if !trimmed.starts_with('{') {
                continue;
            }
            let type_open = after_tag_start + leading_ws;
            let Some((type_expr, _)) =
                Self::parse_jsdoc_curly_type_expr(raw_line.get(type_open..).unwrap_or_default())
            else {
                continue;
            };
            let Some(paren_offset) = Self::jsdoc_closure_function_type_offset(type_expr) else {
                continue;
            };
            let error_pos = (line_start + type_open + 1 + paren_offset) as u32;
            let close_brace_expected = format_message(diagnostic_messages::EXPECTED, &["}"]);
            self.emit_jsdoc_param_syntax_diagnostic_once(
                error_pos,
                1,
                &close_brace_expected,
                diagnostic_codes::EXPECTED,
            );
            return;
        }
    }

    /// Whole-file pass: TS1005 for every Closure `function(...)` JSDoc type.
    ///
    /// `check_closure_function_type_on_tag_line` only sees comments attached to
    /// a function, which covers `@param`/`@return`. A `@type` tag sits on a
    /// variable or property just as often, so those need a scan over every
    /// JSDoc comment in the file. Emission is deduped by position, so the
    /// overlap with the per-function walk is harmless.
    ///
    /// JS files only: in `.ts`/`.tsx`, JSDoc types are not types, and the
    /// pinned compiler reports nothing for this construct there.
    pub(crate) fn check_jsdoc_closure_function_types(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_common::comments::is_jsdoc_comment;

        const TAGS: [&str; 6] = ["type", "param", "arg", "argument", "return", "returns"];

        let mut sites: Vec<u32> = Vec::new();
        {
            let Some(sf) = self.ctx.arena.source_files.first() else {
                return;
            };
            let source_text = sf.text.as_ref();
            for comment in &sf.comments {
                if !is_jsdoc_comment(comment, source_text) {
                    continue;
                }
                let end = (comment.end as usize).min(source_text.len());
                let Some(text) = source_text.get(comment.pos as usize..end) else {
                    continue;
                };
                let mut line_start = 0usize;
                for chunk in text.split_inclusive('\n') {
                    let raw_line = chunk.trim_end_matches('\n').trim_end_matches('\r');
                    for tag in TAGS {
                        let Some(at_tag) = Self::jsdoc_tag_offset(raw_line, tag) else {
                            continue;
                        };
                        let after_start = at_tag + Self::jsdoc_tag_source_len(tag);
                        let Some(after) = raw_line.get(after_start..) else {
                            continue;
                        };
                        let trimmed = after.trim_start();
                        if !trimmed.starts_with('{') {
                            continue;
                        }
                        let type_open = after_start + (after.len() - trimmed.len());
                        let Some((type_expr, _)) = Self::parse_jsdoc_curly_type_expr(
                            raw_line.get(type_open..).unwrap_or_default(),
                        ) else {
                            continue;
                        };
                        let Some(paren) = Self::jsdoc_closure_function_type_offset(type_expr)
                        else {
                            continue;
                        };
                        sites.push(comment.pos + (line_start + type_open + 1 + paren) as u32);
                        break;
                    }
                    line_start += chunk.len();
                }
            }
        }

        let message = format_message(diagnostic_messages::EXPECTED, &["}"]);
        for pos in sites {
            self.emit_jsdoc_param_syntax_diagnostic_once(
                pos,
                1,
                &message,
                diagnostic_codes::EXPECTED,
            );
        }
    }
}
