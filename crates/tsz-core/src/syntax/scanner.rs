use crate::diagnostics::Diagnostic;
use crate::source::{SourceText, Span};

use super::numeric_literal::{
    ScannedNumericLiteral, ScannedSeparatedNumberLiteral, scan_numeric_literal,
};
use super::regular_expression::ScannedRegularExpressionLiteral;
use super::string_literal::{
    ScannedLineContinuationStringLiteral, ScannedStringLiteral, scan_ordinary_string_literal,
};
use super::template_literal::ScannedTemplateLiteral;
use super::{
    CommentKind, CommentPlacement, CommentTrivia, Token, TokenKind, is_single_line_whitespace,
};

#[derive(Debug)]
pub struct ScanOutput {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
    pub(super) template_literals: Vec<ScannedTemplateLiteral>,
    pub(super) string_literals: Vec<ScannedStringLiteral>,
    pub(super) line_continuation_string_literals: Vec<ScannedLineContinuationStringLiteral>,
    pub(super) numeric_literals: Vec<ScannedNumericLiteral>,
    pub(super) separated_numeric_literals: Vec<ScannedSeparatedNumberLiteral>,
    pub(super) has_unmodeled_numeric_separator: bool,
    pub(super) regular_expression_literals: Vec<ScannedRegularExpressionLiteral>,
    pub(super) comments: Vec<CommentTrivia>,
    pub(super) has_unicode_line_comment_terminator: bool,
    pub(super) has_unmodeled_trivia: bool,
}

pub fn scan_source(source: &SourceText) -> ScanOutput {
    Scanner::new(source).scan()
}

struct Scanner<'a> {
    source: &'a SourceText,
    bytes: &'a [u8],
    offset: usize,
    brace_depth: usize,
    template_expression_depths: Vec<usize>,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
    template_literals: Vec<ScannedTemplateLiteral>,
    string_literals: Vec<ScannedStringLiteral>,
    line_continuation_string_literals: Vec<ScannedLineContinuationStringLiteral>,
    numeric_literals: Vec<ScannedNumericLiteral>,
    separated_numeric_literals: Vec<ScannedSeparatedNumberLiteral>,
    has_unmodeled_numeric_separator: bool,
    regular_expression_literals: Vec<ScannedRegularExpressionLiteral>,
    comments: Vec<CommentTrivia>,
    has_unicode_line_comment_terminator: bool,
    has_unmodeled_trivia: bool,
}

impl<'a> Scanner<'a> {
    fn new(source: &'a SourceText) -> Self {
        Self {
            source,
            bytes: source.text.as_bytes(),
            offset: 0,
            brace_depth: 0,
            template_expression_depths: Vec::new(),
            tokens: Vec::new(),
            diagnostics: Vec::new(),
            template_literals: Vec::new(),
            string_literals: Vec::new(),
            line_continuation_string_literals: Vec::new(),
            numeric_literals: Vec::new(),
            separated_numeric_literals: Vec::new(),
            has_unmodeled_numeric_separator: false,
            regular_expression_literals: Vec::new(),
            comments: Vec::new(),
            has_unicode_line_comment_terminator: false,
            has_unmodeled_trivia: false,
        }
    }

    fn scan(mut self) -> ScanOutput {
        while self.offset < self.bytes.len() {
            self.skip_trivia();
            if self.offset >= self.bytes.len() {
                break;
            }
            let start = self.offset;
            let byte = self.bytes[self.offset];
            let kind = if byte == b'}'
                && self.template_expression_depths.last() == Some(&self.brace_depth)
            {
                self.scan_template_continuation(start)
            } else if is_identifier_start(byte) || self.is_identifier_escape_at(self.offset) {
                self.scan_identifier(start)
            } else if byte.is_ascii_digit()
                || (byte == b'.'
                    && self
                        .bytes
                        .get(self.offset + 1)
                        .is_some_and(u8::is_ascii_digit))
            {
                self.scan_number(start)
            } else {
                self.scan_punctuation_or_literal(start)
            };
            match kind {
                TokenKind::LeftBrace => self.brace_depth += 1,
                TokenKind::RightBrace => self.brace_depth = self.brace_depth.saturating_sub(1),
                _ => {}
            }
            self.tokens.push(Token {
                kind,
                span: Span::new(self.source.id, start, self.offset),
            });
        }
        let end = self.bytes.len();
        self.tokens.push(Token {
            kind: TokenKind::EndOfFile,
            span: Span::new(self.source.id, end, end),
        });
        ScanOutput {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
            template_literals: self.template_literals,
            string_literals: self.string_literals,
            line_continuation_string_literals: self.line_continuation_string_literals,
            numeric_literals: self.numeric_literals,
            separated_numeric_literals: self.separated_numeric_literals,
            has_unmodeled_numeric_separator: self.has_unmodeled_numeric_separator,
            regular_expression_literals: self.regular_expression_literals,
            comments: self.comments,
            has_unicode_line_comment_terminator: self.has_unicode_line_comment_terminator,
            has_unmodeled_trivia: self.has_unmodeled_trivia,
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            if self.offset == 0 && self.bytes.get(..3) == Some(&[0xef, 0xbb, 0xbf]) {
                self.offset += 3;
            }
            if self.offset == 0 && self.bytes.get(..2) == Some(b"#!") {
                self.has_unmodeled_trivia = true;
                self.offset += 2;
                while self
                    .bytes
                    .get(self.offset)
                    .is_some_and(|byte| *byte != b'\n' && *byte != b'\r')
                    && !self.is_unicode_line_separator_at(self.offset)
                {
                    self.offset += 1;
                }
                self.has_unicode_line_comment_terminator |=
                    self.is_unicode_line_separator_at(self.offset);
                continue;
            }
            while self.skip_one_whitespace() {}
            if self.bytes.get(self.offset..self.offset + 2) == Some(b"//") {
                let start = self.offset;
                let placement = self.comment_placement(start);
                self.offset += 2;
                while self
                    .bytes
                    .get(self.offset)
                    .is_some_and(|byte| *byte != b'\n' && *byte != b'\r')
                    && !self.is_unicode_line_separator_at(self.offset)
                {
                    self.offset += 1;
                }
                self.has_unicode_line_comment_terminator |=
                    self.is_unicode_line_separator_at(self.offset);
                let plain = self.is_plain_line_comment(start, self.offset);
                self.comments.push(CommentTrivia {
                    span: Span::new(self.source.id, start, self.offset),
                    kind: CommentKind::Line,
                    placement,
                    has_trailing_line_break: self.has_line_break_at_offset(),
                    plain,
                });
                self.has_unmodeled_trivia |= !plain;
                continue;
            }
            if self.bytes.get(self.offset..self.offset + 2) == Some(b"/*") {
                self.has_unmodeled_trivia = true;
                let start = self.offset;
                let placement = self.comment_placement(start);
                self.offset += 2;
                while self.offset + 1 < self.bytes.len()
                    && self.bytes.get(self.offset..self.offset + 2) != Some(b"*/")
                {
                    self.offset += 1;
                }
                if self.offset + 1 < self.bytes.len() {
                    self.offset += 2;
                } else {
                    self.diagnostics.push(Diagnostic::at(
                        self.source,
                        Span::new(self.source.id, start, self.bytes.len()),
                        "'*/' expected.".to_string(),
                        1010,
                    ));
                    self.offset = self.bytes.len();
                }
                self.comments.push(CommentTrivia {
                    span: Span::new(self.source.id, start, self.offset),
                    kind: CommentKind::Block,
                    placement,
                    has_trailing_line_break: self.has_line_break_at_offset(),
                    plain: false,
                });
                continue;
            }
            break;
        }
    }

