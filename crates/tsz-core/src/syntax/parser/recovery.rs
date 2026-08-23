use super::Parser;
use crate::source::Span;
use crate::syntax::{
    ParserRecoveryFact, ParserRecoveryKind, ParserRecoveryOwner, Statement, Token, TokenKind,
    TypeNode, TypeNodeKind,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct PendingParserRecoveryFact {
    authored_span: Span,
    recovery_extent: Span,
    kind: ParserRecoveryKind,
}

impl Parser<'_> {
    pub(super) fn observe_unmodeled_postfix_expression(&mut self, receiver: Span) {
        self.observe_unmodeled_non_null_template_adjacency();
        self.reject_tagged_template(receiver);
    }

    pub(super) fn retain_parser_recovery(
        &mut self,
        kind: ParserRecoveryKind,
        authored_span: Span,
        recovery_extent: Span,
    ) {
        if self.speculating {
            return;
        }
        debug_assert_eq!(authored_span.file, recovery_extent.file);
        debug_assert!(recovery_extent.start <= authored_span.start);
        debug_assert!(authored_span.end <= recovery_extent.end);
        self.parser_recovery_facts.push(PendingParserRecoveryFact {
            authored_span,
            recovery_extent,
            kind,
        });
    }

    pub(super) fn finish_parser_recovery_facts(
        &self,
        statements: &[Statement],
    ) -> Vec<ParserRecoveryFact> {
        self.parser_recovery_facts
            .iter()
            .map(|fact| {
                let owner = recovery_owner(statements, fact.authored_span)
                    .expect("a parser recovery token must have a represented statement owner");
                ParserRecoveryFact {
                    authored_span: fact.authored_span,
                    recovery_extent: fact.recovery_extent,
                    kind: fact.kind,
                    owner,
                }
            })
            .collect()
    }

    pub(super) fn balanced_recovery_brace_extent(&self, index: usize) -> Option<Span> {
        let start = self.tokens.get(index)?.span;
        if self.tokens[index].kind != TokenKind::LeftBrace {
            return None;
        }
        let mut depth = 0_u32;
        for token in self.tokens.iter().skip(index) {
            match token.kind {
                TokenKind::LeftBrace => depth += 1,
                TokenKind::RightBrace => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(start.merge(token.span));
                    }
                }
                TokenKind::EndOfFile => return None,
                _ => {}
            }
        }
        None
    }

    pub(super) fn promote_parser_recovery_extent(&mut self, start: usize, extent: Span) {
        for fact in &mut self.parser_recovery_facts[start..] {
            fact.recovery_extent = fact.recovery_extent.merge(extent);
        }
    }

    /// Whether a later postfix fragment is still part of an earlier recovery
    /// segment. The receiver's trailing edge and `[` must remain in that
    /// segment. A line break is accepted only while the recovery delimiter is
    /// still open, so a following expression statement stays independent.
    pub(super) fn postfix_continues_retained_recovery(&self, receiver: Span) -> bool {
        let bracket = self.current().span;
        self.parser_recovery_facts.iter().any(|fact| {
            fact.recovery_extent.file == receiver.file
                && fact.recovery_extent.start < receiver.end
                && receiver.end <= fact.recovery_extent.end
                && contains_authored_span(fact.recovery_extent, bracket)
                && (self.tokens_are_on_same_line(
                    self.tokens
                        .partition_point(|token| token.span.start < fact.authored_span.start),
                    self.index,
                ) || self.recovery_delimiter_remains_open(fact.authored_span.start))
        })
    }

    fn recovery_delimiter_remains_open(&self, recovery_start: u32) -> bool {
        let mut depth = 0_u32;
        for token in &self.tokens[self
            .tokens
            .partition_point(|token| token.span.start < recovery_start)
            ..self.index]
        {
            match token.kind {
                TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace => depth += 1,
                TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace
                    if depth > 0 =>
                {
                    depth -= 1;
                    if depth == 0 {
                        return false;
                    }
                }
                _ => {}
            }
        }
        depth > 0
    }

    pub(super) fn recover_missing_type(&mut self, token: Token) -> TypeNode {
        self.observe_unmodeled_regular_expression_if_current();
        self.observe_unmodeled_template_if_current();
        if !matches!(
            token.kind,
            TokenKind::NoSubstitutionTemplateLiteral
                | TokenKind::TemplateHead
                | TokenKind::TemplateMiddle
                | TokenKind::TemplateTail
        ) {
            let recovery_extent = self.recovery_extent_from_current(token.span);
            self.retain_parser_recovery(ParserRecoveryKind::Type, token.span, recovery_extent);
        }
        self.error_current("Type expected.", 1110);
        self.bump();
        TypeNode {
            span: token.span,
            kind: TypeNodeKind::Missing,
        }
    }

    /// Bound a failed type parse through the declaration fragment that the
    /// ordinary statement parser will otherwise revisit as standalone syntax.
    /// A terminator closes the fragment. In semicolon-free code, a declaration
    /// starter on a later line closes it without consuming the next sibling.
    pub(super) fn recovery_extent_from_current(&self, authored_span: Span) -> Span {
        let current = self.current().span;
        debug_assert_eq!(authored_span.file, current.file);
        if matches!(
            authored_token_boundary(self.kind()),
            RecoveryBoundary::Closed
        ) {
            return authored_span.merge(current);
        }

        let mut end = current.end;
        let mut relative_depth = 0_u32;
        let mut previous = self.index;
        for index in self.index + 1..self.tokens.len() {
            let token = self.tokens[index];
            if token.kind == TokenKind::EndOfFile {
                break;
            }
            if relative_depth == 0 && self.later_line_starts_declaration(previous, index) {
                break;
            }
            end = token.span.end;
            match token.kind {
                TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace => {
                    relative_depth += 1;
                }
                TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace
                    if relative_depth == 0 =>
                {
                    let next = self
                        .tokens
                        .get(index + 1)
                        .map_or(TokenKind::EndOfFile, |next| next.kind);
                    if !continues_recovered_owner(next) {
                        break;
                    }
                }
                TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace => {
                    relative_depth -= 1;
                }
                TokenKind::Semicolon if relative_depth == 0 => break,
                _ => {}
            }
            previous = index;
        }
        Span {
            file: authored_span.file,
            start: authored_span.start,
            end,
        }
    }

    pub(super) fn later_line_starts_declaration(&self, previous: usize, index: usize) -> bool {
        !self.tokens_are_on_same_line(previous, index)
            && starts_declaration_boundary(
                self.tokens[index].kind,
                self.tokens
                    .get(index + 1)
                    .map_or(TokenKind::EndOfFile, |next| next.kind),
            )
    }
}

