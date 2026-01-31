//! Parser state - interface, type alias, enum, module, import/export, and control flow parsing methods

use super::state::{CONTEXT_FLAG_DISALLOW_IN, ParserState};
use crate::interner::Atom;
use crate::parser::{NodeIndex, NodeList, node::*, node_flags, syntax_kind_ext};
use crate::scanner::SyntaxKind;

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
        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            self.parse_identifier()
        };

        // Parse type parameters: interface IList<T> {}
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        // Parse heritage clauses (extends only for interfaces)
        // Interfaces can extend multiple types: interface A extends B, C, D { }
        let heritage_clauses = if self.is_token(SyntaxKind::ExtendsKeyword) {
            let clause_start = self.token_pos();
            self.next_token();

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
            Some(self.make_node_list(vec![clause]))
        } else {
            None
        };

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
            let member = self.parse_type_member();
            if !member.is_none() {
                members.push(member);
            }

            // Handle semicolons or commas
            self.parse_optional(SyntaxKind::SemicolonToken);
            self.parse_optional(SyntaxKind::CommaToken);

            // If we didn't make progress, skip the current token to avoid infinite loop
            if self.token_pos() == start_pos && !self.is_token(SyntaxKind::CloseBraceToken) {
                self.next_token();
            }
        }

        self.make_node_list(members)
    }

    /// Parse a single type member (property signature, method signature, call signature, construct signature)
    pub(crate) fn parse_type_member(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Handle invalid access modifiers (private/protected/public) on type members.
        if matches!(
            self.token(),
            SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::AccessorKeyword
        ) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Modifiers cannot appear here.",
                diagnostic_codes::MODIFIERS_NOT_ALLOWED_HERE,
            );
            self.next_token();
            if self.is_token(SyntaxKind::OpenBracketToken) && self.look_ahead_is_index_signature() {
                return self.parse_index_signature_with_modifiers(None, start_pos);
            }
        }

        // Handle generic call signature: <T>(): returnType
        if self.is_token(SyntaxKind::LessThanToken) {
            return self.parse_call_signature(start_pos);
        }

        // Handle call signature: (): returnType
        if self.is_token(SyntaxKind::OpenParenToken) {
            return self.parse_call_signature(start_pos);
        }

        // Handle construct signature: new (): returnType
        if self.is_token(SyntaxKind::NewKeyword) {
            return self.parse_construct_signature(start_pos);
        }

        // Handle get accessor: get foo(): type
        // But not if 'get' is used as property name (get: T or get?: T or get() or get<T>())
        if self.is_token(SyntaxKind::GetKeyword)
            && !self.look_ahead_is_property_name_after_keyword()
        {
            return self.parse_get_accessor_signature(start_pos);
        }

        // Handle set accessor: set foo(v: type)
        // But not if 'set' is used as property name
        if self.is_token(SyntaxKind::SetKeyword)
            && !self.look_ahead_is_property_name_after_keyword()
        {
            return self.parse_set_accessor_signature(start_pos);
        }

        // Parse optional readonly modifier
        // But not if 'readonly' is used as property name
        let readonly = if self.is_token(SyntaxKind::ReadonlyKeyword)
            && !self.look_ahead_is_property_name_after_keyword()
        {
            self.next_token();
            true
        } else {
            false
        };

        // Parse property/method name
        // Include keywords that can be property names
        let name = if self.is_token(SyntaxKind::Identifier)
            || self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_property_name_keyword()
        {
            self.parse_property_name()
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            // Check if it's an index signature: [key: string]: value
            // vs computed property name: [Symbol.iterator](): type
            if self.look_ahead_is_index_signature() {
                // Build modifiers list if readonly was present
                let modifiers = if readonly {
                    let mod_idx = self
                        .arena
                        .create_modifier(SyntaxKind::ReadonlyKeyword, start_pos);
                    Some(self.make_node_list(vec![mod_idx]))
                } else {
                    None
                };
                return self.parse_index_signature_with_modifiers(modifiers, start_pos);
            } else {
                // Computed property name
                self.parse_property_name()
            }
        } else {
            return NodeIndex::NONE;
        };

        // Optional question mark
        let question_token = self.parse_optional(SyntaxKind::QuestionToken);

        // Build modifiers list if readonly was present
        let modifiers = if readonly {
            let mod_idx = self
                .arena
                .create_modifier(SyntaxKind::ReadonlyKeyword, start_pos);
            Some(self.make_node_list(vec![mod_idx]))
        } else {
            None
        };

        // Check if it's a method signature or property signature
        // Method signature: foo(): T or foo<T>(): U
        if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::LessThanToken) {
            // Parse optional type parameters: foo<T, U>()
            let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
                Some(self.parse_type_parameters())
            } else {
                None
            };

            // Method signature
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
        } else {
            // Property signature
            let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_type()
            } else {
                NodeIndex::NONE
            };

            // Skip initializer if present (invalid in type context but should produce error, not crash)
            // Example: { bar: number = 5 } - the "= 5" is invalid but we parse it for recovery
            if self.parse_optional(SyntaxKind::EqualsToken) {
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
    }

    /// Parse call signature: (): returnType or <T>(): returnType
    pub(crate) fn parse_call_signature(&mut self, start_pos: u32) -> NodeIndex {
        // Parse optional type parameters: <T, U>
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

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
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

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

    /// Parse index signature: [key: string]: value
    #[allow(dead_code)] // Infrastructure for full TypeScript parsing
    pub(crate) fn parse_index_signature(&mut self) -> NodeIndex {
        self.parse_index_signature_with_modifiers(None, self.token_pos())
    }

    /// Parse index signature with modifiers (static, readonly, etc.): static [key: string]: value
    pub(crate) fn parse_index_signature_with_modifiers(
        &mut self,
        modifiers: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::OpenBracketToken);

        // Parse parameter
        let param_start = self.token_pos();
        let param_name = self.parse_identifier();
        self.parse_expected(SyntaxKind::ColonToken);
        let param_type = self.parse_type(); // Type of the index parameter (e.g., string, number)

        // Allow trailing comma (invalid syntax but should produce error, not crash)
        self.parse_optional(SyntaxKind::CommaToken);

        self.parse_expected(SyntaxKind::CloseBracketToken);

        // Value type is optional - [index: any]; is valid (but semantically an error)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        let param_end = self.token_end();
        let param_node = self.arena.add_parameter(
            syntax_kind_ext::PARAMETER,
            param_start,
            param_end,
            ParameterData {
                modifiers: None,
                dot_dot_dot_token: false,
                name: param_name,
                question_token: false,
                type_annotation: param_type,
                initializer: NodeIndex::NONE,
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

    /// Parse get accessor signature in type context: get foo(): type
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
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        // Parse expected equals token, but recover gracefully if missing
        // If the next token can start a type (e.g., {, (, [), emit error and continue parsing
        if !self.is_token(SyntaxKind::EqualsToken) {
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
        } else {
            self.next_token(); // Consume the equals token
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
        use crate::checker::types::diagnostics::diagnostic_codes;
        let mut members = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let start_pos = self.token_pos();

            // Enum member names can be identifiers, string literals, or computed property names
            // Computed property names ([x]) are not valid in enums but we recover gracefully
            let name = if self.is_token(SyntaxKind::OpenBracketToken) {
                // Handle computed property name - emit TS1164 and recover
                self.parse_error_at_current_token(
                    "Computed property names are not allowed in enums.",
                    diagnostic_codes::COMPUTED_PROPERTY_NAME_IN_ENUM,
                );
                self.parse_property_name()
            } else if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else if self.is_token(SyntaxKind::PrivateIdentifier) {
                self.parse_private_identifier()
            } else {
                self.parse_identifier_name()
            };

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
                    self.parse_error_at_current_token(
                        "',' expected",
                        diagnostic_codes::TOKEN_EXPECTED,
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
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                self.parse_declare_module(start_pos, declare_modifier)
            }
            SyntaxKind::GlobalKeyword => self.parse_declare_module(start_pos, declare_modifier),
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
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Namespace must be given a name.",
                    diagnostic_codes::IDENTIFIER_EXPECTED,
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
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_module_block()
        } else if self.is_token(SyntaxKind::DotToken) {
            // Nested module: module A.B.C { }
            self.next_token();
            self.parse_nested_module_declaration(None)
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

        if is_global && let Some(node) = self.arena.get_mut(module_idx) {
            node.flags |= node_flags::GLOBAL_AUGMENTATION as u16;
        }

        module_idx
    }

    /// Parse declare module: declare module "name" {}
    pub(crate) fn parse_declare_module(
        &mut self,
        start_pos: u32,
        declare_modifier: NodeIndex,
    ) -> NodeIndex {
        // Skip module/namespace/global keyword
        let is_global = self.is_token(SyntaxKind::GlobalKeyword);
        let modifiers = Some(self.make_node_list(vec![declare_modifier]));
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
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Namespace must be given a name.",
                    diagnostic_codes::IDENTIFIER_EXPECTED,
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
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_module_block()
        } else if self.is_token(SyntaxKind::DotToken) {
            // Nested module: module A.B.C { }
            self.next_token();
            self.parse_nested_module_declaration(modifiers.clone())
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

        if is_global && let Some(node) = self.arena.get_mut(module_idx) {
            node.flags |= node_flags::GLOBAL_AUGMENTATION as u16;
        }

        module_idx
    }

    pub(crate) fn parse_nested_module_declaration(
        &mut self,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        let start_pos = self.token_pos();

        let name = if self.is_token(SyntaxKind::StringLiteral) {
            self.parse_string_literal()
        } else {
            // Allow keywords in dotted namespace segments (e.g., namespace chrome.debugger {})
            self.parse_identifier_name()
        };

        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_module_block()
        } else if self.is_token(SyntaxKind::DotToken) {
            self.next_token();
            self.parse_nested_module_declaration(modifiers.clone())
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

    /// Parse module name (can be dotted: A.B.C)
    #[allow(dead_code)] // Infrastructure for full TypeScript parsing
    pub(crate) fn parse_module_name(&mut self) -> NodeIndex {
        let mut left = self.parse_identifier();

        while self.is_token(SyntaxKind::DotToken) {
            self.next_token();
            let right = self.parse_identifier();
            let start = if let Some(n) = self.arena.get(left) {
                n.pos
            } else {
                0
            };
            let end = self.token_end();

            left = self.arena.add_qualified_name(
                syntax_kind_ext::QUALIFIED_NAME,
                start,
                end,
                QualifiedNameData { left, right },
            );
        }

        left
    }

    /// Parse module block: { statements }
    pub(crate) fn parse_module_block(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let statements = self.parse_statements();

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
        let module_specifier = if !import_clause.is_none() {
            self.parse_expected(SyntaxKind::FromKeyword);
            self.parse_string_literal()
        } else {
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
    /// Returns NodeIndex::NONE if no attributes are present.
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
                .map(|n| n.end)
                .unwrap_or(self.token_end());

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

        // Check for "type" keyword (import type { ... })
        if self.is_token(SyntaxKind::TypeKeyword) {
            // Look ahead to see if this is "type" followed by identifier or "{"
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            if self.is_token(SyntaxKind::Identifier)
                || self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::AsteriskToken)
            {
                is_type_only = true;
            } else {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
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
        let name = if self.is_token(SyntaxKind::Identifier) {
            self.parse_identifier()
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

    /// Check if next token is "from" keyword
    #[allow(dead_code)] // Infrastructure for full TypeScript parsing
    pub(crate) fn is_next_token_from(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let is_from = self.is_token(SyntaxKind::FromKeyword);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_from
    }

    /// Parse namespace import: * as name
    pub(crate) fn parse_namespace_import(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::AsteriskToken);
        self.parse_expected(SyntaxKind::AsKeyword);
        let name = self.parse_identifier();
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
    /// export function f() {}
    /// export class C {}
    pub(crate) fn parse_export_declaration(&mut self) -> NodeIndex {
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
        if self.is_token(SyntaxKind::ImportKeyword) {
            return self.parse_export_import_equals(start_pos);
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
    /// This creates a NamespaceExportDeclaration node. The syntax declares that the module's
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

        // Optional "as namespace" for re-export
        let export_clause = if self.parse_optional(SyntaxKind::AsKeyword) {
            self.parse_identifier()
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
        let attributes = if !module_specifier.is_none() {
            self.parse_import_attributes()
        } else {
            NodeIndex::NONE
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
        // Parse the declaration and wrap it
        let declaration = match self.token() {
            SyntaxKind::FunctionKeyword => self.parse_function_declaration(),
            SyntaxKind::AsyncKeyword => {
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
                    self.parse_expression_statement()
                }
            }
            SyntaxKind::ClassKeyword => self.parse_class_declaration(),
            SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(),
            SyntaxKind::TypeKeyword => self.parse_type_alias_declaration(),
            SyntaxKind::EnumKeyword => self.parse_enum_declaration(),
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                self.parse_module_declaration()
            }
            SyntaxKind::AbstractKeyword => {
                // export abstract class ...
                self.parse_abstract_class_declaration()
            }
            SyntaxKind::DeclareKeyword => {
                // export declare function/class/namespace/var/etc.
                self.parse_ambient_declaration()
            }
            SyntaxKind::VarKeyword
            | SyntaxKind::LetKeyword
            | SyntaxKind::UsingKeyword
            | SyntaxKind::AwaitKeyword => self.parse_variable_statement(),
            SyntaxKind::ConstKeyword => {
                // export const enum or export const variable
                if self.look_ahead_is_const_enum() {
                    self.parse_const_enum_declaration(self.token_pos(), Vec::new())
                } else {
                    self.parse_variable_statement()
                }
            }
            _ => {
                // Unsupported export
                self.error_statement_expected();
                self.parse_expression_statement()
            }
        };

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

    /// Parse a string literal (used for module specifiers)
    pub(crate) fn parse_string_literal(&mut self) -> NodeIndex {
        if !self.is_token(SyntaxKind::StringLiteral) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "String literal expected",
                diagnostic_codes::TOKEN_EXPECTED,
            );
            return NodeIndex::NONE;
        }

        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        let text = self.scanner.get_token_value_ref().to_string();
        self.next_token();

        self.arena.add_literal(
            SyntaxKind::StringLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
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

        let then_statement = self.parse_statement();
        self.check_using_outside_block(then_statement);

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
        let expression = if !self.can_parse_semicolon_for_restricted_production() {
            self.parse_expression()
        } else {
            NodeIndex::NONE
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
        let initializer = if !self.is_token(SyntaxKind::SemicolonToken) {
            if self.is_token(SyntaxKind::VarKeyword)
                || self.is_token(SyntaxKind::LetKeyword)
                || self.is_token(SyntaxKind::ConstKeyword)
            {
                self.parse_for_variable_declaration()
            } else {
                self.parse_expression()
            }
        } else {
            NodeIndex::NONE
        };
        self.context_flags = saved_flags;

        // Error recovery: if initializer parsing failed badly, resync to semicolon
        if initializer.is_none()
            && !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::InKeyword)
            && !self.is_token(SyntaxKind::OfKeyword)
        {
            self.resync_after_error();
        }

        // Check for for-in or for-of
        if self.is_token(SyntaxKind::InKeyword) {
            return self.parse_for_in_statement_rest(start_pos, initializer);
        }
        if self.is_token(SyntaxKind::OfKeyword) {
            return self.parse_for_of_statement_rest(start_pos, initializer, await_modifier);
        }

        // Regular for statement: for (init; cond; incr)
        self.parse_expected(SyntaxKind::SemicolonToken);

        // Condition
        let condition = if !self.is_token(SyntaxKind::SemicolonToken) {
            let cond = self.parse_expression();

            // Check for missing for condition: for (init; ; incr) when there was content to parse
            if cond == NodeIndex::NONE {
                self.error_expression_expected();
            }

            cond
        } else {
            NodeIndex::NONE
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
        let incrementor = if !self.is_token(SyntaxKind::CloseParenToken) {
            let incr = self.parse_expression();

            // Check for missing for incrementor: for (init; cond; ) when there was content to parse
            if incr == NodeIndex::NONE {
                self.error_expression_expected();
            }

            incr
        } else {
            NodeIndex::NONE
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

    /// Parse variable declaration list for for statement
    /// Supports multiple declarations for regular for: for (let x = 0, y = 1; ...)
    /// Single declaration for for-in/for-of: for (let x in/of ...)
    pub(crate) fn parse_for_variable_declaration(&mut self) -> NodeIndex {
        use crate::parser::node_flags;

        let start_pos = self.token_pos();
        let decl_keyword = self.token();
        let flags: u16 = match decl_keyword {
            SyntaxKind::LetKeyword => node_flags::LET as u16,
            SyntaxKind::ConstKeyword => node_flags::CONST as u16,
            _ => 0,
        };
        self.next_token(); // consume var/let/const

        let mut declarations = Vec::new();

        loop {
            let decl_start = self.token_pos();

            // Parse variable name (identifier or binding pattern)
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

            let decl = self.arena.add_variable_declaration(
                syntax_kind_ext::VARIABLE_DECLARATION,
                decl_start,
                self.token_end(),
                VariableDeclarationData {
                    name,
                    exclamation_token,
                    type_annotation,
                    initializer,
                },
            );
            declarations.push(decl);

            // Check for comma (more declarations) or end of list
            // For for-in/for-of, stop at 'in' or 'of' keyword
            // For regular for, stop at ';' or ')'
            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

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

    /// Parse for-in statement after initializer: for (x in obj)
    pub(crate) fn parse_for_in_statement_rest(
        &mut self,
        start_pos: u32,
        initializer: NodeIndex,
    ) -> NodeIndex {
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
        use crate::checker::types::diagnostics::diagnostic_codes;

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

        // Check for missing switch expression: switch () { }
        if expression == NodeIndex::NONE {
            self.error_expression_expected();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);
        self.parse_expected(SyntaxKind::OpenBraceToken);

        // Parse case clauses
        let mut clauses = Vec::new();
        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            if self.is_token(SyntaxKind::CaseKeyword) {
                let clause_start = self.token_pos();
                self.next_token();
                let clause_expr = self.parse_expression();
                self.parse_expected(SyntaxKind::ColonToken);

                let mut statements = Vec::new();
                while !self.is_token(SyntaxKind::CaseKeyword)
                    && !self.is_token(SyntaxKind::DefaultKeyword)
                    && !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    let pos_before = self.token_pos();
                    let stmt = self.parse_statement();
                    if !stmt.is_none() {
                        statements.push(stmt);
                    }
                    // Safety: if position didn't advance, force-skip to prevent infinite loop
                    if self.token_pos() == pos_before {
                        self.next_token();
                    }
                }

                let clause_end = self.token_end();
                clauses.push(self.arena.add_case_clause(
                    syntax_kind_ext::CASE_CLAUSE,
                    clause_start,
                    clause_end,
                    CaseClauseData {
                        expression: clause_expr,
                        statements: self.make_node_list(statements),
                    },
                ));
            } else if self.is_token(SyntaxKind::DefaultKeyword) {
                let clause_start = self.token_pos();
                self.next_token();
                self.parse_expected(SyntaxKind::ColonToken);

                let mut statements = Vec::new();
                while !self.is_token(SyntaxKind::CaseKeyword)
                    && !self.is_token(SyntaxKind::DefaultKeyword)
                    && !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    let pos_before = self.token_pos();
                    let stmt = self.parse_statement();
                    if !stmt.is_none() {
                        statements.push(stmt);
                    }
                    // Safety: if position didn't advance, force-skip to prevent infinite loop
                    if self.token_pos() == pos_before {
                        self.next_token();
                    }
                }

                let clause_end = self.token_end();
                clauses.push(self.arena.add_case_clause(
                    syntax_kind_ext::DEFAULT_CLAUSE,
                    clause_start,
                    clause_end,
                    CaseClauseData {
                        expression: NodeIndex::NONE,
                        statements: self.make_node_list(statements),
                    },
                ));
            } else {
                // Unexpected token in switch body - emit error and recover
                if self.token_pos() != self.last_error_pos {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "case or default expected.",
                        diagnostic_codes::TOKEN_EXPECTED,
                    );
                }
                // Skip unexpected token and continue
                self.next_token();
            }
        }

        let case_block_end = self.token_end();
        self.parse_expected(SyntaxKind::CloseBraceToken);
        let end_pos = self.token_end();

        // Create the case block node
        let case_block = self.arena.add_block(
            syntax_kind_ext::CASE_BLOCK,
            start_pos, // Case block starts with the opening brace
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

    /// Parse try statement
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
                    self.parse_variable_declaration()
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
            use crate::checker::types::diagnostics::diagnostic_codes;
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
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
            }
            // Try to parse semicolon for partial recovery, then resync
            let _ = self.can_parse_semicolon();
            if !self.is_token(SyntaxKind::SemicolonToken) {
                self.resync_after_error();
            } else {
                self.next_token();
            }
            return NodeIndex::NONE;
        }

        self.parse_semicolon();
        let end_pos = self.token_end();

        self.arena.add_expr_statement(
            syntax_kind_ext::EXPRESSION_STATEMENT,
            start_pos,
            end_pos,
            ExprStatementData { expression },
        )
    }
}
