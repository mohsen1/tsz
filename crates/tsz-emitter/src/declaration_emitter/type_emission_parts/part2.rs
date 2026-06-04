impl<'a> DeclarationEmitter<'a> {
    fn current_output_is_type_parameter_constraint(&self) -> bool {
        let output = self.writer.get_output();
        let mut balance = 0i32;
        let mut candidate_start = None;
        for (idx, ch) in output.char_indices().rev() {
            match ch {
                '>' => balance += 1,
                '<' if balance == 0 => {
                    candidate_start = Some(idx);
                    break;
                }
                '<' => balance -= 1,
                _ => {}
            }
        }

        let Some(start) = candidate_start else {
            return false;
        };
        let tail = &output[start..];
        tail.contains(" extends ") && !tail.contains('\n')
    }

    fn emit_mapped_type_inline(&mut self, mapped_type: &tsz_parser::parser::node::MappedTypeData) {
        self.write("{ ");

        if let Some(readonly_node) = self.arena.get(mapped_type.readonly_token) {
            match readonly_node.kind {
                k if k == SyntaxKind::PlusToken as u16 => self.write("+readonly "),
                k if k == SyntaxKind::MinusToken as u16 => self.write("-readonly "),
                _ => self.write("readonly "),
            }
        }

        self.write("[");
        if let Some(type_param_node) = self.arena.get(mapped_type.type_parameter)
            && let Some(type_param) = self.arena.get_type_parameter(type_param_node)
        {
            self.emit_node(type_param.name);
            self.write(" in ");
            if type_param.constraint.is_some() {
                self.emit_mapped_type_constraint(type_param.constraint);
            }
        }

        if mapped_type.name_type.is_some() {
            self.emit_mapped_type_as_clause(mapped_type.name_type);
        }

        self.write("]");
        if let Some(question_node) = self.arena.get(mapped_type.question_token) {
            match question_node.kind {
                k if k == SyntaxKind::PlusToken as u16 => self.write("+?"),
                k if k == SyntaxKind::MinusToken as u16 => self.write("-?"),
                _ => self.write("?"),
            }
        }

        self.write(": ");
        self.emit_mapped_type_value_type(mapped_type.type_node);
        self.write("; }");
    }

    fn mapped_type_body_can_emit_inline(
        &self,
        mapped_type: &tsz_parser::parser::node::MappedTypeData,
    ) -> bool {
        self.arena.get(mapped_type.type_node).is_none_or(|node| {
            node.kind != syntax_kind_ext::TYPE_LITERAL && node.kind != syntax_kind_ext::MAPPED_TYPE
        })
    }

    fn expand_mapped_type_to_portable_properties(&self, type_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(type_idx)?;
        let text = self.get_source_slice(node.pos, node.end)?;
        let trimmed = text.trim().trim_end_matches(';').trim();
        let inner = trimmed
            .strip_prefix('{')
            .and_then(|text| text.strip_suffix('}'))
            .map(str::trim)
            .unwrap_or(trimmed);

        self.expand_portable_mapped_object_text(self.arena, inner)
    }

    pub(in crate::declaration_emitter) fn emit_mapped_type_constraint(
        &mut self,
        constraint_idx: NodeIndex,
    ) {
        let contains_type_literal = self.type_node_contains_type_literal(constraint_idx, 0);
        if let Some(node) = self.arena.get(constraint_idx)
            && let Some(text) = self.get_source_slice(node.pos, node.end)
            && !contains_type_literal
        {
            let text = Self::mapped_type_constraint_source_text(&text);
            if !text.is_empty() {
                self.write(text);
                return;
            }
        }

        // tsc keeps mapped-type constraint expressions on a single line; suppress multiline tuple formatting.
        let saved_indent = self.indent_level;
        if !contains_type_literal {
            self.indent_level = 0;
        }
        self.emit_type(constraint_idx);
        self.indent_level = saved_indent;
    }

    pub(in crate::declaration_emitter) fn emit_mapped_type_name_type(
        &mut self,
        name_type_idx: NodeIndex,
    ) {
        if let Some(node) = self.arena.get(name_type_idx)
            && let Some(text) = self.get_source_slice(node.pos, node.end)
            && !self.type_node_contains_mapped_type(name_type_idx, 0)
        {
            let text = Self::mapped_type_name_source_text(&text);
            if !text.is_empty() {
                self.write(text);
                return;
            }
        }

        // tsc keeps mapped-type name-type expressions on a single line; suppress multiline tuple
        // formatting. When the as-clause itself contains a nested mapped type, preserve indentation.
        if self.type_node_contains_mapped_type(name_type_idx, 0) {
            self.emit_type(name_type_idx);
        } else {
            let saved_indent = self.indent_level;
            self.indent_level = 0;
            self.emit_type(name_type_idx);
            self.indent_level = saved_indent;
        }
    }

    pub(in crate::declaration_emitter) fn emit_mapped_type_as_clause(
        &mut self,
        name_type_idx: NodeIndex,
    ) {
        self.write(" as ");

        if let Some(node) = self.arena.get(name_type_idx)
            && let Some(text) = self.get_source_slice(node.pos, node.end)
            && !self.type_node_contains_mapped_type(name_type_idx, 0)
        {
            let text = Self::mapped_type_name_source_text(&text);
            if !text.is_empty() {
                self.write(text);
                return;
            }
        }

        let start = self.writer.len();
        self.emit_mapped_type_name_type(name_type_idx);
        let emitted = self.writer.get_output()[start..].to_string();
        let normalized = Self::mapped_type_name_source_text(&emitted);
        if normalized != emitted.trim() {
            let normalized = normalized.to_string();
            self.writer.truncate(start);
            self.write(&normalized);
        }
    }

    fn type_node_contains_mapped_type(&self, type_idx: NodeIndex, depth: usize) -> bool {
        if type_idx.is_none() || depth > 128 {
            return false;
        }
        let Some(node) = self.arena.get(type_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::MAPPED_TYPE {
            return true;
        }
        self.arena
            .get_children(type_idx)
            .into_iter()
            .any(|child_idx| self.type_node_contains_mapped_type(child_idx, depth + 1))
    }

    fn type_node_contains_type_literal(&self, type_idx: NodeIndex, depth: usize) -> bool {
        if type_idx.is_none() || depth > 128 {
            return false;
        }
        let Some(node) = self.arena.get(type_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::TYPE_LITERAL {
            return true;
        }
        self.arena
            .get_children(type_idx)
            .into_iter()
            .any(|child_idx| self.type_node_contains_type_literal(child_idx, depth + 1))
    }

    fn mapped_type_constraint_source_text(text: &str) -> &str {
        let text = text.trim();
        let text = Self::split_mapped_as_clause(text)
            .map(|(before, _)| before.trim_end())
            .unwrap_or_else(|| Self::trim_trailing_mapped_as_keyword(text));
        Self::trim_unbalanced_closing_bracket(text)
    }

    fn mapped_type_name_source_text(text: &str) -> &str {
        let text = text.trim();
        let text = Self::split_mapped_as_clause(text)
            .map(|(_, after)| after.trim_start())
            .unwrap_or_else(|| Self::trim_leading_mapped_as_keyword(text));
        Self::trim_unbalanced_closing_bracket(text)
    }

    fn trim_leading_mapped_as_keyword(text: &str) -> &str {
        let mut trimmed = text.trim_start();
        while let Some(after_as) = trimmed.strip_prefix("as") {
            let has_boundary = after_as
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || !Self::is_identifier_part(ch));
            if !has_boundary {
                break;
            }
            trimmed = after_as.trim_start();
        }
        trimmed
    }

    fn split_mapped_as_clause(text: &str) -> Option<(&str, &str)> {
        let mut string_quote: Option<char> = None;
        let mut escaped = false;
        let mut angle_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut bracket_depth = 0u32;
        let mut paren_depth = 0u32;

        for (idx, ch) in text.char_indices() {
            if let Some(quote) = string_quote {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == quote {
                    string_quote = None;
                }
                continue;
            }

            match ch {
                '\'' | '"' | '`' => {
                    string_quote = Some(ch);
                    continue;
                }
                '<' => {
                    angle_depth += 1;
                    continue;
                }
                '>' => {
                    angle_depth = angle_depth.saturating_sub(1);
                    continue;
                }
                '{' => {
                    brace_depth += 1;
                    continue;
                }
                '}' => {
                    brace_depth = brace_depth.saturating_sub(1);
                    continue;
                }
                '[' => {
                    bracket_depth += 1;
                    continue;
                }
                ']' => {
                    bracket_depth = bracket_depth.saturating_sub(1);
                    continue;
                }
                '(' => {
                    paren_depth += 1;
                    continue;
                }
                ')' => {
                    paren_depth = paren_depth.saturating_sub(1);
                    continue;
                }
                _ => {}
            }

            if ch != 'a'
                || !text[idx..].starts_with("as")
                || angle_depth != 0
                || brace_depth != 0
                || bracket_depth != 0
                || paren_depth != 0
            {
                continue;
            }

            let before = &text[..idx];
            let after = &text[idx + 2..];
            let before_boundary = before
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_whitespace() || !Self::is_identifier_part(ch));
            let after_boundary = after
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || !Self::is_identifier_part(ch));
            if before_boundary && after_boundary {
                return Some((before, after));
            }
        }
        None
    }

    fn trim_trailing_mapped_as_keyword(text: &str) -> &str {
        let trimmed = text.trim_end();
        let Some(before_as) = trimmed.strip_suffix("as") else {
            return trimmed;
        };
        let had_separator = before_as
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace);
        let before_as = before_as.trim_end();
        let has_boundary = before_as
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_whitespace() || !Self::is_identifier_part(ch));
        if had_separator || has_boundary {
            before_as
        } else {
            trimmed
        }
    }

    fn trim_unbalanced_closing_bracket(text: &str) -> &str {
        let trimmed = text.trim_end();
        if !trimmed.ends_with(']') {
            return trimmed;
        }

        let opens = trimmed.chars().filter(|&ch| ch == '[').count();
        let closes = trimmed.chars().filter(|&ch| ch == ']').count();
        if closes > opens {
            trimmed[..trimmed.len() - 1].trim_end()
        } else {
            trimmed
        }
    }

    const fn is_identifier_part(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
    }
}
