impl<'a> CheckerState<'a> {
    /// Emit JSDoc `@template` syntax diagnostics for invalid brace forms like
    /// `@template {T}`. tsc reports both TS1069 at `{` and TS2304 at `T`.
    pub(crate) fn validate_jsdoc_template_tag_syntax_at_decl(&mut self, decl_idx: NodeIndex) {
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };
        let Some((_, comment_pos)) =
            self.try_leading_jsdoc_with_pos(comments, node.pos, source_text)
        else {
            return;
        };
        let comment_end = node.pos.min(source_text.len() as u32);
        let comment_range = &source_text[comment_pos as usize..comment_end as usize];

        let mut scan_start = 0usize;
        while let Some(template_offset) =
            Self::jsdoc_tag_offset(&comment_range[scan_start..], "template")
        {
            let template_start = scan_start + template_offset;
            let rest = &comment_range[template_start + "@template".len()..];
            let trimmed = rest.trim_start();
            if !trimmed.starts_with('{') {
                scan_start = template_start + "@template".len();
                continue;
            }

            let leading_ws = rest.len() - trimmed.len();
            let brace_rel = template_start + "@template".len() + leading_ws;
            let after_brace = &trimmed[1..];

            // tsc accepts `@template {Constraint} Name` as a type parameter
            // with a constraint (equivalent to `Name extends Constraint`).
            // Detect that form by finding the matching close-brace and
            // checking whether an identifier follows it (after whitespace).
            // If so, this is valid JSDoc syntax — skip.
            let balanced_close_brace_offset = |s: &str| -> Option<usize> {
                let mut depth: i32 = 1;
                for (i, ch) in s.char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                return Some(i);
                            }
                        }
                        _ => {}
                    }
                }
                None
            };
            if let Some(close_rel) = balanced_close_brace_offset(after_brace) {
                let after_close = &after_brace[close_rel + 1..];
                let ws_len = after_close
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .map(|c| c.len_utf8())
                    .sum::<usize>();
                let ident_rest = &after_close[ws_len..];
                let has_ident = ident_rest
                    .chars()
                    .next()
                    .is_some_and(|c| c == '_' || c == '$' || c.is_ascii_alphabetic());
                if has_ident {
                    scan_start = brace_rel + 1 + close_rel + 1;
                    continue;
                }
            }

            let name_len = after_brace
                .chars()
                .take_while(|ch| *ch == '_' || *ch == '$' || ch.is_ascii_alphanumeric())
                .count();
            let error_rel = brace_rel
                + 1
                + name_len
                + usize::from(
                    after_brace
                        .get(name_len..)
                        .is_some_and(|rest| rest.starts_with('}')),
                );
            let brace_pos = comment_pos + error_rel as u32;
            self.ctx.error(
                brace_pos,
                1,
                crate::diagnostics::diagnostic_messages::UNEXPECTED_TOKEN_A_TYPE_PARAMETER_NAME_WAS_EXPECTED_WITHOUT_CURLY_BRACES.to_string(),
                crate::diagnostics::diagnostic_codes::UNEXPECTED_TOKEN_A_TYPE_PARAMETER_NAME_WAS_EXPECTED_WITHOUT_CURLY_BRACES,
            );

            if name_len > 0 {
                let name = &after_brace[..name_len];
                self.emit_jsdoc_cannot_find_name(name, comment_pos, comment_end, source_text);
            }

            scan_start = brace_rel + 1;
        }
    }

    /// Extract a simple identifier from `@returns {T}` / `@return {T}`.
    ///
    /// Returns `None` for complex type expressions.
    pub(crate) fn jsdoc_returns_type_name(jsdoc: &str) -> Option<String> {
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let Some(rest) = Self::strip_jsdoc_return_tag_prefix(trimmed) else {
                continue;
            };
            let Some(type_expr) = Self::jsdoc_balanced_braced_type_expr(rest) else {
                continue;
            };
            if !type_expr.is_empty()
                && type_expr
                    .chars()
                    .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
            {
                return Some(type_expr.to_string());
            }
        }
        None
    }

    /// Extract the raw type expression from `@returns {Type}` / `@return {Type}`.
    pub(crate) fn jsdoc_returns_type_expression(jsdoc: &str) -> Option<String> {
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let Some(rest) = Self::strip_jsdoc_return_tag_prefix(trimmed) else {
                continue;
            };
            let Some(type_expr) = Self::jsdoc_balanced_braced_type_expr(rest) else {
                continue;
            };
            if !type_expr.is_empty() {
                return Some(type_expr.to_string());
            }
        }
        None
    }

    pub(crate) fn jsdoc_type_expression_is_type_predicate(type_expr: &str) -> bool {
        let (is_asserts, remainder) = Self::split_jsdoc_asserts_prefix(type_expr);
        is_asserts || Self::find_jsdoc_type_predicate_is(remainder).is_some()
    }

    /// Extract a type predicate from `@returns {x is Type}` / `@return {this is Entry}`.
    ///
    /// Returns `Some((is_asserts, param_name, type_str))` if the `@returns` tag
    /// contains a type predicate pattern like `{x is string}` or `{this is Entry}`.
    /// Also handles `{asserts x is Type}` and `{asserts x}` patterns.
    pub(crate) fn jsdoc_returns_type_predicate(
        jsdoc: &str,
    ) -> Option<(bool, String, Option<String>)> {
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let Some(rest) = Self::strip_jsdoc_return_tag_prefix(trimmed) else {
                continue;
            };
            let Some(type_expr) = Self::jsdoc_balanced_braced_type_expr(rest) else {
                continue;
            };

            let (is_asserts, remainder) = Self::split_jsdoc_asserts_prefix(type_expr);

            if let Some((is_pos, is_end)) = Self::find_jsdoc_type_predicate_is(remainder) {
                let param_name = remainder[..is_pos].trim();
                let type_str = remainder[is_end..].trim();
                // Validate param_name is a simple identifier or "this"
                if !param_name.is_empty()
                    && (param_name == "this"
                        || param_name
                            .chars()
                            .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
                    && !type_str.is_empty()
                {
                    return Some((
                        is_asserts,
                        param_name.to_string(),
                        Some(type_str.to_string()),
                    ));
                }
            } else if is_asserts {
                // "asserts x" without " is Type" — assertion without narrowing type
                let param_name = remainder;
                if !param_name.is_empty()
                    && (param_name == "this"
                        || param_name
                            .chars()
                            .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
                {
                    return Some((true, param_name.to_string(), None));
                }
            }
        }
        None
    }
}
