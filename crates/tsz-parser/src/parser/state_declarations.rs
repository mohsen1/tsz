//! Parser state - interface, type alias, enum, module, import/export, and control flow parsing methods

use super::state::ParserState;
use crate::parser::{
    NodeIndex, NodeList,
    node::{
        EnumData, EnumMemberData, IdentifierData, ImportClauseData, ImportDeclData,
        NamedImportsData, ParameterData, SpecifierData,
    },
    node_flags, syntax_kind_ext,
};
use tsz_common::interner::Atom;
use tsz_scanner::SyntaxKind;

enum TypeMemberPropertyOrMethodName {
    Property(NodeIndex),
    IndexSignature(NodeIndex),
}

impl ParserState {
    /// Parse interface declaration
    pub(crate) fn parse_interface_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_interface_declaration_with_modifiers(start_pos, None)
    }

    /// Parse interface declaration with explicit modifiers
    pub(crate) fn parse_interface_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::InterfaceKeyword);

        // Parse interface name - keywords like 'string', 'abstract' can be used as interface names
        // BUT reserved words like 'void', 'null' cannot be used
        let name = if self.is_token(SyntaxKind::YieldKeyword) {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Identifier expected. 'yield' is a reserved word in strict mode.",
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
            );
            self.parse_identifier_name()
        } else if self.is_identifier_or_keyword() {
            // TS1005: Reserved words cannot be used as interface names
            if self.is_reserved_word() {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
                // Consume the invalid token to avoid cascading errors
                self.next_token();
                NodeIndex::NONE
            } else {
                self.parse_identifier_name()
            }
        } else if self.is_token(SyntaxKind::OpenBraceToken) {
            // TS1438: Interface must be given a name (e.g., `interface { }`)
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at_current_token(
                diagnostic_messages::INTERFACE_MUST_BE_GIVEN_A_NAME,
                diagnostic_codes::INTERFACE_MUST_BE_GIVEN_A_NAME,
            );
            NodeIndex::NONE
        } else {
            self.parse_identifier()
        };

        // Parse type parameters: interface IList<T> {}
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends only for interfaces)
        // Interfaces can extend multiple types: interface A extends B, C, D { }
        let heritage_clauses = self.is_token(SyntaxKind::ExtendsKeyword).then(|| {
            let clause_start = self.token_pos();
            self.next_token();

            // TS1097: 'extends' list cannot be empty.
            if self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::ImplementsKeyword)
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "'extends' list cannot be empty.",
                    diagnostic_codes::LIST_CANNOT_BE_EMPTY,
                );
                // Return an empty heritage clause so we can still parse the body
                let clause_end = self.token_end();
                let clause = self.arena.add_heritage(
                    syntax_kind_ext::HERITAGE_CLAUSE,
                    clause_start,
                    clause_end,
                    crate::parser::node::HeritageData {
                        token: SyntaxKind::ExtendsKeyword as u16,
                        types: self.make_node_list(vec![]),
                    },
                );
                return self.make_node_list(vec![clause]);
            }

            let mut types = Vec::new();
            loop {
                let type_ref = self.parse_interface_heritage_type_reference();
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
                crate::parser::node::HeritageData {
                    token: SyntaxKind::ExtendsKeyword as u16,
                    types: self.make_node_list(types),
                },
            );
            self.make_node_list(vec![clause])
        });

        // TS1176: Interface declaration cannot have 'implements' clause.
        // Parse the clause for recovery, treating it like extends.
        if self.is_token(SyntaxKind::ImplementsKeyword) {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Interface declaration cannot have 'implements' clause.",
                diagnostic_codes::INTERFACE_DECLARATION_CANNOT_HAVE_IMPLEMENTS_CLAUSE,
            );
            // Parse the implements types for error recovery (reuse extends parsing)
            self.next_token();
            while self.is_identifier_or_keyword() || self.is_token(SyntaxKind::CommaToken) {
                self.next_token();
                if self.is_token(SyntaxKind::LessThanToken) {
                    let _ = self.parse_type_arguments();
                }
            }
        }

        // Check for duplicate extends clause: interface I extends A extends B { }
        if self.is_token(SyntaxKind::ExtendsKeyword) {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "'extends' clause already seen.",
                diagnostic_codes::EXTENDS_CLAUSE_ALREADY_SEEN,
            );
            // Skip the duplicate extends and its types for recovery
            self.next_token();
            while self.is_identifier_or_keyword() || self.is_token(SyntaxKind::CommaToken) {
                self.next_token();
                if self.is_token(SyntaxKind::LessThanToken) {
                    // Skip type arguments
                    let _ = self.parse_type_arguments();
                }
            }
        }

        // Parse interface body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let members = self.parse_type_members();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_interface(
            syntax_kind_ext::INTERFACE_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::node::InterfaceData {
                modifiers,
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse type members (for interfaces and type literals)
    pub(crate) fn parse_type_members(&mut self) -> NodeList {
        let mut members = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let start_pos = self.token_pos();
            let member = self.parse_type_member(true);
            if member.is_some() {
                members.push(member);
            }

            self.parse_type_member_separator_with_asi();

            // If we didn't make progress, skip the current token to avoid infinite loop
            if self.token_pos() == start_pos && !self.is_token(SyntaxKind::CloseBraceToken) {
                self.next_token();
            }
        }

        self.make_node_list(members)
    }

    /// Parse a single type member (property signature, method signature, call signature, construct signature)
    pub(crate) fn parse_type_member(&mut self, in_interface_declaration: bool) -> NodeIndex {
        let start_pos = self.token_pos();
        if let Some(member) = self.parse_type_member_explicit_signature(start_pos) {
            member
        } else {
            self.parse_type_member_property_or_method(start_pos, in_interface_declaration)
        }
    }

    fn parse_type_member_explicit_signature(&mut self, start_pos: u32) -> Option<NodeIndex> {
        if let Some(node) = self.parse_type_member_visibility_modifier_error(start_pos) {
            return Some(node);
        }

        self.parse_async_type_member_restriction();

        if self.is_token(SyntaxKind::LessThanToken) {
            return Some(self.parse_call_signature(start_pos));
        }
        if self.is_token(SyntaxKind::OpenParenToken) {
            return Some(self.parse_call_signature(start_pos));
        }

        if self.is_token(SyntaxKind::NewKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            let is_property_name = self.is_token(SyntaxKind::ColonToken)
                || self.is_token(SyntaxKind::QuestionToken)
                || self.is_token(SyntaxKind::SemicolonToken)
                || self.is_token(SyntaxKind::CommaToken)
                || self.is_token(SyntaxKind::CloseBraceToken);
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            if !is_property_name {
                return Some(self.parse_construct_signature(start_pos));
            }
        }

        if self.is_token(SyntaxKind::GetKeyword)
            && !self.look_ahead_is_property_name_after_keyword()
        {
            return Some(self.parse_get_accessor_signature(start_pos));
        }

        if self.is_token(SyntaxKind::SetKeyword)
            && !self.look_ahead_is_property_name_after_keyword()
        {
            return Some(self.parse_set_accessor_signature(start_pos));
        }

        None
    }

    fn parse_type_member_visibility_modifier_error(&mut self, start_pos: u32) -> Option<NodeIndex> {
        if matches!(
            self.token(),
            SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::OverrideKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::DeclareKeyword
        ) && !self.look_ahead_is_property_name_after_keyword()
            && !self.look_ahead_has_line_break_after_keyword()
        {
            use tsz_common::diagnostics::diagnostic_codes;

            let modifier_text = self.scanner.get_token_text();

            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            // Skip past `readonly` if present (e.g., `static readonly [s: string]: number`)
            if self.is_token(SyntaxKind::ReadonlyKeyword) {
                self.next_token();
            }
            let is_index_signature =
                self.is_token(SyntaxKind::OpenBracketToken) && self.look_ahead_is_index_signature();
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if is_index_signature {
                self.parse_error_at_current_token(
                    &format!("'{modifier_text}' modifier cannot appear on an index signature."),
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_AN_INDEX_SIGNATURE,
                );
            } else {
                self.parse_error_at_current_token(
                    &format!("'{modifier_text}' modifier cannot appear on a type member."),
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER,
                );
            }

            self.next_token();
            if is_index_signature {
                // Skip `readonly` if present (e.g., `static readonly [s: string]: number`)
                if self.is_token(SyntaxKind::ReadonlyKeyword) {
                    self.next_token();
                }
                return Some(self.parse_index_signature_with_modifiers(None, start_pos));
            }
        }

        None
    }

    fn parse_async_type_member_restriction(&mut self) {
        if self.is_token(SyntaxKind::AsyncKeyword)
            && !self.look_ahead_is_property_name_after_keyword()
        {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "'async' modifier cannot appear on a type member.",
                diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER,
            );
            self.next_token();
        }
    }

    fn parse_type_member_property_or_method(
        &mut self,
        start_pos: u32,
        in_interface_declaration: bool,
    ) -> NodeIndex {
        let readonly = if self.is_token(SyntaxKind::ReadonlyKeyword)
            && !self.look_ahead_is_property_name_after_keyword()
        {
            self.next_token();
            true
        } else {
            false
        };

        let Some(name) = self.parse_type_member_property_or_method_name(start_pos, readonly) else {
            return NodeIndex::NONE;
        };

        let name = match name {
            TypeMemberPropertyOrMethodName::IndexSignature(index_signature) => {
                return index_signature;
            }
            TypeMemberPropertyOrMethodName::Property(name) => name,
        };

        let question_token = self.parse_optional(SyntaxKind::QuestionToken);
        let modifiers = self.readonly_modifier_node_list(start_pos, readonly);

        if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::LessThanToken) {
            return self.parse_type_member_method_signature(
                start_pos,
                name,
                modifiers,
                question_token,
            );
        }

        self.parse_type_member_property_signature(
            start_pos,
            name,
            modifiers,
            question_token,
            in_interface_declaration,
        )
    }

    fn parse_type_member_property_or_method_name(
        &mut self,
        start_pos: u32,
        readonly: bool,
    ) -> Option<TypeMemberPropertyOrMethodName> {
        if self.is_token(SyntaxKind::Identifier)
            || self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_identifier_or_keyword()
        {
            Some(TypeMemberPropertyOrMethodName::Property(
                self.parse_property_name(),
            ))
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            if self.look_ahead_is_index_signature() || self.look_ahead_is_empty_index_signature() {
                let modifiers = self.readonly_modifier_node_list(start_pos, readonly);
                Some(TypeMemberPropertyOrMethodName::IndexSignature(
                    self.parse_index_signature_with_modifiers(modifiers, start_pos),
                ))
            } else {
                Some(TypeMemberPropertyOrMethodName::Property(
                    self.parse_property_name(),
                ))
            }
        } else {
            None
        }
    }

    fn readonly_modifier_node_list(
        &mut self,
        start_pos: u32,
        is_readonly: bool,
    ) -> Option<NodeList> {
        is_readonly.then(|| {
            let mod_idx = self
                .arena
                .create_modifier(SyntaxKind::ReadonlyKeyword, start_pos);
            self.make_node_list(vec![mod_idx])
        })
    }

    fn parse_type_member_method_signature(
        &mut self,
        start_pos: u32,
        name: NodeIndex,
        modifiers: Option<NodeList>,
        question_token: bool,
    ) -> NodeIndex {
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

        let end_pos = self.token_end();
        self.arena.add_signature(
            syntax_kind_ext::METHOD_SIGNATURE,
            start_pos,
            end_pos,
            crate::parser::node::SignatureData {
                modifiers,
                name,
                question_token,
                type_parameters,
                parameters: Some(parameters),
                type_annotation,
            },
        )
    }

    fn parse_type_member_property_signature(
        &mut self,
        start_pos: u32,
        name: NodeIndex,
        modifiers: Option<NodeList>,
        question_token: bool,
        in_interface_declaration: bool,
    ) -> NodeIndex {
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        if self.parse_optional(SyntaxKind::EqualsToken) {
            use tsz_common::diagnostics::diagnostic_codes;
            let (message, code) = if in_interface_declaration {
                (
                    "An interface property cannot have an initializer.",
                    diagnostic_codes::AN_INTERFACE_PROPERTY_CANNOT_HAVE_AN_INITIALIZER,
                )
            } else {
                (
                    "A type literal property cannot have an initializer.",
                    diagnostic_codes::A_TYPE_LITERAL_PROPERTY_CANNOT_HAVE_AN_INITIALIZER,
                )
            };
            self.parse_error_at_current_token(message, code);
            self.parse_assignment_expression();
        }

        let end_pos = self.token_end();
        self.arena.add_signature(
            syntax_kind_ext::PROPERTY_SIGNATURE,
            start_pos,
            end_pos,
            crate::parser::node::SignatureData {
                modifiers,
                name,
                question_token,
                type_parameters: None,
                parameters: None,
                type_annotation,
            },
        )
    }

    /// Parse call signature: (): returnType or <T>(): returnType
    pub(crate) fn parse_call_signature(&mut self, start_pos: u32) -> NodeIndex {
        // Parse optional type parameters: <T, U>
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

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
            crate::parser::node::SignatureData {
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
    pub(crate) fn parse_construct_signature(&mut self, start_pos: u32) -> NodeIndex {
        self.parse_expected(SyntaxKind::NewKeyword);

        // Parse optional type parameters: new <T>()
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

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
            crate::parser::node::SignatureData {
                modifiers: None,
                name: NodeIndex::NONE,
                question_token: false,
                type_parameters,
                parameters: Some(parameters),
                type_annotation,
            },
        )
    }

    /// Parse index signature with modifiers (static, readonly, etc.): static [key: string]: value
    ///
    /// Handles malformed index signatures with rest params (`...`), optional params (`?`),
    /// initializers (`= expr`), and multiple params — emitting the same error codes as tsc.
    pub(crate) fn parse_index_signature_with_modifiers(
        &mut self,
        modifiers: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        self.parse_expected(SyntaxKind::OpenBracketToken);

        // TS1096: empty index signature `[]` — no parameters at all
        if self.is_token(SyntaxKind::CloseBracketToken) {
            self.parse_error_at_current_token(
                "An index signature must have exactly one parameter.",
                diagnostic_codes::AN_INDEX_SIGNATURE_MUST_HAVE_EXACTLY_ONE_PARAMETER,
            );
            self.next_token(); // consume `]`

            // Still need the type annotation
            let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_type()
            } else {
                NodeIndex::NONE
            };

            let end_pos = self.token_end();
            return self.arena.add_index_signature(
                syntax_kind_ext::INDEX_SIGNATURE,
                start_pos,
                end_pos,
                crate::parser::node::IndexSignatureData {
                    modifiers,
                    parameters: self.make_node_list(vec![]),
                    type_annotation,
                },
            );
        }

        // Parse first parameter, handling malformed forms
        let param_start = self.token_pos();

        // TS1018: accessibility modifier on parameter
        let mut param_modifiers = Vec::new();
        while self.is_valid_parameter_modifier() {
            param_modifiers.push(
                self.arena
                    .create_modifier(self.current_token, self.token_pos()),
            );
            self.parse_error_at_current_token(
                "An index signature parameter cannot have an accessibility modifier.",
                diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_ACCESSIBILITY_MODIFIER,
            );
            self.next_token();
        }

        // TS1017: rest parameter in index signature
        let dot_dot_dot_token = self.parse_optional(SyntaxKind::DotDotDotToken);
        if dot_dot_dot_token {
            self.parse_error_at(
                param_start,
                3,
                "An index signature cannot have a rest parameter.",
                diagnostic_codes::AN_INDEX_SIGNATURE_CANNOT_HAVE_A_REST_PARAMETER,
            );
        }

        let param_name = self.parse_identifier();

        // TS1019: optional parameter in index signature
        let question_token = self.parse_optional(SyntaxKind::QuestionToken);
        if question_token {
            let q_end = self.token_pos();
            self.parse_error_at(
                q_end - 1,
                1,
                "An index signature parameter cannot have a question mark.",
                diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_A_QUESTION_MARK,
            );
        }

        // Parse colon and parameter type.
        // If the next token is `]` or `,`, skip — the signature is malformed
        // (e.g., `[...a]`, `[a?]`, or `[a, b]`) and other errors will be reported.
        let (param_type_token, param_type) = if self.is_token(SyntaxKind::CloseBracketToken)
            || self.is_token(SyntaxKind::CommaToken)
        {
            (self.token(), NodeIndex::NONE)
        } else {
            self.parse_expected(SyntaxKind::ColonToken);
            let tok = self.token();
            let ty = self.parse_type();
            (tok, ty)
        };

        // TS1020: initializer in index signature
        let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
            let init_start = self.token_pos();
            let init = self.parse_assignment_expression();
            self.parse_error_at(
                init_start,
                self.token_pos() - init_start,
                "An index signature parameter cannot have an initializer.",
                diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_INITIALIZER,
            );
            init
        } else {
            NodeIndex::NONE
        };

        let param_end = self.token_end();

        // TS1096: multiple parameters — consume remaining params for error recovery
        let mut has_multiple_params = false;
        if self.parse_optional(SyntaxKind::CommaToken) {
            has_multiple_params = true;
            // Consume remaining parameters for recovery
            while !self.is_token(SyntaxKind::CloseBracketToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                // Skip rest token
                self.parse_optional(SyntaxKind::DotDotDotToken);
                if self.is_identifier_or_keyword() {
                    self.next_token();
                }
                // Skip optional marker
                self.parse_optional(SyntaxKind::QuestionToken);
                // Skip type annotation
                if self.parse_optional(SyntaxKind::ColonToken) {
                    let _ = self.parse_type();
                }
                // Skip initializer
                if self.parse_optional(SyntaxKind::EqualsToken) {
                    let _ = self.parse_assignment_expression();
                }
                if !self.parse_optional(SyntaxKind::CommaToken) {
                    break;
                }
            }
        }

        if has_multiple_params {
            self.parse_error_at_current_token(
                "An index signature must have exactly one parameter.",
                diagnostic_codes::AN_INDEX_SIGNATURE_MUST_HAVE_EXACTLY_ONE_PARAMETER,
            );
        }

        self.parse_expected(SyntaxKind::CloseBracketToken);

        // Detect non-valid index signature parameter types.
        // Valid types are: string, number, symbol, or template literal types.
        // TS1268 is emitted by the checker for anything else; the parser suppresses
        // TS1021 (missing type annotation) when the param type will trigger TS1268.
        let is_valid_param_type = matches!(
            param_type_token,
            SyntaxKind::StringKeyword | SyntaxKind::NumberKeyword | SyntaxKind::SymbolKeyword
        ) || matches!(
            param_type_token,
            SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead
        );
        let has_invalid_param_type = param_type.is_some() && !is_valid_param_type;

        // Index signatures must have a type annotation (TS1021).
        // Suppress when the parameter type is already invalid (TS1268),
        // or when other index signature errors were already emitted — matches tsc behavior.
        let has_param_errors = dot_dot_dot_token
            || question_token
            || has_multiple_params
            || !param_modifiers.is_empty();
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else if !has_invalid_param_type && !has_param_errors {
            self.parse_error_at_current_token(
                "An index signature must have a type annotation.",
                diagnostic_codes::AN_INDEX_SIGNATURE_MUST_HAVE_A_TYPE_ANNOTATION,
            );
            NodeIndex::NONE
        } else {
            NodeIndex::NONE
        };

        let param_node = self.arena.add_parameter(
            syntax_kind_ext::PARAMETER,
            param_start,
            param_end,
            ParameterData {
                modifiers: if param_modifiers.is_empty() {
                    None
                } else {
                    Some(self.make_node_list(param_modifiers))
                },
                dot_dot_dot_token,
                name: param_name,
                question_token,
                type_annotation: param_type,
                initializer,
            },
        );

        let end_pos = self.token_end();
        self.arena.add_index_signature(
            syntax_kind_ext::INDEX_SIGNATURE,
            start_pos,
            end_pos,
            crate::parser::node::IndexSignatureData {
                modifiers,
                parameters: self.make_node_list(vec![param_node]),
                type_annotation,
            },
        )
    }

    /// Parse get accessor signature in type context: get `foo()`: type
    /// Note: TypeScript allows bodies here (which is an error), so we parse them for error recovery
    pub(crate) fn parse_get_accessor_signature(&mut self, start_pos: u32) -> NodeIndex {
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
            crate::parser::node::AccessorData {
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
    pub(crate) fn parse_set_accessor_signature(&mut self, start_pos: u32) -> NodeIndex {
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
            crate::parser::node::AccessorData {
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
    pub(crate) fn parse_type_alias_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_type_alias_declaration_with_modifiers(start_pos, None)
    }

    pub(crate) fn parse_type_alias_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::TypeKeyword);

        let name = self.parse_identifier();

        // Parse optional type parameters: <T, U extends Foo>
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse expected equals token, but recover gracefully if missing
        // If the next token can start a type (e.g., {, (, [), emit error and continue parsing
        if self.is_token(SyntaxKind::EqualsToken) {
            self.next_token(); // Consume the equals token
        } else {
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
                    crate::parser::node::TypeAliasData {
                        modifiers,
                        name,
                        type_parameters,
                        type_node: NodeIndex::NONE,
                    },
                );
            }
        }

        let type_node = self.parse_type();

        self.parse_semicolon();

        let end_pos = self.token_end();
        self.arena.add_type_alias(
            syntax_kind_ext::TYPE_ALIAS_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::node::TypeAliasData {
                modifiers,
                name,
                type_parameters,
                type_node,
            },
        )
    }

    /// Parse enum declaration
    pub(crate) fn parse_enum_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_enum_declaration_with_modifiers(start_pos, None)
    }

    /// Parse enum declaration with explicit modifiers
    pub(crate) fn parse_enum_declaration_with_modifiers(
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
    pub(crate) fn parse_enum_members(&mut self) -> NodeList {
        use tsz_common::diagnostics::diagnostic_codes;
        let mut members = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let start_pos = self.token_pos();

            // Handle leading comma - emit TS1132 "Enum member expected" and skip
            if self.is_token(SyntaxKind::CommaToken) {
                self.parse_error_at_current_token(
                    "Enum member expected.",
                    diagnostic_codes::ENUM_MEMBER_EXPECTED,
                );
                self.next_token(); // Skip the comma
                continue;
            }

            // Enum member names can be identifiers, string literals, or computed property names.
            // Numeric literals are parsed as names for error recovery (TS2452 reported by checker).
            // Computed property names ([x]) are not valid in enums but we recover gracefully.
            let name = if self.is_token(SyntaxKind::OpenBracketToken) {
                // Parse computed property name for recovery. TS1164 is emitted by the
                // checker (grammar check), not the parser, matching tsc's behavior.
                // This avoids position-based dedup conflicts with TS1357.
                self.parse_property_name()
            } else if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else if self.is_token(SyntaxKind::NumericLiteral) {
                // Parse numeric literal as name for recovery (checker emits TS2452)
                self.parse_numeric_literal()
            } else if self.is_token(SyntaxKind::BigIntLiteral) {
                // Parse bigint literal as name for recovery (checker emits TS2452)
                self.parse_bigint_literal()
            } else if self.is_token(SyntaxKind::PrivateIdentifier) {
                self.parse_error_at_current_token(
                    "An enum member cannot be named with a private identifier.",
                    diagnostic_codes::AN_ENUM_MEMBER_CANNOT_BE_NAMED_WITH_A_PRIVATE_IDENTIFIER,
                );
                self.parse_private_identifier()
            } else {
                self.parse_identifier_name()
            };

            // Check for unexpected token after enum member name - emit TS1357
            // Valid tokens after name are: '=', ',', '}'
            if !self.is_token(SyntaxKind::EqualsToken)
                && !self.is_token(SyntaxKind::CommaToken)
                && !self.is_token(SyntaxKind::CloseBraceToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                self.parse_error_at_current_token(
                    "An enum member name must be followed by a ',', '=', or '}'.",
                    diagnostic_codes::AN_ENUM_MEMBER_NAME_MUST_BE_FOLLOWED_BY_A_OR,
                );

                // If `:` was the unexpected token (like `a: 1`), skip past `:` and its
                // value so the recovery can pick up the next member correctly.
                if self.is_token(SyntaxKind::ColonToken) {
                    self.next_token(); // skip `:`
                    // Check if next token starts a new member (e.g., the `1` in `a: 1`)
                    let starts_member = self.is_token(SyntaxKind::OpenBracketToken)
                        || self.is_token(SyntaxKind::StringLiteral)
                        || self.is_token(SyntaxKind::NumericLiteral)
                        || self.is_token(SyntaxKind::BigIntLiteral)
                        || self.is_token(SyntaxKind::PrivateIdentifier)
                        || self.is_identifier_or_keyword();
                    if starts_member {
                        continue;
                    }
                } else {
                    let starts_member = self.is_token(SyntaxKind::OpenBracketToken)
                        || self.is_token(SyntaxKind::StringLiteral)
                        || self.is_token(SyntaxKind::NumericLiteral)
                        || self.is_token(SyntaxKind::BigIntLiteral)
                        || self.is_token(SyntaxKind::PrivateIdentifier)
                        || self.is_identifier_or_keyword();
                    if starts_member {
                        continue;
                    }
                }

                // Skip to next comma, closing brace, or EOF to recover
                while !self.is_token(SyntaxKind::CommaToken)
                    && !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    self.next_token();
                }

                // Also skip the comma if we landed on one to avoid triggering TS1132
                // on the next iteration
                if self.is_token(SyntaxKind::CommaToken) {
                    self.next_token();
                }
                continue;
            }

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
                // emit TS1357 and continue parsing instead of breaking.
                // tsc uses TS1357 (enum-specific) rather than generic TS1005 here.
                if self.is_token(SyntaxKind::Identifier)
                    || self.is_token(SyntaxKind::StringLiteral)
                    || self.is_token(SyntaxKind::PrivateIdentifier)
                    || self.is_token(SyntaxKind::OpenBracketToken)
                {
                    self.parse_error_at_current_token(
                        "An enum member name must be followed by a ',', '=', or '}'.",
                        diagnostic_codes::AN_ENUM_MEMBER_NAME_MUST_BE_FOLLOWED_BY_A_OR,
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
    pub(crate) fn parse_ambient_declaration(&mut self) -> NodeIndex {
        self.parse_ambient_declaration_with_modifiers(Vec::new())
    }

    pub(crate) fn parse_ambient_declaration_with_modifiers(
        &mut self,
        prefix_modifiers: Vec<NodeIndex>,
    ) -> NodeIndex {
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

        // Combine prefix modifiers (like export) with declare modifier
        let mut all_modifiers = prefix_modifiers;
        all_modifiers.push(declare_modifier);

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
            SyntaxKind::NamespaceKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::GlobalKeyword => {
                self.parse_declare_module_with_modifiers(start_pos, all_modifiers)
            }
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
    pub(crate) fn parse_module_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_module_declaration_with_modifiers(start_pos, None)
    }

    pub(crate) fn parse_module_declaration_with_modifiers(
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
                    atom: self.scanner.interner_mut().intern("global"),
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
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Namespace must be given a name.",
                    diagnostic_codes::NAMESPACE_MUST_BE_GIVEN_A_NAME,
                );
                // Create a missing identifier for anonymous module
                let name_start = self.token_pos();
                let name_end = self.token_pos();
                self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    name_start,
                    name_end,
                    IdentifierData {
                        atom: Atom::NONE,
                        escaped_text: String::new(),
                        original_text: None,
                        type_arguments: None,
                    },
                )
            } else if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else if tsz_scanner::token_is_template_literal(self.token()) {
                // TS1443: Module declaration names may only use ' or " quoted strings.
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Module declaration names may only use ' or \" quoted strings.",
                    diagnostic_codes::MODULE_DECLARATION_NAMES_MAY_ONLY_USE_OR_QUOTED_STRINGS,
                );
                // Consume the entire template expression/literal to avoid trailing errors
                self.parse_template_literal();

                // Create a missing identifier for recovery so the name is always valid
                let name_start = self.token_pos();
                let name_end = self.token_pos();
                self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    name_start,
                    name_end,
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
        };

        // Parse body
        // Check if this is a declare namespace/module (has declare modifier)
        let is_declare = modifiers
            .as_ref()
            .and_then(|m| m.nodes.first())
            .is_some_and(|&node| {
                self.arena
                    .get(node)
                    .is_some_and(|n| n.kind == SyntaxKind::DeclareKeyword as u16)
            });
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_module_block(is_declare)
        } else if self.is_token(SyntaxKind::DotToken) {
            // Nested module: module A.B.C { }
            self.next_token();
            self.parse_nested_module_declaration(modifiers.clone(), is_declare)
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();

        let module_idx = self.arena.add_module(
            syntax_kind_ext::MODULE_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::node::ModuleData {
                modifiers,
                name,
                body,
            },
        );

        let global_augmentation_flag = self.u16_from_node_flags(node_flags::GLOBAL_AUGMENTATION);
        if is_global && let Some(node) = self.arena.get_mut(module_idx) {
            node.flags |= global_augmentation_flag;
        }

        module_idx
    }

    pub(crate) fn parse_declare_module_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers_vec: Vec<NodeIndex>,
    ) -> NodeIndex {
        // Skip module/namespace/global keyword
        let is_global = self.is_token(SyntaxKind::GlobalKeyword);
        let modifiers = Some(self.make_node_list(modifiers_vec));
        let name = if is_global {
            let name_start = self.token_pos();
            let name_end = self.token_end();
            self.next_token();
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                name_start,
                name_end,
                IdentifierData {
                    atom: self.scanner.interner_mut().intern("global"),
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
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Namespace must be given a name.",
                    diagnostic_codes::NAMESPACE_MUST_BE_GIVEN_A_NAME,
                );
                // Create a missing identifier for anonymous module
                let name_start = self.token_pos();
                let name_end = self.token_pos();
                self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    name_start,
                    name_end,
                    IdentifierData {
                        atom: Atom::NONE,
                        escaped_text: String::new(),
                        original_text: None,
                        type_arguments: None,
                    },
                )
            } else if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else if tsz_scanner::token_is_template_literal(self.token()) {
                // TS1443: Module declaration names may only use ' or " quoted strings.
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Module declaration names may only use ' or \" quoted strings.",
                    diagnostic_codes::MODULE_DECLARATION_NAMES_MAY_ONLY_USE_OR_QUOTED_STRINGS,
                );
                // Consume the entire template expression/literal to avoid trailing errors
                self.parse_template_literal();

                // Create a missing identifier for recovery so the name is always valid
                let name_start = self.token_pos();
                let name_end = self.token_pos();
                self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    name_start,
                    name_end,
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
        };

        // Parse body
        // Declare module blocks are always ambient
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_module_block(true)
        } else if self.is_token(SyntaxKind::DotToken) {
            // Nested module: module A.B.C { }
            self.next_token();
            self.parse_nested_module_declaration(modifiers.clone(), true)
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();

        let module_idx = self.arena.add_module(
            syntax_kind_ext::MODULE_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::node::ModuleData {
                modifiers,
                name,
                body,
            },
        );

        let global_augmentation_flag = self.u16_from_node_flags(node_flags::GLOBAL_AUGMENTATION);
        if is_global && let Some(node) = self.arena.get_mut(module_idx) {
            node.flags |= global_augmentation_flag;
        }

        module_idx
    }

    pub(crate) fn parse_nested_module_declaration(
        &mut self,
        modifiers: Option<NodeList>,
        is_ambient: bool,
    ) -> NodeIndex {
        let start_pos = self.token_pos();

        let name = if self.is_token(SyntaxKind::StringLiteral) {
            self.parse_string_literal()
        } else {
            // Allow keywords in dotted namespace segments (e.g., namespace chrome.debugger {})
            self.parse_identifier_name()
        };

        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_module_block(is_ambient)
        } else if self.is_token(SyntaxKind::DotToken) {
            self.next_token();
            self.parse_nested_module_declaration(modifiers.clone(), is_ambient)
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();

        self.arena.add_module(
            syntax_kind_ext::MODULE_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::node::ModuleData {
                modifiers,
                name,
                body,
            },
        )
    }

    /// Parse module block: { statements }
    /// `is_ambient`: true if this is a declare namespace/module, false for regular namespace
    pub(crate) fn parse_module_block(&mut self, is_ambient: bool) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        // Set ambient context flag for declare namespace/module body only
        // Clear IN_BLOCK flag since module body allows export/declare
        let saved_flags = self.context_flags;
        self.context_flags &= !crate::parser::state::CONTEXT_FLAG_IN_BLOCK;
        if is_ambient {
            self.context_flags |= crate::parser::state::CONTEXT_FLAG_AMBIENT;
        }

        let statements = self.parse_statements();

        // Restore context flags
        self.context_flags = saved_flags;

        self.parse_expected(SyntaxKind::CloseBraceToken);
        let end_pos = self.token_end();

        self.arena.add_module_block(
            syntax_kind_ext::MODULE_BLOCK,
            start_pos,
            end_pos,
            crate::parser::node::ModuleBlockData {
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
    pub(crate) fn parse_import_declaration(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ImportKeyword);

        // Check for import "module" (no import clause)
        let import_clause = if self.is_token(SyntaxKind::StringLiteral) {
            NodeIndex::NONE
        } else {
            self.parse_import_clause()
        };

        // Parse module specifier
        let module_specifier = if import_clause.is_none() {
            self.parse_string_literal()
        } else {
            if !self.is_token(SyntaxKind::FromKeyword) {
                self.parse_error_at_current_token(
                    "Import statement expects a 'from' clause.",
                    diagnostic_codes::IMPORT_EXPECTS_FROM_CLAUSE,
                );
            } else {
                self.parse_expected(SyntaxKind::FromKeyword);
            }
            self.parse_string_literal()
        };

        // Parse optional import attributes: with { ... } or assert { ... }
        let attributes = self.parse_import_attributes();

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
                attributes,
            },
        )
    }

    /// Parse optional import attributes: `with { type: "json" }` or `assert { type: "json" }`
    /// Returns `NodeIndex::NONE` if no attributes are present.
    pub(crate) fn parse_import_attributes(&mut self) -> NodeIndex {
        // Check for 'with' or 'assert' keyword (not on a new line to avoid ASI issues)
        if !self.is_token(SyntaxKind::WithKeyword) && !self.is_token(SyntaxKind::AssertKeyword) {
            return NodeIndex::NONE;
        }

        let start_pos = self.token_pos();
        let token = self.current_token as u16;
        self.next_token(); // consume 'with' or 'assert'

        if !self.is_token(SyntaxKind::OpenBraceToken) {
            self.error_token_expected("{");
            return NodeIndex::NONE;
        }

        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut elements = Vec::new();
        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let attr_start = self.token_pos();
            // Name can be identifier or string literal
            let name = if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else {
                self.parse_identifier_name()
            };
            self.parse_expected(SyntaxKind::ColonToken);
            let value = self.parse_assignment_expression();
            let attr_end = self
                .arena
                .get(value)
                .map_or_else(|| self.token_end(), |n| n.end);

            let attr_node = self.arena.add_import_attribute(
                syntax_kind_ext::IMPORT_ATTRIBUTE,
                attr_start,
                attr_end,
                crate::parser::node::ImportAttributeData { name, value },
            );
            elements.push(attr_node);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        self.parse_expected(SyntaxKind::CloseBraceToken);
        let end_pos = self.token_end();

        let node_list = self.make_node_list(elements);
        self.arena.add_import_attributes(
            syntax_kind_ext::IMPORT_ATTRIBUTES,
            start_pos,
            end_pos,
            crate::parser::node::ImportAttributesData {
                token,
                elements: node_list,
                multi_line: false,
            },
        )
    }

    /// Parse import clause: default, namespace, or named imports
    pub(crate) fn parse_import_clause(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut is_type_only = false;
        let mut is_deferred = false;

        // Check for "type" keyword modifier (import type { ... } / import type X from ...)
        // Disambiguation: `import type` can mean either:
        //   - `type` is a modifier: `import type X from '...'`, `import type { X } from '...'`
        //   - `type` is the default import name: `import type from '...'`
        //   - `type` is modifier with keyword as name: `import type from from '...'`
        if self.is_token(SyntaxKind::TypeKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            let saved_arena_len = self.arena.nodes.len();
            let saved_diagnostics_len = self.parse_diagnostics.len();
            self.next_token();

            if self.is_token(SyntaxKind::OpenBraceToken) || self.is_token(SyntaxKind::AsteriskToken)
            {
                // `import type { ... }` or `import type * as ...` — type is modifier
                is_type_only = true;
            } else if self.is_identifier_or_keyword() {
                // Could be `import type X from` (modifier + import name)
                // or `import type from '...'` (type is import name).
                // Look one more token ahead to disambiguate.
                self.next_token();
                if self.is_token(SyntaxKind::FromKeyword)
                    || self.is_token(SyntaxKind::CommaToken)
                    || self.is_token(SyntaxKind::EqualsToken)
                {
                    // `import type X from/,/=` — type is modifier
                    is_type_only = true;
                }
                // Restore either way (we'll re-parse the import name below)
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                self.arena.nodes.truncate(saved_arena_len);
                self.parse_diagnostics.truncate(saved_diagnostics_len);
                if is_type_only {
                    // Re-consume `type` token since it's the modifier
                    self.next_token();
                }
            } else {
                // Not an identifier/keyword after `type` — type is import name
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                self.arena.nodes.truncate(saved_arena_len);
                self.parse_diagnostics.truncate(saved_diagnostics_len);
            }
        }

        // Check for "defer" keyword (import defer * as ns from ...)
        if self.is_token(SyntaxKind::DeferKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            if self.is_token(SyntaxKind::Identifier) || self.is_token(SyntaxKind::AsteriskToken) {
                is_deferred = true;
            } else {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
            }
        }

        // Parse default import (identifier followed by "from" or ",")
        // For "import foo from", next token is "from"
        // For "import foo, { bar } from", next token is ","
        // Keywords can be used as default import names (e.g., `import defer from "mod"`)
        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        // Parse comma if we have both default and named/namespace
        if name.is_some() && self.parse_optional(SyntaxKind::CommaToken) {
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
                is_deferred,
                name,
                named_bindings,
            },
        )
    }

    /// Parse namespace import: * as name
    pub(crate) fn parse_namespace_import(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::AsteriskToken);
        self.parse_expected(SyntaxKind::AsKeyword);
        // Keywords can be used as namespace import names (e.g., `import * as import from "mod"`)
        let name = self.parse_identifier_name();
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
    pub(crate) fn parse_named_imports(&mut self) -> NodeIndex {
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

    /// Parse a module export name: either an identifier/keyword or a string literal.
    /// ES2022 allows string literals as import/export specifier names
    /// (arbitrary module namespace identifiers).
    pub(crate) fn parse_specifier_identifier_name(&mut self) -> NodeIndex {
        // ES2022: string literals are valid as module export names
        if self.is_token(SyntaxKind::StringLiteral) {
            return self.parse_string_literal();
        }

        let start_pos = self.token_pos();
        let end_pos = self.token_end();

        if self.is_identifier_or_keyword() {
            return self.parse_identifier_name();
        }

        self.error_identifier_expected();
        if !self.is_token(SyntaxKind::EndOfFileToken) {
            self.next_token();
        }

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
    }

    /// Parse import specifier: x or x as y or "str" as y
    pub(crate) fn parse_import_specifier(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut is_type_only = false;

        // Check for "type" keyword
        if self.is_token(SyntaxKind::TypeKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            // type-only if followed by identifier or string literal (ES2022)
            if self.is_token(SyntaxKind::Identifier) || self.is_token(SyntaxKind::StringLiteral) {
                is_type_only = true;
            } else {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
            }
        }

        let first_name = self.parse_specifier_identifier_name();

        // Check for "as" alias
        let (property_name, name) = if self.parse_optional(SyntaxKind::AsKeyword) {
            let alias = self.parse_specifier_identifier_name();
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

    // Export declarations and control flow statements → state_declarations_exports.rs
}
