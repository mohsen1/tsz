//! ThinParser - Cache-optimized parser using ThinNodeArena
//!
//! This parser uses the ThinNode architecture (16 bytes per node vs 208 bytes)
//! for 13x better cache locality. It produces the same AST semantically
//! but stored in a more efficient format.
//!
//! # Architecture
//!
//! - Uses ThinNodeArena instead of NodeArena
//! - Each node is 16 bytes (vs 208 bytes for fat Node enum)

//! - Node data is stored in separate typed pools
//! - 4 nodes fit per 64-byte cache line (vs 0.31 for fat nodes)

use crate::parser::{
    NodeIndex, NodeList, node_flags, syntax_kind_ext,
    thin_node::{
        AccessExprData, BinaryExprData, BlockData, CallExprData, CaseClauseData, CatchClauseData,
        ClassData, ConditionalExprData, EnumData, EnumMemberData, ExportAssignmentData,
        ExportDeclData, ExprStatementData, FunctionData, IdentifierData, IfStatementData,
        ImportClauseData, ImportDeclData, LabeledData, LiteralData, LiteralExprData, LoopData,
        NamedImportsData, ParameterData, ParenthesizedData, QualifiedNameData, ReturnData,
        SourceFileData, SpecifierData, SwitchData, TaggedTemplateData, TemplateExprData,
        TemplateSpanData, ThinNodeArena, TryData, TypeAssertionData, UnaryExprData,
        UnaryExprDataEx, VariableData, VariableDeclarationData,
    },
};
use crate::scanner::SyntaxKind;
use crate::scanner_impl::{ScannerState, TokenFlags};
// =============================================================================
// Parser Context Flags
// =============================================================================

/// Context flag: inside an async function/method/arrow
const CONTEXT_FLAG_ASYNC: u32 = 1;
/// Context flag: inside a generator function/method
const CONTEXT_FLAG_GENERATOR: u32 = 2;
/// Context flag: inside a static block (where 'await' is reserved)
const CONTEXT_FLAG_STATIC_BLOCK: u32 = 4;
/// Context flag: parsing a parameter default (where 'await' is not allowed)
const CONTEXT_FLAG_PARAMETER_DEFAULT: u32 = 8;

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
// ThinParserState
// =============================================================================

/// A high-performance parser using ThinNode architecture.
///
/// This parser produces the same AST semantically as ParserState,
/// but uses the cache-optimized ThinNodeArena for storage.
pub struct ThinParserState {
    /// The scanner for tokenizing
    scanner: ScannerState,
    /// Arena for allocating ThinNodes
    pub arena: ThinNodeArena,
    /// Source file name
    file_name: String,
    /// Parser context flags
    context_flags: u32,
    /// Current token
    current_token: SyntaxKind,
    /// List of parse diagnostics
    parse_diagnostics: Vec<ParseDiagnostic>,
    /// Node count for assigning IDs
    node_count: u32,
    /// Recursion depth for stack overflow protection
    recursion_depth: u32,
    /// Position of last error (to prevent cascading errors at same position)
    last_error_pos: u32,
}

