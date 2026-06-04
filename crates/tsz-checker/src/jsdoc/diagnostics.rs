use crate::state::CheckerState;

use rustc_hash::{FxHashMap, FxHashSet};

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

#[derive(Clone)]
struct JsdocNamedDecl {
    name: String,
    pos: u32,
    len: u32,
    file_idx: usize,
    is_global_script_decl: bool,
}

include!("diagnostics_parts/part1.rs");
include!("diagnostics_parts/part2.rs");

impl<'a> CheckerState<'a> {
    pub(crate) fn report_malformed_jsdoc_satisfies_tags(&mut self, idx: NodeIndex) {
        use tsz_common::comments::is_jsdoc_comment;

        if !self.ctx.should_resolve_jsdoc() {
            return;
        }

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        if let Some((_jsdoc, jsdoc_start)) =
            self.try_jsdoc_with_ancestor_walk_and_pos(idx, comments, source_text)
            && let Some(comment) = comments.iter().find(|c| c.pos == jsdoc_start)
        {
            self.emit_malformed_jsdoc_satisfies_diagnostics(source_text, comment.pos, comment.end);
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };
        if var_decl.initializer.is_none() {
            return;
        }

        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return;
        };
        if let Some((_, pos)) =
            self.try_leading_jsdoc_with_pos(comments, init_node.pos, source_text)
            && let Some(comment) = comments
                .iter()
                .find(|c| c.pos == pos)
                .filter(|c| is_jsdoc_comment(c, source_text))
        {
            self.emit_malformed_jsdoc_satisfies_diagnostics(source_text, comment.pos, comment.end);
        }
    }

    fn emit_malformed_jsdoc_satisfies_diagnostics(
        &mut self,
        source_text: &str,
        comment_pos: u32,
        comment_end: u32,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        for (name_pos, name_len, name) in
            Self::malformed_jsdoc_satisfies_unexpected_names(source_text, comment_pos, comment_end)
        {
            if self.resolve_jsdoc_type_str(&name).is_some() {
                continue;
            }
            self.ctx.error(
                name_pos,
                name_len,
                format_message(diagnostic_messages::CANNOT_FIND_NAME, &[&name]),
                diagnostic_codes::CANNOT_FIND_NAME,
            );
        }
        for (open_pos, close_pos) in
            Self::malformed_jsdoc_satisfies_positions(source_text, comment_pos, comment_end)
        {
            self.ctx.error(
                open_pos,
                0,
                format_message(diagnostic_messages::EXPECTED, &["{"]),
                diagnostic_codes::EXPECTED,
            );
            self.ctx.error(
                close_pos,
                0,
                format_message(diagnostic_messages::EXPECTED, &["}"]),
                diagnostic_codes::EXPECTED,
            );
        }
    }

    pub(crate) fn report_duplicate_jsdoc_satisfies_tags(&mut self, idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_common::comments::is_jsdoc_comment;

        if !self.ctx.should_resolve_jsdoc() {
            return;
        }

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        let mut attached_positions: Vec<u32> = Vec::new();
        let mut attached_comment_pos = None;
        if let Some((_jsdoc, jsdoc_start)) =
            self.try_jsdoc_with_ancestor_walk_and_pos(idx, comments, source_text)
        {
            if let Some(comment) = comments.iter().find(|c| c.pos == jsdoc_start) {
                let malformed_positions = Self::malformed_jsdoc_satisfies_positions(
                    source_text,
                    comment.pos,
                    comment.end,
                );
                if malformed_positions.is_empty() {
                    let raw = &source_text[comment.pos as usize..comment.end as usize];
                    attached_positions = Self::jsdoc_satisfies_keyword_positions(raw, jsdoc_start);
                }
            }
            attached_comment_pos = Some(jsdoc_start);
            self.emit_duplicate_jsdoc_satisfies_positions(&attached_positions);
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };
        if var_decl.initializer.is_none() {
            return;
        }

        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return;
        };
        let Some(comment) = self
            .try_leading_jsdoc_with_pos(comments, init_node.pos, source_text)
            .and_then(|(_, pos)| comments.iter().find(|c| c.pos == pos))
            .filter(|c| is_jsdoc_comment(c, source_text))
        else {
            return;
        };

        let inline_positions =
            if Self::malformed_jsdoc_satisfies_positions(source_text, comment.pos, comment.end)
                .is_empty()
            {
                Self::jsdoc_satisfies_keyword_positions(
                    &source_text[comment.pos as usize..comment.end as usize],
                    comment.pos,
                )
            } else {
                Vec::new()
            };
        self.emit_duplicate_jsdoc_satisfies_positions(&inline_positions);

        if !attached_positions.is_empty()
            && !inline_positions.is_empty()
            && attached_comment_pos != Some(comment.pos)
        {
            let message =
                format_message(diagnostic_messages::TAG_ALREADY_SPECIFIED, &["satisfies"]);
            self.ctx.error(
                attached_positions[0],
                "satisfies".len() as u32,
                message,
                diagnostic_codes::TAG_ALREADY_SPECIFIED,
            );
        }
    }

    fn jsdoc_satisfies_keyword_positions(jsdoc: &str, jsdoc_start: u32) -> Vec<u32> {
        Self::jsdoc_tag_offsets(jsdoc, "satisfies")
            .into_iter()
            .map(|absolute| jsdoc_start + absolute as u32 + 1)
            .collect()
    }

    fn emit_duplicate_jsdoc_satisfies_positions(&mut self, positions: &[u32]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if positions.len() < 2 {
            return;
        }
        let message = format_message(diagnostic_messages::TAG_ALREADY_SPECIFIED, &["satisfies"]);
        for &pos in &positions[1..] {
            self.ctx.error(
                pos,
                "satisfies".len() as u32,
                message.clone(),
                diagnostic_codes::TAG_ALREADY_SPECIFIED,
            );
        }
    }

    /// Extract `@param {BareType}` and `@return {BareType}` / `@returns {BareType}` type
    /// expressions from a raw JSDoc comment block (the full `/**...*/` text slice).
    ///
    /// Returns `(type_expr_str, byte_offset_from_comment_start)` for each match where
    /// the type expression is a simple identifier (no `<`, `|`, `&`, `[`, `(`, `.`, spaces).
    /// Only bare names are returned; already-parameterized types like `Array<T>` are excluded.
    fn jsdoc_bare_param_return_type_spans(comment_text: &str) -> Vec<(String, usize)> {
        let mut results = Vec::new();
        let mut line_offset = 0usize;
        for raw_line in comment_text.split_inclusive('\n') {
            let line = raw_line.trim_end_matches('\n').trim_end_matches('\r');
            let trimmed = line.trim().trim_start_matches('*').trim();
            let is_param_or_return = Self::strip_jsdoc_tag_prefix(trimmed, "param").is_some()
                || Self::strip_jsdoc_return_tag_prefix(trimmed).is_some();
            if is_param_or_return && let Some(open_pos_in_line) = line.find('{') {
                let after_open = &line[open_pos_in_line + 1..];
                if let Some(close_rel) = after_open.find('}') {
                    let raw_type = &after_open[..close_rel];
                    let type_expr = raw_type.trim();
                    if !type_expr.is_empty()
                        && !type_expr.contains('<')
                        && !type_expr.contains('|')
                        && !type_expr.contains('&')
                        && !type_expr.contains('[')
                        && !type_expr.contains('(')
                        && !type_expr.contains('.')
                        && !type_expr.contains(' ')
                        && !type_expr.contains('\t')
                    {
                        let ws_before = raw_type.len().saturating_sub(raw_type.trim_start().len());
                        let type_expr_offset = line_offset + open_pos_in_line + 1 + ws_before;
                        results.push((type_expr.to_string(), type_expr_offset));
                    }
                }
            }
            line_offset += raw_line.len();
        }
        results
    }

    fn jsdoc_param_return_type_spans(comment_text: &str) -> Vec<(String, usize)> {
        let mut results = Vec::new();
        let mut line_offset = 0usize;
        for raw_line in comment_text.split_inclusive('\n') {
            let line = raw_line.trim_end_matches('\n').trim_end_matches('\r');
            // Strip both single-line (`/** … */`) and multi-line (`* …`)
            // JSDoc framing so single-line JSDoc tags like
            // `/** @param {T} y */` are recognised. Without the `/**` /
            // `*/` strip, the per-line `@param` detection below missed
            // every single-line JSDoc, leaking diagnostics like #3506.
            let trimmed = line
                .trim()
                .trim_start_matches("/**")
                .trim_start_matches('*')
                .trim_end_matches("*/")
                .trim();
            let is_param_or_return = trimmed.starts_with("@param ")
                || trimmed.starts_with("@param\t")
                || trimmed.starts_with("@param{")
                || trimmed.starts_with("@returns ")
                || trimmed.starts_with("@returns\t")
                || trimmed.starts_with("@returns{")
                || trimmed.starts_with("@return ")
                || trimmed.starts_with("@return\t")
                || trimmed.starts_with("@return{");
            if is_param_or_return && let Some(open_pos_in_line) = line.find('{') {
                let after_open = &line[open_pos_in_line + 1..];
                // Balance nested braces so `@param {{ a: T }} obj` extracts
                // the full `{ a: T }` body, not just `{ a: T`.
                let mut depth = 1usize;
                let mut close_rel = None;
                for (i, ch) in after_open.char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                close_rel = Some(i);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(close_rel) = close_rel {
                    let raw_type = &after_open[..close_rel];
                    let type_expr = raw_type.trim();
                    if !type_expr.is_empty() {
                        let ws_before = raw_type.len().saturating_sub(raw_type.trim_start().len());
                        let type_expr_offset = line_offset + open_pos_in_line + 1 + ws_before;
                        results.push((type_expr.to_string(), type_expr_offset));
                    }
                }
            }
            line_offset += raw_line.len();
        }
        results
    }

    fn malformed_jsdoc_satisfies_positions(
        source_text: &str,
        comment_pos: u32,
        comment_end: u32,
    ) -> Vec<(u32, u32)> {
        let raw = &source_text[comment_pos as usize..comment_end as usize];
        let mut result = Vec::new();
        for tag_start in Self::jsdoc_tag_offsets(raw, "satisfies") {
            let after_tag = tag_start + "@satisfies".len();
            let ws_trimmed = raw[after_tag..].trim_start_matches(char::is_whitespace);
            let skipped = raw[after_tag..].len() - ws_trimmed.len();
            if !ws_trimmed.starts_with('{') {
                let open_pos = comment_pos + (after_tag + skipped) as u32;
                let close_pos = comment_end.saturating_sub(2);
                result.push((open_pos, close_pos));
            }
        }
        result
    }

    fn malformed_jsdoc_satisfies_unexpected_names(
        source_text: &str,
        comment_pos: u32,
        comment_end: u32,
    ) -> Vec<(u32, u32, String)> {
        let raw = &source_text[comment_pos as usize..comment_end as usize];
        let mut result = Vec::new();
        for tag_start in Self::jsdoc_tag_offsets(raw, "satisfies") {
            let after_tag = tag_start + "@satisfies".len();
            let ws_trimmed = raw[after_tag..].trim_start_matches(char::is_whitespace);
            let skipped = raw[after_tag..].len() - ws_trimmed.len();
            if ws_trimmed.starts_with('{') {
                continue;
            }

            let name_start = after_tag + skipped;
            let mut name_end = name_start;
            for (offset, ch) in raw[name_start..].char_indices() {
                let is_first = offset == 0;
                let is_name_char = if is_first {
                    ch.is_ascii_alphabetic() || ch == '_' || ch == '$'
                } else {
                    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
                };
                if !is_name_char {
                    break;
                }
                name_end = name_start + offset + ch.len_utf8();
            }
            if name_end > name_start {
                result.push((
                    comment_pos + name_start as u32,
                    (name_end - name_start) as u32,
                    raw[name_start..name_end].to_string(),
                ));
            }
        }
        result
    }
}
