//! Pure text processing helpers for quickinfo display string normalization,
//! parameter type extraction, and signature parsing.
//!
//! These are free functions (no Server/AST dependency) extracted from
//! `handlers_quickinfo.rs` to keep that file under the 2000-LOC threshold.

/// Check if a byte is a valid JS identifier character (ASCII alphanumeric, `_`, or `$`).
pub(super) const fn is_js_identifier_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

/// Extract a trailing type name from a display string like `"(var) x: Foo"` → `"Foo"`.
pub(super) fn extract_trailing_type_name(display: &str) -> Option<String> {
    let (_, ty) = display.rsplit_once(": ")?;
    let ty = ty.trim();
    if ty.is_empty() {
        return None;
    }
    if ty.bytes().all(|b| is_js_identifier_char(b) || b == b'.') {
        Some(ty.to_string())
    } else {
        None
    }
}

/// Extract the identifier at a given byte offset in source text.
pub(super) fn identifier_at(source_text: &str, offset: u32) -> Option<String> {
    let bytes = source_text.as_bytes();
    let len = bytes.len() as u32;
    if offset >= len {
        return None;
    }
    let mut start = offset;
    while start > 0 && is_js_identifier_char(bytes[(start - 1) as usize]) {
        start -= 1;
    }
    let mut end = start;
    while end < len && is_js_identifier_char(bytes[end as usize]) {
        end += 1;
    }
    (end > start).then(|| source_text[start as usize..end as usize].to_string())
}

/// Heuristic: detect whether `offset` is inside a type annotation context
/// (`foo: Type`) by scanning backward to the nearest top-level `:` delimiter.
pub(super) fn is_type_annotation_context(source_text: &str, offset: u32) -> bool {
    let bytes = source_text.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let len = bytes.len() as u32;
    let probe = nearest_identifier_offset(source_text, offset).unwrap_or(offset);
    let mut cursor = probe.min(len.saturating_sub(1)) as i32;

    while cursor >= 0 {
        let byte = bytes[cursor as usize];
        if byte.is_ascii_whitespace() {
            cursor -= 1;
            continue;
        }

        if byte == b'/' && cursor > 0 && bytes[(cursor - 1) as usize] == b'*' {
            cursor -= 2;
            while cursor >= 1 {
                if bytes[(cursor - 1) as usize] == b'/' && bytes[cursor as usize] == b'*' {
                    cursor -= 2;
                    break;
                }
                cursor -= 1;
            }
            continue;
        }

        match byte {
            b':' => return true,
            b'=' | b';' | b'\n' | b'\r' | b'{' | b'(' | b',' => return false,
            _ => {}
        }

        cursor -= 1;
    }

    false
}

/// Clean a raw JSDoc comment (`/** ... */`) into plain text.
pub(super) fn clean_jsdoc_comment(raw: &str) -> String {
    let inner = raw
        .trim()
        .trim_start_matches("/**")
        .trim_end_matches("*/")
        .trim();
    inner
        .lines()
        .map(|line| line.trim().trim_start_matches('*').trim())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Split an arrow function signature at the top-level `=>`.
pub(super) fn split_top_level_arrow_signature(sig: &str) -> Option<(&str, &str)> {
    let bytes = sig.as_bytes();
    let mut depth = 0i32;
    let mut i = 0usize;
    let mut arrow_pos = None;
    while i + 1 < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b'=' if bytes[i + 1] == b'>' && depth == 0 => {
                arrow_pos = Some(i);
            }
            _ => {}
        }
        i += 1;
    }
    let i = arrow_pos?;
    let params = sig[..i].trim();
    let ret = sig[i + 2..].trim();
    Some((params, ret))
}

/// Convert an arrow type text like `(x: number) => string` to `function(x: number): string`.
pub(super) fn arrow_function_display_string(type_text: &str) -> Option<String> {
    let trimmed = type_text.trim();
    let (params, ret) = split_top_level_arrow_signature(trimmed)?;
    if !(params.starts_with('(') && params.ends_with(')')) || ret.is_empty() {
        return None;
    }
    Some(format!("function{params}: {ret}"))
}

