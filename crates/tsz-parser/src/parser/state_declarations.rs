//! Parser state - interface, type alias, enum, module, import/export, and control flow parsing methods

use super::state::ParserState;
use crate::parser::{
    NodeIndex, NodeList,
    node::{EnumData, EnumMemberData, IdentifierData, ParameterData},
    syntax_kind_ext,
};
use tsz_common::interner::Atom;
use tsz_scanner::SyntaxKind;

fn is_reserved_interface_type_name(name: &str) -> bool {
    matches!(
        name,
        "any"
            | "unknown"
            | "never"
            | "string"
            | "number"
            | "boolean"
            | "symbol"
            | "bigint"
            | "void"
            | "undefined"
            | "null"
            | "object"
    )
}

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
        let mut has_invalid_numeric_name = false;
        let mut has_invalid_hard_keyword_name = false;

        // Parse interface name - keywords like 'string', 'abstract' can be used as interface names
        // Type keywords like 'void' are parsed as names and rejected by the checker (TS2427)
        // tsc allows `yield` as an interface name even inside generators
        let name = if self.is_token(SyntaxKind::YieldKeyword) {
            self.parse_identifier_name()
        } else if self.is_identifier_or_keyword() {
            // Type keywords (void, null) are accepted as names by the parser.
            // The checker emits TS2427 for predefined type names used as interface names.
            // Other reserved words (class, function, return, etc.) still get TS1005.
            if self.is_reserved_word()
                && !matches!(
                    self.current_token,
                    SyntaxKind::VoidKeyword | SyntaxKind::NullKeyword
                )
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
                // Consume the invalid token to avoid cascading errors
                self.next_token();
                NodeIndex::NONE
            } else {
                has_invalid_hard_keyword_name = matches!(
                    self.current_token,
                    SyntaxKind::VoidKeyword | SyntaxKind::NullKeyword
                );
                let name_text = self.scanner.get_token_value();
                if is_reserved_interface_type_name(name_text.as_str()) {
                    use tsz_common::diagnostics::diagnostic_codes;
                    let name_start = self.token_pos();
                    let name_end = self.token_end();
                    self.parse_error_at(
                        name_start,
                        name_end - name_start,
                        &format!("Interface name cannot be '{name_text}'."),
                        diagnostic_codes::INTERFACE_NAME_CANNOT_BE,
                    );
                }
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
        } else if self.is_token(SyntaxKind::NumericLiteral) {
            use tsz_common::diagnostics::diagnostic_codes;
            let name_start = self.token_pos();
            let name_end = self.token_end();
            let name_text = self.scanner.get_token_value();
            self.parse_error_at(
                name_start,
                name_end - name_start,
                &format!("Interface name cannot be '{name_text}'."),
                diagnostic_codes::INTERFACE_NAME_CANNOT_BE,
            );
            self.next_token();
            has_invalid_numeric_name = true;
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
        };

        // TS1434: Dotted names like `Foo.I1` are not valid interface names.
        // tsc emits "Unexpected keyword or identifier" and "{  expected" at the dotted part.
        if self.is_token(SyntaxKind::DotToken) {
            use tsz_common::diagnostics::diagnostic_codes;
            // Emit '{' expected at the dot position (tsc expects { after the name)
            self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
            // Skip over the dotted name segments (e.g., `.I1`)
            while self.is_token(SyntaxKind::DotToken) {
                self.next_token(); // skip '.'
                if self.is_identifier_or_keyword() {
                    self.parse_error_at_current_token(
                        "Unexpected keyword or identifier.",
                        diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                    );
                    self.next_token(); // skip the identifier
                }
            }
        }

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
                // Use full start position (including leading trivia) to match TSC's
                // grammarErrorAtPos(node, types.pos, 0, ...) which uses getTokenFullStart().
                self.parse_error_at(
                    self.token_full_start(),
                    0,
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

        if has_invalid_numeric_name && self.is_token(SyntaxKind::OpenBraceToken) {
            use tsz_common::diagnostics::diagnostic_codes;
            let brace_pos = self.token_pos();
            self.parse_error_at(brace_pos, 1, "';' expected.", diagnostic_codes::EXPECTED);
        }
        if has_invalid_hard_keyword_name && self.is_token(SyntaxKind::OpenBraceToken) {
            use tsz_common::diagnostics::diagnostic_codes;
            let is_null_name = self
                .arena
                .get(name)
                .and_then(|name_node| self.arena.get_identifier(name_node))
                .is_some_and(|ident| ident.escaped_text == "null");
            if is_null_name {
                self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            }
        }

        // Parse interface body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let saved_type_member_depth = self.type_member_container_depth;
        self.type_member_container_depth += 1;
        let members = self.parse_type_members();
        let end_pos = self.finish_type_member_container_close_brace();
        self.type_member_container_depth = saved_type_member_depth;
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

            // Check for mapped type member: [identifier in ...] (TS 4.1+)
            if self.is_token(SyntaxKind::OpenBracketToken) && self.look_ahead_is_mapped_type_start()
            {
                let member = self.parse_mapped_type_member();
                if member.is_some() {
                    members.push(member);
                }
                self.parse_type_member_separator_with_asi();
                continue;
            }

            let member = self.parse_type_member(true);
            if member.is_some() {
                members.push(member);
            }

            if self.deferred_type_member_close_braces >= self.type_member_container_depth {
                break;
            }

            self.parse_type_member_separator_with_asi();

            // If we didn't make progress, emit TS1131 and skip tokens to avoid infinite loops.
            if self.token_pos() == start_pos && !self.is_token(SyntaxKind::CloseBraceToken) {
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::PROPERTY_OR_SIGNATURE_EXPECTED,
                    tsz_common::diagnostics::diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED,
                );

                // `var` declarations are not valid type members. tsc recovers by
                // abandoning the malformed member tail (`var x: T<>;`) and then
                // surfacing a declaration-level TS1128 at the following `}`.
                if self.is_token(SyntaxKind::VarKeyword) {
                    self.next_token(); // consume `var`
                    while !matches!(
                        self.token(),
                        SyntaxKind::SemicolonToken
                            | SyntaxKind::CloseBraceToken
                            | SyntaxKind::EndOfFileToken
                    ) {
                        self.next_token();
                    }
                    if self.is_token(SyntaxKind::SemicolonToken) {
                        self.next_token();
                    }
                    if self.is_token(SyntaxKind::CloseBraceToken) {
                        self.parse_error_at_current_token(
                            tsz_common::diagnostics::diagnostic_messages::DECLARATION_OR_STATEMENT_EXPECTED,
                            tsz_common::diagnostics::diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                        );
                    }
                } else {
                    self.next_token();
                }
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
        if self.is_token(SyntaxKind::PrivateIdentifier) {
            // TS18016: Private identifiers are not allowed outside class bodies.
            // Parse the private identifier so the member is well-formed, but emit a diagnostic.
            let name = self.parse_property_name();
            if let Some(name_node) = self.arena.get(name) {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "Private identifiers are not allowed outside class bodies.",
                    diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
                );
            }
            Some(TypeMemberPropertyOrMethodName::Property(name))
        } else if self.is_token(SyntaxKind::Identifier)
            || self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_token(SyntaxKind::BigIntLiteral)
            || self.is_identifier_or_keyword()
        {
            // Lookahead: match tsc's isTypeMemberStart() — after consuming the property name,
            // the next token must be a valid type member continuation token (`:`, `?`, `(`, `<`,
            // `,`, or ASI-eligible). Without this check, keywords like `return` in
            // `{ return true; }` would be greedily parsed as property names.
            let snapshot = self.scanner.save_state();
            let saved_token = self.current_token;
            self.next_token(); // skip past the property name
            let is_valid_continuation = matches!(
                self.current_token,
                SyntaxKind::OpenParenToken
                    | SyntaxKind::LessThanToken
                    | SyntaxKind::QuestionToken
                    | SyntaxKind::ColonToken
                    | SyntaxKind::CommaToken
            ) || self.can_parse_semicolon();
            self.scanner.restore_state(snapshot);
            self.current_token = saved_token;

            if is_valid_continuation {
                Some(TypeMemberPropertyOrMethodName::Property(
                    self.parse_property_name(),
                ))
            } else {
                None
            }
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

        // TS1005: method signatures cannot place `?` after the parameter list.
        // tsc reports that at `?`, then reports TS1131 at a following `:`.
        if self.is_token(SyntaxKind::QuestionToken) {
            self.parse_error_at_current_token(
                "';' expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPECTED,
            );
            self.next_token();
            if self.is_token(SyntaxKind::ColonToken) {
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::PROPERTY_OR_SIGNATURE_EXPECTED,
                    tsz_common::diagnostics::diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED,
                );
            }
        }

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

        let has_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if has_open_paren {
            let parameters = self.parse_parameter_list();
            self.parse_expected(SyntaxKind::CloseParenToken);
            parameters
        } else {
            self.make_node_list(vec![])
        };

        // TS1005: call signatures cannot be optional — emit "';' expected." at '?'
        // Do NOT skip '?' — let the member parsing loop handle recovery so it emits TS1131
        if self.is_token(SyntaxKind::QuestionToken) {
            self.parse_error_at_current_token(
                "';' expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPECTED,
            );
        }

        // Return type (supports type predicates: param is T)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
            // tsc reports `':' expected` for `(args) => T` in type members.
            self.parse_error_at_current_token(
                "':' expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPECTED,
            );
            self.next_token(); // consume `=>` and recover by parsing the return type
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

        let has_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if has_open_paren {
            let parameters = self.parse_parameter_list();
            self.parse_expected(SyntaxKind::CloseParenToken);
            parameters
        } else {
            self.make_node_list(vec![])
        };

        // TS1005: construct signatures cannot be optional — emit "';' expected." at '?'
        // Do NOT skip '?' — let the member parsing loop handle recovery so it emits TS1131
        if self.is_token(SyntaxKind::QuestionToken) {
            self.parse_error_at_current_token(
                "';' expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPECTED,
            );
        }

        // Return type (supports type predicates)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
            // tsc reports `':' expected` for `new (...) => T` in type members.
            self.parse_error_at_current_token(
                "':' expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPECTED,
            );
            self.next_token(); // consume `=>` and recover by parsing the return type
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

        let bracket_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBracketToken);

        // TS1096: empty index signature `[]` — no parameters at all
        if self.is_token(SyntaxKind::CloseBracketToken) {
            // TSC emits this as grammarErrorOnNode(node, ...) in checkGrammarIndexSignatureParameters,
            // which uses the full index signature node span starting at `[`.
            // Use bracket_pos to match TSC's position.
            let bracket_end = self.token_pos(); // position of `]`
            self.parse_error_at(
                bracket_pos,
                bracket_end - bracket_pos,
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
        // Collect modifiers without emitting error - we'll emit at param name position
        let mut has_accessibility_modifier = false;
        let mut param_modifiers = Vec::new();
        while self.is_valid_parameter_modifier() {
            param_modifiers.push(
                self.arena
                    .create_modifier(self.current_token, self.token_pos()),
            );
            has_accessibility_modifier = true;
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

        // TS1018: accessibility modifier on parameter - emit at param name position
        if has_accessibility_modifier {
            if let Some(name_node) = self.arena.get(param_name) {
                self.parse_error_at(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "An index signature parameter cannot have an accessibility modifier.",
                    diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_ACCESSIBILITY_MODIFIER,
                );
            } else {
                // Fallback if we can't get the node
                self.parse_error_at_current_token(
                    "An index signature parameter cannot have an accessibility modifier.",
                    diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_ACCESSIBILITY_MODIFIER,
                );
            }
        }

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
        let (_param_type_token, param_type) = if self.is_token(SyntaxKind::CloseBracketToken)
            || self.is_token(SyntaxKind::CommaToken)
        {
            (self.token(), NodeIndex::NONE)
        } else {
            self.parse_expected(SyntaxKind::ColonToken);
            let tok = self.token();
            let ty = self.parse_type();
            (tok, ty)
        };

        // TS1020: initializer in index signature - emit at param name position
        let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
            let init = self.parse_assignment_expression();
            // TSC emits error at parameter name position, not initializer position
            if let Some(name_node) = self.arena.get(param_name) {
                self.parse_error_at(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "An index signature parameter cannot have an initializer.",
                    diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_INITIALIZER,
                );
            } else {
                self.parse_error_at_current_token(
                    "An index signature parameter cannot have an initializer.",
                    diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_INITIALIZER,
                );
            }
            init
        } else {
            NodeIndex::NONE
        };

        let param_end = self.token_end();

        // Handle comma after first parameter - could be trailing comma (TS1025) or multiple params (TS1096)
        let mut has_multiple_params = false;
        let mut has_trailing_comma = false;
        let mut trailing_comma_pos = 0;
        let comma_pos = self.token_pos(); // Position of comma before consuming it
        if self.parse_optional(SyntaxKind::CommaToken) {
            // Save the comma position for TS1025 error
            trailing_comma_pos = comma_pos;
            // Check if this is a trailing comma (comma followed by `]`)
            if self.is_token(SyntaxKind::CloseBracketToken) {
                has_trailing_comma = true;
            } else {
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
        }

        if has_multiple_params {
            // TSC emits grammarErrorOnNode(parameter.name, ...) — pointing at the
            // first parameter's name, not at the end of the parameter list.
            if let Some(name_node) = self.arena.get(param_name) {
                self.parse_error_at(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "An index signature must have exactly one parameter.",
                    diagnostic_codes::AN_INDEX_SIGNATURE_MUST_HAVE_EXACTLY_ONE_PARAMETER,
                );
            } else {
                self.parse_error_at_current_token(
                    "An index signature must have exactly one parameter.",
                    diagnostic_codes::AN_INDEX_SIGNATURE_MUST_HAVE_EXACTLY_ONE_PARAMETER,
                );
            }
        }

        // TS1025: trailing comma in index signature
        if has_trailing_comma {
            self.parse_error_at(
                trailing_comma_pos,
                1, // Length of the comma
                "An index signature cannot have a trailing comma.",
                diagnostic_codes::AN_INDEX_SIGNATURE_CANNOT_HAVE_A_TRAILING_COMMA,
            );
        }

        self.parse_expected(SyntaxKind::CloseBracketToken);

        // TS1005: index signatures cannot be optional — emit "';' expected." at '?'
        // Skip '?' but abort type annotation parsing — leave `: any;` for the member loop
        // to handle, so TS1131 is emitted at the right position (at `:`, not at `?`).
        let saw_question_after_bracket = if self.is_token(SyntaxKind::QuestionToken) {
            self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            self.next_token(); // skip '?'
            true
        } else {
            false
        };

        // Parse the type annotation after `]`.
        // TS1021 (missing type annotation) is checked by the checker, not the parser,
        // matching TSC's checkGrammarIndexSignatureParameters which uses early returns
        // to suppress TS1021 when other grammar errors are present.
        let type_annotation = if saw_question_after_bracket {
            // When `?` was after `]`, don't parse the type annotation.
            // The remaining `: any;` will be handled by the member loop which emits TS1131.
            NodeIndex::NONE
        } else if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        if self.is_token(SyntaxKind::IsKeyword) {
            // `[index: number]: p1 is C;` is not a valid index-signature type.
            // TSC reports the missing separator at `is` and then recovers the
            // invalid tail as ordinary statements after the interface body is
            // abandoned. Leave the `is C` tokens in the stream so emit can
            // preserve the recovered expression statements.
            self.error_token_expected(";");
            self.deferred_type_member_close_braces = self
                .deferred_type_member_close_braces
                .max(self.type_member_container_depth);
        }

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
        let mut has_invalid_numeric_name = false;

        // TS1142: Line break not permitted between `type` and the alias name.
        // When `declare type\nT1 = ...` has a newline, tsc still parses it as a
        // type alias but emits TS1142. Without modifiers, the lookahead in
        // look_ahead_is_type_alias_declaration prevents reaching here, but the
        // `declare` path bypasses that lookahead.
        if self.scanner.has_preceding_line_break() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Line break not permitted here.",
                diagnostic_codes::LINE_BREAK_NOT_PERMITTED_HERE,
            );
        }

        // For `type void = ...`, TSC accepts `void` as the identifier name
        // and emits TS1109 "Expression expected" from the parser (the checker
        // separately emits TS2457 "Type alias name cannot be 'void'").
        // We must not fall through to parse_identifier() which would emit TS1359.
        let name = if self.is_token(SyntaxKind::VoidKeyword) {
            let id_start = self.token_pos();
            let id_end = self.token_end();
            let atom = self.scanner.get_token_atom();
            let text = self.scanner.get_token_value_ref().to_string();
            self.next_token(); // consume `void`
            // Emit TS1109 at the `=` position (matching TSC behavior)
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at_current_token(
                diagnostic_messages::EXPRESSION_EXPECTED,
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                id_start,
                id_end,
                crate::parser::node::IdentifierData {
                    atom,
                    escaped_text: text,
                    original_text: None,
                    type_arguments: None,
                },
            )
        } else if self.is_token(SyntaxKind::NumericLiteral) {
            use tsz_common::diagnostics::diagnostic_codes;
            let id_start = self.token_pos();
            let id_end = self.token_end();
            let text = self.scanner.get_token_value();
            self.parse_error_at(
                id_start,
                id_end - id_start,
                &format!("Type alias name cannot be '{text}'."),
                diagnostic_codes::TYPE_ALIAS_NAME_CANNOT_BE,
            );
            self.next_token();
            has_invalid_numeric_name = true;
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                id_start,
                id_end,
                crate::parser::node::IdentifierData {
                    atom: Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            )
        } else {
            self.parse_identifier()
        };

        // Parse optional type parameters: <T, U extends Foo>
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        if has_invalid_numeric_name {
            use tsz_common::diagnostics::diagnostic_codes;
            if self.is_token(SyntaxKind::OpenBraceToken) {
                let brace_pos = self.token_pos();
                self.parse_error_at(brace_pos, 1, "';' expected.", diagnostic_codes::EXPECTED);
                let _ = self.parse_block();
            }
            self.parse_semicolon();
            let end_pos = self.token_full_start();
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

        let end_pos = self.token_full_start();
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
        let enum_keyword_end = self.token_end();
        self.parse_expected(SyntaxKind::EnumKeyword);

        let name = self.parse_enum_declaration_name();

        let has_open_brace = self.parse_expected(SyntaxKind::OpenBraceToken);

        let members = if has_open_brace {
            self.parse_enum_members()
        } else {
            self.make_node_list(Vec::new())
        };

        if has_open_brace {
            self.parse_expected(SyntaxKind::CloseBraceToken);
        }

        let end_pos = if has_open_brace {
            self.token_end()
        } else {
            enum_keyword_end
        };
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

    fn parse_enum_declaration_name(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let end_pos = self.token_end();

        if self.is_reserved_word() {
            // `tsc` reports the missing enum name but leaves the reserved word
            // for the outer statement parser. This preserves recovered forms like
            // `enum void {}` as an anonymous enum plus a following `void {}`.
            self.error_identifier_expected();
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                start_pos,
                end_pos,
                IdentifierData {
                    atom: Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            );
        }

        self.parse_identifier()
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

            // Handle @ inside enum body - not a valid enum member start.
            // Emit TS1132 and break out so the outer statement parser handles the
            // decorator-like syntax (producing TS1146 + TS1128 matching tsc).
            if self.is_token(SyntaxKind::AtToken) {
                self.parse_error_at_current_token(
                    "Enum member expected.",
                    diagnostic_codes::ENUM_MEMBER_EXPECTED,
                );
                break;
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

            // Check for unexpected token after enum member name - emit TS1357.
            // `tsc` still records the malformed member before recovering, so emit
            // continues to allocate enum values for invalid names such as
            // `name: 1` and `name;`.
            if !self.is_token(SyntaxKind::EqualsToken)
                && !self.is_token(SyntaxKind::CommaToken)
                && !self.is_token(SyntaxKind::CloseBraceToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                self.parse_error_at_current_token(
                    "An enum member name must be followed by a ',', '=', or '}'.",
                    diagnostic_codes::AN_ENUM_MEMBER_NAME_MUST_BE_FOLLOWED_BY_A_OR,
                );

                let member_end = self.arena.get(name).map_or(start_pos, |node| node.end);
                let member = self.arena.add_enum_member(
                    syntax_kind_ext::ENUM_MEMBER,
                    start_pos,
                    member_end,
                    EnumMemberData {
                        name,
                        initializer: NodeIndex::NONE,
                    },
                );
                members.push(member);

                // Recover by moving past one offending token unless that token
                // can itself start the next enum member. This keeps namelike
                // recovery tokens (`any`, `"hello"`, `1`) available to the next
                // iteration, matching `tsc`'s invalid-member AST.
                let starts_member = self.is_token(SyntaxKind::OpenBracketToken)
                    || self.is_token(SyntaxKind::StringLiteral)
                    || self.is_token(SyntaxKind::NumericLiteral)
                    || self.is_token(SyntaxKind::BigIntLiteral)
                    || self.is_token(SyntaxKind::PrivateIdentifier)
                    || self.is_identifier_or_keyword();
                if !starts_member {
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
        let saved_flags = self.context_flags;
        self.context_flags |= crate::parser::state::CONTEXT_FLAG_AMBIENT;

        let node = match self.token() {
            SyntaxKind::FunctionKeyword => {
                let modifiers = Some(self.make_node_list(vec![declare_modifier]));
                self.parse_function_declaration_with_async(false, modifiers)
            }
            SyntaxKind::ClassKeyword => self.parse_declare_class(start_pos, declare_modifier),
            SyntaxKind::AbstractKeyword => {
                // declare abstract class
                self.parse_declare_abstract_class(start_pos, declare_modifier)
            }
            SyntaxKind::InterfaceKeyword => {
                let modifiers = Some(self.make_node_list(vec![declare_modifier]));
                self.parse_interface_declaration_with_modifiers(start_pos, modifiers)
            }
            SyntaxKind::TypeKeyword => {
                let modifiers = Some(self.make_node_list(vec![declare_modifier]));
                self.parse_type_alias_declaration_with_modifiers(start_pos, modifiers)
            }
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
            SyntaxKind::UsingKeyword => {
                // declare using
                let modifiers = self.make_node_list(vec![declare_modifier]);
                self.parse_variable_statement_with_modifiers(Some(start_pos), Some(modifiers))
            }
            SyntaxKind::ImportKeyword => {
                use tsz_common::diagnostics::diagnostic_codes;

                self.parse_error_at(
                    declare_start,
                    declare_end - declare_start,
                    "A 'declare' modifier cannot be used with an import declaration.",
                    diagnostic_codes::A_MODIFIER_CANNOT_BE_USED_WITH_AN_IMPORT_DECLARATION,
                );

                let modifiers = Some(self.make_node_list(all_modifiers));
                if self.look_ahead_is_import_equals() {
                    self.parse_import_equals_declaration_with_modifiers(start_pos, modifiers)
                } else {
                    self.parse_import_declaration_with_modifiers(start_pos, modifiers)
                }
            }
            SyntaxKind::AwaitKeyword => {
                // declare await using
                let modifiers = self.make_node_list(vec![declare_modifier]);
                self.parse_variable_statement_with_modifiers(Some(start_pos), Some(modifiers))
            }
            SyntaxKind::ExportKeyword => {
                // declare export ... — consume 'export' and parse the inner declaration
                // with declare modifier, so the entire statement is treated as ambient.
                let export_start = self.token_pos();
                self.parse_expected(SyntaxKind::ExportKeyword);
                let export_end = self.token_end();
                let export_modifier = self.arena.add_token(
                    SyntaxKind::ExportKeyword as u16,
                    export_start,
                    export_end,
                );
                let modifiers = self.make_node_list(vec![declare_modifier, export_modifier]);
                // TS1029: 'export' modifier must precede 'declare' modifier.
                // Skip for `declare export as namespace` (valid UMD pattern) and
                // `declare export = expr` (export assignment — TS1120 handles it).
                // Also skip when already in an ambient context (e.g. inside `declare module`),
                // because the checker will emit TS1038 instead and tsc does not emit both.
                // Also skip in block context: tsc emits TS1029 via grammarErrorOnNode
                // in the checker, which is suppressed by hasParseDiagnostics when
                // TS1184 (Modifiers cannot appear here) is already emitted.
                // Also skip for `declare export module/namespace` — tsc 6.0 accepts this
                // form without TS1029 for ambient module/namespace declarations.
                if !self.in_block_context()
                    && !self.is_token(SyntaxKind::AsKeyword)
                    && !self.is_token(SyntaxKind::EqualsToken)
                    && !self.is_token(SyntaxKind::ModuleKeyword)
                    && !self.is_token(SyntaxKind::NamespaceKeyword)
                    && (saved_flags & crate::parser::state::CONTEXT_FLAG_AMBIENT) == 0
                {
                    self.parse_error_at(
                        export_start,
                        export_end - export_start,
                        &tsz_common::diagnostics::diagnostic_messages::MODIFIER_MUST_PRECEDE_MODIFIER
                            .replace("{0}", "export")
                            .replace("{1}", "declare"),
                        tsz_common::diagnostics::diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                match self.token() {
                    SyntaxKind::AsKeyword => {
                        // `declare export as namespace Foo;` — parse as namespace export declaration.
                        // TSC treats `declare` as a modifier on the export-as-namespace statement
                        // and produces no error for this form.
                        self.parse_namespace_export_declaration(start_pos)
                    }
                    SyntaxKind::FunctionKeyword => {
                        self.parse_function_declaration_with_async(false, Some(modifiers))
                    }
                    SyntaxKind::ClassKeyword => {
                        self.parse_declare_class(start_pos, declare_modifier)
                    }
                    SyntaxKind::VarKeyword
                    | SyntaxKind::LetKeyword
                    | SyntaxKind::ConstKeyword
                    | SyntaxKind::UsingKeyword
                    | SyntaxKind::AwaitKeyword => self
                        .parse_variable_statement_with_modifiers(Some(start_pos), Some(modifiers)),
                    SyntaxKind::EqualsToken => {
                        // `declare export = expr` or `export declare export = expr`
                        // tsc reports TS1120: An export assignment cannot have modifiers.
                        // Error span starts from the first modifier (export if present, else declare).
                        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                        let error_start = all_modifiers
                            .first()
                            .and_then(|idx| self.arena.get(*idx))
                            .map_or(start_pos, |node| node.pos);
                        self.parse_error_at(
                            error_start,
                            self.token_pos() - error_start,
                            diagnostic_messages::AN_EXPORT_ASSIGNMENT_CANNOT_HAVE_MODIFIERS,
                            diagnostic_codes::AN_EXPORT_ASSIGNMENT_CANNOT_HAVE_MODIFIERS,
                        );
                        self.parse_export_assignment(error_start)
                    }
                    SyntaxKind::ImportKeyword => {
                        // `declare export import a = x.c;`
                        if self.look_ahead_is_import_equals() {
                            self.parse_import_equals_declaration_with_modifiers(
                                start_pos,
                                Some(modifiers),
                            )
                        } else {
                            self.parse_import_declaration_with_modifiers(start_pos, Some(modifiers))
                        }
                    }
                    SyntaxKind::ModuleKeyword | SyntaxKind::NamespaceKeyword => {
                        // `declare export module "..."` or `declare export namespace Foo`
                        self.parse_module_declaration_with_modifiers(start_pos, Some(modifiers))
                    }
                    SyntaxKind::InterfaceKeyword => {
                        // `declare export interface X { ... }`
                        self.parse_interface_declaration_with_modifiers(start_pos, Some(modifiers))
                    }
                    SyntaxKind::TypeKeyword => {
                        // `declare export type X = ...`
                        self.parse_type_alias_declaration_with_modifiers(start_pos, Some(modifiers))
                    }
                    SyntaxKind::EnumKeyword => {
                        // `declare export enum X { ... }`
                        self.parse_enum_declaration_with_modifiers(start_pos, Some(modifiers))
                    }
                    _ => {
                        self.error_declaration_expected();
                        self.parse_expression_statement()
                    }
                }
            }
            SyntaxKind::AsyncKeyword if self.look_ahead_is_async_function() => {
                // declare async function
                // TS1040: 'async' modifier cannot be used in an ambient context
                // Emit at the 'async' keyword before consuming it, matching tsc.
                {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'async' modifier cannot be used in an ambient context.",
                        diagnostic_codes::MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                    );
                }
                // Pass the declare modifier to the function
                self.parse_expected(SyntaxKind::AsyncKeyword);
                let modifiers = Some(self.make_node_list(vec![declare_modifier]));
                self.parse_function_declaration_with_async(true, modifiers)
            }
            _ => {
                self.error_declaration_expected();
                self.parse_expression_statement()
            }
        };

        self.context_flags = saved_flags;
        node
    }

    // Module/import declarations -> state_declarations_modules.rs
}
