//! Completion context and suppression filters.
//!
//! This module isolates lexical/AST heuristics that decide whether completions
//! should be shown at a cursor location.

use super::*;

impl<'a> Completions<'a> {
    /// Check if the cursor is inside a context where completions should not be offered,
    /// such as inside string literals (non-module-specifier), comments, or regex literals.
    pub(super) fn is_in_no_completion_context(&self, offset: u32) -> bool {
        if self.should_offer_constructor_keyword(offset) {
            return false;
        }
        if self.is_class_member_continuation_without_separator(offset) {
            return true;
        }
        if self.is_ambiguous_numeric_dot_context(offset) {
            return true;
        }
        if self.is_ambiguous_slash_dot_context(offset) {
            return true;
        }
        if self.is_ambiguous_slash_dot_context(offset) {
            return true;
        }
        // Check if we're at an identifier definition location first - this works
        // even when offset == source_text.len() (cursor at end of file).
        if self.is_at_definition_location(offset) {
            return true;
        }
        // JSX child text content is not a completion context. Matches TS's
        // `isInJsxText` blocker used by `isCompletionListBlocker`.
        if self.is_in_jsx_child_text(offset) {
            return true;
        }
        // Type argument position on a non-generic target yields no completions.
        // Matches TS's `getTypeArgumentConstraint` returning undefined for a
        // non-generic `TypeReference`, which produces an empty completion list.
        if self.is_in_type_argument_of_non_generic(offset) {
            return true;
        }

        // Check for comments before the offset >= len guard, since comments at
        // end-of-file (offset == len) should still suppress completions.
        let i = offset as usize;
        if i > 0 {
            // Check for line comments: if we find // before offset on same line
            let line_start = self.source_text[..i].rfind('\n').map_or(0, |p| p + 1);
            let line_prefix = &self.source_text[line_start..i];
            if line_prefix.contains("//") {
                // Check that the // is not inside a string
                let comment_pos = line_prefix
                    .find("//")
                    .expect("guarded by line_prefix.contains(\"//\")");
                let before_comment = &line_prefix[..comment_pos];
                let single_quotes = before_comment.chars().filter(|&c| c == '\'').count();
                let double_quotes = before_comment.chars().filter(|&c| c == '"').count();
                let backticks = before_comment.chars().filter(|&c| c == '`').count();
                if single_quotes % 2 == 0 && double_quotes % 2 == 0 && backticks % 2 == 0 {
                    return true;
                }
            }

            // Check for block comments: scan backwards for /* without matching */
            if let Some(block_start) = self.source_text[..i].rfind("/*") {
                let after_block = &self.source_text[block_start + 2..i];
                if !after_block.contains("*/") {
                    return true;
                }
            }

            // Text-based regex literal detection: after /pattern/ or /pattern/flags
            // This catches cases where cursor is at end-of-file after a regex.
            if self.text_is_inside_regex(i) {
                return true;
            }

            // Text-based template literal detection: inside backtick strings
            if self.text_is_inside_template_literal(i) {
                return true;
            }

            // Text-based string literal detection: inside unclosed quotes
            if self.text_is_inside_string_literal(i) {
                return true;
            }
        }

        // Check if we're inside a string literal, comment, or regex by examining
        // the source text character context around the offset.
        let bytes = self.source_text.as_bytes();
        let len = bytes.len();
        if offset as usize >= len {
            return false;
        }

        // Check if we're inside a numeric literal (including BigInt suffixed with 'n')
        // No completions should appear at the end of numeric literals like `0n`, `123`, `0xff`
        if offset > 0 {
            let check_offset = (offset - 1) as usize;
            if check_offset < len {
                let prev_byte = bytes[check_offset];
                // After a digit or 'n' suffix (BigInt), check if we're in a numeric literal
                if prev_byte.is_ascii_digit()
                    || prev_byte == b'n'
                    || prev_byte == b'x'
                    || prev_byte == b'o'
                    || prev_byte == b'b'
                {
                    let node_idx_check = find_node_at_offset(self.arena, offset.saturating_sub(1));
                    if node_idx_check.is_some()
                        && let Some(node) = self.arena.get(node_idx_check)
                        && (node.kind == SyntaxKind::NumericLiteral as u16
                            || node.kind == SyntaxKind::BigIntLiteral as u16)
                    {
                        // We're right after a numeric/BigInt literal
                        return true;
                    }
                }
            }
        }

        // Check if we're inside a string literal using the AST
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_some()
            && let Some(node) = self.arena.get(node_idx)
        {
            let kind = node.kind;
            // String literal (not inside an import/require module specifier)
            if kind == SyntaxKind::StringLiteral as u16 {
                // Check if parent is an import declaration's module specifier
                if let Some(ext) = self.arena.get_extended(node_idx) {
                    let parent = self.arena.get(ext.parent);
                    if let Some(p) = parent
                        && (p.kind == syntax_kind_ext::IMPORT_DECLARATION
                            || p.kind == syntax_kind_ext::EXPORT_DECLARATION
                            || p.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE)
                    {
                        return false; // Module specifier - allow completions
                    }
                }
                return true; // Regular string literal - no completions
            }
            // No-substitution template literal
            if kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16 {
                return true;
            }
            // Template head/middle/tail (inside template literal parts, not expressions)
            if kind == SyntaxKind::TemplateHead as u16
                || kind == SyntaxKind::TemplateMiddle as u16
                || kind == SyntaxKind::TemplateTail as u16
            {
                return true;
            }
            // Regular expression literal
            if kind == SyntaxKind::RegularExpressionLiteral as u16 {
                return true;
            }
        }

        false
    }