    fn comment_placement(&self, comment_start: usize) -> CommentPlacement {
        let Some(previous) = self.tokens.last() else {
            return CommentPlacement::Leading;
        };
        if self.contains_line_break(previous.span.end as usize, comment_start) {
            CommentPlacement::Leading
        } else {
            CommentPlacement::Trailing
        }
    }

    fn is_plain_line_comment(&self, start: usize, end: usize) -> bool {
        let body = &self.source.text[start + 2..end];
        let first = body
            .chars()
            .find(|character| !is_single_line_whitespace(*character));
        body.starts_with(' ') && !matches!(first, Some('/' | '@' | '!' | '#'))
    }

    fn has_line_break_at_offset(&self) -> bool {
        self.bytes
            .get(self.offset)
            .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
            || self.is_unicode_line_separator_at(self.offset)
    }

    fn contains_line_break(&self, start: usize, end: usize) -> bool {
        let mut offset = start;
        while offset < end {
            if matches!(self.bytes[offset], b'\r' | b'\n')
                || self.is_unicode_line_separator_at(offset)
            {
                return true;
            }
            offset += 1;
        }
        false
    }

    fn is_unicode_line_separator_at(&self, offset: usize) -> bool {
        matches!(
            self.bytes.get(offset..offset + 3),
            Some([0xe2, 0x80, 0xa8 | 0xa9])
        )
    }

    fn skip_one_whitespace(&mut self) -> bool {
        let Some(byte) = self.bytes.get(self.offset).copied() else {
            return false;
        };
        if byte.is_ascii_whitespace() {
            self.offset += 1;
            return true;
        }
        if byte < 0x80 {
            return false;
        }
        let Some(character) = self.source.text[self.offset..].chars().next() else {
            return false;
        };
        if character.is_whitespace() || character == '\u{feff}' {
            self.offset += character.len_utf8();
            true
        } else {
            false
        }
    }

    fn scan_identifier(&mut self, start: usize) -> TokenKind {
        self.consume_identifier_character();
        while self
            .bytes
            .get(self.offset)
            .is_some_and(|byte| is_identifier_continue(*byte))
            || self.is_identifier_escape_at(self.offset)
        {
            self.consume_identifier_character();
        }
        let text = &self.source.text[start..self.offset];
        if text.as_bytes().contains(&b'\\') {
            TokenKind::Identifier
        } else {
            keyword_kind(text)
        }
    }

    fn consume_identifier_character(&mut self) {
        if let Some(length) = self.identifier_escape_len_at(self.offset) {
            self.offset += length;
            return;
        }
        let Some(character) = self.source.text[self.offset..].chars().next() else {
            return;
        };
        self.offset += character.len_utf8();
    }

    fn is_identifier_escape_at(&self, offset: usize) -> bool {
        self.identifier_escape_len_at(offset).is_some()
    }

    fn identifier_escape_len_at(&self, offset: usize) -> Option<usize> {
        if self.bytes.get(offset..offset + 2) != Some(b"\\u") {
            return None;
        }
        if self.bytes.get(offset + 2) == Some(&b'{') {
            let mut cursor = offset + 3;
            let digits_start = cursor;
            while self.bytes.get(cursor).is_some_and(u8::is_ascii_hexdigit)
                && cursor - digits_start < 6
            {
                cursor += 1;
            }
            return (cursor > digits_start && self.bytes.get(cursor) == Some(&b'}'))
                .then_some(cursor + 1 - offset);
        }
        self.bytes
            .get(offset + 2..offset + 6)
            .filter(|digits| digits.iter().all(u8::is_ascii_hexdigit))
            .map(|_| 6)
    }

