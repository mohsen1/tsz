use super::state::*;
use crate::parser::node::*;
use crate::parser::{NodeIndex, NodeList};
use rustc_hash::FxHashMap;
use tracing::trace;
use tsz_common::Atom;
use tsz_scanner::SyntaxKind;

impl ParserState {
    pub(crate) fn missing_semicolon_after_expression_text(
        &self,
        expression: NodeIndex,
    ) -> Option<(u32, u32, String)> {
        let node = self.arena.get(expression)?;

        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        // Use source text directly — arena identifier data may be empty for
        // identifiers created during parsing before data is fully populated.
        let source = self.scanner.source_text();
        let text = &source[node.pos as usize..node.end as usize];
        if text.is_empty() {
            return None;
        }

        Some((node.pos, node.end - node.pos, text.to_string()))
    }

    pub(crate) fn parse_missing_semicolon_keyword_error(
        &mut self,
        pos: u32,
        len: u32,
        expression_text: &str,
    ) -> bool {
        use tsz_common::diagnostics::diagnostic_codes;

        match expression_text {
            "const" | "let" | "var" => {
                self.parse_error_at(
                    pos,
                    len,
                    "Variable declaration not allowed at this location.",
                    diagnostic_codes::VARIABLE_DECLARATION_NOT_ALLOWED_AT_THIS_LOCATION,
                );
                true
            }
            "declare" => true,
            "interface" => {
                if self.is_token(SyntaxKind::OpenBraceToken) {
                    self.parse_error_at_current_token(
                        "Interface must be given a name.",
                        diagnostic_codes::INTERFACE_MUST_BE_GIVEN_A_NAME,
                    );
                } else {
                    let name = self.scanner.get_token_value_ref().to_string();
                    self.parse_error_at_current_token(
                        &format!("Interface name cannot be '{name}'."),
                        diagnostic_codes::EXPECTED,
                    );
                }
                true
            }
            "type" => {
                self.parse_error_at_current_token("'=' expected.", diagnostic_codes::EXPECTED);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn should_suppress_type_or_keyword_suggestion_for_missing_semicolon(
        &self,
        text: &str,
        token_pos: u32,
    ) -> bool {
        if !matches!(
            text,
            "string"
                | "number"
                | "boolean"
                | "symbol"
                | "bigint"
                | "object"
                | "void"
                | "undefined"
                | "null"
                | "never"
                | "unknown"
                | "any"
        ) {
            return false;
        }

        let source = self.scanner.source_text().as_bytes();
        let mut i = token_pos as usize;
        while i > 0 && source[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        i > 0 && source[i - 1] == b':'
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
    /// Note: This matches TypeScript's `canParseSemicolon()` implementation exactly.
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
    pub(crate) const fn is_at_expression_end(&self) -> bool {
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
    pub(crate) const fn is_statement_start(&self) -> bool {
        matches!(
            self.token(),
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
                | SyntaxKind::CatchKeyword
                | SyntaxKind::FinallyKeyword
                | SyntaxKind::WithKeyword
                | SyntaxKind::DebuggerKeyword
                | SyntaxKind::ReturnKeyword
                | SyntaxKind::BreakKeyword
                | SyntaxKind::ContinueKeyword
                | SyntaxKind::ThrowKeyword
                | SyntaxKind::YieldKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::UsingKeyword
                | SyntaxKind::AwaitKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::TypeKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::ImportKeyword
                | SyntaxKind::ExportKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::Identifier
                | SyntaxKind::StringLiteral
                | SyntaxKind::AtToken
                | SyntaxKind::NumericLiteral
                | SyntaxKind::BigIntLiteral
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::NullKeyword
                | SyntaxKind::ThisKeyword
                | SyntaxKind::SuperKeyword
                | SyntaxKind::ExclamationToken
                | SyntaxKind::TildeToken
                | SyntaxKind::PlusToken
                | SyntaxKind::MinusToken
                | SyntaxKind::PlusPlusToken
                | SyntaxKind::MinusMinusToken
                | SyntaxKind::TypeOfKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::DeleteKeyword
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::SemicolonToken
                | SyntaxKind::OpenParenToken
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::LessThanToken
        )
    }

    /// Check if current token is a synchronization point for error recovery
    /// This includes statement starts plus additional keywords that indicate
    /// boundaries in control structures (else, case, default, catch, finally, etc.)
    pub(crate) const fn is_resync_sync_point(&self) -> bool {
        self.is_statement_start()
            || matches!(
                self.token(),
                SyntaxKind::ElseKeyword
                    | SyntaxKind::CaseKeyword
                    | SyntaxKind::DefaultKeyword
                    | SyntaxKind::CatchKeyword
                    | SyntaxKind::FinallyKeyword
                    | SyntaxKind::CommaToken
            )
    }

    /// Resynchronize after a parse error by skipping to the next statement boundary
    /// This prevents cascading errors by finding a known good synchronization point.
    /// `allow_statement_starts` controls whether token kinds that begin statements
    /// (especially identifiers) are valid sync points.
    pub(crate) fn resync_after_error_with_statement_starts(
        &mut self,
        allow_statement_starts: bool,
    ) {
        // If we're already at a sync point or EOF, no need to resync
        if self.is_resync_sync_point_with_statement_starts(allow_statement_starts) {
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
                    if self.is_resync_sync_point_with_statement_starts(allow_statement_starts) {
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
                && self.is_resync_sync_point_with_statement_starts(allow_statement_starts)
            {
                break;
            }

            // Keep skipping tokens
            self.next_token();
        }
    }

    pub(crate) fn is_resync_sync_point_with_statement_starts(
        &self,
        allow_statement_starts: bool,
    ) -> bool {
        self.is_resync_sync_point()
            && (allow_statement_starts || self.token() != SyntaxKind::Identifier)
    }

    /// Default resync behavior: allow statement starts as sync points.
    pub(crate) fn resync_after_error(&mut self) {
        self.resync_after_error_with_statement_starts(true);
    }

    // =========================================================================
    // Expression-Level Error Recovery
    // =========================================================================

    /// Check if current token can start an expression
    pub(crate) const fn is_expression_start(&self) -> bool {
        matches!(
            self.token(),
            SyntaxKind::NumericLiteral
                | SyntaxKind::BigIntLiteral
                | SyntaxKind::StringLiteral
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::TemplateHead
                | SyntaxKind::TemplateMiddle
                | SyntaxKind::TemplateTail
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::NullKeyword
                | SyntaxKind::Identifier
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
                | SyntaxKind::LetKeyword
                | SyntaxKind::NewKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::DeleteKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::TypeOfKeyword
                | SyntaxKind::InstanceOfKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::OverrideKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::GetKeyword
                | SyntaxKind::SetKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::OfKeyword
                | SyntaxKind::SatisfiesKeyword
                | SyntaxKind::FromKeyword
                | SyntaxKind::AsKeyword
                | SyntaxKind::IsKeyword
                | SyntaxKind::AssertKeyword
                | SyntaxKind::AssertsKeyword
                | SyntaxKind::IntrinsicKeyword
                | SyntaxKind::OutKeyword
                | SyntaxKind::InferKeyword
                | SyntaxKind::ConstructorKeyword
                | SyntaxKind::UsingKeyword
                | SyntaxKind::KeyOfKeyword
                | SyntaxKind::UniqueKeyword
                | SyntaxKind::GlobalKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::DeferKeyword
                | SyntaxKind::PrivateIdentifier
                | SyntaxKind::PlusToken
                | SyntaxKind::MinusToken
                | SyntaxKind::AsteriskToken
                | SyntaxKind::TildeToken
                | SyntaxKind::ExclamationToken
                | SyntaxKind::PlusPlusToken
                | SyntaxKind::MinusMinusToken
                | SyntaxKind::OpenParenToken
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::LessThanToken
                | SyntaxKind::SlashToken
                | SyntaxKind::SlashEqualsToken
                | SyntaxKind::AtToken
        )
    }

    /// Check if current token is a binary operator
    pub(crate) const fn is_binary_operator(&self) -> bool {
        let precedence = self.get_operator_precedence(self.token());
        precedence > 0
    }

    /// Check if current token is at an expression boundary (a natural stopping point)
    pub(crate) const fn is_expression_boundary(&self) -> bool {
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
        if self.is_expression_boundary() || self.is_statement_start() {
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
                // error_token_expected already has error suppression check
                self.error_token_expected(">");
            }
        }
    }

    /// Check if the current token starts with `<` (includes `<<` and `<<=`).
    /// Mirrors `is_greater_than_or_compound` for the opening side.
    pub(crate) const fn is_less_than_or_compound(&self) -> bool {
        matches!(
            self.current_token,
            SyntaxKind::LessThanToken
                | SyntaxKind::LessThanLessThanToken
                | SyntaxKind::LessThanLessThanEqualsToken
        )
    }

    /// Consume a single `<` from the current token.
    /// Handles compound tokens like `<<` and `<<=` by leaving the scanner
    /// position unchanged (past the compound token) and setting `current_token`
    /// to the remainder. Unlike `>`, `<` is eagerly combined by the scanner,
    /// so we cannot back up—the scanner would re-combine. Instead, we leave
    /// pos past the compound and set `current_token` to the remainder.
    /// When the remainder is later consumed via `parse_expected(<)` →
    /// `next_token()`, the scanner scans from past the compound, correctly
    /// yielding the token that follows.
    pub(crate) fn parse_expected_less_than(&mut self) {
        match self.current_token {
            SyntaxKind::LessThanToken => {
                self.next_token();
            }
            SyntaxKind::LessThanLessThanToken => {
                // `<<` → consume first `<`, remainder is `<`
                // Scanner pos stays past `<<`; when the second `<` is consumed,
                // next_token() will scan from past both, yielding the following token.
                self.current_token = SyntaxKind::LessThanToken;
            }
            SyntaxKind::LessThanLessThanEqualsToken => {
                // `<<=` → consume first `<`, remainder is `<=`
                // Scanner pos stays past `<<=`; same logic as above.
                self.current_token = SyntaxKind::LessThanEqualsToken;
            }
            _ => {
                self.error_token_expected("<");
            }
        }
    }

    /// Create a `NodeList` from a Vec of `NodeIndex`
    pub(crate) const fn make_node_list(&self, nodes: Vec<NodeIndex>) -> NodeList {
        let _ = self;
        NodeList {
            nodes,
            pos: 0,
            end: 0,
            has_trailing_comma: false,
        }
    }

    pub(crate) fn drain_pending_recovered_expression_statements(
        &mut self,
        statements: &mut Vec<NodeIndex>,
    ) {
        statements.append(&mut self.pending_recovered_expression_statements);
    }

    /// Get operator precedence
    pub(crate) const fn get_operator_precedence(&self, token: SyntaxKind) -> u8 {
        // NOTE: Assignment operators (=, +=, -=, etc.) are NOT handled by the
        // binary expression chain. They are handled at a higher level in
        // parse_assignment_expression, matching tsc's separation of
        // parseAssignmentExpressionOrHigher vs parseBinaryExpressionRest.
        // They fall through to the default `_ => 0` arm below, which prevents
        // the binary expression loop from consuming `=` after error recovery
        // (e.g., `1 >> = 2` should parse as `1 >> <missing>; = 2;`, not
        // `(1 >> <missing>) = 2`).
        match token {
            SyntaxKind::CommaToken => 1,
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

    /// Push a new label scope (called when entering a function or module)
    pub(crate) fn push_label_scope(&mut self) {
        let new_depth = self.label_scopes.len() + 1;
        trace!(pos = self.token_pos(), new_depth, "push_label_scope");
        self.label_scopes.push(FxHashMap::default());
    }

    /// Pop the current label scope (called when exiting a function or module)
    pub(crate) fn pop_label_scope(&mut self) {
        let old_depth = self.label_scopes.len();
        trace!(pos = self.token_pos(), old_depth, "pop_label_scope");
        self.label_scopes.pop();
    }

    /// Check if a label already exists in the current scope, and if so, emit TS1114.
    /// Returns true if the label is a duplicate.
    pub(crate) fn check_duplicate_label(&mut self, label_name: &str, label_pos: u32) -> bool {
        let scope_depth = self.label_scopes.len();
        trace!(label_name, label_pos, scope_depth, "check_duplicate_label");
        if let Some(current_scope) = self.label_scopes.last_mut() {
            if current_scope.contains_key(label_name) {
                // Duplicate label - emit TS1114
                use tsz_common::diagnostics::diagnostic_codes;
                let message = format!("Duplicate label '{label_name}'.");
                trace!(label_name, "duplicate label found");
                self.parse_error_at(
                    label_pos,
                    self.u32_from_usize(label_name.len()),
                    &message,
                    diagnostic_codes::DUPLICATE_LABEL,
                );
                return true;
            }
            // Not a duplicate - record this label
            trace!(label_name, "adding label to scope");
            current_scope.insert(label_name.to_string(), label_pos);
        }
        false
    }
}
