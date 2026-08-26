use crate::diagnostics::Diagnostic;
use crate::source::{SourceText, Span};

use super::numeric_literal::{
    ScannedNumericLiteral, ScannedSeparatedNumberLiteral, scan_numeric_literal,
};
use super::regular_expression::ScannedRegularExpressionLiteral;
use super::string_literal::{
    AuthoredEscape, ScannedCookedStringLiteral, ScannedStringLiteral, decode_authored_escape,
    scan_ordinary_string_literal,
};
use super::template_literal::ScannedTemplateLiteral;
use super::{
    CommentClass, CommentKind, CommentPlacement, CommentSourcePosition, CommentTrivia, Token,
    TokenKind, is_single_line_whitespace,
};

#[derive(Debug, Default)]
pub struct ScanOutput {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
    pub(super) identifier_values: Vec<ScannedIdentifierValue>,
    pub(super) template_literals: Vec<ScannedTemplateLiteral>,
    pub(super) string_literals: Vec<ScannedStringLiteral>,
    pub(super) cooked_string_literals: Vec<ScannedCookedStringLiteral>,
    pub(super) numeric_literals: Vec<ScannedNumericLiteral>,
    pub(super) separated_numeric_literals: Vec<ScannedSeparatedNumberLiteral>,
    pub(super) numeric_separator_spans: Vec<Span>,
    pub(super) has_unmodeled_numeric_separator: bool,
    pub(super) regular_expression_literals: Vec<ScannedRegularExpressionLiteral>,
    pub(super) comments: Vec<CommentTrivia>,
    pub(super) has_unicode_line_comment_terminator: bool,
}

/// Scanner-owned semantic spelling for one authored identifier escape.
///
/// `span` continues to identify the exact source spelling. The cooked value is
/// the sole identifier identity consumed by the parser, while escape kind
/// remains lexical provenance for consumers that must preserve authored text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScannedIdentifierValue {
    pub(super) span: Span,
    pub(super) cooked: String,
    pub(super) escape: IdentifierEscapeProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IdentifierEscapeProvenance {
    Unicode,
    ExtendedUnicode,
    UnicodeAndExtendedUnicode,
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
    output: ScanOutput,
}

impl<'a> Scanner<'a> {
    fn new(source: &'a SourceText) -> Self {
        Self {
            source,
            bytes: source.text.as_bytes(),
            offset: 0,
            brace_depth: 0,
            template_expression_depths: Vec::new(),
            output: ScanOutput::default(),
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
            } else if is_identifier_start(byte) || self.is_identifier_escape_start_at(self.offset) {
                self.scan_identifier(start, false)
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
            let span = Span::new(self.source.id, start, self.offset);
            if matches!(kind, TokenKind::NumericLiteral | TokenKind::BigIntLiteral)
                && self.bytes[start..self.offset].contains(&b'_')
            {
                self.output.numeric_separator_spans.push(span);
            }
            self.output.tokens.push(Token { kind, span });
        }
        let end = self.bytes.len();
        self.output.tokens.push(Token {
            kind: TokenKind::EndOfFile,
            span: Span::new(self.source.id, end, end),
        });
        self.mark_detached_pinned_runs();
        self.output
    }