    fn scan_number(&mut self, start: usize) -> TokenKind {
        let scanned = scan_numeric_literal(self.source, start, self.tokens.last().copied());
        self.offset = scanned.end;
        self.diagnostics.extend(scanned.diagnostics);
        if let Some(literal) = scanned.recovery_literal {
            self.numeric_literals.push(literal);
        }
        if let Some(literal) = scanned.separated_literal {
            self.separated_numeric_literals.push(literal);
        }
        self.has_unmodeled_numeric_separator |= scanned.has_unmodeled_separator;
        scanned.kind
    }

    fn scan_punctuation_or_literal(&mut self, start: usize) -> TokenKind {
        let byte = self.bytes[self.offset];
        self.offset += 1;
        match byte {
            b'{' => TokenKind::LeftBrace,
            b'}' => TokenKind::RightBrace,
            b'(' => TokenKind::LeftParen,
            b')' => TokenKind::RightParen,
            b'[' => TokenKind::LeftBracket,
            b']' => TokenKind::RightBracket,
            b':' => TokenKind::Colon,
            b';' => TokenKind::Semicolon,
            b',' => TokenKind::Comma,
            b'.' if self.consume_suffix(b"..") => TokenKind::DotDotDot,
            b'.' => TokenKind::Dot,
            b'?' if self.consume_suffix(b"?=") => TokenKind::QuestionQuestionEquals,
            b'?' if self.consume_suffix(b"?") => TokenKind::QuestionQuestion,
            b'?' if self.bytes.get(self.offset) == Some(&b'.')
                && !self
                    .bytes
                    .get(self.offset + 1)
                    .is_some_and(u8::is_ascii_digit) =>
            {
                self.offset += 1;
                TokenKind::QuestionDot
            }
            b'?' => TokenKind::Question,
            b'+' if self.consume_suffix(b"+") => TokenKind::PlusPlus,
            b'+' if self.consume_suffix(b"=") => TokenKind::PlusEquals,
            b'+' => TokenKind::Plus,
            b'-' if self.consume_suffix(b"-") => TokenKind::MinusMinus,
            b'-' if self.consume_suffix(b"=") => TokenKind::MinusEquals,
            b'-' => TokenKind::Minus,
            b'*' if self.consume_suffix(b"*=") => TokenKind::StarStarEquals,
            b'*' if self.consume_suffix(b"*") => TokenKind::StarStar,
            b'*' if self.consume_suffix(b"=") => TokenKind::StarEquals,
            b'*' => TokenKind::Star,
            b'/' => self.scan_slash_or_regular_expression(start),
            b'%' if self.consume_suffix(b"=") => TokenKind::PercentEquals,
            b'%' => TokenKind::Percent,
            b'|' if self.consume_suffix(b"|=") => TokenKind::BarBarEquals,
            b'|' if self.consume_suffix(b"|") => TokenKind::BarBar,
            b'|' if self.consume_suffix(b"=") => TokenKind::BarEquals,
            b'|' => TokenKind::Bar,
            b'&' if self.consume_suffix(b"&=") => TokenKind::AmpersandAmpersandEquals,
            b'&' if self.consume_suffix(b"&") => TokenKind::AmpersandAmpersand,
            b'&' if self.consume_suffix(b"=") => TokenKind::AmpersandEquals,
            b'&' => TokenKind::Ampersand,
            b'^' if self.consume_suffix(b"=") => TokenKind::CaretEquals,
            b'^' => TokenKind::Caret,
            b'<' if self.consume_suffix(b"<=") => TokenKind::LessThanLessThanEquals,
            b'<' if self.consume_suffix(b"<") => TokenKind::LessThanLessThan,
            b'<' if self.consume_suffix(b"=") => TokenKind::LessThanEquals,
            b'<' if self.consume_suffix(b"/") => TokenKind::LessThanSlash,
            b'<' => TokenKind::LessThan,
            b'>' if self.consume_suffix(b">>=") => {
                TokenKind::GreaterThanGreaterThanGreaterThanEquals
            }
            b'>' if self.consume_suffix(b">>") => TokenKind::GreaterThanGreaterThanGreaterThan,
            b'>' if self.consume_suffix(b">=") => TokenKind::GreaterThanGreaterThanEquals,
            b'>' if self.consume_suffix(b">") => TokenKind::GreaterThanGreaterThan,
            b'>' if self.consume_suffix(b"=") => TokenKind::GreaterThanEquals,
            b'>' => TokenKind::GreaterThan,
            b'!' if self.consume_suffix(b"==") => TokenKind::BangEqualsEquals,
            b'!' if self.consume_suffix(b"=") => TokenKind::BangEquals,
            b'!' => TokenKind::Bang,
            b'=' if self.consume_suffix(b"==") => TokenKind::EqualsEqualsEquals,
            b'=' if self.consume_suffix(b"=") => TokenKind::EqualsEquals,
            b'=' if self.consume_suffix(b">") => TokenKind::FatArrow,
            b'=' => TokenKind::Equals,
            b'~' => TokenKind::Tilde,
            b'@' => TokenKind::At,
            b'#' if self
                .bytes
                .get(self.offset)
                .is_some_and(|byte| is_identifier_start(*byte))
                || self.is_identifier_escape_at(self.offset) =>
            {
                self.scan_identifier(self.offset);
                TokenKind::PrivateIdentifier
            }
            b'#' => TokenKind::Hash,
            b'\'' | b'"' => self.scan_string(start, byte),
            b'`' => self.scan_template_start(start),
            _ => {
                self.diagnostics.push(Diagnostic::at(
                    self.source,
                    Span::new(self.source.id, start, self.offset),
                    "Invalid character.".to_string(),
                    1127,
                ));
                TokenKind::Identifier
            }
        }
    }

