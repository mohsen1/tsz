//! Parser state - type parsing, JSX, accessors, and `into_parts` methods

use super::state::ParserState;
use crate::parser::{NodeIndex, NodeList, node, syntax_kind_ext};
use tsz_common::interner::Atom;
use tsz_scanner::SyntaxKind;

impl ParserState {
    // =========================================================================
    // Parse Methods - Types (minimal implementation)
    // =========================================================================

    pub(crate) fn is_asserts_keyword(&self) -> bool {
        self.is_token(SyntaxKind::AssertsKeyword)
            || (self.is_token(SyntaxKind::Identifier)
                && self.scanner.get_token_value_ref() == "asserts")
    }

    pub(crate) fn is_asserts_type_predicate_start(&mut self) -> bool {
        if !self.is_asserts_keyword() {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let is_param = self.is_identifier_or_keyword() || self.is_token(SyntaxKind::ThisKeyword);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_param
    }

    pub(crate) fn consume_asserts_keyword(&mut self) {
        if self.is_asserts_keyword() {
            self.next_token();
        } else {
            self.parse_expected(SyntaxKind::AssertsKeyword);
        }
    }

    /// Parse a type (handles keywords, type references, unions, intersections, conditionals)
    pub(crate) fn parse_type(&mut self) -> NodeIndex {
        if self.is_asserts_type_predicate_start() {
            return self.parse_asserts_type_predicate();
        }

        // Allow type predicate parsing in type positions to avoid cascading errors.
        if self.is_identifier_or_keyword() || self.is_token(SyntaxKind::ThisKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;

            self.next_token();
            let is_predicate = self.is_token(SyntaxKind::IsKeyword);
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if is_predicate {
                let name = self.parse_type_predicate_parameter_name();
                let start_pos = if let Some(node) = self.arena.get(name) {
                    node.pos
                } else {
                    self.token_pos()
                };

                self.next_token(); // consume 'is'
                let type_node = self.parse_type();
                let end_pos = self.token_end();

                return self.arena.add_type_predicate(
                    syntax_kind_ext::TYPE_PREDICATE,
                    start_pos,
                    end_pos,
                    crate::parser::node::TypePredicateData {
                        asserts_modifier: false,
                        parameter_name: name,
                        type_node,
                    },
                );
            }
        }

        // Error recovery: if the token cannot start a type and we're at a boundary
        // (statement start, EOF, or type terminator like `)` `,` `=>`), emit TS1110.
        // Note: We must check can_token_start_type() because identifiers are both
        // statement starters AND valid type names (e.g., "let x: MyType = ...")
        if !self.can_token_start_type()
            && (self.is_statement_start()
                || self.is_token(SyntaxKind::EndOfFileToken)
                || self.is_type_terminator_token())
        {
            self.error_type_expected();
            return self.error_node();
        }

        self.parse_conditional_type()
    }

    /// Create an error node for recovery when type parsing fails
    pub(crate) fn error_node(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let end_pos = start_pos;
        self.arena
            .add_token(SyntaxKind::Identifier as u16, start_pos, end_pos)
    }

    /// Parse return type, which may be a type predicate (x is T) or a regular type
    pub(crate) fn parse_return_type(&mut self) -> NodeIndex {
        // Re-enable conditional types for return type parsing.
        // Return types are nested type contexts where conditional types should be allowed
        // even if disabled by an outer `infer T extends X` or conditional extends.
        let saved_flags = self.context_flags;
        self.context_flags &= !crate::parser::state::CONTEXT_FLAG_DISALLOW_CONDITIONAL_TYPES;

        let result = self.parse_return_type_inner();

        self.context_flags = saved_flags;
        result
    }

    fn parse_return_type_inner(&mut self) -> NodeIndex {
        if self.is_asserts_type_predicate_start() {
            return self.parse_asserts_type_predicate();
        }

        // Check if this is a type predicate: identifier 'is' Type
        // We need to look ahead to see if there's an identifier followed by 'is'
        if self.is_identifier_or_keyword() || self.is_token(SyntaxKind::ThisKeyword) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;

            self.next_token();
            let is_predicate = self.is_token(SyntaxKind::IsKeyword);
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if is_predicate {
                let name = self.parse_type_predicate_parameter_name();
                // This is a type predicate: x is T
                let start_pos = if let Some(node) = self.arena.get(name) {
                    node.pos
                } else {
                    self.token_pos()
                };

                self.next_token(); // consume 'is'
                let type_node = self.parse_type();
                let end_pos = self.token_end();

                return self.arena.add_type_predicate(
                    syntax_kind_ext::TYPE_PREDICATE,
                    start_pos,
                    end_pos,
                    crate::parser::node::TypePredicateData {
                        asserts_modifier: false,
                        parameter_name: name,
                        type_node,
                    },
                );
            }
        }

        self.parse_type()
    }

    pub(crate) fn parse_type_predicate_parameter_name(&mut self) -> NodeIndex {
        if self.is_token(SyntaxKind::ThisKeyword) {
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            self.next_token();
            return self
                .arena
                .add_token(SyntaxKind::ThisKeyword as u16, start_pos, end_pos);
        }

        self.parse_identifier_name()
    }

