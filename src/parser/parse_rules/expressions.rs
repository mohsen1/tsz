//! Expression parsing rules
//!
//! This module contains all expression parsing logic extracted from
//! parser/state.rs to reduce code duplication and improve maintainability.

use crate::parser::{
    NodeIndex,
    node::{
        AccessExprData, BinaryExprData, CallExprData, FunctionData, IdentifierData,
        LiteralExprData, NodeArena, ParameterData, ParenthesizedData, TaggedTemplateData,
        TemplateExprData, TemplateSpanData, UnaryExprData, UnaryExprDataEx,
    },
    syntax_kind_ext,
};
use crate::scanner::SyntaxKind;
use crate::scanner_impl::ScannerState;

// =============================================================================
// Expression Parsing Context
// =============================================================================

/// Context for expression parsing (shared with parser state)
pub struct ExpressionParseContext<'a> {
    pub arena: &'a mut NodeArena,
    pub scanner: &'a mut ScannerState,
    pub current_token: SyntaxKind,
    pub context_flags: u32,
    pub node_count: u32,
}

impl<'a> ExpressionParseContext<'a> {
    pub fn new(
        arena: &'a mut NodeArena,
        scanner: &'a mut ScannerState,
        current_token: SyntaxKind,
        context_flags: u32,
        node_count: u32,
    ) -> Self {
        Self {
            arena,
            scanner,
            current_token,
            context_flags,
            node_count,
        }
    }
}

// =============================================================================
// Primary Expressions
// =============================================================================

/// Parse a primary expression.
///
/// Primary expressions are the most basic expressions:
/// - Literals (null, true, false, numbers, strings, etc.)
/// - this keyword
/// - super keyword
/// - Identifiers
/// - Parenthesized expressions
/// - Array literals
/// - Object literals
/// - Function expressions
/// - Class expressions
/// - Template literals
/// - JSX expressions
/// - import expressions
pub fn parse_primary_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    match ctx.current_token {
        SyntaxKind::ThisKeyword => parse_this_expression(ctx),
        SyntaxKind::SuperKeyword => parse_super_expression(ctx),
        SyntaxKind::NullKeyword
        | SyntaxKind::TrueKeyword
        | SyntaxKind::FalseKeyword
        | SyntaxKind::NumericLiteral
        | SyntaxKind::BigIntLiteral
        | SyntaxKind::StringLiteral
        | SyntaxKind::NoSubstitutionTemplateLiteral => parse_literal_expression(ctx),
        SyntaxKind::OpenParenToken => parse_parenthesized_expression(ctx),
        SyntaxKind::OpenBracketToken => parse_array_literal_expression(ctx),
        SyntaxKind::OpenBraceToken => parse_object_literal_expression(ctx),
        SyntaxKind::FunctionKeyword => parse_function_expression(ctx),
        SyntaxKind::ClassKeyword => parse_class_expression(ctx),
        SyntaxKind::TemplateHead => parse_template_expression(ctx),
        SyntaxKind::ImportKeyword => parse_import_expression(ctx),
        _ => {
            if is_jsx_start(ctx.current_token) {
                parse_jsx_expression(ctx)
            } else {
                parse_identifier_expression(ctx)
            }
        }
    }
}

/// Parse a 'this' keyword expression.
pub fn parse_this_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let start_pos = token_pos(ctx);
    let end_pos = token_end(ctx);
    next_token(ctx);
    ctx.arena.add_identifier(
        SyntaxKind::ThisKeyword as u16,
        start_pos,
        end_pos,
        IdentifierData { name: None },
    )
}

/// Parse a 'super' keyword expression.
pub fn parse_super_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let start_pos = token_pos(ctx);
    let end_pos = token_end(ctx);
    next_token(ctx);
    ctx.arena.add_identifier(
        SyntaxKind::SuperKeyword as u16,
        start_pos,
        end_pos,
        IdentifierData { name: None },
    )
}

/// Parse a literal expression (null, true, false, number, string, etc.)
pub fn parse_literal_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let start_pos = token_pos(ctx);
    let kind = ctx.current_token;
    let end_pos = token_end(ctx);
    next_token(ctx);
    ctx.arena.add_literal_expr(
        kind as u16,
        start_pos,
        end_pos,
        LiteralExprData { kind },
    )
}

