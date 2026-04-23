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

    /// Check if the cursor is immediately after `..` (double dot).
    /// This indicates a spread operator or syntax error, not member access.
    pub(super) fn is_after_double_dot(&self, offset: u32) -> bool {
        if offset >= 2 {
            let trimmed = self.source_text[..offset as usize].trim_end();
            // Check for ".." but not "..." (spread operator — which should get
            // completions for the spread argument)
            trimmed.ends_with("..") && !trimmed.ends_with("...")
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
        if trimmed.ends_with('.') && self.is_dotted_namespace_name_context(trimmed) {
            return true;
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
            // and NOT in type alias definitions (`type X = |`) or type parameter defaults (`<T = |>`)
            Some(b'=') => {
                if self.is_in_parameter_list(offset) {
                    return false;
                }
                let before = &trimmed[..trimmed.len() - 1];
                let prev = before.as_bytes().last().copied();
                if prev != Some(b'=')
                    && prev != Some(b'!')
                    && prev != Some(b'>')
                    && prev != Some(b'<')
                {
                    // Check if we're in a type context (type alias or type parameter default)
                    if self.is_in_type_context(node_idx) {
                        return false;
                    }
                    return true;
                }
            }
            // After `(`, `,` (parameter/argument list),
            // `|`, `&` (union/intersection), `!` (non-null/negation)
            Some(b'(') | Some(b',') | Some(b'|') | Some(b'&') | Some(b'!') => {
                // In type contexts (type alias body, type parameter defaults, type arguments),
                // `|` and `&` are union/intersection operators — not new identifier positions
                if matches!(last_char, Some(b'|') | Some(b'&') | Some(b','))
                    && self.is_in_type_context(node_idx)
                {
                    return false;
                }
                return true;
            }
            // After `?` — only true in parameter lists (optional param), not ternary.
            // Ternary operator `cond ? |` is an expression position, not new identifier.
            Some(b'?') if !self.is_in_type_context(node_idx) => {
                return true;
            }
            // After `[` - only in specific contexts (array literal, binding pattern)
            // NOT in element access expressions.
            Some(b'[') if !self.is_element_access_context(trimmed) => {
                return true;
            }
            // After `<` - only for type parameter lists, NOT for JSX or comparison.
            // Type parameter: `<T, |` or `func<|`.
            Some(b'<') if self.is_type_parameter_context(trimmed) => {
                return true;
            }
            _ => {}
        }

        // After `${` in template literal expressions
        if trimmed.ends_with("${") {
            return true;
        }
        // Dotted namespace/module declaration names are identifier-definition
        // contexts: `namespace A.|` and `namespace A.B.|`.
        if trimmed.ends_with('.') && self.is_dotted_namespace_name_context(trimmed) {
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
            if current_word
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_digit())
            {
                return false;
            }
            if self.is_in_class_body_context(node_idx) {
                let mut j = idx;
                while j > 0 && bytes[j - 1].is_ascii_whitespace() {
                    j -= 1;
                }
                if j == 0 {
                    return true;
                }
                let prev = bytes[j - 1];
                if prev == b'{' || prev == b';' || prev == b'}' {
                    return true;
                }
            }

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
            if self.text_likely_in_class_body(offset)
                && matches!(prev_sig, '\n' | '\r' | ';' | '{' | '}')
            {
                return true;
            }
            // `?` is excluded: ternary `cond ? expr|` should not be isNewIdentifierLocation.
            // In type contexts, `|` and `&` are type operators, not new-identifier positions.
            if self.is_in_type_context(node_idx) && matches!(prev_sig, '=' | '|' | '&' | ',' | '<')
            {
                return false;
            }
            return matches!(
                prev_sig,
                '=' | '(' | ',' | '|' | '&' | '!' | '+' | '-' | '*' | '/' | '%'
            );
        }

        if self.is_in_class_body_context(node_idx) {
            return true;
        }

        if self.text_likely_in_class_body(offset) {
            return true;
        }

        false
    }

    pub(super) fn is_dotted_namespace_completion_context(&self, offset: u32) -> bool {
        let text = &self.source_text[..offset as usize];
        let trimmed = text.trim_end();
        let line_start = trimmed.rfind('\n').map_or(0, |idx| idx + 1);
        let line = trimmed[line_start..].trim_start();
        if let Some(rest) = line.strip_prefix("namespace ") {
            return rest.contains('.');
        }
        if let Some(rest) = line.strip_prefix("module ") {
            return rest.contains('.');
        }
        false
    }

    fn is_dotted_namespace_name_context(&self, trimmed: &str) -> bool {
        let line_start = trimmed.rfind('\n').map_or(0, |idx| idx + 1);
        let line = trimmed[line_start..].trim_start();
        if let Some(rest) = line.strip_prefix("namespace ") {
            return !rest.is_empty();
        }
        if let Some(rest) = line.strip_prefix("module ") {
            return !rest.is_empty();
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
                    // After `{`, `;`, or `}` in class/interface body - member position.
                    // `)` is excluded; inside method bodies it often trails expressions
                    // and is not a declaration site.
                    return matches!(last_non_ws, Some(b'{') | Some(b';') | Some(b'}'));
                }
                // Stop at function boundaries - we're inside a function body, not member position
                if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::CONSTRUCTOR
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
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
    pub(super) fn is_in_class_body_context(&self, node_idx: NodeIndex) -> bool {
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
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::CONSTRUCTOR
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
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

    /// Check if `<` at end of text is a type parameter *declaration* context
    /// (where you'd introduce new type parameter names), vs a type *argument*
    /// context (where you reference existing types).
    fn is_type_parameter_context(&self, trimmed: &str) -> bool {
        // Look at what precedes the `<` to determine if this is a declaration or usage.
        let before = trimmed[..trimmed.len() - 1].trim_end();
        if before.is_empty() {
            return false;
        }
        // Find the word before `<`
        let word_end = before.len();
        let word_start = before
            .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
            .map_or(0, |p| p + 1);
        let word = &before[word_start..word_end];

        // Type parameter declarations: `function foo<|`, `class Foo<|`, `interface Bar<|`,
        // `type Alias<|`, `method<|` (in declarations)
        // These are contexts where you NAME new type parameters → true
        //
        // Type argument usages: `foo<|` (calling generic), `Foo<|` (using generic type),
        // `new Foo<|`, etc. — you're specifying existing types → false
        //
        // Heuristic: if preceded by a keyword that starts a declaration, it's a declaration.
        matches!(
            word,
            "function" | "class" | "interface" | "type" | "extends" | "implements"
        )
    }

    /// Check if the cursor is inside a type context (type alias body, type parameter
    /// default, type argument list, etc.) by walking AST ancestors.
    fn is_in_type_context(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        for _ in 0..20 {
            if let Some(node) = self.arena.get(current) {
                let k = node.kind;
                // Type-position AST nodes
                if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || k == syntax_kind_ext::TYPE_REFERENCE
                    || k == syntax_kind_ext::UNION_TYPE
                    || k == syntax_kind_ext::INTERSECTION_TYPE
                    || k == syntax_kind_ext::MAPPED_TYPE
                    || k == syntax_kind_ext::CONDITIONAL_TYPE
                    || k == syntax_kind_ext::FUNCTION_TYPE
                    || k == syntax_kind_ext::CONSTRUCTOR_TYPE
                    || k == syntax_kind_ext::TUPLE_TYPE
                    || k == syntax_kind_ext::ARRAY_TYPE
                    || k == syntax_kind_ext::PARENTHESIZED_TYPE
                    || k == syntax_kind_ext::INDEXED_ACCESS_TYPE
                    || k == syntax_kind_ext::TYPE_QUERY
                    || k == syntax_kind_ext::TYPE_OPERATOR
                    || k == syntax_kind_ext::TYPE_LITERAL
                    || k == syntax_kind_ext::TYPE_PARAMETER
                    || k == syntax_kind_ext::TYPE_PREDICATE
                    || k == syntax_kind_ext::IMPORT_TYPE
                {
                    return true;
                }
                // Stop at expression/statement/declaration boundaries
                if k == syntax_kind_ext::SOURCE_FILE
                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::VARIABLE_DECLARATION
                    || k == syntax_kind_ext::CALL_EXPRESSION
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

    pub(super) fn should_offer_constructor_keyword(&self, offset: u32) -> bool {
        let node_idx = crate::utils::find_node_at_offset(self.arena, offset);
        let in_class_body = node_idx.is_some() && self.is_in_class_body_context(node_idx);
        if !in_class_body && !self.text_likely_in_class_body(offset) {
            return false;
        }

        let end = (offset as usize).min(self.source_text.len());
        let text = &self.source_text[..end];
        let line_start = text.rfind('\n').map_or(0, |idx| idx + 1);
        let line = &text[line_start..];
        let prefix = line.trim_end();
        if prefix.is_empty() {
            return true;
        }

        let bytes = prefix.as_bytes();
        let mut idx = bytes.len();
        while idx > 0 {
            let ch = bytes[idx - 1] as char;
            if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                idx -= 1;
            } else {
                break;
            }
        }
        let word = &prefix[idx..];
        if !word.is_empty() && !word.starts_with("con") {
            return false;
        }
        let before = prefix[..idx].trim_end();
        if before.is_empty() {
            return !word.is_empty();
        }
        if before.contains('=') || before.contains(':') {
            return false;
        }
        matches!(
            before.as_bytes().last().copied(),
            Some(b';') | Some(b'{') | Some(b'}')
        )
    }

    pub(super) fn text_likely_in_class_body(&self, offset: u32) -> bool {
        let end = (offset as usize).min(self.source_text.len());
        let text = &self.source_text[..end];
        let Some(class_pos) = text.rfind("class ") else {
            return false;
        };
        let Some(rel_open) = text[class_pos..].find('{') else {
            return false;
        };
        let open = class_pos + rel_open;
        if open + 1 >= end {
            return true;
        }
        let mut depth = 1i32;
        for &b in &text.as_bytes()[open + 1..] {
            match b {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            if depth <= 0 {
                return false;
            }
        }
        depth == 1
    }

    pub(super) fn get_symbol_detail(&self, symbol: &tsz_binder::Symbol) -> Option<String> {
        use tsz_binder::symbol_flags;

        if symbol.has_any_flags(symbol_flags::FUNCTION) {
            Some("function".to_string())
        } else if symbol.has_any_flags(symbol_flags::CLASS) {
            Some("class".to_string())
        } else if symbol.has_any_flags(symbol_flags::INTERFACE) {
            Some("interface".to_string())
        } else if symbol.has_any_flags(symbol_flags::REGULAR_ENUM)
            || symbol.has_any_flags(symbol_flags::CONST_ENUM)
        {
            Some("enum".to_string())
        } else if symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
            Some("type".to_string())
        } else if symbol.has_any_flags(symbol_flags::TYPE_PARAMETER) {
            Some("type parameter".to_string())
        } else if symbol.has_any_flags(symbol_flags::METHOD) {
            Some("method".to_string())
        } else if symbol.has_any_flags(symbol_flags::PROPERTY) {
            Some("property".to_string())
        } else if symbol.has_any_flags(symbol_flags::BLOCK_SCOPED_VARIABLE) {
            Some("let/const".to_string())
        } else if symbol.has_any_flags(symbol_flags::FUNCTION_SCOPED_VARIABLE) {
            Some("var".to_string())
        } else if symbol.has_any_flags(symbol_flags::VALUE_MODULE)
            || symbol.has_any_flags(symbol_flags::NAMESPACE_MODULE)
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
        use tsz_parser::parser::flags::node_flags;

        let mut mods = Vec::new();
        if symbol.has_any_flags(symbol_flags::EXPORT_VALUE) {
            mods.push("export");
        }
        // Check declaration node for ambient (declare) and deprecated
        if let Some(decl_idx) = symbol.primary_declaration() {
            if let Some(decl_node) = self.arena.get(decl_idx) {
                let nf = decl_node.flags as u32;
                // Check for deprecated (set by JSDoc @deprecated tag during parsing)
                if nf & node_flags::DEPRECATED != 0 {
                    mods.push("deprecated");
                }
            }
            // Check for declare by scanning children for DeclareKeyword
            if self.has_declare_modifier(decl_idx) {
                mods.push("declare");
            }
        }
        if symbol.has_any_flags(symbol_flags::ABSTRACT) {
            mods.push("abstract");
        }
        if symbol.has_any_flags(symbol_flags::STATIC) {
            mods.push("static");
        }
        if symbol.has_any_flags(symbol_flags::PRIVATE) {
            mods.push("private");
        }
        if symbol.has_any_flags(symbol_flags::PROTECTED) {
            mods.push("protected");
        }
        if symbol.has_any_flags(symbol_flags::OPTIONAL) {
            mods.push("optional");
        }
        if mods.is_empty() {
            None
        } else {
            Some(mods.join(","))
        }
    }

    /// Check if a declaration node has a `declare` modifier by looking at its
    /// modifiers list for a `DeclareKeyword` node.
    fn has_declare_modifier(&self, decl_idx: NodeIndex) -> bool {
        let declare_kind = SyntaxKind::DeclareKeyword as u16;
        // Check children of the declaration for DeclareKeyword
        if self.has_declare_child(decl_idx, declare_kind) {
            return true;
        }
        // For VariableDeclaration nodes, `declare` lives on the parent
        // VariableStatement, so check the parent chain
        if let Some(ext) = self.arena.get_extended(decl_idx) {
            let parent = ext.parent;
            if parent.is_some() {
                if self.has_declare_child(parent, declare_kind) {
                    return true;
                }
                // Also check grandparent (VariableDeclaration -> VariableDeclarationList -> VariableStatement)
                if let Some(gext) = self.arena.get_extended(parent)
                    && gext.parent.is_some()
                    && self.has_declare_child(gext.parent, declare_kind)
                {
                    return true;
                }
            }
        }
        false
    }

    fn has_declare_child(&self, node_idx: NodeIndex, declare_kind: u16) -> bool {
        if let Some(node) = self.arena.get(node_idx) {
            for child_idx in self.arena.get_children(node_idx) {
                if let Some(child) = self.arena.get(child_idx) {
                    if child.kind == declare_kind {
                        return true;
                    }
                    if child.pos >= node.pos + 20 {
                        break;
                    }
                }
            }
        }
        false
    }
}
