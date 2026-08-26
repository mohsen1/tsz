use super::Parser;
use crate::source::{SourceKind, Span};
use crate::syntax::{
    ParserRecoveryFact, ParserRecoveryKind, ParserRecoveryOwner, Statement, Token, TokenKind,
    TypeNode, TypeNodeKind,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct PendingParserRecoveryFact {
    authored_span: Span,
    recovery_extent: Span,
    kind: ParserRecoveryKind,
    participation: PendingParserRecoveryParticipation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingParserRecoveryParticipation {
    /// The recovery was observed while parsing and may still bound token consumption.
    ControlAndAnalysis,
    /// The recovery was synthesized for downstream ownership after its syntax was parsed.
    AnalysisOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClassMemberListRecovery {
    Continue,
    AbortBeforeStatement,
}

impl ClassMemberListRecovery {
    pub(super) const fn aborts_list(self) -> bool {
        matches!(self, Self::AbortBeforeStatement)
    }
}

impl Parser<'_> {
    pub(super) fn error_current(&mut self, message: &str, code: u32) {
        self.diagnostics.push(crate::diagnostics::Diagnostic::at(
            self.source,
            self.current().span,
            message.to_string(),
            code,
        ));
    }

    pub(super) fn expect(&mut self, kind: TokenKind, message: &str, code: u32) -> bool {
        if self.eat(kind) {
            true
        } else {
            self.error_current(message, code);
            false
        }
    }

    pub(super) fn recover_stray_statement_close(&mut self) -> bool {
        if self.pending_stray_statement_closes == 0 || !self.at(TokenKind::RightBrace) {
            return false;
        }
        self.pending_stray_statement_closes -= 1;
        self.error_current("Declaration or statement expected.", 1128);
        self.bump();
        true
    }

    /// Mirrors TypeScript's `parseSemicolonAfterPropertyName` followed by
    /// class-list recovery for the empty call-shaped tail left by a definite
    /// property. The opening parenthesis belongs to the property error; the
    /// closing parenthesis is skipped by the class-member list, while a block
    /// start is retained for the enclosing source-element list.
    pub(super) fn recover_definite_property_call(&mut self) -> ClassMemberListRecovery {
        if !self.at(TokenKind::LeftParen) {
            return ClassMemberListRecovery::Continue;
        }
        self.error_current("Cannot start a function call in a type annotation.", 1441);
        self.bump();
        if self.at(TokenKind::RightParen) {
            self.error_class_member_list_token();
            self.bump();
        }
        if self.at(TokenKind::LeftBrace) {
            self.error_class_member_list_token();
            self.pending_stray_statement_closes_after_block += 1;
            ClassMemberListRecovery::AbortBeforeStatement
        } else {
            ClassMemberListRecovery::Continue
        }
    }

    fn error_class_member_list_token(&mut self) {
        self.error_current(
            "Unexpected token. A constructor, method, accessor, or property was expected.",
            1068,
        );
    }

    pub(super) fn consume_balanced_tokens(
        &mut self,
        open: TokenKind,
        close: TokenKind,
        missing_message: &str,
    ) -> Span {
        debug_assert!(self.at(open));
        let start = self.bump().span;
        let mut depth = 1_u32;
        while depth != 0 && !self.at(TokenKind::EndOfFile) {
            let kind = self.kind();
            self.bump();
            if kind == open {
                depth += 1;
            } else if kind == close {
                depth -= 1;
            }
        }
        if depth != 0 {
            self.error_current(missing_message, 1005);
        }
        start.merge(self.previous().span)
    }

    pub(super) fn observe_unmodeled_postfix_expression(&mut self, receiver: Span) {
        self.observe_unmodeled_non_null_template_adjacency();
        self.reject_tagged_template(receiver);
        if self.at(TokenKind::Satisfies) {
            let authored_span = self.current().span;
            self.retain_parser_recovery(
                ParserRecoveryKind::Expression,
                authored_span,
                self.recovery_extent_from_current(authored_span),
            );
        }
    }

    pub(super) fn retain_parser_recovery(
        &mut self,
        kind: ParserRecoveryKind,
        authored_span: Span,
        recovery_extent: Span,
    ) {
        self.record_parser_recovery(
            kind,
            authored_span,
            recovery_extent,
            PendingParserRecoveryParticipation::ControlAndAnalysis,
        );
    }

    pub(super) fn record_parser_recovery_for_analysis(
        &mut self,
        kind: ParserRecoveryKind,
        authored_span: Span,
        recovery_extent: Span,
    ) {
        self.record_parser_recovery(
            kind,
            authored_span,
            recovery_extent,
            PendingParserRecoveryParticipation::AnalysisOnly,
        );
    }

    pub(super) fn recover_statement(&mut self, recovery_extent: Option<Span>) {
        while !self.at_any(&[
            TokenKind::Semicolon,
            TokenKind::RightBrace,
            TokenKind::EndOfFile,
        ]) && recovery_extent.is_none_or(|extent| self.current().span.start < extent.end)
        {
            self.observe_unmodeled_regular_expression_if_current();
            self.observe_unmodeled_template_if_current();
            self.bump();
        }
        if recovery_extent.is_none_or(|extent| self.current().span.end <= extent.end) {
            self.eat(TokenKind::Semicolon);
        }
    }

    fn record_parser_recovery(
        &mut self,
        kind: ParserRecoveryKind,
        authored_span: Span,
        recovery_extent: Span,
        participation: PendingParserRecoveryParticipation,
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
            participation,
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

    /// Whether `[` continues a retained recovery across the receiver's trailing
    /// edge, including line breaks while the recovery delimiter remains open.
    pub(super) fn postfix_continues_retained_recovery(&self, receiver: Span) -> bool {
        let bracket = self.current().span;
        self.parser_recovery_facts.iter().any(|fact| {
            fact.participation == PendingParserRecoveryParticipation::ControlAndAnalysis
                && fact.recovery_extent.file == receiver.file
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

    pub(super) fn parenthesis_follows_recovered_generic_prefix(&self) -> bool {
        if self.index == 0
            || !matches!(
                self.tokens[self.index - 1].kind,
                TokenKind::GreaterThan
                    | TokenKind::GreaterThanGreaterThan
                    | TokenKind::GreaterThanGreaterThanGreaterThan
                    | TokenKind::GreaterThanEquals
                    | TokenKind::GreaterThanGreaterThanEquals
                    | TokenKind::GreaterThanGreaterThanGreaterThanEquals
            )
        {
            return false;
        }
        self.parser_recovery_facts.iter().any(|fact| {
            fact.participation == PendingParserRecoveryParticipation::ControlAndAnalysis
                && matches!(
                    fact.kind,
                    ParserRecoveryKind::Expression | ParserRecoveryKind::RejectedGenericArrowPrefix
                )
                && fact.recovery_extent.start <= self.current().span.start
                && self.current().span.end <= fact.recovery_extent.end
                && self.tokens.iter().any(|token| {
                    token.kind == TokenKind::LessThan && token.span == fact.authored_span
                })
        })
    }

    pub(super) fn parenthesis_continues_recovered_function_declaration(&self) -> bool {
        self.index > 0
            && self.tokens[self.index - 1].kind.is_identifier_name()
            && self.current_continues_recovered_function_declaration()
    }

    pub(super) fn current_continues_recovered_function_declaration(&self) -> bool {
        self.parser_recovery_facts.iter().any(|fact| {
            fact.kind == ParserRecoveryKind::GeneratorFunctionLike
                && contains_authored_span(fact.recovery_extent, self.current().span)
                && self.tokens.iter().any(|token| {
                    token.kind == TokenKind::Function && token.span == fact.authored_span
                })
        })
    }

    pub(super) fn current_starts_rejected_generic_arrow_prefix(&self) -> bool {
        self.parser_recovery_facts.iter().any(|fact| {
            fact.kind == ParserRecoveryKind::RejectedGenericArrowPrefix
                && fact.authored_span == self.current().span
        })
    }

    pub(super) fn expression_starts_rejected_generic_arrow_prefix(&self, span: Span) -> bool {
        self.parser_recovery_facts.iter().any(|fact| {
            fact.kind == ParserRecoveryKind::RejectedGenericArrowPrefix
                && fact.authored_span.start == span.start
        })
    }

    pub(super) fn current_is_inside_rejected_generic_arrow_prefix(&self) -> bool {
        self.parser_recovery_facts.iter().any(|fact| {
            fact.kind == ParserRecoveryKind::RejectedGenericArrowPrefix
                && fact.authored_span.start < self.current().span.start
                && contains_authored_span(fact.recovery_extent, self.current().span)
        })
    }

    pub(super) fn expression_is_inside_rejected_generic_arrow_prefix(&self, span: Span) -> bool {
        self.parser_recovery_facts.iter().any(|fact| {
            fact.kind == ParserRecoveryKind::RejectedGenericArrowPrefix
                && contains_authored_span(fact.recovery_extent, span)
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

    pub(super) fn recover_missing_type(&mut self, token: Token, consume: bool) -> TypeNode {
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
        if consume {
            self.bump();
        }
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
        let jsx = matches!(
            self.source.kind(),
            SourceKind::TypeScriptJsx | SourceKind::JavaScriptJsx
        ) && self.kind() == TokenKind::LessThan;
        debug_assert_eq!(authored_span.file, current.file);
        if authored_token_closes_recovery(self.kind())
            && !closed_token_continues_recovered_owner(self.kind(), self.peek_kind(1))
        {
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
                    let next = self.token_kind_at(index + 1);
                    if !continues_recovered_owner(next) {
                        break;
                    }
                }
                TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace => {
                    relative_depth -= 1;
                }
                TokenKind::Semicolon
                    if relative_depth == 0
                        && (!jsx
                            || self.token_kind_at(index.saturating_sub(1))
                                == TokenKind::GreaterThan) =>
                {
                    break;
                }
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
            && starts_declaration_boundary(self.tokens[index].kind, self.token_kind_at(index + 1))
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
    let mut best = (root.span.len(), std::cmp::Reverse(root.span.start));
    root.for_each_statement(&mut |candidate| {
        let order = (
            candidate.span.len(),
            std::cmp::Reverse(candidate.span.start),
        );
        if contains_authored_span(candidate.span, authored_span) && order < best {
            statement = candidate.id;
            best = order;
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

const fn authored_token_closes_recovery(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::RightParen
            | TokenKind::RightBracket
            | TokenKind::RightBrace
            | TokenKind::Semicolon
            | TokenKind::EndOfFile
    )
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
            | TokenKind::As
            | TokenKind::Satisfies
    )
}

const fn closed_token_continues_recovered_owner(current: TokenKind, next: TokenKind) -> bool {
    matches!(current, TokenKind::RightBrace | TokenKind::RightBracket)
        && matches!(
            next,
            TokenKind::Equals | TokenKind::Comma | TokenKind::As | TokenKind::Satisfies
        )
}
