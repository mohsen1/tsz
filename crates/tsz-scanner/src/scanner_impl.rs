//! Scanner implementation - the lexical analyzer for TypeScript.
//!
//! This module implements the core Scanner struct that tokenizes TypeScript source code.
//! It's designed to produce the same token stream as TypeScript's scanner.ts.
//!
//! IMPORTANT: All positions are byte-based internally for UTF-8 performance.
//! For ASCII-only files (99% of TypeScript), byte position == character position.
use crate::SyntaxKind;
use crate::char_codes::CharacterCodes;
use std::sync::Arc;
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
use tsz_common::interner::{Atom, Interner};
use wasm_bindgen::prelude::wasm_bindgen;

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
    /// String/template literal unterminated because EOF was reached (not newline).
    /// Used to distinguish TS1126 "Unexpected end of text" from TS1002 "Unterminated string literal".
    UnterminatedAtEof = 65536,
}

// =============================================================================
// Scanner State
// =============================================================================

/// A general scanner diagnostic (e.g., conflict markers).
#[derive(Clone, Debug)]
pub struct ScannerDiagnostic {
    /// Position of the error
    pub pos: usize,
    /// Length of the error span
    pub length: usize,
    /// Diagnostic message
    pub message: &'static str,
    /// Diagnostic code
    pub code: u32,
}

/// A regex flag error detected during scanning.
#[derive(Clone, Debug)]
pub struct RegexFlagError {
    /// Kind of error
    pub kind: RegexFlagErrorKind,
    /// Position of the error character
    pub pos: usize,
}

/// Kind of regex flag error
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegexFlagErrorKind {
    /// Duplicate flag (e.g., /foo/gg)
    Duplicate,
    /// Invalid flag character (e.g., /foo/x)
    InvalidFlag,
    /// Incompatible flags (u and v cannot be used together)
    IncompatibleFlags,
}

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
    pub regex_flag_errors: Vec<RegexFlagError>,
    pub scanner_diagnostics: Vec<ScannerDiagnostic>,
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
    /// Regex flag errors detected during scanning
    regex_flag_errors: Vec<RegexFlagError>,
    /// General scanner diagnostics (e.g., conflict markers)
    scanner_diagnostics: Vec<ScannerDiagnostic>,
    /// Whether to skip trivia (whitespace, comments)
    skip_trivia: bool,
    /// String interner for identifier deduplication
    #[wasm_bindgen(skip)]
    pub interner: Interner,
    /// Interned atom for current identifier token (avoids string comparison)
    token_atom: Atom,
}

