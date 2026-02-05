//! Parser state - type parsing, JSX, accessors, and into_parts methods

use super::state::{ParseDiagnostic, ParserState};
use crate::interner::Atom;
use crate::parser::{NodeIndex, NodeList, node::*, syntax_kind_ext};
use crate::scanner::SyntaxKind;

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

        // Additional error recovery: if we're at a clear statement boundary or EOF,
        // and the token cannot start a type, return an error node.
        // Note: We must check can_token_start_type() because identifiers are both
        // statement starters AND valid type names (e.g., "let x: MyType = ...")
        if !self.can_token_start_type()
            && (self.is_statement_start() || self.is_token(SyntaxKind::EndOfFileToken))
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

    /// Enhanced parse_expected for better error recovery in type annotation contexts
    #[allow(dead_code)]
    pub(crate) fn parse_expected_in_type_context(&mut self, kind: SyntaxKind) -> bool {
        if self.is_token(kind) {
            self.next_token();
            true
        } else {
            // Enhanced error recovery for type contexts - be more permissive
            let should_suppress = match kind {
                SyntaxKind::GreaterThanToken => {
                    // For missing > in type arguments, if we see statement starters or closing tokens,
                    // it's likely a type parsing error that shouldn't cascade
                    if self.is_statement_start()
                        || self.is_token(SyntaxKind::CloseBraceToken)
                        || self.is_token(SyntaxKind::CloseParenToken)
                        || self.is_token(SyntaxKind::CloseBracketToken)
                    {
                        return true;
                    }
                    false
                }
                SyntaxKind::CloseParenToken
                | SyntaxKind::CloseBracketToken
                | SyntaxKind::CloseBraceToken => {
                    // In type contexts, if we see statement starters, suppress the error
                    self.is_statement_start()
                        || self.is_token(SyntaxKind::SemicolonToken)
                        || self.scanner.has_preceding_line_break()
                }
                SyntaxKind::ColonToken => {
                    // For missing colon in type annotations, be more permissive
                    self.is_statement_start()
                }
                _ => false,
            };

            if !should_suppress {
                self.error_token_expected(Self::token_to_string(kind));
            }
            false
        }
    }

    /// Parse return type, which may be a type predicate (x is T) or a regular type
    pub(crate) fn parse_return_type(&mut self) -> NodeIndex {
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

        // Check for extends keyword to form conditional
        if !self.is_token(SyntaxKind::ExtendsKeyword) {
            return check_type;
        }

        self.next_token(); // consume extends

        // Parse the extends type (right side of extends)
        let extends_type = self.parse_union_type();

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

        // If we encounter a token that can't start a type, emit TS1110 instead of TS1005
        if !self.can_token_start_type() {
            self.error_type_expected();
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

        // Handle abstract constructor types: abstract new () => T
        if self.is_token(SyntaxKind::AbstractKeyword) {
            // Look ahead to see if this is "abstract new"
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token();
            let is_abstract_new = self.is_token(SyntaxKind::NewKeyword);
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if is_abstract_new {
                // Consume 'abstract' and parse the constructor type
                self.next_token();
                return self.parse_constructor_type(true);
            }
        }

        // Handle constructor types: new () => T or new <T>() => T
        if self.is_token(SyntaxKind::NewKeyword) {
            return self.parse_constructor_type(false);
        }

        // Handle generic function types: <T>() => T or <T, U>(x: T) => U
        if self.is_token(SyntaxKind::LessThanToken) {
            return self.parse_generic_function_type();
        }

        // Handle parenthesized types or function types
        if self.is_token(SyntaxKind::OpenParenToken) {
            // Check if this is a function type: () => T or (x: T) => U
            if self.look_ahead_is_function_type() {
                return self.parse_function_type();
            }

            // Otherwise it's a parenthesized type
            self.next_token();
            let inner = self.parse_type();
            self.parse_expected(SyntaxKind::CloseParenToken);

            // Handle array types on parenthesized: (A | B)[]
            if self.is_token(SyntaxKind::OpenBracketToken) {
                return self.parse_array_type(start_pos, inner);
            }
            return inner;
        }

        // Handle tuple types: [T, U, V]
        if self.is_token(SyntaxKind::OpenBracketToken) {
            return self.parse_tuple_type();
        }

        // Handle object type literal or mapped type: { ... } or { [K in T]: U }
        if self.is_token(SyntaxKind::OpenBraceToken) {
            let obj_type = self.parse_object_or_mapped_type();
            // Handle array/indexed access on object literal: {...}[] or {...}["key"]
            if self.is_token(SyntaxKind::OpenBracketToken) {
                return self.parse_array_type(start_pos, obj_type);
            }
            return obj_type;
        }

        // Handle typeof type: typeof x, typeof x[]
        if self.is_token(SyntaxKind::TypeOfKeyword) {
            let typeof_type = self.parse_typeof_type();
            // Handle array type on typeof: typeof x[]
            if self.is_token(SyntaxKind::OpenBracketToken) {
                return self.parse_array_type(start_pos, typeof_type);
            }
            return typeof_type;
        }

        // Handle keyof type: keyof T, keyof T[]
        if self.is_token(SyntaxKind::KeyOfKeyword) {
            let keyof_type = self.parse_keyof_type();
            // Handle array type on keyof: keyof T[]
            if self.is_token(SyntaxKind::OpenBracketToken) {
                return self.parse_array_type(start_pos, keyof_type);
            }
            return keyof_type;
        }

        // Handle unique type: unique symbol
        if self.is_token(SyntaxKind::UniqueKeyword) {
            let unique_type = self.parse_unique_type();
            // Handle array type on unique: unique symbol[]
            if self.is_token(SyntaxKind::OpenBracketToken) {
                return self.parse_array_type(start_pos, unique_type);
            }
            return unique_type;
        }

        // Handle readonly type: readonly T[]
        if self.is_token(SyntaxKind::ReadonlyKeyword) {
            return self.parse_readonly_type();
        }

        // Handle infer type: infer T (used in conditional types)
        if self.is_token(SyntaxKind::InferKeyword) {
            return self.parse_infer_type();
        }

        // Handle 'this' type (polymorphic this)
        if self.is_token(SyntaxKind::ThisKeyword) {
            let this_start = self.token_pos();
            let this_end = self.token_end();
            self.next_token();
            return self
                .arena
                .add_token(syntax_kind_ext::THIS_TYPE, this_start, this_end);
        }

        // Handle literal types: "foo", 42, true, false
        if self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_token(SyntaxKind::BigIntLiteral)
            || self.is_token(SyntaxKind::TrueKeyword)
            || self.is_token(SyntaxKind::FalseKeyword)
        {
            return self.parse_literal_type();
        }

        // Handle negative numeric literal types: -1, -42
        if self.is_token(SyntaxKind::MinusToken) {
            return self.parse_prefix_unary_literal_type();
        }

        // Handle template literal types: `hello` or `prefix${T}suffix`
        if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
            || self.is_token(SyntaxKind::TemplateHead)
        {
            return self.parse_template_literal_type();
        }

        // Check for type keywords (string, number, boolean, etc.)
        // Also handle contextual keywords (await, yield) which are valid as type names
        let first_name = match self.token() {
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
            | SyntaxKind::AssertsKeyword => {
                // Parse keyword as identifier for type reference
                self.parse_keyword_as_identifier()
            }
            SyntaxKind::PrivateIdentifier => self.parse_private_identifier(),
            _ => {
                // Regular identifier
                self.parse_identifier()
            }
        };

        // Handle qualified names (foo.Bar, A.B.C)
        let type_name = self.parse_qualified_name_rest(first_name);

        // Check for type arguments: Foo<T, U>
        let type_arguments = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_arguments())
        } else {
            None
        };

        let base_type = self.arena.add_type_ref(
            syntax_kind_ext::TYPE_REFERENCE,
            start_pos,
            self.token_end(),
            crate::parser::node::TypeRefData {
                type_name,
                type_arguments,
            },
        );

        // Handle array types (T[])
        if self.is_token(SyntaxKind::OpenBracketToken) {
            return self.parse_array_type(start_pos, base_type);
        }

        base_type
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
            let has_question = self.parse_optional(SyntaxKind::QuestionToken);
            let has_colon = self.is_token(SyntaxKind::ColonToken);

            if has_colon || has_question {
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
    /// represented as a PrefixUnaryExpression wrapped in a LiteralType
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

    /// Parse typeof type: typeof x, typeof x.y
    pub(crate) fn parse_typeof_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::TypeOfKeyword);

        // Parse the expression name (can be qualified: x.y.z)
        let expr_name = self.parse_entity_name();

        // Parse optional type arguments for instantiation expressions: typeof Err<U>
        let type_arguments = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_arguments())
        } else {
            None
        };

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
    pub(crate) fn parse_infer_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::InferKeyword);

        // Parse the type parameter to infer
        let type_parameter = self.parse_type_parameter();

        let end_pos = self.token_end();

        self.arena.add_infer_type(
            syntax_kind_ext::INFER_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::InferTypeData { type_parameter },
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

    /// Parse template literal head (NoSubstitutionTemplateLiteral or TemplateHead)
    pub(crate) fn parse_template_literal_head(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let is_unterminated = self.scanner.is_unterminated();
        let kind = self.token() as u16;
        let text = self.scanner.get_token_value_ref().to_string();
        let literal_end = self.token_end();
        self.next_token();
        let end_pos = self.token_end();
        if is_unterminated {
            self.error_unterminated_template_literal_at(start_pos, literal_end);
        }
        self.arena.add_literal(
            kind,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    /// Parse template literal span (TemplateMiddle or TemplateTail)
    pub(crate) fn parse_template_literal_span(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let is_unterminated = self.scanner.is_unterminated();
        let kind = self.token() as u16;
        let text = self.scanner.get_token_value_ref().to_string();
        let literal_end = self.token_end();
        self.next_token();
        let end_pos = self.token_end();
        if is_unterminated {
            self.error_unterminated_template_literal_at(start_pos, literal_end);
        }
        self.arena.add_literal(
            kind,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    /// Parse object type literal or mapped type
    /// Object type: { prop: T; method(): U }
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

        self.next_token(); // skip readonly
        let is_mapped = self.is_token(SyntaxKind::OpenBracketToken);

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

        let constraint = self.parse_type();

        // Parse optional 'as' clause for key remapping: [K in T as NewKey]
        let name_type = if self.parse_optional(SyntaxKind::AsKeyword) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

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
            let member = self.parse_type_member();

            // If parse_type_member returned NONE (couldn't parse) and we haven't advanced,
            // skip the current token to prevent infinite loops
            if member.is_none() && self.token_pos() == saved_pos {
                self.error_unexpected_token();
                self.next_token(); // Skip the problematic token
                continue;
            }

            if !member.is_none() {
                members.push(member);
            }

            // Allow comma or semicolon as separator
            if !self.parse_optional(SyntaxKind::SemicolonToken) {
                self.parse_optional(SyntaxKind::CommaToken);
            }
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
    pub(crate) fn is_greater_than_or_compound(&self) -> bool {
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
    pub(crate) fn parse_type_arguments(&mut self) -> NodeList {
        self.parse_expected(SyntaxKind::LessThanToken);

        let mut args = Vec::new();

        while !self.is_greater_than_or_compound() && !self.is_token(SyntaxKind::EndOfFileToken) {
            args.push(self.parse_type());

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
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

        // Consume <
        self.next_token();

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
                // Continue to next type argument
                continue;
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

            // Check if followed by ( or ` (which indicates a call/tagged template)
            if self.is_token(SyntaxKind::OpenParenToken)
                || self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
                || self.is_token(SyntaxKind::TemplateHead)
            {
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

    /// Check if current token is a keyword that can be used as a property name
    pub(crate) fn is_property_name_keyword(&self) -> bool {
        matches!(
            self.token(),
            SyntaxKind::TypeKeyword
                | SyntaxKind::GetKeyword
                | SyntaxKind::SetKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::AwaitKeyword
                | SyntaxKind::NewKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::OverrideKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::ExportKeyword
                | SyntaxKind::DefaultKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::VarKeyword
                | SyntaxKind::IfKeyword
                | SyntaxKind::ElseKeyword
                | SyntaxKind::ForKeyword
                | SyntaxKind::WhileKeyword
                | SyntaxKind::DoKeyword
                | SyntaxKind::SwitchKeyword
                | SyntaxKind::CaseKeyword
                | SyntaxKind::BreakKeyword
                | SyntaxKind::ContinueKeyword
                | SyntaxKind::ReturnKeyword
                | SyntaxKind::ThrowKeyword
                | SyntaxKind::TryKeyword
                | SyntaxKind::CatchKeyword
                | SyntaxKind::FinallyKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::ImportKeyword
                | SyntaxKind::FromKeyword
                | SyntaxKind::AsKeyword
                | SyntaxKind::InKeyword
                | SyntaxKind::OfKeyword
                | SyntaxKind::InstanceOfKeyword
                | SyntaxKind::ThisKeyword
                | SyntaxKind::SuperKeyword
                | SyntaxKind::DeleteKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::TypeOfKeyword
                | SyntaxKind::YieldKeyword
                | SyntaxKind::ConstructorKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::ImplementsKeyword
                | SyntaxKind::ExtendsKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::RequireKeyword
                | SyntaxKind::GlobalKeyword
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::NullKeyword
                | SyntaxKind::UndefinedKeyword
                | SyntaxKind::OutKeyword
                | SyntaxKind::SatisfiesKeyword
                | SyntaxKind::AssertKeyword
                | SyntaxKind::AssertsKeyword
                | SyntaxKind::KeyOfKeyword
                | SyntaxKind::UniqueKeyword
                | SyntaxKind::InferKeyword
                | SyntaxKind::IsKeyword
                | SyntaxKind::AnyKeyword
                | SyntaxKind::NeverKeyword
                | SyntaxKind::UnknownKeyword
                | SyntaxKind::BigIntKeyword
                | SyntaxKind::ObjectKeyword
                | SyntaxKind::StringKeyword
                | SyntaxKind::NumberKeyword
                | SyntaxKind::SymbolKeyword
                | SyntaxKind::UsingKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::DeferKeyword
        )
    }

    /// Look ahead to see if ( starts a function type: () => T or (x: T) => U
    pub(crate) fn look_ahead_is_function_type(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip (
        self.next_token();

        // Empty params: () =>
        if self.is_token(SyntaxKind::CloseParenToken) {
            self.next_token();
            let is_arrow = self.is_token(SyntaxKind::EqualsGreaterThanToken);
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return is_arrow;
        }

        // Check for parameter-like syntax: identifier or keyword followed by : or )
        // If we see just a type (like `string`), it could be parenthesized type
        // Function type params have: `name:` or `modifier name` where modifier is public/private/protected/readonly
        if self.is_identifier_or_keyword() {
            self.next_token();
            // If followed by : it's definitely a function type parameter
            if self.is_token(SyntaxKind::ColonToken) {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                return true;
            }
            // If followed by a parameter modifier (public, private, protected, readonly), it's a parameter
            // But NOT if followed by 'extends' - that's a conditional type!
            let is_param_modifier = matches!(
                self.token(),
                SyntaxKind::PublicKeyword
                    | SyntaxKind::PrivateKeyword
                    | SyntaxKind::ProtectedKeyword
                    | SyntaxKind::ReadonlyKeyword
            );
            if is_param_modifier {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                return true;
            }
        }

        // For other cases, skip to matching ) to check for =>
        // First restore, then scan again
        self.scanner.restore_state(snapshot);
        self.current_token = current;

        let snapshot2 = self.scanner.save_state();
        self.next_token(); // Skip (

        let mut depth = 1;
        while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
            if self.is_token(SyntaxKind::OpenParenToken) {
                depth += 1;
            } else if self.is_token(SyntaxKind::CloseParenToken) {
                depth -= 1;
            }
            self.next_token();
        }

        let is_arrow = self.is_token(SyntaxKind::EqualsGreaterThanToken);
        self.scanner.restore_state(snapshot2);
        self.current_token = current;
        is_arrow
    }

    /// Parse function type: (x: T, y: U) => V
    pub(crate) fn parse_function_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse parameters
        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_type_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Parse =>
        self.parse_expected(SyntaxKind::EqualsGreaterThanToken);

        // Parse return type (supports type predicates: param is T)
        let type_annotation = self.parse_return_type();

        let end_pos = self.token_end();

        self.arena.add_function_type(
            syntax_kind_ext::FUNCTION_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::FunctionTypeData {
                type_parameters: None,
                parameters,
                type_annotation,
                is_abstract: false,
            },
        )
    }

    /// Parse generic function type: <T>() => T or <T, U extends V>(x: T) => U
    pub(crate) fn parse_generic_function_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse type parameters: <T, U extends V>
        let type_parameters = self.parse_type_parameters();

        // Parse parameters: (x: T, y: U)
        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_type_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Parse =>
        self.parse_expected(SyntaxKind::EqualsGreaterThanToken);

        // Parse return type (supports type predicates: param is T)
        let type_annotation = self.parse_return_type();

        let end_pos = self.token_end();

        self.arena.add_function_type(
            syntax_kind_ext::FUNCTION_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::FunctionTypeData {
                type_parameters: Some(type_parameters),
                parameters,
                type_annotation,
                is_abstract: false,
            },
        )
    }

    /// Parse constructor type: new () => T or new <T>() => T
    /// Also handles abstract constructor types: abstract new () => T
    pub(crate) fn parse_constructor_type(&mut self, is_abstract: bool) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::NewKeyword);

        // Parse optional type parameters: new <T>() => T
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        // Parse parameters: new (x: T, y: U) => ...
        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_type_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Parse => and return type
        self.parse_expected(SyntaxKind::EqualsGreaterThanToken);
        let type_annotation = self.parse_return_type();

        let end_pos = self.token_end();

        // Use ConstructorType kind - reuse FunctionTypeData since structure is the same
        self.arena.add_function_type(
            syntax_kind_ext::CONSTRUCTOR_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::FunctionTypeData {
                type_parameters,
                parameters,
                type_annotation,
                is_abstract,
            },
        )
    }

    /// Parse type parameter list for function types: (x: T, y: U)
    /// Also handles invalid modifiers like (public x) which TypeScript parses but errors on semantically
    pub(crate) fn parse_type_parameter_list(&mut self) -> NodeList {
        let mut params = Vec::new();

        while !self.is_token(SyntaxKind::CloseParenToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let param_start = self.token_pos();

            // Parse optional modifiers (public/private/protected/readonly)
            // These are syntactically valid but semantically invalid in function types
            let modifiers = self.parse_parameter_modifiers();

            // Parse optional ...rest
            let dot_dot_dot = self.parse_optional(SyntaxKind::DotDotDotToken);

            // Parse parameter name - can be identifier, keyword, or binding pattern
            let name = if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_object_binding_pattern()
            } else if self.is_token(SyntaxKind::OpenBracketToken) {
                self.parse_array_binding_pattern()
            } else if self.is_identifier_or_keyword() {
                self.parse_identifier_name()
            } else {
                self.parse_identifier()
            };

            // Parse optional ?
            let question = self.parse_optional(SyntaxKind::QuestionToken);

            // Parse type annotation
            let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_type()
            } else {
                NodeIndex::NONE
            };

            let param_end = self.token_end();

            let param = self.arena.add_parameter(
                syntax_kind_ext::PARAMETER,
                param_start,
                param_end,
                crate::parser::node::ParameterData {
                    modifiers,
                    dot_dot_dot_token: dot_dot_dot,
                    name,
                    question_token: question,
                    type_annotation,
                    initializer: NodeIndex::NONE,
                },
            );
            params.push(param);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        self.make_node_list(params)
    }

    /// Parse a keyword as an identifier (for type keywords like string, number, etc.)
    pub(crate) fn parse_keyword_as_identifier(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // OPTIMIZATION: Capture atom for O(1) comparison
        let atom = self.scanner.get_token_atom();
        let text = self.scanner.get_token_value_ref().to_string();
        self.next_token();
        let end_pos = self.token_end();

        self.arena.add_identifier(
            SyntaxKind::Identifier as u16,
            start_pos,
            end_pos,
            IdentifierData {
                atom,
                escaped_text: text,
                original_text: None,
                type_arguments: None,
            },
        )
    }

    /// Parse qualified name rest: given a left name, parse `.Right.Rest` parts
    /// Handles: foo.Bar, A.B.C, etc.
    pub(crate) fn parse_qualified_name_rest(&mut self, left: NodeIndex) -> NodeIndex {
        let mut current = left;

        while self.is_token(SyntaxKind::DotToken) {
            let start_pos = if let Some(node) = self.arena.get(current) {
                node.pos
            } else {
                self.token_pos()
            };

            self.next_token(); // consume .
            let right = self.parse_identifier_name();
            let end_pos = self.token_end();

            current = self.arena.add_qualified_name(
                syntax_kind_ext::QUALIFIED_NAME,
                start_pos,
                end_pos,
                crate::parser::node::QualifiedNameData {
                    left: current,
                    right,
                },
            );
        }

        current
    }

    // =========================================================================
    // Accessors
    // =========================================================================

    /// Get parse diagnostics
    pub fn get_diagnostics(&self) -> &[ParseDiagnostic] {
        &self.parse_diagnostics
    }

    /// Get the arena
    pub fn get_arena(&self) -> &NodeArena {
        &self.arena
    }

    /// Consume the parser and return the arena.
    /// This is used for lib files where we need to store the arena in an Arc.
    pub fn into_arena(mut self) -> NodeArena {
        // Transfer the interner from the scanner to the arena so atoms can be resolved
        self.arena.set_interner(self.scanner.take_interner());
        self.arena
    }

    /// Get node count
    pub fn get_node_count(&self) -> usize {
        self.arena.len()
    }

    /// Get the source text.
    /// Delegates to the scanner which owns the source text.
    pub fn get_source_text(&self) -> &str {
        self.scanner.source_text()
    }

    /// Get the file name
    pub fn get_file_name(&self) -> &str {
        &self.file_name
    }

    // =========================================================================
    // JSX Parsing
    // =========================================================================

    /// Determine if we should parse a type assertion or JSX element.
    /// Type assertions use <Type>expr syntax, JSX uses <Element>.
    pub(crate) fn parse_jsx_element_or_type_assertion(&mut self) -> NodeIndex {
        // In .tsx/.jsx files, all <...> syntax is JSX (use "as Type" for type assertions)
        // In .ts files, we need to distinguish type assertions from JSX
        if self.is_jsx_file() {
            return self.parse_jsx_element_or_self_closing_or_fragment(true);
        }

        // In .ts files (non-JSX), always try to parse as type assertion first.
        // This will produce appropriate errors (e.g., TS1005 " '>' expected") for invalid JSX-like syntax.
        self.parse_type_assertion()
    }

    /// Parse a type assertion: <Type>expression
    pub(crate) fn parse_type_assertion(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::LessThanToken);
        let type_node = self.parse_type();
        self.parse_expected(SyntaxKind::GreaterThanToken);
        let expression = self.parse_unary_expression();
        let end_pos = self.token_end();

        self.arena.add_type_assertion(
            syntax_kind_ext::TYPE_ASSERTION,
            start_pos,
            end_pos,
            TypeAssertionData {
                type_node,
                expression,
            },
        )
    }

    /// Parse a JSX element, self-closing element, or fragment.
    /// Called when we see `<` in an expression context.
    pub(crate) fn parse_jsx_element_or_self_closing_or_fragment(
        &mut self,
        in_expression_context: bool,
    ) -> NodeIndex {
        let start_pos = self.token_pos();
        let opening = self.parse_jsx_opening_or_self_closing_or_fragment(in_expression_context);

        // Check what type of opening element we got
        let kind = self.arena.get(opening).map(|n| n.kind).unwrap_or(0);

        if kind == syntax_kind_ext::JSX_OPENING_ELEMENT {
            // Parse children and closing element
            let children = self.parse_jsx_children();
            let closing = self.parse_jsx_closing_element();
            let end_pos = self.token_end();

            self.arena.add_jsx_element(
                syntax_kind_ext::JSX_ELEMENT,
                start_pos,
                end_pos,
                crate::parser::node::JsxElementData {
                    opening_element: opening,
                    children,
                    closing_element: closing,
                },
            )
        } else if kind == syntax_kind_ext::JSX_OPENING_FRAGMENT {
            // Parse children and closing fragment
            let children = self.parse_jsx_children();
            let closing = self.parse_jsx_closing_fragment();
            let end_pos = self.token_end();

            self.arena.add_jsx_fragment(
                syntax_kind_ext::JSX_FRAGMENT,
                start_pos,
                end_pos,
                crate::parser::node::JsxFragmentData {
                    opening_fragment: opening,
                    children,
                    closing_fragment: closing,
                },
            )
        } else {
            // Self-closing element, already complete
            opening
        }
    }

    /// Parse JSX opening element, self-closing element, or opening fragment.
    pub(crate) fn parse_jsx_opening_or_self_closing_or_fragment(
        &mut self,
        _in_expression_context: bool,
    ) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::LessThanToken);

        // Check for fragment: <>
        if self.is_token(SyntaxKind::GreaterThanToken) {
            let end_pos = self.token_end();
            self.next_token(); // consume >
            return self
                .arena
                .add_token(syntax_kind_ext::JSX_OPENING_FRAGMENT, start_pos, end_pos);
        }

        // Parse tag name
        let tag_name = self.parse_jsx_element_name();

        // Parse optional type arguments
        let type_arguments = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_arguments())
        } else {
            None
        };

        // Parse attributes
        let attributes = self.parse_jsx_attributes();

        // Check for self-closing: />
        if self.is_token(SyntaxKind::SlashToken) {
            self.next_token(); // consume /
            let end_pos = self.token_end();
            self.parse_expected(SyntaxKind::GreaterThanToken);
            return self.arena.add_jsx_opening(
                syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT,
                start_pos,
                end_pos,
                crate::parser::node::JsxOpeningData {
                    tag_name,
                    type_arguments,
                    attributes,
                },
            );
        }

        // Opening element: consume > and continue parsing children
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::GreaterThanToken);
        self.arena.add_jsx_opening(
            syntax_kind_ext::JSX_OPENING_ELEMENT,
            start_pos,
            end_pos,
            crate::parser::node::JsxOpeningData {
                tag_name,
                type_arguments,
                attributes,
            },
        )
    }

    /// Parse JSX element name (identifier, this, namespaced, or property access).
    pub(crate) fn parse_jsx_element_name(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Error recovery: if the current token can't start a JSX element name,
        // return a missing identifier to avoid crashes
        if !self.is_token(SyntaxKind::Identifier)
            && !self.is_token(SyntaxKind::ThisKeyword)
            && !self.is_identifier_or_keyword()
        {
            self.error_identifier_expected();
            // Create a missing identifier node
            let end_pos = self.token_end();
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

        // Parse the initial name (identifier or this)
        let mut expr = if self.is_token(SyntaxKind::ThisKeyword) {
            let pos = self.token_pos();
            self.next_token();
            let end_pos = self.token_end();
            self.arena
                .add_token(SyntaxKind::ThisKeyword as u16, pos, end_pos)
        } else {
            if self.is_token(SyntaxKind::Identifier) {
                self.scanner.scan_jsx_identifier();
            }
            let name = self.parse_identifier();

            // Check for namespaced name (a:b)
            if self.is_token(SyntaxKind::ColonToken) {
                self.next_token(); // consume :
                let local_name = self.parse_identifier();
                let end_pos = self.token_end();
                return self.arena.add_jsx_namespaced_name(
                    syntax_kind_ext::JSX_NAMESPACED_NAME,
                    start_pos,
                    end_pos,
                    crate::parser::node::JsxNamespacedNameData {
                        namespace: name,
                        name: local_name,
                    },
                );
            }

            name
        };

        // Parse property access chain (Foo.Bar.Baz)
        while self.is_token(SyntaxKind::DotToken) {
            self.next_token(); // consume .
            let name = self.parse_identifier();
            let end_pos = self.token_end();
            expr = self.arena.add_access_expr(
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                start_pos,
                end_pos,
                crate::parser::node::AccessExprData {
                    expression: expr,
                    name_or_argument: name,
                    question_dot_token: false,
                },
            );
        }

        expr
    }

    /// Parse JSX attributes list.
    pub(crate) fn parse_jsx_attributes(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut properties = Vec::new();

        while !self.is_token(SyntaxKind::GreaterThanToken)
            && !self.is_token(SyntaxKind::SlashToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            if self.is_token(SyntaxKind::OpenBraceToken) {
                // Spread attribute: {...props}
                properties.push(self.parse_jsx_spread_attribute());
            } else {
                // Regular attribute: name="value" or name={expr} or just name
                properties.push(self.parse_jsx_attribute());
            }
        }

        let end_pos = self.token_end();
        self.arena.add_jsx_attributes(
            syntax_kind_ext::JSX_ATTRIBUTES,
            start_pos,
            end_pos,
            crate::parser::node::JsxAttributesData {
                properties: self.make_node_list(properties),
            },
        )
    }

    /// Parse a single JSX attribute.
    pub(crate) fn parse_jsx_attribute(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Error recovery: if the current token can't start an attribute name,
        // report error and skip to next attribute or end of attributes
        if !self.is_token(SyntaxKind::Identifier) && !self.is_identifier_or_keyword() {
            self.error_identifier_expected();
            // Skip the invalid token to prevent infinite loops
            self.next_token();
            // Return a dummy attribute with missing name
            let end_pos = self.token_end();
            return self.arena.add_jsx_attribute(
                syntax_kind_ext::JSX_ATTRIBUTE,
                start_pos,
                end_pos,
                crate::parser::node::JsxAttributeData {
                    name: NodeIndex::NONE,
                    initializer: NodeIndex::NONE,
                },
            );
        }

        let name = self.parse_jsx_attribute_name();

        // Check for value: = followed by string, expression, or nested JSX
        let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
            if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_jsx_expression()
            } else if self.is_token(SyntaxKind::LessThanToken) {
                self.parse_jsx_element_or_self_closing_or_fragment(true)
            } else {
                self.error_expression_expected();
                NodeIndex::NONE
            }
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_jsx_attribute(
            syntax_kind_ext::JSX_ATTRIBUTE,
            start_pos,
            end_pos,
            crate::parser::node::JsxAttributeData { name, initializer },
        )
    }

    /// Parse JSX attribute name (possibly namespaced).
    /// JSX attribute names can be keywords like "extends", "class", etc.
    pub(crate) fn parse_jsx_attribute_name(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        if self.is_token(SyntaxKind::Identifier) {
            self.scanner.scan_jsx_identifier();
        }
        // Use parse_identifier_name to allow keywords as attribute names
        let name = self.parse_identifier_name();

        // Check for namespaced name (a:b)
        if self.is_token(SyntaxKind::ColonToken) {
            self.next_token(); // consume :
            // Also allow keywords for the local part of namespaced names
            let local_name = self.parse_identifier_name();
            let end_pos = self.token_end();
            return self.arena.add_jsx_namespaced_name(
                syntax_kind_ext::JSX_NAMESPACED_NAME,
                start_pos,
                end_pos,
                crate::parser::node::JsxNamespacedNameData {
                    namespace: name,
                    name: local_name,
                },
            );
        }

        name
    }

    /// Parse a JSX spread attribute: {...props}
    pub(crate) fn parse_jsx_spread_attribute(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);
        self.parse_expected(SyntaxKind::DotDotDotToken);
        let expression = self.parse_expression();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_jsx_spread_attribute(
            syntax_kind_ext::JSX_SPREAD_ATTRIBUTE,
            start_pos,
            end_pos,
            crate::parser::node::JsxSpreadAttributeData { expression },
        )
    }

    /// Parse a JSX expression: {expr} or {...expr}
    pub(crate) fn parse_jsx_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        // Check for spread: {...}
        let dot_dot_dot_token = self.parse_optional(SyntaxKind::DotDotDotToken);

        // Check for empty expression: {}
        let expression = if self.is_token(SyntaxKind::CloseBraceToken) {
            NodeIndex::NONE
        } else {
            self.parse_expression()
        };

        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_jsx_expression(
            syntax_kind_ext::JSX_EXPRESSION,
            start_pos,
            end_pos,
            crate::parser::node::JsxExpressionData {
                dot_dot_dot_token,
                expression,
            },
        )
    }

    /// Parse JSX children (elements, text, expressions).
    pub(crate) fn parse_jsx_children(&mut self) -> NodeList {
        let mut children = Vec::new();

        loop {
            // Rescan in JSX context to get proper JsxText tokens and LessThanSlashToken
            // This is necessary because after parsing expressions or nested elements,
            // the scanner may not be in JSX mode.
            self.current_token = self.scanner.re_scan_jsx_token(true);

            match self.current_token {
                SyntaxKind::LessThanSlashToken => {
                    // Closing tag/fragment - stop parsing children
                    break;
                }
                SyntaxKind::LessThanToken => {
                    // Nested JSX element
                    children.push(self.parse_jsx_element_or_self_closing_or_fragment(false));
                }
                SyntaxKind::OpenBraceToken => {
                    // JSX expression: {expr}
                    children.push(self.parse_jsx_expression());
                }
                SyntaxKind::JsxText => {
                    // Text node
                    children.push(self.parse_jsx_text());
                }
                SyntaxKind::EndOfFileToken => {
                    break;
                }
                _ => {
                    // Unknown token in JSX children - stop
                    break;
                }
            }
        }

        self.make_node_list(children)
    }

    /// Parse JSX text content.
    pub(crate) fn parse_jsx_text(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let text = self.scanner.get_token_value_ref().to_string();
        let end_pos = self.token_end();
        self.next_token();

        self.arena.add_jsx_text(
            SyntaxKind::JsxText as u16,
            start_pos,
            end_pos,
            crate::parser::node::JsxTextData {
                text,
                contains_only_trivia_white_spaces: false,
            },
        )
    }

    /// Parse a JSX closing element: </Foo>
    pub(crate) fn parse_jsx_closing_element(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // In JSX mode, </ is scanned as a single LessThanSlashToken
        self.parse_expected(SyntaxKind::LessThanSlashToken);
        let tag_name = self.parse_jsx_element_name();
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::GreaterThanToken);
        self.arena.add_jsx_closing(
            syntax_kind_ext::JSX_CLOSING_ELEMENT,
            start_pos,
            end_pos,
            crate::parser::node::JsxClosingData { tag_name },
        )
    }

    /// Parse a JSX closing fragment: </>
    pub(crate) fn parse_jsx_closing_fragment(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // In JSX mode, </ is scanned as a single LessThanSlashToken
        self.parse_expected(SyntaxKind::LessThanSlashToken);
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::GreaterThanToken);
        self.arena
            .add_token(syntax_kind_ext::JSX_CLOSING_FRAGMENT, start_pos, end_pos)
    }

    /// Consume the parser and return its parts.
    /// This is useful for taking ownership of the arena after parsing.
    pub fn into_parts(mut self) -> (NodeArena, Vec<ParseDiagnostic>) {
        // Transfer the interner from the scanner to the arena so atoms can be resolved
        self.arena.set_interner(self.scanner.take_interner());
        (self.arena, self.parse_diagnostics)
    }
}