    fn mark_detached_pinned_runs(&mut self) {
        let leading = self.output.comments.partition_point(|comment| {
            comment.source_position == CommentSourcePosition::SourceLeading
        });
        let token_start = self.output.tokens[0].span.start;
        let mut start = 0;
        for end in 0..leading {
            let next = if end + 1 < leading {
                self.output.comments[end + 1].span.start
            } else {
                token_start
            };
            let previous = self.output.comments[end].span.end as usize;
            if self.source.text[previous..next as usize].lines().count() > 1 {
                for comment in &mut self.output.comments[start..=end] {
                    if comment.class == CommentClass::Pinned {
                        comment.class = CommentClass::DetachedPinned;
                    }
                }
                start = end + 1;
            }
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            if self.offset == 0 && self.bytes.get(..3) == Some(&[0xef, 0xbb, 0xbf]) {
                self.offset += 3;
            }
            if self.offset == 0 && self.bytes.get(..2) == Some(b"#!") {
                self.skip_line_body();
                continue;
            }
            while self.skip_one_whitespace() {}
            if self.bytes.get(self.offset..self.offset + 2) == Some(b"//") {
                let start = self.offset;
                let placement = self.comment_placement(start);
                let source_position = self.comment_source_position();
                self.skip_line_body();
                let plain = self.is_plain_line_comment(start, self.offset);
                let class = if is_recognized_triple_slash(&self.source.text[start..self.offset]) {
                    CommentClass::TripleSlashReference
                } else {
                    CommentClass::Ordinary
                };
                self.output.comments.push(CommentTrivia {
                    span: Span::new(self.source.id, start, self.offset),
                    preceding_token_end: self.output.tokens.last().map(|token| token.span.end),
                    preceding_token_kind: self.output.tokens.last().map(|token| token.kind),
                    kind: CommentKind::Line,
                    class,
                    jsdoc: false,
                    placement,
                    source_position,
                    has_trailing_line_break: self.has_line_break_at_offset(),
                    plain,
                });
                continue;
            }
            if self.bytes.get(self.offset..self.offset + 2) == Some(b"/*") {
                let start = self.offset;
                let placement = self.comment_placement(start);
                let source_position = self.comment_source_position();
                self.offset += 2;
                while self.offset + 1 < self.bytes.len()
                    && self.bytes.get(self.offset..self.offset + 2) != Some(b"*/")
                {
                    self.offset += 1;
                }
                if self.offset + 1 < self.bytes.len() {
                    self.offset += 2;
                } else {
                    self.output.diagnostics.push(Diagnostic::at(
                        self.source,
                        Span::new(self.source.id, start, self.bytes.len()),
                        "'*/' expected.".to_string(),
                        1010,
                    ));
                    self.offset = self.bytes.len();
                }
                self.output.comments.push(CommentTrivia {
                    span: Span::new(self.source.id, start, self.offset),
                    preceding_token_end: self.output.tokens.last().map(|token| token.span.end),
                    preceding_token_kind: self.output.tokens.last().map(|token| token.kind),
                    kind: CommentKind::Block,
                    class: if self.bytes.get(start + 2) == Some(&b'!') {
                        CommentClass::Pinned
                    } else {
                        CommentClass::Ordinary
                    },
                    jsdoc: self.bytes.get(start..start + 3) == Some(b"/**")
                        && self.bytes.get(start + 3) != Some(&b'/'),
                    placement,
                    source_position,
                    has_trailing_line_break: self.has_line_break_at_offset(),
                    plain: false,
                });
                continue;
            }
            break;
        }
    }

    fn skip_line_body(&mut self) {
        self.offset += 2;
        while self
            .bytes
            .get(self.offset)
            .is_some_and(|byte| *byte != b'\n' && *byte != b'\r')
            && !self.is_unicode_line_separator_at(self.offset)
        {
            self.offset += 1;
        }
        self.output.has_unicode_line_comment_terminator |=
            self.is_unicode_line_separator_at(self.offset);
    }

    fn comment_placement(&self, comment_start: usize) -> CommentPlacement {
        let Some(previous) = self.output.tokens.last() else {
            return CommentPlacement::Leading;
        };
        if self.contains_line_break(previous.span.end as usize, comment_start) {
            CommentPlacement::Leading
        } else {
            CommentPlacement::Trailing
        }
    }

