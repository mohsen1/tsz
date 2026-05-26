//! Parser state - module and import declarations.

use super::state::ParserState;
use crate::parser::{
    NodeIndex, NodeList,
    node::{IdentifierData, ImportClauseData, ImportDeclData, NamedImportsData, SpecifierData},
    node_flags, syntax_kind_ext,
};
use tsz_common::interner::Atom;
use tsz_scanner::SyntaxKind;

impl ParserState {
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
        let is_namespace_keyword = self.is_token(SyntaxKind::NamespaceKeyword);
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
                // Check for reserved word or numeric literal as namespace name - emit TS2819
                if self.is_reserved_word() || self.is_token(SyntaxKind::NumericLiteral) {
                    let word = if self.is_token(SyntaxKind::NumericLiteral) {
                        self.scanner.get_token_value()
                    } else {
                        self.current_keyword_text().to_string()
                    };
                    let prev_token = self.current_token;
                    let name_start = self.token_pos();
                    let name_end = self.token_end();
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at(
                        name_start,
                        name_end - name_start,
                        &format!("Namespace name cannot be '{word}'."),
                        diagnostic_codes::NAMESPACE_NAME_CANNOT_BE,
                    );
                    self.next_token();
                    // tsc emits TS1005 ("';' expected") for some reserved words
                    // followed by '{', specifically for literals and references
                    // that cannot be followed by '{}' in valid expressions.
                    // Keywords like void, return, typeof can be followed by '{}'.
                    if self.is_token(SyntaxKind::OpenBraceToken)
                        && matches!(
                            prev_token,
                            SyntaxKind::NullKeyword
                                | SyntaxKind::TrueKeyword
                                | SyntaxKind::FalseKeyword
                                | SyntaxKind::ThisKeyword
                                | SyntaxKind::SuperKeyword
                                | SyntaxKind::NumericLiteral
                        )
                    {
                        self.parse_expected(SyntaxKind::SemicolonToken);
                    }
                    // Create a missing identifier for recovery
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
            self.parse_nested_module_declaration(
                modifiers.clone(),
                is_declare,
                is_namespace_keyword,
            )
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
        let namespace_flag = self.u16_from_node_flags(node_flags::NAMESPACE);
        if let Some(node) = self.arena.get_mut(module_idx) {
            if is_global {
                node.flags |= global_augmentation_flag;
            }
            if is_namespace_keyword {
                node.flags |= namespace_flag;
            }
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
        let is_namespace_keyword = self.is_token(SyntaxKind::NamespaceKeyword);
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
                // Check for reserved word or numeric literal as namespace name - emit TS2819
                if self.is_reserved_word() || self.is_token(SyntaxKind::NumericLiteral) {
                    let word = if self.is_token(SyntaxKind::NumericLiteral) {
                        self.scanner.get_token_value()
                    } else {
                        self.current_keyword_text().to_string()
                    };
                    let name_start = self.token_pos();
                    let name_end = self.token_end();
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at(
                        name_start,
                        name_end - name_start,
                        &format!("Namespace name cannot be '{word}'."),
                        diagnostic_codes::NAMESPACE_NAME_CANNOT_BE,
                    );
                    self.next_token();
                    // tsc also emits TS1005 ("';' expected") at the token after
                    // the reserved word when followed by '{'.
                    if self.is_token(SyntaxKind::OpenBraceToken) {
                        self.parse_expected(SyntaxKind::SemicolonToken);
                    }
                    // Create a missing identifier for recovery
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
            }
        };

        // Parse body
        // Declare module blocks are always ambient
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_module_block(true)
        } else if self.is_token(SyntaxKind::DotToken) {
            // Nested module: module A.B.C { }
            self.next_token();
            self.parse_nested_module_declaration(modifiers.clone(), true, is_namespace_keyword)
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
        let namespace_flag = self.u16_from_node_flags(node_flags::NAMESPACE);
        if let Some(node) = self.arena.get_mut(module_idx) {
            if is_global {
                node.flags |= global_augmentation_flag;
            }
            if is_namespace_keyword {
                node.flags |= namespace_flag;
            }
        }

        module_idx
    }

