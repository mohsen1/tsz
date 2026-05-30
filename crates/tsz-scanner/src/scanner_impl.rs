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
use tsz_common::ScriptTarget;
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
use tsz_common::interner::{Atom, Interner};
use wasm_bindgen::prelude::wasm_bindgen;

mod identifiers;
mod jsdoc;
mod jsx;
mod numbers;
mod slash;
mod strings;
mod templates;

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
    /// Diagnostic message template (may contain `{0}`, `{1}` placeholders)
    pub message: &'static str,
    /// Diagnostic code
    pub code: u32,
    /// Arguments to substitute into the message template
    pub args: Vec<String>,
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
    pub(crate) pos: usize,
    /// End byte position
    pub(crate) end: usize,
    /// Full start position including leading trivia (byte offset)
    full_start_pos: usize,
    /// Token start position (excluding trivia, byte offset)
    pub(crate) token_start: usize,
    /// Current token kind
    pub(crate) token: SyntaxKind,
    /// Current token's string value
    pub(crate) token_value: String,
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
    /// Whether identifier scanning should admit non-BMP code points.
    allow_astral_identifier_chars: bool,
    /// Whether to skip trivia (whitespace, comments)
    skip_trivia: bool,
    /// String interner for identifier deduplication
    #[wasm_bindgen(skip)]
    pub interner: Interner,
    /// Interned atom for current identifier token (avoids string comparison)
    token_atom: Atom,
}

// `#[wasm_bindgen]` forbids `const fn`; suppress the lint for this impl block only.
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
            allow_astral_identifier_chars: true,
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
    pub(crate) fn char_code_unchecked(&self, index: usize) -> u32 {
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
    pub(crate) fn char_code_at(&self, index: usize) -> Option<u32> {
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
    pub(crate) fn char_len_at(&self, index: usize) -> usize {
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
    pub(crate) fn substring(&self, start: usize, end: usize) -> String {
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
                            // Single-line comments are terminated by any of
                            // tsc's line-terminator characters: LF, CR,
                            // U+2028, U+2029. Without U+2028/U+2029 the
                            // comment would swallow the next source line.
                            // See https://github.com/mohsen1/tsz/issues/3331.
                            if c == CharacterCodes::LINE_FEED
                                || c == CharacterCodes::CARRIAGE_RETURN
                                || c == CharacterCodes::LINE_SEPARATOR
                                || c == CharacterCodes::PARAGRAPH_SEPARATOR
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
                            if is_line_break(c) {
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
                                args: Vec::new(),
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
                    self.pos += 1;
                    if self.pos < self.end
                        && self.is_identifier_start(self.char_code_unchecked(self.pos))
                    {
                        self.pos += self.char_len_at(self.pos); // Handle multi-byte UTF-8
                        // Check for unicode escapes in the continuation
                        let has_escapes = self.scan_private_identifier_rest();
                        if has_escapes {
                            // token_value was set by scan_private_identifier_rest
                        } else {
                            self.token_value = self.substring(self.token_start, self.pos);
                        }
                        self.token = SyntaxKind::PrivateIdentifier;
                    } else if self.pos < self.end
                        && self.char_code_unchecked(self.pos) == CharacterCodes::BACKSLASH
                    {
                        // Private identifier starting with unicode escape: #\u0078
                        if let Some(code_point) = self.peek_unicode_escape()
                            && self.is_identifier_start(code_point)
                        {
                            self.scan_private_identifier_with_escapes();
                        } else {
                            self.token = SyntaxKind::HashToken;
                        }
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
                        && self.is_identifier_start(code_point)
                    {
                        self.scan_identifier_with_escapes();
                        return self.token;
                    }
                    if let Some(code_point) = escaped_ch {
                        if !self.allow_astral_identifier_chars
                            && code_point > 0xFFFF
                            && self
                                .source
                                .as_bytes()
                                .get(self.pos + 2)
                                .is_some_and(|&b| b == b'{')
                        {
                            self.pos += 1;
                            self.token = SyntaxKind::Unknown;
                            return self.token;
                        }
                        let _ = self.scan_unicode_escape_value();
                        self.token = SyntaxKind::Unknown;
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
                    if self.is_identifier_start(ch) {
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
}

// =============================================================================
// Non-wasm methods for internal use
// =============================================================================

impl ScannerState {
    /// Set the ECMAScript language version used by target-sensitive scanning.
    pub const fn set_language_version(&mut self, language_version: ScriptTarget) {
        self.allow_astral_identifier_chars = language_version.supports_es2015();
    }

    #[inline]
    pub(crate) fn is_identifier_start(&self, ch: u32) -> bool {
        (self.allow_astral_identifier_chars || ch <= 0xFFFF) && is_identifier_start(ch)
    }

    #[inline]
    pub(crate) fn is_identifier_part(&self, ch: u32) -> bool {
        (self.allow_astral_identifier_chars || ch <= 0xFFFF) && is_identifier_part(ch)
    }

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

    /// Clear accumulated scanner diagnostics. Used by `ParserState::reset` so a
    /// reused parser doesn't carry stale scanner-side errors into a new parse.
    /// `set_text` does NOT clear them — callers like the LSP that re-text the
    /// scanner across edits without going through `ParserState` may want the
    /// previous diagnostics to remain accessible.
    pub fn clear_scanner_diagnostics(&mut self) {
        self.scanner_diagnostics.clear();
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
            args: Vec::new(),
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

pub(crate) fn is_digit(ch: u32) -> bool {
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

pub(crate) fn is_identifier_start(ch: u32) -> bool {
    // Fast path for ASCII (0-127)
    if ch < 128 {
        return (CharacterCodes::UPPER_A..=CharacterCodes::UPPER_Z).contains(&ch)
            || (CharacterCodes::LOWER_A..=CharacterCodes::LOWER_Z).contains(&ch)
            || ch == CharacterCodes::UNDERSCORE
            || ch == CharacterCodes::DOLLAR;
    }

    if let Some(c) = char::from_u32(ch) {
        return unicode_ident::is_xid_start(c);
    }

    false
}

pub(crate) fn is_identifier_part(ch: u32) -> bool {
    // Fast path for ASCII
    if ch < 128 {
        return is_identifier_start(ch) || is_digit(ch);
    }

    // Unicode path: ECMAScript ID_Continue includes ID_Start plus marks,
    // decimal digits, connector punctuation, ZWNJ, and ZWJ. The unicode-ident
    // table keeps astral-plane identifier characters valid without admitting
    // `No` digits such as subscript/superscript numerals.
    if let Some(c) = char::from_u32(ch)
        && unicode_ident::is_xid_continue(c)
    {
        return true;
    }

    // ZWNJ and ZWJ
    if ch == 0x200C || ch == 0x200D {
        return true;
    }

    if is_unicode_other_id_continue(ch) {
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

/// Unicode `Other_ID_Continue` code points that ECMAScript admits as
/// identifier continuation characters even though they are not alphabetic,
/// decimal digits, join controls, or combining marks.
const fn is_unicode_other_id_continue(ch: u32) -> bool {
    matches!(
        ch,
        0x00B7 // MIDDLE DOT
            | 0x0387 // GREEK ANO TELEIA
            | 0x1369
            ..=0x1371 // ETHIOPIC DIGIT ONE..THREE
            | 0x19DA // NEW TAI LUE THAM DIGIT ONE
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
mod tests;