/// Parse a parameter hover display like `(parameter) x: number` → `"number"`.
pub(super) fn parse_hover_parameter_type(display: &str, param_name: &str) -> Option<String> {
    let prefix = format!("(parameter) {param_name}: ");
    display
        .strip_prefix(&prefix)
        .map(str::trim)
        .filter(|ty| !ty.is_empty())
        .map(str::to_string)
}

/// Normalize a union type by sorting primitives and deduplicating.
pub(super) fn normalize_union_type_text(ty: &str) -> String {
    let mut parts: Vec<String> = ty
        .split('|')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    if parts.len() <= 1 {
        return ty.trim().to_string();
    }
    parts.sort_by_key(|part| match part.as_str() {
        "string" => 0u8,
        "number" => 1u8,
        "boolean" => 2u8,
        "bigint" => 3u8,
        "symbol" => 4u8,
        "undefined" => 5u8,
        "null" => 6u8,
        _ => 7u8,
    });
    parts.dedup();
    parts.join(" | ")
}

/// Normalize a parameter type text by trimming the trailing `) =>` suffix.
pub(super) fn normalize_parameter_type_text(ty: &str) -> String {
    let head = ty.split(") =>").next().unwrap_or(ty).trim();
    normalize_union_type_text(head)
}

/// Normalize a quickinfo display string (function signature formatting).
pub(super) fn normalize_quickinfo_display_string(display: &str) -> String {
    let trimmed = display.trim();
    let normalized = if trimmed.starts_with("function(") {
        let Some(ret_sep) = trimmed.rfind("): ") else {
            return normalize_call_signature_colon_spacing(trimmed);
        };
        let ret = trimmed[ret_sep + 3..].trim();
        let params_with_name = &trimmed["function(".len()..ret_sep];
        let params_clean = params_with_name
            .split(") =>")
            .next()
            .unwrap_or(params_with_name)
            .trim();
        let Some((name, ty)) = params_clean.split_once(':') else {
            return normalize_call_signature_colon_spacing(trimmed);
        };
        let name = name.trim();
        if name.is_empty() {
            return normalize_call_signature_colon_spacing(trimmed);
        }
        let ty = normalize_parameter_type_text(ty);
        format!("function({name}: {ty}): {ret}")
    } else {
        trimmed.to_string()
    };
    let normalized = normalize_call_signature_colon_spacing(&normalized);
    let normalized = normalize_single_rest_tuple_function_params(&normalized);
    let normalized = normalize_single_any_rest_param_function_params(&normalized);
    let normalized = normalize_single_call_signature_object_types(&normalized);
    normalize_single_index_signature_objects(&normalized)
}

/// Normalize `) :` to `):` in call signatures.
pub(super) fn normalize_call_signature_colon_spacing(display: &str) -> String {
    display.replace(") :", "):")
}

fn normalize_single_call_signature_object_types(display: &str) -> String {
    let mut out = String::with_capacity(display.len());
    let bytes = display.as_bytes();
    let mut cursor = 0usize;

    while let Some(rel_open) = display[cursor..].find('{') {
        let open = cursor + rel_open;
        out.push_str(&display[cursor..open]);

        let Some(close) = find_matching_brace(display, open) else {
            out.push_str(&display[open..]);
            return out;
        };
        let inner = display[open + 1..close].trim();
        if let Some(arrow_sig) = single_call_signature_object_to_arrow(inner) {
            let mut probe = close + 1;
            while probe < bytes.len() && bytes[probe].is_ascii_whitespace() {
                probe += 1;
            }
            if probe < bytes.len() && bytes[probe] == b'[' {
                out.push('(');
                out.push_str(&arrow_sig);
                out.push(')');
            } else {
                out.push_str(&arrow_sig);
            }
        } else {
            out.push_str(&display[open..=close]);
        }
        cursor = close + 1;
    }

    out.push_str(&display[cursor..]);
    out
}