impl ThinParserState {
    /// Create a new ThinParser for the given source text.
    pub fn new(file_name: String, source_text: String) -> ThinParserState {
        let estimated_nodes = source_text.len() / 20; // Rough estimate
        // Zero-copy: Pass source_text directly to scanner without cloning
        // This eliminates the 2x memory overhead from duplicating the source
        let scanner = ScannerState::new(source_text, true);
        ThinParserState {
            scanner,
            arena: ThinNodeArena::with_capacity(estimated_nodes),
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
    fn enter_recursion(&mut self) -> bool {
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
    fn exit_recursion(&mut self) {
        self.recursion_depth = self.recursion_depth.saturating_sub(1);
    }

    // =========================================================================
    // Token Utilities (shared with regular parser)
    // =========================================================================

    /// Check if we're in a JSX file (.tsx or .jsx)
    fn is_jsx_file(&self) -> bool {
        self.file_name.ends_with(".tsx") || self.file_name.ends_with(".jsx")
    }

    /// Get current token
    #[inline]
    fn token(&self) -> SyntaxKind {
        self.current_token
    }

    /// Get current token position
    #[inline]
    fn token_pos(&self) -> u32 {
        self.scanner.get_token_start() as u32
    }

    /// Get current token end position
    #[inline]
    fn token_end(&self) -> u32 {
        self.scanner.get_token_end() as u32
    }

    /// Advance to next token
    fn next_token(&mut self) -> SyntaxKind {
        self.current_token = self.scanner.scan();
        self.current_token
    }

    /// Check if current token matches kind
    #[inline]
    fn is_token(&self, kind: SyntaxKind) -> bool {
        self.current_token == kind
    }

    /// Check if current token is an identifier or any keyword
    /// Keywords can be used as identifiers in many contexts (e.g., class names, property names)
    #[inline]
    fn is_identifier_or_keyword(&self) -> bool {
        self.current_token as u16 >= SyntaxKind::Identifier as u16
    }

    /// Check if current token can be a property name
    /// Includes identifiers, keywords (as property names), string/numeric literals, computed properties
    #[inline]
    fn is_property_name(&self) -> bool {
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
    fn can_token_start_type(&self) -> bool {
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
    fn in_async_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_ASYNC) != 0
    }

    /// Check if we're inside a static block
    #[inline]
    fn in_static_block_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_STATIC_BLOCK) != 0
    }

    /// Check if we're parsing a parameter default (where 'await' is not allowed)
    #[inline]
    fn in_parameter_default_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_PARAMETER_DEFAULT) != 0
    }

    /// Check if the current token is an illegal binding identifier in the current context
    /// Returns true if illegal and emits appropriate diagnostic
    fn check_illegal_binding_identifier(&mut self) -> bool {
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
                            else if self.scanner.has_preceding_line_break() {
                                true
                            } else {
                                false
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
    fn token_to_string(kind: SyntaxKind) -> &'static str {
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

    fn parse_error_at(&mut self, start: u32, length: u32, message: &str, code: u32) {
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
    fn error_expression_expected(&mut self) {
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
    fn error_type_expected(&mut self) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token("Type expected", diagnostic_codes::TYPE_EXPECTED);
    }

    /// Error: Identifier expected (TS1003)
    fn error_identifier_expected(&mut self) {
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
    fn error_token_expected(&mut self, token: &str) {
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
    fn error_comma_expected(&mut self) {
        self.error_token_expected(",");
    }

    /// Check if current token could start a parameter
    fn is_parameter_start(&self) -> bool {
        // Parameters can start with modifiers, identifiers, or binding patterns
        self.is_parameter_modifier()
            || self.is_token(SyntaxKind::AtToken) // decorators on parameters
            || self.is_token(SyntaxKind::DotDotDotToken) // rest parameter
            || self.is_identifier_or_keyword()
            || self.is_token(SyntaxKind::OpenBraceToken) // object binding pattern
            || self.is_token(SyntaxKind::OpenBracketToken) // array binding pattern
    }

    /// Error: Unterminated template literal (TS1160)
    fn error_unterminated_template_literal_at(&mut self, start: u32, end: u32) {
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
    fn error_declaration_expected(&mut self) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token(
            "Declaration expected",
            diagnostic_codes::DECLARATION_EXPECTED,
        );
    }

    /// Error: Statement expected (TS1129)
    fn error_statement_expected(&mut self) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token(
            "Statement expected",
            diagnostic_codes::STATEMENT_EXPECTED,
        );
    }

    /// Error: Unexpected token (TS1012)
    fn error_unexpected_token(&mut self) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token("Unexpected token", diagnostic_codes::UNEXPECTED_TOKEN);
    }

    /// Error: 'async' modifier cannot be used here. (TS1042)
    fn error_async_modifier_cannot_be_used_here(&mut self) {
        use crate::checker::types::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token(
            "'async' modifier cannot be used here.",
            diagnostic_codes::ASYNC_MODIFIER_CANNOT_BE_USED_HERE,
        );
    }

    /// Parse semicolon (or recover from missing)
    fn parse_semicolon(&mut self) {
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
    fn can_parse_semicolon(&self) -> bool {
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
    fn can_parse_semicolon_for_restricted_production(&self) -> bool {
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
    fn is_at_expression_end(&self) -> bool {
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
    fn is_statement_start(&self) -> bool {
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
    fn is_resync_sync_point(&self) -> bool {
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
    fn resync_after_error(&mut self) {
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
    fn is_expression_start(&self) -> bool {
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
    fn is_binary_operator(&self) -> bool {
        let precedence = self.get_operator_precedence(self.token());
        precedence > 0
    }

    /// Resynchronize to next expression boundary after parse error
    fn resync_to_next_expression_boundary(&mut self) {
        let max_iterations = 100;
        for _ in 0..max_iterations {
            if self.is_token(SyntaxKind::EndOfFileToken) {
                break;
            }
            if self.is_token(SyntaxKind::SemicolonToken)
                || self.is_token(SyntaxKind::CloseBraceToken)
                || self.is_token(SyntaxKind::CloseParenToken)
                || self.is_token(SyntaxKind::CloseBracketToken)
                || self.is_token(SyntaxKind::CaseKeyword)
                || self.is_token(SyntaxKind::DefaultKeyword)
            {
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

    /// Try to rescan `>` as a compound token (`>>`, `>>>`, `>=`, `>>=`, `>>>=`)
    /// Returns the rescanned token (which may be unchanged if no compound token found)
    fn try_rescan_greater_token(&mut self) -> SyntaxKind {
        if self.current_token == SyntaxKind::GreaterThanToken {
            self.current_token = self.scanner.re_scan_greater_token();
        }
        self.current_token
    }

    /// Parse expected `>` token, handling compound tokens like `>>` and `>>>`
    /// When we have `>>`, we need to consume just one `>` and leave `>` for the next parse
    fn parse_expected_greater_than(&mut self) {
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
    fn make_node_list(&self, nodes: Vec<NodeIndex>) -> NodeList {
        NodeList {
            nodes,
            pos: 0,
            end: 0,
            has_trailing_comma: false,
        }
    }

    /// Get operator precedence
    fn get_operator_precedence(&self, token: SyntaxKind) -> u8 {
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
            SyntaxKind::LessThanToken
            | SyntaxKind::GreaterThanToken
            | SyntaxKind::LessThanEqualsToken
            | SyntaxKind::GreaterThanEqualsToken
            | SyntaxKind::InstanceOfKeyword
            | SyntaxKind::InKeyword
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

    // =========================================================================
    // Parse Methods - Core Expressions
    // =========================================================================

    /// Parse a source file
    pub fn parse_source_file(&mut self) -> NodeIndex {
        let start_pos = 0u32;

        // Initialize scanner
        self.next_token();

        // Parse statements (using source file version that handles stray braces)
        let statements = self.parse_source_file_statements();

        // Cache comment ranges once during parsing (O(N) scan, done only once)
        // This avoids rescanning on every hover/documentation request
        // Use scanner's source text (no duplicate allocation)
        let comments = crate::comments::get_comment_ranges(self.scanner.source_text());

        // Create source file node
        let end_pos = self.token_end();
        let eof_token = self
            .arena
            .add_token(SyntaxKind::EndOfFileToken as u16, end_pos, end_pos);

        self.arena.add_source_file(
            start_pos,
            end_pos,
            SourceFileData {
                statements,
                end_of_file_token: eof_token,
                file_name: self.file_name.clone(),
                text: self.scanner.source_text_arc(),
                language_version: 99,
                language_variant: 0,
                script_kind: 3,
                is_declaration_file: false,
                has_no_default_lib: false,
                comments, // Cached comment ranges
                parent: NodeIndex::NONE,
                id: 0,
                modifier_flags: 0,
                transform_flags: 0,
            },
        )
    }

    pub(crate) fn parse_source_file_statements_from_offset(
        &mut self,
        file_name: String,
        source_text: String,
        start: u32,
    ) -> IncrementalParseResult {
        let start = usize::min(start as usize, source_text.len());
        let reparse_start = start as u32;

        self.file_name = file_name;
        self.scanner.set_text(source_text, Some(start), None);
        self.context_flags = 0;
        self.current_token = SyntaxKind::Unknown;
        self.parse_diagnostics.clear();
        self.recursion_depth = 0;

        self.next_token();
        let statements = self.parse_source_file_statements();
        let end_pos = self.token_end();
        let eof_token = self
            .arena
            .add_token(SyntaxKind::EndOfFileToken as u16, end_pos, end_pos);

        IncrementalParseResult {
            statements,
            end_pos,
            end_of_file_token: eof_token,
            reparse_start,
        }
    }

    /// Parse list of statements for a source file (top-level).
    /// Reports error 1128 for unexpected closing braces.
    /// Uses resynchronization to recover from errors and continue parsing.
    fn parse_source_file_statements(&mut self) -> NodeList {
        let mut statements = Vec::new();

        while !self.is_token(SyntaxKind::EndOfFileToken) {
            // If we see a closing brace at the top level, report error 1128
            if self.is_token(SyntaxKind::CloseBraceToken) {
                // Only emit error if we haven't already emitted one at this position
                if self.token_pos() != self.last_error_pos {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                }
                self.next_token();
                // Resync to next statement boundary
                self.resync_after_error();
                continue;
            }

            let stmt = self.parse_statement();
            if !stmt.is_none() {
                statements.push(stmt);
            } else {
                // Statement parsing failed, resync to recover
                // Emit error for unexpected token if we haven't already
                if self.token_pos() != self.last_error_pos
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                }
                // Resync to next statement boundary to continue parsing
                self.resync_after_error();
            }

            // Safety: break on Unknown tokens to avoid infinite loop
            if self.is_token(SyntaxKind::Unknown) {
                break;
            }
        }

        self.make_node_list(statements)
    }

    /// Parse list of statements (for blocks, function bodies, etc.).
    /// Stops at closing brace without error (closing brace is expected).
    /// Uses resynchronization to recover from errors and continue parsing.
    fn parse_statements(&mut self) -> NodeList {
        let mut statements = Vec::new();

        while !self.is_token(SyntaxKind::EndOfFileToken)
            && !self.is_token(SyntaxKind::CloseBraceToken)
        {
            let stmt = self.parse_statement();
            if !stmt.is_none() {
                statements.push(stmt);
            } else {
                // Statement parsing failed, resync to recover
                // Emit error for unexpected token if we haven't already
                if self.token_pos() != self.last_error_pos
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                }
                // Resync to next statement boundary to continue parsing
                self.resync_after_error();
            }

            // Safety: break on Unknown tokens to avoid infinite loop
            if self.is_token(SyntaxKind::Unknown) {
                break;
            }
        }

        self.make_node_list(statements)
    }

    /// Parse a statement
    pub fn parse_statement(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::OpenBraceToken => self.parse_block(),
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword => self.parse_variable_statement(),
            SyntaxKind::ConstKeyword => {
                // const enum or const variable
                if self.look_ahead_is_const_enum() {
                    let start_pos = self.token_pos();
                    self.parse_const_enum_declaration(start_pos, Vec::new())
                } else {
                    self.parse_variable_statement()
                }
            }
            SyntaxKind::FunctionKeyword => self.parse_function_declaration(),
            SyntaxKind::AsyncKeyword => {
                // async function declaration or async arrow expression statement
                // Look ahead to see if it's "async function"
                if self.look_ahead_is_async_function() {
                    self.parse_async_function_declaration()
                } else if self.look_ahead_is_async_declaration() {
                    let start_pos = self.token_pos();
                    self.error_async_modifier_cannot_be_used_here();
                    let async_start = self.token_pos();
                    self.parse_expected(SyntaxKind::AsyncKeyword);
                    let async_end = self.token_end();
                    let async_modifier = self.arena.add_token(
                        SyntaxKind::AsyncKeyword as u16,
                        async_start,
                        async_end,
                    );
                    let modifiers = Some(self.make_node_list(vec![async_modifier]));
                    match self.token() {
                        SyntaxKind::ClassKeyword => {
                            self.parse_class_declaration_with_modifiers(start_pos, modifiers)
                        }
                        SyntaxKind::EnumKeyword => {
                            self.parse_enum_declaration_with_modifiers(start_pos, modifiers)
                        }
                        SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(),
                        SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                            if self.look_ahead_is_module_declaration() {
                                self.parse_module_declaration()
                            } else {
                                self.parse_expression_statement()
                            }
                        }
                        _ => self.parse_expression_statement(),
                    }
                } else {
                    // It's an async arrow function as expression statement
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::AtToken => {
                // Decorator: @decorator class/function
                self.parse_decorated_declaration()
            }
            SyntaxKind::ClassKeyword => self.parse_class_declaration(),
            SyntaxKind::AbstractKeyword => {
                // abstract class declaration
                if self.look_ahead_is_abstract_class() {
                    self.parse_abstract_class_declaration()
                } else if self.look_ahead_is_abstract_declaration() {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Modifiers cannot appear here.",
                        diagnostic_codes::MODIFIERS_NOT_ALLOWED_HERE,
                    );
                    self.next_token();
                    match self.token() {
                        SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(),
                        SyntaxKind::EnumKeyword => self.parse_enum_declaration(),
                        SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                            if self.look_ahead_is_module_declaration() {
                                self.parse_module_declaration()
                            } else {
                                self.parse_expression_statement()
                            }
                        }
                        _ => self.parse_expression_statement(),
                    }
                } else {
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::AccessorKeyword => {
                if self.look_ahead_is_accessor_declaration() {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Modifiers cannot appear here.",
                        diagnostic_codes::MODIFIERS_NOT_ALLOWED_HERE,
                    );
                    self.next_token();
                    self.parse_statement()
                } else {
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::DefaultKeyword => {
                self.error_unexpected_token();
                self.next_token();
                self.parse_statement()
            }
            SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(),
            SyntaxKind::TypeKeyword => {
                if self.look_ahead_is_type_alias_declaration() {
                    self.parse_type_alias_declaration()
                } else {
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::EnumKeyword => self.parse_enum_declaration(),
            SyntaxKind::DeclareKeyword => self.parse_ambient_declaration(),
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                if self.look_ahead_is_module_declaration() {
                    self.parse_module_declaration()
                } else {
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::IfKeyword => self.parse_if_statement(),
            SyntaxKind::ReturnKeyword => self.parse_return_statement(),
            SyntaxKind::WhileKeyword => self.parse_while_statement(),
            SyntaxKind::ForKeyword => self.parse_for_statement(),
            SyntaxKind::SemicolonToken => self.parse_empty_statement(),
            SyntaxKind::ExportKeyword => self.parse_export_declaration(),
            SyntaxKind::ImportKeyword => {
                // Check for dynamic import: import(...)
                if self.look_ahead_is_import_call() {
                    self.parse_expression_statement()
                // Check for import = (import equals declaration)
                } else if self.look_ahead_is_import_equals() {
                    self.parse_import_equals_declaration()
                } else {
                    self.parse_import_declaration()
                }
            }
            SyntaxKind::BreakKeyword => self.parse_break_statement(),
            SyntaxKind::ContinueKeyword => self.parse_continue_statement(),
            SyntaxKind::ThrowKeyword => self.parse_throw_statement(),
            SyntaxKind::DoKeyword => self.parse_do_statement(),
            SyntaxKind::SwitchKeyword => self.parse_switch_statement(),
            SyntaxKind::TryKeyword => self.parse_try_statement(),
            SyntaxKind::WithKeyword => self.parse_with_statement(),
            SyntaxKind::DebuggerKeyword => self.parse_debugger_statement(),
            SyntaxKind::Identifier => {
                // Check for labeled statement: label: statement
                if self.look_ahead_is_labeled_statement() {
                    self.parse_labeled_statement()
                } else {
                    self.parse_expression_statement()
                }
            }
            _ => {
                // Check for labeled statement with keyword as label (e.g., await: if (...))
                // TypeScript/JavaScript allow reserved keywords as labels
                // This enables: await: ..., arguments: ..., eval: ..., etc.
                if self.is_identifier_or_keyword() && self.look_ahead_is_labeled_statement() {
                    self.parse_labeled_statement()
                } else {
                    self.parse_expression_statement()
                }
            }
        }
    }

    /// Look ahead to see if we have "async function"
    fn look_ahead_is_async_function(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip 'async'
        self.next_token();
        let is_function = self.is_token(SyntaxKind::FunctionKeyword);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_function
    }

    /// Look ahead to see if "async" is followed by a declaration keyword.
    fn look_ahead_is_async_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip 'async'
        self.next_token();
        let is_decl = matches!(
            self.token(),
            SyntaxKind::ClassKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
        );

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_decl
    }

    /// Look ahead to see if we have "abstract class"
    fn look_ahead_is_abstract_class(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip 'abstract'
        self.next_token();
        let is_class = self.is_token(SyntaxKind::ClassKeyword);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_class
    }

    /// Look ahead to see if "abstract" is followed by another declaration keyword.
    fn look_ahead_is_abstract_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip 'abstract'
        let is_decl = matches!(
            self.token(),
            SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
        );

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_decl
    }

    /// Look ahead to see if "accessor" is followed by a declaration keyword.
    fn look_ahead_is_accessor_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip 'accessor'
        let is_decl = matches!(
            self.token(),
            SyntaxKind::ClassKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::VarKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::TypeKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::ImportKeyword
                | SyntaxKind::ExportKeyword
        );

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_decl
    }

    /// Look ahead to see if we have "import identifier ="
    fn look_ahead_is_import_equals(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip 'import'
        self.next_token();
        // Check for identifier or keyword that can be used as identifier (require, exports, etc.)
        if !self.is_identifier_or_keyword() {
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return false;
        }
        // Skip identifier/keyword
        self.next_token();
        // Check for '='
        let is_equals = self.is_token(SyntaxKind::EqualsToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_equals
    }

    /// Look ahead to see if we have "import (" (dynamic import call)
    fn look_ahead_is_import_call(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip 'import'
        self.next_token();
        // Check for '(' or '.' (import.meta)
        let is_call =
            self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::DotToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_call
    }

    /// Look ahead to see if "namespace"/"module" starts a declaration.
    /// Updated to recognize anonymous modules: module { ... }
    fn look_ahead_is_module_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip namespace/module
        let is_decl = matches!(
            self.token(),
            SyntaxKind::Identifier | SyntaxKind::StringLiteral | SyntaxKind::OpenBraceToken
        );

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_decl
    }

    /// Look ahead to see if "type" starts a type alias declaration.
    fn look_ahead_is_type_alias_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip 'type'
        let is_decl = self.is_token(SyntaxKind::Identifier);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_decl
    }

    /// Look ahead to see if we have "identifier :" (labeled statement)
    fn look_ahead_is_labeled_statement(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip identifier
        self.next_token();
        // Check for ':'
        let is_colon = self.is_token(SyntaxKind::ColonToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_colon
    }

    /// Look ahead to see if we have "const enum"
    fn look_ahead_is_const_enum(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip 'const'
        self.next_token();
        // Check for 'enum'
        let is_enum = self.is_token(SyntaxKind::EnumKeyword);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_enum
    }

    /// Parse const enum declaration
    fn parse_const_enum_declaration(
        &mut self,
        start_pos: u32,
        mut modifiers: Vec<NodeIndex>,
    ) -> NodeIndex {
        let const_start = self.token_pos();
        self.parse_expected(SyntaxKind::ConstKeyword);
        let const_end = self.token_end();
        let const_modifier =
            self.arena
                .add_token(SyntaxKind::ConstKeyword as u16, const_start, const_end);
        modifiers.push(const_modifier);

        let modifiers = Some(self.make_node_list(modifiers));
        self.parse_enum_declaration_with_modifiers(start_pos, modifiers)
    }

    /// Parse labeled statement: label: statement
    fn parse_labeled_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the label (identifier)
        let label = self.parse_identifier_name();

        // Consume the colon
        self.parse_expected(SyntaxKind::ColonToken);

        // Parse the statement
        let statement = self.parse_statement();

        let end_pos = self.token_end();

        self.arena.add_labeled(
            syntax_kind_ext::LABELED_STATEMENT,
            start_pos,
            end_pos,
            LabeledData { label, statement },
        )
    }

    /// Parse import equals declaration: import X = require("...") or import X = Y.Z
    fn parse_import_equals_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ImportKeyword);

        // Parse the name - allows keywords like 'require' and 'exports' as valid names
        let name = self.parse_identifier_name();

        self.parse_expected(SyntaxKind::EqualsToken);

        // Parse module reference: require("...") or qualified name
        let module_reference = if self.is_token(SyntaxKind::RequireKeyword) {
            self.parse_external_module_reference()
        } else {
            self.parse_entity_name()
        };

        self.parse_semicolon();
        let end_pos = self.token_end();

        // Use ImportDeclData with import_clause as the name and module_specifier as reference
        // This is a simplified representation
        self.arena.add_import_decl(
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION,
            start_pos,
            end_pos,
            ImportDeclData {
                modifiers: None,
                import_clause: name,
                module_specifier: module_reference,
                attributes: NodeIndex::NONE,
            },
        )
    }

    /// Parse external module reference: require("...")
    fn parse_external_module_reference(&mut self) -> NodeIndex {
        self.parse_expected(SyntaxKind::RequireKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);
        let expression = self.parse_string_literal();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Return the string literal as the module reference
        expression
    }

    /// Parse entity name: A or A.B.C or this or this.x
    fn parse_entity_name(&mut self) -> NodeIndex {
        // Handle 'this' keyword as a valid start for typeof expressions
        let mut left = if self.is_token(SyntaxKind::ThisKeyword) {
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            self.next_token();
            self.arena
                .add_token(SyntaxKind::ThisKeyword as u16, start_pos, end_pos)
        } else {
            self.parse_identifier()
        };

        while self.is_token(SyntaxKind::DotToken) {
            self.next_token();
            let right = self.parse_identifier_name(); // Use identifier_name to allow keywords as property names
            let start_pos = if let Some(node) = self.arena.get(left) {
                node.pos
            } else {
                0
            };
            let end_pos = self.token_end();

            left = self.arena.add_qualified_name(
                syntax_kind_ext::QUALIFIED_NAME,
                start_pos,
                end_pos,
                QualifiedNameData { left, right },
            );
        }

        left
    }

    /// Parse async function declaration
    fn parse_async_function_declaration(&mut self) -> NodeIndex {
        self.parse_expected(SyntaxKind::AsyncKeyword);
        self.parse_function_declaration_with_async(true, None)
    }

    /// Parse a block statement
    fn parse_block(&mut self) -> NodeIndex {
        // Check recursion limit to prevent stack overflow on deeply nested code
        if !self.enter_recursion() {
            return NodeIndex::NONE;
        }

        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let statements = self.parse_statements();

        self.parse_expected(SyntaxKind::CloseBraceToken);
        let end_pos = self.token_end();

        self.exit_recursion();

        self.arena.add_block(
            syntax_kind_ext::BLOCK,
            start_pos,
            end_pos,
            BlockData {
                statements,
                multi_line: true,
            },
        )
    }

    /// Parse empty statement
    fn parse_empty_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::SemicolonToken);
        let end_pos = self.token_end();

        self.arena
            .add_token(syntax_kind_ext::EMPTY_STATEMENT, start_pos, end_pos)
    }

    /// Parse variable statement (var/let/const)
    fn parse_variable_statement(&mut self) -> NodeIndex {
        self.parse_variable_statement_with_modifiers(None, None)
    }

    /// Parse variable statement with optional start position and modifiers (for declare statements)
    fn parse_variable_statement_with_modifiers(
        &mut self,
        override_start_pos: Option<u32>,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        let start_pos = override_start_pos.unwrap_or_else(|| self.token_pos());
        let declaration_list = self.parse_variable_declaration_list();
        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena.add_variable(
            syntax_kind_ext::VARIABLE_STATEMENT,
            start_pos,
            end_pos,
            VariableData {
                modifiers,
                declarations: self.make_node_list(vec![declaration_list]),
            },
        )
    }

    /// Parse variable declaration list
    fn parse_variable_declaration_list(&mut self) -> NodeIndex {
        use crate::parser::node_flags;

        let start_pos = self.token_pos();

        // Consume var/let/const and get flags
        let flags: u16 = match self.token() {
            SyntaxKind::LetKeyword => {
                self.next_token();
                node_flags::LET as u16
            }
            SyntaxKind::ConstKeyword => {
                self.next_token();
                node_flags::CONST as u16
            }
            _ => {
                self.next_token();
                0
            } // var
        };

        // Parse declarations with enhanced error recovery
        let mut declarations = Vec::new();
        loop {
            // Check if we can start a variable declaration
            // Can be: identifier, keyword as identifier, or binding pattern (object/array)
            let can_start_decl = self.is_identifier_or_keyword()
                || self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::OpenBracketToken);

            if !can_start_decl {
                // Invalid token for variable declaration - emit error and recover
                if !self.is_token(SyntaxKind::SemicolonToken)
                    && !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Variable declaration expected.",
                        diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                    );
                }
                break;
            }

            let decl = self.parse_variable_declaration();
            declarations.push(decl);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }

            // After comma, check if next token can start another declaration
            // Handle cases like: let x, , y (missing declaration between commas)
            let can_start_next = self.is_identifier_or_keyword()
                || self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::OpenBracketToken);

            if !can_start_next {
                // Next token cannot start a declaration - emit error for missing declaration
                // and break to avoid consuming tokens that belong to the next statement
                use crate::checker::types::diagnostics::diagnostic_codes;
                if !self.is_token(SyntaxKind::SemicolonToken)
                    && !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    self.parse_error_at_current_token(
                        "Variable declaration expected.",
                        diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                    );
                }
                break;
            }
        }

        let end_pos = self.token_end();
        self.arena.add_variable_with_flags(
            syntax_kind_ext::VARIABLE_DECLARATION_LIST,
            start_pos,
            end_pos,
            VariableData {
                modifiers: None,
                declarations: self.make_node_list(declarations),
            },
            flags,
        )
    }

    /// Parse variable declaration
    fn parse_variable_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse name - can be identifier, keyword as identifier, or binding pattern
        // Check for illegal binding identifiers (e.g., 'await' in static blocks)
        self.check_illegal_binding_identifier();

        let name = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_object_binding_pattern()
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            self.parse_array_binding_pattern()
        } else if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            self.parse_identifier()
        };

        // Parse definite assignment assertion (!)
        let exclamation_token = self.parse_optional(SyntaxKind::ExclamationToken);

        // Parse optional type annotation
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        // Parse optional initializer
        let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
            if self.is_token(SyntaxKind::ConstKeyword)
                || self.is_token(SyntaxKind::LetKeyword)
                || self.is_token(SyntaxKind::VarKeyword)
            {
                self.error_expression_expected();
                NodeIndex::NONE
            } else {
                let expr = self.parse_assignment_expression();
                if expr.is_none() {
                    // Emit TS1109 for missing variable initializer: let x = [missing]
                    self.error_expression_expected();
                }
                expr
            }
        } else {
            NodeIndex::NONE
        };

        // Calculate end position from the last component present (child node, not token)
        let end_pos = if !initializer.is_none() {
            self.arena
                .get(initializer)
                .map(|n| n.end)
                .unwrap_or(self.token_pos())
        } else if !type_annotation.is_none() {
            self.arena
                .get(type_annotation)
                .map(|n| n.end)
                .unwrap_or(self.token_pos())
        } else {
            self.arena
                .get(name)
                .map(|n| n.end)
                .unwrap_or(self.token_pos())
        };

        self.arena.add_variable_declaration(
            syntax_kind_ext::VARIABLE_DECLARATION,
            start_pos,
            end_pos,
            VariableDeclarationData {
                name,
                exclamation_token,
                type_annotation,
                initializer,
            },
        )
    }

    /// Parse function declaration (optionally async)
    fn parse_function_declaration(&mut self) -> NodeIndex {
        self.parse_function_declaration_with_async(false, None)
    }

    /// Parse function declaration with async modifier already consumed
    fn parse_function_declaration_with_async(
        &mut self,
        is_async: bool,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        let start_pos = self.token_pos();

        // Check for async modifier if not already parsed
        let is_async = is_async || self.parse_optional(SyntaxKind::AsyncKeyword);

        self.parse_expected(SyntaxKind::FunctionKeyword);

        // Check for generator asterisk
        let asterisk_token = self.parse_optional(SyntaxKind::AsteriskToken);

        // Parse name - keywords like 'abstract' can be used as function names
        // Check for illegal binding identifiers (e.g., 'await' in static blocks)
        self.check_illegal_binding_identifier();

        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            self.parse_identifier()
        };

        // Parse optional type parameters: <T, U extends V>
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        // Parse parameters
        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Parse optional return type (may be a type predicate: param is T)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        // Parse body - may be missing for overload signatures (just a semicolon)
        // Set context flags for async/generator to properly parse await/yield
        let saved_flags = self.context_flags;
        if is_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }
        if asterisk_token {
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
        }

        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            // Consume the semicolon if present (overload signature)
            self.parse_optional(SyntaxKind::SemicolonToken);
            NodeIndex::NONE
        };

        // Restore context flags
        self.context_flags = saved_flags;

        let end_pos = self.token_end();
        self.arena.add_function(
            syntax_kind_ext::FUNCTION_DECLARATION,
            start_pos,
            end_pos,
            FunctionData {
                modifiers,
                is_async,
                asterisk_token,
                name,
                type_parameters,
                parameters,
                type_annotation,
                body,
                equals_greater_than_token: false,
            },
        )
    }

    /// Parse function expression: function() {} or function name() {}
    ///
    /// Unlike function declarations, function expressions can be anonymous.
    fn parse_function_expression(&mut self) -> NodeIndex {
        self.parse_function_expression_with_async(false)
    }

    /// Parse async function expression: async function() {} or async function name() {}
    fn parse_async_function_expression(&mut self) -> NodeIndex {
        self.parse_function_expression_with_async(true)
    }

    /// Parse function expression with optional async modifier
    fn parse_function_expression_with_async(&mut self, is_async: bool) -> NodeIndex {
        let start_pos = self.token_pos();

        // Consume async if present - only if we haven't already determined it's async
        // (When called from parse_async_function_expression, async hasn't been consumed yet)
        let is_async = if is_async {
            self.parse_expected(SyntaxKind::AsyncKeyword);
            true
        } else {
            self.parse_optional(SyntaxKind::AsyncKeyword)
        };

        self.parse_expected(SyntaxKind::FunctionKeyword);

        // Check for generator asterisk
        let asterisk_token = self.parse_optional(SyntaxKind::AsteriskToken);

        // Parse optional name (function expressions can be anonymous)
        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        // Parse optional type parameters: <T, U extends V>
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        // Parse parameters
        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Parse optional return type (may be a type predicate: param is T)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        // Parse body with context flags for async/generator
        let saved_flags = self.context_flags;
        if is_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }
        if asterisk_token {
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
        }

        let body = self.parse_block();

        // Restore context flags
        self.context_flags = saved_flags;

        let end_pos = self.token_end();
        self.arena.add_function(
            syntax_kind_ext::FUNCTION_EXPRESSION,
            start_pos,
            end_pos,
            FunctionData {
                modifiers: None,
                is_async,
                asterisk_token,
                name,
                type_parameters,
                parameters,
                type_annotation,
                body,
                equals_greater_than_token: false,
            },
        )
    }

    /// Parse class expression: class {} or class Name {}
    ///
    /// Unlike class declarations, class expressions can be anonymous.
    fn parse_class_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse optional name (class expressions can be anonymous)
        let name = if self.is_token(SyntaxKind::Identifier) {
            self.parse_identifier()
        } else {
            NodeIndex::NONE
        };

        // Parse optional type parameters
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        // Parse heritage (extends/implements)
        let heritage = self.parse_heritage_clauses();

        // Parse body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let members = self.parse_class_members();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        self.arena.add_class(
            syntax_kind_ext::CLASS_EXPRESSION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: None,
                name,
                type_parameters,
                heritage_clauses: heritage,
                members,
            },
        )
    }

    /// Parse parameter list
    fn parse_parameter_list(&mut self) -> NodeList {
        let mut params = Vec::new();

        while !self.is_token(SyntaxKind::CloseParenToken) {
            let param = self.parse_parameter();
            params.push(param);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                // Check if there's another parameter without comma - this is a TS1005 error
                if !self.is_token(SyntaxKind::CloseParenToken) && self.is_parameter_start() {
                    // Emit TS1005 for missing comma between parameters: f(a b)
                    self.error_comma_expected();
                }
                break;
            }
        }

        self.make_node_list(params)
    }

    /// Check if current token is a parameter modifier
    fn is_parameter_modifier(&self) -> bool {
        matches!(
            self.current_token,
            SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::ReadonlyKeyword
        )
    }

    /// Parse parameter modifiers (public, private, protected, readonly)
    fn parse_parameter_modifiers(&mut self) -> Option<NodeList> {
        let mut modifiers = Vec::new();

        while self.is_parameter_modifier() {
            let mod_start = self.token_pos();
            let mod_kind = self.current_token;
            self.next_token();
            let mod_end = self.token_end();
            modifiers.push(self.arena.add_token(mod_kind as u16, mod_start, mod_end));
        }

        if modifiers.is_empty() {
            None
        } else {
            Some(self.make_node_list(modifiers))
        }
    }

    /// Parse a single parameter
    fn parse_parameter(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse parameter decorators and parameter modifiers (public/private/readonly).
        // We store decorators in the same `modifiers` list used elsewhere in the Thin AST.
        let decorators = self.parse_decorators();
        let param_modifiers = self.parse_parameter_modifiers();
        let modifiers = match (decorators, param_modifiers) {
            (None, None) => None,
            (Some(list), None) | (None, Some(list)) => Some(list),
            (Some(decorators), Some(param_modifiers)) => {
                let mut nodes = Vec::with_capacity(
                    decorators
                        .nodes
                        .len()
                        .saturating_add(param_modifiers.nodes.len()),
                );
                nodes.extend(decorators.nodes);
                nodes.extend(param_modifiers.nodes);
                Some(self.make_node_list(nodes))
            }
        };

        // Parse rest parameter (...)
        let dot_dot_dot_token = self.parse_optional(SyntaxKind::DotDotDotToken);

        // Parse parameter name - can be an identifier, keyword, or binding pattern
        let name = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_object_binding_pattern()
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            self.parse_array_binding_pattern()
        } else if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            self.parse_identifier()
        };

        // Parse optional question mark
        let question_token = self.parse_optional(SyntaxKind::QuestionToken);

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
            // Default parameter values are evaluated in the parent scope, not in the function body.
            // Set parameter default context flag to detect 'await' usage (TS1109).
            let saved_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_PARAMETER_DEFAULT;
            // Also temporarily disable async context so 'await' is not treated as an await expression
            self.context_flags &= !CONTEXT_FLAG_ASYNC;
            let initializer = self.parse_assignment_expression();
            if initializer.is_none() {
                // Emit TS1109 for missing parameter default value: param = [missing]
                self.error_expression_expected();
            }
            self.context_flags = saved_flags;
            initializer
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_parameter(
            syntax_kind_ext::PARAMETER,
            start_pos,
            end_pos,
            crate::parser::thin_node::ParameterData {
                modifiers,
                dot_dot_dot_token,
                name,
                question_token,
                type_annotation,
                initializer,
            },
        )
    }

    /// Parse class declaration
    fn parse_class_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name - keywords like 'any', 'string' can be used as class names
        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        // Parse type parameters: class Foo<T, U> {}
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let members = self.parse_class_members();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: None,
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse class declaration with explicit modifiers.
    fn parse_class_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name - keywords like 'any', 'string' can be used as class names
        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        // Parse type parameters: class Foo<T, U> {}
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let members = self.parse_class_members();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers,
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse abstract class declaration: abstract class Foo {}
    fn parse_abstract_class_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Create abstract modifier node
        let abstract_start = self.token_pos();
        self.parse_expected(SyntaxKind::AbstractKeyword);
        let abstract_end = self.token_end();
        let abstract_modifier = self.arena.add_token(
            SyntaxKind::AbstractKeyword as u16,
            abstract_start,
            abstract_end,
        );

        // Now parse the class
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name - keywords like 'any', 'string' can be used as class names
        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        // Parse type parameters: abstract class Foo<T, U> {}
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let members = self.parse_class_members();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: Some(self.make_node_list(vec![abstract_modifier])),
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse declare class: declare class Foo {}
    fn parse_declare_class(&mut self, start_pos: u32, declare_modifier: NodeIndex) -> NodeIndex {
        self.parse_expected(SyntaxKind::ClassKeyword);

        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        let heritage_clauses = self.parse_heritage_clauses();

        self.parse_expected(SyntaxKind::OpenBraceToken);
        let members = self.parse_class_members();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: Some(self.make_node_list(vec![declare_modifier])),
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse declare abstract class: declare abstract class Foo {}
    fn parse_declare_abstract_class(
        &mut self,
        start_pos: u32,
        declare_modifier: NodeIndex,
    ) -> NodeIndex {
        // Create abstract modifier node
        let abstract_start = self.token_pos();
        self.parse_expected(SyntaxKind::AbstractKeyword);
        let abstract_end = self.token_end();
        let abstract_modifier = self.arena.add_token(
            SyntaxKind::AbstractKeyword as u16,
            abstract_start,
            abstract_end,
        );

        self.parse_expected(SyntaxKind::ClassKeyword);

        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        let heritage_clauses = self.parse_heritage_clauses();

        self.parse_expected(SyntaxKind::OpenBraceToken);
        let members = self.parse_class_members();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: Some(self.make_node_list(vec![declare_modifier, abstract_modifier])),
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse a decorated declaration: @decorator class/function
    fn parse_decorated_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse decorators
        let decorators = self.parse_decorators();

        // After decorators, expect class, abstract class, or function
        match self.token() {
            SyntaxKind::ClassKeyword => {
                self.parse_class_declaration_with_decorators(decorators, start_pos)
            }
            SyntaxKind::AbstractKeyword => {
                // abstract class with decorators
                self.parse_abstract_class_declaration_with_decorators(decorators, start_pos)
            }
            SyntaxKind::FunctionKeyword => {
                // For now, just parse the function and ignore decorators
                // Full decorator support would need function modifications
                self.parse_function_declaration()
            }
            SyntaxKind::EnumKeyword => {
                self.parse_enum_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::InterfaceKeyword => {
                self.parse_interface_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::TypeKeyword => {
                self.parse_type_alias_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                self.parse_module_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword | SyntaxKind::ConstKeyword => {
                self.parse_variable_statement_with_modifiers(Some(start_pos), decorators)
            }
            SyntaxKind::ExportKeyword => {
                // Export with decorators: @decorator export class Foo {}
                self.parse_export_declaration()
            }
            _ => {
                // Unexpected - just continue
                self.parse_expression_statement()
            }
        }
    }

    /// Parse decorators: @decorator1 @decorator2(arg) ...
    fn parse_decorators(&mut self) -> Option<NodeList> {
        if !self.is_token(SyntaxKind::AtToken) {
            return None;
        }

        let mut decorators = Vec::new();

        while self.is_token(SyntaxKind::AtToken) {
            if let Some(decorator) = self.try_parse_decorator() {
                decorators.push(decorator);
            } else {
                break;
            }
        }

        if decorators.is_empty() {
            None
        } else {
            Some(self.make_node_list(decorators))
        }
    }

    /// Try to parse a single decorator
    fn try_parse_decorator(&mut self) -> Option<NodeIndex> {
        if !self.is_token(SyntaxKind::AtToken) {
            return None;
        }

        let start_pos = self.token_pos();
        self.next_token(); // consume @

        // Parse the decorator expression (identifier, member access, or call)
        let expression = self.parse_left_hand_side_expression();

        let end_pos = self.token_end();
        Some(self.arena.add_decorator(
            syntax_kind_ext::DECORATOR,
            start_pos,
            end_pos,
            crate::parser::thin_node::DecoratorData { expression },
        ))
    }

    /// Parse class declaration with pre-parsed decorators
    fn parse_class_declaration_with_decorators(
        &mut self,
        decorators: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name
        // Check for illegal binding identifiers (e.g., 'await' in static blocks)
        self.check_illegal_binding_identifier();

        let name = if self.is_token(SyntaxKind::Identifier) {
            self.parse_identifier()
        } else {
            NodeIndex::NONE
        };

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let members = self.parse_class_members();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        // Create a modifiers list from decorators
        // In TypeScript, decorators are part of the modifiers
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: decorators,
                name,
                type_parameters: None,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse abstract class declaration with pre-parsed decorators
    fn parse_abstract_class_declaration_with_decorators(
        &mut self,
        decorators: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        // Create abstract modifier node
        let abstract_start = self.token_pos();
        self.parse_expected(SyntaxKind::AbstractKeyword);
        let abstract_end = self.token_end();
        let abstract_modifier = self.arena.add_token(
            SyntaxKind::AbstractKeyword as u16,
            abstract_start,
            abstract_end,
        );

        // Now parse the class
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name
        let name = if self.is_token(SyntaxKind::Identifier) {
            self.parse_identifier()
        } else {
            NodeIndex::NONE
        };

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let members = self.parse_class_members();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        // Combine decorators with abstract modifier
        let modifiers = if let Some(dec_list) = decorators {
            // Add abstract modifier to decorator list
            let mut nodes: Vec<NodeIndex> = dec_list.nodes.clone();
            nodes.push(abstract_modifier);
            Some(self.make_node_list(nodes))
        } else {
            Some(self.make_node_list(vec![abstract_modifier]))
        };

        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers,
                name,
                type_parameters: None,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse heritage clauses (extends, implements)
    fn parse_heritage_clauses(&mut self) -> Option<NodeList> {
        let mut clauses = Vec::new();
        let mut seen_extends = false;
        let mut seen_implements = false;

        loop {
            if self.is_token(SyntaxKind::ExtendsKeyword) {
                use crate::checker::types::diagnostics::diagnostic_codes;
                let start_pos = self.token_pos();
                if seen_extends {
                    self.parse_error_at_current_token(
                        "extends clause already seen.",
                        diagnostic_codes::EXTENDS_CLAUSE_ALREADY_SEEN,
                    );
                }
                if seen_implements {
                    self.parse_error_at_current_token(
                        "extends clause must precede implements clause.",
                        diagnostic_codes::EXTENDS_CLAUSE_MUST_PRECEDE_IMPLEMENTS_CLAUSE,
                    );
                }
                let should_add = !seen_extends;
                seen_extends = true;
                self.next_token();
                let type_ref = self.parse_heritage_type_reference();

                while self.is_token(SyntaxKind::CommaToken) {
                    self.parse_error_at_current_token(
                        "Classes can only extend a single class.",
                        diagnostic_codes::CLASSES_CAN_ONLY_EXTEND_A_SINGLE_CLASS,
                    );
                    self.next_token();
                    let _ = self.parse_heritage_type_reference();
                }

                let end_pos = self.token_end();
                if should_add {
                    let clause = self.arena.add_heritage(
                        syntax_kind_ext::HERITAGE_CLAUSE,
                        start_pos,
                        end_pos,
                        crate::parser::thin_node::HeritageData {
                            token: SyntaxKind::ExtendsKeyword as u16,
                            types: self.make_node_list(vec![type_ref]),
                        },
                    );
                    clauses.push(clause);
                }
                continue;
            }

            if self.is_token(SyntaxKind::ImplementsKeyword) {
                use crate::checker::types::diagnostics::diagnostic_codes;
                let start_pos = self.token_pos();
                if seen_implements {
                    self.parse_error_at_current_token(
                        "implements clause already seen.",
                        diagnostic_codes::IMPLEMENTS_CLAUSE_ALREADY_SEEN,
                    );
                }
                let should_add = !seen_implements;
                seen_implements = true;
                self.next_token();

                let mut types = Vec::new();
                loop {
                    let type_ref = self.parse_heritage_type_reference();
                    types.push(type_ref);
                    if !self.parse_optional(SyntaxKind::CommaToken) {
                        break;
                    }
                }

                let end_pos = self.token_end();
                if should_add {
                    let clause = self.arena.add_heritage(
                        syntax_kind_ext::HERITAGE_CLAUSE,
                        start_pos,
                        end_pos,
                        crate::parser::thin_node::HeritageData {
                            token: SyntaxKind::ImplementsKeyword as u16,
                            types: self.make_node_list(types),
                        },
                    );
                    clauses.push(clause);
                }
                continue;
            }

            break;
        }

        if clauses.is_empty() {
            None
        } else {
            Some(self.make_node_list(clauses))
        }
    }

    /// Parse a heritage type reference: Foo or Foo<T> or Foo.Bar<T> or base<T>()
    /// This is used in extends/implements clauses
    fn parse_heritage_type_reference(&mut self) -> NodeIndex {
        // parse_heritage_left_hand_expression now handles:
        // - Simple identifiers: Foo
        // - Property access: Foo.Bar.Baz
        // - Type arguments: Foo<T>
        // - Call expressions: Mixin(Parent) or base<T>()
        self.parse_heritage_left_hand_expression()
    }

    /// Parse left-hand expression for heritage clauses: Foo, Foo.Bar, or Mixin(Parent)
    /// This is a subset of member expression that allows identifiers, dots, and call expressions
    fn parse_heritage_left_hand_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Start with identifier or inline class expression
        let mut expr = if self.is_token(SyntaxKind::ClassKeyword) {
            // Inline class expression in extends clause: class extends class Expr {} {...}
            self.parse_class_expression()
        } else if self.is_token(SyntaxKind::ThisKeyword) {
            self.parse_this_expression()
        } else if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::NewKeyword)
        {
            self.parse_left_hand_side_expression()
        } else if matches!(
            self.token(),
            SyntaxKind::NullKeyword
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::UndefinedKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::NumericLiteral
                | SyntaxKind::StringLiteral
                | SyntaxKind::BigIntLiteral
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::TemplateHead
        ) {
            // Parse literals as valid expressions - the checker will emit TS2507
            // "Type 'X' is not a constructor function type" for invalid extends clauses
            self.parse_primary_expression()
        } else if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            // Invalid token in heritage clause - emit more specific error
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Class name or type expression expected",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            // Create unknown token and continue
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            self.next_token();
            self.arena
                .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
        };

        // Handle property access chain and call expressions: Foo.Bar.Baz or Mixin(Parent)
        loop {
            if self.is_token(SyntaxKind::DotToken) {
                self.next_token();
                let name = if self.is_identifier_or_keyword() {
                    self.parse_identifier_name()
                } else {
                    self.parse_identifier()
                };

                let end_pos = self.token_end();
                expr = self.arena.add_access_expr(
                    syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                    start_pos,
                    end_pos,
                    crate::parser::thin_node::AccessExprData {
                        expression: expr,
                        name_or_argument: name,
                        question_dot_token: false,
                    },
                );
            } else if self.is_token(SyntaxKind::QuestionDotToken) {
                // Optional chaining in heritage clause: A?.B
                // TypeScript allows optional chaining in extends/implements clauses
                self.next_token();
                let name = if self.is_identifier_or_keyword() {
                    self.parse_identifier_name()
                } else {
                    self.parse_identifier()
                };

                let end_pos = self.token_end();
                expr = self.arena.add_access_expr(
                    syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                    start_pos,
                    end_pos,
                    crate::parser::thin_node::AccessExprData {
                        expression: expr,
                        name_or_argument: name,
                        question_dot_token: true,
                    },
                );
            } else if self.is_token(SyntaxKind::LessThanToken) {
                // Generic call expression: base<T>() or base<T, U>()
                // Parse type arguments then check for call
                self.next_token();
                let mut type_args = Vec::new();
                while !self.is_token(SyntaxKind::GreaterThanToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    let type_arg = self.parse_type();
                    type_args.push(type_arg);
                    if !self.parse_optional(SyntaxKind::CommaToken) {
                        break;
                    }
                }
                self.parse_expected(SyntaxKind::GreaterThanToken);

                // Now check for call expression after type arguments
                if self.is_token(SyntaxKind::OpenParenToken) {
                    self.next_token();
                    let mut args = Vec::new();
                    while !self.is_token(SyntaxKind::CloseParenToken)
                        && !self.is_token(SyntaxKind::EndOfFileToken)
                    {
                        let arg = self.parse_assignment_expression();
                        args.push(arg);
                        if !self.parse_optional(SyntaxKind::CommaToken) {
                            break;
                        }
                    }
                    let end_pos = self.token_end();
                    self.parse_expected(SyntaxKind::CloseParenToken);
                    expr = self.arena.add_call_expr(
                        syntax_kind_ext::CALL_EXPRESSION,
                        start_pos,
                        end_pos,
                        crate::parser::thin_node::CallExprData {
                            expression: expr,
                            type_arguments: Some(self.make_node_list(type_args)),
                            arguments: Some(self.make_node_list(args)),
                        },
                    );
                } else {
                    // Just type arguments, no call - create expression with type arguments
                    let end_pos = self.token_end();
                    expr = self.arena.add_expr_with_type_args(
                        syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS,
                        start_pos,
                        end_pos,
                        crate::parser::thin_node::ExprWithTypeArgsData {
                            expression: expr,
                            type_arguments: Some(self.make_node_list(type_args)),
                        },
                    );
                    // Don't break here - continue the loop for potential chaining
                }
            } else if self.is_token(SyntaxKind::OpenParenToken) {
                // Call expression without type args: Mixin(Parent)
                self.next_token();
                let mut args = Vec::new();
                while !self.is_token(SyntaxKind::CloseParenToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    let arg = self.parse_assignment_expression();
                    args.push(arg);
                    if !self.parse_optional(SyntaxKind::CommaToken) {
                        break;
                    }
                }
                let end_pos = self.token_end();
                self.parse_expected(SyntaxKind::CloseParenToken);
                expr = self.arena.add_call_expr(
                    syntax_kind_ext::CALL_EXPRESSION,
                    start_pos,
                    end_pos,
                    crate::parser::thin_node::CallExprData {
                        expression: expr,
                        type_arguments: None,
                        arguments: Some(self.make_node_list(args)),
                    },
                );
            } else {
                break;
            }
        }

        expr
    }

    /// Parse class member modifiers (static, public, private, protected, readonly, abstract, override)
    fn parse_class_member_modifiers(&mut self) -> Option<NodeList> {
        let mut modifiers = Vec::new();

        loop {
            if self.should_stop_class_member_modifier() {
                break;
            }
            let start_pos = self.token_pos();
            let modifier = match self.token() {
                SyntaxKind::StaticKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::StaticKeyword, start_pos)
                }
                SyntaxKind::PublicKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::PublicKeyword, start_pos)
                }
                SyntaxKind::PrivateKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::PrivateKeyword, start_pos)
                }
                SyntaxKind::ProtectedKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::ProtectedKeyword, start_pos)
                }
                SyntaxKind::ReadonlyKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::ReadonlyKeyword, start_pos)
                }
                SyntaxKind::AbstractKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::AbstractKeyword, start_pos)
                }
                SyntaxKind::OverrideKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::OverrideKeyword, start_pos)
                }
                SyntaxKind::AsyncKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::AsyncKeyword, start_pos)
                }
                SyntaxKind::DeclareKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::DeclareKeyword, start_pos)
                }
                SyntaxKind::AccessorKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::AccessorKeyword, start_pos)
                }
                // Handle const as a modifier - error is reported by checker (1248)
                // But only if not followed by line break (ASI would make it a property name)
                SyntaxKind::ConstKeyword => {
                    // Look ahead: if there's a line break after const, treat as property name not modifier
                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token();
                    if self.scanner.has_preceding_line_break() {
                        // Restore and break - const is a property name
                        self.scanner.restore_state(snapshot);
                        self.current_token = saved_token;
                        break;
                    }
                    self.arena
                        .create_modifier(SyntaxKind::ConstKeyword, start_pos)
                }
                // Handle 'export' - not valid as class member modifier
                SyntaxKind::ExportKeyword => {
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::ExportKeyword, start_pos)
                }
                _ => break,
            };
            modifiers.push(modifier);
        }

        if modifiers.is_empty() {
            None
        } else {
            Some(self.make_node_list(modifiers))
        }
    }

    fn should_stop_class_member_modifier(&mut self) -> bool {
        if !matches!(
            self.token(),
            SyntaxKind::StaticKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::OverrideKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::ExportKeyword
        ) {
            return false;
        }

        if self.is_token(SyntaxKind::StaticKeyword) && self.look_ahead_is_static_block() {
            return true;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let next = self.current_token;
        self.scanner.restore_state(snapshot);
        self.current_token = current;

        matches!(
            next,
            SyntaxKind::OpenParenToken
                | SyntaxKind::LessThanToken
                | SyntaxKind::QuestionToken
                | SyntaxKind::ExclamationToken
                | SyntaxKind::ColonToken
                | SyntaxKind::EqualsToken
                | SyntaxKind::SemicolonToken
        )
    }

    /// Parse constructor with modifiers
    fn parse_constructor_with_modifiers(&mut self, modifiers: Option<NodeList>) -> NodeIndex {
        use crate::checker::types::diagnostics::diagnostic_codes;
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ConstructorKeyword);

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Recovery: Handle return type annotation on constructor (invalid but users write it)
        if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_error_at_current_token(
                "Constructor cannot have a return type annotation.",
                diagnostic_codes::CONSTRUCTOR_CANNOT_HAVE_RETURN_TYPE,
            );
            // Consume the type annotation for recovery
            let _ = self.parse_type();
        };

        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_constructor(
            syntax_kind_ext::CONSTRUCTOR,
            start_pos,
            end_pos,
            crate::parser::thin_node::ConstructorData {
                modifiers,
                type_parameters: None,
                parameters,
                body,
            },
        )
    }

    /// Parse get accessor with modifiers: static get foo() { }
    fn parse_get_accessor_with_modifiers(
        &mut self,
        modifiers: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::GetKeyword);

        let name = self.parse_property_name();

        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "An accessor cannot have type parameters.",
                diagnostic_codes::ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS,
            );
            Some(self.parse_type_parameters())
        } else {
            None
        };

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if self.is_token(SyntaxKind::CloseParenToken) {
            self.make_node_list(vec![])
        } else {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "A 'get' accessor cannot have parameters.",
                diagnostic_codes::GETTER_MUST_NOT_HAVE_PARAMETERS,
            );
            self.parse_parameter_list()
        };
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Optional return type (supports type predicates)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        // Parse body (may be empty for ambient declarations)
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            self.parse_semicolon();
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_accessor(
            syntax_kind_ext::GET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::thin_node::AccessorData {
                modifiers,
                name,
                type_parameters,
                parameters,
                type_annotation,
                body,
            },
        )
    }

    /// Parse set accessor with modifiers: static set foo(value) { }
    fn parse_set_accessor_with_modifiers(
        &mut self,
        modifiers: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::SetKeyword);

        let name = self.parse_property_name();

        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "An accessor cannot have type parameters.",
                diagnostic_codes::ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS,
            );
            Some(self.parse_type_parameters())
        } else {
            None
        };

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if self.is_token(SyntaxKind::CloseParenToken) {
            self.make_node_list(vec![])
        } else {
            self.parse_parameter_list()
        };
        self.parse_expected(SyntaxKind::CloseParenToken);

        if parameters.len() != 1 {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "A 'set' accessor must have exactly one parameter.",
                diagnostic_codes::SETTER_MUST_HAVE_EXACTLY_ONE_PARAMETER,
            );
        }

        if self.parse_optional(SyntaxKind::ColonToken) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "A 'set' accessor cannot have a return type annotation.",
                diagnostic_codes::SETTER_CANNOT_HAVE_RETURN_TYPE,
            );
            let _ = self.parse_type();
        }

        // Parse body (may be empty for ambient declarations)
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            self.parse_semicolon();
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_accessor(
            syntax_kind_ext::SET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::thin_node::AccessorData {
                modifiers,
                name,
                type_parameters,
                parameters,
                type_annotation: NodeIndex::NONE,
                body,
            },
        )
    }

    /// Parse class members
    fn parse_class_members(&mut self) -> NodeList {
        let mut members = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let member = self.parse_class_member();
            if !member.is_none() {
                members.push(member);
            }

            // Handle semicolons
            self.parse_optional(SyntaxKind::SemicolonToken);
        }

        self.make_node_list(members)
    }

    /// Parse a single class member
    fn parse_class_member(&mut self) -> NodeIndex {
        use crate::checker::types::diagnostics::diagnostic_codes;
        let start_pos = self.token_pos();

        // Parse decorators first (if any)
        let _decorators = self.parse_decorators();

        // Handle empty statement (semicolon) in class body - this is valid TypeScript/JavaScript
        // A standalone semicolon in a class body is an empty class element
        if self.is_token(SyntaxKind::SemicolonToken) {
            // Consume the semicolon and return NONE (empty class element)
            self.next_token();
            return NodeIndex::NONE;
        }

        // Recovery: Handle stray statements in class bodies (common copy-paste error)
        // Users often accidentally leave statements like `if`, `while`, `return` in class bodies
        let is_statement_keyword = matches!(
            self.token(),
            SyntaxKind::IfKeyword         // if (x) { }
            | SyntaxKind::WhileKeyword     // while (x) { }
            | SyntaxKind::DoKeyword        // do { } while (x)
            | SyntaxKind::ForKeyword       // for (...) { }
            | SyntaxKind::SwitchKeyword    // switch (x) { }
            | SyntaxKind::ReturnKeyword    // return x;
            | SyntaxKind::ThrowKeyword     // throw x;
            | SyntaxKind::TryKeyword       // try { } catch { }
            | SyntaxKind::WithKeyword      // with (x) { }
            | SyntaxKind::DebuggerKeyword // debugger;
        );

        if is_statement_keyword {
            // Only emit error if we haven't already emitted one at this position
            if self.token_pos() != self.last_error_pos {
                self.parse_error_at_current_token(
                    "Declaration or statement expected.",
                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                );
            }
            // Parse the statement to consume it and balance braces
            // This maintains parsing sync so we can continue parsing the rest of the class
            let _ = self.parse_statement();
            // Return NONE to indicate this is not a valid class member
            return NodeIndex::NONE;
        }

        // Parse decorators if present
        let decorators = self.parse_decorators();

        // If decorators were found before a static block, emit TS1206
        if decorators.is_some()
            && self.is_token(SyntaxKind::StaticKeyword)
            && self.look_ahead_is_static_block()
        {
            self.parse_error_at_current_token(
                "Decorators are not valid here.",
                diagnostic_codes::DECORATORS_NOT_VALID_HERE,
            );
            return self.parse_static_block();
        }

        // Handle static block: static { ... }
        if self.is_token(SyntaxKind::StaticKeyword) && self.look_ahead_is_static_block() {
            return self.parse_static_block();
        }

        // Parse modifiers (static, public, private, protected, readonly, abstract, override)
        let modifiers = self.parse_class_member_modifiers();

        // Handle static block after modifiers: { ... }
        if self.is_token(SyntaxKind::StaticKeyword) && self.look_ahead_is_static_block() {
            if modifiers.is_some() {
                self.parse_error_at_current_token(
                    "Modifiers cannot appear on a static block.",
                    diagnostic_codes::MODIFIERS_NOT_ALLOWED_HERE,
                );
            }
            return self.parse_static_block();
        }

        // Handle constructor
        if self.is_token(SyntaxKind::ConstructorKeyword) {
            return self.parse_constructor_with_modifiers(modifiers);
        }

        // Handle generator methods: *foo() or async *#bar()
        let asterisk_token = self.parse_optional(SyntaxKind::AsteriskToken);

        // Handle get accessor: get foo() { }
        if self.is_token(SyntaxKind::GetKeyword) && self.look_ahead_is_accessor() {
            return self.parse_get_accessor_with_modifiers(modifiers, start_pos);
        }

        // Handle set accessor: set foo(value) { }
        if self.is_token(SyntaxKind::SetKeyword) && self.look_ahead_is_accessor() {
            return self.parse_set_accessor_with_modifiers(modifiers, start_pos);
        }

        // Handle index signatures: [key: Type]: ValueType
        if self.is_token(SyntaxKind::OpenBracketToken) && self.look_ahead_is_index_signature() {
            return self.parse_index_signature_with_modifiers(modifiers, start_pos);
        }

        // Recovery: Handle 'function' keyword in class members
        // Note: 'var', 'let', 'const' are allowed as property/method names (e.g., `var() {}`)
        // 'function' is invalid as a class member keyword, but we recover gracefully
        // by silently consuming it and parsing the rest as a method
        if self.is_token(SyntaxKind::FunctionKeyword) {
            // Silently consume 'function' without emitting TS1068
            self.next_token();
        }

        // Recovery: Handle 'const'/'let'/'var' used as modifiers in class members
        // Distinguish between: `const x = 1` (invalid, error) vs `const() {}` (valid method name)
        if matches!(
            self.token(),
            SyntaxKind::ConstKeyword | SyntaxKind::LetKeyword | SyntaxKind::VarKeyword
        ) {
            // Look ahead to determine if this is being used as a modifier or as a name
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token(); // skip const/let/var
            let next_token = self.token();
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            // If followed by `(`, it's a method name (e.g., `const() {}`), which is valid
            // If followed by identifier and then `:` or `=`, it's being used as a modifier (invalid)
            if !matches!(next_token, SyntaxKind::OpenParenToken) {
                // This is likely being used as a modifier, emit error and recover
                self.parse_error_at_current_token(
                    "A class member cannot have the 'const', 'let', or 'var' keyword.",
                    diagnostic_codes::UNEXPECTED_TOKEN_CLASS_MEMBER,
                );
                // Consume the invalid keyword and continue parsing
                // The next identifier will be treated as the property/method name
                self.next_token();
            }
        }

        // Handle methods and properties
        // For now, just parse name and check for ( for methods
        // Note: Many reserved keywords can be used as property names (const, class, etc.)
        let name = if self.is_property_name() {
            self.parse_property_name()
        } else {
            // Report error for unknown token
            self.parse_error_at_current_token(
                "Unexpected token. A constructor, method, accessor, or property was expected.",
                diagnostic_codes::UNEXPECTED_TOKEN_CLASS_MEMBER,
            );
            self.next_token();
            return NodeIndex::NONE;
        };

        // Parse optional ? or ! after property name
        let question_token = self.parse_optional(SyntaxKind::QuestionToken);
        let exclamation_token = if !question_token {
            self.parse_optional(SyntaxKind::ExclamationToken)
        } else {
            false
        };

        // Check if it's a method or property
        // Method: foo() or foo<T>()
        if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::LessThanToken) {
            // Parse optional type parameters: foo<T, U>()
            let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
                Some(self.parse_type_parameters())
            } else {
                None
            };

            // Method
            self.parse_expected(SyntaxKind::OpenParenToken);
            let parameters = self.parse_parameter_list();
            self.parse_expected(SyntaxKind::CloseParenToken);

            // Optional return type (supports type predicates: param is T)
            let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_return_type()
            } else {
                NodeIndex::NONE
            };

            // Check if method has async modifier
            let is_async = modifiers.as_ref().map_or(false, |mods| {
                mods.nodes.iter().any(|&idx| {
                    self.arena
                        .nodes
                        .get(idx.0 as usize)
                        .map_or(false, |node| node.kind == SyntaxKind::AsyncKeyword as u16)
                })
            });

            // Set context flags for async method body
            let saved_flags = self.context_flags;
            if is_async {
                self.context_flags |= CONTEXT_FLAG_ASYNC;
            }

            // Parse body
            let body = if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_block()
            } else {
                NodeIndex::NONE
            };

            // Restore context flags
            self.context_flags = saved_flags;

            let end_pos = self.token_end();
            self.arena.add_method_decl(
                syntax_kind_ext::METHOD_DECLARATION,
                start_pos,
                end_pos,
                crate::parser::thin_node::MethodDeclData {
                    modifiers,
                    asterisk_token,
                    name,
                    question_token,
                    type_parameters,
                    parameters,
                    type_annotation,
                    body,
                },
            )
        } else {
            // Property - parse optional type and initializer
            let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_type()
            } else {
                NodeIndex::NONE
            };

            let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
                self.parse_assignment_expression()
            } else {
                NodeIndex::NONE
            };

            let end_pos = self.token_end();
            self.arena.add_property_decl(
                syntax_kind_ext::PROPERTY_DECLARATION,
                start_pos,
                end_pos,
                crate::parser::thin_node::PropertyDeclData {
                    modifiers,
                    name,
                    question_token,
                    exclamation_token,
                    type_annotation,
                    initializer,
                },
            )
        }
    }

    /// Look ahead to see if we have an accessor (get/set followed by property name and ()
    fn look_ahead_is_accessor(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip 'get' or 'set'
        self.next_token();

        // Check the token AFTER 'get' or 'set' to determine what we have:
        // - `:`, `=`, `;`, `}`, `?` → property named 'get'/'set' (e.g., `get: number`)
        // - `(` → method named 'get'/'set' (e.g., `get() {}`)
        // - identifier/string/etc → accessor (e.g., `get foo() {}`)
        let next_token = self.token();
        let is_accessor = !matches!(
            next_token,
            SyntaxKind::ColonToken          // `get: number` - property
                | SyntaxKind::EqualsToken     // `get = 1` - property
                | SyntaxKind::SemicolonToken  // `get;` - property
                | SyntaxKind::CloseBraceToken // `get }` - property
                | SyntaxKind::OpenParenToken  // `get()` - method
                | SyntaxKind::QuestionToken // `get?` - property
        ) && self.is_property_name(); // Also ensure there's a valid property name

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_accessor
    }

    /// Look ahead to see if we have a static block: static { ... }
    fn look_ahead_is_static_block(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip 'static'
        self.next_token();
        // Check for '{'
        let is_block = self.is_token(SyntaxKind::OpenBraceToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_block
    }

    /// Parse static block: static { ... }
    fn parse_static_block(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Consume 'static'
        self.parse_expected(SyntaxKind::StaticKeyword);

        // Parse the block body with static block context (where 'await' is reserved)
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_STATIC_BLOCK;
        let statements = self.parse_statements();
        self.context_flags = saved_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        self.arena.add_block(
            syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::thin_node::BlockData {
                statements,
                multi_line: true,
            },
        )
    }

    /// Look ahead to see if this is an index signature: [key: Type]: ValueType
    /// vs a computed property: [expr]: Type or [computed]()
    fn look_ahead_is_index_signature(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip '['
        self.next_token();

        // Check for identifier followed by ':'
        let is_index_sig = if self.is_identifier_or_keyword() {
            self.next_token();
            self.is_token(SyntaxKind::ColonToken)
        } else {
            false
        };

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_index_sig
    }

    /// Parse interface declaration
    fn parse_interface_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_interface_declaration_with_modifiers(start_pos, None)
    }

    /// Parse interface declaration with explicit modifiers
    fn parse_interface_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::InterfaceKeyword);

        // Parse interface name - keywords like 'string', 'abstract' can be used as interface names
        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            self.parse_identifier()
        };

        // Parse type parameters: interface IList<T> {}
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        // Parse heritage clauses (extends only for interfaces)
        // Interfaces can extend multiple types: interface A extends B, C, D { }
        let heritage_clauses = if self.is_token(SyntaxKind::ExtendsKeyword) {
            let clause_start = self.token_pos();
            self.next_token();

            let mut types = Vec::new();
            loop {
                let type_ref = self.parse_heritage_type_reference();
                types.push(type_ref);
                if !self.parse_optional(SyntaxKind::CommaToken) {
                    break;
                }
            }

            let clause_end = self.token_end();
            let clause = self.arena.add_heritage(
                syntax_kind_ext::HERITAGE_CLAUSE,
                clause_start,
                clause_end,
                crate::parser::thin_node::HeritageData {
                    token: SyntaxKind::ExtendsKeyword as u16,
                    types: self.make_node_list(types),
                },
            );
            Some(self.make_node_list(vec![clause]))
        } else {
            None
        };

        // Parse interface body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let members = self.parse_type_members();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_interface(
            syntax_kind_ext::INTERFACE_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::thin_node::InterfaceData {
                modifiers,
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse type members (for interfaces and type literals)
    fn parse_type_members(&mut self) -> NodeList {
        let mut members = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let start_pos = self.token_pos();
            let member = self.parse_type_member();
            if !member.is_none() {
                members.push(member);
            }

            // Handle semicolons or commas
            self.parse_optional(SyntaxKind::SemicolonToken);
            self.parse_optional(SyntaxKind::CommaToken);

            // If we didn't make progress, skip the current token to avoid infinite loop
            if self.token_pos() == start_pos && !self.is_token(SyntaxKind::CloseBraceToken) {
                self.next_token();
            }
        }

        self.make_node_list(members)
    }

    /// Parse a single type member (property signature, method signature, call signature, construct signature)
    fn parse_type_member(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Handle invalid access modifiers (private/protected/public) on type members.
        if matches!(
            self.token(),
            SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::AccessorKeyword
        ) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Modifiers cannot appear here.",
                diagnostic_codes::MODIFIERS_NOT_ALLOWED_HERE,
            );
            self.next_token();
            if self.is_token(SyntaxKind::OpenBracketToken) && self.look_ahead_is_index_signature() {
                return self.parse_index_signature_with_modifiers(None, start_pos);
            }
        }

        // Handle generic call signature: <T>(): returnType
        if self.is_token(SyntaxKind::LessThanToken) {
            return self.parse_call_signature(start_pos);
        }

        // Handle call signature: (): returnType
        if self.is_token(SyntaxKind::OpenParenToken) {
            return self.parse_call_signature(start_pos);
        }

        // Handle construct signature: new (): returnType
        if self.is_token(SyntaxKind::NewKeyword) {
            return self.parse_construct_signature(start_pos);
        }

        // Handle get accessor: get foo(): type
        // But not if 'get' is used as property name (get: T or get?: T or get() or get<T>())
        if self.is_token(SyntaxKind::GetKeyword)
            && !self.look_ahead_is_property_name_after_keyword()
        {
            return self.parse_get_accessor_signature(start_pos);
        }

        // Handle set accessor: set foo(v: type)
        // But not if 'set' is used as property name
        if self.is_token(SyntaxKind::SetKeyword)
            && !self.look_ahead_is_property_name_after_keyword()
        {
            return self.parse_set_accessor_signature(start_pos);
        }

        // Parse optional readonly modifier
        // But not if 'readonly' is used as property name
        let readonly = if self.is_token(SyntaxKind::ReadonlyKeyword)
            && !self.look_ahead_is_property_name_after_keyword()
        {
            self.next_token();
            true
        } else {
            false
        };

        // Parse property/method name
        // Include keywords that can be property names
        let name = if self.is_token(SyntaxKind::Identifier)
            || self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_property_name_keyword()
        {
            self.parse_property_name()
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            // Check if it's an index signature: [key: string]: value
            // vs computed property name: [Symbol.iterator](): type
            if self.look_ahead_is_index_signature() {
                // Build modifiers list if readonly was present
                let modifiers = if readonly {
                    let mod_idx = self
                        .arena
                        .create_modifier(SyntaxKind::ReadonlyKeyword, start_pos);
                    Some(self.make_node_list(vec![mod_idx]))
                } else {
                    None
                };
                return self.parse_index_signature_with_modifiers(modifiers, start_pos);
            } else {
                // Computed property name
                self.parse_property_name()
            }
        } else {
            return NodeIndex::NONE;
        };

        // Optional question mark
        let question_token = self.parse_optional(SyntaxKind::QuestionToken);

        // Build modifiers list if readonly was present
        let modifiers = if readonly {
            let mod_idx = self
                .arena
                .create_modifier(SyntaxKind::ReadonlyKeyword, start_pos);
            Some(self.make_node_list(vec![mod_idx]))
        } else {
            None
        };

        // Check if it's a method signature or property signature
        // Method signature: foo(): T or foo<T>(): U
        if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::LessThanToken) {
            // Parse optional type parameters: foo<T, U>()
            let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
                Some(self.parse_type_parameters())
            } else {
                None
            };

            // Method signature
            self.parse_expected(SyntaxKind::OpenParenToken);
            let parameters = self.parse_parameter_list();
            self.parse_expected(SyntaxKind::CloseParenToken);

            // Return type (supports type predicates: param is T)
            let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_return_type()
            } else {
                NodeIndex::NONE
            };

            let end_pos = self.token_end();
            self.arena.add_signature(
                syntax_kind_ext::METHOD_SIGNATURE,
                start_pos,
                end_pos,
                crate::parser::thin_node::SignatureData {
                    modifiers,
                    name,
                    question_token,
                    type_parameters,
                    parameters: Some(parameters),
                    type_annotation,
                },
            )
        } else {
            // Property signature
            let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_type()
            } else {
                NodeIndex::NONE
            };

            // Skip initializer if present (invalid in type context but should produce error, not crash)
            // Example: { bar: number = 5 } - the "= 5" is invalid but we parse it for recovery
            if self.parse_optional(SyntaxKind::EqualsToken) {
                self.parse_assignment_expression();
            }

            let end_pos = self.token_end();
            self.arena.add_signature(
                syntax_kind_ext::PROPERTY_SIGNATURE,
                start_pos,
                end_pos,
                crate::parser::thin_node::SignatureData {
                    modifiers,
                    name,
                    question_token,
                    type_parameters: None,
                    parameters: None,
                    type_annotation,
                },
            )
        }
    }

    /// Parse call signature: (): returnType or <T>(): returnType
    fn parse_call_signature(&mut self, start_pos: u32) -> NodeIndex {
        // Parse optional type parameters: <T, U>
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Return type (supports type predicates: param is T)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_signature(
            syntax_kind_ext::CALL_SIGNATURE,
            start_pos,
            end_pos,
            crate::parser::thin_node::SignatureData {
                modifiers: None,
                name: NodeIndex::NONE,
                question_token: false,
                type_parameters,
                parameters: Some(parameters),
                type_annotation,
            },
        )
    }

    /// Parse construct signature: new (): returnType or new <T>(): returnType
    fn parse_construct_signature(&mut self, start_pos: u32) -> NodeIndex {
        self.parse_expected(SyntaxKind::NewKeyword);

        // Parse optional type parameters: new <T>()
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Return type (supports type predicates)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_signature(
            syntax_kind_ext::CONSTRUCT_SIGNATURE,
            start_pos,
            end_pos,
            crate::parser::thin_node::SignatureData {
                modifiers: None,
                name: NodeIndex::NONE,
                question_token: false,
                type_parameters,
                parameters: Some(parameters),
                type_annotation,
            },
        )
    }

    /// Parse index signature: [key: string]: value
    fn parse_index_signature(&mut self) -> NodeIndex {
        self.parse_index_signature_with_modifiers(None, self.token_pos())
    }

    /// Parse index signature with modifiers (static, readonly, etc.): static [key: string]: value
    fn parse_index_signature_with_modifiers(
        &mut self,
        modifiers: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::OpenBracketToken);

        // Parse parameter
        let param_start = self.token_pos();
        let param_name = self.parse_identifier();
        self.parse_expected(SyntaxKind::ColonToken);
        let param_type = self.parse_type(); // Type of the index parameter (e.g., string, number)

        // Allow trailing comma (invalid syntax but should produce error, not crash)
        self.parse_optional(SyntaxKind::CommaToken);

        self.parse_expected(SyntaxKind::CloseBracketToken);

        // Value type is optional - [index: any]; is valid (but semantically an error)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        let param_end = self.token_end();
        let param_node = self.arena.add_parameter(
            syntax_kind_ext::PARAMETER,
            param_start,
            param_end,
            ParameterData {
                modifiers: None,
                dot_dot_dot_token: false,
                name: param_name,
                question_token: false,
                type_annotation: param_type,
                initializer: NodeIndex::NONE,
            },
        );

        let end_pos = self.token_end();
        self.arena.add_index_signature(
            syntax_kind_ext::INDEX_SIGNATURE,
            start_pos,
            end_pos,
            crate::parser::thin_node::IndexSignatureData {
                modifiers,
                parameters: self.make_node_list(vec![param_node]),
                type_annotation,
            },
        )
    }

    /// Parse get accessor signature in type context: get foo(): type
    /// Note: TypeScript allows bodies here (which is an error), so we parse them for error recovery
    fn parse_get_accessor_signature(&mut self, start_pos: u32) -> NodeIndex {
        self.parse_expected(SyntaxKind::GetKeyword);

        let name = self.parse_property_name();

        self.parse_expected(SyntaxKind::OpenParenToken);
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Return type (supports type predicates)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        // Parse body if present (this is an error in type context, but we handle it)
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_accessor(
            syntax_kind_ext::GET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::thin_node::AccessorData {
                modifiers: None,
                name,
                type_parameters: None,
                parameters: self.make_node_list(vec![]),
                type_annotation,
                body,
            },
        )
    }

    /// Parse set accessor signature in type context: set foo(v: type)
    /// Note: TypeScript allows bodies here (which is an error), so we parse them for error recovery
    fn parse_set_accessor_signature(&mut self, start_pos: u32) -> NodeIndex {
        self.parse_expected(SyntaxKind::SetKeyword);

        let name = self.parse_property_name();

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Parse body if present (this is an error in type context, but we handle it)
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_accessor(
            syntax_kind_ext::SET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::thin_node::AccessorData {
                modifiers: None,
                name,
                type_parameters: None,
                parameters,
                type_annotation: NodeIndex::NONE,
                body,
            },
        )
    }

    /// Parse type alias declaration: type Foo = ... or type Foo<T> = ...
    fn parse_type_alias_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_type_alias_declaration_with_modifiers(start_pos, None)
    }

    fn parse_type_alias_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::TypeKeyword);

        let name = self.parse_identifier();

        // Parse optional type parameters: <T, U extends Foo>
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        // Parse expected equals token, but recover gracefully if missing
        // If the next token can start a type (e.g., {, (, [), emit error and continue parsing
        if !self.is_token(SyntaxKind::EqualsToken) {
            // Emit TS1005 for missing equals token
            self.error_token_expected("=");
            // If the next token looks like a type, continue parsing anyway
            if !self.can_token_start_type() {
                // Can't recover, return early with a dummy type
                let end_pos = self.token_end();
                return self.arena.add_type_alias(
                    syntax_kind_ext::TYPE_ALIAS_DECLARATION,
                    start_pos,
                    end_pos,
                    crate::parser::thin_node::TypeAliasData {
                        modifiers,
                        name,
                        type_parameters,
                        type_node: NodeIndex::NONE,
                    },
                );
            }
        } else {
            self.next_token(); // Consume the equals token
        }

        let type_node = self.parse_type();

        self.parse_semicolon();

        let end_pos = self.token_end();
        self.arena.add_type_alias(
            syntax_kind_ext::TYPE_ALIAS_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::thin_node::TypeAliasData {
                modifiers,
                name,
                type_parameters,
                type_node,
            },
        )
    }

    /// Parse enum declaration
    fn parse_enum_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_enum_declaration_with_modifiers(start_pos, None)
    }

    /// Parse enum declaration with explicit modifiers
    fn parse_enum_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::EnumKeyword);

        let name = self.parse_identifier();

        self.parse_expected(SyntaxKind::OpenBraceToken);

        let members = self.parse_enum_members();

        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_enum(
            syntax_kind_ext::ENUM_DECLARATION,
            start_pos,
            end_pos,
            EnumData {
                modifiers,
                name,
                members,
            },
        )
    }

    /// Parse enum members
    fn parse_enum_members(&mut self) -> NodeList {
        use crate::checker::types::diagnostics::diagnostic_codes;
        let mut members = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let start_pos = self.token_pos();

            // Enum member names can be identifiers, string literals, or computed property names
            // Computed property names ([x]) are not valid in enums but we recover gracefully
            let name = if self.is_token(SyntaxKind::OpenBracketToken) {
                // Handle computed property name - emit TS1164 and recover
                self.parse_error_at_current_token(
                    "Computed property names are not allowed in enums.",
                    diagnostic_codes::COMPUTED_PROPERTY_NAME_IN_ENUM,
                );
                self.parse_property_name()
            } else if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else if self.is_token(SyntaxKind::PrivateIdentifier) {
                self.parse_private_identifier()
            } else {
                self.parse_identifier_name()
            };

            let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
                self.parse_assignment_expression()
            } else {
                NodeIndex::NONE
            };

            let end_pos = self.token_end();
            let member = self.arena.add_enum_member(
                syntax_kind_ext::ENUM_MEMBER,
                start_pos,
                end_pos,
                EnumMemberData { name, initializer },
            );
            members.push(member);

            // Parse comma or recover with missing comma
            if !self.parse_optional(SyntaxKind::CommaToken) {
                // Recovery: If the next token looks like the start of a valid enum member,
                // emit TS1005 and continue parsing instead of breaking
                if self.is_token(SyntaxKind::Identifier)
                    || self.is_token(SyntaxKind::StringLiteral)
                    || self.is_token(SyntaxKind::PrivateIdentifier)
                    || self.is_token(SyntaxKind::OpenBracketToken)
                {
                    self.parse_error_at_current_token(
                        "',' expected",
                        diagnostic_codes::TOKEN_EXPECTED,
                    );
                    // Continue to next iteration to parse the next member
                    continue;
                }
                break;
            }
        }

        self.make_node_list(members)
    }

    // =========================================================================
    // Module/Namespace Declarations
    // =========================================================================

    /// Parse ambient declaration: declare function/class/namespace/var/etc.
    fn parse_ambient_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Create declare modifier node
        let declare_start = self.token_pos();
        self.parse_expected(SyntaxKind::DeclareKeyword);
        let declare_end = self.token_end();
        let declare_modifier = self.arena.add_token(
            SyntaxKind::DeclareKeyword as u16,
            declare_start,
            declare_end,
        );

        // Parse the inner declaration based on what follows 'declare'
        match self.token() {
            SyntaxKind::FunctionKeyword => {
                let modifiers = Some(self.make_node_list(vec![declare_modifier]));
                self.parse_function_declaration_with_async(false, modifiers)
            }
            SyntaxKind::ClassKeyword => self.parse_declare_class(start_pos, declare_modifier),
            SyntaxKind::AbstractKeyword => {
                // declare abstract class
                self.parse_declare_abstract_class(start_pos, declare_modifier)
            }
            SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(),
            SyntaxKind::TypeKeyword => self.parse_type_alias_declaration(),
            SyntaxKind::EnumKeyword => {
                let modifiers = Some(self.make_node_list(vec![declare_modifier]));
                self.parse_enum_declaration_with_modifiers(start_pos, modifiers)
            }
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                self.parse_declare_module(start_pos, declare_modifier)
            }
            SyntaxKind::GlobalKeyword => self.parse_declare_module(start_pos, declare_modifier),
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword => {
                let modifiers = self.make_node_list(vec![declare_modifier]);
                self.parse_variable_statement_with_modifiers(Some(start_pos), Some(modifiers))
            }
            SyntaxKind::ConstKeyword => {
                // declare const enum or declare const variable
                if self.look_ahead_is_const_enum() {
                    self.parse_const_enum_declaration(start_pos, vec![declare_modifier])
                } else {
                    let modifiers = self.make_node_list(vec![declare_modifier]);
                    self.parse_variable_statement_with_modifiers(Some(start_pos), Some(modifiers))
                }
            }
            SyntaxKind::AsyncKeyword => {
                // declare async function
                if self.look_ahead_is_async_function() {
                    // Pass the declare modifier to the function
                    self.parse_expected(SyntaxKind::AsyncKeyword);
                    let modifiers = Some(self.make_node_list(vec![declare_modifier]));
                    self.parse_function_declaration_with_async(true, modifiers)
                } else {
                    self.error_declaration_expected();
                    self.parse_expression_statement()
                }
            }
            _ => {
                self.error_declaration_expected();
                self.parse_expression_statement()
            }
        }
    }

    /// Parse module or namespace declaration: module "name" { } or namespace X { }
    fn parse_module_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_module_declaration_with_modifiers(start_pos, None)
    }

    fn parse_module_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        // Skip module/namespace/global keyword
        let is_global = self.is_token(SyntaxKind::GlobalKeyword);
        let name = if is_global {
            let name_start = self.token_pos();
            let name_end = self.token_end();
            self.next_token();
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                name_start,
                name_end,
                IdentifierData {
                    escaped_text: "global".to_string(),
                    original_text: None,
                    type_arguments: None,
                },
            )
        } else {
            self.next_token();
            // Check for anonymous module: module { ... }
            // This is invalid syntax but should parse gracefully without cascading errors
            if self.is_token(SyntaxKind::OpenBraceToken) {
                // Emit appropriate error for anonymous module (missing name)
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Namespace must be given a name.",
                    diagnostic_codes::IDENTIFIER_EXPECTED,
                );
                // Create a missing identifier for anonymous module
                let name_start = self.token_pos();
                let name_end = self.token_pos();
                self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    name_start,
                    name_end,
                    IdentifierData {
                        escaped_text: String::new(),
                        original_text: None,
                        type_arguments: None,
                    },
                )
            } else if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else {
                self.parse_identifier()
            }
        };

        // Parse body
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_module_block()
        } else if self.is_token(SyntaxKind::DotToken) {
            // Nested module: module A.B.C { }
            self.next_token();
            self.parse_nested_module_declaration(None)
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();

        let module_idx = self.arena.add_module(
            syntax_kind_ext::MODULE_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::thin_node::ModuleData {
                modifiers,
                name,
                body,
            },
        );

        if is_global {
            if let Some(node) = self.arena.get_mut(module_idx) {
                node.flags |= node_flags::GLOBAL_AUGMENTATION as u16;
            }
        }

        module_idx
    }

    /// Parse declare module: declare module "name" {}
    fn parse_declare_module(&mut self, start_pos: u32, declare_modifier: NodeIndex) -> NodeIndex {
        // Skip module/namespace/global keyword
        let is_global = self.is_token(SyntaxKind::GlobalKeyword);
        let modifiers = Some(self.make_node_list(vec![declare_modifier]));
        let name = if is_global {
            let name_start = self.token_pos();
            let name_end = self.token_end();
            self.next_token();
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                name_start,
                name_end,
                IdentifierData {
                    escaped_text: "global".to_string(),
                    original_text: None,
                    type_arguments: None,
                },
            )
        } else {
            self.next_token();
            // Check for anonymous module: module { ... }
            // This is invalid syntax but should parse gracefully without cascading errors
            if self.is_token(SyntaxKind::OpenBraceToken) {
                // Emit appropriate error for anonymous module (missing name)
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Namespace must be given a name.",
                    diagnostic_codes::IDENTIFIER_EXPECTED,
                );
                // Create a missing identifier for anonymous module
                let name_start = self.token_pos();
                let name_end = self.token_pos();
                self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    name_start,
                    name_end,
                    IdentifierData {
                        escaped_text: String::new(),
                        original_text: None,
                        type_arguments: None,
                    },
                )
            } else if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else {
                self.parse_identifier()
            }
        };

        // Parse body
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_module_block()
        } else if self.is_token(SyntaxKind::DotToken) {
            // Nested module: module A.B.C { }
            self.next_token();
            self.parse_nested_module_declaration(modifiers.clone())
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();

        let module_idx = self.arena.add_module(
            syntax_kind_ext::MODULE_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::thin_node::ModuleData {
                modifiers,
                name,
                body,
            },
        );

        if is_global {
            if let Some(node) = self.arena.get_mut(module_idx) {
                node.flags |= node_flags::GLOBAL_AUGMENTATION as u16;
            }
        }

        module_idx
    }

    fn parse_nested_module_declaration(&mut self, modifiers: Option<NodeList>) -> NodeIndex {
        let start_pos = self.token_pos();

        let name = if self.is_token(SyntaxKind::StringLiteral) {
            self.parse_string_literal()
        } else {
            // Allow keywords in dotted namespace segments (e.g., namespace chrome.debugger {})
            self.parse_identifier_name()
        };

        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_module_block()
        } else if self.is_token(SyntaxKind::DotToken) {
            self.next_token();
            self.parse_nested_module_declaration(modifiers.clone())
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();

        self.arena.add_module(
            syntax_kind_ext::MODULE_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::thin_node::ModuleData {
                modifiers,
                name,
                body,
            },
        )
    }

    /// Parse module name (can be dotted: A.B.C)
    fn parse_module_name(&mut self) -> NodeIndex {
        let mut left = self.parse_identifier();

        while self.is_token(SyntaxKind::DotToken) {
            self.next_token();
            let right = self.parse_identifier();
            let start = if let Some(n) = self.arena.get(left) {
                n.pos
            } else {
                0
            };
            let end = self.token_end();

            left = self.arena.add_qualified_name(
                syntax_kind_ext::QUALIFIED_NAME,
                start,
                end,
                QualifiedNameData { left, right },
            );
        }

        left
    }

    /// Parse module block: { statements }
    fn parse_module_block(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let statements = self.parse_statements();

        self.parse_expected(SyntaxKind::CloseBraceToken);
        let end_pos = self.token_end();

        self.arena.add_module_block(
            syntax_kind_ext::MODULE_BLOCK,
            start_pos,
            end_pos,
            crate::parser::thin_node::ModuleBlockData {
                statements: Some(statements),
            },
        )
    }

    // =========================================================================
    // Import/Export Declarations
    // =========================================================================

    /// Parse import declaration
    /// import x from "mod";
    /// import { x, y } from "mod";
    /// import * as x from "mod";
    /// import "mod";
    fn parse_import_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ImportKeyword);

        // Check for import "module" (no import clause)
        let import_clause = if self.is_token(SyntaxKind::StringLiteral) {
            NodeIndex::NONE
        } else {
            self.parse_import_clause()
        };

        // Parse module specifier
        let module_specifier = if !import_clause.is_none() {
            self.parse_expected(SyntaxKind::FromKeyword);
            self.parse_string_literal()
        } else {
            self.parse_string_literal()
        };

        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena.add_import_decl(
            syntax_kind_ext::IMPORT_DECLARATION,
            start_pos,
            end_pos,
            ImportDeclData {
                modifiers: None,
                import_clause,
                module_specifier,
                attributes: NodeIndex::NONE,
            },
        )
    }

    /// Parse import clause: default, namespace, or named imports
    fn parse_import_clause(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut is_type_only = false;

        // Check for "type" keyword (import type { ... })
        if self.is_token(SyntaxKind::TypeKeyword) {
            // Look ahead to see if this is "type" followed by identifier or "{"
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            if self.is_token(SyntaxKind::Identifier)
                || self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::AsteriskToken)
            {
                is_type_only = true;
            } else {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
            }
        }

        // Parse default import (identifier followed by "from" or ",")
        // For "import foo from", next token is "from"
        // For "import foo, { bar } from", next token is ","
        let name = if self.is_token(SyntaxKind::Identifier) {
            self.parse_identifier()
        } else {
            NodeIndex::NONE
        };

        // Parse comma if we have both default and named/namespace
        if !name.is_none() && self.parse_optional(SyntaxKind::CommaToken) {
            // Continue to parse named bindings
        }

        // Parse named bindings: * as ns or { x, y }
        let named_bindings = if self.is_token(SyntaxKind::AsteriskToken) {
            self.parse_namespace_import()
        } else if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_named_imports()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_import_clause(
            syntax_kind_ext::IMPORT_CLAUSE,
            start_pos,
            end_pos,
            ImportClauseData {
                is_type_only,
                name,
                named_bindings,
            },
        )
    }

    /// Check if next token is "from" keyword
    fn is_next_token_from(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let is_from = self.is_token(SyntaxKind::FromKeyword);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_from
    }

    /// Parse namespace import: * as name
    fn parse_namespace_import(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::AsteriskToken);
        self.parse_expected(SyntaxKind::AsKeyword);
        let name = self.parse_identifier();
        let end_pos = self.token_end();

        self.arena.add_named_imports(
            syntax_kind_ext::NAMESPACE_IMPORT,
            start_pos,
            end_pos,
            NamedImportsData {
                name,
                elements: self.make_node_list(Vec::new()),
            },
        )
    }

    /// Parse named imports: { x, y as z }
    fn parse_named_imports(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut elements = Vec::new();
        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            // Pattern 4: Import/Export specifier brace mismatch cascading error suppression
            // If we encounter 'from' keyword in the specifier list, it likely means we have:
            // import { a from "module"  (missing closing brace)
            // In this case, break the loop to avoid parsing 'from' as an identifier
            if self.is_token(SyntaxKind::FromKeyword) {
                break;
            }

            let spec = self.parse_import_specifier();
            elements.push(spec);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        self.parse_expected(SyntaxKind::CloseBraceToken);
        let end_pos = self.token_end();

        self.arena.add_named_imports(
            syntax_kind_ext::NAMED_IMPORTS,
            start_pos,
            end_pos,
            NamedImportsData {
                name: NodeIndex::NONE, // Not a namespace import
                elements: self.make_node_list(elements),
            },
        )
    }

    /// Parse import specifier: x or x as y
    fn parse_import_specifier(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut is_type_only = false;

        // Check for "type" keyword
        if self.is_token(SyntaxKind::TypeKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            if self.is_token(SyntaxKind::Identifier) {
                is_type_only = true;
            } else {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
            }
        }

        let first_name = self.parse_identifier_name();

        // Check for "as" alias
        let (property_name, name) = if self.parse_optional(SyntaxKind::AsKeyword) {
            let alias = self.parse_identifier_name();
            (first_name, alias)
        } else {
            (NodeIndex::NONE, first_name)
        };

        let end_pos = self.token_end();
        self.arena.add_specifier(
            syntax_kind_ext::IMPORT_SPECIFIER,
            start_pos,
            end_pos,
            SpecifierData {
                is_type_only,
                property_name,
                name,
            },
        )
    }

    /// Parse export declaration
    /// export { x, y };
    /// export { x } from "mod";
    /// export * from "mod";
    /// export default x;
    /// export function f() {}
    /// export class C {}
    fn parse_export_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ExportKeyword);

        // Check for type-only export vs export type alias
        // "export type { X }" or "export type * from" = type-only export
        // "export type X = Y" = exported type alias declaration
        let is_type_only = if self.is_token(SyntaxKind::TypeKeyword) {
            // Look ahead to see if this is a type-only export
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token(); // skip 'type'

            let is_type_only_export = self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::AsteriskToken);

            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if is_type_only_export {
                self.next_token(); // consume 'type' for type-only exports
                true
            } else {
                // Not a type-only export - leave 'type' for parse_export_declaration_or_statement
                false
            }
        } else {
            false
        };

        // export default ...
        if self.is_token(SyntaxKind::DefaultKeyword) {
            return self.parse_export_default(start_pos);
        }

        // export import X = Y (re-export of import equals)
        if self.is_token(SyntaxKind::ImportKeyword) {
            return self.parse_export_import_equals(start_pos);
        }

        // export * from "mod"
        if self.is_token(SyntaxKind::AsteriskToken) {
            return self.parse_export_star(start_pos, is_type_only);
        }

        // export { ... }
        if self.is_token(SyntaxKind::OpenBraceToken) {
            return self.parse_export_named(start_pos, is_type_only);
        }

        // export = expression (CommonJS-style export)
        if self.is_token(SyntaxKind::EqualsToken) {
            return self.parse_export_assignment(start_pos);
        }

        // export function/class/const/let/var/interface/type/enum
        self.parse_export_declaration_or_statement(start_pos)
    }

    /// Parse export import X = Y (re-export of import equals declaration)
    fn parse_export_import_equals(&mut self, start_pos: u32) -> NodeIndex {
        // Parse the import equals declaration
        let import_decl = self.parse_import_equals_declaration();

        let end_pos = self.token_end();

        // Wrap in an export declaration
        self.arena.add_export_decl(
            syntax_kind_ext::EXPORT_DECLARATION,
            start_pos,
            end_pos,
            ExportDeclData {
                modifiers: None,
                is_type_only: false,
                is_default_export: false,
                export_clause: import_decl,
                module_specifier: NodeIndex::NONE,
                attributes: NodeIndex::NONE,
            },
        )
    }

    /// Parse export = expression (CommonJS-style default export)
    fn parse_export_assignment(&mut self, start_pos: u32) -> NodeIndex {
        self.parse_expected(SyntaxKind::EqualsToken);
        let expression = self.parse_assignment_expression();
        self.parse_semicolon();

        let end_pos = self.token_end();

        self.arena.add_export_assignment(
            syntax_kind_ext::EXPORT_ASSIGNMENT,
            start_pos,
            end_pos,
            ExportAssignmentData {
                modifiers: None,
                is_export_equals: true,
                expression,
            },
        )
    }

    /// Parse export default
    fn parse_export_default(&mut self, start_pos: u32) -> NodeIndex {
        self.parse_expected(SyntaxKind::DefaultKeyword);

        // Parse the default expression or declaration
        let expression = match self.token() {
            SyntaxKind::FunctionKeyword => self.parse_function_declaration(),
            SyntaxKind::ClassKeyword => self.parse_class_declaration(),
            SyntaxKind::AbstractKeyword => self.parse_abstract_class_declaration(),
            _ => {
                let expr = self.parse_assignment_expression();
                self.parse_semicolon();
                expr
            }
        };

        let end_pos = self.token_end();
        // Use export assignment for default exports
        self.arena.add_export_decl(
            syntax_kind_ext::EXPORT_DECLARATION,
            start_pos,
            end_pos,
            ExportDeclData {
                modifiers: None,
                is_type_only: false,
                is_default_export: true,
                export_clause: expression,
                module_specifier: NodeIndex::NONE,
                attributes: NodeIndex::NONE,
            },
        )
    }

    /// Parse export * from "mod"
    fn parse_export_star(&mut self, start_pos: u32, is_type_only: bool) -> NodeIndex {
        self.parse_expected(SyntaxKind::AsteriskToken);

        // Optional "as namespace" for re-export
        let export_clause = if self.parse_optional(SyntaxKind::AsKeyword) {
            self.parse_identifier()
        } else {
            NodeIndex::NONE
        };

        self.parse_expected(SyntaxKind::FromKeyword);
        let module_specifier = self.parse_string_literal();
        self.parse_semicolon();

        let end_pos = self.token_end();
        self.arena.add_export_decl(
            syntax_kind_ext::EXPORT_DECLARATION,
            start_pos,
            end_pos,
            ExportDeclData {
                modifiers: None,
                is_type_only,
                is_default_export: false,
                export_clause,
                module_specifier,
                attributes: NodeIndex::NONE,
            },
        )
    }

    /// Parse export { x, y } or export { x } from "mod"
    fn parse_export_named(&mut self, start_pos: u32, is_type_only: bool) -> NodeIndex {
        let export_clause = self.parse_named_exports();

        let module_specifier = if self.parse_optional(SyntaxKind::FromKeyword) {
            self.parse_string_literal()
        } else {
            NodeIndex::NONE
        };

        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena.add_export_decl(
            syntax_kind_ext::EXPORT_DECLARATION,
            start_pos,
            end_pos,
            ExportDeclData {
                modifiers: None,
                is_type_only,
                is_default_export: false,
                export_clause,
                module_specifier,
                attributes: NodeIndex::NONE,
            },
        )
    }

    /// Parse named exports: { x, y as z }
    fn parse_named_exports(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut elements = Vec::new();
        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            // Pattern 4: Import/Export specifier brace mismatch cascading error suppression
            // If we encounter 'from' keyword in the specifier list, it likely means we have:
            // export { a from "module"  (missing closing brace)
            // In this case, break the loop to avoid parsing 'from' as an identifier
            if self.is_token(SyntaxKind::FromKeyword) {
                break;
            }

            let spec = self.parse_export_specifier();
            elements.push(spec);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        self.parse_expected(SyntaxKind::CloseBraceToken);
        let end_pos = self.token_end();

        self.arena.add_named_imports(
            syntax_kind_ext::NAMED_EXPORTS,
            start_pos,
            end_pos,
            NamedImportsData {
                name: NodeIndex::NONE, // Not a namespace export
                elements: self.make_node_list(elements),
            },
        )
    }

    /// Parse export specifier: x or x as y
    fn parse_export_specifier(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut is_type_only = false;

        // Check for "type" keyword
        if self.is_token(SyntaxKind::TypeKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            if self.is_token(SyntaxKind::Identifier) {
                is_type_only = true;
            } else {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
            }
        }

        let first_name = self.parse_identifier_name();

        // Check for "as" alias
        let (property_name, name) = if self.parse_optional(SyntaxKind::AsKeyword) {
            let alias = self.parse_identifier_name();
            (first_name, alias)
        } else {
            (NodeIndex::NONE, first_name)
        };

        let end_pos = self.token_end();
        self.arena.add_specifier(
            syntax_kind_ext::EXPORT_SPECIFIER,
            start_pos,
            end_pos,
            SpecifierData {
                is_type_only,
                property_name,
                name,
            },
        )
    }

    /// Parse exported declaration (export function, export class, etc.)
    fn parse_export_declaration_or_statement(&mut self, start_pos: u32) -> NodeIndex {
        // Parse the declaration and wrap it
        let declaration = match self.token() {
            SyntaxKind::FunctionKeyword => self.parse_function_declaration(),
            SyntaxKind::AsyncKeyword => {
                if self.look_ahead_is_async_function() {
                    self.parse_async_function_declaration()
                } else if self.look_ahead_is_async_declaration() {
                    let start_pos = self.token_pos();
                    self.error_async_modifier_cannot_be_used_here();
                    let async_start = self.token_pos();
                    self.parse_expected(SyntaxKind::AsyncKeyword);
                    let async_end = self.token_end();
                    let async_modifier = self.arena.add_token(
                        SyntaxKind::AsyncKeyword as u16,
                        async_start,
                        async_end,
                    );
                    let modifiers = Some(self.make_node_list(vec![async_modifier]));
                    match self.token() {
                        SyntaxKind::ClassKeyword => {
                            self.parse_class_declaration_with_modifiers(start_pos, modifiers)
                        }
                        SyntaxKind::EnumKeyword => {
                            self.parse_enum_declaration_with_modifiers(start_pos, modifiers)
                        }
                        SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(),
                        SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                            if self.look_ahead_is_module_declaration() {
                                self.parse_module_declaration()
                            } else {
                                self.parse_expression_statement()
                            }
                        }
                        _ => self.parse_expression_statement(),
                    }
                } else {
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::ClassKeyword => self.parse_class_declaration(),
            SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(),
            SyntaxKind::TypeKeyword => self.parse_type_alias_declaration(),
            SyntaxKind::EnumKeyword => self.parse_enum_declaration(),
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                self.parse_module_declaration()
            }
            SyntaxKind::AbstractKeyword => {
                // export abstract class ...
                self.parse_abstract_class_declaration()
            }
            SyntaxKind::DeclareKeyword => {
                // export declare function/class/namespace/var/etc.
                self.parse_ambient_declaration()
            }
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword => self.parse_variable_statement(),
            SyntaxKind::ConstKeyword => {
                // export const enum or export const variable
                if self.look_ahead_is_const_enum() {
                    self.parse_const_enum_declaration(self.token_pos(), Vec::new())
                } else {
                    self.parse_variable_statement()
                }
            }
            _ => {
                // Unsupported export
                self.error_statement_expected();
                self.parse_expression_statement()
            }
        };

        let end_pos = self.token_end();
        self.arena.add_export_decl(
            syntax_kind_ext::EXPORT_DECLARATION,
            start_pos,
            end_pos,
            ExportDeclData {
                modifiers: None,
                is_type_only: false,
                is_default_export: false,
                export_clause: declaration,
                module_specifier: NodeIndex::NONE,
                attributes: NodeIndex::NONE,
            },
        )
    }

    /// Parse a string literal (used for module specifiers)
    fn parse_string_literal(&mut self) -> NodeIndex {
        if !self.is_token(SyntaxKind::StringLiteral) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "String literal expected",
                diagnostic_codes::TOKEN_EXPECTED,
            );
            return NodeIndex::NONE;
        }

        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        let text = self.scanner.get_token_value_ref().to_string();
        self.next_token();

        self.arena.add_literal(
            SyntaxKind::StringLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    /// Parse if statement
    fn parse_if_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::IfKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);

        let expression = self.parse_expression();

        // Check for missing condition expression: if () { }
        if expression == NodeIndex::NONE {
            self.error_expression_expected();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);

        let then_statement = self.parse_statement();

        let else_statement = if self.parse_optional(SyntaxKind::ElseKeyword) {
            self.parse_statement()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_if_statement(
            syntax_kind_ext::IF_STATEMENT,
            start_pos,
            end_pos,
            IfStatementData {
                expression,
                then_statement,
                else_statement,
            },
        )
    }

    /// Parse return statement
    fn parse_return_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ReturnKeyword);

        // For restricted productions (return), ASI applies immediately after line break
        // Use can_parse_semicolon_for_restricted_production() instead of can_parse_semicolon()
        let expression = if !self.can_parse_semicolon_for_restricted_production() {
            self.parse_expression()
        } else {
            NodeIndex::NONE
        };

        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena.add_return(
            syntax_kind_ext::RETURN_STATEMENT,
            start_pos,
            end_pos,
            ReturnData { expression },
        )
    }

    /// Parse while statement
    fn parse_while_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::WhileKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);

        let condition = self.parse_expression();

        // Check for missing while condition: while () { }
        if condition == NodeIndex::NONE {
            self.error_expression_expected();
        }

        // Error recovery: if condition parsing failed badly, resync to close paren
        if condition.is_none() && !self.is_token(SyntaxKind::CloseParenToken) {
            self.resync_after_error();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);

        let statement = self.parse_statement();

        let end_pos = self.token_end();
        self.arena.add_loop(
            syntax_kind_ext::WHILE_STATEMENT,
            start_pos,
            end_pos,
            LoopData {
                initializer: NodeIndex::NONE,
                condition,
                incrementor: NodeIndex::NONE,
                statement,
            },
        )
    }

    /// Parse for statement (basic for loop only, not for-in/for-of yet)
    fn parse_for_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ForKeyword);

        // Check for for-await-of: for await (...)
        let await_modifier = self.parse_optional(SyntaxKind::AwaitKeyword);

        self.parse_expected(SyntaxKind::OpenParenToken);

        // Parse initializer (can be var/let/const declaration or expression)
        let initializer = if !self.is_token(SyntaxKind::SemicolonToken) {
            if self.is_token(SyntaxKind::VarKeyword)
                || self.is_token(SyntaxKind::LetKeyword)
                || self.is_token(SyntaxKind::ConstKeyword)
            {
                self.parse_for_variable_declaration()
            } else {
                self.parse_expression()
            }
        } else {
            NodeIndex::NONE
        };

        // Error recovery: if initializer parsing failed badly, resync to semicolon
        if initializer.is_none()
            && !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::InKeyword)
            && !self.is_token(SyntaxKind::OfKeyword)
        {
            self.resync_after_error();
        }

        // Check for for-in or for-of
        if self.is_token(SyntaxKind::InKeyword) {
            return self.parse_for_in_statement_rest(start_pos, initializer);
        }
        if self.is_token(SyntaxKind::OfKeyword) {
            return self.parse_for_of_statement_rest(start_pos, initializer, await_modifier);
        }

        // Regular for statement: for (init; cond; incr)
        self.parse_expected(SyntaxKind::SemicolonToken);

        // Condition
        let condition = if !self.is_token(SyntaxKind::SemicolonToken) {
            let cond = self.parse_expression();

            // Check for missing for condition: for (init; ; incr) when there was content to parse
            if cond == NodeIndex::NONE {
                self.error_expression_expected();
            }

            cond
        } else {
            NodeIndex::NONE
        };

        // Error recovery: if condition parsing failed badly, resync to semicolon
        if condition.is_none()
            && !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::CloseParenToken)
        {
            self.resync_after_error();
        }

        self.parse_expected(SyntaxKind::SemicolonToken);

        // Incrementor
        let incrementor = if !self.is_token(SyntaxKind::CloseParenToken) {
            let incr = self.parse_expression();

            // Check for missing for incrementor: for (init; cond; ) when there was content to parse
            if incr == NodeIndex::NONE {
                self.error_expression_expected();
            }

            incr
        } else {
            NodeIndex::NONE
        };

        // Error recovery: if incrementor parsing failed badly, resync to close paren
        if incrementor.is_none() && !self.is_token(SyntaxKind::CloseParenToken) {
            self.resync_after_error();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);

        let statement = self.parse_statement();

        let end_pos = self.token_end();
        self.arena.add_loop(
            syntax_kind_ext::FOR_STATEMENT,
            start_pos,
            end_pos,
            LoopData {
                initializer,
                condition,
                incrementor,
                statement,
            },
        )
    }

    /// Parse variable declaration list for for statement
    /// Supports multiple declarations for regular for: for (let x = 0, y = 1; ...)
    /// Single declaration for for-in/for-of: for (let x in/of ...)
    fn parse_for_variable_declaration(&mut self) -> NodeIndex {
        use crate::parser::node_flags;

        let start_pos = self.token_pos();
        let decl_keyword = self.token();
        let flags: u16 = match decl_keyword {
            SyntaxKind::LetKeyword => node_flags::LET as u16,
            SyntaxKind::ConstKeyword => node_flags::CONST as u16,
            _ => 0,
        };
        self.next_token(); // consume var/let/const

        let mut declarations = Vec::new();

        loop {
            let decl_start = self.token_pos();

            // Parse variable name (identifier or binding pattern)
            let name = if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_object_binding_pattern()
            } else if self.is_token(SyntaxKind::OpenBracketToken) {
                self.parse_array_binding_pattern()
            } else if self.is_identifier_or_keyword() {
                self.parse_identifier_name()
            } else {
                self.parse_identifier()
            };

            // Parse definite assignment assertion (!)
            let exclamation_token = self.parse_optional(SyntaxKind::ExclamationToken);

            // Optional type annotation
            let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_type()
            } else {
                NodeIndex::NONE
            };

            // Optional initializer
            let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
                self.parse_assignment_expression()
            } else {
                NodeIndex::NONE
            };

            let decl = self.arena.add_variable_declaration(
                syntax_kind_ext::VARIABLE_DECLARATION,
                decl_start,
                self.token_end(),
                VariableDeclarationData {
                    name,
                    exclamation_token,
                    type_annotation,
                    initializer,
                },
            );
            declarations.push(decl);

            // Check for comma (more declarations) or end of list
            // For for-in/for-of, stop at 'in' or 'of' keyword
            // For regular for, stop at ';' or ')'
            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        let declarations_list = self.make_node_list(declarations);
        let end_pos = self.token_end();

        self.arena.add_variable_with_flags(
            syntax_kind_ext::VARIABLE_DECLARATION_LIST,
            start_pos,
            end_pos,
            VariableData {
                modifiers: None,
                declarations: declarations_list,
            },
            flags,
        )
    }

    /// Parse for-in statement after initializer: for (x in obj)
    fn parse_for_in_statement_rest(&mut self, start_pos: u32, initializer: NodeIndex) -> NodeIndex {
        self.parse_expected(SyntaxKind::InKeyword);
        let expression = self.parse_expression();
        self.parse_expected(SyntaxKind::CloseParenToken);
        let statement = self.parse_statement();

        let end_pos = self.token_end();
        self.arena.add_for_in_of(
            syntax_kind_ext::FOR_IN_STATEMENT,
            start_pos,
            end_pos,
            crate::parser::thin_node::ForInOfData {
                await_modifier: false,
                initializer,
                expression,
                statement,
            },
        )
    }

    /// Parse for-of statement after initializer: for (x of arr)
    fn parse_for_of_statement_rest(
        &mut self,
        start_pos: u32,
        initializer: NodeIndex,
        await_modifier: bool,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::OfKeyword);
        let expression = self.parse_assignment_expression();
        self.parse_expected(SyntaxKind::CloseParenToken);
        let statement = self.parse_statement();

        let end_pos = self.token_end();
        self.arena.add_for_in_of(
            syntax_kind_ext::FOR_OF_STATEMENT,
            start_pos,
            end_pos,
            crate::parser::thin_node::ForInOfData {
                await_modifier,
                initializer,
                expression,
                statement,
            },
        )
    }

    /// Parse break statement
    fn parse_break_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::BreakKeyword);

        // For restricted productions (break), ASI applies immediately after line break
        // Use can_parse_semicolon_for_restricted_production() instead of can_parse_semicolon()
        // Optional label
        let label = if !self.can_parse_semicolon_for_restricted_production()
            && self.is_identifier_or_keyword()
        {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena.add_jump(
            syntax_kind_ext::BREAK_STATEMENT as u16,
            start_pos,
            end_pos,
            crate::parser::thin_node::JumpData { label },
        )
    }

    /// Parse continue statement
    fn parse_continue_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ContinueKeyword);

        // For restricted productions (continue), ASI applies immediately after line break
        // Use can_parse_semicolon_for_restricted_production() instead of can_parse_semicolon()
        // Optional label
        let label = if !self.can_parse_semicolon_for_restricted_production()
            && self.is_identifier_or_keyword()
        {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena.add_jump(
            syntax_kind_ext::CONTINUE_STATEMENT as u16,
            start_pos,
            end_pos,
            crate::parser::thin_node::JumpData { label },
        )
    }

    /// Parse throw statement
    fn parse_throw_statement(&mut self) -> NodeIndex {
        use crate::checker::types::diagnostics::diagnostic_codes;

        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ThrowKeyword);

        // TypeScript requires an expression after throw
        // If there's a line break immediately after throw, emit TS1109 (EXPRESSION_EXPECTED)
        let expression = if self.scanner.has_preceding_line_break()
            && !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            // Line break after throw without semicolon/brace/EOF - emit error
            let start = self.token_pos();
            let end = self.token_end();
            self.parse_error_at(
                start,
                end - start,
                "Expression expected",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            NodeIndex::NONE
        } else if !self.can_parse_semicolon_for_restricted_production() {
            self.parse_expression()
        } else {
            NodeIndex::NONE
        };

        self.parse_semicolon();
        let end_pos = self.token_end();

        // Use return statement node type for throw (same structure)
        self.arena.add_return(
            syntax_kind_ext::THROW_STATEMENT,
            start_pos,
            end_pos,
            ReturnData { expression },
        )
    }

    /// Parse do-while statement
    fn parse_do_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::DoKeyword);

        let statement = self.parse_statement();

        self.parse_expected(SyntaxKind::WhileKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);
        let condition = self.parse_expression();

        // Check for missing condition expression: do { } while ()
        if condition == NodeIndex::NONE {
            self.error_expression_expected();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);

        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena.add_loop(
            syntax_kind_ext::DO_STATEMENT,
            start_pos,
            end_pos,
            LoopData {
                initializer: NodeIndex::NONE,
                condition,
                incrementor: NodeIndex::NONE,
                statement,
            },
        )
    }

    /// Parse switch statement
    fn parse_switch_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::SwitchKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);

        let expression = self.parse_expression();

        // Check for missing switch expression: switch () { }
        if expression == NodeIndex::NONE {
            self.error_expression_expected();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);
        self.parse_expected(SyntaxKind::OpenBraceToken);

        // Parse case clauses
        let mut clauses = Vec::new();
        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            if self.is_token(SyntaxKind::CaseKeyword) {
                let clause_start = self.token_pos();
                self.next_token();
                let clause_expr = self.parse_expression();
                self.parse_expected(SyntaxKind::ColonToken);

                let mut statements = Vec::new();
                while !self.is_token(SyntaxKind::CaseKeyword)
                    && !self.is_token(SyntaxKind::DefaultKeyword)
                    && !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    statements.push(self.parse_statement());
                }

                let clause_end = self.token_end();
                clauses.push(self.arena.add_case_clause(
                    syntax_kind_ext::CASE_CLAUSE,
                    clause_start,
                    clause_end,
                    CaseClauseData {
                        expression: clause_expr,
                        statements: self.make_node_list(statements),
                    },
                ));
            } else if self.is_token(SyntaxKind::DefaultKeyword) {
                let clause_start = self.token_pos();
                self.next_token();
                self.parse_expected(SyntaxKind::ColonToken);

                let mut statements = Vec::new();
                while !self.is_token(SyntaxKind::CaseKeyword)
                    && !self.is_token(SyntaxKind::DefaultKeyword)
                    && !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    statements.push(self.parse_statement());
                }

                let clause_end = self.token_end();
                clauses.push(self.arena.add_case_clause(
                    syntax_kind_ext::DEFAULT_CLAUSE,
                    clause_start,
                    clause_end,
                    CaseClauseData {
                        expression: NodeIndex::NONE,
                        statements: self.make_node_list(statements),
                    },
                ));
            } else {
                // Unexpected token in switch body - emit error and recover
                if self.token_pos() != self.last_error_pos {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "case or default expected.",
                        diagnostic_codes::TOKEN_EXPECTED,
                    );
                }
                // Skip unexpected token and continue
                self.next_token();
            }
        }

        let case_block_end = self.token_end();
        self.parse_expected(SyntaxKind::CloseBraceToken);
        let end_pos = self.token_end();

        // Create the case block node
        let case_block = self.arena.add_block(
            syntax_kind_ext::CASE_BLOCK,
            start_pos, // Case block starts with the opening brace
            case_block_end,
            BlockData {
                statements: self.make_node_list(clauses),
                multi_line: true,
            },
        );

        self.arena.add_switch(
            syntax_kind_ext::SWITCH_STATEMENT,
            start_pos,
            end_pos,
            SwitchData {
                expression,
                case_block,
            },
        )
    }

    /// Parse try statement
    fn parse_try_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::TryKeyword);

        let try_block = self.parse_block();

        // Parse catch clause
        let catch_clause = if self.is_token(SyntaxKind::CatchKeyword) {
            let catch_start = self.token_pos();
            self.next_token();

            // Parse optional catch binding
            let variable_declaration = if self.is_token(SyntaxKind::OpenParenToken) {
                self.next_token();
                let decl = if self.is_token(SyntaxKind::CloseParenToken) {
                    NodeIndex::NONE
                } else {
                    self.parse_variable_declaration()
                };
                self.parse_expected(SyntaxKind::CloseParenToken);
                decl
            } else {
                NodeIndex::NONE
            };

            let catch_block = self.parse_block();
            let catch_end = self.token_end();

            self.arena.add_catch_clause(
                syntax_kind_ext::CATCH_CLAUSE,
                catch_start,
                catch_end,
                CatchClauseData {
                    variable_declaration,
                    block: catch_block,
                },
            )
        } else {
            NodeIndex::NONE
        };

        // Parse finally clause
        let finally_block = if self.is_token(SyntaxKind::FinallyKeyword) {
            self.next_token();
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        // Error recovery: try without catch or finally is invalid
        if catch_clause.is_none() && finally_block.is_none() {
            if self.token_pos() != self.last_error_pos {
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "catch or finally expected.",
                    diagnostic_codes::CATCH_OR_FINALLY_EXPECTED,
                );
            }
        }

        let end_pos = self.token_end();
        self.arena.add_try(
            syntax_kind_ext::TRY_STATEMENT,
            start_pos,
            end_pos,
            TryData {
                try_block,
                catch_clause,
                finally_block,
            },
        )
    }

    /// Parse with statement
    fn parse_with_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::WithKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);

        let expression = self.parse_expression();

        // Check for missing with expression: with () { }
        if expression == NodeIndex::NONE {
            self.error_expression_expected();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);

        let statement = self.parse_statement();

        let end_pos = self.token_end();

        // Use if statement structure for with (expression + statement)
        self.arena.add_if_statement(
            syntax_kind_ext::WITH_STATEMENT,
            start_pos,
            end_pos,
            IfStatementData {
                expression,
                then_statement: statement,
                else_statement: NodeIndex::NONE,
            },
        )
    }

    /// Parse debugger statement
    fn parse_debugger_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::DebuggerKeyword);
        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena.add_token(
            syntax_kind_ext::DEBUGGER_STATEMENT as u16,
            start_pos,
            end_pos,
        )
    }

    /// Parse expression statement
    fn parse_expression_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Early rejection: If the current token cannot start an expression, fail immediately
        // This prevents TS1109 from being emitted for tokens that are obviously not expressions
        // (e.g., }, ], ), etc.) when we fall through to parse_expression_statement() from
        // parse_statement()'s wildcard match.
        if !self.is_expression_start() {
            // Don't emit error here - let the statement-level error handling deal with it
            // Just return NONE to indicate failure
            return NodeIndex::NONE;
        }

        let expression = self.parse_expression();

        // If expression parsing failed completely, resync to recover
        if expression.is_none() {
            // Emit error for unexpected token if we haven't already
            if self.token_pos() != self.last_error_pos && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
            }
            // Try to parse semicolon for partial recovery, then resync
            let _ = self.can_parse_semicolon();
            if !self.is_token(SyntaxKind::SemicolonToken) {
                self.resync_after_error();
            } else {
                self.next_token();
            }
            return NodeIndex::NONE;
        }

        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena.add_expr_statement(
            syntax_kind_ext::EXPRESSION_STATEMENT,
            start_pos,
            end_pos,
            ExprStatementData { expression },
        )
    }

    // =========================================================================
    // Parse Methods - Expressions
    // =========================================================================

    /// Parse an expression (including comma operator)
    pub fn parse_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut left = self.parse_assignment_expression();

        // Handle comma operator: expr, expr, expr
        // Comma expressions create a sequence, returning the last value
        while self.is_token(SyntaxKind::CommaToken) {
            self.next_token(); // consume comma
            let right = self.parse_assignment_expression();
            if right.is_none() {
                // Emit TS1109 for trailing comma or missing expression: expr, [missing]
                self.error_expression_expected();
                break; // Exit loop to prevent cascading errors
            }
            let end_pos = self.token_end();

            left = self.arena.add_binary_expr(
                syntax_kind_ext::BINARY_EXPRESSION,
                start_pos,
                end_pos,
                BinaryExprData {
                    left,
                    operator_token: SyntaxKind::CommaToken as u16,
                    right,
                },
            );
        }

        left
    }

    /// Parse assignment expression
    fn parse_assignment_expression(&mut self) -> NodeIndex {
        // Check for arrow function first (including async arrow)
        if self.is_start_of_arrow_function() {
            // Check if it's an async arrow function
            // Note: `async => x` is a NON-async arrow where 'async' is the parameter name
            // `async x => x` or `async (x) => x` are async arrow functions
            if self.is_token(SyntaxKind::AsyncKeyword) {
                // Need to distinguish:
                // - `async => expr` (non-async, 'async' is param)
                // - `async x => expr` or `async (x) => expr` (async arrow)
                if self.look_ahead_is_simple_arrow_function() {
                    // async => expr - treat 'async' as identifier parameter
                    return self.parse_arrow_function_expression_with_async(false);
                }
                return self.parse_async_arrow_function_expression();
            }
            return self.parse_arrow_function_expression_with_async(false);
        }

        // Start at precedence 2 to skip comma operator (precedence 1)
        // Comma expressions are only valid in certain contexts (e.g., for loop)
        self.parse_binary_expression(2)
    }

    /// Parse async arrow function: async (x) => ... or async x => ...
    fn parse_async_arrow_function_expression(&mut self) -> NodeIndex {
        self.parse_expected(SyntaxKind::AsyncKeyword);
        self.parse_arrow_function_expression_with_async(true)
    }

    /// Check if we're at the start of an arrow function
    fn is_start_of_arrow_function(&mut self) -> bool {
        match self.token() {
            // (params) => ...
            SyntaxKind::OpenParenToken => self.look_ahead_is_arrow_function(),
            // async could be:
            // 1. async (x) => ... or async x => ... (async arrow function)
            // 2. async => ... (non-async arrow where 'async' is parameter name)
            SyntaxKind::AsyncKeyword => {
                // Check if 'async' is immediately followed by '=>'
                // If so, it's 'async' used as parameter name, not async modifier
                if self.look_ahead_is_simple_arrow_function() {
                    // async => expr - treat as simple arrow with 'async' as param
                    true
                } else {
                    // Check for async (x) => ... or async x => ...
                    self.look_ahead_is_arrow_function_after_async()
                }
            }
            // <T>(x) => ... (generic arrow function)
            SyntaxKind::LessThanToken => self.look_ahead_is_generic_arrow_function(),
            _ => self.is_identifier_or_keyword() && self.look_ahead_is_simple_arrow_function(),
        }
    }

    /// Look ahead to see if < starts a generic arrow function: <T>(x) => or <T, U>() =>
    fn look_ahead_is_generic_arrow_function(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip <
        self.next_token();

        // Skip type parameters until we find >
        let mut depth = 1;
        while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
            if self.is_token(SyntaxKind::LessThanToken) {
                depth += 1;
            } else if self.is_token(SyntaxKind::GreaterThanToken) {
                depth -= 1;
            }
            self.next_token();
        }

        // After >, should have (
        if !self.is_token(SyntaxKind::OpenParenToken) {
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return false;
        }

        // Now check if this is an arrow function
        let result = self.look_ahead_is_arrow_function();

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead after async to see if it's an arrow function: async (x) => or async x => or async <T>(x) =>
    ///
    /// ASI Rule: If there's a line break after 'async', it's NOT an async arrow function.
    /// The line break prevents 'async' from being treated as a modifier.
    /// Example: `async\nx => x` parses as `async; (x => x);` not as an async arrow function.
    fn look_ahead_is_arrow_function_after_async(&mut self) -> bool {
        // IMPORTANT: Check for line break BEFORE consuming 'async'
        // If there's a line break after 'async', it cannot be an async arrow function
        if self.scanner.has_preceding_line_break() {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip 'async'
        self.next_token();

        let result = match self.token() {
            // async (params) => ...
            SyntaxKind::OpenParenToken => self.look_ahead_is_arrow_function(),
            // async x => ...
            SyntaxKind::Identifier => self.look_ahead_is_simple_arrow_function(),
            // async <T>(x) => ... (generic async arrow)
            SyntaxKind::LessThanToken => self.look_ahead_is_generic_arrow_function(),
            _ => false,
        };

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead to see if ( starts an arrow function: () => or (x) => or (x, y) =>
    ///
    /// ASI Rule: If there's a line break between ) and =>, it's NOT an arrow function.
    /// Example: `(x)\n=> y` should NOT be parsed as an arrow function.
    fn look_ahead_is_arrow_function(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip (
        self.next_token();

        // Empty params: () => or (): type =>
        if self.is_token(SyntaxKind::CloseParenToken) {
            self.next_token();
            // Check for line break before =>
            let has_line_break = self.scanner.has_preceding_line_break();
            let is_arrow = if has_line_break {
                // Line break before => means this is not an arrow function (ASI applies)
                false
            } else if self.is_token(SyntaxKind::ColonToken) {
                let saved_arena_len = self.arena.nodes.len();
                let saved_diagnostics_len = self.parse_diagnostics.len();

                self.next_token();
                let _ = self.parse_return_type();
                let result = !self.scanner.has_preceding_line_break()
                    && self.is_token(SyntaxKind::EqualsGreaterThanToken);

                self.arena.nodes.truncate(saved_arena_len);
                self.parse_diagnostics.truncate(saved_diagnostics_len);

                result
            } else {
                self.is_token(SyntaxKind::EqualsGreaterThanToken)
            };
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return is_arrow;
        }

        // Skip to matching ) to check for =>
        let mut depth = 1;
        while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
            if self.is_token(SyntaxKind::OpenParenToken) {
                depth += 1;
            } else if self.is_token(SyntaxKind::CloseParenToken) {
                depth -= 1;
            }
            self.next_token();
        }

        // Check for line break before =>
        let has_line_break = self.scanner.has_preceding_line_break();

        // Check for optional return type annotation
        let is_arrow = if has_line_break {
            // Line break before => means this is not an arrow function (ASI applies)
            false
        } else if self.is_token(SyntaxKind::ColonToken) {
            let saved_arena_len = self.arena.nodes.len();
            let saved_diagnostics_len = self.parse_diagnostics.len();

            self.next_token();
            let _ = self.parse_return_type();
            let result = !self.scanner.has_preceding_line_break()
                && self.is_token(SyntaxKind::EqualsGreaterThanToken);

            self.arena.nodes.truncate(saved_arena_len);
            self.parse_diagnostics.truncate(saved_diagnostics_len);

            result
        } else {
            self.is_token(SyntaxKind::EqualsGreaterThanToken)
        };
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_arrow
    }

    /// Look ahead to see if identifier is followed by => (simple arrow function)
    ///
    /// ASI Rule: If there's a line break between the identifier and =>, it's NOT an arrow function.
    /// Example: `x\n=> y` should NOT be parsed as an arrow function.
    fn look_ahead_is_simple_arrow_function(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip identifier
        self.next_token();

        // Check if => is immediately after identifier (no line break)
        // If there's a line break, ASI applies and this is not an arrow function
        let is_arrow = !self.scanner.has_preceding_line_break()
            && self.is_token(SyntaxKind::EqualsGreaterThanToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_arrow
    }

    /// Parse arrow function expression: (params) => body or x => body or <T>(x) => body
    fn parse_arrow_function_expression_with_async(&mut self, is_async: bool) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse optional type parameters: <T, U extends Foo>
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        // Parse parameters
        let parameters = if self.is_token(SyntaxKind::OpenParenToken) {
            // Parenthesized parameter list: (a, b) =>
            self.parse_expected(SyntaxKind::OpenParenToken);
            let params = self.parse_parameter_list();
            self.parse_expected(SyntaxKind::CloseParenToken);
            params
        } else {
            // Single identifier parameter: x => or async => (where async is used as identifier)
            let param_start = self.token_pos();
            // Use parse_identifier_name to allow keywords like 'async' as parameter names
            let name = self.parse_identifier_name();
            let param_end = self.token_end();

            let param = self.arena.add_parameter(
                syntax_kind_ext::PARAMETER,
                param_start,
                param_end,
                crate::parser::thin_node::ParameterData {
                    modifiers: None,
                    dot_dot_dot_token: false,
                    name,
                    question_token: false,
                    type_annotation: NodeIndex::NONE,
                    initializer: NodeIndex::NONE,
                },
            );
            self.make_node_list(vec![param])
        };

        // Parse optional return type annotation (supports type predicates: x is T)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        // Recovery: Handle missing fat arrow - common typo: (a, b) { return a; }
        // If we see { immediately after parameters/return type, the user forgot =>
        if self.is_token(SyntaxKind::OpenBraceToken) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token("'=>' expected.", diagnostic_codes::TOKEN_EXPECTED);
            // Don't consume the {, just continue to body parsing
            // The arrow is logically present but missing
        } else {
            // Normal case: expect =>
            self.parse_expected(SyntaxKind::EqualsGreaterThanToken);
        }

        // Set async context for body parsing
        let saved_flags = self.context_flags;
        if is_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }

        // Parse body (block or expression)
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            self.parse_assignment_expression()
        };

        // Restore context flags
        self.context_flags = saved_flags;

        let end_pos = self.token_end();

        self.arena.add_function(
            syntax_kind_ext::ARROW_FUNCTION,
            start_pos,
            end_pos,
            FunctionData {
                modifiers: None,
                is_async,
                asterisk_token: false,
                name: NodeIndex::NONE,
                type_parameters,
                parameters,
                type_annotation,
                body,
                equals_greater_than_token: true,
            },
        )
    }

    /// Parse type parameters: <T, U extends Foo, V = DefaultType>
    fn parse_type_parameters(&mut self) -> NodeList {
        let mut params = Vec::new();

        self.parse_expected(SyntaxKind::LessThanToken);

        while !self.is_greater_than_or_compound() && !self.is_token(SyntaxKind::EndOfFileToken) {
            let param = self.parse_type_parameter();
            params.push(param);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        self.parse_expected_greater_than();

        self.make_node_list(params)
    }

    /// Parse a single type parameter: T or T extends U or T = Default or T extends U = Default
    fn parse_type_parameter(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the type parameter name
        let name = self.parse_identifier();

        // Parse optional constraint: extends SomeType
        let constraint = if self.parse_optional(SyntaxKind::ExtendsKeyword) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        // Parse optional default: = DefaultType
        let default = if self.parse_optional(SyntaxKind::EqualsToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();

        self.arena.add_type_parameter(
            syntax_kind_ext::TYPE_PARAMETER,
            start_pos,
            end_pos,
            crate::parser::thin_node::TypeParameterData {
                modifiers: None,
                name,
                constraint,
                default,
            },
        )
    }

    /// Parse binary expression with precedence climbing
    fn parse_binary_expression(&mut self, min_precedence: u8) -> NodeIndex {
        // Check recursion limit for deeply nested expressions
        if !self.enter_recursion() {
            return NodeIndex::NONE;
        }

        let start_pos = self.token_pos();
        let mut left = self.parse_unary_expression();

        loop {
            // Try to rescan > as >>, >>>, >=, >>=, >>>= for binary operators
            let op = if self.is_token(SyntaxKind::GreaterThanToken) {
                self.try_rescan_greater_token()
            } else {
                self.token()
            };
            let precedence = self.get_operator_precedence(op);

            if precedence == 0 || precedence < min_precedence {
                break;
            }

            if op == SyntaxKind::AsKeyword || op == SyntaxKind::SatisfiesKeyword {
                left = self.parse_as_or_satisfies_expression(left, start_pos);
                continue;
            }

            let operator_token = op as u16;
            self.next_token();

            // Handle conditional expression
            if op == SyntaxKind::QuestionToken {
                let when_true = self.parse_assignment_expression();
                if when_true.is_none() {
                    // Emit TS1109 for incomplete conditional expression: condition ? [missing]
                    self.error_expression_expected();
                    self.resync_to_next_expression_boundary();
                }
                self.parse_expected(SyntaxKind::ColonToken);
                let when_false = self.parse_assignment_expression();
                if when_false.is_none() {
                    // Emit TS1109 for incomplete conditional expression: condition ? true : [missing]
                    self.error_expression_expected();
                    self.resync_to_next_expression_boundary();
                }
                let end_pos = self.token_end();

                left = self.arena.add_conditional_expr(
                    syntax_kind_ext::CONDITIONAL_EXPRESSION,
                    start_pos,
                    end_pos,
                    ConditionalExprData {
                        condition: left,
                        when_true,
                        when_false,
                    },
                );
            } else {
                // Right associativity for assignment and exponentiation
                // For assignment operators, use parse_assignment_expression to allow arrow functions on RHS
                let is_assignment = matches!(
                    op,
                    SyntaxKind::EqualsToken
                        | SyntaxKind::PlusEqualsToken
                        | SyntaxKind::MinusEqualsToken
                        | SyntaxKind::AsteriskEqualsToken
                        | SyntaxKind::SlashEqualsToken
                        | SyntaxKind::PercentEqualsToken
                        | SyntaxKind::AsteriskAsteriskEqualsToken
                        | SyntaxKind::LessThanLessThanEqualsToken
                        | SyntaxKind::GreaterThanGreaterThanEqualsToken
                        | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                        | SyntaxKind::AmpersandEqualsToken
                        | SyntaxKind::CaretEqualsToken
                        | SyntaxKind::BarEqualsToken
                        | SyntaxKind::BarBarEqualsToken
                        | SyntaxKind::AmpersandAmpersandEqualsToken
                        | SyntaxKind::QuestionQuestionEqualsToken
                );

                let right = if is_assignment {
                    let result = self.parse_assignment_expression();
                    if result.is_none() {
                        // Emit TS1109 for incomplete assignment RHS: a = [missing]
                        self.error_expression_expected();
                        self.resync_to_next_expression_boundary();
                        // Break out of binary expression loop when parsing fails to prevent infinite loops
                        return left;
                    }
                    result
                } else {
                    let next_min = if op == SyntaxKind::AsteriskAsteriskToken {
                        precedence // right associative
                    } else {
                        precedence + 1
                    };
                    let result = self.parse_binary_expression(next_min);
                    if result.is_none() {
                        // Emit TS1109 for incomplete binary expression: a + [missing]
                        self.error_expression_expected();
                        self.resync_to_next_expression_boundary();
                        // Break out of binary expression loop when parsing fails to prevent infinite loops
                        return left;
                    }
                    result
                };
                let end_pos = self.token_end();

                let final_right = if right.is_none() { left } else { right };

                left = self.arena.add_binary_expr(
                    syntax_kind_ext::BINARY_EXPRESSION,
                    start_pos,
                    end_pos,
                    BinaryExprData {
                        left,
                        operator_token,
                        right: final_right,
                    },
                );
            }
        }

        self.exit_recursion();
        left
    }

    /// Parse as/satisfies expression: expr as Type, expr satisfies Type
    /// Also handles const assertion: expr as const
    fn parse_as_or_satisfies_expression(
        &mut self,
        expression: NodeIndex,
        start_pos: u32,
    ) -> NodeIndex {
        let is_satisfies = self.is_token(SyntaxKind::SatisfiesKeyword);
        self.next_token(); // consume 'as' or 'satisfies'

        // Handle 'as const' - const assertion
        let type_node = if !is_satisfies && self.is_token(SyntaxKind::ConstKeyword) {
            // Create a token node for 'const' keyword
            let const_start = self.token_pos();
            let const_end = self.token_end();
            self.next_token(); // consume 'const'
            self.arena
                .add_token(SyntaxKind::ConstKeyword as u16, const_start, const_end)
        } else {
            self.parse_type()
        };
        let end_pos = self.token_end();

        let result = self.arena.add_type_assertion(
            if is_satisfies {
                syntax_kind_ext::SATISFIES_EXPRESSION
            } else {
                syntax_kind_ext::AS_EXPRESSION
            },
            start_pos,
            end_pos,
            crate::parser::thin_node::TypeAssertionData {
                expression,
                type_node,
            },
        );

        // Allow chaining: x as T as U
        if self.is_token(SyntaxKind::AsKeyword) || self.is_token(SyntaxKind::SatisfiesKeyword) {
            return self.parse_as_or_satisfies_expression(result, start_pos);
        }

        result
    }

    /// Parse unary expression
    fn parse_unary_expression(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::TildeToken
            | SyntaxKind::ExclamationToken
            | SyntaxKind::PlusPlusToken
            | SyntaxKind::MinusMinusToken => {
                let start_pos = self.token_pos();
                let operator = self.token() as u16;
                self.next_token();
                let operand = self.parse_unary_expression();
                if operand.is_none() {
                    // Emit TS1109 for incomplete unary expression: +[missing], ++[missing], etc.
                    self.error_expression_expected();
                }
                let end_pos = self.token_end();

                self.arena.add_unary_expr(
                    syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
                    start_pos,
                    end_pos,
                    UnaryExprData { operator, operand },
                )
            }
            SyntaxKind::TypeOfKeyword | SyntaxKind::VoidKeyword | SyntaxKind::DeleteKeyword => {
                let start_pos = self.token_pos();
                let operator = self.token() as u16;
                self.next_token();
                let operand = self.parse_unary_expression();
                if operand.is_none() {
                    // Emit TS1109 for incomplete unary expression: typeof[missing], void[missing], delete[missing]
                    self.error_expression_expected();
                }
                let end_pos = self.token_end();

                self.arena.add_unary_expr(
                    syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
                    start_pos,
                    end_pos,
                    UnaryExprData { operator, operand },
                )
            }
            SyntaxKind::AwaitKeyword => {
                // Only parse as await expression if we're in an async context AND NOT in a parameter default
                // Parameter defaults are evaluated in the parent scope, not the async function body
                if !self.in_async_context() || self.in_parameter_default_context() {
                    // In parameter default context of non-async functions, 'await' should always be treated as identifier
                    if self.in_parameter_default_context() && !self.in_async_context() {
                        // Parse 'await' as regular identifier in parameter defaults of non-async functions
                        let start_pos = self.token_pos();
                        let end_pos = self.token_end(); // capture end before consuming
                        self.next_token(); // consume the await token
                        return self.arena.add_identifier(
                            SyntaxKind::Identifier as u16,
                            start_pos,
                            end_pos,
                            crate::parser::thin_node::IdentifierData {
                                escaped_text: String::from("await"),
                                original_text: None,
                                type_arguments: None,
                            },
                        );
                    }

                    // Outside async context or in other contexts, check if await is used as a bare expression
                    // If followed by tokens that can't start an expression, report "Expression expected"
                    // Examples where await is a reserved identifier but invalid as expression:
                    //   await;  // Error: Expression expected (in static blocks)
                    //   await (1);  // Error: Expression expected (in static blocks)
                    //   async (a = await => x) => {}  // Error: Expression expected (before arrow)

                    // Look ahead to see what token comes after 'await'
                    let snapshot = self.scanner.save_state();
                    let current_token = self.current_token;
                    self.next_token(); // consume 'await'
                    let next_token = self.token();
                    self.scanner.restore_state(snapshot);
                    self.current_token = current_token;

                    let has_following_expression = !matches!(
                        next_token,
                        SyntaxKind::SemicolonToken
                            | SyntaxKind::CloseBracketToken
                            | SyntaxKind::CommaToken
                            | SyntaxKind::ColonToken
                            | SyntaxKind::EqualsGreaterThanToken
                            | SyntaxKind::EndOfFileToken
                    );

                    // Special case: Don't emit TS1109 for 'await' in computed property names like { [await]: foo }
                    // In this context, 'await' is used as an identifier and CloseBracketToken is expected
                    let is_computed_property_context = next_token == SyntaxKind::CloseBracketToken;

                    if !has_following_expression && !is_computed_property_context {
                        self.error_expression_expected();
                    }

                    // Fall through to parse as identifier/postfix expression
                    return self.parse_postfix_expression();
                }

                // In async context, parse as await expression
                let start_pos = self.token_pos();
                self.next_token();

                // Check for missing operand (e.g., just "await" with nothing after it)
                if self.can_parse_semicolon()
                    || self.is_token(SyntaxKind::SemicolonToken)
                    || !self.is_expression_start()
                {
                    self.error_expression_expected();
                }

                let expression = self.parse_unary_expression();
                let end_pos = self.token_end();

                self.arena.add_unary_expr_ex(
                    syntax_kind_ext::AWAIT_EXPRESSION,
                    start_pos,
                    end_pos,
                    UnaryExprDataEx {
                        expression,
                        asterisk_token: false,
                    },
                )
            }
            SyntaxKind::YieldKeyword => {
                let start_pos = self.token_pos();
                self.next_token();

                // Check for yield* (delegate yield)
                let asterisk_token = self.parse_optional(SyntaxKind::AsteriskToken);

                // Parse the expression (may be empty for bare yield)
                let expression = if !self.scanner.has_preceding_line_break()
                    && !self.is_token(SyntaxKind::SemicolonToken)
                    && !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::CloseParenToken)
                    && !self.is_token(SyntaxKind::CloseBracketToken)
                    && !self.is_token(SyntaxKind::ColonToken)
                    && !self.is_token(SyntaxKind::CommaToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    self.parse_assignment_expression()
                } else {
                    NodeIndex::NONE
                };

                let end_pos = self.token_end();

                self.arena.add_unary_expr_ex(
                    syntax_kind_ext::YIELD_EXPRESSION,
                    start_pos,
                    end_pos,
                    UnaryExprDataEx {
                        expression,
                        asterisk_token,
                    },
                )
            }
            _ => self.parse_postfix_expression(),
        }
    }

    /// Parse postfix expression
    fn parse_postfix_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_left_hand_side_expression();

        // Handle postfix operators
        if !self.scanner.has_preceding_line_break() {
            if self.is_token(SyntaxKind::PlusPlusToken)
                || self.is_token(SyntaxKind::MinusMinusToken)
            {
                let operator = self.token() as u16;
                self.next_token();
                let end_pos = self.token_end();

                expr = self.arena.add_unary_expr(
                    syntax_kind_ext::POSTFIX_UNARY_EXPRESSION,
                    start_pos,
                    end_pos,
                    UnaryExprData {
                        operator,
                        operand: expr,
                    },
                );
            }
        }

        expr
    }

    /// Parse left-hand side expression (member access, call, etc.)
    fn parse_left_hand_side_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_primary_expression();

        loop {
            match self.token() {
                SyntaxKind::DotToken => {
                    self.next_token();
                    // Handle both regular identifiers and private identifiers (#name)
                    let name = if self.is_token(SyntaxKind::PrivateIdentifier) {
                        self.parse_private_identifier()
                    } else if self.is_identifier_or_keyword() {
                        self.parse_identifier_name()
                    } else {
                        self.error_identifier_expected();
                        NodeIndex::NONE
                    };
                    let end_pos = self.token_end();

                    expr = self.arena.add_access_expr(
                        syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                        start_pos,
                        end_pos,
                        AccessExprData {
                            expression: expr,
                            name_or_argument: name,
                            question_dot_token: false,
                        },
                    );
                }
                SyntaxKind::OpenBracketToken => {
                    self.next_token();
                    let argument = self.parse_expression();
                    if argument.is_none() {
                        // Emit TS1109 for empty brackets or invalid expression: obj[[missing]]
                        self.error_expression_expected();
                    }
                    let end_pos = self.token_end();
                    self.parse_expected(SyntaxKind::CloseBracketToken);

                    expr = self.arena.add_access_expr(
                        syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION,
                        start_pos,
                        end_pos,
                        AccessExprData {
                            expression: expr,
                            name_or_argument: argument,
                            question_dot_token: false,
                        },
                    );
                }
                SyntaxKind::OpenParenToken => {
                    let callee_expr = expr;
                    self.next_token();
                    let arguments = self.parse_argument_list();
                    let end_pos = self.token_end();
                    self.parse_expected(SyntaxKind::CloseParenToken);

                    let is_optional_chain = self
                        .arena
                        .get(callee_expr)
                        .and_then(|callee_node| self.arena.get_access_expr(callee_node))
                        .is_some_and(|access| access.question_dot_token);
                    let call_expr = self.arena.add_call_expr(
                        syntax_kind_ext::CALL_EXPRESSION,
                        start_pos,
                        end_pos,
                        CallExprData {
                            expression: expr,
                            type_arguments: None,
                            arguments: Some(arguments),
                        },
                    );
                    if is_optional_chain {
                        if let Some(call_node) = self.arena.get_mut(call_expr) {
                            call_node.flags |= node_flags::OPTIONAL_CHAIN as u16;
                        }
                    }
                    expr = call_expr;
                }
                // Tagged template literals: tag`template` or tag`head${expr}tail`
                SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead => {
                    let template = self.parse_template_literal();
                    let end_pos = self.token_end();

                    expr = self.arena.add_tagged_template(
                        syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
                        start_pos,
                        end_pos,
                        TaggedTemplateData {
                            tag: expr,
                            type_arguments: None,
                            template,
                        },
                    );
                }
                // Optional chaining: expr?.prop, expr?.[index], expr?.()
                SyntaxKind::QuestionDotToken => {
                    self.next_token();
                    if self.is_token(SyntaxKind::LessThanToken) {
                        if let Some(type_args) = self.try_parse_type_arguments_for_call() {
                            if self.is_token(SyntaxKind::OpenParenToken) {
                                // expr?.<T>()
                                self.next_token();
                                let arguments = self.parse_argument_list();
                                let end_pos = self.token_end();
                                self.parse_expected(SyntaxKind::CloseParenToken);

                                let call_expr = self.arena.add_call_expr(
                                    syntax_kind_ext::CALL_EXPRESSION,
                                    start_pos,
                                    end_pos,
                                    CallExprData {
                                        expression: expr,
                                        type_arguments: Some(type_args),
                                        arguments: Some(arguments),
                                    },
                                );
                                if let Some(call_node) = self.arena.get_mut(call_expr) {
                                    call_node.flags |= node_flags::OPTIONAL_CHAIN as u16;
                                }
                                expr = call_expr;
                                continue;
                            } else if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
                                || self.is_token(SyntaxKind::TemplateHead)
                            {
                                let template = self.parse_template_literal();
                                let end_pos = self.token_end();

                                expr = self.arena.add_tagged_template(
                                    syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
                                    start_pos,
                                    end_pos,
                                    TaggedTemplateData {
                                        tag: expr,
                                        type_arguments: Some(type_args),
                                        template,
                                    },
                                );
                                continue;
                            }
                        }
                    }
                    if self.is_token(SyntaxKind::OpenBracketToken) {
                        // expr?.[index]
                        self.next_token();
                        let argument = self.parse_expression();
                        let end_pos = self.token_end();
                        self.parse_expected(SyntaxKind::CloseBracketToken);

                        expr = self.arena.add_access_expr(
                            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION,
                            start_pos,
                            end_pos,
                            AccessExprData {
                                expression: expr,
                                name_or_argument: argument,
                                question_dot_token: true,
                            },
                        );
                    } else if self.is_token(SyntaxKind::OpenParenToken) {
                        // expr?.()
                        self.next_token();
                        let arguments = self.parse_argument_list();
                        let end_pos = self.token_end();
                        self.parse_expected(SyntaxKind::CloseParenToken);

                        let call_expr = self.arena.add_call_expr(
                            syntax_kind_ext::CALL_EXPRESSION,
                            start_pos,
                            end_pos,
                            CallExprData {
                                expression: expr,
                                type_arguments: None,
                                arguments: Some(arguments),
                            },
                        );
                        if let Some(call_node) = self.arena.get_mut(call_expr) {
                            call_node.flags |= node_flags::OPTIONAL_CHAIN as u16;
                        }
                        expr = call_expr;
                    } else {
                        // expr?.prop
                        let name = if self.is_token(SyntaxKind::PrivateIdentifier) {
                            self.parse_private_identifier()
                        } else {
                            self.parse_identifier_name()
                        };
                        let end_pos = self.token_end();

                        expr = self.arena.add_access_expr(
                            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                            start_pos,
                            end_pos,
                            AccessExprData {
                                expression: expr,
                                name_or_argument: name,
                                question_dot_token: true,
                            },
                        );
                    }
                }
                // Non-null assertion: expr!
                SyntaxKind::ExclamationToken => {
                    // Non-null assertion only if no line break before
                    if self.scanner.has_preceding_line_break() {
                        break;
                    }
                    self.next_token();
                    let end_pos = self.token_end();

                    expr = self.arena.add_unary_expr_ex(
                        syntax_kind_ext::NON_NULL_EXPRESSION,
                        start_pos,
                        end_pos,
                        crate::parser::thin_node::UnaryExprDataEx {
                            expression: expr,
                            asterisk_token: false,
                        },
                    );
                }
                // Type arguments followed by call: expr<T>() or expr<T, U>()
                SyntaxKind::LessThanToken => {
                    // Try to parse as type arguments for a call expression
                    // This is tricky because < could be comparison operator
                    if let Some(type_args) = self.try_parse_type_arguments_for_call() {
                        // After type arguments, we expect ( for a call or ` for tagged template
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            self.next_token();
                            let arguments = self.parse_argument_list();
                            let end_pos = self.token_end();
                            self.parse_expected(SyntaxKind::CloseParenToken);

                            expr = self.arena.add_call_expr(
                                syntax_kind_ext::CALL_EXPRESSION,
                                start_pos,
                                end_pos,
                                CallExprData {
                                    expression: expr,
                                    type_arguments: Some(type_args),
                                    arguments: Some(arguments),
                                },
                            );
                        } else if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
                            || self.is_token(SyntaxKind::TemplateHead)
                        {
                            // Tagged template with type arguments: tag<T>`template`
                            let template = self.parse_template_literal();
                            let end_pos = self.token_end();

                            expr = self.arena.add_tagged_template(
                                syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
                                start_pos,
                                end_pos,
                                TaggedTemplateData {
                                    tag: expr,
                                    type_arguments: Some(type_args),
                                    template,
                                },
                            );
                        } else {
                            // Not a call - leave type args attached (expression with type args)
                            break;
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        expr
    }

    /// Parse argument list
    fn parse_argument_list(&mut self) -> NodeList {
        let mut args = Vec::new();

        while !self.is_token(SyntaxKind::CloseParenToken) {
            if self.is_token(SyntaxKind::DotDotDotToken) {
                let spread_start = self.token_pos();
                self.next_token();
                let expression = self.parse_assignment_expression();
                if expression.is_none() {
                    // Emit TS1109 for incomplete spread argument: func(...missing)
                    self.error_expression_expected();
                }
                let spread_end = self.token_end();
                let spread = self.arena.add_spread(
                    syntax_kind_ext::SPREAD_ELEMENT,
                    spread_start,
                    spread_end,
                    crate::parser::thin_node::SpreadData { expression },
                );
                args.push(spread);
            } else {
                let arg = self.parse_assignment_expression();
                if arg.is_none() {
                    // Emit TS1109 for missing function argument: func(a, , c)
                    self.error_expression_expected();
                    // Continue parsing for error recovery
                }
                args.push(arg);
            }

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        self.make_node_list(args)
    }

    /// Parse primary expression
    fn parse_primary_expression(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::Identifier => self.parse_identifier(),
            SyntaxKind::PrivateIdentifier => self.parse_private_identifier(),
            SyntaxKind::NumericLiteral => self.parse_numeric_literal(),
            SyntaxKind::BigIntLiteral => self.parse_bigint_literal(),
            SyntaxKind::StringLiteral => self.parse_string_literal(),
            SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword => self.parse_boolean_literal(),
            SyntaxKind::NullKeyword => self.parse_null_literal(),
            SyntaxKind::UndefinedKeyword => self.parse_keyword_as_identifier(),
            SyntaxKind::ThisKeyword => self.parse_this_expression(),
            SyntaxKind::SuperKeyword => self.parse_super_expression(),
            SyntaxKind::OpenParenToken => self.parse_parenthesized_expression(),
            SyntaxKind::OpenBracketToken => self.parse_array_literal(),
            SyntaxKind::OpenBraceToken => self.parse_object_literal(),
            SyntaxKind::NewKeyword => self.parse_new_expression(),
            SyntaxKind::FunctionKeyword => self.parse_function_expression(),
            SyntaxKind::ClassKeyword => self.parse_class_expression(),
            SyntaxKind::AsyncKeyword => {
                // async function expression or async arrow function
                if self.look_ahead_is_async_function() {
                    self.parse_async_function_expression()
                } else {
                    // 'async' used as identifier (e.g., variable named async)
                    // Use parse_identifier_name since 'async' is a keyword
                    self.parse_identifier_name()
                }
            }
            SyntaxKind::LessThanToken => self.parse_jsx_element_or_type_assertion(),
            SyntaxKind::NoSubstitutionTemplateLiteral => {
                self.parse_no_substitution_template_literal()
            }
            SyntaxKind::TemplateHead => self.parse_template_expression(),
            // Regex literal - rescan / or /= as regex
            SyntaxKind::SlashToken | SyntaxKind::SlashEqualsToken => self.parse_regex_literal(),
            // Dynamic import or import.meta
            SyntaxKind::ImportKeyword => self.parse_import_expression(),
            // Type keywords and some reserved words can be used as identifiers in expression context
            // e.g., new any[1], new string(), new require() (when require is aliased), etc.
            SyntaxKind::AnyKeyword
            | SyntaxKind::StringKeyword
            | SyntaxKind::NumberKeyword
            | SyntaxKind::BooleanKeyword
            | SyntaxKind::SymbolKeyword
            | SyntaxKind::BigIntKeyword
            | SyntaxKind::ObjectKeyword
            | SyntaxKind::NeverKeyword
            | SyntaxKind::UnknownKeyword
            | SyntaxKind::RequireKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::AwaitKeyword
            | SyntaxKind::YieldKeyword => self.parse_keyword_as_identifier(),
            _ => {
                // Don't consume clause boundaries or expression terminators here.
                // Let callers decide how to recover so constructs like `switch` can resynchronize
                // without losing `case`/`default` tokens.
                if self.is_at_expression_end()
                    || self.is_token(SyntaxKind::CaseKeyword)
                    || self.is_token(SyntaxKind::DefaultKeyword)
                {
                    return NodeIndex::NONE;
                }

                if self.is_identifier_or_keyword() {
                    self.parse_identifier_name()
                } else {
                    // Unknown primary expression - create an error token
                    let start_pos = self.token_pos();
                    let end_pos = self.token_end();

                    self.error_expression_expected();

                    self.next_token();
                    self.arena
                        .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
                }
            }
        }
    }

    /// Parse identifier
    /// Uses zero-copy accessor and only clones when storing
    fn parse_identifier(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        // Use zero-copy accessor and clone only when storing
        let text = self.scanner.get_token_value_ref().to_string();
        self.parse_expected(SyntaxKind::Identifier);

        self.arena.add_identifier(
            SyntaxKind::Identifier as u16,
            start_pos,
            end_pos,
            IdentifierData {
                escaped_text: text,
                original_text: None,
                type_arguments: None,
            },
        )
    }

    /// Parse identifier name - allows keywords to be used as identifiers
    /// This is used in contexts where keywords are valid identifier names
    /// (e.g., class names, property names, function names)
    fn parse_identifier_name(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        let text = if self.is_identifier_or_keyword() {
            let text = self.scanner.get_token_value_ref().to_string();
            self.next_token();
            text
        } else {
            self.error_identifier_expected();
            String::new()
        };

        self.arena.add_identifier(
            SyntaxKind::Identifier as u16,
            start_pos,
            end_pos,
            IdentifierData {
                escaped_text: text,
                original_text: None,
                type_arguments: None,
            },
        )
    }

    /// Parse private identifier (#name)
    fn parse_private_identifier(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        let text = self.scanner.get_token_value_ref().to_string();
        self.parse_expected(SyntaxKind::PrivateIdentifier);

        self.arena.add_identifier(
            SyntaxKind::PrivateIdentifier as u16,
            start_pos,
            end_pos,
            IdentifierData {
                escaped_text: text,
                original_text: None,
                type_arguments: None,
            },
        )
    }

    /// Parse object binding pattern: { x, y: z, ...rest }
    fn parse_object_binding_pattern(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut elements = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let elem_start = self.token_pos();

            // Handle rest element: ...x
            let dot_dot_dot = self.parse_optional(SyntaxKind::DotDotDotToken);

            if dot_dot_dot {
                // Rest element: just name
                let name = self.parse_binding_element_name();
                if name.is_none() {
                    // Emit TS1109 for missing rest binding element: {...missing}
                    self.error_expression_expected();
                }
                let elem_end = self.token_end();
                elements.push(self.arena.add_binding_element(
                    syntax_kind_ext::BINDING_ELEMENT,
                    elem_start,
                    elem_end,
                    crate::parser::thin_node::BindingElementData {
                        dot_dot_dot_token: true,
                        property_name: NodeIndex::NONE,
                        name,
                        initializer: NodeIndex::NONE,
                    },
                ));
            } else {
                // Regular binding element: name or propertyName: name
                let first_name = self.parse_property_name();

                let (property_name, name) = if self.parse_optional(SyntaxKind::ColonToken) {
                    // propertyName: name
                    let name = self.parse_binding_element_name();
                    if name.is_none() {
                        // Emit TS1109 for missing property binding element: {prop: missing}
                        self.error_expression_expected();
                    }
                    (first_name, name)
                } else {
                    // Just name (shorthand)
                    (NodeIndex::NONE, first_name)
                };

                // Optional initializer: = value
                let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
                    let init = self.parse_assignment_expression();
                    if init.is_none() {
                        // Emit TS1109 for missing object binding initializer: {x = missing}
                        self.error_expression_expected();
                    }
                    init
                } else {
                    NodeIndex::NONE
                };

                let elem_end = self.token_end();
                elements.push(self.arena.add_binding_element(
                    syntax_kind_ext::BINDING_ELEMENT,
                    elem_start,
                    elem_end,
                    crate::parser::thin_node::BindingElementData {
                        dot_dot_dot_token: false,
                        property_name,
                        name,
                        initializer,
                    },
                ));
            }

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        self.arena.add_binding_pattern(
            syntax_kind_ext::OBJECT_BINDING_PATTERN,
            start_pos,
            end_pos,
            crate::parser::thin_node::BindingPatternData {
                elements: self.make_node_list(elements),
            },
        )
    }

    /// Parse array binding pattern: [x, y, ...rest]
    fn parse_array_binding_pattern(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBracketToken);

        let mut elements = Vec::new();

        while !self.is_token(SyntaxKind::CloseBracketToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let elem_start = self.token_pos();

            // Handle omitted element: [, , x]
            if self.is_token(SyntaxKind::CommaToken) {
                // Omitted element - push NONE as placeholder
                elements.push(NodeIndex::NONE);
                self.next_token();
                continue;
            }

            // Handle rest element: ...x
            let dot_dot_dot = self.parse_optional(SyntaxKind::DotDotDotToken);

            // Parse name (can be identifier or nested binding pattern)
            let name = self.parse_binding_element_name();
            if name.is_none() {
                // Emit TS1109 for missing binding element: [...missing] or [missing]
                self.error_expression_expected();
            }

            // Optional initializer: = value
            let initializer = if !dot_dot_dot && self.parse_optional(SyntaxKind::EqualsToken) {
                let init = self.parse_assignment_expression();
                if init.is_none() {
                    // Emit TS1109 for missing binding initializer: [x = missing]
                    self.error_expression_expected();
                }
                init
            } else {
                NodeIndex::NONE
            };

            let elem_end = self.token_end();
            elements.push(self.arena.add_binding_element(
                syntax_kind_ext::BINDING_ELEMENT,
                elem_start,
                elem_end,
                crate::parser::thin_node::BindingElementData {
                    dot_dot_dot_token: dot_dot_dot,
                    property_name: NodeIndex::NONE,
                    name,
                    initializer,
                },
            ));

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseBracketToken);

        self.arena.add_binding_pattern(
            syntax_kind_ext::ARRAY_BINDING_PATTERN,
            start_pos,
            end_pos,
            crate::parser::thin_node::BindingPatternData {
                elements: self.make_node_list(elements),
            },
        )
    }

    /// Parse binding element name (can be identifier or nested binding pattern)
    fn parse_binding_element_name(&mut self) -> NodeIndex {
        // Check for illegal binding identifiers (e.g., 'await' in static blocks)
        self.check_illegal_binding_identifier();

        if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_object_binding_pattern()
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            self.parse_array_binding_pattern()
        } else if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            self.parse_identifier()
        }
    }

    /// Parse numeric literal
    /// Uses zero-copy accessor for parsing, clones only when storing
    fn parse_numeric_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        let text = self.scanner.get_token_value_ref().to_string();
        self.report_invalid_numeric_separator();
        let value = if text.as_bytes().contains(&b'_') {
            let mut sanitized = String::with_capacity(text.len());
            for &byte in text.as_bytes() {
                if byte != b'_' {
                    sanitized.push(byte as char);
                }
            }
            sanitized.parse::<f64>().ok()
        } else {
            text.parse::<f64>().ok()
        };
        self.next_token();

        self.arena.add_literal(
            SyntaxKind::NumericLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value,
            },
        )
    }

    /// Parse bigint literal
    /// Uses zero-copy accessor, stores the raw text (e.g. "123n")
    fn parse_bigint_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let end_pos = self.token_end();
        let text = self.scanner.get_token_value_ref().to_string();
        self.report_invalid_numeric_separator();
        self.next_token();

        self.arena.add_literal(
            SyntaxKind::BigIntLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    fn report_invalid_numeric_separator(&mut self) {
        if (self.scanner.get_token_flags() & TokenFlags::ContainsInvalidSeparator as u32) == 0 {
            return;
        }

        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        let (message, code) = if self.scanner.invalid_separator_is_consecutive() {
            (
                diagnostic_messages::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_NOT_PERMITTED,
                diagnostic_codes::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_NOT_PERMITTED,
            )
        } else {
            (
                diagnostic_messages::NUMERIC_SEPARATORS_NOT_ALLOWED_HERE,
                diagnostic_codes::NUMERIC_SEPARATORS_NOT_ALLOWED_HERE,
            )
        };

        if let Some(pos) = self.scanner.get_invalid_separator_pos() {
            self.parse_error_at(pos as u32, 1, message, code);
        } else {
            self.parse_error_at_current_token(message, code);
        }
    }

    /// Parse boolean literal
    fn parse_boolean_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        let kind = self.token();
        self.next_token();

        self.arena.add_token(kind as u16, start_pos, end_pos)
    }

    /// Parse null literal
    fn parse_null_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        self.next_token();

        self.arena
            .add_token(SyntaxKind::NullKeyword as u16, start_pos, end_pos)
    }

    /// Parse this expression
    fn parse_this_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        self.next_token();

        self.arena
            .add_token(SyntaxKind::ThisKeyword as u16, start_pos, end_pos)
    }

    /// Parse super expression
    fn parse_super_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        self.next_token();

        self.arena
            .add_token(SyntaxKind::SuperKeyword as u16, start_pos, end_pos)
    }

    /// Parse regex literal: /pattern/flags
    fn parse_regex_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Rescan the / or /= as a regex literal
        self.scanner.re_scan_slash_token();
        self.current_token = self.scanner.get_token();

        // Get the regex text (including slashes and flags)
        let text = self.scanner.get_token_value_ref().to_string();
        self.parse_expected(SyntaxKind::RegularExpressionLiteral);
        let end_pos = self.token_end();

        self.arena.add_literal(
            SyntaxKind::RegularExpressionLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    /// Parse import expression: import(...) or import.meta
    fn parse_import_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ImportKeyword);

        // Check for import.meta
        if self.is_token(SyntaxKind::DotToken) {
            self.next_token(); // consume '.'
            // Create import keyword node first (before borrowing arena again)
            let import_node =
                self.arena
                    .add_token(SyntaxKind::ImportKeyword as u16, start_pos, start_pos + 6);
            // Parse 'meta'
            let name = self.parse_identifier_name();
            let end_pos = self.token_end();

            return self.arena.add_access_expr(
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                start_pos,
                end_pos,
                crate::parser::thin_node::AccessExprData {
                    expression: import_node,
                    question_dot_token: false,
                    name_or_argument: name,
                },
            );
        }

        // Dynamic import: import(...)
        self.parse_expected(SyntaxKind::OpenParenToken);
        let argument = self.parse_assignment_expression();

        // Optional second argument (import attributes in some proposals)
        let options = if self.parse_optional(SyntaxKind::CommaToken) {
            if !self.is_token(SyntaxKind::CloseParenToken) {
                Some(self.parse_assignment_expression())
            } else {
                None // Trailing comma
            }
        } else {
            None
        };

        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Create a call expression with import as the callee
        let import_keyword =
            self.arena
                .add_token(SyntaxKind::ImportKeyword as u16, start_pos, start_pos + 6);
        let mut args = vec![argument];
        if let Some(opt) = options {
            args.push(opt);
        }
        let arguments = self.make_node_list(args);

        self.arena.add_call_expr(
            syntax_kind_ext::CALL_EXPRESSION,
            start_pos,
            end_pos,
            crate::parser::thin_node::CallExprData {
                expression: import_keyword,
                type_arguments: None,
                arguments: Some(arguments),
            },
        )
    }

    /// Parse no-substitution template literal: `hello`
    fn parse_no_substitution_template_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let is_unterminated = self.scanner.is_unterminated();
        let text = self.scanner.get_token_value_ref().to_string();
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::NoSubstitutionTemplateLiteral);
        if is_unterminated {
            self.error_unterminated_template_literal_at(start_pos, end_pos);
        }

        self.arena.add_literal(
            SyntaxKind::NoSubstitutionTemplateLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    /// Parse template expression: `hello ${name}!`
    fn parse_template_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse template head: `hello ${
        let head_text = self.scanner.get_token_value_ref().to_string();
        let head_start = self.token_pos();
        let head_end = self.token_end();
        self.parse_expected(SyntaxKind::TemplateHead);

        let head = self.arena.add_literal(
            SyntaxKind::TemplateHead as u16,
            head_start,
            head_end,
            LiteralData {
                text: head_text,
                raw_text: None,
                value: None,
            },
        );

        // Parse template spans
        let mut spans = Vec::new();
        let end_pos = loop {
            // Parse expression in ${ }
            let expression = self.parse_expression();

            // Check for missing expression in template literal: `prefix${}tail`
            if expression.is_none() {
                self.error_expression_expected();
                // Continue parsing for error recovery
            }

            if !self.is_token(SyntaxKind::CloseBraceToken) {
                // Unterminated template expression - report and synthesize tail to avoid looping.
                self.error_token_expected("}");
                let literal_start = self.token_pos();
                let literal_end = self.token_end();
                let literal = self.arena.add_literal(
                    SyntaxKind::TemplateTail as u16,
                    literal_start,
                    literal_end,
                    LiteralData {
                        text: String::new(),
                        raw_text: None,
                        value: None,
                    },
                );
                let span_start = self
                    .arena
                    .get(expression)
                    .map(|node| node.pos)
                    .unwrap_or(literal_start);
                let span = self.arena.add_template_span(
                    syntax_kind_ext::TEMPLATE_SPAN,
                    span_start,
                    literal_end,
                    TemplateSpanData {
                        expression,
                        literal,
                    },
                );
                spans.push(span);
                break literal_end;
            }

            // Now we need to rescan the } as a template continuation
            // The scanner needs to be told to rescan as template
            self.scanner.re_scan_template_token(false);
            self.current_token = self.scanner.get_token();

            // Parse template middle or tail
            let literal_start = self.token_pos();
            let is_tail = self.is_token(SyntaxKind::TemplateTail);
            let is_middle = self.is_token(SyntaxKind::TemplateMiddle);
            if !is_tail && !is_middle {
                // Unexpected token after template span - report and finish.
                self.error_token_expected("`");
                let literal_end = self.token_end();
                let literal = self.arena.add_literal(
                    SyntaxKind::TemplateTail as u16,
                    literal_start,
                    literal_end,
                    LiteralData {
                        text: String::new(),
                        raw_text: None,
                        value: None,
                    },
                );
                let span_start = self
                    .arena
                    .get(expression)
                    .map(|node| node.pos)
                    .unwrap_or(literal_start);
                let span = self.arena.add_template_span(
                    syntax_kind_ext::TEMPLATE_SPAN,
                    span_start,
                    literal_end,
                    TemplateSpanData {
                        expression,
                        literal,
                    },
                );
                spans.push(span);
                break literal_end;
            }

            let is_unterminated = self.scanner.is_unterminated();
            let literal_text = self.scanner.get_token_value_ref().to_string();
            let literal_kind = if is_tail {
                SyntaxKind::TemplateTail
            } else {
                SyntaxKind::TemplateMiddle
            };

            let literal_end = self.token_end();
            self.next_token();

            let literal = self.arena.add_literal(
                literal_kind as u16,
                literal_start,
                literal_end,
                LiteralData {
                    text: literal_text,
                    raw_text: None,
                    value: None,
                },
            );
            if is_unterminated {
                self.error_unterminated_template_literal_at(literal_start, literal_end);
            }

            let span_start = if let Some(node) = self.arena.get(expression) {
                node.pos
            } else {
                literal_start
            };
            let span = self.arena.add_template_span(
                syntax_kind_ext::TEMPLATE_SPAN,
                span_start,
                literal_end,
                TemplateSpanData {
                    expression,
                    literal,
                },
            );
            spans.push(span);

            if is_tail {
                break literal_end;
            }
        };

        self.arena.add_template_expr(
            syntax_kind_ext::TEMPLATE_EXPRESSION,
            start_pos,
            end_pos,
            TemplateExprData {
                head,
                template_spans: self.make_node_list(spans),
            },
        )
    }

    /// Parse template literal (either no-substitution or full template expression)
    /// Used for both standalone template literals and as the template part of tagged templates
    fn parse_template_literal(&mut self) -> NodeIndex {
        if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral) {
            self.parse_no_substitution_template_literal()
        } else {
            self.parse_template_expression()
        }
    }

    /// Parse parenthesized expression
    fn parse_parenthesized_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression();
        if expression.is_none() {
            // Emit TS1109 for empty parentheses or invalid expression: ([missing])
            self.error_expression_expected();
        }
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseParenToken);

        self.arena.add_parenthesized(
            syntax_kind_ext::PARENTHESIZED_EXPRESSION,
            start_pos,
            end_pos,
            ParenthesizedData { expression },
        )
    }

    /// Parse array literal
    fn parse_array_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBracketToken);

        let mut elements = Vec::new();
        while !self.is_token(SyntaxKind::CloseBracketToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            if self.is_token(SyntaxKind::CommaToken) {
                // Elided element
                elements.push(NodeIndex::NONE);
            } else if self.is_token(SyntaxKind::DotDotDotToken) {
                // Spread element: ...expr
                let spread_start = self.token_pos();
                self.next_token();
                let expression = self.parse_assignment_expression();
                if expression.is_none() {
                    // Emit TS1109 for incomplete spread element: [...missing]
                    self.error_expression_expected();
                }
                let spread_end = self.token_end();
                let spread = self.arena.add_spread(
                    syntax_kind_ext::SPREAD_ELEMENT,
                    spread_start,
                    spread_end,
                    crate::parser::thin_node::SpreadData { expression },
                );
                elements.push(spread);
            } else {
                let elem = self.parse_assignment_expression();
                if elem.is_none() {
                    // Emit TS1109 for missing array element: [a, , ] vs [a, b]
                    self.error_expression_expected();
                    // Continue parsing with empty element for error recovery
                }
                elements.push(elem);
            }

            if !self.parse_optional(SyntaxKind::CommaToken) {
                // Missing comma - check if next token looks like another array element
                // If so, suppress the error and continue parsing (better recovery)
                if self.is_expression_start()
                    && !self.is_token(SyntaxKind::CloseBracketToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    // We have an element-like token but no comma - likely missing comma
                    // Suppress the comma error and continue parsing for better recovery
                    // This handles cases like: [1 2 3] instead of [1, 2, 3]
                } else {
                    // Not followed by an element, so we're really done
                    break;
                }
            }
        }

        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseBracketToken);

        self.arena.add_literal_expr(
            syntax_kind_ext::ARRAY_LITERAL_EXPRESSION,
            start_pos,
            end_pos,
            LiteralExprData {
                elements: self.make_node_list(elements),
                multi_line: false,
            },
        )
    }

    /// Check if current token can start an object property
    /// Used for error recovery in object literals when commas are missing
    fn is_property_start(&self) -> bool {
        match self.token() {
            // Spread operator
            SyntaxKind::DotDotDotToken => true,
            // Get/Set accessors
            SyntaxKind::GetKeyword | SyntaxKind::SetKeyword => true,
            // Async keyword (for async methods)
            SyntaxKind::AsyncKeyword => true,
            // Asterisk (for generator methods)
            SyntaxKind::AsteriskToken => true,
            // String/number literals (computed properties or shorthand)
            SyntaxKind::StringLiteral | SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral => {
                true
            }
            // Identifier or keyword (property names)
            SyntaxKind::Identifier => true,
            // Bracket (computed property)
            SyntaxKind::OpenBracketToken => true,
            _ => self.is_identifier_or_keyword(),
        }
    }

    /// Parse object literal
    fn parse_object_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut properties = Vec::new();
        while !self.is_token(SyntaxKind::CloseBraceToken) {
            let prop = self.parse_property_assignment();
            if !prop.is_none() {
                properties.push(prop);
            }

            // Try to parse comma separator
            if !self.parse_optional(SyntaxKind::CommaToken) {
                // Missing comma - check if next token looks like another property
                // If so, suppress the error and continue parsing (better recovery)
                if self.is_property_start() && !self.is_token(SyntaxKind::CloseBraceToken) {
                    // We have a property-like token but no comma - likely missing comma
                    // Suppress the comma error and continue parsing for better recovery
                    // This handles cases like: {a: 1 b: 2} instead of {a: 1, b: 2}
                } else {
                    // Not followed by a property, so we're really done
                    break;
                }
            }
        }

        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        self.arena.add_literal_expr(
            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION,
            start_pos,
            end_pos,
            LiteralExprData {
                elements: self.make_node_list(properties),
                multi_line: false,
            },
        )
    }

    /// Parse property assignment, method, getter, setter, or spread element
    fn parse_property_assignment(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Handle spread element: ...expr
        if self.is_token(SyntaxKind::DotDotDotToken) {
            self.next_token();
            let expression = self.parse_assignment_expression();
            if expression.is_none() {
                // Emit TS1109 for incomplete spread element: {...missing}
                self.error_expression_expected();
            }
            let end_pos = self.token_end();
            return self.arena.add_spread(
                syntax_kind_ext::SPREAD_ASSIGNMENT,
                start_pos,
                end_pos,
                crate::parser::thin_node::SpreadData { expression },
            );
        }

        // Handle invalid modifiers before index signatures in object literals.
        if matches!(
            self.token(),
            SyntaxKind::PrivateKeyword | SyntaxKind::ProtectedKeyword | SyntaxKind::PublicKeyword
        ) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Modifiers cannot appear here.",
                diagnostic_codes::MODIFIERS_NOT_ALLOWED_HERE,
            );
            self.next_token();
            if self.is_token(SyntaxKind::OpenBracketToken) && self.look_ahead_is_index_signature() {
                let _ = self.parse_index_signature_with_modifiers(None, start_pos);
                return NodeIndex::NONE;
            }
        }

        // Handle get accessor: get foo() { }
        if self.is_token(SyntaxKind::GetKeyword) && self.look_ahead_is_object_method() {
            return self.parse_object_get_accessor(start_pos);
        }

        // Handle set accessor: set foo(v) { }
        if self.is_token(SyntaxKind::SetKeyword) && self.look_ahead_is_object_method() {
            return self.parse_object_set_accessor(start_pos);
        }

        // Handle async method: async foo() { }
        if self.is_token(SyntaxKind::AsyncKeyword) && self.look_ahead_is_object_method() {
            return self.parse_object_method(start_pos, true, false);
        }

        // Handle generator method: *foo() { }
        if self.is_token(SyntaxKind::AsteriskToken) {
            self.next_token(); // consume '*'
            return self.parse_object_method(start_pos, false, true);
        }

        if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
            || self.is_token(SyntaxKind::TemplateHead)
        {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Property assignment expected.",
                diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED,
            );
            let name = self.parse_template_literal();
            let initializer = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_assignment_expression()
            } else {
                name
            };
            let end_pos = self.token_end();
            return self.arena.add_property_assignment(
                syntax_kind_ext::PROPERTY_ASSIGNMENT,
                start_pos,
                end_pos,
                crate::parser::thin_node::PropertyAssignmentData {
                    modifiers: None,
                    name,
                    initializer,
                },
            );
        }

        let name = self.parse_property_name();

        // Handle method: foo() { } or foo<T>() { }
        if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::LessThanToken) {
            return self.parse_object_method_after_name(start_pos, name, false, false);
        }

        let initializer = if self.parse_optional(SyntaxKind::ColonToken) {
            let expr = self.parse_assignment_expression();
            if expr.is_none() {
                // Emit TS1109 for missing property value: { prop: }
                self.error_expression_expected();
                name // Use property name as fallback for error recovery
            } else {
                expr
            }
        } else {
            // Shorthand property
            name
        };

        let end_pos = self.token_end();
        self.arena.add_property_assignment(
            syntax_kind_ext::PROPERTY_ASSIGNMENT,
            start_pos,
            end_pos,
            crate::parser::thin_node::PropertyAssignmentData {
                modifiers: None,
                name,
                initializer,
            },
        )
    }

    /// Look ahead to check if get/set/async is a method vs property name
    fn look_ahead_is_object_method(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip get/set/async

        // Check if followed by property name (identifier, keyword, string, number, [)
        // Keywords like 'return', 'throw', 'delete' can be method names
        let is_method = self.is_token(SyntaxKind::Identifier)
            || self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_token(SyntaxKind::OpenBracketToken)
            || self.is_token(SyntaxKind::AsteriskToken) // async *foo()
            || self.is_identifier_or_keyword(); // keywords as method names

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_method
    }

    /// Parse get accessor in object literal: get foo() { }
    fn parse_object_get_accessor(&mut self, start_pos: u32) -> NodeIndex {
        self.next_token(); // consume 'get'
        let name = self.parse_property_name();

        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "An accessor cannot have type parameters.",
                diagnostic_codes::ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS,
            );
            Some(self.parse_type_parameters())
        } else {
            None
        };

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if self.is_token(SyntaxKind::CloseParenToken) {
            self.make_node_list(vec![])
        } else {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "A 'get' accessor cannot have parameters.",
                diagnostic_codes::GETTER_MUST_NOT_HAVE_PARAMETERS,
            );
            self.parse_parameter_list()
        };
        // Save end of ) for error reporting - get it BEFORE consuming the token
        let close_paren_end = self.token_end();
        self.parse_expected(SyntaxKind::CloseParenToken);

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };
        // If there's a type annotation, use its end; otherwise use close paren end
        let signature_end = if !type_annotation.is_none() {
            self.token_pos()
        } else {
            close_paren_end
        };

        // Parse body if present. Missing body is reported in grammar check, not here.
        // This matches TypeScript's behavior of allowing ASI and checking later.
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        // End position: use token_end for normal case, signature_end for missing body
        let end_pos = if body.is_none() {
            signature_end
        } else {
            self.token_end()
        };
        self.arena.add_accessor(
            syntax_kind_ext::GET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::thin_node::AccessorData {
                modifiers: None,
                name,
                type_parameters,
                parameters,
                type_annotation,
                body,
            },
        )
    }

    /// Parse set accessor in object literal: set foo(v) { }
    fn parse_object_set_accessor(&mut self, start_pos: u32) -> NodeIndex {
        self.next_token(); // consume 'set'
        let name = self.parse_property_name();

        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "An accessor cannot have type parameters.",
                diagnostic_codes::ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS,
            );
            Some(self.parse_type_parameters())
        } else {
            None
        };

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if self.is_token(SyntaxKind::CloseParenToken) {
            self.make_node_list(vec![])
        } else {
            self.parse_parameter_list()
        };
        // Save end of ) for error reporting - get it BEFORE consuming the token
        let close_paren_end = self.token_end();
        self.parse_expected(SyntaxKind::CloseParenToken);

        if parameters.len() != 1 {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "A 'set' accessor must have exactly one parameter.",
                diagnostic_codes::SETTER_MUST_HAVE_EXACTLY_ONE_PARAMETER,
            );
        }

        if self.parse_optional(SyntaxKind::ColonToken) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "A 'set' accessor cannot have a return type annotation.",
                diagnostic_codes::SETTER_CANNOT_HAVE_RETURN_TYPE,
            );
            let _ = self.parse_type();
        }

        // Parse body if present. Missing body is reported in grammar check, not here.
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        // End position: use token_end for normal case, close_paren_end for missing body
        let end_pos = if body.is_none() {
            close_paren_end
        } else {
            self.token_end()
        };
        self.arena.add_accessor(
            syntax_kind_ext::SET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::thin_node::AccessorData {
                modifiers: None,
                name,
                type_parameters,
                parameters,
                type_annotation: NodeIndex::NONE,
                body,
            },
        )
    }

    /// Parse method in object literal: foo() { } or async foo() { } or *foo() { }
    fn parse_object_method(
        &mut self,
        start_pos: u32,
        is_async: bool,
        is_generator: bool,
    ) -> NodeIndex {
        // Build modifiers if async
        let modifiers = if is_async {
            self.next_token(); // consume 'async'
            let mod_idx = self
                .arena
                .create_modifier(SyntaxKind::AsyncKeyword, start_pos);
            Some(self.make_node_list(vec![mod_idx]))
        } else {
            None
        };

        // Check for generator after async: async *foo()
        // or standalone generator: *foo()
        let asterisk = if is_generator {
            // Asterisk already consumed by caller for standalone generator
            true
        } else if self.parse_optional(SyntaxKind::AsteriskToken) {
            // async *foo() - consume asterisk here
            true
        } else {
            false
        };

        let name = self.parse_property_name();
        self.parse_object_method_after_name(start_pos, name, asterisk, modifiers.is_some())
    }

    /// Parse method after name has been parsed
    fn parse_object_method_after_name(
        &mut self,
        start_pos: u32,
        name: NodeIndex,
        asterisk: bool,
        is_async: bool,
    ) -> NodeIndex {
        // Optional type parameters
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        // Set context flags for async/generator to properly parse await/yield in method bodies.
        let saved_flags = self.context_flags;
        if is_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }
        if asterisk {
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
        }

        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        // Restore context flags after parsing body.
        self.context_flags = saved_flags;

        let modifiers = if is_async {
            let mod_idx = self
                .arena
                .create_modifier(SyntaxKind::AsyncKeyword, start_pos);
            Some(self.make_node_list(vec![mod_idx]))
        } else {
            None
        };

        let end_pos = self.token_end();
        self.arena.add_method_decl(
            syntax_kind_ext::METHOD_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::thin_node::MethodDeclData {
                modifiers,
                asterisk_token: asterisk,
                name,
                question_token: false,
                type_parameters,
                parameters,
                type_annotation,
                body,
            },
        )
    }

    /// Parse property name (identifier, string literal, numeric literal, computed)
    fn parse_property_name(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::StringLiteral => {
                // String literal can be property name: { "key": value }
                self.parse_string_literal()
            }
            SyntaxKind::NumericLiteral => {
                // Numeric literal can be property name: { 0: value }
                self.parse_numeric_literal()
            }
            SyntaxKind::OpenBracketToken => {
                // Computed property name: { [expr]: value }
                let start_pos = self.token_pos();
                self.next_token();

                // Note: await in computed property name is NOT a parser error
                // The type checker will emit TS2304 if 'await' is not in scope
                // Example: { [await]: foo } should only emit TS2304, not TS1109

                let expression = self.parse_expression();
                if expression.is_none() {
                    // Emit TS1109 for empty computed property: { [[missing]]: value }
                    self.error_expression_expected();
                }
                self.parse_expected(SyntaxKind::CloseBracketToken);
                let end_pos = self.token_end();

                self.arena.add_computed_property(
                    syntax_kind_ext::COMPUTED_PROPERTY_NAME,
                    start_pos,
                    end_pos,
                    crate::parser::thin_node::ComputedPropertyData { expression },
                )
            }
            SyntaxKind::PrivateIdentifier => {
                // Private identifier: #name
                self.parse_private_identifier()
            }
            _ => {
                // Identifier or keyword used as property name
                let start_pos = self.token_pos();
                // Use zero-copy accessor
                let text = self.scanner.get_token_value_ref().to_string();
                self.next_token(); // Accept any token as property name
                let end_pos = self.token_end();

                self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    start_pos,
                    end_pos,
                    IdentifierData {
                        escaped_text: text,
                        original_text: None,
                        type_arguments: None,
                    },
                )
            }
        }
    }

    /// Parse new expression
    fn parse_new_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::NewKeyword);

        // Type assertion syntax (<T>expr) is not valid in new expressions
        // Check if the next token is '<' and report TS1109 if so
        if self.is_token(SyntaxKind::LessThanToken) {
            self.error_expression_expected();
        }

        // Parse the callee expression - member access without call (we handle call ourselves)
        let expression = self.parse_member_expression_base();
        let mut end_pos = self
            .arena
            .get(expression)
            .map(|node| node.end)
            .unwrap_or(self.token_end());

        // Parse type arguments: new Array<string>()
        let type_arguments = if self.is_token(SyntaxKind::LessThanToken) {
            // Try to parse as type arguments
            Some(self.parse_type_arguments())
        } else {
            None
        };
        if let Some(type_args) = type_arguments.as_ref() {
            if let Some(last) = type_args.nodes.last() {
                if let Some(node) = self.arena.get(*last) {
                    end_pos = end_pos.max(node.end);
                }
            }
        }

        let arguments = if self.is_token(SyntaxKind::OpenParenToken) {
            self.next_token();
            let args = self.parse_argument_list();
            let call_end = self.token_end();
            self.parse_expected(SyntaxKind::CloseParenToken);
            end_pos = call_end;
            Some(args)
        } else {
            None
        };

        self.arena.add_call_expr(
            syntax_kind_ext::NEW_EXPRESSION,
            start_pos,
            end_pos,
            CallExprData {
                expression,
                type_arguments,
                arguments,
            },
        )
    }

    /// Parse member expression base (identifier with property/element access, but no calls)
    fn parse_member_expression_base(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_primary_expression();

        loop {
            match self.token() {
                SyntaxKind::DotToken => {
                    self.next_token();
                    let diag_count_before = self.parse_diagnostics.len();
                    let name = if self.is_token(SyntaxKind::PrivateIdentifier) {
                        self.parse_private_identifier()
                    } else if self.is_identifier_or_keyword() {
                        self.parse_identifier_name()
                    } else {
                        self.error_identifier_expected();
                        NodeIndex::NONE
                    };

                    // If parsing the name produced an error, don't create a property access
                    // expression to avoid spurious semantic errors (e.g., TS2339 for incomplete `this.`)
                    if self.parse_diagnostics.len() > diag_count_before {
                        break;
                    }

                    let end_pos = self.token_end();

                    expr = self.arena.add_access_expr(
                        syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                        start_pos,
                        end_pos,
                        AccessExprData {
                            expression: expr,
                            name_or_argument: name,
                            question_dot_token: false,
                        },
                    );
                }
                SyntaxKind::OpenBracketToken => {
                    self.next_token();
                    let argument = self.parse_expression();
                    if argument.is_none() {
                        // Emit TS1109 for empty brackets or invalid expression: obj[[missing]]
                        self.error_expression_expected();
                    }
                    let end_pos = self.token_end();
                    self.parse_expected(SyntaxKind::CloseBracketToken);

                    expr = self.arena.add_access_expr(
                        syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION,
                        start_pos,
                        end_pos,
                        AccessExprData {
                            expression: expr,
                            name_or_argument: argument,
                            question_dot_token: false,
                        },
                    );
                }
                _ => break,
            }
        }

        expr
    }

    // =========================================================================
    // Parse Methods - Types (minimal implementation)
    // =========================================================================

    fn is_asserts_keyword(&self) -> bool {
        self.is_token(SyntaxKind::AssertsKeyword)
            || (self.is_token(SyntaxKind::Identifier)
                && self.scanner.get_token_value_ref() == "asserts")
    }

    fn is_asserts_type_predicate_start(&mut self) -> bool {
        if !self.is_asserts_keyword() {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let is_param = self.is_identifier_or_keyword() || self.is_token(SyntaxKind::ThisKeyword);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_param
    }

    fn consume_asserts_keyword(&mut self) {
        if self.is_asserts_keyword() {
            self.next_token();
        } else {
            self.parse_expected(SyntaxKind::AssertsKeyword);
        }
    }

    /// Parse a type (handles keywords, type references, unions, intersections, conditionals)
    fn parse_type(&mut self) -> NodeIndex {
        if self.is_asserts_type_predicate_start() {
            return self.parse_asserts_type_predicate();
        }

        // Allow type predicate parsing in type positions to avoid cascading errors.
        if self.is_identifier_or_keyword() || self.is_token(SyntaxKind::ThisKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;

            self.next_token();
            let is_predicate = self.is_token(SyntaxKind::IsKeyword);
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if is_predicate {
                let name = self.parse_type_predicate_parameter_name();
                let start_pos = if let Some(node) = self.arena.get(name) {
                    node.pos
                } else {
                    self.token_pos()
                };

                self.next_token(); // consume 'is'
                let type_node = self.parse_type();
                let end_pos = self.token_end();

                return self.arena.add_type_predicate(
                    syntax_kind_ext::TYPE_PREDICATE,
                    start_pos,
                    end_pos,
                    crate::parser::thin_node::TypePredicateData {
                        asserts_modifier: false,
                        parameter_name: name,
                        type_node,
                    },
                );
            }
        }

        self.parse_conditional_type()
    }

    /// Parse return type, which may be a type predicate (x is T) or a regular type
    fn parse_return_type(&mut self) -> NodeIndex {
        if self.is_asserts_type_predicate_start() {
            return self.parse_asserts_type_predicate();
        }

        // Check if this is a type predicate: identifier 'is' Type
        // We need to look ahead to see if there's an identifier followed by 'is'
        if self.is_identifier_or_keyword() || self.is_token(SyntaxKind::ThisKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;

            self.next_token();
            let is_predicate = self.is_token(SyntaxKind::IsKeyword);
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if is_predicate {
                let name = self.parse_type_predicate_parameter_name();
                // This is a type predicate: x is T
                let start_pos = if let Some(node) = self.arena.get(name) {
                    node.pos
                } else {
                    self.token_pos()
                };

                self.next_token(); // consume 'is'
                let type_node = self.parse_type();
                let end_pos = self.token_end();

                return self.arena.add_type_predicate(
                    syntax_kind_ext::TYPE_PREDICATE,
                    start_pos,
                    end_pos,
                    crate::parser::thin_node::TypePredicateData {
                        asserts_modifier: false,
                        parameter_name: name,
                        type_node,
                    },
                );
            }
        }

        self.parse_type()
    }

    fn parse_type_predicate_parameter_name(&mut self) -> NodeIndex {
        if self.is_token(SyntaxKind::ThisKeyword) {
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            self.next_token();
            return self
                .arena
                .add_token(SyntaxKind::ThisKeyword as u16, start_pos, end_pos);
        }

        self.parse_identifier_name()
    }

    /// Parse 'asserts' type predicate: asserts x or asserts x is T
    fn parse_asserts_type_predicate(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.consume_asserts_keyword();

        let parameter_name = self.parse_type_predicate_parameter_name();

        let type_node = if self.is_token(SyntaxKind::IsKeyword) {
            self.next_token();
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();

        self.arena.add_type_predicate(
            syntax_kind_ext::TYPE_PREDICATE,
            start_pos,
            end_pos,
            crate::parser::thin_node::TypePredicateData {
                asserts_modifier: true,
                parameter_name,
                type_node,
            },
        )
    }

    /// Parse conditional type: T extends U ? X : Y
    fn parse_conditional_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the check type (left side of extends)
        let check_type = self.parse_union_type();

        // Check for extends keyword to form conditional
        if !self.is_token(SyntaxKind::ExtendsKeyword) {
            return check_type;
        }

        self.next_token(); // consume extends

        // Parse the extends type (right side of extends)
        let extends_type = self.parse_union_type();

        // Expect ?
        self.parse_expected(SyntaxKind::QuestionToken);

        // Parse true type
        let true_type = self.parse_type();

        // Expect :
        self.parse_expected(SyntaxKind::ColonToken);

        // Parse false type
        let false_type = self.parse_type();

        let end_pos = self.token_end();

        self.arena.add_conditional_type(
            syntax_kind_ext::CONDITIONAL_TYPE,
            start_pos,
            end_pos,
            crate::parser::thin_node::ConditionalTypeData {
                check_type,
                extends_type,
                true_type,
                false_type,
            },
        )
    }

    /// Parse union type: A | B | C
    fn parse_union_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Handle optional leading | (e.g., type T = | A | B)
        let has_leading_bar = self.parse_optional(SyntaxKind::BarToken);

        // Parse first constituent
        let first = self.parse_intersection_type();

        // Check for | to form union
        if !has_leading_bar && !self.is_token(SyntaxKind::BarToken) {
            return first;
        }

        let mut types = vec![first];

        while self.parse_optional(SyntaxKind::BarToken) {
            types.push(self.parse_intersection_type());
        }

        let end_pos = self.token_end();
        self.arena.add_composite_type(
            syntax_kind_ext::UNION_TYPE,
            start_pos,
            end_pos,
            crate::parser::thin_node::CompositeTypeData {
                types: self.make_node_list(types),
            },
        )
    }

    /// Parse intersection type: A & B & C
    fn parse_intersection_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Handle optional leading & (e.g., type T = & A & B)
        let has_leading_amp = self.parse_optional(SyntaxKind::AmpersandToken);

        // Parse first constituent
        let first = self.parse_primary_type();

        // Check for & to form intersection
        if !has_leading_amp && !self.is_token(SyntaxKind::AmpersandToken) {
            return first;
        }

        let mut types = vec![first];

        while self.parse_optional(SyntaxKind::AmpersandToken) {
            types.push(self.parse_primary_type());
        }

        let end_pos = self.token_end();
        self.arena.add_composite_type(
            syntax_kind_ext::INTERSECTION_TYPE,
            start_pos,
            end_pos,
            crate::parser::thin_node::CompositeTypeData {
                types: self.make_node_list(types),
            },
        )
    }

    /// Parse primary type (keywords, references, parenthesized, tuples, arrays, function types)
    fn parse_primary_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // If we encounter a token that can't start a type, emit TS1110 instead of TS1005
        if !self.can_token_start_type() {
            self.error_type_expected();
            // Return a synthetic identifier node to allow parsing to continue
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                start_pos,
                self.token_pos(),
                crate::parser::thin_node::IdentifierData {
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            );
        }

        // Handle abstract constructor types: abstract new () => T
        if self.is_token(SyntaxKind::AbstractKeyword) {
            // Look ahead to see if this is "abstract new"
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            let is_abstract_new = self.is_token(SyntaxKind::NewKeyword);
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if is_abstract_new {
                // Consume 'abstract' and parse the constructor type
                self.next_token();
                return self.parse_constructor_type(true);
            }
        }

        // Handle constructor types: new () => T or new <T>() => T
        if self.is_token(SyntaxKind::NewKeyword) {
            return self.parse_constructor_type(false);
        }

        // Handle generic function types: <T>() => T or <T, U>(x: T) => U
        if self.is_token(SyntaxKind::LessThanToken) {
            return self.parse_generic_function_type();
        }

        // Handle parenthesized types or function types
        if self.is_token(SyntaxKind::OpenParenToken) {
            // Check if this is a function type: () => T or (x: T) => U
            if self.look_ahead_is_function_type() {
                return self.parse_function_type();
            }

            // Otherwise it's a parenthesized type
            self.next_token();
            let inner = self.parse_type();
            self.parse_expected(SyntaxKind::CloseParenToken);

            // Handle array types on parenthesized: (A | B)[]
            if self.is_token(SyntaxKind::OpenBracketToken) {
                return self.parse_array_type(start_pos, inner);
            }
            return inner;
        }

        // Handle tuple types: [T, U, V]
        if self.is_token(SyntaxKind::OpenBracketToken) {
            return self.parse_tuple_type();
        }

        // Handle object type literal or mapped type: { ... } or { [K in T]: U }
        if self.is_token(SyntaxKind::OpenBraceToken) {
            let obj_type = self.parse_object_or_mapped_type();
            // Handle array/indexed access on object literal: {...}[] or {...}["key"]
            if self.is_token(SyntaxKind::OpenBracketToken) {
                return self.parse_array_type(start_pos, obj_type);
            }
            return obj_type;
        }

        // Handle typeof type: typeof x, typeof x[]
        if self.is_token(SyntaxKind::TypeOfKeyword) {
            let typeof_type = self.parse_typeof_type();
            // Handle array type on typeof: typeof x[]
            if self.is_token(SyntaxKind::OpenBracketToken) {
                return self.parse_array_type(start_pos, typeof_type);
            }
            return typeof_type;
        }

        // Handle keyof type: keyof T, keyof T[]
        if self.is_token(SyntaxKind::KeyOfKeyword) {
            let keyof_type = self.parse_keyof_type();
            // Handle array type on keyof: keyof T[]
            if self.is_token(SyntaxKind::OpenBracketToken) {
                return self.parse_array_type(start_pos, keyof_type);
            }
            return keyof_type;
        }

        // Handle unique type: unique symbol
        if self.is_token(SyntaxKind::UniqueKeyword) {
            let unique_type = self.parse_unique_type();
            // Handle array type on unique: unique symbol[]
            if self.is_token(SyntaxKind::OpenBracketToken) {
                return self.parse_array_type(start_pos, unique_type);
            }
            return unique_type;
        }

        // Handle readonly type: readonly T[]
        if self.is_token(SyntaxKind::ReadonlyKeyword) {
            return self.parse_readonly_type();
        }

        // Handle infer type: infer T (used in conditional types)
        if self.is_token(SyntaxKind::InferKeyword) {
            return self.parse_infer_type();
        }

        // Handle 'this' type (polymorphic this)
        if self.is_token(SyntaxKind::ThisKeyword) {
            let this_start = self.token_pos();
            let this_end = self.token_end();
            self.next_token();
            return self
                .arena
                .add_token(syntax_kind_ext::THIS_TYPE, this_start, this_end);
        }

        // Handle literal types: "foo", 42, true, false
        if self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_token(SyntaxKind::BigIntLiteral)
            || self.is_token(SyntaxKind::TrueKeyword)
            || self.is_token(SyntaxKind::FalseKeyword)
        {
            return self.parse_literal_type();
        }

        // Handle negative numeric literal types: -1, -42
        if self.is_token(SyntaxKind::MinusToken) {
            return self.parse_prefix_unary_literal_type();
        }

        // Handle template literal types: `hello` or `prefix${T}suffix`
        if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
            || self.is_token(SyntaxKind::TemplateHead)
        {
            return self.parse_template_literal_type();
        }

        // Check for type keywords (string, number, boolean, etc.)
        // Also handle contextual keywords (await, yield) which are valid as type names
        let first_name = match self.token() {
            SyntaxKind::StringKeyword
            | SyntaxKind::NumberKeyword
            | SyntaxKind::BooleanKeyword
            | SyntaxKind::SymbolKeyword
            | SyntaxKind::BigIntKeyword
            | SyntaxKind::VoidKeyword
            | SyntaxKind::NullKeyword
            | SyntaxKind::UndefinedKeyword
            | SyntaxKind::NeverKeyword
            | SyntaxKind::AnyKeyword
            | SyntaxKind::UnknownKeyword
            | SyntaxKind::ObjectKeyword
            | SyntaxKind::AwaitKeyword
            | SyntaxKind::YieldKeyword
            | SyntaxKind::AssertsKeyword => {
                // Parse keyword as identifier for type reference
                self.parse_keyword_as_identifier()
            }
            SyntaxKind::PrivateIdentifier => self.parse_private_identifier(),
            _ => {
                // Regular identifier
                self.parse_identifier()
            }
        };

        // Handle qualified names (foo.Bar, A.B.C)
        let type_name = self.parse_qualified_name_rest(first_name);

        // Check for type arguments: Foo<T, U>
        let type_arguments = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_arguments())
        } else {
            None
        };

        let base_type = self.arena.add_type_ref(
            syntax_kind_ext::TYPE_REFERENCE,
            start_pos,
            self.token_end(),
            crate::parser::thin_node::TypeRefData {
                type_name,
                type_arguments,
            },
        );

        // Handle array types (T[])
        if self.is_token(SyntaxKind::OpenBracketToken) {
            return self.parse_array_type(start_pos, base_type);
        }

        base_type
    }

    /// Parse a single element in a tuple type, handling:
    /// - Rest elements: ...T[]
    /// - Optional elements: T?
    /// - Named elements: name: T or name?: T
    fn parse_tuple_element_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Handle rest element: ...T[]
        if self.parse_optional(SyntaxKind::DotDotDotToken) {
            let element_type = self.parse_type();
            let end_pos = self.token_end();
            return self.arena.add_wrapped_type(
                syntax_kind_ext::REST_TYPE,
                start_pos,
                end_pos,
                crate::parser::thin_node::WrappedTypeData {
                    type_node: element_type,
                },
            );
        }

        // Check if this is a named tuple element: name: T or name?: T
        // Need to look ahead to see if there's a colon after the identifier
        if self.is_token(SyntaxKind::Identifier) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;

            let _name = self.scanner.get_token_value_ref().to_string();
            self.next_token();

            // Check for optional marker and colon
            let has_question = self.parse_optional(SyntaxKind::QuestionToken);
            let has_colon = self.is_token(SyntaxKind::ColonToken);

            if has_colon || has_question {
                // This is a named tuple element - parse it
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                return self.parse_named_tuple_member();
            }

            // Not a named element, restore and parse as regular type
            self.scanner.restore_state(snapshot);
            self.current_token = current;
        }

        // Parse the type
        let type_node = self.parse_type();

        // Check for optional marker: T?
        if self.parse_optional(SyntaxKind::QuestionToken) {
            let end_pos = self.token_end();
            return self.arena.add_wrapped_type(
                syntax_kind_ext::OPTIONAL_TYPE,
                start_pos,
                end_pos,
                crate::parser::thin_node::WrappedTypeData { type_node },
            );
        }

        type_node
    }

    /// Parse a named tuple member: name: T or name?: T
    fn parse_named_tuple_member(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Check for ... prefix (rest parameter)
        let dot_dot_dot_token = self.parse_optional(SyntaxKind::DotDotDotToken);

        // Parse name
        let name = self.parse_identifier();

        // Check for optional marker
        let question_token = self.parse_optional(SyntaxKind::QuestionToken);

        // Parse : and type
        self.parse_expected(SyntaxKind::ColonToken);
        let type_node = self.parse_type();

        let end_pos = self.token_end();

        // Create a named tuple member node
        self.arena.add_named_tuple_member(
            syntax_kind_ext::NAMED_TUPLE_MEMBER,
            start_pos,
            end_pos,
            crate::parser::thin_node::NamedTupleMemberData {
                dot_dot_dot_token,
                name,
                question_token,
                type_node,
            },
        )
    }

    /// Parse tuple type: [T, U, V], [name: T], [...T[]], [T?]
    fn parse_tuple_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBracketToken);

        let mut elements = Vec::new();

        while !self.is_token(SyntaxKind::CloseBracketToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let element = self.parse_tuple_element_type();
            elements.push(element);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        self.parse_expected(SyntaxKind::CloseBracketToken);
        let end_pos = self.token_end();

        let tuple = self.arena.add_tuple_type(
            syntax_kind_ext::TUPLE_TYPE,
            start_pos,
            end_pos,
            crate::parser::thin_node::TupleTypeData {
                elements: self.make_node_list(elements),
            },
        );

        // Handle array of tuples: [T, U][]
        if self.is_token(SyntaxKind::OpenBracketToken) {
            return self.parse_array_type(start_pos, tuple);
        }

        tuple
    }

    /// Parse literal type: "foo", 42, 123n, true, false
    fn parse_literal_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the literal expression
        let literal = match self.token() {
            SyntaxKind::StringLiteral => self.parse_string_literal(),
            SyntaxKind::NumericLiteral => self.parse_numeric_literal(),
            SyntaxKind::BigIntLiteral => self.parse_bigint_literal(),
            SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword => self.parse_boolean_literal(),
            _ => {
                // Fallback - shouldn't happen
                self.parse_identifier()
            }
        };

        let end_pos = self.token_end();

        self.arena.add_literal_type(
            syntax_kind_ext::LITERAL_TYPE,
            start_pos,
            end_pos,
            crate::parser::thin_node::LiteralTypeData { literal },
        )
    }

    /// Parse prefix unary literal type: -1, -42
    /// In TypeScript, negative number literals in type position are
    /// represented as a PrefixUnaryExpression wrapped in a LiteralType
    fn parse_prefix_unary_literal_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the minus token
        let operator_kind = self.token() as u16;
        self.next_token();

        // Parse the numeric or bigint literal operand
        let operand = if self.is_token(SyntaxKind::BigIntLiteral) {
            self.parse_bigint_literal()
        } else {
            self.parse_numeric_literal()
        };

        let prefix_end = self.token_end();

        // Create prefix unary expression node
        let prefix_expr = self.arena.add_unary_expr(
            syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
            start_pos,
            prefix_end,
            crate::parser::thin_node::UnaryExprData {
                operator: operator_kind,
                operand,
            },
        );

        // Wrap in a literal type
        self.arena.add_literal_type(
            syntax_kind_ext::LITERAL_TYPE,
            start_pos,
            prefix_end,
            crate::parser::thin_node::LiteralTypeData {
                literal: prefix_expr,
            },
        )
    }

    /// Parse typeof type: typeof x, typeof x.y
    fn parse_typeof_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::TypeOfKeyword);

        // Parse the expression name (can be qualified: x.y.z)
        let expr_name = self.parse_entity_name();

        // Parse optional type arguments for instantiation expressions: typeof Err<U>
        let type_arguments = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_arguments())
        } else {
            None
        };

        let end_pos = self.token_end();

        self.arena.add_type_query(
            syntax_kind_ext::TYPE_QUERY,
            start_pos,
            end_pos,
            crate::parser::thin_node::TypeQueryData {
                expr_name,
                type_arguments,
            },
        )
    }

    /// Parse keyof type: keyof T
    fn parse_keyof_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let operator = self.token() as u16;
        self.parse_expected(SyntaxKind::KeyOfKeyword);

        // Parse the type operand
        let type_node = self.parse_primary_type();

        let end_pos = self.token_end();

        self.arena.add_type_operator(
            syntax_kind_ext::TYPE_OPERATOR,
            start_pos,
            end_pos,
            crate::parser::thin_node::TypeOperatorData {
                operator,
                type_node,
            },
        )
    }

    /// Parse unique type: unique symbol
    fn parse_unique_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let operator = self.token() as u16;
        self.parse_expected(SyntaxKind::UniqueKeyword);

        // Parse the type operand (unique symbol)
        let type_node = self.parse_primary_type();

        let end_pos = self.token_end();

        self.arena.add_type_operator(
            syntax_kind_ext::TYPE_OPERATOR,
            start_pos,
            end_pos,
            crate::parser::thin_node::TypeOperatorData {
                operator,
                type_node,
            },
        )
    }

    /// Parse readonly type: readonly T[]
    fn parse_readonly_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let operator = self.token() as u16;
        self.parse_expected(SyntaxKind::ReadonlyKeyword);

        // Parse the type operand
        let type_node = self.parse_primary_type();

        let end_pos = self.token_end();

        self.arena.add_type_operator(
            syntax_kind_ext::TYPE_OPERATOR,
            start_pos,
            end_pos,
            crate::parser::thin_node::TypeOperatorData {
                operator,
                type_node,
            },
        )
    }

    /// Parse infer type: infer T (used in conditional types)
    fn parse_infer_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::InferKeyword);

        // Parse the type parameter to infer
        let type_parameter = self.parse_type_parameter();

        let end_pos = self.token_end();

        self.arena.add_infer_type(
            syntax_kind_ext::INFER_TYPE,
            start_pos,
            end_pos,
            crate::parser::thin_node::InferTypeData { type_parameter },
        )
    }

    /// Parse template literal type: `hello` or `prefix${T}suffix`
    fn parse_template_literal_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the head (either NoSubstitutionTemplateLiteral or TemplateHead)
        if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral) {
            // Simple template literal type with no substitutions: `hello`
            let head = self.parse_template_literal_head();
            let end_pos = self.token_end();

            return self.arena.add_template_literal_type(
                syntax_kind_ext::TEMPLATE_LITERAL_TYPE,
                start_pos,
                end_pos,
                crate::parser::thin_node::TemplateLiteralTypeData {
                    head,
                    template_spans: self.make_node_list(vec![]),
                },
            );
        }

        // Template with substitutions: `prefix${T}middle${U}suffix`
        let head = self.parse_template_literal_head();
        let mut spans = Vec::new();

        // After the head, we need to parse: type, then middle/tail, repeat until tail
        loop {
            // Parse the type inside ${...}
            let type_node = self.parse_type();

            // Now we need to rescan for the template continuation
            // The scanner needs to be told to rescan as template
            self.scanner.re_scan_template_token(false);
            self.current_token = self.scanner.get_token();

            let span_start = self.token_pos();
            let is_tail = self.is_token(SyntaxKind::TemplateTail);

            // Parse the template middle/tail literal
            let literal = self.parse_template_literal_span();
            let span_end = self.token_end();

            // Create a template span node
            // Note: We reuse TemplateSpanData, using 'expression' field for the type node
            let span = self.arena.add_template_span(
                syntax_kind_ext::TEMPLATE_LITERAL_TYPE_SPAN,
                span_start,
                span_end,
                crate::parser::thin_node::TemplateSpanData {
                    expression: type_node,
                    literal,
                },
            );
            spans.push(span);

            if is_tail {
                break;
            }
        }

        let end_pos = self.token_end();

        self.arena.add_template_literal_type(
            syntax_kind_ext::TEMPLATE_LITERAL_TYPE,
            start_pos,
            end_pos,
            crate::parser::thin_node::TemplateLiteralTypeData {
                head,
                template_spans: self.make_node_list(spans),
            },
        )
    }

    /// Parse template literal head (NoSubstitutionTemplateLiteral or TemplateHead)
    fn parse_template_literal_head(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let is_unterminated = self.scanner.is_unterminated();
        let kind = self.token() as u16;
        let text = self.scanner.get_token_value_ref().to_string();
        let literal_end = self.token_end();
        self.next_token();
        let end_pos = self.token_end();
        if is_unterminated {
            self.error_unterminated_template_literal_at(start_pos, literal_end);
        }
        self.arena.add_literal(
            kind,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    /// Parse template literal span (TemplateMiddle or TemplateTail)
    fn parse_template_literal_span(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let is_unterminated = self.scanner.is_unterminated();
        let kind = self.token() as u16;
        let text = self.scanner.get_token_value_ref().to_string();
        let literal_end = self.token_end();
        self.next_token();
        let end_pos = self.token_end();
        if is_unterminated {
            self.error_unterminated_template_literal_at(start_pos, literal_end);
        }
        self.arena.add_literal(
            kind,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    /// Parse object type literal or mapped type
    /// Object type: { prop: T; method(): U }
    /// Mapped type: { [K in keyof T]: U } or { readonly [K in T]?: U }
    /// Index signature: { [key: string]: T }
    fn parse_object_or_mapped_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        // Check if this is a mapped type: [ followed by identifier and 'in'
        // vs index signature: [ followed by identifier and ':'
        if self.is_token(SyntaxKind::OpenBracketToken) {
            if self.look_ahead_is_mapped_type_start() {
                return self.parse_mapped_type_rest(start_pos);
            }
            // Not a mapped type - let type literal parsing handle index signature
            return self.parse_type_literal_rest(start_pos);
        }

        // Check for readonly/+/- prefixed mapped type
        if (self.is_token(SyntaxKind::ReadonlyKeyword) && self.look_ahead_is_mapped_type())
            || (self.is_token(SyntaxKind::PlusToken) || self.is_token(SyntaxKind::MinusToken))
        {
            return self.parse_mapped_type_rest(start_pos);
        }

        // Otherwise it's an object type literal - parse as type literal
        self.parse_type_literal_rest(start_pos)
    }

    /// Look ahead to see if [ starts a mapped type (has 'in' keyword) vs index signature (has ':')
    fn look_ahead_is_mapped_type_start(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip [

        // Skip identifier
        if self.is_token(SyntaxKind::Identifier) {
            self.next_token();
        }

        // Check if followed by 'in' (mapped type) or ':' (index signature)
        let is_mapped = self.is_token(SyntaxKind::InKeyword);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_mapped
    }

    /// Look ahead to check if readonly is followed by [ (mapped type) vs property
    fn look_ahead_is_mapped_type(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip readonly
        let is_mapped = self.is_token(SyntaxKind::OpenBracketToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_mapped
    }

    /// Parse mapped type after opening brace: { [K in T]: U }
    fn parse_mapped_type_rest(&mut self, start_pos: u32) -> NodeIndex {
        // Parse optional readonly modifier with +/- prefix
        let readonly_token = if self.is_token(SyntaxKind::ReadonlyKeyword) {
            let pos = self.token_pos();
            self.next_token();
            self.arena
                .add_token(SyntaxKind::ReadonlyKeyword as u16, pos, self.token_end())
        } else if self.is_token(SyntaxKind::PlusToken) || self.is_token(SyntaxKind::MinusToken) {
            let pos = self.token_pos();
            let kind = self.token() as u16;
            self.next_token();
            if self.is_token(SyntaxKind::ReadonlyKeyword) {
                self.next_token();
            }
            self.arena.add_token(kind, pos, self.token_end())
        } else {
            NodeIndex::NONE
        };

        // Parse [K in T]
        self.parse_expected(SyntaxKind::OpenBracketToken);

        // Parse the type parameter: K in T
        let type_param_start = self.token_pos();
        let param_name = self.parse_identifier();

        self.parse_expected(SyntaxKind::InKeyword);

        let constraint = self.parse_type();

        // Parse optional 'as' clause for key remapping: [K in T as NewKey]
        let name_type = if self.parse_optional(SyntaxKind::AsKeyword) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        let type_param_end = self.token_end();

        let type_parameter = self.arena.add_type_parameter(
            syntax_kind_ext::TYPE_PARAMETER,
            type_param_start,
            type_param_end,
            crate::parser::thin_node::TypeParameterData {
                modifiers: None,
                name: param_name,
                constraint,
                default: NodeIndex::NONE,
            },
        );

        self.parse_expected(SyntaxKind::CloseBracketToken);

        // Parse optional ? modifier with +/- prefix
        let question_token = if self.is_token(SyntaxKind::QuestionToken) {
            let pos = self.token_pos();
            self.next_token();
            self.arena
                .add_token(SyntaxKind::QuestionToken as u16, pos, self.token_end())
        } else if self.is_token(SyntaxKind::PlusToken) || self.is_token(SyntaxKind::MinusToken) {
            let pos = self.token_pos();
            let kind = self.token() as u16;
            self.next_token();
            if self.is_token(SyntaxKind::QuestionToken) {
                self.next_token();
            }
            self.arena.add_token(kind, pos, self.token_end())
        } else {
            NodeIndex::NONE
        };

        // Parse optional : and type (type can be omitted for implicit any)
        let type_node = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        // Parse optional semicolon
        self.parse_optional(SyntaxKind::SemicolonToken);

        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        self.arena.add_mapped_type(
            syntax_kind_ext::MAPPED_TYPE,
            start_pos,
            end_pos,
            crate::parser::thin_node::MappedTypeData {
                readonly_token,
                type_parameter,
                name_type,
                question_token,
                type_node,
                members: None,
            },
        )
    }

    /// Parse type literal (object type) after opening brace
    fn parse_type_literal_rest(&mut self, start_pos: u32) -> NodeIndex {
        let mut members = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let saved_pos = self.token_pos();
            let member = self.parse_type_member();

            // If parse_type_member returned NONE (couldn't parse) and we haven't advanced,
            // skip the current token to prevent infinite loops
            if member.is_none() && self.token_pos() == saved_pos {
                self.error_unexpected_token();
                self.next_token(); // Skip the problematic token
                continue;
            }

            if !member.is_none() {
                members.push(member);
            }

            // Allow comma or semicolon as separator
            if !self.parse_optional(SyntaxKind::SemicolonToken) {
                self.parse_optional(SyntaxKind::CommaToken);
            }
        }

        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        self.arena.add_type_literal(
            syntax_kind_ext::TYPE_LITERAL,
            start_pos,
            end_pos,
            crate::parser::thin_node::TypeLiteralData {
                members: self.make_node_list(members),
            },
        )
    }

    /// Check if the current token starts with `>` (includes compound tokens like `>>`, `>>>`, `>=`, etc.)
    fn is_greater_than_or_compound(&self) -> bool {
        matches!(
            self.current_token,
            SyntaxKind::GreaterThanToken
                | SyntaxKind::GreaterThanGreaterThanToken
                | SyntaxKind::GreaterThanGreaterThanGreaterThanToken
                | SyntaxKind::GreaterThanEqualsToken
                | SyntaxKind::GreaterThanGreaterThanEqualsToken
                | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
        )
    }

    /// Parse type arguments: <T, U, V>
    fn parse_type_arguments(&mut self) -> NodeList {
        self.parse_expected(SyntaxKind::LessThanToken);

        let mut args = Vec::new();

        while !self.is_greater_than_or_compound() && !self.is_token(SyntaxKind::EndOfFileToken) {
            args.push(self.parse_type());

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        self.parse_expected_greater_than();
        self.make_node_list(args)
    }

    /// Try to parse type arguments for a call expression: foo<T>()
    /// Returns Some(NodeList) if successful, None if this is not type arguments.
    /// Uses look-ahead to distinguish from comparison operators.
    fn try_parse_type_arguments_for_call(&mut self) -> Option<NodeList> {
        // Save state for potential rollback
        let snapshot = self.scanner.save_state();
        let saved_token = self.current_token;
        let saved_arena_len = self.arena.nodes.len();
        let saved_diagnostics_len = self.parse_diagnostics.len();

        // Consume <
        self.next_token();

        let mut args = Vec::new();
        let mut depth = 1;

        // Parse type arguments
        while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
            // Try to parse a type
            if args.is_empty() || self.is_token(SyntaxKind::CommaToken) {
                if !args.is_empty() {
                    self.next_token(); // consume comma
                }

                // Check for nested < (generic types within type arguments)
                let type_node = self.parse_type();
                args.push(type_node);
            }

            if self.is_greater_than_or_compound() {
                depth -= 1;
            } else if self.is_token(SyntaxKind::CommaToken) {
                // Continue to next type argument
                continue;
            } else if self.is_token(SyntaxKind::SemicolonToken)
                || self.is_token(SyntaxKind::CloseBraceToken)
                || self.is_token(SyntaxKind::EndOfFileToken)
            {
                // Invalid - not type arguments
                break;
            } else {
                // Something unexpected - might not be type arguments
                break;
            }
        }

        if depth == 0 {
            // Successfully parsed type arguments, now consume >
            self.parse_expected_greater_than();

            // Check if followed by ( or ` (which indicates a call/tagged template)
            if self.is_token(SyntaxKind::OpenParenToken)
                || self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
                || self.is_token(SyntaxKind::TemplateHead)
            {
                return Some(self.make_node_list(args));
            }
        }

        // Not type arguments - restore state
        self.scanner.restore_state(snapshot);
        self.current_token = saved_token;
        // Truncate arena to remove any nodes we added
        self.arena.nodes.truncate(saved_arena_len);
        // Drop any speculative diagnostics from the failed parse
        self.parse_diagnostics.truncate(saved_diagnostics_len);
        None
    }

    /// Parse array type suffix (T[]) or indexed access type (T[K])
    fn parse_array_type(&mut self, start_pos: u32, element_type: NodeIndex) -> NodeIndex {
        let mut current = element_type;

        while self.is_token(SyntaxKind::OpenBracketToken) {
            if self.look_ahead_is_index_signature() {
                break;
            }
            self.next_token();

            // Check if this is array type [] or indexed access type [K]
            if self.is_token(SyntaxKind::CloseBracketToken) {
                // Array type: T[]
                self.next_token();
                let end_pos = self.token_end();

                current = self.arena.add_array_type(
                    syntax_kind_ext::ARRAY_TYPE,
                    start_pos,
                    end_pos,
                    crate::parser::thin_node::ArrayTypeData {
                        element_type: current,
                    },
                );
            } else {
                // Indexed access type: T[K]
                let index_type = self.parse_type();
                self.parse_expected(SyntaxKind::CloseBracketToken);
                let end_pos = self.token_end();

                current = self.arena.add_indexed_access_type(
                    syntax_kind_ext::INDEXED_ACCESS_TYPE,
                    start_pos,
                    end_pos,
                    crate::parser::thin_node::IndexedAccessTypeData {
                        object_type: current,
                        index_type,
                    },
                );
            }
        }

        current
    }

    /// Check if current keyword can be used as a property name
    /// (when followed by :, ?, (, <, or at end of type member)
    fn look_ahead_is_property_name_after_keyword(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip the keyword
        self.next_token();

        // If followed by these, the keyword is being used as a property name
        let is_property_name = self.is_token(SyntaxKind::ColonToken)
            || self.is_token(SyntaxKind::QuestionToken)
            || self.is_token(SyntaxKind::OpenParenToken)
            || self.is_token(SyntaxKind::LessThanToken)
            || self.is_token(SyntaxKind::SemicolonToken)
            || self.is_token(SyntaxKind::CommaToken)
            || self.is_token(SyntaxKind::CloseBraceToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_property_name
    }

    /// Check if current token is a keyword that can be used as a property name
    fn is_property_name_keyword(&self) -> bool {
        matches!(
            self.token(),
            SyntaxKind::TypeKeyword
                | SyntaxKind::GetKeyword
                | SyntaxKind::SetKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::AwaitKeyword
                | SyntaxKind::NewKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::OverrideKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::ExportKeyword
                | SyntaxKind::DefaultKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::VarKeyword
                | SyntaxKind::IfKeyword
                | SyntaxKind::ElseKeyword
                | SyntaxKind::ForKeyword
                | SyntaxKind::WhileKeyword
                | SyntaxKind::DoKeyword
                | SyntaxKind::SwitchKeyword
                | SyntaxKind::CaseKeyword
                | SyntaxKind::BreakKeyword
                | SyntaxKind::ContinueKeyword
                | SyntaxKind::ReturnKeyword
                | SyntaxKind::ThrowKeyword
                | SyntaxKind::TryKeyword
                | SyntaxKind::CatchKeyword
                | SyntaxKind::FinallyKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::ImportKeyword
                | SyntaxKind::FromKeyword
                | SyntaxKind::AsKeyword
                | SyntaxKind::InKeyword
                | SyntaxKind::OfKeyword
                | SyntaxKind::InstanceOfKeyword
                | SyntaxKind::ThisKeyword
                | SyntaxKind::SuperKeyword
                | SyntaxKind::DeleteKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::TypeOfKeyword
                | SyntaxKind::YieldKeyword
                | SyntaxKind::ConstructorKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::ImplementsKeyword
                | SyntaxKind::ExtendsKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::RequireKeyword
                | SyntaxKind::GlobalKeyword
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::NullKeyword
                | SyntaxKind::UndefinedKeyword
                | SyntaxKind::OutKeyword
                | SyntaxKind::SatisfiesKeyword
                | SyntaxKind::AssertKeyword
                | SyntaxKind::AssertsKeyword
                | SyntaxKind::KeyOfKeyword
                | SyntaxKind::UniqueKeyword
                | SyntaxKind::InferKeyword
                | SyntaxKind::IsKeyword
                | SyntaxKind::NeverKeyword
                | SyntaxKind::UnknownKeyword
                | SyntaxKind::BigIntKeyword
                | SyntaxKind::ObjectKeyword
                | SyntaxKind::StringKeyword
                | SyntaxKind::NumberKeyword
                | SyntaxKind::SymbolKeyword
                | SyntaxKind::UsingKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::DeferKeyword
        )
    }

    /// Look ahead to see if ( starts a function type: () => T or (x: T) => U
    fn look_ahead_is_function_type(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip (
        self.next_token();

        // Empty params: () =>
        if self.is_token(SyntaxKind::CloseParenToken) {
            self.next_token();
            let is_arrow = self.is_token(SyntaxKind::EqualsGreaterThanToken);
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return is_arrow;
        }

        // Check for parameter-like syntax: identifier or keyword followed by : or )
        // If we see just a type (like `string`), it could be parenthesized type
        // Function type params have: `name:` or `modifier name` where modifier is public/private/protected/readonly
        if self.is_identifier_or_keyword() {
            self.next_token();
            // If followed by : it's definitely a function type parameter
            if self.is_token(SyntaxKind::ColonToken) {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                return true;
            }
            // If followed by a parameter modifier (public, private, protected, readonly), it's a parameter
            // But NOT if followed by 'extends' - that's a conditional type!
            let is_param_modifier = matches!(
                self.token(),
                SyntaxKind::PublicKeyword
                    | SyntaxKind::PrivateKeyword
                    | SyntaxKind::ProtectedKeyword
                    | SyntaxKind::ReadonlyKeyword
            );
            if is_param_modifier {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                return true;
            }
        }

        // For other cases, skip to matching ) to check for =>
        // First restore, then scan again
        self.scanner.restore_state(snapshot);
        self.current_token = current;

        let snapshot2 = self.scanner.save_state();
        self.next_token(); // Skip (

        let mut depth = 1;
        while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
            if self.is_token(SyntaxKind::OpenParenToken) {
                depth += 1;
            } else if self.is_token(SyntaxKind::CloseParenToken) {
                depth -= 1;
            }
            self.next_token();
        }

        let is_arrow = self.is_token(SyntaxKind::EqualsGreaterThanToken);
        self.scanner.restore_state(snapshot2);
        self.current_token = current;
        is_arrow
    }

    /// Parse function type: (x: T, y: U) => V
    fn parse_function_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse parameters
        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_type_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Parse =>
        self.parse_expected(SyntaxKind::EqualsGreaterThanToken);

        // Parse return type (supports type predicates: param is T)
        let type_annotation = self.parse_return_type();

        let end_pos = self.token_end();

        self.arena.add_function_type(
            syntax_kind_ext::FUNCTION_TYPE,
            start_pos,
            end_pos,
            crate::parser::thin_node::FunctionTypeData {
                type_parameters: None,
                parameters,
                type_annotation,
                is_abstract: false,
            },
        )
    }

    /// Parse generic function type: <T>() => T or <T, U extends V>(x: T) => U
    fn parse_generic_function_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse type parameters: <T, U extends V>
        let type_parameters = self.parse_type_parameters();

        // Parse parameters: (x: T, y: U)
        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_type_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Parse =>
        self.parse_expected(SyntaxKind::EqualsGreaterThanToken);

        // Parse return type (supports type predicates: param is T)
        let type_annotation = self.parse_return_type();

        let end_pos = self.token_end();

        self.arena.add_function_type(
            syntax_kind_ext::FUNCTION_TYPE,
            start_pos,
            end_pos,
            crate::parser::thin_node::FunctionTypeData {
                type_parameters: Some(type_parameters),
                parameters,
                type_annotation,
                is_abstract: false,
            },
        )
    }

    /// Parse constructor type: new () => T or new <T>() => T
    /// Also handles abstract constructor types: abstract new () => T
    fn parse_constructor_type(&mut self, is_abstract: bool) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::NewKeyword);

        // Parse optional type parameters: new <T>() => T
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        // Parse parameters: new (x: T, y: U) => ...
        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_type_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Parse => and return type
        self.parse_expected(SyntaxKind::EqualsGreaterThanToken);
        let type_annotation = self.parse_return_type();

        let end_pos = self.token_end();

        // Use ConstructorType kind - reuse FunctionTypeData since structure is the same
        self.arena.add_function_type(
            syntax_kind_ext::CONSTRUCTOR_TYPE,
            start_pos,
            end_pos,
            crate::parser::thin_node::FunctionTypeData {
                type_parameters,
                parameters,
                type_annotation,
                is_abstract,
            },
        )
    }

    /// Parse type parameter list for function types: (x: T, y: U)
    /// Also handles invalid modifiers like (public x) which TypeScript parses but errors on semantically
    fn parse_type_parameter_list(&mut self) -> NodeList {
        let mut params = Vec::new();

        while !self.is_token(SyntaxKind::CloseParenToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let param_start = self.token_pos();

            // Parse optional modifiers (public/private/protected/readonly)
            // These are syntactically valid but semantically invalid in function types
            let modifiers = self.parse_parameter_modifiers();

            // Parse optional ...rest
            let dot_dot_dot = self.parse_optional(SyntaxKind::DotDotDotToken);

            // Parse parameter name - can be identifier, keyword, or binding pattern
            let name = if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_object_binding_pattern()
            } else if self.is_token(SyntaxKind::OpenBracketToken) {
                self.parse_array_binding_pattern()
            } else if self.is_identifier_or_keyword() {
                self.parse_identifier_name()
            } else {
                self.parse_identifier()
            };

            // Parse optional ?
            let question = self.parse_optional(SyntaxKind::QuestionToken);

            // Parse type annotation
            let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_type()
            } else {
                NodeIndex::NONE
            };

            let param_end = self.token_end();

            let param = self.arena.add_parameter(
                syntax_kind_ext::PARAMETER,
                param_start,
                param_end,
                crate::parser::thin_node::ParameterData {
                    modifiers,
                    dot_dot_dot_token: dot_dot_dot,
                    name,
                    question_token: question,
                    type_annotation,
                    initializer: NodeIndex::NONE,
                },
            );
            params.push(param);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        self.make_node_list(params)
    }

    /// Parse a keyword as an identifier (for type keywords like string, number, etc.)
    fn parse_keyword_as_identifier(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let text = self.scanner.get_token_value_ref().to_string();
        self.next_token();
        let end_pos = self.token_end();

        self.arena.add_identifier(
            SyntaxKind::Identifier as u16,
            start_pos,
            end_pos,
            IdentifierData {
                escaped_text: text,
                original_text: None,
                type_arguments: None,
            },
        )
    }

    /// Parse qualified name rest: given a left name, parse `.Right.Rest` parts
    /// Handles: foo.Bar, A.B.C, etc.
    fn parse_qualified_name_rest(&mut self, left: NodeIndex) -> NodeIndex {
        let mut current = left;

        while self.is_token(SyntaxKind::DotToken) {
            let start_pos = if let Some(node) = self.arena.get(current) {
                node.pos
            } else {
                self.token_pos()
            };

            self.next_token(); // consume .
            let right = self.parse_identifier_name();
            let end_pos = self.token_end();

            current = self.arena.add_qualified_name(
                syntax_kind_ext::QUALIFIED_NAME,
                start_pos,
                end_pos,
                crate::parser::thin_node::QualifiedNameData {
                    left: current,
                    right,
                },
            );
        }

        current
    }

    // =========================================================================
    // Accessors
    // =========================================================================

    /// Get parse diagnostics
    pub fn get_diagnostics(&self) -> &[ParseDiagnostic] {
        &self.parse_diagnostics
    }

    /// Get the arena
    pub fn get_arena(&self) -> &ThinNodeArena {
        &self.arena
    }

    /// Consume the parser and return the arena.
    /// This is used for lib files where we need to store the arena in an Arc.
    pub fn into_arena(self) -> ThinNodeArena {
        self.arena
    }

    /// Get node count
    pub fn get_node_count(&self) -> usize {
        self.arena.len()
    }

    /// Get the source text.
    /// Delegates to the scanner which owns the source text.
    pub fn get_source_text(&self) -> &str {
        self.scanner.source_text()
    }

    /// Get the file name
    pub fn get_file_name(&self) -> &str {
        &self.file_name
    }

    // =========================================================================
    // JSX Parsing
    // =========================================================================

    /// Determine if we should parse a type assertion or JSX element.
    /// Type assertions use <Type>expr syntax, JSX uses <Element>.
    fn parse_jsx_element_or_type_assertion(&mut self) -> NodeIndex {
        // In .tsx/.jsx files, all <...> syntax is JSX (use "as Type" for type assertions)
        // In .ts files, we need to distinguish type assertions from JSX
        if self.is_jsx_file() {
            return self.parse_jsx_element_or_self_closing_or_fragment(true);
        }

        // In .ts files (non-JSX), always try to parse as type assertion first.
        // This will produce appropriate errors (e.g., TS1005 " '>' expected") for invalid JSX-like syntax.
        self.parse_type_assertion()
    }

    /// Check if this is a JSX fragment: <>
    fn look_ahead_is_jsx_fragment(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // consume <
        let is_fragment = self.is_token(SyntaxKind::GreaterThanToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_fragment
    }

    /// Parse a type assertion: <Type>expression
    fn parse_type_assertion(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::LessThanToken);
        let type_node = self.parse_type();
        self.parse_expected(SyntaxKind::GreaterThanToken);
        let expression = self.parse_unary_expression();
        let end_pos = self.token_end();

        self.arena.add_type_assertion(
            syntax_kind_ext::TYPE_ASSERTION,
            start_pos,
            end_pos,
            TypeAssertionData {
                type_node,
                expression,
            },
        )
    }

    /// Parse a JSX element, self-closing element, or fragment.
    /// Called when we see `<` in an expression context.
    fn parse_jsx_element_or_self_closing_or_fragment(
        &mut self,
        in_expression_context: bool,
    ) -> NodeIndex {
        let start_pos = self.token_pos();
        let opening = self.parse_jsx_opening_or_self_closing_or_fragment(in_expression_context);

        // Check what type of opening element we got
        let kind = self.arena.get(opening).map(|n| n.kind).unwrap_or(0);

        if kind == syntax_kind_ext::JSX_OPENING_ELEMENT {
            // Parse children and closing element
            let children = self.parse_jsx_children();
            let closing = self.parse_jsx_closing_element();
            let end_pos = self.token_end();

            self.arena.add_jsx_element(
                syntax_kind_ext::JSX_ELEMENT,
                start_pos,
                end_pos,
                crate::parser::thin_node::JsxElementData {
                    opening_element: opening,
                    children,
                    closing_element: closing,
                },
            )
        } else if kind == syntax_kind_ext::JSX_OPENING_FRAGMENT {
            // Parse children and closing fragment
            let children = self.parse_jsx_children();
            let closing = self.parse_jsx_closing_fragment();
            let end_pos = self.token_end();

            self.arena.add_jsx_fragment(
                syntax_kind_ext::JSX_FRAGMENT,
                start_pos,
                end_pos,
                crate::parser::thin_node::JsxFragmentData {
                    opening_fragment: opening,
                    children,
                    closing_fragment: closing,
                },
            )
        } else {
            // Self-closing element, already complete
            opening
        }
    }

    /// Parse JSX opening element, self-closing element, or opening fragment.
    fn parse_jsx_opening_or_self_closing_or_fragment(
        &mut self,
        _in_expression_context: bool,
    ) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::LessThanToken);

        // Check for fragment: <>
        if self.is_token(SyntaxKind::GreaterThanToken) {
            let end_pos = self.token_end();
            self.next_token(); // consume >
            return self
                .arena
                .add_token(syntax_kind_ext::JSX_OPENING_FRAGMENT, start_pos, end_pos);
        }

        // Parse tag name
        let tag_name = self.parse_jsx_element_name();

        // Parse optional type arguments
        let type_arguments = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_arguments())
        } else {
            None
        };

        // Parse attributes
        let attributes = self.parse_jsx_attributes();

        // Check for self-closing: />
        if self.is_token(SyntaxKind::SlashToken) {
            self.next_token(); // consume /
            let end_pos = self.token_end();
            self.parse_expected(SyntaxKind::GreaterThanToken);
            return self.arena.add_jsx_opening(
                syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT,
                start_pos,
                end_pos,
                crate::parser::thin_node::JsxOpeningData {
                    tag_name,
                    type_arguments,
                    attributes,
                },
            );
        }

        // Opening element: consume > and continue parsing children
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::GreaterThanToken);
        self.arena.add_jsx_opening(
            syntax_kind_ext::JSX_OPENING_ELEMENT,
            start_pos,
            end_pos,
            crate::parser::thin_node::JsxOpeningData {
                tag_name,
                type_arguments,
                attributes,
            },
        )
    }

    /// Parse JSX element name (identifier, this, namespaced, or property access).
    fn parse_jsx_element_name(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Error recovery: if the current token can't start a JSX element name,
        // return a missing identifier to avoid crashes
        if !self.is_token(SyntaxKind::Identifier)
            && !self.is_token(SyntaxKind::ThisKeyword)
            && !self.is_identifier_or_keyword()
        {
            self.error_identifier_expected();
            // Create a missing identifier node
            let end_pos = self.token_end();
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                start_pos,
                end_pos,
                IdentifierData {
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            );
        }

        // Parse the initial name (identifier or this)
        let mut expr = if self.is_token(SyntaxKind::ThisKeyword) {
            let pos = self.token_pos();
            self.next_token();
            let end_pos = self.token_end();
            self.arena
                .add_token(SyntaxKind::ThisKeyword as u16, pos, end_pos)
        } else {
            if self.is_token(SyntaxKind::Identifier) {
                self.scanner.scan_jsx_identifier();
            }
            let name = self.parse_identifier();

            // Check for namespaced name (a:b)
            if self.is_token(SyntaxKind::ColonToken) {
                self.next_token(); // consume :
                let local_name = self.parse_identifier();
                let end_pos = self.token_end();
                return self.arena.add_jsx_namespaced_name(
                    syntax_kind_ext::JSX_NAMESPACED_NAME,
                    start_pos,
                    end_pos,
                    crate::parser::thin_node::JsxNamespacedNameData {
                        namespace: name,
                        name: local_name,
                    },
                );
            }

            name
        };

        // Parse property access chain (Foo.Bar.Baz)
        while self.is_token(SyntaxKind::DotToken) {
            self.next_token(); // consume .
            let name = self.parse_identifier();
            let end_pos = self.token_end();
            expr = self.arena.add_access_expr(
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                start_pos,
                end_pos,
                crate::parser::thin_node::AccessExprData {
                    expression: expr,
                    name_or_argument: name,
                    question_dot_token: false,
                },
            );
        }

        expr
    }

    /// Parse JSX attributes list.
    fn parse_jsx_attributes(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut properties = Vec::new();

        while !self.is_token(SyntaxKind::GreaterThanToken)
            && !self.is_token(SyntaxKind::SlashToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            if self.is_token(SyntaxKind::OpenBraceToken) {
                // Spread attribute: {...props}
                properties.push(self.parse_jsx_spread_attribute());
            } else {
                // Regular attribute: name="value" or name={expr} or just name
                properties.push(self.parse_jsx_attribute());
            }
        }

        let end_pos = self.token_end();
        self.arena.add_jsx_attributes(
            syntax_kind_ext::JSX_ATTRIBUTES,
            start_pos,
            end_pos,
            crate::parser::thin_node::JsxAttributesData {
                properties: self.make_node_list(properties),
            },
        )
    }

    /// Parse a single JSX attribute.
    fn parse_jsx_attribute(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Error recovery: if the current token can't start an attribute name,
        // report error and skip to next attribute or end of attributes
        if !self.is_token(SyntaxKind::Identifier) && !self.is_identifier_or_keyword() {
            self.error_identifier_expected();
            // Skip the invalid token to prevent infinite loops
            self.next_token();
            // Return a dummy attribute with missing name
            let end_pos = self.token_end();
            return self.arena.add_jsx_attribute(
                syntax_kind_ext::JSX_ATTRIBUTE,
                start_pos,
                end_pos,
                crate::parser::thin_node::JsxAttributeData {
                    name: NodeIndex::NONE,
                    initializer: NodeIndex::NONE,
                },
            );
        }

        let name = self.parse_jsx_attribute_name();

        // Check for value: = followed by string, expression, or nested JSX
        let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
            if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_jsx_expression()
            } else if self.is_token(SyntaxKind::LessThanToken) {
                self.parse_jsx_element_or_self_closing_or_fragment(true)
            } else {
                self.error_expression_expected();
                NodeIndex::NONE
            }
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_jsx_attribute(
            syntax_kind_ext::JSX_ATTRIBUTE,
            start_pos,
            end_pos,
            crate::parser::thin_node::JsxAttributeData { name, initializer },
        )
    }

    /// Parse JSX attribute name (possibly namespaced).
    /// JSX attribute names can be keywords like "extends", "class", etc.
    fn parse_jsx_attribute_name(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        if self.is_token(SyntaxKind::Identifier) {
            self.scanner.scan_jsx_identifier();
        }
        // Use parse_identifier_name to allow keywords as attribute names
        let name = self.parse_identifier_name();

        // Check for namespaced name (a:b)
        if self.is_token(SyntaxKind::ColonToken) {
            self.next_token(); // consume :
            // Also allow keywords for the local part of namespaced names
            let local_name = self.parse_identifier_name();
            let end_pos = self.token_end();
            return self.arena.add_jsx_namespaced_name(
                syntax_kind_ext::JSX_NAMESPACED_NAME,
                start_pos,
                end_pos,
                crate::parser::thin_node::JsxNamespacedNameData {
                    namespace: name,
                    name: local_name,
                },
            );
        }

        name
    }

    /// Parse a JSX spread attribute: {...props}
    fn parse_jsx_spread_attribute(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);
        self.parse_expected(SyntaxKind::DotDotDotToken);
        let expression = self.parse_expression();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_jsx_spread_attribute(
            syntax_kind_ext::JSX_SPREAD_ATTRIBUTE,
            start_pos,
            end_pos,
            crate::parser::thin_node::JsxSpreadAttributeData { expression },
        )
    }

    /// Parse a JSX expression: {expr} or {...expr}
    fn parse_jsx_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        // Check for spread: {...}
        let dot_dot_dot_token = self.parse_optional(SyntaxKind::DotDotDotToken);

        // Check for empty expression: {}
        let expression = if self.is_token(SyntaxKind::CloseBraceToken) {
            NodeIndex::NONE
        } else {
            self.parse_expression()
        };

        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_jsx_expression(
            syntax_kind_ext::JSX_EXPRESSION,
            start_pos,
            end_pos,
            crate::parser::thin_node::JsxExpressionData {
                dot_dot_dot_token,
                expression,
            },
        )
    }

    /// Parse JSX children (elements, text, expressions).
    fn parse_jsx_children(&mut self) -> NodeList {
        let mut children = Vec::new();

        loop {
            // Rescan in JSX context to get proper JsxText tokens and LessThanSlashToken
            // This is necessary because after parsing expressions or nested elements,
            // the scanner may not be in JSX mode.
            self.current_token = self.scanner.re_scan_jsx_token(true);

            match self.current_token {
                SyntaxKind::LessThanSlashToken => {
                    // Closing tag/fragment - stop parsing children
                    break;
                }
                SyntaxKind::LessThanToken => {
                    // Nested JSX element
                    children.push(self.parse_jsx_element_or_self_closing_or_fragment(false));
                }
                SyntaxKind::OpenBraceToken => {
                    // JSX expression: {expr}
                    children.push(self.parse_jsx_expression());
                }
                SyntaxKind::JsxText => {
                    // Text node
                    children.push(self.parse_jsx_text());
                }
                SyntaxKind::EndOfFileToken => {
                    break;
                }
                _ => {
                    // Unknown token in JSX children - stop
                    break;
                }
            }
        }

        self.make_node_list(children)
    }

    /// Parse JSX text content.
    fn parse_jsx_text(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let text = self.scanner.get_token_value_ref().to_string();
        let end_pos = self.token_end();
        self.next_token();

        self.arena.add_jsx_text(
            SyntaxKind::JsxText as u16,
            start_pos,
            end_pos,
            crate::parser::thin_node::JsxTextData {
                text,
                contains_only_trivia_white_spaces: false,
            },
        )
    }

    /// Parse a JSX closing element: </Foo>
    fn parse_jsx_closing_element(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // In JSX mode, </ is scanned as a single LessThanSlashToken
        self.parse_expected(SyntaxKind::LessThanSlashToken);
        let tag_name = self.parse_jsx_element_name();
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::GreaterThanToken);
        self.arena.add_jsx_closing(
            syntax_kind_ext::JSX_CLOSING_ELEMENT,
            start_pos,
            end_pos,
            crate::parser::thin_node::JsxClosingData { tag_name },
        )
    }

    /// Parse a JSX closing fragment: </>
    fn parse_jsx_closing_fragment(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // In JSX mode, </ is scanned as a single LessThanSlashToken
        self.parse_expected(SyntaxKind::LessThanSlashToken);
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::GreaterThanToken);
        self.arena
            .add_token(syntax_kind_ext::JSX_CLOSING_FRAGMENT, start_pos, end_pos)
    }

    /// Consume the parser and return its parts.
    /// This is useful for taking ownership of the arena after parsing.
    pub fn into_parts(self) -> (ThinNodeArena, Vec<ParseDiagnostic>) {
        (self.arena, self.parse_diagnostics)
    }
}
