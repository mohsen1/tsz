//! Parser state - interface, type alias, enum, module, import/export, and control flow parsing methods

use super::state::{CONTEXT_FLAG_DISALLOW_IN, ParserState};
use crate::parser::{
    NodeIndex, NodeList,
    node::{
        BlockData, CaseClauseData, CatchClauseData, EnumData, EnumMemberData, ExportAssignmentData,
        ExportDeclData, ExprStatementData, IdentifierData, IfStatementData, ImportClauseData,
        ImportDeclData, LiteralData, LoopData, NamedImportsData, ParameterData, ReturnData,
        SpecifierData, SwitchData, TryData, VariableData, VariableDeclarationData,
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
            if !member.is_none() {
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
                | SyntaxKind::AccessorKeyword
        ) && !self.look_ahead_is_property_name_after_keyword()
            && !self.look_ahead_has_line_break_after_keyword()
        {
            use tsz_common::diagnostics::diagnostic_codes;

            let modifier_text = self.scanner.get_token_text();

            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
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
            || self.is_property_name_keyword()
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

        // Parse colon and parameter type
        self.parse_expected(SyntaxKind::ColonToken);
        let param_type_token = self.token();
        let param_type = self.parse_type();

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

        // Detect known-invalid keyword types for index signature parameters.
        // TS1268 is emitted by the checker; the parser only tracks this to suppress
        // the TS1021 (missing type annotation) error when the param type is invalid.
        let has_invalid_param_type = matches!(
            param_type_token,
            SyntaxKind::AnyKeyword
                | SyntaxKind::BooleanKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::NeverKeyword
                | SyntaxKind::UnknownKeyword
                | SyntaxKind::ObjectKeyword
                | SyntaxKind::BigIntKeyword
                | SyntaxKind::UndefinedKeyword
        );

        // Index signatures must have a type annotation (TS1021).
        // Suppress when the parameter type is already invalid (TS1268),
        // or when other index signature errors were already emitted — matches tsc behavior.
        let has_param_errors = dot_dot_dot_token || question_token || has_multiple_params;
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
                modifiers: None,
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

            // Enum member names can be identifiers, string literals, or computed property names
            // Computed property names ([x]) are not valid in enums but we recover gracefully
            let name = if self.is_token(SyntaxKind::OpenBracketToken) {
                // Handle computed property name - emit TS1164 and recover
                self.parse_error_at_current_token(
                    "Computed property names are not allowed in enums.",
                    diagnostic_codes::COMPUTED_PROPERTY_NAMES_ARE_NOT_ALLOWED_IN_ENUMS,
                );
                self.parse_property_name()
            } else if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
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
                let next_token_starts_member = self.is_token(SyntaxKind::OpenBracketToken)
                    || self.is_token(SyntaxKind::StringLiteral)
                    || self.is_token(SyntaxKind::PrivateIdentifier)
                    || self.is_identifier_or_keyword();

                self.parse_error_at_current_token(
                    "An enum member name must be followed by a ',', '=', or '}'.",
                    diagnostic_codes::AN_ENUM_MEMBER_NAME_MUST_BE_FOLLOWED_BY_A_OR,
                );
                if next_token_starts_member {
                    continue;
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
                // emit TS1005 and continue parsing instead of breaking
                if self.is_token(SyntaxKind::Identifier)
                    || self.is_token(SyntaxKind::StringLiteral)
                    || self.is_token(SyntaxKind::PrivateIdentifier)
                    || self.is_token(SyntaxKind::OpenBracketToken)
                {
                    self.parse_error_at_current_token("',' expected", diagnostic_codes::EXPECTED);
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
            self.parse_expected(SyntaxKind::FromKeyword);
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
        if !name.is_none() && self.parse_optional(SyntaxKind::CommaToken) {
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

    /// Parse import specifier: x or x as y
    pub(crate) fn parse_import_specifier(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut is_type_only = false;

        // Check for "type" keyword
        if self.is_token(SyntaxKind::TypeKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            if self.is_token(SyntaxKind::Identifier) {
                is_type_only = true;
            } else {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
            }
        }

        let first_name = self.parse_identifier_name();

        // Check for "as" alias
        let (property_name, name) = if self.parse_optional(SyntaxKind::AsKeyword) {
            let alias = self.parse_identifier_name();
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

    /// Parse export declaration
    /// export { x, y };
    /// export { x } from "mod";
    /// export * from "mod";
    /// export default x;
    /// export function `f()` {}
    /// export class C {}
    pub(crate) fn parse_export_declaration(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ExportKeyword);

        // Check for type-only export vs export type alias
        // "export type { X }" or "export type * from" = type-only export
        // "export type X = Y" = exported type alias declaration
        let is_type_only = if self.is_token(SyntaxKind::TypeKeyword) {
            // Look ahead to see if this is a type-only export
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token(); // skip 'type'

            let is_type_only_export = self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::AsteriskToken);

            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if is_type_only_export {
                self.next_token(); // consume 'type' for type-only exports
                true
            } else {
                // Not a type-only export - leave 'type' for parse_export_declaration_or_statement
                false
            }
        } else {
            false
        };

        // export default ...
        if self.is_token(SyntaxKind::DefaultKeyword) {
            return self.parse_export_default(start_pos);
        }

        // export import X = Y (re-export of import equals)
        // export import X from "..." (ES6 import with export modifier — TS1191)
        if self.is_token(SyntaxKind::ImportKeyword) {
            if self.look_ahead_is_import_equals() {
                return self.parse_export_import_equals(start_pos);
            }
            // ES6 import with export modifier — emit TS1191 and parse as import
            self.parse_error_at_current_token(
                "An import declaration cannot have modifiers.",
                diagnostic_codes::AN_IMPORT_DECLARATION_CANNOT_HAVE_MODIFIERS,
            );
            let import_decl = self.parse_import_declaration();
            let end_pos = self.token_end();
            return self.arena.add_export_decl(
                syntax_kind_ext::EXPORT_DECLARATION,
                start_pos,
                end_pos,
                ExportDeclData {
                    modifiers: None,
                    is_type_only: false,
                    is_default_export: false,
                    export_clause: import_decl,
                    module_specifier: NodeIndex::NONE,
                    attributes: NodeIndex::NONE,
                },
            );
        }

        // export * from "mod"
        if self.is_token(SyntaxKind::AsteriskToken) {
            return self.parse_export_star(start_pos, is_type_only);
        }

        // export { ... }
        if self.is_token(SyntaxKind::OpenBraceToken) {
            return self.parse_export_named(start_pos, is_type_only);
        }

        // export = expression (CommonJS-style export)
        if self.is_token(SyntaxKind::EqualsToken) {
            return self.parse_export_assignment(start_pos);
        }

        // export as namespace Foo (UMD global namespace declaration)
        if self.is_token(SyntaxKind::AsKeyword) {
            return self.parse_namespace_export_declaration(start_pos);
        }

        // export function/class/const/let/var/interface/type/enum
        self.parse_export_declaration_or_statement(start_pos)
    }

    /// Parse export import X = Y (re-export of import equals declaration)
    pub(crate) fn parse_export_import_equals(&mut self, start_pos: u32) -> NodeIndex {
        // Parse the import equals declaration
        let import_decl = self.parse_import_equals_declaration();

        let end_pos = self.token_end();

        // Wrap in an export declaration
        self.arena.add_export_decl(
            syntax_kind_ext::EXPORT_DECLARATION,
            start_pos,
            end_pos,
            ExportDeclData {
                modifiers: None,
                is_type_only: false,
                is_default_export: false,
                export_clause: import_decl,
                module_specifier: NodeIndex::NONE,
                attributes: NodeIndex::NONE,
            },
        )
    }

    /// Parse export = expression (CommonJS-style default export)
    pub(crate) fn parse_export_assignment(&mut self, start_pos: u32) -> NodeIndex {
        self.parse_expected(SyntaxKind::EqualsToken);
        let expression = self.parse_assignment_expression();
        self.parse_semicolon();

        let end_pos = self.token_end();

        self.arena.add_export_assignment(
            syntax_kind_ext::EXPORT_ASSIGNMENT,
            start_pos,
            end_pos,
            ExportAssignmentData {
                modifiers: None,
                is_export_equals: true,
                expression,
            },
        )
    }

    /// Parse `export as namespace Foo;` (UMD global namespace declaration)
    ///
    /// This creates a `NamespaceExportDeclaration` node. The syntax declares that the module's
    /// exports are also available globally under the given namespace name.
    pub(crate) fn parse_namespace_export_declaration(&mut self, start_pos: u32) -> NodeIndex {
        self.parse_expected(SyntaxKind::AsKeyword);
        self.parse_expected(SyntaxKind::NamespaceKeyword);
        let name = self.parse_identifier();
        self.parse_semicolon();

        let end_pos = self.token_end();
        self.arena.add_export_decl(
            syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION,
            start_pos,
            end_pos,
            ExportDeclData {
                modifiers: None,
                is_type_only: false,
                is_default_export: false,
                export_clause: name,
                module_specifier: NodeIndex::NONE,
                attributes: NodeIndex::NONE,
            },
        )
    }

    /// Parse export default
    pub(crate) fn parse_export_default(&mut self, start_pos: u32) -> NodeIndex {
        self.parse_expected(SyntaxKind::DefaultKeyword);

        // Parse the default expression or declaration
        // For export default, function/class names are optional:
        //   export default function() {}    // valid, anonymous
        //   export default function foo() {} // valid, named
        let expression = match self.token() {
            SyntaxKind::FunctionKeyword => {
                self.parse_function_declaration_with_async_optional_name(false, None)
            }
            SyntaxKind::AsyncKeyword if self.look_ahead_is_async_function() => {
                self.next_token(); // consume 'async'
                self.parse_function_declaration_with_async_optional_name(true, None)
            }
            SyntaxKind::ClassKeyword => self.parse_class_declaration(),
            SyntaxKind::AbstractKeyword => self.parse_abstract_class_declaration(),
            SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(),
            _ => {
                let expr = self.parse_assignment_expression();
                self.parse_semicolon();
                expr
            }
        };

        let end_pos = self.token_end();
        // Use export assignment for default exports
        self.arena.add_export_decl(
            syntax_kind_ext::EXPORT_DECLARATION,
            start_pos,
            end_pos,
            ExportDeclData {
                modifiers: None,
                is_type_only: false,
                is_default_export: true,
                export_clause: expression,
                module_specifier: NodeIndex::NONE,
                attributes: NodeIndex::NONE,
            },
        )
    }

    /// Parse export * from "mod"
    pub(crate) fn parse_export_star(&mut self, start_pos: u32, is_type_only: bool) -> NodeIndex {
        self.parse_expected(SyntaxKind::AsteriskToken);

        // Optional "as namespace" for re-export (keywords allowed as names)
        let export_clause = if self.parse_optional(SyntaxKind::AsKeyword) {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        self.parse_expected(SyntaxKind::FromKeyword);
        let module_specifier = self.parse_string_literal();

        // Parse optional import attributes: with { ... } or assert { ... }
        let attributes = self.parse_import_attributes();

        self.parse_semicolon();

        let end_pos = self.token_end();
        self.arena.add_export_decl(
            syntax_kind_ext::EXPORT_DECLARATION,
            start_pos,
            end_pos,
            ExportDeclData {
                modifiers: None,
                is_type_only,
                is_default_export: false,
                export_clause,
                module_specifier,
                attributes,
            },
        )
    }

    /// Parse export { x, y } or export { x } from "mod"
    pub(crate) fn parse_export_named(&mut self, start_pos: u32, is_type_only: bool) -> NodeIndex {
        let export_clause = self.parse_named_exports();

        let module_specifier = if self.parse_optional(SyntaxKind::FromKeyword) {
            self.parse_string_literal()
        } else {
            NodeIndex::NONE
        };

        // Parse optional import attributes: with { ... } or assert { ... }
        let attributes = if module_specifier.is_none() {
            NodeIndex::NONE
        } else {
            self.parse_import_attributes()
        };

        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena.add_export_decl(
            syntax_kind_ext::EXPORT_DECLARATION,
            start_pos,
            end_pos,
            ExportDeclData {
                modifiers: None,
                is_type_only,
                is_default_export: false,
                export_clause,
                module_specifier,
                attributes,
            },
        )
    }

    /// Parse named exports: { x, y as z }
    pub(crate) fn parse_named_exports(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut elements = Vec::new();
        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            // Pattern 4: Import/Export specifier brace mismatch cascading error suppression
            // If we encounter 'from' keyword in the specifier list, it likely means we have:
            // export { a from "module"  (missing closing brace)
            // In this case, break the loop to avoid parsing 'from' as an identifier
            if self.is_token(SyntaxKind::FromKeyword) {
                break;
            }

            let spec = self.parse_export_specifier();
            elements.push(spec);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        self.parse_expected(SyntaxKind::CloseBraceToken);
        let end_pos = self.token_end();

        self.arena.add_named_imports(
            syntax_kind_ext::NAMED_EXPORTS,
            start_pos,
            end_pos,
            NamedImportsData {
                name: NodeIndex::NONE, // Not a namespace export
                elements: self.make_node_list(elements),
            },
        )
    }

    /// Parse export specifier: x or x as y
    pub(crate) fn parse_export_specifier(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut is_type_only = false;

        // Check for "type" keyword
        if self.is_token(SyntaxKind::TypeKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            if self.is_token(SyntaxKind::Identifier) {
                is_type_only = true;
            } else {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
            }
        }

        let first_name = self.parse_identifier_name();

        // Check for "as" alias
        let (property_name, name) = if self.parse_optional(SyntaxKind::AsKeyword) {
            let alias = self.parse_identifier_name();
            (first_name, alias)
        } else {
            (NodeIndex::NONE, first_name)
        };

        let end_pos = self.token_end();
        self.arena.add_specifier(
            syntax_kind_ext::EXPORT_SPECIFIER,
            start_pos,
            end_pos,
            SpecifierData {
                is_type_only,
                property_name,
                name,
            },
        )
    }

    /// Parse exported declaration (export function, export class, etc.)
    pub(crate) fn parse_export_declaration_or_statement(&mut self, start_pos: u32) -> NodeIndex {
        let declaration = self.parse_exported_declaration(start_pos);

        let end_pos = self.token_end();
        self.arena.add_export_decl(
            syntax_kind_ext::EXPORT_DECLARATION,
            start_pos,
            end_pos,
            ExportDeclData {
                modifiers: None,
                is_type_only: false,
                is_default_export: false,
                export_clause: declaration,
                module_specifier: NodeIndex::NONE,
                attributes: NodeIndex::NONE,
            },
        )
    }

    fn parse_exported_declaration(&mut self, start_pos: u32) -> NodeIndex {
        match self.token() {
            SyntaxKind::FunctionKeyword => self.parse_function_declaration(),
            SyntaxKind::AsyncKeyword => self.parse_export_async_declaration_or_expression(),
            SyntaxKind::ClassKeyword => self.parse_class_declaration(),
            SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(),
            SyntaxKind::TypeKeyword => self.parse_type_alias_declaration(),
            SyntaxKind::EnumKeyword => self.parse_enum_declaration(),
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                self.parse_module_declaration()
            }
            SyntaxKind::AbstractKeyword => self.parse_abstract_class_declaration(),
            SyntaxKind::DeclareKeyword => self.parse_export_declare_declaration(start_pos),
            SyntaxKind::VarKeyword
            | SyntaxKind::LetKeyword
            | SyntaxKind::UsingKeyword
            | SyntaxKind::AwaitKeyword => self.parse_variable_statement(),
            SyntaxKind::ConstKeyword => self.parse_export_const_or_variable(),
            SyntaxKind::AtToken => self.parse_export_decorated_declaration(),
            _ => {
                self.error_statement_expected();
                self.parse_expression_statement()
            }
        }
    }

    fn parse_export_async_declaration_or_expression(&mut self) -> NodeIndex {
        if self.look_ahead_is_async_function() {
            self.parse_async_function_declaration()
        } else if self.look_ahead_is_async_declaration() {
            let async_start_pos = self.token_pos();
            // TS1042 is reported by the checker (checkGrammarModifiers), not the parser
            let async_start = self.token_pos();
            self.parse_expected(SyntaxKind::AsyncKeyword);
            let async_end = self.token_end();
            let async_modifier =
                self.arena
                    .add_token(SyntaxKind::AsyncKeyword as u16, async_start, async_end);
            let modifiers = Some(self.make_node_list(vec![async_modifier]));
            match self.token() {
                SyntaxKind::ClassKeyword => {
                    self.parse_class_declaration_with_modifiers(async_start_pos, modifiers)
                }
                SyntaxKind::EnumKeyword => {
                    self.parse_enum_declaration_with_modifiers(async_start_pos, modifiers)
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
            self.parse_expression_statement()
        }
    }

    fn parse_export_declare_declaration(&mut self, start_pos: u32) -> NodeIndex {
        // export declare function/class/namespace/var/etc.
        // Create an export modifier to pass to the ambient declaration
        // The export keyword was already consumed in parse_export_declaration
        // We need to create a token for it at the start_pos
        let export_modifier =
            self.arena
                .add_token(SyntaxKind::ExportKeyword as u16, start_pos, start_pos + 6); // "export" is 6 chars
        self.parse_ambient_declaration_with_modifiers(vec![export_modifier])
    }

    fn parse_export_const_or_variable(&mut self) -> NodeIndex {
        // export const enum or export const variable
        if self.look_ahead_is_const_enum() {
            self.parse_const_enum_declaration(self.token_pos(), Vec::new())
        } else {
            self.parse_variable_statement()
        }
    }

    fn parse_export_decorated_declaration(&mut self) -> NodeIndex {
        // export @decorator class Foo {}
        let decorators = self.parse_decorators();
        match self.token() {
            SyntaxKind::ClassKeyword => {
                self.parse_class_declaration_with_decorators(decorators, self.token_pos())
            }
            SyntaxKind::AbstractKeyword => {
                self.parse_abstract_class_declaration_with_decorators(decorators, self.token_pos())
            }
            _ => {
                self.error_statement_expected();
                self.parse_expression_statement()
            }
        }
    }

    /// Parse a string literal (used for module specifiers)
    pub(crate) fn parse_string_literal(&mut self) -> NodeIndex {
        if !self.is_token(SyntaxKind::StringLiteral) {
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at_current_token(
                diagnostic_messages::STRING_LITERAL_EXPECTED,
                diagnostic_codes::STRING_LITERAL_EXPECTED,
            );
            return NodeIndex::NONE;
        }

        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        let text = self.scanner.get_token_value_ref().to_string();
        let raw_text = self.scanner.get_token_text_ref().to_string();
        self.report_invalid_string_or_template_escape_errors();
        self.next_token();

        self.arena.add_literal(
            SyntaxKind::StringLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: Some(raw_text),
                value: None,
            },
        )
    }

    /// Parse if statement
    pub(crate) fn parse_if_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::IfKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);

        let expression = self.parse_expression();

        // Check for missing condition expression: if () { }
        if expression == NodeIndex::NONE {
            self.error_expression_expected();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);

        let then_statement =
            if self.is_token(SyntaxKind::Unknown) || self.is_token(SyntaxKind::AsteriskToken) {
                while self.is_token(SyntaxKind::Unknown) {
                    self.next_token();
                }
                // Recovery for malformed `if (a) * expr;` cases: parse an empty
                // then-statement and let `* expr;` be parsed as the next statement.
                self.arena.add_token(
                    syntax_kind_ext::EMPTY_STATEMENT,
                    self.token_pos(),
                    self.token_pos(),
                )
            } else {
                self.parse_statement()
            };
        self.check_using_outside_block(then_statement);

        // TS1313: Check if the body of the if statement is an empty statement
        if let Some(node) = self.arena.get(then_statement)
            && node.kind == syntax_kind_ext::EMPTY_STATEMENT
        {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(
                node.pos,
                node.end - node.pos,
                "The body of an 'if' statement cannot be the empty statement.",
                diagnostic_codes::THE_BODY_OF_AN_IF_STATEMENT_CANNOT_BE_THE_EMPTY_STATEMENT,
            );
        }

        let else_statement = if self.parse_optional(SyntaxKind::ElseKeyword) {
            let stmt = self.parse_statement();
            self.check_using_outside_block(stmt);
            stmt
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_if_statement(
            syntax_kind_ext::IF_STATEMENT,
            start_pos,
            end_pos,
            IfStatementData {
                expression,
                then_statement,
                else_statement,
            },
        )
    }

    /// Parse return statement
    pub(crate) fn parse_return_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ReturnKeyword);

        // For restricted productions (return), ASI applies immediately after line break
        // Use can_parse_semicolon_for_restricted_production() instead of can_parse_semicolon()
        let expression = if self.can_parse_semicolon_for_restricted_production() {
            NodeIndex::NONE
        } else {
            self.parse_expression()
        };

        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena.add_return(
            syntax_kind_ext::RETURN_STATEMENT,
            start_pos,
            end_pos,
            ReturnData { expression },
        )
    }

    /// Parse while statement
    pub(crate) fn parse_while_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::WhileKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);

        let condition = self.parse_expression();

        // Check for missing while condition: while () { }
        if condition == NodeIndex::NONE {
            self.error_expression_expected();
        }

        // Error recovery: if condition parsing failed badly, resync to close paren
        if condition.is_none() && !self.is_token(SyntaxKind::CloseParenToken) {
            self.resync_after_error();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);

        let statement = self.parse_statement();
        self.check_using_outside_block(statement);

        let end_pos = self.token_end();
        self.arena.add_loop(
            syntax_kind_ext::WHILE_STATEMENT,
            start_pos,
            end_pos,
            LoopData {
                initializer: NodeIndex::NONE,
                condition,
                incrementor: NodeIndex::NONE,
                statement,
            },
        )
    }

    /// Parse for statement (basic for loop only, not for-in/for-of yet)
    pub(crate) fn parse_for_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ForKeyword);

        // Check for for-await-of: for await (...)
        let await_modifier = self.parse_optional(SyntaxKind::AwaitKeyword);

        self.parse_expected(SyntaxKind::OpenParenToken);

        // Parse initializer (can be var/let/const declaration or expression)
        // Disallow 'in' as a binary operator so it's recognized as the for-in keyword
        let saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_DISALLOW_IN;
        let initializer = if self.is_token(SyntaxKind::SemicolonToken) {
            NodeIndex::NONE
        } else if self.is_token(SyntaxKind::VarKeyword)
            || self.is_token(SyntaxKind::LetKeyword)
            || self.is_token(SyntaxKind::ConstKeyword)
            || self.is_token(SyntaxKind::UsingKeyword)
            || (self.is_token(SyntaxKind::AwaitKeyword) && self.look_ahead_is_await_using())
        {
            self.parse_for_variable_declaration()
        } else {
            self.parse_expression()
        };
        self.context_flags = saved_flags;

        // Error recovery: if initializer parsing failed badly, resync to semicolon
        if initializer.is_none()
            && !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::InKeyword)
            && !self.is_token(SyntaxKind::OfKeyword)
        {
            if self.is_token(SyntaxKind::CloseParenToken) {
                // for () — empty parens. Emit TS1109 and skip to after )
                self.error_expression_expected();
                self.next_token(); // consume )
                let body = self.parse_statement();
                let end_pos = self.token_end();
                return self.arena.add_loop(
                    syntax_kind_ext::FOR_STATEMENT,
                    start_pos,
                    end_pos,
                    LoopData {
                        initializer: NodeIndex::NONE,
                        condition: NodeIndex::NONE,
                        incrementor: NodeIndex::NONE,
                        statement: body,
                    },
                );
            }
            self.resync_after_error();
        }

        // Check for for-in or for-of
        if self.is_token(SyntaxKind::InKeyword) {
            // TS1005: for-await can only be used with 'of', not 'in'
            if await_modifier {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'of' expected.", diagnostic_codes::EXPECTED);
            }
            return self.parse_for_in_statement_rest(start_pos, initializer);
        }
        if self.is_token(SyntaxKind::OfKeyword) {
            return self.parse_for_of_statement_rest(start_pos, initializer, await_modifier);
        }

        // Regular for statement: for (init; cond; incr)
        self.parse_expected(SyntaxKind::SemicolonToken);

        // Condition
        let condition = if self.is_token(SyntaxKind::SemicolonToken) {
            NodeIndex::NONE
        } else {
            let cond = self.parse_expression();

            // Check for missing for condition: for (init; ; incr) when there was content to parse
            if cond == NodeIndex::NONE {
                self.error_expression_expected();
            }

            cond
        };

        // Error recovery: if condition parsing failed badly, resync to semicolon
        if condition.is_none()
            && !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::CloseParenToken)
        {
            self.resync_after_error();
        }

        self.parse_expected(SyntaxKind::SemicolonToken);

        // Incrementor
        let incrementor = if self.is_token(SyntaxKind::CloseParenToken) {
            NodeIndex::NONE
        } else {
            let incr = self.parse_expression();

            // Check for missing for incrementor: for (init; cond; ) when there was content to parse
            if incr == NodeIndex::NONE {
                self.error_expression_expected();
            }

            incr
        };

        // Error recovery: if incrementor parsing failed badly, resync to close paren
        if incrementor.is_none() && !self.is_token(SyntaxKind::CloseParenToken) {
            self.resync_after_error();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);

        let statement = self.parse_statement();

        let end_pos = self.token_end();
        self.arena.add_loop(
            syntax_kind_ext::FOR_STATEMENT,
            start_pos,
            end_pos,
            LoopData {
                initializer,
                condition,
                incrementor,
                statement,
            },
        )
    }

    pub(crate) fn parse_for_variable_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let (_, flags) = self.parse_for_variable_declaration_declaration_keyword();
        let declarations = self.parse_for_variable_declarations();
        let declarations_list = self.make_node_list(declarations);
        let end_pos = self.token_end();

        self.arena.add_variable_with_flags(
            syntax_kind_ext::VARIABLE_DECLARATION_LIST,
            start_pos,
            end_pos,
            VariableData {
                modifiers: None,
                declarations: declarations_list,
            },
            flags,
        )
    }

    fn parse_for_variable_declaration_declaration_keyword(&mut self) -> (SyntaxKind, u16) {
        use crate::parser::node_flags;

        let decl_keyword = self.token();
        let flags = match decl_keyword {
            SyntaxKind::LetKeyword => self.u16_from_node_flags(node_flags::LET),
            SyntaxKind::ConstKeyword => self.u16_from_node_flags(node_flags::CONST),
            SyntaxKind::UsingKeyword => self.u16_from_node_flags(node_flags::USING),
            SyntaxKind::AwaitKeyword => {
                // await using declaration in for loops
                self.next_token(); // consume 'await'
                self.parse_expected(SyntaxKind::UsingKeyword); // consume 'using'
                self.u16_from_node_flags(node_flags::AWAIT_USING)
            }
            _ => 0,
        };

        if decl_keyword != SyntaxKind::AwaitKeyword {
            self.next_token(); // consume var/let/const/using
        }

        (decl_keyword, flags)
    }

    fn parse_for_variable_declarations(&mut self) -> Vec<NodeIndex> {
        use tsz_common::diagnostics::diagnostic_codes;

        if self.is_for_variable_declaration_empty() {
            self.parse_error_at_current_token(
                "Variable declaration list cannot be empty.",
                diagnostic_codes::VARIABLE_DECLARATION_LIST_CANNOT_BE_EMPTY,
            );

            return Vec::new();
        }

        let mut declarations = Vec::new();
        loop {
            declarations.push(self.parse_for_variable_declaration_entry());
            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }
        declarations
    }

    fn parse_for_variable_declaration_entry(&mut self) -> NodeIndex {
        let decl_start = self.token_pos();

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

        // Optional type annotation
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        // Optional initializer
        let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
            self.parse_assignment_expression()
        } else {
            NodeIndex::NONE
        };

        self.arena.add_variable_declaration(
            syntax_kind_ext::VARIABLE_DECLARATION,
            decl_start,
            self.token_end(),
            VariableDeclarationData {
                name,
                exclamation_token,
                type_annotation,
                initializer,
            },
        )
    }

    fn is_for_variable_declaration_empty(&mut self) -> bool {
        if self.is_token(SyntaxKind::InKeyword) {
            return true;
        }

        if self.is_token(SyntaxKind::OfKeyword) {
            // Look ahead to see if 'of' is used as a variable name
            let snapshot = self.scanner.save_state();
            let saved_token = self.current_token;
            self.next_token(); // skip 'of'
            let next = self.token();
            let is_var_name = matches!(
                next,
                SyntaxKind::SemicolonToken
                    | SyntaxKind::CommaToken
                    | SyntaxKind::EqualsToken
                    | SyntaxKind::ColonToken
                    | SyntaxKind::CloseParenToken
                    | SyntaxKind::InKeyword
                    | SyntaxKind::OfKeyword
                    | SyntaxKind::ExclamationToken
            );
            self.scanner.restore_state(snapshot);
            self.current_token = saved_token;
            return !is_var_name;
        }

        false
    }

    /// Parse for-in statement after initializer: for (x in obj)
    pub(crate) fn parse_for_in_statement_rest(
        &mut self,
        start_pos: u32,
        initializer: NodeIndex,
    ) -> NodeIndex {
        // Check for multiple variable declarations in for-in: for (var a, b in X)
        // TSC emits TS1091 "Only a single variable declaration is allowed in a 'for...in' statement"
        if let Some(node) = self.arena.get(initializer)
            && node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            && let Some(data) = self.arena.get_variable(node)
            && data.declarations.nodes.len() > 1
        {
            use tsz_common::diagnostics::diagnostic_codes;
            // Report error at the second declaration
            if let Some(&second_decl) = data.declarations.nodes.get(1)
                && let Some(second_node) = self.arena.get(second_decl)
            {
                self.parse_error_at(
                                    second_node.pos,
                                    second_node.end - second_node.pos,
                                    "Only a single variable declaration is allowed in a 'for...in' statement.",
                                    diagnostic_codes::ONLY_A_SINGLE_VARIABLE_DECLARATION_IS_ALLOWED_IN_A_FOR_IN_STATEMENT,
                                );
            }
        }
        self.parse_expected(SyntaxKind::InKeyword);
        let expression = self.parse_expression();
        self.parse_expected(SyntaxKind::CloseParenToken);
        let statement = self.parse_statement();
        self.check_using_outside_block(statement);

        let end_pos = self.token_end();
        self.arena.add_for_in_of(
            syntax_kind_ext::FOR_IN_STATEMENT,
            start_pos,
            end_pos,
            crate::parser::node::ForInOfData {
                await_modifier: false,
                initializer,
                expression,
                statement,
            },
        )
    }

    /// Parse for-of statement after initializer: for (x of arr)
    pub(crate) fn parse_for_of_statement_rest(
        &mut self,
        start_pos: u32,
        initializer: NodeIndex,
        await_modifier: bool,
    ) -> NodeIndex {
        // Check for multiple variable declarations in for-of: for (var a, b of X)
        // TSC emits TS1188 "Only a single variable declaration is allowed in a 'for...of' statement"
        if let Some(node) = self.arena.get(initializer)
            && node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            && let Some(data) = self.arena.get_variable(node)
            && data.declarations.nodes.len() > 1
        {
            use tsz_common::diagnostics::diagnostic_codes;
            // Report error at the second declaration
            if let Some(&second_decl) = data.declarations.nodes.get(1)
                && let Some(second_node) = self.arena.get(second_decl)
            {
                self.parse_error_at(
                                    second_node.pos,
                                    second_node.end - second_node.pos,
                                    "Only a single variable declaration is allowed in a 'for...of' statement.",
                                    diagnostic_codes::ONLY_A_SINGLE_VARIABLE_DECLARATION_IS_ALLOWED_IN_A_FOR_OF_STATEMENT,
                                );
            }
        }
        self.parse_expected(SyntaxKind::OfKeyword);
        let expression = self.parse_assignment_expression();
        self.parse_expected(SyntaxKind::CloseParenToken);
        let statement = self.parse_statement();
        self.check_using_outside_block(statement);

        let end_pos = self.token_end();
        self.arena.add_for_in_of(
            syntax_kind_ext::FOR_OF_STATEMENT,
            start_pos,
            end_pos,
            crate::parser::node::ForInOfData {
                await_modifier,
                initializer,
                expression,
                statement,
            },
        )
    }

    /// Parse break statement
    pub(crate) fn parse_break_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::BreakKeyword);

        // For restricted productions (break), ASI applies immediately after line break
        // Use can_parse_semicolon_for_restricted_production() instead of can_parse_semicolon()
        // Optional label
        let label = if !self.can_parse_semicolon_for_restricted_production()
            && self.is_identifier_or_keyword()
        {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena.add_jump(
            syntax_kind_ext::BREAK_STATEMENT,
            start_pos,
            end_pos,
            crate::parser::node::JumpData { label },
        )
    }

    /// Parse continue statement
    pub(crate) fn parse_continue_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ContinueKeyword);

        // For restricted productions (continue), ASI applies immediately after line break
        // Use can_parse_semicolon_for_restricted_production() instead of can_parse_semicolon()
        // Optional label
        let label = if !self.can_parse_semicolon_for_restricted_production()
            && self.is_identifier_or_keyword()
        {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena.add_jump(
            syntax_kind_ext::CONTINUE_STATEMENT,
            start_pos,
            end_pos,
            crate::parser::node::JumpData { label },
        )
    }

    /// Parse throw statement
    pub(crate) fn parse_throw_statement(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ThrowKeyword);

        // TypeScript requires an expression after throw
        // If there's a line break immediately after throw, emit TS1109 (EXPRESSION_EXPECTED)
        let expression = if self.scanner.has_preceding_line_break()
            && !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            // Line break after throw without semicolon/brace/EOF - emit error
            let start = self.token_pos();
            let end = self.token_end();
            self.parse_error_at(
                start,
                end - start,
                "Expression expected",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            NodeIndex::NONE
        } else if self.is_token(SyntaxKind::SemicolonToken)
            || self.is_token(SyntaxKind::CloseBraceToken)
            || self.is_token(SyntaxKind::EndOfFileToken)
        {
            // Explicit semicolon, closing brace, or EOF after throw without expression
            // TypeScript requires an expression after throw
            let start = self.token_pos();
            let end = self.token_end();
            self.parse_error_at(
                start,
                end - start,
                "Expression expected",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            NodeIndex::NONE
        } else if !self.can_parse_semicolon_for_restricted_production() {
            self.parse_expression()
        } else {
            NodeIndex::NONE
        };

        self.parse_semicolon();
        let end_pos = self.token_end();

        // Use return statement node type for throw (same structure)
        self.arena.add_return(
            syntax_kind_ext::THROW_STATEMENT,
            start_pos,
            end_pos,
            ReturnData { expression },
        )
    }

    /// Parse do-while statement
    pub(crate) fn parse_do_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::DoKeyword);

        let statement = self.parse_statement();
        self.check_using_outside_block(statement);

        self.parse_expected(SyntaxKind::WhileKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);
        let condition = self.parse_expression();

        // Check for missing condition expression: do { } while ()
        if condition == NodeIndex::NONE {
            self.error_expression_expected();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);

        // Per ECMAScript spec, semicolons are always auto-inserted after do-while.
        // TypeScript uses parseOptional(SemicolonToken) here, not parseSemicolon().
        self.parse_optional(SyntaxKind::SemicolonToken);
        let end_pos = self.token_end();

        self.arena.add_loop(
            syntax_kind_ext::DO_STATEMENT,
            start_pos,
            end_pos,
            LoopData {
                initializer: NodeIndex::NONE,
                condition,
                incrementor: NodeIndex::NONE,
                statement,
            },
        )
    }

    /// Parse switch statement
    pub(crate) fn parse_switch_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::SwitchKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);

        let expression = self.parse_expression();
        if expression == NodeIndex::NONE {
            self.error_expression_expected();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let clauses = self.parse_switch_case_clauses();

        let case_block_end = self.token_end();
        self.parse_expected(SyntaxKind::CloseBraceToken);
        let end_pos = self.token_end();

        let case_block = self.arena.add_block(
            syntax_kind_ext::CASE_BLOCK,
            start_pos,
            case_block_end,
            BlockData {
                statements: self.make_node_list(clauses),
                multi_line: true,
            },
        );

        self.arena.add_switch(
            syntax_kind_ext::SWITCH_STATEMENT,
            start_pos,
            end_pos,
            SwitchData {
                expression,
                case_block,
            },
        )
    }

    fn parse_switch_case_clauses(&mut self) -> Vec<NodeIndex> {
        let mut clauses = Vec::new();
        let mut seen_default = false;
        let mut reported_duplicate_default = false;
        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            if self.is_token(SyntaxKind::CaseKeyword) {
                clauses.push(self.parse_switch_case_clause());
            } else if self.is_token(SyntaxKind::DefaultKeyword) {
                clauses.push(self.parse_switch_default_clause(
                    &mut seen_default,
                    &mut reported_duplicate_default,
                ));
            } else {
                self.parse_switch_case_recovery();
            }
        }
        clauses
    }

    fn parse_switch_case_clause(&mut self) -> NodeIndex {
        let clause_start = self.token_pos();
        self.next_token();
        let clause_expr = self.parse_expression();
        if clause_expr == NodeIndex::NONE {
            self.error_expression_expected();
        }
        self.parse_expected(SyntaxKind::ColonToken);

        let statements = self.parse_switch_clause_statements();
        let clause_end = self.token_end();
        self.arena.add_case_clause(
            syntax_kind_ext::CASE_CLAUSE,
            clause_start,
            clause_end,
            CaseClauseData {
                expression: clause_expr,
                statements: self.make_node_list(statements),
            },
        )
    }

    fn parse_switch_default_clause(
        &mut self,
        seen_default: &mut bool,
        reported_duplicate_default: &mut bool,
    ) -> NodeIndex {
        let clause_start = self.token_pos();
        if *seen_default && !*reported_duplicate_default {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "A 'default' clause cannot appear more than once in a 'switch' statement.",
                diagnostic_codes::A_DEFAULT_CLAUSE_CANNOT_APPEAR_MORE_THAN_ONCE_IN_A_SWITCH_STATEMENT,
            );
            *reported_duplicate_default = true;
        }
        *seen_default = true;

        self.next_token();
        self.parse_expected(SyntaxKind::ColonToken);
        let statements = self.parse_switch_clause_statements();
        let clause_end = self.token_end();

        self.arena.add_case_clause(
            syntax_kind_ext::DEFAULT_CLAUSE,
            clause_start,
            clause_end,
            CaseClauseData {
                expression: NodeIndex::NONE,
                statements: self.make_node_list(statements),
            },
        )
    }

    fn parse_switch_clause_statements(&mut self) -> Vec<NodeIndex> {
        let mut statements = Vec::new();
        while !self.is_token(SyntaxKind::CaseKeyword)
            && !self.is_token(SyntaxKind::DefaultKeyword)
            && !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let pos_before = self.token_pos();
            let statement = self.parse_statement();
            if !statement.is_none() {
                statements.push(statement);
            }
            if self.token_pos() == pos_before {
                self.next_token();
            }
        }
        statements
    }

    fn parse_switch_case_recovery(&mut self) {
        if self.token_pos() != self.last_error_pos {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "case or default expected.",
                diagnostic_codes::EXPECTED,
            );
        }
        self.next_token();
    }

    /// Parse try statement
    /// Parse orphan catch/finally block (missing try)
    /// Emits TS1005: 'try' expected
    /// Special case: if catch is followed by finally, absorb both with one error
    pub(crate) fn parse_orphan_catch_or_finally_block(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        // Emit TS1005: 'try' expected
        self.parse_error_at_current_token("'try' expected.", diagnostic_codes::EXPECTED);

        let start_pos = self.token_pos();

        // Skip the catch/finally keyword
        let is_catch = self.is_token(SyntaxKind::CatchKeyword);
        self.next_token();

        // Skip the catch binding if present: catch(x)
        if is_catch && self.is_token(SyntaxKind::OpenParenToken) {
            self.next_token();
            // Skip everything until closing paren
            let mut depth = 1;
            while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
                if self.is_token(SyntaxKind::OpenParenToken) {
                    depth += 1;
                } else if self.is_token(SyntaxKind::CloseParenToken) {
                    depth -= 1;
                }
                if depth > 0 {
                    self.next_token();
                }
            }
            if self.is_token(SyntaxKind::CloseParenToken) {
                self.next_token();
            }
        }

        // Skip the block if present
        if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block();
        }

        // TypeScript error recovery: if this was catch and next is finally, absorb it
        // This prevents duplicate errors for "catch { } finally { }" pattern
        if is_catch && self.is_token(SyntaxKind::FinallyKeyword) {
            self.next_token(); // skip 'finally' keyword
            // Skip the finally block if present
            if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_block();
            }
        }

        let end_pos = self.token_end();

        // Return an empty statement as recovery
        self.arena
            .add_token(syntax_kind_ext::EMPTY_STATEMENT, start_pos, end_pos)
    }

    pub(crate) fn parse_try_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::TryKeyword);

        let try_block = self.parse_block();

        // Parse catch clause
        let catch_clause = if self.is_token(SyntaxKind::CatchKeyword) {
            let catch_start = self.token_pos();
            self.next_token();

            // Parse optional catch binding
            let variable_declaration = if self.is_token(SyntaxKind::OpenParenToken) {
                self.next_token();
                let decl = if self.is_token(SyntaxKind::CloseParenToken) {
                    NodeIndex::NONE
                } else {
                    // Pass flag 0x8 (CATCH_CLAUSE_BINDING) to suppress TS1182
                    // since catch bindings are destructuring without initializers
                    self.parse_variable_declaration_with_flags(0x8)
                };
                self.parse_expected(SyntaxKind::CloseParenToken);
                decl
            } else {
                NodeIndex::NONE
            };

            let catch_block = self.parse_block();
            let catch_end = self.token_end();

            self.arena.add_catch_clause(
                syntax_kind_ext::CATCH_CLAUSE,
                catch_start,
                catch_end,
                CatchClauseData {
                    variable_declaration,
                    block: catch_block,
                },
            )
        } else {
            NodeIndex::NONE
        };

        // Parse finally clause
        let finally_block = if self.is_token(SyntaxKind::FinallyKeyword) {
            self.next_token();
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        // Error recovery: try without catch or finally is invalid
        if catch_clause.is_none()
            && finally_block.is_none()
            && self.token_pos() != self.last_error_pos
        {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "catch or finally expected.",
                diagnostic_codes::CATCH_OR_FINALLY_EXPECTED,
            );
        }

        let end_pos = self.token_end();
        self.arena.add_try(
            syntax_kind_ext::TRY_STATEMENT,
            start_pos,
            end_pos,
            TryData {
                try_block,
                catch_clause,
                finally_block,
            },
        )
    }

    /// Parse with statement
    pub(crate) fn parse_with_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::WithKeyword);

        self.parse_expected(SyntaxKind::OpenParenToken);

        let expression = self.parse_expression();

        // Check for missing with expression: with () { }
        if expression == NodeIndex::NONE {
            self.error_expression_expected();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);

        let statement = self.parse_statement();

        let end_pos = self.token_end();

        // Use if statement structure for with (expression + statement)
        self.arena.add_if_statement(
            syntax_kind_ext::WITH_STATEMENT,
            start_pos,
            end_pos,
            IfStatementData {
                expression,
                then_statement: statement,
                else_statement: NodeIndex::NONE,
            },
        )
    }

    /// Parse debugger statement
    pub(crate) fn parse_debugger_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::DebuggerKeyword);
        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena
            .add_token(syntax_kind_ext::DEBUGGER_STATEMENT, start_pos, end_pos)
    }

    /// Parse expression statement
    pub(crate) fn parse_expression_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Early rejection: If the current token cannot start an expression, fail immediately
        // This prevents TS1109 from being emitted for tokens that are obviously not expressions
        // (e.g., }, ], ), etc.) when we fall through to parse_expression_statement() from
        // parse_statement()'s wildcard match.
        if !self.is_expression_start() {
            // Don't emit error here - let the statement-level error handling deal with it
            // Just return NONE to indicate failure
            return NodeIndex::NONE;
        }

        let expression = self.parse_expression();

        // If expression parsing failed completely, resync to recover
        if expression.is_none() {
            // Emit error for unexpected token if we haven't already
            if self.token_pos() != self.last_error_pos && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
            }
            // Try to parse semicolon for partial recovery, then resync
            let _ = self.can_parse_semicolon();
            if self.is_token(SyntaxKind::SemicolonToken) {
                self.next_token();
            } else {
                self.resync_after_error();
            }
            return NodeIndex::NONE;
        }

        // Use smart error reporting for missing semicolons (matches TypeScript's
        // parseExpressionOrLabeledStatement behavior). Instead of generic TS1005 "';' expected",
        // this checks if the expression is a misspelled keyword and emits TS1435/TS1434.
        if self.is_token(SyntaxKind::SemicolonToken) {
            self.next_token();
        } else if !self.can_parse_semicolon() {
            self.parse_error_for_missing_semicolon_after(expression);
            // Recovery for malformed fragments like `this.x: any;`.
            // Consume stray `:` so the following token can still be parsed as
            // a standalone expression statement on the next iteration.
            if self.is_token(SyntaxKind::ColonToken) {
                self.next_token();
            }
        }
        let end_pos = self.token_end();

        self.arena.add_expr_statement(
            syntax_kind_ext::EXPRESSION_STATEMENT,
            start_pos,
            end_pos,
            ExprStatementData { expression },
        )
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
    fn parse_declaration_modules_with_generic_and_type_aliases() {
        let (parser, root) = parse_source(
            "declare module 'mod' {\n  export interface Alias<T> {\n    value: T;\n  }\n}\ndeclare function ready(): void;\n",
        );
        assert_eq!(parser.get_diagnostics().len(), 0);
        let sf = parser.get_arena().get_source_file_at(root).unwrap();
        assert_eq!(sf.statements.nodes.len(), 2);
    }

    #[test]
    fn parse_declaration_with_recovery_for_invalid_member() {
        let (parser, root) = parse_source(
            "declare namespace NS {\n  export interface I {\n    prop: string = 1;\n  }\n}\n",
        );
        assert!(!parser.get_diagnostics().is_empty());
        let sf = parser.get_arena().get_source_file_at(root).unwrap();
        assert_eq!(sf.statements.nodes.len(), 1);
    }

    #[test]
    fn parse_import_equals_declaration_with_targeted_error_recovery() {
        let (parser, _root) = parse_source("import = 'invalid';\nfunction ok() { return 1; }");
        assert!(!parser.get_diagnostics().is_empty());
    }

    #[test]
    fn parse_namespace_recovery_from_missing_closing_brace() {
        let (parser, _root) = parse_source("namespace Recover {\\n  export const value = 1;\\n");
        assert!(
            !parser.get_diagnostics().is_empty(),
            "expected diagnostics for unclosed namespace body"
        );
    }
}
