use crate::source::{SourceText, Span};

use super::Statement;
use super::template_literal::statements_form_no_substitution_template_expression_file;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommentKind {
    Line,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommentPlacement {
    Leading,
    Trailing,
}

/// Scanner-owned comment identity. The parser admits only plain line comments
/// at positions whose JavaScript placement is structurally modeled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommentTrivia {
    pub span: Span,
    pub kind: CommentKind,
    pub placement: CommentPlacement,
    pub has_trailing_line_break: bool,
    pub plain: bool,
}

pub(crate) fn comments_form_no_substitution_template_expression_file(
    source: &SourceText,
    statements: &[Statement],
    comments: &[CommentTrivia],
) -> bool {
    if comments.is_empty() {
        return true;
    }
    let [comment] = comments else {
        return false;
    };
    let [statement] = statements else {
        return false;
    };
    statements_form_no_substitution_template_expression_file(statements)
        && comment.plain
        && comment.kind == CommentKind::Line
        && comment.placement == CommentPlacement::Leading
        && comment.has_trailing_line_break
        && is_column_zero(source, comment.span.start)
        && !source
            .slice(comment.span)
            .chars()
            .last()
            .is_some_and(is_single_line_whitespace)
        && has_one_line_break(source, comment.span.end, statement.span.start)
}

fn has_one_line_break(source: &SourceText, start: u32, end: u32) -> bool {
    let bytes = source.text.as_bytes();
    let Some(gap) = bytes.get(start as usize..end as usize) else {
        return false;
    };
    let mut index = 0;
    match gap.get(index) {
        Some(b'\r') => {
            index += 1;
            if gap.get(index) == Some(&b'\n') {
                index += 1;
            }
        }
        Some(b'\n') => index += 1,
        _ => return false,
    }
    index == gap.len()
}

fn is_column_zero(source: &SourceText, offset: u32) -> bool {
    offset == 0
        || source
            .text
            .as_bytes()
            .get(offset as usize - 1)
            .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
}

pub(crate) const fn is_single_line_whitespace(character: char) -> bool {
    matches!(
        character,
        ' ' | '\t' | '\u{000b}' | '\u{000c}' | '\u{0085}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200b}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}
