//! Parser - Cache-optimized parser using NodeArena
//!
//! This parser uses the Node architecture (16 bytes per node vs 208 bytes)
//! for 13x better cache locality. It produces the same AST semantically
//! but stored in a more efficient format.
//!
//! # Architecture
//!
//! - Uses NodeArena instead of NodeArena
//! - Each node is 16 bytes (vs 208 bytes for fat Node enum)

//! - Node data is stored in separate typed pools
//! - 4 nodes fit per 64-byte cache line (vs 0.31 for fat nodes)

use crate::interner::Atom;
use crate::parser::{
    NodeIndex, NodeList,
    node::{IdentifierData, NodeArena},
    syntax_kind_ext,
};
use crate::scanner::SyntaxKind;
use crate::scanner_impl::ScannerState;
// =============================================================================
// Parser Context Flags
// =============================================================================

/// Context flag: inside an async function/method/arrow
pub(crate) const CONTEXT_FLAG_ASYNC: u32 = 1;
/// Context flag: inside a generator function/method
pub(crate) const CONTEXT_FLAG_GENERATOR: u32 = 2;
/// Context flag: inside a static block (where 'await' is reserved)
pub(crate) const CONTEXT_FLAG_STATIC_BLOCK: u32 = 4;
/// Context flag: parsing a parameter default (where 'await' is not allowed)
pub(crate) const CONTEXT_FLAG_PARAMETER_DEFAULT: u32 = 8;
/// Context flag: disallow 'in' as a binary operator (for for-statement initializers)
pub(crate) const CONTEXT_FLAG_DISALLOW_IN: u32 = 16;

// =============================================================================
// Parse Diagnostic
// =============================================================================

/// A parse-time diagnostic (error or warning).
#[derive(Clone, Debug)]
pub struct ParseDiagnostic {
    pub start: u32,
    pub length: u32,
    pub message: String,
    pub code: u32,
}

pub(crate) struct IncrementalParseResult {
    pub statements: NodeList,
    pub end_pos: u32,
    pub end_of_file_token: NodeIndex,
    pub reparse_start: u32,
}

// =============================================================================
// ParserState
// =============================================================================

/// A high-performance parser using Node architecture.
///
/// This parser produces the same AST semantically as ParserState,
/// but uses the cache-optimized NodeArena for storage.
pub struct ParserState {
    /// The scanner for tokenizing
    pub(crate) scanner: ScannerState,
    /// Arena for allocating Nodes
    pub arena: NodeArena,
    /// Source file name
    pub(crate) file_name: String,
    /// Parser context flags
    pub(crate) context_flags: u32,
    /// Current token
    pub(crate) current_token: SyntaxKind,
    /// List of parse diagnostics
    pub(crate) parse_diagnostics: Vec<ParseDiagnostic>,
    /// Node count for assigning IDs
    pub(crate) node_count: u32,
    /// Recursion depth for stack overflow protection
    pub(crate) recursion_depth: u32,
    /// Position of last error (to prevent cascading errors at same position)
    pub(crate) last_error_pos: u32,
}

impl ParserState {
    /// Create a new Parser for the given source text.
    pub fn new(file_name: String, source_text: String) -> ParserState {
        let estimated_nodes = source_text.len() / 20; // Rough estimate
        // Zero-copy: Pass source_text directly to scanner without cloning
        // This eliminates the 2x memory overhead from duplicating the source
        let scanner = ScannerState::new(source_text, true);
        ParserState {
            scanner,
            arena: NodeArena::with_capacity(estimated_nodes),
            file_name,
            context_flags: 0,
            current_token: SyntaxKind::Unknown,
            parse_diagnostics: Vec::new(),
            node_count: 0,
            recursion_depth: 0,
            last_error_pos: 0,
        }
    }

    pub fn reset(&mut self, file_name: String, source_text: String) {
        self.file_name = file_name;
        self.scanner.set_text(source_text, None, None);
        self.arena.clear();
        self.context_flags = 0;
        self.current_token = SyntaxKind::Unknown;
        self.parse_diagnostics.clear();
        self.node_count = 0;
        self.recursion_depth = 0;
        self.last_error_pos = 0;
    }

    /// Maximum recursion depth to prevent stack overflow on deeply nested code
    const MAX_RECURSION_DEPTH: u32 = 1000;

