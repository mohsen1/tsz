use crate::source::Span;
use crate::syntax::{
    CommentClass, CommentKind, CommentPlacement, CommentSourcePosition, CommentTrivia, Expression,
    SourceUnit, Statement, TokenKind,
};

use super::Printer;

/// Source-ordered comment identities consumed once across overlapping ranges.
#[derive(Default)]
pub(super) struct CommentIndex {
    comments: Vec<CommentTrivia>,
    next: usize,
}

impl CommentIndex {
    pub(super) fn has_comment_within(&self, start: u32, end: u32) -> bool {
        let first = self
            .comments
            .partition_point(|comment| comment.span.start < start);
        self.comments
            .get(first.max(self.next))
            .is_some_and(|comment| comment.span.end <= end)
    }

    fn reset(&mut self, comments: &[CommentTrivia], preserve_comments: bool) {
        self.comments = comments
            .iter()
            .copied()
            .filter(|comment| preserve_comments || comment.class == CommentClass::DetachedPinned)
            .collect();
        self.next = 0;
    }

    fn take_prefix(
        &mut self,
        mut predicate: impl FnMut(CommentTrivia) -> bool,
    ) -> Vec<CommentTrivia> {
        let start = self.next;
        while self
            .comments
            .get(self.next)
            .copied()
            .is_some_and(&mut predicate)
        {
            self.next += 1;
        }
        self.comments[start..self.next].to_vec()
    }

    fn take_before(&mut self, offset: u32) -> Vec<CommentTrivia> {
        self.take_prefix(|comment| comment.span.start < offset)
    }
}

pub(super) enum GapSeparator {
    None,
    Space,
    Indent,
    Newline,
    Hanging,
}

pub(super) enum GapOwner {
    End(u32),
    Kind(TokenKind, u32),
}

