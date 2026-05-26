use super::super::Printer;
use tsz_parser::parser::NodeList;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_recovered_malformed_arrow_block_after_variable_statement(
        &mut self,
        node: &Node,
        recovered_async_arrow_return: bool,
    ) {
        let Some(text) = self.source_text else {
            return;
        };
        let bytes = text.as_bytes();
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        if start >= bytes.len() {
            return;
        }

        let mut line_end = start;
        while line_end < bytes.len() && bytes[line_end] != b'\n' && bytes[line_end] != b'\r' {
            line_end += 1;
        }

        let Ok(line) = std::str::from_utf8(&bytes[start..line_end]) else {
            return;
        };
        let masked_line = Self::source_text_with_quoted_spans_masked(line);
        let line_for_scan = masked_line.as_str();

        if self.ctx.flags.in_class_static_block
            && Self::line_has_static_block_await_arrow_recovery(line_for_scan)
        {
            let Some(arrow_rel) = line_for_scan.find("=>") else {
                return;
            };
            let after_arrow = start + arrow_rel + 2;
            let Some(open_rel) = bytes[after_arrow..line_end].iter().position(|&b| b == b'{')
            else {
                return;
            };
            let open = after_arrow + open_rel;
            let mut pos = open + 1;
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if bytes.get(pos) == Some(&b'}') {
                self.write_line();
                self.write("{ }");
            }
            return;
        }

        if line_for_scan.contains("= @") && line_for_scan.contains("=>") {
            let Some(arrow_rel) = line_for_scan.find("=>") else {
                return;
            };
            let after_arrow = start + arrow_rel + 2;
            let Some(open_rel) = bytes[after_arrow..line_end].iter().position(|&b| b == b'{')
            else {
                return;
            };
            let open = after_arrow + open_rel;
            let mut pos = open + 1;
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if bytes.get(pos) == Some(&b'}') {
                self.write_line();
                self.write("{");
                self.write_line();
                self.write("}");
            }
            return;
        }

        if recovered_async_arrow_return {
            self.write_line();
            self.write_semicolon();
            self.write_line();
            self.write("{");
            self.write_line();
            self.write("}");
            return;
        }

        let Some(arrow_rel) = line_for_scan
            .find("): =>")
            .or_else(|| line_for_scan.find("):=>"))
        else {
            return;
        };

        // Parser recovery for `var v = (a): => { }` ends the variable statement
        // before the recovered empty block. TSC still emits that block as a
        // separate statement after the `var`.
        let after_arrow = start + arrow_rel + line[arrow_rel..].find("=>").unwrap_or(0) + 2;
        let Some(open_rel) = bytes[after_arrow..line_end].iter().position(|&b| b == b'{') else {
            return;
        };
        let open = after_arrow + open_rel;
        let mut pos = open + 1;
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if bytes.get(pos) != Some(&b'}') {
            return;
        }

        self.write_line();
        self.write("{");
        self.write_line();
        self.write("}");
        self.write_line();
        self.write_semicolon();
    }
    pub(in crate::emitter) fn emit_recovered_typeof_member_call_after_variable_statement(
        &mut self,
        node: &Node,
    ) {
        // Only recover when every declaration in the statement lacks an initializer.
        // If any declaration has an initializer, .typeof( is a valid property call
        // in a value expression that was already emitted — not a type-annotation tail.
        if let Some(var_stmt) = self.arena.get_variable(node) {
            if !self.all_declarations_lack_initializer(&var_stmt.declarations) {
                return;
            }
        }

        let Some(text) = self.source_text else {
            return;
        };
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let end = std::cmp::min(node.end as usize, text.len());
        if start >= end {
            return;
        }
        let Some(typeof_pos) = self.find_source_pattern_outside_quoted_text(start, end, ".typeof(")
        else {
            return;
        };
        let open = typeof_pos + ".typeof".len();
        let Some(close) = self.find_matching_source_paren(open, end) else {
            return;
        };
        let argument = text[open + 1..close].trim();
        if argument.is_empty() {
            return;
        }

        self.write_line();
        self.write("typeof (");
        self.write(argument);
        self.write(");");
    }

    fn find_source_pattern_outside_quoted_text(
        &self,
        start: usize,
        limit: usize,
        pattern: &str,
    ) -> Option<usize> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let pattern = pattern.as_bytes();
        let mut i = start;
        let limit = limit.min(bytes.len());
        while i + pattern.len() <= limit {
            match bytes[i] {
                b'\'' | b'"' | b'`' => {
                    i = self.skip_quoted_source_text(i, limit);
                    continue;
                }
                _ if bytes.get(i..i + pattern.len()) == Some(pattern) => return Some(i),
                _ => i += 1,
            }
        }
        None
    }

    fn find_matching_source_paren(&self, open: usize, limit: usize) -> Option<usize> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        if bytes.get(open) != Some(&b'(') {
            return None;
        }

        let mut depth = 1u32;
        let mut i = open + 1;
        while i < limit && i < bytes.len() {
            match bytes[i] {
                b'\'' | b'"' | b'`' => {
                    i = self.skip_quoted_source_text(i, limit);
                    continue;
                }
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    fn skip_quoted_source_text(&self, quote_start: usize, limit: usize) -> usize {
        let Some(text) = self.source_text else {
            return quote_start + 1;
        };
        let bytes = text.as_bytes();
        let quote = bytes[quote_start];
        let mut i = quote_start + 1;
        while i < limit && i < bytes.len() {
            if bytes[i] == b'\\' {
                i = (i + 2).min(limit);
                continue;
            }
            if bytes[i] == quote {
                return i + 1;
            }
            i += 1;
        }
        i
    }

    fn source_text_with_quoted_spans_masked(segment: &str) -> String {
        let mut bytes = segment.as_bytes().to_vec();
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'\'' | b'"' | b'`' => {
                    let quote = bytes[i];
                    bytes[i] = b' ';
                    i += 1;
                    while i < bytes.len() {
                        if bytes[i] == b'\\' {
                            bytes[i] = b' ';
                            if i + 1 < bytes.len() {
                                bytes[i + 1] = b' ';
                            }
                            i = (i + 2).min(bytes.len());
                            continue;
                        }

                        let is_end = bytes[i] == quote;
                        bytes[i] = b' ';
                        i += 1;
                        if is_end {
                            break;
                        }
                    }
                }
                _ => i += 1,
            }
        }
        String::from_utf8(bytes).unwrap_or_default()
    }

    pub(in crate::emitter) fn recovered_async_arrow_return_name(
        &self,
        node: &Node,
    ) -> Option<String> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        if start >= bytes.len() {
            return None;
        }

        let mut line_end = start;
        while line_end < bytes.len() && bytes[line_end] != b'\n' && bytes[line_end] != b'\r' {
            line_end += 1;
        }

        let line = std::str::from_utf8(&bytes[start..line_end]).ok()?;
        let line = Self::source_text_with_quoted_spans_masked(line);
        if !line.contains("async") || !line.contains("= await =>") {
            return None;
        }

        let colon = line.find("):")? + 2;
        let arrow = line[colon..].find("=>")? + colon;
        let return_type = line[colon..arrow].trim();
        let name: String = return_type
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
            .collect();
        if name.is_empty() { None } else { Some(name) }
    }

    pub(in crate::emitter) fn recovered_bare_arrow_return_name(
        &self,
        node: &Node,
    ) -> Option<String> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        if start >= bytes.len() {
            return None;
        }

        let mut line_end = start;
        while line_end < bytes.len() && bytes[line_end] != b'\n' && bytes[line_end] != b'\r' {
            line_end += 1;
        }

        let line = std::str::from_utf8(&bytes[start..line_end]).ok()?;
        let line = Self::source_text_with_quoted_spans_masked(line);
        let equals = line.find('=')?;
        let arrow = line[equals..].find("=>")? + equals;
        let colon = line[equals..arrow].rfind(':')? + equals;
        let arrow_head = line[equals + 1..colon].trim();
        if arrow_head.is_empty()
            || !arrow_head
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
        {
            return None;
        }

        let return_type = line[colon + 1..arrow].trim();
        let name: String = return_type
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
            .collect();
        if name.is_empty() { None } else { Some(name) }
    }

    pub(in crate::emitter) fn emit_recovered_ambiguous_generic_assertion_variable_statement(
        &mut self,
        node: &Node,
    ) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let bytes = text.as_bytes();
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        if start >= bytes.len() {
            return false;
        }

        let mut line_end = start;
        while line_end < bytes.len() && bytes[line_end] != b'\n' && bytes[line_end] != b'\r' {
            line_end += 1;
        }

        let Ok(line) = std::str::from_utf8(&bytes[start..line_end]) else {
            return false;
        };
        let Some((head, recovered)) = Self::recovered_ambiguous_generic_assertion_parts(line)
        else {
            return false;
        };

        self.write(&head);
        self.write_line();
        self.write(&recovered);
        true
    }

    pub(in crate::emitter) fn emit_recovered_template_property_name_variable_statement(
        &mut self,
        node: &Node,
    ) -> bool {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return false;
        };
        if self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
            || var_stmt.declarations.nodes.len() != 1
        {
            return false;
        }
        let Some(decl_list_idx) = var_stmt.declarations.nodes.first().copied() else {
            return false;
        };
        let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
            return false;
        };
        let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
            return false;
        };
        if decl_list.declarations.nodes.len() != 1 {
            return false;
        }
        let Some(decl_idx) = decl_list.declarations.nodes.first().copied() else {
            return false;
        };
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return false;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        if decl.initializer.is_none() {
            return false;
        }
        let Some(init_node) = self.arena.get(decl.initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(obj) = self.arena.get_literal_expr(init_node) else {
            return false;
        };
        if obj.elements.nodes.len() != 1 {
            return false;
        }
        let Some(prop_node) = obj
            .elements
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
        else {
            return false;
        };
        if prop_node.kind != syntax_kind_ext::PROPERTY_ASSIGNMENT {
            return false;
        }
        let Some(prop) = self.arena.get_property_assignment(prop_node) else {
            return false;
        };
        let Some(name_node) = self.arena.get(prop.name) else {
            return false;
        };
        if name_node.kind != SyntaxKind::NoSubstitutionTemplateLiteral as u16
            && name_node.kind != syntax_kind_ext::TEMPLATE_EXPRESSION
        {
            return false;
        }

        let flags = decl_list_node.flags as u32;
        let keyword = if flags & node_flags::CONST != 0 {
            "const"
        } else if flags & node_flags::LET != 0 {
            "let"
        } else {
            "var"
        };
        self.write(keyword);
        self.write(" ");
        self.emit_decl_name(decl.name);
        self.write(" = {} ");
        self.emit(prop.name);
        self.write(";");
        self.write_line();
        self.emit_expression(prop.initializer);
        self.write(";");
        true
    }

    fn recovered_ambiguous_generic_assertion_parts(line: &str) -> Option<(String, String)> {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("var ") || !trimmed.contains("= <<") {
            return None;
        }

        let eq = line.find('=')?;
        let before_eq = line[..eq].trim_end();
        let mut rest = line[eq + 1..].trim_start();
        if !rest.starts_with("<<") {
            return None;
        }
        rest = &rest[2..];

        let type_param_end = rest.find('>')?;
        let type_param = rest[..type_param_end].trim();
        rest = rest[type_param_end + 1..].trim_start();
        if !rest.starts_with('(') {
            return None;
        }
        rest = &rest[1..];

        let param_end = rest.find(')')?;
        let param = rest[..param_end].split(':').next()?.trim();
        rest = rest[param_end + 1..].trim_start();
        if !rest.starts_with("=>") {
            return None;
        }
        rest = rest[2..].trim_start();

        let return_type_end = rest.find('>')?;
        let return_type = rest[..return_type_end].trim();
        rest = rest[return_type_end + 1..].trim_start();

        let (callee, trailing_comment) = if let Some(comment_start) = rest.find("//") {
            (
                rest[..comment_start].trim().trim_end_matches(';').trim(),
                Some(rest[comment_start..].trim()),
            )
        } else {
            (rest.trim().trim_end_matches(';').trim(), None)
        };
        if type_param.is_empty() || param.is_empty() || return_type.is_empty() || callee.is_empty()
        {
            return None;
        }

        let head = format!("{before_eq} =  << {type_param} > ({param}), {return_type};");
        let recovered = if let Some(comment) = trailing_comment {
            format!("{return_type} > {callee}; {comment}")
        } else {
            format!("{return_type} > {callee};")
        };
        Some((head, recovered))
    }

    pub(in crate::emitter) fn emit_es5_empty_binding_pattern_export(
        &mut self,
        declarations: &NodeList,
    ) -> bool {
        let mut initializers = Vec::new();

        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                return false;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                return false;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    return false;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    return false;
                };
                if !self.binding_pattern_is_empty(decl.name) || decl.initializer.is_none() {
                    return false;
                }
                initializers.push(decl.initializer);
            }
        }

        if initializers.is_empty() {
            return false;
        }

        for (index, initializer) in initializers.iter().copied().enumerate() {
            if index > 0 {
                self.write_line();
            }
            let source_temp = self.make_unique_name_hoisted();
            let export_temp = self.make_unique_name();
            self.write("export var ");
            self.write(&export_temp);
            self.write(" = ");
            self.write(&source_temp);
            self.write(" = ");
            self.emit(initializer);
            self.write_semicolon();
        }
        true
    }
}
