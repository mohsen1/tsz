//! Scanner implementation - the lexical analyzer for TypeScript.
//!
//! This module implements the core Scanner struct that tokenizes TypeScript source code.
//! It's designed to produce the same token stream as TypeScript's scanner.ts.
//!
//! IMPORTANT: All positions are character-based (like JavaScript's string indexing),
//! NOT byte-based. This ensures compatibility with TypeScript's scanner positions.

use crate::char_codes::CharacterCodes;
use crate::interner::{Atom, Interner};
use crate::scanner::SyntaxKind;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

// =============================================================================
// Token Flags
// =============================================================================

/// Token flags indicating special properties of scanned tokens.
#[wasm_bindgen]
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TokenFlags {
    #[default]
    None = 0,
    PrecedingLineBreak = 1,
    PrecedingJSDocComment = 2,
    Unterminated = 4,
    ExtendedUnicodeEscape = 8,
    Scientific = 16,
    Octal = 32,
    HexSpecifier = 64,
    BinarySpecifier = 128,
    OctalSpecifier = 256,
    ContainsSeparator = 512,
    UnicodeEscape = 1024,
    ContainsInvalidEscape = 2048,
    HexEscape = 4096,
    ContainsLeadingZero = 8192,
    ContainsInvalidSeparator = 16384,
    PrecedingJSDocLeadingAsterisks = 32768,
}

// =============================================================================
// Scanner State
// =============================================================================

/// A snapshot of scanner state for look-ahead.
#[derive(Clone)]
pub struct ScannerSnapshot {
    pub pos: usize,
    pub full_start_pos: usize,
    pub token_start: usize,
    pub token: SyntaxKind,
    pub token_value: String,
    pub token_flags: u32,
    pub token_atom: Atom,
    pub token_invalid_separator_pos: Option<usize>,
    pub token_invalid_separator_is_consecutive: bool,
}

/// The scanner state that holds the current position and token information.
///
/// ZERO-COPY OPTIMIZATION: Source is stored as UTF-8 text directly (no Vec<char>).
/// For ASCII-only files (99% of TypeScript), byte position == character position.
/// Positions are byte-based internally for performance, converted when needed.
#[wasm_bindgen]
pub struct ScannerState {
    /// The source text as UTF-8 text, shared so we don't duplicate per phase.
    ///
    /// Note: this is still owned memory (Rust must own the bytes), but it can be shared
    /// between the scanner, parser, and Thin AST without cloning the full file text.
    source: Arc<str>,
    /// Current byte position
    pos: usize,
    /// End byte position
    end: usize,
    /// Full start position including leading trivia (byte offset)
    full_start_pos: usize,
    /// Token start position (excluding trivia, byte offset)
    token_start: usize,
    /// Current token kind
    token: SyntaxKind,
    /// Current token's string value
    token_value: String,
    /// Token flags
    token_flags: u32,
    /// First invalid numeric separator position, if any (byte offset)
    token_invalid_separator_pos: Option<usize>,
    /// Whether the first invalid numeric separator is consecutive
    token_invalid_separator_is_consecutive: bool,
    /// Whether to skip trivia (whitespace, comments)
    skip_trivia: bool,
    /// String interner for identifier deduplication
    #[wasm_bindgen(skip)]
    pub interner: Interner,
    /// Interned atom for current identifier token (avoids string comparison)
    token_atom: Atom,
}