    /// Check recursion limit - returns true if we can continue, false if limit exceeded
    pub(crate) fn enter_recursion(&mut self) -> bool {
        self.recursion_depth += 1;
        if self.recursion_depth > Self::MAX_RECURSION_DEPTH {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Maximum recursion depth exceeded",
                diagnostic_codes::UNEXPECTED_TOKEN,
            );
            false
        } else {
            true
        }
    }

    /// Exit recursion scope
    pub(crate) fn exit_recursion(&mut self) {
        self.recursion_depth = self.recursion_depth.saturating_sub(1);
    }

    // =========================================================================
    // Token Utilities (shared with regular parser)
    // =========================================================================

    /// Check if we're in a JSX file (.tsx or .jsx)
    pub(crate) fn is_jsx_file(&self) -> bool {
        self.file_name.ends_with(".tsx") || self.file_name.ends_with(".jsx")
    }

    /// Get current token
    #[inline]
    pub(crate) fn token(&self) -> SyntaxKind {
        self.current_token
    }

    /// Get current token position
    #[inline]
    pub(crate) fn token_pos(&self) -> u32 {
        self.scanner.get_token_start() as u32
    }

    /// Get current token end position
    #[inline]
    pub(crate) fn token_end(&self) -> u32 {
        self.scanner.get_token_end() as u32
    }

    /// Advance to next token
    pub(crate) fn next_token(&mut self) -> SyntaxKind {
        self.current_token = self.scanner.scan();
        self.current_token
    }

    /// Check if current token matches kind
    #[inline]
    pub(crate) fn is_token(&self, kind: SyntaxKind) -> bool {
        self.current_token == kind
    }

    /// Check if current token is an identifier or any keyword
    /// Keywords can be used as identifiers in many contexts (e.g., class names, property names)
    #[inline]
    pub(crate) fn is_identifier_or_keyword(&self) -> bool {
        self.current_token as u16 >= SyntaxKind::Identifier as u16
    }

    /// Check if current token can be a property name
    /// Includes identifiers, keywords (as property names), string/numeric literals, computed properties
    #[inline]
    pub(crate) fn is_property_name(&self) -> bool {
        match self.current_token {
            SyntaxKind::Identifier
            | SyntaxKind::StringLiteral
            | SyntaxKind::NumericLiteral
            | SyntaxKind::PrivateIdentifier
            | SyntaxKind::OpenBracketToken // computed property name
            | SyntaxKind::GetKeyword
            | SyntaxKind::SetKeyword => true,
            // Any keyword can be used as a property name
            _ => self.is_identifier_or_keyword()
        }
    }

    /// Used to emit TS1110 (Type expected) instead of TS1005 (identifier expected)
    /// when a type is expected but we encounter a token that can't start a type
    #[inline]
    pub(crate) fn can_token_start_type(&self) -> bool {
        match self.current_token {
            // Tokens that definitely cannot start a type
            SyntaxKind::CloseParenToken       // )
            | SyntaxKind::CloseBraceToken     // }
            | SyntaxKind::CloseBracketToken   // ]
            | SyntaxKind::CommaToken          // ,
            | SyntaxKind::SemicolonToken      // ;
            | SyntaxKind::ColonToken          // :
            | SyntaxKind::EqualsToken         // =
            | SyntaxKind::EqualsGreaterThanToken  // =>
            | SyntaxKind::GreaterThanToken    // > (e.g., missing type in generic default: T = >)
            | SyntaxKind::BarToken            // | (when at start, not a union)
            | SyntaxKind::AmpersandToken      // & (when at start, not an intersection)
            | SyntaxKind::QuestionToken       // ?
            | SyntaxKind::EndOfFileToken => false,
            // Everything else could potentially start a type
            // (identifiers, keywords, literals, type operators, etc.)
            _ => true
        }
    }

    /// Check if we're inside an async function/method/arrow
    #[inline]
    pub(crate) fn in_async_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_ASYNC) != 0
    }

    /// Check if we're inside a generator function/method
    #[inline]
    pub(crate) fn in_generator_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_GENERATOR) != 0
    }

    /// Check if we're inside a static block
    #[inline]
    pub(crate) fn in_static_block_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_STATIC_BLOCK) != 0
    }

    /// Check if we're parsing a parameter default (where 'await' is not allowed)
    #[inline]
    pub(crate) fn in_parameter_default_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_PARAMETER_DEFAULT) != 0
    }

    /// Check if 'in' is disallowed as a binary operator (e.g., in for-statement initializers)
    #[inline]
    pub(crate) fn in_disallow_in_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_DISALLOW_IN) != 0
    }

    /// Check if the current token is an illegal binding identifier in the current context
    /// Returns true if illegal and emits appropriate diagnostic
    pub(crate) fn check_illegal_binding_identifier(&mut self) -> bool {
        use crate::checker::types::diagnostics::diagnostic_codes;

        // Check if current token is 'await' (either as keyword or identifier)
        let is_await = self.is_token(SyntaxKind::AwaitKeyword)
            || (self.is_token(SyntaxKind::Identifier)
                && self.scanner.get_token_value_ref() == "await");

        if is_await {
            // In static blocks, 'await' cannot be used as a binding identifier
            if self.in_static_block_context() {
                self.parse_error_at_current_token(
                    "Identifier expected. 'await' is a reserved word that cannot be used here.",
                    diagnostic_codes::AWAIT_IDENTIFIER_ILLEGAL,
                );
                return true;
            }

            // In async contexts, 'await' cannot be used as a binding identifier
            if self.in_async_context() {
                self.parse_error_at_current_token(
                    "Identifier expected. 'await' is a reserved word that cannot be used here.",
                    diagnostic_codes::AWAIT_IDENTIFIER_ILLEGAL,
                );
                return true;
            }
        }

        false
    }

    /// Parse optional token, returns true if found
    pub fn parse_optional(&mut self, kind: SyntaxKind) -> bool {
        if self.is_token(kind) {
            self.next_token();
            true
        } else {
            false
        }
    }

    /// Parse expected token, report error if not found
    /// Suppresses error if we already emitted an error at the current position
    /// (to prevent cascading errors from sequential parse_expected calls)
    pub fn parse_expected(&mut self, kind: SyntaxKind) -> bool {
        if self.is_token(kind) {
            self.next_token();
            true
        } else {
            // Special case: Force error emission for missing ) when we see {
            // This is a common error pattern that should always be reported
            let force_emit =
                kind == SyntaxKind::CloseParenToken && self.is_token(SyntaxKind::OpenBraceToken);

            // Only emit error if we haven't already emitted one at this position
            // This prevents cascading errors like "';' expected" followed by "')' expected"
            // when the real issue is a single missing token
            if force_emit || self.token_pos() != self.last_error_pos {
                // Additional check: suppress error for missing closing tokens when we're
                // at a clear statement boundary or EOF (reduces false-positive TS1005 errors)
                let should_suppress = if force_emit {
                    false // Never suppress forced errors
                } else {
                    match kind {
                        SyntaxKind::CloseBraceToken
                        | SyntaxKind::CloseParenToken
                        | SyntaxKind::CloseBracketToken => {
                            // At EOF, the file ended before this closing token. TypeScript reports
                            // these missing closing delimiters, so do not suppress at EOF.
                            if self.is_token(SyntaxKind::EndOfFileToken) {
                                false
                            }
                            // For closing parentheses, be more strict when we see { or if
                            // These are common cases of missing ) in parameters or conditions
                            else if kind == SyntaxKind::CloseParenToken
                                && (self.is_token(SyntaxKind::OpenBraceToken)
                                    || self.is_token(SyntaxKind::IfKeyword))
                            {
                                // If we're expecting ) but see { or if, this is likely a missing )
                                // Don't suppress - emit the error
                                false
                            }
                            // If next token starts a statement, the user has clearly moved on
                            // Don't complain about missing closing token
                            else if self.is_statement_start() {
                                true
                            }
                            // If there's a line break, give the user benefit of doubt
                            else {
                                self.scanner.has_preceding_line_break()
                            }
                        }
                        _ => false,
                    }
                };

                if !should_suppress {
                    // For forced errors, bypass the normal error budget logic
                    if force_emit {
                        use crate::checker::types::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            &format!("'{}' expected", Self::token_to_string(kind)),
                            diagnostic_codes::TOKEN_EXPECTED,
                        );
                    } else {
                        self.error_token_expected(Self::token_to_string(kind));
                    }
                }
            }
            false
        }
    }

    /// Convert SyntaxKind to human-readable token string
    pub(crate) fn token_to_string(kind: SyntaxKind) -> &'static str {
        match kind {
            SyntaxKind::OpenBraceToken => "{",
            SyntaxKind::CloseBraceToken => "}",
            SyntaxKind::OpenParenToken => "(",
            SyntaxKind::CloseParenToken => ")",
            SyntaxKind::OpenBracketToken => "[",
            SyntaxKind::CloseBracketToken => "]",
            SyntaxKind::SemicolonToken => ";",
            SyntaxKind::CommaToken => ",",
            SyntaxKind::ColonToken => ":",
            SyntaxKind::DotToken => ".",
            SyntaxKind::EqualsToken => "=",
            SyntaxKind::GreaterThanToken => ">",
            SyntaxKind::LessThanToken => "<",
            SyntaxKind::QuestionToken => "?",
            SyntaxKind::ExclamationToken => "!",
            SyntaxKind::AtToken => "@",
            SyntaxKind::AmpersandToken => "&",
            SyntaxKind::BarToken => "|",
            SyntaxKind::PlusToken => "+",
            SyntaxKind::MinusToken => "-",
            SyntaxKind::AsteriskToken => "*",
            SyntaxKind::SlashToken => "/",
            SyntaxKind::EqualsGreaterThanToken => "=>",
            SyntaxKind::DotDotDotToken => "...",
            SyntaxKind::Identifier => "identifier",
            _ => "token",
        }
    }

    pub(crate) fn parse_error_at(&mut self, start: u32, length: u32, message: &str, code: u32) {
        // Track the position of this error to prevent cascading errors at same position
        self.last_error_pos = start;
        self.parse_diagnostics.push(ParseDiagnostic {
            start,
            length,
            message: message.to_string(),
            code,
        });
    }

    /// Report parse error at current token with specific error code
    pub fn parse_error_at_current_token(&mut self, message: &str, code: u32) {
        let start = self.scanner.get_token_start() as u32;
        let end = self.scanner.get_token_end() as u32;
        self.parse_error_at(start, end - start, message, code);
    }

    // =========================================================================
    // Typed error helper methods (use these instead of parse_error_at_current_token)
    // =========================================================================

    /// Error: Expression expected (TS1109)
    pub(crate) fn error_expression_expected(&mut self) {
        // Only emit error if we haven't already emitted one at this position
        // This prevents cascading TS1109 errors when TS1005 or other errors already reported
        if self.token_pos() != self.last_error_pos {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Expression expected",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
        }
    }

    /// Error: Type expected (TS1110)
    pub(crate) fn error_type_expected(&mut self) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token("Type expected", diagnostic_codes::TYPE_EXPECTED);
    }

    /// Error: Identifier expected (TS1003)
    pub(crate) fn error_identifier_expected(&mut self) {
        // Only emit error if we haven't already emitted one at this position
        // This prevents cascading errors when a missing token causes identifier to be expected
        if self.token_pos() != self.last_error_pos {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Identifier expected",
                diagnostic_codes::IDENTIFIER_EXPECTED,
            );
        }
    }

    /// Error: '{token}' expected (TS1005)
    pub(crate) fn error_token_expected(&mut self, token: &str) {
        // Only emit error if we haven't already emitted one at this position
        // This prevents cascading errors when parse_semicolon() and similar functions call this
        if self.token_pos() != self.last_error_pos {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                &format!("'{}' expected", token),
                diagnostic_codes::TOKEN_EXPECTED,
            );
        }
    }

    /// Error: Comma expected (TS1005) - specifically for missing commas between parameters/arguments
    pub(crate) fn error_comma_expected(&mut self) {
        self.error_token_expected(",");
    }

    /// Check if current token could start a parameter
    pub(crate) fn is_parameter_start(&self) -> bool {
        // Parameters can start with modifiers, identifiers, or binding patterns
        self.is_parameter_modifier()
            || self.is_token(SyntaxKind::AtToken) // decorators on parameters
            || self.is_token(SyntaxKind::DotDotDotToken) // rest parameter
            || self.is_identifier_or_keyword()
            || self.is_token(SyntaxKind::OpenBraceToken) // object binding pattern
            || self.is_token(SyntaxKind::OpenBracketToken) // array binding pattern
    }

    /// Error: Unterminated template literal (TS1160)
    pub(crate) fn error_unterminated_template_literal_at(&mut self, start: u32, end: u32) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        let length = end.saturating_sub(start).max(1);
        self.parse_error_at(
            start,
            length,
            "Unterminated template literal.",
            diagnostic_codes::UNTERMINATED_TEMPLATE_LITERAL,
        );
    }

    /// Error: Declaration expected (TS1146)
    pub(crate) fn error_declaration_expected(&mut self) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token(
            "Declaration expected",
            diagnostic_codes::DECLARATION_EXPECTED,
        );
    }

    /// Error: Statement expected (TS1129)
    pub(crate) fn error_statement_expected(&mut self) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token(
            "Statement expected",
            diagnostic_codes::STATEMENT_EXPECTED,
        );
    }

    /// Check if a statement is a using/await using declaration not inside a block (TS1156)
    pub(crate) fn check_using_outside_block(&mut self, statement: NodeIndex) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::parser::node_flags;

        if statement.is_none() {
            return;
        }

        // Get the node and check if it's a variable statement with using flags
        if let Some(node) = self.arena.get(statement) {
            // Check if it's a variable statement (not a block)
            if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                // Check if it has using or await using flags
                let is_using =
                    (node.flags & (node_flags::USING as u16 | node_flags::AWAIT_USING as u16)) != 0;
                if is_using {
                    // Emit TS1156 error at the statement position
                    self.parse_error_at(
                        node.pos,
                        node.end.saturating_sub(node.pos).max(1),
                        diagnostic_messages::USING_DECLARATION_ONLY_IN_BLOCK,
                        diagnostic_codes::USING_DECLARATION_ONLY_IN_BLOCK,
                    );
                }
            }
        }
    }

    /// Error: Unexpected token (TS1012)
    pub(crate) fn error_unexpected_token(&mut self) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token("Unexpected token", diagnostic_codes::UNEXPECTED_TOKEN);
    }

    /// Parse semicolon (or recover from missing)
    pub(crate) fn parse_semicolon(&mut self) {
        if self.is_token(SyntaxKind::SemicolonToken) {
            self.next_token();
        } else if !self.can_parse_semicolon() {
            self.error_token_expected(";");
        }
    }

    /// Check if we can parse a semicolon (ASI rules)
    /// Returns true if current token is semicolon or ASI applies
    ///
    /// ASI (Automatic Semicolon Insertion) rules (matching TypeScript):
    /// 1. Explicit semicolon
    /// 2. Before closing brace
    /// 3. At EOF
    /// 4. After line break (no additional checks!)
    ///
    /// Note: This matches TypeScript's canParseSemicolon() implementation exactly.
    /// The previous "enhanced" ASI with statement start checks was causing
    /// false-positive TS1005 errors because it was more restrictive than TypeScript.
    pub(crate) fn can_parse_semicolon(&self) -> bool {
        // Explicit semicolon
        if self.is_token(SyntaxKind::SemicolonToken) {
            return true;
        }

        // ASI applies before closing brace
        if self.is_token(SyntaxKind::CloseBraceToken) {
            return true;
        }

        // ASI applies at EOF
        if self.is_token(SyntaxKind::EndOfFileToken) {
            return true;
        }

        // ASI applies after line break (matching TypeScript - no extra checks!)
        self.scanner.has_preceding_line_break()
    }

    /// Check if ASI applies for restricted productions (return, throw, yield, break, continue)
    ///
    /// Restricted productions have special ASI rules:
    /// ASI applies immediately after a line break, WITHOUT checking if the next token starts a statement.
    ///
    /// Examples:
    /// - `return\nx` parses as `return; x;` (ASI applies due to line break)
    /// - `return x` parses as `return x;` (no ASI, x is the return value)
    /// - `throw\nx` parses as `throw; x;` (ASI applies due to line break)
    /// - `throw x` parses as `throw x;` (no ASI, x is the thrown value)
    pub(crate) fn can_parse_semicolon_for_restricted_production(&self) -> bool {
        // Explicit semicolon
        if self.is_token(SyntaxKind::SemicolonToken) {
            return true;
        }

        // ASI applies before closing brace
        if self.is_token(SyntaxKind::CloseBraceToken) {
            return true;
        }

        // ASI applies at EOF
        if self.is_token(SyntaxKind::EndOfFileToken) {
            return true;
        }

        // ASI applies after line break (without checking statement start)
        // This is the key difference from can_parse_semicolon()
        if self.scanner.has_preceding_line_break() {
            return true;
        }

        false
    }

    // =========================================================================
    // Error Resynchronization
    // =========================================================================

    /// Check if we're at a position where an expression can reasonably end
    /// This is used to suppress spurious TS1109 "expression expected" errors when
    /// the user has clearly moved on to the next statement/context.
    ///
    /// For TS1109 (expression expected), we should only suppress if we've reached a closing
    /// delimiter or EOF. We should NOT suppress on statement start keywords because if we're
    /// expecting an expression and see `var`, `let`, `function`, etc., that's likely an error.
    pub(crate) fn is_at_expression_end(&self) -> bool {
        match self.token() {
            // Only tokens that naturally end expressions and indicate we've moved on
            SyntaxKind::SemicolonToken
            | SyntaxKind::CloseBraceToken
            | SyntaxKind::CloseParenToken
            | SyntaxKind::CloseBracketToken
            | SyntaxKind::EndOfFileToken => true,
            // NOTE: We do NOT suppress on statement start keywords
            // If we're expecting an expression and see `var`, `let`, `function`, etc.,
            // that's likely a genuine error where the user forgot the expression.
            // This fixes the "missing TS1109" issue where errors were being suppressed too aggressively.
            _ => false,
        }
    }

    /// Check if current token can start a statement (synchronization point)
    pub(crate) fn is_statement_start(&self) -> bool {
        match self.token() {
            // Keywords that start statements
            SyntaxKind::VarKeyword
            | SyntaxKind::LetKeyword
            | SyntaxKind::ConstKeyword
            | SyntaxKind::FunctionKeyword
            | SyntaxKind::ClassKeyword
            | SyntaxKind::IfKeyword
            | SyntaxKind::ForKeyword
            | SyntaxKind::WhileKeyword
            | SyntaxKind::DoKeyword
            | SyntaxKind::SwitchKeyword
            | SyntaxKind::TryKeyword
            | SyntaxKind::WithKeyword
            | SyntaxKind::DebuggerKeyword
            | SyntaxKind::ReturnKeyword
            | SyntaxKind::BreakKeyword
            | SyntaxKind::ContinueKeyword
            | SyntaxKind::ThrowKeyword
            | SyntaxKind::AsyncKeyword
            | SyntaxKind::InterfaceKeyword
            | SyntaxKind::TypeKeyword
            | SyntaxKind::EnumKeyword
            | SyntaxKind::NamespaceKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::ImportKeyword
            | SyntaxKind::ExportKeyword
            | SyntaxKind::DeclareKeyword => true,
            // Identifiers, string literals, and decorators can start statements
            SyntaxKind::Identifier
            | SyntaxKind::StringLiteral
            | SyntaxKind::AtToken => true,
            // Expression literals that can start statements (enhanced ASI support)
            SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::NullKeyword
            | SyntaxKind::ThisKeyword
            | SyntaxKind::SuperKeyword => true,
            // Prefix operators that can start expression statements
            SyntaxKind::ExclamationToken  // !
            | SyntaxKind::TildeToken  // ~
            | SyntaxKind::PlusToken  // + (unary)
            | SyntaxKind::MinusToken  // - (unary)
            | SyntaxKind::PlusPlusToken  // ++ (prefix)
            | SyntaxKind::MinusMinusToken  // -- (prefix)
            | SyntaxKind::TypeOfKeyword
            | SyntaxKind::VoidKeyword
            | SyntaxKind::DeleteKeyword => true,
            // Structural tokens that can start statements
            SyntaxKind::OpenBraceToken  // block
            | SyntaxKind::SemicolonToken  // empty statement
            | SyntaxKind::OpenParenToken  // parenthesized expression
            | SyntaxKind::OpenBracketToken  // array literal/destructuring
            | SyntaxKind::LessThanToken => true,  // JSX/type argument
            _ => false,
        }
    }

    /// Check if current token is a synchronization point for error recovery
    /// This includes statement starts plus additional keywords that indicate
    /// boundaries in control structures (else, case, default, catch, finally, etc.)
    pub(crate) fn is_resync_sync_point(&self) -> bool {
        if self.is_statement_start() {
            return true;
        }

        // Additional sync points that indicate statement boundaries in control structures
        match self.token() {
            // Control structure boundaries
            SyntaxKind::ElseKeyword => true, // if statement alternative
            SyntaxKind::CaseKeyword | SyntaxKind::DefaultKeyword => true, // switch cases
            SyntaxKind::CatchKeyword | SyntaxKind::FinallyKeyword => true, // try-catch-finally
            // Comma can be a sync point in declaration lists and object/array literals
            SyntaxKind::CommaToken => true,
            _ => false,
        }
    }

    /// Resynchronize after a parse error by skipping to the next statement boundary
    /// This prevents cascading errors by finding a known good synchronization point
    pub(crate) fn resync_after_error(&mut self) {
        // If we're already at a sync point or EOF, no need to resync
        if self.is_resync_sync_point() || self.is_token(SyntaxKind::EndOfFileToken) {
            return;
        }

        // Skip tokens until we find a synchronization point
        let mut brace_depth = 0u32;
        let mut paren_depth = 0u32;
        let mut bracket_depth = 0u32;
        let max_iterations = 1000; // Prevent infinite loops

        for _ in 0..max_iterations {
            // Check for EOF
            if self.is_token(SyntaxKind::EndOfFileToken) {
                break;
            }

            // Track nesting depth to handle nested structures
            match self.token() {
                SyntaxKind::OpenBraceToken => {
                    brace_depth += 1;
                    self.next_token();
                    continue;
                }
                SyntaxKind::CloseBraceToken => {
                    if brace_depth > 0 {
                        brace_depth -= 1;
                        self.next_token();
                        continue;
                    }
                    // Found closing brace at same level - this is a sync point
                    self.next_token();
                    break;
                }
                SyntaxKind::OpenParenToken => {
                    paren_depth += 1;
                    self.next_token();
                    continue;
                }
                SyntaxKind::CloseParenToken => {
                    if paren_depth > 0 {
                        paren_depth -= 1;
                        self.next_token();
                        continue;
                    }
                    // Found closing paren at same level - could be end of expression
                    // Skip it and check if next token is a sync point
                    self.next_token();
                    if self.is_resync_sync_point() {
                        break;
                    }
                    continue;
                }
                SyntaxKind::OpenBracketToken => {
                    bracket_depth += 1;
                    self.next_token();
                    continue;
                }
                SyntaxKind::CloseBracketToken => {
                    if bracket_depth > 0 {
                        bracket_depth -= 1;
                        self.next_token();
                        continue;
                    }
                    // Found closing bracket at same level - skip it
                    self.next_token();
                    continue;
                }
                SyntaxKind::SemicolonToken => {
                    // Semicolon is always a sync point (even in nested contexts)
                    self.next_token();
                    break;
                }
                _ => {}
            }

            // If we're at depth 0 and found a sync point, we've resync'd
            if brace_depth == 0
                && paren_depth == 0
                && bracket_depth == 0
                && self.is_resync_sync_point()
            {
                break;
            }

            // Keep skipping tokens
            self.next_token();
        }
    }

    // =========================================================================
    // Expression-Level Error Recovery
    // =========================================================================

    /// Check if current token can start an expression
    pub(crate) fn is_expression_start(&self) -> bool {
        match self.token() {
            // Literals
            SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::StringLiteral
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TemplateHead
            | SyntaxKind::TemplateMiddle
            | SyntaxKind::TemplateTail
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::NullKeyword => true,
            // Identifiers and expression keywords
            SyntaxKind::Identifier
            | SyntaxKind::ThisKeyword
            | SyntaxKind::SuperKeyword
            | SyntaxKind::ImportKeyword
            | SyntaxKind::TypeKeyword
            | SyntaxKind::AnyKeyword
            | SyntaxKind::StringKeyword
            | SyntaxKind::NumberKeyword
            | SyntaxKind::BooleanKeyword
            | SyntaxKind::SymbolKeyword
            | SyntaxKind::BigIntKeyword
            | SyntaxKind::ObjectKeyword
            | SyntaxKind::NeverKeyword
            | SyntaxKind::UnknownKeyword
            | SyntaxKind::UndefinedKeyword
            | SyntaxKind::RequireKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::NamespaceKeyword
            | SyntaxKind::AsyncKeyword
            | SyntaxKind::AwaitKeyword
            | SyntaxKind::YieldKeyword
            | SyntaxKind::NewKeyword
            | SyntaxKind::ClassKeyword
            | SyntaxKind::FunctionKeyword
            | SyntaxKind::DeleteKeyword
            | SyntaxKind::VoidKeyword
            | SyntaxKind::InstanceOfKeyword => true,
            // Unary operators
            SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::TildeToken
            | SyntaxKind::ExclamationToken
            | SyntaxKind::PlusPlusToken
            | SyntaxKind::MinusMinusToken => true,
            // Open parentheses/brackets/angle brackets
            SyntaxKind::OpenParenToken
            | SyntaxKind::OpenBracketToken
            | SyntaxKind::LessThanToken => true,
            // Decorators
            SyntaxKind::AtToken => true,
            _ => false,
        }
    }

    /// Check if current token is a binary operator
    pub(crate) fn is_binary_operator(&self) -> bool {
        let precedence = self.get_operator_precedence(self.token());
        precedence > 0
    }

    /// Resynchronize to next expression boundary after parse error
    pub(crate) fn resync_to_next_expression_boundary(&mut self) {
        let max_iterations = 100;
        for _ in 0..max_iterations {
            if self.is_token(SyntaxKind::EndOfFileToken) {
                break;
            }
            if self.is_expression_boundary() {
                break;
            }
            if self.is_binary_operator() {
                break;
            }
            if self.is_expression_start() {
                break;
            }
            self.next_token();
        }
    }

    /// Check if current token is at an expression boundary (a natural stopping point)
    pub(crate) fn is_expression_boundary(&self) -> bool {
        matches!(
            self.token(),
            SyntaxKind::SemicolonToken
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::CloseParenToken
                | SyntaxKind::CloseBracketToken
                | SyntaxKind::CommaToken
                | SyntaxKind::ColonToken
                | SyntaxKind::CaseKeyword
                | SyntaxKind::DefaultKeyword
                | SyntaxKind::ElseKeyword
                | SyntaxKind::WhileKeyword // for do-while
                | SyntaxKind::AsKeyword
                | SyntaxKind::SatisfiesKeyword
        )
    }

    /// Create a missing expression placeholder for error recovery.
    /// This allows the AST to remain structurally valid even when an expression is missing.
    pub(crate) fn create_missing_expression(&mut self) -> NodeIndex {
        let pos = self.token_pos();
        // Create an identifier with empty text to represent missing expression
        self.arena.add_identifier(
            SyntaxKind::Identifier as u16,
            pos,
            pos,
            IdentifierData {
                atom: Atom::NONE,
                escaped_text: String::new(),
                original_text: None,
                type_arguments: None,
            },
        )
    }

    /// Try to recover from a missing right-hand operand in a binary expression.
    /// Returns a placeholder expression if recovery is possible.
    pub(crate) fn try_recover_binary_rhs(&mut self) -> NodeIndex {
        // If we're at an expression boundary after an operator, create a placeholder
        if self.is_expression_boundary() || self.is_token(SyntaxKind::EndOfFileToken) {
            self.create_missing_expression()
        } else {
            NodeIndex::NONE
        }
    }

    /// Try to rescan `>` as a compound token (`>>`, `>>>`, `>=`, `>>=`, `>>>=`)
    /// Returns the rescanned token (which may be unchanged if no compound token found)
    pub(crate) fn try_rescan_greater_token(&mut self) -> SyntaxKind {
        if self.current_token == SyntaxKind::GreaterThanToken {
            self.current_token = self.scanner.re_scan_greater_token();
        }
        self.current_token
    }

    /// Parse expected `>` token, handling compound tokens like `>>` and `>>>`
    /// When we have `>>`, we need to consume just one `>` and leave `>` for the next parse
    pub(crate) fn parse_expected_greater_than(&mut self) {
        match self.current_token {
            SyntaxKind::GreaterThanToken => {
                // Simple case - just consume the single `>`
                self.next_token();
            }
            SyntaxKind::GreaterThanGreaterThanToken => {
                // `>>` - back up scanner and treat as single `>`
                // After consuming, the remaining `>` becomes the current token
                self.scanner.set_pos(self.scanner.get_pos() - 1);
                self.current_token = SyntaxKind::GreaterThanToken;
            }
            SyntaxKind::GreaterThanGreaterThanGreaterThanToken => {
                // `>>>` - back up scanner and treat as single `>`
                // After consuming, the remaining `>>` becomes the current token
                self.scanner.set_pos(self.scanner.get_pos() - 2);
                self.current_token = SyntaxKind::GreaterThanGreaterThanToken;
            }
            SyntaxKind::GreaterThanEqualsToken => {
                // `>=` - back up scanner and treat as single `>`
                self.scanner.set_pos(self.scanner.get_pos() - 1);
                self.current_token = SyntaxKind::EqualsToken;
            }
            SyntaxKind::GreaterThanGreaterThanEqualsToken => {
                // `>>=` - back up scanner and treat as single `>`
                self.scanner.set_pos(self.scanner.get_pos() - 2);
                self.current_token = SyntaxKind::GreaterThanEqualsToken;
            }
            SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken => {
                // `>>>=` - back up scanner and treat as single `>`
                self.scanner.set_pos(self.scanner.get_pos() - 3);
                self.current_token = SyntaxKind::GreaterThanGreaterThanEqualsToken;
            }
            _ => {
                // Only emit error if we haven't already emitted one at this position
                if self.token_pos() != self.last_error_pos {
                    self.error_token_expected(">");
                }
            }
        }
    }

    /// Create a NodeList from a Vec of NodeIndex
    pub(crate) fn make_node_list(&self, nodes: Vec<NodeIndex>) -> NodeList {
        NodeList {
            nodes,
            pos: 0,
            end: 0,
            has_trailing_comma: false,
        }
    }

    /// Get operator precedence
    pub(crate) fn get_operator_precedence(&self, token: SyntaxKind) -> u8 {
        match token {
            SyntaxKind::CommaToken => 1,
            SyntaxKind::EqualsToken
            | SyntaxKind::PlusEqualsToken
            | SyntaxKind::MinusEqualsToken
            | SyntaxKind::AsteriskEqualsToken
            | SyntaxKind::AsteriskAsteriskEqualsToken
            | SyntaxKind::SlashEqualsToken
            | SyntaxKind::PercentEqualsToken
            | SyntaxKind::LessThanLessThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
            | SyntaxKind::AmpersandEqualsToken
            | SyntaxKind::BarEqualsToken
            | SyntaxKind::BarBarEqualsToken
            | SyntaxKind::AmpersandAmpersandEqualsToken
            | SyntaxKind::QuestionQuestionEqualsToken
            | SyntaxKind::CaretEqualsToken => 2,
            SyntaxKind::QuestionToken => 3,
            SyntaxKind::BarBarToken | SyntaxKind::QuestionQuestionToken => 4,
            SyntaxKind::AmpersandAmpersandToken => 5,
            SyntaxKind::BarToken => 6,
            SyntaxKind::CaretToken => 7,
            SyntaxKind::AmpersandToken => 8,
            SyntaxKind::EqualsEqualsToken
            | SyntaxKind::ExclamationEqualsToken
            | SyntaxKind::EqualsEqualsEqualsToken
            | SyntaxKind::ExclamationEqualsEqualsToken => 9,
            // 'in' is not a binary operator in for-statement initializers
            SyntaxKind::InKeyword => {
                if self.in_disallow_in_context() {
                    0
                } else {
                    10
                }
            }
            SyntaxKind::LessThanToken
            | SyntaxKind::GreaterThanToken
            | SyntaxKind::LessThanEqualsToken
            | SyntaxKind::GreaterThanEqualsToken
            | SyntaxKind::InstanceOfKeyword
            | SyntaxKind::AsKeyword
            | SyntaxKind::SatisfiesKeyword => 10,
            SyntaxKind::LessThanLessThanToken
            | SyntaxKind::GreaterThanGreaterThanToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanToken => 11,
            SyntaxKind::PlusToken | SyntaxKind::MinusToken => 12,
            SyntaxKind::AsteriskToken | SyntaxKind::SlashToken | SyntaxKind::PercentToken => 13,
            SyntaxKind::AsteriskAsteriskToken => 14,
            _ => 0,
        }
    }
}
