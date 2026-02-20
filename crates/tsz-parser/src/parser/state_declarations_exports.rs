//! Parser state - export declarations and control flow statement parsing
//!
//! Extracted from `state_declarations.rs`: export declarations, if/while/for/
//! switch/try/do statements, string literals, and expression statements.

use super::state::{CONTEXT_FLAG_DISALLOW_IN, ParserState};
use crate::parser::{
    NodeIndex,
    node::{
        BlockData, CaseClauseData, CatchClauseData, ExportAssignmentData, ExportDeclData,
        ExprStatementData, IfStatementData, LiteralData, LoopData, NamedImportsData, ReturnData,
        SpecifierData, SwitchData, TryData, VariableData, VariableDeclarationData,
    },
    syntax_kind_ext,
};
use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::TokenFlags;

impl ParserState {
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
        if expression == NodeIndex::NONE {
            self.error_expression_expected();
        }
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
            SyntaxKind::TypeKeyword => self.parse_export_type_alias_declaration(),
            SyntaxKind::EnumKeyword => self.parse_enum_declaration(),
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                self.parse_module_declaration()
            }
            SyntaxKind::AbstractKeyword => self.parse_abstract_class_declaration(),
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
            SyntaxKind::AtToken => self.parse_export_decorated_declaration(),
            // Duplicate 'export' modifier (e.g., `export export class Foo {}`)
            SyntaxKind::ExportKeyword => {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    &format!("'{}' modifier already seen.", "export"),
                    diagnostic_codes::MODIFIER_ALREADY_SEEN,
                );
                self.next_token();
                self.parse_exported_declaration(start_pos)
            }
            _ => {
                self.error_statement_expected();
                self.parse_expression_statement()
            }
        }
    }

    fn parse_export_type_alias_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::TypeKeyword);

        let name = self.parse_identifier();
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        let type_node = if self.is_token(SyntaxKind::EqualsToken) {
            self.next_token();
            let diag_len = self.parse_diagnostics.len();
            let parsed_type = self.parse_type();
            if self.parse_diagnostics.len() > diag_len {
                self.parse_diagnostics.truncate(diag_len);
            }
            parsed_type
        } else {
            NodeIndex::NONE
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

        // Check for unterminated string literal (TS1002)
        if (self.scanner.get_token_flags() & TokenFlags::Unterminated as u32) != 0 {
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at(
                start_pos,
                1,
                diagnostic_messages::UNTERMINATED_STRING_LITERAL,
                diagnostic_codes::UNTERMINATED_STRING_LITERAL,
            );
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
        // If there's a line break immediately after throw, emit TS1142
        let expression = if self.scanner.has_preceding_line_break()
            && !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            // Line break after throw - TS1142: Line break not permitted here
            // The error position should be at the end of the `throw` keyword
            let throw_end = start_pos + 5; // "throw" is 5 chars
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
                // Unexpected token in switch body.
                // Emit TS1130 once (guarded by last_error_pos), then try to parse the
                // unexpected tokens as a complete statement so that compound constructs
                // like `class D {}` are consumed in one shot (emitting only ONE TS1130),
                // matching TSC's parseList / abortParsingListOrMoveToNextToken behavior.
                if self.token_pos() != self.last_error_pos {
                    use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.parse_error_at_current_token(
                        diagnostic_messages::CASE_OR_DEFAULT_EXPECTED,
                        diagnostic_codes::CASE_OR_DEFAULT_EXPECTED,
                    );
                }
                let pos_before = self.token_pos();
                let _ = self.parse_statement();
                // Failsafe: if parse_statement didn't advance, advance one token to avoid
                // an infinite loop.
                if self.token_pos() == pos_before {
                    self.next_token();
                }
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
            if statement.is_some() {
                statements.push(statement);
            }
            if self.token_pos() == pos_before {
                self.next_token();
            }
        }
        statements
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
