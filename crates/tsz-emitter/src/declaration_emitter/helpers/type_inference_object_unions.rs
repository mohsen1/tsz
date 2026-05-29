//! Object/function union normalization helpers for declaration inference.
//!
//! These routines tidy inferred array element unions by dropping optional-
//! parameter function subtypes and aligning object union arms that differ by
//! sibling properties or methods.

use super::super::DeclarationEmitter;

pub(in crate::declaration_emitter) type NestedObjectMemberArmsByProperty =
    Vec<(String, Vec<(usize, Vec<ObjectTypeLiteralArm>)>)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::declaration_emitter) enum ObjectTypeLiteralEntry {
    Raw(String),
    Member(ObjectTypeLiteralMember),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::declaration_emitter) struct ObjectTypeLiteralMember {
    name: String,
    prefix: String,
    type_text: Option<String>,
    raw_text: String,
}

impl ObjectTypeLiteralMember {
    pub(in crate::declaration_emitter) fn typed(
        name: String,
        prefix: String,
        type_text: String,
    ) -> Self {
        let raw_text = format!("{prefix}: {type_text}");
        Self {
            name,
            prefix,
            type_text: Some(type_text),
            raw_text,
        }
    }

    pub(in crate::declaration_emitter) const fn raw(name: String, raw_text: String) -> Self {
        Self {
            name,
            prefix: String::new(),
            type_text: None,
            raw_text,
        }
    }

    fn optional_undefined(name: String) -> Self {
        Self::typed(name.clone(), format!("{name}?"), "undefined".to_string())
    }

    pub(in crate::declaration_emitter) fn render(&self) -> String {
        if let Some(type_text) = &self.type_text {
            format!("{}: {type_text}", self.prefix)
        } else {
            self.raw_text.clone()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::declaration_emitter) struct ObjectTypeLiteralArm {
    entries: Vec<ObjectTypeLiteralEntry>,
    depth: u32,
}

impl ObjectTypeLiteralArm {
    pub(in crate::declaration_emitter) const fn empty(depth: u32) -> Self {
        Self {
            entries: Vec::new(),
            depth,
        }
    }

    pub(in crate::declaration_emitter) const fn from_entries(
        entries: Vec<ObjectTypeLiteralEntry>,
        depth: u32,
    ) -> Self {
        Self { entries, depth }
    }

    fn member_names(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                ObjectTypeLiteralEntry::Member(member) => Some(member.name.clone()),
                ObjectTypeLiteralEntry::Raw(_) => None,
            })
            .collect()
    }

    fn member_mut(&mut self, name: &str) -> Option<&mut ObjectTypeLiteralMember> {
        self.entries.iter_mut().find_map(|entry| match entry {
            ObjectTypeLiteralEntry::Member(member) if member.name == name => Some(member),
            _ => None,
        })
    }

    fn insert_optional_undefined_members(&mut self, missing_names: &[String]) {
        if missing_names.is_empty() {
            return;
        }

        let insert_at = self
            .entries
            .iter()
            .position(|entry| match entry {
                ObjectTypeLiteralEntry::Member(member) => {
                    member.prefix.ends_with('?') && member.type_text.as_deref() == Some("undefined")
                }
                ObjectTypeLiteralEntry::Raw(_) => false,
            })
            .unwrap_or(self.entries.len());
        let missing = missing_names
            .iter()
            .cloned()
            .map(|name| {
                ObjectTypeLiteralEntry::Member(ObjectTypeLiteralMember::optional_undefined(name))
            })
            .collect::<Vec<_>>();
        self.entries.splice(insert_at..insert_at, missing);
    }

    pub(in crate::declaration_emitter) fn widen_primitive_literal_members(&mut self) {
        for entry in &mut self.entries {
            let ObjectTypeLiteralEntry::Member(member) = entry else {
                continue;
            };
            let Some(type_text) = member.type_text.as_deref() else {
                continue;
            };
            let Some(widened_type) =
                DeclarationEmitter::primitive_literal_member_widened_type(type_text)
            else {
                continue;
            };
            member.type_text = Some(widened_type.to_string());
        }
    }

