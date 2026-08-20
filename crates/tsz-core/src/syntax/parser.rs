use crate::diagnostics::Diagnostic;
use crate::source::{NodeId, SourceText, Span};

use super::{
    ArrowBody, BinaryOperator, Expression, ExpressionKind, FunctionDeclaration,
    InterfaceDeclaration, KeywordType, Literal, ObjectProperty, Parameter, Statement,
    StatementKind, Token, TokenKind, TypeAliasDeclaration, TypeNode, TypeNodeKind, TypeProperty,
    VariableDeclaration, VariableKind, scan_source,
};

#[derive(Debug)]
pub struct ParseOutput {
    pub unit: super::SourceUnit,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse_source(source: &SourceText) -> ParseOutput {
    let scanned = scan_source(source);
    Parser::new(source, scanned.tokens, scanned.diagnostics).parse()
}

#[derive(Debug, Clone, Copy, Default)]
struct Modifiers {
    exported: bool,
    declared: bool,
    is_async: bool,
}

struct Parser<'a> {
    source: &'a SourceText,
    tokens: Vec<Token>,
    index: usize,
    next_node: u32,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    const fn new(source: &'a SourceText, tokens: Vec<Token>, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            source,
            tokens,
            index: 0,
            next_node: 0,
            diagnostics,
        }
    }

    fn parse(mut self) -> ParseOutput {
        let mut statements = Vec::new();
        while !self.at(TokenKind::EndOfFile) {
            let before = self.index;
            statements.push(self.parse_statement());
            if self.index == before {
                self.bump();
            }
        }
        let end = self.source.text.len();
        ParseOutput {
            unit: super::SourceUnit {
                statements,
                span: Span::new(self.source.id, 0, end),
            },
            diagnostics: self.diagnostics,
        }
    }

    fn parse_statement(&mut self) -> Statement {
        let start = self.current().span.start as usize;
        let modifiers = self.parse_modifiers();
        let kind = match self.kind() {
            TokenKind::Let | TokenKind::Const | TokenKind::Var => {
                StatementKind::Variable(self.parse_variable(modifiers.exported))
            }
            TokenKind::Function => StatementKind::Function(self.parse_function(modifiers)),
            TokenKind::Type => StatementKind::TypeAlias(self.parse_type_alias(modifiers.exported)),
            TokenKind::Interface => {
                StatementKind::Interface(self.parse_interface(modifiers.exported))
            }
            TokenKind::Return => {
                self.bump();
                let expression = if self.at_any(&[
                    TokenKind::Semicolon,
                    TokenKind::RightBrace,
                    TokenKind::EndOfFile,
                ]) {
                    None
                } else {
                    Some(self.parse_expression())
                };
                self.eat(TokenKind::Semicolon);
                StatementKind::Return(expression)
            }
            TokenKind::LeftBrace => StatementKind::Block(self.parse_block()),
            TokenKind::Semicolon => {
                self.bump();
                StatementKind::Empty
            }
            _ if modifiers.exported || modifiers.declared || modifiers.is_async => {
                self.error_current("Declaration expected.", 1146);
                self.recover_statement();
                StatementKind::Unknown
            }
            _ => {
                let expression = self.parse_expression();
                self.eat(TokenKind::Semicolon);
                StatementKind::Expression(expression)
            }
        };
        let end = self.previous_end().max(start);
        Statement {
            id: self.alloc_node(),
            span: Span::new(self.source.id, start, end),
            kind,
        }
    }

    fn parse_modifiers(&mut self) -> Modifiers {
        let mut modifiers = Modifiers::default();
        loop {
            match self.kind() {
                TokenKind::Export => {
                    modifiers.exported = true;
                    self.bump();
                    self.eat(TokenKind::Default);
                }
                TokenKind::Declare => {
                    modifiers.declared = true;
                    self.bump();
                }
                TokenKind::Async => {
                    modifiers.is_async = true;
                    self.bump();
                }
                _ => break,
            }
        }
        modifiers
    }

    fn parse_variable(&mut self, exported: bool) -> VariableDeclaration {
        let declaration_kind = match self.kind() {
            TokenKind::Const => VariableKind::Const,
            TokenKind::Var => VariableKind::Var,
            _ => VariableKind::Let,
        };
        self.bump();
        let (name, name_span) = self.parse_name();
        let annotation = self.eat(TokenKind::Colon).then(|| self.parse_type());
        let initializer = self.eat(TokenKind::Equals).then(|| self.parse_expression());
        self.eat(TokenKind::Semicolon);
        VariableDeclaration {
            declaration_kind,
            name,
            name_span,
            annotation,
            initializer,
            exported,
        }
    }

    fn parse_function(&mut self, modifiers: Modifiers) -> FunctionDeclaration {
        self.expect(TokenKind::Function, "'function' expected.", 1005);
        let (name, name_span) = self.parse_name();
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameters();
        let return_type = self.eat(TokenKind::Colon).then(|| self.parse_type());
        let body = if self.at(TokenKind::LeftBrace) {
            self.parse_block()
        } else {
            self.eat(TokenKind::Semicolon);
            Vec::new()
        };
        FunctionDeclaration {
            name,
            name_span,
            type_parameters,
            parameters,
            return_type,
            body,
            exported: modifiers.exported,
            is_async: modifiers.is_async,
            declared: modifiers.declared,
        }
    }

    fn parse_type_alias(&mut self, exported: bool) -> TypeAliasDeclaration {
        self.bump();
        let (name, name_span) = self.parse_name();
        let type_parameters = self.parse_type_parameters();
        self.expect(TokenKind::Equals, "'=' expected.", 1005);
        let ty = self.parse_type();
        self.eat(TokenKind::Semicolon);
        TypeAliasDeclaration {
            name,
            name_span,
            type_parameters,
            ty,
            exported,
        }
    }

    fn parse_interface(&mut self, exported: bool) -> InterfaceDeclaration {
        self.bump();
        let (name, name_span) = self.parse_name();
        let type_parameters = self.parse_type_parameters();
        let properties = self.parse_type_members();
        InterfaceDeclaration {
            name,
            name_span,
            type_parameters,
            properties,
            exported,
        }
    }

    fn parse_block(&mut self) -> Vec<Statement> {
        self.expect(TokenKind::LeftBrace, "'{' expected.", 1005);
        let mut statements = Vec::new();
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            let before = self.index;
            statements.push(self.parse_statement());
            if before == self.index {
                self.bump();
            }
        }
        self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        statements
    }

    fn parse_parameters(&mut self) -> Vec<Parameter> {
        self.expect(TokenKind::LeftParen, "'(' expected.", 1005);
        let mut parameters = Vec::new();
        while !self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile]) {
            parameters.push(self.parse_parameter());
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RightParen, "')' expected.", 1005);
        parameters
    }

    fn parse_parameter(&mut self) -> Parameter {
        let start = self.current().span.start as usize;
        let (name, name_span) = self.parse_name();
        let optional = self.eat(TokenKind::Question);
        let annotation = self.eat(TokenKind::Colon).then(|| self.parse_type());
        let end = self.previous_end().max(start);
        Parameter {
            name,
            name_span,
            annotation,
            optional,
            span: Span::new(self.source.id, start, end),
        }
    }

    fn parse_type_parameters(&mut self) -> Vec<String> {
        if !self.eat(TokenKind::LessThan) {
            return Vec::new();
        }
        let mut parameters = Vec::new();
        while !self.at_any(&[TokenKind::GreaterThan, TokenKind::EndOfFile]) {
            let (name, _) = self.parse_name();
            parameters.push(name);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::GreaterThan, "'>' expected.", 1005);
        parameters
    }

    fn parse_type(&mut self) -> TypeNode {
        self.parse_union_type()
    }

    fn parse_union_type(&mut self) -> TypeNode {
        let first = self.parse_intersection_type();
        if !self.eat(TokenKind::Bar) {
            return first;
        }
        let start = first.span;
        let mut members = vec![first];
        loop {
            members.push(self.parse_intersection_type());
            if !self.eat(TokenKind::Bar) {
                break;
            }
        }
        let end = members.last().map_or(start, |member| member.span);
        TypeNode {
            span: start.merge(end),
            kind: TypeNodeKind::Union(members),
        }
    }

    fn parse_intersection_type(&mut self) -> TypeNode {
        let first = self.parse_postfix_type();
        if !self.eat(TokenKind::Ampersand) {
            return first;
        }
        let start = first.span;
        let mut members = vec![first];
        loop {
            members.push(self.parse_postfix_type());
            if !self.eat(TokenKind::Ampersand) {
                break;
            }
        }
        let end = members.last().map_or(start, |member| member.span);
        TypeNode {
            span: start.merge(end),
            kind: TypeNodeKind::Intersection(members),
        }
    }

    fn parse_postfix_type(&mut self) -> TypeNode {
        let mut ty = self.parse_primary_type();
        loop {
            if self.at(TokenKind::LeftBracket) && self.peek_kind(1) == TokenKind::RightBracket {
                self.bump();
                let right = self.bump().span;
                let span = ty.span.merge(right);
                ty = TypeNode {
                    span,
                    kind: TypeNodeKind::Array(Box::new(ty)),
                };
            } else if self.eat(TokenKind::LeftBracket) {
                let index = self.parse_type();
                let right = self.current().span;
                self.expect(TokenKind::RightBracket, "']' expected.", 1005);
                let span = ty.span.merge(right);
                ty = TypeNode {
                    span,
                    kind: TypeNodeKind::IndexedAccess {
                        object: Box::new(ty),
                        index: Box::new(index),
                    },
                };
            } else {
                break;
            }
        }
        ty
    }

    fn parse_primary_type(&mut self) -> TypeNode {
        let token = *self.current();
        let keyword = match token.kind {
            TokenKind::Any => Some(KeywordType::Any),
            TokenKind::Unknown => Some(KeywordType::Unknown),
            TokenKind::Never => Some(KeywordType::Never),
            TokenKind::Void => Some(KeywordType::Void),
            TokenKind::Undefined => Some(KeywordType::Undefined),
            TokenKind::Null => Some(KeywordType::Null),
            TokenKind::Boolean => Some(KeywordType::Boolean),
            TokenKind::Number => Some(KeywordType::Number),
            TokenKind::String => Some(KeywordType::String),
            TokenKind::BigInt => Some(KeywordType::BigInt),
            _ => None,
        };
        if let Some(keyword) = keyword {
            self.bump();
            return TypeNode {
                span: token.span,
                kind: TypeNodeKind::Keyword(keyword),
            };
        }
        match token.kind {
            TokenKind::True
            | TokenKind::False
            | TokenKind::NumericLiteral
            | TokenKind::StringLiteral => {
                self.bump();
                TypeNode {
                    span: token.span,
                    kind: TypeNodeKind::Literal(self.literal_from(token)),
                }
            }
            TokenKind::KeyOf => {
                self.bump();
                let operand = self.parse_primary_type();
                TypeNode {
                    span: token.span.merge(operand.span),
                    kind: TypeNodeKind::KeyOf(Box::new(operand)),
                }
            }
            TokenKind::Identifier => {
                self.bump();
                let name = self.text(token.span).to_string();
                let arguments = self.parse_type_arguments();
                let end = arguments
                    .last()
                    .map_or(token.span, |argument| argument.span);
                TypeNode {
                    span: token.span.merge(end),
                    kind: TypeNodeKind::Reference {
                        name,
                        name_span: token.span,
                        arguments,
                    },
                }
            }
            TokenKind::LeftBrace => {
                let properties = self.parse_type_members();
                TypeNode {
                    span: token.span.merge(self.previous().span),
                    kind: TypeNodeKind::Object(properties),
                }
            }
            TokenKind::LeftBracket => {
                self.bump();
                let mut members = Vec::new();
                while !self.at_any(&[TokenKind::RightBracket, TokenKind::EndOfFile]) {
                    members.push(self.parse_type());
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                let end = self.current().span;
                self.expect(TokenKind::RightBracket, "']' expected.", 1005);
                TypeNode {
                    span: token.span.merge(end),
                    kind: TypeNodeKind::Tuple(members),
                }
            }
            TokenKind::LeftParen => self.parse_parenthesized_or_function_type(),
            _ => {
                self.error_current("Type expected.", 1110);
                self.bump();
                TypeNode {
                    span: token.span,
                    kind: TypeNodeKind::Missing,
                }
            }
        }
    }

    fn parse_parenthesized_or_function_type(&mut self) -> TypeNode {
        let left = self.bump().span;
        if self.paren_is_parameter_list() {
            let mut parameters = Vec::new();
            while !self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile]) {
                parameters.push(self.parse_parameter());
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RightParen, "')' expected.", 1005);
            self.expect(TokenKind::FatArrow, "'=>' expected.", 1005);
            let return_type = self.parse_type();
            return TypeNode {
                span: left.merge(return_type.span),
                kind: TypeNodeKind::Function {
                    parameters,
                    return_type: Box::new(return_type),
                },
            };
        }
        let inner = self.parse_type();
        let right = self.current().span;
        self.expect(TokenKind::RightParen, "')' expected.", 1005);
        TypeNode {
            span: left.merge(right),
            kind: TypeNodeKind::Parenthesized(Box::new(inner)),
        }
    }

    fn paren_is_parameter_list(&self) -> bool {
        let mut depth = 1_u32;
        let mut cursor = self.index;
        while let Some(token) = self.tokens.get(cursor) {
            match token.kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        return self.tokens.get(cursor + 1).map(|token| token.kind)
                            == Some(TokenKind::FatArrow);
                    }
                }
                TokenKind::EndOfFile => return false,
                _ => {}
            }
            cursor += 1;
        }
        false
    }

    fn parse_type_members(&mut self) -> Vec<TypeProperty> {
        self.expect(TokenKind::LeftBrace, "'{' expected.", 1005);
        let mut properties = Vec::new();
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            let start = self.current().span;
            let readonly = self.at(TokenKind::Identifier) && self.current_text() == "readonly";
            if readonly {
                self.bump();
            }
            let (name, name_span) = self.parse_property_name();
            let optional = self.eat(TokenKind::Question);
            self.expect(TokenKind::Colon, "':' expected.", 1005);
            let ty = self.parse_type();
            let span = start.merge(ty.span);
            properties.push(TypeProperty {
                name,
                name_span,
                ty,
                optional,
                readonly,
                span,
            });
            if !self.eat(TokenKind::Semicolon) {
                self.eat(TokenKind::Comma);
            }
        }
        self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        properties
    }

    fn parse_type_arguments(&mut self) -> Vec<TypeNode> {
        if !self.eat(TokenKind::LessThan) {
            return Vec::new();
        }
        let mut arguments = Vec::new();
        while !self.at_any(&[TokenKind::GreaterThan, TokenKind::EndOfFile]) {
            arguments.push(self.parse_type());
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::GreaterThan, "'>' expected.", 1005);
        arguments
    }

    fn parse_expression(&mut self) -> Expression {
        self.parse_assignment_expression()
    }

    fn parse_assignment_expression(&mut self) -> Expression {
        let left = self.parse_additive_expression();
        if self.eat(TokenKind::Equals) {
            let right = self.parse_assignment_expression();
            let span = left.span.merge(right.span);
            Expression {
                id: self.alloc_node(),
                span,
                kind: ExpressionKind::Assignment {
                    left: Box::new(left),
                    right: Box::new(right),
                },
            }
        } else {
            left
        }
    }

    fn parse_additive_expression(&mut self) -> Expression {
        let mut expression = self.parse_multiplicative_expression();
        while self.at_any(&[TokenKind::Plus, TokenKind::Minus]) {
            let operator = if self.eat(TokenKind::Plus) {
                BinaryOperator::Add
            } else {
                self.bump();
                BinaryOperator::Subtract
            };
            let right = self.parse_multiplicative_expression();
            let span = expression.span.merge(right.span);
            expression = Expression {
                id: self.alloc_node(),
                span,
                kind: ExpressionKind::Binary {
                    left: Box::new(expression),
                    operator,
                    right: Box::new(right),
                },
            };
        }
        expression
    }

    fn parse_multiplicative_expression(&mut self) -> Expression {
        let mut expression = self.parse_postfix_expression();
        while self.at_any(&[TokenKind::Star, TokenKind::Slash]) {
            let operator = if self.eat(TokenKind::Star) {
                BinaryOperator::Multiply
            } else {
                self.bump();
                BinaryOperator::Divide
            };
            let right = self.parse_postfix_expression();
            let span = expression.span.merge(right.span);
            expression = Expression {
                id: self.alloc_node(),
                span,
                kind: ExpressionKind::Binary {
                    left: Box::new(expression),
                    operator,
                    right: Box::new(right),
                },
            };
        }
        expression
    }

    fn parse_postfix_expression(&mut self) -> Expression {
        let mut expression = self.parse_primary_expression();
        loop {
            if self.eat(TokenKind::LeftParen) {
                let mut arguments = Vec::new();
                while !self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile]) {
                    arguments.push(self.parse_expression());
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                let right = self.current().span;
                self.expect(TokenKind::RightParen, "')' expected.", 1005);
                let span = expression.span.merge(right);
                expression = Expression {
                    id: self.alloc_node(),
                    span,
                    kind: ExpressionKind::Call {
                        callee: Box::new(expression),
                        arguments,
                    },
                };
            } else if self.eat(TokenKind::Dot) {
                let (name, name_span) = self.parse_name();
                let span = expression.span.merge(name_span);
                expression = Expression {
                    id: self.alloc_node(),
                    span,
                    kind: ExpressionKind::Member {
                        object: Box::new(expression),
                        name,
                        name_span,
                    },
                };
            } else if self.eat(TokenKind::As) {
                let ty = self.parse_type();
                let span = expression.span.merge(ty.span);
                expression = Expression {
                    id: self.alloc_node(),
                    span,
                    kind: ExpressionKind::As {
                        expression: Box::new(expression),
                        ty,
                    },
                };
            } else {
                break;
            }
        }
        expression
    }

    fn parse_primary_expression(&mut self) -> Expression {
        let token = *self.current();
        match token.kind {
            TokenKind::Identifier | TokenKind::Undefined => {
                self.bump();
                let name = self.text(token.span).to_string();
                if self.eat(TokenKind::FatArrow) {
                    let parameter = Parameter {
                        name,
                        name_span: token.span,
                        annotation: None,
                        optional: false,
                        span: token.span,
                    };
                    let body = self.parse_arrow_body();
                    let end = self.previous().span;
                    return Expression {
                        id: self.alloc_node(),
                        span: token.span.merge(end),
                        kind: ExpressionKind::Arrow {
                            parameters: vec![parameter],
                            body,
                        },
                    };
                }
                Expression {
                    id: self.alloc_node(),
                    span: token.span,
                    kind: ExpressionKind::Identifier {
                        name,
                        name_span: token.span,
                    },
                }
            }
            TokenKind::True
            | TokenKind::False
            | TokenKind::Null
            | TokenKind::NumericLiteral
            | TokenKind::StringLiteral => {
                self.bump();
                Expression {
                    id: self.alloc_node(),
                    span: token.span,
                    kind: ExpressionKind::Literal(self.literal_from(token)),
                }
            }
            TokenKind::LeftBrace => self.parse_object_literal(),
            TokenKind::LeftBracket => self.parse_array_literal(),
            TokenKind::LeftParen if self.paren_expression_is_arrow() => {
                let left = self.bump().span;
                let mut parameters = Vec::new();
                while !self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile]) {
                    parameters.push(self.parse_parameter());
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RightParen, "')' expected.", 1005);
                self.expect(TokenKind::FatArrow, "'=>' expected.", 1005);
                let body = self.parse_arrow_body();
                let end = self.previous().span;
                Expression {
                    id: self.alloc_node(),
                    span: left.merge(end),
                    kind: ExpressionKind::Arrow { parameters, body },
                }
            }
            TokenKind::LeftParen => {
                let left = self.bump().span;
                let inner = self.parse_expression();
                let right = self.current().span;
                self.expect(TokenKind::RightParen, "')' expected.", 1005);
                Expression {
                    id: self.alloc_node(),
                    span: left.merge(right),
                    kind: ExpressionKind::Parenthesized(Box::new(inner)),
                }
            }
            _ => {
                self.error_current("Expression expected.", 1109);
                self.bump();
                Expression {
                    id: self.alloc_node(),
                    span: token.span,
                    kind: ExpressionKind::Missing,
                }
            }
        }
    }

    fn parse_arrow_body(&mut self) -> ArrowBody {
        if self.at(TokenKind::LeftBrace) {
            ArrowBody::Block(self.parse_block())
        } else {
            ArrowBody::Expression(Box::new(self.parse_expression()))
        }
    }

    fn paren_expression_is_arrow(&self) -> bool {
        if !self.at(TokenKind::LeftParen) {
            return false;
        }
        let mut depth = 0_u32;
        for (cursor, token) in self.tokens.iter().enumerate().skip(self.index) {
            match token.kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return self.tokens.get(cursor + 1).map(|token| token.kind)
                            == Some(TokenKind::FatArrow);
                    }
                }
                TokenKind::EndOfFile => break,
                _ => {}
            }
        }
        false
    }

    fn parse_object_literal(&mut self) -> Expression {
        let left = self.bump().span;
        let mut properties = Vec::new();
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            let start = self.current().span;
            let (name, name_span) = self.parse_property_name();
            let value = if self.eat(TokenKind::Colon) {
                self.parse_expression()
            } else {
                Expression {
                    id: self.alloc_node(),
                    span: name_span,
                    kind: ExpressionKind::Identifier {
                        name: name.clone(),
                        name_span,
                    },
                }
            };
            let span = start.merge(value.span);
            properties.push(ObjectProperty {
                name,
                name_span,
                value,
                span,
            });
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let right = self.current().span;
        self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        Expression {
            id: self.alloc_node(),
            span: left.merge(right),
            kind: ExpressionKind::Object(properties),
        }
    }

    fn parse_array_literal(&mut self) -> Expression {
        let left = self.bump().span;
        let mut elements = Vec::new();
        while !self.at_any(&[TokenKind::RightBracket, TokenKind::EndOfFile]) {
            elements.push(self.parse_expression());
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let right = self.current().span;
        self.expect(TokenKind::RightBracket, "']' expected.", 1005);
        Expression {
            id: self.alloc_node(),
            span: left.merge(right),
            kind: ExpressionKind::Array(elements),
        }
    }

    fn parse_name(&mut self) -> (String, Span) {
        let token = *self.current();
        if token.kind == TokenKind::Identifier {
            self.bump();
            (self.text(token.span).to_string(), token.span)
        } else {
            self.error_current("Identifier expected.", 1003);
            self.bump();
            ("<missing>".to_string(), token.span)
        }
    }

    fn parse_property_name(&mut self) -> (String, Span) {
        let token = *self.current();
        match token.kind {
            TokenKind::Identifier | TokenKind::StringLiteral | TokenKind::NumericLiteral => {
                self.bump();
                let text = self.text(token.span);
                let name = if token.kind == TokenKind::StringLiteral {
                    unquote(text)
                } else {
                    text.to_string()
                };
                (name, token.span)
            }
            _ => self.parse_name(),
        }
    }

    fn literal_from(&self, token: Token) -> Literal {
        match token.kind {
            TokenKind::True => Literal::Boolean(true),
            TokenKind::False => Literal::Boolean(false),
            TokenKind::Null => Literal::Null,
            TokenKind::StringLiteral => Literal::String(unquote(self.text(token.span))),
            _ => Literal::Number(self.text(token.span).to_string()),
        }
    }

    fn recover_statement(&mut self) {
        while !self.at_any(&[
            TokenKind::Semicolon,
            TokenKind::RightBrace,
            TokenKind::EndOfFile,
        ]) {
            self.bump();
        }
        self.eat(TokenKind::Semicolon);
    }

    fn error_current(&mut self, message: &str, code: u32) {
        self.diagnostics.push(Diagnostic::at(
            self.source,
            self.current().span,
            message.to_string(),
            code,
        ));
    }

    fn expect(&mut self, kind: TokenKind, message: &str, code: u32) -> bool {
        if self.eat(kind) {
            true
        } else {
            self.error_current(message, code);
            false
        }
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.kind() == kind
    }

    fn at_any(&self, kinds: &[TokenKind]) -> bool {
        kinds.contains(&self.kind())
    }

    fn kind(&self) -> TokenKind {
        self.current().kind
    }

    fn peek_kind(&self, distance: usize) -> TokenKind {
        self.tokens
            .get(self.index + distance)
            .map_or(TokenKind::EndOfFile, |token| token.kind)
    }

    fn current(&self) -> &Token {
        &self.tokens[self.index.min(self.tokens.len() - 1)]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.index.saturating_sub(1)]
    }

    fn current_text(&self) -> &str {
        self.text(self.current().span)
    }

    fn text(&self, span: Span) -> &str {
        self.source.slice(span)
    }

    fn bump(&mut self) -> Token {
        let token = *self.current();
        if token.kind != TokenKind::EndOfFile {
            self.index += 1;
        }
        token
    }

    fn previous_end(&self) -> usize {
        self.previous().span.end as usize
    }

    const fn alloc_node(&mut self) -> NodeId {
        let id = NodeId(self.next_node);
        self.next_node += 1;
        id
    }
}

fn unquote(text: &str) -> String {
    if text.len() >= 2 {
        let first = text.as_bytes()[0];
        let last = text.as_bytes()[text.len() - 1];
        if (first == b'\'' || first == b'"') && first == last {
            return text[1..text.len() - 1].to_string();
        }
    }
    text.to_string()
}
