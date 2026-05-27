//! Object/function union normalization helpers for declaration inference.
//!
//! These routines tidy inferred array element unions by dropping optional-
//! parameter function subtypes and aligning object union arms that differ by
//! sibling properties or methods.

use super::super::DeclarationEmitter;

type NestedObjectMemberArmsByProperty = Vec<(String, Vec<(usize, Vec<String>)>)>;

#[derive(Clone, Debug)]
pub(in crate::declaration_emitter) struct ObjectTypePropertyEntry {
    name: String,
    type_text: String,
    start_line: usize,
    end_line: usize,
    prefix: String,
}

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

        // Top-level member names for each arm that is an object type literal
        // (`{ ... }`); `None` for non-object arms (primitives, named refs,
        // function types, etc.), which are left untouched.
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

        // Union of every property/method name appearing in any object arm.
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

        // tsc normalizes object literals in a union upon widening: every arm
        // gains an optional `name?: undefined` member for each sibling
        // property it does not itself declare. This applies to every object
        // arm — the empty `{}` arm and property-only arms included — not just
        // arms that happen to contain a method.
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

    pub(in crate::declaration_emitter) fn expand_nested_object_union_member_properties(
        types: &mut [String],
    ) {
        if types.len() <= 1 {
            return;
        }

        let entries_by_arm = types
            .iter()
            .map(|ty| Self::object_type_top_level_property_entries(ty))
            .collect::<Vec<_>>();

        let mut property_names = Vec::<String>::new();
        for entries in entries_by_arm.iter().flatten() {
            for entry in entries {
                if !property_names.iter().any(|name| name == &entry.name) {
                    property_names.push(entry.name.clone());
                }
            }
        }

        for property_name in property_names {
            let mut nested_arms_by_outer =
                Vec::<(usize, ObjectTypePropertyEntry, Vec<String>)>::new();
            let mut all_nested_arms = Vec::<String>::new();

            for (outer_idx, entries) in entries_by_arm.iter().enumerate() {
                let Some(entries) = entries else {
                    continue;
                };
                let Some(entry) = entries.iter().find(|entry| entry.name == property_name) else {
                    continue;
                };
                let trimmed = entry.type_text.trim();
                if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
                    continue;
                }
                let mut nested_arms = vec![trimmed.to_string()];
                nested_arms = nested_arms
                    .into_iter()
                    .map(|arm| Self::widen_object_literal_member_primitive_literal_types(&arm))
                    .collect();
                for arm in &nested_arms {
                    if !all_nested_arms.iter().any(|existing| existing == arm) {
                        all_nested_arms.push(arm.clone());
                    }
                }
                nested_arms_by_outer.push((outer_idx, entry.clone(), nested_arms));
            }

            if nested_arms_by_outer.len() <= 1 || all_nested_arms.len() <= 1 {
                continue;
            }
            let sibling_names = Self::object_union_sibling_property_names(&all_nested_arms);
            if sibling_names.is_empty() {
                continue;
            }

            let mut replacements = Vec::<(usize, ObjectTypePropertyEntry, String)>::new();
            for (outer_idx, entry, mut nested_arms) in nested_arms_by_outer {
                Self::expand_object_arms_with_property_names(&mut nested_arms, &sibling_names);
                replacements.push((outer_idx, entry, nested_arms.join(" | ")));
            }

            for (outer_idx, entry, nested_type_text) in replacements.into_iter().rev() {
                if let Some(replaced) = Self::replace_object_property_type_text(
                    &types[outer_idx],
                    &entry,
                    &nested_type_text,
                ) {
                    types[outer_idx] = replaced;
                }
            }
        }
    }

    pub(in crate::declaration_emitter) fn expand_nested_object_union_member_properties_from_source(
        types: &mut [String],
        nested_member_arms_by_property: NestedObjectMemberArmsByProperty,
    ) {
        if types.len() <= 1 || nested_member_arms_by_property.is_empty() {
            return;
        }

        let entries_by_arm = types
            .iter()
            .map(|ty| Self::object_type_top_level_property_entries(ty))
            .collect::<Vec<_>>();

        for (property_name, nested_arms_by_outer) in nested_member_arms_by_property {
            let mut all_nested_arms = Vec::<String>::new();
            let mut replacements = Vec::<(usize, ObjectTypePropertyEntry, Vec<String>)>::new();

            for (outer_idx, nested_arms) in nested_arms_by_outer {
                let Some(entries) = entries_by_arm.get(outer_idx).and_then(Option::as_ref) else {
                    continue;
                };
                let Some(entry) = entries.iter().find(|entry| entry.name == property_name) else {
                    continue;
                };
                let nested_arms = nested_arms
                    .into_iter()
                    .map(|arm| Self::widen_object_literal_member_primitive_literal_types(&arm))
                    .collect::<Vec<_>>();
                for arm in &nested_arms {
                    if !all_nested_arms.iter().any(|existing| existing == arm) {
                        all_nested_arms.push(arm.clone());
                    }
                }
                replacements.push((outer_idx, entry.clone(), nested_arms));
            }

            if replacements.len() <= 1 || all_nested_arms.len() <= 1 {
                continue;
            }
            let sibling_names = Self::object_union_sibling_property_names(&all_nested_arms);
            if sibling_names.is_empty() {
                continue;
            }

            for (outer_idx, entry, mut nested_arms) in replacements.into_iter().rev() {
                Self::expand_object_arms_with_property_names(&mut nested_arms, &sibling_names);
                if let Some(replaced) = Self::replace_object_property_type_text(
                    &types[outer_idx],
                    &entry,
                    &nested_arms.join(" | "),
                ) {
                    types[outer_idx] = replaced;
                }
            }
        }
    }

    pub(in crate::declaration_emitter) fn widen_object_literal_member_primitive_literal_types(
        type_text: &str,
    ) -> String {
        let Some(entries) = Self::object_type_top_level_property_entries(type_text) else {
            return type_text.to_string();
        };
        let mut widened = type_text.to_string();
        for entry in entries.into_iter().rev() {
            let Some(widened_type) = Self::primitive_literal_member_widened_type(&entry.type_text)
            else {
                continue;
            };
            if let Some(replaced) =
                Self::replace_object_property_type_text(&widened, &entry, widened_type)
            {
                widened = replaced;
            }
        }
        widened
    }

    fn primitive_literal_member_widened_type(type_text: &str) -> Option<&'static str> {
        let trimmed = type_text.trim();
        if matches!(trimmed, "true" | "false") {
            return Some("boolean");
        }
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            return Some("string");
        }
        if !trimmed.is_empty()
            && trimmed
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'-' | b'.'))
            && trimmed.parse::<f64>().is_ok()
        {
            return Some("number");
        }
        None
    }

    fn object_union_sibling_property_names(types: &[String]) -> Vec<String> {
        let mut property_names = Vec::<String>::new();
        for ty in types {
            let trimmed = ty.trim();
            if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
                continue;
            }
            for name in Self::object_type_top_level_member_names(ty, true) {
                if !property_names.iter().any(|existing| existing == &name) {
                    property_names.push(name);
                }
            }
        }
        property_names
    }

    fn expand_object_arms_with_property_names(types: &mut [String], property_names: &[String]) {
        for ty in types {
            let trimmed = ty.trim();
            if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
                continue;
            }
            let present_names = Self::object_type_top_level_member_names(ty, true);
            let missing_names = property_names
                .iter()
                .filter(|name| !present_names.iter().any(|present| present == *name))
                .cloned()
                .collect::<Vec<_>>();
            if !missing_names.is_empty() {
                *ty = Self::append_optional_undefined_members_before_existing_optionals(
                    ty,
                    &missing_names,
                );
            }
        }
    }

    fn append_optional_undefined_members_before_existing_optionals(
        type_text: &str,
        missing_names: &[String],
    ) -> String {
        if type_text.trim() == "{}" {
            return Self::append_optional_undefined_members(type_text, missing_names);
        }

        let mut lines = type_text.lines().map(str::to_string).collect::<Vec<_>>();
        let close_at = lines
            .iter()
            .rposition(|line| line.trim() == "}")
            .unwrap_or(lines.len());
        let insert_at = lines
            .iter()
            .take(close_at)
            .position(|line| line.trim_end().ends_with("?: undefined;"))
            .unwrap_or(close_at);

        let indent: String = lines[..close_at]
            .iter()
            .rev()
            .find(|line| !line.trim().is_empty() && line.trim() != "{")
            .map(|line| {
                line.chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect()
            })
            .unwrap_or_else(|| "    ".to_string());

        let missing_members = missing_names
            .iter()
            .map(|name| format!("{indent}{name}?: undefined;"))
            .collect::<Vec<_>>();
        lines.splice(insert_at..insert_at, missing_members);
        lines.join("\n")
    }

    fn object_type_top_level_property_entries(
        type_text: &str,
    ) -> Option<Vec<ObjectTypePropertyEntry>> {
        let trimmed = type_text.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') || trimmed == "{}" {
            return None;
        }

        let lines = trimmed.lines().collect::<Vec<_>>();
        if lines.len() < 2 {
            return None;
        }

        let mut depth = 0usize;
        let mut starts = Vec::<usize>::new();
        for (idx, line) in lines.iter().enumerate() {
            if depth == 1 && Self::object_type_property_name_from_line(line).is_some() {
                starts.push(idx);
            }
            depth = Self::update_object_text_brace_depth(depth, line);
        }

        if starts.is_empty() {
            return None;
        }

        let root_close = lines.len().saturating_sub(1);
        let mut entries = Vec::with_capacity(starts.len());
        for (start_pos, start_line) in starts.iter().copied().enumerate() {
            let end_line = starts
                .get(start_pos + 1)
                .copied()
                .map(|next| next.saturating_sub(1))
                .unwrap_or_else(|| root_close.saturating_sub(1));
            let first_line = lines[start_line];
            let colon = Self::top_level_property_type_colon(first_line)?;
            let name = Self::object_type_property_name_from_line(first_line)?;
            let prefix = first_line.get(..colon)?.to_string();
            let mut type_lines = Vec::new();
            let first_type = first_line.get(colon + 1..)?.trim_start();
            if !first_type.is_empty() {
                type_lines.push(first_type.to_string());
            }
            for line in &lines[start_line + 1..=end_line] {
                type_lines.push((*line).to_string());
            }
            if let Some(last) = type_lines.last_mut() {
                *last = last.trim_end().trim_end_matches(';').to_string();
            }
            entries.push(ObjectTypePropertyEntry {
                name,
                type_text: type_lines.join("\n").trim().to_string(),
                start_line,
                end_line,
                prefix,
            });
        }
        Some(entries)
    }

    fn replace_object_property_type_text(
        type_text: &str,
        entry: &ObjectTypePropertyEntry,
        replacement_type_text: &str,
    ) -> Option<String> {
        let mut lines = type_text
            .trim()
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();
        if entry.start_line >= lines.len() || entry.end_line >= lines.len() {
            return None;
        }

        let mut replacement_lines = Vec::new();
        let mut type_lines = replacement_type_text.trim().lines();
        let first_type_line = type_lines.next().unwrap_or(replacement_type_text.trim());
        replacement_lines.push(format!("{}: {}", entry.prefix, first_type_line));
        for line in type_lines {
            replacement_lines.push(line.to_string());
        }
        if let Some(last) = replacement_lines.last_mut()
            && !last.trim_end().ends_with(';')
        {
            last.push(';');
        }
        lines.splice(entry.start_line..=entry.end_line, replacement_lines);
        Some(lines.join("\n"))
    }

    pub(in crate::declaration_emitter) fn append_optional_undefined_members(
        type_text: &str,
        missing_names: &[String],
    ) -> String {
        if type_text.trim() == "{}" {
            let missing_members = missing_names
                .iter()
                .map(|name| format!("    {name}?: undefined;"))
                .collect::<Vec<_>>();
            return format!("{{\n{}\n}}", missing_members.join("\n"));
        }

        let mut lines = type_text.lines().map(str::to_string).collect::<Vec<_>>();
        let insert_at = lines
            .iter()
            .rposition(|line| line.trim() == "}")
            .unwrap_or(lines.len());

        // Match the existing members' indentation so nested object arms (e.g.
        // array elements inside a JSON object) keep their column alignment
        // instead of being forced to a fixed two-level indent.
        let indent: String = lines[..insert_at]
            .iter()
            .rev()
            .find(|line| !line.trim().is_empty() && line.trim() != "{")
            .map(|line| {
                line.chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect()
            })
            .unwrap_or_else(|| "    ".to_string());

        let missing_members = missing_names
            .iter()
            .map(|name| format!("{indent}{name}?: undefined;"))
            .collect::<Vec<_>>();
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
