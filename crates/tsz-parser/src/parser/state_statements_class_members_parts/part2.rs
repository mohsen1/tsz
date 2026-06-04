impl ParserState {
    /// Construct a class member after modifiers have been scanned and classified.
    ///
    /// Dispatches to constructor, get/set accessor, index signature, mapped-type
    /// member, and ordinary method/property declaration paths.
    fn construct_class_member(
        &mut self,
        start_pos: u32,
        mods: ClassMemberModifierSet,
    ) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        // Handle constructor — but not when var/let is in modifiers (invalid pattern).
        if self.is_token(SyntaxKind::ConstructorKeyword) && !mods.has_var_let {
            // TS1206: Decorators are not valid on constructors.
            if mods.has_decorators {
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
            }

            if mods.has_static {
                self.emit_modifier_error_on_constructor(
                    &mods.modifiers,
                    SyntaxKind::StaticKeyword,
                    "'static' modifier cannot appear on a constructor declaration.",
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
                );
            }

            // TS1031: tsc anchors at the modifier keyword via grammarErrorOnNode(modifier)
            if mods.has_export {
                self.emit_modifier_error_on_constructor(
                    &mods.modifiers,
                    SyntaxKind::ExportKeyword,
                    "'export' modifier cannot appear on class elements of this kind.",
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND,
                );
            } else if mods.has_declare {
                self.emit_modifier_error_on_constructor(
                    &mods.modifiers,
                    SyntaxKind::DeclareKeyword,
                    "'declare' modifier cannot appear on class elements of this kind.",
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND,
                );
            }

            // TS1275: 'accessor' modifier can only appear on a property declaration.
            if mods.has_accessor {
                self.emit_accessor_modifier_only_on_property_error(&mods.modifiers);
            }

            return self.parse_constructor_with_modifiers(mods.modifiers);
        }

        // Handle generator methods: *foo() or async *#bar()
        let asterisk_token = self.parse_optional(SyntaxKind::AsteriskToken);

        // Handle get accessor: get foo() { }
        if !asterisk_token && self.is_token(SyntaxKind::GetKeyword) && self.look_ahead_is_accessor()
        {
            // TS1031: 'declare' modifier cannot appear on class elements of this kind
            if mods.has_declare {
                self.emit_declare_on_non_property_error(&mods.modifiers);
            }
            // TS1275: 'accessor' modifier can only appear on a property declaration.
            if mods.has_accessor {
                self.emit_accessor_modifier_only_on_property_error(&mods.modifiers);
            }
            let saved_member_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_CLASS_MEMBER_NAME;
            let accessor = self.parse_get_accessor_with_modifiers(mods.modifiers, start_pos);
            self.context_flags = saved_member_flags;
            return accessor;
        }

        // Handle set accessor: set foo(value) { }
        if !asterisk_token && self.is_token(SyntaxKind::SetKeyword) && self.look_ahead_is_accessor()
        {
            // TS1031: 'declare' modifier cannot appear on class elements of this kind
            if mods.has_declare {
                self.emit_declare_on_non_property_error(&mods.modifiers);
            }
            // TS1275: 'accessor' modifier can only appear on a property declaration.
            if mods.has_accessor {
                self.emit_accessor_modifier_only_on_property_error(&mods.modifiers);
            }
            let saved_member_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_CLASS_MEMBER_NAME;
            let accessor = self.parse_set_accessor_with_modifiers(mods.modifiers, start_pos);
            self.context_flags = saved_member_flags;
            return accessor;
        }

        // Handle index signatures: [key: Type]: ValueType
        if self.is_token(SyntaxKind::OpenBracketToken) && self.look_ahead_is_index_signature() {
            let sig = self.parse_index_signature_with_modifiers(mods.modifiers, start_pos);
            self.parse_semicolon();
            return sig;
        }

        // Handle mapped type member in class body: [P in K]: T (TS 4.1+)
        if self.is_token(SyntaxKind::OpenBracketToken) && self.look_ahead_is_mapped_type_start() {
            return self.parse_mapped_type_member();
        }

        // `function foo() {}` inside a class body is handled by
        // `look_ahead_is_class_body_function_statement` above and recovers
        // via `recover_invalid_module_like_class_member` to match tsc.
        // Keep `function` as a potential member name here for valid forms like
        // `function() {}` or `function;`.

        // `const x = 1` is invalid; `const() {}` is a valid method name. Trigger only when
        // an identifier/bracket follows on the same line (no ASI), indicating a modifier misuse.
        if matches!(
            self.token(),
            SyntaxKind::ConstKeyword | SyntaxKind::LetKeyword | SyntaxKind::VarKeyword
        ) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            let next_token = self.token();
            let has_line_break = self.scanner.has_preceding_line_break();
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if !has_line_break
                && matches!(
                    next_token,
                    SyntaxKind::Identifier
                        | SyntaxKind::PrivateIdentifier
                        | SyntaxKind::OpenBracketToken
                )
            {
                self.parse_error_at_current_token(
                    "A class member cannot have the 'const', 'let', or 'var' keyword.",
                    diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                );
                self.next_token();
            }
        }

        // `try { ... }` is not a valid class member even after modifiers like `public`. Emit
        // TS1068 rather than cascading into TS1434/TS1435. Unlike const/let/var, `try` is a
        // valid property name so we only emit the diagnostic and let it continue as a name.
        if self.is_token(SyntaxKind::TryKeyword) && self.look_ahead_is_try_block_same_line() {
            self.parse_error_at_current_token(
                "Unexpected token. A constructor, method, accessor, or property was expected.",
                diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
            );
        }

        // `class D {}` / `enum E {}` are invalid class members even after modifiers like `public`.
        // `class;` and `class(){}` are valid property/method names — only trigger when an
        // identifier follows on the same line (distinguishes declaration from member name).
        if matches!(
            self.token(),
            SyntaxKind::ClassKeyword | SyntaxKind::EnumKeyword
        ) && self.look_ahead_next_is_identifier_or_keyword_on_same_line()
        {
            self.parse_error_at_current_token(
                "Unexpected token. A constructor, method, accessor, or property was expected.",
                diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
            );
            self.next_token(); // skip class/enum keyword
            self.next_token(); // skip the identifier name
            // Skip extends/implements clauses (e.g., `class D extends E {`).
            while self.is_identifier_or_keyword()
                && !self.is_token(SyntaxKind::OpenBraceToken)
                && !self.is_token(SyntaxKind::CloseBraceToken)
                && !self.is_token(SyntaxKind::SemicolonToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                self.next_token();
                if self.is_token(SyntaxKind::CommaToken) {
                    self.next_token();
                }
            }
            if self.is_token(SyntaxKind::OpenBraceToken) {
                // DON'T consume the final `}` — leave it for the outer class body to use
                // as its closing brace (tsc error recovery behavior).
                let mut depth = 1u32;
                self.next_token(); // consume `{`
                while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
                    if self.is_token(SyntaxKind::OpenBraceToken) {
                        depth += 1;
                    } else if self.is_token(SyntaxKind::CloseBraceToken) {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    self.next_token();
                }
            }
            return NodeIndex::NONE;
        }

        // Parse member name
        let name_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_CLASS_MEMBER_NAME;
        // Note: Do NOT set CONTEXT_FLAG_GENERATOR or CONTEXT_FLAG_ASYNC here.
        // The yield/await context must only be active during the method body
        // (parameters + block), not during property name parsing.  Otherwise
        // `yield` inside a computed property name like `async * [yield]()`
        // would be parsed as a YieldExpression instead of an Identifier.
        // The generator/async flags are correctly set later (inside construct_class_member_method).
        // However, track the generator asterisk so we can suppress TS1213
        // for `yield` in computed property names of generator methods — tsc
        // does not emit TS1213 in this position.
        if asterisk_token {
            self.context_flags |= CONTEXT_FLAG_GENERATOR_MEMBER_NAME;
        }
        let has_modifiers = mods.modifiers.is_some();
        let name = if self.is_property_name() {
            self.parse_property_name()
        } else if has_modifiers
            && self.is_token(SyntaxKind::OpenBraceToken)
            && self.next_token_is_open_bracket()
        {
            let token_start = self.token_pos();
            let decl_pos = if token_start > 0 { token_start - 1 } else { 0 };
            self.parse_error_at(
                decl_pos,
                1,
                "Declaration expected.",
                diagnostic_codes::DECLARATION_EXPECTED,
            );
            self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            self.next_token();
            while !self.is_token(SyntaxKind::CloseBraceToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                let before = self.token_pos();
                let _ = self.parse_statement();
                if self.token_pos() == before {
                    self.next_token();
                }
            }
            self.context_flags = name_saved_flags;
            return NodeIndex::NONE;
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
            if has_modifiers {
                // TSC emits TS1146 at the position where the name was expected
                // (just before the current token) and TS1005 at the current token.
                // We must emit them at different positions so the dedup logic
                // in parse_error_at doesn't suppress the second one.
                let token_start = self.token_pos();
                let decl_pos = if token_start > 0 { token_start - 1 } else { 0 };
                self.parse_error_at(
                    decl_pos,
                    1,
                    "Declaration expected.",
                    diagnostic_codes::DECLARATION_EXPECTED,
                );
                self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            } else {
                self.parse_error_at_current_token(
                    "Unexpected token. A constructor, method, accessor, or property was expected.",
                    diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                );
            }
            self.context_flags = name_saved_flags;
            self.next_token();
            return NodeIndex::NONE;
        };
        self.context_flags = name_saved_flags;

        // TS18012: '#constructor' is a reserved word
        if let Some(name_node) = self.arena.get(name)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            && let Some(ident) = self.arena.get_identifier(name_node)
            && (ident.escaped_text == "constructor" || ident.escaped_text == "#constructor")
        {
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "'#constructor' is a reserved word.",
                diagnostic_codes::CONSTRUCTOR_IS_A_RESERVED_WORD,
            );
        }

        // Parse optional ? or ! after property name
        let question_token_pos = self.token_pos();
        let question_token_end = self.token_end();
        let question_token = self.parse_optional(SyntaxKind::QuestionToken);
        let exclamation_token = if question_token {
            false
        } else {
            self.parse_optional(SyntaxKind::ExclamationToken)
        };

        // TS1436: Decorator after property name (e.g., `private prop @decorator`).
        // Detect `@` after the member name where `:`, `=`, `;`, `(`, or `<` is expected.
        // Only when `@` is on the SAME line — if on a new line, ASI applies and the
        // property ends normally; the `@` starts a new decorated member.
        let late_property_name_decorator =
            self.is_token(SyntaxKind::AtToken) && !self.scanner.has_preceding_line_break();
        if late_property_name_decorator {
            self.parse_error_at_current_token(
                "Decorators must precede the name and all keywords of property declarations.",
                diagnostic_codes::DECORATORS_MUST_PRECEDE_THE_NAME_AND_ALL_KEYWORDS_OF_PROPERTY_DECLARATIONS,
            );
            // Keep the decorator token in the stream. TSC reports TS1436 for
            // the property, then recovers by applying the decorator to the
            // following class member.
        }

        let method_saved_flags = self.context_flags;
        self.context_flags &=
            !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR | CONTEXT_FLAG_STATIC_BLOCK);
        if mods.has_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }
        if asterisk_token {
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
        }
        self.context_flags |= CONTEXT_FLAG_FUNCTION_BODY;

        // Check if it's a method or property.
        // Method: foo() or foo<T>().
        // `async *` members always require a member body/parameter list form, so treat
        // asterisk forms as methods even when '(' is missing (for recovery).
        let is_method_like = !mods.has_var_let
            && (asterisk_token
                || self.is_token(SyntaxKind::OpenParenToken)
                || self.is_token(SyntaxKind::LessThanToken));

        // TS1276: An 'accessor' property cannot be declared optional.
        // tsc anchors at the `?` token for properties only; accessor methods
        // report TS1275 instead.
        if !is_method_like && question_token && mods.has_accessor {
            self.parse_error_at(
                question_token_pos,
                question_token_end - question_token_pos,
                "An 'accessor' property cannot be declared optional.",
                diagnostic_codes::AN_ACCESSOR_PROPERTY_CANNOT_BE_DECLARED_OPTIONAL,
            );
        }

        if is_method_like {
            self.construct_class_member_method(
                start_pos,
                mods,
                asterisk_token,
                name,
                question_token,
                method_saved_flags,
            )
        } else if mods.has_var_let
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
            self.construct_class_member_property(
                start_pos,
                mods,
                name,
                question_token,
                exclamation_token,
                late_property_name_decorator,
                method_saved_flags,
            )
        }
    }

    /// Construct the body of a method class member: parse type params, parameter
    /// list, return-type annotation, and method body.
    fn construct_class_member_method(
        &mut self,
        start_pos: u32,
        mods: ClassMemberModifierSet,
        asterisk_token: bool,
        name: NodeIndex,
        question_token: bool,
        method_saved_flags: u32,
    ) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        // TS1031: 'declare' modifier cannot appear on class elements of this kind
        // (methods cannot be declared, only properties can)
        if mods.has_declare {
            self.emit_declare_on_non_property_error(&mods.modifiers);
        }
        // TS1275: 'accessor' modifier can only appear on a property declaration.
        if mods.has_accessor {
            self.emit_accessor_modifier_only_on_property_error(&mods.modifiers);
        }

        // Parse optional type parameters: foo<T, U>()
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        let has_open_paren = self.parse_optional(SyntaxKind::OpenParenToken);
        let mut body_already_consumed_by_recovery = false;
        let parameters = if has_open_paren {
            let saved_flags = self.context_flags;
            if self.class_member_name_is_if_keyword(name) {
                self.context_flags |=
                    crate::parser::state::CONTEXT_FLAG_RECOVERED_IF_CLASS_MEMBER_PARAMETERS;
            }
            let parameters = self.parse_parameter_list();
            self.context_flags = saved_flags;
            self.parse_expected(SyntaxKind::CloseParenToken);
            parameters
        } else if asterisk_token {
            // `async *` members must be methods. Missing `(` here should emit one
            // TS1005 and recover without producing a declaration node, so we avoid
            // downstream errors like TS2391 on malformed members.
            self.parse_error_at_current_token("'(' expected.", diagnostic_codes::EXPECTED);
            self.recover_from_missing_method_open_paren();
            self.context_flags = method_saved_flags;
            return NodeIndex::NONE;
        } else {
            self.parse_error_at_current_token("'(' expected.", diagnostic_codes::EXPECTED);
            body_already_consumed_by_recovery = self.recover_from_missing_method_open_paren();
            self.make_node_list(vec![])
        };

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };
        let recovered_if_comparison_tail =
            self.class_member_name_is_if_keyword(name) && self.current_token_is_comparison_tail();

        self.push_label_scope();
        let body = if body_already_consumed_by_recovery {
            NodeIndex::NONE
        } else if recovered_if_comparison_tail {
            self.arena.add_block(
                syntax_kind_ext::BLOCK,
                self.token_pos(),
                self.token_pos(),
                crate::parser::node::BlockData {
                    statements: self.make_node_list(Vec::new()),
                    multi_line: false,
                },
            )
        } else if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            // Consume the semicolon if present (method signature).
            // Use can_parse_semicolon() which handles ASI: a preceding line break
            // acts as an implicit semicolon (matching tsc's parseFunctionBlockOrSemicolon).
            if self.can_parse_semicolon() {
                self.parse_semicolon();
            } else {
                // TS1144: '{' or ';' expected — unexpected token after method signature
                self.parse_error_at_current_token(
                    "'{' or ';' expected.",
                    tsz_common::diagnostics::diagnostic_codes::OR_EXPECTED,
                );
            }
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
                modifiers: mods.modifiers,
                asterisk_token,
                name,
                question_token,
                type_parameters,
                parameters,
                type_annotation,
                body,
            },
        )
    }

    fn class_member_name_is_if_keyword(&self, name: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name) else {
            return false;
        };
        name_node.kind == SyntaxKind::IfKeyword as u16
            || self
                .arena
                .get_identifier(name_node)
                .is_some_and(|ident| ident.escaped_text == "if")
    }
}
