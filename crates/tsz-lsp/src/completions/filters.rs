//! Completion context and suppression filters.
//!
//! This module isolates lexical/AST heuristics that decide whether completions
//! should be shown at a cursor location.

use super::*;

impl<'a> Completions<'a> {
    /// Check if the cursor is inside a context where completions should not be offered,
    /// such as inside string literals (non-module-specifier), comments, or regex literals.
    pub(super) fn is_in_no_completion_context(&self, offset: u32) -> bool {
        // Check if we're at an identifier definition location first - this works
        // even when offset == source_text.len() (cursor at end of file).
        if self.is_at_definition_location(offset) {
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
                let comment_pos = line_prefix.find("//").unwrap();
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
            && definition_keywords
                .iter()
                .any(|kw| is_whole_word(trimmed, kw))
        {
            return true;
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
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
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
}
