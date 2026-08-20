use crate::diagnostics::Diagnostic;
use crate::source::{SourceText, Span};

use super::{Token, TokenKind};

#[derive(Debug)]
pub struct ScanOutput {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn scan_source(source: &SourceText) -> ScanOutput {
    Scanner::new(source).scan()
}

struct Scanner<'a> {
    source: &'a SourceText,
    bytes: &'a [u8],
    offset: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Scanner<'a> {
    fn new(source: &'a SourceText) -> Self {
        Self {
            source,
            bytes: source.text.as_bytes(),
            offset: 0,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
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
            let kind = if is_identifier_start(byte) {
                self.offset += 1;
                while self
                    .bytes
                    .get(self.offset)
                    .is_some_and(|byte| is_identifier_continue(*byte))
                {
                    self.offset += 1;
                }
                keyword_kind(&self.source.text[start..self.offset])
            } else if byte.is_ascii_digit() {
                self.scan_number()
            } else {
                self.scan_punctuation_or_literal(start)
            };
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
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            while self
                .bytes
                .get(self.offset)
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                self.offset += 1;
            }
            if self.bytes.get(self.offset..self.offset + 2) == Some(b"//") {
                self.offset += 2;
                while self
                    .bytes
                    .get(self.offset)
                    .is_some_and(|byte| *byte != b'\n' && *byte != b'\r')
                {
                    self.offset += 1;
                }
                continue;
            }
            if self.bytes.get(self.offset..self.offset + 2) == Some(b"/*") {
                let start = self.offset;
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
                continue;
            }
            break;
        }
    }

    fn scan_number(&mut self) -> TokenKind {
        self.offset += 1;
        while self.bytes.get(self.offset).is_some_and(u8::is_ascii_digit) {
            self.offset += 1;
        }
        if self.bytes.get(self.offset) == Some(&b'.')
            && self
                .bytes
                .get(self.offset + 1)
                .is_some_and(u8::is_ascii_digit)
        {
            self.offset += 1;
            while self.bytes.get(self.offset).is_some_and(u8::is_ascii_digit) {
                self.offset += 1;
            }
        }
        TokenKind::NumericLiteral
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
            b'.' => TokenKind::Dot,
            b'?' => TokenKind::Question,
            b'+' => TokenKind::Plus,
            b'*' => TokenKind::Star,
            b'/' => TokenKind::Slash,
            b'|' => TokenKind::Bar,
            b'&' => TokenKind::Ampersand,
            b'<' => TokenKind::LessThan,
            b'>' => TokenKind::GreaterThan,
            b'!' => TokenKind::Bang,
            b'-' => TokenKind::Minus,
            b'=' if self.bytes.get(self.offset) == Some(&b'>') => {
                self.offset += 1;
                TokenKind::FatArrow
            }
            b'=' => TokenKind::Equals,
            b'\'' | b'"' => self.scan_string(start, byte),
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

    fn scan_string(&mut self, start: usize, quote: u8) -> TokenKind {
        let mut terminated = false;
        while let Some(byte) = self.bytes.get(self.offset).copied() {
            if byte == quote {
                self.offset += 1;
                terminated = true;
                break;
            }
            if byte == b'\\' {
                self.offset = (self.offset + 2).min(self.bytes.len());
                continue;
            }
            if byte == b'\n' || byte == b'\r' {
                break;
            }
            self.offset += 1;
        }
        if !terminated {
            self.diagnostics.push(Diagnostic::at(
                self.source,
                Span::new(self.source.id, start, self.offset),
                "Unterminated string literal.".to_string(),
                1002,
            ));
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

fn keyword_kind(text: &str) -> TokenKind {
    match text {
        "let" => TokenKind::Let,
        "const" => TokenKind::Const,
        "var" => TokenKind::Var,
        "function" => TokenKind::Function,
        "return" => TokenKind::Return,
        "type" => TokenKind::Type,
        "interface" => TokenKind::Interface,
        "export" => TokenKind::Export,
        "default" => TokenKind::Default,
        "declare" => TokenKind::Declare,
        "async" => TokenKind::Async,
        "true" => TokenKind::True,
        "false" => TokenKind::False,
        "null" => TokenKind::Null,
        "undefined" => TokenKind::Undefined,
        "any" => TokenKind::Any,
        "unknown" => TokenKind::Unknown,
        "never" => TokenKind::Never,
        "void" => TokenKind::Void,
        "boolean" => TokenKind::Boolean,
        "number" => TokenKind::Number,
        "string" => TokenKind::String,
        "bigint" => TokenKind::BigInt,
        "keyof" => TokenKind::KeyOf,
        "as" => TokenKind::As,
        _ => TokenKind::Identifier,
    }
}
