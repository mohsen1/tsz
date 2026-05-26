//! Parser state - class declarations, decorators, and heritage clauses.

use super::state::{CONTEXT_FLAG_AMBIENT, CONTEXT_FLAG_IN_CLASS, ParserState};
use crate::parser::{NodeIndex, NodeList, node::ClassData, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::SyntaxKind;

/// Missing-class-body recovery at a stray `.` leaves both the abandoned class
/// close and its outer container close visible as statement-level stray braces.
const CLASS_DOT_RECOVERY_STRAY_CLOSE_BRACE_COUNT: u8 = 2;

impl ParserState {
    /// Parse class declaration
    pub(crate) fn parse_class_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Check for illegal binding identifiers (e.g., 'await' in static blocks)
        self.check_illegal_binding_identifier();

        // Parse class name - keywords like 'any', 'string' can be used as class names
        // EXCEPT extends/implements which start heritage clauses
        // AND reserved words like 'void', 'null' which cannot be identifiers
        //
        // Special case: `class implements {` and `class extends {` — if the next
        // token after extends/implements is `{`, the keyword is the class name
        // (the class body follows immediately), not a heritage clause start.
        // tsc uses the same disambiguation via isImplementsClause() lookahead.
        let is_heritage_keyword = (self.is_token(SyntaxKind::ExtendsKeyword)
            || self.is_token(SyntaxKind::ImplementsKeyword))
            && !self.look_ahead_next_is_open_brace_on_same_line();
        let mut recover_reserved_class_name_as_statement = false;
        let name = if self.is_identifier_or_keyword() && !is_heritage_keyword {
            // TS1005: Reserved words cannot be used as class names
            if self.is_reserved_word() {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
                recover_reserved_class_name_as_statement = true;
                NodeIndex::NONE
            } else {
                self.parse_identifier_name()
            }
        } else {
            NodeIndex::NONE
        };

        let (type_parameters, heritage_clauses, members) =
            if recover_reserved_class_name_as_statement {
                (None, None, self.make_node_list(Vec::new()))
            } else {
                let type_parameters = self
                    .is_token(SyntaxKind::LessThanToken)
                    .then(|| self.parse_type_parameters());
                let heritage_clauses = self.parse_heritage_clauses();
                let has_open_brace = self.parse_expected(SyntaxKind::OpenBraceToken);
                let members = if !has_open_brace && self.is_token(SyntaxKind::DotToken) {
                    self.next_token();
                    self.non_block_close_brace_statement_errors_remaining =
                        CLASS_DOT_RECOVERY_STRAY_CLOSE_BRACE_COUNT;
                    self.make_node_list(Vec::new())
                } else {
                    let class_saved_flags = self.context_flags;
                    self.context_flags |= CONTEXT_FLAG_IN_CLASS;
                    let members = self.parse_class_members();
                    self.context_flags = class_saved_flags;
                    self.parse_expected(SyntaxKind::CloseBraceToken);
                    members
                };
                (type_parameters, heritage_clauses, members)
            };

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: None,
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse class declaration with explicit modifiers.
    pub(crate) fn parse_class_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name - keywords like 'any', 'string' can be used as class names
        // EXCEPT extends/implements which start heritage clauses
        // AND reserved words like 'void', 'null' which cannot be identifiers
        //
        // Special case: `class implements {` and `class extends {` — if the next
        // token after extends/implements is `{`, the keyword is the class name
        // (the class body follows immediately), not a heritage clause start.
        let is_heritage_keyword = (self.is_token(SyntaxKind::ExtendsKeyword)
            || self.is_token(SyntaxKind::ImplementsKeyword))
            && !self.look_ahead_next_is_open_brace_on_same_line();
        let mut recover_reserved_class_name_as_statement = false;
        let name = if self.is_identifier_or_keyword() && !is_heritage_keyword {
            // TS1005: Reserved words cannot be used as class names
            if self.is_reserved_word() {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
                recover_reserved_class_name_as_statement = true;
                NodeIndex::NONE
            } else {
                self.parse_identifier_name()
            }
        } else {
            NodeIndex::NONE
        };

        let (type_parameters, heritage_clauses, members) =
            if recover_reserved_class_name_as_statement {
                (None, None, self.make_node_list(Vec::new()))
            } else {
                let type_parameters = self
                    .is_token(SyntaxKind::LessThanToken)
                    .then(|| self.parse_type_parameters());
                let heritage_clauses = self.parse_heritage_clauses();
                let has_open_brace = self.parse_expected(SyntaxKind::OpenBraceToken);
                let members = if !has_open_brace && self.is_token(SyntaxKind::DotToken) {
                    self.next_token();
                    self.non_block_close_brace_statement_errors_remaining =
                        CLASS_DOT_RECOVERY_STRAY_CLOSE_BRACE_COUNT;
                    self.make_node_list(Vec::new())
                } else {
                    let class_saved_flags = self.context_flags;
                    self.context_flags |= CONTEXT_FLAG_IN_CLASS;
                    let members = self.parse_class_members();
                    self.context_flags = class_saved_flags;
                    self.parse_expected(SyntaxKind::CloseBraceToken);
                    members
                };
                (type_parameters, heritage_clauses, members)
            };

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers,
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse abstract class declaration: abstract class Foo {}
    pub(crate) fn parse_abstract_class_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Create abstract modifier node
        let abstract_start = self.token_pos();
        self.parse_expected(SyntaxKind::AbstractKeyword);
        let abstract_end = self.token_end();
        let abstract_modifier = self.arena.add_token(
            SyntaxKind::AbstractKeyword as u16,
            abstract_start,
            abstract_end,
        );

        // Now parse the class
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name - keywords like 'any', 'string' can be used as class names
        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        // Parse type parameters: abstract class Foo<T, U> {}
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: Some(self.make_node_list(vec![abstract_modifier])),
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse declare class: declare class Foo {}
    pub(crate) fn parse_declare_class(
        &mut self,
        start_pos: u32,
        declare_modifier: NodeIndex,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::ClassKeyword);

        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        let heritage_clauses = self.parse_heritage_clauses();

        let members = if self.parse_expected(SyntaxKind::OpenBraceToken) {
            // Set ambient context for class members
            let saved_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_AMBIENT | CONTEXT_FLAG_IN_CLASS;
            let members = self.parse_class_members();
            // Restore context flags
            self.context_flags = saved_flags;
            self.parse_expected(SyntaxKind::CloseBraceToken);
            members
        } else if self.is_token(SyntaxKind::OpenParenToken) {
            // `declare class Foo();` should recover with TS1109 at `(` and not
            // cascade into class-member TS1068 diagnostics.
            self.parse_error_at(
                self.token_pos().saturating_add(1),
                1,
                "Expression expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPRESSION_EXPECTED,
            );
            self.recover_parenthesized_class_declaration_tail();
            self.make_node_list(Vec::new())
        } else {
            // Preserve existing fallback behavior for other malformed class bodies.
            let saved_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_AMBIENT | CONTEXT_FLAG_IN_CLASS;
            let members = self.parse_class_members();
            self.context_flags = saved_flags;
            self.parse_expected(SyntaxKind::CloseBraceToken);
            members
        };

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: Some(self.make_node_list(vec![declare_modifier])),
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse declare abstract class: declare abstract class Foo {}
    pub(crate) fn parse_declare_abstract_class(
        &mut self,
        start_pos: u32,
        declare_modifier: NodeIndex,
    ) -> NodeIndex {
        // Create abstract modifier node
        let abstract_start = self.token_pos();
        self.parse_expected(SyntaxKind::AbstractKeyword);
        let abstract_end = self.token_end();
        let abstract_modifier = self.arena.add_token(
            SyntaxKind::AbstractKeyword as u16,
            abstract_start,
            abstract_end,
        );

        self.parse_expected(SyntaxKind::ClassKeyword);

        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        let heritage_clauses = self.parse_heritage_clauses();

        let members = if self.parse_expected(SyntaxKind::OpenBraceToken) {
            // Set ambient context for class members
            let saved_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_AMBIENT | CONTEXT_FLAG_IN_CLASS;
            let members = self.parse_class_members();
            // Restore context flags
            self.context_flags = saved_flags;
            self.parse_expected(SyntaxKind::CloseBraceToken);
            members
        } else if self.is_token(SyntaxKind::OpenParenToken) {
            self.parse_error_at(
                self.token_pos().saturating_add(1),
                1,
                "Expression expected.",
                tsz_common::diagnostics::diagnostic_codes::EXPRESSION_EXPECTED,
            );
            self.recover_parenthesized_class_declaration_tail();
            self.make_node_list(Vec::new())
        } else {
            let saved_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_AMBIENT | CONTEXT_FLAG_IN_CLASS;
            let members = self.parse_class_members();
            self.context_flags = saved_flags;
            self.parse_expected(SyntaxKind::CloseBraceToken);
            members
        };

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: Some(self.make_node_list(vec![declare_modifier, abstract_modifier])),
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse a decorated declaration: @decorator class/function
    pub(crate) fn parse_decorated_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse decorators
        let decorators = self.parse_decorators();

        // After decorators, expect class or abstract class
        // Decorators on other declarations are invalid (TS1206)
        match self.token() {
            SyntaxKind::ClassKeyword => {
                self.parse_class_declaration_with_decorators(decorators, start_pos)
            }
            SyntaxKind::AbstractKeyword => {
                // abstract class with decorators
                self.parse_abstract_class_declaration_with_decorators(decorators, start_pos)
            }
            SyntaxKind::FunctionKeyword => {
                // TS1206: Decorators are not valid on function declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_function_declaration()
            }
            SyntaxKind::EnumKeyword => {
                // TS1206: Decorators are not valid on enum declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_enum_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::InterfaceKeyword => {
                // TS1206: Decorators are not valid on interface declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_interface_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::TypeKeyword => {
                // TS1206: Decorators are not valid on type alias declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_type_alias_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                // TS1206: Decorators are not valid on namespace/module declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_module_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword | SyntaxKind::ConstKeyword => {
                // TS1206: Decorators are not valid on variable statements
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_variable_statement_with_modifiers(Some(start_pos), decorators)
            }
            SyntaxKind::UsingKeyword => {
                // tsc does NOT emit TS1206 for `@dec using ...`; it relies on
                // the variable-declaration parser to emit TS1134 when the syntax
                // after `using` is invalid (e.g., `using 1`).  When the syntax IS
                // valid (`using x`), no error is reported at all.
                self.parse_variable_statement_with_modifiers(Some(start_pos), decorators)
            }
            SyntaxKind::ImportKeyword => {
                // TS1206: Decorators are not valid on import statements
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                // Check if this is import equals (import X = ...) or regular import
                if self.look_ahead_is_import_equals() {
                    self.parse_import_equals_declaration()
                } else {
                    self.parse_import_declaration()
                }
            }
            SyntaxKind::ExportKeyword => {
                // Export with decorators: @decorator export class Foo {}
                self.parse_export_declaration_with_decorators(start_pos, decorators)
            }
            SyntaxKind::DefaultKeyword => {
                // TS1029: `export` must precede `default`.
                use tsz_common::diagnostics::diagnostic_codes;
                let default_start = self.token_pos();
                let default_end = self.token_end();
                self.parse_error_at(
                    default_start,
                    default_end - default_start,
                    "'export' modifier must precede 'default' modifier.",
                    diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                );

                // Consume `default` so declaration parsing can continue.
                self.next_token();
                let default_modifier = self.arena.add_token(
                    SyntaxKind::DefaultKeyword as u16,
                    default_start,
                    default_end,
                );
                let mut nodes = decorators.map(|list| list.nodes).unwrap_or_default();
                nodes.push(default_modifier);
                let modifiers = Some(self.make_node_list(nodes));

                match self.token() {
                    SyntaxKind::ClassKeyword => {
                        self.parse_class_declaration_with_modifiers(start_pos, modifiers)
                    }
                    SyntaxKind::AbstractKeyword => self.parse_abstract_class_declaration(),
                    SyntaxKind::InterfaceKeyword => {
                        self.parse_interface_declaration_with_modifiers(start_pos, modifiers)
                    }
                    _ => self.parse_expression_statement(),
                }
            }
            _ => {
                let type_assertion_decorator = decorators
                    .as_ref()
                    .and_then(|list| list.nodes.first().copied())
                    .and_then(|decorator_idx| self.arena.get(decorator_idx))
                    .and_then(|decorator_node| self.arena.get_decorator(decorator_node))
                    .and_then(|decorator| self.arena.get(decorator.expression))
                    .is_some_and(|expr| expr.kind == syntax_kind_ext::TYPE_ASSERTION);

                if type_assertion_decorator && self.is_token(SyntaxKind::EndOfFileToken) {
                    let missing_brace_pos = self.token_end();
                    let already_reported_missing_brace =
                        self.parse_diagnostics.iter().any(|diag| {
                            diag.start == missing_brace_pos
                                && diag.code == diagnostic_codes::EXPECTED
                                && diag.message == "'{' expected."
                        });
                    if !already_reported_missing_brace {
                        self.parse_error_at(
                            missing_brace_pos,
                            0,
                            "'{' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                    }
                } else {
                    // TS1146: When decorators are followed by a non-declaration token,
                    // tsc emits "Declaration expected" rather than "Decorators are not valid here"
                    // because the decorator implies the user intended to write a declaration.
                    // Use token_full_start (including leading trivia) to match tsc's error position.
                    let err_pos = self.token_full_start();
                    self.parse_error_at(
                        err_pos,
                        0,
                        "Declaration expected.",
                        diagnostic_codes::DECLARATION_EXPECTED,
                    );
                }
                self.parse_expression_statement()
            }
        }
    }

    fn recover_parenthesized_class_declaration_tail(&mut self) {
        let mut paren_depth = 0usize;
        while !self.is_token(SyntaxKind::EndOfFileToken) {
            match self.token() {
                SyntaxKind::OpenParenToken => {
                    paren_depth += 1;
                    self.next_token();
                }
                SyntaxKind::CloseParenToken => {
                    paren_depth = paren_depth.saturating_sub(1);
                    self.next_token();
                    if paren_depth == 0 && self.is_token(SyntaxKind::SemicolonToken) {
                        self.next_token();
                        break;
                    }
                }
                SyntaxKind::SemicolonToken if paren_depth == 0 => {
                    self.next_token();
                    break;
                }
                SyntaxKind::OpenBraceToken if paren_depth == 0 => {
                    break;
                }
                _ => {
                    self.next_token();
                }
            }
        }
    }

    /// Parse decorators: @decorator1 @decorator2(arg) ...
    pub(crate) fn parse_decorators(&mut self) -> Option<NodeList> {
        if !self.is_token(SyntaxKind::AtToken) {
            return None;
        }

        let mut decorators = Vec::new();

        while self.is_token(SyntaxKind::AtToken) {
            if let Some(decorator) = self.try_parse_decorator() {
                decorators.push(decorator);
            } else {
                break;
            }
        }

        if decorators.is_empty() {
            None
        } else {
            Some(self.make_node_list(decorators))
        }
    }

    /// Try to parse a single decorator
    pub(crate) fn try_parse_decorator(&mut self) -> Option<NodeIndex> {
        if !self.is_token(SyntaxKind::AtToken) {
            return None;
        }

        let start_pos = self.token_pos();
        let saved_diagnostics_len = self.parse_diagnostics.len();
        let snapshot = self.scanner.save_state();
        let at_token = self.current_token;
        self.next_token(); // consume @
        if self.is_token(SyntaxKind::Unknown) {
            self.scanner.restore_state(snapshot);
            self.current_token = at_token;
            return None;
        }

        // Parse the decorator expression (identifier, member access, or call)
        // Set CONTEXT_FLAG_IN_DECORATOR so that '[' is NOT treated as element access
        // (it starts a computed property name on the decorated member instead)
        let saved_flags = self.context_flags;
        self.context_flags |= crate::parser::state::CONTEXT_FLAG_IN_DECORATOR;
        let parenthesized_await_error_pos = self
            .look_ahead_decorator_parenthesized_await_error_pos()
            .filter(|_| !self.is_token(SyntaxKind::AwaitKeyword));
        if self.is_token(SyntaxKind::AwaitKeyword) {
            self.parse_error_at_current_token(
                "Expression expected.",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
        }
        let expression = self.parse_left_hand_side_expression();
        if let Some(error_pos) = parenthesized_await_error_pos {
            self.parse_error_at(
                error_pos,
                1,
                "Expression expected.",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
        }
        self.context_flags = saved_flags;

        if self.is_token(SyntaxKind::EndOfFileToken)
            && let Some(expr_node) = self.arena.get(expression)
            && expr_node.kind == syntax_kind_ext::TYPE_ASSERTION
        {
            let mut diagnostics = self.parse_diagnostics.split_off(saved_diagnostics_len);
            diagnostics.retain(|diag| {
                !(diag.code == diagnostic_codes::EXPRESSION_EXPECTED
                    && diag.start >= self.token_full_start())
            });
            self.parse_diagnostics.extend(diagnostics);
            self.parse_error_at(
                expr_node.pos,
                0,
                "Expression expected.",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
        }

        let end_pos = self.token_end();
        Some(self.arena.add_decorator(
            syntax_kind_ext::DECORATOR,
            start_pos,
            end_pos,
            crate::parser::node::DecoratorData { expression },
        ))
    }

    fn look_ahead_decorator_parenthesized_await_error_pos(&mut self) -> Option<u32> {
        if !self.is_token(SyntaxKind::OpenParenToken) {
            return None;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let result = if self.is_token(SyntaxKind::AwaitKeyword) {
            self.next_token();
            self.is_token(SyntaxKind::CloseParenToken)
                .then(|| self.token_pos())
        } else {
            None
        };
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Parse class declaration with pre-parsed decorators
    pub(crate) fn parse_class_declaration_with_decorators(
        &mut self,
        decorators: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name
        // Check for illegal binding identifiers (e.g., 'await' in static blocks)
        self.check_illegal_binding_identifier();

        let name = if self.is_token(SyntaxKind::Identifier) {
            self.parse_identifier()
        } else {
            NodeIndex::NONE
        };

        // Parse type parameters
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        // Create a modifiers list from decorators
        // In TypeScript, decorators are part of the modifiers
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: decorators,
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse abstract class declaration with pre-parsed decorators
    pub(crate) fn parse_abstract_class_declaration_with_decorators(
        &mut self,
        decorators: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        // Create abstract modifier node
        let abstract_start = self.token_pos();
        self.parse_expected(SyntaxKind::AbstractKeyword);
        let abstract_end = self.token_end();
        let abstract_modifier = self.arena.add_token(
            SyntaxKind::AbstractKeyword as u16,
            abstract_start,
            abstract_end,
        );

        // Now parse the class
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name
        let name = if self.is_token(SyntaxKind::Identifier) {
            self.parse_identifier()
        } else {
            NodeIndex::NONE
        };

        // Parse type parameters
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        // Combine decorators with abstract modifier
        let modifiers = if let Some(dec_list) = decorators {
            // Add abstract modifier to decorator list
            let mut nodes: Vec<NodeIndex> = dec_list.nodes;
            nodes.push(abstract_modifier);
            Some(self.make_node_list(nodes))
        } else {
            Some(self.make_node_list(vec![abstract_modifier]))
        };

        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers,
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse heritage clauses (extends, implements)
    pub(crate) fn parse_heritage_clauses(&mut self) -> Option<NodeList> {
        let mut clauses = Vec::new();
        let mut seen_extends = false;
        let mut seen_implements = false;

        loop {
            if self.is_token(SyntaxKind::ExtendsKeyword) {
                if let Some(clause) =
                    self.parse_heritage_clause_extends(&mut seen_extends, seen_implements)
                {
                    clauses.push(clause);
                }
                continue;
            }

            if self.is_token(SyntaxKind::ImplementsKeyword) {
                if let Some(clause) = self.parse_heritage_clause_implements(&mut seen_implements) {
                    clauses.push(clause);
                }
                continue;
            }

            break;
        }

        if clauses.is_empty() {
            None
        } else {
            Some(self.make_node_list(clauses))
        }
    }

    fn parse_heritage_clause_extends(
        &mut self,
        seen_extends: &mut bool,
        seen_implements: bool,
    ) -> Option<NodeIndex> {
        use tsz_common::diagnostics::diagnostic_codes;

        let start_pos = self.token_pos();
        let is_duplicate = *seen_extends;

        if is_duplicate {
            self.parse_error_at_current_token(
                "'extends' clause already seen.",
                diagnostic_codes::EXTENDS_CLAUSE_ALREADY_SEEN,
            );
        } else if seen_implements {
            self.parse_error_at_current_token(
                "'extends' clause must precede 'implements' clause.",
                diagnostic_codes::EXTENDS_CLAUSE_MUST_PRECEDE_IMPLEMENTS_CLAUSE,
            );
        }

        *seen_extends = true;
        self.next_token();

        if self.is_token(SyntaxKind::ImplementsKeyword)
            || (self.is_token(SyntaxKind::OpenBraceToken)
                && !self.look_ahead_is_object_literal_heritage_expression())
        {
            // Use full start position (including leading trivia) to match TSC's
            // grammarErrorAtPos(node, types.pos, 0, ...) which uses getTokenFullStart().
            self.parse_error_at(
                self.token_full_start(),
                0,
                "'extends' list cannot be empty.",
                diagnostic_codes::LIST_CANNOT_BE_EMPTY,
            );
            // Still create the heritage clause with empty types list for error recovery.
            // tsc preserves the `extends` keyword in the output even when the list is empty.
            let end_pos = self.token_full_start();
            return Some(self.arena.add_heritage(
                syntax_kind_ext::HERITAGE_CLAUSE,
                start_pos,
                end_pos,
                crate::parser::node::HeritageData {
                    token: SyntaxKind::ExtendsKeyword as u16,
                    types: self.make_node_list(Vec::new()),
                },
            ));
        }

        let type_ref = self.parse_heritage_type_reference();
        let mut type_refs = vec![type_ref];

        while self.is_token(SyntaxKind::CommaToken) {
            let comma_pos = self.token_pos();
            let comma_end = self.token_end();
            self.next_token();
            if self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::ImplementsKeyword)
            {
                self.parse_error_at(
                    comma_pos,
                    comma_end - comma_pos,
                    tsz_common::diagnostics::diagnostic_messages::TRAILING_COMMA_NOT_ALLOWED,
                    diagnostic_codes::TRAILING_COMMA_NOT_ALLOWED,
                );
                break;
            }
            // Only emit "Classes can only extend a single class" for the first
            // (non-duplicate) extends clause. For a duplicate `extends` clause,
            // the duplicate-keyword diagnostic already covers the bases.
            if !is_duplicate {
                self.parse_error_at(
                    self.token_pos(),
                    0,
                    "Classes can only extend a single class.",
                    diagnostic_codes::CLASSES_CAN_ONLY_EXTEND_A_SINGLE_CLASS,
                );
            }
            let extra_ref = self.parse_heritage_type_reference();
            type_refs.push(extra_ref);
        }

        let end_pos = self.token_end();
        Some(self.arena.add_heritage(
            syntax_kind_ext::HERITAGE_CLAUSE,
            start_pos,
            end_pos,
            crate::parser::node::HeritageData {
                token: SyntaxKind::ExtendsKeyword as u16,
                types: self.make_node_list(type_refs),
            },
        ))
    }

    fn parse_heritage_clause_implements(
        &mut self,
        seen_implements: &mut bool,
    ) -> Option<NodeIndex> {
        use tsz_common::diagnostics::diagnostic_codes;

        let start_pos = self.token_pos();
        if *seen_implements {
            self.parse_error_at_current_token(
                "'implements' clause already seen.",
                diagnostic_codes::IMPLEMENTS_CLAUSE_ALREADY_SEEN,
            );
        }

        let is_duplicate = *seen_implements;
        *seen_implements = true;
        self.next_token();

        // TS1097: 'implements' list cannot be empty.
        if self.is_token(SyntaxKind::OpenBraceToken) || self.is_token(SyntaxKind::ExtendsKeyword) {
            // Use full start position (including leading trivia) to match TSC's
            // grammarErrorAtPos(node, types.pos, 0, ...) which uses getTokenFullStart().
            self.parse_error_at(
                self.token_full_start(),
                0,
                "'implements' list cannot be empty.",
                diagnostic_codes::LIST_CANNOT_BE_EMPTY,
            );
            return None;
        }

        if is_duplicate {
            self.skip_heritage_type_references_for_recovery();
            return None;
        }

        let mut types = Vec::new();
        loop {
            let type_ref = self.parse_heritage_type_reference();
            types.push(type_ref);
            if self.is_token(SyntaxKind::CommaToken) {
                let comma_pos = self.token_pos();
                let comma_end = self.token_end();
                self.next_token();
                // Trailing comma before { — emit TS1009 like the extends clause does
                if self.is_token(SyntaxKind::OpenBraceToken)
                    || self.is_token(SyntaxKind::ExtendsKeyword)
                {
                    self.parse_error_at(
                        comma_pos,
                        comma_end - comma_pos,
                        tsz_common::diagnostics::diagnostic_messages::TRAILING_COMMA_NOT_ALLOWED,
                        diagnostic_codes::TRAILING_COMMA_NOT_ALLOWED,
                    );
                    break;
                }
            } else {
                break;
            }
        }

        let end_pos = self.token_end();
        Some(self.arena.add_heritage(
            syntax_kind_ext::HERITAGE_CLAUSE,
            start_pos,
            end_pos,
            crate::parser::node::HeritageData {
                token: SyntaxKind::ImplementsKeyword as u16,
                types: self.make_node_list(types),
            },
        ))
    }

    fn skip_heritage_type_references_for_recovery(&mut self) {
        while !self.is_token(SyntaxKind::OpenBraceToken)
            && !self.is_token(SyntaxKind::ExtendsKeyword)
            && !self.is_token(SyntaxKind::ImplementsKeyword)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let _ = self.parse_heritage_type_reference();
            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }
    }

    /// Parse a heritage type reference: Foo or Foo<T> or Foo.Bar<T> or base<T>()
    /// This is used in extends/implements clauses
    pub(crate) fn parse_heritage_type_reference(&mut self) -> NodeIndex {
        // parse_heritage_left_hand_expression now handles:
        // - Simple identifiers: Foo
        // - Property access: Foo.Bar.Baz
        // - Type arguments: Foo<T>
        // - Call expressions: Mixin(Parent) or base<T>()
        self.parse_heritage_left_hand_expression()
    }

    fn look_ahead_is_object_literal_heritage_expression(&mut self) -> bool {
        if !self.is_token(SyntaxKind::OpenBraceToken) {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let saved_token = self.current_token;
        let mut brace_depth = 0u32;
        let mut result = false;

        while !self.is_token(SyntaxKind::EndOfFileToken) {
            match self.token() {
                SyntaxKind::OpenBraceToken => {
                    brace_depth += 1;
                    self.next_token();
                }
                SyntaxKind::CloseBraceToken => {
                    if brace_depth == 0 {
                        break;
                    }
                    brace_depth -= 1;
                    self.next_token();
                    if brace_depth == 0 {
                        result = self.is_token(SyntaxKind::OpenBraceToken);
                        break;
                    }
                }
                _ => {
                    self.next_token();
                }
            }
        }

        self.scanner.restore_state(snapshot);
        self.current_token = saved_token;
        result
    }

    /// Parse heritage type reference for interfaces (extends clause).
    /// Interfaces must reference types; literals or arbitrary expressions should produce diagnostics.
    pub(crate) fn parse_interface_heritage_type_reference(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::OpenBracketToken)
        {
            let start_pos = self.token_pos();
            let invalid_ref = self.parse_heritage_type_reference();
            let end_pos = self.token_end();
            self.parse_error_at(
                start_pos,
                end_pos - start_pos,
                "An interface can only extend an identifier/qualified-name with optional type arguments.",
                diagnostic_codes::AN_INTERFACE_CAN_ONLY_EXTEND_AN_IDENTIFIER_QUALIFIED_NAME_WITH_OPTIONAL_TYPE_ARG,
            );
            return invalid_ref;
        }

        if matches!(
            self.token(),
            SyntaxKind::NullKeyword
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::UndefinedKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::NumericLiteral
                | SyntaxKind::StringLiteral
                | SyntaxKind::BigIntLiteral
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::TemplateHead
                | SyntaxKind::ClassKeyword
                | SyntaxKind::NewKeyword
                | SyntaxKind::OpenParenToken
        ) {
            let start = self.token_pos();
            let end = self.token_end();
            self.parse_error_at_current_token(
                "Type name expected in interface extends clause.",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            self.next_token();
            return self.arena.add_token(SyntaxKind::Unknown as u16, start, end);
        }

        self.parse_heritage_type_reference()
    }

    /// Parse left-hand expression for heritage clauses: Foo, Foo.Bar, or Mixin(Parent)
    /// This is a subset of member expression that allows identifiers, dots, and call expressions
    pub(crate) fn parse_heritage_left_hand_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_heritage_left_hand_expression_base();

        while let Some(next_expr) = self.parse_heritage_left_hand_expression_chain(start_pos, expr)
        {
            expr = next_expr;
        }

        expr
    }

    fn parse_heritage_left_hand_expression_base(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        if self.is_token(SyntaxKind::ClassKeyword) {
            self.parse_class_expression()
        } else if self.is_token(SyntaxKind::AwaitKeyword) {
            // tsc allows `await` as an identifier in `.d.ts` files, including
            // heritage clause references like `declare class C extends await {}`.
            // Skip the TS1109 "Expression expected" emission in that context.
            if !self.is_declaration_file() {
                self.parse_error_at_current_token(
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
            }
            self.parse_identifier_name()
        } else if self.is_token(SyntaxKind::ThisKeyword) {
            self.parse_this_expression()
        } else if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::NewKeyword)
        {
            self.parse_left_hand_side_expression()
        } else if self.is_token(SyntaxKind::VoidKeyword) {
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            self.error_expression_expected();
            self.next_token();
            self.arena
                .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
        } else if matches!(
            self.token(),
            SyntaxKind::NullKeyword
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::UndefinedKeyword
                | SyntaxKind::NumericLiteral
                | SyntaxKind::StringLiteral
                | SyntaxKind::BigIntLiteral
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::TemplateHead
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::OpenBracketToken
        ) {
            self.parse_primary_expression()
        } else if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            self.parse_error_at_current_token(
                "Class name or type expression expected",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            self.next_token();
            self.arena
                .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
        }
    }

    fn parse_heritage_left_hand_expression_chain(
        &mut self,
        start_pos: u32,
        expr: NodeIndex,
    ) -> Option<NodeIndex> {
        match self.token() {
            SyntaxKind::DotToken => {
                self.next_token();
                let name = if self.is_identifier_or_keyword() {
                    self.parse_identifier_name()
                } else {
                    self.parse_identifier()
                };
                let end_pos = self.token_end();
                Some(self.arena.add_access_expr(
                    syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                    start_pos,
                    end_pos,
                    crate::parser::node::AccessExprData {
                        expression: expr,
                        name_or_argument: name,
                        question_dot_token: false,
                    },
                ))
            }
            SyntaxKind::QuestionDotToken => {
                self.next_token();
                let name = if self.is_identifier_or_keyword() {
                    self.parse_identifier_name()
                } else {
                    self.parse_identifier()
                };
                let end_pos = self.token_end();
                Some(self.arena.add_access_expr(
                    syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                    start_pos,
                    end_pos,
                    crate::parser::node::AccessExprData {
                        expression: expr,
                        name_or_argument: name,
                        question_dot_token: true,
                    },
                ))
            }
            SyntaxKind::LessThanToken => {
                let invalid_await_heritage = self.node_is_identifier_text(expr, "await");
                if invalid_await_heritage {
                    self.parse_error_at_current_token(
                        "Expression expected.",
                        diagnostic_codes::EXPRESSION_EXPECTED,
                    );
                }
                self.next_token();
                let mut type_args = Vec::new();
                while !self.is_token(SyntaxKind::GreaterThanToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    type_args.push(self.parse_type());
                    if !self.parse_optional(SyntaxKind::CommaToken) {
                        break;
                    }
                }
                if invalid_await_heritage && self.is_token(SyntaxKind::GreaterThanToken) {
                    self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
                }
                self.parse_expected(SyntaxKind::GreaterThanToken);
                if self.is_token(SyntaxKind::OpenParenToken) {
                    self.next_token();
                    let (end_pos, args) = self.parse_heritage_call_arguments();
                    Some(self.arena.add_call_expr(
                        syntax_kind_ext::CALL_EXPRESSION,
                        start_pos,
                        end_pos,
                        crate::parser::node::CallExprData {
                            expression: expr,
                            type_arguments: Some(self.make_node_list(type_args)),
                            arguments: Some(args),
                        },
                    ))
                } else {
                    Some(self.arena.add_expr_with_type_args(
                        syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS,
                        start_pos,
                        self.token_end(),
                        crate::parser::node::ExprWithTypeArgsData {
                            expression: expr,
                            type_arguments: Some(self.make_node_list(type_args)),
                        },
                    ))
                }
            }
            SyntaxKind::OpenParenToken => {
                self.next_token();
                let (end_pos, args) = self.parse_heritage_call_arguments();
                Some(self.arena.add_call_expr(
                    syntax_kind_ext::CALL_EXPRESSION,
                    start_pos,
                    end_pos,
                    crate::parser::node::CallExprData {
                        expression: expr,
                        type_arguments: None,
                        arguments: Some(args),
                    },
                ))
            }
            _ => None,
        }
    }

    fn node_is_identifier_text(&self, node_idx: NodeIndex, text: &str) -> bool {
        self.arena
            .get(node_idx)
            .and_then(|node| self.arena.get_identifier(node))
            .is_some_and(|ident| self.arena.resolve_identifier_text(ident) == text)
    }

    fn parse_heritage_call_arguments(&mut self) -> (u32, NodeList) {
        let mut args = Vec::new();
        while !self.is_token(SyntaxKind::CloseParenToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            args.push(self.parse_assignment_expression());
            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseParenToken);
        (end_pos, self.make_node_list(args))
    }
}
