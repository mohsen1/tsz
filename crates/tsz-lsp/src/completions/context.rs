//! Completion request context helpers.
//!
//! This module groups cursor-context detection and symbol metadata formatting
//! used by completion entry assembly.

use super::*;

impl<'a> Completions<'a> {
    pub(super) fn is_member_context(&self, offset: u32) -> bool {
        if offset > 0 {
            let bytes = self.source_text.as_bytes();
            let prev = bytes.get((offset - 1) as usize).copied();
            if prev == Some(b'.') {
                // Check this isn't `..` (spread) or a number literal like `1.`
                // A `?.` counts — the char before offset-1 would be `?`
                // but that's still a member context
                let before_dot = if offset >= 2 {
                    bytes.get((offset - 2) as usize).copied()
                } else {
                    None
                };
                // Exclude `..` (spread operator or rest)
                before_dot != Some(b'.')
            } else {
                false
            }
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

        // Check if we're inside a JSX context - most JSX positions return false
        if self.is_in_jsx_context(node_idx) {
            return false;
        }

        // Check if inside a class/interface/object-literal/type-literal body
        // at a member declaration position
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

            // Method declarations in class body - if cursor is after the closing `}`
            // of a method, it's a new member position
            if k == syntax_kind_ext::METHOD_DECLARATION {
                // If the cursor is past the end of this method, we're at member position
                if offset >= node.end {
                    return true;
                }
            }

            // Inside class/interface body at member position
            if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::INTERFACE_DECLARATION
            {
                let text_before = &self.source_text[..offset as usize];
                let in_decl_text = &text_before[node.pos as usize..];
                let last_non_ws = in_decl_text.trim_end().as_bytes().last().copied();
                // After `{`, `;`, or `}` (end of method body) in class/interface body
                if matches!(last_non_ws, Some(b'{') | Some(b';') | Some(b'}')) {
                    return true;
                }
            }

            // Inside type literal - new member names are valid
            if k == syntax_kind_ext::TYPE_LITERAL {
                return true;
            }

            // Inside an import clause - namespace import binding (`import * as |`)
            if k == syntax_kind_ext::NAMESPACE_IMPORT {
                return true;
            }

            // Note: Object literal expression (`OBJECT_LITERAL_EXPRESSION`) is NOT
            // included here. TypeScript only returns `isNewIdentifierLocation = true`
            // for object literals when the contextual type has an index signature.
            // Since we lack type-checking context, we default to `false` which matches
            // the common case (object literals with known property names).

            // Note: Enum member positions use CompletionKind.MemberLike in TypeScript,
            // which returns isNewIdentifierLocation = false. So enum body is NOT
            // included here.
        }

        // Walk up to find enclosing context for member position detection
        if self.is_in_class_or_interface_member_position(node_idx, offset) {
            return true;
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
        if matches!(
            last_word,
            "module" | "namespace" | "function" | "class" | "interface" | "enum" | "type"
        ) {
            return true;
        }

        // Class member modifiers: after these keywords in class body = true
        if matches!(
            last_word,
            "public"
                | "private"
                | "protected"
                | "static"
                | "abstract"
                | "readonly"
                | "declare"
                | "async"
                | "override"
                | "accessor"
        ) && self.is_in_class_body_context(node_idx)
        {
            return true;
        }

        // `import` keyword: only true at statement level (not import type expressions)
        if last_word == "import" {
            return true;
        }

        // Check the last non-whitespace character for common expression-start operators.
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
            // After `(`, `,` (parameter/argument list), `?` (ternary),
            // `|`, `&` (union/intersection), `!` (non-null/negation)
            Some(b'(') | Some(b',') | Some(b'?') | Some(b'|') | Some(b'&') | Some(b'!') => {
                return true;
            }
            // After `[` - only in specific contexts (array literal, binding pattern)
            // NOT in element access expressions
            Some(b'[') => {
                // Check if this is a computed property access (obj[|]) - should be false
                // vs array literal [|] or binding pattern - should be true
                if !self.is_element_access_context(trimmed) {
                    return true;
                }
            }
            // After `<` - only for type parameter lists, NOT for JSX or comparison
            // Type parameter: `<T, |` or `func<|`
            Some(b'<') => {
                // Check if this looks like a type parameter list
                if self.is_type_parameter_context(trimmed) {
                    return true;
                }
            }
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
                    | "export"
                    | "from"
                    | "this"
                    | "super"
                    | "yield"
                    | "await"
                    | "case"
                    | "default"
                    | "instanceof"
            ) {
                return false;
            }
            // `import` prefix is handled above; don't match partial identifiers like "importSomething"
            if current_word == "import" {
                return true;
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
            // Check if we're in a class/interface member position with a partial identifier
            if self.is_in_class_or_interface_member_position(node_idx, offset) {
                return true;
            }
            return matches!(
                prev_sig,
                '=' | '(' | ',' | '?' | '|' | '&' | '!' | '+' | '-' | '*' | '/' | '%'
            );
        }

        false
    }