    /// Parse 'asserts' type predicate: asserts x or asserts x is T
    pub(crate) fn parse_asserts_type_predicate(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.consume_asserts_keyword();

        let parameter_name = self.parse_type_predicate_parameter_name();

        let type_node = if self.is_token(SyntaxKind::IsKeyword) {
            self.next_token();
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();

        self.arena.add_type_predicate(
            syntax_kind_ext::TYPE_PREDICATE,
            start_pos,
            end_pos,
            crate::parser::node::TypePredicateData {
                asserts_modifier: true,
                parameter_name,
                type_node,
            },
        )
    }

    /// Parse conditional type: T extends U ? X : Y
    pub(crate) fn parse_conditional_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the check type (left side of extends)
        let check_type = self.parse_union_type();

        // Check for extends keyword to form conditional type.
        // A line break before `extends` prevents conditional type parsing (ASI).
        // This matches tsc's behavior: `!scanner.hasPrecedingLineBreak()`.
        // Also, when DISALLOW_CONDITIONAL_TYPES is set (inside `infer T extends X` parsing),
        // don't parse as conditional type.
        if !self.is_token(SyntaxKind::ExtendsKeyword)
            || self.scanner.has_preceding_line_break()
            || (self.context_flags & crate::parser::state::CONTEXT_FLAG_DISALLOW_CONDITIONAL_TYPES)
                != 0
        {
            return check_type;
        }

        self.next_token(); // consume extends

        // Parse the extends type (right side of extends) with conditional types disabled.
        // This matches tsc's `disallowConditionalTypesAnd(parseType)` — nested conditional types
        // are not allowed in the extends position. This is critical for `infer T extends U`
        // disambiguation: `T extends infer U extends number ? 1 : 0` should parse the
        // infer constraint as `extends number` and the `?` belongs to the outer conditional.
        let saved_flags = self.context_flags;
        self.context_flags |= crate::parser::state::CONTEXT_FLAG_DISALLOW_CONDITIONAL_TYPES;
        let extends_type = self.parse_type();
        self.context_flags = saved_flags;

        // Expect ?
        self.parse_expected(SyntaxKind::QuestionToken);

        // Parse true type
        let true_type = self.parse_type();

        // Expect :
        self.parse_expected(SyntaxKind::ColonToken);

        // Parse false type
        let false_type = self.parse_type();

        let end_pos = self.token_end();

        self.arena.add_conditional_type(
            syntax_kind_ext::CONDITIONAL_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::ConditionalTypeData {
                check_type,
                extends_type,
                true_type,
                false_type,
            },
        )
    }

    /// Parse union type: A | B | C
    pub(crate) fn parse_union_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Handle optional leading | (e.g., type T = | A | B)
        let has_leading_bar = self.parse_optional(SyntaxKind::BarToken);

        // Parse first constituent
        let first = self.parse_intersection_type();

        // Check for | to form union
        if !has_leading_bar && !self.is_token(SyntaxKind::BarToken) {
            return first;
        }

        let mut types = vec![first];

        while self.parse_optional(SyntaxKind::BarToken) {
            types.push(self.parse_intersection_type());
        }