fn normalize_single_index_signature_objects(display: &str) -> String {
    let mut out = String::with_capacity(display.len());
    let mut cursor = 0usize;

    while let Some(rel_open) = display[cursor..].find('{') {
        let open = cursor + rel_open;
        out.push_str(&display[cursor..open]);
        let Some(close) = find_matching_brace(display, open) else {
            out.push_str(&display[open..]);
            return out;
        };
        let inner = display[open + 1..close].trim();
        if inner.contains('\n') || inner.contains('{') || inner.contains('}') {
            out.push_str(&display[open..=close]);
            cursor = close + 1;
            continue;
        }
        let cleaned = inner.trim_end_matches(';').trim();
        if cleaned.starts_with('[') && cleaned.contains(':') {
            out.push_str("{\n    ");
            out.push_str(cleaned);
            out.push_str(";\n}");
        } else {
            out.push_str(&display[open..=close]);
        }
        cursor = close + 1;
    }

    out.push_str(&display[cursor..]);
    out
}

fn normalize_single_rest_tuple_function_params(display: &str) -> String {
    let mut out = String::with_capacity(display.len());
    let bytes = display.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        // Normalize a single-parameter rest tuple function type:
        // `(...a: [x: X, y: Y]) => R` -> `(x: X, y: Y) => R`
        if bytes[i] == b'(' && i + 4 < bytes.len() && &bytes[i + 1..i + 4] == b"..." {
            let mut cursor = i + 4;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            let name_start = cursor;
            while cursor < bytes.len() && is_js_identifier_char(bytes[cursor]) {
                cursor += 1;
            }
            if cursor == name_start {
                out.push(bytes[i] as char);
                i += 1;
                continue;
            }
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor >= bytes.len() || bytes[cursor] != b':' {
                out.push(bytes[i] as char);
                i += 1;
                continue;
            }
            cursor += 1;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor >= bytes.len() || bytes[cursor] != b'[' {
                out.push(bytes[i] as char);
                i += 1;
                continue;
            }

            let tuple_open = cursor;
            let mut depth = 0i32;
            let mut tuple_close = None;
            while cursor < bytes.len() {
                match bytes[cursor] {
                    b'[' => depth += 1,
                    b']' => {
                        depth -= 1;
                        if depth == 0 {
                            tuple_close = Some(cursor);
                            break;
                        }
                    }
                    _ => {}
                }
                cursor += 1;
            }
            let Some(tuple_close) = tuple_close else {
                out.push(bytes[i] as char);
                i += 1;
                continue;
            };
            let tuple_inner = display[tuple_open + 1..tuple_close].trim();
            if tuple_inner.is_empty() {
                out.push(bytes[i] as char);
                i += 1;
                continue;
            }

            cursor = tuple_close + 1;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor >= bytes.len() || bytes[cursor] != b')' {
                out.push(bytes[i] as char);
                i += 1;
                continue;
            }
            let close_paren = cursor;
            cursor += 1;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor + 1 >= bytes.len() || bytes[cursor] != b'=' || bytes[cursor + 1] != b'>' {
                out.push(bytes[i] as char);
                i += 1;
                continue;
            }

            out.push('(');
            out.push_str(tuple_inner);
            out.push(')');
            i = close_paren + 1;
            continue;
        }

        out.push(bytes[i] as char);
        i += 1;
    }

    out
}