#[wasm_bindgen]
impl ScannerState {
    /// Create a new scanner state with the given text.
    /// ZERO-COPY: No Vec<char> allocation, works directly with UTF-8 bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(text: String, skip_trivia: bool) -> ScannerState {
        let end = text.len(); // byte length
        let mut interner = Interner::new();
        interner.intern_common(); // Pre-intern common keywords
        let source: Arc<str> = Arc::from(text.into_boxed_str());
        ScannerState {
            source,
            pos: 0,
            end,
            full_start_pos: 0,
            token_start: 0,
            token: SyntaxKind::Unknown,
            token_value: String::new(),
            token_flags: 0,
            token_invalid_separator_pos: None,
            token_invalid_separator_is_consecutive: false,
            skip_trivia,
            interner,
            token_atom: Atom::NONE,
        }
    }

    /// Get the current position (end position of current token).
    #[wasm_bindgen(js_name = getPos)]
    pub fn get_pos(&self) -> usize {
        self.pos
    }

    /// Set the current position (used for rescanning compound tokens).
    /// This allows consuming partial tokens like splitting `>>` into `>` + `>`.
    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Get the full start position (including leading trivia).
    #[wasm_bindgen(js_name = getTokenFullStart)]
    pub fn get_token_full_start(&self) -> usize {
        self.full_start_pos
    }

    /// Get the start position of the current token (excluding trivia).
    #[wasm_bindgen(js_name = getTokenStart)]
    pub fn get_token_start(&self) -> usize {
        self.token_start
    }

    /// Get the end position of the current token.
    #[wasm_bindgen(js_name = getTokenEnd)]
    pub fn get_token_end(&self) -> usize {
        self.pos
    }

    /// Get the current token kind.
    #[wasm_bindgen(js_name = getToken)]
    pub fn get_token(&self) -> SyntaxKind {
        self.token
    }

    /// Get the current token's string value.
    #[wasm_bindgen(js_name = getTokenValue)]
    pub fn get_token_value(&self) -> String {
        self.token_value.clone()
    }

    /// Get the current token's text from the source.
    #[wasm_bindgen(js_name = getTokenText)]
    pub fn get_token_text(&self) -> String {
        self.source[self.token_start..self.pos].to_string()
    }

    /// Get the token flags.
    #[wasm_bindgen(js_name = getTokenFlags)]
    pub fn get_token_flags(&self) -> u32 {
        self.token_flags
    }

    /// Check if there was a preceding line break.
    #[wasm_bindgen(js_name = hasPrecedingLineBreak)]
    pub fn has_preceding_line_break(&self) -> bool {
        (self.token_flags & TokenFlags::PrecedingLineBreak as u32) != 0
    }

    /// Check if the token is unterminated.
    #[wasm_bindgen(js_name = isUnterminated)]
    pub fn is_unterminated(&self) -> bool {
        (self.token_flags & TokenFlags::Unterminated as u32) != 0
    }

    /// Check if the current token is an identifier.
    #[wasm_bindgen(js_name = isIdentifier)]
    pub fn is_identifier(&self) -> bool {
        self.token == SyntaxKind::Identifier
            || (self.token as u16) > (SyntaxKind::WithKeyword as u16)
    }

    /// Check if the current token is a reserved word.
    #[wasm_bindgen(js_name = isReservedWord)]
    pub fn is_reserved_word(&self) -> bool {
        let t = self.token as u16;
        t >= SyntaxKind::BreakKeyword as u16 && t <= SyntaxKind::WithKeyword as u16
    }

    /// Set the text to scan.
    /// ZERO-COPY: Works directly with UTF-8 bytes.
    #[wasm_bindgen(js_name = setText)]
    pub fn set_text(&mut self, text: String, start: Option<usize>, length: Option<usize>) {
        let start = start.unwrap_or(0);
        let len = length.unwrap_or(text.len() - start);
        self.source = Arc::from(text.into_boxed_str());
        self.pos = start;
        self.end = start + len;
        self.full_start_pos = start;
        self.token_start = start;
        self.token = SyntaxKind::Unknown;
        self.token_value = String::new();
        self.token_flags = 0;
    }

    /// Reset the token state to a specific position.
    #[wasm_bindgen(js_name = resetTokenState)]
    pub fn reset_token_state(&mut self, new_pos: usize) {
        self.pos = new_pos;
        self.full_start_pos = new_pos;
        self.token_start = new_pos;
        self.token = SyntaxKind::Unknown;
        self.token_value = String::new();
        self.token_flags = 0;
    }

    /// Get the source text.
    #[wasm_bindgen(js_name = getText)]
    pub fn get_text(&self) -> String {
        self.source.to_string()
    }

    // =========================================================================
    // Helper methods (byte-indexed for zero-copy performance)
    // =========================================================================

    /// Get the byte at the given index. Returns None if out of bounds.
    #[inline]
    #[allow(dead_code)] // Infrastructure for scanner extensions
    fn byte_at(&self, index: usize) -> Option<u8> {
        self.source.as_bytes().get(index).copied()
    }

    /// Get byte at index as u32 char code. Returns 0 if out of bounds.
    /// FAST PATH: For ASCII bytes (0-127), this is the character code.
    #[inline(always)]
    fn char_code_unchecked(&self, index: usize) -> u32 {
        let bytes = self.source.as_bytes();
        if index < bytes.len() {
            let b = bytes[index];
            if b < 128 {
                // ASCII: byte value == char code
                b as u32
            } else {
                // Non-ASCII: decode UTF-8 char
                self.source[index..]
                    .chars()
                    .next()
                    .map(|c| c as u32)
                    .unwrap_or(0)
            }
        } else {
            0
        }
    }

    /// Get the character code at the given byte index.
    /// Returns None if out of bounds.
    #[inline]
    fn char_code_at(&self, index: usize) -> Option<u32> {
        let bytes = self.source.as_bytes();
        if index < bytes.len() {
            let b = bytes[index];
            if b < 128 {
                Some(b as u32)
            } else {
                self.source[index..].chars().next().map(|c| c as u32)
            }
        } else {
            None
        }
    }

    #[inline]
    #[allow(dead_code)] // Infrastructure for scanner extensions
    fn is_at_end(&self) -> bool {
        self.pos >= self.end
    }

    /// Get byte length of character at position (1 for ASCII, 1-4 for UTF-8)
    #[inline(always)]
    fn char_len_at(&self, index: usize) -> usize {
        let bytes = self.source.as_bytes();
        if index >= bytes.len() {
            return 0;
        }
        let b = bytes[index];
        if b < 128 {
            1 // ASCII
        } else if b < 0xE0 {
            2 // 2-byte UTF-8
        } else if b < 0xF0 {
            3 // 3-byte UTF-8
        } else {
            4 // 4-byte UTF-8
        }
    }

    /// Get a substring from start to end byte indices.
    #[inline]
    fn substring(&self, start: usize, end: usize) -> String {
        let len = self.source.len();
        let clamped_start = start.min(len);
        let clamped_end = end.min(len);
        if clamped_start >= clamped_end {
            return String::new();
        }
        self.source[clamped_start..clamped_end].to_string()
    }

    // =========================================================================
    // Scanning methods
    // =========================================================================

    /// Scan the next token.
    #[wasm_bindgen]
    pub fn scan(&mut self) -> SyntaxKind {
        self.full_start_pos = self.pos;
        self.token_flags = 0;
        self.token_invalid_separator_pos = None;
        self.token_invalid_separator_is_consecutive = false;
        self.token_value.clear();
        self.token_atom = Atom::NONE; // Reset atom for non-identifier tokens

        loop {
            self.token_start = self.pos;

            if self.pos >= self.end {
                self.token = SyntaxKind::EndOfFileToken;
                return self.token;
            }

            let ch = self.char_code_unchecked(self.pos);

            match ch {
                // Newlines
                CharacterCodes::LINE_FEED | CharacterCodes::CARRIAGE_RETURN => {
                    self.token_flags |= TokenFlags::PrecedingLineBreak as u32;
                    if self.skip_trivia {
                        self.pos += 1;
                        if ch == CharacterCodes::CARRIAGE_RETURN
                            && self.pos < self.end
                            && self.char_code_unchecked(self.pos) == CharacterCodes::LINE_FEED
                        {
                            self.pos += 1;
                        }
                        continue;
                    } else {
                        if ch == CharacterCodes::CARRIAGE_RETURN
                            && self.pos + 1 < self.end
                            && self.char_code_unchecked(self.pos + 1) == CharacterCodes::LINE_FEED
                        {
                            self.pos += 2;
                        } else {
                            self.pos += 1;
                        }
                        self.token = SyntaxKind::NewLineTrivia;
                        return self.token;
                    }
                }

                // Whitespace - ASCII single-byte chars and NON_BREAKING_SPACE (2 bytes in UTF-8)
                CharacterCodes::TAB
                | CharacterCodes::VERTICAL_TAB
                | CharacterCodes::FORM_FEED
                | CharacterCodes::SPACE
                | CharacterCodes::NON_BREAKING_SPACE => {
                    if self.skip_trivia {
                        // Use char_len_at for proper UTF-8 handling (NON_BREAKING_SPACE is 2 bytes)
                        self.pos += self.char_len_at(self.pos);
                        while self.pos < self.end
                            && is_white_space_single_line(self.char_code_unchecked(self.pos))
                        {
                            self.pos += self.char_len_at(self.pos);
                        }
                        continue;
                    } else {
                        while self.pos < self.end
                            && is_white_space_single_line(self.char_code_unchecked(self.pos))
                        {
                            self.pos += self.char_len_at(self.pos);
                        }
                        self.token = SyntaxKind::WhitespaceTrivia;
                        return self.token;
                    }
                }

                // BOM (Byte Order Mark) - 3 bytes in UTF-8
                CharacterCodes::BYTE_ORDER_MARK => {
                    if self.skip_trivia {
                        self.pos += 3; // BOM is 3 bytes in UTF-8
                        while self.pos < self.end
                            && is_white_space_single_line(self.char_code_unchecked(self.pos))
                        {
                            self.pos += self.char_len_at(self.pos);
                        }
                        continue;
                    } else {
                        self.pos += 3; // BOM is 3 bytes in UTF-8
                        while self.pos < self.end
                            && is_white_space_single_line(self.char_code_unchecked(self.pos))
                        {
                            self.pos += self.char_len_at(self.pos);
                        }
                        self.token = SyntaxKind::WhitespaceTrivia;
                        return self.token;
                    }
                }

                // Punctuation - Single characters
                CharacterCodes::OPEN_BRACE => {
                    self.pos += 1;
                    self.token = SyntaxKind::OpenBraceToken;
                    return self.token;
                }
                CharacterCodes::CLOSE_BRACE => {
                    self.pos += 1;
                    self.token = SyntaxKind::CloseBraceToken;
                    return self.token;
                }
                CharacterCodes::OPEN_PAREN => {
                    self.pos += 1;
                    self.token = SyntaxKind::OpenParenToken;
                    return self.token;
                }
                CharacterCodes::CLOSE_PAREN => {
                    self.pos += 1;
                    self.token = SyntaxKind::CloseParenToken;
                    return self.token;
                }
                CharacterCodes::OPEN_BRACKET => {
                    self.pos += 1;
                    self.token = SyntaxKind::OpenBracketToken;
                    return self.token;
                }
                CharacterCodes::CLOSE_BRACKET => {
                    self.pos += 1;
                    self.token = SyntaxKind::CloseBracketToken;
                    return self.token;
                }
                CharacterCodes::SEMICOLON => {
                    self.pos += 1;
                    self.token = SyntaxKind::SemicolonToken;
                    return self.token;
                }
                CharacterCodes::COMMA => {
                    self.pos += 1;
                    self.token = SyntaxKind::CommaToken;
                    return self.token;
                }
                CharacterCodes::TILDE => {
                    self.pos += 1;
                    self.token = SyntaxKind::TildeToken;
                    return self.token;
                }
                CharacterCodes::AT => {
                    self.pos += 1;
                    self.token = SyntaxKind::AtToken;
                    return self.token;
                }
                CharacterCodes::COLON => {
                    self.pos += 1;
                    self.token = SyntaxKind::ColonToken;
                    return self.token;
                }

                // Multi-character punctuation
                CharacterCodes::DOT => {
                    if self.pos + 1 < self.end && is_digit(self.char_code_unchecked(self.pos + 1)) {
                        self.scan_number();
                        return self.token;
                    }
                    if self.pos + 2 < self.end
                        && self.char_code_unchecked(self.pos + 1) == CharacterCodes::DOT
                        && self.char_code_unchecked(self.pos + 2) == CharacterCodes::DOT
                    {
                        self.pos += 3;
                        self.token = SyntaxKind::DotDotDotToken;
                        return self.token;
                    }
                    self.pos += 1;
                    self.token = SyntaxKind::DotToken;
                    return self.token;
                }

                // Exclamation
                CharacterCodes::EXCLAMATION => {
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::EQUALS) {
                        if self.char_code_at(self.pos + 2) == Some(CharacterCodes::EQUALS) {
                            self.pos += 3;
                            self.token = SyntaxKind::ExclamationEqualsEqualsToken;
                            return self.token;
                        }
                        self.pos += 2;
                        self.token = SyntaxKind::ExclamationEqualsToken;
                        return self.token;
                    }
                    self.pos += 1;
                    self.token = SyntaxKind::ExclamationToken;
                    return self.token;
                }

                // Equals
                CharacterCodes::EQUALS => {
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::EQUALS) {
                        if self.char_code_at(self.pos + 2) == Some(CharacterCodes::EQUALS) {
                            self.pos += 3;
                            self.token = SyntaxKind::EqualsEqualsEqualsToken;
                            return self.token;
                        }
                        self.pos += 2;
                        self.token = SyntaxKind::EqualsEqualsToken;
                        return self.token;
                    }
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::GREATER_THAN) {
                        self.pos += 2;
                        self.token = SyntaxKind::EqualsGreaterThanToken;
                        return self.token;
                    }
                    self.pos += 1;
                    self.token = SyntaxKind::EqualsToken;
                    return self.token;
                }

                // Plus
                CharacterCodes::PLUS => {
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::PLUS) {
                        self.pos += 2;
                        self.token = SyntaxKind::PlusPlusToken;
                        return self.token;
                    }
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::EQUALS) {
                        self.pos += 2;
                        self.token = SyntaxKind::PlusEqualsToken;
                        return self.token;
                    }
                    self.pos += 1;
                    self.token = SyntaxKind::PlusToken;
                    return self.token;
                }

                // Minus
                CharacterCodes::MINUS => {
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::MINUS) {
                        self.pos += 2;
                        self.token = SyntaxKind::MinusMinusToken;
                        return self.token;
                    }
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::EQUALS) {
                        self.pos += 2;
                        self.token = SyntaxKind::MinusEqualsToken;
                        return self.token;
                    }
                    self.pos += 1;
                    self.token = SyntaxKind::MinusToken;
                    return self.token;
                }

                // Asterisk
                CharacterCodes::ASTERISK => {
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::ASTERISK) {
                        if self.char_code_at(self.pos + 2) == Some(CharacterCodes::EQUALS) {
                            self.pos += 3;
                            self.token = SyntaxKind::AsteriskAsteriskEqualsToken;
                            return self.token;
                        }
                        self.pos += 2;
                        self.token = SyntaxKind::AsteriskAsteriskToken;
                        return self.token;
                    }
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::EQUALS) {
                        self.pos += 2;
                        self.token = SyntaxKind::AsteriskEqualsToken;
                        return self.token;
                    }
                    self.pos += 1;
                    self.token = SyntaxKind::AsteriskToken;
                    return self.token;
                }

                // Percent
                CharacterCodes::PERCENT => {
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::EQUALS) {
                        self.pos += 2;
                        self.token = SyntaxKind::PercentEqualsToken;
                        return self.token;
                    }
                    self.pos += 1;
                    self.token = SyntaxKind::PercentToken;
                    return self.token;
                }

                // Ampersand
                CharacterCodes::AMPERSAND => {
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::AMPERSAND) {
                        if self.char_code_at(self.pos + 2) == Some(CharacterCodes::EQUALS) {
                            self.pos += 3;
                            self.token = SyntaxKind::AmpersandAmpersandEqualsToken;
                            return self.token;
                        }
                        self.pos += 2;
                        self.token = SyntaxKind::AmpersandAmpersandToken;
                        return self.token;
                    }
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::EQUALS) {
                        self.pos += 2;
                        self.token = SyntaxKind::AmpersandEqualsToken;
                        return self.token;
                    }
                    self.pos += 1;
                    self.token = SyntaxKind::AmpersandToken;
                    return self.token;
                }

                // Bar (pipe)
                CharacterCodes::BAR => {
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::BAR) {
                        if self.char_code_at(self.pos + 2) == Some(CharacterCodes::EQUALS) {
                            self.pos += 3;
                            self.token = SyntaxKind::BarBarEqualsToken;
                            return self.token;
                        }
                        self.pos += 2;
                        self.token = SyntaxKind::BarBarToken;
                        return self.token;
                    }
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::EQUALS) {
                        self.pos += 2;
                        self.token = SyntaxKind::BarEqualsToken;
                        return self.token;
                    }
                    self.pos += 1;
                    self.token = SyntaxKind::BarToken;
                    return self.token;
                }

                // Caret
                CharacterCodes::CARET => {
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::EQUALS) {
                        self.pos += 2;
                        self.token = SyntaxKind::CaretEqualsToken;
                        return self.token;
                    }
                    self.pos += 1;
                    self.token = SyntaxKind::CaretToken;
                    return self.token;
                }

                // Question mark
                CharacterCodes::QUESTION => {
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::DOT)
                        && !is_digit(self.char_code_at(self.pos + 2).unwrap_or(0))
                    {
                        self.pos += 2;
                        self.token = SyntaxKind::QuestionDotToken;
                        return self.token;
                    }
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::QUESTION) {
                        if self.char_code_at(self.pos + 2) == Some(CharacterCodes::EQUALS) {
                            self.pos += 3;
                            self.token = SyntaxKind::QuestionQuestionEqualsToken;
                            return self.token;
                        }
                        self.pos += 2;
                        self.token = SyntaxKind::QuestionQuestionToken;
                        return self.token;
                    }
                    self.pos += 1;
                    self.token = SyntaxKind::QuestionToken;
                    return self.token;
                }

                // Less than
                // Note: `</` (LessThanSlashToken) is only used in JSX mode.
                // In regular mode, `<` and `/` are separate tokens.
                CharacterCodes::LESS_THAN => {
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::LESS_THAN) {
                        if self.char_code_at(self.pos + 2) == Some(CharacterCodes::EQUALS) {
                            self.pos += 3;
                            self.token = SyntaxKind::LessThanLessThanEqualsToken;
                            return self.token;
                        }
                        self.pos += 2;
                        self.token = SyntaxKind::LessThanLessThanToken;
                        return self.token;
                    }
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::EQUALS) {
                        self.pos += 2;
                        self.token = SyntaxKind::LessThanEqualsToken;
                        return self.token;
                    }
                    // LessThanSlashToken is JSX-only, not returned in regular scanning
                    self.pos += 1;
                    self.token = SyntaxKind::LessThanToken;
                    return self.token;
                }

                // Greater than - only return GreaterThanToken
                // The parser calls reScanGreaterToken() to get >=, >>, >>>, >>=, >>>=
                CharacterCodes::GREATER_THAN => {
                    self.pos += 1;
                    self.token = SyntaxKind::GreaterThanToken;
                    return self.token;
                }

                // Slash - comment or division
                CharacterCodes::SLASH => {
                    // Check for comments
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::SLASH) {
                        self.pos += 2;
                        while self.pos < self.end {
                            let c = self.char_code_unchecked(self.pos);
                            if c == CharacterCodes::LINE_FEED
                                || c == CharacterCodes::CARRIAGE_RETURN
                            {
                                break;
                            }
                            self.pos += self.char_len_at(self.pos); // Handle multi-byte UTF-8
                        }
                        if self.skip_trivia {
                            continue;
                        } else {
                            self.token = SyntaxKind::SingleLineCommentTrivia;
                            return self.token;
                        }
                    }
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::ASTERISK) {
                        self.pos += 2;
                        let mut comment_closed = false;
                        while self.pos < self.end {
                            let c = self.char_code_unchecked(self.pos);
                            if c == CharacterCodes::ASTERISK
                                && self.char_code_at(self.pos + 1) == Some(CharacterCodes::SLASH)
                            {
                                self.pos += 2;
                                comment_closed = true;
                                break;
                            }
                            if c == CharacterCodes::LINE_FEED
                                || c == CharacterCodes::CARRIAGE_RETURN
                            {
                                self.token_flags |= TokenFlags::PrecedingLineBreak as u32;
                            }
                            self.pos += self.char_len_at(self.pos); // Handle multi-byte UTF-8
                        }
                        if !comment_closed {
                            self.token_flags |= TokenFlags::Unterminated as u32;
                        }
                        if self.skip_trivia {
                            continue;
                        } else {
                            self.token = SyntaxKind::MultiLineCommentTrivia;
                            return self.token;
                        }
                    }
                    if self.char_code_at(self.pos + 1) == Some(CharacterCodes::EQUALS) {
                        self.pos += 2;
                        self.token = SyntaxKind::SlashEqualsToken;
                        return self.token;
                    }
                    self.pos += 1;
                    self.token = SyntaxKind::SlashToken;
                    return self.token;
                }

                // String literals
                CharacterCodes::DOUBLE_QUOTE | CharacterCodes::SINGLE_QUOTE => {
                    self.scan_string(ch);
                    return self.token;
                }

                // Backtick (template literal)
                CharacterCodes::BACKTICK => {
                    self.scan_template_literal();
                    return self.token;
                }

                // Hash (private identifier)
                CharacterCodes::HASH => {
                    // Simplified: just treat as hash token
                    // Full implementation would check for private identifier
                    self.pos += 1;
                    if self.pos < self.end
                        && is_identifier_start(self.char_code_unchecked(self.pos))
                    {
                        self.pos += self.char_len_at(self.pos); // Handle multi-byte UTF-8
                        while self.pos < self.end
                            && is_identifier_part(self.char_code_unchecked(self.pos))
                        {
                            self.pos += self.char_len_at(self.pos); // Handle multi-byte UTF-8
                        }
                        self.token_value = self.substring(self.token_start, self.pos);
                        self.token = SyntaxKind::PrivateIdentifier;
                    } else {
                        self.token = SyntaxKind::HashToken;
                    }
                    return self.token;
                }

                // Numbers
                CharacterCodes::_0..=CharacterCodes::_9 => {
                    self.scan_number();
                    return self.token;
                }

                // Default: identifier or unknown
                _ => {
                    if is_identifier_start(ch) {
                        self.scan_identifier();
                        return self.token;
                    }
                    // Skip unknown character
                    self.pos += 1;
                    self.token = SyntaxKind::Unknown;
                    return self.token;
                }
            }
        }
    }

    /// Scan a string literal.
    fn scan_string(&mut self, quote: u32) {
        self.pos += 1; // Skip opening quote
        let mut result = String::new();

        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == quote {
                self.pos += 1; // Closing quote
                self.token_value = result;
                self.token = SyntaxKind::StringLiteral;
                return;
            }
            if ch == CharacterCodes::BACKSLASH {
                // Handle escape sequences
                self.pos += 1;
                if self.pos < self.end {
                    let escaped = self.char_code_unchecked(self.pos);
                    // Get byte length of the escaped character BEFORE advancing
                    let escaped_char_len = self.char_len_at(self.pos);
                    self.pos += escaped_char_len;
                    match escaped {
                        CharacterCodes::LOWER_N => result.push('\n'),
                        CharacterCodes::LOWER_R => result.push('\r'),
                        CharacterCodes::LOWER_T => result.push('\t'),
                        CharacterCodes::BACKSLASH => result.push('\\'),
                        c if c == quote => result.push(char::from_u32(quote).unwrap_or('\0')),
                        // Line continuation: backslash followed by line terminator
                        CharacterCodes::LINE_FEED
                        | CharacterCodes::CARRIAGE_RETURN
                        | CharacterCodes::LINE_SEPARATOR
                        | CharacterCodes::PARAGRAPH_SEPARATOR => {
                            // Line continuation - don't add anything to result
                            // Also handle CR+LF as a single line break
                            if escaped == CharacterCodes::CARRIAGE_RETURN
                                && self.pos < self.end
                                && self.char_code_unchecked(self.pos) == CharacterCodes::LINE_FEED
                            {
                                self.pos += 1;
                            }
                        }
                        _ => {
                            result.push('\\');
                            if let Some(c) = char::from_u32(escaped) {
                                result.push(c);
                            }
                        }
                    }
                }
            } else if ch == CharacterCodes::LINE_FEED || ch == CharacterCodes::CARRIAGE_RETURN {
                // Unterminated string
                self.token_flags |= TokenFlags::Unterminated as u32;
                self.token_value = result;
                self.token = SyntaxKind::StringLiteral;
                return;
            } else {
                if let Some(c) = char::from_u32(ch) {
                    result.push(c);
                }
                self.pos += self.char_len_at(self.pos); // Advance by character byte length
            }
        }

        // Unterminated string
        self.token_flags |= TokenFlags::Unterminated as u32;
        self.token_value = result;
        self.token = SyntaxKind::StringLiteral;
    }

    /// Scan a template literal (simplified).
    fn scan_template_literal(&mut self) {
        self.pos += 1; // Skip backtick
        let mut result = String::new();

        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::BACKTICK {
                self.pos += 1;
                self.token_value = result;
                self.token = SyntaxKind::NoSubstitutionTemplateLiteral;
                return;
            }
            if ch == CharacterCodes::DOLLAR
                && self.char_code_at(self.pos + 1) == Some(CharacterCodes::OPEN_BRACE)
            {
                self.pos += 2;
                self.token_value = result;
                self.token = SyntaxKind::TemplateHead;
                return;
            }
            if ch == CharacterCodes::BACKSLASH {
                self.pos += 1;
                if self.pos < self.end {
                    let escaped = self.char_code_unchecked(self.pos);
                    // Get byte length of the escaped character BEFORE advancing
                    let escaped_char_len = self.char_len_at(self.pos);
                    self.pos += escaped_char_len;
                    match escaped {
                        CharacterCodes::LOWER_N => result.push('\n'),
                        CharacterCodes::LOWER_R => result.push('\r'),
                        CharacterCodes::LOWER_T => result.push('\t'),
                        CharacterCodes::BACKTICK => result.push('`'),
                        CharacterCodes::DOLLAR => result.push('$'),
                        CharacterCodes::BACKSLASH => result.push('\\'),
                        // Line continuation: backslash followed by line terminator
                        CharacterCodes::LINE_FEED
                        | CharacterCodes::CARRIAGE_RETURN
                        | CharacterCodes::LINE_SEPARATOR
                        | CharacterCodes::PARAGRAPH_SEPARATOR => {
                            // Line continuation - don't add anything to result
                            // Also handle CR+LF as a single line break
                            if escaped == CharacterCodes::CARRIAGE_RETURN
                                && self.pos < self.end
                                && self.char_code_unchecked(self.pos) == CharacterCodes::LINE_FEED
                            {
                                self.pos += 1;
                            }
                        }
                        _ => {
                            result.push('\\');
                            if let Some(c) = char::from_u32(escaped) {
                                result.push(c);
                            }
                        }
                    }
                }
            } else {
                if ch == CharacterCodes::LINE_FEED || ch == CharacterCodes::CARRIAGE_RETURN {
                    self.token_flags |= TokenFlags::PrecedingLineBreak as u32;
                }
                if let Some(c) = char::from_u32(ch) {
                    result.push(c);
                }
                self.pos += self.char_len_at(self.pos); // Advance by character byte length
            }
        }

        self.token_flags |= TokenFlags::Unterminated as u32;
        self.token_value = result;
        self.token = SyntaxKind::NoSubstitutionTemplateLiteral;
    }

    /// Scan a number literal (simplified).
    fn scan_number(&mut self) {
        let start = self.pos;

        // Check for hex, octal, binary
        if self.char_code_unchecked(self.pos) == CharacterCodes::_0 {
            let next = self.char_code_at(self.pos + 1).unwrap_or(0);
            if next == CharacterCodes::LOWER_X || next == CharacterCodes::UPPER_X {
                // Hex number
                self.pos += 2;
                self.token_flags |= TokenFlags::HexSpecifier as u32;
                self.scan_digits_with_separators(is_hex_digit);
                if self.pos < self.end
                    && self.char_code_unchecked(self.pos) == CharacterCodes::LOWER_N
                {
                    self.pos += 1;
                    self.token_value = self.substring(start, self.pos);
                    self.token = SyntaxKind::BigIntLiteral;
                    return;
                }
                self.token_value = self.substring(start, self.pos);
                self.token = SyntaxKind::NumericLiteral;
                return;
            }
            if next == CharacterCodes::LOWER_B || next == CharacterCodes::UPPER_B {
                // Binary number
                self.pos += 2;
                self.token_flags |= TokenFlags::BinarySpecifier as u32;
                self.scan_digits_with_separators(is_binary_digit);
                if self.pos < self.end
                    && self.char_code_unchecked(self.pos) == CharacterCodes::LOWER_N
                {
                    self.pos += 1;
                    self.token_value = self.substring(start, self.pos);
                    self.token = SyntaxKind::BigIntLiteral;
                    return;
                }
                self.token_value = self.substring(start, self.pos);
                self.token = SyntaxKind::NumericLiteral;
                return;
            }
            if next == CharacterCodes::LOWER_O || next == CharacterCodes::UPPER_O {
                // Octal number
                self.pos += 2;
                self.token_flags |= TokenFlags::OctalSpecifier as u32;
                self.scan_digits_with_separators(is_octal_digit);
                if self.pos < self.end
                    && self.char_code_unchecked(self.pos) == CharacterCodes::LOWER_N
                {
                    self.pos += 1;
                    self.token_value = self.substring(start, self.pos);
                    self.token = SyntaxKind::BigIntLiteral;
                    return;
                }
                self.token_value = self.substring(start, self.pos);
                self.token = SyntaxKind::NumericLiteral;
                return;
            }
        }

        // Decimal number
        self.scan_digits_with_separators(is_digit);

        // Decimal point
        if self.pos < self.end && self.char_code_unchecked(self.pos) == CharacterCodes::DOT {
            self.pos += 1;
            self.scan_digits_with_separators(is_digit);
        }

        // Exponent
        if self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::LOWER_E || ch == CharacterCodes::UPPER_E {
                self.pos += 1;
                self.token_flags |= TokenFlags::Scientific as u32;
                if self.pos < self.end {
                    let sign = self.char_code_unchecked(self.pos);
                    if sign == CharacterCodes::PLUS || sign == CharacterCodes::MINUS {
                        self.pos += 1;
                    }
                }
                self.scan_digits_with_separators(is_digit);
            }
        }

        // BigInt suffix
        if self.pos < self.end && self.char_code_unchecked(self.pos) == CharacterCodes::LOWER_N {
            self.pos += 1;
            self.token_value = self.substring(start, self.pos);
            self.token = SyntaxKind::BigIntLiteral;
            return;
        }

        self.token_value = self.substring(start, self.pos);
        self.token = SyntaxKind::NumericLiteral;
    }

    fn scan_digits_with_separators(&mut self, is_valid_digit: fn(u32) -> bool) {
        let mut saw_digit = false;
        let mut prev_separator = false;

        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::UNDERSCORE {
                self.token_flags |= TokenFlags::ContainsSeparator as u32;
                if !saw_digit || prev_separator {
                    self.token_flags |= TokenFlags::ContainsInvalidSeparator as u32;
                    if self.token_invalid_separator_pos.is_none() {
                        self.token_invalid_separator_pos = Some(self.pos);
                        self.token_invalid_separator_is_consecutive = prev_separator;
                    }
                }
                prev_separator = true;
                self.pos += 1;
                continue;
            }
            if is_valid_digit(ch) {
                saw_digit = true;
                prev_separator = false;
                self.pos += 1;
                continue;
            }
            break;
        }

        if prev_separator {
            self.token_flags |= TokenFlags::ContainsInvalidSeparator as u32;
            if self.token_invalid_separator_pos.is_none() {
                self.token_invalid_separator_pos = Some(self.pos.saturating_sub(1));
                self.token_invalid_separator_is_consecutive = false;
            }
        }
    }

    /// Scan an identifier.
    /// ZERO-ALLOCATION: Identifiers are interned, returning an Atom (u32) for O(1) comparison.
    fn scan_identifier(&mut self) {
        let start = self.pos;
        // Advance past first character (may be multi-byte)
        self.pos += self.char_len_at(self.pos);

        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if !is_identifier_part(ch) {
                break;
            }
            self.pos += self.char_len_at(self.pos); // Handle multi-byte UTF-8
        }

        // Get slice reference instead of allocating new String
        let text_slice = &self.source[start..self.pos];

        // Check if it's a keyword first (common keywords are pre-interned)
        self.token = crate::scanner::text_to_keyword(text_slice).unwrap_or(SyntaxKind::Identifier);

        // Intern the identifier for O(1) comparison (reuses existing interned string)
        self.token_atom = self.interner.intern(text_slice);

        // Store token value (still needed for compatibility, but could be lazy in future)
        self.token_value = text_slice.to_string();
    }

    // =========================================================================
    // Rescan methods - for context-sensitive parsing
    // =========================================================================

    /// Re-scan the current `>` token to see if it should be `>=`, `>>`, `>>>`, `>>=`, or `>>>=`.
    /// This is used by the parser for type arguments and bitwise operators.
    #[wasm_bindgen(js_name = reScanGreaterToken)]
    pub fn re_scan_greater_token(&mut self) -> SyntaxKind {
        if self.token == SyntaxKind::GreaterThanToken {
            let next_char = self.char_code_unchecked(self.pos);
            if next_char == CharacterCodes::GREATER_THAN {
                let next_next = self.char_code_unchecked(self.pos + 1);
                if next_next == CharacterCodes::GREATER_THAN {
                    // >>>
                    let next_next_next = self.char_code_unchecked(self.pos + 2);
                    if next_next_next == CharacterCodes::EQUALS {
                        // >>>=
                        self.pos += 3;
                        self.token = SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken;
                        return self.token;
                    }
                    self.pos += 2;
                    self.token = SyntaxKind::GreaterThanGreaterThanGreaterThanToken;
                    return self.token;
                }
                if next_next == CharacterCodes::EQUALS {
                    // >>=
                    self.pos += 2;
                    self.token = SyntaxKind::GreaterThanGreaterThanEqualsToken;
                    return self.token;
                }
                // >>
                self.pos += 1;
                self.token = SyntaxKind::GreaterThanGreaterThanToken;
                return self.token;
            }
            if next_char == CharacterCodes::EQUALS {
                // >=
                self.pos += 1;
                self.token = SyntaxKind::GreaterThanEqualsToken;
                return self.token;
            }
        }
        self.token
    }

    /// Re-scan the current `/` or `/=` token as a regex literal.
    /// This is used by the parser when it determines the context requires a regex.
    #[wasm_bindgen(js_name = reScanSlashToken)]
    pub fn re_scan_slash_token(&mut self) -> SyntaxKind {
        if self.token == SyntaxKind::SlashToken || self.token == SyntaxKind::SlashEqualsToken {
            // Start scanning from after the initial /
            let start_of_regex_body = self.token_start + 1;
            self.pos = start_of_regex_body;
            let mut in_escape = false;
            let mut in_character_class = false;

            // Scan until we find the closing /
            while self.pos < self.end {
                let ch = self.char_code_unchecked(self.pos);

                // Unterminated regex if we hit a newline
                if is_line_break(ch) {
                    self.token_flags |= TokenFlags::Unterminated as u32;
                    break;
                }

                if in_escape {
                    // After backslash, just consume the next character
                    in_escape = false;
                } else if ch == CharacterCodes::SLASH && !in_character_class {
                    // Found the closing /
                    break;
                } else if ch == CharacterCodes::OPEN_BRACKET {
                    in_character_class = true;
                } else if ch == CharacterCodes::BACKSLASH {
                    in_escape = true;
                } else if ch == CharacterCodes::CLOSE_BRACKET {
                    in_character_class = false;
                }
                // Use char_len_at to properly advance past multi-byte UTF-8 characters
                self.pos += self.char_len_at(self.pos);
            }

            if (self.token_flags & TokenFlags::Unterminated as u32) == 0 {
                // Consume the closing /
                self.pos += 1;

                // Scan regex flags (g, i, m, s, u, v, y, d)
                // Also handles non-ASCII characters that may appear as invalid flags
                while self.pos < self.end {
                    let ch = self.char_code_unchecked(self.pos);
                    if !is_regex_flag(ch) && !is_identifier_part(ch) {
                        break;
                    }
                    // Use char_len_at for proper UTF-8 handling (handles non-ASCII flags)
                    self.pos += self.char_len_at(self.pos);
                }
            }

            self.token_value = self.substring(self.token_start, self.pos);
            self.token = SyntaxKind::RegularExpressionLiteral;
        }
        self.token
    }

    /// Re-scan the current `*=` token as `*` followed by `=`.
    /// Used when parsing computed property names.
    #[wasm_bindgen(js_name = reScanAsteriskEqualsToken)]
    pub fn re_scan_asterisk_equals_token(&mut self) -> SyntaxKind {
        if self.token == SyntaxKind::AsteriskEqualsToken {
            self.pos = self.token_start + 1;
            self.token = SyntaxKind::EqualsToken;
        }
        self.token
    }

    /// Re-scan the current `}` token as the continuation of a template literal.
    /// Called by the parser when it determines that a `}` is closing a template expression.
    ///
    /// # Arguments
    /// * `is_tagged_template` - If true, invalid escape sequences should not report errors
    ///   (tagged templates can have invalid escapes that get passed to the tag function as raw).
    ///   For now, we don't report errors anyway, so this parameter affects nothing.
    #[wasm_bindgen(js_name = reScanTemplateToken)]
    pub fn re_scan_template_token(&mut self, _is_tagged_template: bool) -> SyntaxKind {
        // Reset position to token start and scan the template continuation
        // Make sure token_start is within bounds
        if self.token_start >= self.end {
            self.token = SyntaxKind::EndOfFileToken;
            return self.token;
        }
        self.pos = self.token_start;
        self.token = self.scan_template_and_set_token_value(false);
        self.token
    }

    /// Re-scan template head or no-substitution template.
    /// Used when the parser needs to rescan the start of a template.
    #[wasm_bindgen(js_name = reScanTemplateHeadOrNoSubstitutionTemplate)]
    pub fn re_scan_template_head_or_no_substitution_template(&mut self) -> SyntaxKind {
        self.pos = self.token_start;
        self.token = self.scan_template_and_set_token_value(true);
        self.token
    }

    /// Internal helper to scan a template literal part and set the token value.
    ///
    /// # Arguments
    /// * `started_with_backtick` - true if this is the start of a template (head or no-substitution),
    ///   false if this is a continuation after a `}` (middle or tail).
    fn scan_template_and_set_token_value(&mut self, started_with_backtick: bool) -> SyntaxKind {
        // Move past the opening character (backtick for head, } for middle/tail)
        // Safety check: ensure we don't move past the end
        if self.pos >= self.end {
            self.token_flags |= TokenFlags::Unterminated as u32;
            self.token_value = String::new();
            return if started_with_backtick {
                SyntaxKind::NoSubstitutionTemplateLiteral
            } else {
                SyntaxKind::TemplateTail
            };
        }
        self.pos += 1;
        let mut start = self.pos;
        let mut contents = String::new();

        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);

            // End of template: backtick
            if ch == CharacterCodes::BACKTICK {
                contents.push_str(&self.substring(start, self.pos));
                self.pos += 1;
                self.token_value = contents;
                return if started_with_backtick {
                    SyntaxKind::NoSubstitutionTemplateLiteral
                } else {
                    SyntaxKind::TemplateTail
                };
            }

            // Template expression: ${
            if ch == CharacterCodes::DOLLAR
                && self.pos + 1 < self.end
                && self.char_code_unchecked(self.pos + 1) == CharacterCodes::OPEN_BRACE
            {
                contents.push_str(&self.substring(start, self.pos));
                self.pos += 2;
                self.token_value = contents;
                return if started_with_backtick {
                    SyntaxKind::TemplateHead
                } else {
                    SyntaxKind::TemplateMiddle
                };
            }

            // Escape sequence
            if ch == CharacterCodes::BACKSLASH {
                contents.push_str(&self.substring(start, self.pos));
                let escaped = self.scan_template_escape_sequence();
                contents.push_str(&escaped);
                // Reset start to current position after the escape
                start = self.pos;
                continue;
            }

            // CR normalization (CR or CRLF -> LF)
            if ch == CharacterCodes::CARRIAGE_RETURN {
                contents.push_str(&self.substring(start, self.pos));
                self.pos += 1;
                if self.pos < self.end
                    && self.char_code_unchecked(self.pos) == CharacterCodes::LINE_FEED
                {
                    self.pos += 1;
                }
                contents.push('\n');
                // Reset start to current position after the CR
                start = self.pos;
                continue;
            }

            self.pos += 1;
        }

        // Unterminated template
        contents.push_str(&self.substring(start, self.pos));
        self.token_flags |= TokenFlags::Unterminated as u32;
        self.token_value = contents;
        if started_with_backtick {
            SyntaxKind::NoSubstitutionTemplateLiteral
        } else {
            SyntaxKind::TemplateTail
        }
    }

    /// Scan an escape sequence in a template literal.
    /// Returns the resulting string and advances self.pos.
    fn scan_template_escape_sequence(&mut self) -> String {
        // Skip the backslash
        self.pos += 1;

        if self.pos >= self.end {
            return String::from("\\");
        }

        let ch = self.char_code_unchecked(self.pos);
        // Use char_len_at for proper UTF-8 handling of multi-byte chars
        let ch_len = self.char_len_at(self.pos);
        self.pos += ch_len;

        match ch {
            CharacterCodes::_0 => {
                // Check if it's followed by a digit (octal)
                if self.pos < self.end && is_digit(self.char_code_unchecked(self.pos)) {
                    // Just return the literal for now (octal in template is complex)
                    String::from("\0")
                } else {
                    String::from("\0")
                }
            }
            CharacterCodes::LOWER_N => String::from("\n"),
            CharacterCodes::LOWER_R => String::from("\r"),
            CharacterCodes::LOWER_T => String::from("\t"),
            CharacterCodes::LOWER_V => String::from("\x0B"),
            CharacterCodes::LOWER_B => String::from("\x08"),
            CharacterCodes::LOWER_F => String::from("\x0C"),
            CharacterCodes::SINGLE_QUOTE => String::from("'"),
            CharacterCodes::DOUBLE_QUOTE => String::from("\""),
            CharacterCodes::BACKTICK => String::from("`"),
            CharacterCodes::BACKSLASH => String::from("\\"),
            CharacterCodes::DOLLAR => String::from("$"),
            CharacterCodes::LINE_FEED
            | CharacterCodes::LINE_SEPARATOR
            | CharacterCodes::PARAGRAPH_SEPARATOR => String::new(), // Line continuation
            CharacterCodes::CARRIAGE_RETURN => {
                // Skip following LF if present
                if self.pos < self.end
                    && self.char_code_unchecked(self.pos) == CharacterCodes::LINE_FEED
                {
                    self.pos += 1;
                }
                String::new() // Line continuation
            }
            CharacterCodes::LOWER_X => {
                // Hex escape \xHH
                if self.pos + 2 <= self.end {
                    let hex = self.substring(self.pos, self.pos + 2);
                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                        self.pos += 2;
                        if let Some(c) = char::from_u32(code) {
                            return c.to_string();
                        }
                    }
                }
                self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
                "\\x".to_string()
            }
            CharacterCodes::LOWER_U => {
                // Unicode escape \uHHHH or \u{H+}
                if self.pos < self.end
                    && self.char_code_unchecked(self.pos) == CharacterCodes::OPEN_BRACE
                {
                    // \u{...}
                    self.pos += 1;
                    let hex_start = self.pos;
                    while self.pos < self.end && is_hex_digit(self.char_code_unchecked(self.pos)) {
                        self.pos += 1;
                    }
                    if self.pos < self.end
                        && self.char_code_unchecked(self.pos) == CharacterCodes::CLOSE_BRACE
                    {
                        let hex = self.substring(hex_start, self.pos);
                        self.pos += 1;
                        if let Ok(code) = u32::from_str_radix(&hex, 16)
                            && let Some(c) = char::from_u32(code)
                        {
                            return c.to_string();
                        }
                    }
                    self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
                    String::from("\\u")
                } else if self.pos + 4 <= self.end {
                    // \uHHHH
                    let hex = self.substring(self.pos, self.pos + 4);
                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                        self.pos += 4;
                        if let Some(c) = char::from_u32(code) {
                            return c.to_string();
                        }
                    }
                    self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
                    String::from("\\u")
                } else {
                    self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
                    String::from("\\u")
                }
            }
            _ => {
                // Unknown escape - just return the character
                if let Some(c) = char::from_u32(ch) {
                    c.to_string()
                } else {
                    String::new()
                }
            }
        }
    }

    // =========================================================================
    // JSX Scanning Methods
    // =========================================================================

    /// Scan a JSX identifier.
    /// In JSX, identifiers can contain hyphens (like `data-testid`).
    #[wasm_bindgen(js_name = scanJsxIdentifier)]
    pub fn scan_jsx_identifier(&mut self) -> SyntaxKind {
        if self.token == SyntaxKind::Identifier {
            // Continue scanning to include any hyphenated parts
            // JSX identifiers can be like: foo-bar-baz
            while self.pos < self.end {
                let ch = self.char_code_unchecked(self.pos);
                if ch == CharacterCodes::MINUS {
                    // In JSX, hyphens are allowed in identifiers
                    self.pos += 1;
                    // After hyphen, we need more identifier characters
                    if self.pos < self.end
                        && is_identifier_start(self.char_code_unchecked(self.pos))
                    {
                        self.pos += self.char_len_at(self.pos); // Handle multi-byte UTF-8
                        while self.pos < self.end
                            && is_identifier_part(self.char_code_unchecked(self.pos))
                        {
                            self.pos += self.char_len_at(self.pos); // Handle multi-byte UTF-8
                        }
                    }
                } else {
                    break;
                }
            }
            self.token_value = self.substring(self.token_start, self.pos);
            self.token_atom = self.interner.intern(self.token_value.as_str());
        }
        self.token
    }

    /// Re-scan the current token as a JSX token.
    /// Used when the parser enters JSX context and needs to rescan.
    #[wasm_bindgen(js_name = reScanJsxToken)]
    pub fn re_scan_jsx_token(&mut self, allow_multiline_jsx_text: bool) -> SyntaxKind {
        self.pos = self.token_start;
        self.scan_jsx_token(allow_multiline_jsx_text)
    }

    /// Scan a JSX token (text, open element, close element, etc.)
    fn scan_jsx_token(&mut self, allow_multiline_jsx_text: bool) -> SyntaxKind {
        self.full_start_pos = self.pos;
        self.token_start = self.pos;

        if self.pos >= self.end {
            self.token = SyntaxKind::EndOfFileToken;
            return self.token;
        }

        let ch = self.char_code_unchecked(self.pos);

        // Check for JSX opening/closing angle brackets
        if ch == CharacterCodes::LESS_THAN {
            // Check for </
            if self.char_code_at(self.pos + 1) == Some(CharacterCodes::SLASH) {
                self.pos += 2;
                self.token = SyntaxKind::LessThanSlashToken;
                return self.token;
            }
            self.pos += 1;
            self.token = SyntaxKind::LessThanToken;
            return self.token;
        }

        if ch == CharacterCodes::OPEN_BRACE {
            self.pos += 1;
            self.token = SyntaxKind::OpenBraceToken;
            return self.token;
        }

        // Scan JSX text
        let mut text = String::new();
        while self.pos < self.end {
            let c = self.char_code_unchecked(self.pos);

            // Stop on JSX special characters
            if c == CharacterCodes::OPEN_BRACE || c == CharacterCodes::LESS_THAN {
                break;
            }

            // Handle newlines in JSX text
            if is_line_break(c) {
                if !allow_multiline_jsx_text {
                    break;
                }
                self.token_flags |= TokenFlags::PrecedingLineBreak as u32;
            }

            if let Some(char) = char::from_u32(c) {
                text.push(char);
            }
            self.pos += self.char_len_at(self.pos); // Handle multi-byte UTF-8
        }

        if !text.is_empty() {
            self.token_value = text;
            self.token = SyntaxKind::JsxText;
            return self.token;
        }

        self.token = SyntaxKind::Unknown;
        self.token
    }

    /// Scan a JSX attribute value (string literal or expression).
    #[wasm_bindgen(js_name = scanJsxAttributeValue)]
    pub fn scan_jsx_attribute_value(&mut self) -> SyntaxKind {
        self.full_start_pos = self.pos;

        // Skip whitespace
        while self.pos < self.end && is_white_space_single_line(self.char_code_unchecked(self.pos))
        {
            self.pos += 1;
        }

        self.token_start = self.pos;

        if self.pos >= self.end {
            self.token = SyntaxKind::EndOfFileToken;
            return self.token;
        }

        let ch = self.char_code_unchecked(self.pos);

        // String literal
        if ch == CharacterCodes::DOUBLE_QUOTE || ch == CharacterCodes::SINGLE_QUOTE {
            self.scan_jsx_string_literal(ch);
            return self.token;
        }

        // Expression container
        if ch == CharacterCodes::OPEN_BRACE {
            self.pos += 1;
            self.token = SyntaxKind::OpenBraceToken;
            return self.token;
        }

        self.token = SyntaxKind::Unknown;
        self.token
    }

    /// Scan a JSX string literal (used for attribute values).
    /// Unlike regular strings, JSX strings don't support escape sequences.
    fn scan_jsx_string_literal(&mut self, quote: u32) {
        self.pos += 1; // Skip opening quote
        let mut result = String::new();

        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == quote {
                self.pos += 1; // Closing quote
                self.token_value = result;
                self.token = SyntaxKind::StringLiteral;
                return;
            }
            // JSX strings don't process escape sequences - they're literal
            if let Some(c) = char::from_u32(ch) {
                result.push(c);
            }
            self.pos += 1;
        }

        // Unterminated string
        self.token_flags |= TokenFlags::Unterminated as u32;
        self.token_value = result;
        self.token = SyntaxKind::StringLiteral;
    }

    /// Re-scan a JSX attribute value from the current token position.
    #[wasm_bindgen(js_name = reScanJsxAttributeValue)]
    pub fn re_scan_jsx_attribute_value(&mut self) -> SyntaxKind {
        self.pos = self.token_start;
        self.scan_jsx_attribute_value()
    }

    /// Re-scan a `<` token in JSX context.
    /// Returns LessThanSlashToken if followed by `/`, otherwise LessThanToken.
    #[wasm_bindgen(js_name = reScanLessThanToken)]
    pub fn re_scan_less_than_token(&mut self) -> SyntaxKind {
        if self.token == SyntaxKind::LessThanToken
            && self.pos < self.end
            && self.char_code_unchecked(self.pos) == CharacterCodes::SLASH
        {
            self.pos += 1;
            self.token = SyntaxKind::LessThanSlashToken;
        }
        self.token
    }

    /// Re-scan the current `#` token as a hash token or private identifier.
    #[wasm_bindgen(js_name = reScanHashToken)]
    pub fn re_scan_hash_token(&mut self) -> SyntaxKind {
        if self.token == SyntaxKind::HashToken
            && self.pos < self.end
            && is_identifier_start(self.char_code_unchecked(self.pos))
        {
            self.pos += 1;
            while self.pos < self.end && is_identifier_part(self.char_code_unchecked(self.pos)) {
                self.pos += 1;
            }
            self.token_value = self.substring(self.token_start, self.pos);
            self.token = SyntaxKind::PrivateIdentifier;
        }
        self.token
    }

    /// Re-scan the current `?` token for optional chaining.
    #[wasm_bindgen(js_name = reScanQuestionToken)]
    pub fn re_scan_question_token(&mut self) -> SyntaxKind {
        if self.token == SyntaxKind::QuestionToken {
            let ch = self.char_code_at(self.pos);
            if ch == Some(CharacterCodes::DOT) {
                // Check it's not ?. followed by a digit
                let next = self.char_code_at(self.pos + 1);
                if !next.map(is_digit).unwrap_or(false) {
                    self.pos += 1;
                    self.token = SyntaxKind::QuestionDotToken;
                }
            } else if ch == Some(CharacterCodes::QUESTION) {
                if self.char_code_at(self.pos + 1) == Some(CharacterCodes::EQUALS) {
                    self.pos += 2;
                    self.token = SyntaxKind::QuestionQuestionEqualsToken;
                } else {
                    self.pos += 1;
                    self.token = SyntaxKind::QuestionQuestionToken;
                }
            }
        }
        self.token
    }

    // =========================================================================
    // JSDoc Scanning Methods
    // =========================================================================

    /// Scan a JSDoc token.
    /// Used when parsing JSDoc comments.
    #[wasm_bindgen(js_name = scanJsDocToken)]
    pub fn scan_jsdoc_token(&mut self) -> SyntaxKind {
        self.full_start_pos = self.pos;
        self.token_flags = 0;

        if self.pos >= self.end {
            self.token = SyntaxKind::EndOfFileToken;
            return self.token;
        }

        self.token_start = self.pos;
        let ch = self.char_code_unchecked(self.pos);

        // Handle newlines
        if ch == CharacterCodes::LINE_FEED || ch == CharacterCodes::CARRIAGE_RETURN {
            self.token_flags |= TokenFlags::PrecedingLineBreak as u32;
            self.pos += 1;
            if ch == CharacterCodes::CARRIAGE_RETURN
                && self.pos < self.end
                && self.char_code_unchecked(self.pos) == CharacterCodes::LINE_FEED
            {
                self.pos += 1;
            }
            self.token = SyntaxKind::NewLineTrivia;
            return self.token;
        }

        // Handle whitespace
        if is_white_space_single_line(ch) {
            while self.pos < self.end
                && is_white_space_single_line(self.char_code_unchecked(self.pos))
            {
                self.pos += 1;
            }
            self.token = SyntaxKind::WhitespaceTrivia;
            return self.token;
        }

        // JSDoc special tokens
        match ch {
            CharacterCodes::AT => {
                self.pos += 1;
                self.token = SyntaxKind::AtToken;
                return self.token;
            }
            CharacterCodes::ASTERISK => {
                self.pos += 1;
                self.token = SyntaxKind::AsteriskToken;
                return self.token;
            }
            CharacterCodes::OPEN_BRACE => {
                self.pos += 1;
                self.token = SyntaxKind::OpenBraceToken;
                return self.token;
            }
            CharacterCodes::CLOSE_BRACE => {
                self.pos += 1;
                self.token = SyntaxKind::CloseBraceToken;
                return self.token;
            }
            CharacterCodes::OPEN_BRACKET => {
                self.pos += 1;
                self.token = SyntaxKind::OpenBracketToken;
                return self.token;
            }
            CharacterCodes::CLOSE_BRACKET => {
                self.pos += 1;
                self.token = SyntaxKind::CloseBracketToken;
                return self.token;
            }
            CharacterCodes::LESS_THAN => {
                self.pos += 1;
                self.token = SyntaxKind::LessThanToken;
                return self.token;
            }
            CharacterCodes::GREATER_THAN => {
                self.pos += 1;
                self.token = SyntaxKind::GreaterThanToken;
                return self.token;
            }
            CharacterCodes::EQUALS => {
                self.pos += 1;
                self.token = SyntaxKind::EqualsToken;
                return self.token;
            }
            CharacterCodes::COMMA => {
                self.pos += 1;
                self.token = SyntaxKind::CommaToken;
                return self.token;
            }
            CharacterCodes::DOT => {
                self.pos += 1;
                self.token = SyntaxKind::DotToken;
                return self.token;
            }
            CharacterCodes::BACKTICK => {
                // Scan backtick-quoted string in JSDoc
                self.pos += 1;
                while self.pos < self.end
                    && self.char_code_unchecked(self.pos) != CharacterCodes::BACKTICK
                {
                    self.pos += 1;
                }
                if self.pos < self.end {
                    self.pos += 1; // consume closing backtick
                }
                self.token_value = self.substring(self.token_start, self.pos);
                self.token = SyntaxKind::NoSubstitutionTemplateLiteral;
                return self.token;
            }
            _ => {}
        }

        // Check for identifier
        if is_identifier_start(ch) {
            self.pos += 1;
            while self.pos < self.end && is_identifier_part(self.char_code_unchecked(self.pos)) {
                self.pos += 1;
            }
            self.token_value = self.substring(self.token_start, self.pos);
            self.token = crate::scanner::text_to_keyword(&self.token_value)
                .unwrap_or(SyntaxKind::Identifier);
            return self.token;
        }

        // Unknown character - advance and return Unknown
        self.pos += 1;
        self.token = SyntaxKind::Unknown;
        self.token
    }

    /// Scan JSDoc comment text token.
    /// Used for scanning the text content within JSDoc comments.
    #[wasm_bindgen(js_name = scanJsDocCommentTextToken)]
    pub fn scan_jsdoc_comment_text_token(&mut self, in_backticks: bool) -> SyntaxKind {
        self.full_start_pos = self.pos;
        self.token_flags = 0;
        self.token_start = self.pos;

        if self.pos >= self.end {
            self.token = SyntaxKind::EndOfFileToken;
            return self.token;
        }

        // Scan until we hit a special character
        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);

            // Check for end markers
            match ch {
                // Always stop on newline
                CharacterCodes::LINE_FEED | CharacterCodes::CARRIAGE_RETURN => {
                    break;
                }
                // Stop on @ unless in backticks
                CharacterCodes::AT if !in_backticks => {
                    break;
                }
                // Stop on backtick - it toggles the mode
                CharacterCodes::BACKTICK => {
                    if self.pos > self.token_start {
                        break; // Return text first
                    }
                    // Return the backtick token
                    self.pos += 1;
                    self.token = SyntaxKind::Unknown; // Use Unknown to signal backtick
                    return self.token;
                }
                // Stop on { and } for type expressions
                CharacterCodes::OPEN_BRACE | CharacterCodes::CLOSE_BRACE if !in_backticks => {
                    break;
                }
                _ => {
                    self.pos += 1;
                }
            }
        }

        if self.pos > self.token_start {
            self.token_value = self.substring(self.token_start, self.pos);
            // JSDocText token would be returned here, but we use Identifier as a stand-in
            self.token = SyntaxKind::Identifier;
        } else {
            self.token = SyntaxKind::EndOfFileToken;
        }
        self.token
    }

    // =========================================================================
    // Shebang Handling
    // =========================================================================

    /// Scan a shebang (#!) at the start of the file.
    /// Returns the length of the shebang line (including newline), or 0 if no shebang.
    #[wasm_bindgen(js_name = scanShebangTrivia)]
    pub fn scan_shebang_trivia(&mut self) -> usize {
        // Shebang must be at the very start of the file
        if self.pos != 0 {
            return 0;
        }

        // Check for #!
        if self.pos + 1 < self.end
            && self.char_code_unchecked(self.pos) == CharacterCodes::HASH
            && self.char_code_unchecked(self.pos + 1) == CharacterCodes::EXCLAMATION
        {
            let start = self.pos;
            self.pos += 2;

            // Scan to end of line
            while self.pos < self.end {
                let ch = self.char_code_unchecked(self.pos);
                if ch == CharacterCodes::LINE_FEED || ch == CharacterCodes::CARRIAGE_RETURN {
                    break;
                }
                self.pos += 1;
            }

            // Include the newline in the shebang
            if self.pos < self.end {
                let ch = self.char_code_unchecked(self.pos);
                if ch == CharacterCodes::CARRIAGE_RETURN {
                    self.pos += 1;
                    if self.pos < self.end
                        && self.char_code_unchecked(self.pos) == CharacterCodes::LINE_FEED
                    {
                        self.pos += 1;
                    }
                } else if ch == CharacterCodes::LINE_FEED {
                    self.pos += 1;
                }
            }

            return self.pos - start;
        }

        0
    }

    /// Re-scan an invalid identifier to check if it's valid in a specific context.
    #[wasm_bindgen(js_name = reScanInvalidIdentifier)]
    pub fn re_scan_invalid_identifier(&mut self) -> SyntaxKind {
        // This method is used when the parser encounters an invalid identifier
        // and wants to check if it could be valid in certain contexts (like keywords)
        if self.token == SyntaxKind::Unknown && !self.token_value.is_empty() {
            // Check if the token value is a valid identifier
            let chars: Vec<char> = self.token_value.chars().collect();
            if !chars.is_empty() && is_identifier_start(chars[0] as u32) {
                let mut all_valid = true;
                for c in chars.iter().skip(1) {
                    if !is_identifier_part(*c as u32) {
                        all_valid = false;
                        break;
                    }
                }
                if all_valid {
                    self.token = crate::scanner::text_to_keyword(&self.token_value)
                        .unwrap_or(SyntaxKind::Identifier);
                }
            }
        }
        self.token
    }
}

