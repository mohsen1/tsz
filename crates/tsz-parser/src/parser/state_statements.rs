//! Parser state - statement and declaration parsing methods
use super::state::{
    CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_GENERATOR, CONTEXT_FLAG_IN_BLOCK,
    CONTEXT_FLAG_PARAMETER_DEFAULT, IncrementalParseResult, ParserState,
};
use crate::parser::{
    NodeIndex, NodeList,
    node::{
        BlockData, FunctionData, ImportDeclData, LabeledData, QualifiedNameData, SourceFileData,
        VariableData, VariableDeclarationData,
    },
    parse_rules::{
        is_identifier_or_keyword, look_ahead_is, look_ahead_is_abstract_declaration,
        look_ahead_is_async_declaration, look_ahead_is_const_enum, look_ahead_is_import_call,
        look_ahead_is_import_equals, look_ahead_is_module_declaration,
        look_ahead_is_type_alias_declaration,
    },
    syntax_kind_ext,
};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::SyntaxKind;

impl ParserState {
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

        while !self.is_token(SyntaxKind::EndOfFileToken) {
            let pos_before = self.token_pos();
            if skip_after_binary_payload {
                break;
            }

            // Handle Unknown tokens (invalid characters) - must be checked FIRST
            if self.is_token(SyntaxKind::Unknown) {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Invalid character.",
                    diagnostic_codes::INVALID_CHARACTER,
                );
                self.resync_after_error_with_statement_starts(false);
                continue;
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
                // Resync to next statement boundary
                self.resync_after_error();
                continue;
            }

