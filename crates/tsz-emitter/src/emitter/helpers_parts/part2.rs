impl<'a> Printer<'a> {
    /// Check if a `declare;` expression statement is an artifact of the parser not
    /// recognizing `declare` as a modifier before certain keywords. Looks at the source
    /// text after `declare` to see if the next non-whitespace content on the same line
    /// is a keyword (import, export, declare, await, using, etc.) rather than `;` or a
    /// newline, which would indicate a legitimate expression statement.
    pub(super) fn is_declare_modifier_artifact(&self, node: &Node) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let bytes = text.as_bytes();
        // Start scanning after the `declare` keyword (7 chars: "declare")
        let declare_end = node.pos as usize + 7;
        let node_end = node.end as usize;
        if declare_end >= bytes.len() || declare_end > node_end {
            return false;
        }
        // Skip leading trivia (whitespace) to find where `declare` actually starts
        let mut pos = node.pos as usize;
        while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
            pos += 1;
        }
        // Verify this actually starts with "declare"
        if pos + 7 > bytes.len() || &bytes[pos..pos + 7] != b"declare" {
            return false;
        }
        pos += 7;
        // Skip spaces/tabs after "declare" (but NOT newlines — a newline means ASI)
        while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
            pos += 1;
        }
        // If we hit a newline, semicolon, or end of source, this is a real expression
        if pos >= bytes.len() || bytes[pos] == b'\n' || bytes[pos] == b'\r' || bytes[pos] == b';' {
            return false;
        }
        // Check if the next token is a keyword that `declare` should modify.
        // Prefix matches such as `interfaceX` are ordinary identifiers and must
        // keep the preceding `declare;` expression in recovery emit.
        let remaining = &text[pos..];
        [
            "import",
            "export",
            "declare",
            "function",
            "class",
            "abstract",
            "interface",
            "type",
            "enum",
            "namespace",
            "module",
            "var",
            "let",
            "const",
            "async",
            "await",
            "using",
            "global",
        ]
        .iter()
        .any(|keyword| starts_with_keyword_token(remaining, keyword))
    }

    /// Check if a module/namespace has any value-producing (instantiated) members.
    /// A module is NOT instantiated if it only contains type-only declarations
    /// (interfaces, type aliases, import type, etc.) or is empty.
    /// TypeScript skips emitting IIFE wrappers for non-instantiated modules.
    pub(super) fn is_instantiated_module(&self, module_body: NodeIndex) -> bool {
        crate::transforms::emit_utils::is_instantiated_module_ext(
            self.arena,
            module_body,
            self.ctx.options.preserve_const_enums,
        )
    }

    /// Scan forward from `pos` past whitespace and comments to find the actual
    /// token start. Used because node.pos includes leading trivia.
    pub(super) fn skip_trivia_forward(&self, start: u32, end: u32) -> u32 {
        crate::transforms::emit_utils::skip_trivia_forward(self.source_text, start, end)
    }

    /// Scan forward from `pos` past whitespace only (preserving comments).
    /// Used to find the start of a statement while preserving comments
    /// that may belong to nested expressions.
    pub fn skip_whitespace_forward(&self, start: u32, end: u32) -> u32 {
        let Some(text) = self.source_text else {
            return start;
        };
        let bytes = text.as_bytes();
        let mut pos = start as usize;
        let end = std::cmp::min(end as usize, bytes.len());
        while pos < end {
            match bytes[pos] {
                b' ' | b'\t' | b'\r' | b'\n' => pos += 1,
                _ => break,
            }
        }
        pos as u32
    }

    /// Returns true if the source character just before `c_pos` (skipping spaces/tabs)
    /// is a newline — meaning the comment at `c_pos` starts on its own line rather than
    /// being a trailing same-line comment.
    pub(super) fn comment_preceded_by_newline(&self, c_pos: u32) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let bytes = text.as_bytes();
        let mut i = c_pos as usize;
        while i > 0 {
            i -= 1;
            match bytes[i] {
                b' ' | b'\t' => continue,
                b'\n' | b'\r' => return true,
                _ => return false,
            }
        }
        false
    }

    /// Find the position of a specific byte in source text between `from` and `to`.
    pub(super) fn find_char_after(&self, from: u32, to: u32, ch: u8) -> Option<u32> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let end = (to as usize).min(bytes.len());
        let mut i = from as usize;
        while i < end {
            if bytes[i] == ch {
                return Some(i as u32);
            }
            i += 1;
        }
        None
    }

    /// Find the position of the first top-level ',' in source text after `from` and before `to`.
    /// Skips over nested brackets, strings, and comments so we don't match commas inside
    /// nested expressions (e.g. `[a, [b, c], d]` — the inner comma is skipped).
    pub(super) fn find_comma_pos_after(&self, from: u32, to: u32) -> Option<u32> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let to = to as usize;
        let mut i = from as usize;
        let mut depth = 0i32;
        while i < to.min(bytes.len()) {
            match bytes[i] {
                b',' if depth == 0 => return Some(i as u32),
                b'(' | b'[' | b'{' => {
                    depth += 1;
                    i += 1;
                }
                b')' | b']' | b'}' => {
                    if depth > 0 {
                        depth -= 1;
                    } else {
                        break; // exited our scope
                    }
                    i += 1;
                }
                b'\'' | b'"' => {
                    let q = bytes[i];
                    i += 1;
                    while i < to.min(bytes.len()) {
                        if bytes[i] == b'\\' {
                            i += 2;
                        } else if bytes[i] == q {
                            i += 1;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                }
                b'`' => {
                    i += 1;
                    while i < to.min(bytes.len()) {
                        if bytes[i] == b'\\' {
                            i += 2;
                        } else if bytes[i] == b'`' {
                            i += 1;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                }
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                    i += 2;
                    while i < to.min(bytes.len()) && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                    i += 2;
                    while i + 1 < to.min(bytes.len()) {
                        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    i += 1;
                }
            }
        }
        None
    }

    pub(in crate::emitter) fn comma_immediately_before_pos(&self, pos: u32) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let bytes = text.as_bytes();
        let mut i = (pos as usize).min(bytes.len());
        while i > 0 {
            i -= 1;
            match bytes[i] {
                b',' => return true,
                b' ' | b'\t' | b'\n' | b'\r' => continue,
                _ => return false,
            }
        }
        false
    }

    /// Check if the source text has a trailing comma after the last element
    /// in a list (object literal, array literal, etc.)
    ///
    /// Scans backwards from the closing bracket/brace to find if there's a
    /// comma before it (skipping whitespace). The parser includes the trailing
    /// comma in the last element's `end` position, so we scan backwards from
    /// the container's closing delimiter instead.
    pub(super) fn has_trailing_comma_in_source(
        &self,
        container: &tsz_parser::parser::node::Node,
        elements: &[NodeIndex],
    ) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };

        let end = std::cmp::min(container.end as usize, text.len());
        if end == 0 {
            return false;
        }

        let bytes = text.as_bytes();

        // Find the closing bracket/brace by scanning backwards from the container end
        let mut pos = end;
        while pos > 0 {
            pos -= 1;
            match bytes[pos] {
                b'}' | b']' | b')' => break,
                _ => continue,
            }
        }

        // Scan backwards from the closing bracket to find comma (skipping whitespace and comments).
        // This matches TypeScript behavior for cases like `yield 1, /*comment*/`.
        while pos > 0 {
            pos -= 1;
            if bytes[pos].is_ascii_whitespace() {
                continue;
            }

            // Skip block comments when scanning backwards.
            // We land on the `/` of `*/` when scanning right-to-left.
            if bytes[pos] == b'/' && pos > 0 && bytes[pos - 1] == b'*' {
                pos -= 1; // now at '*'
                // Find the matching `/*`
                while pos > 1 {
                    pos -= 1;
                    if bytes[pos] == b'*' && pos > 0 && bytes[pos - 1] == b'/' {
                        pos -= 1; // now at '/'
                        break;
                    }
                }
                continue;
            }

            // Skip line comments: the current `pos` might be inside a `//`
            // comment that either starts the line or appears inline after code
            // (e.g. `value, // comment`).  Scan forwards from the start of the
            // line to find the first `//` that is not inside a string/regex,
            // and if `pos` is at or after it, rewind to just before the `//`.
            {
                // Find the start of the current line.
                let line_start = {
                    let mut ls = pos;
                    while ls > 0 && bytes[ls - 1] != b'\n' {
                        ls -= 1;
                    }
                    ls
                };

                // Scan forward through the line to find an unquoted `//`.
                // We do a simplified scan: track single/double quotes and
                // skip escaped characters.  Regex literals could in theory
                // contain `//` but that is extremely rare and would require a
                // full parser rescan; the simplified approach is sufficient for
                // the trailing-comma detection use case.
                let mut scan = line_start;
                let mut found_line_comment = None;
                while scan < pos {
                    let b = bytes[scan];
                    if b == b'/' && scan + 1 < bytes.len() && bytes[scan + 1] == b'/' {
                        found_line_comment = Some(scan);
                        break;
                    }
                    // Skip string literals so `"//"` doesn't trigger.
                    if b == b'"' || b == b'\'' || b == b'`' {
                        scan += 1;
                        while scan < bytes.len() && bytes[scan] != b {
                            if bytes[scan] == b'\\' {
                                scan += 1; // skip escaped char
                            }
                            scan += 1;
                        }
                        // skip closing quote
                        scan += 1;
                        continue;
                    }
                    scan += 1;
                }

                if let Some(comment_start) = found_line_comment
                    && pos >= comment_start
                {
                    // `pos` is inside (or at) the line comment; rewind
                    // to just before the `//`.
                    pos = comment_start;
                    // Now continue the outer loop which will decrement
                    // pos and re-check.
                    continue;
                }
            }

            return bytes[pos] == b',';
        }

        // Fallback for recovery/edge cases: if source between the last element
        // and the container close contains a comma, treat it as trailing comma.
        if let Some(&last_idx) = elements.last()
            && let Some(last_node) = self.arena.get(last_idx)
        {
            let start = std::cmp::min(last_node.end as usize, text.len());
            let end = std::cmp::min(container.end as usize, text.len());
            if start < end && text[start..end].contains(',') {
                return true;
            }
        }

        false
    }
}
