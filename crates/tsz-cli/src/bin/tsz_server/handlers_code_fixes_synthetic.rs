//! Synthetic diagnostic helpers for missing names, JSDoc annotations, and interface implementation.

use super::Server;
use super::handlers_code_fixes_utils::{
    extract_jsdoc_imported_names, extract_jsdoc_type_identifier_spans, extract_type_identifiers,
    is_identifier, parse_bare_identifier_expression, parse_identifier_call_expression,
};
use tsz::checker::diagnostics::DiagnosticCategory;

impl Server {
    pub(super) fn synthetic_jsdoc_suggestion_diagnostic(
        file_path: &str,
        content: &str,
    ) -> Option<tsz::checker::diagnostics::Diagnostic> {
        let _ = Self::apply_simple_jsdoc_annotation_fallback(content)?;

        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return None;
        }

        let mut line_offsets = Vec::with_capacity(lines.len());
        let mut running = 0u32;
        for line in &lines {
            line_offsets.push(running);
            running += line.len() as u32 + 1;
        }

        let mut i = 0usize;
        while i < lines.len() {
            if !lines[i].contains("/**") {
                i += 1;
                continue;
            }

            let block_start = i;
            let mut block_end = i;
            while block_end < lines.len() && !lines[block_end].contains("*/") {
                block_end += 1;
            }
            if block_end >= lines.len() {
                break;
            }

            let mut has_relevant_tag = false;
            for line in &lines[block_start..=block_end] {
                has_relevant_tag |= line.contains("@type {")
                    || line.contains("@param")
                    || line.contains("@return {")
                    || line.contains("@returns {");
            }
            if !has_relevant_tag {
                i = block_end + 1;
                continue;
            }

            let Some(target_line_idx) = lines
                .iter()
                .enumerate()
                .skip(block_end + 1)
                .find_map(|(idx, line)| (!line.trim().is_empty()).then_some(idx))
            else {
                break;
            };
            let target_line = lines[target_line_idx];
            let target_offset = line_offsets[target_line_idx];

            if let Some(var_pos) = target_line.find("var ") {
                let rest = &target_line[var_pos + 4..];
                let name_len = rest
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .count();
                if name_len > 0 {
                    return Some(tsz::checker::diagnostics::Diagnostic {
                        category: DiagnosticCategory::Suggestion,
                        code: tsz_checker::diagnostics::diagnostic_codes::JSDOC_TYPES_MAY_BE_MOVED_TO_TYPESCRIPT_TYPES,
                        file: file_path.to_string(),
                        start: target_offset + (var_pos + 4) as u32,
                        length: name_len as u32,
                        message_text: "JSDoc types may be moved to TypeScript types.".to_string(),
                        related_information: Vec::new(),
                    });
                }
            }

            if let Some(function_pos) = target_line.find("function ") {
                let rest = &target_line[function_pos + "function ".len()..];
                let name_len = rest
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .count();
                if name_len > 0 {
                    return Some(tsz::checker::diagnostics::Diagnostic {
                        category: DiagnosticCategory::Suggestion,
                        code: tsz_checker::diagnostics::diagnostic_codes::JSDOC_TYPES_MAY_BE_MOVED_TO_TYPESCRIPT_TYPES,
                        file: file_path.to_string(),
                        start: target_offset + (function_pos + "function ".len()) as u32,
                        length: name_len as u32,
                        message_text: "JSDoc types may be moved to TypeScript types.".to_string(),
                        related_information: Vec::new(),
                    });
                }
            }

            if let Some(name_start) =
                target_line.find(|ch: char| !ch.is_ascii_whitespace() && ch != '*')
            {
                let rest = &target_line[name_start..];
                let name_len = rest
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .count();
                if name_len > 0 {
                    return Some(tsz::checker::diagnostics::Diagnostic {
                        category: DiagnosticCategory::Suggestion,
                        code: tsz_checker::diagnostics::diagnostic_codes::JSDOC_TYPES_MAY_BE_MOVED_TO_TYPESCRIPT_TYPES,
                        file: file_path.to_string(),
                        start: target_offset + name_start as u32,
                        length: name_len as u32,
                        message_text: "JSDoc types may be moved to TypeScript types.".to_string(),
                        related_information: Vec::new(),
                    });
                }
            }

            if let Some(open_paren) = target_line.find('(') {
                let prefix = target_line[..open_paren].trim_end();
                let name_start = prefix
                    .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
                    .map_or(0, |idx| idx + 1);
                if name_start < prefix.len() {
                    let name = &prefix[name_start..];
                    if !name.is_empty() {
                        return Some(tsz::checker::diagnostics::Diagnostic {
                            category: DiagnosticCategory::Suggestion,
                            code: tsz_checker::diagnostics::diagnostic_codes::JSDOC_TYPES_MAY_BE_MOVED_TO_TYPESCRIPT_TYPES,
                            file: file_path.to_string(),
                            start: target_offset + name_start as u32,
                            length: name.len() as u32,
                            message_text: "JSDoc types may be moved to TypeScript types."
                                .to_string(),
                            related_information: Vec::new(),
                        });
                    }
                }
            }

            i = block_end + 1;
        }

