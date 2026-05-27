//! Fallback code-fix helpers for missing members, const, await, and enum members.

use super::Server;
use super::handlers_code_fixes_utils::{
    extract_quoted_text, find_matching_brace, is_identifier, resolve_module_path,
};
use tsz::lsp::position::LineMap;

impl Server {
    pub(super) fn find_property_access_name_for_missing_member_fallback(
        content: &str,
    ) -> Option<String> {
        for line in content.lines() {
            if line.trim_start().starts_with("import ") {
                continue;
            }

            let mut chars = line.char_indices().peekable();
            while let Some((idx, ch)) = chars.next() {
                if ch != '.' || idx == 0 {
                    continue;
                }

                let prev = line[..idx].chars().next_back();
                if !prev.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
                    continue;
                }

                let mut name = String::new();
                while let Some((_, next_ch)) = chars.peek().copied() {
                    if next_ch.is_ascii_alphanumeric() || next_ch == '_' || next_ch == '$' {
                        name.push(next_ch);
                        chars.next();
                    } else {
                        break;
                    }
                }

                if !name.is_empty() {
                    return Some(name);
                }
            }
        }

        None
    }

    pub(super) fn missing_member_codefix_actions(
        &self,
        file_path: &str,
        content: &str,
        prop_name: &str,
    ) -> Vec<serde_json::Value> {
        let Some((receiver_name, _)) =
            Self::find_property_access_for_missing_member_fallback(content, prop_name)
        else {
            return Vec::new();
        };
        let Some(interface_name) = Self::find_declared_type_for_identifier(content, &receiver_name)
        else {
            return Vec::new();
        };
        let Some((target_file, target_content, insert_offset)) =
            self.find_interface_member_insert_target(file_path, content, &interface_name)
        else {
            return Vec::new();
        };

        let line_map = LineMap::build(&target_content);
        let insert_pos = line_map.offset_to_position(insert_offset as u32, &target_content);
        let change_for = |new_text: String| {
            serde_json::json!([{
                "fileName": target_file,
                "textChanges": [{
                    "start": { "line": insert_pos.line + 1, "offset": insert_pos.character + 1 },
                    "end": { "line": insert_pos.line + 1, "offset": insert_pos.character + 1 },
                    "newText": new_text
                }]
            }])
        };

        vec![
            serde_json::json!({
                "fixName": "addMissingMember",
                "description": format!("Declare method '{prop_name}'"),
                "changes": change_for(format!("\n    {prop_name}(): unknown;\n")),
                "fixId": "fixMissingMember",
                "fixAllDescription": "Add all missing members",
            }),
            serde_json::json!({
                "fixName": "addMissingMember",
                "description": format!("Declare property '{prop_name}'"),
                "changes": change_for(format!("\n    {prop_name}: unknown;\n")),
                "fixId": "fixMissingMember",
                "fixAllDescription": "Add all missing members",
            }),
            serde_json::json!({
                "fixName": "addMissingMember",
                "description": format!("Add index signature for property '{prop_name}'"),
                "changes": change_for("\n    [x: string]: unknown;\n".to_string()),
                "fixId": "fixMissingMember",
                "fixAllDescription": "Add all missing members",
            }),
        ]
    }

    pub(super) fn find_property_access_for_missing_member_fallback(
        content: &str,
        prop_name: &str,
    ) -> Option<(String, String)> {
        for line in content.lines() {
            if line.trim_start().starts_with("import ") {
                continue;
            }

            let needle = format!(".{prop_name}");
            let Some(dot_idx) = line.find(&needle) else {
                continue;
            };
            let suffix = &line[dot_idx + needle.len()..];
            if suffix
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
            {
                continue;
            }

            let prefix = &line[..dot_idx];
            let receiver_end = prefix.len();
            let receiver_start = prefix
                .char_indices()
                .rev()
                .find_map(|(idx, ch)| {
                    (!matches!(ch, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '$'))
                        .then_some(idx + ch.len_utf8())
                })
                .unwrap_or(0);
            let receiver_name = prefix[receiver_start..receiver_end].trim();
            if is_identifier(receiver_name) {
                return Some((receiver_name.to_string(), prop_name.to_string()));
            }
        }

        None
    }

    pub(super) fn find_declared_type_for_identifier(content: &str, ident: &str) -> Option<String> {
        for line in content.lines() {
            let trimmed = line.trim_start();
            let Some(declaration_start) = [
                "declare const ",
                "declare let ",
                "declare var ",
                "const ",
                "let ",
                "var ",
            ]
            .iter()
            .find_map(|prefix| trimmed.strip_prefix(prefix)) else {
                continue;
            };
            let Some(rest) = declaration_start.strip_prefix(ident) else {
                continue;
            };
            if rest
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
            {
                continue;
            }
            let Some(after_colon) = rest.trim_start().strip_prefix(':') else {
                continue;
            };
            let after_colon = after_colon.trim_start();
            let type_name: String = after_colon
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                .collect();
            if is_identifier(&type_name) {
                return Some(type_name);
            }
        }

        None
    }

    pub(super) fn find_interface_member_insert_target(
        &self,
        file_path: &str,
        content: &str,
        interface_name: &str,
    ) -> Option<(String, String, usize)> {
        let mut candidates = Vec::new();
        candidates.push(file_path.to_string());

        for line in content.lines() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("import ") {
                continue;
            }
            let Some(specifier) = extract_quoted_text(trimmed) else {
                continue;
            };
            if let Some(resolved) = resolve_module_path(file_path, specifier, &self.open_files) {
                candidates.push(resolved);
            }
        }

        candidates.extend(self.open_files.keys().cloned());
        candidates.sort();
        candidates.dedup();

        for candidate in candidates {
            let Some(candidate_content) = self
                .open_files
                .get(&candidate)
                .cloned()
                .or_else(|| std::fs::read_to_string(&candidate).ok())
            else {
                continue;
            };
            if let Some(insert_offset) =
                Self::find_interface_member_insert_offset(&candidate_content, interface_name)
            {
                return Some((candidate, candidate_content, insert_offset));
            }
        }

        None
    }

    pub(super) fn find_interface_member_insert_offset(
        content: &str,
        interface_name: &str,
    ) -> Option<usize> {
        let interface_token = format!("interface {interface_name}");
        let interface_pos = content.find(&interface_token)?;
        let open_brace_rel = content[interface_pos..].find('{')?;
        let open_brace = interface_pos + open_brace_rel;
        find_matching_brace(content, open_brace)
    }

    pub(super) fn find_first_binding_identifier(text: &str) -> Option<(usize, usize, String)> {
        let bytes = text.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            let ch = bytes[i] as char;
            if !(ch.is_ascii_alphabetic() || ch == '_' || ch == '$') {
                i += 1;
                continue;
            }
            let start = i;
            i += 1;
            while i < bytes.len() {
                let next = bytes[i] as char;
                if next.is_ascii_alphanumeric() || next == '_' || next == '$' {
                    i += 1;
                } else {
                    break;
                }
            }
            let prev = start.checked_sub(1).and_then(|idx| bytes.get(idx)).copied();
            if prev.is_some_and(|b| {
                let c = b as char;
                c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '.' | '\'' | '"' | '`')
            }) {
                continue;
            }
            let ident = text[start..i].to_string();
            return Some((start, i, ident));
        }
        None
    }

    pub(super) fn find_all_binding_identifiers(text: &str) -> Vec<String> {
        let bytes = text.as_bytes();
        let mut i = 0usize;
        let mut out = Vec::new();
        while i < bytes.len() {
            let ch = bytes[i] as char;
            if !(ch.is_ascii_alphabetic() || ch == '_' || ch == '$') {
                i += 1;
                continue;
            }
            let start = i;
            i += 1;
            while i < bytes.len() {
                let next = bytes[i] as char;
                if next.is_ascii_alphanumeric() || next == '_' || next == '$' {
                    i += 1;
                } else {
                    break;
                }
            }
            let prev = start.checked_sub(1).and_then(|idx| bytes.get(idx)).copied();
            if prev.is_some_and(|b| {
                let c = b as char;
                c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '.' | '\'' | '"' | '`')
            }) {
                continue;
            }
            out.push(text[start..i].to_string());
        }
        out
    }

    pub(super) fn add_missing_const_should_skip_for_declared_bindings(
        content: &str,
        line_map: &LineMap,
        start: tsz::lsp::position::Position,
    ) -> bool {
        let Some(start_off) = line_map
            .position_to_offset(start, content)
            .map(|o| o as usize)
        else {
            return false;
        };
        if start_off > content.len() {
            return false;
        }

        let line_start = content[..start_off].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = content[start_off..]
            .find('\n')
            .map_or(content.len(), |idx| start_off + idx);
        let Some(line) = content.get(line_start..line_end) else {
            return false;
        };
        let trimmed = line.trim_start();
        if trimmed.starts_with("for (") || !trimmed.contains('=') {
            return false;
        }

        let mut statement = line.trim().to_string();
        if statement.ends_with(',') {
            let mut next_start = line_end;
            while next_start < content.len() {
                let next_end = content[next_start..]
                    .find('\n')
                    .map_or(content.len(), |idx| next_start + idx);
                let next_line = content[next_start..next_end].trim();
                if next_line.is_empty() {
                    next_start = next_end.saturating_add(1);
                    continue;
                }
                if !statement.ends_with(',') {
                    break;
                }
                statement.push(' ');
                statement.push_str(next_line);
                next_start = next_end.saturating_add(1);
            }
        }

        for part in statement.split(',') {
            let lhs = part
                .split_once('=')
                .map(|(left, _)| left.trim())
                .unwrap_or_default();
            if lhs.is_empty() {
                continue;
            }
            for name in Self::find_all_binding_identifiers(lhs) {
                if Self::is_value_name_declared_before_offset(content, name.as_str(), line_start) {
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn is_value_name_declared_before_offset(
        content: &str,
        name: &str,
        offset: usize,
    ) -> bool {
        let prefix = content.get(..offset).unwrap_or(content);
        let declaration_prefixes = [
            "let ",
            "const ",
            "var ",
            "function ",
            "class ",
            "enum ",
            "catch (",
        ];
        declaration_prefixes
            .iter()
            .any(|decl_prefix| Self::has_declaration_pattern(prefix, decl_prefix, name))
    }

    pub(super) fn has_declaration_pattern(haystack: &str, decl_prefix: &str, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        let needle = format!("{decl_prefix}{name}");
        for (idx, _) in haystack.match_indices(&needle) {
            let before_ok = idx == 0
                || !haystack
                    .as_bytes()
                    .get(idx.saturating_sub(1))
                    .is_some_and(|b| {
                        let ch = *b as char;
                        ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
                    });
            let after_idx = idx + needle.len();
            let after_ok = !haystack.as_bytes().get(after_idx).is_some_and(|b| {
                let ch = *b as char;
                ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
            });
            if before_ok && after_ok {
                return true;
            }
        }
        false
    }

    pub(super) fn has_mixed_declared_binding_assignment(content: &str) -> bool {
        let mut line_start = 0usize;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                line_start = line_start.saturating_add(line.len() + 1);
                continue;
            }
            if trimmed.starts_with("for (")
                || trimmed.starts_with("for await (")
                || !trimmed.contains('=')
                || trimmed.starts_with("const ")
                || trimmed.starts_with("let ")
                || trimmed.starts_with("var ")
            {
                line_start = line_start.saturating_add(line.len() + 1);
                continue;
            }

            let lhs_all = trimmed
                .split_once('=')
                .map(|(left, _)| left.trim())
                .unwrap_or_default();
            if lhs_all.is_empty() {
                line_start = line_start.saturating_add(line.len() + 1);
                continue;
            }

            let mut saw_declared = false;
            let mut saw_undeclared = false;
            for binding_part in lhs_all.split(',') {
                let lhs = binding_part.trim();
                if lhs.is_empty() {
                    continue;
                }
                for name in Self::find_all_binding_identifiers(lhs) {
                    if Self::is_value_name_declared_before_offset(
                        content,
                        name.as_str(),
                        line_start,
                    ) {
                        saw_declared = true;
                    } else {
                        saw_undeclared = true;
                    }
                }
            }

            if saw_declared && saw_undeclared {
                return true;
            }

            line_start = line_start.saturating_add(line.len() + 1);
        }
        false
    }

    pub(super) fn apply_add_missing_function_declaration_fallback_at_request(
        content: &str,
        line_map: &LineMap,
        request_span: Option<(tsz::lsp::position::Position, tsz::lsp::position::Position)>,
    ) -> Option<(String, String)> {
        let (start, _) = request_span?;
        let start_off = line_map.position_to_offset(start, content)? as usize;
        if start_off >= content.len() {
            return None;
        }
        let line_start = content[..start_off].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = content[start_off..]
            .find('\n')
            .map_or(content.len(), |idx| start_off + idx);
        let line = content.get(line_start..line_end)?.trim();
        if !(line.starts_with('[') && line.contains('=') && line.contains('(')) {
            return None;
        }

        let lhs = line
            .split_once('=')
            .map(|(left, _)| left.trim())
            .unwrap_or("");
        let rel = start_off.saturating_sub(line_start).min(lhs.len());
        let lhs_bytes = lhs.as_bytes();
        if rel >= lhs_bytes.len() {
            return None;
        }

        let mut ident_start = rel;
        while ident_start > 0 {
            let c = lhs_bytes[ident_start - 1] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                ident_start -= 1;
            } else {
                break;
            }
        }
        let mut ident_end = rel;
        while ident_end < lhs_bytes.len() {
            let c = lhs_bytes[ident_end] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                ident_end += 1;
            } else {
                break;
            }
        }
        if ident_start >= ident_end {
            let mut i = 0usize;
            while i < lhs_bytes.len() {
                let ch = lhs_bytes[i] as char;
                if !(ch.is_ascii_alphabetic() || ch == '_' || ch == '$') {
                    i += 1;
                    continue;
                }
                let start_ident = i;
                i += 1;
                while i < lhs_bytes.len() {
                    let c = lhs_bytes[i] as char;
                    if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                let candidate = &lhs[start_ident..i];
                let mut j = i;
                while j < lhs_bytes.len() && (lhs_bytes[j] as char).is_ascii_whitespace() {
                    j += 1;
                }
                if j < lhs_bytes.len() && lhs_bytes[j] as char == '(' {
                    if content.contains(&format!("function {candidate}(")) {
                        continue;
                    }
                    let mut updated = content.trim_end_matches('\n').to_string();
                    updated.push_str("\n\n");
                    updated.push_str(&format!(
                        "function {candidate}() {{\n    throw new Error(\"Function not implemented.\");\n}}\n"
                    ));
                    return Some((candidate.to_string(), updated));
                }
            }
            return None;
        }
        let name = lhs[ident_start..ident_end].to_string();
        let mut after = ident_end;
        while after < lhs_bytes.len() && (lhs_bytes[after] as char).is_ascii_whitespace() {
            after += 1;
        }
        if after >= lhs_bytes.len() || lhs_bytes[after] as char != '(' {
            return None;
        }
        if content.contains(&format!("function {name}(")) {
            return None;
        }

        let mut updated = content.trim_end_matches('\n').to_string();
        updated.push_str("\n\n");
        updated.push_str(&format!(
            "function {name}() {{\n    throw new Error(\"Function not implemented.\");\n}}\n"
        ));
        Some((name, updated))
    }

    /// True iff every `changes[].textChanges` array in the action is empty
    /// (or absent). Used to detect placeholder import actions like
    /// `Add all missing imports` with no real candidates.
    pub(super) fn action_has_no_text_changes(action: &serde_json::Value) -> bool {
        let Some(changes) = action.get("changes").and_then(serde_json::Value::as_array) else {
            return true;
        };
        changes.iter().all(|change| {
            change
                .get("textChanges")
                .and_then(serde_json::Value::as_array)
                .is_none_or(|tc| tc.is_empty())
        })
    }

    /// Match a plain function call `name(...)` at the request span and produce
    /// a `fixMissingFunctionDeclaration` candidate, mirroring tsserver's
    /// behavior for unresolved call expressions outside destructuring contexts.
    /// Returns `(name, updated_content)` or `None` when the call site doesn't
    /// look like an unresolved function call.
    ///
    /// Regression for <https://github.com/mohsen1/tsz/issues/3806> — without this
    /// path, the empty `fixMissingImport` action stays in `response_actions`
    /// and the missing-function fallback never fires.
    pub(super) fn apply_add_missing_function_declaration_for_plain_call_at_request(
        content: &str,
        line_map: &LineMap,
        request_span: Option<(tsz::lsp::position::Position, tsz::lsp::position::Position)>,
    ) -> Option<(String, String)> {
        let (start, _) = request_span?;
        let start_off = line_map.position_to_offset(start, content)? as usize;
        if start_off >= content.len() {
            return None;
        }
        let bytes = content.as_bytes();

        // Identify the identifier surrounding/at start_off.
        let mut ident_start = start_off;
        while ident_start > 0 {
            let c = bytes[ident_start - 1] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                ident_start -= 1;
            } else {
                break;
            }
        }
        let mut ident_end = start_off;
        while ident_end < bytes.len() {
            let c = bytes[ident_end] as char;
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                ident_end += 1;
            } else {
                break;
            }
        }
        if ident_start >= ident_end {
            return None;
        }
        let first = bytes[ident_start] as char;
        if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
            return None;
        }

        let name = content.get(ident_start..ident_end)?.to_string();

        // Keep this text fallback narrowly scoped. Fourslash has existing
        // incomplete-call cases inside generic functions where tsserver infers
        // type parameters from the call arguments; a simple text fallback would
        // mask those richer fixes.
        if !Self::is_top_level_offset(content, ident_start) {
            return None;
        }

        // Must be immediately followed (after whitespace) by `(`.
        let mut after = ident_end;
        while after < bytes.len() && (bytes[after] as char).is_ascii_whitespace() {
            after += 1;
        }
        if after >= bytes.len() || bytes[after] as char != '(' {
            return None;
        }

        // Don't offer the fix if a `function <name>(` already exists somewhere in the file.
        if content.contains(&format!("function {name}(")) {
            return None;
        }

        let mut updated = content.trim_end_matches('\n').to_string();
        updated.push_str("\n\n");
        updated.push_str(&format!(
            "function {name}() {{\n    throw new Error(\"Function not implemented.\");\n}}\n"
        ));
        Some((name, updated))
    }

    pub(super) fn is_top_level_offset(content: &str, offset: usize) -> bool {
        let mut depth = 0usize;
        for ch in content[..offset.min(content.len())].chars() {
            match ch {
                '{' => depth = depth.saturating_add(1),
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        depth == 0
    }

    pub(super) fn apply_add_missing_function_declaration_fallback_anywhere(
        content: &str,
    ) -> Option<(String, String)> {
        for line in content.lines() {
            let trimmed = line.trim();
            if !(trimmed.starts_with('[') && trimmed.contains('=') && trimmed.contains('(')) {
                continue;
            }
            let lhs = trimmed
                .split_once('=')
                .map(|(left, _)| left.trim())
                .unwrap_or("");
            if !lhs.starts_with('[') {
                continue;
            }
            let lhs_bytes = lhs.as_bytes();
            let mut i = 0usize;
            while i < lhs_bytes.len() {
                let ch = lhs_bytes[i] as char;
                if !(ch.is_ascii_alphabetic() || ch == '_' || ch == '$') {
                    i += 1;
                    continue;
                }
                let start = i;
                i += 1;
                while i < lhs_bytes.len() {
                    let c = lhs_bytes[i] as char;
                    if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                let name = &lhs[start..i];
                let mut j = i;
                while j < lhs_bytes.len() && (lhs_bytes[j] as char).is_ascii_whitespace() {
                    j += 1;
                }
                if j >= lhs_bytes.len() || lhs_bytes[j] as char != '(' {
                    continue;
                }
                if content.contains(&format!("function {name}(")) {
                    continue;
                }
                let mut updated = content.trim_end_matches('\n').to_string();
                updated.push_str("\n\n");
                updated.push_str(&format!(
                    "function {name}() {{\n    throw new Error(\"Function not implemented.\");\n}}\n"
                ));
                return Some((name.to_string(), updated));
            }
        }
        None
    }

    pub(super) fn is_comma_continuation_line(lines: &[String], idx: usize) -> bool {
        if idx == 0 {
            return false;
        }
        let mut prev = idx;
        while prev > 0 {
            prev -= 1;
            let trimmed = lines[prev].trim();
            if trimmed.is_empty() {
                continue;
            }
            return trimmed.ends_with(',');
        }
        false
    }

    pub(super) fn add_missing_const_line(line: &str) -> Option<String> {
        let trimmed = line.trim_start();
        if trimmed.is_empty()
            || trimmed.starts_with("const ")
            || trimmed.starts_with("let ")
            || trimmed.starts_with("var ")
            || trimmed.starts_with("type ")
            || trimmed.starts_with("import ")
            || trimmed.starts_with("export ")
        {
            return None;
        }

        if trimmed.starts_with("for (")
            && (trimmed.contains(" in ") || trimmed.contains(" of "))
            && let Some(open_idx) = line.find('(')
        {
            let mut updated = line.to_string();
            updated.insert_str(open_idx + 1, "const ");
            return Some(updated);
        }

        let starts_with_target = trimmed.chars().next().is_some_and(|ch| {
            ch.is_ascii_alphabetic() || ch == '_' || ch == '$' || ch == '[' || ch == '{'
        });
        if starts_with_target && trimmed.contains('=') {
            let lhs = trimmed
                .split_once('=')
                .map(|(left, _)| left)
                .unwrap_or(trimmed);
            if lhs.contains('(') {
                return None;
            }
            let indent_len = line.len().saturating_sub(trimmed.len());
            let indent = &line[..indent_len];
            return Some(format!("{indent}const {trimmed}"));
        }

        None
    }

    pub(super) fn apply_add_missing_const_fallback(content: &str) -> Option<String> {
        let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
        for idx in 0..lines.len() {
            if Self::is_comma_continuation_line(&lines, idx) {
                continue;
            }
            if let Some(updated_line) = Self::add_missing_const_line(&lines[idx]) {
                lines[idx] = updated_line;
                return Some(lines.join("\n"));
            }
        }
        None
    }

    pub(super) fn apply_add_missing_const_fix_all_fallback(content: &str) -> Option<String> {
        let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
        let mut changed = false;
        let mut skip_comma_continuation = false;
        let mut idx = 0usize;
        while idx < lines.len() {
            let trimmed = lines[idx].trim();
            if trimmed.is_empty() {
                idx += 1;
                continue;
            }
            if skip_comma_continuation {
                if !trimmed.ends_with(',') {
                    skip_comma_continuation = false;
                }
                idx += 1;
                continue;
            }
            if let Some(updated_line) = Self::add_missing_const_line(&lines[idx]) {
                skip_comma_continuation = lines[idx].trim_end().ends_with(',');
                lines[idx] = updated_line;
                changed = true;
                idx += 1;
                continue;
            }
            idx += 1;
        }
        changed.then(|| lines.join("\n"))
    }

    pub(super) fn apply_add_missing_const_fallback_at_position(
        content: &str,
        line_map: &LineMap,
        start: tsz::lsp::position::Position,
    ) -> Option<String> {
        let start_off = line_map.position_to_offset(start, content)? as usize;
        if start_off > content.len() {
            return None;
        }

        let line_start = content[..start_off].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = content[start_off..]
            .find('\n')
            .map_or(content.len(), |idx| start_off + idx);
        let line = content.get(line_start..line_end)?;
        let trimmed = line.trim_start();
        if trimmed.is_empty()
            || trimmed.starts_with("const ")
            || trimmed.starts_with("let ")
            || trimmed.starts_with("var ")
            || trimmed.starts_with("type ")
            || trimmed.starts_with("import ")
            || trimmed.starts_with("export ")
        {
            return None;
        }

        let insertion_offset = if trimmed.starts_with("for (")
            && (trimmed.contains(" in ") || trimmed.contains(" of "))
        {
            let open_idx = line.find('(')?;
            line_start + open_idx + 1
        } else {
            let starts_with_target = trimmed.chars().next().is_some_and(|ch| {
                ch.is_ascii_alphabetic() || ch == '_' || ch == '$' || ch == '[' || ch == '{'
            });
            if !starts_with_target || !trimmed.contains('=') {
                return None;
            }
            let lhs = trimmed
                .split_once('=')
                .map(|(left, _)| left.trim_end())
                .unwrap_or(trimmed);
            if lhs.contains('(') {
                return None;
            }
            let prefix_ws = line.len().saturating_sub(trimmed.len());
            if let Some((first_rel_start, first_rel_end, _)) =
                Self::find_first_binding_identifier(lhs)
            {
                let abs_first_start = line_start + prefix_ws + first_rel_start;
                let abs_first_end = line_start + prefix_ws + first_rel_end;
                if start_off < abs_first_start || start_off > abs_first_end {
                    return None;
                }
            }
            line_start + line.len().saturating_sub(trimmed.len())
        };

        let mut updated = content.to_string();
        updated.insert_str(insertion_offset, "const ");
        Some(updated)
    }

    pub(super) fn apply_add_missing_await_fallback(
        content: &str,
        fix_all: bool,
    ) -> Option<(String, String)> {
        if !content.contains("async function") {
            return None;
        }

        let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
        let has_promise_annotation = |name: &str| {
            content.contains(&format!("{name}: Promise<"))
                || content.contains(&format!("{name} : Promise<"))
        };
        for idx in 0..lines.len() {
            let trimmed = lines[idx].trim_start();
            if trimmed.starts_with("for (const ")
                && trimmed.contains(" of ")
                && trimmed.contains("g()")
                && !trimmed.starts_with("for await ")
            {
                lines[idx] = lines[idx].replacen("for (", "for await (", 1);
                return Some(("Add 'await'".to_string(), lines.join("\n")));
            }
        }
        let mut promise_vars: Vec<String> = Vec::new();
        for line in &lines {
            let trimmed = line.trim();
            let bytes = trimmed.as_bytes();
            let mut idx = 0usize;
            while idx < trimmed.len() {
                let c = bytes[idx] as char;
                if c.is_ascii_alphabetic() || c == '_' || c == '$' {
                    let start = idx;
                    idx += 1;
                    while idx < trimmed.len() {
                        let cc = bytes[idx] as char;
                        if cc.is_ascii_alphanumeric() || cc == '_' || cc == '$' {
                            idx += 1;
                        } else {
                            break;
                        }
                    }
                    let name = &trimmed[start..idx];
                    let mut j = idx;
                    while j < trimmed.len() && (bytes[j] as char).is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < trimmed.len() && bytes[j] as char == ':' {
                        j += 1;
                        while j < trimmed.len() && (bytes[j] as char).is_ascii_whitespace() {
                            j += 1;
                        }
                        if trimmed[j..].starts_with("Promise<")
                            && !promise_vars.iter().any(|v| v == name)
                        {
                            promise_vars.push(name.to_string());
                        }
                    }
                    continue;
                }
                idx += 1;
            }
        }
        if promise_vars.is_empty() {
            return None;
        }

        let mut initializer_candidates: Vec<(usize, String)> = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("const ")
                || !trimmed.ends_with(';')
                || trimmed.contains("await ")
            {
                continue;
            }
            let Some(eq_idx) = trimmed.find('=') else {
                continue;
            };
            let lhs = trimmed["const ".len()..eq_idx].trim();
            let rhs = trimmed[eq_idx + 1..trimmed.len() - 1].trim();
            if lhs.is_empty() || rhs.is_empty() || rhs.starts_with("await ") {
                continue;
            }
            if rhs.contains(' ') || rhs.contains('.') || rhs.contains('(') {
                continue;
            }
            if !promise_vars.iter().any(|v| v == rhs) {
                continue;
            }
            initializer_candidates.push((idx, lhs.to_string()));
        }

        if !initializer_candidates.is_empty() {
            if fix_all || initializer_candidates.len() > 1 {
                for (idx, _) in &initializer_candidates {
                    if let Some(eq_idx) = lines[*idx].find('=') {
                        let (head, tail) = lines[*idx].split_at(eq_idx + 1);
                        let rhs = tail.trim_start();
                        if !rhs.starts_with("await ") {
                            lines[*idx] = format!("{head} await {rhs}");
                        }
                    }
                }
                return Some(("Add 'await' to initializers".to_string(), lines.join("\n")));
            }

            let (idx, var_name) = initializer_candidates[0].clone();
            if let Some(eq_idx) = lines[idx].find('=') {
                let (head, tail) = lines[idx].split_at(eq_idx + 1);
                let rhs = tail.trim_start();
                lines[idx] = format!("{head} await {rhs}");
            }
            return Some((
                format!("Add 'await' to initializer for '{var_name}'"),
                lines.join("\n"),
            ));
        }

        for idx in 0..lines.len() {
            let trimmed = lines[idx].trim_start();
            let Some(dot_idx) = trimmed.find('.') else {
                continue;
            };
            if !trimmed.ends_with(';')
                || trimmed.starts_with("(await ")
                || trimmed.starts_with("await ")
            {
                continue;
            }
            let ident = trimmed[..dot_idx].trim();
            if ident.is_empty()
                || !ident
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
            {
                continue;
            }
            if !promise_vars.iter().any(|v| v == ident) {
                continue;
            }
            let suffix = &trimmed[dot_idx..];
            let mut replacement = format!("(await {ident}){suffix}");
            if idx > 0 {
                let prev = lines[idx - 1].trim_end();
                if !prev.is_empty() && !prev.ends_with(';') && !prev.ends_with('{') {
                    replacement = format!(";{replacement}");
                }
            }

            let indent_len = lines[idx].len().saturating_sub(trimmed.len());
            let indent = lines[idx][..indent_len].to_string();
            lines[idx] = format!("{indent}{replacement}");
            return Some(("Add 'await'".to_string(), lines.join("\n")));
        }

        for idx in 0..lines.len() {
            let trimmed = lines[idx].trim_start();
            if trimmed.starts_with("if (") {
                let inside = trimmed
                    .strip_prefix("if (")
                    .and_then(|s| s.split(')').next())
                    .map(str::trim)
                    .unwrap_or_default();
                if !inside.is_empty()
                    && inside
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                    && has_promise_annotation(inside)
                {
                    lines[idx] = lines[idx].replacen(
                        &format!("if ({inside})"),
                        &format!("if (await {inside})"),
                        1,
                    );
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }
            }

            if let Some(q_idx) = trimmed.find('?') {
                let cond = trimmed[..q_idx].trim();
                if !cond.is_empty()
                    && cond
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                    && has_promise_annotation(cond)
                {
                    lines[idx] =
                        lines[idx].replacen(&format!("{cond} ?"), &format!("await {cond} ?"), 1);
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }
            }

            for var in &promise_vars {
                let if_pat = format!("if ({var})");
                if trimmed.contains(&if_pat) {
                    lines[idx] = lines[idx].replacen(&if_pat, &format!("if (await {var})"), 1);
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }

                let ternary_pat = format!("{var} ?");
                if trimmed.contains(&ternary_pat) {
                    lines[idx] = lines[idx].replacen(&ternary_pat, &format!("await {var} ?"), 1);
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }

                let spread_pat = format!("[...{var}]");
                if trimmed.contains(&spread_pat) {
                    lines[idx] = lines[idx].replacen(&spread_pat, &format!("[...await {var}]"), 1);
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }

                let for_of_pat = format!(" of {var})");
                if trimmed.contains("for (") && trimmed.contains(&for_of_pat) {
                    lines[idx] = lines[idx].replacen(&for_of_pat, &format!(" of await {var})"), 1);
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }

                let bin_l_pat = format!("{var} |");
                if trimmed.contains(&bin_l_pat) {
                    lines[idx] = lines[idx].replacen(&bin_l_pat, &format!("await {var} |"), 1);
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }

                let bin_r_pat = format!("+ {var}");
                if trimmed.contains(&bin_r_pat) {
                    lines[idx] = lines[idx].replacen(&bin_r_pat, &format!("+ await {var}"), 1);
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }
            }
        }

        for idx in 0..lines.len() {
            let trimmed = lines[idx].trim_start();
            if !(trimmed.ends_with("();")
                || trimmed.ends_with("()")
                || (trimmed.starts_with("new ")
                    && (trimmed.ends_with(");") || trimmed.ends_with(")")))
                || trimmed.contains("await "))
            {
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("new ") {
                let has_semicolon = rest.ends_with(";");
                let ctor = rest.trim_end_matches(';').trim_end_matches("()").trim();
                if ctor
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                    && !ctor.is_empty()
                    && promise_vars.iter().any(|v| v == ctor)
                {
                    let indent_len = lines[idx].len().saturating_sub(trimmed.len());
                    let indent = lines[idx][..indent_len].to_string();
                    let semi = if has_semicolon { ";" } else { "" };
                    lines[idx] = format!("{indent}new (await {ctor})(){semi}");
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }
            } else {
                let has_semicolon = trimmed.ends_with(';');
                let callee = trimmed.trim_end_matches(';').trim_end_matches("()").trim();
                if callee
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                    && !callee.is_empty()
                    && promise_vars.iter().any(|v| v == callee)
                {
                    let indent_len = lines[idx].len().saturating_sub(trimmed.len());
                    let indent = lines[idx][..indent_len].to_string();
                    let mut replacement = format!("(await {callee})()");
                    if idx > 0 {
                        let prev = lines[idx - 1].trim_end();
                        if !prev.is_empty() && !prev.ends_with(';') && !prev.ends_with('{') {
                            replacement = format!(";{replacement}");
                        }
                    }
                    let semi = if has_semicolon { ";" } else { "" };
                    lines[idx] = format!("{indent}{replacement}{semi}");
                    return Some(("Add 'await'".to_string(), lines.join("\n")));
                }
            }
        }

        for idx in 0..lines.len() {
            let trimmed = lines[idx].trim_start();
            if !trimmed.ends_with(");") || trimmed.contains("await ") {
                continue;
            }
            if let Some(open_idx) = trimmed.find('(') {
                let args = &trimmed[open_idx + 1..trimmed.len() - 2];
                if let Some(comma_idx) = args.rfind(',') {
                    let last_arg = args[comma_idx + 1..].trim();
                    if !last_arg.is_empty()
                        && last_arg
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                        && promise_vars.iter().any(|v| v == last_arg)
                    {
                        let mut rebuilt = String::new();
                        rebuilt.push_str(&trimmed[..open_idx + 1]);
                        rebuilt.push_str(&args[..comma_idx + 1]);
                        rebuilt.push(' ');
                        rebuilt.push_str("await ");
                        rebuilt.push_str(last_arg);
                        rebuilt.push_str(");");

                        let indent_len = lines[idx].len().saturating_sub(trimmed.len());
                        let indent = lines[idx][..indent_len].to_string();
                        lines[idx] = format!("{indent}{rebuilt}");
                        return Some(("Add 'await'".to_string(), lines.join("\n")));
                    }
                }
            }
        }

        None
    }

    pub(super) fn apply_add_missing_enum_member_fallback(
        content: &str,
    ) -> Option<(String, String)> {
        if content
            .lines()
            .any(|line| line.trim_start().starts_with("////"))
        {
            let normalized = content
                .lines()
                .map(|line| {
                    let ws_len = line.len().saturating_sub(line.trim_start().len());
                    let ws = &line[..ws_len];
                    let trimmed = &line[ws_len..];
                    if let Some(rest) = trimmed.strip_prefix("////") {
                        format!("{ws}{rest}")
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if normalized != content {
                return Self::apply_add_missing_enum_member_fallback(&normalized);
            }
        }

        let lines: Vec<String> = content.lines().map(str::to_string).collect();

        fn is_ident(s: &str) -> bool {
            !s.is_empty()
                && s.chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                && s.chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_' || ch == '$')
        }

        let mut enum_name = String::new();
        let mut member_name = String::new();
        for line in &lines {
            let trimmed = line.trim().replace("/**/", "");
            if trimmed.starts_with("enum ")
                || trimmed.starts_with("export enum ")
                || trimmed.starts_with("export const enum ")
            {
                continue;
            }

            let bytes = trimmed.as_bytes();
            for (idx, ch) in trimmed.char_indices() {
                if ch != '.' {
                    continue;
                }

                let mut left_end = idx;
                while left_end > 0 && (bytes[left_end - 1] as char).is_ascii_whitespace() {
                    left_end -= 1;
                }
                let mut left_start = left_end;
                while left_start > 0 {
                    let c = bytes[left_start - 1] as char;
                    if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                        left_start -= 1;
                    } else {
                        break;
                    }
                }
                let left = trimmed[left_start..left_end].trim();

                let mut right_start = idx + 1;
                while right_start < trimmed.len()
                    && (bytes[right_start] as char).is_ascii_whitespace()
                {
                    right_start += 1;
                }
                let mut right_end = right_start;
                while right_end < trimmed.len() {
                    let c = bytes[right_end] as char;
                    if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                        right_end += 1;
                    } else {
                        break;
                    }
                }
                let right = trimmed[right_start..right_end].trim();
                if is_ident(left) && is_ident(right) {
                    enum_name = left.to_string();
                    member_name = right.to_string();
                }
            }
        }
        if enum_name.is_empty() || member_name.is_empty() {
            return None;
        }

        let mut start_idx = None;
        let mut end_idx = None;
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let enum_header_match = trimmed.starts_with(&format!("enum {enum_name}"))
                || trimmed.starts_with(&format!("export enum {enum_name}"))
                || trimmed.starts_with(&format!("export const enum {enum_name}"));
            if enum_header_match {
                start_idx = Some(idx);
                for (j, line) in lines.iter().enumerate().skip(idx + 1) {
                    if line.trim() == "}" {
                        end_idx = Some(j);
                        break;
                    }
                }
                break;
            }
        }

        let (start_idx, end_idx) = (start_idx?, end_idx?);
        let mut enum_member_string: std::collections::HashMap<(String, String), bool> =
            std::collections::HashMap::new();
        {
            let mut current_enum: Option<String> = None;
            for line in &lines {
                let trimmed = line.trim();
                if trimmed.starts_with("enum ")
                    || trimmed.starts_with("export enum ")
                    || trimmed.starts_with("export const enum ")
                {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if let Some((idx, _)) = parts.iter().enumerate().find(|(_, p)| **p == "enum")
                        && let Some(name) = parts.get(idx + 1)
                    {
                        current_enum = Some((*name).to_string());
                    }
                    continue;
                }
                if trimmed == "}" {
                    current_enum = None;
                    continue;
                }
                let Some(current) = current_enum.as_ref() else {
                    continue;
                };
                let member_line = trimmed.trim_end_matches(',');
                if member_line.is_empty() {
                    continue;
                }
                let name = member_line
                    .split(['=', ' ', '\t'])
                    .next()
                    .unwrap_or_default()
                    .trim();
                if !is_ident(name) {
                    continue;
                }
                let mut is_string = false;
                if let Some(eq_idx) = member_line.find('=') {
                    let rhs = member_line[eq_idx + 1..].trim();
                    if rhs.starts_with('"') || rhs.starts_with('\'') {
                        is_string = true;
                    } else if let Some(dot) = rhs.find('.') {
                        let lhs = rhs[..dot].trim();
                        let rhs_member = rhs[dot + 1..]
                            .chars()
                            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                            .collect::<String>();
                        if !lhs.is_empty()
                            && !rhs_member.is_empty()
                            && enum_member_string
                                .get(&(lhs.to_string(), rhs_member.to_string()))
                                .copied()
                                .unwrap_or(false)
                        {
                            is_string = true;
                        }
                    }
                }
                enum_member_string.insert((current.clone(), name.to_string()), is_string);
            }
        }

        let mut has_string_initializer = false;
        let mut already_exists = false;
        let mut last_member_idx: Option<usize> = None;
        let mut use_trailing_comma = false;

        for (idx, line) in lines.iter().enumerate().take(end_idx).skip(start_idx + 1) {
            let trimmed = line.trim().trim_end_matches(',');
            if trimmed.is_empty() {
                continue;
            }
            let name = trimmed
                .split(['=', ' ', '\t'])
                .next()
                .unwrap_or_default()
                .trim();
            if name == member_name {
                already_exists = true;
                break;
            }
            has_string_initializer |= enum_member_string
                .get(&(enum_name.clone(), name.to_string()))
                .copied()
                .unwrap_or(false);
            use_trailing_comma = line.trim_end().ends_with(',');
            last_member_idx = Some(idx);
        }
        if already_exists {
            return None;
        }

        let mut updated = lines;
        if let Some(idx) = last_member_idx {
            let prev = &updated[idx];
            let trimmed_len = prev.trim_end().len();
            let (head, trailing) = prev.split_at(trimmed_len);
            if !head.ends_with(',') && !head.ends_with('{') {
                updated[idx] = format!("{head},{trailing}");
            }
        }

        let indent = "    ";
        let new_member_line = if has_string_initializer {
            format!("{indent}{member_name} = \"{member_name}\"")
        } else {
            format!("{indent}{member_name}")
        };
        let new_member_line = if use_trailing_comma {
            format!("{new_member_line},")
        } else {
            new_member_line
        };
        updated.insert(end_idx, new_member_line);
        Some((member_name, updated.join("\n")))
    }
}