    fn consume_suffix(&mut self, suffix: &[u8]) -> bool {
        let end = self.offset + suffix.len();
        if self.bytes.get(self.offset..end) == Some(suffix) {
            self.offset = end;
            true
        } else {
            false
        }
    }

    fn can_start_regular_expression(&self) -> bool {
        !self
            .tokens
            .last()
            .is_some_and(|token| token_can_end_expression(token.kind))
    }

    fn scan_slash_or_regular_expression(&mut self, start: usize) -> TokenKind {
        if self.can_start_regular_expression()
            && let Some(literal) = self.scan_regular_expression(start)
        {
            self.regular_expression_literals.push(literal);
            return TokenKind::RegularExpressionLiteral;
        }
        if self.consume_suffix(b"=") {
            TokenKind::SlashEquals
        } else {
            TokenKind::Slash
        }
    }

    fn scan_regular_expression(&mut self, start: usize) -> Option<ScannedRegularExpressionLiteral> {
        let mut cursor = self.offset;
        let mut in_character_class = false;
        while let Some(byte) = self.bytes.get(cursor).copied() {
            if self.is_unicode_line_separator_at(cursor) {
                return self.finish_unterminated_regular_expression(start, cursor, true);
            }
            match byte {
                b'\n' | b'\r' => {
                    return self.finish_unterminated_regular_expression(start, cursor, true);
                }
                b'\\' => {
                    cursor += 1;
                    if self.bytes.get(cursor).is_none() {
                        return self.finish_unterminated_regular_expression(start, cursor, false);
                    }
                    if self
                        .bytes
                        .get(cursor)
                        .is_some_and(|byte| matches!(byte, b'\n' | b'\r'))
                        || self.is_unicode_line_separator_at(cursor)
                    {
                        return self.finish_unterminated_regular_expression(start, cursor, true);
                    }
                    let Some(character) = self.source.text[cursor..].chars().next() else {
                        return self.finish_unterminated_regular_expression(start, cursor, false);
                    };
                    cursor += character.len_utf8();
                }
                b'[' => {
                    in_character_class = true;
                    cursor += 1;
                }
                b']' => {
                    in_character_class = false;
                    cursor += 1;
                }
                b'/' if !in_character_class => {
                    let pattern_end = cursor;
                    cursor += 1;
                    let flags_start = cursor;
                    while let Some(character) = self.source.text[cursor..].chars().next() {
                        if character == '$'
                            || character == '_'
                            || character.is_alphanumeric()
                            || !character.is_ascii()
                        {
                            cursor += character.len_utf8();
                        } else {
                            break;
                        }
                    }
                    self.offset = cursor;
                    return Some(ScannedRegularExpressionLiteral::terminated(
                        self.source,
                        start,
                        pattern_end,
                        flags_start,
                        cursor,
                    ));
                }
                _ => {
                    let Some(character) = self.source.text[cursor..].chars().next() else {
                        return self.finish_unterminated_regular_expression(start, cursor, false);
                    };
                    cursor += character.len_utf8();
                }
            }
        }
        self.finish_unterminated_regular_expression(start, cursor, false)
    }

    fn finish_unterminated_regular_expression(
        &mut self,
        start: usize,
        end: usize,
        at_line_break: bool,
    ) -> Option<ScannedRegularExpressionLiteral> {
        if !self.tokens.is_empty()
            && !self
                .tokens
                .last()
                .is_some_and(|token| token.kind == TokenKind::Equals)
        {
            self.offset = start + 1;
            return None;
        }
        self.offset = end;
        let span = Span::new(self.source.id, start, end);
        self.diagnostics.push(Diagnostic::at(
            self.source,
            span,
            "Unterminated regular expression literal.".to_string(),
            1161,
        ));
        if at_line_break {
            Some(ScannedRegularExpressionLiteral::unterminated_at_line_break(
                self.source,
                start,
                end,
            ))
        } else {
            Some(ScannedRegularExpressionLiteral::unterminated(
                self.source,
                start,
                end,
            ))
        }
    }

    fn scan_template_start(&mut self, start: usize) -> TokenKind {
        self.scan_template_chunk(start, true)
    }

    fn scan_template_continuation(&mut self, start: usize) -> TokenKind {
        debug_assert_eq!(self.bytes.get(self.offset), Some(&b'}'));
        self.offset += 1;
        self.scan_template_chunk(start, false)
    }

    fn scan_template_chunk(&mut self, start: usize, is_start: bool) -> TokenKind {
        while let Some(byte) = self.bytes.get(self.offset).copied() {
            match byte {
                b'`' => {
                    self.offset += 1;
                    if is_start {
                        let span = Span::new(self.source.id, start, self.offset);
                        self.template_literals
                            .push(ScannedTemplateLiteral::terminated(
                                span,
                                &self.source.text[start..self.offset],
                            ));
                        return TokenKind::NoSubstitutionTemplateLiteral;
                    }
                    self.template_expression_depths.pop();
                    return TokenKind::TemplateTail;
                }
                b'$' if self.bytes.get(self.offset + 1) == Some(&b'{') => {
                    self.offset += 2;
                    if is_start {
                        self.template_expression_depths.push(self.brace_depth);
                        return TokenKind::TemplateHead;
                    }
                    return TokenKind::TemplateMiddle;
                }
                b'\\' => self.skip_escape_sequence(),
                _ => self.advance_character(),
            }
        }
        self.diagnostics.push(Diagnostic::at(
            self.source,
            Span::new(self.source.id, start, self.offset),
            "Unterminated template literal.".to_string(),
            1160,
        ));
        if is_start {
            let span = Span::new(self.source.id, start, self.offset);
            self.template_literals
                .push(ScannedTemplateLiteral::unterminated(
                    span,
                    &self.source.text[start..self.offset],
                ));
            TokenKind::NoSubstitutionTemplateLiteral
        } else {
            self.template_expression_depths.pop();
            TokenKind::TemplateTail
        }
    }