// =============================================================================
// Non-wasm methods for internal use
// =============================================================================

impl ScannerState {
    /// Save the current scanner state for look-ahead.
    pub fn save_state(&self) -> ScannerSnapshot {
        ScannerSnapshot {
            pos: self.pos,
            full_start_pos: self.full_start_pos,
            token_start: self.token_start,
            token: self.token,
            token_value: self.token_value.clone(),
            token_flags: self.token_flags,
            token_atom: self.token_atom,
            token_invalid_separator_pos: self.token_invalid_separator_pos,
            token_invalid_separator_is_consecutive: self.token_invalid_separator_is_consecutive,
        }
    }

    /// Restore a saved scanner state.
    pub fn restore_state(&mut self, snapshot: ScannerSnapshot) {
        self.pos = snapshot.pos;
        self.full_start_pos = snapshot.full_start_pos;
        self.token_start = snapshot.token_start;
        self.token = snapshot.token;
        self.token_value = snapshot.token_value;
        self.token_flags = snapshot.token_flags;
        self.token_atom = snapshot.token_atom;
        self.token_invalid_separator_pos = snapshot.token_invalid_separator_pos;
        self.token_invalid_separator_is_consecutive =
            snapshot.token_invalid_separator_is_consecutive;
    }

    /// Get the interned atom for the current identifier token.
    /// Returns Atom::NONE if the current token is not an identifier.
    /// This enables O(1) string comparison for identifiers.
    pub fn get_token_atom(&self) -> Atom {
        self.token_atom
    }

