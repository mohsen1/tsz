//! Pure string-level parsing helpers for JSDoc type annotations.
//!
//! These are associated functions on `CheckerState` (no `&self`/`&mut self`)
//! that perform nesting-aware string splitting, tag extraction, and typedef
//! definition parsing. No type resolution or checker state access.

use super::types::{
    JsdocCallbackInfo, JsdocPropertyTagInfo, JsdocTemplateParamInfo, JsdocTypedefInfo,
};
use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    // -----------------------------------------------------------------
    // Low-level nesting-aware string splitting
    // -----------------------------------------------------------------

    /// Find the first occurrence of a character at the top level.
    pub(super) fn find_top_level_char(s: &str, target: char) -> Option<usize> {
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
    pub(super) fn split_top_level_binary(s: &str, op: char) -> Option<Vec<&str>> {
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
    pub(super) fn jsdoc_contains_tag(jsdoc: &str, tag_name: &str) -> bool {
        let needle = format!("@{tag_name}");
        for pos_match in jsdoc.match_indices(&needle) {
            let after = pos_match.0 + needle.len();
            if after >= jsdoc.len() {
                return true;
            }
            let next_ch = jsdoc[after..]
                .chars()
                .next()
                .expect("after < jsdoc.len() checked above");
            if !next_ch.is_ascii_alphanumeric() {
                return true;
            }
        }
        false
    }

    pub(super) fn extract_jsdoc_type_expression(jsdoc: &str) -> Option<&str> {
        let typedef_pos = jsdoc.find("@typedef");
        let mut tag_pos = jsdoc.find("@type");
        while let Some(pos) = tag_pos {
            let next_char = jsdoc[pos + "@type".len()..].chars().next();
            if !next_char.is_some_and(|c| c.is_alphabetic()) {
                if let Some(td_pos) = typedef_pos
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
                break;
            }
            tag_pos = jsdoc[pos + 1..].find("@type").map(|p| p + pos + 1);
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

    pub(super) fn extract_jsdoc_satisfies_expression(jsdoc: &str) -> Option<&str> {
        let tag_pos = jsdoc.find("@satisfies")?;
        let rest = &jsdoc[tag_pos + "@satisfies".len()..];
        let open = rest.find('{')?;
        let after_open = &rest[open + 1..];
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
        if quote != '"' && quote != '\'' && quote != '`' {
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

    pub(super) fn jsdoc_template_constraints(jsdoc: &str) -> Vec<(String, Option<String>)> {
        let mut out = Vec::new();
        for raw_line in jsdoc.lines() {
            let trimmed = raw_line.trim().trim_start_matches('*').trim();
            let Some(rest) = trimmed.strip_prefix("@template") else {
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
        let template_params: Vec<JsdocTemplateParamInfo> = Self::jsdoc_template_constraints(jsdoc)
            .into_iter()
            .map(|(name, constraint)| JsdocTemplateParamInfo { name, constraint })
            .collect();
        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            if let Some(body_lines) = wrapped_typedef_body.as_mut() {
                if line.is_empty() {
                    continue;
                }
                if let Some(rest) = line.strip_prefix("}}") {
                    let name = rest.trim();
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
                        current_name = Some(name.to_string());
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
            if let Some(rest) = line.strip_prefix("@import") {
                for (local_name, specifier, import_name) in Self::parse_jsdoc_import_tag(rest) {
                    let import_type = if import_name == "*" || import_name == "default" {
                        format!("import(\"{specifier}\")")
                    } else {
                        format!("import(\"{specifier}\").{import_name}")
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
            if let Some(rest) = line.strip_prefix("@typedef") {
                let rest = rest.trim();
                if rest.starts_with("{{") && !rest.contains("}}") {
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
            if let Some(rest) = line.strip_prefix("@callback") {
                let name = rest.trim().to_string();
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
                if let Some(rest) = line.strip_prefix("@param") {
                    if let Some(param_info) = Self::parse_jsdoc_param_tag(rest)
                        && let Some(ref mut cb) = current_info.callback
                    {
                        cb.params.push(param_info);
                    }
                    continue;
                }
                if let Some(rest) = line
                    .strip_prefix("@returns")
                    .or_else(|| line.strip_prefix("@return"))
                {
                    let rest = rest.trim();
                    if rest.starts_with('{')
                        && let Some(end) = rest[1..].find('}')
                    {
                        let type_expr = rest[1..1 + end].trim();
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
    pub(super) fn jsdoc_returns_type_predicate_from_type_expr(
        type_expr: &str,
    ) -> Option<(bool, String, Option<String>)> {
        let (is_asserts, remainder) = if let Some(after) = type_expr.strip_prefix("asserts ") {
            (true, after.trim())
        } else {
            (false, type_expr)
        };
        if let Some(is_pos) = remainder.find(" is ") {
            let param_name = remainder[..is_pos].trim();
            let type_str = remainder[is_pos + 4..].trim();
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

    pub(super) fn parse_jsdoc_import_tag(rest: &str) -> Vec<(String, String, String)> {
        let rest = rest.trim();
        let mut results = Vec::new();
        if let Some(from_idx) = rest.rfind("from") {
            let before_from = rest[..from_idx].trim();
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
                    for part in inner.split(',') {
                        let part = part.trim();
                        if part.is_empty() {
                            continue;
                        }
                        let parts: Vec<&str> = part.split(" as ").collect();
                        if parts.len() == 2 {
                            results.push((
                                parts[1].trim().to_string(),
                                specifier.clone(),
                                parts[0].trim().to_string(),
                            ));
                        } else {
                            results.push((part.to_string(), specifier.clone(), part.to_string()));
                        }
                    }
                } else if let Some(ns_name) = before_from.strip_prefix("* as ") {
                    let ns_name = ns_name.trim().to_string();
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
        Some((name.to_string(), base_type))
    }

    pub(super) fn parse_jsdoc_property_type(line: &str) -> Option<JsdocPropertyTagInfo> {
        let mut rest = line.trim();
        if let Some(after_tag) = rest.strip_prefix("@property") {
            rest = after_tag.trim();
        } else if let Some(after_tag) = rest.strip_prefix("@prop") {
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
        for (idx, ch) in line.char_indices() {
            match ch {
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
