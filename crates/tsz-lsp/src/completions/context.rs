//! Completion request context helpers.
//!
//! This module groups cursor-context detection and symbol metadata formatting
//! used by completion entry assembly.

use super::*;

impl<'a> Completions<'a> {
    pub(super) fn is_member_context(&self, offset: u32) -> bool {
        if offset > 0 {
            self.source_text
                .as_bytes()
                .get((offset - 1) as usize)
                .copied()
                .is_some_and(|ch| ch == b'.')
        } else {
            false
        }
    }

    /// Determine `isNewIdentifierLocation` by examining the AST context at the
    /// given byte offset. This matches tsserver's `computeCommitCharactersAndIsNewIdentifier`.
    ///
    /// Returns `true` when the cursor is in a position where the user might be
    /// typing a brand-new identifier (e.g. a variable name after `const`, a
    /// parameter name, an import binding name, etc.).
    pub fn compute_is_new_identifier_location(&self, root: NodeIndex, offset: u32) -> bool {
        // TypeScript's isNewIdentifierLocation defaults to false and only returns true
        // for specific token/parent-kind combinations. Our heuristic approximates this
        // by checking AST context and text patterns.

        let node_idx = self.find_completions_node(root, offset);

        // Check if inside a class/interface body at a member declaration position
        if let Some(node) = self.arena.get(node_idx) {
            let k = node.kind;

            // Property/method declarations and signatures in class/interface bodies
            if k == syntax_kind_ext::PROPERTY_DECLARATION
                || k == syntax_kind_ext::PROPERTY_SIGNATURE
                || k == syntax_kind_ext::METHOD_SIGNATURE
                || k == syntax_kind_ext::INDEX_SIGNATURE
            {
                return true;
            }

            // Inside class/interface body at member position (after `{`)
            if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::INTERFACE_DECLARATION
            {
                let text_before = &self.source_text[..offset as usize];
                if text_before[node.pos as usize..].contains('{') {
                    return true;
                }
            }

            // TODO: More AST-based checks needed for:
            // - Object literal with index signatures
            // - Type literal positions
            // - Function call argument positions
            // - Array literal positions
            // These require careful type-checking context that we don't have yet.
        }

        // Text-based heuristic for the context token
        let text = &self.source_text[..offset as usize];
        let trimmed = text.trim_end();
        if trimmed.is_empty() {
            return false;
        }

        // Find the last word before cursor
        let last_word_start = trimmed
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map_or(0, |p| p + 1);
        let last_word = &trimmed[last_word_start..];

        // Keywords after which we are creating a new identifier (name declaration position).
        // TypeScript's isNewIdentifierLocation only returns true for specific declaration
        // contexts where the user is expected to type a new name.
        // Keywords like `const`, `let`, `var`, `return`, `as` return false because
        // they expect existing identifiers or type names.
        if matches!(
            last_word,
            "module"
                | "namespace"
                | "import"
                | "function"
                | "class"
                | "interface"
                | "enum"
                | "type"
        ) {
            return true;
        }

        // Check the last non-whitespace character for common expression-start operators.
        // These match TypeScript's isNewIdentifierDefinitionLocation logic for tokens
        // that indicate the user may type a new expression (variable initializer,
        // function argument, array element, etc.).
        let last_char = trimmed.as_bytes().last().copied();
        match last_char {
            // After `=` in variable declarations and property assignments,
            // but NOT after `==`, `===`, `!=`, `>=`, `<=`
            Some(b'=') => {
                let before = &trimmed[..trimmed.len() - 1];
                let prev = before.as_bytes().last().copied();
                if prev != Some(b'=')
                    && prev != Some(b'!')
                    && prev != Some(b'>')
                    && prev != Some(b'<')
                {
                    return true;
                }
            }
            // After these characters, a new identifier/expression may start.
            Some(b'(') | Some(b',') | Some(b'[') | Some(b'{') | Some(b'<') | Some(b':')
            | Some(b'?') | Some(b'|') | Some(b'&') | Some(b'!') => return true,
            _ => {}
        }

        // After `${` in template literal expressions
        if trimmed.ends_with("${") {
            return true;
        }

        // If the user is typing an identifier prefix in expression/member-declaration
        // context, treat this as a new identifier location.
        if let Some(prev) = trimmed.chars().last()
            && (prev == '_' || prev == '$' || prev.is_ascii_alphanumeric())
        {
            let bytes = trimmed.as_bytes();
            let mut idx = bytes.len();
            while idx > 0 {
                let ch = bytes[idx - 1] as char;
                if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                    idx -= 1;
                } else {
                    break;
                }
            }
            let current_word = &trimmed[idx..];
            if matches!(
                current_word,
                "new"
                    | "as"
                    | "return"
                    | "throw"
                    | "typeof"
                    | "void"
                    | "delete"
                    | "in"
                    | "of"
                    | "extends"
                    | "implements"
                    | "import"
                    | "export"
                    | "from"
            ) {
                return false;
            }
            let mut prev_sig_idx = idx;
            while prev_sig_idx > 0 && bytes[prev_sig_idx - 1].is_ascii_whitespace() {
                prev_sig_idx -= 1;
            }
            if prev_sig_idx == 0 {
                return false;
            }
            let prev_sig = bytes[prev_sig_idx - 1] as char;
            // Member access completion (`obj.|`) is not a new identifier location.
            if prev_sig == '.' {
                return false;
            }
            return matches!(
                prev_sig,
                '=' | '('
                    | ','
                    | '['
                    | '{'
                    | '<'
                    | ':'
                    | '?'
                    | '|'
                    | '&'
                    | '!'
                    | ';'
                    | '+'
                    | '-'
                    | '*'
                    | '/'
                    | '%'
            );
        }