/// Parse a parenthesized expression: ( expression )
pub fn parse_parenthesized_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let start_pos = token_pos(ctx);
    expect_token(ctx, SyntaxKind::OpenParenToken);

    let expression = parse_expression(ctx);

    expect_token(ctx, SyntaxKind::CloseParenToken);
    let end_pos = token_end(ctx);

    ctx.arena.add_parenthesized(
        syntax_kind_ext::PARENTHESIZED_EXPRESSION,
        start_pos,
        end_pos,
        ParenthesizedData { expression },
    )
}

/// Parse an array literal expression: [ element1, element2, ... ]
pub fn parse_array_literal_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let start_pos = token_pos(ctx);
    expect_token(ctx, SyntaxKind::OpenBracketToken);

    let mut elements = Vec::new();
    while !is_token(ctx, SyntaxKind::CloseBracketToken) && !is_token(ctx, SyntaxKind::EndOfFileToken) {
        elements.push(parse_array_element(ctx));

        if !is_token(ctx, SyntaxKind::CloseBracketToken) {
            expect_token(ctx, SyntaxKind::CommaToken);
        }
    }

    let end_pos = token_end(ctx);
    expect_token(ctx, SyntaxKind::CloseBracketToken);

    ctx.arena.add_node_list(
        syntax_kind_ext::ARRAY_LITERAL_EXPRESSION,
        start_pos,
        end_pos,
        elements,
    )
}

/// Parse a single array element (may be a spread element).
fn parse_array_element(ctx: &mut ExpressionParseContext) -> NodeIndex {
    if is_token(ctx, SyntaxKind::DotDotDotToken) {
        parse_spread_element(ctx)
    } else {
        parse_expression(ctx)
    }
}

/// Parse a spread element: ...expression
fn parse_spread_element(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let start_pos = token_pos(ctx);
    expect_token(ctx, SyntaxKind::DotDotDotToken);

    let expression = parse_expression(ctx);
    let end_pos = token_end(ctx);

    ctx.arena.add_unary_expr(
        syntax_kind_ext::SPREAD_ELEMENT,
        start_pos,
        end_pos,
        UnaryExprData {
            operator: SyntaxKind::DotDotDotToken,
            operand: expression,
        },
    )
}

/// Parse an object literal expression: { prop1: value1, prop2: value2, ... }
pub fn parse_object_literal_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let start_pos = token_pos(ctx);
    expect_token(ctx, SyntaxKind::OpenBraceToken);

    let mut properties = Vec::new();
    while !is_token(ctx, SyntaxKind::CloseBraceToken) && !is_token(ctx, SyntaxKind::EndOfFileToken) {
        properties.push(parse_object_property(ctx));

        if !is_token(ctx, SyntaxKind::CloseBraceToken) {
            if is_token(ctx, SyntaxKind::CommaToken) {
                next_token(ctx);
            } else {
                // Try to recover
                break;
            }
        }
    }

    let end_pos = token_end(ctx);
    expect_token(ctx, SyntaxKind::CloseBraceToken);

    ctx.arena.add_node_list(
        syntax_kind_ext::OBJECT_LITERAL_EXPRESSION,
        start_pos,
        end_pos,
        properties,
    )
}

/// Parse a single object property (may be shorthand, method, or spread).
fn parse_object_property(ctx: &mut ExpressionParseContext) -> NodeIndex {
    if is_token(ctx, SyntaxKind::DotDotDotToken) {
        parse_spread_element(ctx)
    } else {
        parse_property_assignment(ctx)
    }
}

/// Parse a property assignment in an object literal.
fn parse_property_assignment(ctx: &mut ExpressionParseContext) -> NodeIndex {
    // This is simplified - full implementation would handle:
    // - Shorthand properties
    // - Computed properties
    // - Methods
    // - Getters/setters
    parse_identifier_expression(ctx)
}

// =============================================================================
// Unary and Binary Expressions
// =============================================================================

/// Parse a unary expression: +expr, -expr, !expr, ~expr, typeof expr, etc.
pub fn parse_unary_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    if is_unary_operator(ctx.current_token) {
        let start_pos = token_pos(ctx);
        let operator = ctx.current_token;
        next_token(ctx);

        let operand = parse_unary_expression(ctx);
        let end_pos = token_end(ctx);

        ctx.arena.add_unary_expr(
            syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
            start_pos,
            end_pos,
            UnaryExprData { operator, operand },
        )
    } else {
        parse_postfix_expression(ctx)
    }
}

