use super::*;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn jsdoc_overload_signatures_for_node(
        &self,
        idx: NodeIndex,
    ) -> Vec<JsdocOverloadSignature> {
        self.leading_jsdoc_comment_chain_for_node_or_ancestors(idx)
            .into_iter()
            .flat_map(|comment| Self::parse_jsdoc_overload_signatures(&comment))
            .collect()
    }

    pub(in crate::declaration_emitter) fn jsdoc_overload_function_node_for_statement(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        if !self.source_is_js_file {
            return None;
        }

        let stmt_node = self.arena.get(stmt_idx)?;
        if stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            return Some(stmt_idx);
        }

        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            let export = self.arena.get_export_decl(stmt_node)?;
            let clause_node = self.arena.get(export.export_clause)?;
            if clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                return Some(export.export_clause);
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn hoisted_jsdoc_source_comment_is_multiline(
        &self,
        pos: u32,
    ) -> bool {
        let Some(text) = self.source_file_text.as_deref() else {
            return true;
        };
        let Some(comment) = self.all_comments.iter().rev().find(|comment| {
            comment.end <= pos
                && is_jsdoc_comment(comment, text)
                && text
                    .get(comment.end as usize..pos as usize)
                    .is_some_and(|between| between.trim().is_empty())
        }) else {
            return true;
        };
        text.get(comment.pos as usize..comment.end as usize)
            .is_none_or(|raw| raw.contains('\n'))
    }

    fn parse_jsdoc_overload_signatures(jsdoc: &str) -> Vec<JsdocOverloadSignature> {
        let lines = Self::normalized_jsdoc_lines(jsdoc);
        let overload_positions: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter_map(|(idx, line)| Self::jsdoc_tag_rest(line, "overload").map(|_| idx))
            .collect();
        let Some(&first_overload) = overload_positions.first() else {
            return Vec::new();
        };

        let global_type_params =
            Self::parse_jsdoc_template_params_from_lines(&lines[..first_overload]);

        overload_positions
            .iter()
            .enumerate()
            .filter_map(|(idx, &start)| {
                let end = overload_positions
                    .get(idx + 1)
                    .copied()
                    .unwrap_or(lines.len());
                let overload_rest = Self::jsdoc_tag_rest(&lines[start], "overload").unwrap_or("");
                Self::parse_jsdoc_overload_signature_segment(
                    jsdoc,
                    &global_type_params,
                    overload_rest,
                    &lines[start + 1..end],
                )
            })
            .collect()
    }

    fn parse_jsdoc_overload_signature_segment(
        jsdoc: &str,
        global_type_params: &[String],
        overload_rest: &str,
        lines: &[String],
    ) -> Option<JsdocOverloadSignature> {
        let mut type_params = global_type_params.to_vec();
        let mut params = Vec::new();
        let mut return_type = None;
        let mut seen_return = false;

        for line in lines {
            if let Some(rest) = Self::jsdoc_tag_rest(line, "template") {
                Self::push_jsdoc_type_params_unique(
                    &mut type_params,
                    &Self::parse_jsdoc_template_params(&format!("@template {rest}")),
                );
                continue;
            }

            if let Some(param) = Self::parse_jsdoc_param_decl(line) {
                if !seen_return {
                    params.push(Self::normalize_jsdoc_overload_param(param));
                }
                continue;
            }

            if let Some(parsed_return) = Self::parse_jsdoc_return_type_line(line) {
                if return_type.is_none() {
                    return_type = Some(Self::normalize_jsdoc_overload_type_text(&parsed_return));
                    seen_return = true;
                }
            }
        }

        if params.is_empty() && return_type.is_none() {
            return Self::parse_legacy_jsdoc_overload_signature(
                jsdoc,
                global_type_params,
                overload_rest,
                lines,
            );
        }

        Some(JsdocOverloadSignature {
            comment: jsdoc.to_string(),
            type_params,
            params,
            return_type: return_type.unwrap_or_else(|| "any".to_string()),
        })
    }

    fn parse_legacy_jsdoc_overload_signature(
        jsdoc: &str,
        global_type_params: &[String],
        overload_rest: &str,
        lines: &[String],
    ) -> Option<JsdocOverloadSignature> {
        if overload_rest.is_empty() {
            return None;
        }

        let simple_call = Self::jsdoc_overload_rest_has_simple_call_params(overload_rest);
        let params = if simple_call {
            lines
                .iter()
                .filter_map(|line| Self::parse_legacy_jsdoc_param_decl(line))
                .collect()
        } else {
            Vec::new()
        };

        Some(JsdocOverloadSignature {
            comment: jsdoc.to_string(),
            type_params: global_type_params.to_vec(),
            params,
            return_type: "any".to_string(),
        })
    }

    fn jsdoc_overload_rest_has_simple_call_params(rest: &str) -> bool {
        let Some(open_idx) = rest.find('(') else {
            return false;
        };
        let after_open = &rest[open_idx + 1..];
        let Some(close_idx) = Self::find_matching_paren(after_open) else {
            return false;
        };
        if !after_open[close_idx + 1..].trim().is_empty() {
            return false;
        }

        Self::split_jsdoc_params(&after_open[..close_idx])
            .into_iter()
            .all(|param| {
                let param = param.trim();
                param.is_empty()
                    || param.chars().enumerate().all(|(idx, ch)| {
                        ch == '_'
                            || ch == '$'
                            || (idx > 0 && ch.is_ascii_digit())
                            || ch.is_ascii_alphabetic()
                    })
            })
    }

    pub(in crate::declaration_emitter) fn parse_legacy_jsdoc_param_decl(
        line: &str,
    ) -> Option<JsdocParamDecl> {
        let rest = Self::jsdoc_tag_rest(line, "param")?;
        if rest.trim_start().starts_with('{') {
            return None;
        }
        let raw_name = rest
            .split_whitespace()
            .next()
            .filter(|name| !name.is_empty())?;
        let (name, optional) = Self::normalize_jsdoc_param_name(raw_name);
        if name.is_empty() || name.contains('.') {
            return None;
        }

        Some(JsdocParamDecl {
            name,
            type_text: "any".to_string(),
            optional,
            rest: false,
        })
    }

    fn normalized_jsdoc_lines(jsdoc: &str) -> Vec<String> {
        jsdoc
            .lines()
            .map(|line| line.trim_start_matches('*').trim().to_string())
            .collect()
    }

    fn jsdoc_tag_rest<'b>(line: &'b str, tag: &str) -> Option<&'b str> {
        let rest = line.strip_prefix('@')?.strip_prefix(tag)?;
        if rest
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
        {
            return None;
        }
        Some(rest.trim())
    }

    fn parse_jsdoc_template_params_from_lines(lines: &[String]) -> Vec<String> {
        let mut params = Vec::new();
        for line in lines {
            if let Some(rest) = Self::jsdoc_tag_rest(line, "template") {
                Self::push_jsdoc_type_params_unique(
                    &mut params,
                    &Self::parse_jsdoc_template_params(&format!("@template {rest}")),
                );
            }
        }
        params
    }

    fn push_jsdoc_type_params_unique(params: &mut Vec<String>, additions: &[String]) {
        for param in additions {
            let key = Self::jsdoc_template_param_name_key(param).to_string();
            if params
                .iter()
                .all(|existing| Self::jsdoc_template_param_name_key(existing) != key)
            {
                params.push(param.clone());
            }
        }
    }

    fn parse_jsdoc_return_type_line(line: &str) -> Option<String> {
        let rest = Self::jsdoc_tag_rest(line, "returns")
            .or_else(|| Self::jsdoc_tag_rest(line, "return"))?;
        let (type_expr, _) = Self::parse_jsdoc_braced_type_and_name(rest)?;
        let text = Self::normalize_jsdoc_type_text(type_expr, false);
        if Self::jsdoc_type_needs_checker_resolution(&text) {
            return Self::convert_jsdoc_function_type(&text);
        }
        Some(text)
    }

    fn normalize_jsdoc_overload_param(mut param: JsdocParamDecl) -> JsdocParamDecl {
        param.type_text = Self::normalize_jsdoc_overload_type_text(&param.type_text);
        param
    }

    fn normalize_jsdoc_overload_type_text(type_text: &str) -> String {
        Self::normalize_jsdoc_string_literal_quotes(type_text.trim())
    }

    fn normalize_jsdoc_string_literal_quotes(type_text: &str) -> String {
        let mut out = String::with_capacity(type_text.len());
        let mut chars = type_text.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch != '\'' {
                out.push(ch);
                continue;
            }

            let mut content = String::new();
            let mut escaped = false;
            let mut closed = false;
            for inner in chars.by_ref() {
                if escaped {
                    match inner {
                        '\'' => content.push('\''),
                        '\\' => content.push('\\'),
                        other => {
                            content.push('\\');
                            content.push(other);
                        }
                    }
                    escaped = false;
                } else if inner == '\\' {
                    escaped = true;
                } else if inner == '\'' {
                    closed = true;
                    break;
                } else {
                    content.push(inner);
                }
            }

            if closed {
                out.push('"');
                out.push_str(&escape_string_for_double_quote(&content));
                out.push('"');
            } else {
                out.push('\'');
                out.push_str(&content);
            }
        }

        out
    }

    pub(crate) fn jsdoc_param_decl_for_parameter(
        &self,
        param_idx: NodeIndex,
        position: usize,
    ) -> Option<JsdocParamDecl> {
        let jsdoc = self.function_like_jsdoc_for_node(param_idx)?;
        let params = Self::parse_jsdoc_param_decls(&jsdoc);
        if params.is_empty() {
            return None;
        }

        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        if let Some(name) = self.get_identifier_text(param.name)
            && let Some(found) = params.iter().find(|decl| decl.name == name)
        {
            return Some(found.clone());
        }

        params.into_iter().nth(position)
    }

    pub(crate) fn jsdoc_object_binding_param_type_literal(
        &self,
        param_idx: NodeIndex,
        position: usize,
    ) -> Option<String> {
        let jsdoc = self.function_like_jsdoc_for_node(param_idx)?;
        let params = Self::parse_jsdoc_param_decls(&jsdoc);
        if params.is_empty() {
            return None;
        }

        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        let pattern_node = self.arena.get(param.name)?;
        if pattern_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return None;
        }

        let root_decl = if let Some(name) = self.get_identifier_text(param.name) {
            params.iter().find(|decl| decl.name == name)
        } else {
            params.get(position)
        }?;
        if !matches!(root_decl.type_text.as_str(), "object" | "Object") {
            return None;
        }

        let pattern = self.arena.get_binding_pattern(pattern_node)?;
        let mut members = Vec::new();
        for &elem_idx in &pattern.elements.nodes {
            let elem_node = self.arena.get(elem_idx)?;
            let elem = self.arena.get_binding_element(elem_node)?;
            if elem.dot_dot_dot_token {
                return None;
            }

            let prop_name_idx = if elem.property_name.is_some() {
                elem.property_name
            } else {
                elem.name
            };
            let prop_node = self.arena.get(prop_name_idx)?;
            if prop_node.kind != SyntaxKind::Identifier as u16 {
                return None;
            }
            let prop_name = self.arena.get_identifier(prop_node)?.escaped_text.as_str();
            let qualified_name = format!("{}.{}", root_decl.name, prop_name);
            let prop_decl = params.iter().find(|decl| decl.name == qualified_name)?;

            let mut member = String::new();
            member.push_str(prop_name);
            if prop_decl.optional {
                member.push('?');
            }
            member.push_str(": ");
            let type_text = if matches!(prop_decl.type_text.as_str(), "object" | "Object") {
                self.jsdoc_object_param_nested_type_literal(&params, &qualified_name, 2)
                    .unwrap_or_else(|| prop_decl.type_text.clone())
            } else {
                prop_decl.type_text.clone()
            };
            member.push_str(&type_text);
            if prop_decl.optional && !Self::type_text_has_undefined_branch(&prop_decl.type_text) {
                member.push_str(" | undefined");
            }
            member.push(';');
            members.push(member);
        }

        let member_indent = "    ".repeat((self.indent_level + 1) as usize);
        let closing_indent = "    ".repeat(self.indent_level as usize);
        let lines: Vec<String> = members
            .into_iter()
            .map(|member| format!("{member_indent}{member}"))
            .collect();
        (!lines.is_empty()).then(|| format!("{{\n{}\n{closing_indent}}}", lines.join("\n")))
    }

    fn jsdoc_object_param_nested_type_literal(
        &self,
        params: &[JsdocParamDecl],
        object_name: &str,
        depth: u32,
    ) -> Option<String> {
        let prefix = format!("{object_name}.");
        let mut members = Vec::new();
        for prop_decl in params.iter().filter(|decl| decl.name.starts_with(&prefix)) {
            let rest = &prop_decl.name[prefix.len()..];
            if rest.is_empty() || rest.contains('.') {
                continue;
            }

            let mut member = String::new();
            member.push_str(rest);
            if prop_decl.optional {
                member.push('?');
            }
            member.push_str(": ");
            let type_text = if matches!(prop_decl.type_text.as_str(), "object" | "Object") {
                self.jsdoc_object_param_nested_type_literal(params, &prop_decl.name, depth + 1)
                    .unwrap_or_else(|| prop_decl.type_text.clone())
            } else {
                prop_decl.type_text.clone()
            };
            member.push_str(&type_text);
            if prop_decl.optional && !Self::type_text_has_undefined_branch(&prop_decl.type_text) {
                member.push_str(" | undefined");
            }
            member.push(';');
            members.push(member);
        }
        if members.is_empty() {
            return None;
        }

        let member_indent = "    ".repeat((self.indent_level + depth) as usize);
        let closing_indent = "    ".repeat((self.indent_level + depth - 1) as usize);
        let lines: Vec<String> = members
            .into_iter()
            .map(|member| format!("{member_indent}{member}"))
            .collect();
        Some(format!("{{\n{}\n{closing_indent}}}", lines.join("\n")))
    }

    pub(crate) fn jsdoc_satisfies_param_decl_for_parameter(
        &self,
        param_idx: NodeIndex,
        position: usize,
    ) -> Option<JsdocParamDecl> {
        let jsdoc = self.function_like_jsdoc_for_node(param_idx)?;
        let params = Self::parse_jsdoc_satisfies_param_decls(&jsdoc);
        if params.is_empty() {
            return None;
        }

        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        let source_is_rest = param.dot_dot_dot_token;

        if let Some(name) = self.get_identifier_text(param.name)
            && let Some(found) = params.iter().find(|decl| decl.name == name)
        {
            return Some(Self::adapt_jsdoc_satisfies_param_decl(
                found,
                source_is_rest,
            ));
        }

        let mut next_position = 0usize;
        let mut rest_decl = None;
        for decl in &params {
            if decl.rest {
                rest_decl = Some(decl);
                continue;
            }
            if next_position == position {
                return Some(Self::adapt_jsdoc_satisfies_param_decl(decl, source_is_rest));
            }
            next_position += 1;
        }

        rest_decl.map(|decl| Self::adapt_jsdoc_satisfies_param_decl(decl, source_is_rest))
    }

    fn adapt_jsdoc_satisfies_param_decl(
        decl: &JsdocParamDecl,
        source_is_rest: bool,
    ) -> JsdocParamDecl {
        let mut adapted = decl.clone();
        if source_is_rest {
            return adapted;
        }

        adapted.rest = false;
        if decl.rest {
            adapted.optional = false;
            if let Some(element_type) = adapted.type_text.strip_suffix("[]") {
                adapted.type_text = element_type.trim().to_string();
            }
        }
        adapted
    }

    pub(in crate::declaration_emitter) fn parse_jsdoc_satisfies_param_decls(
        jsdoc: &str,
    ) -> Vec<JsdocParamDecl> {
        let Some(type_expr) = Self::extract_jsdoc_satisfies_expression(jsdoc) else {
            return Vec::new();
        };
        Self::parse_function_type_param_decls(type_expr)
    }

    fn extract_jsdoc_satisfies_expression(jsdoc: &str) -> Option<&str> {
        let tag_pos = Self::jsdoc_tag_offset(jsdoc, "satisfies")?;
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

    fn parse_function_type_param_decls(type_expr: &str) -> Vec<JsdocParamDecl> {
        let type_expr = type_expr.trim();
        let Some(params_text) = Self::function_type_params_text(type_expr) else {
            return Vec::new();
        };
        Self::split_jsdoc_params(params_text)
            .into_iter()
            .enumerate()
            .filter_map(|(index, raw)| Self::parse_function_type_param_decl(raw, index))
            .filter(|decl| decl.name != "this")
            .collect()
    }

    fn function_type_params_text(type_expr: &str) -> Option<&str> {
        if let Some(rest) = type_expr.strip_prefix("function") {
            let rest = rest.trim_start();
            let rest = rest.strip_prefix('(')?;
            let close_idx = Self::find_matching_paren(rest)?;
            return Some(&rest[..close_idx]);
        }

        let rest = type_expr.strip_prefix('(')?;
        let close_idx = Self::find_matching_paren(rest)?;
        let after_close = rest[close_idx + 1..].trim_start();
        if !after_close.starts_with("=>") {
            return None;
        }
        Some(&rest[..close_idx])
    }

    fn find_matching_paren(text: &str) -> Option<usize> {
        let mut depth = 1usize;
        let mut quote: Option<char> = None;
        let mut escaped = false;
        for (i, ch) in text.char_indices() {
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
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn parse_function_type_param_decl(raw: &str, index: usize) -> Option<JsdocParamDecl> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }

        let (rest_param, raw) = if let Some(stripped) = raw.strip_prefix("...") {
            (true, stripped.trim())
        } else {
            (false, raw)
        };

        let (name, optional, type_expr) = if let Some(colon_idx) = Self::find_top_level_colon(raw) {
            let raw_name = raw[..colon_idx].trim();
            let raw_type = raw[colon_idx + 1..].trim();
            let optional = raw_name.ends_with('?');
            let name = raw_name.trim_end_matches('?').trim();
            let name = if name.is_empty() {
                format!("arg{index}")
            } else {
                name.to_string()
            };
            (name, optional, raw_type)
        } else {
            (format!("arg{index}"), false, raw)
        };

        Some(JsdocParamDecl {
            name,
            type_text: Self::normalize_jsdoc_type_text(type_expr, rest_param),
            optional,
            rest: rest_param,
        })
    }

    fn find_top_level_colon(text: &str) -> Option<usize> {
        let mut depth = 0usize;
        let mut quote: Option<char> = None;
        let mut escaped = false;
        for (i, ch) in text.char_indices() {
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
                '(' | '<' | '{' | '[' => depth += 1,
                ')' | '>' | '}' | ']' => depth = depth.saturating_sub(1),
                ':' if depth == 0 => return Some(i),
                _ => {}
            }
        }
        None
    }

    pub(crate) fn parse_jsdoc_return_type_text(jsdoc: &str) -> Option<String> {
        for raw_line in jsdoc.lines() {
            let line = raw_line.trim().trim_start_matches('*').trim();
            let Some(rest) = line
                .strip_prefix("@returns")
                .or_else(|| line.strip_prefix("@return"))
            else {
                continue;
            };
            let rest = rest.trim();
            let (type_expr, _) = Self::parse_jsdoc_braced_type_and_name(rest)?;
            let text = Self::normalize_jsdoc_type_text(type_expr, false);
            if Self::jsdoc_type_needs_checker_resolution(&text) {
                return Self::convert_jsdoc_function_type(&text);
            }
            return Some(text);
        }
        None
    }

    pub(crate) fn parse_jsdoc_type_text(jsdoc: &str) -> Option<String> {
        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            let Some(rest) = line.strip_prefix("@type") else {
                continue;
            };
            if rest
                .chars()
                .next()
                .is_some_and(Self::is_jsdoc_tag_name_continuation)
            {
                continue;
            }
            if rest.starts_with("def") {
                continue;
            }
            let rest = rest.trim();
            let (type_expr, _) = Self::parse_jsdoc_braced_type_and_name(rest)?;
            let text = if type_expr.trim() == "?" {
                "unknown".to_string()
            } else {
                Self::normalize_jsdoc_type_text(type_expr, false)
            };
            if Self::jsdoc_type_needs_checker_resolution(&text) {
                return Self::convert_jsdoc_function_type(&text);
            }
            return Some(text);
        }
        None
    }

    pub(crate) fn jsdoc_return_type_text_for_node(&self, idx: NodeIndex) -> Option<String> {
        let jsdoc = self.function_like_jsdoc_for_node(idx)?;
        Self::parse_jsdoc_return_type_text(&jsdoc)
    }

    pub(crate) fn jsdoc_type_text_for_node(&self, idx: NodeIndex) -> Option<String> {
        let jsdoc = self.function_like_jsdoc_for_node(idx)?;
        let type_text = Self::parse_jsdoc_type_text(&jsdoc)?;
        self.local_semicolon_class_member_typedef_type_text(idx, &type_text)
            .or(Some(type_text))
    }

    fn local_semicolon_class_member_typedef_type_text(
        &self,
        idx: NodeIndex,
        type_text: &str,
    ) -> Option<String> {
        let node = self.arena.get(idx)?;
        if self.arena.get_property_decl(node).is_none()
            || !Self::is_simple_jsdoc_type_name(type_text)
        {
            return None;
        }

        let text = self.source_file_text.as_deref()?;
        let mut cursor = node.pos as usize;
        while cursor < text.len() && matches!(text.as_bytes()[cursor], b' ' | b'\t' | b'\r' | b'\n')
        {
            cursor += 1;
        }

        for comment in self
            .all_comments
            .iter()
            .filter(|comment| comment.end as usize <= cursor)
            .filter(|comment| is_jsdoc_comment(comment, text))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            let between = &text[comment.end as usize..cursor];
            if !between
                .bytes()
                .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b';'))
            {
                break;
            }

            let jsdoc = get_jsdoc_content(comment, text);
            if let Some((name, base_type)) = Self::parse_jsdoc_typedef_alias(&jsdoc)
                && name == type_text
            {
                return Some(Self::normalize_jsdoc_type_text(&base_type, false));
            }
            cursor = comment.pos as usize;
        }

        None
    }

    fn is_simple_jsdoc_type_name(type_text: &str) -> bool {
        let mut chars = type_text.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        (first == '_' || first == '$' || first.is_ascii_alphabetic())
            && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    pub(crate) fn jsdoc_function_type_signature_for_node(
        &self,
        idx: NodeIndex,
    ) -> Option<JsdocFunctionTypeSignature> {
        let jsdoc = self.function_like_jsdoc_for_node(idx)?;
        let type_name = Self::parse_jsdoc_type_text(&jsdoc)?;
        if !Self::is_simple_jsdoc_type_name(&type_name) {
            return None;
        }

        for comment in self.leading_jsdoc_comment_chain_for_node_or_ancestors(idx) {
            let Some((name, type_text)) = Self::parse_jsdoc_typedef_alias(&comment) else {
                continue;
            };
            if name != type_name {
                continue;
            }
            if let Some(signature) = parse_jsdoc_function_type_signature(&type_text) {
                return Some(signature);
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn statement_jsdoc_type_function_signature_node(
        &self,
        stmt_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let stmt_node = self.arena.get(stmt_idx)?;
        let func_idx = if stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            stmt_idx
        } else if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            let export = self.arena.get_export_decl(stmt_node)?;
            let clause_node = self.arena.get(export.export_clause)?;
            (clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION)
                .then_some(export.export_clause)?
        } else {
            return None;
        };
        self.jsdoc_function_type_signature_for_node(func_idx)
            .map(|_| func_idx)
    }

    pub(crate) fn emit_jsdoc_function_type_signature(
        &mut self,
        type_params: &[String],
        params: &[(String, String)],
        return_type: &str,
    ) {
        self.emit_jsdoc_template_parameters(type_params);
        self.write("(");
        for (idx, (name, type_text)) in params.iter().enumerate() {
            if idx > 0 {
                self.write(", ");
            }
            self.write(name);
            self.write(": ");
            self.write(type_text);
        }
        self.write("): ");
        self.write(return_type);
    }

    pub(in crate::declaration_emitter) fn emit_jsdoc_overload_function_signatures(
        &mut self,
        func_idx: NodeIndex,
        is_exported: bool,
        emit_export_keyword: bool,
        signatures: &[JsdocOverloadSignature],
    ) -> bool {
        if signatures.is_empty() {
            return false;
        }

        let Some(func_node) = self.arena.get(func_idx) else {
            return false;
        };
        let Some(func) = self.arena.get_function(func_node) else {
            return false;
        };

        for signature in signatures {
            self.emit_jsdoc_comment_chain(std::slice::from_ref(&signature.comment));
            self.write_indent();
            if emit_export_keyword {
                self.write("export ");
            }
            if self.should_emit_declare_keyword(is_exported) {
                self.write("declare ");
            }
            self.write("function ");
            self.emit_node(func.name);
            self.emit_jsdoc_overload_signature(signature);
            self.write(";");
            self.write_line();
        }

        self.skip_comments_in_node(func_node.pos, func_node.end);
        true
    }

    fn emit_jsdoc_overload_signature(&mut self, signature: &JsdocOverloadSignature) {
        self.emit_jsdoc_template_parameters(&signature.type_params);
        self.write("(");
        self.emit_jsdoc_overload_parameters(signature);
        self.write("): ");
        self.write(&signature.return_type);
    }

    pub(in crate::declaration_emitter) fn emit_jsdoc_overload_method_signatures(
        &mut self,
        method_idx: NodeIndex,
        signatures: &[JsdocOverloadSignature],
    ) -> bool {
        if signatures.is_empty() {
            return false;
        }

        let Some(method_node) = self.arena.get(method_idx) else {
            return false;
        };
        let Some(method) = self.arena.get_method_decl(method_node) else {
            return false;
        };

        for signature in signatures {
            self.emit_jsdoc_comment_chain(std::slice::from_ref(&signature.comment));
            self.write_indent();
            self.emit_member_modifiers(&method.modifiers);
            self.emit_node(method.name);
            if method.question_token {
                self.write("?");
            }
            self.emit_jsdoc_overload_signature(signature);
            self.write(";");
            self.write_line();
        }

        self.skip_comments_in_node(method_node.pos, method_node.end);
        true
    }

    pub(in crate::declaration_emitter) fn emit_jsdoc_overload_constructor_signatures(
        &mut self,
        ctor_idx: NodeIndex,
        signatures: &[JsdocOverloadSignature],
    ) -> bool {
        if signatures.is_empty() {
            return false;
        }

        let Some(ctor_node) = self.arena.get(ctor_idx) else {
            return false;
        };
        let Some(ctor) = self.arena.get_constructor(ctor_node) else {
            return false;
        };

        for signature in signatures {
            self.emit_jsdoc_comment_chain(std::slice::from_ref(&signature.comment));
            self.write_indent();
            if let Some(ref mods) = ctor.modifiers {
                for &mod_idx in &mods.nodes {
                    if let Some(mod_node) = self.arena.get(mod_idx) {
                        match mod_node.kind {
                            k if k == SyntaxKind::PrivateKeyword as u16 => self.write("private "),
                            k if k == SyntaxKind::ProtectedKeyword as u16 => {
                                self.write("protected ");
                            }
                            _ => {}
                        }
                    }
                }
            }
            self.write("constructor(");
            self.emit_jsdoc_overload_parameters(signature);
            self.write(");");
            self.write_line();
        }

        self.skip_comments_in_node(ctor_node.pos, ctor_node.end);
        true
    }

    pub(in crate::declaration_emitter) fn emit_jsdoc_overload_namespace_function_signatures(
        &mut self,
        name_idx: NodeIndex,
        overload_source_idx: NodeIndex,
        signatures: &[JsdocOverloadSignature],
    ) -> bool {
        if signatures.is_empty() {
            return false;
        }

        let Some(export_name) = self.get_identifier_text(name_idx) else {
            return false;
        };
        let export_alias = if export_name == "constructor" && signatures.len() > 1 {
            let local_name = self.generate_unique_name(&export_name);
            self.reserved_names.insert(local_name.clone());
            Some((export_name.clone(), local_name))
        } else {
            None
        };
        let emitted_name = export_alias
            .as_ref()
            .map_or(export_name.as_str(), |(_, local_name)| local_name.as_str());

        let overload_source_pos = self
            .arena
            .get(overload_source_idx)
            .map_or(0, |node| node.pos);

        for signature in signatures {
            if !self.emit_jsdoc_comment_verbatim_for_pos(overload_source_pos, &signature.comment) {
                self.emit_jsdoc_comment_chain(std::slice::from_ref(&signature.comment));
            }
            self.write_indent();
            if export_alias.is_some() {
                self.write("export ");
            }
            self.write("function ");
            self.write(emitted_name);
            self.emit_jsdoc_overload_signature(signature);
            self.write(";");
            self.write_line();
        }

        if let Some((export_name, local_name)) = export_alias {
            self.write_indent();
            self.write("export { ");
            self.write(&local_name);
            self.write(" as ");
            self.write(&export_name);
            self.write(" };");
            self.write_line();
        }

        if let Some(node) = self.arena.get(overload_source_idx) {
            self.skip_comments_in_node(node.pos, node.end);
        }
        true
    }

    fn emit_jsdoc_overload_parameters(&mut self, signature: &JsdocOverloadSignature) {
        for (idx, param) in signature.params.iter().enumerate() {
            if idx > 0 {
                self.write(", ");
            }
            if param.rest {
                self.write("...");
            }
            self.write(&param.name);
            if param.optional && !param.rest {
                self.write("?");
            }
            self.write(": ");
            self.write(&param.type_text);
        }
    }

    pub(crate) fn jsdoc_template_params_for_node(&self, idx: NodeIndex) -> Vec<String> {
        self.function_like_jsdoc_for_node(idx)
            .map(|jsdoc| Self::parse_jsdoc_template_params(&jsdoc))
            .unwrap_or_default()
    }

    pub(in crate::declaration_emitter) fn jsdoc_template_params_for_class_declaration(
        &self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> Vec<String> {
        let mut params = Vec::new();
        let mut seen = FxHashSet::default();
        for jsdoc in self.jsdoc_chain_for_class_declaration(class_idx, class) {
            for param in Self::parse_jsdoc_template_params(&jsdoc) {
                let key = Self::jsdoc_template_param_name_key(&param).to_string();
                if seen.insert(key) {
                    params.push(param);
                }
            }
        }
        params
    }

    pub(in crate::declaration_emitter) fn jsdoc_template_param_name(param: &str) -> &str {
        Self::jsdoc_template_param_name_key(param)
    }

    pub(in crate::declaration_emitter) fn jsdoc_extends_type_for_class_declaration(
        &self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> Option<String> {
        for jsdoc in self.jsdoc_chain_for_class_declaration(class_idx, class) {
            for raw_line in jsdoc.lines() {
                let line = raw_line.trim().trim_start_matches('*').trim();
                let rest = line
                    .strip_prefix("@extends")
                    .or_else(|| line.strip_prefix("@augments"));
                let Some(rest) = rest else {
                    continue;
                };
                let rest = Self::trim_jsdoc_same_line_following_tags(rest.trim_start());
                let Some((type_expr, _)) = Self::parse_jsdoc_braced_type_and_name(rest) else {
                    continue;
                };
                let type_text = Self::normalize_jsdoc_type_text(type_expr, false);
                return Some(self.jsdoc_type_text_for_declaration_emit(&type_text));
            }
        }
        None
    }

    fn jsdoc_chain_for_class_declaration(
        &self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> Vec<String> {
        let Some(class_node) = self.arena.get(class_idx) else {
            return Vec::new();
        };
        let jsdoc_template_anchor = class
            .modifiers
            .as_ref()
            .and_then(|mods| mods.nodes.first().copied())
            .and_then(|mod_idx| self.arena.get(mod_idx))
            .map(|mod_node| mod_node.pos)
            .unwrap_or(class_node.pos);

        let mut chain = self.current_statement_jsdoc_chain.clone();
        if chain.is_empty() {
            chain = self.leading_jsdoc_comment_chain_for_pos(jsdoc_template_anchor);
        }
        if chain.is_empty() {
            chain = self.leading_jsdoc_comment_chain_for_pos(class_node.pos);
        }
        if chain.is_empty() {
            if let Some(name_node) = self.arena.get(class.name) {
                chain = self.leading_jsdoc_comment_chain_for_pos(name_node.pos);
            }
        }
        if chain.is_empty()
            && let Some(jsdoc) = self.function_like_jsdoc_for_node(class_idx)
        {
            chain.push(jsdoc);
        }
        if chain.is_empty()
            && let Some(jsdoc) = self.function_like_jsdoc_for_node(class.name)
        {
            chain.push(jsdoc);
        }
        chain
    }

    pub(crate) fn jsdoc_has_readonly_for_node(&self, idx: NodeIndex) -> bool {
        self.function_like_jsdoc_for_node(idx)
            .as_deref()
            .is_some_and(|jsdoc| {
                jsdoc.lines().any(|raw_line| {
                    let line = raw_line.trim_start_matches('*').trim();
                    line == "@readonly" || line.starts_with("@readonly ")
                })
            })
    }

    pub(crate) fn jsdoc_has_protected_for_node(&self, idx: NodeIndex) -> bool {
        self.function_like_jsdoc_for_node(idx)
            .as_deref()
            .is_some_and(|jsdoc| {
                jsdoc.lines().any(|raw_line| {
                    let line = raw_line.trim_start_matches('*').trim();
                    line == "@protected" || line.starts_with("@protected ")
                })
            })
    }

    pub(crate) fn emit_jsdoc_template_parameters(&mut self, type_params: &[String]) {
        if type_params.is_empty() {
            return;
        }

        self.write("<");
        for (i, param) in type_params.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.write(param);
        }
        self.write(">");
    }

    pub(in crate::declaration_emitter) fn jsdoc_has_function_signature_tags(jsdoc: &str) -> bool {
        jsdoc.lines().any(|raw_line| {
            let line = raw_line.trim().trim_start_matches('*').trim();
            line.starts_with("@param")
                || line.starts_with("@returns")
                || line.starts_with("@return")
                || line.starts_with("@template")
        })
    }

    pub(in crate::declaration_emitter) fn jsdoc_has_satisfies_tag(jsdoc: &str) -> bool {
        Self::jsdoc_tag_offset(jsdoc, "satisfies").is_some()
    }

    fn jsdoc_tag_offset(jsdoc: &str, tag_name: &str) -> Option<usize> {
        let needle = format!("@{tag_name}");
        for (pos, _) in jsdoc.match_indices(&needle) {
            let after = pos + needle.len();
            if after >= jsdoc.len() {
                return Some(pos);
            }
            let next_ch = jsdoc[after..]
                .chars()
                .next()
                .expect("after < jsdoc.len() checked above");
            if !Self::is_jsdoc_tag_name_continuation(next_ch) {
                return Some(pos);
            }
        }
        None
    }

    fn jsdoc_contains_type_tag(jsdoc: &str) -> bool {
        jsdoc.lines().any(|raw_line| {
            let line = raw_line.trim_start_matches('*').trim();
            line.strip_prefix("@type").is_some_and(|rest| {
                !rest
                    .chars()
                    .next()
                    .is_some_and(Self::is_jsdoc_tag_name_continuation)
                    && !rest.trim_start().starts_with("def")
            })
        })
    }

    pub(in crate::declaration_emitter) fn jsdoc_contains_type_alias_tag(jsdoc: &str) -> bool {
        Self::jsdoc_has_property_tags(jsdoc) || Self::parse_jsdoc_typedef_alias(jsdoc).is_some()
    }

    pub(in crate::declaration_emitter) fn jsdoc_chain_without_type_or_alias_tags(
        chain: &[String],
    ) -> Vec<String> {
        chain
            .iter()
            .filter(|jsdoc| {
                !Self::jsdoc_contains_type_tag(jsdoc) && !Self::jsdoc_contains_type_alias_tag(jsdoc)
            })
            .cloned()
            .collect()
    }

    pub(crate) fn emit_js_function_variable_declaration_if_possible(
        &mut self,
        decl_idx: NodeIndex,
        decl_name: NodeIndex,
        initializer: NodeIndex,
        is_exported: bool,
    ) -> bool {
        if !self.source_is_js_file || !initializer.is_some() {
            return false;
        }

        let Some(name_node) = self.arena.get(decl_name) else {
            return false;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        if self
            .leading_jsdoc_type_expr_for_pos(name_node.pos)
            .is_some()
        {
            return false;
        }
        let is_export_equals_root = self.is_js_export_equals_name(decl_name);

        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && init_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return false;
        }
        let Some(func) = self.arena.get_function(init_node) else {
            return false;
        };

        let jsdoc = self.function_like_jsdoc_for_node(initializer);
        // In JS files, tsc always converts `const x = (arrow) => ...` or
        // `const x = function(...) {}` to `function x(...)` in declarations,
        // regardless of whether JSDoc @param/@returns tags are present.
        // Only bail out for non-export-equals when there are no JSDoc tags
        // AND no attached JSDoc comment at all (so we don't lose doc comments).
        let has_jsdoc_tags = jsdoc
            .as_deref()
            .is_some_and(Self::jsdoc_has_function_signature_tags);
        let has_any_jsdoc = jsdoc.is_some();
        if !has_jsdoc_tags
            && !is_export_equals_root
            && !has_any_jsdoc
            && !is_exported
            && !self.emitting_js_default_export_declaration
        {
            return false;
        }

        if self
            .current_statement_jsdoc_chain
            .iter()
            .any(|jsdoc| Self::jsdoc_has_satisfies_tag(jsdoc))
        {
            self.suppress_current_statement_jsdoc_comments = true;
        }

        self.emit_pending_js_export_equals_for_name(decl_name);
        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("function ");
        self.emit_node(decl_name);

        let jsdoc_template_params = if func
            .type_parameters
            .as_ref()
            .is_none_or(|type_params| type_params.nodes.is_empty())
        {
            jsdoc
                .as_deref()
                .map(Self::parse_jsdoc_template_params)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if let Some(ref type_params) = func.type_parameters {
            if !type_params.nodes.is_empty() {
                self.emit_type_parameters(type_params);
            } else if !jsdoc_template_params.is_empty() {
                self.emit_jsdoc_template_parameters(&jsdoc_template_params);
            }
        } else if !jsdoc_template_params.is_empty() {
            self.emit_jsdoc_template_parameters(&jsdoc_template_params);
        }

        self.write("(");
        self.use_jsdoc_satisfies_parameter_fallback = true;
        self.emit_parameters_with_body(&func.parameters, func.body);
        self.use_jsdoc_satisfies_parameter_fallback = false;
        self.write(")");

        if func.type_annotation.is_some() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        } else if let Some(return_type_text) = jsdoc
            .as_deref()
            .and_then(Self::parse_jsdoc_return_type_text)
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let Some(return_type_text) = self
            .js_function_body_preferred_return_text_for_declaration(
                func.body,
                decl_name,
                &func.parameters,
            )
        {
            self.write(": ");
            self.write(&return_type_text);
        } else if let (Some(interner), Some(cache)) = (&self.type_interner, &self.type_cache) {
            let func_type_id = cache
                .node_types
                .get(&initializer.0)
                .copied()
                .or_else(|| self.get_node_type_or_names(&[decl_idx, decl_name, initializer]));
            if let Some(func_type_id) = func_type_id {
                if let Some(predicate_text) =
                    self.function_type_predicate_text(func_type_id, func.type_parameters.as_ref())
                {
                    self.write(": ");
                    self.write(&predicate_text);
                } else if let Some(return_type_id) =
                    tsz_solver::type_queries::get_return_type(*interner, func_type_id)
                {
                    if return_type_id == tsz_solver::types::TypeId::ANY
                        && func.body.is_some()
                        && self.body_returns_void(func.body)
                    {
                        self.write(": void");
                    } else {
                        self.write(": ");
                        self.write(&self.print_type_id(return_type_id));
                    }
                }
            } else if func.body.is_some() && self.body_returns_void(func.body) {
                self.write(": void");
            }
        } else if func.body.is_some() && self.body_returns_void(func.body) {
            self.write(": void");
        }

        self.write(";");
        self.write_line();
        self.emit_js_function_computed_binding_key_declarations(&func.parameters);
        self.emit_js_function_like_class_if_needed(
            decl_name,
            &func.parameters,
            func.body,
            is_exported,
            initializer,
        );
        self.emit_js_class_static_members_namespace(decl_name, is_exported);
        self.emit_js_namespace_export_aliases_for_name(decl_name, is_exported);
        true
    }
}
