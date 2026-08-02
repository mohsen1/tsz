//! JSDoc `@typedef` name-syntax diagnostics for `CheckerState`.

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    /// Offsets, relative to the start of `comment_text`, of the closing `}` of
    /// every `@typedef {Type}` tag in that JSDoc comment that carries **no name**
    /// after its braced type expression.
    ///
    /// The nameless form is one tsc fact with two consequences, so it has one
    /// scanner: the tag is a grammar error (TS1003, `check_jsdoc_typedef_missing_name`)
    /// *and* it still declares a type alias named after the declaration the
    /// comment annotates (`jsdoc_nameless_typedef_host_name`). Keeping the
    /// detection in one place is what stops those two consumers from drifting.
    pub(crate) fn jsdoc_nameless_typedef_close_offsets(comment_text: &str) -> Vec<usize> {
        fn balanced_close(s: &str) -> Option<usize> {
            let mut depth: i32 = 1;
            for (i, ch) in s.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i);
                        }
                    }
                    _ => {}
                }
            }
            None
        }

        // Whether a typedef name follows the type expression. Continuation
        // whitespace and leading `*` line markers are skipped; a name may be a
        // plain or dotted identifier, both of which begin with an identifier
        // start character.
        fn name_follows(after: &str) -> bool {
            for ch in after.chars() {
                if ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n' || ch == '*' {
                    continue;
                }
                return ch == '_' || ch == '$' || ch.is_ascii_alphabetic();
            }
            false
        }

        let mut offsets = Vec::new();
        for tag_off in Self::jsdoc_tag_offsets(comment_text, "typedef") {
            let after_tag = tag_off + "@typedef".len();
            let rest = &comment_text[after_tag..];
            let trimmed = rest.trim_start();
            if !trimmed.starts_with('{') {
                continue;
            }
            let leading = rest.len() - trimmed.len();
            let brace_off = after_tag + leading;
            let Some(close_rel) = balanced_close(&comment_text[brace_off + 1..]) else {
                continue;
            };
            let close_off = brace_off + 1 + close_rel;
            if name_follows(&comment_text[close_off + 1..]) {
                continue;
            }
            offsets.push(close_off);
        }
        offsets
    }

    /// TS1003: `@typedef {Type}` with no name after the type expression.
    /// TypeScript's JSDoc parser requires a typedef name to follow the braced
    /// type; when the tag ends (comment end or the next `@tag`) with no name it
    /// reports "Identifier expected." at the closing brace of the type
    /// expression. Runs once per JS source file over every JSDoc comment.
    ///
    /// Only the nameless form is handled here. Malformed *names* (deprecated
    /// `.<T>` generics, `~` inner namepaths, `module:` import types) are a
    /// distinct type-expression parse concern and are left to their owners.
    pub(crate) fn check_jsdoc_typedef_missing_name(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::comments::is_jsdoc_comment;

        if !self.is_js_file() {
            return;
        }
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;

        let mut anchors: Vec<u32> = Vec::new();
        for comment in &sf.comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }
            let text = comment.get_text(source_text);
            let base = comment.pos;
            for close_off in Self::jsdoc_nameless_typedef_close_offsets(text) {
                anchors.push(base + close_off as u32);
            }
        }

        for anchor in anchors {
            self.ctx.error(
                anchor,
                1,
                diagnostic_messages::IDENTIFIER_EXPECTED.to_string(),
                diagnostic_codes::IDENTIFIER_EXPECTED,
            );
        }
    }
}