    const fn comment_source_position(&self) -> CommentSourcePosition {
        if self.output.tokens.is_empty() {
            CommentSourcePosition::SourceLeading
        } else {
            CommentSourcePosition::AfterToken
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

    fn scan_identifier(&mut self, start: usize, private: bool) -> TokenKind {
        self.consume_identifier_character();
        while self
            .bytes
            .get(self.offset)
            .is_some_and(|byte| is_identifier_continue(*byte))
            || self.is_identifier_escape_part_at(self.offset)
        {
            self.consume_identifier_character();
        }
        let text = &self.source.text[start..self.offset];
        if text.as_bytes().contains(&b'\\') {
            let CookedIdentifier { cooked, escape } = cook_identifier(text);
            self.output.identifier_values.push(ScannedIdentifierValue {
                span: Span::new(self.source.id, start, self.offset),
                cooked: cooked.clone(),
                escape,
            });
            if private {
                TokenKind::PrivateIdentifier
            } else {
                TokenKind::from_keyword(&cooked)
            }
        } else if private {
            TokenKind::PrivateIdentifier
        } else {
            TokenKind::from_keyword(text)
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

    fn is_identifier_escape_start_at(&self, offset: usize) -> bool {
        identifier_escape_at(&self.source.text, offset)
            .is_some_and(|escape| is_identifier_start_character(escape.character))
    }

    fn is_identifier_escape_part_at(&self, offset: usize) -> bool {
        identifier_escape_at(&self.source.text, offset)
            .is_some_and(|escape| is_identifier_part_character(escape.character))
    }

    fn identifier_escape_len_at(&self, offset: usize) -> Option<usize> {
        identifier_escape_at(&self.source.text, offset).map(|escape| escape.length)
    }

    fn scan_number(&mut self, start: usize) -> TokenKind {
        let scanned = scan_numeric_literal(self.source, start, self.output.tokens.last().copied());
        self.offset = scanned.end;
        self.output.diagnostics.extend(scanned.diagnostics);
        if let Some(literal) = scanned.recovery_literal {
            self.output.numeric_literals.push(literal);
        }
        if let Some(literal) = scanned.separated_literal {
            self.output.separated_numeric_literals.push(literal);
        }
        self.output.has_unmodeled_numeric_separator |= scanned.has_unmodeled_separator;
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
                || self.is_identifier_escape_start_at(self.offset) =>
            {
                self.scan_identifier(start, true)
            }
            b'#' => TokenKind::Hash,
            b'\'' | b'"' => self.scan_string(start, byte),
            b'`' => self.scan_template_start(start),
            b'\\' => {
                // TypeScript reports an invalid identifier escape at the
                // authored backslash with a zero-width scanner diagnostic.
                self.output.diagnostics.push(Diagnostic::at(
                    self.source,
                    Span::new(self.source.id, start, start),
                    "Invalid character.".to_string(),
                    1127,
                ));
                TokenKind::Identifier
            }
            _ => {
                self.output.diagnostics.push(Diagnostic::at(
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
            .output
            .tokens
            .last()
            .is_some_and(|token| token_can_end_expression(token.kind))
    }

    fn scan_slash_or_regular_expression(&mut self, start: usize) -> TokenKind {
        if self.can_start_regular_expression()
            && let Some(literal) = self.scan_regular_expression(start)
        {
            self.output.regular_expression_literals.push(literal);
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
                    return Some(ScannedRegularExpressionLiteral::from_source(
                        self.source,
                        start,
                        pattern_end,
                        flags_start,
                        cursor,
                        true,
                        false,
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
        if !self.output.tokens.is_empty()
            && !self
                .output
                .tokens
                .last()
                .is_some_and(|token| token.kind == TokenKind::Equals)
        {
            self.offset = start + 1;
            return None;
        }
        self.offset = end;
        let span = Span::new(self.source.id, start, end);
        self.output.diagnostics.push(Diagnostic::at(
            self.source,
            span,
            "Unterminated regular expression literal.".to_string(),
            1161,
        ));
        Some(ScannedRegularExpressionLiteral::from_source(
            self.source,
            start,
            end,
            end,
            end,
            false,
            at_line_break,
        ))
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
                        self.output
                            .template_literals
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
        self.output.diagnostics.push(Diagnostic::at(
            self.source,
            Span::new(self.source.id, start, self.offset),
            "Unterminated template literal.".to_string(),
            1160,
        ));
        if is_start {
            let span = Span::new(self.source.id, start, self.offset);
            self.output
                .template_literals
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
        self.output.diagnostics.extend(scanned.diagnostics);
        if let Some(literal) = scanned.extended_literal {
            self.output.string_literals.push(literal);
        }
        if let Some(literal) = scanned.cooked_literal {
            self.output.cooked_string_literals.push(literal);
        }
        TokenKind::StringLiteral
    }
}

struct CookedIdentifier {
    cooked: String,
    escape: IdentifierEscapeProvenance,
}

fn cook_identifier(text: &str) -> CookedIdentifier {
    let bytes = text.as_bytes();
    let mut cooked = String::with_capacity(text.len());
    let mut offset = 0;
    let mut unicode = false;
    let mut extended_unicode = false;
    while offset < bytes.len() {
        if let Some(identifier_escape) = identifier_escape_at(text, offset) {
            unicode |= identifier_escape.escape == IdentifierEscapeProvenance::Unicode;
            extended_unicode |=
                identifier_escape.escape == IdentifierEscapeProvenance::ExtendedUnicode;
            cooked.push(identifier_escape.character);
            offset += identifier_escape.length;
            continue;
        }
        let Some(character) = text[offset..].chars().next() else {
            break;
        };
        cooked.push(character);
        offset += character.len_utf8();
    }
    let escape = match (unicode, extended_unicode) {
        (true, true) => IdentifierEscapeProvenance::UnicodeAndExtendedUnicode,
        (true, false) => IdentifierEscapeProvenance::Unicode,
        (false, true) => IdentifierEscapeProvenance::ExtendedUnicode,
        (false, false) => unreachable!("cooking requires an identifier escape"),
    };
    CookedIdentifier { cooked, escape }
}

#[derive(Debug, Clone, Copy)]
struct IdentifierEscape {
    character: char,
    length: usize,
    escape: IdentifierEscapeProvenance,
}

fn identifier_escape_at(text: &str, offset: usize) -> Option<IdentifierEscape> {
    let bytes = text.as_bytes();
    if bytes.get(offset..offset + 2) != Some(b"\\u") {
        return None;
    }
    let mut end = offset;
    let (value, escape) = match decode_authored_escape(text, &mut end, text.len()) {
        AuthoredEscape::CodePoint(value) => (value, IdentifierEscapeProvenance::Unicode),
        AuthoredEscape::ExtendedUnicode {
            digits_start,
            digits_end,
            value,
            closed: true,
        } if digits_start < digits_end && digits_end - digits_start <= 6 => (
            u32::try_from(value).ok()?,
            IdentifierEscapeProvenance::ExtendedUnicode,
        ),
        _ => return None,
    };
    // Fixed-width escapes denote individual identifier code points. A
    // surrogate code unit is invalid independently and cannot pair with the
    // following escape to manufacture an astral identifier.
    let character = char::from_u32(value)?;
    Some(IdentifierEscape {
        character,
        length: end - offset,
        escape,
    })
}

const fn is_identifier_start_character(character: char) -> bool {
    if character.is_ascii() {
        is_identifier_start(character as u8)
    } else {
        // Keep escaped scalar acceptance aligned with the rewrite scanner's
        // current authored non-ASCII domain. Invalid UTF-16 units were already
        // rejected by `char::from_u32` above.
        true
    }
}

const fn is_identifier_part_character(character: char) -> bool {
    if character.is_ascii() {
        is_identifier_continue(character as u8)
    } else {
        true
    }
}

fn is_recognized_triple_slash(text: &str) -> bool {
    let Some(body) = text.strip_prefix("///") else {
        return false;
    };
    let compact = body.replace(char::is_whitespace, "");
    compact
        .strip_suffix("/>")
        .and_then(|body| body.split_once('='))
        .is_some_and(|(head, value)| {
            matches!(value.as_bytes().first(), Some(b'\'' | b'"'))
                && match head {
                    "<referencepath"
                    | "<referencetypes"
                    | "<referencelib"
                    | "<amd-dependencypath"
                    | "<amd-modulename" => true,
                    "<referenceno-default-lib" => {
                        value.starts_with("'true'") || value.starts_with("\"true\"")
                    }
                    _ => false,
                }
        })
}

const fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' || byte >= 0x80
}

const fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

const fn token_can_end_expression(kind: TokenKind) -> bool {
    (kind.is_identifier() && !matches!(kind, TokenKind::Await | TokenKind::Yield))
        || matches!(
            kind,
            TokenKind::PrivateIdentifier
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
        )
}

#[cfg(test)]
#[path = "../../rewrite-tests/scanner_lexical_unit.rs"]
mod tests;