        let end_pos = self.token_end();
        self.arena.add_composite_type(
            syntax_kind_ext::UNION_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::CompositeTypeData {
                types: self.make_node_list(types),
            },
        )
    }

    /// Parse intersection type: A & B & C
    pub(crate) fn parse_intersection_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Handle optional leading & (e.g., type T = & A & B)
        let has_leading_amp = self.parse_optional(SyntaxKind::AmpersandToken);

        // Parse first constituent
        let first = self.parse_primary_type();

        // Check for & to form intersection
        if !has_leading_amp && !self.is_token(SyntaxKind::AmpersandToken) {
            return first;
        }

        let mut types = vec![first];

        while self.parse_optional(SyntaxKind::AmpersandToken) {
            types.push(self.parse_primary_type());
        }

        let end_pos = self.token_end();
        self.arena.add_composite_type(
            syntax_kind_ext::INTERSECTION_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::CompositeTypeData {
                types: self.make_node_list(types),
            },
        )
    }

    /// Parse primary type (keywords, references, parenthesized, tuples, arrays, function types)
    pub(crate) fn parse_primary_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // If we encounter a token that can't start a type, emit TS1110 (Type expected).
        // However, suppress the error for delimiter/terminator tokens that indicate a
        // *missing* type rather than an *incorrect* token used as a type. TSC silently
        // creates a missing node for these cases (e.g., `(a: ) =>`, `x: ;`).
        if !self.can_token_start_type() {
            if !self.is_type_terminator_token() {
                self.error_type_expected();
            }
            // Return a synthetic identifier node to allow parsing to continue
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                start_pos,
                self.token_pos(),
                crate::parser::node::IdentifierData {
                    atom: Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            );
        }

        let base_type = if self.should_parse_abstract_constructor_type() {
            self.next_token();
            self.parse_constructor_type(true)
        } else if self.is_token(SyntaxKind::NewKeyword) {
            self.parse_constructor_type(false)
        } else if self.is_token(SyntaxKind::LessThanToken) {
            self.parse_generic_function_type()
        } else {
            self.parse_primary_type_base(start_pos)
        };

        self.parse_primary_type_array_suffix(start_pos, base_type)
    }

    fn should_parse_abstract_constructor_type(&mut self) -> bool {
        if !self.is_token(SyntaxKind::AbstractKeyword) {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let is_abstract_new = self.is_token(SyntaxKind::NewKeyword);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_abstract_new
    }

    fn parse_primary_type_base(&mut self, start_pos: u32) -> NodeIndex {
        if self.is_token(SyntaxKind::OpenParenToken) {
            return self.parse_parenthesized_type_or_function_type();
        }

        if self.is_token(SyntaxKind::OpenBracketToken) {
            return self.parse_tuple_type();
        }

        if self.is_token(SyntaxKind::OpenBraceToken) {
            return self.parse_object_or_mapped_type();
        }

        if self.is_token(SyntaxKind::TypeOfKeyword) {
            return self.parse_typeof_type();
        }

        if self.is_token(SyntaxKind::KeyOfKeyword) {
            return self.parse_keyof_type();
        }

        if self.is_token(SyntaxKind::UniqueKeyword) {
            return self.parse_unique_type();
        }

        if self.is_token(SyntaxKind::ReadonlyKeyword) {
            return self.parse_readonly_type();
        }

        if self.is_token(SyntaxKind::InferKeyword) {
            return self.parse_infer_type();
        }

        if self.is_token(SyntaxKind::ThisKeyword) {
            let this_start = self.token_pos();
            let this_end = self.token_end();
            self.next_token();
            return self
                .arena
                .add_token(syntax_kind_ext::THIS_TYPE, this_start, this_end);
        }

        if self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_token(SyntaxKind::BigIntLiteral)
            || self.is_token(SyntaxKind::TrueKeyword)
            || self.is_token(SyntaxKind::FalseKeyword)
        {
            return self.parse_literal_type();
        }

        if self.is_token(SyntaxKind::MinusToken) {
            return self.parse_prefix_unary_literal_type();
        }

        if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
            || self.is_token(SyntaxKind::TemplateHead)
        {
            return self.parse_template_literal_type();
        }

        if self.is_token(SyntaxKind::ImportKeyword) {
            return self.parse_import_type();
        }

        let first_name = self.parse_type_identifier_or_keyword();
        let type_name = self.parse_qualified_name_rest(first_name);
        // Only parse type arguments if `<` is on the same line (no preceding line break).
        // A line break before `<` means it's a new construct (e.g., a call signature
        // in a type literal), not type arguments for this type reference.
        // This matches tsc's `!scanner.hasPrecedingLineBreak()` check.
        let type_arguments = (self.is_less_than_or_compound()
            && !self.scanner.has_preceding_line_break())
        .then(|| self.parse_type_arguments());

        self.arena.add_type_ref(
            syntax_kind_ext::TYPE_REFERENCE,
            start_pos,
            self.token_end(),
            crate::parser::node::TypeRefData {
                type_name,
                type_arguments,
            },
        )
    }

    fn parse_primary_type_array_suffix(
        &mut self,
        start_pos: u32,
        base_type: NodeIndex,
    ) -> NodeIndex {
        if self.is_token(SyntaxKind::OpenBracketToken) && !self.scanner.has_preceding_line_break() {
            self.parse_array_type(start_pos, base_type)
        } else {
            base_type
        }
    }

    fn parse_parenthesized_type_or_function_type(&mut self) -> NodeIndex {
        if self.look_ahead_is_function_type() {
            return self.parse_function_type();
        }

        self.next_token();
        // Re-enable conditional types inside parentheses, even if they were disabled
        // for an outer `infer T extends X` disambiguation. Matches tsc's
        // `allowConditionalTypesAnd(parseType)` in `parseParenthesizedType`.
        let saved_flags = self.context_flags;
        self.context_flags &= !crate::parser::state::CONTEXT_FLAG_DISALLOW_CONDITIONAL_TYPES;
        let inner = self.parse_type();
        self.context_flags = saved_flags;
        self.parse_expected(SyntaxKind::CloseParenToken);
        inner
    }

    fn parse_type_identifier_or_keyword(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::StringKeyword
            | SyntaxKind::NumberKeyword
            | SyntaxKind::BooleanKeyword
            | SyntaxKind::SymbolKeyword
            | SyntaxKind::BigIntKeyword
            | SyntaxKind::VoidKeyword
            | SyntaxKind::NullKeyword
            | SyntaxKind::UndefinedKeyword
            | SyntaxKind::NeverKeyword
            | SyntaxKind::AnyKeyword
            | SyntaxKind::UnknownKeyword
            | SyntaxKind::ObjectKeyword
            | SyntaxKind::AwaitKeyword
            | SyntaxKind::YieldKeyword
            | SyntaxKind::AssertsKeyword => self.parse_keyword_as_identifier(),
            SyntaxKind::PrivateIdentifier => self.parse_private_identifier(),
            _ => self.parse_identifier(),
        }
    }

    /// Parse a single element in a tuple type, handling:
    /// - Rest elements: ...T[]
    /// - Optional elements: T?
    /// - Named elements: name: T or name?: T
    pub(crate) fn parse_tuple_element_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Handle rest element: ...T[] or labeled rest: ...name: T
        if self.is_token(SyntaxKind::DotDotDotToken) {
            // Look ahead: if ...identifier: or ...identifier?: then it's a labeled rest element
            let snapshot = self.scanner.save_state();
            let saved_token = self.current_token;
            self.next_token(); // consume ...

            if self.is_token(SyntaxKind::Identifier) {
                self.next_token(); // consume identifier
                let has_question = self.parse_optional(SyntaxKind::QuestionToken);
                let has_colon = self.is_token(SyntaxKind::ColonToken);
                if has_colon || has_question {
                    // Labeled rest element: ...name: T - delegate to named tuple member
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;
                    return self.parse_named_tuple_member();
                }
            }

            // Not a labeled rest - restore and parse as regular rest type
            self.scanner.restore_state(snapshot);
            self.current_token = saved_token;

            self.next_token(); // consume ...
            let element_type = self.parse_type();
            let end_pos = self.token_end();
            return self.arena.add_wrapped_type(
                syntax_kind_ext::REST_TYPE,
                start_pos,
                end_pos,
                crate::parser::node::WrappedTypeData {
                    type_node: element_type,
                },
            );
        }

        // Check if this is a named tuple element: name: T or name?: T
        // Need to look ahead to see if there's a colon after the identifier
        if self.is_token(SyntaxKind::Identifier) {
            let snapshot = self.scanner.save_state();
            let current = self.current_token;

            let _name = self.scanner.get_token_value_ref().to_string();
            self.next_token();

            // Check for optional marker and colon
            let _has_question = self.parse_optional(SyntaxKind::QuestionToken);
            let has_colon = self.is_token(SyntaxKind::ColonToken);

            // Only treat as named tuple member if there's a colon after the identifier
            // (with or without the optional marker: name: T or name?: T)
            // A standalone identifier with ? but no colon is just an optional type: T?
            if has_colon {
                // This is a named tuple element - parse it
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                return self.parse_named_tuple_member();
            }

            // Not a named element, restore and parse as regular type
            self.scanner.restore_state(snapshot);
            self.current_token = current;
        }

        // Parse the type
        let type_node = self.parse_type();

        // Check for optional marker: T?
        if self.parse_optional(SyntaxKind::QuestionToken) {
            let end_pos = self.token_end();
            return self.arena.add_wrapped_type(
                syntax_kind_ext::OPTIONAL_TYPE,
                start_pos,
                end_pos,
                crate::parser::node::WrappedTypeData { type_node },
            );
        }

        type_node
    }

    /// Parse a named tuple member: name: T or name?: T
    pub(crate) fn parse_named_tuple_member(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Check for ... prefix (rest parameter)
        let dot_dot_dot_token = self.parse_optional(SyntaxKind::DotDotDotToken);

        // Parse name
        let name = self.parse_identifier();

        // Check for optional marker
        let question_token = self.parse_optional(SyntaxKind::QuestionToken);

        // Parse : and type
        self.parse_expected(SyntaxKind::ColonToken);
        let type_node = self.parse_type();

        let end_pos = self.token_end();

        // Create a named tuple member node
        self.arena.add_named_tuple_member(
            syntax_kind_ext::NAMED_TUPLE_MEMBER,
            start_pos,
            end_pos,
            crate::parser::node::NamedTupleMemberData {
                dot_dot_dot_token,
                name,
                question_token,
                type_node,
            },
        )
    }

    /// Parse tuple type: [T, U, V], [name: T], [...T[]], [T?]
    pub(crate) fn parse_tuple_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBracketToken);

        let mut elements = Vec::new();

        while !self.is_token(SyntaxKind::CloseBracketToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let element = self.parse_tuple_element_type();
            elements.push(element);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        self.parse_expected(SyntaxKind::CloseBracketToken);
        let end_pos = self.token_end();

        let tuple = self.arena.add_tuple_type(
            syntax_kind_ext::TUPLE_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::TupleTypeData {
                elements: self.make_node_list(elements),
            },
        );

        // Handle array of tuples: [T, U][]
        if self.is_token(SyntaxKind::OpenBracketToken) {
            return self.parse_array_type(start_pos, tuple);
        }

        tuple
    }

    /// Parse literal type: "foo", 42, 123n, true, false
    pub(crate) fn parse_literal_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the literal expression
        let literal = match self.token() {
            SyntaxKind::StringLiteral => self.parse_string_literal(),
            SyntaxKind::NumericLiteral => self.parse_numeric_literal(),
            SyntaxKind::BigIntLiteral => self.parse_bigint_literal(),
            SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword => self.parse_boolean_literal(),
            _ => {
                // Fallback - shouldn't happen
                self.parse_identifier()
            }
        };

        let end_pos = self.token_end();

        self.arena.add_literal_type(
            syntax_kind_ext::LITERAL_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::LiteralTypeData { literal },
        )
    }

    /// Parse prefix unary literal type: -1, -42
    /// In TypeScript, negative number literals in type position are
    /// represented as a `PrefixUnaryExpression` wrapped in a `LiteralType`
    pub(crate) fn parse_prefix_unary_literal_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the minus token
        let operator_kind = self.token() as u16;
        self.next_token();

        // Parse the numeric or bigint literal operand
        let operand = if self.is_token(SyntaxKind::BigIntLiteral) {
            self.parse_bigint_literal()
        } else {
            self.parse_numeric_literal()
        };

        let prefix_end = self.token_end();

        // Create prefix unary expression node
        let prefix_expr = self.arena.add_unary_expr(
            syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
            start_pos,
            prefix_end,
            crate::parser::node::UnaryExprData {
                operator: operator_kind,
                operand,
            },
        );

        // Wrap in a literal type
        self.arena.add_literal_type(
            syntax_kind_ext::LITERAL_TYPE,
            start_pos,
            prefix_end,
            crate::parser::node::LiteralTypeData {
                literal: prefix_expr,
            },
        )
    }

    /// Parse typeof type: typeof x, typeof x.y, typeof import("...").A.B
    pub(crate) fn parse_typeof_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::TypeOfKeyword);

        // Parse the expression name (can be qualified: x.y.z or typeof import("..."))
        let mut expr_name = if self.is_token(SyntaxKind::ImportKeyword) {
            self.parse_import_expression()
        } else {
            self.parse_entity_name()
        };

        // Parse member access after import(): typeof import("./a").A.foo
        // This handles cases like: typeof import("module").Class.staticMember
        while self.is_token(SyntaxKind::DotToken) {
            self.next_token();
            let right = self.parse_identifier_name(); // Use identifier_name to allow keywords as property names
            let node_start_pos = if let Some(node) = self.arena.get(expr_name) {
                node.pos
            } else {
                start_pos
            };
            let end_pos = self.token_end();

            expr_name = self.arena.add_qualified_name(
                syntax_kind_ext::QUALIFIED_NAME,
                node_start_pos,
                end_pos,
                crate::parser::node::QualifiedNameData {
                    left: expr_name,
                    right,
                },
            );
        }

        // Parse optional type arguments for instantiation expressions: typeof Err<U>
        // but only when `<` appears on the same line; a line break before `<`
        // indicates a subsequent declaration/signature, not type arguments.
        let type_arguments = (self.is_less_than_or_compound()
            && !self.scanner.has_preceding_line_break())
        .then(|| self.parse_type_arguments());

        let end_pos = self.token_end();

        self.arena.add_type_query(
            syntax_kind_ext::TYPE_QUERY,
            start_pos,
            end_pos,
            crate::parser::node::TypeQueryData {
                expr_name,
                type_arguments,
            },
        )
    }

    /// Parse import type: import("./module") or import("./module").Type
    pub(crate) fn parse_import_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the import call: import("./module")
        let argument = self.parse_import_expression();

        // Check that the argument is a string literal (TS1141)
        if let Some(call_node) = self.arena.get(argument)
            && call_node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call_data) = self.arena.get_call_expr(call_node)
            && let Some(args) = &call_data.arguments
            && let Some(&first_arg) = args.nodes.first()
            && let Some(arg_node) = self.arena.get(first_arg)
            && arg_node.kind != SyntaxKind::StringLiteral as u16
        {
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at(
                arg_node.pos,
                arg_node.end.saturating_sub(arg_node.pos),
                diagnostic_messages::STRING_LITERAL_EXPECTED,
                diagnostic_codes::STRING_LITERAL_EXPECTED,
            );
        }

        // Parse member access after import: import("./a").Type.SubType
        let mut qualifier = argument;
        while self.is_token(SyntaxKind::DotToken) {
            self.next_token();
            let right = self.parse_identifier_name();
            let node_start_pos = if let Some(node) = self.arena.get(qualifier) {
                node.pos
            } else {
                start_pos
            };
            let end_pos = self.token_end();

            qualifier = self.arena.add_qualified_name(
                syntax_kind_ext::QUALIFIED_NAME,
                node_start_pos,
                end_pos,
                crate::parser::node::QualifiedNameData {
                    left: qualifier,
                    right,
                },
            );
        }

        // Parse optional type arguments: import("./a").Type<T>, but only when `<`
        // appears on the same line.
        let type_arguments = (self.is_less_than_or_compound()
            && !self.scanner.has_preceding_line_break())
        .then(|| self.parse_type_arguments());

        let end_pos = self.token_end();

        // Return as a type reference with the import expression as the type name
        self.arena.add_type_ref(
            syntax_kind_ext::TYPE_REFERENCE,
            start_pos,
            end_pos,
            crate::parser::node::TypeRefData {
                type_name: qualifier,
                type_arguments,
            },
        )
    }

    /// Parse keyof type: keyof T
    pub(crate) fn parse_keyof_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let operator = self.token() as u16;
        self.parse_expected(SyntaxKind::KeyOfKeyword);

        // Parse the type operand
        let type_node = self.parse_primary_type();

        let end_pos = self.token_end();

        self.arena.add_type_operator(
            syntax_kind_ext::TYPE_OPERATOR,
            start_pos,
            end_pos,
            crate::parser::node::TypeOperatorData {
                operator,
                type_node,
            },
        )
    }

    /// Parse unique type: unique symbol
    pub(crate) fn parse_unique_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let operator = self.token() as u16;
        self.parse_expected(SyntaxKind::UniqueKeyword);

        // Parse the type operand (unique symbol)
        let type_node = self.parse_primary_type();

        let end_pos = self.token_end();

        self.arena.add_type_operator(
            syntax_kind_ext::TYPE_OPERATOR,
            start_pos,
            end_pos,
            crate::parser::node::TypeOperatorData {
                operator,
                type_node,
            },
        )
    }

    /// Parse readonly type: readonly T[]
    pub(crate) fn parse_readonly_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let operator = self.token() as u16;
        self.parse_expected(SyntaxKind::ReadonlyKeyword);

        // Parse the type operand
        let type_node = self.parse_primary_type();

        let end_pos = self.token_end();

        self.arena.add_type_operator(
            syntax_kind_ext::TYPE_OPERATOR,
            start_pos,
            end_pos,
            crate::parser::node::TypeOperatorData {
                operator,
                type_node,
            },
        )
    }

    /// Parse infer type: infer T (used in conditional types)
    ///
    /// Handles the `infer T extends U` disambiguation:
    /// - `infer U extends number ? 1 : 0` → parsed as conditional (U has no constraint)
    /// - `infer U extends number` → parsed as infer with constraint
    ///
    /// Uses speculative lookahead: parse `extends Type` with conditional types disabled,
    /// then check if `?` follows. If so, the extends belongs to the outer conditional type,
    /// not to the infer constraint.
    pub(crate) fn parse_infer_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::InferKeyword);

        // Parse the type parameter with speculative infer-extends handling
        let type_parameter = self.parse_type_parameter_of_infer_type();

        let end_pos = self.token_end();

        self.arena.add_infer_type(
            syntax_kind_ext::INFER_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::InferTypeData { type_parameter },
        )
    }

    /// Parse a type parameter specifically for `infer` types.
    /// Handles the `infer T extends U ?` disambiguation by using speculative parsing.
    fn parse_type_parameter_of_infer_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the type parameter name (no modifiers for infer type params)
        let name = self.parse_identifier();

        // Try to parse constraint with speculative lookahead.
        // Save state before consuming `extends`, so we can backtrack if
        // the `extends` actually belongs to an outer conditional type.
        let constraint = if self.is_token(SyntaxKind::ExtendsKeyword) {
            let already_disallowed = (self.context_flags
                & crate::parser::state::CONTEXT_FLAG_DISALLOW_CONDITIONAL_TYPES)
                != 0;

            // Save full parser state for backtracking
            let snapshot = self.scanner.save_state();
            let saved_token = self.current_token;
            let arena_len = self.arena.nodes.len();
            let diag_len = self.parse_diagnostics.len();

            self.next_token(); // consume `extends`

            // Parse the constraint type with conditional types disallowed.
            // This prevents `number ? 1 : 0` from being parsed as a conditional type
            // within the constraint itself.
            let saved_flags = self.context_flags;
            self.context_flags |= crate::parser::state::CONTEXT_FLAG_DISALLOW_CONDITIONAL_TYPES;
            let constraint_type = self.parse_type();
            self.context_flags = saved_flags;

            // Now check: if `?` follows and we're not already in a no-conditional context,
            // then this `extends` belongs to an outer conditional type, not the infer constraint.
            // Backtrack in that case.
            if !already_disallowed && self.is_token(SyntaxKind::QuestionToken) {
                // Backtrack: restore scanner, token, arena, and diagnostics
                self.scanner.restore_state(snapshot);
                self.current_token = saved_token;
                self.arena.nodes.truncate(arena_len);
                self.parse_diagnostics.truncate(diag_len);
                NodeIndex::NONE
            } else {
                constraint_type
            }
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();

        self.arena.add_type_parameter(
            crate::parser::syntax_kind_ext::TYPE_PARAMETER,
            start_pos,
            end_pos,
            crate::parser::node::TypeParameterData {
                modifiers: None,
                name,
                constraint,
                default: NodeIndex::NONE,
            },
        )
    }

    /// Parse template literal type: `hello` or `prefix${T}suffix`
    pub(crate) fn parse_template_literal_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the head (either NoSubstitutionTemplateLiteral or TemplateHead)
        if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral) {
            // Simple template literal type with no substitutions: `hello`
            let head = self.parse_template_literal_head();
            let end_pos = self.token_end();

            return self.arena.add_template_literal_type(
                syntax_kind_ext::TEMPLATE_LITERAL_TYPE,
                start_pos,
                end_pos,
                crate::parser::node::TemplateLiteralTypeData {
                    head,
                    template_spans: self.make_node_list(vec![]),
                },
            );
        }

        // Template with substitutions: `prefix${T}middle${U}suffix`
        let head = self.parse_template_literal_head();
        let mut spans = Vec::new();

        // After the head, we need to parse: type, then middle/tail, repeat until tail
        loop {
            // Parse the type inside ${...}
            let type_node = self.parse_type();

            // Now we need to rescan for the template continuation
            // The scanner needs to be told to rescan as template
            self.scanner.re_scan_template_token(false);
            self.current_token = self.scanner.get_token();

            let span_start = self.token_pos();
            let is_tail = self.is_token(SyntaxKind::TemplateTail);

            // Parse the template middle/tail literal
            let literal = self.parse_template_literal_span();
            let span_end = self.token_end();

            // Create a template span node
            // Note: We reuse TemplateSpanData, using 'expression' field for the type node
            let span = self.arena.add_template_span(
                syntax_kind_ext::TEMPLATE_LITERAL_TYPE_SPAN,
                span_start,
                span_end,
                crate::parser::node::TemplateSpanData {
                    expression: type_node,
                    literal,
                },
            );
            spans.push(span);

            if is_tail {
                break;
            }
        }

        let end_pos = self.token_end();

        self.arena.add_template_literal_type(
            syntax_kind_ext::TEMPLATE_LITERAL_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::TemplateLiteralTypeData {
                head,
                template_spans: self.make_node_list(spans),
            },
        )
    }

    /// Parse template literal head (`NoSubstitutionTemplateLiteral` or `TemplateHead`)
    pub(crate) fn parse_template_literal_head(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let is_unterminated = self.scanner.is_unterminated();
        let kind = self.token() as u16;
        let text = self.scanner.get_token_value_ref().to_string();
        let literal_end = self.token_end();
        self.report_invalid_string_or_template_escape_errors();
        self.next_token();
        let end_pos = self.token_end();
        if is_unterminated {
            self.error_unterminated_template_literal_at(start_pos, literal_end);
        }
        self.arena.add_literal(
            kind,
            start_pos,
            end_pos,
            node::LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    /// Parse template literal span (`TemplateMiddle` or `TemplateTail`)
    pub(crate) fn parse_template_literal_span(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let is_unterminated = self.scanner.is_unterminated();
        let kind = self.token() as u16;
        let text = self.scanner.get_token_value_ref().to_string();
        let literal_end = self.token_end();
        self.report_invalid_string_or_template_escape_errors();
        self.next_token();
        let end_pos = self.token_end();
        if is_unterminated {
            self.error_unterminated_template_literal_at(start_pos, literal_end);
        }
        self.arena.add_literal(
            kind,
            start_pos,
            end_pos,
            node::LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    /// Parse object type literal or mapped type
    /// Object type: { prop: T; `method()`: U }
    /// Mapped type: { [K in keyof T]: U } or { readonly [K in T]?: U }
    /// Index signature: { [key: string]: T }
    pub(crate) fn parse_object_or_mapped_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        // Check if this is a mapped type: [ followed by identifier and 'in'
        // vs index signature: [ followed by identifier and ':'
        if self.is_token(SyntaxKind::OpenBracketToken) {
            if self.look_ahead_is_mapped_type_start() {
                return self.parse_mapped_type_rest(start_pos);
            }
            // Not a mapped type - let type literal parsing handle index signature
            return self.parse_type_literal_rest(start_pos);
        }

        // Check for readonly/+/- prefixed mapped type
        if (self.is_token(SyntaxKind::ReadonlyKeyword) && self.look_ahead_is_mapped_type())
            || (self.is_token(SyntaxKind::PlusToken) || self.is_token(SyntaxKind::MinusToken))
        {
            return self.parse_mapped_type_rest(start_pos);
        }

        // Otherwise it's an object type literal - parse as type literal
        self.parse_type_literal_rest(start_pos)
    }

    /// Look ahead to see if [ starts a mapped type (has 'in' keyword) vs index signature (has ':')
    pub(crate) fn look_ahead_is_mapped_type_start(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip [

        // Skip identifier
        if self.is_token(SyntaxKind::Identifier) {
            self.next_token();
        }

        // Check if followed by 'in' (mapped type) or ':' (index signature)
        let is_mapped = self.is_token(SyntaxKind::InKeyword);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_mapped
    }

    /// Look ahead to check if readonly is followed by [ (mapped type) vs property
    pub(crate) fn look_ahead_is_mapped_type(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip readonly/+/-
        // After readonly, check for `[identifier in` pattern (mapped type)
        // vs `[identifier :` pattern (index signature)
        let is_mapped = if self.is_token(SyntaxKind::OpenBracketToken) {
            self.next_token(); // skip [
            if self.is_token(SyntaxKind::Identifier) {
                self.next_token(); // skip identifier
            }
            self.is_token(SyntaxKind::InKeyword)
        } else {
            false
        };

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_mapped
    }

    /// Parse mapped type after opening brace: { [K in T]: U }
    pub(crate) fn parse_mapped_type_rest(&mut self, start_pos: u32) -> NodeIndex {
        // Parse optional readonly modifier with +/- prefix
        let readonly_token = if self.is_token(SyntaxKind::ReadonlyKeyword) {
            let pos = self.token_pos();
            self.next_token();
            self.arena
                .add_token(SyntaxKind::ReadonlyKeyword as u16, pos, self.token_end())
        } else if self.is_token(SyntaxKind::PlusToken) || self.is_token(SyntaxKind::MinusToken) {
            let pos = self.token_pos();
            let kind = self.token() as u16;
            self.next_token();
            if self.is_token(SyntaxKind::ReadonlyKeyword) {
                self.next_token();
            }
            self.arena.add_token(kind, pos, self.token_end())
        } else {
            NodeIndex::NONE
        };

        // Parse [K in T]
        self.parse_expected(SyntaxKind::OpenBracketToken);

        // Parse the type parameter: K in T
        let type_param_start = self.token_pos();
        let param_name = self.parse_identifier();

        self.parse_expected(SyntaxKind::InKeyword);

        // Re-enable conditional types inside mapped type bracket.
        // The outer `T extends { [P in ...] }` may have disabled them,
        // but inside the mapped type we need them again.
        let saved_flags = self.context_flags;
        self.context_flags &= !crate::parser::state::CONTEXT_FLAG_DISALLOW_CONDITIONAL_TYPES;
        let constraint = self.parse_type();

        // Parse optional 'as' clause for key remapping: [K in T as NewKey]
        let name_type = if self.parse_optional(SyntaxKind::AsKeyword) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };
        self.context_flags = saved_flags;

        let type_param_end = self.token_end();

        let type_parameter = self.arena.add_type_parameter(
            syntax_kind_ext::TYPE_PARAMETER,
            type_param_start,
            type_param_end,
            crate::parser::node::TypeParameterData {
                modifiers: None,
                name: param_name,
                constraint,
                default: NodeIndex::NONE,
            },
        );

        self.parse_expected(SyntaxKind::CloseBracketToken);

        // Parse optional ? modifier with +/- prefix
        let question_token = if self.is_token(SyntaxKind::QuestionToken) {
            let pos = self.token_pos();
            self.next_token();
            self.arena
                .add_token(SyntaxKind::QuestionToken as u16, pos, self.token_end())
        } else if self.is_token(SyntaxKind::PlusToken) || self.is_token(SyntaxKind::MinusToken) {
            let pos = self.token_pos();
            let kind = self.token() as u16;
            self.next_token();
            if self.is_token(SyntaxKind::QuestionToken) {
                self.next_token();
            }
            self.arena.add_token(kind, pos, self.token_end())
        } else {
            NodeIndex::NONE
        };

        // Parse optional : and type (type can be omitted for implicit any)
        let type_node = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        // Parse optional semicolon
        self.parse_optional(SyntaxKind::SemicolonToken);

        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        self.arena.add_mapped_type(
            syntax_kind_ext::MAPPED_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::MappedTypeData {
                readonly_token,
                type_parameter,
                name_type,
                question_token,
                type_node,
                members: None,
            },
        )
    }

    /// Parse type literal (object type) after opening brace
    pub(crate) fn parse_type_literal_rest(&mut self, start_pos: u32) -> NodeIndex {
        let mut members = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let saved_pos = self.token_pos();
            let member = self.parse_type_member(false);

            // If parse_type_member returned NONE (couldn't parse) and we haven't advanced,
            // skip the current token to prevent infinite loops
            if member.is_none() && self.token_pos() == saved_pos {
                self.error_unexpected_token();
                self.next_token(); // Skip the problematic token
                continue;
            }

            if member.is_some() {
                members.push(member);
            }

            self.parse_type_member_separator_with_asi();
        }

        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        self.arena.add_type_literal(
            syntax_kind_ext::TYPE_LITERAL,
            start_pos,
            end_pos,
            crate::parser::node::TypeLiteralData {
                members: self.make_node_list(members),
            },
        )
    }

    /// Check if the current token starts with `>` (includes compound tokens like `>>`, `>>>`, `>=`, etc.)
    pub(crate) const fn is_greater_than_or_compound(&self) -> bool {
        matches!(
            self.current_token,
            SyntaxKind::GreaterThanToken
                | SyntaxKind::GreaterThanGreaterThanToken
                | SyntaxKind::GreaterThanGreaterThanGreaterThanToken
                | SyntaxKind::GreaterThanEqualsToken
                | SyntaxKind::GreaterThanGreaterThanEqualsToken
                | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
        )
    }

    /// Parse type arguments: <T, U, V>
    /// Handles compound `<<` by splitting into two `<` tokens.
    pub(crate) fn parse_type_arguments(&mut self) -> NodeList {
        self.parse_expected_less_than();

        let mut args = Vec::new();

        // Check for empty type argument list: <>
        // TypeScript reports TS1099: "Type argument list cannot be empty"
        if self.is_greater_than_or_compound() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Type argument list cannot be empty.",
                diagnostic_codes::TYPE_ARGUMENT_LIST_CANNOT_BE_EMPTY,
            );
        } else {
            while !self.is_greater_than_or_compound() && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                args.push(self.parse_type());

                if !self.parse_optional(SyntaxKind::CommaToken) {
                    break;
                }
            }
        }

        self.parse_expected_greater_than();
        self.make_node_list(args)
    }

    /// Try to parse type arguments for a call expression: foo<T>()
    /// Returns Some(NodeList) if successful, None if this is not type arguments.
    /// Uses look-ahead to distinguish from comparison operators.
    pub(crate) fn try_parse_type_arguments_for_call(&mut self) -> Option<NodeList> {
        // Save state for potential rollback
        let snapshot = self.scanner.save_state();
        let saved_token = self.current_token;
        let saved_arena_len = self.arena.nodes.len();
        let saved_diagnostics_len = self.parse_diagnostics.len();

        // Consume `<` (handles `<<` by splitting into two `<` tokens)
        self.parse_expected_less_than();

        // Check for empty type argument list: <>
        // TypeScript reports TS1099: "Type argument list cannot be empty"
        if self.is_greater_than_or_compound() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Type argument list cannot be empty.",
                diagnostic_codes::TYPE_ARGUMENT_LIST_CANNOT_BE_EMPTY,
            );
            self.parse_expected_greater_than();

            // Check if followed by ( to confirm this is a call
            if !self.is_token(SyntaxKind::OpenParenToken) {
                // Not a call - rollback
                self.scanner.restore_state(snapshot);
                self.current_token = saved_token;
                self.arena.nodes.truncate(saved_arena_len);
                self.parse_diagnostics.truncate(saved_diagnostics_len);
                return None;
            }
            return Some(self.make_node_list(Vec::new()));
        }

        let mut args = Vec::new();
        let mut depth = 1;

        // Parse type arguments
        while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
            // Try to parse a type
            if args.is_empty() || self.is_token(SyntaxKind::CommaToken) {
                if !args.is_empty() {
                    self.next_token(); // consume comma
                }

                // Check for nested < (generic types within type arguments)
                let type_node = self.parse_type();
                args.push(type_node);
            }

            if self.is_greater_than_or_compound() {
                depth -= 1;
            } else if self.is_token(SyntaxKind::CommaToken) {
                // Comma indicates another type argument follows.
            } else if self.is_token(SyntaxKind::SemicolonToken)
                || self.is_token(SyntaxKind::CloseBraceToken)
                || self.is_token(SyntaxKind::EndOfFileToken)
            {
                // Invalid - not type arguments
                break;
            } else {
                // Something unexpected - might not be type arguments
                break;
            }
        }

        if depth == 0 {
            // Successfully parsed type arguments, now consume >
            self.parse_expected_greater_than();

            // Check if the following token indicates these were type arguments
            // (call, tagged template, or instantiation expression)
            if self.can_follow_type_arguments_in_expression() {
                return Some(self.make_node_list(args));
            }
        }

        // Not type arguments - restore state
        self.scanner.restore_state(snapshot);
        self.current_token = saved_token;
        // Truncate arena to remove any nodes we added
        self.arena.nodes.truncate(saved_arena_len);
        // Drop any speculative diagnostics from the failed parse
        self.parse_diagnostics.truncate(saved_diagnostics_len);
        None
    }

    /// Check if the token following `>` can follow type arguments in an expression.
    /// Implements tsc's `canFollowTypeArgumentsInExpression()`.
    ///
    /// Returns true for:
    /// - `(` — call expression: `f<T>(args)`
    /// - template literal — tagged template: "f<T>\`...\`"
    /// - line break — instantiation expression: `f<T>\n`
    /// - binary operator — instantiation expression: `f<T> || fallback`
    /// - non-expression-starter — instantiation expression: `f<T>; f<T>}`
    ///
    /// Returns false for:
    /// - `<` — ambiguous: `f<T><U>` → treat as relational
    /// - `>` — ambiguous: `f<T>>` → treat as relational
    /// - `+`/`-` — unary: `f < T > +1` → treat as relational chain
    fn can_follow_type_arguments_in_expression(&self) -> bool {
        match self.token() {
            // These always indicate type arguments (call or tagged template)
            SyntaxKind::OpenParenToken
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TemplateHead => true,

            // These never follow type arguments (ambiguous with relational)
            SyntaxKind::LessThanToken
            | SyntaxKind::GreaterThanToken
            | SyntaxKind::PlusToken
            | SyntaxKind::MinusToken => false,

            // Everything else: favor type arguments when followed by
            // a line break, binary operator, or non-expression-starter
            _ => {
                self.scanner.has_preceding_line_break()
                    || self.is_binary_operator()
                    || !self.is_expression_start()
            }
        }
    }

    /// Parse array type suffix (T[]) or indexed access type (T[K])
    pub(crate) fn parse_array_type(
        &mut self,
        start_pos: u32,
        element_type: NodeIndex,
    ) -> NodeIndex {
        let mut current = element_type;

        while self.is_token(SyntaxKind::OpenBracketToken) {
            if self.look_ahead_is_index_signature() {
                break;
            }
            self.next_token();

            // Check if this is array type [] or indexed access type [K]
            if self.is_token(SyntaxKind::CloseBracketToken) {
                // Array type: T[]
                self.next_token();
                let end_pos = self.token_end();

                current = self.arena.add_array_type(
                    syntax_kind_ext::ARRAY_TYPE,
                    start_pos,
                    end_pos,
                    crate::parser::node::ArrayTypeData {
                        element_type: current,
                    },
                );
            } else {
                // Indexed access type: T[K]
                let index_type = self.parse_type();
                self.parse_expected(SyntaxKind::CloseBracketToken);
                let end_pos = self.token_end();

                current = self.arena.add_indexed_access_type(
                    syntax_kind_ext::INDEXED_ACCESS_TYPE,
                    start_pos,
                    end_pos,
                    crate::parser::node::IndexedAccessTypeData {
                        object_type: current,
                        index_type,
                    },
                );
            }
        }

        current
    }

    /// Check if current keyword can be used as a property name
    /// (when followed by :, ?, (, <, or at end of type member)
    pub(crate) fn look_ahead_is_property_name_after_keyword(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip the keyword
        self.next_token();

        // If followed by these, the keyword is being used as a property name
        let is_property_name = self.is_token(SyntaxKind::ColonToken)
            || self.is_token(SyntaxKind::QuestionToken)
            || self.is_token(SyntaxKind::OpenParenToken)
            || self.is_token(SyntaxKind::LessThanToken)
            || self.is_token(SyntaxKind::SemicolonToken)
            || self.is_token(SyntaxKind::CommaToken)
            || self.is_token(SyntaxKind::CloseBraceToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_property_name
    }

    /// Check if there is a line break between the current keyword and the next token.
    /// Used to detect ASI in type member contexts where `protected\n p` means two properties.
    pub(crate) fn look_ahead_has_line_break_after_keyword(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token();
        let has_line_break = self.scanner.has_preceding_line_break();

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        has_line_break
    }

    // Function types, type assertions, JSX → state_types_jsx.rs
}
