use super::Parser;
use crate::syntax::{
    AuthoredBindingName, ParserRecoveryKind, TokenKind, VariableDeclaration, VariableKind,
};

impl Parser<'_> {
    pub(super) fn starts_import_declaration(&self) -> bool {
        self.at(TokenKind::Import)
            && !matches!(
                self.peek_kind(1),
                TokenKind::LeftParen | TokenKind::LessThan | TokenKind::Dot
            )
    }

    pub(super) fn starts_type_alias_declaration(&self) -> bool {
        self.at(TokenKind::Type)
            && self.peek_kind(1).is_identifier()
            && self.tokens_are_on_same_line(self.index, self.index + 1)
    }

    pub(super) fn parse_variable(&mut self, exported: bool) -> VariableDeclaration {
        let declaration_kind = match self.kind() {
            TokenKind::Const => VariableKind::Const,
            TokenKind::Var => VariableKind::Var,
            _ => VariableKind::Let,
        };
        self.bump();
        let binding_start = self.current().span;
        let modeled_binding = self.kind().is_identifier();
        let recovered_binding_names = self.recovered_binding_names(self.index);
        let (name, name_span) = self.parse_recovered_binding_head();
        self.retain_variable_declaration_recovery(
            binding_start,
            name_span,
            modeled_binding,
            !recovered_binding_names.is_empty(),
        );
        let annotation = self.eat(TokenKind::Colon).then(|| self.parse_type());
        let initializer = self.eat(TokenKind::Equals).then(|| self.parse_expression());
        if initializer.as_ref().is_some_and(|expression| {
            self.expression_starts_rejected_generic_arrow_prefix(expression.span)
        }) && self.eat(TokenKind::Comma)
        {
            self.error_current("Variable declaration expected.", 1134);
            self.bump();
        }
        if initializer.as_ref().is_some_and(|expression| {
            matches!(expression.kind, crate::syntax::ExpressionKind::As { .. })
                && !self.expression_starts_rejected_generic_arrow_prefix(expression.span)
        }) && !self.at_any(&[
            TokenKind::Semicolon,
            TokenKind::RightBrace,
            TokenKind::EndOfFile,
        ]) && self.tokens_are_on_same_line(self.index.saturating_sub(1), self.index)
        {
            let authored_span = self.current().span;
            let recovery_extent = self.recovery_extent_from_current(authored_span);
            self.retain_parser_recovery(
                ParserRecoveryKind::Declaration,
                authored_span,
                recovery_extent,
            );
            self.record_parser_recovery_for_analysis(
                ParserRecoveryKind::VariableDeclaratorTail,
                authored_span,
                recovery_extent,
            );
            self.recover_statement(Some(recovery_extent));
        }
        self.eat(TokenKind::Semicolon);
        VariableDeclaration {
            declaration_kind,
            name,
            name_span,
            recovered_binding_names,
            annotation,
            initializer,
            exported,
        }
    }

    /// Retain a structurally closed binding head without claiming its model.
    /// The authored names and declaration recovery remain owned by the side scan.
    pub(super) fn parse_recovered_binding_head(&mut self) -> (String, crate::source::Span) {
        let opening = *self.current();
        let closing_kind = match opening.kind {
            TokenKind::LeftBrace => TokenKind::RightBrace,
            TokenKind::LeftBracket => TokenKind::RightBracket,
            _ => return self.parse_name(),
        };
        let first = self.peek_kind(1);
        let valid_first = match opening.kind {
            TokenKind::LeftBrace => {
                matches!(
                    first,
                    TokenKind::RightBrace
                        | TokenKind::DotDotDot
                        | TokenKind::LeftBracket
                        | TokenKind::StringLiteral
                        | TokenKind::NumericLiteral
                        | TokenKind::BigIntLiteral
                ) || first.is_identifier_name()
            }
            TokenKind::LeftBracket => {
                matches!(
                    first,
                    TokenKind::RightBracket
                        | TokenKind::Comma
                        | TokenKind::DotDotDot
                        | TokenKind::LeftBrace
                        | TokenKind::LeftBracket
                ) || first.is_identifier()
            }
            _ => unreachable!(),
        };
        if !valid_first {
            return self.parse_name();
        }
        let mut cursor = self.index;
        let mut ignored_names = Vec::new();
        self.scan_binding_target(&mut cursor, &mut ignored_names);
        if cursor <= self.index + 1 || self.tokens[cursor - 1].kind != closing_kind {
            return self.parse_name();
        }
        let closing = self.tokens[cursor - 1].span;
        self.index = cursor;
        ("<missing>".to_string(), opening.span.merge(closing))
    }

    pub(super) fn recovered_binding_names_in_target(
        &self,
        start: usize,
    ) -> Vec<AuthoredBindingName> {
        if self.tokens[start].kind == TokenKind::EndOfFile
            || !matches!(
                self.tokens[start].kind,
                TokenKind::LeftBrace | TokenKind::LeftBracket
            )
        {
            return Vec::new();
        }
        let mut names = Vec::new();
        let mut cursor = start;
        self.scan_binding_target(&mut cursor, &mut names);
        names
    }

    fn recovered_binding_names(&self, start: usize) -> Vec<AuthoredBindingName> {
        if self.tokens[start].kind == TokenKind::EndOfFile {
            return Vec::new();
        }
        let binding_pattern = matches!(
            self.tokens[start].kind,
            TokenKind::LeftBrace | TokenKind::LeftBracket
        );
        let mut names = Vec::new();
        let mut cursor = start;
        let mut recovered_declarator = false;
        loop {
            self.scan_binding_target(&mut cursor, &mut names);
            if !self.scan_to_next_variable_declarator(&mut cursor) {
                break;
            }
            recovered_declarator = true;
        }
        if binding_pattern || recovered_declarator {
            names
        } else {
            Vec::new()
        }
    }

    /// Retain the binding identity at each authored variable-list separator.
    /// The statement model currently represents only the first declarator, so
    /// commas nested in an annotation or initializer must not manufacture a
    /// peer declaration.
    fn scan_to_next_variable_declarator(&self, cursor: &mut usize) -> bool {
        let mut depth = 0_u32;
        let mut type_argument_depth = 0_u32;
        let mut in_type_annotation = false;
        while let Some(token) = self.tokens.get(*cursor) {
            let kind = token.kind;
            if kind == TokenKind::EndOfFile {
                break;
            }
            if depth == 0 {
                if kind == TokenKind::Comma
                    && type_argument_depth == 0
                    && self.comma_starts_variable_declarator(*cursor)
                {
                    *cursor += 1;
                    return true;
                }
                if matches!(
                    kind,
                    TokenKind::Semicolon
                        | TokenKind::RightBrace
                        | TokenKind::RightParen
                        | TokenKind::In
                        | TokenKind::Of
                ) {
                    return false;
                }
                match kind {
                    TokenKind::Colon if type_argument_depth == 0 => in_type_annotation = true,
                    TokenKind::Equals if type_argument_depth == 0 => in_type_annotation = false,
                    TokenKind::LessThan if in_type_annotation => type_argument_depth += 1,
                    TokenKind::GreaterThan
                    | TokenKind::GreaterThanEquals
                    | TokenKind::GreaterThanGreaterThan
                    | TokenKind::GreaterThanGreaterThanEquals
                    | TokenKind::GreaterThanGreaterThanGreaterThan
                    | TokenKind::GreaterThanGreaterThanGreaterThanEquals
                        if in_type_annotation && type_argument_depth > 0 =>
                    {
                        let close_count = match kind {
                            TokenKind::GreaterThan | TokenKind::GreaterThanEquals => 1,
                            TokenKind::GreaterThanGreaterThan
                            | TokenKind::GreaterThanGreaterThanEquals => 2,
                            TokenKind::GreaterThanGreaterThanGreaterThan
                            | TokenKind::GreaterThanGreaterThanGreaterThanEquals => 3,
                            _ => unreachable!(),
                        };
                        type_argument_depth = type_argument_depth.saturating_sub(close_count);
                        if matches!(
                            kind,
                            TokenKind::GreaterThanEquals
                                | TokenKind::GreaterThanGreaterThanEquals
                                | TokenKind::GreaterThanGreaterThanGreaterThanEquals
                        ) && type_argument_depth == 0
                        {
                            in_type_annotation = false;
                        }
                    }
                    _ => {}
                }
            }
            match kind {
                TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace => depth += 1,
                TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace
                    if depth > 0 =>
                {
                    depth -= 1
                }
                _ => {}
            }
            *cursor += 1;
        }
        false
    }

    fn comma_starts_variable_declarator(&self, comma: usize) -> bool {
        let mut cursor = comma + 1;
        if self
            .tokens
            .get(cursor)
            .is_none_or(|token| token.kind == TokenKind::EndOfFile)
        {
            return false;
        }
        let mut names = Vec::new();
        self.scan_binding_target(&mut cursor, &mut names);
        !names.is_empty()
            && matches!(
                self.tokens
                    .get(cursor)
                    .map_or(TokenKind::EndOfFile, |token| token.kind),
                TokenKind::Colon
                    | TokenKind::Equals
                    | TokenKind::Bang
                    | TokenKind::Comma
                    | TokenKind::Semicolon
                    | TokenKind::RightBrace
                    | TokenKind::RightParen
                    | TokenKind::In
                    | TokenKind::Of
                    | TokenKind::EndOfFile
            )
    }

    fn scan_binding_target(&self, cursor: &mut usize, names: &mut Vec<AuthoredBindingName>) {
        let Some(token) = self.tokens.get(*cursor) else {
            return;
        };
        match token.kind {
            TokenKind::LeftBrace => self.scan_object_binding(cursor, names),
            TokenKind::LeftBracket => self.scan_array_binding(cursor, names),
            kind if kind.is_identifier() => {
                let token = *token;
                names.push(AuthoredBindingName {
                    name: self.text(token.span).to_string(),
                    span: token.span,
                    token_kind: token.kind,
                });
                *cursor += 1;
            }
            TokenKind::EndOfFile => {}
            _ => *cursor += 1,
        }
    }

    fn scan_object_binding(&self, cursor: &mut usize, names: &mut Vec<AuthoredBindingName>) {
        *cursor += 1;
        while !matches!(
            self.tokens[*cursor].kind,
            TokenKind::RightBrace | TokenKind::EndOfFile
        ) {
            if self.tokens[*cursor].kind == TokenKind::Comma {
                *cursor += 1;
                continue;
            }
            if self.tokens[*cursor].kind == TokenKind::DotDotDot {
                *cursor += 1;
                self.scan_binding_target(cursor, names);
            } else if self.tokens[*cursor].kind == TokenKind::LeftBracket {
                self.skip_balanced_binding_part(
                    cursor,
                    TokenKind::LeftBracket,
                    TokenKind::RightBracket,
                );
                if self.tokens[*cursor].kind == TokenKind::Colon {
                    *cursor += 1;
                    self.scan_binding_target(cursor, names);
                }
            } else {
                let property = self.tokens[*cursor];
                *cursor += 1;
                if self.tokens[*cursor].kind == TokenKind::Colon {
                    *cursor += 1;
                    self.scan_binding_target(cursor, names);
                } else if property.kind.is_identifier() {
                    names.push(AuthoredBindingName {
                        name: self.text(property.span).to_string(),
                        span: property.span,
                        token_kind: property.kind,
                    });
                }
            }
            if self.tokens[*cursor].kind == TokenKind::Equals {
                *cursor += 1;
                self.skip_binding_initializer(cursor, TokenKind::RightBrace);
            }
        }
        if self.tokens[*cursor].kind == TokenKind::RightBrace {
            *cursor += 1;
        }
    }

    fn scan_array_binding(&self, cursor: &mut usize, names: &mut Vec<AuthoredBindingName>) {
        *cursor += 1;
        while !matches!(
            self.tokens[*cursor].kind,
            TokenKind::RightBracket | TokenKind::EndOfFile
        ) {
            if self.tokens[*cursor].kind == TokenKind::Comma {
                *cursor += 1;
                continue;
            }
            if self.tokens[*cursor].kind == TokenKind::DotDotDot {
                *cursor += 1;
            }
            self.scan_binding_target(cursor, names);
            if self.tokens[*cursor].kind == TokenKind::Equals {
                *cursor += 1;
                self.skip_binding_initializer(cursor, TokenKind::RightBracket);
            }
        }
        if self.tokens[*cursor].kind == TokenKind::RightBracket {
            *cursor += 1;
        }
    }

    fn skip_balanced_binding_part(&self, cursor: &mut usize, open: TokenKind, close: TokenKind) {
        let mut depth = 0_u32;
        while self.tokens[*cursor].kind != TokenKind::EndOfFile {
            let kind = self.tokens[*cursor].kind;
            *cursor += 1;
            if kind == open {
                depth += 1;
            } else if kind == close {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
        }
    }

    fn skip_binding_initializer(&self, cursor: &mut usize, owner_close: TokenKind) {
        let mut depth = 0_u32;
        loop {
            let kind = self.tokens[*cursor].kind;
            if kind == TokenKind::EndOfFile
                || depth == 0 && matches!(kind, TokenKind::Comma)
                || depth == 0 && kind == owner_close
            {
                break;
            }
            match kind {
                TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace => depth += 1,
                TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace
                    if depth > 0 =>
                {
                    depth -= 1
                }
                _ => {}
            }
            *cursor += 1;
        }
    }

    fn retain_variable_declaration_recovery(
        &mut self,
        binding_start: crate::source::Span,
        name_span: crate::source::Span,
        modeled_binding: bool,
        has_recovered_binding_names: bool,
    ) {
        let tail_requires_recovery = !self.at_any(&[
            TokenKind::Colon,
            TokenKind::Equals,
            TokenKind::Semicolon,
            TokenKind::RightBrace,
            TokenKind::EndOfFile,
        ]) && self
            .tokens_are_on_same_line(self.index.saturating_sub(1), self.index);
        if modeled_binding && !tail_requires_recovery && !has_recovered_binding_names {
            return;
        }
        let authored_span = if modeled_binding {
            name_span
        } else {
            binding_start
        };
        let recovery_extent = self.recovery_extent_from_current(authored_span);
        self.retain_parser_recovery(
            ParserRecoveryKind::Declaration,
            authored_span,
            recovery_extent,
        );
    }
}
