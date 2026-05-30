use super::*;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn parse_jsdoc_callback_alias(
        jsdoc: &str,
    ) -> Option<(String, String)> {
        let (name, type_text, _) = Self::parse_jsdoc_callback_alias_parts(jsdoc)?;
        Some((name, type_text))
    }

    fn parse_jsdoc_callback_alias_parts(jsdoc: &str) -> Option<(String, String, Vec<String>)> {
        let mut name = None;
        let mut params = Vec::new();
        let mut return_type = None;
        let mut description_lines = Self::jsdoc_description_lines(jsdoc);
        let mut collecting_callback_description = false;
        let mut seen_callback = false;

        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            if line.is_empty() {
                collecting_callback_description = false;
                continue;
            }

            if let Some(rest) = line.strip_prefix("@callback") {
                let rest = rest.trim();
                if let Some(callback_name) = rest.split_whitespace().next()
                    && !callback_name.is_empty()
                {
                    name = Some(callback_name.to_string());
                    let tail = rest[callback_name.len()..].trim();
                    if !tail.is_empty() {
                        description_lines.push(tail.to_string());
                    }
                }
                seen_callback = true;
                collecting_callback_description = true;
                continue;
            }

            if let Some(rest) = line.strip_prefix("@param") {
                collecting_callback_description = false;
                if !seen_callback {
                    continue;
                }
                let rest = rest.trim();
                if rest.starts_with('{')
                    && let Some(end) = rest[1..].find('}')
                {
                    let type_expr = rest[1..1 + end].trim();
                    let param_name = rest[2 + end..]
                        .split_whitespace()
                        .next()
                        .filter(|name| !name.is_empty())
                        .unwrap_or("arg");
                    let (rest_param, base_type) =
                        if let Some(stripped) = type_expr.strip_prefix("...") {
                            (true, stripped.trim())
                        } else {
                            (false, type_expr)
                        };
                    let ts_type = if base_type == "*" {
                        "any".to_string()
                    } else if rest_param {
                        format!("{base_type}[]")
                    } else {
                        base_type.to_string()
                    };
                    if rest_param {
                        params.push(format!("...{param_name}: {ts_type}"));
                    } else {
                        params.push(format!("{param_name}: {ts_type}"));
                    }
                } else if let Some(param) = Self::parse_legacy_jsdoc_param_decl(line) {
                    params.push(format!("{}: any", param.name));
                }
                continue;
            }

            if let Some(rest) = line
                .strip_prefix("@returns")
                .or_else(|| line.strip_prefix("@return"))
            {
                collecting_callback_description = false;
                let rest = rest.trim();
                if rest.starts_with('{')
                    && let Some(end) = rest[1..].find('}')
                {
                    let type_expr = rest[1..1 + end].trim();
                    return_type = Some(if type_expr == "*" {
                        "any".to_string()
                    } else {
                        type_expr.to_string()
                    });
                }
                continue;
            }

            if line.starts_with('@') {
                collecting_callback_description = false;
                continue;
            }

            if collecting_callback_description {
                description_lines.push(format!("    {line}"));
            }
        }

        let name = name?;
        let return_type = return_type.unwrap_or_else(|| "any".to_string());
        Some((
            name,
            format!("({}) => {return_type}", params.join(", ")),
            description_lines,
        ))
    }

    pub(crate) fn parse_jsdoc_template_params(jsdoc: &str) -> Vec<String> {
        let mut params = Vec::new();
        let mut seen = FxHashSet::default();

        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            let Some(rest) = line.strip_prefix("@template") else {
                continue;
            };

            let mut rest = Self::trim_jsdoc_template_description(
                Self::trim_jsdoc_same_line_following_tags(rest.trim()),
            );
            if let Some((constraint, name_rest)) = Self::parse_jsdoc_braced_type_and_name(rest)
                && let Some((name, remaining)) = Self::take_jsdoc_template_name(name_rest)
            {
                let constraint = Self::normalize_jsdoc_type_text(constraint, false);
                let name_str = Self::format_constrained_jsdoc_template_param(name, &constraint);
                let name_key = Self::jsdoc_template_param_name_key(&name_str).to_string();
                if seen.insert(name_key) {
                    params.push(name_str);
                }
                rest = Self::trim_jsdoc_template_description(remaining);
            }

            for name in Self::split_jsdoc_template_param_segments(rest) {
                // Bracket-default form `@template [T=string]` declares type
                // parameter `T` with default `string`. Without unwrapping the
                // brackets, the verbatim segment `[T=string]` would be
                // emitted between `<` and `>` and produce invalid `.d.ts`
                // output (issue #4005).
                let normalized = Self::normalize_jsdoc_template_bracket_default(name);
                let name_str = normalized.into_owned();
                let key = Self::jsdoc_template_param_name_key(&name_str).to_string();
                if seen.insert(key) {
                    params.push(name_str);
                }
            }
        }

        params
    }

    fn split_jsdoc_template_param_segments(text: &str) -> Vec<&str> {
        let mut segments = Vec::new();
        let mut start = None;
        let mut bracket_depth = 0usize;

        for (idx, ch) in text.char_indices() {
            if start.is_none() {
                if matches!(ch, ',' | ' ' | '\t') {
                    continue;
                }
                start = Some(idx);
            }

            match ch {
                '[' => bracket_depth += 1,
                ']' if bracket_depth > 0 => bracket_depth -= 1,
                ',' | ' ' | '\t' if bracket_depth == 0 => {
                    if let Some(seg_start) = start.take() {
                        let segment = text[seg_start..idx].trim();
                        if !segment.is_empty() {
                            segments.push(segment);
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(seg_start) = start {
            let segment = text[seg_start..].trim();
            if !segment.is_empty() {
                segments.push(segment);
            }
        }

        segments
    }

    pub(in crate::declaration_emitter) fn trim_jsdoc_same_line_following_tags(text: &str) -> &str {
        text.find(" @")
            .map(|idx| text[..idx].trim_end())
            .unwrap_or(text)
    }

    fn trim_jsdoc_template_description(text: &str) -> &str {
        for (idx, ch) in text.char_indices() {
            if ch != '-' {
                continue;
            }
            let before_is_boundary = text[..idx]
                .chars()
                .next_back()
                .is_none_or(char::is_whitespace);
            let after_is_boundary = text[idx + ch.len_utf8()..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace);
            if before_is_boundary && after_is_boundary {
                return text[..idx].trim_end();
            }
        }
        text
    }

    /// Strip `[…]` from a `@template` segment and rewrite `T=default` as
    /// `T = default` so the result is valid TypeScript type-parameter
    /// syntax. Non-bracket segments are returned unchanged.
    fn normalize_jsdoc_template_bracket_default(segment: &str) -> std::borrow::Cow<'_, str> {
        let trimmed = segment.trim();
        if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
            return std::borrow::Cow::Borrowed(segment);
        }
        let inner = &trimmed[1..trimmed.len() - 1];
        if let Some((name, default)) = inner.split_once('=') {
            std::borrow::Cow::Owned(format!("{} = {}", name.trim(), default.trim()))
        } else {
            std::borrow::Cow::Owned(inner.trim().to_string())
        }
    }

    fn format_constrained_jsdoc_template_param(name: &str, constraint: &str) -> String {
        let trimmed = name.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let inner = &trimmed[1..trimmed.len() - 1];
            let (name, default) = inner
                .split_once('=')
                .map(|(name, default)| {
                    let default = default.trim();
                    (
                        name.trim(),
                        if default.is_empty() { "any" } else { default },
                    )
                })
                .unwrap_or_else(|| (inner.trim(), "any"));
            return format!("{name} extends {constraint} = {default}");
        }
        format!("{trimmed} extends {constraint}")
    }

    pub(in crate::declaration_emitter) fn jsdoc_template_param_name_key(text: &str) -> &str {
        let trimmed = text.trim();
        let end = trimmed
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(trimmed.len());
        trimmed[..end].trim()
    }

    fn take_jsdoc_template_name(text: &str) -> Option<(&str, &str)> {
        let text = text.trim_start_matches([',', ' ', '\t']);
        if text.is_empty() {
            return None;
        }

        let end = if text.starts_with('[') {
            text.find(']')
                .map(|idx| idx + 1)
                .unwrap_or_else(|| text.find([',', ' ', '\t']).unwrap_or(text.len()))
        } else {
            text.find([',', ' ', '\t']).unwrap_or(text.len())
        };
        let name = text[..end].trim();
        if name.is_empty() {
            return None;
        }
        Some((name, &text[end..]))
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_typedef_alias(
        jsdoc: &str,
    ) -> Option<(String, String)> {
        let normalized = Self::normalize_jsdoc_block(jsdoc);
        let tag_pos = normalized.find("@typedef")?;
        let rest = normalized[tag_pos + "@typedef".len()..].trim();
        let (type_expr, name_rest) = Self::parse_jsdoc_braced_type_and_name(rest)?;
        let name = name_rest
            .split_whitespace()
            .next()
            .filter(|name| !name.is_empty())?;
        if type_expr.is_empty() {
            return None;
        }
        let name = name
            .find('<')
            .and_then(|generic_start| name[..generic_start].split_whitespace().next())
            .filter(|base| !base.is_empty())
            .unwrap_or(name);
        Some((name.to_string(), type_expr.to_string()))
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_braced_type_and_name(
        text: &str,
    ) -> Option<(&str, &str)> {
        let text = text.trim();
        if !text.starts_with('{') {
            return None;
        }

        let mut depth = 0usize;
        for (idx, ch) in text.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let ty = text[1..idx].trim();
                        let rest = text[idx + 1..].trim();
                        return Some((ty, rest));
                    }
                }
                _ => {}
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn jsdoc_description_lines(jsdoc: &str) -> Vec<String> {
        let mut lines = Vec::new();
        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            if line.starts_with('@') {
                break;
            }
            if !line.is_empty() {
                lines.push(line.to_string());
            }
        }
        lines
    }

    fn jsdoc_typedef_trailing_description_lines(jsdoc: &str) -> Vec<String> {
        let normalized = Self::normalize_jsdoc_block(jsdoc);
        let Some(tag_pos) = normalized.find("@typedef") else {
            return Vec::new();
        };
        let rest = normalized[tag_pos + "@typedef".len()..]
            .lines()
            .next()
            .unwrap_or("")
            .trim();
        let Some((_, name_rest)) = Self::parse_jsdoc_braced_type_and_name(rest) else {
            return Vec::new();
        };
        let name_rest = name_rest.trim();
        if name_rest.is_empty() {
            return Vec::new();
        }

        let name_end = name_rest
            .find(char::is_whitespace)
            .unwrap_or(name_rest.len());
        let description = name_rest[name_end..].trim();
        if description.is_empty() || description.starts_with('@') {
            Vec::new()
        } else {
            vec![description.to_string()]
        }
    }

    pub(in crate::declaration_emitter) fn jsdoc_has_property_tags(jsdoc: &str) -> bool {
        jsdoc.lines().any(|raw_line| {
            let line = raw_line.trim_start_matches('*').trim();
            line.starts_with("@property") || line.starts_with("@prop")
        })
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_property_type_alias(
        jsdoc: &str,
    ) -> Option<(String, String)> {
        let (name, base_type) = Self::parse_jsdoc_typedef_alias(jsdoc)
            .or_else(|| Self::parse_jsdoc_name_only_typedef_alias(jsdoc))?;
        if name == "default" || !matches!(base_type.as_str(), "Object" | "object") {
            return None;
        }

        let mut properties = Vec::new();
        let mut current_property: Option<(String, bool, String, Vec<String>)> = None;

        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            if line.is_empty() {
                continue;
            }

            if let Some(rest) = line
                .strip_prefix("@property")
                .or_else(|| line.strip_prefix("@prop"))
            {
                if let Some(property) = current_property.take() {
                    properties.push(property);
                }

                let rest = rest.trim();
                let (type_expr, name_rest) = Self::parse_jsdoc_braced_type_and_name(rest)?;
                let mut parts = name_rest.split_whitespace();
                let property_name = parts.next()?.trim();
                if property_name.is_empty() {
                    return None;
                }

                let (property_name, optional) =
                    if property_name.starts_with('[') && property_name.ends_with(']') {
                        let trimmed = property_name
                            .trim_start_matches('[')
                            .trim_end_matches(']')
                            .trim_end_matches('=')
                            .to_string();
                        (trimmed, true)
                    } else {
                        (property_name.to_string(), false)
                    };

                let inline_description = parts.collect::<Vec<_>>().join(" ");
                let mut description_lines = Vec::new();
                if !inline_description.is_empty() {
                    description_lines.push(inline_description);
                }

                current_property = Some((
                    property_name,
                    optional,
                    Self::normalize_jsdoc_primitive_type_name(type_expr),
                    description_lines,
                ));
                continue;
            }

            if line.starts_with('@') {
                if let Some(property) = current_property.take() {
                    properties.push(property);
                }
                continue;
            }

            if let Some((_, _, _, description_lines)) = current_property.as_mut() {
                description_lines.push(line.to_string());
            }
        }

        if let Some(property) = current_property.take() {
            properties.push(property);
        }
        if properties.is_empty() {
            return None;
        }

        let mut type_text = String::from("{\n");
        for (property_name, optional, property_type, description_lines) in properties {
            if !description_lines.is_empty() {
                type_text.push_str("    /**\n");
                for line in description_lines {
                    type_text.push_str("     * ");
                    type_text.push_str(&line);
                    type_text.push('\n');
                }
                type_text.push_str("     */\n");
            }
            type_text.push_str("    ");
            type_text.push_str(&Self::render_jsdoc_property_name(&property_name));
            if optional {
                type_text.push('?');
            }
            type_text.push_str(": ");
            type_text.push_str(&property_type);
            type_text.push_str(";\n");
        }
        type_text.push('}');

        Some((name, type_text))
    }

    fn parse_jsdoc_name_only_typedef_alias(jsdoc: &str) -> Option<(String, String)> {
        let normalized = Self::normalize_jsdoc_block(jsdoc);
        let tag_pos = normalized.find("@typedef")?;
        let rest = normalized[tag_pos + "@typedef".len()..].trim();
        if rest.starts_with('{') {
            return None;
        }
        let name = rest
            .split_whitespace()
            .next()
            .filter(|name| !name.is_empty())?;
        Some((name.to_string(), "Object".to_string()))
    }

    pub(in crate::declaration_emitter) fn normalize_jsdoc_primitive_type_name(
        type_name: &str,
    ) -> String {
        match type_name.trim() {
            "String" => "string".to_string(),
            "Number" => "number".to_string(),
            "Boolean" => "boolean".to_string(),
            "Symbol" => "symbol".to_string(),
            "BigInt" => "bigint".to_string(),
            "Undefined" => "undefined".to_string(),
            "Null" => "null".to_string(),
            "Object" => "object".to_string(),
            other => other.to_string(),
        }
    }

    fn render_jsdoc_property_name(name: &str) -> String {
        if Self::is_jsdoc_property_identifier_name(name) {
            return name.to_string();
        }
        if Self::is_quoted_jsdoc_property_name(name) {
            return name.to_string();
        }

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

    fn is_jsdoc_property_identifier_name(name: &str) -> bool {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        (first == '_' || first == '$' || first.is_ascii_alphabetic())
            && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    fn is_quoted_jsdoc_property_name(name: &str) -> bool {
        let mut chars = name.chars();
        let Some(quote @ ('"' | '\'')) = chars.next() else {
            return false;
        };
        name.ends_with(quote) && name.len() > quote.len_utf8()
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_type_alias_decl(
        jsdoc: &str,
    ) -> Option<JsdocTypeAliasDecl> {
        let type_params = Self::parse_jsdoc_template_params(jsdoc);
        let mut description_lines = Self::jsdoc_description_lines(jsdoc);
        description_lines.extend(Self::jsdoc_typedef_trailing_description_lines(jsdoc));

        if Self::jsdoc_has_property_tags(jsdoc) {
            let (name, type_text) = Self::parse_jsdoc_property_type_alias(jsdoc)?;
            if name == "default" {
                return None;
            }
            return Some(JsdocTypeAliasDecl {
                name,
                type_params,
                type_text,
                description_lines,
                render_verbatim: true,
            });
        }

        if let Some((name, type_text)) = Self::parse_jsdoc_typedef_alias(jsdoc) {
            if name == "default" {
                return None;
            }
            return Some(JsdocTypeAliasDecl {
                name,
                type_params,
                type_text,
                description_lines,
                render_verbatim: false,
            });
        }

        if let Some((name, type_text, description_lines)) =
            Self::parse_jsdoc_callback_alias_parts(jsdoc)
        {
            return Some(JsdocTypeAliasDecl {
                name,
                type_params,
                type_text,
                description_lines,
                render_verbatim: false,
            });
        }

        None
    }

    fn parse_jsdoc_default_typedef_alias_decl(
        jsdoc: &str,
        alias_name: &str,
    ) -> Option<JsdocTypeAliasDecl> {
        let type_params = Self::parse_jsdoc_template_params(jsdoc);
        let (name, type_text) = if Self::jsdoc_has_property_tags(jsdoc) {
            Self::parse_jsdoc_property_type_alias(jsdoc)?
        } else {
            Self::parse_jsdoc_typedef_alias(jsdoc)?
        };
        if name != "default" {
            return None;
        }

        Some(JsdocTypeAliasDecl {
            name: alias_name.to_string(),
            type_params,
            type_text,
            description_lines: Vec::new(),
            render_verbatim: Self::jsdoc_has_property_tags(jsdoc),
        })
    }

    #[cfg(test)]
    pub(in crate::declaration_emitter) fn render_jsdoc_type_alias_decl(
        decl: &JsdocTypeAliasDecl,
        exported: bool,
    ) -> Option<String> {
        Self::render_jsdoc_type_alias_decl_with_type_text(decl, exported, &decl.type_text)
    }

    fn render_jsdoc_type_alias_decl_with_type_text(
        decl: &JsdocTypeAliasDecl,
        exported: bool,
        type_text: &str,
    ) -> Option<String> {
        let mut source = String::new();
        if !decl.description_lines.is_empty() {
            source.push_str("/**\n");
            for line in &decl.description_lines {
                source.push_str(" * ");
                source.push_str(line);
                source.push('\n');
            }
            source.push_str(" */\n");
        }
        source.push_str(if exported { "export type " } else { "type " });
        source.push_str(&decl.name);
        if !decl.type_params.is_empty() {
            source.push('<');
            source.push_str(&decl.type_params.join(", "));
            source.push('>');
        }
        source.push_str(" = ");
        source.push_str(&Self::jsdoc_type_alias_parser_type_text(type_text));
        source.push_str(";\n");

        if decl.render_verbatim {
            return Some(source);
        }

        let mut parser = ParserState::new("jsdoc-alias.ts".to_string(), source);
        let root = parser.parse_source_file();
        let mut emitter = DeclarationEmitter::new(&parser.arena);
        emitter.normalize_string_literal_type_quotes = true;
        let mut rendered = emitter.emit(root);
        rendered = Self::compact_rendered_jsdoc_type_alias(&rendered);
        if !decl.type_params.is_empty() && decl.type_text.contains('\n') {
            let type_params = decl.type_params.join(", ");
            rendered = format!("/**\n * <{type_params}>\n */\n{rendered}");
        }
        if rendered.trim().is_empty() {
            None
        } else {
            Some(rendered)
        }
    }

    fn render_jsdoc_type_alias_decl_in_context(
        &self,
        decl: &JsdocTypeAliasDecl,
        exported: bool,
    ) -> Option<String> {
        let type_text = if decl.render_verbatim {
            // Pre-formatted type text (e.g. from @property tags) already has proper
            // TypeScript syntax with semicolons. Only apply portability rewrites; skip
            // the object-type reformatter which expects comma-separated members.
            let portable = self.rewrite_ambient_module_relative_import_type_text(&decl.type_text);
            self.rewrite_jsdoc_bare_module_import_type_text(&portable)
        } else {
            self.jsdoc_type_alias_text_for_declaration_emit(&decl.type_text)
        };
        Self::render_jsdoc_type_alias_decl_with_type_text(decl, exported, &type_text)
    }

    fn compact_rendered_jsdoc_type_alias(rendered: &str) -> String {
        let lines = rendered.lines().collect::<Vec<_>>();
        let mut output = String::new();
        let mut i = 0usize;
        while i < lines.len() {
            let line = lines[i];
            if line.trim_end().ends_with(": {")
                && i + 2 < lines.len()
                && lines[i + 1].trim_start().starts_with("[")
                && lines[i + 2].trim() == "};"
            {
                let prefix = line.trim_end().trim_end_matches('{').trim_end();
                output.push_str(prefix);
                output.push_str(" { ");
                output.push_str(lines[i + 1].trim());
                output.push_str(" };\n");
                i += 3;
                continue;
            }

            if line.trim() == "} & {"
                && i + 2 < lines.len()
                && lines[i + 1].trim_start().starts_with("[")
                && lines[i + 2].trim() == "};"
            {
                output.push_str("} & { ");
                output.push_str(lines[i + 1].trim());
                output.push_str(" };\n");
                i += 3;
                continue;
            }

            output.push_str(line);
            output.push('\n');
            i += 1;
        }
        output
    }

    fn jsdoc_type_alias_parser_type_text(type_text: &str) -> String {
        if !type_text.contains('\n') {
            return type_text.to_string();
        }

        let mut normalized = String::new();
        for raw_line in type_text.lines() {
            let line = raw_line.trim_end();
            let trimmed = line.trim();
            normalized.push_str(line);
            if Self::jsdoc_multiline_type_line_needs_separator(trimmed) {
                normalized.push(';');
            }
            normalized.push('\n');
        }
        normalized.trim_end().to_string()
    }

    fn jsdoc_multiline_type_line_needs_separator(line: &str) -> bool {
        if line.is_empty()
            || line.starts_with(':')
            || line.ends_with(';')
            || line.ends_with(',')
            || line.ends_with('{')
            || line.ends_with('(')
            || line.ends_with('&')
            || line.ends_with('|')
        {
            return false;
        }

        line.contains("?:") || line.ends_with(')')
    }

    pub(in crate::declaration_emitter) fn emit_rendered_jsdoc_type_alias(
        &mut self,
        decl: JsdocTypeAliasDecl,
        exported: bool,
    ) {
        if !self.emitted_jsdoc_type_aliases.insert(decl.name.clone()) {
            return;
        }
        let Some(rendered) = self.render_jsdoc_type_alias_decl_in_context(&decl, exported) else {
            return;
        };
        self.write(&rendered);
        if exported {
            self.emitted_module_indicator = true;
        }
    }

    pub(crate) fn emit_leading_jsdoc_type_aliases_for_pos(&mut self, pos: u32, exported: bool) {
        if !self.source_is_js_file {
            return;
        }
        if !self.js_export_equals_names.is_empty() {
            return;
        }
        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(pos) {
            if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                self.emit_rendered_jsdoc_type_alias(decl, exported);
            }
        }
    }

    pub(crate) fn emit_js_export_equals_type_alias_namespace_for_name(
        &mut self,
        name_idx: NodeIndex,
        pos: u32,
    ) {
        if !self.is_js_export_equals_name(name_idx) {
            return;
        }
        let aliases = self.jsdoc_type_alias_decls_before_pos(pos);
        if aliases.is_empty() {
            return;
        }

        self.write_indent();
        if self.should_emit_declare_keyword(false) {
            self.write("declare ");
        }
        self.write("namespace ");
        self.emit_node(name_idx);
        self.write(" {");
        self.write_line();
        self.increase_indent();
        self.write_indent();
        self.write("export { ");
        for (idx, alias) in aliases.iter().enumerate() {
            if idx > 0 {
                self.write(", ");
            }
            self.write(&alias.name);
        }
        self.write(" };");
        self.write_line();
        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn jsdoc_type_alias_decls_before_pos(
        &self,
        pos: u32,
    ) -> Vec<JsdocTypeAliasDecl> {
        if !self.source_is_js_file {
            return Vec::new();
        }
        let Some(text) = self.source_file_text.as_deref() else {
            return Vec::new();
        };
        self.all_comments
            .iter()
            .filter(|comment| comment.end <= pos)
            .filter(|comment| is_jsdoc_comment(comment, text))
            .map(|comment| get_jsdoc_content(comment, text))
            .filter_map(|jsdoc| Self::parse_jsdoc_type_alias_decl(&jsdoc))
            .collect()
    }

    pub(crate) fn emit_jsdoc_callback_type_aliases_for_variable_statement(
        &mut self,
        stmt_idx: NodeIndex,
        force_exported: bool,
    ) {
        if !self.source_is_js_file {
            return;
        }
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let (var_stmt, callback_pos) = if let Some(var_stmt) = self.arena.get_variable(stmt_node) {
            (var_stmt, stmt_node.pos)
        } else if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            let Some(export) = self.arena.get_export_decl(stmt_node) else {
                return;
            };
            let Some(export_clause_node) = self.arena.get(export.export_clause) else {
                return;
            };
            let Some(var_stmt) = self.arena.get_variable(export_clause_node) else {
                return;
            };
            (var_stmt, stmt_node.pos)
        } else {
            return;
        };

        let callback_chain = self.leading_jsdoc_comment_chain_for_pos(callback_pos);
        if callback_chain.is_empty() {
            return;
        }

        let callback_aliases = callback_chain
            .iter()
            .filter_map(|jsdoc| Self::parse_jsdoc_callback_alias(jsdoc))
            .collect::<FxHashMap<_, _>>();
        if callback_aliases.is_empty() {
            return;
        }

        let has_export_modifier = self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            if decl_list_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                continue;
            }
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                let is_exported = force_exported
                    || has_export_modifier
                    || self.is_js_named_exported_name(decl.name);
                if !is_exported {
                    continue;
                }

                let Some(type_name) = self
                    .jsdoc_name_like_type_expr_for_pos(callback_pos)
                    .or_else(|| self.jsdoc_name_like_type_expr_for_node(decl_idx))
                    .or_else(|| self.jsdoc_name_like_type_expr_for_node(decl.name))
                else {
                    continue;
                };

                let Some(type_text) = callback_aliases.get(&type_name) else {
                    continue;
                };
                if !self.emitted_jsdoc_type_aliases.insert(type_name.clone()) {
                    continue;
                }

                self.write_indent();
                self.write("export type ");
                self.write(&type_name);
                self.write(" = ");
                self.write(type_text);
                self.write(";");
                self.write_line();
            }
        }
    }

    pub(crate) fn emit_jsdoc_callback_type_aliases_for_object_literal_namespace(
        &mut self,
        initializer: NodeIndex,
        exported: bool,
    ) {
        if !self.source_is_js_file {
            return;
        }
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return;
        }
        let Some(object) = self.arena.get_literal_expr(init_node) else {
            return;
        };

        for &member_idx in &object.elements.nodes {
            for jsdoc in self.leading_jsdoc_comment_chain_for_node_or_ancestors(member_idx) {
                let Some((name, type_text, description_lines)) =
                    Self::parse_jsdoc_callback_alias_parts(&jsdoc)
                else {
                    continue;
                };
                self.emit_rendered_jsdoc_type_alias(
                    JsdocTypeAliasDecl {
                        name,
                        type_params: Vec::new(),
                        type_text,
                        description_lines,
                        render_verbatim: true,
                    },
                    exported,
                );
            }
        }
    }

    pub(crate) fn emit_pending_jsdoc_callback_type_aliases(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if !self.source_is_js_file {
            return;
        }

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            match stmt_node.kind {
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    self.emit_jsdoc_callback_type_aliases_for_variable_statement(stmt_idx, false);
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    let Some(export) = self.arena.get_export_decl(stmt_node) else {
                        continue;
                    };
                    let Some(clause_node) = self.arena.get(export.export_clause) else {
                        continue;
                    };
                    if clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                        self.emit_jsdoc_callback_type_aliases_for_variable_statement(
                            stmt_idx, true,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    pub(crate) fn emit_trailing_top_level_jsdoc_type_aliases(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if !self.source_is_js_file {
            return;
        }

        let Ok(eof_pos) = u32::try_from(source_file.text.len()) else {
            return;
        };

        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(eof_pos) {
            if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                self.emit_rendered_jsdoc_type_alias(decl, self.js_export_equals_names.is_empty());
            }
        }
    }

    pub(crate) fn emit_pending_top_level_jsdoc_type_aliases(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if !self.source_is_js_file {
            return;
        }
        let exported = self.source_file_has_module_syntax(source_file)
            && self.js_export_equals_names.is_empty();

        let mut decls = Vec::new();
        let mut variable_decls = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            for jsdoc in self.leading_jsdoc_comment_chain_for_pos(stmt_node.pos) {
                if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                    if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                        variable_decls.push(decl);
                    } else {
                        decls.push(decl);
                    }
                }
            }
        }

        let Ok(eof_pos) = u32::try_from(source_file.text.len()) else {
            return;
        };
        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(eof_pos) {
            if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                decls.push(decl);
            }
        }
        decls.extend(variable_decls);

        for decl in decls {
            self.emit_rendered_jsdoc_type_alias(decl, exported);
        }
    }

    pub(crate) fn emit_commonjs_named_export_top_level_jsdoc_type_aliases(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if !self.source_is_js_file
            || !self.js_export_equals_names.is_empty()
            || self.source_file_has_native_esm_syntax(source_file)
        {
            return;
        }
        let has_commonjs_named_exports = !self.js_named_export_names.is_empty()
            || source_file.statements.nodes.iter().any(|&stmt_idx| {
                self.js_anonymous_module_exports_named_members_initializer(stmt_idx)
                    .is_some()
                    || self
                        .js_module_exports_property_assignment(stmt_idx)
                        .is_some()
                    || self
                        .js_commonjs_named_export_for_statement(stmt_idx)
                        .is_some()
            });
        if !has_commonjs_named_exports {
            return;
        }

        let mut decls = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            for jsdoc in self.leading_jsdoc_comment_chain_for_pos(stmt_node.pos) {
                if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                    decls.push(decl);
                }
            }
        }

        let Ok(eof_pos) = u32::try_from(source_file.text.len()) else {
            return;
        };
        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(eof_pos) {
            if let Some(decl) = Self::parse_jsdoc_type_alias_decl(&jsdoc) {
                decls.push(decl);
            }
        }

        for decl in decls {
            self.emit_rendered_jsdoc_type_alias(decl, true);
        }
    }

    pub(crate) fn emit_jsdoc_default_typedef_aliases_for_js_default_export(
        &mut self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) {
        if !self.source_is_js_file || self.js_export_default_names.len() != 1 {
            return;
        }
        let Some(alias_name) = self.js_export_default_names.iter().next().cloned() else {
            return;
        };

        let exported = self.source_file_has_module_syntax(source_file)
            && self.js_export_equals_names.is_empty();

        let alias_can_share_declaration_name =
            self.js_default_typedef_alias_can_share_declaration_name(source_file, &alias_name);

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            for jsdoc in self.leading_jsdoc_comment_chain_for_pos(stmt_node.pos) {
                self.emit_jsdoc_default_typedef_alias_decl_for_comment(
                    &jsdoc,
                    &alias_name,
                    exported,
                    alias_can_share_declaration_name,
                );
            }
        }

        let Ok(eof_pos) = u32::try_from(source_file.text.len()) else {
            return;
        };
        for jsdoc in self.leading_jsdoc_comment_chain_for_pos(eof_pos) {
            self.emit_jsdoc_default_typedef_alias_decl_for_comment(
                &jsdoc,
                &alias_name,
                exported,
                alias_can_share_declaration_name,
            );
        }
    }

    pub(crate) fn emit_jsdoc_default_typedef_aliases_for_js_default_export_in_current_file(
        &mut self,
    ) {
        let source_file = self
            .current_source_file_idx
            .and_then(|root_idx| self.arena.get(root_idx))
            .and_then(|root_node| self.arena.get_source_file(root_node))
            .cloned();

        if let Some(source_file) = source_file {
            self.emit_jsdoc_default_typedef_aliases_for_js_default_export(&source_file);
        }
    }

    fn emit_jsdoc_default_typedef_alias_decl_for_comment(
        &mut self,
        jsdoc: &str,
        alias_name: &str,
        exported: bool,
        alias_can_share_declaration_name: bool,
    ) {
        let Some(mut decl) = Self::parse_jsdoc_default_typedef_alias_decl(jsdoc, alias_name) else {
            return;
        };

        if self.reserved_names.contains(&decl.name) && !alias_can_share_declaration_name {
            decl.name = self.generate_unique_name(&decl.name);
        }
        self.reserved_names.insert(decl.name.clone());

        self.emit_rendered_jsdoc_type_alias(decl, exported);
    }

    fn js_default_typedef_alias_can_share_declaration_name(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
        alias_name: &str,
    ) -> bool {
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            match stmt_node.kind {
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_DECLARATION =>
                {
                    if self.extract_declaration_name(stmt_idx).as_deref() == Some(alias_name) {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }
}