/// Check if a token is a unary operator.
fn is_unary_operator(token: SyntaxKind) -> bool {
    matches!(
        token,
        SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::TildeToken
            | SyntaxKind::ExclamationToken
            | SyntaxKind::PlusPlusToken
            | SyntaxKind::MinusMinusToken
            | SyntaxKind::TypeOfKeyword
            | SyntaxKind::VoidKeyword
            | SyntaxKind::DeleteKeyword
            | SyntaxKind::AwaitKeyword
    )
}

/// Parse a postfix expression: expr++, expr--
pub fn parse_postfix_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let expression = parse_left_hand_side_expression(ctx);

    if matches!(
        ctx.current_token,
        SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
    ) && !scanner_has_preceding_line_break(ctx)
    {
        let start_pos = get_node_pos(ctx, expression);
        let operator = ctx.current_token;
        next_token(ctx);
        let end_pos = token_end(ctx);

        ctx.arena.add_unary_expr_ex(
            syntax_kind_ext::POSTFIX_UNARY_EXPRESSION,
            start_pos,
            end_pos,
            UnaryExprDataEx {
                operator,
                operand: expression,
            },
        )
    } else {
        expression
    }
}

/// Parse a left-hand side expression: member calls, property access, etc.
pub fn parse_left_hand_side_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let mut expression = parse_primary_expression(ctx);

    loop {
        match ctx.current_token {
            SyntaxKind::DotToken => {
                expression = parse_property_access_expression(ctx, expression);
            }
            SyntaxKind::OpenBracketToken => {
                expression = parse_element_access_expression(ctx, expression);
            }
            SyntaxKind::OpenParenToken => {
                expression = parse_call_expression(ctx, expression);
            }
            SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead => {
                expression = parse_tagged_template_expression(ctx, expression);
            }
            SyntaxKind::ExclamationToken | SyntaxKind::MinusGreaterThanToken => {
                expression = parse_non_null_expression(ctx, expression);
            }
            _ => break,
        }
    }

    expression
}

/// Parse a property access expression: expr.property
fn parse_property_access_expression(ctx: &mut ExpressionParseContext, expression: NodeIndex) -> NodeIndex {
    let start_pos = get_node_pos(ctx, expression);
    expect_token(ctx, SyntaxKind::DotToken);

    let name = parse_identifier_name(ctx);
    let end_pos = token_end(ctx);

    ctx.arena.add_access_expr(
        syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
        start_pos,
        end_pos,
        AccessExprData {
            expression,
            name: Some(name),
        },
    )
}

/// Parse an element access expression: expr[index]
fn parse_element_access_expression(ctx: &mut ExpressionParseContext, expression: NodeIndex) -> NodeIndex {
    let start_pos = get_node_pos(ctx, expression);
    expect_token(ctx, SyntaxKind::OpenBracketToken);

    let argument = parse_expression(ctx);
    let end_pos = token_end(ctx);
    expect_token(ctx, SyntaxKind::CloseBracketToken);

    ctx.arena.add_access_expr(
        syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION,
        start_pos,
        end_pos,
        AccessExprData {
            expression,
            name: Some(argument),
        },
    )
}

/// Parse a call expression: func(arg1, arg2, ...)
fn parse_call_expression(ctx: &mut ExpressionParseContext, expression: NodeIndex) -> NodeIndex {
    let start_pos = get_node_pos(ctx, expression);
    let mut args = Vec::new();

    expect_token(ctx, SyntaxKind::OpenParenToken);

    while !is_token(ctx, SyntaxKind::CloseParenToken) && !is_token(ctx, SyntaxKind::EndOfFileToken) {
        args.push(parse_expression(ctx));

        if !is_token(ctx, SyntaxKind::CloseParenToken) {
            expect_token(ctx, SyntaxKind::CommaToken);
        }
    }

    let end_pos = token_end(ctx);
    expect_token(ctx, SyntaxKind::CloseParenToken);

    ctx.arena.add_call_expr(
        syntax_kind_ext::CALL_EXPRESSION,
        start_pos,
        end_pos,
        CallExprData {
            expression,
            type_args: None,
            args: Some(args.into()),
        },
    )
}