    fn skip_escape_sequence(&mut self) {
        debug_assert_eq!(self.bytes.get(self.offset), Some(&b'\\'));
        self.offset += 1;
        if self.bytes.get(self.offset..self.offset + 2) == Some(b"\r\n") {
            self.offset += 2;
        } else {
            self.advance_character();
        }
    }

    fn advance_character(&mut self) {
        if let Some(character) = self.source.text[self.offset..].chars().next() {
            self.offset += character.len_utf8();
        }
    }

    fn scan_string(&mut self, start: usize, quote: u8) -> TokenKind {
        let scanned = scan_ordinary_string_literal(self.source, start, quote);
        self.offset = scanned.end;
        self.diagnostics.extend(scanned.diagnostics);
        if let Some(literal) = scanned.extended_literal {
            self.string_literals.push(literal);
        }
        if let Some(literal) = scanned.line_continuation_literal {
            self.line_continuation_string_literals.push(literal);
        }
        TokenKind::StringLiteral
    }
}

const fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' || byte >= 0x80
}

const fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

const fn token_can_end_expression(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::PrivateIdentifier
            | TokenKind::NumericLiteral
            | TokenKind::BigIntLiteral
            | TokenKind::StringLiteral
            | TokenKind::RegularExpressionLiteral
            | TokenKind::NoSubstitutionTemplateLiteral
            | TokenKind::TemplateTail
            | TokenKind::RightBrace
            | TokenKind::RightParen
            | TokenKind::RightBracket
            | TokenKind::PlusPlus
            | TokenKind::MinusMinus
            | TokenKind::This
            | TokenKind::Super
            | TokenKind::True
            | TokenKind::False
            | TokenKind::Null
            | TokenKind::Undefined
            | TokenKind::Abstract
            | TokenKind::Accessor
            | TokenKind::Any
            | TokenKind::As
            | TokenKind::Assert
            | TokenKind::Asserts
            | TokenKind::Async
            | TokenKind::BigInt
            | TokenKind::Boolean
            | TokenKind::Constructor
            | TokenKind::Declare
            | TokenKind::Defer
            | TokenKind::From
            | TokenKind::Get
            | TokenKind::Global
            | TokenKind::Implements
            | TokenKind::Infer
            | TokenKind::Interface
            | TokenKind::Intrinsic
            | TokenKind::Is
            | TokenKind::KeyOf
            | TokenKind::Let
            | TokenKind::Module
            | TokenKind::Namespace
            | TokenKind::Never
            | TokenKind::Number
            | TokenKind::Object
            | TokenKind::Of
            | TokenKind::Out
            | TokenKind::Override
            | TokenKind::Package
            | TokenKind::Private
            | TokenKind::Protected
            | TokenKind::Public
            | TokenKind::Readonly
            | TokenKind::Require
            | TokenKind::Satisfies
            | TokenKind::Set
            | TokenKind::Static
            | TokenKind::String
            | TokenKind::Symbol
            | TokenKind::Type
            | TokenKind::Unique
            | TokenKind::Unknown
            | TokenKind::Using
    )
}

fn keyword_kind(text: &str) -> TokenKind {
    match text {
        "abstract" => TokenKind::Abstract,
        "accessor" => TokenKind::Accessor,
        "any" => TokenKind::Any,
        "as" => TokenKind::As,
        "assert" => TokenKind::Assert,
        "asserts" => TokenKind::Asserts,
        "async" => TokenKind::Async,
        "await" => TokenKind::Await,
        "bigint" => TokenKind::BigInt,
        "boolean" => TokenKind::Boolean,
        "break" => TokenKind::Break,
        "case" => TokenKind::Case,
        "catch" => TokenKind::Catch,
        "class" => TokenKind::Class,
        "const" => TokenKind::Const,
        "constructor" => TokenKind::Constructor,
        "continue" => TokenKind::Continue,
        "debugger" => TokenKind::Debugger,
        "declare" => TokenKind::Declare,
        "default" => TokenKind::Default,
        "defer" => TokenKind::Defer,
        "delete" => TokenKind::Delete,
        "do" => TokenKind::Do,
        "else" => TokenKind::Else,
        "enum" => TokenKind::Enum,
        "export" => TokenKind::Export,
        "extends" => TokenKind::Extends,
        "false" => TokenKind::False,
        "finally" => TokenKind::Finally,
        "for" => TokenKind::For,
        "from" => TokenKind::From,
        "function" => TokenKind::Function,
        "get" => TokenKind::Get,
        "global" => TokenKind::Global,
        "if" => TokenKind::If,
        "implements" => TokenKind::Implements,
        "import" => TokenKind::Import,
        "in" => TokenKind::In,
        "infer" => TokenKind::Infer,
        "instanceof" => TokenKind::InstanceOf,
        "interface" => TokenKind::Interface,
        "intrinsic" => TokenKind::Intrinsic,
        "is" => TokenKind::Is,
        "keyof" => TokenKind::KeyOf,
        "let" => TokenKind::Let,
        "module" => TokenKind::Module,
        "namespace" => TokenKind::Namespace,
        "never" => TokenKind::Never,
        "new" => TokenKind::New,
        "null" => TokenKind::Null,
        "number" => TokenKind::Number,
        "object" => TokenKind::Object,
        "of" => TokenKind::Of,
        "out" => TokenKind::Out,
        "override" => TokenKind::Override,
        "package" => TokenKind::Package,
        "private" => TokenKind::Private,
        "protected" => TokenKind::Protected,
        "public" => TokenKind::Public,
        "readonly" => TokenKind::Readonly,
        "require" => TokenKind::Require,
        "return" => TokenKind::Return,
        "satisfies" => TokenKind::Satisfies,
        "set" => TokenKind::Set,
        "static" => TokenKind::Static,
        "string" => TokenKind::String,
        "super" => TokenKind::Super,
        "switch" => TokenKind::Switch,
        "symbol" => TokenKind::Symbol,
        "this" => TokenKind::This,
        "throw" => TokenKind::Throw,
        "true" => TokenKind::True,
        "try" => TokenKind::Try,
        "type" => TokenKind::Type,
        "typeof" => TokenKind::TypeOf,
        "undefined" => TokenKind::Undefined,
        "unique" => TokenKind::Unique,
        "unknown" => TokenKind::Unknown,
        "using" => TokenKind::Using,
        "var" => TokenKind::Var,
        "void" => TokenKind::Void,
        "while" => TokenKind::While,
        "with" => TokenKind::With,
        "yield" => TokenKind::Yield,
        _ => TokenKind::Identifier,
    }
}

