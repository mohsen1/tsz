//! Pure string-level parsing helpers for JSDoc type annotations.
//!
//! These are associated functions on `CheckerState` (no `&self`/`&mut self`)
//! that perform nesting-aware string splitting, tag extraction, and typedef
//! definition parsing. No type resolution or checker state access.

use super::types::{
    JsdocCallbackInfo, JsdocPropertyTagInfo, JsdocTemplateParamInfo, JsdocTypedefInfo,
};
use crate::state::CheckerState;

type JSDocImportQueryMembers = Vec<(usize, String)>;

impl<'a> CheckerState<'a> {
    // -----------------------------------------------------------------
    // Low-level nesting-aware string splitting
    // -----------------------------------------------------------------

    pub(crate) fn jsdoc_balanced_braced_type_expr(rest: &str) -> Option<&str> {
        let rest = rest.trim_start();
        let (type_expr, _) = Self::parse_jsdoc_curly_type_expr(rest)?;
        Some(type_expr.trim())
    }

    /// Find the first occurrence of a character at the top level.
    pub(crate) fn find_top_level_char(s: &str, target: char) -> Option<usize> {
        let mut angle_depth = 0u32;
        let mut paren_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut square_depth = 0u32;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        for (i, ch) in s.char_indices() {
            if ch == '\'' && !in_double_quote {
                in_single_quote = !in_single_quote;
                continue;
            }
            if ch == '"' && !in_single_quote {
                in_double_quote = !in_double_quote;
                continue;
            }
            if in_single_quote || in_double_quote {
                continue;
            }
            if ch == target
                && angle_depth == 0
                && paren_depth == 0
                && brace_depth == 0
                && square_depth == 0
            {
                return Some(i);
            }
            match ch {
                '<' => angle_depth += 1,
                '>' if angle_depth > 0 => angle_depth -= 1,
                '(' => paren_depth += 1,
                ')' if paren_depth > 0 => paren_depth -= 1,
                '{' => brace_depth += 1,
                '}' if brace_depth > 0 => brace_depth -= 1,
                '[' => square_depth += 1,
                ']' if square_depth > 0 => square_depth -= 1,
                _ => {}
            }
        }
        None
    }

