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
use tsz_common::interner::{AstAtom, IdentText, Interner};
use wasm_bindgen::prelude::wasm_bindgen;

mod diagnostics;
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
    pub token_atom: AstAtom,
    pub token_invalid_separator_pos: Option<usize>,
    pub token_invalid_separator_is_consecutive: bool,
    pub regex_flag_errors: Vec<RegexFlagError>,
    pub scanner_diagnostics_len: usize,
}

/// The scanner state that holds the current position and token information.
///
/// ZERO-COPY OPTIMIZATION: Source is stored as UTF-8 text directly (no Vec<char>).
/// Positions are byte-based internally; for ASCII-only files, byte position == character position.
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
    token_atom: AstAtom,
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
            token_atom: AstAtom::NONE,
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
        self.token_atom = AstAtom::NONE; // Reset atom for non-identifier tokens

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
                            // See https://github.com/tsz-org/tsz/issues/3331.
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
                            self.push_diag(
                                self.pos,
                                0,
                                diagnostic_messages::EXPECTED_2,
                                diagnostic_codes::EXPECTED_2,
                            );
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
            scanner_diagnostics_len: self.scanner_diagnostics.len(),
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
        self.scanner_diagnostics
            .truncate(snapshot.scanner_diagnostics_len);
    }

    /// Get the interned atom for the current identifier token.
    /// Returns `AstAtom::NONE` if the current token is not an identifier.
    /// This enables O(1) string comparison for identifiers.
    #[must_use]
    pub const fn get_token_atom(&self) -> AstAtom {
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

    /// Resolve an atom back to its string value.
    /// Panics if the atom is invalid.
    #[must_use]
    pub fn resolve_atom(&self, atom: AstAtom) -> &str {
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
        if self.token_atom != AstAtom::NONE {
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

    /// Get the current token's cooked text as a shared [`IdentText`] handle.
    ///
    /// For interned tokens (identifiers, keywords, no-escape string literals)
    /// this clones the interner's existing `Arc<str>` — a refcount bump, no
    /// allocation. Tokens without an atom (recovery paths, escaped values)
    /// are interned first so repeated occurrences still share one allocation.
    #[must_use]
    pub fn token_ident_text(&mut self) -> IdentText {
        if self.token_atom != AstAtom::NONE {
            return self.interner.resolve_text(self.token_atom);
        }
        let value = self.get_token_value_ref();
        if value.is_empty() {
            return IdentText::empty();
        }
        let value = value.to_string();
        let atom = self.interner.intern_owned(value);
        self.interner.resolve_text(atom)
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
        return unicode_ident::is_xid_start(c) || is_es_id_start_not_xid_start(ch);
    }

    false
}

pub(crate) fn is_identifier_part(ch: u32) -> bool {
    // Fast path for ASCII
    if ch < 128 {
        return is_identifier_start(ch) || is_digit(ch);
    }

    // Unicode path: ECMAScript `IdentifierPartChar` is `ID_Continue` plus
    // ZWNJ/ZWJ. The unicode-ident crate implements `XID_Continue`; every
    // `ID_Continue` code point that `XID_Continue` excludes is also in
    // `ID_Start - XID_Start`, so the same patch set restores exact
    // `ID_Continue` membership (verified by exhaustive sweep against tsc's
    // `unicodeESNextIdentifierPart` table).
    if let Some(c) = char::from_u32(ch)
        && (unicode_ident::is_xid_continue(c) || is_es_id_start_not_xid_start(ch))
    {
        return true;
    }

    // ZWNJ and ZWJ join controls (ES2024 12.7 IdentifierPartChar).
    if ch == 0x200C || ch == 0x200D {
        return true;
    }

    is_unicode_other_id_continue(ch)
}

/// ECMAScript `ID_Start` code points that Unicode `XID_Start` excludes.
///
/// ES2024 12.7 defines `UnicodeIDStart` with the Unicode `ID_Start`
/// property, while the unicode-ident crate implements UAX #31 `XID_Start`,
/// which removes `ID_Start` code points whose NFKC normalization is not
/// identifier-shaped (UAX #31 5.1). This is the complete difference set,
/// verified by an exhaustive code-point sweep against tsc's
/// `unicodeESNextIdentifierStart` table (`TypeScript/src/compiler/scanner.ts`,
/// generated from Unicode 15.1 `ID_Start`/`Other_ID_Start`).
///
/// Note: U+2118 SCRIPT CAPITAL P and U+212E ESTIMATED SYMBOL are also in
/// `Other_ID_Start` (UCD `PropList.txt`) but are NFKC-stable, so `XID_Start`
/// already contains them and they need no patch here.
const fn is_es_id_start_not_xid_start(ch: u32) -> bool {
    matches!(
        ch,
        0x037A // GREEK YPOGEGRAMMENI (Lm); NFKC: space + U+0345
            | 0x0E33 // THAI CHARACTER SARA AM (Lo); NFKC: U+0E4D U+0E32
            | 0x0EB3 // LAO VOWEL SIGN AM (Lo); NFKC: U+0ECD U+0EB2
            | 0x309B // KATAKANA-HIRAGANA VOICED SOUND MARK (Sk, Other_ID_Start)
            | 0x309C // KATAKANA-HIRAGANA SEMI-VOICED SOUND MARK (Sk, Other_ID_Start)
            | 0xFC5E..=0xFC63 // ARABIC LIGATURE ... ISOLATED FORM (Lo); NFKC: space + marks
            | 0xFDFA..=0xFDFB // ARABIC LIGATURE SALLALLAHOU/JALLAJALALOUHOU (Lo)
            | 0xFE70 // ARABIC FATHATAN ISOLATED FORM (Lo); NFKC: space + mark
            | 0xFE72 // ARABIC DAMMATAN ISOLATED FORM (Lo)
            | 0xFE74 // ARABIC KASRATAN ISOLATED FORM (Lo)
            | 0xFE76 // ARABIC FATHA ISOLATED FORM (Lo)
            | 0xFE78 // ARABIC DAMMA ISOLATED FORM (Lo)
            | 0xFE7A // ARABIC KASRA ISOLATED FORM (Lo)
            | 0xFE7C // ARABIC SHADDA ISOLATED FORM (Lo)
            | 0xFE7E // ARABIC SUKUN ISOLATED FORM (Lo)
            | 0xFF9E..=0xFF9F // HALFWIDTH KATAKANA (SEMI-)VOICED SOUND MARK (Lm); NFKC: U+3099/U+309A
    )
}

/// Unicode `Other_ID_Continue` code points (UCD `PropList.txt`) that
/// ECMAScript admits as identifier continuation characters even though they
/// are not alphabetic, decimal digits, join controls, or combining marks.
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