    pub fn get_invalid_separator_pos(&self) -> Option<usize> {
        self.token_invalid_separator_pos
    }

    pub fn invalid_separator_is_consecutive(&self) -> bool {
        self.token_invalid_separator_is_consecutive
    }

    /// Resolve an atom back to its string value.
    /// Panics if the atom is invalid.
    pub fn resolve_atom(&self, atom: Atom) -> &str {
        self.interner.resolve(atom)
    }

    /// Get a reference to the interner for direct use by the parser.
    pub fn interner(&self) -> &Interner {
        &self.interner
    }

    /// Get a mutable reference to the interner.
    pub fn interner_mut(&mut self) -> &mut Interner {
        &mut self.interner
    }

    /// ZERO-COPY: Get the current token value as a reference.
    /// For identifiers/keywords, returns the interned string.
    /// For other tokens, returns a slice of the source text.
    /// This avoids allocation compared to get_token_value().
    #[inline]
    pub fn get_token_value_ref(&self) -> &str {
        // For identifiers with an interned atom, return the interned string
        if self.token_atom != Atom::NONE {
            return self.interner.resolve(self.token_atom);
        }
        // For other tokens, return the source slice
        // Note: This won't work for tokens with escape processing,
        // which is why we still keep token_value for now
        &self.token_value
    }