#[wasm_bindgen]
#[allow(clippy::missing_const_for_fn)]
impl ScannerState {
    /// Exported scanner accessors are JS bindings and cannot be made `const`
    /// because `#[wasm_bindgen]` methods in this crate are non-`const`.
    /// Create a new scanner state with the given text.
    /// ZERO-COPY: No Vec<char> allocation, works directly with UTF-8 bytes.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(text: String, skip_trivia: bool) -> Self {
        // Common keywords are interned on-demand for faster startup
        let end = text.len();
        let interner = Interner::new();
        let source: Arc<str> = Arc::from(text.into_boxed_str());
        Self {
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
            regex_flag_errors: Vec::new(),
            scanner_diagnostics: Vec::new(),
            skip_trivia,
            interner,
            token_atom: Atom::NONE,
        }
    }

    /// Get the current position (end position of current token).
    #[wasm_bindgen(js_name = getPos)]
    #[must_use]
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
    #[must_use]
    pub fn get_token_full_start(&self) -> usize {
        self.full_start_pos
    }

    /// Get the start position of the current token (excluding trivia).
    #[wasm_bindgen(js_name = getTokenStart)]
    #[must_use]
    pub fn get_token_start(&self) -> usize {
        self.token_start
    }

    /// Get the end position of the current token.
    #[wasm_bindgen(js_name = getTokenEnd)]
    #[must_use]
    pub fn get_token_end(&self) -> usize {
        self.pos
    }

    /// Get the current token kind.
    #[wasm_bindgen(js_name = getToken)]
    #[must_use]
    pub fn get_token(&self) -> SyntaxKind {
        self.token
    }

    /// Get the current token's string value.
    /// Note: Prefer `get_token_value_ref()` to avoid allocation when possible.
    #[must_use]
    #[wasm_bindgen(js_name = getTokenValue)]
    pub fn get_token_value(&self) -> String {
        self.get_token_value_ref().to_string()
    }

    /// Get the current token's text from the source.
    #[must_use]
    #[wasm_bindgen(js_name = getTokenText)]
    pub fn get_token_text(&self) -> String {
        self.source[self.token_start..self.pos].to_string()
    }

    /// Get the token flags.
    #[must_use]
    #[wasm_bindgen(js_name = getTokenFlags)]
    pub fn get_token_flags(&self) -> u32 {
        self.token_flags
    }

    /// Check if there was a preceding line break.
    #[must_use]
    #[wasm_bindgen(js_name = hasPrecedingLineBreak)]
    pub fn has_preceding_line_break(&self) -> bool {
        (self.token_flags & TokenFlags::PrecedingLineBreak as u32) != 0
    }

    /// Check if the token is unterminated.
    #[must_use]
    #[wasm_bindgen(js_name = isUnterminated)]
    pub fn is_unterminated(&self) -> bool {
        (self.token_flags & TokenFlags::Unterminated as u32) != 0
    }

    /// Check if the current token is an identifier.
    #[must_use]
    #[wasm_bindgen(js_name = isIdentifier)]
    pub fn is_identifier(&self) -> bool {
        self.token == SyntaxKind::Identifier
            || (self.token as u16) > (SyntaxKind::WithKeyword as u16)
    }

    /// Check if the current token is a reserved word.
    #[must_use]
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
    #[must_use]
    #[wasm_bindgen(js_name = getText)]
    pub fn get_text(&self) -> String {
        self.source.to_string()
    }

    // =========================================================================
    // Helper methods (byte-indexed for zero-copy performance)
    // =========================================================================

    /// Get byte at index as u32 char code. Returns 0 if out of bounds.
    /// FAST PATH: For ASCII bytes (0-127), this is the character code.
    #[inline]
    #[must_use]
    fn char_code_unchecked(&self, index: usize) -> u32 {
        let bytes = self.source.as_bytes();
        if index < bytes.len() {
            let b = bytes[index];
            if b < 128 {
                // ASCII: byte value == char code
                u32::from(b)
            } else {
                // Non-ASCII: decode UTF-8 char.
                // Guard: index must be on a char boundary; if not, scan back to find it.
                if self.source.is_char_boundary(index) {
                    self.source[index..].chars().next().map_or(0, |c| c as u32)
                } else {
                    // Find the start of the current char by scanning back
                    let mut start = index;
                    while start > 0 && !self.source.is_char_boundary(start) {
                        start -= 1;
                    }
                    self.source[start..].chars().next().map_or(0, |c| c as u32)
                }
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
                Some(u32::from(b))
            } else if self.source.is_char_boundary(index) {
                self.source[index..].chars().next().map(|c| c as u32)
            } else {
                let mut start = index;
                while start > 0 && !self.source.is_char_boundary(start) {
                    start -= 1;
                }
                self.source[start..].chars().next().map(|c| c as u32)
            }
        } else {
            None
        }
    }

    /// Get byte length of character at position (1 for ASCII, 1-4 for UTF-8)
    #[inline]
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
        self.regex_flag_errors.clear();
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
                    }
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
                    }
                    while self.pos < self.end
                        && is_white_space_single_line(self.char_code_unchecked(self.pos))
                    {
                        self.pos += self.char_len_at(self.pos);
                    }
                    self.token = SyntaxKind::WhitespaceTrivia;
                    return self.token;
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
                    }
                    self.pos += 3; // BOM is 3 bytes in UTF-8
                    while self.pos < self.end
                        && is_white_space_single_line(self.char_code_unchecked(self.pos))
                    {
                        self.pos += self.char_len_at(self.pos);
                    }
                    self.token = SyntaxKind::WhitespaceTrivia;
                    return self.token;
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
                    if self.is_conflict_marker_trivia() {
                        self.scan_conflict_marker_trivia();
                        if self.skip_trivia {
                            continue;
                        }
                        self.token = SyntaxKind::ConflictMarkerTrivia;
                        return self.token;
                    }
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
                    if self.is_conflict_marker_trivia() {
                        self.scan_conflict_marker_trivia();
                        if self.skip_trivia {
                            continue;
                        }
                        self.token = SyntaxKind::ConflictMarkerTrivia;
                        return self.token;
                    }
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
                    if self.is_conflict_marker_trivia() {
                        self.scan_conflict_marker_trivia();
                        if self.skip_trivia {
                            continue;
                        }
                        self.token = SyntaxKind::ConflictMarkerTrivia;
                        return self.token;
                    }
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
                    if self.is_conflict_marker_trivia() {
                        self.scan_conflict_marker_trivia();
                        if self.skip_trivia {
                            continue;
                        }
                        self.token = SyntaxKind::ConflictMarkerTrivia;
                        return self.token;
                    }
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
                        }
                        self.token = SyntaxKind::SingleLineCommentTrivia;
                        return self.token;
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
                            // TS1010: "'*/' expected."
                            self.scanner_diagnostics.push(ScannerDiagnostic {
                                pos: self.pos,
                                length: 0,
                                message: diagnostic_messages::EXPECTED_2,
                                code: diagnostic_codes::EXPECTED_2,
                            });
                        }
                        if self.skip_trivia {
                            continue;
                        }
                        self.token = SyntaxKind::MultiLineCommentTrivia;
                        return self.token;
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

                // Backslash - Unicode escape sequence starting an identifier (\uXXXX)
                CharacterCodes::BACKSLASH => {
                    // In TypeScript, \uXXXX can start an identifier
                    // e.g., \u0041 is 'A', so `let \u0041 = 1;` is valid
                    let escaped_ch = self.peek_unicode_escape();
                    if let Some(code_point) = escaped_ch
                        && is_identifier_start(code_point)
                    {
                        self.scan_identifier_with_escapes();
                        return self.token;
                    }
                    // Not a valid unicode escape - treat as unknown
                    self.pos += 1;
                    self.token = SyntaxKind::Unknown;
                    return self.token;
                }

                // Default: identifier or unknown
                _ => {
                    // Handle Unicode line separators (U+2028, U+2029) as newlines
                    if ch == CharacterCodes::LINE_SEPARATOR
                        || ch == CharacterCodes::PARAGRAPH_SEPARATOR
                    {
                        self.token_flags |= TokenFlags::PrecedingLineBreak as u32;
                        if self.skip_trivia {
                            self.pos += self.char_len_at(self.pos);
                            continue;
                        }
                        self.pos += self.char_len_at(self.pos);
                        self.token = SyntaxKind::NewLineTrivia;
                        return self.token;
                    }
                    // Handle additional Unicode whitespace characters not in the fast path above
                    if ch > 127 && is_white_space_single_line(ch) {
                        if self.skip_trivia {
                            self.pos += self.char_len_at(self.pos);
                            while self.pos < self.end
                                && is_white_space_single_line(self.char_code_unchecked(self.pos))
                            {
                                self.pos += self.char_len_at(self.pos);
                            }
                            continue;
                        }
                        self.pos += self.char_len_at(self.pos);
                        while self.pos < self.end
                            && is_white_space_single_line(self.char_code_unchecked(self.pos))
                        {
                            self.pos += self.char_len_at(self.pos);
                        }
                        self.token = SyntaxKind::WhitespaceTrivia;
                        return self.token;
                    }
                    if is_identifier_start(ch) {
                        self.scan_identifier();
                        return self.token;
                    }
                    // Skip unknown character (properly handle multi-byte UTF-8)
                    self.pos += self.char_len_at(self.pos);
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
                self.pos += 1;
                if self.pos < self.end {
                    self.scan_string_escape(quote, &mut result);
                } else {
                    // Backslash at EOF — incomplete escape sequence.
                    // Mark with UnterminatedAtEof so the parser can emit TS1126
                    // instead of TS1002.
                    self.token_flags |=
                        TokenFlags::Unterminated as u32 | TokenFlags::UnterminatedAtEof as u32;
                    self.token_value = result;
                    self.token = SyntaxKind::StringLiteral;
                    return;
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

        // Unterminated string — reached EOF without closing quote.
        // If the string contains an invalid/incomplete escape sequence (e.g., \u{...
        // without closing brace, or bare \u at EOF), tsc emits TS1126 "Unexpected end
        // of text". Otherwise, tsc emits TS1002 "Unterminated string literal".
        self.token_flags |= TokenFlags::Unterminated as u32;
        if (self.token_flags & TokenFlags::ContainsInvalidEscape as u32) != 0 {
            self.token_flags |= TokenFlags::UnterminatedAtEof as u32;
        }
        self.token_value = result;
        self.token = SyntaxKind::StringLiteral;
    }

    fn scan_string_escape(&mut self, quote: u32, result: &mut String) {
        let escaped = self.char_code_unchecked(self.pos);
        // Get byte length of the escaped character before advancing.
        let escaped_len = self.char_len_at(self.pos);
        self.pos += escaped_len;

        match escaped {
            CharacterCodes::_0 => self.scan_string_escape_zero(result),
            CharacterCodes::_1
            | CharacterCodes::_2
            | CharacterCodes::_3
            | CharacterCodes::_4
            | CharacterCodes::_5
            | CharacterCodes::_6
            | CharacterCodes::_7 => self.scan_string_escape_octal(escaped, result),
            CharacterCodes::LOWER_N => result.push('\n'),
            CharacterCodes::LOWER_R => result.push('\r'),
            CharacterCodes::LOWER_T => result.push('\t'),
            CharacterCodes::LOWER_V => result.push('\x0B'),
            CharacterCodes::LOWER_B => result.push('\x08'),
            CharacterCodes::LOWER_F => result.push('\x0C'),
            CharacterCodes::BACKSLASH => result.push('\\'),
            c if c == quote => result.push(char::from_u32(quote).unwrap_or('\0')),
            CharacterCodes::LOWER_X => self.scan_string_escape_hex(result),
            CharacterCodes::LOWER_U => self.scan_string_escape_unicode(result),
            CharacterCodes::LINE_FEED
            | CharacterCodes::CARRIAGE_RETURN
            | CharacterCodes::LINE_SEPARATOR
            | CharacterCodes::PARAGRAPH_SEPARATOR => {
                // Line continuation - also handle CR+LF as a single line break
                if escaped == CharacterCodes::CARRIAGE_RETURN
                    && self.pos < self.end
                    && self.char_code_unchecked(self.pos) == CharacterCodes::LINE_FEED
                {
                    self.pos += 1;
                }
            }
            _ => {
                if let Some(c) = char::from_u32(escaped) {
                    result.push(c);
                }
            }
        }
    }

    fn scan_string_escape_zero(&mut self, result: &mut String) {
        if self.pos < self.end && is_digit(self.char_code_unchecked(self.pos)) {
            // Legacy octal escape: \0N - scan as octal
            let mut value = 0u32;
            let octal_start = self.pos - 1; // include the '0'
            self.pos = octal_start;
            while self.pos < self.end
                && self.pos < octal_start + 3
                && is_octal_digit(self.char_code_unchecked(self.pos))
            {
                value = value * 8 + (self.char_code_unchecked(self.pos) - CharacterCodes::_0);
                self.pos += 1;
            }
            if let Some(c) = char::from_u32(value) {
                result.push(c);
            }
        } else {
            result.push('\0');
        }
    }

    fn scan_string_escape_octal(&mut self, escaped: u32, result: &mut String) {
        // Legacy octal escape: \1 through \7
        let mut value = escaped - CharacterCodes::_0;
        let mut count = 1;
        while count < 3 && self.pos < self.end && is_octal_digit(self.char_code_unchecked(self.pos))
        {
            value = value * 8 + (self.char_code_unchecked(self.pos) - CharacterCodes::_0);
            self.pos += 1;
            count += 1;
        }
        if let Some(c) = char::from_u32(value) {
            result.push(c);
        }
    }

    fn scan_string_escape_hex(&mut self, result: &mut String) {
        if self.pos + 2 <= self.end {
            let hex = self.substring(self.pos, self.pos + 2);
            if let Ok(code) = u32::from_str_radix(&hex, 16) {
                self.pos += 2;
                if let Some(c) = char::from_u32(code) {
                    result.push(c);
                }
                return;
            }
        }
        result.push('\\');
        result.push('x');
    }

    fn scan_string_escape_unicode(&mut self, result: &mut String) {
        if self.pos < self.end && self.char_code_unchecked(self.pos) == CharacterCodes::OPEN_BRACE {
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
                    result.push(c);
                    return;
                }
            }
            // Invalid or unterminated \u{...} escape
            self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
            result.push('\\');
            result.push('u');
            return;
        }
        if self.pos + 4 <= self.end {
            let hex = self.substring(self.pos, self.pos + 4);
            if let Ok(code) = u32::from_str_radix(&hex, 16)
                && let Some(c) = char::from_u32(code)
            {
                self.pos += 4;
                result.push(c);
                return;
            }
            // Invalid \uXXXX escape
            self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
            result.push('\\');
            result.push('u');
            return;
        }
        // Incomplete \u escape (not enough chars)
        self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
        result.push('\\');
        result.push('u');
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
                // Scan escaped character after the backslash.
                self.pos += 1;
                let escaped = self.scan_template_escape_sequence();
                result.push_str(&escaped);
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
            if self.scan_prefixed_number(start, next) {
                return;
            }

            // After leading 0, scan all consecutive digits and check if all are octal.
            // This matches tsc's scanDigits(): scan 0-9, return whether all were 0-7.
            if is_digit(next) && self.scan_legacy_octal_number(start) {
                return;
            }
        }

        // Decimal number
        self.scan_decimal_number(start);
    }

    fn scan_prefixed_number(&mut self, start: usize, next: u32) -> bool {
        match next {
            CharacterCodes::LOWER_X | CharacterCodes::UPPER_X => {
                self.scan_integer_base_literal(start, is_hex_digit, TokenFlags::HexSpecifier);
                true
            }
            CharacterCodes::LOWER_B | CharacterCodes::UPPER_B => {
                self.scan_integer_base_literal(start, is_binary_digit, TokenFlags::BinarySpecifier);
                true
            }
            CharacterCodes::LOWER_O | CharacterCodes::UPPER_O => {
                self.scan_integer_base_literal(start, is_octal_digit, TokenFlags::OctalSpecifier);
                true
            }
            _ => false,
        }
    }

    fn scan_integer_base_literal(
        &mut self,
        start: usize,
        is_valid_digit: fn(u32) -> bool,
        specifier_flag: TokenFlags,
    ) {
        self.pos += 2;
        self.token_flags |= specifier_flag as u32;
        self.scan_digits_with_separators(is_valid_digit);

        if self.pos < self.end && self.char_code_unchecked(self.pos) == CharacterCodes::LOWER_N {
            self.pos += 1;
            self.token_value = self.substring(start, self.pos);
            self.check_for_identifier_start_after_numeric_literal(
                start, /*is_scientific*/ false,
            );
            self.token = SyntaxKind::BigIntLiteral;
            return;
        }

        self.set_numeric_token_value(start);
        self.check_for_identifier_start_after_numeric_literal(start, /*is_scientific*/ false);
        self.token = SyntaxKind::NumericLiteral;
    }

    fn scan_legacy_octal_number(&mut self, start: usize) -> bool {
        let mut all_octal = true;
        let digit_start = self.pos + 1; // skip the leading 0
        let mut scan_pos = digit_start;
        while scan_pos < self.end && is_digit(self.char_code_unchecked(scan_pos)) {
            if !is_octal_digit(self.char_code_unchecked(scan_pos)) {
                all_octal = false;
            }
            scan_pos += 1;
        }
        if all_octal && scan_pos > digit_start {
            self.pos = scan_pos;
            self.token_flags |= TokenFlags::Octal as u32;
            self.set_numeric_token_value(start);
            self.token = SyntaxKind::NumericLiteral;
            true
        } else {
            self.token_flags |= TokenFlags::ContainsLeadingZero as u32;
            false
        }
    }

    fn set_numeric_token_value(&mut self, start: usize) {
        if (self.token_flags & TokenFlags::ContainsSeparator as u32) != 0 {
            self.token_value = self.substring(start, self.pos);
        } else {
            self.token_value.clear();
        }
    }

    fn scan_decimal_number(&mut self, start: usize) {
        self.scan_digits_with_separators(is_digit);
        let mut has_decimal_point = false;

        // Decimal point
        if self.pos < self.end && self.char_code_unchecked(self.pos) == CharacterCodes::DOT {
            has_decimal_point = true;
            self.pos += 1;
            self.scan_digits_with_separators(is_digit);
        }

        // Exponent
        let mut has_exponent = false;
        if self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::LOWER_E || ch == CharacterCodes::UPPER_E {
                has_exponent = true;
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
        if !has_decimal_point
            && !has_exponent
            && self.pos < self.end
            && self.char_code_unchecked(self.pos) == CharacterCodes::LOWER_N
        {
            self.pos += 1;
            self.token_value = self.substring(start, self.pos);
            self.check_for_identifier_start_after_numeric_literal(
                start, /*is_scientific*/ false,
            );
            self.token = SyntaxKind::BigIntLiteral;
            return;
        }

        let numeric_end = self.pos;
        // OPTIMIZATION: Only allocate token_value if number contains separators
        // Plain numbers (no underscores) can use source slice via get_token_value_ref()
        self.set_numeric_token_value(start);
        if self.check_for_identifier_start_after_numeric_literal(start, has_exponent)
            && self.token_value.is_empty()
        {
            self.token_value = self.substring(start, numeric_end);
        }
        self.token = SyntaxKind::NumericLiteral;
    }

    fn check_for_identifier_start_after_numeric_literal(
        &mut self,
        numeric_start: usize,
        is_scientific: bool,
    ) -> bool {
        if self.pos >= self.end {
            return false;
        }

        // Only check the raw character code, not unicode escapes.
        // TSC uses `codePointAt(text, pos)` here which reads the literal char,
        // so `\u005F` (backslash) is NOT treated as an identifier start.
        let starts_identifier = is_identifier_start(self.char_code_unchecked(self.pos));

        if !starts_identifier {
            return false;
        }

        let identifier_start = self.pos;
        self.scan_identifier_parts_after_numeric_literal();
        let identifier_end = self.pos;
        let identifier_text = &self.source[identifier_start..identifier_end];

        if identifier_text == "n" {
            self.scanner_diagnostics.push(ScannerDiagnostic {
                pos: numeric_start,
                length: identifier_end - numeric_start,
                message: if is_scientific {
                    diagnostic_messages::A_BIGINT_LITERAL_CANNOT_USE_EXPONENTIAL_NOTATION
                } else {
                    diagnostic_messages::A_BIGINT_LITERAL_MUST_BE_AN_INTEGER
                },
                code: if is_scientific {
                    diagnostic_codes::A_BIGINT_LITERAL_CANNOT_USE_EXPONENTIAL_NOTATION
                } else {
                    diagnostic_codes::A_BIGINT_LITERAL_MUST_BE_AN_INTEGER
                },
            });
            true
        } else {
            self.scanner_diagnostics.push(ScannerDiagnostic {
                pos: identifier_start,
                length: identifier_end - identifier_start,
                message: diagnostic_messages::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
                code: diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
            });
            self.pos = identifier_start;
            false
        }
    }

    fn scan_identifier_parts_after_numeric_literal(&mut self) {
        if self.pos >= self.end {
            return;
        }

        if self.char_code_unchecked(self.pos) == CharacterCodes::BACKSLASH {
            if let Some(code_point) = self.peek_unicode_escape() {
                if is_identifier_start(code_point) {
                    let _ = self.scan_unicode_escape_value();
                } else {
                    return;
                }
            } else {
                return;
            }
        } else if is_identifier_start(self.char_code_unchecked(self.pos)) {
            self.pos += self.char_len_at(self.pos);
        } else {
            return;
        }

        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::BACKSLASH {
                if let Some(code_point) = self.peek_unicode_escape()
                    && is_identifier_part(code_point)
                {
                    let _ = self.scan_unicode_escape_value();
                    continue;
                }
                break;
            }
            if !is_identifier_part(ch) {
                break;
            }
            self.pos += self.char_len_at(self.pos);
        }
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
    /// When a unicode escape is encountered mid-identifier, switches to allocation mode.
    fn scan_identifier(&mut self) {
        let start = self.pos;
        // Advance past first character (may be multi-byte)
        self.pos += self.char_len_at(self.pos);

        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::BACKSLASH {
                // Check if this is a unicode escape that produces an identifier part
                if let Some(code_point) = self.peek_unicode_escape()
                    && is_identifier_part(code_point)
                {
                    // Switch to allocation mode and continue scanning with escapes
                    self.continue_identifier_with_escapes(start);
                    return;
                }
                // Invalid escape or not an identifier part - stop here
                break;
            }
            if !is_identifier_part(ch) {
                break;
            }
            self.pos += self.char_len_at(self.pos); // Handle multi-byte UTF-8
        }

        // Get slice reference instead of allocating new String
        let text_slice = &self.source[start..self.pos];

        // Check if it's a keyword first (common keywords are pre-interned)
        self.token = crate::text_to_keyword(text_slice).unwrap_or(SyntaxKind::Identifier);

        // Intern the identifier for O(1) comparison (reuses existing interned string)
        self.token_atom = self.interner.intern(text_slice);

        // ZERO-ALLOCATION: Don't store token_value for identifiers.
        // get_token_value_ref() will resolve from token_atom or fall back to source slice.
        self.token_value.clear();
    }

    /// Continue scanning an identifier that has a unicode escape mid-identifier.
    /// Called when `scan_identifier()` encounters a valid unicode escape.
    /// This switches to allocation mode since escapes require building a String.
    fn continue_identifier_with_escapes(&mut self, start: usize) {
        // Copy the already-scanned part into a String
        let mut result = String::from(&self.source[start..self.pos]);

        // Continue scanning identifier parts, handling unicode escapes
        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::BACKSLASH {
                // Check for unicode escape
                if let Some(code_point) = self.peek_unicode_escape()
                    && is_identifier_part(code_point)
                {
                    // Consume the escape and add the character
                    if let Some(c) = char::from_u32(self.scan_unicode_escape_value().unwrap_or(0)) {
                        result.push(c);
                    }
                    continue;
                }
                // Invalid escape or not an identifier part - stop here
                break;
            }
            if !is_identifier_part(ch) {
                break;
            }
            if let Some(c) = char::from_u32(ch) {
                result.push(c);
            }
            self.pos += self.char_len_at(self.pos);
        }

        self.token = crate::text_to_keyword(&result).unwrap_or(SyntaxKind::Identifier);
        self.token_atom = self.interner.intern(&result);
        self.token_value.clear();
        self.token_flags |= TokenFlags::UnicodeEscape as u32;
    }

    /// Peek at a unicode escape sequence without advancing the position.
    /// Returns the code point if the escape is valid (\uXXXX or \u{XXXXX}), None otherwise.
    fn peek_unicode_escape(&self) -> Option<u32> {
        // Must start with \u
        if self.pos + 1 >= self.end {
            return None;
        }
        let bytes = self.source.as_bytes();
        if bytes.get(self.pos + 1).copied() != Some(b'u') {
            return None;
        }
        // \u{XXXXX} form
        if bytes.get(self.pos + 2).copied() == Some(b'{') {
            let start = self.pos + 3;
            let mut end = start;
            while end < self.end && bytes.get(end).is_some_and(u8::is_ascii_hexdigit) {
                end += 1;
            }
            if end == start || bytes.get(end).copied() != Some(b'}') {
                return None;
            }
            let hex = &self.source[start..end];
            u32::from_str_radix(hex, 16)
                .ok()
                .filter(|&cp| cp <= 0x0010_FFFF)
        } else {
            // \uXXXX form (exactly 4 hex digits)
            if self.pos + 5 >= self.end {
                return None;
            }
            let hex = &self.source[self.pos + 2..self.pos + 6];
            if hex.len() == 4 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                u32::from_str_radix(hex, 16).ok()
            } else {
                None
            }
        }
    }

    /// Scan an identifier that starts with a unicode escape sequence (\uXXXX).
    fn scan_identifier_with_escapes(&mut self) {
        let mut result = String::new();

        // Process initial unicode escape
        if let Some(ch) = self.scan_unicode_escape_value()
            && let Some(c) = char::from_u32(ch)
        {
            result.push(c);
        }

        // Continue scanning identifier parts
        while self.pos < self.end {
            let ch = self.char_code_unchecked(self.pos);
            if ch == CharacterCodes::BACKSLASH {
                // Another unicode escape in identifier
                if let Some(code_point) = self.peek_unicode_escape()
                    && is_identifier_part(code_point)
                {
                    if let Some(c) = char::from_u32(self.scan_unicode_escape_value().unwrap_or(0)) {
                        result.push(c);
                    }
                    continue;
                }
                break;
            }
            if !is_identifier_part(ch) {
                break;
            }
            if let Some(c) = char::from_u32(ch) {
                result.push(c);
            }
            self.pos += self.char_len_at(self.pos);
        }

        self.token = crate::text_to_keyword(&result).unwrap_or(SyntaxKind::Identifier);
        self.token_atom = self.interner.intern(&result);
        self.token_value.clear();
        self.token_flags |= TokenFlags::UnicodeEscape as u32;
    }

    /// Consume a unicode escape sequence and return its code point.
    /// Advances self.pos past the escape.
    fn scan_unicode_escape_value(&mut self) -> Option<u32> {
        // Skip the backslash
        self.pos += 1;
        if self.pos >= self.end || self.source.as_bytes()[self.pos] != b'u' {
            return None;
        }
        self.pos += 1; // Skip 'u'

        if self.pos < self.end && self.source.as_bytes()[self.pos] == b'{' {
            // \u{XXXXX} form
            self.pos += 1;
            let start = self.pos;
            while self.pos < self.end
                && self
                    .source
                    .as_bytes()
                    .get(self.pos)
                    .is_some_and(u8::is_ascii_hexdigit)
            {
                self.pos += 1;
            }
            let result = u32::from_str_radix(&self.source[start..self.pos], 16).ok();
            if self.pos < self.end && self.source.as_bytes()[self.pos] == b'}' {
                self.pos += 1;
            }
            result
        } else {
            // \uXXXX form (exactly 4 hex digits)
            if self.pos + 4 > self.end {
                return None;
            }
            let hex = &self.source[self.pos..self.pos + 4];
            if hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                self.pos += 4;
                u32::from_str_radix(hex, 16).ok()
            } else {
                None
            }
        }
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

            // If we reached EOF without finding closing /, mark as unterminated
            if self.pos >= self.end && (self.token_flags & TokenFlags::Unterminated as u32) == 0 {
                self.token_flags |= TokenFlags::Unterminated as u32;
            }

            if (self.token_flags & TokenFlags::Unterminated as u32) == 0 {
                // Consume the closing /
                self.pos += 1;

                // Scan and validate regex flags (g, i, m, s, u, v, y, d)
                // Track seen flags as a bitmask for duplicate detection
                let mut seen_flags: u8 = 0;
                let mut has_u = false;
                let mut has_v = false;

                while self.pos < self.end {
                    let ch = self.char_code_unchecked(self.pos);
                    if !is_regex_flag(ch) && !is_identifier_part(ch) {
                        break;
                    }

                    // Check for valid flags and detect errors
                    let flag_bit = match ch {
                        CharacterCodes::LOWER_G => Some(0),
                        CharacterCodes::LOWER_I => Some(1),
                        CharacterCodes::LOWER_M => Some(2),
                        CharacterCodes::LOWER_S => Some(3),
                        CharacterCodes::LOWER_U => {
                            has_u = true;
                            Some(4)
                        }
                        CharacterCodes::LOWER_V => {
                            has_v = true;
                            Some(5)
                        }
                        CharacterCodes::LOWER_Y => Some(6),
                        CharacterCodes::LOWER_D => Some(7),
                        _ => None,
                    };

                    if let Some(bit) = flag_bit {
                        let mask = 1 << bit;
                        if seen_flags & mask != 0 {
                            // Duplicate flag - emit error for each duplicate
                            self.regex_flag_errors.push(RegexFlagError {
                                kind: RegexFlagErrorKind::Duplicate,
                                pos: self.pos,
                            });
                        }
                        seen_flags |= mask;
                    } else if is_identifier_part(ch) {
                        // Invalid flag character (identifier char but not a valid flag)
                        self.regex_flag_errors.push(RegexFlagError {
                            kind: RegexFlagErrorKind::InvalidFlag,
                            pos: self.pos,
                        });
                    }

                    // Use char_len_at for proper UTF-8 handling (handles non-ASCII flags)
                    self.pos += self.char_len_at(self.pos);
                }

                // Check for incompatible u and v flags
                if has_u && has_v {
                    // Emit error at the end of flags (similar to TypeScript)
                    self.regex_flag_errors.push(RegexFlagError {
                        kind: RegexFlagErrorKind::IncompatibleFlags,
                        pos: self.pos,
                    });
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
                self.pos += 1; // Advance past the backslash
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

            // Advance by full UTF-8 codepoint width so multi-byte chars (e.g. µ) don't
            // move the scanner into a non-char-boundary byte index.
            self.pos += self.char_len_at(self.pos);
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
    ///
    /// `self.pos` is expected to point at the escaped character (just after `\`).
    fn scan_template_escape_sequence(&mut self) -> String {
        if self.pos >= self.end {
            return String::from("\\");
        }

        let ch = self.char_code_unchecked(self.pos);
        // Use char_len_at for proper UTF-8 handling of multi-byte chars
        let ch_len = self.char_len_at(self.pos);
        self.pos += ch_len;

        match ch {
            CharacterCodes::_0 => self.scan_template_escape_digit_zero(),
            CharacterCodes::_1
            | CharacterCodes::_2
            | CharacterCodes::_3
            | CharacterCodes::_4
            | CharacterCodes::_5
            | CharacterCodes::_6
            | CharacterCodes::_7
            | CharacterCodes::_8
            | CharacterCodes::_9 => self.scan_template_escape_octal_digit(ch),
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
            | CharacterCodes::PARAGRAPH_SEPARATOR => String::new(),
            CharacterCodes::CARRIAGE_RETURN => self.scan_template_escape_cr(),
            CharacterCodes::LOWER_X => self.scan_template_hex_escape(),
            CharacterCodes::LOWER_U => self.scan_template_unicode_escape(),
            _ => Self::scan_template_unknown_escape(ch),
        }
    }

    fn scan_template_escape_digit_zero(&mut self) -> String {
        if self.pos < self.end && is_digit(self.char_code_unchecked(self.pos)) {
            self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
            return String::from("\\0");
        }
        String::from("\0")
    }

    fn scan_template_escape_octal_digit(&mut self, ch: u32) -> String {
        self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
        let digit = char::from_u32(ch).unwrap_or('?');
        format!("\\{digit}")
    }

    fn scan_template_escape_cr(&mut self) -> String {
        if self.pos < self.end && self.char_code_unchecked(self.pos) == CharacterCodes::LINE_FEED {
            self.pos += 1;
        }
        String::new()
    }

    fn scan_template_hex_escape(&mut self) -> String {
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

    fn scan_template_unicode_escape(&mut self) -> String {
        if self.pos < self.end && self.char_code_unchecked(self.pos) == CharacterCodes::OPEN_BRACE {
            return self.scan_template_brace_unicode_escape();
        }

        if self.pos + 4 <= self.end {
            let hex = self.substring(self.pos, self.pos + 4);
            if let Ok(code) = u32::from_str_radix(&hex, 16) {
                self.pos += 4;
                if let Some(c) = char::from_u32(code) {
                    return c.to_string();
                }
            }
            self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
            return String::from("\\u");
        }

        self.token_flags |= TokenFlags::ContainsInvalidEscape as u32;
        String::from("\\u")
    }

    fn scan_template_brace_unicode_escape(&mut self) -> String {
        self.pos += 1;
        let hex_start = self.pos;
        while self.pos < self.end && is_hex_digit(self.char_code_unchecked(self.pos)) {
            self.pos += 1;
        }
        if self.pos < self.end && self.char_code_unchecked(self.pos) == CharacterCodes::CLOSE_BRACE
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
    }

    fn scan_template_unknown_escape(ch: u32) -> String {
        if let Some(c) = char::from_u32(ch) {
            c.to_string()
        } else {
            String::new()
        }
    }

    // =========================================================================
    // JSX Scanning Methods
    // =========================================================================

    /// Scan a JSX identifier.
    /// In JSX, identifiers can contain hyphens (like `data-testid`).
    #[wasm_bindgen(js_name = scanJsxIdentifier)]
    pub fn scan_jsx_identifier(&mut self) -> SyntaxKind {
        if crate::token_is_identifier_or_keyword(self.token) {
            // Continue scanning to include any hyphenated parts.
            // JSX identifiers can be like: foo-bar-baz, class-id, etc.
            // Keywords like `class` can also start JSX attribute names.
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
            // ZERO-ALLOCATION: Intern directly from source slice, clear token_value
            self.token_atom = self
                .interner
                .intern(&self.source[self.token_start..self.pos]);
            self.token_value.clear();
            // After extending with hyphens, the token becomes an Identifier
            self.token = SyntaxKind::Identifier;
        }
        self.token
    }

    /// Re-scan the current token as a JSX token.
    /// Used when the parser enters JSX context and needs to rescan.
    /// Must reset to `full_start_pos` (before trivia), not `token_start` (after trivia),
    /// so that JSX text nodes include leading whitespace/newlines.
    /// Matches tsc: `pos = tokenStart = fullStartPos;`
    #[wasm_bindgen(js_name = reScanJsxToken)]
    pub fn re_scan_jsx_token(&mut self, allow_multiline_jsx_text: bool) -> SyntaxKind {
        // Remove any scanner diagnostics emitted at positions >= full_start_pos.
        // The previous scan (in normal JS mode) may have produced false diagnostics
        // (e.g., TS1351 for `7x` in JSX text content). Since we're rescanning this
        // range in JSX mode, those diagnostics are invalid.
        let rescan_start = self.full_start_pos;
        self.scanner_diagnostics.retain(|d| d.pos < rescan_start);
        self.pos = self.full_start_pos;
        self.scan_jsx_token(allow_multiline_jsx_text)
    }

    /// Scan a JSX token (text, open element, close element, etc.)
    fn scan_jsx_token(&mut self, allow_multiline_jsx_text: bool) -> SyntaxKind {
        self.full_start_pos = self.pos;
        self.token_start = self.pos;
        // Clear stale atom from any prior identifier scan so get_token_value_ref()
        // returns the freshly scanned JSX text, not the old interned identifier.
        self.token_atom = Atom::NONE;

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

            // TS1382: bare `>` in JSX text
            if c == CharacterCodes::GREATER_THAN {
                self.scanner_diagnostics.push(ScannerDiagnostic {
                    pos: self.pos,
                    length: 1,
                    message: diagnostic_messages::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT,
                    code: diagnostic_codes::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT,
                });
            }

            // TS1381: bare `}` in JSX text
            if c == CharacterCodes::CLOSE_BRACE {
                self.scanner_diagnostics.push(ScannerDiagnostic {
                    pos: self.pos,
                    length: 1,
                    message: diagnostic_messages::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_RBRACE,
                    code: diagnostic_codes::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_RBRACE,
                });
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
        self.token_flags = 0;

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

        self.scan()
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
    /// Returns `LessThanSlashToken` if followed by `/`, otherwise `LessThanToken`.
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
            // Properly handle multi-byte UTF-8 characters in private identifiers
            self.pos += self.char_len_at(self.pos);
            while self.pos < self.end && is_identifier_part(self.char_code_unchecked(self.pos)) {
                self.pos += self.char_len_at(self.pos);
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
                if !next.is_some_and(is_digit) {
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

    /// Scan a `JSDoc` token.
    /// Used when parsing `JSDoc` comments.
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

        if self.scan_jsdoc_punctuation_token(ch) {
            return self.token;
        }

        // Check for identifier
        if is_identifier_start(ch) {
            return self.scan_jsdoc_identifier();
        }

        // Unknown character - advance and return Unknown (properly handle multi-byte UTF-8)
        self.scan_jsdoc_unknown_character();
        self.token
    }

    fn scan_jsdoc_punctuation_token(&mut self, ch: u32) -> bool {
        match ch {
            CharacterCodes::AT => {
                self.pos += 1;
                self.token = SyntaxKind::AtToken;
            }
            CharacterCodes::ASTERISK => {
                self.pos += 1;
                self.token = SyntaxKind::AsteriskToken;
            }
            CharacterCodes::OPEN_BRACE => {
                self.pos += 1;
                self.token = SyntaxKind::OpenBraceToken;
            }
            CharacterCodes::CLOSE_BRACE => {
                self.pos += 1;
                self.token = SyntaxKind::CloseBraceToken;
            }
            CharacterCodes::OPEN_BRACKET => {
                self.pos += 1;
                self.token = SyntaxKind::OpenBracketToken;
            }
            CharacterCodes::CLOSE_BRACKET => {
                self.pos += 1;
                self.token = SyntaxKind::CloseBracketToken;
            }
            CharacterCodes::LESS_THAN => {
                self.pos += 1;
                self.token = SyntaxKind::LessThanToken;
            }
            CharacterCodes::GREATER_THAN => {
                self.pos += 1;
                self.token = SyntaxKind::GreaterThanToken;
            }
            CharacterCodes::EQUALS => {
                self.pos += 1;
                self.token = SyntaxKind::EqualsToken;
            }
            CharacterCodes::COMMA => {
                self.pos += 1;
                self.token = SyntaxKind::CommaToken;
            }
            CharacterCodes::DOT => {
                self.pos += 1;
                self.token = SyntaxKind::DotToken;
            }
            CharacterCodes::BACKTICK => {
                self.pos += 1;
                while self.pos < self.end
                    && self.char_code_unchecked(self.pos) != CharacterCodes::BACKTICK
                {
                    self.pos += 1;
                }
                if self.pos < self.end {
                    self.pos += 1;
                }
                self.token_value = self.substring(self.token_start, self.pos);
                self.token = SyntaxKind::NoSubstitutionTemplateLiteral;
            }
            _ => return false,
        }
        true
    }

    fn scan_jsdoc_identifier(&mut self) -> SyntaxKind {
        self.pos += self.char_len_at(self.pos);
        while self.pos < self.end && is_identifier_part(self.char_code_unchecked(self.pos)) {
            self.pos += self.char_len_at(self.pos);
        }
        self.token_value = self.substring(self.token_start, self.pos);
        self.token = crate::text_to_keyword(&self.token_value).unwrap_or(SyntaxKind::Identifier);
        self.token
    }

    fn scan_jsdoc_unknown_character(&mut self) {
        self.pos += self.char_len_at(self.pos);
        self.token = SyntaxKind::Unknown;
    }

    /// Scan `JSDoc` comment text token.
    /// Used for scanning the text content within `JSDoc` comments.
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
                CharacterCodes::AT | CharacterCodes::OPEN_BRACE | CharacterCodes::CLOSE_BRACE
                    if !in_backticks =>
                {
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
                _ => {
                    // Properly handle multi-byte UTF-8 characters
                    self.pos += self.char_len_at(self.pos);
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
                // Use char_len_at to properly handle multi-byte UTF-8 characters
                self.pos += self.char_len_at(self.pos);
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
                    self.token =
                        crate::text_to_keyword(&self.token_value).unwrap_or(SyntaxKind::Identifier);
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
    #[must_use]
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
            regex_flag_errors: self.regex_flag_errors.clone(),
            scanner_diagnostics: self.scanner_diagnostics.clone(),
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
        self.regex_flag_errors = snapshot.regex_flag_errors;
        self.scanner_diagnostics = snapshot.scanner_diagnostics;
    }

    /// Get the interned atom for the current identifier token.
    /// Returns `Atom::NONE` if the current token is not an identifier.
    /// This enables O(1) string comparison for identifiers.
    #[must_use]
    pub const fn get_token_atom(&self) -> Atom {
        self.token_atom
    }

    #[must_use]
    pub const fn get_invalid_separator_pos(&self) -> Option<usize> {
        self.token_invalid_separator_pos
    }

    #[must_use]
    pub const fn invalid_separator_is_consecutive(&self) -> bool {
        self.token_invalid_separator_is_consecutive
    }

    /// Get the regex flag errors detected during scanning.
    #[must_use]
    pub fn get_regex_flag_errors(&self) -> &[RegexFlagError] {
        &self.regex_flag_errors
    }

    /// Get general scanner diagnostics (e.g., conflict marker errors).
    #[must_use]
    pub fn get_scanner_diagnostics(&self) -> &[ScannerDiagnostic] {
        &self.scanner_diagnostics
    }

    /// Merge conflict marker length (7 characters: `<<<<<<<`, `=======`, etc.)
    const MERGE_CONFLICT_MARKER_LENGTH: usize = 7;

    /// Check if the current position is a merge conflict marker.
    /// A conflict marker must be at the start of a line, consist of 7 identical
    /// characters (`<`, `=`, `>`, or `|`), and for non-`=` markers, be followed
    /// by a space.
    fn is_conflict_marker_trivia(&self) -> bool {
        let pos = self.pos;
        // Must be at start of line (pos == 0 or preceded by line break)
        if pos > 0 && !is_line_break(self.char_code_unchecked(pos - 1)) {
            return false;
        }
        // Must have room for 7 characters
        if pos + Self::MERGE_CONFLICT_MARKER_LENGTH >= self.end {
            return false;
        }
        let ch = self.char_code_unchecked(pos);
        // All 7 characters must be the same
        for i in 1..Self::MERGE_CONFLICT_MARKER_LENGTH {
            if self.char_code_unchecked(pos + i) != ch {
                return false;
            }
        }
        // For `=======`: no additional check needed
        // For `<<<<<<<`, `>>>>>>>`, `|||||||`: must be followed by a space
        ch == CharacterCodes::EQUALS
            || (pos + Self::MERGE_CONFLICT_MARKER_LENGTH < self.end
                && self.char_code_unchecked(pos + Self::MERGE_CONFLICT_MARKER_LENGTH)
                    == CharacterCodes::SPACE)
    }

    /// Scan past a conflict marker, emitting a TS1185 diagnostic.
    /// For `<` and `>` markers: skip to end of line.
    /// For `|` and `=` markers: skip until the next `=======` or `>>>>>>>` marker.
    fn scan_conflict_marker_trivia(&mut self) {
        // Emit TS1185: "Merge conflict marker encountered."
        self.scanner_diagnostics.push(ScannerDiagnostic {
            pos: self.pos,
            length: Self::MERGE_CONFLICT_MARKER_LENGTH,
            message: diagnostic_messages::MERGE_CONFLICT_MARKER_ENCOUNTERED,
            code: diagnostic_codes::MERGE_CONFLICT_MARKER_ENCOUNTERED,
        });

        let ch = self.char_code_unchecked(self.pos);
        if ch == CharacterCodes::LESS_THAN || ch == CharacterCodes::GREATER_THAN {
            // `<<<<<<<` or `>>>>>>>`: skip to end of line
            while self.pos < self.end && !is_line_break(self.char_code_unchecked(self.pos)) {
                self.pos += 1;
            }
        } else {
            // `|||||||` or `=======`: skip until next `=======` or `>>>>>>>` marker
            while self.pos < self.end {
                let current_char = self.char_code_unchecked(self.pos);
                if (current_char == CharacterCodes::EQUALS
                    || current_char == CharacterCodes::GREATER_THAN)
                    && current_char != ch
                    && self.is_conflict_marker_trivia()
                {
                    break;
                }
                self.pos += 1;
            }
        }
    }

    /// Resolve an atom back to its string value.
    /// Panics if the atom is invalid.
    #[must_use]
    pub fn resolve_atom(&self, atom: Atom) -> &str {
        self.interner.resolve(atom)
    }

    /// Get a reference to the interner for direct use by the parser.
    #[must_use]
    pub const fn interner(&self) -> &Interner {
        &self.interner
    }

    /// Get a mutable reference to the interner.
    pub const fn interner_mut(&mut self) -> &mut Interner {
        &mut self.interner
    }

    /// Take ownership of the interner, replacing it with a new empty one.
    /// Used to transfer the interner to `NodeArena` after parsing.
    pub fn take_interner(&mut self) -> Interner {
        std::mem::take(&mut self.interner)
    }

    /// ZERO-COPY: Get the current token value as a reference.
    /// For identifiers/keywords, returns the interned string.
    /// For other tokens, returns the `token_value` or raw source slice.
    /// This avoids allocation compared to `get_token_value()`.
    #[inline]
    #[must_use]
    pub fn get_token_value_ref(&self) -> &str {
        // 1. Fast path: Interned atom (identifiers, keywords)
        // When token_atom is set, we can always resolve from interner
        if self.token_atom != Atom::NONE {
            return self.interner.resolve(self.token_atom);
        }

        // 2. Processed value (strings with escapes, template literals, etc.)
        // For template literals and string literals, we must return token_value even if empty
        // to avoid returning the raw source slice with backticks/quotes
        if !self.token_value.is_empty()
            || super::token_is_template_literal(self.token)
            || self.token == SyntaxKind::StringLiteral
        {
            return &self.token_value;
        }

        // 3. Fallback: raw source slice (for identifiers, numbers, operators that match source)
        // This is the optimization - avoids redundant String allocations
        &self.source[self.token_start..self.pos]
    }

    /// ZERO-COPY: Get the raw token text directly from source.
    /// This is the unprocessed text from `token_start` to current pos.
    #[inline]
    #[must_use]
    pub fn get_token_text_ref(&self) -> &str {
        &self.source[self.token_start..self.pos]
    }

    /// ZERO-COPY: Get a slice of the source text by positions.
    #[inline]
    #[must_use]
    pub fn source_slice(&self, start: usize, end: usize) -> &str {
        &self.source[start..end]
    }

    /// Get the source text reference.
    #[inline]
    #[must_use]
    pub fn source_text(&self) -> &str {
        &self.source
    }
}

impl ScannerState {
    /// Get a cloned handle to the shared source text.
    #[inline]
    #[must_use]
    pub fn source_text_arc(&self) -> Arc<str> {
        std::sync::Arc::clone(&self.source)
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
        || ch == CharacterCodes::NEXT_LINE // U+0085 NEL (Next Line)
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

const fn is_binary_digit(ch: u32) -> bool {
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
    // Fast path for ASCII (0-127)
    if ch < 128 {
        return (CharacterCodes::UPPER_A..=CharacterCodes::UPPER_Z).contains(&ch)
            || (CharacterCodes::LOWER_A..=CharacterCodes::LOWER_Z).contains(&ch)
            || ch == CharacterCodes::UNDERSCORE
            || ch == CharacterCodes::DOLLAR;
    }

    // Unicode path: Use Rust's char::is_alphabetic() which covers:
    // Lu (Uppercase Letter), Ll (Lowercase Letter), Lt (Titlecase Letter),
    // Lm (Modifier Letter), Lo (Other Letter), Nl (Letter Number)
    // This correctly rejects U+00A0 (Whitespace), U+2026 (Punctuation), U+2194 (Symbol)
    if let Some(c) = char::from_u32(ch) {
        return c.is_alphabetic();
    }

    false
}

fn is_identifier_part(ch: u32) -> bool {
    // Fast path for ASCII
    if ch < 128 {
        return is_identifier_start(ch) || is_digit(ch);
    }

    // Unicode path: ECMAScript ID_Continue includes: ID_Start + Mn + Mc + Nd + Pc + ZWNJ + ZWJ
    // We use is_alphabetic() for letters (Lu, Ll, Lt, Lm, Lo, Nl) and a dedicated Nd check
    // for decimal digits. Note: is_alphanumeric() is too broad — it includes No (Number, other)
    // like subscript/superscript digits (U+2081 etc.) which are NOT valid in identifiers.
    if let Some(c) = char::from_u32(ch)
        && c.is_alphabetic()
    {
        return true;
    }
    if is_unicode_decimal_digit(ch) {
        return true;
    }

    // ZWNJ and ZWJ
    if ch == 0x200C || ch == 0x200D {
        return true;
    }

    // Unicode combining marks (Mn, Mc categories) - needed for scripts like Devanagari, Arabic, etc.
    // This covers the most common combining mark ranges used in identifiers:
    // - Combining Diacritical Marks (U+0300-U+036F)
    // - Devanagari combining marks (U+0900-U+097F range includes vowel signs and virama)
    // - Arabic combining marks (U+064B-U+0652)
    // - Hebrew combining marks (U+0591-U+05C7)
    // - And other Indic scripts
    is_unicode_combining_mark(ch)
}

/// Check if a character is a Unicode decimal digit (Nd category).
/// This covers digit characters from various scripts that are valid in
/// ECMAScript identifiers as `ID_Continue` characters.
/// Unlike `char::is_numeric()`, this excludes No (Number, other) like
/// subscript/superscript digits and Nl (Number, letter) like Roman numerals.
const fn is_unicode_decimal_digit(ch: u32) -> bool {
    // ASCII digits are handled by the fast path in is_identifier_part
    // This covers non-ASCII Nd ranges from Unicode
    matches!(ch,
        0x0660..=0x0669   // Arabic-Indic Digits
        | 0x06F0..=0x06F9 // Extended Arabic-Indic Digits
        | 0x07C0..=0x07C9 // NKo Digits
        | 0x0966..=0x096F // Devanagari Digits
        | 0x09E6..=0x09EF // Bengali Digits
        | 0x0A66..=0x0A6F // Gurmukhi Digits
        | 0x0AE6..=0x0AEF // Gujarati Digits
        | 0x0B66..=0x0B6F // Oriya Digits
        | 0x0BE6..=0x0BEF // Tamil Digits
        | 0x0C66..=0x0C6F // Telugu Digits
        | 0x0CE6..=0x0CEF // Kannada Digits
        | 0x0D66..=0x0D6F // Malayalam Digits
        | 0x0DE6..=0x0DEF // Sinhala Lith Digits
        | 0x0E50..=0x0E59 // Thai Digits
        | 0x0ED0..=0x0ED9 // Lao Digits
        | 0x0F20..=0x0F29 // Tibetan Digits
        | 0x1040..=0x1049 // Myanmar Digits
        | 0x1090..=0x1099 // Myanmar Shan Digits
        | 0x17E0..=0x17E9 // Khmer Digits
        | 0x1810..=0x1819 // Mongolian Digits
        | 0x1946..=0x194F // Limbu Digits
        | 0x19D0..=0x19D9 // New Tai Lue Digits
        | 0x1A80..=0x1A89 // Tai Tham Hora Digits
        | 0x1A90..=0x1A99 // Tai Tham Tham Digits
        | 0x1B50..=0x1B59 // Balinese Digits
        | 0x1BB0..=0x1BB9 // Sundanese Digits
        | 0x1C40..=0x1C49 // Lepcha Digits
        | 0x1C50..=0x1C59 // Ol Chiki Digits
        | 0xA620..=0xA629 // Vai Digits
        | 0xA8D0..=0xA8D9 // Saurashtra Digits
        | 0xA900..=0xA909 // Kayah Li Digits
        | 0xA9D0..=0xA9D9 // Javanese Digits
        | 0xA9F0..=0xA9F9 // Myanmar Tai Laing Digits
        | 0xAA50..=0xAA59 // Cham Digits
        | 0xABF0..=0xABF9 // Meetei Mayek Digits
        | 0xFF10..=0xFF19 // Fullwidth Digits
        | 0x104A0..=0x104A9 // Osmanya Digits
        | 0x10D30..=0x10D39 // Hanifi Rohingya Digits
        | 0x11066..=0x1106F // Brahmi Digits
        | 0x110F0..=0x110F9 // Sora Sompeng Digits
        | 0x11136..=0x1113F // Chakma Digits
        | 0x111D0..=0x111D9 // Sharada Digits
        | 0x112F0..=0x112F9 // Khudawadi Digits
        | 0x11450..=0x11459 // Newa Digits
        | 0x114D0..=0x114D9 // Tirhuta Digits
        | 0x11650..=0x11659 // Modi Digits
        | 0x116C0..=0x116C9 // Takri Digits
        | 0x11730..=0x11739 // Ahom Digits
        | 0x118E0..=0x118E9 // Warang Citi Digits
        | 0x11950..=0x11959 // Dives Akuru Digits
        | 0x11C50..=0x11C59 // Bhaiksuki Digits
        | 0x11D50..=0x11D59 // Masaram Gondi Digits
        | 0x11DA0..=0x11DA9 // Gunjala Gondi Digits
        | 0x11F50..=0x11F59 // Soyombo Digits
        | 0x16A60..=0x16A69 // Mro Digits
        | 0x16AC0..=0x16AC9 // Tangsa Digits
        | 0x16B50..=0x16B59 // Pahawh Hmong Digits
        | 0x1D7CE..=0x1D7FF // Mathematical Digits (all styles)
        | 0x1E140..=0x1E149 // Nyiakeng Puachue Hmong Digits
        | 0x1E2F0..=0x1E2F9 // Wancho Digits
        | 0x1E4F0..=0x1E4F9 // Nag Mundari Digits
        | 0x1E950..=0x1E959 // Adlam Digits
        | 0x1FBF0..=0x1FBF9 // Segmented Digit
    )
}

/// Check if a character is a Unicode combining mark (Mn or Mc category).
/// These are characters that modify the preceding base character.
fn is_unicode_combining_mark(ch: u32) -> bool {
    // Combining Diacritical Marks
    if (0x0300..=0x036F).contains(&ch) {
        return true;
    }
    // Devanagari vowel signs, virama, etc. (U+0900-U+0903, U+093A-U+094F, U+0951-U+0957, U+0962-U+0963)
    if (0x0900..=0x0903).contains(&ch)
        || (0x093A..=0x094F).contains(&ch)
        || (0x0951..=0x0957).contains(&ch)
        || (0x0962..=0x0963).contains(&ch)
    {
        return true;
    }
    // Bengali combining marks
    if (0x0981..=0x0983).contains(&ch) || (0x09BC..=0x09CD).contains(&ch) {
        return true;
    }
    // Arabic combining marks (tashkil/harakat)
    if (0x064B..=0x0652).contains(&ch) || (0x0670..=0x0670).contains(&ch) {
        return true;
    }
    // Hebrew combining marks
    if (0x0591..=0x05C7).contains(&ch) {
        return true;
    }
    // Other Indic scripts - Tamil, Telugu, Kannada, Malayalam, etc.
    if (0x0B01..=0x0B03).contains(&ch)  // Oriya
        || (0x0B3C..=0x0B4D).contains(&ch)
        || (0x0B82..=0x0B83).contains(&ch)  // Tamil
        || (0x0BBE..=0x0BCD).contains(&ch)
        || (0x0C00..=0x0C04).contains(&ch)  // Telugu
        || (0x0C3E..=0x0C4D).contains(&ch)
        || (0x0C81..=0x0C83).contains(&ch)  // Kannada
        || (0x0CBC..=0x0CCD).contains(&ch)
        || (0x0D00..=0x0D03).contains(&ch)  // Malayalam
        || (0x0D3B..=0x0D4D).contains(&ch)
    {
        return true;
    }
    // Thai and other Southeast Asian
    if (0x0E31..=0x0E3A).contains(&ch) || (0x0E47..=0x0E4E).contains(&ch) {
        return true;
    }
    // Combining Diacritical Marks Extended, Supplement, for Symbols
    if (0x1AB0..=0x1AFF).contains(&ch)
        || (0x1DC0..=0x1DFF).contains(&ch)
        || (0x20D0..=0x20FF).contains(&ch)
    {
        return true;
    }
    false
}

const fn is_line_break(ch: u32) -> bool {
    ch == CharacterCodes::LINE_FEED
        || ch == CharacterCodes::CARRIAGE_RETURN
        || ch == CharacterCodes::LINE_SEPARATOR
        || ch == CharacterCodes::PARAGRAPH_SEPARATOR
}

/// Check if a character is a valid regex flag (g, i, m, s, u, v, y, d)
const fn is_regex_flag(ch: u32) -> bool {
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

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a scanner that skips trivia and collect all tokens.
    fn scan_all(source: &str) -> Vec<(SyntaxKind, String)> {
        let mut scanner = ScannerState::new(source.to_string(), true);
        let mut tokens = Vec::new();
        loop {
            let kind = scanner.scan();
            if kind == SyntaxKind::EndOfFileToken {
                break;
            }
            tokens.push((kind, scanner.get_token_value()));
        }
        tokens
    }

    /// Helper: create a scanner that preserves trivia and collect all tokens.
    fn scan_all_with_trivia(source: &str) -> Vec<(SyntaxKind, String)> {
        let mut scanner = ScannerState::new(source.to_string(), false);
        let mut tokens = Vec::new();
        loop {
            let kind = scanner.scan();
            if kind == SyntaxKind::EndOfFileToken {
                break;
            }
            tokens.push((kind, scanner.get_token_text()));
        }
        tokens
    }

    // ── Empty input ───────────────────────────────────────────────────

    #[test]
    fn empty_input_returns_eof() {
        let mut scanner = ScannerState::new(String::new(), true);
        assert_eq!(scanner.scan(), SyntaxKind::EndOfFileToken);
    }

    // ── Identifiers ───────────────────────────────────────────────────

    #[test]
    fn scan_identifiers() {
        let tokens = scan_all("foo bar _baz $qux");
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0], (SyntaxKind::Identifier, "foo".to_string()));
        assert_eq!(tokens[1], (SyntaxKind::Identifier, "bar".to_string()));
        assert_eq!(tokens[2], (SyntaxKind::Identifier, "_baz".to_string()));
        assert_eq!(tokens[3], (SyntaxKind::Identifier, "$qux".to_string()));
    }

    // ── Keywords ──────────────────────────────────────────────────────

    #[test]
    fn scan_keywords() {
        let tokens = scan_all("if else while for const let var");
        assert_eq!(tokens.len(), 7);
        assert_eq!(tokens[0].0, SyntaxKind::IfKeyword);
        assert_eq!(tokens[1].0, SyntaxKind::ElseKeyword);
        assert_eq!(tokens[2].0, SyntaxKind::WhileKeyword);
        assert_eq!(tokens[3].0, SyntaxKind::ForKeyword);
        assert_eq!(tokens[4].0, SyntaxKind::ConstKeyword);
        assert_eq!(tokens[5].0, SyntaxKind::LetKeyword);
        assert_eq!(tokens[6].0, SyntaxKind::VarKeyword);
    }

    // ── Numeric literals ──────────────────────────────────────────────

    #[test]
    fn scan_numeric_literals() {
        let tokens = scan_all("0 42 3.14 0xFF 0b1010 0o777 1_000");
        assert_eq!(tokens.len(), 7);
        for (kind, _) in &tokens {
            assert_eq!(*kind, SyntaxKind::NumericLiteral);
        }
        assert_eq!(tokens[0].1, "0");
        assert_eq!(tokens[1].1, "42");
        assert_eq!(tokens[2].1, "3.14");
        assert_eq!(tokens[3].1, "0xFF");
        assert_eq!(tokens[4].1, "0b1010");
        assert_eq!(tokens[5].1, "0o777");
        assert_eq!(tokens[6].1, "1_000");
    }

    // ── String literals ───────────────────────────────────────────────

    #[test]
    fn scan_string_literals() {
        let tokens = scan_all(r#""hello" 'world'"#);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].0, SyntaxKind::StringLiteral);
        assert_eq!(tokens[0].1, "hello");
        assert_eq!(tokens[1].0, SyntaxKind::StringLiteral);
        assert_eq!(tokens[1].1, "world");
    }

    // ── Template literals ─────────────────────────────────────────────

    #[test]
    fn scan_no_substitution_template() {
        let tokens = scan_all("`hello world`");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(tokens[0].1, "hello world");
    }

    // ── Punctuation ───────────────────────────────────────────────────

    #[test]
    fn scan_punctuation() {
        let tokens = scan_all("{ } ( ) [ ] ; , . ...");
        let kinds: Vec<SyntaxKind> = tokens.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            kinds,
            vec![
                SyntaxKind::OpenBraceToken,
                SyntaxKind::CloseBraceToken,
                SyntaxKind::OpenParenToken,
                SyntaxKind::CloseParenToken,
                SyntaxKind::OpenBracketToken,
                SyntaxKind::CloseBracketToken,
                SyntaxKind::SemicolonToken,
                SyntaxKind::CommaToken,
                SyntaxKind::DotToken,
                SyntaxKind::DotDotDotToken,
            ]
        );
    }

    // ── Operators ─────────────────────────────────────────────────────

    #[test]
    fn scan_comparison_operators() {
        // Note: > is always scanned as GreaterThanToken; the parser calls
        // re_scan_greater_token() to disambiguate >= / >> / >>> etc.
        let tokens = scan_all("== != === !== < <=");
        let kinds: Vec<SyntaxKind> = tokens.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            kinds,
            vec![
                SyntaxKind::EqualsEqualsToken,
                SyntaxKind::ExclamationEqualsToken,
                SyntaxKind::EqualsEqualsEqualsToken,
                SyntaxKind::ExclamationEqualsEqualsToken,
                SyntaxKind::LessThanToken,
                SyntaxKind::LessThanEqualsToken,
            ]
        );
    }

    #[test]
    fn scan_greater_than_is_single_token() {
        // The scanner always produces GreaterThanToken for '>'.
        // Multi-char variants (>=, >>, >>>, >>=, >>>=) require re_scan_greater_token().
        let tokens = scan_all(">");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, SyntaxKind::GreaterThanToken);

        // ">=" is scanned as ">" then "=" by the raw scanner
        let tokens = scan_all(">=");
        assert_eq!(tokens[0].0, SyntaxKind::GreaterThanToken);
    }

    #[test]
    fn scan_assignment_operators() {
        // Exclude >>= and >>>= which need re_scan_greater_token from parser context
        let tokens = scan_all("= += -= *= **= /= %= <<= &= |= ^= ||= &&= ??=");
        let kinds: Vec<SyntaxKind> = tokens.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            kinds,
            vec![
                SyntaxKind::EqualsToken,
                SyntaxKind::PlusEqualsToken,
                SyntaxKind::MinusEqualsToken,
                SyntaxKind::AsteriskEqualsToken,
                SyntaxKind::AsteriskAsteriskEqualsToken,
                SyntaxKind::SlashEqualsToken,
                SyntaxKind::PercentEqualsToken,
                SyntaxKind::LessThanLessThanEqualsToken,
                SyntaxKind::AmpersandEqualsToken,
                SyntaxKind::BarEqualsToken,
                SyntaxKind::CaretEqualsToken,
                SyntaxKind::BarBarEqualsToken,
                SyntaxKind::AmpersandAmpersandEqualsToken,
                SyntaxKind::QuestionQuestionEqualsToken,
            ]
        );
    }

    #[test]
    fn scan_logical_operators() {
        let tokens = scan_all("&& || ?? !");
        let kinds: Vec<SyntaxKind> = tokens.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            kinds,
            vec![
                SyntaxKind::AmpersandAmpersandToken,
                SyntaxKind::BarBarToken,
                SyntaxKind::QuestionQuestionToken,
                SyntaxKind::ExclamationToken,
            ]
        );
    }

    #[test]
    fn scan_arrow_function() {
        let tokens = scan_all("=>");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, SyntaxKind::EqualsGreaterThanToken);
    }

    // ── Trivia handling ───────────────────────────────────────────────

    #[test]
    fn trivia_skip_mode() {
        let tokens = scan_all("  a  b  ");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].1, "a");
        assert_eq!(tokens[1].1, "b");
    }

    #[test]
    fn trivia_preserve_mode() {
        let tokens = scan_all_with_trivia(" a ");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].0, SyntaxKind::WhitespaceTrivia);
        assert_eq!(tokens[1].0, SyntaxKind::Identifier);
        assert_eq!(tokens[2].0, SyntaxKind::WhitespaceTrivia);
    }

    #[test]
    fn newline_trivia_preserved() {
        let tokens = scan_all_with_trivia("a\nb");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].0, SyntaxKind::Identifier);
        assert_eq!(tokens[1].0, SyntaxKind::NewLineTrivia);
        assert_eq!(tokens[2].0, SyntaxKind::Identifier);
    }

    // ── Comments ──────────────────────────────────────────────────────

    #[test]
    fn single_line_comment_skipped() {
        let tokens = scan_all("a // comment\nb");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].1, "a");
        assert_eq!(tokens[1].1, "b");
    }

    #[test]
    fn multi_line_comment_skipped() {
        let tokens = scan_all("a /* comment */ b");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].1, "a");
        assert_eq!(tokens[1].1, "b");
    }

    // ── Scanner state methods ─────────────────────────────────────────

    #[test]
    fn scanner_position_tracking() {
        let mut scanner = ScannerState::new("abc def".to_string(), true);
        scanner.scan(); // "abc"
        assert_eq!(scanner.get_token_start(), 0);
        assert_eq!(scanner.get_token_end(), 3);

        scanner.scan(); // "def"
        assert_eq!(scanner.get_token_start(), 4);
        assert_eq!(scanner.get_token_end(), 7);
    }

    #[test]
    fn scanner_set_text() {
        let mut scanner = ScannerState::new("abc".to_string(), true);
        scanner.scan();
        assert_eq!(scanner.get_token_value(), "abc");

        scanner.set_text("xyz".to_string(), None, None);
        scanner.scan();
        assert_eq!(scanner.get_token_value(), "xyz");
    }

    #[test]
    fn scanner_reset_token_state() {
        let mut scanner = ScannerState::new("ab cd".to_string(), true);
        scanner.scan(); // "ab"
        scanner.scan(); // "cd"
        scanner.reset_token_state(0);
        scanner.scan();
        assert_eq!(scanner.get_token_value(), "ab");
    }

    // ── Preceding line break ──────────────────────────────────────────

    #[test]
    fn preceding_line_break_detection() {
        let mut scanner = ScannerState::new("a\nb".to_string(), true);
        scanner.scan(); // "a"
        assert!(!scanner.has_preceding_line_break());
        scanner.scan(); // "b"
        assert!(scanner.has_preceding_line_break());
    }

    // ── BigInt literals ───────────────────────────────────────────────

    #[test]
    fn scan_bigint_literal() {
        let tokens = scan_all("42n 0xFFn");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].0, SyntaxKind::BigIntLiteral);
        assert_eq!(tokens[1].0, SyntaxKind::BigIntLiteral);
    }

    // ── Optional chaining ─────────────────────────────────────────────

    #[test]
    fn scan_optional_chaining() {
        let tokens = scan_all("a?.b");
        let kinds: Vec<SyntaxKind> = tokens.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            kinds,
            vec![
                SyntaxKind::Identifier,
                SyntaxKind::QuestionDotToken,
                SyntaxKind::Identifier,
            ]
        );
    }

    // ── Hash/private identifier ───────────────────────────────────────

    #[test]
    fn scan_private_identifier() {
        let tokens = scan_all("#field");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, SyntaxKind::PrivateIdentifier);
        assert_eq!(tokens[0].1, "#field");
    }

    // ── Helper function tests ─────────────────────────────────────────

    #[test]
    fn is_line_break_chars() {
        assert!(is_line_break(CharacterCodes::LINE_FEED));
        assert!(is_line_break(CharacterCodes::CARRIAGE_RETURN));
        assert!(is_line_break(CharacterCodes::LINE_SEPARATOR));
        assert!(is_line_break(CharacterCodes::PARAGRAPH_SEPARATOR));
        assert!(!is_line_break(CharacterCodes::SPACE));
        assert!(!is_line_break(CharacterCodes::TAB));
    }

    #[test]
    fn is_white_space_single_line_chars() {
        assert!(is_white_space_single_line(CharacterCodes::SPACE));
        assert!(is_white_space_single_line(CharacterCodes::TAB));
        assert!(is_white_space_single_line(CharacterCodes::FORM_FEED));
        assert!(is_white_space_single_line(
            CharacterCodes::NON_BREAKING_SPACE
        ));
        assert!(!is_white_space_single_line(CharacterCodes::LINE_FEED));
        assert!(!is_white_space_single_line(CharacterCodes::CARRIAGE_RETURN));
        assert!(!is_white_space_single_line(0x41)); // 'A'
    }

    #[test]
    fn is_identifier_start_chars() {
        assert!(is_identifier_start(CharacterCodes::LOWER_A));
        assert!(is_identifier_start(CharacterCodes::UPPER_Z));
        assert!(is_identifier_start(CharacterCodes::UNDERSCORE));
        assert!(is_identifier_start(CharacterCodes::DOLLAR));
        assert!(!is_identifier_start(CharacterCodes::_0));
        assert!(!is_identifier_start(CharacterCodes::SPACE));
        assert!(!is_identifier_start(CharacterCodes::PLUS));
    }

    #[test]
    fn is_identifier_part_rejects_subscript_digits() {
        // U+2081 SUBSCRIPT ONE is No (Number, other), NOT Nd — must be rejected
        assert!(!is_identifier_part(0x2081)); // ₁
        assert!(!is_identifier_part(0x2082)); // ₂
        assert!(!is_identifier_part(0x00B2)); // ² SUPERSCRIPT TWO (No)
        assert!(!is_identifier_part(0x00B3)); // ³ SUPERSCRIPT THREE (No)
        assert!(!is_identifier_part(0x00BC)); // ¼ VULGAR FRACTION ONE QUARTER (No)
        // Nd digits should be accepted
        assert!(is_identifier_part(0x0966)); // Devanagari digit zero (Nd)
        assert!(is_identifier_part(0x0660)); // Arabic-Indic digit zero (Nd)
        assert!(is_identifier_part(0xFF10)); // Fullwidth digit zero (Nd)
        // ASCII digits should be accepted
        assert!(is_identifier_part(0x30)); // '0'
        assert!(is_identifier_part(0x39)); // '9'
        // Letters should be accepted
        assert!(is_identifier_part(0x61)); // 'a'
    }

    #[test]
    fn is_digit_chars() {
        assert!(is_digit(CharacterCodes::_0));
        assert!(is_digit(CharacterCodes::_9));
        assert!(!is_digit(CharacterCodes::LOWER_A));
        assert!(!is_digit(CharacterCodes::SPACE));
    }

    #[test]
    fn is_regex_flag_chars() {
        assert!(is_regex_flag(CharacterCodes::LOWER_G));
        assert!(is_regex_flag(CharacterCodes::LOWER_I));
        assert!(is_regex_flag(CharacterCodes::LOWER_M));
        assert!(is_regex_flag(CharacterCodes::LOWER_S));
        assert!(is_regex_flag(CharacterCodes::LOWER_U));
        assert!(is_regex_flag(CharacterCodes::LOWER_V));
        assert!(is_regex_flag(CharacterCodes::LOWER_Y));
        assert!(is_regex_flag(CharacterCodes::LOWER_D));
        assert!(!is_regex_flag(CharacterCodes::LOWER_A));
        assert!(!is_regex_flag(CharacterCodes::LOWER_Z));
    }

    // ── Scanner snapshot/restore ──────────────────────────────────────

    #[test]
    fn scanner_snapshot_and_restore() {
        let mut scanner = ScannerState::new("a + b".to_string(), true);
        scanner.scan(); // "a"
        let snapshot = scanner.save_state();
        scanner.scan(); // "+"
        scanner.scan(); // "b"
        assert_eq!(scanner.get_token_value(), "b");
        scanner.restore_state(snapshot);
        assert_eq!(scanner.get_token(), SyntaxKind::Identifier);
        // After restoring, scanning again should give "+"
        let next = scanner.scan();
        assert_eq!(next, SyntaxKind::PlusToken);
    }

    // ── Rescan methods ────────────────────────────────────────────────

    #[test]
    fn rescan_greater_than_token() {
        // The scanner always scans ">" as GreaterThanToken.
        // re_scan_greater_token() is used by the parser to check if it could be >=, >>, etc.
        let mut scanner = ScannerState::new(">= x".to_string(), true);
        scanner.scan(); // scans ">"
        assert_eq!(scanner.get_token(), SyntaxKind::GreaterThanToken);

        // Rescan to get >=
        let rescanned = scanner.re_scan_greater_token();
        assert_eq!(rescanned, SyntaxKind::GreaterThanEqualsToken);
    }
}
