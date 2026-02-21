//! Parser state - class member parsing.

use super::state::{
    CONTEXT_FLAG_AMBIENT, CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_CLASS_MEMBER_NAME,
    CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS, CONTEXT_FLAG_GENERATOR, CONTEXT_FLAG_STATIC_BLOCK,
    ParserState,
};
use crate::parser::{
    NodeIndex, NodeList,
    node::{self},
    syntax_kind_ext,
};
use tsz_common::Atom;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::SyntaxKind;

impl ParserState {
    /// Parse class member modifiers (static, public, private, protected, readonly, abstract, override)
    pub(crate) fn parse_class_member_modifiers(&mut self) -> Option<NodeList> {
        let mut modifiers = Vec::new();

        // State tracking for TS1028 (duplicates) and TS1029 (ordering)
        let mut seen_accessibility = false;
        let mut reported_accessibility_duplicate = false;
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
                if seen_accessibility && !reported_accessibility_duplicate {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Accessibility modifier already seen.",
                        diagnostic_codes::ACCESSIBILITY_MODIFIER_ALREADY_SEEN,
                    );
                    reported_accessibility_duplicate = true;
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
                        &format!(
                            "'{current_mod}' modifier must precede '{conflicting_mod}' modifier."
                        ),
                        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                seen_accessibility = true;
            } else if current_kind == SyntaxKind::StaticKeyword {
                // Check for duplicate static modifier
                // In tsc 6.0+, duplicate `static` in class members emits TS1434
                // (Unexpected keyword or identifier) because the second `static`
                // is treated as a potential property name rather than a duplicate modifier.
                if seen_static {
                    use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.parse_error_at_current_token(
                        diagnostic_messages::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                        diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
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
                if seen_accessor || seen_async || seen_readonly {
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
                    // Skip TS1031 for index signatures (e.g., `export [x: string]: string`).
                    // The checker emits the more specific TS1071 instead.
                    let next_is_index_sig = self.current_token == SyntaxKind::OpenBracketToken
                        && !self.scanner.has_preceding_line_break();
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;

                    if !next_is_constructor && !next_is_index_sig {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "'export' modifier cannot appear on class elements of this kind.",
                            diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND,
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

    /// Emit TS1031 "'declare' modifier cannot appear on class elements of this kind."
    /// at the position of the `declare` modifier in the given modifier list.
    fn emit_declare_on_non_property_error(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            for &idx in &mods.nodes {
                if let Some(node) = self.arena.get(idx)
                    && node.kind == SyntaxKind::DeclareKeyword as u16
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at(
                        node.pos,
                        node.end - node.pos,
                        "'declare' modifier cannot appear on class elements of this kind.",
                        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND,
                    );
                    break;
                }
            }
        }
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

        // TS1051: A 'set' accessor cannot have an optional parameter
        if let Some(&first_param) = parameters.nodes.first()
            && let Some(param_node) = self.arena.get(first_param)
        {
            let data_idx = param_node.data_index as usize;
            if let Some(param_data) = self.arena.parameters.get(data_idx)
                && param_data.question_token
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    param_node.pos,
                    param_node.end - param_node.pos,
                    "A 'set' accessor cannot have an optional parameter.",
                    diagnostic_codes::A_SET_ACCESSOR_CANNOT_HAVE_AN_OPTIONAL_PARAMETER,
                );
            }
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
        use tsz_common::diagnostics::diagnostic_codes;

        let mut members = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let member = self.parse_class_member();
            if member.is_some() {
                // Don't consume trailing semicolon if the member itself is a
                // SemicolonClassElement — that would eat the next standalone `;`.
                let is_semi_element = self
                    .arena
                    .get(member)
                    .is_some_and(|n| n.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT);
                if !is_semi_element {
                    self.parse_optional(SyntaxKind::SemicolonToken);
                }
                members.push(member);

                // After a successfully parsed member without a trailing semicolon,
                // if the next token cannot start a new class member, emit TS1005
                // "';' expected" and skip. This matches tsc's behavior when expression
                // continuations across line breaks (e.g., `= 0[e2]`) leave trailing
                // tokens like `:` or `{` that can't start a class member.
                if !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                    && !self.is_token(SyntaxKind::SemicolonToken)
                    && !self.is_token(SyntaxKind::AtToken) // decorator
                    && !self.is_property_name()
                {
                    self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
                    self.next_token();
                }
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
            // TS1031: 'declare' modifier cannot appear on class elements of this kind
            if has_declare_modifier {
                self.emit_declare_on_non_property_error(&modifiers);
            }
            let saved_member_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_CLASS_MEMBER_NAME;
            let accessor = self.parse_get_accessor_with_modifiers(modifiers, start_pos);
            self.context_flags = saved_member_flags;
            return accessor;
        }

        // Handle set accessor: set foo(value) { }
        if !asterisk_token && self.is_token(SyntaxKind::SetKeyword) && self.look_ahead_is_accessor()
        {
            // TS1031: 'declare' modifier cannot appear on class elements of this kind
            if has_declare_modifier {
                self.emit_declare_on_non_property_error(&modifiers);
            }
            let saved_member_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_CLASS_MEMBER_NAME;
            let accessor = self.parse_set_accessor_with_modifiers(modifiers, start_pos);
            self.context_flags = saved_member_flags;
            return accessor;
        }

        // Handle index signatures: [key: Type]: ValueType
        if self.is_token(SyntaxKind::OpenBracketToken) && self.look_ahead_is_index_signature() {
            let sig = self.parse_index_signature_with_modifiers(modifiers, start_pos);
            self.parse_semicolon();
            return sig;
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

        // Recovery: Handle 'class'/'enum' keywords that are misplaced declarations in class body.
        // `class D { }` or `enum E { }` inside a class body are invalid — classes and enums
        // can't be nested as class members. But `class;` or `class(){}` are valid property names.
        // When followed by an identifier on the same line, emit TS1068 and skip the declaration.
        if modifiers.is_none()
            && matches!(
                self.token(),
                SyntaxKind::ClassKeyword | SyntaxKind::EnumKeyword
            )
        {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            let next_is_identifier =
                self.is_identifier_or_keyword() && !self.scanner.has_preceding_line_break();
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if next_is_identifier {
                // Misplaced class/enum declaration. Emit TS1068 at the keyword position.
                self.parse_error_at_current_token(
                    "Unexpected token. A constructor, method, accessor, or property was expected.",
                    diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                );
                // Skip keyword + name. If `{` follows, skip the block body but leave
                // the matching `}` for the outer class when the inner block has exactly
                // one level of braces and the outer class brace is pending.
                self.next_token(); // skip class/enum keyword
                self.next_token(); // skip the identifier name
                // Skip extends/implements clauses if present (e.g., `class D extends E {`)
                while self.is_identifier_or_keyword()
                    && !self.is_token(SyntaxKind::OpenBraceToken)
                    && !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::SemicolonToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    self.next_token();
                    // Skip comma-separated items
                    if self.is_token(SyntaxKind::CommaToken) {
                        self.next_token();
                    }
                }
                if self.is_token(SyntaxKind::OpenBraceToken) {
                    // Skip tokens inside the block body until we find the matching }.
                    // DON'T consume the final } — leave it for the outer class body
                    // to use as its closing brace (error recovery behavior matching TSC).
                    let mut depth = 1u32;
                    self.next_token(); // consume {
                    while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
                        if self.is_token(SyntaxKind::OpenBraceToken) {
                            depth += 1;
                        } else if self.is_token(SyntaxKind::CloseBraceToken) {
                            depth -= 1;
                            if depth == 0 {
                                // Leave the } for the outer class to consume
                                break;
                            }
                        }
                        self.next_token();
                    }
                }
                return NodeIndex::NONE;
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
        // Note: Do NOT set CONTEXT_FLAG_GENERATOR or CONTEXT_FLAG_ASYNC here.
        // The yield/await context must only be active during the method body
        // (parameters + block), not during property name parsing.  Otherwise
        // `yield` inside a computed property name like `async * [yield]()`
        // would be parsed as a YieldExpression instead of an Identifier.
        // The generator/async flags are correctly set later (lines ~3970-3974).
        let has_modifiers = modifiers.is_some();
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
                    "Declaration expected",
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
        self.context_flags &= !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR);
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
            // TS1031: 'declare' modifier cannot appear on class elements of this kind
            // (methods cannot be declared, only properties can)
            if has_declare_modifier {
                self.emit_declare_on_non_property_error(&modifiers);
            }

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

            // TS1442: Expected '=' for property initializer.
            // When a class property has a type annotation and the next token is
            // an expression start (not '=', ';', '}', or EOF), emit TS1442 and
            // treat the expression as the initializer for recovery.
            let init_saved_flags = self.context_flags;
            self.context_flags &= !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR);

            let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
                self.parse_assignment_expression()
            } else if type_annotation != NodeIndex::NONE
                && !self.is_token(SyntaxKind::SemicolonToken)
                && !self.is_token(SyntaxKind::CloseBraceToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
                && (self.is_token(SyntaxKind::StringLiteral)
                    || self.is_token(SyntaxKind::NumericLiteral))
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Expected '=' for property initializer.",
                    diagnostic_codes::EXPECTED_FOR_PROPERTY_INITIALIZER,
                );
                self.parse_assignment_expression()
            } else {
                NodeIndex::NONE
            };

            self.context_flags = init_saved_flags;

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

        let is_index_sig = if self.is_token(SyntaxKind::DotDotDotToken) {
            true
        } else if self.is_parameter_modifier() {
            self.next_token();
            self.is_identifier_or_keyword()
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
