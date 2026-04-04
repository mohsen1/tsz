//! Parser state - statement and declaration parsing methods
use super::state::{
    CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_CLASS_FIELD_INITIALIZER, CONTEXT_FLAG_GENERATOR,
    CONTEXT_FLAG_IN_BLOCK, CONTEXT_FLAG_PARAMETER_DEFAULT, CONTEXT_FLAG_STATIC_BLOCK,
    IncrementalParseResult, ParserState,
};
use crate::parser::{
    NodeIndex, NodeList,
    node::{
        BlockData, FunctionData, IdentifierData, ImportDeclData, LabeledData, QualifiedNameData,
        SourceFileData, VariableData, VariableDeclarationData,
    },
    parse_rules::{
        is_identifier_or_contextual_keyword, is_identifier_or_keyword, look_ahead_is,
        look_ahead_is_abstract_declaration, look_ahead_is_async_declaration,
        look_ahead_is_const_enum, look_ahead_is_import_call, look_ahead_is_import_equals,
        look_ahead_is_module_declaration, look_ahead_is_type_alias_declaration,
    },
    syntax_kind_ext,
};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::interner::Atom;
use tsz_scanner::{SyntaxKind, token_is_keyword};

impl ParserState {
    fn look_ahead_is_invalid_shebang(&mut self) -> bool {
        if !self.is_token(SyntaxKind::HashToken) || self.token_pos() == 0 {
            return false;
        }
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let result = self.is_token(SyntaxKind::ExclamationToken);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    fn recover_invalid_shebang_line(&mut self) {
        let start = self.u32_from_usize(self.token_pos() as usize);
        self.parse_error_at(
            start,
            2,
            "'#!' can only be used at the start of a file.",
            diagnostic_codes::CAN_ONLY_BE_USED_AT_THE_START_OF_A_FILE,
        );

        let source = self.scanner.source_text().as_bytes();
        let mut pos = self.token_pos() as usize + 2;
        let line_end = source[pos..]
            .iter()
            .position(|b| *b == b'\n' || *b == b'\r')
            .map_or(source.len(), |offset| pos + offset);

        while pos < line_end && source[pos].is_ascii_whitespace() {
            pos += 1;
        }
        while pos < line_end && !source[pos].is_ascii_whitespace() {
            pos += 1;
        }

        let mut arg_ranges = Vec::new();
        while pos < line_end {
            while pos < line_end && source[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if pos >= line_end {
                break;
            }
            let arg_start = pos;
            while pos < line_end && !source[pos].is_ascii_whitespace() {
                pos += 1;
            }
            arg_ranges.push((arg_start, pos - arg_start));
        }

        for (arg_start, arg_len) in arg_ranges {
            self.parse_error_at(
                self.u32_from_usize(arg_start),
                self.u32_from_usize(arg_len),
                "';' expected.",
                diagnostic_codes::EXPECTED,
            );
        }

        self.next_token(); // consume '#'
        if self.is_token(SyntaxKind::ExclamationToken) {
            self.next_token();
        }
        while !self.is_token(SyntaxKind::EndOfFileToken) && !self.scanner.has_preceding_line_break()
        {
            self.next_token();
        }
    }

    // =========================================================================
    // Parse Methods - Core Expressions
    // =========================================================================

    /// Parse a source file
    pub fn parse_source_file(&mut self) -> NodeIndex {
        let start_pos = 0u32;

        // Skip shebang (#!) if present at start of file
        self.scanner.scan_shebang_trivia();

        // Initialize scanner
        self.next_token();

        // Parse statements (using source file version that handles stray braces)
        let statements = self.parse_source_file_statements();

        // Cache comment ranges once during parsing (O(N) scan, done only once)
        // This avoids rescanning on every hover/documentation request
        // Use scanner's source text (no duplicate allocation)
        let comments = tsz_common::comments::get_comment_ranges(self.scanner.source_text());

        // Collect scanner-level diagnostics (e.g., conflict markers TS1185) into
        // parse diagnostics so they appear in the final diagnostic output.
        for diag in self.scanner.get_scanner_diagnostics() {
            self.parse_diagnostics.push(super::state::ParseDiagnostic {
                start: self.u32_from_usize(diag.pos),
                length: self.u32_from_usize(diag.length),
                message: diag.message.to_string(),
                code: diag.code,
            });
        }
        // Sort diagnostics by position to maintain correct order after merging
        self.parse_diagnostics.sort_by_key(|d| d.start);

        // Create source file node
        let end_pos = self.token_end();
        let eof_token = self
            .arena
            .add_token(SyntaxKind::EndOfFileToken as u16, end_pos, end_pos);

        // Transfer the scanner's string interner to the arena so that atom-based
        // identifier text resolution works via get_arena() (not just into_arena()).
        // This is essential for LSP features that resolve identifier references.
        self.arena.set_interner(self.scanner.interner().clone());

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
                is_declaration_file: self.is_declaration_file(),
                has_no_default_lib: false,
                comments, // Cached comment ranges
                parent: NodeIndex::NONE,
                id: 0,
                modifier_flags: 0,
                transform_flags: 0,
            },
        )
    }

    pub fn parse_source_file_statements_from_offset(
        &mut self,
        file_name: String,
        source_text: String,
        start: u32,
    ) -> IncrementalParseResult {
        let start = usize::min(start as usize, source_text.len());
        let reparse_start = self.u32_from_usize(start);

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
    pub(crate) fn parse_source_file_statements(&mut self) -> NodeList {
        let mut statements = Vec::new();
        let mut skip_after_binary_payload = false;
        let mut previous_statement_was_block = false;

        while !self.is_token(SyntaxKind::EndOfFileToken) {
            let pos_before = self.token_pos();
            if skip_after_binary_payload {
                break;
            }

            if self.look_ahead_is_invalid_shebang() {
                self.recover_invalid_shebang_line();
                previous_statement_was_block = false;
                continue;
            }

            if previous_statement_was_block && self.is_token(SyntaxKind::EqualsToken) {
                self.parse_error_at_current_token(
                    "Declaration or statement expected. This '=' follows a block of statements, so if you intended to write a destructuring assignment, you might need to wrap the whole assignment in parentheses.",
                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED_THIS_FOLLOWS_A_BLOCK_OF_STATEMENTS_SO_IF_YOU_I,
                );
                self.next_token();
                previous_statement_was_block = false;
                continue;
            }

            // Handle Unknown tokens (invalid characters) - must be checked FIRST
            // In tsc, the scanner emits TS1127 for each invalid character individually.
            // We must NOT resync here, because resync would skip over subsequent Unknown
            // tokens without emitting TS1127 for each one. Just advance one token.
            if self.is_token(SyntaxKind::Unknown) {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                    diagnostic_codes::INVALID_CHARACTER,
                );
                self.next_token();
                previous_statement_was_block = false;
                continue;
            }

            // Handle bare `#` that can't become a PrivateIdentifier.
            // In tsc, the scanner emits TS1127 for a standalone `#` that is not
            // followed by a valid identifier character. We try to rescan as a
            // PrivateIdentifier; if that fails, emit TS1127 and skip.
            if self.is_token(SyntaxKind::HashToken) {
                let rescanned = self.scanner.re_scan_hash_token();
                if rescanned == SyntaxKind::PrivateIdentifier {
                    // Got a valid private identifier — let the normal statement
                    // parser handle it (it will likely fail with a meaningful error).
                    self.current_token = rescanned;
                } else {
                    // Bare `#` — emit TS1127 and skip, matching tsc.
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                        diagnostic_codes::INVALID_CHARACTER,
                    );
                    self.next_token();
                    previous_statement_was_block = false;
                    continue;
                }
            }

            // If we see a closing brace at the top level, report error 1128
            if self.is_token(SyntaxKind::CloseBraceToken) {
                // Only emit error if we haven't already emitted one at this position
                if self.token_pos() != self.last_error_pos {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                }
                self.next_token();
                previous_statement_was_block = false;
                // If the token after a stray top-level `}` already starts an expression,
                // keep parsing there instead of resyncing past it. This preserves
                // follow-up recovery like `from "./foo"` -> TS1434 in malformed
                // import/export specifiers.
                if !self.is_expression_start() && !self.is_token(SyntaxKind::CloseBraceToken) {
                    self.resync_after_error();
                }
                continue;
            }

            if self.is_token(SyntaxKind::AtToken) {
                let snapshot = self.scanner.save_state();
                let at_token = self.current_token;
                self.next_token();
                if self.is_token(SyntaxKind::Unknown) {
                    // Current token is the Unknown (e.g. \x04) right after @.
                    // Report TS1127 at this position, not after re-scanning.
                    self.parse_error_at_current_token(
                        tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                        tsz_common::diagnostics::diagnostic_codes::INVALID_CHARACTER,
                    );

                    self.next_token();
                    if !self.is_token(SyntaxKind::EndOfFileToken) {
                        self.parse_error_at_current_token(
                            "Declaration or statement expected.",
                            tsz_common::diagnostics::diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                        );
                    }

                    skip_after_binary_payload = true;
                    previous_statement_was_block = false;
                    continue;
                }
                self.scanner.restore_state(snapshot);
                self.current_token = at_token;
            }

            let statement_start_token = self.token();
            let stmt = self.parse_statement();
            if stmt.is_none() {
                if self.is_token(SyntaxKind::GreaterThanToken) {
                    let snapshot = self.scanner.save_state();
                    let current_token = self.current_token;
                    self.next_token();
                    let followed_by_expression = self.is_expression_start();
                    self.scanner.restore_state(snapshot);
                    self.current_token = current_token;
                    if followed_by_expression {
                        self.next_token();
                        continue;
                    }
                }

                if self.is_token(SyntaxKind::CloseParenToken)
                    && !self.scanner.has_preceding_line_break()
                {
                    let source = self.scanner.source_text().as_bytes();
                    let mut i = self.token_pos() as usize;
                    while i > 0 && source[i - 1].is_ascii_whitespace() {
                        i -= 1;
                    }
                    if i > 0 && source[i - 1] == b')' {
                        self.parse_error_at_current_token(
                            "';' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        self.next_token();
                        continue;
                    }
                }

                if matches!(
                    self.token(),
                    SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken
                ) {
                    self.parse_error_at_current_token(
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                    self.next_token();
                    previous_statement_was_block = false;
                    continue;
                }

                // Statement parsing failed, resync to recover
                // Suppress cascading errors when:
                // 1. A recent error was within 3 chars, OR
                // 2. The token is a closing bracket/paren that is likely a
                //    stray artifact from earlier bracket-mismatch errors.
                let current = self.token_pos();
                let is_stray_close = self.last_error_pos != 0
                    && matches!(
                        self.token(),
                        SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken
                    );
                if (self.last_error_pos == 0 || current.abs_diff(self.last_error_pos) > 3)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                    && !is_stray_close
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                }
                // Resync to next statement boundary to continue parsing
                let allow_statement_starts = if statement_start_token == SyntaxKind::AtToken {
                    false
                } else {
                    !self.is_statement_start()
                };
                self.resync_after_error_with_statement_starts(allow_statement_starts);
                previous_statement_was_block = false;
            } else {
                previous_statement_was_block = self.arena.get(stmt).is_some_and(|node| {
                    node.kind == syntax_kind_ext::BLOCK
                        || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                        || node.kind == syntax_kind_ext::CLASS_DECLARATION
                        || node.kind == syntax_kind_ext::IF_STATEMENT
                        || node.kind == syntax_kind_ext::FOR_STATEMENT
                        || node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                        || node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                        || node.kind == syntax_kind_ext::WHILE_STATEMENT
                        || node.kind == syntax_kind_ext::DO_STATEMENT
                        || node.kind == syntax_kind_ext::SWITCH_STATEMENT
                        || node.kind == syntax_kind_ext::TRY_STATEMENT
                        || node.kind == syntax_kind_ext::WITH_STATEMENT
                });
                statements.push(stmt);
            }

            // Safety: if position didn't advance, force-skip the current token
            // to prevent infinite loop when resync returns at a sync point
            // that parse_statement can't handle
            if self.token_pos() == pos_before && !self.is_token(SyntaxKind::EndOfFileToken) {
                self.next_token();
            }
        }

        self.make_node_list(statements)
    }

