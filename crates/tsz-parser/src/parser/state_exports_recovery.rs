use super::state::*;
use crate::parser::node::*;
use crate::parser::parse_rules::*;
use crate::parser::{NodeIndex, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::{SyntaxKind, keyword_text_len};

impl ParserState {
    pub(crate) fn parse_exported_declaration(&mut self, start_pos: u32) -> NodeIndex {
        match self.token() {
            SyntaxKind::FunctionKeyword => self.parse_function_declaration(),
            SyntaxKind::AsyncKeyword => self.parse_export_async_declaration_or_expression(),
            SyntaxKind::ClassKeyword => self.parse_class_declaration(),
            SyntaxKind::InterfaceKeyword => {
                self.report_export_invalid_name_statement_expected(start_pos);
                self.parse_interface_declaration()
            }
            SyntaxKind::TypeKeyword => {
                self.report_export_invalid_name_statement_expected(start_pos);
                self.parse_export_type_alias_declaration()
            }
            SyntaxKind::EnumKeyword => self.parse_enum_declaration(),
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                self.report_export_invalid_name_statement_expected(start_pos);
                self.parse_module_declaration()
            }
            SyntaxKind::AbstractKeyword => {
                // When 'abstract' is followed by '@', it's `export abstract @dec class` —
                // an invalid decorator placement. tsc emits:
                //   TS1128 at the 'export' position (Declaration or statement expected.)
                //   TS1434 at the 'abstract' position (Unexpected keyword or identifier.)
                // then recovers by parsing 'abstract' as an expression statement.
                if look_ahead_is(&mut self.scanner, self.current_token, |t| {
                    t == SyntaxKind::AtToken
                }) {
                    self.parse_error_at(
                        start_pos,
                        keyword_text_len(SyntaxKind::ExportKeyword),
                        "Declaration or statement expected.",
                        diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                    );
                    let abstract_pos = self.token_pos();
                    self.parse_error_at(
                        abstract_pos,
                        keyword_text_len(SyntaxKind::AbstractKeyword),
                        "Unexpected keyword or identifier.",
                        diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                    );
                    // Consume 'abstract' as expression, skip rest of the bad statement
                    self.next_token(); // consume 'abstract'
                    // skip the rest: @dec class C14 {}
                    while !self.is_token(SyntaxKind::SemicolonToken)
                        && !self.is_token(SyntaxKind::EndOfFileToken)
                        && !self.is_token(SyntaxKind::CloseBraceToken)
                    {
                        if self.is_token(SyntaxKind::OpenBraceToken) {
                            // consume balanced {} block
                            self.next_token();
                            let mut depth = 1u32;
                            while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
                                if self.is_token(SyntaxKind::OpenBraceToken) {
                                    depth += 1;
                                } else if self.is_token(SyntaxKind::CloseBraceToken) {
                                    depth -= 1;
                                }
                                self.next_token();
                            }
                            break;
                        }
                        self.next_token();
                    }
                    return NodeIndex::NONE;
                }
                self.parse_abstract_class_declaration()
            }
            SyntaxKind::DeclareKeyword => self.parse_export_declare_declaration(start_pos),
            SyntaxKind::VarKeyword
            | SyntaxKind::LetKeyword
            | SyntaxKind::UsingKeyword
            | SyntaxKind::AwaitKeyword => {
                use tsz_scanner::SyntaxKind;
                let export_node = self.arena.add_token(
                    SyntaxKind::ExportKeyword as u16,
                    start_pos,
                    start_pos + 6,
                );
                let modifiers = self.make_node_list(vec![export_node]);
                self.parse_variable_statement_with_modifiers(Some(start_pos), Some(modifiers))
            }
            SyntaxKind::ConstKeyword => self.parse_export_const_or_variable(),
            SyntaxKind::ImportKeyword => {
                if self.look_ahead_is_import_equals() {
                    self.parse_export_import_equals(start_pos)
                } else {
                    self.parse_error_at(
                        start_pos,
                        6,
                        "An import declaration cannot have modifiers.",
                        diagnostic_codes::AN_IMPORT_DECLARATION_CANNOT_HAVE_MODIFIERS,
                    );
                    let import_decl = self.parse_import_declaration();
                    let end_pos = self.token_end();
                    self.arena.add_export_decl(
                        syntax_kind_ext::EXPORT_DECLARATION,
                        start_pos,
                        end_pos,
                        ExportDeclData {
                            modifiers: None,
                            is_type_only: false,
                            is_default_export: false,
                            default_keyword_pos: None,
                            export_clause: import_decl,
                            module_specifier: NodeIndex::NONE,
                            attributes: NodeIndex::NONE,
                        },
                    )
                }
            }
            SyntaxKind::AtToken => self.parse_export_decorated_declaration(),
            // TS1044: Class modifiers (public/private/protected/static/readonly) cannot
            // appear on a module or namespace element.
            SyntaxKind::PublicKeyword
            | SyntaxKind::PrivateKeyword
            | SyntaxKind::ProtectedKeyword
            | SyntaxKind::StaticKeyword
            | SyntaxKind::ReadonlyKeyword => {
                let modifier_text = self.scanner.get_token_text();
                self.parse_error_at_current_token(
                    &format!(
                        "'{modifier_text}' modifier cannot appear on a module or namespace element."
                    ),
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
                );
                self.next_token();
                self.parse_exported_declaration(start_pos)
            }
            // Duplicate 'export' modifier (e.g., `export export class Foo {}`)
            // or `export export = x` (export assignment with modifiers)
            SyntaxKind::ExportKeyword => {
                let second_export_pos = self.token_pos();
                let second_export_end = self.token_end();
                self.next_token();
                if self.is_token(SyntaxKind::EqualsToken) {
                    // `export export = x` — this is an export assignment with modifiers.
                    // tsc reports TS1120: An export assignment cannot have modifiers.
                    use tsz_common::diagnostics::diagnostic_messages;
                    self.parse_error_at(
                        start_pos,
                        second_export_pos + 6 - start_pos, // span covers "export export"
                        diagnostic_messages::AN_EXPORT_ASSIGNMENT_CANNOT_HAVE_MODIFIERS,
                        diagnostic_codes::AN_EXPORT_ASSIGNMENT_CANNOT_HAVE_MODIFIERS,
                    );
                    self.parse_export_assignment(start_pos)
                } else {
                    // Genuine duplicate export modifier
                    self.parse_error_at(
                        second_export_pos,
                        keyword_text_len(SyntaxKind::ExportKeyword),
                        &format!("'{}' modifier already seen.", "export"),
                        diagnostic_codes::MODIFIER_ALREADY_SEEN,
                    );
                    if self.is_token(SyntaxKind::ClassKeyword) {
                        let export_modifier = self.arena.add_token(
                            SyntaxKind::ExportKeyword as u16,
                            second_export_pos,
                            second_export_end,
                        );
                        let modifiers = Some(self.make_node_list(vec![export_modifier]));
                        self.parse_class_declaration_with_modifiers(second_export_pos, modifiers)
                    } else {
                        self.parse_exported_declaration(start_pos)
                    }
                }
            }
            _ => {
                self.parse_error_at(
                    start_pos,
                    6,
                    "Declaration or statement expected.",
                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                );
                self.parse_expression_statement()
            }
        }
    }

    pub(crate) fn report_export_invalid_name_statement_expected(&mut self, export_pos: u32) {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let should_report =
            !self.scanner.has_preceding_line_break() && self.is_token(SyntaxKind::NumericLiteral);
        self.scanner.restore_state(snapshot);
        self.current_token = current;

        if should_report {
            self.parse_error_at(
                export_pos,
                6,
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
        }
    }

    pub(crate) fn parse_export_type_alias_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::TypeKeyword);
        let mut has_invalid_numeric_name = false;

        let name = if self.is_token(SyntaxKind::NumericLiteral) {
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
                    atom: tsz_common::interner::Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            )
        } else {
            self.parse_identifier()
        };
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        if has_invalid_numeric_name {
            if self.is_token(SyntaxKind::OpenBraceToken) {
                let brace_pos = self.token_pos();
                self.parse_error_at(brace_pos, 1, "';' expected.", diagnostic_codes::EXPECTED);
                let _ = self.parse_block();
            }
            self.parse_optional(SyntaxKind::SemicolonToken);
            let end_pos = self.token_end();
            return self.arena.add_type_alias(
                syntax_kind_ext::TYPE_ALIAS_DECLARATION,
                start_pos,
                end_pos,
                crate::parser::node::TypeAliasData {
                    modifiers: None,
                    name,
                    type_parameters,
                    type_node: NodeIndex::NONE,
                },
            );
        }

        let type_node = if self.is_token(SyntaxKind::EqualsToken) {
            let equals_end = self.token_end();
            self.next_token();
            let diag_len = self.parse_diagnostics.len();
            let parsed_type = if self.is_token(SyntaxKind::EndOfFileToken) {
                self.parse_error_at(
                    equals_end,
                    0,
                    "Type expected.",
                    diagnostic_codes::TYPE_EXPECTED,
                );
                NodeIndex::NONE
            } else {
                self.parse_type()
            };
            // Keep TS1110 "Type expected" (emitted by parse_type for missing type
            // after `=`), but preserve targeted import-attribute recovery diagnostics.
            if self.parse_diagnostics.len() > diag_len {
                let kept: Vec<_> = self.parse_diagnostics[diag_len..]
                    .iter()
                    .filter(|d| {
                        matches!(
                            d.code,
                            diagnostic_codes::TYPE_EXPECTED
                                | diagnostic_codes::IDENTIFIER_OR_STRING_LITERAL_EXPECTED
                                | diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED
                                | diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER
                                | diagnostic_codes::EXPECTED
                        )
                    })
                    .cloned()
                    .collect();
                self.parse_diagnostics.truncate(diag_len);
                self.parse_diagnostics.extend(kept);
            }
            parsed_type
        } else {
            // Emit TS1005 for missing equals token
            self.error_token_expected("=");
            // If the next token looks like a type, continue parsing anyway
            if self.can_token_start_type() {
                self.parse_type()
            } else {
                // Emit TS1110 at the name's end position (tsc emits "Type expected"
                // here; use a position distinct from TS1005 to avoid dedup).
                let name_end = self.arena.get(name).map_or(self.token_pos(), |n| n.end);
                self.parse_error_at(
                    name_end,
                    0,
                    "Type expected.",
                    diagnostic_codes::TYPE_EXPECTED,
                );
                NodeIndex::NONE
            }
        };

        self.parse_optional(SyntaxKind::SemicolonToken);

        let end_pos = self.token_end();
        self.arena.add_type_alias(
            syntax_kind_ext::TYPE_ALIAS_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::node::TypeAliasData {
                modifiers: None,
                name,
                type_parameters,
                type_node,
            },
        )
    }

    pub(crate) fn parse_export_async_declaration_or_expression(&mut self) -> NodeIndex {
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

    pub(crate) fn parse_export_declare_declaration(&mut self, start_pos: u32) -> NodeIndex {
        // export declare function/class/namespace/var/etc.
        // Create an export modifier to pass to the ambient declaration
        // The export keyword was already consumed in parse_export_declaration
        // We need to create a token for it at the start_pos
        let export_modifier = self.arena.add_token(
            SyntaxKind::ExportKeyword as u16,
            start_pos,
            start_pos + keyword_text_len(SyntaxKind::ExportKeyword),
        );
        self.parse_ambient_declaration_with_modifiers(vec![export_modifier])
    }

    pub(crate) fn parse_export_const_or_variable(&mut self) -> NodeIndex {
        // export const enum or export const variable
        if self.look_ahead_is_const_enum() {
            self.parse_const_enum_declaration(self.token_pos(), Vec::new())
        } else {
            // Add ExportKeyword modifier so the variable statement knows it's exported.
            // Without this, System module emit can't detect the export and omits
            // the exports_1() wrapper.
            let start_pos = self.token_pos();
            let export_end = if start_pos >= 6 { start_pos } else { 6 };
            let export_start = export_end - 6;
            let export_node = self.arena.add_token(
                tsz_scanner::SyntaxKind::ExportKeyword as u16,
                export_start,
                export_end,
            );
            let modifiers = self.make_node_list(vec![export_node]);
            self.parse_variable_statement_with_modifiers(Some(export_start), Some(modifiers))
        }
    }

    pub(crate) fn parse_export_decorated_declaration(&mut self) -> NodeIndex {
        // export @decorator class Foo {}
        let dec_start = self.token_pos();
        let decorators = self.parse_decorators();
        match self.token() {
            SyntaxKind::ClassKeyword => {
                self.parse_class_declaration_with_decorators(decorators, self.token_pos())
            }
            SyntaxKind::AbstractKeyword => {
                self.parse_abstract_class_declaration_with_decorators(decorators, self.token_pos())
            }
            SyntaxKind::DefaultKeyword => {
                // export @dec default class C {} — TS1206 at the decorator position
                let dec_end = self.token_pos(); // end of decorators = start of 'default'
                self.parse_error_at(
                    dec_start,
                    dec_end - dec_start,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.next_token(); // consume 'default'
                match self.token() {
                    SyntaxKind::ClassKeyword => {
                        self.parse_class_declaration_with_decorators(decorators, self.token_pos())
                    }
                    SyntaxKind::AbstractKeyword => self
                        .parse_abstract_class_declaration_with_decorators(
                            decorators,
                            self.token_pos(),
                        ),
                    _ => {
                        let expr = self.parse_assignment_expression();
                        self.parse_semicolon();
                        expr
                    }
                }
            }
            _ => {
                self.error_statement_expected();
                self.parse_expression_statement()
            }
        }
    }
}