            if self.is_token(SyntaxKind::AtToken) {
                let snapshot = self.scanner.save_state();
                let at_token = self.current_token;
                self.next_token();
                if self.is_token(SyntaxKind::Unknown) {
                    self.current_token = at_token;
                    self.next_token();
                    self.parse_error_at_current_token(
                        "Invalid character.",
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
                    continue;
                }
                self.scanner.restore_state(snapshot);
                self.current_token = at_token;
            }

            let statement_start_token = self.token();
            let stmt = self.parse_statement();
            if stmt.is_none() {
                // Statement parsing failed, resync to recover
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
            } else {
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

        while !self.is_token(SyntaxKind::EndOfFileToken)
            && !self.is_token(SyntaxKind::CloseBraceToken)
        {
            let pos_before = self.token_pos();

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
                    "Invalid character.",
                    diagnostic_codes::INVALID_CHARACTER,
                );
                self.resync_after_error_with_statement_starts(false);
                continue;
            }

            let statement_start_token = self.token();
            let stmt = self.parse_statement();
            if stmt.is_none() {
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
            } else {
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
            SyntaxKind::VarKeyword | SyntaxKind::UsingKeyword => self.parse_variable_statement(),
            SyntaxKind::LetKeyword => {
                // In strict mode (modules, classes, etc.), `let` is a reserved word and
                // cannot be used as an identifier. But `let;` or `let` followed by a
                // non-declaration-start token should NOT be parsed as a variable declaration.
                // tsc checks `isLetDeclaration()`: next token must be identifier, `{`, or `[`.
                if self.look_ahead_is_let_declaration() {
                    self.parse_variable_statement()
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
                // Look ahead to see if it's "await using"
                if self.look_ahead_is_await_using() {
                    self.parse_variable_statement()
                } else {
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::AtToken => {
                // Decorator: @decorator class/function
                self.parse_decorated_declaration()
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
                } else {
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::TypeKeyword => self.parse_statement_type_keyword(),
            SyntaxKind::EnumKeyword => self.parse_enum_declaration(),
            SyntaxKind::DeclareKeyword => {
                if self.in_block_context() && self.look_ahead_is_declare_before_declaration() {
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
                // Keep parity with tsc recovery for malformed `export =` in blocks:
                // prefer the parse error from export assignment over generic TS1184.
                if self.in_block_context() && !self.look_ahead_is_export_assignment() {
                    self.parse_error_at_current_token(
                        "Modifiers cannot appear here.",
                        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                    );
                }
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
            self.parse_error_at_current_token(
                "Modifiers cannot appear here.",
                diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
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
        } else if self.look_ahead_next_is_identifier_or_keyword_on_same_line() {
            self.parse_error_at_current_token(
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
            self.next_token();
            let diag_count = self.parse_diagnostics.len();
            let result = self.parse_statement();
            let mut i = diag_count;
            while i < self.parse_diagnostics.len() {
                if self.parse_diagnostics[i].code == diagnostic_codes::EXPECTED {
                    self.parse_diagnostics.remove(i);
                } else {
                    i += 1;
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
        } else {
            self.parse_import_declaration()
        }
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
        let is_decl = !self.scanner.has_preceding_line_break()
            && matches!(
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
            );
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
            is_identifier_or_keyword,
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

    /// Look ahead to see if we have `export =`.
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
        let _ = is_type_only; // stored for future use in type checking

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
    pub(crate) fn parse_external_module_reference(&mut self) -> NodeIndex {
        self.parse_expected(SyntaxKind::RequireKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);
        let expression = self.parse_string_literal();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Return the string literal as the module reference
        expression
    }

    /// Parse entity name: A or A.B.C or this or this.x
    pub(crate) fn parse_entity_name(&mut self) -> NodeIndex {
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
                }
                break;
            }

            let decl = self.parse_variable_declaration_with_flags(flags);
            declarations.push(decl);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                // If ASI applies (line break, closing brace, EOF, or semicolon),
                // just break - parse_semicolon() in the caller will handle it
                if self.can_parse_semicolon() {
                    break;
                }

                // No ASI - check if next token looks like another declaration
                // on the same line. If so, emit comma error for better diagnostics.
                let can_start_next = self.is_identifier_or_keyword()
                    || self.is_token(SyntaxKind::OpenBraceToken)
                    || self.is_token(SyntaxKind::OpenBracketToken);

                if can_start_next {
                    self.error_comma_expected();
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
                    // Report at the comma position (one token back).
                    // The comma was already consumed by parse_optional above.
                    let end = self.token_pos();
                    let start = end.saturating_sub(1);
                    self.parse_error_at(
                        start,
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
        if declarations.is_empty() && !self.is_token(SyntaxKind::Unknown) {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
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
        {
            use crate::parser::syntax_kind_ext::{ARRAY_BINDING_PATTERN, OBJECT_BINDING_PATTERN};
            if name_node.kind == OBJECT_BINDING_PATTERN || name_node.kind == ARRAY_BINDING_PATTERN {
                self.parse_error_at(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "A destructuring declaration must have an initializer.",
                    diagnostic_codes::A_DESTRUCTURING_DECLARATION_MUST_HAVE_AN_INITIALIZER,
                );
            }
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
                let is_class_context = self.in_class_body() || self.in_class_member_name();
                if is_class_context {
                    self.parse_error_at_current_token(
                        "Identifier expected. 'yield' is a reserved word in strict mode. Class definitions are automatically in strict mode.",
                        diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
                    );
                } else {
                    self.parse_error_at_current_token(
                        "Identifier expected. 'yield' is a reserved word in strict mode.",
                        diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
                    );
                }
            }
        }

        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            self.parse_identifier()
        };

        // Parse optional type parameters: <T, U extends V>
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse parameters
        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Parse optional return type (may be a type predicate: param is T)
        // Note: Type annotations are not in async/generator context
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        // Parse body - may be missing for overload signatures (just a semicolon)
        // Context flags remain set for await/yield expressions in body
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
            // Consume the semicolon if present (overload signature)
            self.parse_optional(SyntaxKind::SemicolonToken);
            NodeIndex::NONE
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

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        let saved_flags = self.context_flags;
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
        // Parameter-default context is for the containing parameter initializer only.
        // Nested function expressions create a new parsing context where this flag
        // must not leak into function body parsing.
        self.context_flags &= !CONTEXT_FLAG_PARAMETER_DEFAULT;
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
                && (self.in_static_block_context() || is_async || self.in_async_context())
            {
                self.parse_error_at_current_token(
                    "Identifier expected. 'await' is a reserved word that cannot be used here.",
                    diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
                );
            } else if self.is_token(SyntaxKind::YieldKeyword) && self.in_generator_context() {
                let is_class_context = self.in_class_body() || self.in_class_member_name();
                if is_class_context {
                    self.parse_error_at_current_token(
                        "Identifier expected. 'yield' is a reserved word in strict mode. Class definitions are automatically in strict mode.",
                        diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
                    );
                } else {
                    self.parse_error_at_current_token(
                        "Identifier expected. 'yield' is a reserved word in strict mode.",
                        diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
                    );
                }
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

        // Parse parameters
        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

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