    /// Parse list of statements (for blocks, function bodies, etc.).
    /// Stops at closing brace without error (closing brace is expected).
    /// Uses resynchronization to recover from errors and continue parsing.
    pub(crate) fn parse_statements(&mut self) -> NodeList {
        let mut statements = Vec::new();
        let mut previous_statement_was_block = false;

        while !self.is_token(SyntaxKind::EndOfFileToken)
            && !self.is_token(SyntaxKind::CloseBraceToken)
        {
            let pos_before = self.token_pos();

            if self.look_ahead_is_invalid_shebang() {
                self.recover_invalid_shebang_line();
                previous_statement_was_block = false;
                continue;
            }

            if previous_statement_was_block && self.is_token(SyntaxKind::EqualsToken) {
                self.parse_error_at_current_token(
                    "Declaration or statement expected. This '=' follows a block of statements, so if you intended to write a destructuring assignment, you might need to wrap the whole assignment in parentheses.",
                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED_THIS_FOLLOWS_A_BLOCK_OF_STATEMENTS_SO_IF_YOU_I,
                );
                self.next_token();
                previous_statement_was_block = false;
                continue;
            }

            // Error recovery: when inside a nested block within a class body (e.g.,
            // a method body with an unclosed `{`), terminate the block if we encounter
            // a class member modifier followed by an identifier on the same line. This
            // matches TSC's "abort parsing list" behavior: tokens that could start a
            // class member in an outer context cause the inner block list to terminate
            // rather than consuming tokens that belong to the class body.
            if self.in_block_context()
                && self.in_class_body()
                && matches!(
                    self.token(),
                    SyntaxKind::PublicKeyword
                        | SyntaxKind::PrivateKeyword
                        | SyntaxKind::ProtectedKeyword
                        | SyntaxKind::StaticKeyword
                        | SyntaxKind::AbstractKeyword
                        | SyntaxKind::ReadonlyKeyword
                        | SyntaxKind::OverrideKeyword
                        | SyntaxKind::AccessorKeyword
                )
                && self.look_ahead_next_is_identifier_or_keyword_on_same_line()
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Declaration or statement expected.",
                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                );
                break;
            }