    /// Check if the current node is inside a JSX element/attribute context
    fn is_in_jsx_context(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        for _ in 0..10 {
            if let Some(node) = self.arena.get(current) {
                let k = node.kind;
                if k == syntax_kind_ext::JSX_ELEMENT
                    || k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                    || k == syntax_kind_ext::JSX_OPENING_ELEMENT
                    || k == syntax_kind_ext::JSX_ATTRIBUTES
                    || k == syntax_kind_ext::JSX_ATTRIBUTE
                    || k == syntax_kind_ext::JSX_FRAGMENT
                {
                    return true;
                }
                // Stop at function/class/source file boundaries
                if k == syntax_kind_ext::SOURCE_FILE
                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                {
                    return false;
                }
            }
            if let Some(ext) = self.arena.get_extended(current) {
                current = ext.parent;
            } else {
                break;
            }
        }
        false
    }

    /// Check if we're at a member position inside a class or interface body
    /// by walking ancestors to find the containing class/interface
    fn is_in_class_or_interface_member_position(&self, node_idx: NodeIndex, offset: u32) -> bool {
        let mut current = node_idx;
        for _ in 0..15 {
            if let Some(node) = self.arena.get(current) {
                let k = node.kind;
                // Found a class/interface ancestor - check if cursor is in member position
                if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION
                    || k == syntax_kind_ext::INTERFACE_DECLARATION
                {
                    let text_before = &self.source_text[..offset as usize];
                    let in_decl_text = &text_before[node.pos as usize..];
                    let last_non_ws = in_decl_text.trim_end().as_bytes().last().copied();
                    // After `{`, `;`, `}`, or `)` in class/interface body - member position
                    return matches!(
                        last_non_ws,
                        Some(b'{') | Some(b';') | Some(b'}') | Some(b')')
                    );
                }
                // Stop at function boundaries - we're inside a function body, not member position
                if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::SOURCE_FILE
                {
                    return false;
                }
            }
            if let Some(ext) = self.arena.get_extended(current) {
                current = ext.parent;
            } else {
                break;
            }
        }
        false
    }

    /// Check if we're inside a class body (not inside a method body within it)
    fn is_in_class_body_context(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        for _ in 0..15 {
            if let Some(node) = self.arena.get(current) {
                let k = node.kind;
                if k == syntax_kind_ext::CLASS_DECLARATION || k == syntax_kind_ext::CLASS_EXPRESSION
                {
                    return true;
                }
                if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::SOURCE_FILE
                {
                    return false;
                }
            }
            if let Some(ext) = self.arena.get_extended(current) {
                current = ext.parent;
            } else {
                break;
            }
        }
        false
    }

    /// Check if `[` at end of text is an element access (obj[|]) rather than array literal
    fn is_element_access_context(&self, trimmed: &str) -> bool {
        // If the character before `[` is an identifier char, closing paren, or closing bracket,
        // it's likely element access: `obj[`, `arr[0][`, `fn()[`
        let before = &trimmed[..trimmed.len() - 1];
        let before_trimmed = before.trim_end();
        if let Some(last) = before_trimmed.as_bytes().last() {
            matches!(
                last,
                b'a'..=b'z'
                    | b'A'..=b'Z'
                    | b'0'..=b'9'
                    | b'_'
                    | b'$'
                    | b')'
                    | b']'
            )
        } else {
            false
        }
    }

    /// Check if `<` at end of text is a type parameter context
    const fn is_type_parameter_context(&self, _trimmed: &str) -> bool {
        // Conservatively return true - type parameter is the more common case
        // for `<` in TypeScript than less-than comparison in completion context.
        // JSX is handled separately by the JSX context check.
        true
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