    fn is_class_member_continuation_without_separator(&self, offset: u32) -> bool {
        if !self.text_likely_in_class_body(offset) {
            return false;
        }
        let end = (offset as usize).min(self.source_text.len());
        let text = &self.source_text[..end];
        let line_start = text.rfind('\n').map_or(0, |idx| idx + 1);
        let line = &text[line_start..];
        let prefix = line.trim_end();
        if prefix.is_empty() {
            return false;
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
        if idx == 0 {
            return !word.starts_with("con");
        }
        let before = prefix[..idx].trim_end();
        if before.is_empty() {
            return !word.starts_with("con");
        }
        if before.contains('=') || before.contains(':') {
            return true;
        }
        !matches!(before.as_bytes().last().copied(), Some(b';') | Some(b'{'))
    }

    /// Check if the cursor is at a position where a new identifier is being defined.
    /// At these locations, completions should not be offered because the user is
    /// typing a new name, not referencing an existing one.
    pub(super) fn is_at_definition_location(&self, offset: u32) -> bool {
        // Use the full text up to cursor (including trailing whitespace)
        let text = &self.source_text[..offset as usize];

        // Strategy: look at what's right before the cursor. We need to handle:
        // 1. "var |" - cursor after keyword + space
        // 2. "var a|" - cursor after keyword + partial identifier
        // 3. "var a, |" - cursor after comma in declaration list
        // 4. "function foo(|" - cursor at parameter position

        // First, check the untrimmed text for trailing whitespace patterns
        // (cursor is after space following a keyword)
        let trimmed = text.trim_end();
        if trimmed.is_empty() {
            return false;
        }

        // Extract the last word from trimmed text
        let last_word_start = trimmed
            .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
            .map_or(0, |p| p + 1);
        let last_word = &trimmed[last_word_start..];

        // Check if we have whitespace after the last word (before cursor)
        let has_trailing_ws = text.len() > trimmed.len();
        let trailing_ws = if has_trailing_ws {
            &text[trimmed.len()..]
        } else {
            ""
        };
        let trailing_has_line_break = trailing_ws.contains('\n') || trailing_ws.contains('\r');

        let definition_keywords = [
            "var",
            "let",
            "const",
            "function",
            "class",
            "interface",
            "type",
            "enum",
            "namespace",
            "module",
            "infer",
        ];

        // Helper to check whole-word boundary
        let is_whole_word = |text: &str, kw: &str| -> bool {
            if !text.ends_with(kw) {
                return false;
            }
            let kw_start = text.len() - kw.len();
            kw_start == 0 || {
                let c = text.as_bytes()[kw_start - 1];
                !c.is_ascii_alphanumeric() && c != b'_' && c != b'$'
            }
        };

        // Case 1: "keyword |" - cursor after keyword + whitespace
        if has_trailing_ws
            && !trailing_has_line_break
            && definition_keywords
                .iter()
                .any(|kw| is_whole_word(trimmed, kw))
        {
            return true;
        }
        if has_trailing_ws && !trailing_has_line_break && !last_word.is_empty() {
            let before_word = trimmed[..last_word_start].trim_end();
            // "class Name |" or "interface Name |" - NOT a definition location,
            // this is where extends/implements goes.
            let is_heritage_position = matches!(
                before_word
                    .rsplit(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
                    .next()
                    .unwrap_or(""),
                "class" | "interface"
            ) && !definition_keywords.contains(&last_word);
            if !is_heritage_position
                && definition_keywords
                    .iter()
                    .any(|kw| is_whole_word(before_word, kw))
            {
                return true;
            }
            if before_word.ends_with('*') {
                let before_star = before_word.trim_end_matches('*').trim_end();
                if is_whole_word(before_star, "function") {
                    return true;
                }
            }
        }

        // Case 2: "keyword partialId|" - cursor while typing identifier after keyword
        if !has_trailing_ws && !last_word.is_empty() {
            let before_word = trimmed[..last_word_start].trim_end();
            if definition_keywords
                .iter()
                .any(|kw| is_whole_word(before_word, kw))
            {
                return true;
            }
            // "function* name|" - generator function name
            if before_word.ends_with('*') {
                let before_star = before_word.trim_end_matches('*').trim_end();
                if is_whole_word(before_star, "function") {
                    return true;
                }
            }
            // "...name|" in parameter list - rest parameter
            if before_word.ends_with("...") && self.is_in_parameter_list(offset) {
                return true;
            }
        }

        // The text before the cursor (or before the partial identifier being typed)
        let check_before = if has_trailing_ws {
            trimmed
        } else {
            trimmed[..last_word_start].trim_end()
        };

        // Case 3: comma in declarations: "var a, |", "function f(a, |", "<T, |"
        if check_before.ends_with(',') {
            // Try AST-based detection first, then text-based fallback
            if self.is_in_variable_declaration_list(offset)
                || self.text_looks_like_var_declaration_list(check_before)
            {
                return true;
            }
            if self.is_in_parameter_list(offset)
                || self.text_looks_like_parameter_list(check_before)
            {
                return true;
            }
            if self.is_in_type_parameter_list(offset)
                || self.text_looks_like_type_param_list(check_before)
            {
                return true;
            }
        }

        // Case 4: function parameter names at opening paren: "function foo(|"
        if check_before.ends_with('(')
            && (self.is_in_parameter_list(offset)
                || self.text_looks_like_parameter_list(check_before))
        {
            return true;
        }

        // Case 4b: "...name" in parameter list - rest parameter
        if has_trailing_ws && trimmed.ends_with("...") && self.is_in_parameter_list(offset) {
            return true;
        }

        // Case 5: catch clause: "catch (|" or "catch (x|"
        if check_before.ends_with("catch(") || check_before.ends_with("catch (") {
            return true;
        }
        if !has_trailing_ws && !last_word.is_empty() {
            let before_word_trimmed = trimmed[..last_word_start].trim_end();
            if before_word_trimmed.ends_with("catch(") || before_word_trimmed.ends_with("catch (") {
                return true;
            }
        }

        // Case 6: type parameter list opener: "class A<|", "interface B<|"
        if check_before.ends_with('<')
            && (self.is_in_type_parameter_list(offset)
                || self.text_looks_like_type_param_opener(check_before))
        {
            return true;
        }

        // Case 7: enum member position
        if self.is_in_enum_member_position(offset) {
            return true;
        }

        // Case 8: destructuring binding: "let { |" or "let [|"
        if self.is_in_binding_pattern_definition(offset) {
            return true;
        }

        false
    }

    /// Text-based heuristic to detect if we're in a var/let/const declaration list
    /// after a comma. This is a fallback for when the AST-based check fails due to
    /// parser error recovery.
    pub(super) fn text_looks_like_var_declaration_list(&self, text_before_comma: &str) -> bool {
        // Find the most recent var/let/const keyword by scanning backward.
        // Check that there's no statement boundary (`;`, `{`, `}`) between
        // the keyword and the comma that isn't inside a nested expression.
        let bytes = text_before_comma.as_bytes();
        let keywords: &[&str] = &["var ", "let ", "const "];

        for kw in keywords {
            // Search backward for this keyword
            let mut search_from = text_before_comma.len();
            while let Some(pos) = text_before_comma[..search_from].rfind(kw) {
                // Check word boundary before keyword
                if pos > 0 {
                    let c = bytes[pos - 1];
                    if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' {
                        search_from = pos;
                        continue;
                    }
                }
                // Check no unbalanced statement boundaries between keyword and comma
                let between = &text_before_comma[pos + kw.len()..];
                let mut brace_depth: i32 = 0;
                let mut paren_depth: i32 = 0;
                let mut _bracket_depth: i32 = 0;
                let mut has_boundary = false;
                for &b in between.as_bytes() {
                    match b {
                        b'{' => brace_depth += 1,
                        b'}' => {
                            brace_depth -= 1;
                            if brace_depth < 0 {
                                has_boundary = true;
                                break;
                            }
                        }
                        b'(' => paren_depth += 1,
                        b')' => paren_depth -= 1,
                        b'[' => _bracket_depth += 1,
                        b']' => _bracket_depth -= 1,
                        b';' if brace_depth == 0 && paren_depth == 0 => {
                            has_boundary = true;
                            break;
                        }
                        _ => {}
                    }
                }
                if !has_boundary && brace_depth == 0 {
                    return true;
                }
                search_from = pos;
            }
        }
        false
    }

    /// Text-based heuristic to detect if cursor is in a function/method parameter list.
    /// Only matches clearly identifiable declaration patterns to avoid false positives
    /// with function calls.
    pub(super) fn text_looks_like_parameter_list(&self, text_before: &str) -> bool {
        // Scan backward for an unmatched '('
        let mut paren_depth: i32 = 0;
        let bytes = text_before.as_bytes();
        for i in (0..bytes.len()).rev() {
            match bytes[i] {
                b')' => paren_depth += 1,
                b'(' => {
                    if paren_depth == 0 {
                        // Found unmatched '(' - check what's before it
                        let before_paren = text_before[..i].trim_end();
                        if before_paren.is_empty() {
                            return false;
                        }
                        let last_char = before_paren.as_bytes()[before_paren.len() - 1];
                        if last_char.is_ascii_alphanumeric()
                            || last_char == b'_'
                            || last_char == b'$'
                        {
                            let word_start = before_paren
                                .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
                                .map_or(0, |p| p + 1);
                            let word = &before_paren[word_start..];
                            let before_word = before_paren[..word_start].trim_end();
                            // "function foo(" or "function* foo("
                            if before_word.ends_with("function")
                                || before_word.ends_with("function*")
                            {
                                return true;
                            }
                            // "constructor(" pattern
                            if word == "constructor" {
                                return true;
                            }
                        }
                        // Could also have type params: "function foo<T>(" or "class.method<T>( "
                        if last_char == b'>' {
                            // Scan back past the type params to find identifier
                            let mut angle_depth: i32 = 0;
                            for j in (0..before_paren.len()).rev() {
                                match before_paren.as_bytes()[j] {
                                    b'>' => angle_depth += 1,
                                    b'<' => {
                                        angle_depth -= 1;
                                        if angle_depth == 0 {
                                            let before_angle = before_paren[..j].trim_end();
                                            if !before_angle.is_empty() {
                                                let ws = before_angle
                                                    .rfind(|c: char| {
                                                        !c.is_alphanumeric() && c != '_' && c != '$'
                                                    })
                                                    .map_or(0, |p| p + 1);
                                                let bw = before_angle[..ws].trim_end();
                                                if bw.ends_with("function")
                                                    || bw.ends_with("function*")
                                                {
                                                    return true;
                                                }
                                            }
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        return false;
                    }
                    paren_depth -= 1;
                }
                b';' | b'{' | b'}' if paren_depth == 0 => return false,
                _ => {}
            }
        }
        false
    }

    /// Text-based heuristic to detect if cursor is after a comma in a type parameter list.
    /// Looks for an unmatched '<' preceded by a type-parameterizable declaration.
    pub(super) fn text_looks_like_type_param_list(&self, text_before: &str) -> bool {
        // Scan backward for an unmatched '<'
        let mut angle_depth: i32 = 0;
        let bytes = text_before.as_bytes();
        for i in (0..bytes.len()).rev() {
            match bytes[i] {
                b'>' => angle_depth += 1,
                b'<' => {
                    if angle_depth == 0 {
                        // Found unmatched '<' - check if it's a type param opener
                        return Self::text_before_angle_is_type_param(&text_before[..i]);
                    }
                    angle_depth -= 1;
                }
                b';' | b'{' | b'}' => return false,
                _ => {}
            }
        }
        false
    }

    /// Text-based heuristic to detect if '<' at end of text opens a type parameter list.
    /// Pattern: "class A<", "interface B<", "function C<", "type D<", "f<" (method)
    pub(super) fn text_looks_like_type_param_opener(&self, text_ending_with_angle: &str) -> bool {
        let before_angle = text_ending_with_angle[..text_ending_with_angle.len() - 1].trim_end();
        Self::text_before_angle_is_type_param(before_angle)
    }

    pub(super) fn text_before_angle_is_type_param(before_angle: &str) -> bool {
        if before_angle.is_empty() {
            return false;
        }
        let last_char = before_angle.as_bytes()[before_angle.len() - 1];
        if !last_char.is_ascii_alphanumeric() && last_char != b'_' && last_char != b'$' {
            return false;
        }
        let word_start = before_angle
            .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
            .map_or(0, |p| p + 1);
        let before_word = before_angle[..word_start].trim_end();
        let type_param_keywords = ["class", "interface", "function", "type"];
        // "class A<", "interface B<", etc.
        for kw in &type_param_keywords {
            if before_word.ends_with(kw) {
                let kw_start = before_word.len() - kw.len();
                if kw_start == 0 || {
                    let c = before_word.as_bytes()[kw_start - 1];
                    !c.is_ascii_alphanumeric() && c != b'_' && c != b'$'
                } {
                    return true;
                }
            }
        }
        // Method in class body: any identifier followed by '<' could be a method
        // type parameter. Check if inside a class body by looking for '{' balance.
        // For simplicity, if we see an unbalanced '{' before the word, it could be
        // inside a class/interface body.
        let mut brace_depth: i32 = 0;
        for &b in before_word.as_bytes().iter().rev() {
            match b {
                b'}' => brace_depth += 1,
                b'{' => {
                    brace_depth -= 1;
                    if brace_depth < 0 {
                        // Inside a block - could be class body
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Check if offset is inside a destructuring binding pattern in a declaration
    pub(super) fn is_in_binding_pattern_definition(&self, offset: u32) -> bool {
        let node_idx = find_node_at_offset(self.arena, offset);
        let start = if node_idx.is_none() && offset > 0 {
            find_node_at_offset(self.arena, offset - 1)
        } else {
            node_idx
        };
        let mut current = start;
        let mut in_binding_pattern = false;
        let mut depth = 0;
        while current.is_some() && depth < 50 {
            if let Some(node) = self.arena.get(current) {
                if node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                {
                    in_binding_pattern = true;
                }
                if in_binding_pattern
                    && (node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                        || node.kind == syntax_kind_ext::VARIABLE_STATEMENT
                        || node.kind == syntax_kind_ext::PARAMETER)
                {
                    return true;
                }
            }
            if let Some(ext) = self.arena.get_extended(current) {
                if ext.parent == current {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
            depth += 1;
        }
        false
    }

    /// Check if offset is inside a var/let/const declaration list (for comma detection)
    pub(super) fn is_in_variable_declaration_list(&self, offset: u32) -> bool {
        let node_idx = find_node_at_offset(self.arena, offset);
        let start = if node_idx.is_none() && offset > 0 {
            find_node_at_offset(self.arena, offset - 1)
        } else {
            node_idx
        };
        let mut current = start;
        let mut depth = 0;
        while current.is_some() && depth < 50 {
            if let Some(node) = self.arena.get(current)
                && (node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                    || node.kind == syntax_kind_ext::VARIABLE_STATEMENT)
            {
                return true;
            }
            if let Some(ext) = self.arena.get_extended(current) {
                if ext.parent == current {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
            depth += 1;
        }
        false
    }

    /// Check if offset is inside a parameter list
    pub(super) fn is_in_parameter_list(&self, offset: u32) -> bool {
        let node_idx = find_node_at_offset(self.arena, offset);
        let start = if node_idx.is_none() && offset > 0 {
            find_node_at_offset(self.arena, offset - 1)
        } else {
            node_idx
        };
        let mut current = start;
        let mut depth = 0;
        while current.is_some() && depth < 50 {
            if let Some(node) = self.arena.get(current) {
                if node.kind == syntax_kind_ext::PARAMETER {
                    return true;
                }
                // Stop at function boundary
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.is_function_expression_or_arrow()
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                {
                    return false;
                }
            }
            if let Some(ext) = self.arena.get_extended(current) {
                if ext.parent == current {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
            depth += 1;
        }
        false
    }

    /// Check if offset is in a type parameter list `<T, U>`
    pub(super) fn is_in_type_parameter_list(&self, offset: u32) -> bool {
        let node_idx = find_node_at_offset(self.arena, offset);
        let start = if node_idx.is_none() && offset > 0 {
            find_node_at_offset(self.arena, offset - 1)
        } else {
            node_idx
        };
        let mut current = start;
        let mut depth = 0;
        while current.is_some() && depth < 50 {
            if let Some(node) = self.arena.get(current)
                && node.kind == syntax_kind_ext::TYPE_PARAMETER
            {
                return true;
            }
            if let Some(ext) = self.arena.get_extended(current) {
                if ext.parent == current {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
            depth += 1;
        }
        false
    }

    /// Check if offset is at an enum member name position
    pub(super) fn is_in_enum_member_position(&self, offset: u32) -> bool {
        let node_idx = find_node_at_offset(self.arena, offset);
        let start = if node_idx.is_none() && offset > 0 {
            find_node_at_offset(self.arena, offset - 1)
        } else {
            node_idx
        };
        let mut current = start;
        let mut depth = 0;
        while current.is_some() && depth < 50 {
            if let Some(node) = self.arena.get(current) {
                if node.kind == syntax_kind_ext::ENUM_MEMBER {
                    return true;
                }
                if node.kind == syntax_kind_ext::ENUM_DECLARATION {
                    // Check if cursor is after `{` and still within the enum body
                    if offset >= node.end {
                        return false; // Cursor is past the closing `}`
                    }
                    let text_before = &self.source_text[node.pos as usize..offset as usize];
                    if text_before.contains('{') {
                        return true;
                    }
                    return false;
                }
            }
            if let Some(ext) = self.arena.get_extended(current) {
                if ext.parent == current {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
            depth += 1;
        }
        false
    }

    /// Text-based check: is cursor inside a regex literal?
    /// Scans backward from `i` looking for an unmatched `/pattern/` or `/pattern/flags`.
    pub(super) fn text_is_inside_regex(&self, i: usize) -> bool {
        let text = &self.source_text[..i];
        // Strategy: scan backward from i looking for a `/`.
        // A regex literal is `/pattern/flags` where flags are [gimsuy]*.
        // We need to find the closing `/` and determine if we're in the flags portion.
        let bytes = text.as_bytes();

        // First check if cursor is right after potential regex flags
        let mut pos = i;
        // Skip back over potential regex flags
        while pos > 0
            && matches!(
                bytes[pos - 1],
                b'g' | b'i' | b'm' | b's' | b'u' | b'y' | b'd'
            )
        {
            pos -= 1;
        }

        // Now check if there's a `/` right before the flags position
        if pos > 0 && bytes[pos - 1] == b'/' {
            let slash_pos = pos - 1;
            // Scan backward to find the opening `/` of the regex
            if slash_pos > 0 {
                // Look for the opening slash by scanning backward
                let mut j = slash_pos - 1;
                loop {
                    if bytes[j] == b'/' {
                        // Found potential opening slash - check if it's actually a regex
                        // The character before the opening slash should be an operator, keyword,
                        // or start of line (not an identifier character or closing paren/bracket)
                        if j == 0 {
                            return true; // Start of file
                        }
                        let before = bytes[j - 1];
                        if before == b'='
                            || before == b'('
                            || before == b','
                            || before == b':'
                            || before == b';'
                            || before == b'!'
                            || before == b'&'
                            || before == b'|'
                            || before == b'?'
                            || before == b'{'
                            || before == b'}'
                            || before == b'['
                            || before == b'\n'
                            || before == b'\r'
                            || before == b'\t'
                            || before == b' '
                            || before == b'+'
                            || before == b'-'
                            || before == b'~'
                            || before == b'^'
                        {
                            return true;
                        }
                        break;
                    }
                    if bytes[j] == b'\n' || bytes[j] == b'\r' {
                        break; // Regex can't span lines
                    }
                    if j == 0 {
                        break;
                    }
                    j -= 1;
                }
            }
        }
        false
    }

    /// Text-based check: is cursor inside a template literal (backtick string)?
    /// Counts unescaped backticks before cursor; odd count means inside template.
    pub(super) fn text_is_inside_template_literal(&self, i: usize) -> bool {
        let text = &self.source_text[..i];
        let bytes = text.as_bytes();
        let mut backtick_count = 0;
        let mut j = 0;
        while j < bytes.len() {
            if bytes[j] == b'\\' {
                j += 2; // Skip escaped character
                continue;
            }
            if bytes[j] == b'`' {
                backtick_count += 1;
            }
            j += 1;
        }
        // If odd number of backticks, we're inside a template literal.
        // However, we might be inside a ${} expression within the template.
        if backtick_count % 2 == 0 {
            return false;
        }
        // We're inside a template. Check if we're inside a ${} expression.
        // Scan backward from cursor for `${` that isn't matched by `}`.
        let mut brace_depth: i32 = 0;
        let mut k = i;
        while k > 0 {
            k -= 1;
            if bytes[k] == b'\\' && k > 0 {
                k -= 1; // Skip escaped chars going backward (approximate)
                continue;
            }
            if bytes[k] == b'}' {
                brace_depth += 1;
            } else if bytes[k] == b'{' {
                if k > 0 && bytes[k - 1] == b'$' {
                    if brace_depth == 0 {
                        // We're inside a ${} expression, allow completions
                        return false;
                    }
                    brace_depth -= 1;
                    k -= 1; // Skip the $
                } else {
                    // Regular { - just balance
                    if brace_depth > 0 {
                        brace_depth -= 1;
                    }
                }
            } else if bytes[k] == b'`' {
                // Hit the opening backtick without being in an expression
                return true;
            }
        }
        true
    }

    fn is_ambiguous_slash_dot_context(&self, offset: u32) -> bool {
        let end = (offset as usize).min(self.source_text.len());
        if end == 0 {
            return false;
        }
        let prefix = self.source_text[..end].trim_end();
        let line_start = prefix.rfind('\n').map_or(0, |idx| idx + 1);
        let line = prefix[line_start..].trim_end();
        let Some(before_dot) = line.strip_suffix('.') else {
            return false;
        };
        let Some(before_slash) = before_dot.strip_suffix('/') else {
            return false;
        };
        let expr = before_slash.trim_end();
        !expr.contains('/')
    }

    fn is_ambiguous_numeric_dot_context(&self, offset: u32) -> bool {
        let end = (offset as usize).min(self.source_text.len());
        if end == 0 {
            return false;
        }
        let prefix = self.source_text[..end].trim_end();
        let line_start = prefix.rfind('\n').map_or(0, |idx| idx + 1);
        let line = prefix[line_start..].trim_end();
        let Some(before_dot) = line.strip_suffix('.') else {
            return false;
        };
        let expr = before_dot.trim_end();
        if expr.ends_with(')') || expr.is_empty() {
            return false;
        }
        let token_start = expr
            .rfind(|c: char| !(c.is_ascii_digit() || c == '_'))
            .map_or(0, |p| p + 1);
        let token = &expr[token_start..];
        if token.is_empty() || !token.chars().all(|c| c.is_ascii_digit() || c == '_') {
            return false;
        }
        let before_token = &expr[..token_start];
        if before_token.ends_with('.') {
            return false;
        }
        if before_token
            .chars()
            .next_back()
            .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
        {
            return false;
        }
        true
    }

    /// Text-based check: is cursor inside a string literal (single/double quotes)?
    pub(super) fn text_is_inside_string_literal(&self, i: usize) -> bool {
        let text = &self.source_text[..i];
        let bytes = text.as_bytes();
        // Track quote state by scanning from beginning
        let mut in_single = false;
        let mut in_double = false;
        let mut j = 0;
        while j < bytes.len() {
            if bytes[j] == b'\\' && (in_single || in_double) {
                j += 2; // Skip escaped character
                continue;
            }
            match bytes[j] {
                b'\'' if !in_double => in_single = !in_single,
                b'"' if !in_single => in_double = !in_double,
                b'\n' | b'\r' => {
                    // Newlines terminate string literals (unless escaped, handled above)
                    in_single = false;
                    in_double = false;
                }
                _ => {}
            }
            j += 1;
        }
        in_single || in_double
    }

    /// Check if the cursor is inside a function body (for keyword selection).
    pub(super) fn is_inside_function(&self, offset: u32) -> bool {
        let node_idx = find_node_at_offset(self.arena, offset);
        let start = if node_idx.is_none() && offset > 0 {
            find_node_at_offset(self.arena, offset.saturating_sub(1))
        } else {
            node_idx
        };
        let mut current = start;
        while current.is_some() {
            if let Some(node) = self.arena.get(current) {
                let k = node.kind;
                if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION
                    || k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::CONSTRUCTOR
                    || k == syntax_kind_ext::GET_ACCESSOR
                    || k == syntax_kind_ext::SET_ACCESSOR
                {
                    return true;
                }
            }
            if let Some(ext) = self.arena.get_extended(current) {
                if ext.parent == current {
                    break;
                }
                current = ext.parent;
            } else {
                break;
            }
        }
        false
    }

    /// Find the best node for completions at the given offset.
    /// When the cursor is in whitespace, finds the smallest containing scope node.
    pub(super) fn find_completions_node(&self, root: NodeIndex, offset: u32) -> NodeIndex {
        // Try exact offset first
        let mut node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_some() {
            return node_idx;
        }
        // Try offset-1 (common when cursor is right after a token boundary)
        if offset > 0 {
            node_idx = find_node_at_offset(self.arena, offset - 1);
            if node_idx.is_some() {
                return node_idx;
            }
        }
        // Fallback: find the smallest node whose range contains the offset
        // This handles whitespace inside blocks where pos <= offset < end
        let mut best = root;
        let mut best_len = u32::MAX;
        for (i, node) in self.arena.nodes.iter().enumerate() {
            if node.pos <= offset && node.end >= offset {
                let len = node.end - node.pos;
                if len < best_len {
                    best_len = len;
                    best = NodeIndex(i as u32);
                }
            }
        }
        best
    }

    /// Check if the cursor is inside a JSX element's child text content. This
    /// mirrors TypeScript's `isInJsxText` check used by
    /// `isCompletionListBlocker`. Returns `true` when:
    /// - The node at or just before the offset is a `JsxText` token, OR
    /// - The cursor is positioned immediately after the `>` that closes a
    ///   JSX opening, closing, or self-closing tag whose containing JSX
    ///   element expects child content at that position.
    pub(super) fn is_in_jsx_child_text(&self, offset: u32) -> bool {
        // 1) Check the node at the lookup offset first, then (for cursors at a
        //    token boundary) the node immediately before.
        let bytes = self.source_text.as_bytes();
        let len = bytes.len() as u32;
        let candidates = [offset, offset.saturating_sub(1)];
        for &probe in &candidates {
            if probe >= len {
                continue;
            }
            let node_idx = find_node_at_offset(self.arena, probe);
            if !node_idx.is_some() {
                continue;
            }
            let Some(node) = self.arena.get(node_idx) else {
                continue;
            };
            if node.kind == SyntaxKind::JsxText as u16 {
                return true;
            }
        }

        // 2) Text-based fallback for the "`<div>/*...*/<div/>`" case where
        //    the cursor sits between a `>` that closes a JSX tag and the
        //    next JSX child. The AST at that offset is ambiguous (the block
        //    comment lives inside the JSX element children), so we scan
        //    backward past whitespace and comments to find the governing
        //    `>` and confirm its enclosing JSX element context.
        let mut cursor = offset.min(len);
        loop {
            while cursor > 0 {
                let b = bytes[(cursor - 1) as usize];
                if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                    cursor -= 1;
                    continue;
                }
                break;
            }
            if cursor >= 2
                && bytes[(cursor - 2) as usize] == b'*'
                && bytes[(cursor - 1) as usize] == b'/'
            {
                let mut scan = cursor - 2;
                let mut found = false;
                while scan >= 2 {
                    if bytes[(scan - 2) as usize] == b'/' && bytes[(scan - 1) as usize] == b'*' {
                        cursor = scan - 2;
                        found = true;
                        break;
                    }
                    scan -= 1;
                }
                if found {
                    continue;
                }
                break;
            }
            break;
        }
        if cursor == 0 || bytes[(cursor - 1) as usize] != b'>' {
            return false;
        }
        // Find the node whose `end` equals `cursor`: that's the `>` of a JSX
        // opening/closing/self-closing element.
        let gt_end = cursor;
        for (i, node) in self.arena.nodes.iter().enumerate() {
            if node.end != gt_end {
                continue;
            }
            let kind = node.kind;
            let is_jsx_tag = kind == syntax_kind_ext::JSX_OPENING_ELEMENT
                || kind == syntax_kind_ext::JSX_CLOSING_ELEMENT
                || kind == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT;
            if !is_jsx_tag {
                continue;
            }
            // Confirm the containing element expects child content: walk up
            // ancestors to find a JsxElement/JsxFragment parent.
            let mut current = NodeIndex(i as u32);
            for _ in 0..6 {
                let Some(ext) = self.arena.get_extended(current) else {
                    break;
                };
                if !ext.parent.is_some() || ext.parent == current {
                    break;
                }
                current = ext.parent;
                if let Some(parent_node) = self.arena.get(current)
                    && (parent_node.kind == syntax_kind_ext::JSX_ELEMENT
                        || parent_node.kind == syntax_kind_ext::JSX_FRAGMENT)
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if the cursor is in a type argument of a type reference whose
    /// target is non-generic (no type parameters). Matches TypeScript's
    /// behavior where `getTypeArgumentConstraint(typeArg)` returns `undefined`
    /// for a non-generic `TypeReference`, yielding an empty completion list.
    pub(super) fn is_in_type_argument_of_non_generic(&self, offset: u32) -> bool {
        let node_idx = find_node_at_offset(self.arena, offset);
        let start = if !node_idx.is_some() && offset > 0 {
            find_node_at_offset(self.arena, offset - 1)
        } else {
            node_idx
        };
        let mut current = start;
        let mut depth = 0;
        while current.is_some() && depth < 20 {
            if let Some(node) = self.arena.get(current) {
                if node.kind == syntax_kind_ext::TYPE_REFERENCE {
                    if let Some(type_ref) = self.arena.get_type_ref(node)
                        && let Some(type_args) = type_ref.type_arguments.as_ref()
                    {
                        let in_args = type_args.nodes.iter().any(|&arg_idx| {
                            self.arena
                                .get(arg_idx)
                                .is_some_and(|arg| arg.pos <= offset && offset <= arg.end)
                        });
                        if in_args {
                            let Some(name) = self.arena.get_identifier_text(type_ref.type_name)
                            else {
                                return false;
                            };
                            if self.type_name_refers_to_non_generic(name) {
                                return true;
                            }
                        }
                    }
                    return false;
                }
                if node.kind == syntax_kind_ext::SOURCE_FILE {
                    return false;
                }
            }
            let Some(ext) = self.arena.get_extended(current) else {
                break;
            };
            if !ext.parent.is_some() || ext.parent == current {
                break;
            }
            current = ext.parent;
            depth += 1;
        }
        false
    }

    /// Resolve the given type name in the current file and determine whether
    /// its declaration(s) are non-generic. Returns `true` when the name is
    /// bound to an interface, class, or type alias whose declarations have
    /// no type parameters.
    fn type_name_refers_to_non_generic(&self, name: &str) -> bool {
        let Some(symbol_id) = self.binder.file_locals.get(name) else {
            return false;
        };
        let Some(symbol) = self.binder.symbols.get(symbol_id) else {
            return false;
        };
        let mut has_declaration = false;
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let k = decl_node.kind;
            if k == syntax_kind_ext::INTERFACE_DECLARATION {
                has_declaration = true;
                if let Some(iface) = self.arena.get_interface(decl_node)
                    && iface
                        .type_parameters
                        .as_ref()
                        .is_some_and(|params| !params.nodes.is_empty())
                {
                    return false;
                }
            } else if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION
            {
                has_declaration = true;
                if let Some(class_decl) = self.arena.get_class(decl_node)
                    && class_decl
                        .type_parameters
                        .as_ref()
                        .is_some_and(|params| !params.nodes.is_empty())
                {
                    return false;
                }
            } else if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                has_declaration = true;
                if let Some(alias) = self.arena.get_type_alias(decl_node)
                    && alias
                        .type_parameters
                        .as_ref()
                        .is_some_and(|params| !params.nodes.is_empty())
                {
                    return false;
                }
            } else {
                // Conservative: unknown declaration shapes (e.g. imports)
                // may be generic; avoid suppression in that case.
                return false;
            }
        }
        has_declaration
    }
}
