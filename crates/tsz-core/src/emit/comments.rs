use crate::source::SourceText;
use crate::syntax::{
    CommentKind, CommentPlacement, CommentTrivia, Expression, SourceUnit, Statement, StatementKind,
};

use super::Printer;

#[derive(Default)]
pub(super) struct CommentCursor {
    comments: Vec<CommentTrivia>,
    next: usize,
}

impl CommentCursor {
    pub(super) fn has_comment_within(&self, start: u32, end: u32) -> bool {
        self.comments
            .iter()
            .skip(self.next)
            .any(|comment| start <= comment.span.start && comment.span.end <= end)
    }

    fn reset(&mut self, comments: &[CommentTrivia]) {
        *self = Self {
            comments: comments.to_vec(),
            next: 0,
        };
    }

    fn write_while(
        &mut self,
        source: &SourceText,
        output: &mut String,
        followed_by_token: bool,
        mut take: impl FnMut(CommentTrivia) -> bool,
    ) {
        let mut separate_from_token = false;
        while let Some(&comment) = self.comments.get(self.next) {
            if !take(comment) {
                break;
            }
            self.next += 1;
            if comment.placement == CommentPlacement::Leading
                && !output.is_empty()
                && !output.ends_with('\n')
            {
                output.push('\n');
            } else if comment.placement == CommentPlacement::Trailing
                && !output.chars().last().is_some_and(char::is_whitespace)
            {
                output.push(' ');
            }
            output.push_str(source.slice(comment.span));
            if comment.kind == CommentKind::Line && !output.ends_with('\n') {
                output.push('\n');
            }
            separate_from_token = comment.kind == CommentKind::Block
                && comment.placement == CommentPlacement::Leading;
        }
        if followed_by_token
            && separate_from_token
            && !output.chars().last().is_some_and(char::is_whitespace)
        {
            output.push(' ');
        }
    }

    fn discard_while(&mut self, mut take: impl FnMut(CommentTrivia) -> bool) {
        while self.comments.get(self.next).copied().is_some_and(&mut take) {
            self.next += 1;
        }
    }
}

impl Printer<'_> {
    pub(super) fn write_javascript_statements(&mut self, unit: &SourceUnit) {
        if self.preserve_comments {
            self.comment_cursor.reset(unit.comments());
        }
        for statement in &unit.statements {
            self.write_javascript_statement(statement, true);
        }
        self.comment_cursor
            .write_while(self.source, &mut self.output, false, |_| true);
    }

    pub(super) fn write_comments_before(&mut self, offset: u32) {
        self.comment_cursor
            .write_while(self.source, &mut self.output, true, |comment| {
                comment.span.start < offset
            });
    }

    pub(super) fn write_comments_through_token(&mut self, token_end: u32) {
        self.comment_cursor
            .write_while(self.source, &mut self.output, true, |comment| {
                comment
                    .preceding_token_end
                    .is_some_and(|preceding| preceding <= token_end)
            });
    }

    pub(super) fn write_comments_before_parameter(&mut self, offset: u32) {
        let boundary = self.output.len();
        self.write_comments_before(offset);
        if self.output.len() > boundary {
            if self.output[boundary..].starts_with(' ') {
                self.output.remove(boundary);
            }
            if !self.output.chars().last().is_some_and(char::is_whitespace) {
                self.output.push(' ');
            }
        }
    }

    pub(super) fn write_comments_after_parameter_open(&mut self) {
        let bytes = self.source.text.as_bytes();
        self.comment_cursor
            .write_while(self.source, &mut self.output, false, |comment| {
                comment
                    .preceding_token_end
                    .and_then(|end| bytes.get(end.saturating_sub(1) as usize))
                    == Some(&b'(')
            });
    }

    pub(super) fn discard_comments_through_token(&mut self, token_end: u32) {
        self.comment_cursor.discard_while(|comment| {
            comment
                .preceding_token_end
                .is_some_and(|preceding| preceding <= token_end)
        });
    }

    pub(super) fn discard_erased_statement_comments(&mut self, statement: &Statement) -> bool {
        if !matches!(
            statement.kind,
            StatementKind::TypeAlias(_) | StatementKind::Interface(_)
        ) {
            return false;
        }
        let end = statement.span.end;
        self.comment_cursor.discard_while(|comment| {
            comment.span.start < end
                || comment.preceding_token_end == Some(end)
                    && comment.placement == CommentPlacement::Trailing
        });
        true
    }

    pub(super) fn write_commented_expression_statement(
        &mut self,
        statement: &Statement,
        expression: &Expression,
    ) {
        self.write_indent();
        self.write_expression_statement_expression(expression);
        if statement.span.end > expression.span.end {
            self.write_comments_through_token(expression.span.end);
        }
        self.output.push(';');
        self.comment_cursor
            .write_while(self.source, &mut self.output, false, |comment| {
                comment.preceding_token_end == Some(statement.span.end)
                    && comment.placement == CommentPlacement::Trailing
            });
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }
}
