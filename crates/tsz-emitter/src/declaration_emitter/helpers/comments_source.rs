//! Comment emission, source mapping, and writer utilities

#[allow(unused_imports)]
use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
#[allow(unused_imports)]
use crate::emitter::type_printer::TypePrinter;
#[allow(unused_imports)]
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};
#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use tracing::debug;
#[allow(unused_imports)]
use tsz_binder::{BinderState, SymbolId, symbol_flags};
#[allow(unused_imports)]
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
#[allow(unused_imports)]
use tsz_parser::parser::ParserState;
#[allow(unused_imports)]
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn emit_leading_jsdoc_comments(&mut self, pos: u32) {
        if self.remove_comments {
            return;
        }
        let Some(ref text) = self.source_file_text else {
            return;
        };
        let text = text.clone();
        let bytes = text.as_bytes();
        let mut actual_start = pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }
        let actual_start = actual_start as u32;
        while self.comment_emit_idx < self.all_comments.len() {
            let c_pos = self.all_comments[self.comment_emit_idx].pos;
            let c_end = self.all_comments[self.comment_emit_idx].end;
            if c_end > actual_start {
                break;
            }
            let ct = &text[c_pos as usize..c_end as usize];
            // Skip empty block comments like /**/
            if ct.starts_with("/**") && ct != "/**/" {
                let si = {
                    let cp = c_pos as usize;
                    let mut ls = cp;
                    if ls > 0 {
                        let mut i = ls;
                        while i > 0 {
                            i -= 1;
                            if bytes[i] == b'\n' || bytes[i] == b'\r' {
                                ls = i + 1;
                                break;
                            }
                            if i == 0 {
                                ls = 0;
                            }
                        }
                    }
                    let mut w = 0usize;
                    for &b in &bytes[ls..cp] {
                        if b == b' ' {
                            w += 1;
                        } else if b == b'\t' {
                            w = (w / 4 + 1) * 4;
                        } else {
                            break;
                        }
                    }
                    w
                };
                // Check if the next comment is a JSDoc comment on the same
                // source line — if so, emit a space instead of a newline to
                // keep consecutive JSDoc comments on one line (matching tsc).
                let next_idx = self.comment_emit_idx + 1;
                let next_on_same_line = next_idx < self.all_comments.len() && {
                    let n_pos = self.all_comments[next_idx].pos;
                    let n_end = self.all_comments[next_idx].end;
                    n_end <= actual_start && {
                        let between = &text[c_end as usize..n_pos as usize];
                        let next_ct = &text[n_pos as usize..n_end as usize];
                        next_ct.starts_with("/**") && next_ct != "/**/" && !between.contains('\n')
                    }
                };
                self.write_indent();
                if ct.contains('\n') {
                    let mut first = true;
                    for line in ct.split('\n') {
                        if first {
                            self.write(line.trim_end());
                            first = false;
                        } else {
                            self.write_line();
                            let line_bytes = line.as_bytes();
                            // Count leading whitespace visual width
                            // (tabs expand to next multiple of 4)
                            let mut line_ws = 0usize;
                            let mut char_ws = 0usize;
                            for &b in line_bytes.iter() {
                                if b == b' ' {
                                    line_ws += 1;
                                    char_ws += 1;
                                } else if b == b'\t' {
                                    line_ws = (line_ws / 4 + 1) * 4;
                                    char_ws += 1;
                                } else {
                                    break;
                                }
                            }
                            let content = line[char_ws..].trim_end();
                            // Compute output indent: apply the relative offset
                            // from the source /** indent to the output indent.
                            let output_indent = (self.indent_level as usize) * 4;
                            let out_ws = if line_ws >= si {
                                output_indent + (line_ws - si)
                            } else {
                                output_indent.saturating_sub(si - line_ws)
                            };
                            for _ in 0..out_ws {
                                self.write_raw(" ");
                            }
                            self.write(content);
                        }
                    }
                } else {
                    self.write(ct);
                }
                if next_on_same_line {
                    self.write(" ");
                } else {
                    self.write_line();
                }
            }
            self.comment_emit_idx += 1;
        }
    }

    /// Emit all inline block comments (both `/*...*/` and `/**...*/`) that appear
    /// before `name_pos`. Used for variable declarations where tsc preserves
    /// comments between the keyword and the variable name (e.g. `var /*4*/ point`).
    pub(crate) fn emit_inline_block_comments(&mut self, name_pos: u32) {
        if self.remove_comments {
            return;
        }
        let Some(ref text) = self.source_file_text else {
            return;
        };
        let text = text.clone();
        let bytes = text.as_bytes();
        let mut actual_start = name_pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }
        let actual_start = actual_start as u32;
        while self.comment_emit_idx < self.all_comments.len() {
            let comment = &self.all_comments[self.comment_emit_idx];
            if comment.end > actual_start {
                break;
            }
            let ct = &text[comment.pos as usize..comment.end as usize];
            if ct.starts_with("/*") {
                self.write(ct);
                self.write(" ");
            }
            self.comment_emit_idx += 1;
        }
    }

    pub(crate) fn emit_inline_parameter_comment(&mut self, param_pos: u32) {
        if self.remove_comments {
            return;
        }
        let Some(ref text) = self.source_file_text else {
            return;
        };
        let text = text.clone();
        let bytes = text.as_bytes();
        let mut actual_start = param_pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }
        let actual_start = actual_start as u32;
        while self.comment_emit_idx < self.all_comments.len() {
            let comment = &self.all_comments[self.comment_emit_idx];
            if comment.end > actual_start {
                break;
            }
            let c_pos = comment.pos as usize;
            let c_end = comment.end as usize;
            let ct = &text[c_pos..c_end];
            if ct.starts_with("/*") {
                // Determine if this is a "leading" comment (before a parameter name)
                // or a "trailing" comment (after a parameter's type annotation).
                // Leading: preceded by `(`, `,`, `[`, `<`, whitespace, or another comment.
                // Trailing: preceded by identifier chars, `)`, type annotation, etc.
                let is_leading = {
                    let mut p = c_pos;
                    let mut leading = true;
                    while p > 0 {
                        p -= 1;
                        match bytes[p] {
                            b' ' | b'\t' | b'\r' | b'\n' => continue,
                            b'(' | b',' | b'[' | b'<' => break,
                            b'/' if p > 0 && bytes[p - 1] == b'*' => break, // end of another comment
                            _ => {
                                leading = false;
                                break;
                            }
                        }
                    }
                    leading
                };

                if is_leading {
                    // Check if the comment was on a new line in the source.
                    let has_newline = {
                        let mut pos = c_pos;
                        let mut found = false;
                        while pos > 0 {
                            pos -= 1;
                            match bytes[pos] {
                                b'\n' => {
                                    found = true;
                                    break;
                                }
                                b' ' | b'\t' | b'\r' => continue,
                                _ => break,
                            }
                        }
                        found
                    };
                    if has_newline {
                        self.write_line();
                        self.write_indent();
                    }
                    self.write(ct);
                    if has_newline {
                        self.write_line();
                        self.write_indent();
                    } else {
                        self.write(" ");
                    }
                }
            }
            self.comment_emit_idx += 1;
        }
    }

    /// Check if there is a trailing block comment on the same source line as `node_end`,
    /// and if so, emit it (space-separated) before the caller emits a newline.
    /// Returns true if a trailing comment was emitted.
    pub(crate) fn emit_trailing_comment(&mut self, node_end: u32) -> bool {
        if self.remove_comments {
            return false;
        }
        let Some(ref text) = self.source_file_text else {
            return false;
        };
        let text = text.clone();
        let bytes = text.as_bytes();
        if self.comment_emit_idx >= self.all_comments.len() {
            return false;
        }
        let c_pos = self.all_comments[self.comment_emit_idx].pos;
        let c_end = self.all_comments[self.comment_emit_idx].end;
        // The comment must start after the node end
        if c_pos < node_end {
            return false;
        }
        let ct = &text[c_pos as usize..c_end as usize];
        // Only handle block comments (/* ... */), not line comments
        if !ct.starts_with("/*") {
            return false;
        }
        // Check that there's no newline between node_end and the comment start
        let between = &bytes[node_end as usize..c_pos as usize];
        if between.contains(&b'\n') || between.contains(&b'\r') {
            return false;
        }
        // Emit as trailing comment
        self.write(" ");
        self.write(ct);
        self.comment_emit_idx += 1;
        true
    }

    /// Advance the comment index past any comments that end before `pos`,
    /// without emitting them. Used to skip comments that belong to a parent
    /// context (e.g. comments between `:` and the type's opening paren).
    pub(crate) fn skip_comments_before(&mut self, pos: u32) {
        while self.comment_emit_idx < self.all_comments.len() {
            if self.all_comments[self.comment_emit_idx].end <= pos {
                self.comment_emit_idx += 1;
            } else {
                break;
            }
        }
    }

    pub(crate) fn skip_comments_in_node(&mut self, pos: u32, end: u32) {
        let ae = self.find_node_code_end(pos, end);
        while self.comment_emit_idx < self.all_comments.len() {
            if self.all_comments[self.comment_emit_idx].pos < ae {
                self.comment_emit_idx += 1;
            } else {
                break;
            }
        }
    }

    pub(in crate::declaration_emitter) fn find_node_code_end(&self, pos: u32, end: u32) -> u32 {
        let Some(ref text) = self.source_file_text else {
            return end;
        };
        let bytes = text.as_bytes();
        let s = pos as usize;
        let e = std::cmp::min(end as usize, bytes.len());
        if s >= e {
            return end;
        }
        let mut d: i32 = 0;
        let mut lt: Option<usize> = None;
        let mut i = s;
        while i < e {
            match bytes[i] {
                b'{' => {
                    d += 1;
                    i += 1;
                }
                b'}' => {
                    d -= 1;
                    if d == 0 {
                        lt = Some(i + 1);
                    }
                    i += 1;
                }
                b';' => {
                    if d == 0 {
                        lt = Some(i + 1);
                    }
                    i += 1;
                }
                b'\'' | b'"' | b'`' => {
                    let q = bytes[i];
                    i += 1;
                    while i < e {
                        if bytes[i] == b'\\' {
                            i += 2;
                        } else if bytes[i] == q {
                            i += 1;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                }
                b'/' if i + 1 < e && bytes[i + 1] == b'/' => {
                    i += 2;
                    while i < e && bytes[i] != b'\n' && bytes[i] != b'\r' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < e && bytes[i + 1] == b'*' => {
                    i += 2;
                    while i + 1 < e {
                        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }
        lt.map_or(end, |x| x as u32)
    }

    pub(crate) fn queue_source_mapping(&mut self, node: &Node) {
        if !self.writer.has_source_map() {
            self.pending_source_pos = None;
            return;
        }

        let Some(text) = self.source_map_text else {
            self.pending_source_pos = None;
            return;
        };

        self.pending_source_pos = Some(source_position_from_offset(text, node.pos));
    }

    pub(crate) const fn take_pending_source_pos(&mut self) -> Option<SourcePosition> {
        self.pending_source_pos.take()
    }

    /// Returns the quote character used for a string literal in the original source.
    /// Falls back to double quote if source text is unavailable.
    pub(crate) fn original_quote_char(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> &'static str {
        if let Some(text) = self.source_file_text.as_ref() {
            let pos = node.pos as usize;
            if pos < text.len() {
                let ch = text.as_bytes()[pos];
                if ch == b'\'' {
                    return "'";
                }
            }
        }
        "\""
    }

    pub(crate) fn get_source_slice(&self, start: u32, end: u32) -> Option<String> {
        let text = self.source_file_text.as_ref()?;
        let start = start as usize;
        let end = end as usize;
        if start > end || end > text.len() {
            return None;
        }

        let slice = text[start..end].trim().to_string();
        if slice.is_empty() { None } else { Some(slice) }
    }

    /// Like `get_source_slice` but also strips a trailing `;` if present.
    /// Use this when extracting type/value text from source that will be
    /// embedded in a statement where the caller adds its own `;`.
    pub(crate) fn get_source_slice_no_semi(&self, start: u32, end: u32) -> Option<String> {
        let mut s = self.get_source_slice(start, end)?;
        if s.ends_with(';') {
            s.pop();
            let trimmed = s.trim_end().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        } else {
            Some(s)
        }
    }

    pub(crate) fn write_raw(&mut self, s: &str) {
        self.writer.write(s);
    }

    pub(crate) fn write(&mut self, s: &str) {
        if let Some(source_pos) = self.take_pending_source_pos() {
            self.writer.write_node(s, source_pos);
        } else {
            self.writer.write(s);
        }
    }

    pub(crate) fn write_line(&mut self) {
        self.writer.write_line();
    }

    pub(crate) fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.write_raw("    ");
        }
    }

    pub(crate) const fn increase_indent(&mut self) {
        self.indent_level += 1;
    }

    pub(crate) const fn decrease_indent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }
}
