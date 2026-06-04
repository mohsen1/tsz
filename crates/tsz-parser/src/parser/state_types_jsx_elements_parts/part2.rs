impl ParserState {
    // =========================================================================
    // JSX Parsing
    // =========================================================================

    /// Get the tight end position of a JSX tag name, following property access chains.
    pub(crate) fn get_jsx_tag_name_end(&self, tag_name: NodeIndex) -> u32 {
        if let Some(node) = self.arena.get(tag_name) {
            // For property access expressions (Foo.Bar), use the name child's end
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(node)
            {
                return self.get_jsx_tag_name_end(access.name_or_argument);
            }
            node.end
        } else {
            0
        }
    }

    /// Emit TS17008: JSX element '{0}' has no corresponding closing tag.
    /// Points at the opening tag name span (tight end for property access chains).
    pub(crate) fn emit_jsx_unclosed_tag_error(&mut self, tag_name: NodeIndex) {
        use tsz_common::diagnostics::diagnostic_codes;
        let tag_text = self.get_jsx_tag_name_text(tag_name);
        if let Some(node) = self.arena.get(tag_name) {
            let start = node.pos;
            let end = self.get_jsx_tag_name_end(tag_name);
            self.parse_error_at(
                start,
                end - start,
                &format!("JSX element '{tag_text}' has no corresponding closing tag."),
                diagnostic_codes::JSX_ELEMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
            );
        }
    }

    pub(crate) fn emit_jsx_unclosed_fragment_error(&mut self, opening_fragment: NodeIndex) {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
        if let Some(node) = self.arena.get(opening_fragment) {
            let start = if self.is_js_file() {
                node.pos.saturating_sub(1)
            } else {
                node.pos
            };
            self.parse_error_at(
                start,
                node.end - start,
                diagnostic_messages::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
                diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
            );
        }
    }

    /// Emit TS17002: Expected corresponding JSX closing tag for '{0}'.
    /// Points at the closing tag name span (where the mismatch is).
    pub(crate) fn emit_jsx_mismatched_closing_tag_error(
        &mut self,
        open_tag_name: NodeIndex,
        close_tag_name: NodeIndex,
    ) {
        use tsz_common::diagnostics::diagnostic_codes;
        let open_text = self.get_jsx_tag_name_text(open_tag_name);
        if let Some(close_node) = self.arena.get(close_tag_name) {
            let start = close_node.pos;
            let length = close_node.end - close_node.pos;
            self.parse_error_at(
                start,
                length,
                &format!("Expected corresponding JSX closing tag for '{open_text}'."),
                diagnostic_codes::EXPECTED_CORRESPONDING_JSX_CLOSING_TAG_FOR,
            );
        }
    }

    /// Check if a child `JsxElement` has mismatched tags where its closing tag
    /// matches the given parent opening tag name. This implements the tsc pattern
    /// where a child element "steals" the parent's closing tag.
    pub(crate) fn jsx_child_stole_closer(
        &self,
        child: NodeIndex,
        parent_tag_name: NodeIndex,
    ) -> bool {
        let child_node = match self.arena.get(child) {
            Some(n) if n.kind == syntax_kind_ext::JSX_ELEMENT => n,
            _ => return false,
        };
        let elem_data = match self.arena.get_jsx_element(child_node) {
            Some(d) => d.clone(),
            None => return false,
        };
        // Get the child's opening and closing tag names
        let child_open_tag = self
            .arena
            .get(elem_data.opening_element)
            .and_then(|n| self.arena.get_jsx_opening(n))
            .map(|d| d.tag_name);
        let child_close_tag = self
            .arena
            .get(elem_data.closing_element)
            .and_then(|n| self.arena.get_jsx_closing(n))
            .map(|d| d.tag_name);
        match (child_open_tag, child_close_tag) {
            (Some(open), Some(close)) => {
                // Child has mismatched tags AND its closing matches our opening
                !self.jsx_tag_names_match(open, close)
                    && self.jsx_tag_names_match(close, parent_tag_name)
            }
            _ => false,
        }
    }

    /// Check if the last child in a `NodeList` stole the parent's closing tag.
    /// Returns (`child_opening_tag_name`, `child_closing_element`) if so.
    pub(crate) fn check_last_child_stole_closer(
        &self,
        children: &NodeList,
        parent_tag_name: Option<NodeIndex>,
    ) -> Option<(NodeIndex, NodeIndex)> {
        let parent_tag = parent_tag_name?;
        let last_child = *children.nodes.last()?;
        let child_node = self.arena.get(last_child)?;
        if child_node.kind != syntax_kind_ext::JSX_ELEMENT {
            return None;
        }
        let elem_data = self.arena.get_jsx_element(child_node)?.clone();
        let child_open_tag = self
            .arena
            .get(elem_data.opening_element)
            .and_then(|n| self.arena.get_jsx_opening(n))
            .map(|d| d.tag_name)?;
        let child_close_tag = self
            .arena
            .get(elem_data.closing_element)
            .and_then(|n| self.arena.get_jsx_closing(n))
            .map(|d| d.tag_name)?;
        if !self.jsx_tag_names_match(child_open_tag, child_close_tag)
            && self.jsx_tag_names_match(child_close_tag, parent_tag)
        {
            Some((child_open_tag, elem_data.closing_element))
        } else {
            None
        }
    }

    /// Parse a JSX closing element: </Foo>
    pub(crate) fn parse_jsx_closing_element(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // In JSX mode, </ is scanned as a single LessThanSlashToken
        self.parse_expected(SyntaxKind::LessThanSlashToken);
        let tag_name = self.parse_jsx_element_name();
        let tag_name_end = self
            .arena
            .get(tag_name)
            .map_or(self.token_pos(), |node| node.end);
        let end_pos = if self.is_token(SyntaxKind::GreaterThanToken) {
            self.token_end()
        } else {
            tag_name_end
        };
        if self.is_token(SyntaxKind::OpenBraceToken) {
            self.recover_jsx_closing_tag_trailing_tail = true;
        }
        if !self.parse_expected(SyntaxKind::GreaterThanToken) {
            if self.is_token(SyntaxKind::ColonToken) {
                // Match tsc's malformed namespaced-closing-tag recovery: the
                // closing name stops after the first namespace pair (`</a:b`).
                // A later separator belongs to the outer malformed syntax, where
                // declaration/expression recovery can preserve the tail.
                self.recover_jsx_closing_tag_extra_namespace_tail = true;
            } else if self.is_token(SyntaxKind::DotToken) {
                // For a malformed namespaced close like `</b:c.x>`, tsc drops
                // the stray `.` and lets `x >` recover as a following expression.
                self.next_token();
            }
        }
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
        if !self.is_js_file() && !self.is_token(SyntaxKind::LessThanSlashToken) {
            // For non-JS JSX files, EOF still reports the missing `</` token.
            if self.is_token(SyntaxKind::EndOfFileToken) {
                self.parse_expected(SyntaxKind::LessThanSlashToken);
                return self.arena.add_token(
                    syntax_kind_ext::JSX_CLOSING_FRAGMENT,
                    start_pos,
                    self.token_pos(),
                );
            }
            while !self.is_token(SyntaxKind::EndOfFileToken)
                && !self.scanner.has_preceding_line_break()
                && !self.is_token(SyntaxKind::SemicolonToken)
            {
                self.next_token();
            }
            return self.arena.add_token(
                syntax_kind_ext::JSX_CLOSING_FRAGMENT,
                start_pos,
                self.token_pos(),
            );
        }
        // In JSX mode, </ is scanned as a single LessThanSlashToken
        self.parse_expected(SyntaxKind::LessThanSlashToken);
        if !self.is_js_file() && !self.is_token(SyntaxKind::GreaterThanToken) {
            // A fragment closed by a mismatched NAMED tag (e.g. `<>...</div>`).
            // tsc mirrors `parseExpected(GreaterThanToken, /*shouldAdvance*/ false)`
            // here: the missing `>` is reported non-advancingly (the diagnostics
            // are emitted by the caller's malformed-closing-fragment recovery), and
            // the unexpected tag tokens (`div`, `>`) are left UNCONSUMED so they
            // reparse as the following statement (`div > ...`). Consuming them would
            // swallow the trailing expression and drop the TS2304 name reference.
            return self.arena.add_token(
                syntax_kind_ext::JSX_CLOSING_FRAGMENT,
                start_pos,
                self.token_pos(),
            );
        }
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::GreaterThanToken);
        self.arena
            .add_token(syntax_kind_ext::JSX_CLOSING_FRAGMENT, start_pos, end_pos)
    }

    /// Consume the parser and return its parts.
    /// This is useful for taking ownership of the arena after parsing.
    #[must_use]
    pub fn into_parts(mut self) -> (NodeArena, Vec<ParseDiagnostic>) {
        // Transfer the interner from the scanner to the arena so atoms can be resolved
        self.arena.set_interner(self.scanner.take_interner());
        (self.arena, self.parse_diagnostics)
    }
}
