//! Parser state - export declarations and control flow statement parsing.
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

/// Parser state - export declarations and control flow statement parsing
//
//
/// switch/try/do statements, string literals, and expression statements.
use super::state::{CONTEXT_FLAG_DISALLOW_IN, ParserState};
use crate::parser::parse_rules::look_ahead_is;
use crate::parser::{
    NodeIndex,
    node::{
        BlockData, ExportAssignmentData, ExportDeclData, IfStatementData, LiteralData, LoopData,
        NamedImportsData, ReturnData, SwitchData, VariableData, VariableDeclarationData,
    },
    syntax_kind_ext,
};
use tsz_scanner::SyntaxKind;
use tsz_scanner::keyword_text_len;
use tsz_scanner::scanner_impl::TokenFlags;

impl ParserState {
    // Parse export declaration
    // export { x, y };
    // export { x } from "mod";
    // export * from "mod";
    // export default x;
    // export function `f()` {}
    // export class C {}
    pub(crate) fn parse_export_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.seen_module_indicator = true;
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
            // ES6 import with export modifier — emit TS1191 at the `export` keyword
            // (tsc points the error at the modifier, not the `import` keyword)
            self.parse_error_at(
                start_pos,
                keyword_text_len(SyntaxKind::ExportKeyword),
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
                    default_keyword_pos: None,
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

    /// Parse `@decorator export class ...` form, preserving decorators on the class node.
    pub(crate) fn parse_export_declaration_with_decorators(
        &mut self,
        start_pos: u32,
        decorators: Option<crate::parser::NodeList>,
    ) -> NodeIndex {
        self.seen_module_indicator = true;
        self.parse_expected(SyntaxKind::ExportKeyword);

        if self.is_token(SyntaxKind::DefaultKeyword) {
            return self.parse_export_default_with_decorators(start_pos, decorators);
        }

        let declaration = match self.token() {
            SyntaxKind::ClassKeyword => {
                self.parse_class_declaration_with_decorators(decorators, self.token_pos())
            }
            SyntaxKind::AbstractKeyword => {
                self.parse_abstract_class_declaration_with_decorators(decorators, self.token_pos())
            }
            SyntaxKind::AtToken => {
                // Decorators after `export` when decorators also appeared before `export`:
                // @dec export @dec class Foo {}
                let post_decorators = self.parse_decorators();
                if decorators.is_some()
                    && let Some(ref post_decs) = post_decorators
                {
                    for &dec_node in &post_decs.nodes {
                        if let Some(node) = self.arena.get(dec_node) {
                            self.parse_error_at(
                                node.pos,
                                node.end - node.pos,
                                "Decorators may not appear after 'export' or 'export default' if they also appear before 'export'.",
                                diagnostic_codes::DECORATORS_MAY_NOT_APPEAR_AFTER_EXPORT_OR_EXPORT_DEFAULT_IF_THEY_ALSO_APPEAR_BEF,
                            );
                        }
                    }
                }
                // Use pre-export decorators for the class
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
                        self.error_statement_expected();
                        self.parse_expression_statement()
                    }
                }
            }
            _ => {
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
                default_keyword_pos: None,
                export_clause: declaration,
                module_specifier: NodeIndex::NONE,
                attributes: NodeIndex::NONE,
            },
        )
    }

    pub(crate) fn parse_export_default_with_decorators(
        &mut self,
        start_pos: u32,
        decorators: Option<crate::parser::NodeList>,
    ) -> NodeIndex {
        let default_pos = self.token_pos();
        self.parse_expected(SyntaxKind::DefaultKeyword);

        let expression = match self.token() {
            SyntaxKind::ClassKeyword => {
                self.parse_class_declaration_with_decorators(decorators, self.token_pos())
            }
            SyntaxKind::AbstractKeyword => {
                self.parse_abstract_class_declaration_with_decorators(decorators, self.token_pos())
            }
            SyntaxKind::AtToken => {
                // Decorators after `export default` when decorators also appeared before `export`:
                // @dec export default @dec class Foo {}
                let post_decorators = self.parse_decorators();
                if decorators.is_some()
                    && let Some(ref post_decs) = post_decorators
                {
                    for &dec_node in &post_decs.nodes {
                        if let Some(node) = self.arena.get(dec_node) {
                            self.parse_error_at(
                                node.pos,
                                node.end - node.pos,
                                "Decorators may not appear after 'export' or 'export default' if they also appear before 'export'.",
                                diagnostic_codes::DECORATORS_MAY_NOT_APPEAR_AFTER_EXPORT_OR_EXPORT_DEFAULT_IF_THEY_ALSO_APPEAR_BEF,
                            );
                        }
                    }
                }
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
                        self.parse_error_at(
                            start_pos,
                            0,
                            "Decorators are not valid here.",
                            diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                        );
                        let expr = self.parse_assignment_expression();
                        self.parse_semicolon();
                        expr
                    }
                }
            }
            SyntaxKind::FunctionKeyword => {
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_function_declaration_with_async_optional_name(false, None)
            }
            SyntaxKind::AsyncKeyword if self.look_ahead_is_async_function() => {
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.next_token(); // consume 'async'
                self.parse_function_declaration_with_async_optional_name(true, None)
            }
            SyntaxKind::InterfaceKeyword => {
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_interface_declaration()
            }
            _ => {
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
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
                default_keyword_pos: Some(default_pos),
                export_clause: expression,
                module_specifier: NodeIndex::NONE,
                attributes: NodeIndex::NONE,
            },
        )
    }

    // Parse export import X = Y (re-export of import equals declaration)
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
                default_keyword_pos: None,
                export_clause: import_decl,
                module_specifier: NodeIndex::NONE,
                attributes: NodeIndex::NONE,
            },
        )
    }

    // Parse export = expression (CommonJS-style default export)
    pub(crate) fn parse_export_assignment(&mut self, start_pos: u32) -> NodeIndex {
        self.parse_expected(SyntaxKind::EqualsToken);
        let expression = self.parse_assignment_expression();
        if expression == NodeIndex::NONE {
            self.error_expression_expected();
        }
        self.parse_semicolon();

        let end_pos = self.token_full_start();

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

    // Parse `export as namespace Foo;` (UMD global namespace declaration)
    //
    // This creates a `NamespaceExportDeclaration` node. The syntax declares that the module's
    // exports are also available globally under the given namespace name.
    pub(crate) fn parse_namespace_export_declaration(&mut self, start_pos: u32) -> NodeIndex {
        self.parse_expected(SyntaxKind::AsKeyword);
        self.parse_expected(SyntaxKind::NamespaceKeyword);
        let name = self.parse_identifier();
        self.parse_semicolon();

        let end_pos = self.token_full_start();
        self.arena.add_export_decl(
            syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION,
            start_pos,
            end_pos,
            ExportDeclData {
                modifiers: None,
                is_type_only: false,
                is_default_export: false,
                default_keyword_pos: None,
                export_clause: name,
                module_specifier: NodeIndex::NONE,
                attributes: NodeIndex::NONE,
            },
        )
    }

    // Parse export default
    pub(crate) fn parse_export_default(&mut self, start_pos: u32) -> NodeIndex {
        let default_pos = self.token_pos();
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
            SyntaxKind::AbstractKeyword => {
                // When 'abstract' is followed by '@', it's `export default abstract @dec class` —
                // an invalid pattern. tsc parses 'abstract' as an expression identifier and
                // then emits TS1005 "';' expected." when it sees '@' where a semicolon is needed.
                // Fall through to the default expression path which does exactly this.
                if look_ahead_is(&mut self.scanner, self.current_token, |t| {
                    t == SyntaxKind::AtToken
                }) {
                    let expr = self.parse_assignment_expression();
                    self.parse_semicolon();
                    expr
                } else {
                    self.parse_abstract_class_declaration()
                }
            }
            SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(),
            SyntaxKind::AtToken => {
                // export default @dec class {} — parse decorators then class
                let decorators = self.parse_decorators();
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
                        // Decorators are not valid on non-class default exports
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at(
                            start_pos,
                            0,
                            "Decorators are not valid here.",
                            diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                        );
                        let expr = self.parse_assignment_expression();
                        self.parse_semicolon();
                        expr
                    }
                }
            }
            // var/let/const after `export default` is invalid — emit TS1109
            // and parse the variable statement as recovery (consuming the
            // entire `var a = 10;` so no cascading TS1005).
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword | SyntaxKind::ConstKeyword => {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
                // Parse as variable statement for recovery
                let _ = self.parse_variable_statement();
                NodeIndex::NONE
            }
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
                default_keyword_pos: Some(default_pos),
                export_clause: expression,
                module_specifier: NodeIndex::NONE,
                attributes: NodeIndex::NONE,
            },
        )
    }

    // Parse export * from "mod"
    pub(crate) fn parse_export_star(&mut self, start_pos: u32, is_type_only: bool) -> NodeIndex {
        self.parse_expected(SyntaxKind::AsteriskToken);

        // Optional "as namespace" for re-export (keywords and string literals allowed)
        let export_clause = if self.parse_optional(SyntaxKind::AsKeyword) {
            // ES2022: `export * as "name"` uses string literal as export name
            if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else {
                self.parse_identifier_name()
            }
        } else {
            NodeIndex::NONE
        };

        self.parse_expected(SyntaxKind::FromKeyword);
        // Module specifier is usually a string literal, but inside `namespace`
        // declarations it can be an identifier (e.g., `export * from Aaa;`).
        let module_specifier = if self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
        {
            self.parse_string_literal()
        } else {
            self.parse_identifier_name()
        };

        // Parse optional import attributes: with { ... } or assert { ... }
        let attributes = self.parse_import_attributes();

        self.parse_semicolon();

        let end_pos = self.token_full_start();
        self.arena.add_export_decl(
            syntax_kind_ext::EXPORT_DECLARATION,
            start_pos,
            end_pos,
            ExportDeclData {
                modifiers: None,
                is_type_only,
                is_default_export: false,
                default_keyword_pos: None,
                export_clause,
                module_specifier,
                attributes,
            },
        )
    }

    // Parse export { x, y } or export { x } from "mod"
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
        let end_pos = self.token_full_start();

        self.arena.add_export_decl(
            syntax_kind_ext::EXPORT_DECLARATION,
            start_pos,
            end_pos,
            ExportDeclData {
                modifiers: None,
                is_type_only,
                is_default_export: false,
                default_keyword_pos: None,
                export_clause,
                module_specifier,
                attributes,
            },
        )
    }

    // Parse named exports: { x, y as z }
    pub(crate) fn parse_named_exports(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut elements = Vec::new();
        let mut emitted_comma_error = false;
        let mut leave_closing_brace_for_statement_recovery = false;
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
            let spec_recovered_braced_unicode_escape_debris =
                self.current_specifier_recovered_braced_unicode_escape_debris;

            if !self.parse_optional(SyntaxKind::CommaToken) {
                // tsc uses parseDelimitedList which emits `',' expected.` when
                // a comma-separated list element is not followed by `,` or `}`.
                if !self.is_token(SyntaxKind::CloseBraceToken) {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        &format!(
                            "'{}' expected.",
                            Self::token_to_string(SyntaxKind::CommaToken)
                        ),
                        diagnostic_codes::EXPECTED,
                    );
                    emitted_comma_error = true;
                    if spec_recovered_braced_unicode_escape_debris
                        && self.is_token(SyntaxKind::OpenBraceToken)
                    {
                        self.next_token(); // consume the `{` from `\u{...}` debris
                        while !matches!(
                            self.token(),
                            SyntaxKind::CloseBraceToken | SyntaxKind::EndOfFileToken
                        ) {
                            self.next_token();
                        }
                        if self.is_token(SyntaxKind::CloseBraceToken) {
                            self.next_token(); // consume the `}` from the braced escape
                        }
                        if self.is_token(SyntaxKind::CloseBraceToken) {
                            leave_closing_brace_for_statement_recovery = true;
                        }
                    }
                }
                break;
            }
        }

        // Skip '}' expected if we already emitted ',' expected at the same position.
        // tsc's parseDelimitedList emits only the comma error, not a closing brace error.
        if !emitted_comma_error && !leave_closing_brace_for_statement_recovery {
            self.parse_expected(SyntaxKind::CloseBraceToken);
        }
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

    // Parse export specifier: x or x as y, with type-only modifier disambiguation.
    // Follows tsc's parseImportOrExportSpecifier algorithm for handling the ambiguous
    // `type` keyword that can be either a modifier or an identifier name.
    pub(crate) fn parse_export_specifier(&mut self) -> NodeIndex {
        self.parse_import_or_export_specifier(syntax_kind_ext::EXPORT_SPECIFIER)
    }

    // Parse exported declaration (export function, export class, etc.)
    pub(crate) fn parse_export_declaration_or_statement(&mut self, start_pos: u32) -> NodeIndex {
        let declaration = self.parse_exported_declaration(start_pos);

        // If the inner parse already produced an export wrapper, don't double-wrap.
        // `export import = ...` produces an EXPORT_DECLARATION so the binder can
        // reach the import-equals alias, and `export export = ...` recovers as an
        // EXPORT_ASSIGNMENT with an invalid extra modifier.
        if let Some(declaration_node) = self.arena.get(declaration)
            && (declaration_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                || declaration_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT)
        {
            return declaration;
        }

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
                export_clause: declaration,
                module_specifier: NodeIndex::NONE,
                attributes: NodeIndex::NONE,
            },
        )
    }

    // Parse a string literal (used for module specifiers)
    pub(crate) fn parse_string_literal(&mut self) -> NodeIndex {
        if !self.is_token(SyntaxKind::StringLiteral) {
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

        // Check for unterminated string literal.
        // tsc emits TS1002 for regular unterminated strings (newline or EOF without backslash).
        // tsc emits TS1126 for strings ending with an incomplete backslash escape at EOF.
        if (self.scanner.get_token_flags() & TokenFlags::Unterminated as u32) != 0 {
            let is_backslash_at_eof =
                (self.scanner.get_token_flags() & TokenFlags::UnterminatedAtEof as u32) != 0;
            if is_backslash_at_eof {
                self.parse_error_at(
                    end_pos,
                    0,
                    diagnostic_messages::UNEXPECTED_END_OF_TEXT,
                    diagnostic_codes::UNEXPECTED_END_OF_TEXT,
                );
            } else {
                self.parse_error_at(
                    end_pos,
                    0,
                    diagnostic_messages::UNTERMINATED_STRING_LITERAL,
                    diagnostic_codes::UNTERMINATED_STRING_LITERAL,
                );
            }
        }

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
                has_invalid_escape: false,
            },
        )
    }

    // Parse if statement
    pub(crate) fn parse_if_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::IfKeyword);
        self.parse_expected(SyntaxKind::OpenParenToken);

        let saved_context_flags = self.context_flags;
        self.context_flags |= crate::parser::state::CONTEXT_FLAG_IN_PARENTHESIZED_EXPRESSION;
        let expression = self.parse_expression();
        self.context_flags = saved_context_flags;

        // Check for missing condition expression: if () { }
        if expression == NodeIndex::NONE {
            self.error_expression_expected();
        }

        self.parse_expected(SyntaxKind::CloseParenToken);

        let then_statement = if self.is_token(SyntaxKind::Unknown) {
            // Emit TS1127 for any unexpected characters before this malformed body,
            // then continue to check for the next token.
            while self.is_token(SyntaxKind::Unknown) {
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                    diagnostic_codes::INVALID_CHARACTER,
                );
                self.next_token();
            }

            // tsc reports TS1109 at the `*` when it immediately follows invalid
            // junk in `if (cond) *...` bodies. Do NOT consume the `*` — leave it
            // for the outer parser to reparse `* expr;` as a separate statement,
            // matching tsc's emit (e.g. `if (a) ¬ * bar;` -> `if (a) ;\n * bar;`).
            if self.is_token(SyntaxKind::AsteriskToken) {
                self.parse_error_at_current_token(
                    diagnostic_messages::EXPRESSION_EXPECTED,
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
            }
            NodeIndex::NONE
        } else if self.is_token(SyntaxKind::CloseBraceToken) {
            // TS1109: `if (cond) }` — missing then-clause. Emit "Expression expected"
            // at the `}` position and create an empty statement. Don't consume `}` so
            // it can close the enclosing block.
            self.error_expression_expected();
            self.arena.add_token(
                syntax_kind_ext::EMPTY_STATEMENT,
                self.token_pos(),
                self.token_pos(),
            )
        } else {
            // Set IN_BLOCK flag so that `export`/`declare` in a single-
            // statement if-body emit TS1184, matching tsc's behavior.
            let saved_flags = self.context_flags;
            self.context_flags |= crate::parser::state::CONTEXT_FLAG_IN_BLOCK;
            let stmt = self.parse_statement();
            self.context_flags = saved_flags;
            stmt
        };
        self.check_using_outside_block(then_statement);

        // TS1313: Check if the body of the if statement is an empty statement
        if let Some(node) = self.arena.get(then_statement)
            && node.kind == syntax_kind_ext::EMPTY_STATEMENT
        {
            self.parse_error_at(
                node.pos,
                node.end - node.pos,
                "The body of an 'if' statement cannot be the empty statement.",
                diagnostic_codes::THE_BODY_OF_AN_IF_STATEMENT_CANNOT_BE_THE_EMPTY_STATEMENT,
            );
        }

        let else_statement = if self.parse_optional(SyntaxKind::ElseKeyword) {
            // Set IN_BLOCK for the else-clause's single-statement body too.
            let saved_flags = self.context_flags;
            self.context_flags |= crate::parser::state::CONTEXT_FLAG_IN_BLOCK;
            let stmt = self.parse_statement();
            self.context_flags = saved_flags;
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

    // Parse return statement
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
        let end_pos = self.token_full_start();

        self.arena.add_return(
            syntax_kind_ext::RETURN_STATEMENT,
            start_pos,
            end_pos,
            ReturnData { expression },
        )
    }

    // Parse while statement
    pub(crate) fn parse_while_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::WhileKeyword);
        let has_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let missing_open_paren_before_colon =
            !has_open_paren && self.parse_optional(SyntaxKind::ColonToken);

        let condition = self.parse_expression();

        // Check for missing while condition: while () { }
        if condition == NodeIndex::NONE {
            self.error_expression_expected();
        }

        if missing_open_paren_before_colon {
            if self.is_token(SyntaxKind::DotDotDotToken) {
                self.error_expression_expected();
                self.next_token();
                let _ = self.parse_expression();
            }
            if self.is_token(SyntaxKind::ColonToken) {
                self.next_token();
                let _ = self.parse_expression();
            }
            while !matches!(
                self.token(),
                SyntaxKind::CloseParenToken
                    | SyntaxKind::OpenBraceToken
                    | SyntaxKind::CloseBraceToken
                    | SyntaxKind::EndOfFileToken
            ) {
                if self.is_token(SyntaxKind::CloseBracketToken) {
                    self.parse_error_at_current_token(
                        tsz_common::diagnostics::diagnostic_messages::AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT,
                        diagnostic_codes::AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT,
                    );
                }
                self.next_token();
            }
            if self.is_token(SyntaxKind::CloseParenToken) {
                self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
                self.next_token();
            }
        } else {
            // Error recovery: if condition parsing failed badly, resync to close paren
            if condition.is_none() && !self.is_token(SyntaxKind::CloseParenToken) {
                self.resync_after_error();
            }

            self.parse_expected(SyntaxKind::CloseParenToken);
        }

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

    // Parse for statement (basic for loop only, not for-in/for-of yet)
    pub(crate) fn parse_for_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ForKeyword);

        // Check for for-await-of: for await (...)
        let await_modifier = self.parse_optional(SyntaxKind::AwaitKeyword);

        self.parse_expected(SyntaxKind::OpenParenToken);
        if let Some(node) = self.try_parse_invalid_let_of_for_statement(start_pos) {
            return node;
        }
        let let_declaration_in_for_header = self.is_token(SyntaxKind::LetKeyword)
            && self.look_ahead_is_let_declaration_in_for_header();

        // Parse initializer (can be var/let/const declaration or expression)
        // Disallow 'in' as a binary operator so it's recognized as the for-in keyword
        let saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_DISALLOW_IN;
        let initializer = if self.is_token(SyntaxKind::SemicolonToken) {
            NodeIndex::NONE
        } else if self.is_token(SyntaxKind::VarKeyword)
            || self.is_token(SyntaxKind::ConstKeyword)
            || let_declaration_in_for_header
            || (self.is_token(SyntaxKind::UsingKeyword)
                && self.look_ahead_is_using_declaration_in_for())
            || (self.is_token(SyntaxKind::AwaitKeyword)
                && self.look_ahead_is_await_using_declaration_in_for())
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
                self.parse_error_at_current_token("'of' expected.", diagnostic_codes::EXPECTED);
            }
            return self.parse_for_in_statement_rest(start_pos, initializer);
        }
        if self.is_token(SyntaxKind::OfKeyword) {
            return self.parse_for_of_statement_rest(start_pos, initializer, await_modifier);
        }
        // Regular for statement: for (init; cond; incr)
        // When the initializer is a variable declaration list and the next token
        // is `)` instead of `;`, tsc's `parseDelimitedList(VariableDeclarations)`
        // recovery emits `',' expected.` at the unexpected token (treating it as
        // the missing comma between declarators) rather than the default
        // `';' expected.`. Mirror that message so our diagnostic at the `)`
        // matches tsc for `for (let X)`-style malformed inputs.
        let init_is_var_decl = self
            .arena
            .get(initializer)
            .is_some_and(|n| n.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST);
        if init_is_var_decl && self.is_token(SyntaxKind::CloseParenToken) {
            self.parse_expected(SyntaxKind::CommaToken);
            let statement = self.recover_for_variable_declaration_close_paren_body();
            let end_pos = self.token_end();
            return self.arena.add_loop(
                syntax_kind_ext::FOR_STATEMENT,
                start_pos,
                end_pos,
                LoopData {
                    initializer,
                    condition: NodeIndex::NONE,
                    incrementor: NodeIndex::NONE,
                    statement,
                },
            );
        } else {
            self.parse_expected(SyntaxKind::SemicolonToken);
        }

        // Condition
        let condition = if self.is_token(SyntaxKind::SemicolonToken) {
            NodeIndex::NONE
        } else {
            let cond = self.parse_expression();

            // Check for missing for condition: for (init; ; incr) when there was content to parse.
            // Use companion error to bypass position-based deduplication: the earlier
            // parse_expected(SemicolonToken) may have emitted TS1005 at the same
            // token position, and parse_error_at suppresses a second diagnostic at
            // the same start. tsc emits both TS1005 and TS1109 in this scenario
            // because its dedup only suppresses same-position + same-error repeats.
            if cond == NodeIndex::NONE {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_companion_error_at_current_token(
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
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

            // Check for missing for incrementor: for (init; cond; ) when there was content to parse.
            // Use companion error to bypass position-based deduplication (same
            // rationale as the condition case above).
            if incr == NodeIndex::NONE {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_companion_error_at_current_token(
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
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

    fn recover_for_variable_declaration_close_paren_body(&mut self) -> NodeIndex {
        self.parse_optional(SyntaxKind::CloseParenToken);

        if !self.is_token(SyntaxKind::OpenBraceToken) {
            return self.parse_statement();
        }

        let start_pos = self.token_pos();
        self.next_token();

        if !matches!(
            self.token(),
            SyntaxKind::CloseBraceToken | SyntaxKind::EndOfFileToken
        ) {
            if self.is_identifier_or_keyword() {
                let snapshot = self.scanner.save_state();
                let current = self.current_token;
                self.next_token();
                let identifier_followed_by_call = self.is_token(SyntaxKind::OpenParenToken)
                    && !self.scanner.has_preceding_line_break();
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                if identifier_followed_by_call {
                    self.next_token();
                }
            }
            self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
            while !matches!(
                self.token(),
                SyntaxKind::CloseBraceToken | SyntaxKind::EndOfFileToken
            ) {
                self.next_token();
            }
        }

        let end_pos = self.token_end();
        if self.is_token(SyntaxKind::CloseBraceToken) {
            self.parse_error_at_current_token(
                "Expression expected.",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            self.next_token();
        }

        self.arena.add_block(
            syntax_kind_ext::BLOCK,
            start_pos,
            end_pos,
            BlockData {
                statements: self.make_node_list(Vec::new()),
                multi_line: true,
            },
        )
    }

    pub(crate) fn parse_for_variable_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let (declaration_keyword, flags) =
            self.parse_for_variable_declaration_declaration_keyword();
        let declarations = self.parse_for_variable_declarations(declaration_keyword);
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

    fn parse_for_variable_declarations(
        &mut self,
        declaration_keyword: SyntaxKind,
    ) -> Vec<NodeIndex> {
        if self.is_for_variable_declaration_empty(declaration_keyword) {
            let pos = self.token_full_start();
            self.parse_error_at(
                pos,
                0,
                "Variable declaration list cannot be empty.",
                diagnostic_codes::VARIABLE_DECLARATION_LIST_CANNOT_BE_EMPTY,
            );

            return Vec::new();
        }

        let mut declarations = Vec::new();
        loop {
            declarations.push(self.parse_for_variable_declaration_entry(declaration_keyword));
            if self.parse_optional(SyntaxKind::CommaToken) {
                continue;
            }
            if self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::OpenBracketToken)
                || (self.is_identifier_or_keyword()
                    && !self.is_token(SyntaxKind::InKeyword)
                    && !self.is_token(SyntaxKind::OfKeyword))
            {
                self.parse_expected(SyntaxKind::CommaToken);
                if self.is_token(SyntaxKind::OpenBracketToken) {
                    let saved_token = self.current_token;
                    let saved_state = self.scanner.save_state();
                    self.next_token();
                    if !self.is_token(SyntaxKind::CloseBracketToken)
                        && !self.is_token(SyntaxKind::CommaToken)
                        && !self.is_token(SyntaxKind::DotDotDotToken)
                        && !self.is_token(SyntaxKind::OpenBraceToken)
                        && !self.is_token(SyntaxKind::OpenBracketToken)
                        && !self.is_identifier_or_keyword()
                    {
                        self.parse_error_at(
                            self.token_pos(),
                            self.token_end() - self.token_pos(),
                            "Array element destructuring pattern expected.",
                            1181,
                        );
                    }
                    self.scanner.restore_state(saved_state);
                    self.current_token = saved_token;
                }
                continue;
            }
            break;
        }
        declarations
    }

    fn parse_for_variable_declaration_entry(
        &mut self,
        declaration_keyword: SyntaxKind,
    ) -> NodeIndex {
        let decl_start = self.token_pos();

        let name = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_object_binding_pattern()
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            self.parse_array_binding_pattern()
        } else if declaration_keyword != SyntaxKind::VarKeyword
            && self.is_token(SyntaxKind::InKeyword)
        {
            self.error_array_element_destructuring_pattern_expected();
            NodeIndex::NONE
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

    fn try_parse_invalid_let_of_for_statement(&mut self, start_pos: u32) -> Option<NodeIndex> {
        if !self.look_ahead_is_invalid_let_of_for_header() {
            return None;
        }

        self.parse_expected(SyntaxKind::LetKeyword);
        self.next_token();
        self.parse_expected(SyntaxKind::CommaToken);
        if self.is_token(SyntaxKind::OpenBracketToken) {
            let saved_token = self.current_token;
            let saved_state = self.scanner.save_state();
            self.next_token();
            if !self.is_token(SyntaxKind::CloseBracketToken)
                && !self.is_token(SyntaxKind::CommaToken)
                && !self.is_token(SyntaxKind::DotDotDotToken)
                && !self.is_token(SyntaxKind::OpenBraceToken)
                && !self.is_token(SyntaxKind::OpenBracketToken)
                && !self.is_identifier_or_keyword()
            {
                self.parse_error_at(
                    self.token_pos(),
                    self.token_end() - self.token_pos(),
                    "Array element destructuring pattern expected.",
                    1181,
                );
            }
            self.scanner.restore_state(saved_state);
            self.current_token = saved_token;

            self.next_token();
            while !self.is_token(SyntaxKind::CloseBracketToken)
                && !self.is_token(SyntaxKind::CloseParenToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                self.next_token();
            }
        }
        if self.is_token(SyntaxKind::CloseBracketToken) {
            self.parse_expected(SyntaxKind::SemicolonToken);
            self.next_token();
        }
        if self.is_token(SyntaxKind::CloseParenToken) {
            self.parse_error_at_current_token(
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
            self.next_token();
        }

        let body = self.arena.add_token(
            syntax_kind_ext::EMPTY_STATEMENT,
            self.token_pos(),
            self.token_pos(),
        );
        let end_pos = self.token_end();
        Some(self.arena.add_loop(
            syntax_kind_ext::FOR_STATEMENT,
            start_pos,
            end_pos,
            LoopData {
                initializer: NodeIndex::NONE,
                condition: NodeIndex::NONE,
                incrementor: NodeIndex::NONE,
                statement: body,
            },
        ))
    }

    fn look_ahead_is_let_declaration_in_for_header(&mut self) -> bool {
        let saved_token = self.current_token;
        let saved_state = self.scanner.save_state();
        self.next_token();
        let result = if !self.scanner.has_preceding_line_break()
            && self.current_token == SyntaxKind::InKeyword
        {
            false
        } else if !self.scanner.has_preceding_line_break()
            && self.current_token == SyntaxKind::OfKeyword
        {
            true
        } else {
            self.scanner.restore_state(saved_state);
            self.current_token = saved_token;
            return self.look_ahead_is_let_declaration();
        };
        self.scanner.restore_state(saved_state);
        self.current_token = saved_token;
        result
    }

    fn look_ahead_is_invalid_let_of_for_header(&mut self) -> bool {
        if !self.is_token(SyntaxKind::LetKeyword) {
            return false;
        }

        let saved_token = self.current_token;
        let saved_state = self.scanner.save_state();
        self.next_token();
        let result = !self.scanner.has_preceding_line_break()
            && self.current_token == SyntaxKind::OfKeyword
            && {
                self.next_token();
                !self.scanner.has_preceding_line_break()
                    && self.current_token == SyntaxKind::OpenBracketToken
            };
        self.scanner.restore_state(saved_state);
        self.current_token = saved_token;
        result
    }

    fn is_for_variable_declaration_empty(&mut self, declaration_keyword: SyntaxKind) -> bool {
        if declaration_keyword != SyntaxKind::VarKeyword {
            return false;
        }

        if self.is_token(SyntaxKind::InKeyword) {
            return true;
        }

        if self.is_token(SyntaxKind::OfKeyword) {
            // Look ahead to see if 'of' is used as a variable name.
            // `for (var of ...)` — `of` could be a variable name OR the for-of keyword.
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
            // Special case: `for (var of of)` — when `of` is followed by `of` then `)`,
            // the first `of` is the for-of keyword (not a variable name), making the
            // declaration list empty. tsc emits TS1123 for this pattern.
            let is_of_of_pattern = next == SyntaxKind::OfKeyword && {
                self.next_token(); // skip second 'of'
                self.is_token(SyntaxKind::CloseParenToken)
            };
            self.scanner.restore_state(snapshot);
            self.current_token = saved_token;
            if is_of_of_pattern {
                return true;
            }
            return !is_var_name;
        }

        false
    }

    // Parse for-in statement after initializer: for (x in obj)
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

    // Parse for-of statement after initializer: for (x of arr)
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

    // Parse break statement
    pub(crate) fn parse_break_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::BreakKeyword);

        // For restricted productions (break), ASI applies immediately after line break
        // Use can_parse_semicolon_for_restricted_production() instead of can_parse_semicolon()
        // Optional label — matching tsc's isIdentifier() which returns false for
        // `await` in await/static-block context and `yield` in generator context.
        // When the label would be a contextually reserved word (e.g., `break await;` in a
        // static block), tsc's parseIdentifier emits TS1003 "Identifier expected" and
        // leaves the token unconsumed. The outer statement loop then re-parses the
        // reserved word as an expression statement (e.g. `await` as an await expression
        // with a missing operand), which is where TS1109 originates.
        let label = if !self.can_parse_semicolon_for_restricted_production()
            && self.is_identifier_or_keyword()
        {
            if self.is_contextually_reserved_label() {
                // Emit TS1003 matching tsc's createIdentifier(false) behavior
                self.error_identifier_expected();
                NodeIndex::NONE
            } else {
                self.parse_identifier_name()
            }
        } else {
            NodeIndex::NONE
        };

        self.parse_semicolon();
        let end_pos = self.token_full_start();

        self.arena.add_jump(
            syntax_kind_ext::BREAK_STATEMENT,
            start_pos,
            end_pos,
            crate::parser::node::JumpData { label },
        )
    }

    // Parse continue statement
    pub(crate) fn parse_continue_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ContinueKeyword);

        // For restricted productions (continue), ASI applies immediately after line break
        // Use can_parse_semicolon_for_restricted_production() instead of can_parse_semicolon().
        // For contextually reserved-word labels (e.g. `continue await` in a static block),
        // see `parse_break_statement` above for the full rationale: emit TS1003 and leave
        // the token unconsumed so the outer loop can re-parse it as an expression.
        let label = if !self.can_parse_semicolon_for_restricted_production()
            && self.is_identifier_or_keyword()
        {
            if self.is_contextually_reserved_label() {
                self.error_identifier_expected();
                NodeIndex::NONE
            } else {
                self.parse_identifier_name()
            }
        } else {
            NodeIndex::NONE
        };

        self.parse_semicolon();
        let end_pos = self.token_full_start();

        self.arena.add_jump(
            syntax_kind_ext::CONTINUE_STATEMENT,
            start_pos,
            end_pos,
            crate::parser::node::JumpData { label },
        )
    }

    // Parse throw statement
    pub(crate) fn parse_throw_statement(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ThrowKeyword);

        // TypeScript requires an expression after throw
        // If there's a line break immediately after throw, emit TS1142
        let expression = if self.scanner.has_preceding_line_break()
            && !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            // Line break after throw - TS1142: Line break not permitted here
            // The error position should be at the end of the `throw` keyword
            let throw_end = start_pos + keyword_text_len(SyntaxKind::ThrowKeyword);
            self.parse_error_at(
                throw_end,
                0,
                "Line break not permitted here.",
                diagnostic_codes::LINE_BREAK_NOT_PERMITTED_HERE,
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
                "Expression expected.",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            NodeIndex::NONE
        } else if !self.can_parse_semicolon_for_restricted_production() {
            self.parse_expression()
        } else {
            NodeIndex::NONE
        };

        self.parse_semicolon();
        let end_pos = self.token_full_start();

        // Use return statement node type for throw (same structure)
        self.arena.add_return(
            syntax_kind_ext::THROW_STATEMENT,
            start_pos,
            end_pos,
            ReturnData { expression },
        )
    }

    // Parse do-while statement
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

    // Parse switch statement
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
}