        false
    }

    pub(super) fn get_symbol_detail(&self, symbol: &tsz_binder::Symbol) -> Option<String> {
        use tsz_binder::symbol_flags;

        if symbol.flags & symbol_flags::FUNCTION != 0 {
            Some("function".to_string())
        } else if symbol.flags & symbol_flags::CLASS != 0 {
            Some("class".to_string())
        } else if symbol.flags & symbol_flags::INTERFACE != 0 {
            Some("interface".to_string())
        } else if symbol.flags & symbol_flags::REGULAR_ENUM != 0
            || symbol.flags & symbol_flags::CONST_ENUM != 0
        {
            Some("enum".to_string())
        } else if symbol.flags & symbol_flags::TYPE_ALIAS != 0 {
            Some("type".to_string())
        } else if symbol.flags & symbol_flags::TYPE_PARAMETER != 0 {
            Some("type parameter".to_string())
        } else if symbol.flags & symbol_flags::METHOD != 0 {
            Some("method".to_string())
        } else if symbol.flags & symbol_flags::PROPERTY != 0 {
            Some("property".to_string())
        } else if symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            Some("let/const".to_string())
        } else if symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            Some("var".to_string())
        } else if symbol.flags & symbol_flags::VALUE_MODULE != 0
            || symbol.flags & symbol_flags::NAMESPACE_MODULE != 0
        {
            Some("module".to_string())
        } else {
            None
        }
    }

    /// Build a comma-separated `kindModifiers` string for a symbol, matching
    /// tsserver's convention: `"export"`, `"declare"`, `"abstract"`, `"static"`,
    /// `"private"`, `"protected"`.
    pub(super) fn build_kind_modifiers(&self, symbol: &tsz_binder::Symbol) -> Option<String> {
        use tsz_binder::symbol_flags;

        let mut mods = Vec::new();
        if symbol.flags & symbol_flags::EXPORT_VALUE != 0 {
            mods.push("export");
        }
        if symbol.flags & symbol_flags::ABSTRACT != 0 {
            mods.push("abstract");
        }
        if symbol.flags & symbol_flags::STATIC != 0 {
            mods.push("static");
        }
        if symbol.flags & symbol_flags::PRIVATE != 0 {
            mods.push("private");
        }
        if symbol.flags & symbol_flags::PROTECTED != 0 {
            mods.push("protected");
        }
        if symbol.flags & symbol_flags::OPTIONAL != 0 {
            mods.push("optional");
        }
        if mods.is_empty() {
            None
        } else {
            Some(mods.join(","))
        }
    }
}
