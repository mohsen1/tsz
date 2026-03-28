//! String formatting utilities for hover display.
//!
//! Pure text transformation functions used by the hover provider to format
//! type signatures, object literals, and union/array type strings.

/// Convert colon notation `(params): ret` to arrow notation `(params) => ret`.
pub(crate) fn colon_to_arrow_signature(signature: &str) -> String {
    let trimmed = signature.trim();
    if !trimmed.starts_with('(') {
        return trimmed.to_string();
    }
    let bytes = trimmed.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    let after = trimmed[i + 1..].trim_start();
                    if let Some(rest) = after.strip_prefix(':') {
                        return format!("{} => {}", &trimmed[..=i], rest.trim_start());
                    }
                    break;
                }
            }
            _ => {}
        }
    }
    trimmed.to_string()
}

/// Convert arrow notation `(params) => ret` to colon notation `(params): ret`.
/// Used when displaying named functions/methods where TypeScript uses `:` for
/// the return type, not `=>`.
pub(crate) fn arrow_to_colon(type_string: &str) -> String {
    // Find the last `) => ` at paren depth 0 and replace with `): `
    let bytes = type_string.as_bytes();
    let mut depth = 0i32;
    let mut last_close = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    last_close = Some(i);
                }
            }
            _ => {}
        }
    }
    if let Some(close_idx) = last_close {
        let after = &type_string[close_idx + 1..];
        if let Some(arrow_pos) = after.find(" => ") {
            let before = &type_string[..close_idx + 1];
            let ret = &after[arrow_pos + 4..];
            return format!("{before}: {ret}");
        }
    }
    type_string.to_string()
}

pub(crate) fn format_hover_variable_type(type_string: &str) -> String {
    let stripped = strip_optional_undefined_props(type_string);
    let expanded = expand_inline_object_literals(&stripped);
    normalize_union_array_precedence(&expanded)
}

/// Strip `name?: undefined;` properties from inline object literals.
///
/// The solver's BCT normalization adds optional-undefined properties for each
/// property that exists in sibling union members but not in the current member.
/// TypeScript does not display these synthetic properties, so we remove them
/// before formatting.
fn strip_optional_undefined_props(type_string: &str) -> String {
    if !type_string.contains("?: undefined") {
        return type_string.to_string();
    }

    let mut out = String::with_capacity(type_string.len());
    let mut cursor = 0usize;

    while let Some(rel_open) = type_string[cursor..].find('{') {
        let open = cursor + rel_open;
        out.push_str(&type_string[cursor..open]);
        let Some(close) = find_matching_brace(type_string, open) else {
            out.push_str(&type_string[open..]);
            return out;
        };
        let inner = &type_string[open + 1..close];
        // Only process flat object literals (no nested braces)
        if !inner.contains('{') && inner.contains("?: undefined") {
            out.push_str("{ ");
            let props: Vec<&str> = inner
                .split(';')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .collect();
            let filtered: Vec<&str> = props
                .into_iter()
                .filter(|p| {
                    // Remove properties like "name?: undefined"
                    if let Some(colon_pos) = p.find(':') {
                        let before_colon = p[..colon_pos].trim();
                        let after_colon = p[colon_pos + 1..].trim();
                        !(before_colon.ends_with('?') && after_colon == "undefined")
                    } else {
                        true
                    }
                })
                .collect();
            out.push_str(&filtered.join("; "));
            if !filtered.is_empty() {
                out.push_str("; ");
            }
            out.push('}');
        } else {
            out.push_str(&type_string[open..=close]);
        }
        cursor = close + 1;
    }

    out.push_str(&type_string[cursor..]);
    out
}

fn expand_inline_object_literals(type_string: &str) -> String {
    let mut out = String::with_capacity(type_string.len() + 16);
    let mut cursor = 0usize;

    while let Some(rel_open) = type_string[cursor..].find('{') {
        let open = cursor + rel_open;
        out.push_str(&type_string[cursor..open]);
        let Some(close) = find_matching_brace(type_string, open) else {
            out.push_str(&type_string[open..]);
            return out;
        };
        let inner = &type_string[open + 1..close];
        if let Some(multiline) = format_object_inner_multiline(inner) {
            out.push_str(&multiline);
        } else {
            out.push_str(&type_string[open..=close]);
        }
        cursor = close + 1;
    }

    out.push_str(&type_string[cursor..]);
    out
}

fn find_matching_brace(text: &str, open_brace: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (idx, ch) in text[open_brace..].char_indices() {
        let absolute = open_brace + idx;
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(absolute);
                }
            }
            _ => {}
        }
    }
    None
}

fn format_object_inner_multiline(inner: &str) -> Option<String> {
    if inner.contains('\n') {
        return None;
    }
    let props: Vec<&str> = inner
        .split(';')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if props.len() < 2 {
        return None;
    }
    if props.iter().any(|p| p.contains('{') || p.contains('}')) {
        return None;
    }
    if !props.iter().all(|p| p.contains(':')) {
        return None;
    }
    Some(format!("{{\n    {};\n}}", props.join(";\n    ")))
}

fn normalize_union_array_precedence(type_string: &str) -> String {
    let trimmed = type_string.trim();
    if !trimmed.ends_with("[]") || trimmed.ends_with(")[]") {
        return type_string.to_string();
    }

    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth_paren = 0i32;
    let mut depth_brace = 0i32;
    let mut depth_bracket = 0i32;

    for (idx, ch) in trimmed.char_indices() {
        match ch {
            '(' => depth_paren += 1,
            ')' => depth_paren -= 1,
            '{' => depth_brace += 1,
            '}' => depth_brace -= 1,
            '[' => depth_bracket += 1,
            ']' => depth_bracket -= 1,
            '|' if depth_paren == 0 && depth_brace == 0 && depth_bracket == 0 => {
                parts.push(trimmed[start..idx].trim().to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }

    if parts.is_empty() {
        return type_string.to_string();
    }
    parts.push(trimmed[start..].trim().to_string());

    let Some(last) = parts.last() else {
        return type_string.to_string();
    };
    if !last.ends_with("[]") {
        return type_string.to_string();
    }
    if parts[..parts.len().saturating_sub(1)]
        .iter()
        .any(|part| part.ends_with("[]"))
    {
        return type_string.to_string();
    }

    let mut normalized = parts;
    if let Some(last_part) = normalized.last_mut() {
        *last_part = last_part
            .strip_suffix("[]")
            .unwrap_or(last_part.as_str())
            .trim()
            .to_string();
    }
    format!("({})[]", normalized.join(" | "))
}
