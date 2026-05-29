//! Trailing-comment attachment for converted statements.
//!
//! Extracted from `class_es5_ast_to_ir.rs` so the central AST→IR conversion
//! file stays under the §19 2000-line cap. Behavior is unchanged: the
//! attachment policy mirrors `tsc`'s rule that a comment immediately after a
//! statement (on the same line or the next non-blank line, with only
//! whitespace/`;` in between) is treated as trailing for that statement and
//! emitted alongside it.

use super::AstToIr;
use crate::transforms::ir::IRNode;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> AstToIr<'a> {
    pub(super) fn attach_trailing_comment(
        &self,
        node: &tsz_parser::parser::node::Node,
        statement: IRNode,
    ) -> IRNode {
        let Some(source_text) = self.source_text else {
            return statement;
        };
        if let Some(comment_text) = self.included_trailing_line_comment(node, source_text) {
            return IRNode::Sequence(vec![
                statement,
                IRNode::TrailingComment(comment_text.into()),
            ]);
        }
        let scan_start =
            Self::find_actual_statement_end(source_text, node.pos as usize, node.end as usize);
        for comment in crate::emitter::get_trailing_comment_ranges(source_text, scan_start) {
            if !self.comment_starts_before_limit(comment.pos) {
                continue;
            }
            let gap_start = scan_start.min(source_text.len());
            let gap_end = (comment.pos as usize).min(source_text.len());
            if gap_start > gap_end
                || !Self::can_attach_trailing_comment_gap(&source_text[gap_start..gap_end])
            {
                continue;
            }
            let comment_text = &source_text[comment.pos as usize..comment.end as usize];
            let trimmed = comment_text.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") {
                return IRNode::Sequence(vec![
                    statement,
                    IRNode::TrailingComment(comment_text.to_string().into()),
                ]);
            }
        }
        if node.kind == syntax_kind_ext::RETURN_STATEMENT {
            return statement;
        }
        let line_start = scan_start.min(source_text.len());
        let line_end = source_text[line_start..]
            .find(['\n', '\r'])
            .map_or(source_text.len(), |offset| line_start + offset);
        let line = &source_text[line_start..line_end];
        if let Some(comment_start) = Self::attached_line_comment_start(node.kind, line) {
            let comment_pos = line_start + comment_start;
            if !self.comment_starts_before_limit(comment_pos as u32) {
                return statement;
            }
            let comment_text = &line[comment_start..];
            return IRNode::Sequence(vec![
                statement,
                IRNode::TrailingComment(comment_text.to_string().into()),
            ]);
        }
        statement
    }

    /// Scan `[node_start, node_end)` and return the position after the last depth-0
    /// statement terminator (`;` or closing bracket). The parser sets `node.end` to
    /// the NEXT token's end, so we need to scan back to the actual statement boundary
    /// before searching for trailing comments.
    pub(super) fn find_actual_statement_end(
        source_text: &str,
        node_start: usize,
        node_end: usize,
    ) -> usize {
        let bytes = source_text.as_bytes();
        let end = node_end.min(bytes.len());
        let start = node_start.min(end);

        let mut depth: i32 = 0;
        let mut last_stmt_end = end;
        let mut i = start;
        let mut in_string: Option<u8> = None;

        while i < end {
            let ch = bytes[i];
            if let Some(quote) = in_string {
                if ch == b'\\' {
                    i = (i + 2).min(end);
                    continue;
                }
                if ch == quote {
                    in_string = None;
                }
                i += 1;
                continue;
            }
            match ch {
                b'\'' | b'"' | b'`' => {
                    in_string = Some(ch);
                    i += 1;
                }
                b'/' if i + 1 < end && bytes[i + 1] == b'/' => {
                    i += 2;
                    while i < end && bytes[i] != b'\n' && bytes[i] != b'\r' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < end && bytes[i + 1] == b'*' => {
                    i += 2;
                    while i + 1 < end && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                        i += 1;
                    }
                    if i + 1 < end {
                        i += 2;
                    }
                }
                b'{' | b'(' | b'[' => {
                    depth += 1;
                    i += 1;
                }
                b'}' | b')' | b']' => {
                    depth = (depth - 1).max(0);
                    if depth == 0 {
                        last_stmt_end = i + 1;
                    }
                    i += 1;
                }
                b';' if depth == 0 => {
                    last_stmt_end = i + 1;
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        last_stmt_end
    }

    fn included_trailing_line_comment(
        &self,
        node: &tsz_parser::parser::node::Node,
        source_text: &str,
    ) -> Option<String> {
        let node_start = (node.pos as usize).min(source_text.len());
        let node_end = self
            .trailing_comment_limit
            .get()
            .map_or(node.end as usize, |limit| {
                (limit as usize).min(node.end as usize)
            })
            .min(source_text.len());
        if node_start >= node_end {
            return None;
        }

        let mut scan_end = node_end;
        while scan_end > node_start {
            let Some(ch) = source_text[..scan_end].chars().next_back() else {
                break;
            };
            if !ch.is_whitespace() {
                break;
            }
            scan_end -= ch.len_utf8();
        }
        if node_start >= scan_end {
            return None;
        }

        let line_start = source_text[..scan_end]
            .rfind(['\n', '\r'])
            .map_or(node_start, |offset| (offset + 1).max(node_start));
        let line = &source_text[line_start..scan_end];
        let comment_start = Self::line_comment_start(line)?;
        let comment_pos = line_start + comment_start;
        if comment_pos < node_start || !self.comment_starts_before_limit(comment_pos as u32) {
            return None;
        }
        Some(line[comment_start..].trim_end().to_string())
    }

    fn comment_starts_before_limit(&self, comment_pos: u32) -> bool {
        self.trailing_comment_limit
            .get()
            .is_none_or(|limit| comment_pos < limit)
    }

    pub(super) fn attached_line_comment_start(node_kind: u16, line_suffix: &str) -> Option<usize> {
        let comment_start = Self::line_comment_start(line_suffix)?;
        let gap = &line_suffix[..comment_start];
        let gap_belongs_to_statement = if node_kind == syntax_kind_ext::EXPRESSION_STATEMENT {
            Self::expression_statement_trailing_comment_gap(gap)
        } else {
            Self::can_attach_trailing_comment_gap(gap)
        };
        gap_belongs_to_statement.then_some(comment_start)
    }

    fn expression_statement_trailing_comment_gap(gap: &str) -> bool {
        gap.bytes()
            .all(|byte| matches!(byte, b' ' | b'\t' | b';' | b')' | b']' | b'}'))
    }

    pub(super) fn can_attach_trailing_comment_gap(gap: &str) -> bool {
        gap.chars().all(|ch| ch.is_whitespace() || ch == ';')
    }

    fn line_comment_start(line: &str) -> Option<usize> {
        let bytes = line.as_bytes();
        let mut i = 0;
        let mut quote = None;
        while i + 1 < bytes.len() {
            let byte = bytes[i];
            if let Some(delim) = quote {
                if byte == b'\\' {
                    i += 2;
                    continue;
                }
                if byte == delim {
                    quote = None;
                }
                i += 1;
                continue;
            }
            if byte == b'\'' || byte == b'"' || byte == b'`' {
                quote = Some(byte);
                i += 1;
                continue;
            }
            if byte == b'/' && bytes[i + 1] == b'/' {
                return Some(i);
            }
            i += 1;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::AstToIr;
    use tsz_parser::parser::syntax_kind_ext;

    #[test]
    fn attached_line_comment_start_allows_statement_trailing_comments() {
        assert_eq!(
            AstToIr::attached_line_comment_start(syntax_kind_ext::VARIABLE_STATEMENT, " // ok"),
            Some(1)
        );
        assert_eq!(
            AstToIr::attached_line_comment_start(syntax_kind_ext::VARIABLE_STATEMENT, "; // ok"),
            Some(2)
        );
    }

    #[test]
    fn attached_line_comment_start_rejects_comments_after_parent_delimiters() {
        assert_eq!(
            AstToIr::attached_line_comment_start(
                syntax_kind_ext::VARIABLE_STATEMENT,
                " } // not inner",
            ),
            None
        );
        assert_eq!(
            AstToIr::attached_line_comment_start(
                syntax_kind_ext::VARIABLE_STATEMENT,
                " }) // not inner",
            ),
            None
        );
    }

    #[test]
    fn can_attach_trailing_comment_gap_rejects_parent_delimiters() {
        assert!(AstToIr::can_attach_trailing_comment_gap(" ; "));
        assert!(!AstToIr::can_attach_trailing_comment_gap(" } "));
        assert!(!AstToIr::can_attach_trailing_comment_gap(" }) "));
    }
}