        None
    }

    pub(super) fn synthetic_jsdoc_infer_from_usage_diagnostics(
        file_path: &str,
        content: &str,
    ) -> Vec<tsz::checker::diagnostics::Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return diagnostics;
        }

        let mut line_offsets = Vec::with_capacity(lines.len());
        let mut running = 0u32;
        for line in &lines {
            line_offsets.push(running);
            running += line.len() as u32 + 1;
        }

        let mut i = 0usize;
        while i < lines.len() {
            if !lines[i].contains("/**") {
                i += 1;
                continue;
            }

            let block_start = i;
            let mut block_end = i;
            while block_end < lines.len() && !lines[block_end].contains("*/") {
                block_end += 1;
            }
            if block_end >= lines.len() {
                break;
            }

            let mut has_type_tag = false;
            let mut typed_params: Vec<String> = Vec::new();
            for line in &lines[block_start..=block_end] {
                if !has_type_tag {
                    has_type_tag = Self::extract_jsdoc_tag_type(line, "type").is_some();
                }
                if let Some(param_tag) = Self::extract_jsdoc_param_tag(line)
                    && param_tag.path.len() == 1
                {
                    typed_params.push(param_tag.path[0].clone());
                }
            }

            let Some(target_line_idx) = lines
                .iter()
                .enumerate()
                .skip(block_end + 1)
                .find_map(|(idx, line)| (!line.trim().is_empty()).then_some(idx))
            else {
                break;
            };
            let target_line = lines[target_line_idx];
            let target_offset = line_offsets[target_line_idx];

            if has_type_tag && let Some(var_pos) = target_line.find("var ") {
                let rest = &target_line[var_pos + 4..];
                let name_len = rest
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                    .count();
                if name_len > 0 {
                    let name = &rest[..name_len];
                    let suffix = &rest[name_len..];
                    if !suffix.trim_start().starts_with(':') {
                        diagnostics.push(tsz::checker::diagnostics::Diagnostic {
                            category: DiagnosticCategory::Suggestion,
                            code: tsz_checker::diagnostics::diagnostic_codes::VARIABLE_IMPLICITLY_HAS_AN_TYPE_BUT_A_BETTER_TYPE_MAY_BE_INFERRED_FROM_USAGE,
                            file: file_path.to_string(),
                            start: target_offset + (var_pos + 4) as u32,
                            length: name_len as u32,
                            message_text: format!(
                                "Variable '{name}' implicitly has an 'any' type, but a better type may be inferred from usage."
                            ),
                            related_information: Vec::new(),
                        });
                    }
                }
            }

            if !typed_params.is_empty()
                && let (Some(open), Some(close)) = (target_line.find('('), target_line.rfind(')'))
                && close > open
            {
                let params_text = &target_line[open + 1..close];
                for param_name in typed_params {
                    let Some(name_rel) = params_text.find(&param_name) else {
                        continue;
                    };
                    let seg_start = params_text[..name_rel].rfind(',').map_or(0, |idx| idx + 1);
                    let seg_end = params_text[name_rel..]
                        .find(',')
                        .map_or(params_text.len(), |idx| name_rel + idx);
                    let segment = &params_text[seg_start..seg_end];
                    if segment.contains(':') {
                        continue;
                    }
                    diagnostics.push(tsz::checker::diagnostics::Diagnostic {
                        category: DiagnosticCategory::Suggestion,
                        code: tsz_checker::diagnostics::diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE_BUT_A_BETTER_TYPE_MAY_BE_INFERRED_FROM_USAGE,
                        file: file_path.to_string(),
                        start: target_offset + (open + 1 + name_rel) as u32,
                        length: param_name.len() as u32,
                        message_text: format!(
                            "Parameter '{param_name}' implicitly has an 'any' type, but a better type may be inferred from usage."
                        ),
                        related_information: Vec::new(),
                    });
                }
            }

            i = block_end + 1;
        }

        diagnostics
    }

    pub(super) fn synthetic_missing_name_expression_diagnostics(
        &self,
        file_path: &str,
        content: &str,
        binder: &tsz::binder::BinderState,
    ) -> Vec<tsz::checker::diagnostics::Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut seen_spans = std::collections::HashSet::new();
        let jsdoc_imported_names = extract_jsdoc_imported_names(content);
        let mut offset = 0usize;

        for line_with_newline in content.split_inclusive('\n') {
            let line = line_with_newline.trim_end_matches(['\r', '\n']);
            let trimmed = line.trim_start();
            let is_comment_line =
                trimmed.starts_with("/*") || trimmed.starts_with('*') || trimmed.starts_with("//");
            let leading_ws = line.len().saturating_sub(trimmed.len());
            let skip_scanning = trimmed.starts_with("import ")
                || trimmed.starts_with("export ")
                || trimmed.starts_with("interface ")
                || trimmed.starts_with("type ")
                || trimmed.starts_with("class ")
                || trimmed.starts_with("function ");

            if is_comment_line {
                let is_jsdoc_type_tag = line.contains("@param")
                    || line.contains("@type")
                    || line.contains("@returns")
                    || line.contains("@return");
                if is_jsdoc_type_tag {
                    for (name, rel_start) in extract_jsdoc_type_identifier_spans(line) {
                        if jsdoc_imported_names.contains(name.as_str()) {
                            continue;
                        }
                        if binder.file_locals.get(name.as_str()).is_some() {
                            continue;
                        }
                        if !self.has_potential_auto_import_symbol(file_path, name.as_str()) {
                            continue;
                        }
                        if seen_spans.insert((offset + rel_start, name.len())) {
                            diagnostics.push(tsz::checker::diagnostics::Diagnostic {
                                category: DiagnosticCategory::Error,
                                code: tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                                file: file_path.to_string(),
                                start: (offset + rel_start) as u32,
                                length: name.len() as u32,
                                message_text: format!("Cannot find name '{name}'."),
                                related_information: Vec::new(),
                            });
                        }
                    }
                }

                offset += line_with_newline.len();
                continue;
            }

            if trimmed.starts_with("type ")
                && let Some(eq_idx) = trimmed.find('=')
            {
                self.push_synthetic_missing_type_identifiers(
                    &mut diagnostics,
                    &mut seen_spans,
                    binder,
                    file_path,
                    offset,
                    &trimmed[eq_idx + 1..],
                    leading_ws + eq_idx + 1,
                );
            }

            if trimmed.starts_with("const ")
                || trimmed.starts_with("let ")
                || trimmed.starts_with("var ")
                || trimmed.starts_with("function ")
                || trimmed.starts_with("export function ")
                || trimmed.starts_with("async function ")
                || trimmed.starts_with("export async function ")
            {
                let mut search_from = 0usize;
                while let Some(colon_rel) = trimmed[search_from..].find(':') {
                    let colon_idx = search_from + colon_rel;
                    let after_colon = &trimmed[colon_idx + 1..];
                    let after_colon_trimmed = after_colon.trim_start();
                    let after_colon_ws =
                        after_colon.len().saturating_sub(after_colon_trimmed.len());
                    let fragment_len = after_colon_trimmed
                        .find([',', ')', ';', '=', '{'])
                        .unwrap_or(after_colon_trimmed.len());
                    let fragment = &after_colon_trimmed[..fragment_len];
                    if !fragment.is_empty() {
                        self.push_synthetic_missing_type_identifiers(
                            &mut diagnostics,
                            &mut seen_spans,
                            binder,
                            file_path,
                            offset,
                            fragment,
                            leading_ws + colon_idx + 1 + after_colon_ws,
                        );
                    }
                    search_from = colon_idx + 1;
                }
            }

            if trimmed.starts_with('[') && trimmed.contains('=') {
                let lhs = trimmed
                    .split_once('=')
                    .map(|(left, _)| left)
                    .unwrap_or(trimmed);
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
                        let next = lhs_bytes[i] as char;
                        if next.is_ascii_alphanumeric() || next == '_' || next == '$' {
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    let Some(name) = lhs.get(start..i) else {
                        continue;
                    };
                    if binder.file_locals.get(name).is_some() {
                        continue;
                    }
                    if !seen_spans.insert((offset + start, name.len())) {
                        continue;
                    }
                    diagnostics.push(tsz::checker::diagnostics::Diagnostic {
                        category: DiagnosticCategory::Error,
                        code: tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                        file: file_path.to_string(),
                        start: (offset + start) as u32,
                        length: name.len() as u32,
                        message_text: format!("Cannot find name '{name}'."),
                        related_information: Vec::new(),
                    });
                }
            }

            if let Some((column, name)) = parse_bare_identifier_expression(line)
                .or_else(|| parse_identifier_call_expression(line))
                && binder.file_locals.get(name).is_none()
                && seen_spans.insert((offset + column, name.len()))
            {
                diagnostics.push(tsz::checker::diagnostics::Diagnostic {
                    category: DiagnosticCategory::Error,
                    code: tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                    file: file_path.to_string(),
                    start: (offset + column) as u32,
                    length: name.len() as u32,
                    message_text: format!("Cannot find name '{name}'."),
                    related_information: Vec::new(),
                });
            }
            if !skip_scanning {
                let bytes = line.as_bytes();
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

                    let Some(name) = line.get(start..i) else {
                        continue;
                    };
                    let prev = start
                        .checked_sub(1)
                        .and_then(|idx| line.as_bytes().get(idx));
                    if prev.is_some_and(|b| matches!(*b as char, '.' | '\'' | '"' | '`' | '#')) {
                        continue;
                    }
                    let is_call_expression = line
                        .get(i..)
                        .is_some_and(|rest| rest.trim_start().starts_with('('));
                    if !name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                        && !is_call_expression
                    {
                        continue;
                    }
                    if !is_identifier(name) {
                        continue;
                    }
                    if binder.file_locals.get(name).is_some() {
                        continue;
                    }
                    if !self.has_potential_auto_import_symbol(file_path, name) {
                        continue;
                    }
                    if !seen_spans.insert((offset + start, name.len())) {
                        continue;
                    }

                    diagnostics.push(tsz::checker::diagnostics::Diagnostic {
                        category: DiagnosticCategory::Error,
                        code: tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                        file: file_path.to_string(),
                        start: (offset + start) as u32,
                        length: name.len() as u32,
                        message_text: format!("Cannot find name '{name}'."),
                        related_information: Vec::new(),
                    });
                }
            }
            offset += line_with_newline.len();
        }

        diagnostics
    }

    pub(super) fn push_synthetic_missing_type_identifiers(
        &self,
        diagnostics: &mut Vec<tsz::checker::diagnostics::Diagnostic>,
        seen_spans: &mut std::collections::HashSet<(usize, usize)>,
        binder: &tsz::binder::BinderState,
        file_path: &str,
        line_offset: usize,
        fragment: &str,
        fragment_offset_in_line: usize,
    ) {
        for (ident, rel_start) in Self::type_identifier_spans(fragment) {
            if binder.file_locals.get(ident.as_str()).is_some() {
                continue;
            }
            if !self.has_potential_auto_import_symbol(file_path, ident.as_str()) {
                continue;
            }
            let absolute_start = line_offset + fragment_offset_in_line + rel_start;
            if !seen_spans.insert((absolute_start, ident.len())) {
                continue;
            }
            diagnostics.push(tsz::checker::diagnostics::Diagnostic {
                category: DiagnosticCategory::Error,
                code: tsz_checker::diagnostics::diagnostic_codes::CANNOT_FIND_NAME,
                file: file_path.to_string(),
                start: absolute_start as u32,
                length: ident.len() as u32,
                message_text: format!("Cannot find name '{ident}'."),
                related_information: Vec::new(),
            });
        }
    }

    pub(super) fn type_identifier_spans(fragment: &str) -> Vec<(String, usize)> {
        let mut spans = Vec::new();
        for ident in extract_type_identifiers(fragment) {
            let mut search_start = 0usize;
            while let Some(found) = fragment[search_start..].find(&ident) {
                let rel_start = search_start + found;
                let rel_end = rel_start + ident.len();
                let prev = rel_start
                    .checked_sub(1)
                    .and_then(|idx| fragment.as_bytes().get(idx))
                    .map(|b| *b as char);
                let next = fragment.as_bytes().get(rel_end).map(|b| *b as char);
                let is_qualified_name_segment = prev == Some('.') || next == Some('.');
                let is_import_type_query_segment =
                    Self::is_within_import_type_query(fragment, rel_start);
                let at_word_boundary = prev
                    .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
                    && next
                        .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'));
                if at_word_boundary && !is_qualified_name_segment && !is_import_type_query_segment {
                    spans.push((ident.clone(), rel_start));
                }
                search_start = rel_end;
            }
        }
        spans
    }

    pub(super) fn is_within_import_type_query(fragment: &str, ident_start: usize) -> bool {
        let bytes = fragment.as_bytes();
        let mut i = 0usize;

        while i < bytes.len() {
            let is_word_start = i == 0 || {
                let ch = bytes[i - 1] as char;
                !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
            };
            if !is_word_start || !fragment[i..].starts_with("import(") {
                i += 1;
                continue;
            }

            let import_start = i;
            i += "import(".len();
            let mut depth = 1usize;

            while i < bytes.len() {
                match bytes[i] as char {
                    '"' | '\'' => {
                        let quote = bytes[i];
                        i += 1;
                        while i < bytes.len() {
                            if bytes[i] == b'\\' {
                                i = (i + 2).min(bytes.len());
                                continue;
                            }
                            let matches_quote = bytes[i] == quote;
                            i += 1;
                            if matches_quote {
                                break;
                            }
                        }
                    }
                    '(' => {
                        depth += 1;
                        i += 1;
                    }
                    ')' => {
                        depth = depth.saturating_sub(1);
                        i += 1;
                        if depth == 0 {
                            return ident_start >= import_start && ident_start < i;
                        }
                    }
                    _ => i += 1,
                }
            }

            return ident_start >= import_start;
        }

        false
    }
}