    pub(in crate::declaration_emitter) fn render(&self) -> String {
        if self.entries.is_empty() {
            return "{}".to_string();
        }

        let member_indent = "    ".repeat((self.depth + 1) as usize);
        let closing_indent = "    ".repeat(self.depth as usize);
        let formatted_members = self
            .entries
            .iter()
            .map(|entry| match entry {
                ObjectTypeLiteralEntry::Raw(text) => {
                    DeclarationEmitter::format_object_member_entry(&member_indent, text)
                }
                ObjectTypeLiteralEntry::Member(member) => {
                    DeclarationEmitter::format_object_member_entry(&member_indent, &member.render())
                }
            })
            .collect::<Vec<_>>();
        format!("{{\n{}\n{closing_indent}}}", formatted_members.join("\n"))
    }
}

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn normalized_object_literal_union_arm_text(
        arms: Vec<ObjectTypeLiteralArm>,
        nested_member_arms_by_property: NestedObjectMemberArmsByProperty,
    ) -> Option<String> {
        let distinct =
            Self::normalized_object_literal_union_arms(arms, nested_member_arms_by_property);
        (!distinct.is_empty()).then(|| {
            distinct
                .iter()
                .map(ObjectTypeLiteralArm::render)
                .collect::<Vec<_>>()
                .join(" | ")
        })
    }

    pub(in crate::declaration_emitter) fn normalized_object_literal_union_arms(
        arms: Vec<ObjectTypeLiteralArm>,
        nested_member_arms_by_property: NestedObjectMemberArmsByProperty,
    ) -> Vec<ObjectTypeLiteralArm> {
        let mut distinct = Vec::<ObjectTypeLiteralArm>::new();
        for arm in arms {
            if !distinct.iter().any(|existing| existing == &arm) {
                distinct.push(arm);
            }
        }
        Self::expand_source_object_union_arms_from_sibling_properties(&mut distinct);
        Self::expand_source_nested_object_union_member_properties(
            &mut distinct,
            nested_member_arms_by_property,
        );
        distinct
    }

    fn expand_source_object_union_arms_from_sibling_properties(arms: &mut [ObjectTypeLiteralArm]) {
        if arms.len() <= 1 {
            return;
        }

        let mut property_names = Vec::<String>::new();
        for arm in arms.iter() {
            for name in arm.member_names() {
                if !property_names.iter().any(|existing| existing == &name) {
                    property_names.push(name);
                }
            }
        }

        if property_names.is_empty() {
            return;
        }

        Self::expand_source_object_arms_with_property_names(arms, &property_names);
    }

    fn expand_source_object_arms_with_property_names(
        arms: &mut [ObjectTypeLiteralArm],
        property_names: &[String],
    ) {
        for arm in arms {
            let present_names = arm.member_names();
            let missing_names = property_names
                .iter()
                .filter(|name| !present_names.iter().any(|present| present == *name))
                .cloned()
                .collect::<Vec<_>>();
            arm.insert_optional_undefined_members(&missing_names);
        }
    }

    fn expand_source_nested_object_union_member_properties(
        arms: &mut [ObjectTypeLiteralArm],
        nested_member_arms_by_property: NestedObjectMemberArmsByProperty,
    ) {
        if arms.len() <= 1 || nested_member_arms_by_property.is_empty() {
            return;
        }

        for (property_name, nested_arms_by_outer) in nested_member_arms_by_property {
            let mut all_nested_arms = Vec::<ObjectTypeLiteralArm>::new();
            let mut replacements = Vec::<(usize, Vec<ObjectTypeLiteralArm>)>::new();

            for (outer_idx, mut nested_arms) in nested_arms_by_outer {
                for arm in &mut nested_arms {
                    arm.widen_primitive_literal_members();
                    if !all_nested_arms.iter().any(|existing| existing == arm) {
                        all_nested_arms.push(arm.clone());
                    }
                }
                replacements.push((outer_idx, nested_arms));
            }

            if replacements.len() <= 1 || all_nested_arms.len() <= 1 {
                continue;
            }

            let sibling_names = all_nested_arms
                .iter()
                .flat_map(ObjectTypeLiteralArm::member_names)
                .fold(Vec::<String>::new(), |mut names, name| {
                    if !names.iter().any(|existing| existing == &name) {
                        names.push(name);
                    }
                    names
                });
            if sibling_names.is_empty() {
                continue;
            }

            for (outer_idx, mut nested_arms) in replacements {
                Self::expand_source_object_arms_with_property_names(
                    &mut nested_arms,
                    &sibling_names,
                );
                let Some(member) = arms
                    .get_mut(outer_idx)
                    .and_then(|arm| arm.member_mut(&property_name))
                else {
                    continue;
                };
                member.type_text = Some(
                    nested_arms
                        .iter()
                        .map(ObjectTypeLiteralArm::render)
                        .collect::<Vec<_>>()
                        .join(" | "),
                );
            }
        }
    }

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
