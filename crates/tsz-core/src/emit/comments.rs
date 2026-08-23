use crate::source::SourceText;
use crate::syntax::{
    CommentKind, CommentPlacement, CommentTrivia, Expression, SourceUnit, Statement, StatementKind,
};

use super::{PREC_LOWEST, Printer};

#[derive(Default)]
pub(super) struct CommentCursor {
    comments: Vec<CommentTrivia>,
    next: usize,
}

impl CommentCursor {
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
        let mut last = None;
        while let Some(&comment) = self.comments.get(self.next) {
            if !take(comment) {
                break;
            }
            self.next += 1;
            match comment.placement {
                CommentPlacement::Leading if !output.is_empty() && !output.ends_with('\n') => {
                    output.push('\n');
                }
                CommentPlacement::Trailing
                    if !output.chars().last().is_some_and(char::is_whitespace) =>
                {
                    output.push(' ');
                }
                CommentPlacement::Leading | CommentPlacement::Trailing => {}
            }
            output.push_str(source.slice(comment.span));
            if comment.kind == CommentKind::Line && !output.ends_with('\n') {
                output.push('\n');
            }
            last = Some(comment);
        }
        if followed_by_token
            && last.is_some_and(|comment| {
                comment.kind == CommentKind::Block && comment.placement == CommentPlacement::Leading
            })
            && !output.chars().last().is_some_and(char::is_whitespace)
        {
            output.push(' ');
        }
    }
}

impl Printer<'_> {
    pub(super) fn write_javascript_statements(&mut self, unit: &SourceUnit) {
        if self.preserve_comments {
            self.reset_comments(unit.comments());
        }
        for statement in &unit.statements {
            self.write_javascript_statement(statement, true);
        }
        self.write_remaining_comments();
    }

    pub(super) fn reset_comments(&mut self, comments: &[CommentTrivia]) {
        self.comment_cursor.reset(comments);
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

    pub(super) fn write_trailing_comments_after_token(&mut self, token_end: u32) {
        self.comment_cursor
            .write_while(self.source, &mut self.output, false, |comment| {
                comment.preceding_token_end == Some(token_end)
                    && comment.placement == CommentPlacement::Trailing
            });
    }

    pub(super) fn discard_erased_statement_comments(&mut self, statement: &Statement) -> bool {
        if !matches!(
            statement.kind,
            StatementKind::TypeAlias(_) | StatementKind::Interface(_)
        ) {
            return false;
        }
        let statement_end = statement.span.end;
        while self
            .comment_cursor
            .comments
            .get(self.comment_cursor.next)
            .is_some_and(|comment| {
                comment.span.start < statement_end
                    || comment.preceding_token_end == Some(statement_end)
                        && comment.placement == CommentPlacement::Trailing
            })
        {
            self.comment_cursor.next += 1;
        }
        true
    }

    pub(super) fn write_commented_expression_statement(
        &mut self,
        statement: &Statement,
        expression: &Expression,
    ) {
        self.write_indent();
        self.write_expression(expression, PREC_LOWEST);
        if statement.span.end > expression.span.end {
            self.write_comments_through_token(expression.span.end);
        }
        self.output.push(';');
        self.write_trailing_comments_after_token(statement.span.end);
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }

    pub(super) fn write_remaining_comments(&mut self) {
        self.comment_cursor
            .write_while(self.source, &mut self.output, false, |_| true);
    }
}
