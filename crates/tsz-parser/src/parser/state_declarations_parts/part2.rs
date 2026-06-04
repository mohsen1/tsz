impl ParserState {
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