pub(super) fn recovery_owner(
    statements: &[Statement],
    authored_span: Span,
) -> Option<ParserRecoveryOwner> {
    let root = statements
        .iter()
        .filter(|statement| contains_authored_span(statement.span, authored_span))
        .min_by_key(|statement| statement.span.len())
        .or_else(|| {
            statements
                .iter()
                .rev()
                .find(|statement| statement.span.start <= authored_span.start)
        })?;
    let mut statement = root.id;
    let mut best = (root.span.len(), root.span.start);
    root.for_each_statement(&mut |candidate| {
        if contains_authored_span(candidate.span, authored_span)
            && (
                candidate.span.len(),
                std::cmp::Reverse(candidate.span.start),
            ) < (best.0, std::cmp::Reverse(best.1))
        {
            statement = candidate.id;
            best = (candidate.span.len(), candidate.span.start);
        }
    });
    Some(ParserRecoveryOwner {
        root_statement: root.id,
        statement,
    })
}

const fn contains_authored_span(owner: Span, authored: Span) -> bool {
    owner.file.0 == authored.file.0 && owner.start <= authored.start && authored.end <= owner.end
}

#[derive(Clone, Copy)]
enum RecoveryBoundary {
    Open,
    Closed,
}

const fn authored_token_boundary(kind: TokenKind) -> RecoveryBoundary {
    if matches!(
        kind,
        TokenKind::RightParen
            | TokenKind::RightBracket
            | TokenKind::RightBrace
            | TokenKind::Semicolon
            | TokenKind::EndOfFile
    ) {
        RecoveryBoundary::Closed
    } else {
        RecoveryBoundary::Open
    }
}

fn starts_declaration_boundary(kind: TokenKind, next: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Let
            | TokenKind::Const
            | TokenKind::Var
            | TokenKind::Function
            | TokenKind::Class
            | TokenKind::Interface
            | TokenKind::Import
            | TokenKind::Export
            | TokenKind::Declare
            | TokenKind::Abstract
            | TokenKind::Async
            | TokenKind::Using
    ) || kind == TokenKind::Type && next.is_identifier()
        || kind == TokenKind::Await && next == TokenKind::Using
}

const fn continues_recovered_owner(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::LeftBrace
            | TokenKind::Comma
            | TokenKind::Colon
            | TokenKind::Equals
            | TokenKind::Bar
            | TokenKind::Ampersand
            | TokenKind::LeftBracket
            | TokenKind::Question
            | TokenKind::Extends
            | TokenKind::FatArrow
    )
}
