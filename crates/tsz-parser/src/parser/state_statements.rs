//! Parser state - statement and declaration parsing methods
use super::state::{
    CONTEXT_FLAG_AMBIENT, CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_CLASS_MEMBER_NAME,
    CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS, CONTEXT_FLAG_GENERATOR, CONTEXT_FLAG_IN_CLASS,
    CONTEXT_FLAG_PARAMETER_DEFAULT, CONTEXT_FLAG_STATIC_BLOCK, IncrementalParseResult, ParserState,
};
use crate::parser::{
    NodeIndex, NodeList,
    node::{
        self, BlockData, ClassData, FunctionData, ImportDeclData, LabeledData, QualifiedNameData,
        SourceFileData, VariableData, VariableDeclarationData,
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
use tsz_common::interner::Atom;
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
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword | SyntaxKind::UsingKeyword => {
                self.parse_variable_statement()
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
                self.error_unexpected_token();
                self.next_token();
                self.parse_statement()
            }
            SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(),
            SyntaxKind::TypeKeyword => self.parse_statement_type_keyword(),
            SyntaxKind::EnumKeyword => self.parse_enum_declaration(),
            SyntaxKind::DeclareKeyword => self.parse_statement_declare_or_expression(),
            SyntaxKind::NamespaceKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::GlobalKeyword => self.parse_statement_namespace_or_expression(),
            SyntaxKind::IfKeyword => self.parse_if_statement(),
            SyntaxKind::ReturnKeyword => self.parse_return_statement(),
            SyntaxKind::WhileKeyword => self.parse_while_statement(),
            SyntaxKind::ForKeyword => self.parse_for_statement(),
            SyntaxKind::SemicolonToken => self.parse_empty_statement(),
            SyntaxKind::ExportKeyword => self.parse_export_declaration(),
            SyntaxKind::ImportKeyword => self.parse_statement_import_keyword(),
            SyntaxKind::BreakKeyword => self.parse_break_statement(),
            SyntaxKind::ContinueKeyword => self.parse_continue_statement(),
            SyntaxKind::ThrowKeyword => self.parse_throw_statement(),
            SyntaxKind::DoKeyword => self.parse_do_statement(),
            SyntaxKind::SwitchKeyword => self.parse_switch_statement(),
            SyntaxKind::TryKeyword => self.parse_try_statement(),
            SyntaxKind::CatchKeyword | SyntaxKind::FinallyKeyword => {
                // Orphan catch/finally block (missing try)
                self.parse_orphan_catch_or_finally_block()
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
            self.parse_error_at_current_token(
                "Modifier cannot be used here.",
                diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
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

    /// Check if `declare` is followed by a valid declaration keyword.
    /// Used to distinguish `declare class ...` (ambient declaration) from
    /// `declare instanceof C` (expression using `declare` as identifier).
    fn look_ahead_is_declare_before_declaration(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token(); // skip `declare`
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

            // After comma, check if next token can start another declaration
            // Handle cases like: let x, , y (missing declaration between commas)
            let can_start_next = self.is_identifier_or_keyword()
                || self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::OpenBracketToken);

            if !can_start_next {
                // Next token cannot start a declaration - emit error for missing declaration
                // and break to avoid consuming tokens that belong to the next statement
                use tsz_common::diagnostics::diagnostic_codes;
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

        // Check TS1375: 'using' declarations do not support destructuring patterns
        if is_using
            && (self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::OpenBracketToken))
        {
            self.parse_error_at_current_token(
                diagnostic_messages::DECLARATIONS_CAN_ONLY_BE_DECLARED_INSIDE_A_BLOCK,
                diagnostic_codes::DECLARATIONS_CAN_ONLY_BE_DECLARED_INSIDE_A_BLOCK,
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
        if is_catch_clause && !initializer.is_none() {
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
        if !initializer.is_none() {
            self.arena
                .get(initializer)
                .map_or_else(|| self.token_pos(), |n| n.end)
        } else if !type_annotation.is_none() {
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

    /// Parse class expression: class {} or class Name {}
    ///
    /// Unlike class declarations, class expressions can be anonymous.
    pub(crate) fn parse_class_expression(&mut self) -> NodeIndex {
        self.parse_class_expression_with_decorators(None, self.token_pos())
    }

    pub(crate) fn parse_class_expression_with_decorators(
        &mut self,
        decorators: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse optional name (class expressions can be anonymous)
        // Like class declarations, keywords can be used as class names
        // EXCEPT extends/implements which start heritage clauses
        let name = if self.is_identifier_or_keyword()
            && !self.is_token(SyntaxKind::ExtendsKeyword)
            && !self.is_token(SyntaxKind::ImplementsKeyword)
        {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        // Parse optional type parameters
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage (extends/implements)
        let heritage = self.parse_heritage_clauses();

        // Parse body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        self.arena.add_class(
            syntax_kind_ext::CLASS_EXPRESSION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: decorators,
                name,
                type_parameters,
                heritage_clauses: heritage,
                members,
            },
        )
    }

    /// Parse parameter list
    pub(crate) fn parse_parameter_list(&mut self) -> NodeList {
        let mut params = Vec::new();
        let mut seen_rest_parameter = false;
        let mut emitted_rest_error = false;

        while !self.is_token(SyntaxKind::CloseParenToken) {
            // TS1014: A rest parameter must be last in a parameter list
            // Check BEFORE parsing the next parameter (but only emit once)
            if seen_rest_parameter && !emitted_rest_error {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "A rest parameter must be last in a parameter list.",
                    diagnostic_codes::A_REST_PARAMETER_MUST_BE_LAST_IN_A_PARAMETER_LIST,
                );
                emitted_rest_error = true;
            }

            let param = self.parse_parameter();

            // Check if this is a rest parameter (...)
            let is_rest_param = if let Some(node) = self.arena.get(param) {
                if let Some(param_data) = self.arena.get_parameter(node) {
                    param_data.dot_dot_dot_token
                } else {
                    false
                }
            } else {
                false
            };

            seen_rest_parameter = seen_rest_parameter || is_rest_param;
            params.push(param);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                // Recovery: in malformed parameter initializers like
                // `function* f(a = yield => yield) {}` or
                // `async function f(a = await => await) {}`
                // treat `=>` as a missing comma boundary to continue parsing.
                if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                    self.error_comma_expected();
                    self.next_token(); // consume =>
                    if self.is_parameter_start() {
                        continue;
                    }
                    break;
                }
                // Trailing commas are allowed in parameter lists
                // Only emit an error if we have another parameter without a comma
                if !self.is_token(SyntaxKind::CloseParenToken) && self.is_parameter_start() {
                    // Emit TS1005 for missing comma between parameters: f(a b)
                    self.error_comma_expected();
                }
                break;
            }
        }

        self.make_node_list(params)
    }

    /// Check if current token is a valid parameter modifier
    const fn is_valid_parameter_modifier(&self) -> bool {
        matches!(
            self.current_token,
            SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::OverrideKeyword
        )
    }

    /// Check if current token is a modifier keyword used as a parameter modifier.
    /// This includes invalid modifiers like static/export that tsc accepts during
    /// parsing but reports TS1090 for in the checker.
    /// Uses look-ahead to distinguish `(static x: number)` (modifier) from
    /// `(async: boolean)` (parameter name).
    pub(crate) fn is_parameter_modifier(&mut self) -> bool {
        if self.is_valid_parameter_modifier() {
            return true;
        }
        if !matches!(
            self.current_token,
            SyntaxKind::StaticKeyword
                | SyntaxKind::ExportKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::DefaultKeyword
                | SyntaxKind::InKeyword
                | SyntaxKind::OutKeyword
        ) {
            return false;
        }
        // Look ahead: if the next token can follow a modifier (identifier/keyword,
        // string/number literal, [, {, *, ...), then this keyword is being used as
        // a modifier. Otherwise it's a parameter name (e.g., `async: boolean`).
        // This mirrors tsc's canFollowModifier() + isLiteralPropertyName() check.
        let snapshot = self.scanner.save_state();
        let saved_token = self.current_token;
        self.next_token();
        let can_follow = !self.scanner.has_preceding_line_break()
            && (matches!(
                self.current_token,
                SyntaxKind::OpenBracketToken
                    | SyntaxKind::OpenBraceToken
                    | SyntaxKind::AsteriskToken
                    | SyntaxKind::DotDotDotToken
                    | SyntaxKind::StringLiteral
                    | SyntaxKind::NumericLiteral
            ) || self.is_identifier_or_keyword());
        self.scanner.restore_state(snapshot);
        self.current_token = saved_token;
        can_follow
    }

    /// Parse parameter modifiers (public, private, protected, readonly, override,
    /// and invalid ones like static/export/declare/async which get TS1090).
    pub(crate) fn parse_parameter_modifiers(&mut self) -> Option<NodeList> {
        let mut modifiers = Vec::new();
        let mut seen_readonly = false;
        let mut seen_accessibility = false;

        while self.is_parameter_modifier() {
            let mod_start = self.token_pos();
            let mod_kind = self.current_token;

            // Emit TS1090 for modifiers that cannot appear on parameters.
            // tsc does this in the checker via checkGrammarModifiers, but we
            // emit it here during parsing so we don't need checker support yet.
            if !self.is_valid_parameter_modifier() {
                use tsz_common::diagnostics::diagnostic_codes;
                let modifier_name = match mod_kind {
                    SyntaxKind::StaticKeyword => "static",
                    SyntaxKind::ExportKeyword => "export",
                    SyntaxKind::DeclareKeyword => "declare",
                    SyntaxKind::AsyncKeyword => "async",
                    SyntaxKind::AbstractKeyword => "abstract",
                    SyntaxKind::AccessorKeyword => "accessor",
                    SyntaxKind::ConstKeyword => "const",
                    SyntaxKind::DefaultKeyword => "default",
                    SyntaxKind::InKeyword => "in",
                    SyntaxKind::OutKeyword => "out",
                    _ => "modifier",
                };
                self.parse_error_at_current_token(
                    &format!("'{modifier_name}' modifier cannot appear on a parameter."),
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_PARAMETER,
                );
            }

            // Check for modifier ordering violations
            // Parameter modifiers must be in order: accessibility, readonly
            if matches!(
                mod_kind,
                SyntaxKind::PublicKeyword
                    | SyntaxKind::PrivateKeyword
                    | SyntaxKind::ProtectedKeyword
            ) {
                if seen_accessibility {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Accessibility modifier already seen.",
                        diagnostic_codes::ACCESSIBILITY_MODIFIER_ALREADY_SEEN,
                    );
                }
                // TS1029: Accessibility modifier must come before readonly
                if seen_readonly {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'accessibility modifier' must come before 'readonly' modifier.",
                        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                seen_accessibility = true;
            } else if mod_kind == SyntaxKind::ReadonlyKeyword {
                seen_readonly = true;
            }

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
    pub(crate) fn parse_parameter(&mut self) -> NodeIndex {
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

        // Check for illegal binding identifiers (e.g., 'await' in async contexts, 'yield' in generator contexts)
        // This must be called BEFORE parsing the parameter name to catch reserved words
        self.check_illegal_binding_identifier();
        if (self.context_flags & CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS) != 0
            && self.is_token(SyntaxKind::StaticKeyword)
        {
            self.parse_error_at_current_token(
                "Identifier expected. 'static' is a reserved word in strict mode. Class definitions are automatically in strict mode.",
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
            );
        }

        // TS18009: Check for private identifiers used as parameters (check before parsing)
        if self.is_token(SyntaxKind::PrivateIdentifier) {
            let start = self.token_pos();
            let length = self.token_end() - start;
            self.parse_error_at(
                start,
                length,
                "Private identifiers cannot be used as parameters.",
                diagnostic_codes::PRIVATE_IDENTIFIERS_CANNOT_BE_USED_AS_PARAMETERS,
            );
        }

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
            // Check if parameter has both optional marker (?) and initializer (=)
            // TS1015: Parameter cannot have question mark and initializer
            // This applies to all parameter contexts, including arrow functions.
            if question_token {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "A parameter cannot have question mark and initializer.",
                    diagnostic_codes::PARAMETER_CANNOT_HAVE_QUESTION_MARK_AND_INITIALIZER,
                );
            }

            // Default parameter values are evaluated in the parent scope, not in the function body.
            // Set parameter default context flag to detect 'await' usage.
            // IMPORTANT: Keep async context set - TSC emits TS1109 "Expression expected" when
            // 'await' appears in a parameter default without an operand (e.g., `async (a = await)`)
            let saved_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_PARAMETER_DEFAULT;
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
            crate::parser::node::ParameterData {
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
    pub(crate) fn parse_class_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name - keywords like 'any', 'string' can be used as class names
        // EXCEPT extends/implements which start heritage clauses
        // AND reserved words like 'void', 'null' which cannot be identifiers
        let name = if self.is_identifier_or_keyword()
            && !self.is_token(SyntaxKind::ExtendsKeyword)
            && !self.is_token(SyntaxKind::ImplementsKeyword)
        {
            // TS1005: Reserved words cannot be used as class names
            if self.is_reserved_word() {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
                // Consume the invalid token to avoid cascading errors
                self.next_token();
                NodeIndex::NONE
            } else {
                self.parse_identifier_name()
            }
        } else {
            NodeIndex::NONE
        };

        // Parse type parameters: class Foo<T, U> {}
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
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
    pub(crate) fn parse_class_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name - keywords like 'any', 'string' can be used as class names
        // EXCEPT extends/implements which start heritage clauses
        // AND reserved words like 'void', 'null' which cannot be identifiers
        let name = if self.is_identifier_or_keyword()
            && !self.is_token(SyntaxKind::ExtendsKeyword)
            && !self.is_token(SyntaxKind::ImplementsKeyword)
        {
            // TS1005: Reserved words cannot be used as class names
            if self.is_reserved_word() {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
                // Consume the invalid token to avoid cascading errors
                self.next_token();
                NodeIndex::NONE
            } else {
                self.parse_identifier_name()
            }
        } else {
            NodeIndex::NONE
        };

        // Parse type parameters: class Foo<T, U> {}
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
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
    pub(crate) fn parse_abstract_class_declaration(&mut self) -> NodeIndex {
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
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
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
    pub(crate) fn parse_declare_class(
        &mut self,
        start_pos: u32,
        declare_modifier: NodeIndex,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::ClassKeyword);

        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        let heritage_clauses = self.parse_heritage_clauses();

        self.parse_expected(SyntaxKind::OpenBraceToken);

        // Set ambient context for class members
        let saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_AMBIENT | CONTEXT_FLAG_IN_CLASS;

        let members = self.parse_class_members();

        // Restore context flags
        self.context_flags = saved_flags;

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
    pub(crate) fn parse_declare_abstract_class(
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

        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        let heritage_clauses = self.parse_heritage_clauses();

        self.parse_expected(SyntaxKind::OpenBraceToken);

        // Set ambient context for class members
        let saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_AMBIENT | CONTEXT_FLAG_IN_CLASS;

        let members = self.parse_class_members();

        // Restore context flags
        self.context_flags = saved_flags;

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
    pub(crate) fn parse_decorated_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse decorators
        let decorators = self.parse_decorators();

        // After decorators, expect class or abstract class
        // Decorators on other declarations are invalid (TS1206)
        match self.token() {
            SyntaxKind::ClassKeyword => {
                self.parse_class_declaration_with_decorators(decorators, start_pos)
            }
            SyntaxKind::AbstractKeyword => {
                // abstract class with decorators
                self.parse_abstract_class_declaration_with_decorators(decorators, start_pos)
            }
            SyntaxKind::FunctionKeyword => {
                // TS1206: Decorators are not valid on function declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_function_declaration()
            }
            SyntaxKind::EnumKeyword => {
                // TS1206: Decorators are not valid on enum declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_enum_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::InterfaceKeyword => {
                // TS1206: Decorators are not valid on interface declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_interface_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::TypeKeyword => {
                // TS1206: Decorators are not valid on type alias declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_type_alias_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                // TS1206: Decorators are not valid on namespace/module declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_module_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword | SyntaxKind::ConstKeyword => {
                // TS1206: Decorators are not valid on variable statements
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_variable_statement_with_modifiers(Some(start_pos), decorators)
            }
            SyntaxKind::ImportKeyword => {
                // TS1206: Decorators are not valid on import statements
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                // Check if this is import equals (import X = ...) or regular import
                if self.look_ahead_is_import_equals() {
                    self.parse_import_equals_declaration()
                } else {
                    self.parse_import_declaration()
                }
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
    pub(crate) fn parse_decorators(&mut self) -> Option<NodeList> {
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
    pub(crate) fn try_parse_decorator(&mut self) -> Option<NodeIndex> {
        if !self.is_token(SyntaxKind::AtToken) {
            return None;
        }

        let start_pos = self.token_pos();
        let snapshot = self.scanner.save_state();
        let at_token = self.current_token;
        self.next_token(); // consume @
        if self.is_token(SyntaxKind::Unknown) {
            self.scanner.restore_state(snapshot);
            self.current_token = at_token;
            return None;
        }

        // Parse the decorator expression (identifier, member access, or call)
        // Set CONTEXT_FLAG_IN_DECORATOR so that '[' is NOT treated as element access
        // (it starts a computed property name on the decorated member instead)
        let saved_flags = self.context_flags;
        self.context_flags |= crate::parser::state::CONTEXT_FLAG_IN_DECORATOR;
        let expression = self.parse_left_hand_side_expression();
        self.context_flags = saved_flags;

        let end_pos = self.token_end();
        Some(self.arena.add_decorator(
            syntax_kind_ext::DECORATOR,
            start_pos,
            end_pos,
            crate::parser::node::DecoratorData { expression },
        ))
    }

    /// Parse class declaration with pre-parsed decorators
    pub(crate) fn parse_class_declaration_with_decorators(
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

        // Parse type parameters
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
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
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse abstract class declaration with pre-parsed decorators
    pub(crate) fn parse_abstract_class_declaration_with_decorators(
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

        // Parse type parameters
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        // Combine decorators with abstract modifier
        let modifiers = if let Some(dec_list) = decorators {
            // Add abstract modifier to decorator list
            let mut nodes: Vec<NodeIndex> = dec_list.nodes;
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
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse heritage clauses (extends, implements)
    pub(crate) fn parse_heritage_clauses(&mut self) -> Option<NodeList> {
        let mut clauses = Vec::new();
        let mut seen_extends = false;
        let mut seen_implements = false;

        loop {
            if self.is_token(SyntaxKind::ExtendsKeyword) {
                if let Some(clause) =
                    self.parse_heritage_clause_extends(&mut seen_extends, seen_implements)
                {
                    clauses.push(clause);
                }
                continue;
            }

            if self.is_token(SyntaxKind::ImplementsKeyword) {
                if let Some(clause) = self.parse_heritage_clause_implements(&mut seen_implements) {
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

    fn parse_heritage_clause_extends(
        &mut self,
        seen_extends: &mut bool,
        seen_implements: bool,
    ) -> Option<NodeIndex> {
        use tsz_common::diagnostics::diagnostic_codes;

        let start_pos = self.token_pos();
        let is_duplicate = *seen_extends;

        if is_duplicate {
            self.parse_error_at_current_token(
                "extends clause already seen.",
                diagnostic_codes::EXTENDS_CLAUSE_ALREADY_SEEN,
            );
        } else if seen_implements {
            self.parse_error_at_current_token(
                "extends clause must precede implements clause.",
                diagnostic_codes::EXTENDS_CLAUSE_MUST_PRECEDE_IMPLEMENTS_CLAUSE,
            );
        }

        let should_add = !*seen_extends;
        *seen_extends = true;
        self.next_token();

        if self.is_token(SyntaxKind::OpenBraceToken) || self.is_token(SyntaxKind::ImplementsKeyword)
        {
            self.parse_error_at_current_token(
                "'extends' list cannot be empty.",
                diagnostic_codes::LIST_CANNOT_BE_EMPTY,
            );
            return None;
        }

        let type_ref = self.parse_heritage_type_reference();

        while self.is_token(SyntaxKind::CommaToken) {
            let comma_pos = self.token_pos();
            let comma_end = self.token_end();
            self.next_token();
            if self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::ImplementsKeyword)
            {
                self.parse_error_at(
                    comma_pos,
                    comma_end - comma_pos,
                    tsz_common::diagnostics::diagnostic_messages::TRAILING_COMMA_NOT_ALLOWED,
                    diagnostic_codes::TRAILING_COMMA_NOT_ALLOWED,
                );
                break;
            }
            self.parse_error_at(
                comma_pos,
                comma_end - comma_pos,
                "Classes can only extend a single class.",
                diagnostic_codes::CLASSES_CAN_ONLY_EXTEND_A_SINGLE_CLASS,
            );
            let _ = self.parse_heritage_type_reference();
        }

        if !should_add {
            return None;
        }

        let end_pos = self.token_end();
        Some(self.arena.add_heritage(
            syntax_kind_ext::HERITAGE_CLAUSE,
            start_pos,
            end_pos,
            crate::parser::node::HeritageData {
                token: SyntaxKind::ExtendsKeyword as u16,
                types: self.make_node_list(vec![type_ref]),
            },
        ))
    }

    fn parse_heritage_clause_implements(
        &mut self,
        seen_implements: &mut bool,
    ) -> Option<NodeIndex> {
        use tsz_common::diagnostics::diagnostic_codes;

        let start_pos = self.token_pos();
        if *seen_implements {
            self.parse_error_at_current_token(
                "implements clause already seen.",
                diagnostic_codes::IMPLEMENTS_CLAUSE_ALREADY_SEEN,
            );
        }

        let should_add = !*seen_implements;
        *seen_implements = true;
        self.next_token();

        let mut types = Vec::new();
        loop {
            let type_ref = self.parse_heritage_type_reference();
            types.push(type_ref);
            if self.is_token(SyntaxKind::CommaToken) {
                let comma_pos = self.token_pos();
                let comma_end = self.token_end();
                self.next_token();
                // Trailing comma before { — emit TS1009 like the extends clause does
                if self.is_token(SyntaxKind::OpenBraceToken)
                    || self.is_token(SyntaxKind::ExtendsKeyword)
                {
                    self.parse_error_at(
                        comma_pos,
                        comma_end - comma_pos,
                        tsz_common::diagnostics::diagnostic_messages::TRAILING_COMMA_NOT_ALLOWED,
                        diagnostic_codes::TRAILING_COMMA_NOT_ALLOWED,
                    );
                    break;
                }
            } else {
                break;
            }
        }

        if !should_add {
            return None;
        }

        let end_pos = self.token_end();
        Some(self.arena.add_heritage(
            syntax_kind_ext::HERITAGE_CLAUSE,
            start_pos,
            end_pos,
            crate::parser::node::HeritageData {
                token: SyntaxKind::ImplementsKeyword as u16,
                types: self.make_node_list(types),
            },
        ))
    }

    /// Parse a heritage type reference: Foo or Foo<T> or Foo.Bar<T> or base<T>()
    /// This is used in extends/implements clauses
    pub(crate) fn parse_heritage_type_reference(&mut self) -> NodeIndex {
        // parse_heritage_left_hand_expression now handles:
        // - Simple identifiers: Foo
        // - Property access: Foo.Bar.Baz
        // - Type arguments: Foo<T>
        // - Call expressions: Mixin(Parent) or base<T>()
        self.parse_heritage_left_hand_expression()
    }

    /// Parse heritage type reference for interfaces (extends clause).
    /// Interfaces must reference types; literals or arbitrary expressions should produce diagnostics.
    pub(crate) fn parse_interface_heritage_type_reference(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        if matches!(
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
                | SyntaxKind::ClassKeyword
                | SyntaxKind::NewKeyword
                | SyntaxKind::OpenParenToken
        ) {
            let start = self.token_pos();
            let end = self.token_end();
            self.parse_error_at_current_token(
                "Type name expected in interface extends clause.",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            self.next_token();
            return self.arena.add_token(SyntaxKind::Unknown as u16, start, end);
        }

        self.parse_heritage_type_reference()
    }

    /// Parse left-hand expression for heritage clauses: Foo, Foo.Bar, or Mixin(Parent)
    /// This is a subset of member expression that allows identifiers, dots, and call expressions
    pub(crate) fn parse_heritage_left_hand_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_heritage_left_hand_expression_base();

        while let Some(next_expr) = self.parse_heritage_left_hand_expression_chain(start_pos, expr)
        {
            expr = next_expr;
        }

        expr
    }

    fn parse_heritage_left_hand_expression_base(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        if self.is_token(SyntaxKind::ClassKeyword) {
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
            self.parse_primary_expression()
        } else if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            self.parse_error_at_current_token(
                "Class name or type expression expected",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            self.next_token();
            self.arena
                .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
        }
    }

    fn parse_heritage_left_hand_expression_chain(
        &mut self,
        start_pos: u32,
        expr: NodeIndex,
    ) -> Option<NodeIndex> {
        match self.token() {
            SyntaxKind::DotToken => {
                self.next_token();
                let name = if self.is_identifier_or_keyword() {
                    self.parse_identifier_name()
                } else {
                    self.parse_identifier()
                };
                let end_pos = self.token_end();
                Some(self.arena.add_access_expr(
                    syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                    start_pos,
                    end_pos,
                    crate::parser::node::AccessExprData {
                        expression: expr,
                        name_or_argument: name,
                        question_dot_token: false,
                    },
                ))
            }
            SyntaxKind::QuestionDotToken => {
                self.next_token();
                let name = if self.is_identifier_or_keyword() {
                    self.parse_identifier_name()
                } else {
                    self.parse_identifier()
                };
                let end_pos = self.token_end();
                Some(self.arena.add_access_expr(
                    syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                    start_pos,
                    end_pos,
                    crate::parser::node::AccessExprData {
                        expression: expr,
                        name_or_argument: name,
                        question_dot_token: true,
                    },
                ))
            }
            SyntaxKind::LessThanToken => {
                self.next_token();
                let mut type_args = Vec::new();
                while !self.is_token(SyntaxKind::GreaterThanToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    type_args.push(self.parse_type());
                    if !self.parse_optional(SyntaxKind::CommaToken) {
                        break;
                    }
                }
                self.parse_expected(SyntaxKind::GreaterThanToken);
                if self.is_token(SyntaxKind::OpenParenToken) {
                    self.next_token();
                    let (end_pos, args) = self.parse_heritage_call_arguments();
                    Some(self.arena.add_call_expr(
                        syntax_kind_ext::CALL_EXPRESSION,
                        start_pos,
                        end_pos,
                        crate::parser::node::CallExprData {
                            expression: expr,
                            type_arguments: Some(self.make_node_list(type_args)),
                            arguments: Some(args),
                        },
                    ))
                } else {
                    Some(self.arena.add_expr_with_type_args(
                        syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS,
                        start_pos,
                        self.token_end(),
                        crate::parser::node::ExprWithTypeArgsData {
                            expression: expr,
                            type_arguments: Some(self.make_node_list(type_args)),
                        },
                    ))
                }
            }
            SyntaxKind::OpenParenToken => {
                self.next_token();
                let (end_pos, args) = self.parse_heritage_call_arguments();
                Some(self.arena.add_call_expr(
                    syntax_kind_ext::CALL_EXPRESSION,
                    start_pos,
                    end_pos,
                    crate::parser::node::CallExprData {
                        expression: expr,
                        type_arguments: None,
                        arguments: Some(args),
                    },
                ))
            }
            _ => None,
        }
    }

    fn parse_heritage_call_arguments(&mut self) -> (u32, NodeList) {
        let mut args = Vec::new();
        while !self.is_token(SyntaxKind::CloseParenToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            args.push(self.parse_assignment_expression());
            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseParenToken);
        (end_pos, self.make_node_list(args))
    }

    /// Parse class member modifiers (static, public, private, protected, readonly, abstract, override)
    pub(crate) fn parse_class_member_modifiers(&mut self) -> Option<NodeList> {
        let mut modifiers = Vec::new();

        // State tracking for TS1028 (duplicates) and TS1029 (ordering)
        let mut seen_accessibility = false;
        let mut seen_static = false;
        let mut seen_abstract = false;
        let mut seen_readonly = false;
        let mut seen_override = false;
        let mut seen_accessor = false;
        let mut seen_async = false;

        loop {
            if self.should_stop_class_member_modifier() {
                break;
            }
            let start_pos = self.token_pos();

            // Before consuming token, check for TS1028 (duplicate accessibility) and TS1029 (wrong order)
            let current_kind = self.token();

            if matches!(
                current_kind,
                SyntaxKind::PublicKeyword
                    | SyntaxKind::PrivateKeyword
                    | SyntaxKind::ProtectedKeyword
            ) {
                if seen_accessibility {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Accessibility modifier already seen.",
                        diagnostic_codes::ACCESSIBILITY_MODIFIER_ALREADY_SEEN,
                    );
                }
                // TS1029: accessibility must come after certain modifiers
                if seen_static
                    || seen_abstract
                    || seen_readonly
                    || seen_override
                    || seen_accessor
                    || seen_async
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    let current_mod = match current_kind {
                        SyntaxKind::PublicKeyword => "public",
                        SyntaxKind::PrivateKeyword => "private",
                        SyntaxKind::ProtectedKeyword => "protected",
                        _ => "accessibility",
                    };
                    let conflicting_mod = if seen_static {
                        "static"
                    } else if seen_abstract {
                        "abstract"
                    } else if seen_readonly {
                        "readonly"
                    } else if seen_override {
                        "override"
                    } else if seen_accessor {
                        "accessor"
                    } else {
                        "async"
                    };
                    self.parse_error_at_current_token(
                        &format!("'{current_mod}' modifier must precede '{conflicting_mod}' modifier."),
                        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                seen_accessibility = true;
            } else if current_kind == SyntaxKind::StaticKeyword {
                // Check for duplicate static modifier
                if seen_static {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'static' modifier already seen.",
                        diagnostic_codes::MODIFIER_ALREADY_SEEN,
                    );
                }
                // TS1029: static must come after accessibility, before certain others
                if seen_abstract || seen_readonly || seen_override || seen_accessor || seen_async {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'static' modifier must precede current modifier.",
                        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                seen_static = true;
            } else if current_kind == SyntaxKind::AbstractKeyword {
                // Check for duplicate abstract modifier
                if seen_abstract {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'abstract' modifier already seen.",
                        diagnostic_codes::MODIFIER_ALREADY_SEEN,
                    );
                }
                if seen_readonly || seen_override || seen_accessor || seen_async {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'abstract' modifier must precede current modifier.",
                        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                seen_abstract = true;
            } else if current_kind == SyntaxKind::ReadonlyKeyword {
                // Check for duplicate readonly modifier
                if seen_readonly {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'readonly' modifier already seen.",
                        diagnostic_codes::MODIFIER_ALREADY_SEEN,
                    );
                }
                if seen_override || seen_accessor || seen_async {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'readonly' modifier must precede current modifier.",
                        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                seen_readonly = true;
            } else if current_kind == SyntaxKind::OverrideKeyword {
                // Check for duplicate override modifier
                if seen_override {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'override' modifier already seen.",
                        diagnostic_codes::MODIFIER_ALREADY_SEEN,
                    );
                }
                if seen_accessor || seen_async {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'override' modifier must precede current modifier.",
                        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                seen_override = true;
            } else if current_kind == SyntaxKind::AccessorKeyword {
                // Check for duplicate accessor modifier
                if seen_accessor {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'accessor' modifier already seen.",
                        diagnostic_codes::MODIFIER_ALREADY_SEEN,
                    );
                }
                if seen_async {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'accessor' modifier must precede 'async' modifier.",
                        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                seen_accessor = true;
            } else if current_kind == SyntaxKind::AsyncKeyword {
                // Check for duplicate async modifier
                if seen_async {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'async' modifier already seen.",
                        diagnostic_codes::MODIFIER_ALREADY_SEEN,
                    );
                }
                seen_async = true;
            }

            let modifier = match current_kind {
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
                    // TS1040: 'async' modifier cannot be used in an ambient context
                    if (self.context_flags & crate::parser::state::CONTEXT_FLAG_AMBIENT) != 0 {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "'async' modifier cannot be used in an ambient context.",
                            diagnostic_codes::MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                        );
                    }
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

                    // Check if followed by var/let (invalid pattern: const var foo)
                    // In this case, consume const without adding to modifiers, let var/let handler emit error
                    if matches!(
                        self.current_token,
                        SyntaxKind::VarKeyword | SyntaxKind::LetKeyword
                    ) {
                        // Restore state, consume const, and continue - var/let will emit TS1440
                        self.scanner.restore_state(snapshot);
                        self.current_token = saved_token;
                        self.next_token(); // Consume const
                        continue;
                    }

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
                    // Skip emitting generic unexpected modifier for export when it
                    // introduces a constructor declaration. Constructor-specific
                    // validation emits TS1031.
                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token();
                    let next_is_constructor = self.current_token == SyntaxKind::ConstructorKeyword
                        && !self.scanner.has_preceding_line_break();
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;

                    if !next_is_constructor {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "Unexpected modifier.",
                            diagnostic_codes::UNEXPECTED_TOKEN,
                        );
                    }
                    self.next_token();
                    self.arena
                        .create_modifier(SyntaxKind::ExportKeyword, start_pos)
                }
                // Handle 'let' and 'var' - could be property names or invalid modifiers
                SyntaxKind::LetKeyword | SyntaxKind::VarKeyword => {
                    // Look ahead to distinguish between property name and modifier
                    // var() { } or var followed by line break -> property name (valid)
                    // public var foo -> modifier (invalid)
                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token();

                    // If followed by open paren, it's a method name (valid)
                    if self.current_token == SyntaxKind::OpenParenToken {
                        // Restore and break - var/let is a property name
                        self.scanner.restore_state(snapshot);
                        self.current_token = saved_token;
                        break;
                    }

                    // If followed by line break, ASI makes it a property name (valid)
                    if self.scanner.has_preceding_line_break() {
                        // Restore and break - var/let is a property name
                        self.scanner.restore_state(snapshot);
                        self.current_token = saved_token;
                        break;
                    }

                    // If followed by semicolon, comma, equals, or closing brace, it's a property name (valid)
                    // Examples: var; | var, | var = | var }
                    if matches!(
                        self.current_token,
                        SyntaxKind::SemicolonToken
                            | SyntaxKind::CommaToken
                            | SyntaxKind::EqualsToken
                            | SyntaxKind::CloseBraceToken
                    ) {
                        // Restore and break - var/let is a property name
                        self.scanner.restore_state(snapshot);
                        self.current_token = saved_token;
                        break;
                    }

                    // Otherwise it's being used as a modifier (invalid)
                    // Restore state to emit error at var/let position, then consume it
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;

                    // Check if followed by 'constructor' - emit TS1068 instead of TS1440
                    let is_followed_by_constructor = if self.current_token == SyntaxKind::VarKeyword
                        || self.current_token == SyntaxKind::LetKeyword
                    {
                        let snapshot2 = self.scanner.save_state();
                        let saved_token2 = self.current_token;
                        self.next_token();
                        let result = self.current_token == SyntaxKind::ConstructorKeyword;
                        self.scanner.restore_state(snapshot2);
                        self.current_token = saved_token2;
                        result
                    } else {
                        false
                    };

                    if is_followed_by_constructor {
                        self.parse_error_at_current_token(
                            "Unexpected token. A constructor, method, accessor, or property was expected.",
                            diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                        );
                    } else {
                        self.parse_error_at_current_token(
                            "Variable declaration not allowed at this location.",
                            diagnostic_codes::VARIABLE_DECLARATION_NOT_ALLOWED_AT_THIS_LOCATION,
                        );
                    }
                    // Consume var/let and add to modifiers list
                    // This prevents parse_constructor_with_modifiers from being called
                    let var_token = self.token();
                    self.next_token();

                    // Add var/let to modifiers and return early
                    // Don't continue parsing modifiers (e.g., don't process 'export' in 'var export foo')
                    let var_modifier = self.arena.create_modifier(var_token, start_pos);
                    modifiers.push(var_modifier);
                    return Some(self.make_node_list(modifiers));
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

    pub(crate) fn should_stop_class_member_modifier(&mut self) -> bool {
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
        let has_line_break = self.scanner.has_preceding_line_break();
        self.scanner.restore_state(snapshot);
        self.current_token = current;

        // ASI: if the next token is on a new line, treat the keyword as a property name
        if has_line_break {
            return true;
        }

        matches!(
            next,
            SyntaxKind::OpenParenToken
                | SyntaxKind::LessThanToken
                | SyntaxKind::QuestionToken
                | SyntaxKind::ExclamationToken
                | SyntaxKind::ColonToken
                | SyntaxKind::EqualsToken
                | SyntaxKind::SemicolonToken
                // When followed by } or EOF, treat the keyword as a property name, not a modifier
                // This allows patterns like: class C { public }
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::EndOfFileToken
        )
    }

    /// Parse constructor with modifiers
    pub(crate) fn parse_constructor_with_modifiers(
        &mut self,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ConstructorKeyword);

        // Check for type parameters on constructor (invalid but parse for better error reporting)
        let type_parameters = self.is_token(SyntaxKind::LessThanToken).then(|| {
            self.parse_error_at_current_token(
                "Type parameters cannot appear on a constructor declaration.",
                diagnostic_codes::TYPE_PARAMETERS_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
            );
            self.parse_type_parameters()
        });

        self.parse_expected(SyntaxKind::OpenParenToken);
        let saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS;
        let parameters = self.parse_parameter_list();
        self.context_flags = saved_flags;
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Recovery: Handle return type annotation on constructor (invalid but users write it)
        if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_error_at_current_token(
                "Constructor cannot have a return type annotation.",
                diagnostic_codes::TYPE_ANNOTATION_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
            );
            // Consume the type annotation for recovery
            let _ = self.parse_type();
        }

        // Push a new label scope for the constructor body
        self.push_label_scope();
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };
        self.pop_label_scope();

        let end_pos = self.token_end();
        self.arena.add_constructor(
            syntax_kind_ext::CONSTRUCTOR,
            start_pos,
            end_pos,
            crate::parser::node::ConstructorData {
                modifiers,
                type_parameters,
                parameters,
                body,
            },
        )
    }

    /// Parse get accessor with modifiers: static get `foo()` { }
    pub(crate) fn parse_get_accessor_with_modifiers(
        &mut self,
        modifiers: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::GetKeyword);

        let name = self.parse_property_name();

        let type_parameters = self.is_token(SyntaxKind::LessThanToken).then(|| {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "An accessor cannot have type parameters.",
                diagnostic_codes::AN_ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS,
            );
            self.parse_type_parameters()
        });

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if self.is_token(SyntaxKind::CloseParenToken) {
            self.make_node_list(vec![])
        } else {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "A 'get' accessor cannot have parameters.",
                diagnostic_codes::A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS,
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

        // Parse body (may be empty for ambient declarations or abstract accessors)
        self.push_label_scope();
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            // Accessors must have a body unless in an ambient context or if abstract
            let has_abstract = modifiers.as_ref().is_some_and(|mods| {
                mods.nodes.iter().any(|&idx| {
                    self.arena
                        .nodes
                        .get(idx.0 as usize)
                        .is_some_and(|node| node.kind == SyntaxKind::AbstractKeyword as u16)
                })
            });

            if (self.context_flags & CONTEXT_FLAG_AMBIENT) == 0 && !has_abstract {
                self.error_token_expected("{");
            }
            self.parse_semicolon();
            NodeIndex::NONE
        };
        self.pop_label_scope();

        let end_pos = self.token_end();
        self.arena.add_accessor(
            syntax_kind_ext::GET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::node::AccessorData {
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
    pub(crate) fn parse_set_accessor_with_modifiers(
        &mut self,
        modifiers: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::SetKeyword);

        let name = self.parse_property_name();

        let type_parameters = self.is_token(SyntaxKind::LessThanToken).then(|| {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "An accessor cannot have type parameters.",
                diagnostic_codes::AN_ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS,
            );
            self.parse_type_parameters()
        });

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if self.is_token(SyntaxKind::CloseParenToken) {
            self.make_node_list(vec![])
        } else {
            self.parse_parameter_list()
        };
        self.parse_expected(SyntaxKind::CloseParenToken);

        if parameters.len() != 1 {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "A 'set' accessor must have exactly one parameter.",
                diagnostic_codes::A_SET_ACCESSOR_MUST_HAVE_EXACTLY_ONE_PARAMETER,
            );
        }

        if self.parse_optional(SyntaxKind::ColonToken) {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "A 'set' accessor cannot have a return type annotation.",
                diagnostic_codes::A_SET_ACCESSOR_CANNOT_HAVE_A_RETURN_TYPE_ANNOTATION,
            );
            let _ = self.parse_type();
        }

        // Parse body (may be empty for ambient declarations or abstract accessors)
        self.push_label_scope();
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            // Accessors must have a body unless in an ambient context or if abstract
            let has_abstract = modifiers.as_ref().is_some_and(|mods| {
                mods.nodes.iter().any(|&idx| {
                    self.arena
                        .nodes
                        .get(idx.0 as usize)
                        .is_some_and(|node| node.kind == SyntaxKind::AbstractKeyword as u16)
                })
            });

            if (self.context_flags & CONTEXT_FLAG_AMBIENT) == 0 && !has_abstract {
                self.error_token_expected("{");
            }
            self.parse_semicolon();
            NodeIndex::NONE
        };
        self.pop_label_scope();

        let end_pos = self.token_end();
        self.arena.add_accessor(
            syntax_kind_ext::SET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::node::AccessorData {
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
    pub(crate) fn parse_class_members(&mut self) -> NodeList {
        let mut members = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let member = self.parse_class_member();
            if !member.is_none() {
                self.parse_optional(SyntaxKind::SemicolonToken);
                members.push(member);
            }
        }

        self.make_node_list(members)
    }

    /// Parse a single class member
    pub(crate) fn parse_class_member(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;
        let start_pos = self.token_pos();

        // Handle empty statement (semicolon) in class body - this is valid TypeScript/JavaScript
        // A standalone semicolon in a class body is a SemicolonClassElement
        if self.is_token(SyntaxKind::SemicolonToken) {
            let end_pos = self.token_end();
            self.next_token();
            return self.arena.add_token(
                syntax_kind_ext::SEMICOLON_CLASS_ELEMENT,
                start_pos,
                end_pos,
            );
        }

        // Note: Reserved keywords like `if`, `for`, `delete`, `function`, etc. are valid
        // property names in class bodies (e.g., `class C { delete; for; if() {} }`).
        // We do NOT reject them here — they flow through to normal class member parsing
        // where is_property_name() correctly accepts them.

        // Parse decorators if present
        let decorators = self.parse_decorators();
        let has_decorators = decorators.is_some();

        // If decorators were found before a static block, emit TS1206
        if decorators.is_some()
            && self.is_token(SyntaxKind::StaticKeyword)
            && self.look_ahead_is_static_block()
        {
            self.parse_error_at_current_token(
                "Decorators are not valid here.",
                diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
            );
            return self.parse_static_block();
        }

        // Handle static block: static { ... }
        if self.is_token(SyntaxKind::StaticKeyword) && self.look_ahead_is_static_block() {
            return self.parse_static_block();
        }

        // Parse modifiers (static, public, private, protected, readonly, abstract, override)
        let parsed_modifiers = self.parse_class_member_modifiers();

        // Combine decorators and modifiers into a single modifiers list
        // TypeScript stores decorators as part of the modifiers array
        let modifiers = match (decorators, parsed_modifiers) {
            (Some(dec), Some(mods)) => {
                // Combine: decorators come first, then regular modifiers
                let mut combined = dec.nodes;
                combined.extend(mods.nodes);
                Some(crate::parser::NodeList {
                    nodes: combined,
                    pos: dec.pos,
                    end: mods.end,
                    has_trailing_comma: false,
                })
            }
            (Some(dec), None) => Some(dec),
            (None, Some(mods)) => Some(mods),
            (None, None) => None,
        };

        // Handle static block after modifiers: { ... }
        if self.is_token(SyntaxKind::StaticKeyword) && self.look_ahead_is_static_block() {
            if modifiers.is_some() {
                self.parse_error_at_current_token(
                    "Modifiers cannot appear on a static block.",
                    diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                );
            }
            return self.parse_static_block();
        }

        // Handle constructor
        // But not if var/let is in modifiers - that's an invalid pattern
        let has_var_let_modifier = modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&idx| {
                self.arena.nodes.get(idx.0 as usize).is_some_and(|node| {
                    node.kind == SyntaxKind::VarKeyword as u16
                        || node.kind == SyntaxKind::LetKeyword as u16
                })
            })
        });

        let has_static_modifier = modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&idx| {
                self.arena
                    .nodes
                    .get(idx.0 as usize)
                    .is_some_and(|node| node.kind == SyntaxKind::StaticKeyword as u16)
            })
        });

        let has_export_modifier = modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&idx| {
                self.arena
                    .nodes
                    .get(idx.0 as usize)
                    .is_some_and(|node| node.kind == SyntaxKind::ExportKeyword as u16)
            })
        });

        let has_declare_modifier = modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&idx| {
                self.arena
                    .nodes
                    .get(idx.0 as usize)
                    .is_some_and(|node| node.kind == SyntaxKind::DeclareKeyword as u16)
            })
        });

        if self.is_token(SyntaxKind::ConstructorKeyword) && !has_var_let_modifier {
            // TS1206: Decorators are not valid on constructors
            if has_decorators {
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
            }

            use tsz_common::diagnostics::diagnostic_codes;

            if has_static_modifier {
                self.parse_error_at_current_token(
                    "'static' modifier cannot appear on a constructor declaration.",
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
                );
            }

            if has_export_modifier {
                self.parse_error_at_current_token(
                    "'export' modifier cannot appear on class elements of this kind.",
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND,
                );
            } else if has_declare_modifier {
                self.parse_error_at_current_token(
                    "'declare' modifier cannot appear on class elements of this kind.",
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND,
                );
            }

            return self.parse_constructor_with_modifiers(modifiers);
        }

        // Handle generator methods: *foo() or async *#bar()
        let asterisk_token = self.parse_optional(SyntaxKind::AsteriskToken);

        // Handle get accessor: get foo() { }
        if !asterisk_token && self.is_token(SyntaxKind::GetKeyword) && self.look_ahead_is_accessor()
        {
            let saved_member_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_CLASS_MEMBER_NAME;
            let accessor = self.parse_get_accessor_with_modifiers(modifiers, start_pos);
            self.context_flags = saved_member_flags;
            return accessor;
        }

        // Handle set accessor: set foo(value) { }
        if !asterisk_token && self.is_token(SyntaxKind::SetKeyword) && self.look_ahead_is_accessor()
        {
            let saved_member_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_CLASS_MEMBER_NAME;
            let accessor = self.parse_set_accessor_with_modifiers(modifiers, start_pos);
            self.context_flags = saved_member_flags;
            return accessor;
        }

        // Handle index signatures: [key: Type]: ValueType
        if self.is_token(SyntaxKind::OpenBracketToken) && self.look_ahead_is_index_signature() {
            return self.parse_index_signature_with_modifiers(modifiers, start_pos);
        }

        // Recovery: Handle 'function' keyword used as a modifier in class members
        // `function foo() {}` is invalid in a class (the `function` keyword is not a modifier).
        // But `function;` or `function(){}` are valid property/method names.
        // Only consume `function` as a modifier when followed by an identifier on the same line.
        if self.is_token(SyntaxKind::FunctionKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            let next_is_identifier =
                self.is_identifier_or_keyword() && !self.scanner.has_preceding_line_break();
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if next_is_identifier {
                // `function foo(){}` — consume `function` and let it parse as a method
                self.next_token();
            }
            // Otherwise, `function` will be parsed as a property/method name below
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
            let has_line_break = self.scanner.has_preceding_line_break();
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            // If followed by `(`, it's a method name (e.g., `const() {}`), which is valid
            // If followed by `;`, `}`, `=`, `!`, `?`, or newline (ASI), treat as property name
            // If followed by identifier ON THE SAME LINE, it's being used as a modifier (invalid: `const x = 1`)
            // If there's a line break before the next token, ASI applies and the keyword is a property name
            if !has_line_break
                && matches!(
                    next_token,
                    SyntaxKind::Identifier
                        | SyntaxKind::PrivateIdentifier
                        | SyntaxKind::OpenBracketToken
                )
            {
                // This is likely being used as a modifier, emit error and recover
                self.parse_error_at_current_token(
                    "A class member cannot have the 'const', 'let', or 'var' keyword.",
                    diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                );
                // Consume the invalid keyword and continue parsing
                // The next identifier will be treated as the property/method name
                self.next_token();
            }
        }

        // Whether this is an async method; needed while parsing parameters.
        let is_async = modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&idx| {
                self.arena
                    .nodes
                    .get(idx.0 as usize)
                    .is_some_and(|node| node.kind == SyntaxKind::AsyncKeyword as u16)
            })
        });

        // Handle methods and properties
        // For now, just parse name and check for ( for methods
        // Note: Many reserved keywords can be used as property names (const, class, etc.)
        let name_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_CLASS_MEMBER_NAME;
        if is_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }
        if asterisk_token {
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
        }
        let name = if self.is_property_name() {
            self.parse_property_name()
        } else if asterisk_token {
            // After asterisk (*), we expect an identifier (method name).
            // Create a missing identifier and continue parsing the method
            // body so we don't produce cascading TS1068/TS1128 errors.
            self.error_identifier_expected();
            let pos = self.token_pos();
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                pos,
                pos,
                node::IdentifierData {
                    atom: Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            )
        } else {
            self.parse_error_at_current_token(
                "Unexpected token. A constructor, method, accessor, or property was expected.",
                diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
            );
            self.context_flags = name_saved_flags;
            self.next_token();
            return NodeIndex::NONE;
        };
        self.context_flags = name_saved_flags;

        // TS18012: '#constructor' is a reserved word
        if let Some(name_node) = self.arena.get(name)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            && let Some(ident) = self.arena.get_identifier(name_node)
            && ident.escaped_text == "#constructor"
        {
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "'#constructor' is a reserved word.",
                diagnostic_codes::CONSTRUCTOR_IS_A_RESERVED_WORD,
            );
        }

        // Parse optional ? or ! after property name
        let question_token = self.parse_optional(SyntaxKind::QuestionToken);
        let exclamation_token = if question_token {
            false
        } else {
            self.parse_optional(SyntaxKind::ExclamationToken)
        };
        let method_saved_flags = self.context_flags;
        if is_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }
        if asterisk_token {
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
        }

        // Check if it's a method or property.
        // Method: foo() or foo<T>().
        // `async *` members always require a member body/parameter list form, so treat
        // asterisk forms as methods even when '(' is missing (for recovery).
        let is_method_like = !has_var_let_modifier
            && (asterisk_token
                || self.is_token(SyntaxKind::OpenParenToken)
                || self.is_token(SyntaxKind::LessThanToken));

        if is_method_like {
            // Parse optional type parameters: foo<T, U>()
            let type_parameters = self
                .is_token(SyntaxKind::LessThanToken)
                .then(|| self.parse_type_parameters());

            // Method
            let has_open_paren = self.parse_optional(SyntaxKind::OpenParenToken);
            let parameters = if has_open_paren {
                let parameters = self.parse_parameter_list();
                self.parse_expected(SyntaxKind::CloseParenToken);
                parameters
            } else if asterisk_token {
                // `async *` members must be methods. Missing `(` here should emit one
                // TS1005 and recover without producing a declaration node, so we avoid
                // downstream errors like TS2391 on malformed members.
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'(' expected.", diagnostic_codes::EXPECTED);
                self.recover_from_missing_method_open_paren();
                self.context_flags = method_saved_flags;
                return NodeIndex::NONE;
            } else {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'(' expected.", diagnostic_codes::EXPECTED);
                self.recover_from_missing_method_open_paren();
                self.make_node_list(vec![])
            };

            // Optional return type (supports type predicates: param is T)
            let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_return_type()
            } else {
                NodeIndex::NONE
            };

            // Parse body
            self.push_label_scope();
            let body = if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_block()
            } else {
                NodeIndex::NONE
            };
            self.pop_label_scope();

            self.context_flags = method_saved_flags;

            let end_pos = self.token_end();
            self.arena.add_method_decl(
                syntax_kind_ext::METHOD_DECLARATION,
                start_pos,
                end_pos,
                crate::parser::node::MethodDeclData {
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
        } else if has_var_let_modifier
            && (self.is_token(SyntaxKind::OpenParenToken)
                || self.is_token(SyntaxKind::LessThanToken))
        {
            // var/let modifier followed by () - emit errors and attempt recovery
            use tsz_common::diagnostics::diagnostic_codes;

            // Emit error for '('
            if self.is_token(SyntaxKind::OpenParenToken) {
                self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
                // Consume '(' for recovery
                self.next_token();

                // Parse parameters (may be empty)
                let _ = self.parse_parameter_list();

                // Consume ')' without emitting an error
                self.parse_expected(SyntaxKind::CloseParenToken);
            }

            // Skip optional type parameters and return type for recovery
            if self.is_token(SyntaxKind::LessThanToken) {
                let _ = self.parse_type_parameters();
            }
            if self.parse_optional(SyntaxKind::ColonToken) {
                let _ = self.parse_return_type();
            }

            // Emit error for '{' - "'=>' expected"
            if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_error_at_current_token("'=>' expected.", diagnostic_codes::EXPECTED);
                self.next_token(); // Consume '{'
            }

            // Parse a statement to balance braces
            // This consumes '{ }' so the class members loop doesn't see them
            self.context_flags = method_saved_flags;
            let _ = self.parse_statement();

            // Return NONE to indicate this is not a valid member
            NodeIndex::NONE
        } else {
            // Property - parse optional type and initializer
            self.context_flags = method_saved_flags;
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
                crate::parser::node::PropertyDeclData {
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
    pub(crate) fn look_ahead_is_accessor(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip 'get' or 'set'
        self.next_token();

        // Note: line breaks after get/set do NOT prevent accessor parsing.
        // The ECMAScript grammar has no [no LineTerminator here] restriction
        // for get/set in class method definitions.

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
    pub(crate) fn look_ahead_is_static_block(&mut self) -> bool {
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
    pub(crate) fn parse_static_block(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Consume 'static'
        self.parse_expected(SyntaxKind::StaticKeyword);

        // Parse the block body with static block context (where 'await' is reserved)
        // IMPORTANT: Static blocks create a fresh execution context - they do NOT inherit
        // async/generator context from enclosing functions. Clear those flags.
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let saved_flags = self.context_flags;
        // Clear async/generator flags and set static block flag
        self.context_flags &= !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR);
        self.context_flags |= CONTEXT_FLAG_STATIC_BLOCK;
        let statements = self.parse_statements();
        self.context_flags = saved_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        self.arena.add_block(
            syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::node::BlockData {
                statements,
                multi_line: true,
            },
        )
    }

    /// Look ahead to see if this is an index signature: [key: Type]: `ValueType`
    /// vs a computed property: [expr]: Type or [computed]()
    ///
    /// Matches tsc's `isUnambiguouslyIndexSignature`. Recognizes:
    ///   [id:    [id,    [id?:    [id?,    [id?]    [...    [modifier id
    pub(crate) fn look_ahead_is_index_signature(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip '['
        self.next_token();

        // `[...` — unambiguously index signature (malformed rest param).
        // Note: we do NOT match `[]` (CloseBracketToken) here because `[]` is used
        // for empty tuple types in type contexts (e.g., `unknown[] | []`).
        let is_index_sig = if self.is_token(SyntaxKind::DotDotDotToken) {
            true
        } else if !self.is_identifier_or_keyword() {
            false
        } else {
            self.next_token();
            if self.is_token(SyntaxKind::ColonToken) || self.is_token(SyntaxKind::CommaToken) {
                // `[id:` or `[id,`
                true
            } else if self.is_token(SyntaxKind::QuestionToken) {
                // `[id?` — check what follows: `:`, `,`, or `]` means index signature
                self.next_token();
                self.is_token(SyntaxKind::ColonToken)
                    || self.is_token(SyntaxKind::CommaToken)
                    || self.is_token(SyntaxKind::CloseBracketToken)
            } else {
                false
            }
        };

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_index_sig
    }

    /// Check if this is `[]` — an empty index signature (malformed, no parameters).
    /// Used in type member contexts where `[]` should be an empty index signature,
    /// NOT in type suffix contexts where `[]` is an array type.
    pub(crate) fn look_ahead_is_empty_index_signature(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip `[`
        let is_empty = self.is_token(SyntaxKind::CloseBracketToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_empty
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::{NodeIndex, ParserState};

    fn parse_source(source: &str) -> (ParserState, NodeIndex) {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        (parser, root)
    }

    #[test]
    fn parse_statement_recovery_on_malformed_top_level_diagnostics() {
        let (parser, root) = parse_source("const x = 1\nconst y = ;\nconst z = 3;");
        let sf = parser.get_arena().get_source_file_at(root).unwrap();
        assert!(sf.statements.nodes.len() >= 2);
        assert!(!parser.get_diagnostics().is_empty());
    }

    #[test]
    fn parse_static_block_statement_is_supported() {
        let (parser, root) = parse_source(
            "class Holder {\n    static {\n        const v = 1;\n    }\n}\nconst ok = 1;",
        );
        assert_eq!(parser.get_diagnostics().len(), 0);
        let sf = parser.get_arena().get_source_file_at(root).unwrap();
        assert_eq!(sf.statements.nodes.len(), 2);
    }

    #[test]
    fn parse_with_statement_with_recovery_when_expression_missing() {
        let (parser, _root) = parse_source("with () {}\nconst ok = 1;");
        assert!(!parser.get_diagnostics().is_empty());
    }

    #[test]
    fn parse_template_recovery_preserves_follow_up_statement() {
        let (parser, root) = parse_source("const bad = `head${1 + 2`;\nconst ok = 1;");
        let sf = parser.get_arena().get_source_file_at(root).unwrap();

        assert!(!sf.statements.nodes.is_empty());
        assert!(!parser.get_diagnostics().is_empty() || !sf.statements.nodes.is_empty());
    }

    #[test]
    fn parse_return_statement_outside_function_recovers_and_continues() {
        let (parser, root) = parse_source("return;\nconst ok = 1;");
        let sf = parser.get_arena().get_source_file_at(root).unwrap();

        assert!(!sf.statements.nodes.is_empty());
    }

    #[test]
    fn parse_index_signature_optional_param_emits_ts1019() {
        let (parser, _root) = parse_source("interface Foo { [p2?: string]; }");
        let diags = parser.get_diagnostics();
        let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        // Should emit TS1019 (optional param in index sig), NOT TS1109 (expression expected)
        assert!(
            codes.contains(&1019),
            "Expected TS1019, got codes: {codes:?}"
        );
        assert!(
            !codes.contains(&1109),
            "Should NOT emit TS1109, got codes: {codes:?}"
        );
    }

    #[test]
    fn parse_index_signature_rest_param_emits_ts1017() {
        let (parser, _root) = parse_source("interface Foo { [...p3: any[]]; }");
        let diags = parser.get_diagnostics();
        let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&1017),
            "Expected TS1017, got codes: {codes:?}"
        );
        assert!(
            !codes.contains(&1109),
            "Should NOT emit TS1109, got codes: {codes:?}"
        );
    }
}
