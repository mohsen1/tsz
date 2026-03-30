//! Parser state - class member parsing.

use super::state::{
    CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_CLASS_MEMBER_NAME, CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS,
    CONTEXT_FLAG_GENERATOR, CONTEXT_FLAG_GENERATOR_MEMBER_NAME, CONTEXT_FLAG_STATIC_BLOCK,
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
        let mut seen_static = false;
        let mut seen_abstract = false;
        let mut seen_readonly = false;
        let mut seen_override = false;
        let mut seen_accessor = false;
        let mut seen_async = false;
        let mut seen_declare = false;

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
                    // tsc silently accepts duplicate/mixed accessibility on property
                    // declarations (e.g., `public public p1;`) — no TS1028 emitted.
                    // But for methods/constructors (e.g., `public public Foo()`,
                    // `public public constructor()`), tsc emits TS1028.
                    //
                    // Detect method/constructor context with two-token lookahead:
                    // if the token after the duplicate keyword is `constructor`, or
                    // an identifier/keyword followed by `(` or `<`, this is a method.
                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token(); // skip duplicate accessibility keyword
                    let token_after = self.current_token;
                    let is_method_context = if token_after == SyntaxKind::ConstructorKeyword {
                        true
                    } else {
                        // Look one more ahead to check for `(` or `<`
                        self.next_token();
                        let token_after_name = self.current_token;
                        matches!(
                            token_after_name,
                            SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken
                        )
                    };
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;

                    if is_method_context {
                        // Method/constructor context — emit TS1028
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "Accessibility modifier already seen.",
                            diagnostic_codes::ACCESSIBILITY_MODIFIER_ALREADY_SEEN,
                        );
                    }
                    // For property context, silently accept the duplicate modifier
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
                    let other = if seen_abstract {
                        "abstract"
                    } else if seen_override {
                        "override"
                    } else if seen_readonly {
                        "readonly"
                    } else if seen_accessor {
                        "accessor"
                    } else {
                        "async"
                    };
                    self.parse_error_at_current_token(
                        &format!("'static' modifier must precede '{other}' modifier."),
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
                    let other = if seen_override {
                        "override"
                    } else if seen_readonly {
                        "readonly"
                    } else if seen_accessor {
                        "accessor"
                    } else {
                        "async"
                    };
                    self.parse_error_at_current_token(
                        &format!("'abstract' modifier must precede '{other}' modifier."),
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
                if seen_accessor || seen_async {
                    use tsz_common::diagnostics::diagnostic_codes;
                    let other = if seen_accessor { "accessor" } else { "async" };
                    self.parse_error_at_current_token(
                        &format!("'readonly' modifier must precede '{other}' modifier."),
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
                // TS1040: 'override' modifier cannot be used in an ambient context
                // Handles `declare override` ordering (override after declare on same member)
                if seen_declare {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'override' modifier cannot be used in an ambient context.",
                        diagnostic_codes::MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                    );
                }
                if seen_accessor || seen_async || seen_readonly {
                    use tsz_common::diagnostics::diagnostic_codes;
                    let other = if seen_accessor {
                        "accessor"
                    } else if seen_async {
                        "async"
                    } else {
                        "readonly"
                    };
                    self.parse_error_at_current_token(
                        &format!("'override' modifier must precede '{other}' modifier."),
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
                    // TS1040: 'override' modifier cannot be used in an ambient context
                    // When `override` precedes `declare`, report at `declare` position
                    if seen_override {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            "'override' modifier cannot be used in an ambient context.",
                            diagnostic_codes::MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                        );
                    }
                    seen_declare = true;
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
        // tsc emits TS1092 in the checker at the typeParameters NodeArray position,
        // which starts after '<' (i.e., at the first type parameter or '>' if empty).
        // We emit it here in the parser but must match tsc's position: after '<'.
        let type_parameters = self.is_token(SyntaxKind::LessThanToken).then(|| {
            let less_than_end = self.token_end();
            let type_params = self.parse_type_parameters();
            self.parse_error_at(
                less_than_end,
                0,
                "Type parameters cannot appear on a constructor declaration.",
                diagnostic_codes::TYPE_PARAMETERS_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
            );
            type_params
        });

        let has_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS;
        let parameters = if has_open_paren {
            let params = self.parse_parameter_list();
            self.context_flags = saved_flags;
            self.parse_expected(SyntaxKind::CloseParenToken);
            params
        } else {
            // When `(` is missing (e.g., `constructor\n}`), skip parameter parsing
            // and `)` expectation to avoid cascading `')' expected` errors.
            self.context_flags = saved_flags;
            NodeList::new()
        };

        // Recovery: Handle return type annotation on constructor (invalid but users write it)
        if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_error_at_current_token(
                "Type annotation cannot appear on a constructor declaration.",
                diagnostic_codes::TYPE_ANNOTATION_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
            );
            // Consume the type annotation for recovery (use parse_return_type to match tsc,
            // which parses type predicates even in invalid constructor return types)
            let _ = self.parse_return_type();
        }

        // Push a new label scope for the constructor body
        // Clear static block flag - constructor creates a new function boundary
        let body_saved_flags = self.context_flags;
        self.context_flags &= !CONTEXT_FLAG_STATIC_BLOCK;
        self.push_label_scope();
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };
        self.pop_label_scope();
        self.context_flags = body_saved_flags;

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
        } else if self.is_token(SyntaxKind::CommaToken) {
            // `get x(,)` — comma can't start a parameter declaration.
            // tsc emits TS1138 "Parameter declaration expected" here,
            // NOT TS1054 (which is for getters that have actual parameters).
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at_current_token(
                diagnostic_messages::PARAMETER_DECLARATION_EXPECTED,
                diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
            );
            // Skip the comma and continue parsing to recover
            self.next_token();
            self.make_node_list(vec![])
        } else {
            use tsz_common::diagnostics::diagnostic_codes;
            // Report error at the accessor name, matching tsc behavior
            if let Some(name_node) = self.arena.get(name) {
                self.parse_error_at(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "A 'get' accessor cannot have parameters.",
                    diagnostic_codes::A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS,
                );
            } else {
                self.parse_error_at_current_token(
                    "A 'get' accessor cannot have parameters.",
                    diagnostic_codes::A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS,
                );
            }
            self.parse_parameter_list()
        };
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Optional return type (supports type predicates)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        let body = self.parse_accessor_body(&modifiers);

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

    /// Parse the body of an accessor (get or set).
    /// Returns `NodeIndex::NONE` for ambient or abstract accessors with no body.
    fn parse_accessor_body(&mut self, modifiers: &Option<NodeList>) -> NodeIndex {
        // Clear static block flag - accessor creates a new function boundary
        let saved_flags = self.context_flags;
        self.context_flags &= !CONTEXT_FLAG_STATIC_BLOCK;
        self.push_label_scope();
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            let has_abstract = modifiers.as_ref().is_some_and(|mods| {
                mods.nodes.iter().any(|&idx| {
                    self.arena
                        .nodes
                        .get(idx.0 as usize)
                        .is_some_and(|node| node.kind == SyntaxKind::AbstractKeyword as u16)
                })
            });

            // tsc's parser accepts accessors without bodies even in non-abstract,
            // non-ambient contexts — the grammar checker handles this later with
            // TS2378/TS1049 ("A 'get' accessor must have a body"). We do NOT emit
            // TS1005 here to match tsc's parser behavior and avoid false positives
            // that would incorrectly suppress TS5107 deprecation diagnostics.
            let _ = has_abstract;
            self.parse_semicolon();
            NodeIndex::NONE
        };
        self.pop_label_scope();
        self.context_flags = saved_flags;
        body
    }

    /// Emit TS1031 at the position of a specific modifier keyword in the modifier list.
    /// Used for constructor declarations where tsc's grammarErrorOnNode anchors at the modifier.
    fn emit_modifier_error_on_constructor(
        &mut self,
        modifiers: &Option<NodeList>,
        kind: SyntaxKind,
        message: &str,
        code: u32,
    ) {
        if let Some(mods) = modifiers {
            for &idx in &mods.nodes {
                if let Some(node) = self.arena.get(idx)
                    && node.kind == kind as u16
                {
                    self.parse_error_at(node.pos, node.end - node.pos, message, code);
                    return;
                }
            }
        }
        // Fallback if modifier not found in list
        self.parse_error_at_current_token(message, code);
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

        // TS1051: A 'set' accessor cannot have an optional parameter
        // tsc anchors the error at the `?` token, which is right after the
        // parameter name.
        if let Some(&first_param) = parameters.nodes.first()
            && let Some(param_node) = self.arena.get(first_param)
        {
            let data_idx = param_node.data_index as usize;
            if let Some(param_data) = self.arena.parameters.get(data_idx)
                && param_data.question_token
            {
                use tsz_common::diagnostics::diagnostic_codes;
                // Anchor at the `?` token: it starts at param_name.end
                let question_pos = self
                    .arena
                    .get(param_data.name)
                    .map_or(param_node.pos, |name_node| name_node.end);
                self.parse_error_at(
                    question_pos,
                    1, // `?` is a single character
                    "A 'set' accessor cannot have an optional parameter.",
                    diagnostic_codes::A_SET_ACCESSOR_CANNOT_HAVE_AN_OPTIONAL_PARAMETER,
                );
            }
        }

        // Parse return type annotation for error recovery (tsc preserves it in JS output).
        // Setters cannot legally have return type annotations, but we store it so the
        // emitter can preserve it.
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            use tsz_common::diagnostics::diagnostic_codes;
            // Report error at the accessor name, matching tsc behavior
            if let Some(name_node) = self.arena.get(name) {
                self.parse_error_at(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "A 'set' accessor cannot have a return type annotation.",
                    diagnostic_codes::A_SET_ACCESSOR_CANNOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                );
            } else {
                self.parse_error_at_current_token(
                    "A 'set' accessor cannot have a return type annotation.",
                    diagnostic_codes::A_SET_ACCESSOR_CANNOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                );
            }
            // Use parse_return_type to match tsc, which parses type predicates
            // even in invalid setter return types
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        let body = self.parse_accessor_body(&modifiers);

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
                type_annotation,
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
                // if the next token cannot start a new class member, emit TS1068
                // and skip. This matches tsc's parseList/abortParsingListOrMoveToNextToken
                // behavior for ClassMembers context. If a prior TS1005 was already emitted
                // at this exact position (from parseSemicolon within the member), the
                // parse_error_at dedup will suppress this TS1068, preserving the TS1005.
                if !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                    && !self.is_token(SyntaxKind::SemicolonToken)
                    && !self.is_token(SyntaxKind::AtToken) // decorator
                    && !self.is_token(SyntaxKind::AsteriskToken) // generator method
                    && !self.is_property_name()
                {
                    self.parse_error_at_current_token(
                        "Unexpected token. A constructor, method, accessor, or property was expected.",
                        diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                    );
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

        // Handle bare `#` that can't become a PrivateIdentifier.
        // In tsc, the scanner emits TS1127 for a standalone `#` (e.g., `# name` with a
        // space, or `#` followed by a non-identifier char). Rescan; if still HashToken,
        // emit TS1127 and skip to avoid cascading TS1003/TS1005/TS1068/TS1128.
        if self.is_token(SyntaxKind::HashToken) {
            let rescanned = self.scanner.re_scan_hash_token();
            if rescanned != SyntaxKind::PrivateIdentifier {
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                    diagnostic_codes::INVALID_CHARACTER,
                );
                self.next_token();
                return NodeIndex::NONE;
            }
            self.current_token = rescanned;
        }

        // Parse decorators if present
        let decorators = self.parse_decorators();
        let has_decorators = decorators.is_some();

        // If decorators were found before a static block, emit TS1206
        // TSC anchors this error at the decorator position, not the `static` keyword.
        if decorators.is_some()
            && self.is_token(SyntaxKind::StaticKeyword)
            && self.look_ahead_is_static_block()
        {
            if let Some(ref dec_list) = decorators
                && let Some(&first_dec_idx) = dec_list.nodes.first()
                && let Some(dec_node) = self.arena.get(first_dec_idx)
            {
                let start = dec_node.pos;
                let length = dec_node.end.saturating_sub(dec_node.pos);
                self.parse_error_at(
                    start,
                    length,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
            }
            return self.parse_static_block();
        }

        // Handle static block: static { ... }
        if self.is_token(SyntaxKind::StaticKeyword) && self.look_ahead_is_static_block() {
            return self.parse_static_block();
        }

        if matches!(
            self.token(),
            SyntaxKind::GlobalKeyword | SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword
        ) && self.look_ahead_is_module_declaration()
        {
            self.recover_invalid_module_like_class_member();
            return NodeIndex::NONE;
        }

        if self.look_ahead_is_class_body_variable_statement() {
            self.recover_invalid_class_body_variable_statement();
            return NodeIndex::NONE;
        }

        // Parse modifiers (static, public, private, protected, readonly, abstract, override)
        let diag_len_before_modifiers = self.parse_diagnostics.len();
        let parsed_modifiers = self.parse_class_member_modifiers();
        let had_keyword_modifiers = parsed_modifiers.is_some();

        // Combine decorators and modifiers into a single modifiers list
        // TypeScript stores decorators as part of the modifiers array
        let mut modifiers = match (decorators, parsed_modifiers) {
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

        // TS1436: Decorators must precede the name and all keywords of property declarations.
        // Detect `@` appearing after keyword modifiers (e.g., `public @dec prop`).
        // Emit the specific diagnostic, then parse and consume the misplaced decorators
        // so the rest of the member can be parsed normally for recovery.
        if had_keyword_modifiers && self.is_token(SyntaxKind::AtToken) {
            self.parse_error_at_current_token(
                "Decorators must precede the name and all keywords of property declarations.",
                diagnostic_codes::DECORATORS_MUST_PRECEDE_THE_NAME_AND_ALL_KEYWORDS_OF_PROPERTY_DECLARATIONS,
            );
            // Parse the misplaced decorators to consume them
            if let Some(late_decs) = self.parse_decorators() {
                if let Some(ref mut mods) = modifiers {
                    mods.nodes.extend(late_decs.nodes);
                    mods.end = late_decs.end;
                } else {
                    modifiers = Some(late_decs);
                }
            }
        }

        // Handle static block after modifiers: { ... }
        // Case 1: `static` not yet consumed (no preceding modifiers or only decorators)
        if self.is_token(SyntaxKind::StaticKeyword) && self.look_ahead_is_static_block() {
            if let Some(ref mods) = modifiers {
                // Truncate modifier-ordering diagnostics (TS1028/TS1029) emitted
                // during parse_class_member_modifiers — tsc only emits TS1184 here.
                self.parse_diagnostics.truncate(diag_len_before_modifiers);
                if let Some(first_node) = self.arena.get(mods.nodes[0]) {
                    self.parse_error_at(
                        first_node.pos,
                        first_node.end - first_node.pos,
                        "Modifiers cannot appear here.",
                        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                    );
                }
            }
            return self.parse_static_block();
        }
        // Case 2: `static` was consumed as a modifier and `{` follows (e.g. `async static {`)
        // The last modifier is `static` and current token is `{` — this is a static block
        // with invalid preceding modifiers.
        if self.is_token(SyntaxKind::OpenBraceToken)
            && let Some(ref mods) = modifiers
        {
            let last_is_static = mods
                .nodes
                .last()
                .and_then(|&idx| self.arena.get(idx))
                .is_some_and(|n| n.kind == SyntaxKind::StaticKeyword as u16);
            if last_is_static {
                // Truncate modifier-ordering diagnostics — tsc only emits TS1184.
                self.parse_diagnostics.truncate(diag_len_before_modifiers);
                // Emit TS1184 at the first modifier's position (matches tsc).
                if let Some(first_node) = self.arena.get(mods.nodes[0]) {
                    self.parse_error_at(
                        first_node.pos,
                        first_node.end - first_node.pos,
                        "Modifiers cannot appear here.",
                        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                    );
                }
                return self.parse_static_block();
            }
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
                self.emit_modifier_error_on_constructor(
                    &modifiers,
                    SyntaxKind::StaticKeyword,
                    "'static' modifier cannot appear on a constructor declaration.",
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
                );
            }

            // TS1031: tsc anchors at the modifier keyword via grammarErrorOnNode(modifier)
            if has_export_modifier {
                self.emit_modifier_error_on_constructor(
                    &modifiers,
                    SyntaxKind::ExportKeyword,
                    "'export' modifier cannot appear on class elements of this kind.",
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND,
                );
            } else if has_declare_modifier {
                self.emit_modifier_error_on_constructor(
                    &modifiers,
                    SyntaxKind::DeclareKeyword,
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

        // Handle mapped type member in class body: [P in K]: T (TS 4.1+)
        if self.is_token(SyntaxKind::OpenBracketToken) && self.look_ahead_is_mapped_type_start() {
            let member = self.parse_mapped_type_member();
            return member;
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

        // Recovery: Handle `try` keyword misplaced in class body.
        // `try { ... }` is not a valid class member, even after modifiers like
        // `public`. When followed by `{` on the same line, emit TS1068 to match tsc
        // rather than parsing `try` as a property name and cascading into TS1434/TS1435.
        if self.is_token(SyntaxKind::TryKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            let next_is_open_brace = self.is_token(SyntaxKind::OpenBraceToken)
                && !self.scanner.has_preceding_line_break();
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            if next_is_open_brace {
                self.parse_error_at_current_token(
                    "Unexpected token. A constructor, method, accessor, or property was expected.",
                    diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                );
            }
        }

        // Recovery: Handle `class`/`enum` keywords that are misplaced declarations in a class body.
        // `class D {}` or `enum E {}` inside a class body are invalid, even after
        // modifiers like `public`. But `class;` or `class(){}` remain valid property
        // and method names, so only trigger when an identifier follows on the same line.
        if matches!(
            self.token(),
            SyntaxKind::ClassKeyword | SyntaxKind::EnumKeyword
        ) {
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
        // However, track the generator asterisk so we can suppress TS1213
        // for `yield` in computed property names of generator methods — tsc
        // does not emit TS1213 in this position.
        if asterisk_token {
            self.context_flags |= CONTEXT_FLAG_GENERATOR_MEMBER_NAME;
        }
        let has_modifiers = modifiers.is_some();
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

        // TS1436: Decorator after property name (e.g., `private prop @decorator`).
        // Detect `@` after the member name where `:`, `=`, `;`, `(`, or `<` is expected.
        // Only when `@` is on the SAME line — if on a new line, ASI applies and the
        // property ends normally; the `@` starts a new decorated member.
        if self.is_token(SyntaxKind::AtToken) && !self.scanner.has_preceding_line_break() {
            self.parse_error_at_current_token(
                "Decorators must precede the name and all keywords of property declarations.",
                diagnostic_codes::DECORATORS_MUST_PRECEDE_THE_NAME_AND_ALL_KEYWORDS_OF_PROPERTY_DECLARATIONS,
            );
            // Parse and consume the misplaced decorator(s) for recovery
            let _ = self.parse_decorators();
        }

        let method_saved_flags = self.context_flags;
        self.context_flags &=
            !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR | CONTEXT_FLAG_STATIC_BLOCK);
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
            self.context_flags &=
                !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR | CONTEXT_FLAG_STATIC_BLOCK);
            self.context_flags |= crate::parser::state::CONTEXT_FLAG_CLASS_FIELD_INITIALIZER;

            let has_equals_initializer = self.parse_optional(SyntaxKind::EqualsToken);
            let initializer = if has_equals_initializer {
                self.parse_assignment_expression()
            } else if type_annotation != NodeIndex::NONE
                && !self.is_token(SyntaxKind::SemicolonToken)
                && !self.is_token(SyntaxKind::CloseBraceToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
                && (((self.is_token(SyntaxKind::StringLiteral)
                    || self.is_token(SyntaxKind::NumericLiteral)
                    || self.is_token(SyntaxKind::BigIntLiteral))
                    // Don't treat string/numeric/bigint literals as initializers if they look
                    // like the next class member property name (followed by `:` or `?`).
                    // e.g., `"d": string; "e": number;` — `"e"` is a property name.
                    && !self.look_ahead_is_next_class_member_property_name())
                    // TS1442 for `.` after a type annotation: `a: this.foo;`.
                    || self.is_token(SyntaxKind::DotToken))
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

            if has_equals_initializer
                && self.is_token(SyntaxKind::CommaToken)
                && !self.scanner.has_preceding_line_break()
            {
                self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            }

            // When a property with an initializer is followed by a line break and
            // a continuation token (`[`, `(`, `.`), report a missing semicolon.
            // Exception: if the property has a computed name, no type annotation,
            // and the next line starts with `[`, treat `[` as a new computed
            // property (ASI), not element access on the initializer.
            // tsc only treats `[` as a continuation when there IS a type
            // annotation (e.g., `[e]: number = 0\n[e2]` → TS1005), but not
            // when there's only an initializer (e.g., `[e] = "A"\n[e2] = "B"`).
            let is_computed_name = self
                .arena
                .get(name)
                .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);
            if initializer != NodeIndex::NONE
                && !self.is_token(SyntaxKind::SemicolonToken)
                && self.scanner.has_preceding_line_break()
                && self.class_member_initializer_continues_on_next_line()
                && !(is_computed_name
                    && type_annotation == NodeIndex::NONE
                    && self.is_token(SyntaxKind::OpenBracketToken)
                    && !self.look_ahead_is_invalid_class_member_method_like_continuation())
            {
                self.report_missing_semicolon_after_class_field_initializer();
                self.recover_invalid_class_member_initializer_continuation();
            }

            // TS1442: when a property has a type annotation but no initializer
            // and the next token cannot end the declaration (not `;`, `}`, EOF,
            // and no preceding line break), emit "Expected '=' for property
            // initializer." — matching tsc's parseSemicolonAfterPropertyName.
            if type_annotation != NodeIndex::NONE
                && initializer == NodeIndex::NONE
                && !self.can_parse_semicolon()
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Expected '=' for property initializer.",
                    diagnostic_codes::EXPECTED_FOR_PROPERTY_INITIALIZER,
                );
            }

            // Match tsc's parseSemicolonAfterPropertyName: when a property has
            // no type annotation and no initializer and no semicolon follows,
            // use keyword-aware semicolon error (TS1434/TS1435) instead of
            // the generic "';' expected". This produces "Unexpected keyword or
            // identifier" for bare identifiers like `NoMove` in class bodies.
            if !has_var_let_modifier
                && type_annotation == NodeIndex::NONE
                && initializer == NodeIndex::NONE
                && !self.is_token(SyntaxKind::SemicolonToken)
                && !self.can_parse_semicolon()
            {
                self.parse_error_for_missing_semicolon_after(name);
            }

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

    fn recover_invalid_module_like_class_member(&mut self) {
        self.parse_error_at_current_token(
            "Unexpected token. A constructor, method, accessor, or property was expected.",
            diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
        );
        self.next_token();

        if !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
            && !self.scanner.has_preceding_line_break()
        {
            self.error_token_expected(";");

            while !self.is_token(SyntaxKind::CloseBraceToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
                && !self.scanner.has_preceding_line_break()
            {
                self.next_token();
            }
        }

        if self.is_token(SyntaxKind::CloseBraceToken) {
            self.parse_error_at_current_token(
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
        }
    }

    fn look_ahead_is_class_body_variable_statement(&mut self) -> bool {
        if !matches!(
            self.token(),
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword
        ) {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token();
        let is_match = if self.scanner.has_preceding_line_break() {
            false
        } else if matches!(
            self.token(),
            SyntaxKind::OpenBraceToken | SyntaxKind::OpenBracketToken
        ) {
            true
        } else if self.is_identifier_or_keyword() || self.is_token(SyntaxKind::PrivateIdentifier) {
            self.next_token();
            !self.scanner.has_preceding_line_break() && !self.is_token(SyntaxKind::OpenParenToken)
        } else {
            false
        };

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_match
    }

    fn recover_invalid_class_body_variable_statement(&mut self) {
        self.parse_error_at_current_token(
            "Unexpected token. A constructor, method, accessor, or property was expected.",
            diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
        );

        while !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            self.next_token();
        }

        if self.is_token(SyntaxKind::SemicolonToken) {
            self.next_token();
        }

        if self.is_token(SyntaxKind::CloseBraceToken) {
            self.parse_error_at_current_token(
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
        }
    }

    const fn class_member_initializer_continues_on_next_line(&self) -> bool {
        matches!(
            self.token(),
            SyntaxKind::OpenParenToken
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::DotToken
                | SyntaxKind::QuestionDotToken
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::TemplateHead
        )
    }

    fn report_missing_semicolon_after_class_field_initializer(&mut self) {
        if let Some((pos, len)) = self.class_field_initializer_continuation_anchor() {
            self.parse_error_at(pos, len, "';' expected.", diagnostic_codes::EXPECTED);
        } else {
            self.error_token_expected(";");
        }
    }

    fn class_field_initializer_continuation_anchor(&mut self) -> Option<(u32, u32)> {
        let (open, close) = match self.token() {
            SyntaxKind::OpenBracketToken => {
                (SyntaxKind::OpenBracketToken, SyntaxKind::CloseBracketToken)
            }
            SyntaxKind::OpenParenToken => (SyntaxKind::OpenParenToken, SyntaxKind::CloseParenToken),
            _ => return None,
        };

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        let mut depth = 0u32;
        let mut anchor = None;

        while !self.is_token(SyntaxKind::EndOfFileToken) {
            if self.is_token(open) {
                depth += 1;
            } else if self.is_token(close) {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let pos = self.token_end();
                    anchor = Some((pos, 1));

                    if open == SyntaxKind::OpenBracketToken {
                        self.next_token();
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            let mut paren_depth = 0u32;
                            while !self.is_token(SyntaxKind::EndOfFileToken) {
                                if self.is_token(SyntaxKind::OpenParenToken) {
                                    paren_depth += 1;
                                } else if self.is_token(SyntaxKind::CloseParenToken) {
                                    paren_depth = paren_depth.saturating_sub(1);
                                    if paren_depth == 0 {
                                        self.next_token();
                                        if self.is_token(SyntaxKind::OpenBraceToken) {
                                            anchor = Some((self.token_pos(), 1));
                                        }
                                        break;
                                    }
                                }
                                self.next_token();
                            }
                        }
                    }
                    break;
                }
            }
            self.next_token();
        }

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        anchor
    }

    fn recover_invalid_class_member_initializer_continuation(&mut self) {
        if !self.look_ahead_is_invalid_class_member_method_like_continuation() {
            return;
        }

        if self.is_token(SyntaxKind::OpenBracketToken) {
            let mut bracket_depth = 0u32;
            while !self.is_token(SyntaxKind::EndOfFileToken) {
                if self.is_token(SyntaxKind::OpenBracketToken) {
                    bracket_depth += 1;
                } else if self.is_token(SyntaxKind::CloseBracketToken) {
                    bracket_depth = bracket_depth.saturating_sub(1);
                    if bracket_depth == 0 {
                        self.next_token();
                        break;
                    }
                }
                self.next_token();
            }
        }

        if !self.is_token(SyntaxKind::OpenParenToken) {
            return;
        }

        let mut paren_depth = 0u32;
        while !self.is_token(SyntaxKind::EndOfFileToken) {
            if self.is_token(SyntaxKind::OpenParenToken) {
                paren_depth += 1;
            } else if self.is_token(SyntaxKind::CloseParenToken) {
                paren_depth = paren_depth.saturating_sub(1);
                if paren_depth == 0 {
                    self.next_token();
                    break;
                }
            }
            self.next_token();
        }

        if !self.is_token(SyntaxKind::OpenBraceToken) {
            return;
        }

        let mut brace_depth = 0u32;
        while !self.is_token(SyntaxKind::EndOfFileToken) {
            if self.is_token(SyntaxKind::OpenBraceToken) {
                brace_depth += 1;
            } else if self.is_token(SyntaxKind::CloseBraceToken) {
                brace_depth = brace_depth.saturating_sub(1);
                self.next_token();
                if brace_depth == 0 {
                    break;
                }
                continue;
            }
            self.next_token();
        }

        if self.is_token(SyntaxKind::CloseBraceToken) {
            self.parse_error_at_current_token(
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
        }
    }

    fn look_ahead_is_invalid_class_member_method_like_continuation(&mut self) -> bool {
        if !self.is_token(SyntaxKind::OpenBracketToken)
            && !self.is_token(SyntaxKind::OpenParenToken)
        {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        let mut is_match = false;

        if self.is_token(SyntaxKind::OpenBracketToken) {
            let mut bracket_depth = 0u32;
            while !self.is_token(SyntaxKind::EndOfFileToken) {
                if self.is_token(SyntaxKind::OpenBracketToken) {
                    bracket_depth += 1;
                } else if self.is_token(SyntaxKind::CloseBracketToken) {
                    bracket_depth = bracket_depth.saturating_sub(1);
                    if bracket_depth == 0 {
                        self.next_token();
                        break;
                    }
                }
                self.next_token();
            }
        }

        if self.is_token(SyntaxKind::OpenParenToken) {
            let mut paren_depth = 0u32;
            while !self.is_token(SyntaxKind::EndOfFileToken) {
                if self.is_token(SyntaxKind::OpenParenToken) {
                    paren_depth += 1;
                } else if self.is_token(SyntaxKind::CloseParenToken) {
                    paren_depth = paren_depth.saturating_sub(1);
                    if paren_depth == 0 {
                        self.next_token();
                        is_match = self.is_token(SyntaxKind::OpenBraceToken);
                        break;
                    }
                }
                self.next_token();
            }
        }

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_match
    }

    /// Look ahead to check if the current string/numeric literal token is actually
    /// the property name of the next class member (followed by `:` or `?`).
    /// This prevents false TS1442 when two string-literal-named properties appear
    /// in sequence, e.g., `"d": string; "e": number;`.
    fn look_ahead_is_next_class_member_property_name(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip the string/numeric literal

        let is_property_name = matches!(
            self.token(),
            SyntaxKind::ColonToken       // "d": type
            | SyntaxKind::QuestionToken // "d"?: type
        );

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_property_name
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