    /// Split a type expression on a top-level binary operator (`|` or `&`).
    pub(crate) fn split_top_level_binary(s: &str, op: char) -> Option<Vec<&str>> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut angle_depth = 0u32;
        let mut paren_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut square_depth = 0u32;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        for (i, ch) in s.char_indices() {
            match ch {
                '\'' if !in_double_quote => in_single_quote = !in_single_quote,
                '"' if !in_single_quote => in_double_quote = !in_double_quote,
                _ if in_single_quote || in_double_quote => continue,
                '<' => angle_depth += 1,
                '>' if angle_depth > 0 => angle_depth -= 1,
                '(' => paren_depth += 1,
                ')' if paren_depth > 0 => paren_depth -= 1,
                '{' => brace_depth += 1,
                '}' if brace_depth > 0 => brace_depth -= 1,
                '[' => square_depth += 1,
                ']' if square_depth > 0 => square_depth -= 1,
                c if c == op
                    && angle_depth == 0
                    && paren_depth == 0
                    && brace_depth == 0
                    && square_depth == 0 =>
                {
                    parts.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        if parts.is_empty() {
            return None; // no split found — not a binary type
        }
        parts.push(&s[start..]);
        Some(parts)
    }

    /// Split a comma-separated list of type arguments, respecting `<>`, `()`, `{}` nesting.
    pub(super) fn split_type_args_respecting_nesting(s: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut angle_depth = 0u32;
        let mut paren_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut square_depth = 0u32;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        for (i, ch) in s.char_indices() {
            match ch {
                '\'' if !in_double_quote => in_single_quote = !in_single_quote,
                '"' if !in_single_quote => in_double_quote = !in_double_quote,
                _ if in_single_quote || in_double_quote => continue,
                '<' => angle_depth += 1,
                '>' if angle_depth > 0 => angle_depth -= 1,
                '(' => paren_depth += 1,
                ')' if paren_depth > 0 => paren_depth -= 1,
                '{' => brace_depth += 1,
                '}' if brace_depth > 0 => brace_depth -= 1,
                '[' => square_depth += 1,
                ']' if square_depth > 0 => square_depth -= 1,
                ',' if angle_depth == 0
                    && paren_depth == 0
                    && brace_depth == 0
                    && square_depth == 0 =>
                {
                    parts.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        if start < s.len() {
            parts.push(&s[start..]);
        }
        parts
    }

    pub(super) fn split_jsdoc_type_param_constraint(type_param: &str) -> (&str, Option<&str>) {
        let trimmed = type_param.trim();
        for (idx, _) in trimmed.match_indices("extends") {
            let after_idx = idx + "extends".len();
            let before = trimmed[..idx].chars().next_back();
            let after = trimmed[after_idx..].chars().next();
            if before.is_some_and(char::is_whitespace) && after.is_some_and(char::is_whitespace) {
                let name = trimmed[..idx].trim();
                let constraint = trimmed[after_idx..].trim();
                if !name.is_empty() && !constraint.is_empty() {
                    return (name, Some(constraint));
                }
            }
        }
        (trimmed, None)
    }

    /// Split parameter list by commas at the top level.
    pub(super) fn split_top_level_params(s: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut angle_depth = 0u32;
        let mut paren_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut square_depth = 0u32;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        for (i, ch) in s.char_indices() {
            match ch {
                '\'' if !in_double_quote => in_single_quote = !in_single_quote,
                '"' if !in_single_quote => in_double_quote = !in_double_quote,
                _ if in_single_quote || in_double_quote => continue,
                '<' => angle_depth += 1,
                '>' if angle_depth > 0 => angle_depth -= 1,
                '(' => paren_depth += 1,
                ')' if paren_depth > 0 => paren_depth -= 1,
                '{' => brace_depth += 1,
                '}' if brace_depth > 0 => brace_depth -= 1,
                '[' => square_depth += 1,
                ']' if square_depth > 0 => square_depth -= 1,
                ',' if angle_depth == 0
                    && paren_depth == 0
                    && brace_depth == 0
                    && square_depth == 0 =>
                {
                    parts.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        if start < s.len() {
            parts.push(&s[start..]);
        }
        parts
    }

    /// Split object literal properties by ',' or ';' at the top level.
    pub(super) fn split_object_properties(s: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut angle_depth = 0u32;
        let mut paren_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut square_depth = 0u32;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        for (i, ch) in s.char_indices() {
            match ch {
                '\'' if !in_double_quote => in_single_quote = !in_single_quote,
                '"' if !in_single_quote => in_double_quote = !in_double_quote,
                _ if in_single_quote || in_double_quote => continue,
                '<' => angle_depth += 1,
                '>' if angle_depth > 0 => angle_depth -= 1,
                '(' => paren_depth += 1,
                ')' if paren_depth > 0 => paren_depth -= 1,
                '{' => brace_depth += 1,
                '}' if brace_depth > 0 => brace_depth -= 1,
                '[' => square_depth += 1,
                ']' if square_depth > 0 => square_depth -= 1,
                ',' | ';'
                    if angle_depth == 0
                        && paren_depth == 0
                        && brace_depth == 0
                        && square_depth == 0 =>
                {
                    parts.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        if start < s.len() {
            parts.push(&s[start..]);
        }
        parts
    }

    // -----------------------------------------------------------------
    // JSDoc tag / expression extraction
    // -----------------------------------------------------------------

    /// Check if a JSDoc comment string contains a specific `@tag`.
    pub(crate) fn jsdoc_contains_tag(jsdoc: &str, tag_name: &str) -> bool {
        Self::jsdoc_tag_offset(jsdoc, tag_name).is_some()
    }

    pub(crate) fn jsdoc_tag_offset(jsdoc: &str, tag_name: &str) -> Option<usize> {
        let needle = format!("@{tag_name}");
        for pos_match in jsdoc.match_indices(&needle) {
            let after = pos_match.0 + needle.len();
            if after >= jsdoc.len() {
                return Some(pos_match.0);
            }
            let next_ch = jsdoc[after..]
                .chars()
                .next()
                .expect("after < jsdoc.len() checked above");
            if !next_ch.is_ascii_alphanumeric() && next_ch != '_' {
                return Some(pos_match.0);
            }
        }
        None
    }

    /// All offsets at which `@<tag_name>` appears in `jsdoc`, restricted to
    /// occurrences followed by a tag boundary (end of string, whitespace,
    /// `{`, or any other non-identifier character). The naive
    /// `s.find("@tag")` skips this check and treats `@tagx` as `@tag`.
    pub(crate) fn jsdoc_tag_offsets(jsdoc: &str, tag_name: &str) -> Vec<usize> {
        let needle = format!("@{tag_name}");
        let mut positions = Vec::new();
        for pos_match in jsdoc.match_indices(&needle) {
            let after = pos_match.0 + needle.len();
            let is_boundary = match jsdoc[after..].chars().next() {
                None => true,
                Some(c) => !c.is_ascii_alphanumeric() && c != '_',
            };
            if is_boundary {
                positions.push(pos_match.0);
            }
        }
        positions
    }

    /// Returns true when `line` begins with `@<tag_name>` followed by a tag
    /// boundary. Use instead of `line.starts_with("@tag")` when the line may
    /// also start with longer `@tagx` identifiers.
    pub(crate) fn jsdoc_line_starts_with_tag(line: &str, tag_name: &str) -> bool {
        Self::strip_jsdoc_tag_prefix(line, tag_name).is_some()
    }

    pub(crate) fn line_starts_with_jsdoc_tag(line: &str, tag_name: &str) -> bool {
        Self::jsdoc_line_starts_with_tag(line, tag_name)
    }

    pub(crate) fn strip_jsdoc_return_tag_prefix(text: &str) -> Option<&str> {
        Self::strip_jsdoc_tag_prefix(text, "returns")
            .or_else(|| Self::strip_jsdoc_tag_prefix(text, "return"))
    }

    pub(crate) fn extract_jsdoc_type_expression(jsdoc: &str) -> Option<&str> {
        let typedef_pos = Self::jsdoc_tag_offset(jsdoc, "typedef");
        let tag_pos = Self::jsdoc_tag_offset(jsdoc, "type");
        if let Some(pos) = tag_pos
            && let Some(td_pos) = typedef_pos
            && td_pos < pos
        {
            let typedef_rest = &jsdoc[td_pos + "@typedef".len()..pos];
            let mut has_non_object_base = false;
            if let Some(open) = typedef_rest.find('{')
                && let Some(close) = typedef_rest[open..].find('}')
            {
                let base = typedef_rest[open + 1..open + close].trim();
                if base != "Object" && base != "object" && !base.is_empty() {
                    has_non_object_base = true;
                }
            }
            if !has_non_object_base {
                return None; // The @type is absorbed by the @typedef
            }
        }
        let tag_pos = tag_pos?;
        let rest = &jsdoc[tag_pos + "@type".len()..];
        // Try braced form first: @type {expression}
        let rest_trimmed = rest.trim_start();
        if let Some(after_open) = rest_trimmed.strip_prefix('{') {
            let mut depth = 1usize;
            let mut end_idx = None;
            for (i, ch) in after_open.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end_idx = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(end_idx) = end_idx {
                return Some(after_open[..end_idx].trim());
            }
        }
        // Braceless form: @type expression (rest of line after whitespace)
        // Used in tsc for inline types like `@type () => string` or
        // `@type ({ type: 'foo' } | { type: 'bar' }) & { prop: number }`.
        let rest = rest.trim_start();
        if rest.is_empty() || rest.starts_with('@') || rest.starts_with('*') {
            return None;
        }
        // Take the rest of the line (up to end-of-line, closing comment, or next @tag)
        let end = rest
            .find('\n')
            .or_else(|| rest.find("*/"))
            .unwrap_or(rest.len());
        let expr = rest[..end].trim().trim_end_matches('*').trim();
        if expr.is_empty() { None } else { Some(expr) }
    }

    pub(crate) fn extract_jsdoc_enum_type_expression(jsdoc: &str) -> Option<&str> {
        let tag_pos = jsdoc.find("@enum")?;
        let rest = &jsdoc[tag_pos + "@enum".len()..];
        let rest_trimmed = rest.trim_start();
        if let Some(after_open) = rest_trimmed.strip_prefix('{') {
            let mut depth = 1usize;
            let mut end_idx = None;
            for (i, ch) in after_open.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end_idx = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(end_idx) = end_idx {
                return Some(after_open[..end_idx].trim());
            }
        }

        let rest = rest.trim_start();
        if rest.is_empty() || rest.starts_with('@') || rest.starts_with('*') {
            return None;
        }
        let end = rest
            .find('\n')
            .or_else(|| rest.find("*/"))
            .unwrap_or(rest.len());
        let expr = rest[..end].trim().trim_end_matches('*').trim();
        if expr.is_empty() { None } else { Some(expr) }
    }

    pub(super) fn extract_jsdoc_satisfies_expression(jsdoc: &str) -> Option<&str> {
        let tag_pos = Self::jsdoc_tag_offset(jsdoc, "satisfies")?;
        let rest = &jsdoc[tag_pos + "@satisfies".len()..];
        let rest = rest.trim_start();
        let after_open = rest.strip_prefix('{')?;
        let mut depth = 1usize;
        let mut end_idx = None;
        for (i, ch) in after_open.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end_idx = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end_idx = end_idx?;
        Some(after_open[..end_idx].trim())
    }

    pub(super) fn parse_jsdoc_import_type(type_expr: &str) -> Option<(String, Option<String>)> {
        let expr = type_expr.trim();
        let rest = expr.strip_prefix("import(")?;
        let mut rest = rest.trim_start();
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        rest = &rest[quote.len_utf8()..];
        let close_quote = rest.find(quote)?;
        let module_specifier = rest[..close_quote].trim().to_string();
        let after_quote = rest[close_quote + quote.len_utf8()..].trim_start();
        let after_quote = after_quote.strip_prefix(')')?.trim_start();
        if after_quote.is_empty() {
            return Some((module_specifier, None));
        }
        let after_dot = after_quote.strip_prefix('.')?;
        let after_dot = after_dot.trim_start();
        if let Some(member_name) = Self::parse_jsdoc_string_literal(after_dot) {
            return Some((module_specifier, Some(member_name)));
        }
        let mut end = 0usize;
        for (idx, ch) in after_dot.char_indices() {
            if idx == 0 {
                if !ch.is_ascii_alphabetic() && ch != '_' && ch != '$' {
                    return None;
                }
            } else if !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$' {
                break;
            }
            end = idx + ch.len_utf8();
        }
        if end == 0 {
            return None;
        }
        Some((module_specifier, Some(after_dot[..end].to_string())))
    }

    pub(super) fn parse_jsdoc_typeof_import_query(
        type_expr: &str,
    ) -> Option<(String, JSDocImportQueryMembers)> {
        let expr = type_expr.trim();
        let mut cursor = "typeof".len();
        let bytes = expr.as_bytes();
        if !expr.starts_with("typeof") {
            return None;
        }
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if !expr[cursor..].starts_with("import(") {
            return None;
        }
        cursor += "import(".len();
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let quote = *bytes.get(cursor)?;
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        cursor += 1;
        let module_start = cursor;
        while cursor < bytes.len() && bytes[cursor] != quote {
            cursor += 1;
        }
        let module_specifier = expr[module_start..cursor].trim().to_string();
        if cursor >= bytes.len() {
            return None;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if *bytes.get(cursor)? != b')' {
            return None;
        }
        cursor += 1;

        let mut segments = Vec::new();
        while cursor < bytes.len() {
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor >= bytes.len() {
                break;
            }
            if bytes[cursor] != b'.' {
                return None;
            }
            cursor += 1;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            let segment_start = cursor;
            let first = *bytes.get(cursor)?;
            if !first.is_ascii_alphabetic() && first != b'_' && first != b'$' {
                return None;
            }
            cursor += 1;
            while cursor < bytes.len() {
                let ch = bytes[cursor];
                if !ch.is_ascii_alphanumeric() && ch != b'_' && ch != b'$' {
                    break;
                }
                cursor += 1;
            }
            segments.push((segment_start, expr[segment_start..cursor].to_string()));
        }

        Some((module_specifier, segments))
    }

    pub(super) fn jsdoc_backtick_import_argument_offset(type_expr: &str) -> Option<usize> {
        let mut search_from = 0usize;
        while let Some(import_offset) = type_expr[search_from..].find("import(") {
            let mut cursor = search_from + import_offset + "import(".len();
            while cursor < type_expr.len() && type_expr.as_bytes()[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if type_expr.as_bytes().get(cursor).copied() == Some(b'`') {
                return Some(cursor);
            }
            search_from = cursor.saturating_add(1);
        }
        None
    }

    pub(super) fn jsdoc_template_constraints(jsdoc: &str) -> Vec<(String, Option<String>)> {
        let mut out = Vec::new();
        for raw_line in jsdoc.lines() {
            let trimmed = raw_line.trim().trim_start_matches('*').trim();
            let Some(rest) = Self::strip_jsdoc_tag_prefix(trimmed, "template") else {
                continue;
            };
            let rest = rest.trim();
            let (constraint, names_str) = if let Some(rest) = rest.strip_prefix('{') {
                let mut depth = 1usize;
                let mut close_idx = None;
                for (idx, ch) in rest.char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                close_idx = Some(idx);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(close_idx) = close_idx {
                    (
                        Some(rest[..close_idx].trim().to_string()),
                        rest[close_idx + 1..].trim(),
                    )
                } else {
                    (None, rest)
                }
            } else {
                (None, rest)
            };
            let mut cursor = 0usize;
            let bytes = names_str.as_bytes();
            let mut parsed_any = false;
            let mut applied_constraint = false;
            while cursor < bytes.len() {
                let mut saw_comma = false;
                while cursor < bytes.len() {
                    let ch = bytes[cursor] as char;
                    if ch == ',' {
                        saw_comma = true;
                        cursor += 1;
                    } else if ch.is_ascii_whitespace() {
                        cursor += 1;
                    } else {
                        break;
                    }
                }
                if cursor >= bytes.len() {
                    break;
                }

                let start = cursor;
                while cursor < bytes.len() {
                    let ch = bytes[cursor] as char;
                    if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                        cursor += 1;
                    } else {
                        break;
                    }
                }
                if start == cursor {
                    break;
                }

                let name = &names_str[start..cursor];

                // Skip `const` modifier keyword (e.g., `@template const T`).
                if name == "const" {
                    continue;
                }
                // Skip variance modifier keywords (e.g., `@template in T`,
                // `@template out T`). Mirror the skip in
                // `jsdoc_template_type_params` — see that fn's comment.
                if name == "in" || name == "out" {
                    continue;
                }

                if parsed_any && !saw_comma {
                    break;
                }

                let mut lookahead = cursor;
                while lookahead < bytes.len() && (bytes[lookahead] as char).is_ascii_whitespace() {
                    lookahead += 1;
                }

                if parsed_any
                    && !saw_comma
                    && lookahead < bytes.len()
                    && bytes[lookahead] as char != ','
                {
                    break;
                }

                let name_constraint = if applied_constraint {
                    None
                } else {
                    constraint.clone()
                };
                out.push((name.to_string(), name_constraint));
                parsed_any = true;
                applied_constraint = true;
                cursor = lookahead;
            }
        }
        out
    }

    pub(super) fn jsdoc_template_constraints_before_typedef_host(
        jsdoc: &str,
    ) -> Vec<(String, Option<String>)> {
        // tsc accepts @template tags AFTER a single @typedef in the same
        // JSDoc comment (templates bind to the typedef host). It does NOT
        // extend that grace to @callback or @overload — TS8039 fires for
        // misplaced templates after those tags. Match that policy: when the
        // only host in the block is a single @typedef, scan the whole block
        // for @template; otherwise restrict to the prefix before any host.
        let normalize = |raw: &str| -> String {
            raw.trim()
                .trim_start_matches("/**")
                .trim_start_matches("/*")
                .trim_start_matches('*')
                .trim()
                .trim_end_matches("*/")
                .trim()
                .to_string()
        };
        let mut typedef_count = 0usize;
        let mut other_host_count = 0usize;
        for raw_line in jsdoc.lines() {
            let trimmed = normalize(raw_line);
            if Self::jsdoc_line_starts_with_tag(&trimmed, "typedef") {
                typedef_count += 1;
            } else if Self::jsdoc_line_starts_with_tag(&trimmed, "callback")
                || Self::jsdoc_line_starts_with_tag(&trimmed, "overload")
            {
                other_host_count += 1;
            }
        }
        if typedef_count == 1 && other_host_count == 0 {
            return Self::jsdoc_template_constraints(jsdoc);
        }
        let mut prefix = String::new();
        for raw_line in jsdoc.lines() {
            let trimmed = normalize(raw_line);
            if Self::jsdoc_line_starts_with_tag(&trimmed, "typedef")
                || Self::jsdoc_line_starts_with_tag(&trimmed, "callback")
                || Self::jsdoc_line_starts_with_tag(&trimmed, "overload")
            {
                break;
            }
            prefix.push_str(raw_line);
            prefix.push('\n');
        }
        Self::jsdoc_template_constraints(&prefix)
    }

    // -----------------------------------------------------------------
    // JSDoc typedef / callback / import parsing
    // -----------------------------------------------------------------

    pub(crate) fn parse_jsdoc_typedefs(jsdoc: &str) -> Vec<(String, JsdocTypedefInfo)> {
        let mut typedefs = Vec::new();
        let mut current_name: Option<String> = None;
        let mut current_info = JsdocTypedefInfo {
            base_type: None,
            properties: Vec::new(),
            template_params: Vec::new(),
            callback: None,
        };
        let mut wrapped_typedef_body: Option<Vec<String>> = None;
        let template_params: Vec<JsdocTemplateParamInfo> =
            Self::jsdoc_template_constraints_before_typedef_host(jsdoc)
                .into_iter()
                .map(|(name, constraint)| JsdocTemplateParamInfo { name, constraint })
                .collect();
        for raw_line in jsdoc.lines() {
            // Normalize both multiline (`/** ... */`) and single-line (`/** ... */`)
            // JSDoc block-comment lines into tag content before parsing.
            let mut line = raw_line.trim();
            line = line
                .trim_start_matches("/**")
                .trim_start_matches("/*")
                .trim_start_matches('*')
                .trim();
            line = line.trim_end_matches("*/").trim();
            if let Some(body_lines) = wrapped_typedef_body.as_mut() {
                if line.is_empty() {
                    continue;
                }
                if let Some(rest) = line.strip_prefix("}}") {
                    let name = Self::normalize_jsdoc_typedef_name(rest.trim());
                    if !name.is_empty() {
                        if let Some(previous_name) = current_name.take() {
                            typedefs.push((previous_name, current_info));
                            current_info = JsdocTypedefInfo {
                                base_type: None,
                                properties: Vec::new(),
                                template_params: Vec::new(),
                                callback: None,
                            };
                        }
                        let base_type = format!("{{ {} }}", body_lines.join(", "));
                        current_name = Some(name);
                        current_info.base_type = Some(base_type);
                        current_info.properties.clear();
                        current_info.template_params = template_params.clone();
                        current_info.callback = None;
                    }
                    wrapped_typedef_body = None;
                    continue;
                }

                let body_line = line.trim_end_matches(',').trim_end_matches(';').trim();
                if !body_line.is_empty() {
                    body_lines.push(body_line.to_string());
                }
                continue;
            }

            if line.is_empty() || !line.starts_with('@') {
                continue;
            }
            if let Some(rest) = Self::strip_jsdoc_tag_prefix(line, "import") {
                for (local_name, specifier, import_name) in Self::parse_jsdoc_import_tag(rest) {
                    let import_type = if import_name == "*" || import_name == "default" {
                        format!("import(\"{specifier}\")")
                    } else if Self::is_jsdoc_import_identifier_name(&import_name) {
                        format!("import(\"{specifier}\").{import_name}")
                    } else {
                        format!(
                            "import(\"{specifier}\").{}",
                            Self::quote_jsdoc_import_member_name(&import_name)
                        )
                    };
                    if let Some(previous_name) = current_name.take() {
                        typedefs.push((previous_name, current_info));
                        current_info = JsdocTypedefInfo {
                            base_type: None,
                            properties: Vec::new(),
                            template_params: Vec::new(),
                            callback: None,
                        };
                    }
                    typedefs.push((
                        local_name,
                        JsdocTypedefInfo {
                            base_type: Some(import_type),
                            properties: Vec::new(),
                            template_params: Vec::new(),
                            callback: None,
                        },
                    ));
                }
                continue;
            }
            if let Some(tag_idx) = Self::jsdoc_tag_offset(line, "typedef") {
                let rest = line[tag_idx + "@typedef".len()..].trim();
                if rest.starts_with("{{") && Self::parse_jsdoc_curly_type_expr(rest).is_none() {
                    if let Some(previous_name) = current_name.take() {
                        typedefs.push((previous_name, current_info));
                        current_info = JsdocTypedefInfo {
                            base_type: None,
                            properties: Vec::new(),
                            template_params: Vec::new(),
                            callback: None,
                        };
                    }
                    wrapped_typedef_body = Some(Vec::new());
                    let initial_body = rest.trim_start_matches("{{").trim();
                    if !initial_body.is_empty() {
                        wrapped_typedef_body
                            .as_mut()
                            .expect("wrapped typedef body just initialized")
                            .push(
                                initial_body
                                    .trim_end_matches(',')
                                    .trim_end_matches(';')
                                    .trim()
                                    .to_string(),
                            );
                    }
                    continue;
                }

                if let Some((name, base_type)) = Self::parse_jsdoc_typedef_definition(rest) {
                    if let Some(previous_name) = current_name.take() {
                        typedefs.push((previous_name, current_info));
                        current_info = JsdocTypedefInfo {
                            base_type: None,
                            properties: Vec::new(),
                            template_params: Vec::new(),
                            callback: None,
                        };
                    }
                    current_name = Some(name);
                    current_info.base_type = base_type;
                    current_info.properties.clear();
                    current_info.template_params = template_params.clone();
                    current_info.callback = None;
                }
                continue;
            }
            if let Some(tag_idx) = Self::jsdoc_tag_offset(line, "callback") {
                let name = line[tag_idx + "@callback".len()..].trim().to_string();
                if !name.is_empty()
                    && name
                        .chars()
                        .all(|ch| ch == '_' || ch == '$' || ch == '.' || ch.is_ascii_alphanumeric())
                {
                    if let Some(previous_name) = current_name.take() {
                        typedefs.push((previous_name, current_info));
                    }
                    current_name = Some(name);
                    current_info = JsdocTypedefInfo {
                        base_type: None,
                        properties: Vec::new(),
                        template_params: template_params.clone(),
                        callback: Some(JsdocCallbackInfo {
                            params: Vec::new(),
                            return_type: None,
                            predicate: None,
                        }),
                    };
                }
                continue;
            }
            if current_info.callback.is_some() {
                if let Some(rest) = Self::strip_jsdoc_tag_prefix(line, "param") {
                    if let Some(param_info) = Self::parse_jsdoc_param_tag(rest)
                        && let Some(ref mut cb) = current_info.callback
                    {
                        cb.params.push(param_info);
                    }
                    continue;
                }
                if let Some(rest) = Self::strip_jsdoc_return_tag_prefix(line) {
                    let rest = rest.trim();
                    if let Some(type_expr) = Self::jsdoc_balanced_braced_type_expr(rest) {
                        let predicate =
                            Self::jsdoc_returns_type_predicate_from_type_expr(type_expr);
                        if let Some(ref mut cb) = current_info.callback {
                            cb.return_type = Some(type_expr.to_string());
                            cb.predicate = predicate;
                        }
                    }
                    continue;
                }
            }
            if let Some(prop) = Self::parse_jsdoc_property_type(line)
                && current_name.is_some()
            {
                current_info.properties.push(prop);
            }
        }
        if let Some(previous_name) = current_name.take() {
            typedefs.push((previous_name, current_info));
        }
        typedefs
    }

    /// Parse a type predicate from a JSDoc type expression (`x is T`, `asserts x is T`).
    pub(crate) fn jsdoc_returns_type_predicate_from_type_expr(
        type_expr: &str,
    ) -> Option<(bool, String, Option<String>)> {
        let (is_asserts, remainder) = Self::split_jsdoc_asserts_prefix(type_expr);
        if let Some((is_pos, is_end)) = Self::find_jsdoc_type_predicate_is(remainder) {
            let param_name = remainder[..is_pos].trim();
            let type_str = remainder[is_end..].trim();
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
        None
    }

    pub(super) fn split_jsdoc_asserts_prefix(type_expr: &str) -> (bool, &str) {
        let trimmed = type_expr.trim_start();
        let Some(after_asserts) = trimmed.strip_prefix("asserts") else {
            return (false, type_expr);
        };
        if after_asserts
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            (true, after_asserts.trim_start())
        } else {
            (false, type_expr)
        }
    }

    pub(super) fn find_jsdoc_type_predicate_is(type_expr: &str) -> Option<(usize, usize)> {
        for (idx, ch) in type_expr.char_indices() {
            if ch != 'i' || !type_expr[idx..].starts_with("is") {
                continue;
            }
            let after = idx + "is".len();
            let before_is_whitespace = type_expr[..idx]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
            let after_is_whitespace = type_expr[after..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace);
            if before_is_whitespace && after_is_whitespace {
                return Some((idx, after));
            }
        }
        None
    }

    pub(super) fn parse_jsdoc_import_tag(rest: &str) -> Vec<(String, String, String)> {
        let rest = rest.trim();
        let mut results = Vec::new();
        if let Some(from_idx) = Self::find_jsdoc_import_from_keyword(rest) {
            let before_from = rest[..from_idx].trim();
            if matches!(
                before_from.split_whitespace().next(),
                Some("type" | "defer")
            ) && before_from.contains(char::is_whitespace)
            {
                return results;
            }
            let after_from = rest[from_idx + 4..].trim();
            let quote = after_from.chars().next().unwrap_or(' ');
            if quote == '"' || quote == '\'' || quote == '`' {
                let specifier = after_from[1..]
                    .split(quote)
                    .next()
                    .unwrap_or("")
                    .to_string();
                if before_from.starts_with('{') && before_from.ends_with('}') {
                    let inner = &before_from[1..before_from.len() - 1];
                    for part in Self::split_type_args_respecting_nesting(inner) {
                        let part = part.trim();
                        if part.is_empty() {
                            continue;
                        }
                        if let Some((imported_name, local_name)) =
                            Self::split_jsdoc_import_as_keyword(part)
                        {
                            let imported_name = Self::normalize_jsdoc_import_name(imported_name);
                            results.push((
                                local_name.to_string(),
                                specifier.clone(),
                                imported_name,
                            ));
                        } else {
                            let imported_name = Self::normalize_jsdoc_import_name(part);
                            results.push((imported_name.clone(), specifier.clone(), imported_name));
                        }
                    }
                } else if let Some(("*", ns_name)) =
                    Self::split_jsdoc_import_as_keyword(before_from)
                {
                    let ns_name = ns_name.to_string();
                    if !ns_name.is_empty() {
                        results.push((ns_name, specifier, "*".to_string()));
                    }
                } else {
                    let default_name = before_from.to_string();
                    if !default_name.is_empty() {
                        results.push((default_name, specifier, "default".to_string()));
                    }
                }
            }
        }
        results
    }

    fn find_jsdoc_import_from_keyword(rest: &str) -> Option<usize> {
        let mut quote = None;
        let mut escaped = false;
        let mut last_from = None;

        for (idx, ch) in rest.char_indices() {
            if let Some(active_quote) = quote {
                if escaped {
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == active_quote {
                    quote = None;
                }
                continue;
            }

            if ch == '"' || ch == '\'' || ch == '`' {
                quote = Some(ch);
                continue;
            }

            if rest[idx..].starts_with("from")
                && !rest[..idx]
                    .chars()
                    .next_back()
                    .is_some_and(Self::is_jsdoc_import_keyword_part)
                && !rest[idx + 4..]
                    .chars()
                    .next()
                    .is_some_and(Self::is_jsdoc_import_keyword_part)
            {
                last_from = Some(idx);
            }
        }

        last_from
    }

    const fn is_jsdoc_import_keyword_part(ch: char) -> bool {
        ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
    }

    fn is_jsdoc_import_identifier_name(name: &str) -> bool {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        (first.is_ascii_alphabetic() || first == '_' || first == '$')
            && chars.all(Self::is_jsdoc_import_keyword_part)
    }

    fn quote_jsdoc_import_member_name(name: &str) -> String {
        let mut quoted = String::from("\"");
        for ch in name.chars() {
            match ch {
                '"' => quoted.push_str("\\\""),
                '\\' => quoted.push_str("\\\\"),
                '\n' => quoted.push_str("\\n"),
                '\r' => quoted.push_str("\\r"),
                '\t' => quoted.push_str("\\t"),
                _ => quoted.push(ch),
            }
        }
        quoted.push('"');
        quoted
    }

    /// Split a JSDoc `@import` clause at the `as` keyword.
    ///
    /// Recognizes the keyword when it is bounded on both sides by any JS
    /// whitespace, matching tsc's tokenization. Returns `None` if no
    /// whitespace-bounded `as` appears, or if either side would be empty.
    fn split_jsdoc_import_as_keyword(part: &str) -> Option<(&str, &str)> {
        let mut quote = None;
        let mut escaped = false;
        for (abs, ch) in part.char_indices() {
            if let Some(active_quote) = quote {
                if escaped {
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == active_quote {
                    quote = None;
                }
                continue;
            }

            if ch == '"' || ch == '\'' || ch == '`' {
                quote = Some(ch);
                continue;
            }

            if !part[abs..].starts_with("as") {
                continue;
            }
            let before_ok = part[..abs]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
            let after_idx = abs + 2;
            let after_ok = part[after_idx..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace);
            if before_ok && after_ok {
                let imported = part[..abs].trim();
                let local = part[after_idx..].trim();
                if !imported.is_empty() && !local.is_empty() {
                    return Some((imported, local));
                }
            }
        }
        None
    }

    fn normalize_jsdoc_import_name(name: &str) -> String {
        Self::parse_jsdoc_string_literal(name).unwrap_or_else(|| name.trim().to_string())
    }

    fn parse_jsdoc_string_literal(text: &str) -> Option<String> {
        let text = text.trim();
        let quote = text.chars().next()?;
        if quote != '"' && quote != '\'' && quote != '`' {
            return None;
        }

        let mut value = String::new();
        let mut escaped = false;
        let mut close_end = None;
        for (idx, ch) in text[quote.len_utf8()..].char_indices() {
            if escaped {
                value.push(match ch {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    _ => ch,
                });
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == quote {
                close_end = Some(quote.len_utf8() + idx + ch.len_utf8());
                break;
            }
            value.push(ch);
        }

        let close_end = close_end?;
        if !text[close_end..].trim().is_empty() {
            return None;
        }
        Some(value)
    }

    pub(super) fn parse_jsdoc_typedef_definition(line: &str) -> Option<(String, Option<String>)> {
        let mut rest = line.trim();
        if rest.is_empty() {
            return None;
        }
        let base_type = if rest.starts_with('{') {
            let (expr, after_expr) = Self::parse_jsdoc_curly_type_expr(rest)?;
            rest = after_expr.trim();
            Some(expr.trim().to_string())
        } else {
            None
        };
        let name = rest.split_whitespace().next()?;
        Some((Self::normalize_jsdoc_typedef_name(name), base_type))
    }

    fn normalize_jsdoc_typedef_name(name: &str) -> String {
        let trimmed = name.trim().trim_end_matches(',').trim_end_matches(';');
        if let Some(angle_idx) = Self::find_top_level_char(trimmed, '<') {
            trimmed[..angle_idx].trim().to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Extract the base type from an anonymous `@typedef {type}` (one without an
    /// explicit name). In tsc, such typedefs inherit the name of the following
    /// declaration statement.
    pub(crate) fn extract_anonymous_typedef_base_type(jsdoc: &str) -> Option<String> {
        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            if let Some(rest) = Self::strip_jsdoc_tag_prefix(line, "typedef") {
                let rest = rest.trim();
                if rest.starts_with('{')
                    && let Some((expr, after)) = Self::parse_jsdoc_curly_type_expr(rest)
                {
                    let after = after.trim();
                    if after.is_empty()
                        || !after.chars().next().is_some_and(|c| {
                            c.is_alphanumeric() || c == '_' || c == '$' || c == '.'
                        })
                    {
                        let expr = expr.trim();
                        if !expr.is_empty() {
                            return Some(expr.to_string());
                        }
                    }
                }
            }
        }
        None
    }

    pub(super) fn parse_jsdoc_property_type(line: &str) -> Option<JsdocPropertyTagInfo> {
        let mut rest = line.trim();
        if let Some(after_tag) = Self::strip_jsdoc_tag_prefix(rest, "property") {
            rest = after_tag.trim();
        } else if let Some(after_tag) = Self::strip_jsdoc_tag_prefix(rest, "prop") {
            rest = after_tag.trim();
        } else {
            return None;
        }

        let (raw_name, prop_type) = if rest.starts_with('{') {
            let (expr, after_expr) = Self::parse_jsdoc_curly_type_expr(rest)?;
            let raw_name = after_expr.split_whitespace().next()?;
            (raw_name, expr.trim().to_string())
        } else {
            let raw_name = rest.split_whitespace().next()?;
            let after_name = rest[raw_name.len()..].trim();
            let prop_type = if after_name.starts_with('{') {
                let (expr, _after_expr) = Self::parse_jsdoc_curly_type_expr(after_name)?;
                expr.trim().to_string()
            } else {
                "any".to_string()
            };
            (raw_name, prop_type)
        };

        let raw_name = raw_name.trim_end_matches(',').trim();
        let optional = raw_name.starts_with('[') || prop_type.trim_end().ends_with('=');
        let name = raw_name
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split('=')
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if name.is_empty() {
            return None;
        }

        Some(JsdocPropertyTagInfo {
            name,
            type_expr: prop_type,
            optional,
        })
    }

    pub(crate) fn parse_jsdoc_curly_type_expr(line: &str) -> Option<(&str, &str)> {
        if !line.starts_with('{') {
            return None;
        }
        let mut depth = 0usize;
        let mut quote: Option<char> = None;
        let mut escaped = false;
        for (idx, ch) in line.char_indices() {
            if let Some(active_quote) = quote {
                if escaped {
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == active_quote {
                    quote = None;
                }
                continue;
            }
            match ch {
                '"' | '\'' | '`' => quote = Some(ch),
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some((&line[1..idx], &line[idx + 1..]));
                    }
                }
                _ => {}
            }
        }
        None
    }
}

#[cfg(test)]
mod jsdoc_import_as_whitespace_tests {
    use crate::state::CheckerState;

    fn parse(rest: &str) -> Vec<(String, String, String)> {
        CheckerState::parse_jsdoc_import_tag(rest)
    }

    #[test]
    fn import_named_alias_with_space() {
        let imports = parse("{ Foo as LocalFoo } from \"./dep\"");
        assert_eq!(
            imports,
            vec![(
                "LocalFoo".to_string(),
                "./dep".to_string(),
                "Foo".to_string(),
            )]
        );
    }

    #[test]
    fn import_named_alias_with_tab() {
        let imports = parse("{ Foo as\tLocalFoo } from \"./dep\"");
        assert_eq!(
            imports,
            vec![(
                "LocalFoo".to_string(),
                "./dep".to_string(),
                "Foo".to_string(),
            )]
        );
    }

    #[test]
    fn import_named_alias_with_mixed_whitespace() {
        let imports = parse("{ Foo \tas \tLocalFoo } from \"./dep\"");
        assert_eq!(
            imports,
            vec![(
                "LocalFoo".to_string(),
                "./dep".to_string(),
                "Foo".to_string(),
            )]
        );
    }

    #[test]
    fn import_namespace_alias_with_tab() {
        let imports = parse("*\tas\tNS from \"./dep\"");
        assert_eq!(
            imports,
            vec![("NS".to_string(), "./dep".to_string(), "*".to_string())]
        );
    }

    #[test]
    fn import_namespace_alias_with_space() {
        let imports = parse("* as NS from \"./dep\"");
        assert_eq!(
            imports,
            vec![("NS".to_string(), "./dep".to_string(), "*".to_string())]
        );
    }

    #[test]
    fn import_named_no_alias_unchanged() {
        let imports = parse("{ Foo, Bar } from \"./dep\"");
        assert_eq!(
            imports,
            vec![
                ("Foo".to_string(), "./dep".to_string(), "Foo".to_string(),),
                ("Bar".to_string(), "./dep".to_string(), "Bar".to_string(),),
            ]
        );
    }

    #[test]
    fn import_named_alias_does_not_match_inside_identifier() {
        // `Class` contains the substring "as" but is not a renaming.
        let imports = parse("{ Class } from \"./dep\"");
        assert_eq!(
            imports,
            vec![(
                "Class".to_string(),
                "./dep".to_string(),
                "Class".to_string(),
            )]
        );
    }

    #[test]
    fn import_named_alias_with_identifier_containing_as() {
        let imports = parse("{ Class as Klass } from \"./dep\"");
        assert_eq!(
            imports,
            vec![(
                "Klass".to_string(),
                "./dep".to_string(),
                "Class".to_string(),
            )]
        );
    }

    #[test]
    fn import_default_unchanged() {
        let imports = parse("Foo from \"./dep\"");
        assert_eq!(
            imports,
            vec![(
                "Foo".to_string(),
                "./dep".to_string(),
                "default".to_string(),
            )]
        );
    }
}

#[cfg(test)]
mod jsdoc_tag_boundary_tests {
    use crate::state::CheckerState;

    // Issue #2916: longer JSDoc tag names must not match shorter tags. The
    // tag-boundary helpers gate every JSDoc tag-detection path so identifiers
    // such as `@satisfiesx`, `@importx`, `@overridex`, `@thisx`, `@typedefx`,
    // `@callbackx`, and `@constructorx` are not silently treated as the
    // shorter real tags.

    #[test]
    fn jsdoc_contains_tag_rejects_longer_identifier() {
        for tag in [
            "satisfies",
            "import",
            "override",
            "this",
            "typedef",
            "callback",
            "constructor",
        ] {
            let mismatched = format!("/** @{tag}x foo */");
            assert!(
                !CheckerState::jsdoc_contains_tag(&mismatched, tag),
                "@{tag}x must not be treated as @{tag} (input: {mismatched:?})"
            );
            let real = format!("/** @{tag} foo */");
            assert!(
                CheckerState::jsdoc_contains_tag(&real, tag),
                "@{tag} must still be detected (input: {real:?})"
            );
        }
    }

    #[test]
    fn jsdoc_contains_tag_treats_underscore_suffix_as_distinct_tag() {
        assert!(!CheckerState::jsdoc_contains_tag(
            "/** @typedef_internal {{ a }} */",
            "typedef"
        ));
        assert!(CheckerState::jsdoc_contains_tag(
            "/** @typedef\n@template T */",
            "typedef"
        ));
    }

    #[test]
    fn jsdoc_tag_offset_skips_longer_match_and_finds_real_tag() {
        let jsdoc = "/** @satisfiesx not a tag\n * @satisfies {Foo} */";
        let pos = CheckerState::jsdoc_tag_offset(jsdoc, "satisfies")
            .expect("real @satisfies tag must be located");
        assert_eq!(&jsdoc[pos..pos + "@satisfies".len()], "@satisfies");
        let after = &jsdoc[pos + "@satisfies".len()..];
        assert!(
            after.starts_with(' '),
            "boundary must be reached, got rest = {after:?}"
        );
    }

    #[test]
    fn jsdoc_tag_offsets_only_returns_real_tag_positions() {
        let jsdoc = "@satisfiesx skip me\n@satisfies a\n@satisfies b\n@satisfiesy nope\n";
        let offsets = CheckerState::jsdoc_tag_offsets(jsdoc, "satisfies");
        assert_eq!(offsets.len(), 2);
        for off in &offsets {
            let after = &jsdoc[off + "@satisfies".len()..];
            let next = after.chars().next().unwrap_or(' ');
            assert!(
                !next.is_ascii_alphanumeric() && next != '_',
                "expected boundary at offset {off}, found {after:?}"
            );
        }
    }

    #[test]
    fn strip_jsdoc_tag_prefix_rejects_longer_identifiers() {
        for tag in ["import", "typedef", "template", "param"] {
            let mismatched = format!("@{tag}x foo");
            assert!(
                CheckerState::strip_jsdoc_tag_prefix(&mismatched, tag).is_none(),
                "@{tag}x must not strip as @{tag} (input: {mismatched:?})"
            );
            let real = format!("@{tag} foo");
            assert_eq!(
                CheckerState::strip_jsdoc_tag_prefix(&real, tag),
                Some(" foo"),
                "@{tag} must strip with the trailing rest preserved"
            );
            // Bare tag with no trailing characters (end of input is a boundary).
            let bare = format!("@{tag}");
            assert_eq!(CheckerState::strip_jsdoc_tag_prefix(&bare, tag), Some(""));
        }
    }

    #[test]
    fn jsdoc_line_starts_with_tag_handles_boundaries() {
        assert!(CheckerState::jsdoc_line_starts_with_tag(
            "@typedef {{a: number}} Foo",
            "typedef"
        ));
        assert!(!CheckerState::jsdoc_line_starts_with_tag(
            "@typedefx {{a: number}} Foo",
            "typedef"
        ));
        assert!(!CheckerState::jsdoc_line_starts_with_tag(
            "@typedef_inner",
            "typedef"
        ));
        assert!(CheckerState::jsdoc_line_starts_with_tag(
            "@typedef\trest",
            "typedef"
        ));
    }

    #[test]
    fn extract_jsdoc_satisfies_expression_ignores_longer_prefix() {
        // `@satisfiesx {Foo}` must not be parsed as `@satisfies {Foo}`.
        let bogus = "/** @satisfiesx {Foo} */";
        assert!(CheckerState::extract_jsdoc_satisfies_expression(bogus).is_none());
        let real = "/** @satisfies {Foo} */";
        assert_eq!(
            CheckerState::extract_jsdoc_satisfies_expression(real),
            Some("Foo")
        );
    }

    #[test]
    fn parse_jsdoc_typedefs_ignores_typedefx_and_importx() {
        // `@typedefx` must not register a typedef under the name following the
        // bogus tag.
        let typedefs = CheckerState::parse_jsdoc_typedefs("@typedefx {{ n: number }} Foo\n");
        assert_eq!(
            typedefs.len(),
            0,
            "expected no typedefs from @typedefx, got {} entries",
            typedefs.len()
        );

        // `@importx { Foo } from "./types"` must not create an import alias.
        let imports = CheckerState::parse_jsdoc_typedefs("@importx { Foo } from \"./types\"\n");
        assert_eq!(
            imports.len(),
            0,
            "expected no imports from @importx, got {} entries",
            imports.len()
        );

        // The real `@typedef` and `@import` must still be handled.
        let typedefs = CheckerState::parse_jsdoc_typedefs("@typedef {{ n: number }} Foo\n");
        assert_eq!(typedefs.len(), 1);
        assert_eq!(typedefs[0].0, "Foo");

        let imports = CheckerState::parse_jsdoc_typedefs("@import { Foo } from \"./types\"\n");
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].0, "Foo");
    }

    #[test]
    fn parse_jsdoc_callback_preserves_nested_object_return_type() {
        let typedefs = CheckerState::parse_jsdoc_typedefs(
            "\
@callback MakeBox
@returns {{ value: string }}
",
        );

        assert_eq!(typedefs.len(), 1);
        assert_eq!(typedefs[0].0, "MakeBox");
        let callback = typedefs[0].1.callback.as_ref().expect("callback parsed");
        assert_eq!(callback.return_type.as_deref(), Some("{ value: string }"));
    }
}

#[cfg(test)]
mod parse_jsdoc_import_tag_alias_tests {
    use crate::state::CheckerState;

    fn parse(rest: &str) -> Vec<(String, String, String)> {
        CheckerState::parse_jsdoc_import_tag(rest)
    }

    #[test]
    fn named_alias_with_space() {
        let got = parse(r#" { Foo as LocalFoo } from "./dep""#);
        assert_eq!(
            got,
            vec![(
                "LocalFoo".to_string(),
                "./dep".to_string(),
                "Foo".to_string()
            )]
        );
    }

    #[test]
    fn named_alias_with_tab_after_as() {
        let got = parse("\t{ Foo as\tLocalFoo } from \"./dep\"");
        assert_eq!(
            got,
            vec![(
                "LocalFoo".to_string(),
                "./dep".to_string(),
                "Foo".to_string()
            )]
        );
    }

    #[test]
    fn named_alias_with_tab_before_and_after_as() {
        let got = parse("{ Foo\tas\tLocalFoo } from \"./dep\"");
        assert_eq!(
            got,
            vec![(
                "LocalFoo".to_string(),
                "./dep".to_string(),
                "Foo".to_string()
            )]
        );
    }

    #[test]
    fn named_alias_with_multiple_spaces() {
        let got = parse(r#"{ Foo  as  LocalFoo } from "./dep""#);
        assert_eq!(
            got,
            vec![(
                "LocalFoo".to_string(),
                "./dep".to_string(),
                "Foo".to_string()
            )]
        );
    }

    #[test]
    fn named_without_alias() {
        let got = parse(r#"{ Foo } from "./dep""#);
        assert_eq!(
            got,
            vec![("Foo".to_string(), "./dep".to_string(), "Foo".to_string())]
        );
    }

    #[test]
    fn named_alias_string_literal_export_names() {
        let got =
            parse(r#"{ "a,b" as CommaName, "as" as AsName, "from" as FromName } from "./dep""#);
        assert_eq!(
            got,
            vec![
                (
                    "CommaName".to_string(),
                    "./dep".to_string(),
                    "a,b".to_string()
                ),
                ("AsName".to_string(), "./dep".to_string(), "as".to_string()),
                (
                    "FromName".to_string(),
                    "./dep".to_string(),
                    "from".to_string()
                )
            ]
        );
    }

    #[test]
    fn import_type_parses_quoted_member_name() {
        assert_eq!(
            CheckerState::parse_jsdoc_import_type(r#"import("./dep")."a,b""#),
            Some(("./dep".to_string(), Some("a,b".to_string())))
        );
    }

    #[test]
    fn namespace_alias_with_space() {
        let got = parse(r#"* as ns from "./dep""#);
        assert_eq!(
            got,
            vec![("ns".to_string(), "./dep".to_string(), "*".to_string())]
        );
    }

    #[test]
    fn namespace_alias_with_tab_around_as() {
        let got = parse("*\tas\tns from \"./dep\"");
        assert_eq!(
            got,
            vec![("ns".to_string(), "./dep".to_string(), "*".to_string())]
        );
    }

    #[test]
    fn does_not_match_as_inside_identifier() {
        let got = parse(r#"{ Class } from "./dep""#);
        assert_eq!(
            got,
            vec![(
                "Class".to_string(),
                "./dep".to_string(),
                "Class".to_string()
            )]
        );
    }

    #[test]
    fn alias_keyword_named_as() {
        let got = parse(r#"{ as as Foo } from "./dep""#);
        assert_eq!(
            got,
            vec![("Foo".to_string(), "./dep".to_string(), "as".to_string())]
        );
    }

    #[test]
    fn default_alias() {
        let got = parse(r#"{ default as Foo } from "./dep""#);
        assert_eq!(
            got,
            vec![(
                "Foo".to_string(),
                "./dep".to_string(),
                "default".to_string()
            )]
        );
    }
}