impl Printer<'_> {
    pub(super) fn write_javascript_statements(&mut self, unit: &SourceUnit) {
        self.comment_index
            .reset(unit.comments(), self.preserve_comments);
        for statement in &unit.statements {
            self.write_javascript_statement(statement, true);
        }
        let tail_start = unit
            .statements
            .last()
            .map_or(unit.span.start, |statement| statement.span.end);
        let _ = self.comment_index.take_before(tail_start);
        let tail = self
            .comment_index
            .take_prefix(|comment| comment.span.end <= unit.span.end);
        self.write_comment_sequence(&tail, false, true);
    }

    pub(super) fn write_comments_before_node(&mut self, span: Span, emitted: bool) {
        let mut comments = self.comment_index.take_before(span.start);
        if !emitted {
            comments.retain(|comment| {
                comment.class == CommentClass::DetachedPinned
                    || comment.source_position == CommentSourcePosition::SourceLeading
                        && comment.class == CommentClass::TripleSlashReference
            });
        }
        self.write_comment_sequence(&comments, true, true);
    }

    /// Defensively reject emitted nodes with a comment slot not claimed earlier.
    pub(super) fn write_comments_after_node(&mut self, span: Span, emitted: bool) {
        let unresolved = self.comment_index.take_before(span.end);
        self.javascript_supported &= !emitted || unresolved.is_empty();
        let trailing = self.comment_index.take_prefix(|comment| {
            comment.placement == CommentPlacement::Trailing
                && comment.preceding_token_end == Some(span.end)
        });
        if emitted {
            self.write_node_trailing_comments(&trailing);
        }
    }

    pub(super) fn write_comments_before(&mut self, offset: u32) -> bool {
        let comments = self.comment_index.take_before(offset);
        self.write_comment_sequence(&comments, true, true)
    }

    pub(super) fn write_gap(
        &mut self,
        owner: GapOwner,
        followed_by_token: bool,
        separator: GapSeparator,
    ) -> (bool, bool) {
        let comments = self.comment_index.take_prefix(|comment| match owner {
            GapOwner::End(end) => comment.preceding_token_end == Some(end),
            GapOwner::Kind(kind, before) => {
                comment.preceding_token_kind == Some(kind) && comment.span.end <= before
            }
        });
        let broke_line = comments.iter().any(|comment| {
            comment.placement == CommentPlacement::Leading
                || comment.kind == CommentKind::Line
                || comment.has_trailing_line_break
        });
        let ended_line = self.write_comment_sequence(&comments, followed_by_token, true);
        match separator {
            GapSeparator::Space | GapSeparator::Hanging if !ended_line => self.output.push(' '),
            GapSeparator::Space | GapSeparator::Indent if ended_line => self.write_indent(),
            GapSeparator::Newline if !ended_line => self.write_newline(),
            GapSeparator::Hanging if ended_line => {
                self.indent += 1;
                self.write_indent();
                self.indent = self.indent.saturating_sub(1);
            }
            _ => {}
        }
        (ended_line, broke_line)
    }

    pub(super) fn write_comments_before_close(&mut self, end: u32) -> bool {
        let comments = self.comment_index.take_before(end);
        self.write_comment_sequence(&comments, false, true)
    }

    pub(super) fn consume_comments_through_token(&mut self, end: u32) {
        let _ = self.comment_index.take_prefix(
            |comment| matches!(comment.preceding_token_end, Some(start) if start <= end),
        );
    }

    pub(super) fn consume_comments_before(&mut self, offset: u32) {
        let _ = self.comment_index.take_before(offset);
    }

    pub(super) fn consume_parameter_close_comments(&mut self) {
        let _ = self
            .comment_index
            .take_prefix(|comment| comment.preceding_token_kind == Some(TokenKind::Comma));
    }

    pub(super) fn write_commented_expression_statement(
        &mut self,
        statement: &Statement,
        expression: &Expression,
    ) {
        self.write_indent();
        self.write_expression_statement_expression(expression);
        if statement.span.end > expression.span.end {
            self.write_gap(
                GapOwner::End(expression.span.end),
                true,
                GapSeparator::Indent,
            );
        }
        self.output.push_str(";\n");
    }

    fn write_node_trailing_comments(&mut self, comments: &[CommentTrivia]) {
        if comments.is_empty() {
            return;
        }
        let restore_line_break = self.output.ends_with('\n');
        if restore_line_break {
            self.output.pop();
        }
        self.write_comment_sequence(comments, false, false);
        if restore_line_break && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }

    fn write_comment_sequence(
        &mut self,
        comments: &[CommentTrivia],
        followed_by_token: bool,
        indent_at_line_start: bool,
    ) -> bool {
        let mut wrote = false;
        for (index, comment) in comments.iter().copied().enumerate() {
            wrote = true;
            let separated_before = self.output.chars().last().is_some_and(char::is_whitespace);
            if comment.placement == CommentPlacement::Leading
                && !self.output.is_empty()
                && !self.output.ends_with('\n')
            {
                self.output.push('\n');
            } else if comment.placement == CommentPlacement::Trailing
                && !self.output.chars().last().is_some_and(char::is_whitespace)
            {
                self.output.push(' ');
            }
            if indent_at_line_start && self.output.ends_with('\n') {
                self.write_indent();
            }
            self.output
                .push_str(&self.source.slice(comment.span).replace("\r\n", "\n"));
            if comment.kind == CommentKind::Line
                || comment.has_trailing_line_break
                    && (comment.placement == CommentPlacement::Leading || !followed_by_token)
            {
                if !self.output.ends_with('\n') {
                    self.output.push('\n');
                }
            } else if comment.kind == CommentKind::Block
                && (followed_by_token || index + 1 < comments.len())
                && (separated_before || comment.placement == CommentPlacement::Leading)
                && !self.output.chars().last().is_some_and(char::is_whitespace)
            {
                self.output.push(' ');
            }
        }
        wrote && self.output.ends_with('\n')
    }
}