    /// ZERO-COPY: Get the raw token text directly from source.
    /// This is the unprocessed text from token_start to current pos.
    #[inline]
    pub fn get_token_text_ref(&self) -> &str {
        &self.source[self.token_start..self.pos]
    }

    /// ZERO-COPY: Get a slice of the source text by positions.
    #[inline]
    pub fn source_slice(&self, start: usize, end: usize) -> &str {
        &self.source[start..end]
    }

    /// Get the source text reference.
    #[inline]
    pub fn source_text(&self) -> &str {
        &self.source
    }
}

impl ScannerState {
    /// Get a cloned handle to the shared source text.
    #[inline]
    pub fn source_text_arc(&self) -> Arc<str> {
        self.source.clone()
    }
}

// =============================================================================
// Helper functions
// =============================================================================

fn is_white_space_single_line(ch: u32) -> bool {
    ch == CharacterCodes::SPACE
        || ch == CharacterCodes::TAB
        || ch == CharacterCodes::VERTICAL_TAB
        || ch == CharacterCodes::FORM_FEED
        || ch == CharacterCodes::NON_BREAKING_SPACE
        || ch == CharacterCodes::OGHAM
        || (CharacterCodes::EN_QUAD..=CharacterCodes::ZERO_WIDTH_SPACE).contains(&ch)
        || ch == CharacterCodes::NARROW_NO_BREAK_SPACE
        || ch == CharacterCodes::MATHEMATICAL_SPACE
        || ch == CharacterCodes::IDEOGRAPHIC_SPACE
        || ch == CharacterCodes::BYTE_ORDER_MARK
}