            // Handle Unknown tokens (invalid characters)
            if self.is_token(SyntaxKind::Unknown) {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                    diagnostic_codes::INVALID_CHARACTER,
                );
                self.resync_after_error_with_statement_starts(false);
                previous_statement_was_block = false;
                continue;
            }

            let statement_start_token = self.token();
            let stmt = self.parse_statement();
            if stmt.is_none() {
                if matches!(
                    self.token(),
                    SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken
                ) {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                    self.next_token();
                    previous_statement_was_block = false;
                    continue;
                }

                // Statement parsing failed, resync to recover
                // Emit error if we haven't already at the exact same position
                // Suppress cascading errors when a recent error was within 3 chars
                let current = self.token_pos();
                if (self.last_error_pos == 0 || current.abs_diff(self.last_error_pos) > 3)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                }
                // Resync to next statement boundary to continue parsing
                let allow_statement_starts = if statement_start_token == SyntaxKind::AtToken {
                    false
                } else {
                    !self.is_statement_start()
                };
                self.resync_after_error_with_statement_starts(allow_statement_starts);
                previous_statement_was_block = false;
            } else {
                previous_statement_was_block = self.arena.get(stmt).is_some_and(|node| {
                    node.kind == syntax_kind_ext::BLOCK
                        || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                        || node.kind == syntax_kind_ext::CLASS_DECLARATION
                        || node.kind == syntax_kind_ext::IF_STATEMENT
                        || node.kind == syntax_kind_ext::FOR_STATEMENT
                        || node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                        || node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                        || node.kind == syntax_kind_ext::WHILE_STATEMENT
                        || node.kind == syntax_kind_ext::DO_STATEMENT
                        || node.kind == syntax_kind_ext::SWITCH_STATEMENT
                        || node.kind == syntax_kind_ext::TRY_STATEMENT
                        || node.kind == syntax_kind_ext::WITH_STATEMENT
                });
                statements.push(stmt);
            }

            // Safety: if position didn't advance, force-skip the current token
            // to prevent infinite loop when resync returns at a sync point
            // that parse_statement can't handle
            if self.token_pos() == pos_before
                && !self.is_token(SyntaxKind::EndOfFileToken)
                && !self.is_token(SyntaxKind::CloseBraceToken)
            {
                self.next_token();
            }
        }

        self.make_node_list(statements)
    }

    /// Parse a statement
    pub fn parse_statement(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::OpenBraceToken => self.parse_block(),
            SyntaxKind::VarKeyword => self.parse_variable_statement(),
            SyntaxKind::UsingKeyword => {
                if self.look_ahead_is_using_declaration() {
                    self.parse_variable_statement()
                } else {
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::LetKeyword => {
                // In strict mode (modules, classes, etc.), `let` is a reserved word and
                // cannot be used as an identifier. But `let;` or `let` followed by a
                // non-declaration-start token should NOT be parsed as a variable declaration.
                // tsc checks `isLetDeclaration()`: next token must be identifier, `{`, or `[`.
                if self.look_ahead_is_let_declaration() {
                    self.try_parse_invalid_let_array_declaration_statement()
                        .unwrap_or_else(|| self.parse_variable_statement())
                } else {
                    self.parse_expression_statement()
                }
            }
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
            SyntaxKind::AsyncKeyword => self.parse_statement_async_declaration_or_expression(),
            SyntaxKind::AwaitKeyword => {
                // await using declaration (ES2022)
                // Look ahead to see if it's "await using" or "await:" (labeled statement)
                if self.look_ahead_is_await_using_declaration() {
                    self.parse_variable_statement()
                } else if self.is_identifier_or_keyword() && self.look_ahead_is_labeled_statement()
                {
                    // 'await' as a label (e.g., "await: statement")
                    // In static blocks or async contexts, this will emit TS1003
                    self.parse_labeled_statement()
                } else {
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::AtToken => {
                if self.look_ahead_has_missing_decorator_expression() {
                    self.next_token();
                    self.error_expression_expected();
                    self.parse_statement()
                } else {
                    // Decorator: @decorator class/function
                    self.parse_decorated_declaration()
                }
            }
            SyntaxKind::ClassKeyword => self.parse_class_declaration(),
            SyntaxKind::AbstractKeyword => self.parse_statement_abstract_keyword(),
            SyntaxKind::AccessorKeyword => self.parse_statement_accessor_keyword(),
            // Modifier keywords used before declarations at top level
            // e.g., `public interface I {}`, `protected class C {}`, `static class C {}`
            // These should emit TS1044 and then parse the declaration
            SyntaxKind::StaticKeyword
            | SyntaxKind::PublicKeyword
            | SyntaxKind::ProtectedKeyword
            | SyntaxKind::PrivateKeyword
            | SyntaxKind::OverrideKeyword
            | SyntaxKind::ReadonlyKeyword => self.parse_statement_top_level_modifier(),
            SyntaxKind::DefaultKeyword => {
                // 'default' is only valid after 'export': emit TS1005 "'export' expected"
                self.parse_error_at_current_token("'export' expected.", diagnostic_codes::EXPECTED);
                self.next_token();
                self.parse_statement()
            }
            SyntaxKind::InterfaceKeyword => {
                // ASI: `interface\nI {}` should be parsed as expression statement
                // 'interface' followed by identifier 'I', not InterfaceDeclaration.
                if self.look_ahead_next_is_identifier_or_keyword_on_same_line() {
                    self.parse_interface_declaration()
                } else if self.look_ahead_next_is_open_brace_on_same_line() {
                    // `interface { }` — parse as interface with missing name (TS1438)
                    // rather than as expression statement. Matches tsc behavior.
                    self.parse_interface_declaration()
                } else {
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::TypeKeyword => self.parse_statement_type_keyword(),
            SyntaxKind::EnumKeyword => self.parse_enum_declaration(),
            SyntaxKind::DeclareKeyword => {
                // TS1184: 'declare' modifier cannot appear inside a block statement.
                // tsc emits this alongside the ambient declaration's own diagnostics.
                if self.in_block_context() {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Modifiers cannot appear here.",
                        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                    );
                }
                self.parse_statement_declare_or_expression()
            }
            SyntaxKind::NamespaceKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::GlobalKeyword => self.parse_statement_namespace_or_expression(),
            SyntaxKind::IfKeyword => self.parse_if_statement(),
            SyntaxKind::ReturnKeyword => self.parse_return_statement(),
            SyntaxKind::WhileKeyword => self.parse_while_statement(),
            SyntaxKind::ForKeyword => self.parse_for_statement(),
            SyntaxKind::SemicolonToken => self.parse_empty_statement(),
            SyntaxKind::ExportKeyword => {
                // Note: TS1184 for `export` in block context is NOT emitted by the parser.
                // The checker emits the specific grammar errors (TS1231, TS1233, TS1258)
                // and only emits TS1184 for `export` on class/function declarations.
                self.parse_export_declaration()
            }
            SyntaxKind::ImportKeyword => self.parse_statement_import_keyword(),
            SyntaxKind::BreakKeyword => self.parse_break_statement(),
            SyntaxKind::ContinueKeyword => self.parse_continue_statement(),
            SyntaxKind::ThrowKeyword => self.parse_throw_statement(),
            SyntaxKind::DoKeyword => self.parse_do_statement(),
            SyntaxKind::SwitchKeyword => self.parse_switch_statement(),
            SyntaxKind::TryKeyword | SyntaxKind::CatchKeyword | SyntaxKind::FinallyKeyword => {
                self.parse_try_statement()
            }
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

    fn parse_statement_async_declaration_or_expression(&mut self) -> NodeIndex {
        if self.look_ahead_is_async_function() {
            self.parse_async_function_declaration()
        } else if self.look_ahead_is_async_declaration() {
            let start_pos = self.token_pos();
            let async_start = self.token_pos();
            self.parse_expected(SyntaxKind::AsyncKeyword);
            let async_end = self.token_end();
            let async_modifier =
                self.arena
                    .add_token(SyntaxKind::AsyncKeyword as u16, async_start, async_end);
            let modifiers = Some(self.make_node_list(vec![async_modifier]));
            match self.token() {
                SyntaxKind::ClassKeyword => {
                    self.parse_class_declaration_with_modifiers(start_pos, modifiers)
                }
                SyntaxKind::EnumKeyword => {
                    self.parse_enum_declaration_with_modifiers(start_pos, modifiers)
                }
                SyntaxKind::InterfaceKeyword => {
                    self.parse_interface_declaration_with_modifiers(start_pos, modifiers)
                }
                SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::GlobalKeyword => {
                    if self.look_ahead_is_module_declaration() {
                        self.parse_module_declaration_with_modifiers(start_pos, modifiers)
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

    fn parse_statement_abstract_keyword(&mut self) -> NodeIndex {
        if self.next_token_is_on_new_line() {
            self.parse_expression_statement()
        } else if self.look_ahead_is_abstract_class() {
            self.parse_abstract_class_declaration()
        } else if self.look_ahead_is_abstract_declaration() {
            use tsz_common::diagnostics::diagnostic_codes;
            // TSC gives TS1242 specifically for 'abstract' before non-class declarations
            self.parse_error_at_current_token(
                "'abstract' modifier can only appear on a class, method, or property declaration.",
                diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION,
            );
            self.next_token();
            match self.token() {
                SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(),
                SyntaxKind::EnumKeyword => self.parse_enum_declaration(),
                SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::GlobalKeyword => {
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

    fn parse_statement_accessor_keyword(&mut self) -> NodeIndex {
        if self.look_ahead_is_accessor_declaration() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Modifiers cannot appear here.",
                diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
            );
            self.next_token();
            self.parse_statement()
        } else {
            self.parse_expression_statement()
        }
    }

    fn parse_statement_top_level_modifier(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        if self.next_token_is_on_new_line() {
            self.parse_expression_statement()
        } else if self.look_ahead_is_modifier_before_declaration() {
            if self.look_ahead_next_token_is_export_keyword() {
                // Modifier keyword followed by `export as namespace ...`:
                // TSC silently accepts the modifier and parses the export statement.
                // e.g., `static export as namespace Foo;` → no error.
                self.next_token();
                self.parse_statement()
            } else {
                // TS1044: '{0}' modifier cannot appear on a module or namespace element.
                let modifier_text = self.scanner.get_token_text();
                self.parse_error_at_current_token(
                    &format!(
                        "'{modifier_text}' modifier cannot appear on a module or namespace element."
                    ),
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
                );
                self.next_token();
                self.parse_statement()
            }
        } else if self.look_ahead_next_is_identifier_or_keyword_on_same_line() {
            self.parse_error_at_current_token(
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
            self.next_token();
            let downstream_start = self.token_pos();
            let preserve_downstream_expected = matches!(
                self.token(),
                SyntaxKind::BreakKeyword
                    | SyntaxKind::ContinueKeyword
                    | SyntaxKind::DoKeyword
                    | SyntaxKind::ForKeyword
                    | SyntaxKind::IfKeyword
                    | SyntaxKind::ReturnKeyword
                    | SyntaxKind::SwitchKeyword
                    | SyntaxKind::ThrowKeyword
                    | SyntaxKind::TryKeyword
                    | SyntaxKind::WhileKeyword
                    | SyntaxKind::WithKeyword
            );
            let diag_count = self.parse_diagnostics.len();
            let result = self.parse_statement();
            if !preserve_downstream_expected {
                let mut i = diag_count;
                while i < self.parse_diagnostics.len() {
                    if self.parse_diagnostics[i].code == diagnostic_codes::EXPECTED
                        && self.parse_diagnostics[i].start == downstream_start
                    {
                        self.parse_diagnostics.remove(i);
                    } else {
                        i += 1;
                    }
                }
            }
            result
        } else {
            self.parse_expression_statement()
        }
    }

    fn parse_statement_type_keyword(&mut self) -> NodeIndex {
        if self.look_ahead_is_type_alias_declaration() {
            self.parse_type_alias_declaration()
        } else {
            self.parse_expression_statement()
        }
    }

    fn parse_statement_declare_or_expression(&mut self) -> NodeIndex {
        // `declare` is a contextual keyword — it can be used as an identifier.
        // Only parse as ambient declaration if the next token is a valid declaration keyword.
        if self.look_ahead_is_declare_before_declaration() {
            self.parse_ambient_declaration()
        } else {
            self.parse_expression_statement()
        }
    }

    fn parse_statement_namespace_or_expression(&mut self) -> NodeIndex {
        if self.look_ahead_is_module_declaration() {
            self.parse_module_declaration()
        } else {
            self.parse_expression_statement()
        }
    }

    fn parse_statement_import_keyword(&mut self) -> NodeIndex {
        if self.look_ahead_is_import_call() {
            self.parse_expression_statement()
        } else if self.look_ahead_is_import_equals() {
            self.parse_import_equals_declaration()
        } else if self.look_ahead_is_import_declaration() {
            self.parse_import_declaration()
        } else {
            // `import` followed by a token that can't start any valid import form
            // (e.g., `import 10;`). tsc emits TS1128 "Declaration or statement expected"
            // at the `import` position. Emit the error, consume remaining tokens on the
            // line, and return an expression statement to avoid infinite recovery loops.
            let start_pos = self.token_pos();
            self.parse_error_at(
                start_pos,
                6, // length of "import"
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
            self.next_token(); // consume 'import'
            if self.is_token(SyntaxKind::CommaToken) {
                let end_pos = self.token_end();
                return self
                    .arena
                    .add_token(syntax_kind_ext::EMPTY_STATEMENT, start_pos, end_pos);
            }
            // Consume remaining tokens until statement boundary
            while !self.is_token(SyntaxKind::SemicolonToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
                && !self.scanner.has_preceding_line_break()
            {
                self.next_token();
            }
            if self.is_token(SyntaxKind::SemicolonToken) {
                self.next_token();
            }
            let end_pos = self.token_end();
            self.arena
                .add_token(syntax_kind_ext::EMPTY_STATEMENT, start_pos, end_pos)
        }
    }

    fn look_ahead_has_missing_decorator_expression(&mut self) -> bool {
        if !self.is_token(SyntaxKind::AtToken) {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token();
        let result = matches!(
            self.token(),
            SyntaxKind::AbstractKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::DefaultKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::ExportKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::ImportKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::TypeKeyword
                | SyntaxKind::UsingKeyword
                | SyntaxKind::VarKeyword
        );

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead to see if a modifier keyword (public, protected, private, static, etc.)
    /// is followed by a declaration keyword like class, interface, function, etc.
    /// Used to detect `public interface I {}` or `static class C {}` patterns at module level.
    pub(crate) fn look_ahead_is_modifier_before_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip the modifier keyword
        let is_decl = matches!(
            self.token(),
            SyntaxKind::ClassKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::VarKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::TypeKeyword
                | SyntaxKind::ExportKeyword
        );

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_decl
    }

    /// Check if `declare` is followed by a valid declaration keyword on the same line.
    /// Used to distinguish `declare class ...` (ambient declaration) from
    /// `declare instanceof C` (expression using `declare` as identifier).
    /// ASI prevents treating `declare\nclass ...` as an ambient declaration.
    fn look_ahead_is_declare_before_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token(); // skip `declare`
        let is_decl = if self.scanner.has_preceding_line_break() {
            false
        } else if self.is_token(SyntaxKind::ImportKeyword) {
            self.look_ahead_is_import_equals() || self.look_ahead_is_import_declaration()
        } else {
            matches!(
                self.token(),
                SyntaxKind::ClassKeyword
                    | SyntaxKind::InterfaceKeyword
                    | SyntaxKind::EnumKeyword
                    | SyntaxKind::NamespaceKeyword
                    | SyntaxKind::ModuleKeyword
                    | SyntaxKind::FunctionKeyword
                    | SyntaxKind::AbstractKeyword
                    | SyntaxKind::ConstKeyword
                    | SyntaxKind::VarKeyword
                    | SyntaxKind::LetKeyword
                    | SyntaxKind::TypeKeyword
                    | SyntaxKind::GlobalKeyword
                    | SyntaxKind::AsyncKeyword
                    | SyntaxKind::UsingKeyword
                    | SyntaxKind::AwaitKeyword
                    | SyntaxKind::ExportKeyword
            )
        };
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_decl
    }

    /// Check if the next token is an identifier or keyword on the same line.
    /// Matches tsc's `nextTokenIsIdentifierOrKeywordOnSameLine`.
    /// Used by `isStartOfStatement()` for modifier keywords (static, public, etc.)
    /// to distinguish class-member-like context from standalone expressions.
    pub(super) fn look_ahead_next_is_identifier_or_keyword_on_same_line(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token(); // skip the modifier keyword
        let result = !self.scanner.has_preceding_line_break() && self.is_identifier_or_keyword();
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Check if the next token is `{` on the same line.
    /// Used to detect `interface { }` where the interface name is missing.
    pub(super) fn look_ahead_next_is_open_brace_on_same_line(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let result =
            !self.scanner.has_preceding_line_break() && self.is_token(SyntaxKind::OpenBraceToken);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Check if the next token is on a new line (ASI applies).
    /// Used to detect cases like:
    ///   abstract
    ///   class C {}
    /// where ASI should terminate `abstract` as an expression statement.
    fn next_token_is_on_new_line(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        self.scanner.scan();
        let has_line_break = self.scanner.has_preceding_line_break();
        self.scanner.restore_state(snapshot);
        has_line_break
    }

    /// Look ahead to see if the next token is `export` on the same line.
    /// Used to distinguish `static export as namespace ...` (modifier as expression + export statement)
    /// from `static class ...` (modifier before declaration).
    fn look_ahead_next_token_is_export_keyword(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let result =
            !self.scanner.has_preceding_line_break() && self.token() == SyntaxKind::ExportKeyword;
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead to see if we have "async function"
    pub(crate) fn look_ahead_is_async_function(&mut self) -> bool {
        look_ahead_is(&mut self.scanner, self.current_token, |token| {
            token == SyntaxKind::FunctionKeyword
        })
    }

    /// Look ahead to see if "async" is followed by a declaration keyword.
    pub(crate) fn look_ahead_is_async_declaration(&mut self) -> bool {
        look_ahead_is_async_declaration(&mut self.scanner, self.current_token)
    }

    /// Look ahead to see if we have "abstract class"
    pub(crate) fn look_ahead_is_abstract_class(&mut self) -> bool {
        look_ahead_is(&mut self.scanner, self.current_token, |token| {
            token == SyntaxKind::ClassKeyword
        })
    }

    /// Look ahead to see if "abstract" is followed by another declaration keyword.
    pub(crate) fn look_ahead_is_abstract_declaration(&mut self) -> bool {
        look_ahead_is_abstract_declaration(&mut self.scanner, self.current_token)
    }

    /// Look ahead to see if "accessor" is followed by a declaration keyword.
    pub(crate) fn look_ahead_is_accessor_declaration(&mut self) -> bool {
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

    /// Look ahead to see if `let` starts a variable declaration.
    /// In tsc, `let` is only treated as a declaration keyword when followed by
    /// an identifier, `{` (object destructuring), or `[` (array destructuring).
    /// Otherwise (e.g. `let;`), `let` is treated as an identifier expression.
    pub(crate) fn look_ahead_is_let_declaration(&mut self) -> bool {
        look_ahead_is(&mut self.scanner, self.current_token, |token| {
            is_identifier_or_keyword(token)
                || token == SyntaxKind::OpenBraceToken
                || token == SyntaxKind::OpenBracketToken
        })
    }

    /// Look ahead to see if we have "await using"
    pub(crate) fn look_ahead_is_using_declaration(&mut self) -> bool {
        look_ahead_is(&mut self.scanner, self.current_token, |token| {
            is_identifier_or_keyword(token) || token == SyntaxKind::OpenBraceToken
        })
    }

    /// Look ahead for `using` in a for-statement initializer position.
    /// In `for (using of ...)`, `of` is the for-of keyword, not a binding name,
    /// so `using` should be parsed as an identifier expression.
    /// Excludes `of` and `in` as the next token since they indicate for-of/for-in.
    pub(crate) fn look_ahead_is_using_declaration_in_for(&mut self) -> bool {
        look_ahead_is(&mut self.scanner, self.current_token, |token| {
            // `of` and `in` after `using` mean this is `for (using of/in ...)`,
            // not a using declaration.
            if token == SyntaxKind::OfKeyword || token == SyntaxKind::InKeyword {
                return false;
            }
            is_identifier_or_keyword(token) || token == SyntaxKind::OpenBraceToken
        })
    }

    pub(crate) fn look_ahead_is_await_using_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let t1 = self.scanner.scan();
        let t2 = self.scanner.scan();
        let result = t1 == SyntaxKind::UsingKeyword
            && (is_identifier_or_keyword(t2) || t2 == SyntaxKind::OpenBraceToken);
        self.scanner.restore_state(snapshot);
        result
    }

    /// Look ahead for `await using` in a for-statement initializer position.
    /// In `for (await using of ...)`, `of` is the for-of keyword, not a binding name.
    /// Scans past `using` and checks the following token excludes `of`/`in`.
    pub(crate) fn look_ahead_is_await_using_declaration_in_for(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let t1 = self.scanner.scan(); // should be `using`
        let t2 = self.scanner.scan(); // binding name or `of`/`in`
        let result = t1 == SyntaxKind::UsingKeyword
            && t2 != SyntaxKind::OfKeyword
            && t2 != SyntaxKind::InKeyword
            && (is_identifier_or_keyword(t2) || t2 == SyntaxKind::OpenBraceToken);
        self.scanner.restore_state(snapshot);
        result
    }

    #[allow(dead_code)]
    pub(crate) fn look_ahead_is_await_using(&mut self) -> bool {
        look_ahead_is(&mut self.scanner, self.current_token, |token| {
            token == SyntaxKind::UsingKeyword
        })
    }

    /// Look ahead to see if we have "import identifier ="
    pub(crate) fn look_ahead_is_import_equals(&mut self) -> bool {
        look_ahead_is_import_equals(
            &mut self.scanner,
            self.current_token,
            is_identifier_or_contextual_keyword,
        )
    }

    /// Look ahead to check if the current identifier is directly followed by `=`.
    /// Used to disambiguate `import type X =` (where `type` is import name)
    /// from `import type X = require(...)` (where `type` is modifier).
    fn look_ahead_is_equals_after_identifier(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        // Skip current token (the identifier)
        self.next_token();
        let result = self.is_token(SyntaxKind::EqualsToken);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead to see if we have "import (" (dynamic import call)
    pub(crate) fn look_ahead_is_import_call(&mut self) -> bool {
        look_ahead_is_import_call(&mut self.scanner, self.current_token)
    }

    /// Look ahead to see if `import` is starting a declaration rather than an expression.
    /// Valid starts are:
    /// - string literal: `import "mod";`
    /// - identifier/keyword: default import or contextual modifier/name
    /// - `{` / `*`: named or namespace imports
    fn look_ahead_is_import_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token(); // skip `import`
        let result = matches!(
            self.token(),
            SyntaxKind::StringLiteral
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::AsteriskToken
                | SyntaxKind::TypeKeyword
                | SyntaxKind::DeferKeyword
        ) || self.is_identifier_or_keyword();
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead to see if we have `export =`.
    #[allow(dead_code)]
    fn look_ahead_is_export_assignment(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token(); // skip `export`
        let result = self.is_token(SyntaxKind::EqualsToken);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead to see if "namespace"/"module" starts a declaration.
    /// Updated to recognize anonymous modules: module { ... }
    pub(crate) fn look_ahead_is_module_declaration(&mut self) -> bool {
        look_ahead_is_module_declaration(&mut self.scanner, self.current_token)
    }

    /// Look ahead to see if "type" starts a type alias declaration.
    pub(crate) fn look_ahead_is_type_alias_declaration(&mut self) -> bool {
        look_ahead_is_type_alias_declaration(&mut self.scanner, self.current_token)
    }

    /// Look ahead to see if we have "identifier :" (labeled statement)
    pub(crate) fn look_ahead_is_labeled_statement(&mut self) -> bool {
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
    pub(crate) fn look_ahead_is_const_enum(&mut self) -> bool {
        look_ahead_is_const_enum(&mut self.scanner, self.current_token)
    }

    /// Parse const enum declaration
    pub(crate) fn parse_const_enum_declaration(
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
    pub(crate) fn parse_labeled_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the label (identifier)
        let label = self.parse_identifier_name();

        // Note: tsc does NOT emit TS1003 for `await` used as a label in static
        // blocks or async contexts. Instead, it treats `await` as a keyword and
        // parses it as an expression, emitting TS1109 when `:<statement>` follows.
        // We parse it as a labeled statement but skip the TS1003 error to match tsc.

        // Check for duplicate labels (TS1114) and record this label
        let label_name = if let Some(label_node) = self.arena.get(label) {
            if let Some(ident) = self.arena.get_identifier_at(label) {
                let escaped_text = ident.escaped_text.clone();
                let pos = label_node.pos;
                self.check_duplicate_label(&escaped_text, pos);
                Some(escaped_text)
            } else {
                None
            }
        } else {
            None
        };

        // Consume the colon
        self.parse_expected(SyntaxKind::ColonToken);

        // Parse the statement
        let statement = self.parse_statement();

        // Remove the label from the current scope (labels are statement-scoped)
        // This allows sequential labels with the same name: target: stmt1; target: stmt2;
        if let Some(label_name) = label_name
            && let Some(current_scope) = self.label_scopes.last_mut()
        {
            current_scope.remove(&label_name);
        }

        let end_pos = self.token_end();

        self.arena.add_labeled(
            syntax_kind_ext::LABELED_STATEMENT,
            start_pos,
            end_pos,
            LabeledData { label, statement },
        )
    }

    /// Parse import equals declaration: import X = require("...") or import X = Y.Z
    pub(crate) fn parse_import_equals_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_import_equals_declaration_with_modifiers(start_pos, None)
    }

    pub(crate) fn parse_import_equals_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::ImportKeyword);

        // Check for type modifier: `import type X = require(...)`
        let is_type_only = if self.is_token(SyntaxKind::TypeKeyword)
            && !self.look_ahead_is_equals_after_identifier()
        {
            self.next_token();
            true
        } else {
            false
        };
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
                modifiers,
                is_type_only,
                import_clause: name,
                module_specifier: module_reference,
                attributes: NodeIndex::NONE,
            },
        )
    }

    /// Parse external module reference: require("...")
    pub(crate) fn parse_external_module_reference(&mut self) -> NodeIndex {
        self.parse_expected(SyntaxKind::RequireKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);
        let expression = self.parse_string_literal();
        // If parse_string_literal failed (non-string token), skip past the invalid token
        // so we can find the closing paren and avoid cascading errors (e.g. TS1128).
        if expression == NodeIndex::NONE
            && !self.is_token(SyntaxKind::CloseParenToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            self.next_token();
        }
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Return the string literal as the module reference
        expression
    }

    /// Parse entity name: A or A.B.C or this or this.x
    pub(crate) fn parse_entity_name(&mut self) -> NodeIndex {
        self.parse_entity_name_inner(false)
    }

    pub(crate) fn parse_entity_name_allow_reserved(&mut self) -> NodeIndex {
        self.parse_entity_name_inner(true)
    }

    fn parse_entity_name_inner(&mut self, allow_reserved_words: bool) -> NodeIndex {
        // Handle 'this' keyword as a valid start for typeof expressions
        let mut left = if self.is_token(SyntaxKind::ThisKeyword) {
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            self.next_token();
            self.arena
                .add_token(SyntaxKind::ThisKeyword as u16, start_pos, end_pos)
        } else if allow_reserved_words {
            self.parse_identifier_name()
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
            let end_pos = self.token_full_start();

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
    pub(crate) fn parse_async_function_declaration(&mut self) -> NodeIndex {
        // TS1040: 'async' modifier cannot be used in an ambient context
        if (self.context_flags & crate::parser::state::CONTEXT_FLAG_AMBIENT) != 0 {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "'async' modifier cannot be used in an ambient context.",
                diagnostic_codes::MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
            );
        }
        self.parse_expected(SyntaxKind::AsyncKeyword);
        self.parse_function_declaration_with_async(true, None)
    }

    /// Parse a block statement
    pub(crate) fn parse_block(&mut self) -> NodeIndex {
        // Check recursion limit to prevent stack overflow on deeply nested code
        if !self.enter_recursion() {
            return NodeIndex::NONE;
        }

        let start_pos = self.token_pos();
        let statements = if self.parse_expected(SyntaxKind::OpenBraceToken) {
            // Set IN_BLOCK flag so that modifiers like export/declare emit TS1184
            let saved_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_IN_BLOCK;

            let stmts = self.parse_statements();

            self.context_flags = saved_flags;
            self.parse_expected(SyntaxKind::CloseBraceToken);
            stmts
        } else {
            self.make_node_list(Vec::new())
        };
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
    pub(crate) fn parse_empty_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        self.parse_expected(SyntaxKind::SemicolonToken);
        let end_pos = self.token_end();

        self.arena
            .add_token(syntax_kind_ext::EMPTY_STATEMENT, start_pos, end_pos)
    }

    fn try_parse_invalid_let_array_declaration_statement(&mut self) -> Option<NodeIndex> {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        let next = self.scanner.scan();
        let first_elem = if next == SyntaxKind::OpenBracketToken {
            self.scanner.scan()
        } else {
            SyntaxKind::Unknown
        };

        self.scanner.restore_state(snapshot);
        self.current_token = current;

        let invalid_first_array_binding_element = next == SyntaxKind::OpenBracketToken
            && !matches!(
                first_elem,
                SyntaxKind::CloseBracketToken
                    | SyntaxKind::CommaToken
                    | SyntaxKind::DotDotDotToken
                    | SyntaxKind::OpenBraceToken
                    | SyntaxKind::OpenBracketToken
            )
            && !is_identifier_or_keyword(first_elem);

        if !invalid_first_array_binding_element {
            return None;
        }

        let start_pos = self.token_pos();
        self.consume_keyword(); // let
        self.parse_expected(SyntaxKind::OpenBracketToken);
        self.error_array_element_destructuring_pattern_expected();

        if !matches!(
            self.token(),
            SyntaxKind::CloseBracketToken | SyntaxKind::SemicolonToken | SyntaxKind::EndOfFileToken
        ) {
            self.next_token();
        }

        if self.is_token(SyntaxKind::CloseBracketToken) {
            self.parse_error_at_current_token(
                "';' expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPECTED,
            );
            self.next_token();
        }

        if self.is_token(SyntaxKind::EqualsToken) {
            self.parse_error_at_current_token(
                "Declaration or statement expected.",
                tsz_common::diagnostics::diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
            self.next_token();
            while !matches!(
                self.token(),
                SyntaxKind::SemicolonToken | SyntaxKind::EndOfFileToken
            ) {
                self.next_token();
            }
        }

        if self.is_token(SyntaxKind::SemicolonToken) {
            self.next_token();
        }

        let end_pos = self.token_end();
        Some(
            self.arena
                .add_token(syntax_kind_ext::EMPTY_STATEMENT, start_pos, end_pos),
        )
    }

    /// Parse variable statement (var/let/const)
    pub(crate) fn parse_variable_statement(&mut self) -> NodeIndex {
        self.parse_variable_statement_with_modifiers(None, None)
    }

    /// Parse variable statement with optional start position and modifiers (for declare statements)
    pub(crate) fn parse_variable_statement_with_modifiers(
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
    pub(crate) fn parse_variable_declaration_list(&mut self) -> NodeIndex {
        use crate::parser::node_flags;

        let start_pos = self.token_pos();

        // Consume var/let/const/using/await using and get flags
        // Use consume_keyword() for TS1260 check (keywords cannot contain escape characters)
        let flags: u16 = match self.token() {
            SyntaxKind::LetKeyword => {
                self.consume_keyword();
                self.u16_from_node_flags(node_flags::LET)
            }
            SyntaxKind::ConstKeyword => {
                self.consume_keyword();
                self.u16_from_node_flags(node_flags::CONST)
            }
            SyntaxKind::UsingKeyword => {
                self.consume_keyword();
                self.u16_from_node_flags(node_flags::USING)
            }
            SyntaxKind::AwaitKeyword => {
                // await using declaration
                self.consume_keyword(); // consume 'await'
                self.parse_expected(SyntaxKind::UsingKeyword); // consume 'using'
                self.u16_from_node_flags(node_flags::AWAIT_USING)
            }
            _ => {
                self.consume_keyword(); // var
                0
            }
        };

        // Parse declarations with enhanced error recovery
        let mut declarations = Vec::new();
        let mut had_decl_expected_error = false;
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
                    && !self.is_token(SyntaxKind::Unknown)
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Variable declaration expected.",
                        diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                    );
                    had_decl_expected_error = true;
                }
                break;
            }

            let diag_count_before_decl = self.parse_diagnostics.len();
            let decl = self.parse_variable_declaration_with_flags(flags);
            let decl_had_error = self.parse_diagnostics.len() > diag_count_before_decl;
            declarations.push(decl);

            let comma_pos = self.token_pos();
            if !self.parse_optional(SyntaxKind::CommaToken) {
                // If ASI applies (line break, closing brace, EOF, or semicolon),
                // just break - parse_semicolon() in the caller will handle it
                if self.can_parse_semicolon() {
                    break;
                }

                if self.is_token(SyntaxKind::ColonToken) {
                    use tsz_common::diagnostics::diagnostic_codes;

                    let use_failed_async_arrow_recovery =
                        self.pending_failed_async_arrow_colon_recovery;
                    self.pending_failed_async_arrow_colon_recovery = false;

                    self.error_comma_expected();
                    self.next_token();

                    if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                        self.parse_error_at_current_token(
                            "';' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        break;
                    }

                    let generic_like_type_arg_pos =
                        if use_failed_async_arrow_recovery && self.is_identifier_or_keyword() {
                            let snapshot = self.scanner.save_state();
                            let current = self.current_token;
                            self.next_token();
                            let result = self
                                .is_token(SyntaxKind::LessThanToken)
                                .then(|| self.token_pos());
                            self.scanner.restore_state(snapshot);
                            self.current_token = current;
                            result
                        } else {
                            None
                        };

                    let recover_start = self.token_pos();
                    let _ = self.parse_type();
                    if self.token_pos() == recover_start
                        && !matches!(
                            self.token(),
                            SyntaxKind::CommaToken
                                | SyntaxKind::SemicolonToken
                                | SyntaxKind::CloseBraceToken
                                | SyntaxKind::EndOfFileToken
                        )
                    {
                        self.next_token();
                    }

                    if let Some(pos) = generic_like_type_arg_pos {
                        self.parse_error_at(pos, 1, "',' expected.", diagnostic_codes::EXPECTED);
                    }

                    if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                        if use_failed_async_arrow_recovery {
                            self.error_expression_expected();
                        } else {
                            self.parse_error_at_current_token(
                                "';' expected.",
                                diagnostic_codes::EXPECTED,
                            );
                        }
                    }
                    break;
                }

                // `=>` after a declaration is never a valid comma separator.
                // Break silently so parse_semicolon() in the caller can emit
                // "';' expected." at the `=` position, matching tsc's diagnostic.
                // Example: `var tt = (a, (b, c)) => ...` — rejected arrow function.
                if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                    break;
                }

                // When the variable name itself was erroneous (e.g., TS1389 for a
                // reserved word like `const export`), suppress the follow-up TS1005
                // comma error to match tsc's recovery behavior.
                if decl_had_error {
                    break;
                }

                // `var x = 2.toString();` leaves `toString` in the token stream after the
                // scanner reports TS1351 on the identifier. tsc recovers by treating that
                // identifier as the malformed start of a second declaration, which shifts
                // the follow-up diagnostics onto the call tail: TS1005 at `(` and TS1109
                // at `)`. Mirror that recovery shape here instead of emitting a stray
                // comma error at the identifier itself.
                if self.current_token_has_numeric_literal_follow_error() {
                    self.next_token();

                    if self.is_token(SyntaxKind::OpenParenToken) {
                        self.error_comma_expected();
                        self.next_token();

                        if self.is_token(SyntaxKind::CloseParenToken) {
                            let saved_error_pos = self.last_error_pos;
                            self.last_error_pos = 0;
                            self.error_expression_expected();
                            if self.last_error_pos == 0 {
                                self.last_error_pos = saved_error_pos;
                            }
                            self.next_token();
                        }
                    }
                    break;
                }

                // If the unexpected token can start a new variable declaration
                // (identifier/keyword, { or [) AND is not a reserved word, treat
                // the missing comma as the only error and let the loop continue to
                // parse the token as the next declarator.
                // Example: `const a number = "missing colon";`
                //   tsc treats this as `const a, number = "missing colon";`
                //   and emits only one TS1005 at `number`.
                {
                    let can_continue = (self.is_identifier_or_keyword()
                        && !self.is_reserved_word())
                        || self.is_token(SyntaxKind::OpenBraceToken)
                        || self.is_token(SyntaxKind::OpenBracketToken);
                    if can_continue {
                        // Emit ',' expected directly, bypassing the distance-based
                        // suppression heuristic. tsc's parseDelimitedList always
                        // emits TS1005 here (it only deduplicates at the exact same
                        // position). Without force-emit, two adjacent short
                        // identifiers (e.g. `var y: z is number;`) can fall within
                        // the suppression window and lose the second error.
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        continue;
                    }
                }

                // No ASI - emit ',' expected for the unexpected token and stop.
                self.error_comma_expected();

                // Otherwise stop the list. We break instead of continuing to avoid
                // cascading TS1134 errors when the recovery eats into what tsc
                // treats as a separate statement.
                // Example: `var b = new C0 32, '';` - tsc emits only TS1005 at `32`.
                // Only consume the unexpected token if it cannot start a new
                // statement.  Tokens like `delete`, `typeof`, `void`, `~` etc.
                // can begin an expression statement and must be preserved so the
                // subsequent statement-parsing loop can emit them.
                // Example: `var a = q~;` → tsc emits `var a = q;\n~;`
                if !self.is_statement_start() {
                    let unexpected_token = self.token();
                    // When a `.` separates what looks like two declarations
                    // (e.g., `const x: "".typeof(...)`), tsc treats the `.` as
                    // a missing `,` and continues the declaration list. When the
                    // next token is a keyword (e.g., `typeof`), tsc's list-parse
                    // error recovery emits TS1389 "not allowed as a variable
                    // declaration name". Emit the same diagnostic here, bypassing
                    // `error_reserved_word_in_variable_declaration` which would be
                    // suppressed by `should_report_error` proximity heuristic.
                    let was_dot = unexpected_token == SyntaxKind::DotToken;
                    self.next_token();
                    if matches!(
                        unexpected_token,
                        SyntaxKind::CloseBracketToken | SyntaxKind::CloseParenToken
                    ) && matches!(
                        self.token(),
                        SyntaxKind::SlashToken | SyntaxKind::SlashEqualsToken
                    ) {
                        // Keep malformed tails like `var v = /[]/]/` inside the
                        // declaration-list recovery so the trailing slash becomes
                        // TS1134 instead of a fresh unterminated regex statement.
                        self.parse_error_at_current_token(
                            "Variable declaration expected.",
                            diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                        );
                        self.next_token();
                    }
                    if was_dot && token_is_keyword(self.token()) {
                        use tsz_common::diagnostics::diagnostic_messages;
                        let keyword = self.token();
                        let word = self.current_keyword_text();
                        let msg =
                            diagnostic_messages::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME
                                .replace("{0}", word);
                        self.parse_error_at_current_token(
                            &msg,
                            diagnostic_codes::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME,
                        );
                        self.recover_reserved_word_variable_declaration_tail(keyword);
                    } else if was_dot && self.is_identifier_or_keyword() && !self.is_reserved_word()
                    {
                        // `declare const x: "foo".charCodeAt(0);` is recovered by tsc as if
                        // `charCodeAt` started a second declarator. Mirror that by surfacing
                        // the follow-up TS1005 at `(` and then skipping the call tail.
                        self.next_token();
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            self.parse_error_at_current_token(
                                "',' expected.",
                                diagnostic_codes::EXPECTED,
                            );
                            let mut paren_depth = 0u32;
                            while !matches!(
                                self.token(),
                                SyntaxKind::SemicolonToken
                                    | SyntaxKind::CloseBraceToken
                                    | SyntaxKind::EndOfFileToken
                            ) && !self.scanner.has_preceding_line_break()
                            {
                                match self.token() {
                                    SyntaxKind::OpenParenToken => paren_depth += 1,
                                    SyntaxKind::CloseParenToken => {
                                        if paren_depth == 0 {
                                            break;
                                        }
                                        paren_depth -= 1;
                                    }
                                    _ => {}
                                }
                                self.next_token();
                                if paren_depth == 0 {
                                    break;
                                }
                            }
                        }
                    }
                }
                break;
            }

            // After comma, check if next token can start another declaration.
            // Handle cases like: let x, , y (missing declaration between commas).
            // Reserved words (return, if, while, etc.) cannot be binding identifiers,
            // so `var a, return;` should be a trailing comma error, not a new declaration.
            let can_start_next = (self.is_identifier_or_keyword() && !self.is_reserved_word())
                || self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::OpenBracketToken);

            if !can_start_next {
                // Trailing comma in variable declaration list — emit TS1009.
                // This covers `var a,;`, `var a,}`, `var a,` (EOF), and
                // `var a,\nreturn;` (reserved word after comma = trailing comma).
                use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                if self.is_token(SyntaxKind::SemicolonToken)
                    || self.is_token(SyntaxKind::CloseBraceToken)
                    || self.is_token(SyntaxKind::EndOfFileToken)
                    || self.is_reserved_word()
                {
                    self.parse_error_at(
                        comma_pos,
                        1,
                        diagnostic_messages::TRAILING_COMMA_NOT_ALLOWED,
                        diagnostic_codes::TRAILING_COMMA_NOT_ALLOWED,
                    );
                } else {
                    self.parse_error_at_current_token(
                        "Variable declaration expected.",
                        diagnostic_codes::VARIABLE_DECLARATION_EXPECTED,
                    );
                }
                break;
            }
        }

        // Check for empty declaration list: var ;
        // TSC emits TS1123 "Variable declaration list cannot be empty"
        // Skip when TS1134 was already emitted (e.g., `using 1` — TSC only emits TS1134)
        if declarations.is_empty()
            && !had_decl_expected_error
            && !self.is_token(SyntaxKind::Unknown)
        {
            use tsz_common::diagnostics::diagnostic_codes;
            let pos = self.token_full_start();
            self.parse_error_at(
                pos,
                0,
                "Variable declaration list cannot be empty.",
                diagnostic_codes::VARIABLE_DECLARATION_LIST_CANNOT_BE_EMPTY,
            );
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

    /// Parse variable declaration with declaration flags (for using/await using checks)
    /// Flags: bits 0-2 used for LET/CONST/USING, bit 3 for catch-clause binding (suppresses TS1182)
    pub(crate) fn parse_variable_declaration_with_flags(&mut self, flags: u16) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_variable_declaration_with_flags_pre_checks(flags);

        let name = self.parse_variable_declaration_name();
        let exclamation_token = self.parse_optional(SyntaxKind::ExclamationToken);
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };
        let initializer = self.parse_variable_declaration_initializer();
        self.parse_variable_declaration_after_parse_checks(flags, start_pos, name, initializer);

        let end_pos =
            self.parse_variable_declaration_end_pos(start_pos, type_annotation, name, initializer);

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

    fn parse_variable_declaration_with_flags_pre_checks(&mut self, flags: u16) {
        use crate::parser::node_flags;
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Check if this is a 'using' or 'await using' declaration.
        // Only check the USING bit (bit 2). AWAIT_USING = CONST | USING = 6,
        // so checking USING bit matches both USING (4) and AWAIT_USING (6)
        // but NOT CONST (2) which only has bit 1 set.
        let is_using = (flags & self.u16_from_node_flags(node_flags::USING)) != 0;

        // TS1492: 'using'/'await using' declarations may not have binding patterns
        if is_using
            && (self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::OpenBracketToken))
        {
            let is_await_using = (flags & self.u16_from_node_flags(node_flags::AWAIT_USING))
                == self.u16_from_node_flags(node_flags::AWAIT_USING);
            let decl_kind = if is_await_using {
                "await using"
            } else {
                "using"
            };
            let msg = diagnostic_messages::DECLARATIONS_MAY_NOT_HAVE_BINDING_PATTERNS
                .replace("{0}", decl_kind);
            self.parse_error_at_current_token(
                &msg,
                diagnostic_codes::DECLARATIONS_MAY_NOT_HAVE_BINDING_PATTERNS,
            );
        }

        // Parse name - can be identifier, keyword as identifier, or binding pattern
        // Check for illegal binding identifiers (e.g., 'await' in static blocks)
        self.check_illegal_binding_identifier();
        // TS18029: Check for private identifiers in variable declarations (check before parsing)
        if self.is_token(SyntaxKind::PrivateIdentifier) {
            let start = self.token_pos();
            let length = self.token_end() - start;
            self.parse_error_at(
                start,
                length,
                "Private identifiers are not allowed in variable declarations.",
                diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_IN_VARIABLE_DECLARATIONS,
            );
        }
    }

    fn parse_variable_declaration_name(&mut self) -> NodeIndex {
        if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_object_binding_pattern()
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            self.parse_array_binding_pattern()
        } else if self.is_reserved_word() {
            // TS1389: '{0}' is not allowed as a variable declaration name.
            // tsc emits this specific error instead of the generic TS1359 when a reserved
            // word appears as a variable declaration binding name (var/let/const/using).
            self.error_reserved_word_in_variable_declaration();
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                start_pos,
                end_pos,
                IdentifierData {
                    atom: Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            )
        } else {
            self.parse_identifier()
        }
    }

    fn parse_variable_declaration_initializer(&mut self) -> NodeIndex {
        if !self.parse_optional(SyntaxKind::EqualsToken) {
            return NodeIndex::NONE;
        }

        if self.is_token(SyntaxKind::ConstKeyword)
            || self.is_token(SyntaxKind::LetKeyword)
            || self.is_token(SyntaxKind::VarKeyword)
        {
            self.error_expression_expected();
            return NodeIndex::NONE;
        }

        let expr = self.parse_assignment_expression();
        if expr.is_none() {
            self.error_expression_expected();
        }
        expr
    }

    fn parse_variable_declaration_after_parse_checks(
        &mut self,
        flags: u16,
        start_pos: u32,
        name: NodeIndex,
        initializer: NodeIndex,
    ) {
        use tsz_common::diagnostics::diagnostic_codes;

        // TS1182: A destructuring declaration must have an initializer
        // Skip for catch clause bindings (flags bit 3 = CATCH_CLAUSE_BINDING)
        // and for-in/for-of loop variables, which are destructuring without initializers.
        let is_catch_clause = (flags & 0x8) != 0;
        if is_catch_clause && initializer.is_some() {
            let (pos, len) = self
                .arena
                .get(initializer)
                .map_or((start_pos, 0), |n| (n.pos, n.end - n.pos));
            self.parse_error_at(
                pos,
                len,
                "Catch clause variable cannot have an initializer.",
                diagnostic_codes::CATCH_CLAUSE_VARIABLE_CANNOT_HAVE_AN_INITIALIZER,
            );
        }
        if !is_catch_clause
            && initializer.is_none()
            && (self.context_flags & crate::parser::state::CONTEXT_FLAG_AMBIENT) == 0
            && let Some(name_node) = self.arena.get(name)
            && name_node.is_binding_pattern()
        {
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "A destructuring declaration must have an initializer.",
                diagnostic_codes::A_DESTRUCTURING_DECLARATION_MUST_HAVE_AN_INITIALIZER,
            );
        }
        if name == NodeIndex::NONE {
            self.parse_error_at_current_token(
                "Identifier expected.",
                diagnostic_codes::IDENTIFIER_EXPECTED,
            );
        }
    }

    fn parse_variable_declaration_end_pos(
        &mut self,
        start_pos: u32,
        type_annotation: NodeIndex,
        name: NodeIndex,
        initializer: NodeIndex,
    ) -> u32 {
        let mut end_pos = self.token_end();
        // Calculate end position from the last component present (child node, not token)
        if initializer.is_some() {
            self.arena
                .get(initializer)
                .map_or_else(|| self.token_pos(), |n| n.end)
        } else if type_annotation.is_some() {
            self.arena
                .get(type_annotation)
                .map_or_else(|| self.token_pos(), |n| n.end)
        } else {
            self.arena
                .get(name)
                .map_or_else(|| self.token_pos(), |n| n.end)
        };
        end_pos = end_pos.max(self.token_end()).max(start_pos);
        end_pos
    }

    /// Parse function declaration (optionally async)
    pub(crate) fn parse_function_declaration(&mut self) -> NodeIndex {
        tracing::trace!(pos = self.token_pos(), "parse_function_declaration");
        self.parse_function_declaration_with_async(false, None)
    }

    /// Parse function declaration with async modifier already consumed
    pub(crate) fn parse_function_declaration_with_async(
        &mut self,
        is_async: bool,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        let start_pos = self.token_pos();

        // Check for async modifier if not already parsed
        // TS1040: 'async' modifier cannot be used in an ambient context
        let _async_token_pos = self.token_pos();
        let is_async = if !is_async && self.is_token(SyntaxKind::AsyncKeyword) {
            if (self.context_flags & crate::parser::state::CONTEXT_FLAG_AMBIENT) != 0 {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "'async' modifier cannot be used in an ambient context.",
                    diagnostic_codes::MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                );
            }
            self.next_token(); // consume async
            true
        } else {
            is_async
        };

        self.parse_expected(SyntaxKind::FunctionKeyword);

        // Check for generator asterisk
        let asterisk_token = self.parse_optional(SyntaxKind::AsteriskToken);

        // Set context flags BEFORE parsing name and parameters so that
        // reserved keywords (await/yield) are properly detected in function declarations
        // For async function * await() {}, the function name 'await' should error
        // For async function * (await) {}, the parameter name 'await' should error
        let is_async_generator_declaration = is_async && asterisk_token;
        let saved_flags = self.context_flags;
        // Clear async/generator for name parsing (names aren't subject to these restrictions),
        // but keep STATIC_BLOCK set — function names are declarations in the outer scope,
        // so `function await()` inside a static block is still illegal.
        self.context_flags &= !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR);
        if is_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }
        if asterisk_token {
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
        }

        // Parse name - keywords like 'abstract' can be used as function names
        // Note: function names are NOT subject to async/generator context restrictions
        // because the name is a declaration in the outer scope, not a binding in the
        // function body. `async function * await() {}` and `function * yield() {}` are valid.
        // Only check for static block context (where await is always illegal as an identifier)
        if self.in_static_block_context() && self.is_token(SyntaxKind::AwaitKeyword) {
            self.parse_error_at_current_token(
                "Identifier expected. 'await' is a reserved word that cannot be used here.",
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
            );
        }

        // Async and generator function declarations are valid with `await`/`yield` in their
        // own names, but nested function declarations in those contexts are not.
        if !is_async_generator_declaration && self.in_generator_context()
            || (self.in_async_context() && self.is_token(SyntaxKind::AwaitKeyword)) && !is_async
        {
            use tsz_common::diagnostics::diagnostic_codes;
            if self.is_token(SyntaxKind::AwaitKeyword) {
                self.parse_error_at_current_token(
                    "Identifier expected. 'await' is a reserved word that cannot be used here.",
                    diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
                );
            } else if self.is_token(SyntaxKind::YieldKeyword) {
                self.report_yield_reserved_word_error();
            }
        }

        // For async generator declarations, `yield` is valid as the function name
        // (it binds in the outer scope, not the generator body)
        let is_yield_as_generator_name =
            is_async_generator_declaration && self.is_token(SyntaxKind::YieldKeyword);
        let name = if self.is_reserved_word() && !is_yield_as_generator_name {
            use tsz_common::diagnostics::diagnostic_codes;

            let name_start = self.token_pos();
            let name_end = self.token_end();
            let atom = self.scanner.get_token_atom();
            let text = self.scanner.get_token_value_ref().to_string();
            self.error_reserved_word_identifier();
            if self.is_token(SyntaxKind::OpenParenToken) {
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::IDENTIFIER_EXPECTED,
                    diagnostic_codes::IDENTIFIER_EXPECTED,
                );
            }
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                name_start,
                name_end,
                IdentifierData {
                    atom,
                    escaped_text: text,
                    original_text: None,
                    type_arguments: None,
                },
            )
        } else if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            self.parse_identifier()
        };

        // Parse optional type parameters: <T, U extends V>
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Clear STATIC_BLOCK before parsing parameters — function parameters create a
        // new scope where 'await' is a valid identifier (unless the function is async).
        // The function name was already parsed above in the static block context.
        self.context_flags &= !CONTEXT_FLAG_STATIC_BLOCK;

        // Parse parameters. If `(` is missing and we're already at `{`, recover
        // straight into the body instead of parsing the body as a destructuring
        // parameter list.
        let has_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if !has_open_paren && self.is_token(SyntaxKind::OpenBraceToken) {
            NodeList::new()
        } else {
            let params = self.parse_parameter_list();
            self.parse_expected(SyntaxKind::CloseParenToken);
            params
        };

        // Parse optional return type (may be a type predicate: param is T)
        // Note: Type annotations are not in async/generator context
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };
        // Push a new label scope for the function body
        self.push_label_scope();
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
            // TS1144: '{' or ';' expected — user wrote arrow syntax on a function declaration
            self.parse_error_at_current_token(
                "'{' or ';' expected.",
                diagnostic_codes::OR_EXPECTED,
            );
            // Skip past => and the expression for error recovery
            self.next_token();
            let _expr = self.parse_expression();
            self.parse_optional(SyntaxKind::SemicolonToken);
            NodeIndex::NONE
        } else {
            // Consume the semicolon if present (overload signature).
            // Use can_parse_semicolon() which handles ASI: a preceding line break
            // acts as an implicit semicolon (matching tsc's parseFunctionBlockOrSemicolon).
            if self.is_token(SyntaxKind::Unknown) {
                self.error_token_expected("{");
                self.next_token();
                if self.is_token(SyntaxKind::OpenBraceToken) {
                    self.parse_block()
                } else {
                    NodeIndex::NONE
                }
            } else if self.can_parse_semicolon() {
                self.parse_semicolon();
                NodeIndex::NONE
            } else {
                // TS1144: '{' or ';' expected — unexpected token after function signature
                self.parse_error_at_current_token(
                    "'{' or ';' expected.",
                    diagnostic_codes::OR_EXPECTED,
                );
                NodeIndex::NONE
            }
        };
        self.pop_label_scope();

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

    /// Parse function declaration for export default context (name is optional).
    /// Unlike regular function declarations, `export default function() {}` allows anonymous functions.
    /// Unlike function expressions, this creates a `FUNCTION_DECLARATION` node and supports
    /// overload signatures (missing body).
    pub(crate) fn parse_function_declaration_with_async_optional_name(
        &mut self,
        is_async: bool,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        let start_pos = self.token_pos();
        tracing::trace!(
            start_pos,
            "parse_function_declaration_with_async_optional_name"
        );

        let is_async = is_async || self.parse_optional(SyntaxKind::AsyncKeyword);
        self.parse_expected(SyntaxKind::FunctionKeyword);
        let asterisk_token = self.parse_optional(SyntaxKind::AsteriskToken);

        // Name is optional for export default function declarations
        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Clear STATIC_BLOCK before parsing parameters — function parameters create a
        // new scope where 'await' is a valid identifier (unless the function is async).
        self.context_flags &= !CONTEXT_FLAG_STATIC_BLOCK;

        let has_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if !has_open_paren && self.is_token(SyntaxKind::OpenBraceToken) {
            NodeList::new()
        } else {
            let params = self.parse_parameter_list();
            self.parse_expected(SyntaxKind::CloseParenToken);
            params
        };

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        let saved_flags = self.context_flags;
        self.context_flags &=
            !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR | CONTEXT_FLAG_STATIC_BLOCK);
        if is_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }
        if asterisk_token {
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
        }

        // Push a new label scope for the function body
        // Labels are function-scoped, so each function gets its own label namespace
        self.push_label_scope();

        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            self.parse_optional(SyntaxKind::SemicolonToken);
            NodeIndex::NONE
        };

        // Pop the label scope when exiting the function
        self.pop_label_scope();

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

    /// Parse function expression: `function()` {} or function `name()` {}
    ///
    /// Unlike function declarations, function expressions can be anonymous.
    pub(crate) fn parse_function_expression(&mut self) -> NodeIndex {
        self.parse_function_expression_with_async(false)
    }

    /// Parse async function expression: async `function()` {} or async function `name()` {}
    pub(crate) fn parse_async_function_expression(&mut self) -> NodeIndex {
        self.parse_function_expression_with_async(true)
    }

    /// Parse function expression with optional async modifier
    pub(crate) fn parse_function_expression_with_async(&mut self, is_async: bool) -> NodeIndex {
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

        // Set context flags BEFORE parsing name and parameters so that
        // reserved keywords (await/yield) are properly detected in function expressions
        // For async function * await() {}, the function name 'await' should error
        // For async function * (await) {}, the parameter name 'await' should error
        let saved_flags = self.context_flags;
        // Save whether we're in a static block before clearing the flag.
        // Function expression names bind in the outer scope, so 'await' as a name
        // is still illegal inside static blocks.
        let was_in_static_block = self.in_static_block_context();
        // Parameter-default context is for the containing parameter initializer only.
        // Nested function expressions create a new parsing context where this flag
        // must not leak into function body parsing.
        self.context_flags &= !(CONTEXT_FLAG_PARAMETER_DEFAULT
            | CONTEXT_FLAG_ASYNC
            | CONTEXT_FLAG_GENERATOR
            | CONTEXT_FLAG_CLASS_FIELD_INITIALIZER
            | CONTEXT_FLAG_STATIC_BLOCK);
        if is_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }
        if asterisk_token {
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
        }

        // Check for reserved words used as function expression names:
        // - `await` cannot be used as name in async function expressions
        // - `yield` cannot be used as name in generator function expressions
        // - `await` in static blocks is always illegal
        // Note: function DECLARATIONS are different - they bind in outer scope, so
        // `async function await() {}` as a declaration is valid.
        {
            use tsz_common::diagnostics::diagnostic_codes;
            if self.is_token(SyntaxKind::AwaitKeyword)
                && (was_in_static_block || is_async || self.in_async_context())
            {
                self.parse_error_at_current_token(
                    "Identifier expected. 'await' is a reserved word that cannot be used here.",
                    diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
                );
            } else if self.is_token(SyntaxKind::YieldKeyword) && self.in_generator_context() {
                self.report_yield_reserved_word_error();
            }
        }

        // Parse optional name (function expressions can be anonymous)
        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        // Parse optional type parameters: <T, U extends V>
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse parameters. If the opening `(` is missing and we're already at
        // `{`, treat it as the function body so statement recovery can produce
        // the downstream errors instead of parameter-list/object-literal noise.
        let has_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if !has_open_paren && self.is_token(SyntaxKind::OpenBraceToken) {
            NodeList::new()
        } else {
            let params = self.parse_parameter_list();
            self.parse_expected(SyntaxKind::CloseParenToken);
            params
        };

        // Parse optional return type (may be a type predicate: param is T)
        // Note: Type annotations are not in async/generator context
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        // Parse body (context flags remain set for await/yield expressions in body)
        // Push a new label scope for the function body
        self.push_label_scope();
        let body = self.parse_block();
        self.pop_label_scope();

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

    // Class expressions, declarations, and decorators → state_statements_class.rs
    // Class member modifiers, members, and static blocks → state_statements_class_members.rs
}