/// Parse a tagged template expression: tag`template`
fn parse_tagged_template_expression(ctx: &mut ExpressionParseContext, tag: NodeIndex) -> NodeIndex {
    let start_pos = get_node_pos(ctx, tag);
    let template = parse_literal_expression(ctx);
    let end_pos = token_end(ctx);

    ctx.arena.add_tagged_template(
        syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
        start_pos,
        end_pos,
        TaggedTemplateData {
            tag,
            template: Some(template),
        },
    )
}

/// Parse a non-null expression: expr!
fn parse_non_null_expression(ctx: &mut ExpressionParseContext, expression: NodeIndex) -> NodeIndex {
    let start_pos = get_node_pos(ctx, expression);
    expect_token(ctx, SyntaxKind::ExclamationToken);
    let end_pos = token_end(ctx);

    ctx.arena.add_unary_expr(
        syntax_kind_ext::NON_NULL_EXPRESSION,
        start_pos,
        end_pos,
        UnaryExprData {
            operator: SyntaxKind::ExclamationToken,
            operand: expression,
        },
    )
}

// =============================================================================
// Binary Expressions
// =============================================================================

/// Parse a binary expression with precedence climbing.
pub fn parse_binary_expression(ctx: &mut ExpressionParseContext, min_precedence: u8) -> NodeIndex {
    let mut left = parse_unary_expression(ctx);

    loop {
        let precedence = get_operator_precedence(ctx.current_token);
        if precedence < min_precedence {
            break;
        }

        let operator = ctx.current_token;
        next_token(ctx);

        let right = parse_binary_expression(ctx, precedence + 1);
        let start_pos = get_node_pos(ctx, left);
        let end_pos = token_end(ctx);

        left = ctx.arena.add_binary_expr(
            syntax_kind_ext::BINARY_EXPRESSION,
            start_pos,
            end_pos,
            BinaryExprData {
                operator,
                left,
                right,
            },
        );
    }

    left
}

/// Get the precedence of a binary operator.
fn get_operator_precedence(token: SyntaxKind) -> u8 {
    match token {
        SyntaxKind::QuestionQuestionToken => 3,
        SyntaxKind::BarBarToken => 4,
        SyntaxKind::AmpersandAmpersandToken => 5,
        SyntaxKind::BarToken => 6,
        SyntaxKind::CaretToken => 7,
        SyntaxKind::AmpersandToken => 8,
        SyntaxKind::EqualsEqualsToken | SyntaxKind::ExclamationEqualsToken | SyntaxKind::EqualsEqualsEqualsToken | SyntaxKind::ExclamationEqualsEqualsToken => 9,
        SyntaxKind::LessThanToken | SyntaxKind::GreaterThanToken | SyntaxKind::LessThanEqualsToken | SyntaxKind::GreaterThanEqualsToken => 10,
        SyntaxKind::LessThanLessThanToken | SyntaxKind::GreaterThanGreaterThanToken | SyntaxKind::GreaterThanGreaterThanGreaterThanToken => 11,
        SyntaxKind::PlusToken | SyntaxKind::MinusToken => 12,
        SyntaxKind::AsteriskToken | SyntaxKind::SlashToken | SyntaxKind::PercentToken => 13,
        SyntaxKind::AsteriskAsteriskToken => 14,
        _ => 0,
    }
}

// =============================================================================
// General Expression Entry Point
// =============================================================================

/// Parse an expression (general entry point).
pub fn parse_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    parse_binary_expression(ctx, 0)
}

/// Parse an identifier expression.
fn parse_identifier_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    parse_identifier_name(ctx)
}

/// Parse an identifier name (used for identifiers and property names).
fn parse_identifier_name(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let start_pos = token_pos(ctx);
    let end_pos = token_end(ctx);
    next_token(ctx);

    ctx.arena.add_identifier(
        SyntaxKind::Identifier as u16,
        start_pos,
        end_pos,
        IdentifierData { name: None },
    )
}

// =============================================================================
// Placeholder Functions (To be implemented)
// =============================================================================