fn is_digit(ch: u32) -> bool {
    (CharacterCodes::_0..=CharacterCodes::_9).contains(&ch)
}

fn is_binary_digit(ch: u32) -> bool {
    ch == CharacterCodes::_0 || ch == CharacterCodes::_1
}

fn is_octal_digit(ch: u32) -> bool {
    (CharacterCodes::_0..=CharacterCodes::_7).contains(&ch)
}

fn is_hex_digit(ch: u32) -> bool {
    is_digit(ch)
        || (CharacterCodes::UPPER_A..=CharacterCodes::UPPER_F).contains(&ch)
        || (CharacterCodes::LOWER_A..=CharacterCodes::LOWER_F).contains(&ch)
}

fn is_identifier_start(ch: u32) -> bool {
    (CharacterCodes::UPPER_A..=CharacterCodes::UPPER_Z).contains(&ch)
        || (CharacterCodes::LOWER_A..=CharacterCodes::LOWER_Z).contains(&ch)
        || ch == CharacterCodes::UNDERSCORE
        || ch == CharacterCodes::DOLLAR
        || ch > 127 // Unicode letter (simplified check)
}

fn is_identifier_part(ch: u32) -> bool {
    is_identifier_start(ch) || is_digit(ch)
}

fn is_line_break(ch: u32) -> bool {
    ch == CharacterCodes::LINE_FEED
        || ch == CharacterCodes::CARRIAGE_RETURN
        || ch == CharacterCodes::LINE_SEPARATOR
        || ch == CharacterCodes::PARAGRAPH_SEPARATOR
}

/// Check if a character is a valid regex flag (g, i, m, s, u, v, y, d)
fn is_regex_flag(ch: u32) -> bool {
    matches!(
        ch,
        CharacterCodes::LOWER_G  // g - global
        | CharacterCodes::LOWER_I  // i - ignore case
        | CharacterCodes::LOWER_M  // m - multiline
        | CharacterCodes::LOWER_S  // s - dotAll
        | CharacterCodes::LOWER_U  // u - unicode
        | CharacterCodes::LOWER_V  // v - unicode sets
        | CharacterCodes::LOWER_Y  // y - sticky
        | CharacterCodes::LOWER_D // d - has indices
    )
}