fn normalize_single_any_rest_param_function_params(display: &str) -> String {
    display.replace("(...a: any[])", "()")
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

fn single_call_signature_object_to_arrow(inner: &str) -> Option<String> {
    let mut body = inner.trim().trim_end_matches(';').trim();
    if body.is_empty() {
        return None;
    }
    if body.contains('{') || body.contains('}') {
        return None;
    }
    if body.matches(';').count() > 0 {
        // Multiple members/signatures should remain in object-literal form.
        return None;
    }
    if !body.starts_with('(') {
        return None;
    }

    let bytes = body.as_bytes();
    let mut depth = 0i32;
    let mut close = None;
    for (idx, b) in bytes.iter().enumerate() {
        match *b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(idx);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let params = &body[..=close];
    body = body[close + 1..].trim_start();
    let ret = body
        .strip_prefix(':')?
        .trim_start()
        .trim_end_matches(';')
        .trim();
    if ret.is_empty() {
        return None;
    }
    Some(format!("{params} => {ret}"))
}

/// Strip balanced outer parentheses from text.
pub(super) fn strip_outer_parens(mut text: &str) -> &str {
    loop {
        let trimmed = text.trim();
        if !(trimmed.starts_with('(') && trimmed.ends_with(')')) {
            return trimmed;
        }
        let bytes = trimmed.as_bytes();
        let mut depth = 0i32;
        let mut balanced = true;
        for (i, b) in bytes.iter().enumerate() {
            match *b {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 && i + 1 < bytes.len() {
                        balanced = false;
                        break;
                    }
                }
                _ => {}
            }
        }
        if !balanced || depth != 0 {
            return trimmed;
        }
        text = &trimmed[1..trimmed.len() - 1];
    }
}

/// Split text at top-level separator bytes (respects parenthesis depth).
pub(super) fn split_top_level_bytes(text: &str, sep: u8) -> Vec<String> {
    let mut parts = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (i, b) in bytes.iter().enumerate() {
        match *b {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            c if c == sep && depth == 0 => {
                parts.push(text[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim().to_string());
    parts
}

/// Extract the first parameter type from a function type string.
pub(super) fn extract_first_param_type_from_fn_type(type_text: &str) -> Option<(String, bool)> {
    let mut candidate = type_text.trim();
    while let Some(stripped) = candidate.strip_suffix("[]") {
        candidate = stripped.trim_end();
    }
    let trimmed = strip_outer_parens(candidate);
    let open = trimmed.find('(')?;
    let mut depth = 0i32;
    let mut close = None;
    for (i, ch) in trimmed[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let params = trimmed[open + 1..close].trim();
    if params.is_empty() {
        return None;
    }
    let first_param = split_top_level_bytes(params, b',').into_iter().next()?;
    let (name_part, type_part) = first_param.split_once(':')?;
    let is_optional = name_part.trim().ends_with('?');
    let ty = type_part.trim();
    (!ty.is_empty()).then(|| (ty.to_string(), is_optional))
}

/// Extract a parameter type at a given index from a function type string.
pub(super) fn extract_param_type_from_fn_type(
    type_text: &str,
    param_index: usize,
) -> Option<(String, bool)> {
    let mut candidate = type_text.trim();
    while let Some(stripped) = candidate.strip_suffix("[]") {
        candidate = stripped.trim_end();
    }
    let trimmed = strip_outer_parens(candidate);
    let open = trimmed.find('(')?;
    let mut depth = 0i32;
    let mut close = None;
    for (i, ch) in trimmed[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let params = trimmed[open + 1..close].trim();
    if params.is_empty() {
        return None;
    }
    let param = split_top_level_bytes(params, b',')
        .into_iter()
        .nth(param_index)?;
    let (name_part, type_part) = param.split_once(':')?;
    let name_part = name_part.trim().trim_start_matches("...");
    let is_optional = name_part.ends_with('?');
    let ty = type_part.trim();
    (!ty.is_empty()).then(|| (ty.to_string(), is_optional))
}

/// Extract the first parameter type from a type text (handles intersections and unions).
pub(super) fn contextual_first_parameter_type_from_text(type_text: &str) -> Option<String> {
    let type_text = type_text.trim();
    if type_text.is_empty() {
        return None;
    }
    let mut union_parts = Vec::new();
    for part in split_top_level_bytes(type_text, b'&') {
        let Some((ty, optional)) = extract_first_param_type_from_fn_type(&part) else {
            continue;
        };
        if !union_parts.iter().any(|existing| existing == &ty) {
            union_parts.push(ty);
        }
        if optional && !union_parts.iter().any(|existing| existing == "undefined") {
            union_parts.push("undefined".to_string());
        }
    }
    if union_parts.is_empty() {
        return None;
    }
    Some(normalize_union_type_text(&union_parts.join(" | ")))
}

/// Extract a parameter type at a given index from a type text.
pub(super) fn contextual_parameter_type_from_text(
    type_text: &str,
    param_index: usize,
) -> Option<String> {
    let type_text = type_text.trim();
    if type_text.is_empty() {
        return None;
    }
    let mut union_parts = Vec::new();
    for part in split_top_level_bytes(type_text, b'&') {
        let Some((ty, optional)) = extract_param_type_from_fn_type(&part, param_index) else {
            continue;
        };
        let ty = normalize_parameter_type_text(&ty);
        if !union_parts.iter().any(|existing| existing == &ty) {
            union_parts.push(ty);
        }
        if optional && !union_parts.iter().any(|existing| existing == "undefined") {
            union_parts.push("undefined".to_string());
        }
    }
    if union_parts.is_empty() {
        return None;
    }
    Some(normalize_union_type_text(&union_parts.join(" | ")))
}

/// Check if a parameter display string has `any` type and return the parameter name.
pub(super) fn parameter_name_if_any(display: &str) -> Option<String> {
    let rest = display.strip_prefix("(parameter) ")?;
    let (name, ty) = rest.split_once(':')?;
    let name = name.trim();
    let ty = ty.trim();
    if matches!(ty, "any" | "error" | "unknown") && !name.is_empty() {
        Some(name.to_string())
    } else {
        None
    }
}

/// Extract parameter type from a callable display string at a given index.
pub(super) fn parameter_type_from_callable_display(
    display: &str,
    parameter_index: usize,
) -> Option<String> {
    if let Some(close) = display.rfind("):")
        && let Some(open) = display[..close].rfind('(')
    {
        let params = display[open + 1..close].trim();
        if params.is_empty() {
            return None;
        }
        let param = split_top_level_bytes(params, b',')
            .into_iter()
            .nth(parameter_index)?;
        let (_, ty) = param.split_once(':')?;
        let ty = normalize_parameter_type_text(ty.trim());
        if ty != "any" {
            return Some(ty);
        }
    }

    // Fallback for property/variable quickinfo displays where the callable type
    // appears after a declaration prefix, e.g.
    // `(property) C.foo: (a: number, b: string) => void`.
    let (_, type_text) = display.split_once(": ")?;
    let ty = contextual_parameter_type_from_text(type_text, parameter_index)?;
    (ty != "any").then_some(ty)
}

/// Find the offset of a property name before a function position (for `prop: function` patterns).
pub(super) fn property_name_offset_before_function(
    source_text: &str,
    function_pos: u32,
) -> Option<u32> {
    let bytes = source_text.as_bytes();
    let mut cursor = function_pos as i32 - 1;
    while cursor >= 0 && bytes[cursor as usize].is_ascii_whitespace() {
        cursor -= 1;
    }
    if cursor < 0 || bytes[cursor as usize] != b':' {
        return None;
    }
    cursor -= 1;
    while cursor >= 0 && bytes[cursor as usize].is_ascii_whitespace() {
        cursor -= 1;
    }
    let end = cursor + 1;
    while cursor >= 0 && is_js_identifier_char(bytes[cursor as usize]) {
        cursor -= 1;
    }
    let start = cursor + 1;
    (start < end).then_some(start as u32)
}

/// Find the offset of an assignment LHS property name before a function position.
pub(super) fn assignment_lhs_property_offset_before_function(
    source_text: &str,
    function_pos: u32,
) -> Option<u32> {
    let bytes = source_text.as_bytes();
    let mut cursor = function_pos as i32 - 1;
    while cursor >= 0 && bytes[cursor as usize].is_ascii_whitespace() {
        cursor -= 1;
    }
    // Allow wrapped assignment RHS forms like `x.y = [function(...) {}]`
    // and `x.y = (function(...) {})` by skipping the immediate wrapper opener.
    while cursor >= 0 && matches!(bytes[cursor as usize], b'[' | b'(') {
        cursor -= 1;
        while cursor >= 0 && bytes[cursor as usize].is_ascii_whitespace() {
            cursor -= 1;
        }
    }
    if cursor < 0 || bytes[cursor as usize] != b'=' {
        return None;
    }
    cursor -= 1;
    while cursor >= 0 && bytes[cursor as usize].is_ascii_whitespace() {
        cursor -= 1;
    }
    let end = cursor + 1;
    while cursor >= 0 && is_js_identifier_char(bytes[cursor as usize]) {
        cursor -= 1;
    }
    let start = cursor + 1;
    if start >= end {
        return None;
    }
    Some(start as u32)
}

/// Find the nearest identifier offset within ±32 bytes of a base offset.
pub(super) fn nearest_identifier_offset(source_text: &str, base_offset: u32) -> Option<u32> {
    let bytes = source_text.as_bytes();
    let len = bytes.len() as u32;
    if len == 0 {
        return None;
    }
    let offset = base_offset.min(len.saturating_sub(1));
    if is_js_identifier_char(bytes[offset as usize]) {
        return Some(offset);
    }
    for step in 1..=32u32 {
        let forward = offset.saturating_add(step);
        if forward < len && is_js_identifier_char(bytes[forward as usize]) {
            return Some(forward);
        }
        let backward = offset.saturating_sub(step);
        if backward < len && is_js_identifier_char(bytes[backward as usize]) {
            return Some(backward);
        }
    }
    None
}

/// Extract the return type from an arrow type text like `(x: number) => string` → `"string"`.
pub(super) fn arrow_return_type_from_type_text(type_text: &str) -> Option<String> {
    let ret = type_text.rsplit("=>").next()?.trim();
    let ret = ret.trim_end_matches(')').trim();
    (!ret.is_empty()).then(|| ret.to_string())
}

/// Extract the first parameter type from an assignment context (text before arrow start).
pub(super) fn contextual_first_parameter_type_from_assignment(
    source_text: &str,
    arrow_start: u32,
) -> Option<String> {
    let before = source_text.get(..arrow_start as usize)?;
    let bytes = before.as_bytes();
    let mut depth = 0i32;
    let mut top_level_eq = None;
    let mut top_level_colon = None;
    for (i, b) in bytes.iter().enumerate() {
        match *b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'=' if depth == 0 => {
                let next = bytes.get(i + 1).copied();
                if next != Some(b'>') {
                    top_level_eq = Some(i);
                }
            }
            b':' if depth == 0 => top_level_colon = Some(i),
            _ => {}
        }
    }
    let eq = top_level_eq?;
    let before_eq = &before[..eq];
    let colon = top_level_colon?;
    let type_text = before_eq.get(colon + 1..)?.trim();
    contextual_first_parameter_type_from_text(type_text)
}

/// Find an interface member signature in source text (pure text search).
pub(super) fn find_interface_member_signature(
    source_text: &str,
    interface_name: &str,
    member_name: &str,
) -> Option<(String, String)> {
    let iface_pattern = format!("interface {interface_name}");
    let iface_start = source_text.find(&iface_pattern)?;
    let after_iface = &source_text[iface_start..];
    let body_start_rel = after_iface.find('{')?;
    let body_start = iface_start + body_start_rel + 1;
    let body_end = source_text[body_start..].find('}')? + body_start;
    let body = &source_text[body_start..body_end];

    let member_idx = body.find(member_name)?;
    let after_member = &body[member_idx + member_name.len()..];
    let colon_rel = after_member.find(':')?;
    let type_start = member_idx + member_name.len() + colon_rel + 1;
    let type_end = body[type_start..].find(';')? + type_start;
    let member_type = body[type_start..type_end].trim().to_string();

    let prefix = &body[..member_idx];
    let documentation = if let Some(doc_start) = prefix.rfind("/**") {
        if let Some(doc_end_rel) = prefix[doc_start..].find("*/") {
            let doc_end = doc_start + doc_end_rel + 2;
            clean_jsdoc_comment(&prefix[doc_start..doc_end])
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    Some((member_type, documentation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_quickinfo_display_string_normalizes_object_call_signature_spacing() {
        let display = "var c3t7: {\n    (n: number) : number;\n    (s1: string) : number;\n}";
        let normalized = normalize_quickinfo_display_string(display);
        assert_eq!(
            normalized,
            "var c3t7: {\n    (n: number): number;\n    (s1: string): number;\n}"
        );
    }

    #[test]
    fn assignment_lhs_property_offset_before_function_supports_array_wrapped_rhs() {
        let source = "objc8.t11 = [function(n, s) { return s; }];";
        let function_pos = source
            .find("function")
            .expect("function keyword should exist") as u32;
        let offset = assignment_lhs_property_offset_before_function(source, function_pos)
            .expect("should find lhs property offset");
        assert_eq!(
            source[offset as usize..]
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect::<String>(),
            "t11"
        );
    }

    #[test]
    fn contextual_parameter_type_from_text_extracts_function_array_parameter() {
        let type_text = "((n: number, s: string) => string)[]";
        assert_eq!(
            contextual_parameter_type_from_text(type_text, 0).as_deref(),
            Some("number")
        );
        assert_eq!(
            contextual_parameter_type_from_text(type_text, 1).as_deref(),
            Some("string")
        );
    }

    #[test]
    fn normalize_quickinfo_display_string_converts_single_call_signature_object_array() {
        let display = "var c3t11: {(n: number, s: string): string;}[]";
        let normalized = normalize_quickinfo_display_string(display);
        assert_eq!(
            normalized,
            "var c3t11: ((n: number, s: string) => string)[]"
        );
    }

    #[test]
    fn normalize_quickinfo_display_string_keeps_multi_signature_object_literal() {
        let display = "var c3t7: {\n    (n: number): number;\n    (s1: string): number;\n}";
        let normalized = normalize_quickinfo_display_string(display);
        assert_eq!(
            normalized,
            "var c3t7: {\n    (n: number): number;\n    (s1: string): number;\n}"
        );
    }

    #[test]
    fn normalize_quickinfo_display_string_multiline_index_signature_object() {
        let display = "(local var) r2: { [x: string]: T; }";
        let normalized = normalize_quickinfo_display_string(display);
        assert_eq!(normalized, "(local var) r2: {\n    [x: string]: T;\n}");
    }

    #[test]
    fn normalize_quickinfo_display_string_flattens_single_rest_tuple_param() {
        let display = "var fnWrapped: (...a: [str: string, num: number]) => void";
        let normalized = normalize_quickinfo_display_string(display);
        assert_eq!(
            normalized,
            "var fnWrapped: (str: string, num: number) => void"
        );
    }

    #[test]
    fn normalize_quickinfo_display_string_flattens_single_rest_tuple_param_variadic() {
        let display = "var fnVariadicWrapped: (...a: [str: string, ...num: number[]]) => void";
        let normalized = normalize_quickinfo_display_string(display);
        assert_eq!(
            normalized,
            "var fnVariadicWrapped: (str: string, ...num: number[]) => void"
        );
    }

    #[test]
    fn normalize_quickinfo_display_string_collapses_single_any_rest_param() {
        let display = "var fnNoParamsWrapped: (...a: any[]) => void";
        let normalized = normalize_quickinfo_display_string(display);
        assert_eq!(normalized, "var fnNoParamsWrapped: () => void");
    }

    #[test]
    fn is_type_annotation_context_detects_marker_after_type_reference() {
        let source = "const i: foo/*m*/ = { x: 1 };";
        let marker_start = source.find("/*m*/").expect("marker") as u32;
        assert!(is_type_annotation_context(source, marker_start));
    }
}
