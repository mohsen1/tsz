impl<'a> Printer<'a> {
    fn has_optional_call_token(
        &self,
        call_node: &Node,
        callee: NodeIndex,
        args: Option<&tsz_parser::parser::NodeList>,
    ) -> bool {
        let Some(source) = self.source_text_for_map() else {
            let Some(callee_node) = self.arena.get(callee) else {
                return false;
            };
            if self.arena.get_access_expr(callee_node).is_none() {
                return true;
            }
            return false;
        };

        let Some(callee_node) = self.arena.get(callee) else {
            return false;
        };
        // The `(` we want is the one that opens the call's argument list,
        // which is *after* the callee. If the callee is itself a
        // parenthesized expression — `(foo.m as any)?.()` — then
        // `find_call_open_paren_position`'s naive "first `(` between
        // call_node.pos and call_node.end" lands on the *callee's*
        // open paren, not the argument-list `(`. The backward scan for
        // `?.` from that wrong position finds nothing and the optional-
        // call token is silently dropped, producing `foo.m()` instead of
        // `foo.m?.()`. Pin the search start to right after the callee.
        let scan_start = std::cmp::min(callee_node.end as usize, source.len());
        let Some(open_paren) =
            self.find_call_open_paren_position_after(call_node, args, scan_start as u32)
        else {
            return false;
        };

        let bytes = source.as_bytes();
        let mut i = std::cmp::min(open_paren as usize, source.len());
        let start = std::cmp::min(callee_node.pos as usize, source.len());

        while i > start {
            if i == 0 {
                break;
            }
            match bytes[i - 1] {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    i -= 1;
                }
                b'/' if i >= 2 && bytes[i - 2] == b'/' => {
                    while i > start && bytes[i - 1] != b'\n' {
                        i -= 1;
                    }
                    if i > start {
                        i -= 1;
                    }
                }
                b'/' if i >= 2 && bytes[i - 2] == b'*' => {
                    if i >= 2 {
                        i -= 2;
                    }
                    while i >= 2 && !(bytes[i - 2] == b'*' && bytes[i - 1] == b'/') {
                        i -= 1;
                    }
                    if i >= 2 {
                        i -= 2;
                    }
                }
                // Skip over type arguments: `?.<T>()` → scan past `<T>` to find `?.`
                b'>' => {
                    let mut depth = 1u32;
                    i -= 1;
                    while i > start && depth > 0 {
                        match bytes[i - 1] {
                            b'>' => depth += 1,
                            b'<' => depth -= 1,
                            _ => {}
                        }
                        i -= 1;
                    }
                    // After skipping `<...>`, continue scanning for `?.`
                }
                b'?' if i >= 2 && bytes[i - 2] == b'.' => {
                    return true;
                }
                b'.' if i >= 2 && bytes[i - 2] == b'?' && bytes[i - 1] == b'.' => {
                    return true;
                }
                _ => return false,
            }
        }

        false
    }

    fn find_call_open_paren_position(
        &self,
        call_node: &Node,
        args: Option<&tsz_parser::parser::NodeList>,
    ) -> Option<u32> {
        let start_after = self
            .arena
            .get_call_expr(call_node)
            .and_then(|call| self.arena.get(call.expression))
            .map_or(call_node.pos, |callee| callee.end);
        self.find_call_open_paren_position_after(call_node, args, start_after)
    }

    /// Variant of `find_call_open_paren_position` that begins the search
    /// at an explicit offset, used by `has_optional_call_token` to skip
    /// past a parenthesized or type-asserted callee whose own `(` would
    /// otherwise be returned. The offset is clamped to the call node's
    /// end and to the source length.
    fn find_call_open_paren_position_after(
        &self,
        call_node: &Node,
        args: Option<&tsz_parser::parser::NodeList>,
        start_after: u32,
    ) -> Option<u32> {
        let text = self.source_text_for_map()?;
        let bytes = text.as_bytes();
        let start = std::cmp::min(start_after as usize, bytes.len());
        let mut end = std::cmp::min(call_node.end as usize, bytes.len());
        if let Some(args) = args
            && let Some(first) = args.nodes.first()
            && let Some(first_node) = self.arena.get(*first)
        {
            end = std::cmp::min(first_node.pos as usize, end);
        }
        if start >= end {
            return None;
        }
        (start..end)
            .position(|i| bytes[i] == b'(')
            .map(|offset| (start + offset) as u32)
    }

    fn find_call_closing_paren_position(
        &self,
        call_node: &Node,
        args: Option<&tsz_parser::parser::NodeList>,
    ) -> Option<u32> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let open_pos = self.find_call_open_paren_position(call_node, args)? as usize;
        let mut pos = open_pos;
        let mut depth: i32 = 0;

        while pos < bytes.len() {
            match bytes[pos] {
                b'(' => {
                    depth += 1;
                    pos += 1;
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(pos as u32);
                    }
                    pos += 1;
                }
                b'\'' | b'"' | b'`' => {
                    let quote = bytes[pos];
                    pos += 1;
                    while pos < bytes.len() {
                        if bytes[pos] == b'\\' {
                            pos += 2;
                        } else if bytes[pos] == quote {
                            pos += 1;
                            break;
                        } else {
                            pos += 1;
                        }
                    }
                }
                b'/' if pos + 1 < bytes.len() && bytes[pos + 1] == b'/' => {
                    pos += 2;
                    while pos < bytes.len() && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                        pos += 1;
                    }
                }
                b'/' if pos + 1 < bytes.len() && bytes[pos + 1] == b'*' => {
                    pos += 2;
                    while pos + 1 < bytes.len() {
                        if bytes[pos] == b'*' && bytes[pos + 1] == b'/' {
                            pos += 2;
                            break;
                        }
                        pos += 1;
                    }
                }
                _ => pos += 1,
            }
        }

        None
    }

    fn call_argument_comment_boundary(&self, arg_idx: NodeIndex) -> u32 {
        let Some(arg_node) = self.arena.get(arg_idx) else {
            return 0;
        };

        if arg_node.kind == syntax_kind_ext::ARROW_FUNCTION
            && let Some(func) = self.arena.get_function(arg_node)
            && let Some(body_node) = self.arena.get(func.body)
            && body_node.kind == syntax_kind_ext::BLOCK
        {
            return self.find_block_closing_brace_end(body_node);
        }

        self.find_token_end_before_trivia(arg_node.pos, arg_node.end)
    }

    fn emit_call_trailing_argument_comments(&mut self, from_pos: u32, close_paren_pos: u32) {
        if self.ctx.options.remove_comments || from_pos >= close_paren_pos {
            return;
        }

        let Some(text) = self.source_text else {
            return;
        };
        let bytes = text.as_bytes();
        if let Some(comment) = self.all_comments.get(self.comment_emit_idx)
            && comment.pos >= from_pos
            && comment.end <= close_paren_pos
        {
            let gap_start = std::cmp::min(from_pos as usize, bytes.len());
            let gap_end = std::cmp::min(comment.pos as usize, bytes.len());
            if bytes[gap_start..gap_end]
                .iter()
                .any(|&b| b == b'\n' || b == b'\r')
            {
                self.write_line();
            }
        }

        self.emit_unemitted_comments_between(from_pos, close_paren_pos);
    }

    fn emit_empty_call_argument_comments(
        &mut self,
        call_node: &Node,
        args: Option<&tsz_parser::parser::NodeList>,
    ) {
        if self.ctx.options.remove_comments {
            return;
        }

        let Some(text) = self.source_text else {
            return;
        };
        let Some(open_paren_pos) = self.find_call_open_paren_position(call_node, args) else {
            return;
        };
        let Some(close_paren_pos) = self.find_call_closing_paren_position(call_node, args) else {
            return;
        };
        if open_paren_pos + 1 >= close_paren_pos {
            return;
        }

        let bytes = text.as_bytes();
        let mut scan_idx = self.comment_emit_idx;
        let mut previous_pos = open_paren_pos + 1;
        let mut previous_comment_had_trailing_newline = false;

        while scan_idx < self.all_comments.len() {
            let comment_pos = self.all_comments[scan_idx].pos;
            let comment_end = self.all_comments[scan_idx].end;
            let has_trailing_new_line = self.all_comments[scan_idx].has_trailing_new_line;
            if comment_end <= open_paren_pos {
                scan_idx += 1;
                continue;
            }
            if comment_pos >= close_paren_pos {
                break;
            }
            if comment_pos < open_paren_pos || comment_end > close_paren_pos {
                scan_idx += 1;
                continue;
            }

            if previous_comment_had_trailing_newline {
                // The previous comment already advanced to the next output line.
            } else if self.call_comment_range_contains_newline(previous_pos, comment_pos, bytes) {
                self.write_line();
            } else {
                self.write_space();
            }

            if let Ok(comment_text) =
                crate::safe_slice::slice(text, comment_pos as usize, comment_end as usize)
                && !comment_text.is_empty()
            {
                self.write_comment_with_reindent(comment_text, Some(comment_pos));
                if has_trailing_new_line {
                    self.write_line();
                }
            }

            previous_pos = comment_end;
            previous_comment_had_trailing_newline = has_trailing_new_line;
            self.comment_emit_idx = scan_idx + 1;
            scan_idx += 1;
        }
    }

    fn emit_call_leading_argument_comments(&mut self, open_paren_pos: u32, arg_pos: u32) {
        if self.ctx.options.remove_comments || open_paren_pos >= arg_pos {
            return;
        }

        let Some(text) = self.source_text else {
            return;
        };
        let bytes = text.as_bytes();
        if let Some(comment) = self.all_comments.get(self.comment_emit_idx)
            && comment.pos >= open_paren_pos
            && comment.end <= arg_pos
        {
            let gap_start = std::cmp::min(open_paren_pos as usize + 1, bytes.len());
            let gap_end = std::cmp::min(comment.pos as usize, bytes.len());
            let gap_after_start = std::cmp::min(comment.end as usize, bytes.len());
            let gap_after_end = std::cmp::min(arg_pos as usize, bytes.len());
            if bytes[gap_start..gap_end]
                .iter()
                .chain(bytes[gap_after_start..gap_after_end].iter())
                .any(|&b| b == b'\n' || b == b'\r')
            {
                self.emit_call_leading_multiline_argument_comments(open_paren_pos + 1, arg_pos);
                return;
            }
        }

        self.emit_unemitted_comments_between(open_paren_pos, arg_pos);
    }

    fn emit_call_leading_multiline_argument_comments(&mut self, from_pos: u32, arg_pos: u32) {
        let Some(text) = self.source_text else {
            return;
        };
        let bytes = text.as_bytes();
        let mut scan_idx = self.comment_emit_idx;
        let mut previous_pos = from_pos;
        let mut previous_comment_had_trailing_newline = false;
        let mut emitted_any = false;

        while scan_idx < self.all_comments.len() {
            let comment_pos = self.all_comments[scan_idx].pos;
            let comment_end = self.all_comments[scan_idx].end;
            let has_trailing_new_line = self.all_comments[scan_idx].has_trailing_new_line;
            if comment_end <= from_pos {
                scan_idx += 1;
                continue;
            }
            if comment_pos >= arg_pos {
                break;
            }
            if comment_pos < from_pos || comment_end > arg_pos {
                scan_idx += 1;
                continue;
            }

            if previous_comment_had_trailing_newline {
                // The previous comment already moved to the next output line.
            } else if self.call_comment_range_contains_newline(previous_pos, comment_pos, bytes) {
                self.write_line();
            } else if emitted_any {
                self.write_space();
            }

            if let Ok(comment_text) =
                crate::safe_slice::slice(text, comment_pos as usize, comment_end as usize)
                && !comment_text.is_empty()
            {
                self.write_comment_with_reindent(comment_text, Some(comment_pos));
                emitted_any = true;
                if has_trailing_new_line {
                    self.write_line();
                }
            }

            previous_pos = comment_end;
            previous_comment_had_trailing_newline = has_trailing_new_line;
            self.comment_emit_idx = scan_idx + 1;
            scan_idx += 1;
        }

        if emitted_any
            && !previous_comment_had_trailing_newline
            && self.call_comment_range_contains_newline(previous_pos, arg_pos, bytes)
        {
            self.write_line();
        }
    }

    fn call_comment_range_contains_newline(
        &self,
        from_pos: u32,
        to_pos: u32,
        bytes: &[u8],
    ) -> bool {
        let start = std::cmp::min(from_pos as usize, bytes.len());
        let end = std::cmp::min(to_pos as usize, bytes.len());
        bytes[start..end].iter().any(|&b| b == b'\n' || b == b'\r')
    }

    fn emit_dynamic_import_template_specifier(&mut self, expr: NodeIndex) {
        let Some(node) = self.arena.get(expr) else {
            return;
        };

        if self.ctx.emit_await_as_yield_await
            && node.kind == syntax_kind_ext::YIELD_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr_ex(node)
            && !unary.asterisk_token
        {
            self.write("yield ");
            self.write("yield ");
            self.write_helper("__await");
            self.write("(");
            if unary.expression.is_some() {
                self.emit_expression(unary.expression);
            } else {
                self.write("void 0");
            }
            self.write(")");
            return;
        }

        self.emit(expr);
    }

    /// Unwrap parenthesized expressions and type assertions/satisfies to find
    /// the underlying runtime expression. Used by optional call lowering to
    /// detect property access through type assertion wrappers like
    /// `(foo.m as any)?.()`.
    fn unwrap_paren_and_type_assertion(&self, mut idx: NodeIndex) -> NodeIndex {
        loop {
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            match node.kind {
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    let Some(paren) = self.arena.get_parenthesized(node) else {
                        return idx;
                    };
                    idx = paren.expression;
                }
                k if k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    let Some(assert) = self.arena.get_type_assertion(node) else {
                        return idx;
                    };
                    idx = assert.expression;
                }
                _ => return idx,
            }
        }
    }
}