/// Parse a function expression: function name? (...) { ... }
fn parse_function_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let start_pos = token_pos(ctx);
    expect_token(ctx, SyntaxKind::FunctionKeyword);

    // TODO: Parse function name, parameters, and body

    let end_pos = token_end(ctx);
    ctx.arena.add_function(
        syntax_kind_ext::FUNCTION_EXPRESSION,
        start_pos,
        end_pos,
        FunctionData {
            name: None,
            type_params: None,
            parameters: None,
            return_type: None,
            body: None,
        },
    )
}

/// Parse a class expression: class Name { ... }
fn parse_class_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let start_pos = token_pos(ctx);
    expect_token(ctx, SyntaxKind::ClassKeyword);

    // TODO: Parse class name, heritage clause, and members

    let end_pos = token_end(ctx);
    ctx.arena.add_identifier(
        syntax_kind_ext::CLASS_EXPRESSION,
        start_pos,
        end_pos,
        IdentifierData { name: None },
    )
}

/// Parse a template expression: `head ${expr} tail`
fn parse_template_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let start_pos = token_pos(ctx);

    let mut parts = Vec::new();
    while matches!(
        ctx.current_token,
        SyntaxKind::TemplateHead | SyntaxKind::TemplateMiddle
    ) {
        parts.push(parse_template_span(ctx));
    }

    let end_pos = token_end(ctx);

    ctx.arena.add_template_expr(
        syntax_kind_ext::TEMPLATE_EXPRESSION,
        start_pos,
        end_pos,
        TemplateExprData {
            parts: Some(parts.into()),
        },
    )
}

/// Parse a single template span: ${expression}
fn parse_template_span(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let start_pos = token_pos(ctx);
    next_token(ctx); // Consume template head/middle

    let expression = parse_expression(ctx);

    expect_token(ctx, SyntaxKind::CloseBraceToken);
    let end_pos = token_end(ctx);

    ctx.arena.add_template_span(
        syntax_kind_ext::TEMPLATE_SPAN,
        start_pos,
        end_pos,
        TemplateSpanData {
            expression: Some(expression),
            literal: None,
        },
    )
}

/// Parse an import expression: import(arg)
fn parse_import_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let start_pos = token_pos(ctx);
    expect_token(ctx, SyntaxKind::ImportKeyword);

    expect_token(ctx, SyntaxKind::OpenParenToken);
    let arg = parse_expression(ctx);
    expect_token(ctx, SyntaxKind::CloseParenToken);

    let end_pos = token_end(ctx);

    ctx.arena.add_call_expr(
        syntax_kind_ext::IMPORT_TYPE,
        start_pos,
        end_pos,
        CallExprData {
            expression: arg,
            type_args: None,
            args: None,
        },
    )
}

/// Parse a JSX expression (placeholder)
fn parse_jsx_expression(ctx: &mut ExpressionParseContext) -> NodeIndex {
    let start_pos = token_pos(ctx);
    // TODO: Implement full JSX parsing
    let end_pos = token_end(ctx);

    ctx.arena.add_identifier(
        syntax_kind_ext::JSX_ELEMENT,
        start_pos,
        end_pos,
        IdentifierData { name: None },
    )
}

/// Check if token starts JSX
fn is_jsx_start(token: SyntaxKind) -> bool {
    // Simplified check
    false
}

// =============================================================================
// Utility Functions (Context Wrappers)
// =============================================================================

fn token_pos(ctx: &mut ExpressionParseContext) -> u32 {
    ctx.scanner.token_pos()
}

fn token_end(ctx: &mut ExpressionParseContext) -> u32 {
    ctx.scanner.token_end()
}

fn next_token(ctx: &mut ExpressionParseContext) -> SyntaxKind {
    let token = ctx.scanner.next_token();
    ctx.current_token = token;
    token
}

fn is_token(ctx: &mut ExpressionParseContext, kind: SyntaxKind) -> bool {
    ctx.current_token == kind
}

fn expect_token(ctx: &mut ExpressionParseContext, kind: SyntaxKind) -> bool {
    if is_token(ctx, kind) {
        next_token(ctx);
        true
    } else {
        // TODO: Report error
        false
    }
}

fn scanner_has_preceding_line_break(ctx: &mut ExpressionParseContext) -> bool {
    ctx.scanner.has_preceding_line_break()
}

fn get_node_pos(ctx: &mut ExpressionParseContext, node: NodeIndex) -> u32 {
    ctx.arena.get_node_pos(node)
}
