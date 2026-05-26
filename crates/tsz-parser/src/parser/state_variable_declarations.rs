use super::state::*;
use crate::parser::node::*;
use crate::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::interner::Atom;
use tsz_scanner::SyntaxKind;

impl ParserState {
    pub(crate) fn recover_jsx_closing_namespace_tail_greater_statement(&mut self) {
        let start_pos = self.token_pos();
        let left = self.create_missing_expression();
        self.next_token();
        let right = self.create_missing_expression();
        self.parse_error_at_current_token(
            "Expression expected.",
            diagnostic_codes::EXPRESSION_EXPECTED,
        );
        let expr = self.arena.add_binary_expr(
            syntax_kind_ext::BINARY_EXPRESSION,
            start_pos,
            self.token_pos(),
            BinaryExprData {
                left,
                operator_token: SyntaxKind::GreaterThanToken as u16,
                right,
            },
        );
        let stmt = self.arena.add_expr_statement(
            syntax_kind_ext::EXPRESSION_STATEMENT,
            start_pos,
            self.token_full_start(),
            ExprStatementData { expression: expr },
        );
        self.pending_recovered_expression_statements.push(stmt);
    }

    pub(crate) fn parse_variable_declaration_with_flags_pre_checks(&mut self, flags: u16) {
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

    pub(crate) fn parse_variable_declaration_name(&mut self) -> NodeIndex {
        if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_object_binding_pattern()
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            self.parse_array_binding_pattern()
        } else if self.current_unknown_starts_invalid_unicode_identifier_debris() {
            self.parse_recovered_invalid_unicode_escape_identifier()
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

    pub(crate) fn parse_variable_declaration_initializer(&mut self) -> NodeIndex {
        if self.pending_array_binding_tail_recovery {
            return NodeIndex::NONE;
        }

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

    pub(crate) fn parse_variable_declaration_after_parse_checks(
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
        if self.pending_array_binding_tail_recovery {
            self.pending_array_binding_tail_recovery = false;
            if self.is_token(SyntaxKind::EqualsToken) {
                self.parse_error_at_current_token(
                    "Declaration or statement expected.",
                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                );
                self.next_token();
                while !matches!(
                    self.token(),
                    SyntaxKind::SemicolonToken
                        | SyntaxKind::CloseBraceToken
                        | SyntaxKind::EndOfFileToken
                ) {
                    self.next_token();
                }
            }
            return;
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

    pub(crate) fn parse_variable_declaration_end_pos(
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
        let reserved_word_function_name = self.is_reserved_word() && !is_yield_as_generator_name;
        let function_keyword_as_name =
            reserved_word_function_name && self.is_token(SyntaxKind::FunctionKeyword);
        let name = if reserved_word_function_name {
            let name_start = self.token_pos();
            let name_end = self.token_end();
            let atom = self.scanner.get_token_atom();
            let text = self.scanner.get_token_value_ref().to_string();
            self.error_reserved_word_identifier();
            // tsc emits TS1359 + TS1003 specifically when the function name is
            // itself the `function` keyword (e.g. `function function() {}`).
            // For other reserved words (e.g. `throw`, `while`) tsc keeps the
            // legacy "'=>' expected" recovery instead.
            if function_keyword_as_name {
                self.parse_error_at_current_token(
                    "Identifier expected.",
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

        // Keep STATIC_BLOCK during parameter parsing — inside a static block,
        // 'await' is reserved even in function parameters, matching tsc behavior.
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

        // For reserved-word function names other than `function` itself,
        // tsc emits TS1005 ("'=>' expected") when the body opens — matches
        // its arrow-recovery diagnostics for `function throw() {}` etc.
        if reserved_word_function_name
            && !function_keyword_as_name
            && self.is_token(SyntaxKind::OpenBraceToken)
        {
            self.parse_error_at_current_token("'=>' expected.", diagnostic_codes::EXPECTED);
        }

        // Parse optional return type (may be a type predicate: param is T)
        // Note: Type annotations are not in async/generator context
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };
        // Clear STATIC_BLOCK before body — the body is a new scope where
        // 'await' is a valid identifier (unless the function is async).
        self.context_flags &= !CONTEXT_FLAG_STATIC_BLOCK;
        self.context_flags |= CONTEXT_FLAG_FUNCTION_BODY;

        // Push a new label scope for the function body
        self.push_label_scope();
        let mut recovered_arrow_body = false;
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
            // TS1144: '{' or ';' expected — user wrote arrow syntax on a function declaration
            self.parse_error_at_current_token(
                "'{' or ';' expected.",
                diagnostic_codes::OR_EXPECTED,
            );
            // Skip past => and keep the expression for emit recovery.
            self.next_token();
            let expr = self.parse_expression();
            recovered_arrow_body = true;
            self.parse_optional(SyntaxKind::SemicolonToken);
            expr
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
                equals_greater_than_token: recovered_arrow_body,
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

        // Keep STATIC_BLOCK flag during parameter parsing — inside a static block,
        // 'await' is reserved even in function parameters, matching tsc behavior.
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
        self.context_flags |= CONTEXT_FLAG_FUNCTION_BODY;

        // Push a new label scope for the function body
        // Labels are function-scoped, so each function gets its own label namespace
        self.push_label_scope();

        let mut recovered_arrow_body = false;
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
            self.parse_error_at_current_token(
                "'{' or ';' expected.",
                diagnostic_codes::OR_EXPECTED,
            );
            self.next_token();
            let expr = self.parse_expression();
            recovered_arrow_body = true;
            self.parse_optional(SyntaxKind::SemicolonToken);
            expr
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
                equals_greater_than_token: recovered_arrow_body,
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

        // Set context flags BEFORE parsing parameters and body so that
        // reserved keywords (await/yield) are properly detected in parameters and body.
        // For async function * (await) {}, the parameter name 'await' should error.
        let saved_flags = self.context_flags;
        // Save whether we're in a static block before clearing the flag.
        // Inside static blocks, `await` as a function expression name is still
        // illegal (TS1359), even though in async/generator contexts it is not.
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
        self.context_flags |= CONTEXT_FLAG_FUNCTION_BODY;

        // Check for `await` used as function expression name inside static blocks.
        // tsc reports TS1359 for `(function await() {})` in static blocks because
        // `await` is always reserved there.
        // However, tsc does NOT emit TS1359/TS1212 for `await`/`yield` as function
        // expression names in async/generator contexts (e.g., `async function * await() {}`).
        // Those are handled by the checker, not the parser.
        if self.is_token(SyntaxKind::AwaitKeyword) && was_in_static_block {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Identifier expected. 'await' is a reserved word that cannot be used here.",
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
            );
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