pub(super) fn is_plain_strict_binding_identifier(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    text.is_ascii()
        && is_identifier_start(first)
        && rest.iter().copied().all(is_identifier_continue)
        && keyword_kind(text) == TokenKind::Identifier
        && !matches!(text, "eval" | "arguments")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::source::FileId;

    use super::*;

    fn source(text: &str) -> SourceText {
        SourceText::new(
            FileId(7),
            PathBuf::from("scanner-case.ts"),
            Arc::<str>::from(text),
        )
    }

    fn scan(text: &str) -> (SourceText, ScanOutput) {
        let source = source(text);
        let output = scan_source(&source);
        (source, output)
    }

    fn assert_one(text: &str, expected: TokenKind) {
        let (source, output) = scan(text);
        assert!(
            output.diagnostics.is_empty(),
            "unexpected diagnostics for {text:?}: {:?}",
            output.diagnostics
        );
        assert_eq!(
            output
                .tokens
                .iter()
                .map(|token| token.kind)
                .collect::<Vec<_>>(),
            vec![expected, TokenKind::EndOfFile],
            "wrong token for {text:?}"
        );
        assert_eq!(source.slice(output.tokens[0].span), text);
        assert_eq!(output.tokens[0].span.start, 0);
        assert_eq!(output.tokens[0].span.end, text.len() as u32);
    }

    #[test]
    fn recognizes_the_complete_typescript_keyword_set() {
        let cases = [
            ("abstract", TokenKind::Abstract),
            ("accessor", TokenKind::Accessor),
            ("any", TokenKind::Any),
            ("as", TokenKind::As),
            ("assert", TokenKind::Assert),
            ("asserts", TokenKind::Asserts),
            ("async", TokenKind::Async),
            ("await", TokenKind::Await),
            ("bigint", TokenKind::BigInt),
            ("boolean", TokenKind::Boolean),
            ("break", TokenKind::Break),
            ("case", TokenKind::Case),
            ("catch", TokenKind::Catch),
            ("class", TokenKind::Class),
            ("const", TokenKind::Const),
            ("constructor", TokenKind::Constructor),
            ("continue", TokenKind::Continue),
            ("debugger", TokenKind::Debugger),
            ("declare", TokenKind::Declare),
            ("default", TokenKind::Default),
            ("defer", TokenKind::Defer),
            ("delete", TokenKind::Delete),
            ("do", TokenKind::Do),
            ("else", TokenKind::Else),
            ("enum", TokenKind::Enum),
            ("export", TokenKind::Export),
            ("extends", TokenKind::Extends),
            ("false", TokenKind::False),
            ("finally", TokenKind::Finally),
            ("for", TokenKind::For),
            ("from", TokenKind::From),
            ("function", TokenKind::Function),
            ("get", TokenKind::Get),
            ("global", TokenKind::Global),
            ("if", TokenKind::If),
            ("implements", TokenKind::Implements),
            ("import", TokenKind::Import),
            ("in", TokenKind::In),
            ("infer", TokenKind::Infer),
            ("instanceof", TokenKind::InstanceOf),
            ("interface", TokenKind::Interface),
            ("intrinsic", TokenKind::Intrinsic),
            ("is", TokenKind::Is),
            ("keyof", TokenKind::KeyOf),
            ("let", TokenKind::Let),
            ("module", TokenKind::Module),
            ("namespace", TokenKind::Namespace),
            ("never", TokenKind::Never),
            ("new", TokenKind::New),
            ("null", TokenKind::Null),
            ("number", TokenKind::Number),
            ("object", TokenKind::Object),
            ("of", TokenKind::Of),
            ("out", TokenKind::Out),
            ("override", TokenKind::Override),
            ("package", TokenKind::Package),
            ("private", TokenKind::Private),
            ("protected", TokenKind::Protected),
            ("public", TokenKind::Public),
            ("readonly", TokenKind::Readonly),
            ("require", TokenKind::Require),
            ("return", TokenKind::Return),
            ("satisfies", TokenKind::Satisfies),
            ("set", TokenKind::Set),
            ("static", TokenKind::Static),
            ("string", TokenKind::String),
            ("super", TokenKind::Super),
            ("switch", TokenKind::Switch),
            ("symbol", TokenKind::Symbol),
            ("this", TokenKind::This),
            ("throw", TokenKind::Throw),
            ("true", TokenKind::True),
            ("try", TokenKind::Try),
            ("type", TokenKind::Type),
            ("typeof", TokenKind::TypeOf),
            ("undefined", TokenKind::Undefined),
            ("unique", TokenKind::Unique),
            ("unknown", TokenKind::Unknown),
            ("using", TokenKind::Using),
            ("var", TokenKind::Var),
            ("void", TokenKind::Void),
            ("while", TokenKind::While),
            ("with", TokenKind::With),
            ("yield", TokenKind::Yield),
        ];
        for (text, kind) in cases {
            assert_one(text, kind);
        }

        // The pinned TS7 enum reserves `ImmediateKeyword`, but its scanner's
        // `textToKeywordObj` does not map the source spelling to that kind.
        assert_one("immediate", TokenKind::Identifier);
    }

    #[test]
    fn recognizes_modern_punctuation_with_longest_match_spans() {
        let cases = [
            ("...", TokenKind::DotDotDot),
            ("?.", TokenKind::QuestionDot),
            ("??", TokenKind::QuestionQuestion),
            ("??=", TokenKind::QuestionQuestionEquals),
            ("++", TokenKind::PlusPlus),
            ("+=", TokenKind::PlusEquals),
            ("--", TokenKind::MinusMinus),
            ("-=", TokenKind::MinusEquals),
            ("**", TokenKind::StarStar),
            ("*=", TokenKind::StarEquals),
            ("**=", TokenKind::StarStarEquals),
            ("%", TokenKind::Percent),
            ("%=", TokenKind::PercentEquals),
            ("||", TokenKind::BarBar),
            ("|=", TokenKind::BarEquals),
            ("||=", TokenKind::BarBarEquals),
            ("&&", TokenKind::AmpersandAmpersand),
            ("&=", TokenKind::AmpersandEquals),
            ("&&=", TokenKind::AmpersandAmpersandEquals),
            ("^", TokenKind::Caret),
            ("^=", TokenKind::CaretEquals),
            ("</", TokenKind::LessThanSlash),
            ("<=", TokenKind::LessThanEquals),
            ("<<", TokenKind::LessThanLessThan),
            ("<<=", TokenKind::LessThanLessThanEquals),
            (">=", TokenKind::GreaterThanEquals),
            (">>", TokenKind::GreaterThanGreaterThan),
            (">>=", TokenKind::GreaterThanGreaterThanEquals),
            (">>>", TokenKind::GreaterThanGreaterThanGreaterThan),
            (">>>=", TokenKind::GreaterThanGreaterThanGreaterThanEquals),
            ("!=", TokenKind::BangEquals),
            ("!==", TokenKind::BangEqualsEquals),
            ("==", TokenKind::EqualsEquals),
            ("===", TokenKind::EqualsEqualsEquals),
            ("=>", TokenKind::FatArrow),
            ("~", TokenKind::Tilde),
            ("@", TokenKind::At),
            ("#", TokenKind::Hash),
        ];
        for (text, kind) in cases {
            assert_one(text, kind);
        }

        let (_, output) = scan("?.1");
        assert!(output.diagnostics.is_empty());
        assert_eq!(
            output
                .tokens
                .iter()
                .map(|token| token.kind)
                .collect::<Vec<_>>(),
            vec![
                TokenKind::Question,
                TokenKind::NumericLiteral,
                TokenKind::EndOfFile,
            ]
        );
    }

    #[test]
    fn scans_numeric_private_decorator_and_identifier_escape_forms() {
        let text = r"@sealed #field 0 1. 0.25 .5 1e3 1_000 0xff 0b1010 0o755 42n 0xffn \u{006e}ame";
        let (source, output) = scan(text);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let actual = output
            .tokens
            .iter()
            .map(|token| (token.kind, source.slice(token.span)))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                (TokenKind::At, "@"),
                (TokenKind::Identifier, "sealed"),
                (TokenKind::PrivateIdentifier, "#field"),
                (TokenKind::NumericLiteral, "0"),
                (TokenKind::NumericLiteral, "1."),
                (TokenKind::NumericLiteral, "0.25"),
                (TokenKind::NumericLiteral, ".5"),
                (TokenKind::NumericLiteral, "1e3"),
                (TokenKind::NumericLiteral, "1_000"),
                (TokenKind::NumericLiteral, "0xff"),
                (TokenKind::NumericLiteral, "0b1010"),
                (TokenKind::NumericLiteral, "0o755"),
                (TokenKind::BigIntLiteral, "42n"),
                (TokenKind::BigIntLiteral, "0xffn"),
                (TokenKind::Identifier, "\\u{006e}ame"),
                (TokenKind::EndOfFile, ""),
            ]
        );
    }

    #[test]
    fn scans_nested_template_chunks_without_losing_delimiter_spans() {
        assert_one("`plain`", TokenKind::NoSubstitutionTemplateLiteral);

        let text = "`first ${left} middle ${right} last`";
        let (source, output) = scan(text);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let actual = output
            .tokens
            .iter()
            .map(|token| (token.kind, source.slice(token.span)))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                (TokenKind::TemplateHead, "`first ${"),
                (TokenKind::Identifier, "left"),
                (TokenKind::TemplateMiddle, "} middle ${"),
                (TokenKind::Identifier, "right"),
                (TokenKind::TemplateTail, "} last`"),
                (TokenKind::EndOfFile, ""),
            ]
        );

        let text = "`outer ${value + `inner ${item}`} tail`";
        let (source, output) = scan(text);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let actual = output
            .tokens
            .iter()
            .map(|token| (token.kind, source.slice(token.span)))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                (TokenKind::TemplateHead, "`outer ${"),
                (TokenKind::Identifier, "value"),
                (TokenKind::Plus, "+"),
                (TokenKind::TemplateHead, "`inner ${"),
                (TokenKind::Identifier, "item"),
                (TokenKind::TemplateTail, "}`"),
                (TokenKind::TemplateTail, "} tail`"),
                (TokenKind::EndOfFile, ""),
            ]
        );
    }

    #[test]
    fn scans_regex_literals_without_stealing_division_tokens() {
        let text = r"const pattern = /a\/[b-d]+/giu; value / divisor; return /x\/y/;";
        let (source, output) = scan(text);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let actual = output
            .tokens
            .iter()
            .map(|token| (token.kind, source.slice(token.span)))
            .collect::<Vec<_>>();
        assert!(actual.contains(&(TokenKind::RegularExpressionLiteral, r"/a\/[b-d]+/giu")));
        assert!(actual.contains(&(TokenKind::Slash, "/")));
        assert!(actual.contains(&(TokenKind::RegularExpressionLiteral, r"/x\/y/")));
    }

    #[test]
    fn contextual_type_ends_do_not_steal_prefix_or_relational_regex_literals() {
        let text = concat!(
            r"async function f() { return await /[a-z\/]+/; }",
            "\n",
            r"function* g() { yield /[a-z\/]+/; }",
            "\n",
            r"void /[a-z\/]+/;",
            "\n",
            r"value > /[a-z\/]+/.source;",
        );
        let (source, output) = scan(text);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.regular_expression_literals.len(), 4);
        assert_eq!(
            output
                .tokens
                .iter()
                .filter(|token| token.kind == TokenKind::RegularExpressionLiteral)
                .map(|token| source.slice(token.span))
                .collect::<Vec<_>>(),
            vec![r"/[a-z\/]+/"; 4],
        );
    }

    #[test]
    fn records_regex_pattern_flag_spans_and_bounded_unterminated_recovery() {
        let (source, output) = scan("var pattern = /abc/g;");
        let [scanned] = output.regular_expression_literals.as_slice() else {
            panic!("expected one scanned regular expression");
        };
        let literal = scanned.syntax_literal();
        assert_eq!(literal.raw, "/abc/g");
        assert_eq!(literal.pattern, "abc");
        assert_eq!(literal.flags, "g");
        assert_eq!(source.slice(literal.pattern_span), "abc");
        assert_eq!(source.slice(literal.flags_span), "g");
        assert!(literal.terminated);
        assert!(literal.validation_supported());

        let (source, output) = scan("var r = /abc");
        assert_eq!(output.diagnostics.len(), 1);
        assert_eq!(
            (
                output.diagnostics[0].code,
                output.diagnostics[0].start,
                output.diagnostics[0].length,
                output.diagnostics[0].message_text.as_str(),
            ),
            (1161, 8, 4, "Unterminated regular expression literal.")
        );
        let token = output
            .tokens
            .iter()
            .find(|token| token.kind == TokenKind::RegularExpressionLiteral)
            .expect("unterminated recovery remains one regex token");
        assert_eq!(source.slice(token.span), "/abc");
        let literal = output.regular_expression_literals[0].syntax_literal();
        assert!(!literal.terminated);
        assert!(literal.validation_supported());

        let (source, slash_equals_recovery) = scan("/=");
        assert_eq!(slash_equals_recovery.diagnostics[0].code, 1161);
        assert_eq!(
            source.slice(slash_equals_recovery.tokens[0].span),
            "/=",
            "a primary-position slash-equals follows TS7 regex recovery"
        );
        assert_eq!(
            slash_equals_recovery.tokens[0].kind,
            TokenKind::RegularExpressionLiteral
        );

        for text in ["var r = /abc\n", "var r = /abc\u{2028}"] {
            let (_, line_break_recovery) = scan(text);
            let literal = line_break_recovery.regular_expression_literals[0].syntax_literal();
            assert!(literal.recovery_at_line_break, "{text:?}");
            assert!(!literal.validation_supported(), "{text:?}");
        }

        let (_, open_class) = scan("var r = /[/");
        let literal = open_class.regular_expression_literals[0].syntax_literal();
        assert!(!literal.terminated);
        assert!(!literal.validation_supported());
    }

    #[test]
    fn preserves_unterminated_string_and_comment_diagnostics() {
        let (_, string_output) = scan("\"oops");
        assert_eq!(string_output.diagnostics.len(), 1);
        assert_eq!(
            (
                string_output.diagnostics[0].code,
                string_output.diagnostics[0].start,
                string_output.diagnostics[0].length,
                string_output.diagnostics[0].message_text.as_str(),
            ),
            (1002, 0, 5, "Unterminated string literal.")
        );

        let comment = "/* never closes";
        let (_, comment_output) = scan(comment);
        assert_eq!(comment_output.diagnostics.len(), 1);
        assert_eq!(
            (
                comment_output.diagnostics[0].code,
                comment_output.diagnostics[0].start,
                comment_output.diagnostics[0].length,
                comment_output.diagnostics[0].message_text.as_str(),
            ),
            (1010, 0, comment.len() as u32, "'*/' expected.")
        );
    }

    #[test]
    fn valid_modern_lexical_forms_do_not_report_invalid_characters() {
        let text = r#"#!/usr/bin/env node
            import value, { type Shape as Alias } from "pkg";
            @sealed export class Box<T> extends Base implements Shape {
                #value = 0xffn;
                method(...items: T[]) {
                    return this?.#value ?? /[a-z\/]++/v;
                }
            }
        "#;
        let (_, output) = scan(text);
        assert!(
            output
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != 1127),
            "valid lexical forms produced TS1127: {:?}",
            output.diagnostics
        );
    }
}
