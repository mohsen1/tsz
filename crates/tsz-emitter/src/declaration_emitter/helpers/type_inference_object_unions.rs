//! Object/function union normalization helpers for declaration inference.
//!
//! These routines tidy inferred array element unions by dropping optional-
//! parameter function subtypes and aligning object union arms that differ by
//! sibling properties or methods.

use super::super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    /// Drop function-type union arms whose only difference from another arm
    /// is that one or more parameters were marked optional. tsc's array
    /// element type formation applies UnionReduction.Subtype which removes
    /// `(x?: T) => R` when `(x: T) => R` is also in the union, because the
    /// optional-parameter form is a structural subtype of the required-
    /// parameter form. Mirroring that here keeps inferred-array element
    /// unions tidy without any solver-level subtype reduction work.
    pub(in crate::declaration_emitter) fn drop_optional_param_function_subtypes(
        types: &mut Vec<String>,
    ) {
        if types.len() <= 1 {
            return;
        }
        let normalized: Vec<Option<String>> = types
            .iter()
            .map(|ty| Self::function_text_required_param_form(ty))
            .collect();
        let mut to_drop = vec![false; types.len()];
        for (i, normalized_i) in normalized.iter().enumerate() {
            let Some(required_form) = normalized_i else {
                continue;
            };
            // Already a required form (no `?`); leave it alone.
            if required_form == &types[i] {
                continue;
            }
            // Drop this optional-form arm if a sibling arm is the matching
            // required form (either the unchanged text equals the
            // normalized form, or another sibling's required form equals
            // ours — handles two optional forms whose required-equivalents
            // collapse together).
            let has_required_sibling = types
                .iter()
                .enumerate()
                .any(|(j, sibling)| j != i && sibling == required_form);
            if has_required_sibling {
                to_drop[i] = true;
            }
        }
        let mut idx = 0;
        types.retain(|_| {
            let keep = !to_drop[idx];
            idx += 1;
            keep
        });
    }

    /// Produce the "required-parameter" canonical form of a function-type
    /// text by replacing every `param?: T` token with `param: T` at the top
    /// level of the parameter list. Returns `None` if the input does not
    /// look like a function or constructor type (no `=>` or no parameter
    /// list). Only walks the FIRST parenthesized group at the start of the
    /// text — the parameter list — so optional members of nested object
    /// types in the return type are never touched.
    pub(in crate::declaration_emitter) fn function_text_required_param_form(
        type_text: &str,
    ) -> Option<String> {
        if !type_text.contains("=>") {
            return None;
        }
        let bytes = type_text.as_bytes();
        // Skip optional `new ` prefix (constructor types).
        let mut start = 0;
        while start < bytes.len() && bytes[start].is_ascii_whitespace() {
            start += 1;
        }
        if type_text[start..].starts_with("new ") {
            start += 4;
            while start < bytes.len() && bytes[start].is_ascii_whitespace() {
                start += 1;
            }
        }
        if start >= bytes.len() || bytes[start] != b'(' {
            return None;
        }
        let mut depth = 0usize;
        let mut end = start;
        while end < bytes.len() {
            match bytes[end] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        end += 1;
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }
        if depth != 0 {
            return None;
        }
        let prefix = &type_text[..start];
        let params = &type_text[start..end];
        let suffix = &type_text[end..];

        // Strip `?` immediately preceding `:` at the top level of `params`.
        // Skip over nested parens/brackets/braces/quotes.
        let pb = params.as_bytes();
        let mut out = String::with_capacity(params.len());
        let mut i = 0;
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut brace = 0i32;
        let mut angle = 0i32;
        while i < pb.len() {
            let ch = pb[i];
            match ch {
                b'(' => paren += 1,
                b')' => paren -= 1,
                b'[' => bracket += 1,
                b']' => bracket -= 1,
                b'{' => brace += 1,
                b'}' => brace -= 1,
                b'<' => angle += 1,
                b'>' => angle -= 1,
                b'"' | b'\'' => {
                    let quote = ch;
                    out.push(ch as char);
                    i += 1;
                    while i < pb.len() {
                        let c = pb[i];
                        out.push(c as char);
                        i += 1;
                        if c == b'\\' && i < pb.len() {
                            out.push(pb[i] as char);
                            i += 1;
                            continue;
                        }
                        if c == quote {
                            break;
                        }
                    }
                    continue;
                }
                _ => {}
            }
            // Detect `?:` at depth==1 (top level of param list — outer paren counts as 1).
            if ch == b'?'
                && paren == 1
                && bracket == 0
                && brace == 0
                && angle == 0
                && i + 1 < pb.len()
                && pb[i + 1] == b':'
            {
                // Skip the `?`, keep the `:` next iteration.
                i += 1;
                continue;
            }
            out.push(ch as char);
            i += 1;
        }
        Some(format!("{prefix}{out}{suffix}"))
    }

    pub(in crate::declaration_emitter) fn expand_object_union_arms_from_sibling_properties(
        types: &mut [String],
    ) {
        if types.len() <= 1 {
            return;
        }

        let has_empty_arm = types.iter().any(|ty| ty.trim() == "{}");
        let object_arm_names = types
            .iter()
            .map(|ty| {
                let trimmed = ty.trim();
                if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
                    return None;
                }
                Some(Self::object_type_top_level_member_names(ty, true))
            })
            .collect::<Vec<_>>();

        let mut property_names = Vec::<String>::new();
        for names in object_arm_names.iter().flatten() {
            for name in names {
                if !property_names.iter().any(|existing| existing == name) {
                    property_names.push(name.clone());
                }
            }
        }
        if property_names.is_empty() {
            return;
        }

        if has_empty_arm {
            let members = property_names
                .into_iter()
                .map(|name| format!("    {name}?: undefined;"))
                .collect::<Vec<_>>()
                .join("\n");
            let replacement = format!("{{\n{members}\n}}");
            for ty in types.iter_mut() {
                if ty.trim() == "{}" {
                    *ty = replacement.clone();
                }
            }
            return;
        }

        if !types
            .iter()
            .any(|ty| Self::object_type_has_top_level_method(ty))
        {
            return;
        }

        for (ty, present_names) in types.iter_mut().zip(object_arm_names) {
            let Some(present_names) = present_names else {
                continue;
            };
            let missing_names = property_names
                .iter()
                .filter(|name| !present_names.iter().any(|present| present == *name))
                .cloned()
                .collect::<Vec<_>>();
            if !missing_names.is_empty() {
                *ty = Self::append_optional_undefined_members(ty, &missing_names);
            }
        }
    }

    pub(in crate::declaration_emitter) fn append_optional_undefined_members(
        type_text: &str,
        missing_names: &[String],
    ) -> String {
        let missing_members = missing_names
            .iter()
            .map(|name| format!("    {name}?: undefined;"))
            .collect::<Vec<_>>();

        if type_text.trim() == "{}" {
            return format!("{{\n{}\n}}", missing_members.join("\n"));
        }

        let mut lines = type_text.lines().map(str::to_string).collect::<Vec<_>>();
        let insert_at = lines
            .iter()
            .rposition(|line| line.trim() == "}")
            .unwrap_or(lines.len());
        lines.splice(insert_at..insert_at, missing_members);
        lines.join("\n")
    }

    pub(in crate::declaration_emitter) fn object_type_top_level_member_names(
        type_text: &str,
        include_methods: bool,
    ) -> Vec<String> {
        let trimmed = type_text.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') || trimmed == "{}" {
            return Vec::new();
        }

        let mut depth = 0usize;
        let mut names = Vec::new();
        for line in trimmed.lines() {
            if depth == 1
                && let Some(name) = Self::object_type_member_name_from_line(line, include_methods)
            {
                names.push(name);
            }
            depth = Self::update_object_text_brace_depth(depth, line);
        }
        names
    }

    pub(in crate::declaration_emitter) fn object_type_has_top_level_method(
        type_text: &str,
    ) -> bool {
        let trimmed = type_text.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') || trimmed == "{}" {
            return false;
        }

        let mut depth = 0usize;
        for line in trimmed.lines() {
            if depth == 1
                && Self::object_type_member_name_from_line(line, true).is_some_and(|name| {
                    let trimmed_line = line.trim_start();
                    trimmed_line.starts_with(&format!("{name}("))
                        || trimmed_line.starts_with(&format!("{name}?("))
                })
            {
                return true;
            }
            depth = Self::update_object_text_brace_depth(depth, line);
        }
        false
    }

    pub(in crate::declaration_emitter) fn update_object_text_brace_depth(
        depth: usize,
        line: &str,
    ) -> usize {
        let mut depth = depth;
        let mut quote: Option<u8> = None;
        let mut escaped = false;
        for byte in line.bytes() {
            if let Some(active_quote) = quote {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == active_quote {
                    quote = None;
                }
                continue;
            }

            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'{' => depth += 1,
                b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        depth
    }

    pub(in crate::declaration_emitter) fn object_type_member_name_from_line(
        line: &str,
        include_methods: bool,
    ) -> Option<String> {
        if !include_methods {
            return Self::object_type_property_name_from_line(line);
        }

        let line = line.trim().trim_end_matches(';').trim();
        if line.is_empty() || line == "{" || line == "}" || line.starts_with('[') {
            return None;
        }

        let colon = Self::top_level_property_type_colon(line)?;
        let name = line
            .get(..colon)?
            .trim()
            .strip_prefix("readonly ")
            .unwrap_or_else(|| line.get(..colon).unwrap_or_default().trim())
            .trim()
            .trim_end_matches('?')
            .trim();

        if name.contains('(') {
            if !include_methods {
                return None;
            }
            return Self::object_type_method_name_from_prefix(name);
        }

        (!name.is_empty()).then(|| name.to_string())
    }

    pub(in crate::declaration_emitter) fn object_type_method_name_from_prefix(
        prefix: &str,
    ) -> Option<String> {
        let paren = prefix.find('(')?;
        let name = prefix.get(..paren)?.trim().trim_end_matches('?').trim();
        if name.is_empty() || name == "new" || name.contains(' ') {
            return None;
        }
        Some(name.to_string())
    }

    pub(in crate::declaration_emitter) fn object_type_property_name_from_line(
        line: &str,
    ) -> Option<String> {
        let line = line.trim().trim_end_matches(';').trim();
        if line.is_empty() || line == "{" || line == "}" || line.starts_with('[') {
            return None;
        }
        let colon = Self::top_level_property_type_colon(line)?;
        let name = line
            .get(..colon)?
            .trim()
            .strip_prefix("readonly ")
            .unwrap_or_else(|| line.get(..colon).unwrap_or_default().trim())
            .trim()
            .trim_end_matches('?')
            .trim();
        if name.contains('(') {
            return None;
        }
        (!name.is_empty()).then(|| name.to_string())
    }

    pub(in crate::declaration_emitter) fn top_level_property_type_colon(
        line: &str,
    ) -> Option<usize> {
        let mut quote: Option<u8> = None;
        let mut escaped = false;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        for (index, byte) in line.bytes().enumerate() {
            if let Some(active_quote) = quote {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == active_quote {
                    quote = None;
                }
                continue;
            }

            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b':' if paren_depth == 0 && bracket_depth == 0 => return Some(index),
                _ => {}
            }
        }
        None
    }
}
