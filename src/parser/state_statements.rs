//! Parser state - statement and declaration parsing methods

use super::state::{
    CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_GENERATOR, CONTEXT_FLAG_PARAMETER_DEFAULT,
    CONTEXT_FLAG_STATIC_BLOCK, IncrementalParseResult, ParserState,
};
use crate::parser::{NodeIndex, NodeList, node::*, parse_rules::*, syntax_kind_ext};
use crate::scanner::SyntaxKind;

impl ParserState {
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
    pub(crate) fn parse_source_file_statements(&mut self) -> NodeList {
        let mut statements = Vec::new();

        while !self.is_token(SyntaxKind::EndOfFileToken) {
            let pos_before = self.token_pos();

            // Handle Unknown tokens (invalid characters) - must be checked FIRST
            if self.is_token(SyntaxKind::Unknown) {
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Invalid character.",
                    diagnostic_codes::INVALID_CHARACTER,
                );
                self.next_token();
                continue;
            }

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
                // Also suppress cascading errors when a recent error was within 3 chars
                let current = self.token_pos();
                if (self.last_error_pos == 0 || current.abs_diff(self.last_error_pos) > 3)
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
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Invalid character.",
                    diagnostic_codes::INVALID_CHARACTER,
                );
                self.next_token();
                continue;
            }

            let stmt = self.parse_statement();
            if !stmt.is_none() {
                statements.push(stmt);
            } else {
                // Statement parsing failed, resync to recover
                // Emit error for unexpected token if we haven't already
                // Also suppress cascading errors when a recent error was within 3 chars
                let current = self.token_pos();
                if (self.last_error_pos == 0 || current.abs_diff(self.last_error_pos) > 3)
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
            SyntaxKind::AsyncKeyword => {
                // async function declaration or async arrow expression statement
                // Look ahead to see if it's "async function"
                if self.look_ahead_is_async_function() {
                    self.parse_async_function_declaration()
                } else if self.look_ahead_is_async_declaration() {
                    let start_pos = self.token_pos();
                    // TS1042 is reported by the checker (checkGrammarModifiers), not the parser
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
    pub(crate) fn parse_import_equals_declaration(&mut self) -> NodeIndex {
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
        let flags: u16 = match self.token() {
            SyntaxKind::LetKeyword => {
                self.next_token();
                node_flags::LET as u16
            }
            SyntaxKind::ConstKeyword => {
                self.next_token();
                node_flags::CONST as u16
            }
            SyntaxKind::UsingKeyword => {
                self.next_token();
                node_flags::USING as u16
            }
            SyntaxKind::AwaitKeyword => {
                // await using declaration
                self.next_token(); // consume 'await'
                self.parse_expected(SyntaxKind::UsingKeyword); // consume 'using'
                node_flags::AWAIT_USING as u16
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

            let decl = self.parse_variable_declaration_with_flags(flags);
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
    pub(crate) fn parse_variable_declaration(&mut self) -> NodeIndex {
        self.parse_variable_declaration_with_flags(0)
    }

    /// Parse variable declaration with declaration flags (for using/await using checks)
    pub(crate) fn parse_variable_declaration_with_flags(&mut self, flags: u16) -> NodeIndex {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::parser::node_flags;

        let start_pos = self.token_pos();

        // Check if this is a 'using' or 'await using' declaration.
        // Only check the USING bit (bit 2). AWAIT_USING = CONST | USING = 6,
        // so checking USING bit matches both USING (4) and AWAIT_USING (6)
        // but NOT CONST (2) which only has bit 1 set.
        let is_using = (flags & node_flags::USING as u16) != 0;

        // Check TS1375: 'using' declarations do not support destructuring patterns
        if is_using {
            if self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::OpenBracketToken)
            {
                self.parse_error_at_current_token(
                    diagnostic_messages::USING_DECLARATIONS_DO_NOT_SUPPORT_DESTRUCTURING,
                    diagnostic_codes::USING_DECLARATIONS_DO_NOT_SUPPORT_DESTRUCTURING,
                );
            }
        }

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
    pub(crate) fn parse_function_declaration(&mut self) -> NodeIndex {
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

    /// Parse function declaration for export default context (name is optional).
    /// Unlike regular function declarations, `export default function() {}` allows anonymous functions.
    /// Unlike function expressions, this creates a FUNCTION_DECLARATION node and supports
    /// overload signatures (missing body).
    pub(crate) fn parse_function_declaration_with_async_optional_name(
        &mut self,
        is_async: bool,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        let start_pos = self.token_pos();

        let is_async = is_async || self.parse_optional(SyntaxKind::AsyncKeyword);
        self.parse_expected(SyntaxKind::FunctionKeyword);
        let asterisk_token = self.parse_optional(SyntaxKind::AsteriskToken);

        // Name is optional for export default function declarations
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

        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            self.parse_optional(SyntaxKind::SemicolonToken);
            NodeIndex::NONE
        };

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
    pub(crate) fn parse_function_expression(&mut self) -> NodeIndex {
        self.parse_function_expression_with_async(false)
    }

    /// Parse async function expression: async function() {} or async function name() {}
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

        while !self.is_token(SyntaxKind::CloseParenToken) {
            let param = self.parse_parameter();
            params.push(param);

            if !self.parse_optional(SyntaxKind::CommaToken) {
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

    /// Check if current token is a parameter modifier
    pub(crate) fn is_parameter_modifier(&self) -> bool {
        matches!(
            self.current_token,
            SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::ReadonlyKeyword
        )
    }

    /// Parse parameter modifiers (public, private, protected, readonly)
    pub(crate) fn parse_parameter_modifiers(&mut self) -> Option<NodeList> {
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
    pub(crate) fn parse_class_declaration_with_modifiers(
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
    pub(crate) fn parse_decorated_declaration(&mut self) -> NodeIndex {
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
        self.next_token(); // consume @

        // Parse the decorator expression (identifier, member access, or call)
        let expression = self.parse_left_hand_side_expression();

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
    pub(crate) fn parse_heritage_clauses(&mut self) -> Option<NodeList> {
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
                        crate::parser::node::HeritageData {
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
                        crate::parser::node::HeritageData {
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
        use crate::checker::types::diagnostics::diagnostic_codes;

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
                    crate::parser::node::AccessExprData {
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
                    crate::parser::node::AccessExprData {
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
                        crate::parser::node::CallExprData {
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
                        crate::parser::node::ExprWithTypeArgsData {
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
                    crate::parser::node::CallExprData {
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
    pub(crate) fn parse_class_member_modifiers(&mut self) -> Option<NodeList> {
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
    pub(crate) fn parse_constructor_with_modifiers(
        &mut self,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
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
            crate::parser::node::ConstructorData {
                modifiers,
                type_parameters: None,
                parameters,
                body,
            },
        )
    }

    /// Parse get accessor with modifiers: static get foo() { }
    pub(crate) fn parse_get_accessor_with_modifiers(
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
                members.push(member);
            }

            // Handle semicolons
            self.parse_optional(SyntaxKind::SemicolonToken);
        }

        self.make_node_list(members)
    }

    /// Parse a single class member
    pub(crate) fn parse_class_member(&mut self) -> NodeIndex {
        use crate::checker::types::diagnostics::diagnostic_codes;
        let start_pos = self.token_pos();

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
            // If followed by `;`, `}`, `=`, `!`, `?`, or newline (ASI), treat as property name
            // If followed by identifier, it's being used as a modifier (invalid: `const x = 1`)
            if matches!(
                next_token,
                SyntaxKind::Identifier
                    | SyntaxKind::PrivateIdentifier
                    | SyntaxKind::OpenBracketToken
            ) {
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
            let is_async = modifiers.as_ref().is_some_and(|mods| {
                mods.nodes.iter().any(|&idx| {
                    self.arena
                        .nodes
                        .get(idx.0 as usize)
                        .is_some_and(|node| node.kind == SyntaxKind::AsyncKeyword as u16)
                })
            });

            // Set context flags for async/generator method body
            let saved_flags = self.context_flags;
            if is_async {
                self.context_flags |= CONTEXT_FLAG_ASYNC;
            }
            if asterisk_token {
                self.context_flags |= CONTEXT_FLAG_GENERATOR;
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

        // If there's a line break after get/set, it's treated as a property name
        // (shorthand property in class), not as an accessor keyword.
        // This matches TypeScript's ASI behavior.
        if self.scanner.has_preceding_line_break() {
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return false;
        }

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

    /// Look ahead to see if this is an index signature: [key: Type]: ValueType
    /// vs a computed property: [expr]: Type or [computed]()
    pub(crate) fn look_ahead_is_index_signature(&mut self) -> bool {
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
}
