//! Parser state - class expression and class declaration parsing.

use super::state::{
    CONTEXT_FLAG_AMBIENT, CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS, CONTEXT_FLAG_IN_CLASS,
    CONTEXT_FLAG_PARAMETER_DEFAULT, ParserState,
};
use crate::parser::{NodeIndex, NodeList, node::ClassData, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::SyntaxKind;

impl ParserState {
    /// Parse class expression: class {} or class Name {}
    ///
    /// Unlike class declarations, class expressions can be anonymous.
    pub(crate) fn parse_class_expression(&mut self) -> NodeIndex {
        self.parse_class_expression_with_decorators(None, self.token_pos())
    }

    pub(crate) fn parse_class_expression_with_decorators(
        &mut self,
        decorators: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse optional name (class expressions can be anonymous)
        // Like class declarations, keywords can be used as class names
        // EXCEPT extends/implements which start heritage clauses
        let name = if self.is_identifier_or_keyword()
            && !self.is_token(SyntaxKind::ExtendsKeyword)
            && !self.is_token(SyntaxKind::ImplementsKeyword)
        {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        // Parse optional type parameters
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage (extends/implements)
        let heritage = self.parse_heritage_clauses();

        // Parse body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        self.arena.add_class(
            syntax_kind_ext::CLASS_EXPRESSION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: decorators,
                name,
                type_parameters,
                heritage_clauses: heritage,
                members,
            },
        )
    }

    /// Parse parameter list
    pub(crate) fn parse_parameter_list(&mut self) -> NodeList {
        let mut params = Vec::new();
        let mut seen_rest_parameter = false;
        let mut emitted_rest_error = false;

        while !self.is_token(SyntaxKind::CloseParenToken) {
            // If we see `=>` before any parameters were parsed, this is likely a
            // degenerate case like `function =>` with no parens. Don't consume `=>`
            // here — let the caller handle it, avoiding a spurious `)` expected error.
            if self.is_token(SyntaxKind::EqualsGreaterThanToken) && params.is_empty() {
                break;
            }

            // TS1014: A rest parameter must be last in a parameter list
            // Check BEFORE parsing the next parameter (but only emit once)
            if seen_rest_parameter && !emitted_rest_error {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "A rest parameter must be last in a parameter list.",
                    diagnostic_codes::A_REST_PARAMETER_MUST_BE_LAST_IN_A_PARAMETER_LIST,
                );
                emitted_rest_error = true;
            }

            let param = self.parse_parameter();

            // Check if this is a rest parameter (...)
            let is_rest_param = if let Some(node) = self.arena.get(param) {
                if let Some(param_data) = self.arena.get_parameter(node) {
                    param_data.dot_dot_dot_token
                } else {
                    false
                }
            } else {
                false
            };

            seen_rest_parameter = seen_rest_parameter || is_rest_param;
            params.push(param);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                // Recovery: in malformed parameter initializers like
                // `function* f(a = yield => yield) {}` or
                // `async function f(a = await => await) {}`
                // treat `=>` as a missing comma boundary to continue parsing.
                if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                    self.error_comma_expected();
                    self.next_token(); // consume =>
                    if self.is_parameter_start() {
                        continue;
                    }
                    break;
                }
                // Trailing commas are allowed in parameter lists
                // Only emit an error if we have another parameter without a comma
                if !self.is_token(SyntaxKind::CloseParenToken) && self.is_parameter_start() {
                    // Emit TS1005 for missing comma between parameters: f(a b)
                    self.error_comma_expected();
                }
                break;
            }
        }

        self.make_node_list(params)
    }

    /// Check if current token is a valid parameter modifier
    const fn is_valid_parameter_modifier(&self) -> bool {
        matches!(
            self.current_token,
            SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::OverrideKeyword
        )
    }

    /// Check if current token is a modifier keyword used as a parameter modifier.
    /// This includes invalid modifiers like static/export that tsc accepts during
    /// parsing but reports TS1090 for in the checker.
    /// Uses look-ahead to distinguish `(static x: number)` (modifier) from
    /// `(async: boolean)` (parameter name).
    pub(crate) fn is_parameter_modifier(&mut self) -> bool {
        if !self.is_valid_parameter_modifier()
            && !matches!(
                self.current_token,
                SyntaxKind::StaticKeyword
                    | SyntaxKind::ExportKeyword
                    | SyntaxKind::DeclareKeyword
                    | SyntaxKind::AsyncKeyword
                    | SyntaxKind::AbstractKeyword
                    | SyntaxKind::AccessorKeyword
                    | SyntaxKind::ConstKeyword
                    | SyntaxKind::DefaultKeyword
                    | SyntaxKind::InKeyword
                    | SyntaxKind::OutKeyword
            )
        {
            return false;
        }
        // Look ahead: if the next token can follow a modifier (identifier/keyword,
        // string/number literal, [, {, *, ...), then this keyword is being used as
        // a modifier. Otherwise it's a parameter name (e.g., `(readonly)` or
        // `(async: boolean)`). This applies to ALL modifier keywords including
        // valid ones like `readonly` — when `readonly` is followed by `)` it's
        // a parameter name, not a modifier.
        // This mirrors tsc's canFollowModifier() + isLiteralPropertyName() check.
        let snapshot = self.scanner.save_state();
        let saved_token = self.current_token;
        self.next_token();
        let can_follow = !self.scanner.has_preceding_line_break()
            && (matches!(
                self.current_token,
                SyntaxKind::OpenBracketToken
                    | SyntaxKind::OpenBraceToken
                    | SyntaxKind::AsteriskToken
                    | SyntaxKind::DotDotDotToken
                    | SyntaxKind::StringLiteral
                    | SyntaxKind::NumericLiteral
            ) || self.is_identifier_or_keyword());
        self.scanner.restore_state(snapshot);
        self.current_token = saved_token;
        can_follow
    }

    /// Parse parameter modifiers (public, private, protected, readonly, override,
    /// and invalid ones like static/export/declare/async which get TS1090).
    pub(crate) fn parse_parameter_modifiers(&mut self) -> Option<NodeList> {
        let mut modifiers = Vec::new();
        let mut seen_readonly = false;
        let mut seen_accessibility = false;
        let mut reported_accessibility_duplicate = false;

        while self.is_parameter_modifier() {
            let mod_start = self.token_pos();
            let mod_kind = self.current_token;

            // Emit TS1090 for modifiers that cannot appear on parameters.
            // tsc does this in the checker via checkGrammarModifiers, but we
            // emit it here during parsing so we don't need checker support yet.
            if !self.is_valid_parameter_modifier() {
                use tsz_common::diagnostics::diagnostic_codes;
                let modifier_name = match mod_kind {
                    SyntaxKind::StaticKeyword => "static",
                    SyntaxKind::ExportKeyword => "export",
                    SyntaxKind::DeclareKeyword => "declare",
                    SyntaxKind::AsyncKeyword => "async",
                    SyntaxKind::AbstractKeyword => "abstract",
                    SyntaxKind::AccessorKeyword => "accessor",
                    SyntaxKind::ConstKeyword => "const",
                    SyntaxKind::DefaultKeyword => "default",
                    SyntaxKind::InKeyword => "in",
                    SyntaxKind::OutKeyword => "out",
                    _ => "modifier",
                };
                self.parse_error_at_current_token(
                    &format!("'{modifier_name}' modifier cannot appear on a parameter."),
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_PARAMETER,
                );
            }

            // Check for modifier ordering violations
            // Parameter modifiers must be in order: accessibility, readonly
            if matches!(
                mod_kind,
                SyntaxKind::PublicKeyword
                    | SyntaxKind::PrivateKeyword
                    | SyntaxKind::ProtectedKeyword
            ) {
                if seen_accessibility && !reported_accessibility_duplicate {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Accessibility modifier already seen.",
                        diagnostic_codes::ACCESSIBILITY_MODIFIER_ALREADY_SEEN,
                    );
                    reported_accessibility_duplicate = true;
                }
                // TS1029: Accessibility modifier must come before readonly
                if seen_readonly {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'accessibility modifier' must come before 'readonly' modifier.",
                        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                seen_accessibility = true;
            } else if mod_kind == SyntaxKind::ReadonlyKeyword {
                seen_readonly = true;
            }

            self.next_token();
            let mod_end = self.token_end();
            modifiers.push(self.arena.add_token(mod_kind as u16, mod_start, mod_end));
        }

        if modifiers.is_empty() {
            None
        } else {
            Some(self.make_node_list(modifiers))
        }
    }

    /// Parse a single parameter
    pub(crate) fn parse_parameter(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse parameter decorators and parameter modifiers (public/private/readonly).
        // We store decorators in the same `modifiers` list used elsewhere in the Thin AST.
        let decorators = self.parse_decorators();
        let param_modifiers = self.parse_parameter_modifiers();
        let modifiers = match (decorators, param_modifiers) {
            (None, None) => None,
            (Some(list), None) | (None, Some(list)) => Some(list),
            (Some(decorators), Some(param_modifiers)) => {
                let mut nodes = Vec::with_capacity(
                    decorators
                        .nodes
                        .len()
                        .saturating_add(param_modifiers.nodes.len()),
                );
                nodes.extend(decorators.nodes);
                nodes.extend(param_modifiers.nodes);
                Some(self.make_node_list(nodes))
            }
        };

        // Parse rest parameter (...)
        let dot_dot_dot_token = self.parse_optional(SyntaxKind::DotDotDotToken);

        // Check for illegal binding identifiers (e.g., 'await' in async contexts, 'yield' in generator contexts)
        // This must be called BEFORE parsing the parameter name to catch reserved words
        self.check_illegal_binding_identifier();
        if (self.context_flags & CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS) != 0
            && self.is_token(SyntaxKind::StaticKeyword)
        {
            self.parse_error_at_current_token(
                "Identifier expected. 'static' is a reserved word in strict mode. Class definitions are automatically in strict mode.",
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
            );
        }

        // TS18009: Check for private identifiers used as parameters (check before parsing)
        if self.is_token(SyntaxKind::PrivateIdentifier) {
            let start = self.token_pos();
            let length = self.token_end() - start;
            self.parse_error_at(
                start,
                length,
                "Private identifiers cannot be used as parameters.",
                diagnostic_codes::PRIVATE_IDENTIFIERS_CANNOT_BE_USED_AS_PARAMETERS,
            );
        }

        // Parse parameter name - can be an identifier, keyword, or binding pattern
        let name = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_object_binding_pattern()
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            self.parse_array_binding_pattern()
        } else if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            self.parse_identifier()
        };

        // Parse optional question mark
        let question_token = self.parse_optional(SyntaxKind::QuestionToken);

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
            // Check if parameter has both optional marker (?) and initializer (=)
            // TS1015: Parameter cannot have question mark and initializer
            // This applies to all parameter contexts, including arrow functions.
            if question_token {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "A parameter cannot have question mark and initializer.",
                    diagnostic_codes::PARAMETER_CANNOT_HAVE_QUESTION_MARK_AND_INITIALIZER,
                );
            }

            // Default parameter values are evaluated in the parent scope, not in the function body.
            // Set parameter default context flag to detect 'await' usage.
            // IMPORTANT: Keep async context set - TSC emits TS1109 "Expression expected" when
            // 'await' appears in a parameter default without an operand (e.g., `async (a = await)`)
            let saved_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_PARAMETER_DEFAULT;
            let initializer = self.parse_assignment_expression();
            if initializer.is_none() {
                // Emit TS1109 for missing parameter default value: param = [missing]
                self.error_expression_expected();
            }
            self.context_flags = saved_flags;
            initializer
        } else {
            NodeIndex::NONE
        };

        // TS1047: A rest parameter cannot be optional
        if dot_dot_dot_token && question_token {
            use tsz_common::diagnostics::diagnostic_codes;
            if let Some(node) = self.arena.get(name) {
                self.parse_error_at(
                    node.pos,
                    node.end - node.pos,
                    "A rest parameter cannot be optional.",
                    diagnostic_codes::A_REST_PARAMETER_CANNOT_BE_OPTIONAL,
                );
            }
        }

        // TS1048: A rest parameter cannot have an initializer
        if dot_dot_dot_token && initializer != NodeIndex::NONE {
            use tsz_common::diagnostics::diagnostic_codes;
            if let Some(node) = self.arena.get(name) {
                self.parse_error_at(
                    node.pos,
                    node.end - node.pos,
                    "A rest parameter cannot have an initializer.",
                    diagnostic_codes::A_REST_PARAMETER_CANNOT_HAVE_AN_INITIALIZER,
                );
            }
        }

        let end_pos = self.token_end();
        self.arena.add_parameter(
            syntax_kind_ext::PARAMETER,
            start_pos,
            end_pos,
            crate::parser::node::ParameterData {
                modifiers,
                dot_dot_dot_token,
                name,
                question_token,
                type_annotation,
                initializer,
            },
        )
    }

    /// Parse class declaration
    pub(crate) fn parse_class_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name - keywords like 'any', 'string' can be used as class names
        // EXCEPT extends/implements which start heritage clauses
        // AND reserved words like 'void', 'null' which cannot be identifiers
        let name = if self.is_identifier_or_keyword()
            && !self.is_token(SyntaxKind::ExtendsKeyword)
            && !self.is_token(SyntaxKind::ImplementsKeyword)
        {
            // TS1005: Reserved words cannot be used as class names
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
            NodeIndex::NONE
        };

        // Parse type parameters: class Foo<T, U> {}
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
        let name = if self.is_identifier_or_keyword()
            && !self.is_token(SyntaxKind::ExtendsKeyword)
            && !self.is_token(SyntaxKind::ImplementsKeyword)
        {
            // TS1005: Reserved words cannot be used as class names
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
            NodeIndex::NONE
        };

        // Parse type parameters: class Foo<T, U> {}
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

        self.parse_expected(SyntaxKind::OpenBraceToken);

        // Set ambient context for class members
        let saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_AMBIENT | CONTEXT_FLAG_IN_CLASS;

        let members = self.parse_class_members();

        // Restore context flags
        self.context_flags = saved_flags;

        self.parse_expected(SyntaxKind::CloseBraceToken);

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

        self.parse_expected(SyntaxKind::OpenBraceToken);

        // Set ambient context for class members
        let saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_AMBIENT | CONTEXT_FLAG_IN_CLASS;

        let members = self.parse_class_members();

        // Restore context flags
        self.context_flags = saved_flags;

        self.parse_expected(SyntaxKind::CloseBraceToken);

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
            SyntaxKind::VarKeyword
            | SyntaxKind::LetKeyword
            | SyntaxKind::ConstKeyword
            | SyntaxKind::UsingKeyword => {
                // TS1206: Decorators are not valid on variable/using statements
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
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
                // TS1206: Decorators are not valid on expression statements
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_expression_statement()
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
        let expression = self.parse_left_hand_side_expression();
        self.context_flags = saved_flags;

        let end_pos = self.token_end();
        Some(self.arena.add_decorator(
            syntax_kind_ext::DECORATOR,
            start_pos,
            end_pos,
            crate::parser::node::DecoratorData { expression },
        ))
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
                "extends clause already seen.",
                diagnostic_codes::EXTENDS_CLAUSE_ALREADY_SEEN,
            );
        } else if seen_implements {
            self.parse_error_at_current_token(
                "extends clause must precede implements clause.",
                diagnostic_codes::EXTENDS_CLAUSE_MUST_PRECEDE_IMPLEMENTS_CLAUSE,
            );
        }

        let should_add = !*seen_extends;
        *seen_extends = true;
        self.next_token();

        if self.is_token(SyntaxKind::OpenBraceToken) || self.is_token(SyntaxKind::ImplementsKeyword)
        {
            self.parse_error_at_current_token(
                "'extends' list cannot be empty.",
                diagnostic_codes::LIST_CANNOT_BE_EMPTY,
            );
            return None;
        }

        let type_ref = self.parse_heritage_type_reference();

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
            self.parse_error_at(
                comma_pos,
                comma_end - comma_pos,
                "Classes can only extend a single class.",
                diagnostic_codes::CLASSES_CAN_ONLY_EXTEND_A_SINGLE_CLASS,
            );
            let _ = self.parse_heritage_type_reference();
        }

        if !should_add {
            return None;
        }

        let end_pos = self.token_end();
        Some(self.arena.add_heritage(
            syntax_kind_ext::HERITAGE_CLAUSE,
            start_pos,
            end_pos,
            crate::parser::node::HeritageData {
                token: SyntaxKind::ExtendsKeyword as u16,
                types: self.make_node_list(vec![type_ref]),
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
                "implements clause already seen.",
                diagnostic_codes::IMPLEMENTS_CLAUSE_ALREADY_SEEN,
            );
        }

        let should_add = !*seen_implements;
        *seen_implements = true;
        self.next_token();

        // TS1097: 'implements' list cannot be empty.
        if self.is_token(SyntaxKind::OpenBraceToken) || self.is_token(SyntaxKind::ExtendsKeyword) {
            self.parse_error_at_current_token(
                "'implements' list cannot be empty.",
                diagnostic_codes::LIST_CANNOT_BE_EMPTY,
            );
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

        if !should_add {
            return None;
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

    /// Parse heritage type reference for interfaces (extends clause).
    /// Interfaces must reference types; literals or arbitrary expressions should produce diagnostics.
    pub(crate) fn parse_interface_heritage_type_reference(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        if self.is_token(SyntaxKind::OpenParenToken) {
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
        } else if self.is_token(SyntaxKind::ThisKeyword) {
            self.parse_this_expression()
        } else if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::NewKeyword)
        {
            self.parse_left_hand_side_expression()
        } else if matches!(
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