    pub(crate) fn parse_nested_module_declaration(
        &mut self,
        modifiers: Option<NodeList>,
        is_ambient: bool,
        is_namespace: bool,
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
            self.parse_nested_module_declaration(modifiers.clone(), is_ambient, is_namespace)
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

        if is_namespace {
            let namespace_flag = self.u16_from_node_flags(node_flags::NAMESPACE);
            if let Some(node) = self.arena.get_mut(module_idx) {
                node.flags |= namespace_flag;
            }
        }

        module_idx
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

        let end_pos = if self.deferred_module_close_braces > 0
            && self.is_token(SyntaxKind::CloseBraceToken)
        {
            self.deferred_module_close_braces -= 1;
            self.token_pos()
        } else {
            self.parse_expected(SyntaxKind::CloseBraceToken);
            self.token_end()
        };

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
        self.parse_import_declaration_with_modifiers(start_pos, None)
    }

    pub(crate) fn parse_import_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        self.seen_module_indicator = true;
        self.parse_expected(SyntaxKind::ImportKeyword);

        // Check for import "module" (no import clause) or whether the next token
        // can start an import clause. Matches tsc's tryParseImportClause:
        //   - identifier (non-reserved): default import name
        //   - `*`: namespace import
        //   - `{`: named imports
        //   - `type`/`defer`: phase modifiers
        // Reserved words like `import`, `export`, `class` etc. do NOT start an
        // import clause — they cause tsc to skip directly to module specifier
        // parsing (which emits TS1109 "Expression expected" for non-string tokens).
        let diagnostics_before_import_clause = self.parse_diagnostics.len();
        let can_start_import_clause = !self.is_token(SyntaxKind::StringLiteral)
            && (self.is_identifier()
                || self.is_token(SyntaxKind::AsteriskToken)
                || self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::TypeKeyword)
                || self.is_token(SyntaxKind::DeferKeyword));
        let import_clause = if self.is_token(SyntaxKind::StringLiteral) {
            NodeIndex::NONE
        } else if can_start_import_clause {
            self.parse_import_clause()
        } else {
            NodeIndex::NONE
        };
        let import_clause_had_errors =
            self.parse_diagnostics.len() > diagnostics_before_import_clause;

        // Namespace import yielded to statement recovery: bail out of import
        // parsing without touching the remaining tokens so the outer statement
        // parser can pick them up (e.g. `import * as while from "foo"` becomes
        // a WhileStatement on `while from "foo"`, matching tsc's cascade).
        if self.namespace_import_yielded_to_statement {
            self.namespace_import_yielded_to_statement = false;
            let end_pos = self.token_end();
            return self.arena.add_import_decl(
                syntax_kind_ext::IMPORT_DECLARATION,
                start_pos,
                end_pos,
                ImportDeclData {
                    modifiers,
                    is_type_only: false,
                    import_clause,
                    module_specifier: NodeIndex::NONE,
                    attributes: NodeIndex::NONE,
                },
            );
        }

        // Parse module specifier
        let recovered_trailing_comma_before_from =
            !import_clause_had_errors && self.is_token(SyntaxKind::CommaToken);
        if recovered_trailing_comma_before_from {
            self.parse_error_at_current_token(
                "'from' expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPECTED,
            );
            self.next_token();
            if self.is_token(SyntaxKind::FromKeyword) {
                self.next_token();
                if self.is_token(SyntaxKind::StringLiteral) {
                    self.parse_error_at_current_token(
                        "';' expected.",
                        tsz_common::diagnostics::diagnostic_codes::EXPECTED,
                    );
                    let _ = self.parse_string_literal();
                }
            }
        }

        let module_specifier = if recovered_trailing_comma_before_from {
            NodeIndex::NONE
        } else if import_clause.is_none()
            && !can_start_import_clause
            && !self.is_token(SyntaxKind::StringLiteral)
        {
            // No import clause because the token after `import` is a reserved word
            // (e.g., `import\nimport ...`). tsc emits TS1109 "Expression expected"
            // at the reserved word position (via parseModuleSpecifier → parseExpression
            // → parsePrimaryExpression → parseIdentifier(Expression_expected)).
            // Don't consume the token so it can be parsed as the next statement.
            // Note: StringLiteral is excluded — `import "mod"` is a valid bare import.
            self.error_expression_expected();
            NodeIndex::NONE
        } else if import_clause.is_none() {
            self.parse_string_literal()
        } else if import_clause_had_errors
            && self.is_token(SyntaxKind::FromKeyword)
            && self.last_named_imports_recovered_to_from
        {
            // The import clause had errors but we still see `from` — this happens
            // when a named import list failed to consume its closing `}` and we
            // recovered directly into the module clause. Consume `from` and the
            // module specifier normally.
            self.parse_expected(SyntaxKind::FromKeyword);
            self.parse_string_literal()
        } else if import_clause_had_errors
            && self.is_token(SyntaxKind::FromKeyword)
            && self.last_named_imports_consumed_closing_brace
            && !self.last_named_imports_had_structural_error
        {
            // The import clause had semantic errors (e.g., TS1003 for reserved word
            // binding name like `import { default } from "mod"`) but was structurally
            // parsed correctly — the named imports consumed their closing `}`.
            // Parse `from` and module specifier normally.
            self.parse_expected(SyntaxKind::FromKeyword);
            self.parse_string_literal()
        } else if import_clause_had_errors {
            let _import_clause_is_namespace_import = self
                .arena
                .get(import_clause)
                .is_some_and(|node| node.kind == syntax_kind_ext::NAMESPACE_IMPORT);
            if self.is_token(SyntaxKind::CloseBraceToken) {
                NodeIndex::NONE
            } else {
                // The import clause had errors AND we're NOT at `from`.  This happens
                // with malformed namespace imports like `import * from Zero from "./0"`
                // where the parser consumed `from` as the namespace name and leftover
                // tokens remain.  Absorb residual identifier-like tokens on the same
                // line into the import statement to prevent them from being parsed as
                // standalone statements (which would generate cascading TS1434
                // diagnostics).  Stop at delimiters (`}`, `{`) which belong to
                // legitimate syntax (e.g. `import { 0n as foo } from "./foo"`).
                self.parse_expected(SyntaxKind::FromKeyword);
                while !self.scanner.has_preceding_line_break()
                    && !self.is_token(SyntaxKind::SemicolonToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                    && !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::OpenBraceToken)
                {
                    if self.is_token(SyntaxKind::StringLiteral)
                        || self.is_token(SyntaxKind::FromKeyword)
                    {
                        break;
                    }
                    self.next_token();
                }
                // If we reached `from STRING`, emit `;` expected at the leftover
                // `from` position (matching tsc's recovery) then consume the specifier.
                if self.is_token(SyntaxKind::FromKeyword) {
                    self.error_token_expected(";");
                    self.next_token();
                }
                if self.is_token(SyntaxKind::StringLiteral) {
                    self.parse_string_literal()
                } else {
                    NodeIndex::NONE
                }
            }
        } else {
            self.parse_expected(SyntaxKind::FromKeyword);
            self.parse_string_literal()
        };

        // Parse optional import attributes: with { ... } or assert { ... }
        let attributes = self.parse_import_attributes();
        let recover_as_statement_boundary = (import_clause_had_errors
            && module_specifier.is_none()
            && matches!(
                self.token(),
                SyntaxKind::CloseBraceToken | SyntaxKind::FromKeyword | SyntaxKind::CommaToken
            ))
            || (recovered_trailing_comma_before_from && module_specifier.is_none());
        if !recover_as_statement_boundary {
            self.parse_semicolon();
        }
        let end_pos = self.token_full_start();

        self.arena.add_import_decl(
            syntax_kind_ext::IMPORT_DECLARATION,
            start_pos,
            end_pos,
            ImportDeclData {
                modifiers,
                // For regular imports, type-only lives in ImportClauseData, not here.
                is_type_only: false,
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
        self.last_named_imports_consumed_closing_brace = false;
        self.last_named_imports_recovered_to_from = false;
        self.last_named_imports_had_structural_error = false;

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
                // tsc rule: if the identifier after `type` is NOT `from`, then
                // `type` is always a modifier. The ambiguity only exists for
                // `import type from ...` — is `from` the import name or a keyword?
                if !self.is_token(SyntaxKind::FromKeyword) {
                    // `import type X ...` where X != from — type is modifier
                    is_type_only = true;
                } else {
                    // `import type from ...` — need one more lookahead
                    self.next_token();
                    if self.is_token(SyntaxKind::FromKeyword)
                        || self.is_token(SyntaxKind::CommaToken)
                        || self.is_token(SyntaxKind::EqualsToken)
                    {
                        // `import type from from/,/=` — type is modifier, `from` is name
                        is_type_only = true;
                    }
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
        // Disambiguation mirrors the `type` modifier logic above:
        //   import defer * as ns from "..." → defer is modifier
        //   import defer { foo } from "..." → defer is modifier (checker emits TS18059)
        //   import defer foo from "..." → defer is modifier (checker emits TS18058)
        //   import defer from from "..." → defer is modifier (checker emits TS18058)
        //   import defer from "..." → defer is the default import NAME
        //   import defer type * as ns → defer is the default import NAME (modifier conflict)
        // When is_type_only is true, defer cannot be a modifier (matches tsc's
        // `isTypeOnly ? undefined : parseModifierIfPresent(DeferKeyword)`).
        if !is_type_only && self.is_token(SyntaxKind::DeferKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            let saved_arena_len = self.arena.nodes.len();
            let saved_diagnostics_len = self.parse_diagnostics.len();
            self.next_token();

            if self.is_token(SyntaxKind::AsteriskToken) || self.is_token(SyntaxKind::OpenBraceToken)
            {
                // `import defer * ...` or `import defer { ... }` — defer is modifier
                is_deferred = true;
            } else if self.is_token(SyntaxKind::TypeKeyword) {
                // `import defer type ...` — modifier conflict. tsc treats `defer`
                // as the deferred modifier and `type` as the default-import name
                // (a contextual keyword used as an identifier). The cursor stays
                // at `type` so the existing default-name parser handles it; the
                // `from` diagnostic then anchors at the next significant token
                // (e.g. the `*` at column 19) rather than at `type` itself.
                is_deferred = true;
            } else if self.is_identifier_or_keyword() && !self.is_token(SyntaxKind::TypeKeyword) {
                // Could be `import defer foo from` (modifier + name)
                // or `import defer from '...'` (defer is name).
                // Look one more token ahead to disambiguate.
                self.next_token();
                if self.is_token(SyntaxKind::FromKeyword)
                    || self.is_token(SyntaxKind::CommaToken)
                    || self.is_token(SyntaxKind::EqualsToken)
                {
                    // `import defer X from/,/=` — defer is modifier
                    is_deferred = true;
                }
                // Restore either way (we'll re-parse the import name below)
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                self.arena.nodes.truncate(saved_arena_len);
                self.parse_diagnostics.truncate(saved_diagnostics_len);
                if is_deferred {
                    // Re-consume `defer` since it's the modifier
                    self.next_token();
                }
            } else {
                // Not a valid defer target — defer is the import name
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                self.arena.nodes.truncate(saved_arena_len);
                self.parse_diagnostics.truncate(saved_diagnostics_len);
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
        let had_comma = name.is_some() && self.parse_optional(SyntaxKind::CommaToken);

        // Parse named bindings: * as ns or { x, y }
        // Only parse when there is no default name or a comma was consumed.
        // Without a comma after a default name, `*` or `{` are not valid
        // continuations of the import clause.
        let named_bindings = if !name.is_some() || had_comma {
            if self.is_token(SyntaxKind::AsteriskToken) {
                self.parse_namespace_import()
            } else if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_named_imports()
            } else if self.is_token(SyntaxKind::FromKeyword) {
                self.parse_error_at_current_token(
                    "'{' expected.",
                    tsz_common::diagnostics::diagnostic_codes::EXPECTED,
                );
                NodeIndex::NONE
            } else {
                NodeIndex::NONE
            }
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
        // Namespace import names must still reject reserved words like `while`,
        // but allow contextual keywords such as `type`.
        //
        // When the name slot holds a reserved word that can start a statement
        // (e.g. `import * as while from "foo"`), tsc emits TS1359 at the
        // keyword and then lets statement recovery re-parse the keyword as the
        // head of a statement, cascading statement-specific diagnostics onto
        // the following tokens.
        // Replicate that by emitting TS1359 here without consuming the token
        // and signaling the import declaration to bail out of its own recovery.
        let name = if self.is_reserved_word()
            && self.is_namespace_import_recovery_statement_starter()
        {
            use tsz_common::diagnostics::diagnostic_codes;
            let name_pos = self.token_pos();
            let name_end = self.token_end();
            if self.should_report_error() {
                let word = self.current_keyword_text();
                self.parse_error_at_current_token(
                    &format!(
                        "Identifier expected. '{word}' is a reserved word that cannot be used here."
                    ),
                    diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
                );
            }
            self.namespace_import_yielded_to_statement = true;
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                name_pos,
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
        self.last_named_imports_consumed_closing_brace = false;
        self.last_named_imports_recovered_to_from = false;
        self.last_named_imports_had_structural_error = false;
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut elements = Vec::new();
        let mut leave_closing_brace_for_statement_recovery = false;
        let mut consumed_closing_brace = false;
        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            // Pattern 4: Import/Export specifier brace mismatch cascading error suppression
            // If we encounter 'from' keyword in the specifier list, it likely means we have:
            // import { a from "module"  (missing closing brace)
            // In this case, break the loop to avoid parsing 'from' as an identifier
            if self.is_token(SyntaxKind::FromKeyword)
                && !self.next_token_continues_import_specifier_name()
            {
                self.last_named_imports_recovered_to_from = true;
                break;
            }

            if self.is_token(SyntaxKind::AsteriskToken) {
                self.last_named_imports_had_structural_error = true;
                self.error_identifier_expected();
                self.next_token();
                if self.is_token(SyntaxKind::CloseBraceToken) {
                    self.parse_error_at_current_token(
                        "Expression expected.",
                        tsz_common::diagnostics::diagnostic_codes::EXPRESSION_EXPECTED,
                    );
                    let _ = self.parse_expected(SyntaxKind::CloseBraceToken);
                    consumed_closing_brace = true;
                    leave_closing_brace_for_statement_recovery = false;
                    // tsc-parity: after consuming `{ * }`, treat the next
                    // `from` as unexpected — tsc's parser doesn't recover
                    // the rest of the import clause cleanly when the brace
                    // group is `{ * }`. Emits TS1434 ("Unexpected keyword or
                    // identifier") at the `from` keyword. See conformance
                    // test `es6ImportNamedImportParsingError.ts` line 1.
                    if self.is_token(SyntaxKind::FromKeyword) {
                        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.parse_error_at_current_token(
                            diagnostic_messages::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                            diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                        );
                    }
                    break;
                }
            }

            let element_start = self.token_pos();
            let diagnostics_before = self.parse_diagnostics.len();
            let spec = self.parse_import_specifier();
            elements.push(spec);
            let spec_had_errors = self.parse_diagnostics.len() > diagnostics_before;
            let spec_recovered_braced_unicode_escape_debris =
                self.current_specifier_recovered_braced_unicode_escape_debris;

            // Distinguish structural parse failures from semantic errors.
            // A specifier like `{ default }` parses successfully (consuming `default`)
            // and ends at `}` — it has semantic TS1003 but is structurally complete.
            // A specifier like `{ foo as 0n }` consumes `foo as` but gets stuck at `0n` —
            // it's not at a valid list terminator, so recovery is needed.
            let spec_failed_to_parse = spec_had_errors
                && !self.is_token(SyntaxKind::CommaToken)
                && !self.is_token(SyntaxKind::CloseBraceToken);

            if spec_failed_to_parse && self.is_token(SyntaxKind::CloseBraceToken) {
                self.last_named_imports_had_structural_error = true;
                // For malformed import specifiers like `{ 0n as foo }`, tsc
                // ends the import clause before `}` and lets statement recovery
                // surface the stray `}` / `from` follow-up diagnostics (TS1128/TS1434).
                leave_closing_brace_for_statement_recovery = true;
                break;
            }

            if spec_failed_to_parse
                && !self.is_token(SyntaxKind::CommaToken)
                && !self.is_token(SyntaxKind::CloseBraceToken)
                && !self.is_token(SyntaxKind::FromKeyword)
            {
                while !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::CommaToken)
                    && !self.is_token(SyntaxKind::FromKeyword)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    self.next_token();
                }
                if self.is_token(SyntaxKind::CloseBraceToken) {
                    self.last_named_imports_had_structural_error = true;
                    leave_closing_brace_for_statement_recovery = true;
                    break;
                }
                if self.is_token(SyntaxKind::FromKeyword) {
                    self.last_named_imports_had_structural_error = true;
                    self.last_named_imports_recovered_to_from = true;
                    leave_closing_brace_for_statement_recovery = false;
                    break;
                }
            }

            if !self.parse_optional(SyntaxKind::CommaToken) {
                if self.is_token(SyntaxKind::CloseBraceToken) {
                    break;
                }
                // tsc uses parseDelimitedList which emits `',' expected.` when
                // a comma-separated list element is not followed by `,` or `}`.
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    &format!(
                        "'{}' expected.",
                        Self::token_to_string(SyntaxKind::CommaToken)
                    ),
                    diagnostic_codes::EXPECTED,
                );
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
                    self.last_named_imports_had_structural_error = true;
                    if self.is_token(SyntaxKind::CloseBraceToken) {
                        leave_closing_brace_for_statement_recovery = true;
                    }
                    break;
                }
                // If no progress was made (specifier didn't consume any tokens),
                // consume one token to avoid infinite loop — matches tsc behavior.
                if self.token_pos() == element_start {
                    self.next_token();
                }
                // Continue parsing (tsc's parseDelimitedList continues after comma error)
            }
        }

        self.last_named_imports_consumed_closing_brace =
            if leave_closing_brace_for_statement_recovery {
                false
            } else if consumed_closing_brace {
                true
            } else {
                self.parse_expected(SyntaxKind::CloseBraceToken)
            };
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

    fn next_token_continues_import_specifier_name(&mut self) -> bool {
        let saved_token = self.current_token;
        let saved_state = self.scanner.save_state();
        self.next_token();
        let result = matches!(
            self.current_token,
            SyntaxKind::AsKeyword | SyntaxKind::CommaToken | SyntaxKind::CloseBraceToken
        );
        self.scanner.restore_state(saved_state);
        self.current_token = saved_token;
        result
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

        if self.is_token(SyntaxKind::Unknown) {
            while self.is_token(SyntaxKind::Unknown) {
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                    tsz_common::diagnostics::diagnostic_codes::INVALID_CHARACTER,
                );
                self.next_token();
            }
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                start_pos,
                start_pos,
                IdentifierData {
                    atom: Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            );
        }

        self.error_identifier_expected();

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

    fn consume_unknown_specifier_identifier_tail(&mut self) {
        while self.is_token(SyntaxKind::Unknown) {
            if self.current_unknown_starts_braced_unicode_escape_debris() {
                self.consume_braced_unicode_escape_debris_after_unknown();
                self.current_specifier_recovered_braced_unicode_escape_debris = true;
                break;
            }

            self.parse_error_at_current_token(
                tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                tsz_common::diagnostics::diagnostic_codes::INVALID_CHARACTER,
            );
            self.next_token();
        }
    }

    /// Check if current token can start a module export name (identifier, keyword, or string literal).
    fn can_parse_module_export_name(&self) -> bool {
        self.is_identifier_or_keyword() || self.is_token(SyntaxKind::StringLiteral)
    }

    /// Parse an import or export specifier with correct type-only modifier disambiguation.
    ///
    /// Matches tsc's `parseImportOrExportSpecifier` algorithm which handles the ambiguous
    /// `type` keyword that can be either a type-only modifier or an identifier name.
    ///
    /// Disambiguation rules (from tsc):
    /// - `{ type }` → not type-only, name=type
    /// - `{ type as }` → type-only, name=as
    /// - `{ type as as }` → NOT type-only, name=as, propertyName=type (rename)
    /// - `{ type as as as }` → type-only, name=as, propertyName=as
    /// - `{ type as something }` → NOT type-only, name=something, propertyName=type (rename)
    /// - `{ type something }` → type-only, name=something
    /// - `{ type something as alias }` → type-only, name=alias, propertyName=something
    pub(crate) fn parse_import_or_export_specifier(&mut self, kind: u16) -> NodeIndex {
        self.current_specifier_recovered_braced_unicode_escape_debris = false;
        let start_pos = self.token_pos();
        let mut is_type_only = false;
        let mut property_name = NodeIndex::NONE;
        let mut can_parse_as_keyword = true;

        // Track whether the name token is a reserved keyword (for TS1003 on import specifiers).
        // Matches tsc's checkIdentifierIsKeyword/checkIdentifierStart/checkIdentifierEnd.
        // Reserved words (break..with) cannot be binding identifiers in import specifiers.
        let mut check_identifier_is_keyword = self.is_reserved_word();
        let mut check_identifier_start = self.token_pos();
        let mut check_identifier_end = self.token_end();

        // Remember if the first token is `type` keyword BEFORE parsing it
        let first_token_is_type = self.is_token(SyntaxKind::TypeKeyword);

        // Parse the first name (could be `type` or any other identifier)
        let mut name = self.parse_specifier_identifier_name();
        self.consume_unknown_specifier_identifier_tail();

        // Helper macro: update keyword check state before parsing a name.
        // Equivalent to tsc's parseNameWithKeywordCheck.
        macro_rules! parse_name_with_keyword_check {
            ($self:expr) => {{
                check_identifier_is_keyword = $self.is_reserved_word();
                check_identifier_start = $self.token_pos();
                check_identifier_end = $self.token_end();
                let name = $self.parse_specifier_identifier_name();
                $self.consume_unknown_specifier_identifier_tail();
                name
            }};
        }

        // If the first name was `type`, disambiguate whether it's a modifier or a name
        if first_token_is_type {
            if self.is_token(SyntaxKind::AsKeyword) {
                // { type as ...? }
                let first_as = self.parse_specifier_identifier_name();
                if self.is_token(SyntaxKind::AsKeyword) {
                    // { type as as ...? }
                    let second_as = self.parse_specifier_identifier_name();
                    if self.can_parse_module_export_name() {
                        // { type as as something } → type-only, propertyName=as, name=something
                        is_type_only = true;
                        property_name = first_as;
                        name = parse_name_with_keyword_check!(self);
                        can_parse_as_keyword = false;
                    } else {
                        // { type as as } → NOT type-only, propertyName=type, name=as
                        property_name = name;
                        name = second_as;
                        can_parse_as_keyword = false;
                    }
                } else if self.can_parse_module_export_name() {
                    // { type as something } → NOT type-only, propertyName=type, name=something
                    property_name = name;
                    can_parse_as_keyword = false;
                    name = parse_name_with_keyword_check!(self);
                } else {
                    // { type as } → type-only, name=as
                    is_type_only = true;
                    name = first_as;
                }
            } else if self.can_parse_module_export_name() {
                // { type something ...? } → type-only, name=something
                is_type_only = true;
                name = parse_name_with_keyword_check!(self);
            }
            // else: { type } → not type-only, name=type
        }

        // Handle trailing `as alias` rename
        if can_parse_as_keyword && self.parse_optional(SyntaxKind::AsKeyword) {
            property_name = name;
            name = parse_name_with_keyword_check!(self);
        }

        // TS1003: For import specifiers, the binding name must be an identifier,
        // not a reserved keyword or string literal.
        // Matches tsc's check at the end of parseImportOrExportSpecifier.
        if kind == syntax_kind_ext::IMPORT_SPECIFIER {
            use tsz_common::diagnostics::diagnostic_codes;
            if check_identifier_is_keyword {
                self.parse_error_at(
                    check_identifier_start,
                    check_identifier_end.saturating_sub(check_identifier_start),
                    "Identifier expected.",
                    diagnostic_codes::IDENTIFIER_EXPECTED,
                );
            }
            // String literals cannot be used as local binding names in imports.
            // `import { "str" as local }` is valid (string is export name, local is binding).
            // `import { foo as "str" }` is invalid (string can't be a binding).
            // `import { "str" }` is invalid (string without alias can't be a binding).
            if let Some(name_node) = self.arena.get(name)
                && name_node.is_string_literal()
            {
                let name_start = name_node.pos;
                let name_len = name_node.end.saturating_sub(name_node.pos);
                self.parse_error_at(
                    name_start,
                    name_len,
                    "Identifier expected.",
                    diagnostic_codes::IDENTIFIER_EXPECTED,
                );
            }
        }

        let end_pos = self.token_end();
        self.arena.add_specifier(
            kind,
            start_pos,
            end_pos,
            SpecifierData {
                is_type_only,
                property_name,
                name,
            },
        )
    }

    /// Parse import specifier: x or x as y or "str" as y, with type-only modifier
    /// disambiguation. Uses shared logic with export specifier parsing.
    pub(crate) fn parse_import_specifier(&mut self) -> NodeIndex {
        self.parse_import_or_export_specifier(syntax_kind_ext::IMPORT_SPECIFIER)
    }

    // Export declarations and control flow statements → state_declarations_exports.rs
}
