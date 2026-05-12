//! JSDoc function type signature parsing helpers.

pub(in crate::declaration_emitter) type JsdocFunctionTypeParam = String;
pub(in crate::declaration_emitter) type JsdocFunctionParam = (String, String);
pub(in crate::declaration_emitter) type JsdocFunctionTypeSignature =
    (Vec<JsdocFunctionTypeParam>, Vec<JsdocFunctionParam>, String);

pub(in crate::declaration_emitter) fn parse_jsdoc_function_type_signature(
    type_text: &str,
) -> Option<JsdocFunctionTypeSignature> {
    let mut rest = type_text.trim();
    let mut type_params = Vec::new();
    if let Some(after_open) = rest.strip_prefix('<') {
        let close = find_matching_delimiter(after_open, '<', '>')?;
        type_params = split_top_level(after_open[..close].trim(), ',')
            .into_iter()
            .map(str::trim)
            .filter(|param| !param.is_empty())
            .map(str::to_string)
            .collect();
        rest = after_open[close + 1..].trim_start();
    }

    let after_params = rest.strip_prefix('(')?;
    let close = find_matching_delimiter(after_params, '(', ')')?;
    let params_text = &after_params[..close];
    let after_close = after_params[close + 1..].trim_start();
    let return_type = after_close.strip_prefix("=>")?.trim();
    if return_type.is_empty() {
        return None;
    }

    let mut params = Vec::new();
    for raw_param in split_top_level(params_text, ',') {
        let raw_param = raw_param.trim();
        if raw_param.is_empty() {
            continue;
        }
        let colon = find_top_level_char(raw_param, ':')?;
        let name = raw_param[..colon].trim();
        let type_text = raw_param[colon + 1..].trim();
        if name.is_empty() || type_text.is_empty() {
            return None;
        }
        params.push((name.to_string(), type_text.to_string()));
    }

    Some((type_params, params, return_type.to_string()))
}

fn split_top_level(text: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut angle_depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for (idx, ch) in text.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == q {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '<' => angle_depth += 1,
            '>' => angle_depth = angle_depth.saturating_sub(1),
            _ if ch == sep
                && paren_depth == 0
                && bracket_depth == 0
                && brace_depth == 0
                && angle_depth == 0 =>
            {
                parts.push(&text[start..idx]);
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    parts.push(&text[start..]);
    parts
}

fn find_top_level_char(text: &str, target: char) -> Option<usize> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut angle_depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for (idx, ch) in text.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == q {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '<' => angle_depth += 1,
            '>' => angle_depth = angle_depth.saturating_sub(1),
            _ if ch == target
                && paren_depth == 0
                && bracket_depth == 0
                && brace_depth == 0
                && angle_depth == 0 =>
            {
                return Some(idx);
            }
            _ => {}
        }
    }

    None
}

fn find_matching_delimiter(text: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 1usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for (idx, ch) in text.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == q {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            _ if ch == open => depth += 1,
            _ if ch == close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }

    None
}
