//! Pure utility functions for code-fix handlers.
//!
//! Contains text parsing, identifier checking, interface/class analysis,
//! import specifier parsing, JSDoc import helpers, module path resolution,
//! wildcard matching, and import candidate ranking.
//!
//! Extracted from `handlers_code_fixes.rs` to reduce file size.

use tsz::lsp::code_actions::ImportCandidate;

pub(super) fn find_first_implements_class(content: &str) -> Option<(String, String, usize, usize)> {
    let mut cursor = 0usize;
    while let Some(rel_class) = content[cursor..].find("class ") {
        let class_start = cursor + rel_class;
        let class_name_start = class_start + "class ".len();
        let class_name = read_identifier(&content[class_name_start..])?;
        let class_body_open_rel = content[class_name_start..].find('{')?;
        let class_open_brace = class_name_start + class_body_open_rel;
        let header = &content[class_start..class_open_brace];

        if let Some(implements_idx) = header.find("implements ") {
            let interface_name_start = implements_idx + "implements ".len();
            let interface_name = read_identifier(&header[interface_name_start..])?;
            let class_close_brace = find_matching_brace(content, class_open_brace)?;
            return Some((
                class_name.to_string(),
                interface_name.to_string(),
                class_open_brace,
                class_close_brace,
            ));
        }

        cursor = class_name_start;
    }
    None
}

pub(super) fn parse_named_import_map(content: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("import ") {
            continue;
        }
        let Some(open_brace) = trimmed.find('{') else {
            continue;
        };
        let Some(close_brace_rel) = trimmed[open_brace + 1..].find('}') else {
            continue;
        };
        let close_brace = open_brace + 1 + close_brace_rel;
        let Some(from_idx) = trimmed[close_brace..].find("from") else {
            continue;
        };
        let from_segment = &trimmed[close_brace + from_idx + "from".len()..];
        let Some(module_specifier) = extract_quoted_text(from_segment) else {
            continue;
        };
        let imports = &trimmed[open_brace + 1..close_brace];
        for entry in imports.split(',') {
            let import_name = entry.trim().trim_start_matches("type ").trim();
            if import_name.is_empty() {
                continue;
            }
            if let Some((_, local)) = import_name.split_once(" as ") {
                let local_name = local.trim();
                if !local_name.is_empty() {
                    map.insert(local_name.to_string(), module_specifier.to_string());
                }
            } else {
                map.insert(import_name.to_string(), module_specifier.to_string());
            }
        }
    }
    map
}

pub(super) fn parse_interface_properties(
    content: &str,
    interface_name: &str,
) -> Option<Vec<(String, String)>> {
    let interface_token = format!("interface {interface_name}");
    let interface_pos = content.find(&interface_token)?;
    let open_brace_rel = content[interface_pos..].find('{')?;
    let open_brace = interface_pos + open_brace_rel;
    let close_brace = find_matching_brace(content, open_brace)?;
    let body = content.get(open_brace + 1..close_brace)?;

    let mut properties = Vec::new();
    for line in body.lines() {
        let member = line.trim().trim_end_matches(';');
        if member.is_empty() || member.starts_with("//") {
            continue;
        }
        let Some((lhs, rhs)) = member.split_once(':') else {
            continue;
        };
        let mut name = lhs.trim();
        if let Some(rest) = name.strip_prefix("readonly ") {
            name = rest.trim();
        }
        if let Some(rest) = name.strip_suffix('?') {
            name = rest.trim_end();
        }
        if !is_identifier(name) {
            continue;
        }
        properties.push((name.to_string(), rhs.trim().to_string()));
    }
    Some(properties)
}

pub(super) fn class_body_has_member(class_body: &str, member_name: &str) -> bool {
    for line in class_body.lines() {
        let mut trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("readonly ") {
            trimmed = rest.trim_start();
        }
        if let Some(rest) = trimmed.strip_prefix(member_name)
            && rest
                .chars()
                .next()
                .is_some_and(|ch| matches!(ch, ':' | '?' | '(' | '<' | ' '))
        {
            return true;
        }
    }
    false
}

pub(super) fn extract_type_identifiers(type_text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in type_text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
            current.push(ch);
        } else if !current.is_empty() {
            if is_identifier(&current) {
                out.push(current.clone());
            }
            current.clear();
        }
    }
    if !current.is_empty() && is_identifier(&current) {
        out.push(current);
    }
    out
}

pub(super) fn should_import_identifier(ident: &str) -> bool {
    if ident.is_empty() {
        return false;
    }
    if ident
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase())
    {
        return false;
    }
    !matches!(
        ident,
        "Array"
            | "ArrayBuffer"
            | "Boolean"
            | "Date"
            | "Error"
            | "Function"
            | "Number"
            | "Object"
            | "Promise"
            | "ReadonlyArray"
            | "RegExp"
            | "String"
            | "Symbol"
            | "Uint8Array"
    )
}

pub(super) fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
}

pub(super) fn extract_quoted_text(text: &str) -> Option<&str> {
    let quote_idx = text.find(['"', '\''])?;
    let quote = text.as_bytes()[quote_idx] as char;
    let rest = &text[quote_idx + 1..];
    let end_rel = rest.find(quote)?;
    Some(&rest[..end_rel])
}

fn read_identifier(text: &str) -> Option<&str> {
    let trimmed = text.trim_start();
    let start_offset = text.len() - trimmed.len();
    let mut chars = trimmed.char_indices();
    let (_, first) = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return None;
    }
    let mut end = first.len_utf8();
    for (idx, ch) in chars {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    Some(&text[start_offset..start_offset + end])
}

pub(super) fn find_matching_brace(content: &str, open_brace: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in content[open_brace..].char_indices() {
        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(open_brace + idx);
            }
        }
    }
    None
}

pub(super) fn resolve_module_path(
    from_file_path: &str,
    module_specifier: &str,
    files: &rustc_hash::FxHashMap<String, String>,
) -> Option<String> {
    if !module_specifier.starts_with('.') {
        return files
            .keys()
            .find(|path| {
                path.ends_with(module_specifier)
                    || path.trim_start_matches('/').ends_with(module_specifier)
            })
            .cloned();
    }

    for candidate in relative_module_path_candidates(from_file_path, module_specifier) {
        if let Some(key) = find_virtual_file_key(files, &candidate) {
            return Some(key);
        }
        if std::path::Path::new(&candidate).exists() {
            return Some(candidate);
        }
    }
    None
}

pub(super) fn relative_module_path_candidates(
    from_file_path: &str,
    module_specifier: &str,
) -> Vec<String> {
    let Some(base_dir) = std::path::Path::new(from_file_path).parent() else {
        return Vec::new();
    };
    let joined = normalize_simple_path(base_dir.join(module_specifier));
    let joined_str = joined.to_string_lossy().replace('\\', "/");
    let has_ext = std::path::Path::new(&joined_str).extension().is_some();
    if has_ext {
        return vec![joined_str];
    }

    let mut candidates = Vec::new();
    for ext in ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"] {
        candidates.push(format!("{joined_str}.{ext}"));
    }
    for ext in ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"] {
        candidates.push(format!("{joined_str}/index.{ext}"));
    }
    candidates
}

fn find_virtual_file_key(
    files: &rustc_hash::FxHashMap<String, String>,
    candidate: &str,
) -> Option<String> {
    if files.contains_key(candidate) {
        return Some(candidate.to_string());
    }

    let normalize = |value: &str| {
        value
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_ascii_lowercase()
    };
    let candidate_norm = normalize(candidate);
    files
        .keys()
        .find(|key| normalize(key) == candidate_norm)
        .cloned()
}

fn normalize_simple_path(path: std::path::PathBuf) -> std::path::PathBuf {
    let mut out = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::RootDir
            | std::path::Component::Normal(_)
            | std::path::Component::Prefix(_) => out.push(component.as_os_str()),
        }
    }
    out
}

pub(super) fn is_path_excluded_with_patterns(path: &str, patterns: &[String]) -> bool {
    let normalized_path = path.replace('\\', "/");
    let trimmed = normalized_path.trim_start_matches('/');
    patterns.iter().any(|pattern| {
        let normalized_pattern = pattern.replace('\\', "/");
        let pattern_trimmed = normalized_pattern.trim_start_matches('/');
        wildcard_match(pattern_trimmed, trimmed)
            || wildcard_match(&normalized_pattern, &normalized_path)
            || trimmed.ends_with(pattern_trimmed)
    })
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut dp = vec![vec![false; t.len() + 1]; p.len() + 1];
    dp[0][0] = true;

    for i in 1..=p.len() {
        if p[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }
    for i in 1..=p.len() {
        for j in 1..=t.len() {
            if p[i - 1] == '*' {
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if p[i - 1] == t[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }

    dp[p.len()][t.len()]
}

#[derive(Clone)]
pub(super) struct ImportSpecifierEntry {
    pub(super) raw: String,
    pub(super) local_name: String,
    pub(super) is_type_only: bool,
}

pub(super) fn parse_named_import_line(
    line: &str,
) -> Option<(Vec<ImportSpecifierEntry>, String, char)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("import ") {
        return None;
    }
    let open_brace = trimmed.find('{')?;
    let close_brace_rel = trimmed[open_brace + 1..].find('}')?;
    let close_brace = open_brace + 1 + close_brace_rel;
    let import_segment = &trimmed[open_brace + 1..close_brace];
    let from_segment = &trimmed[close_brace + 1..];
    let module_specifier = extract_quoted_text(from_segment)?.to_string();
    let quote = from_segment.find('\'').map(|_| '\'').unwrap_or('"');

    let mut specs = Vec::new();
    for part in import_segment.split(',') {
        if let Some(spec) = parse_import_spec_entry(part) {
            specs.push(spec);
        }
    }
    Some((specs, module_specifier, quote))
}

pub(super) fn parse_inserted_import_spec(new_text: &str) -> Option<ImportSpecifierEntry> {
    let trimmed = new_text
        .trim()
        .trim_start_matches(',')
        .trim_end_matches(',')
        .trim();
    parse_import_spec_entry(trimmed)
}

fn parse_import_spec_entry(text: &str) -> Option<ImportSpecifierEntry> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let is_type_only = trimmed.starts_with("type ");
    let without_type = trimmed.trim_start_matches("type ").trim();
    let local_name = if let Some((_, local)) = without_type.split_once(" as ") {
        local.trim().to_string()
    } else {
        without_type.to_string()
    };
    if !is_identifier(&local_name) {
        return None;
    }
    Some(ImportSpecifierEntry {
        raw: trimmed.to_string(),
        local_name,
        is_type_only,
    })
}

pub(super) fn import_specs_are_sorted(
    specs: &[ImportSpecifierEntry],
    type_order: &str,
    ignore_case: bool,
) -> bool {
    specs.windows(2).all(|pair| {
        import_spec_sort_key(&pair[0], type_order, ignore_case)
            <= import_spec_sort_key(&pair[1], type_order, ignore_case)
    })
}

pub(super) fn import_spec_sort_key(
    spec: &ImportSpecifierEntry,
    type_order: &str,
    ignore_case: bool,
) -> (u8, String, u8, String) {
    let group = match type_order {
        "last" if spec.is_type_only => 1,
        "first" if !spec.is_type_only => 1,
        _ => 0,
    };
    let (folded, case_rank, original) = if ignore_case {
        let folded = spec.local_name.to_ascii_lowercase();
        let case_rank = if spec
            .local_name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
        {
            0
        } else {
            1
        };
        (folded, case_rank, String::new())
    } else {
        (spec.local_name.clone(), 0, String::new())
    };
    (group, folded, case_rank, original)
}

pub(super) const fn position_leq(
    a: tsz::lsp::position::Position,
    b: tsz::lsp::position::Position,
) -> bool {
    a.line < b.line || (a.line == b.line && a.character <= b.character)
}

pub(super) const fn positions_overlap(
    a_start: tsz::lsp::position::Position,
    a_end: tsz::lsp::position::Position,
    b_start: tsz::lsp::position::Position,
    b_end: tsz::lsp::position::Position,
) -> bool {
    position_leq(a_start, b_end) && position_leq(b_start, a_end)
}

pub(super) fn parse_bare_identifier_expression(line: &str) -> Option<(usize, &str)> {
    let trimmed_start = line.trim_start();
    let leading_ws = line.len().saturating_sub(trimmed_start.len());
    let trimmed = trimmed_start.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    let expr = trimmed.strip_suffix(';').unwrap_or(trimmed).trim_end();
    if expr.is_empty() {
        return None;
    }

    let mut chars = expr.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return None;
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$') {
        return None;
    }

    Some((leading_ws, expr))
}

pub(super) fn parse_identifier_call_expression(line: &str) -> Option<(usize, &str)> {
    let trimmed_start = line.trim_start();
    let leading_ws = line.len().saturating_sub(trimmed_start.len());
    let trimmed = trimmed_start.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    let expr = trimmed.strip_suffix(';').unwrap_or(trimmed).trim_end();
    if expr.is_empty() {
        return None;
    }

    let mut chars = expr.char_indices();
    let (_, first) = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return None;
    }

    let mut ident_end = first.len_utf8();
    for (idx, ch) in chars {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
            ident_end = idx + ch.len_utf8();
            continue;
        }
        ident_end = idx;
        break;
    }

    let name = expr.get(..ident_end)?;
    if !is_identifier(name) {
        return None;
    }
    if is_reserved_word(name) {
        return None;
    }

    let rest = expr.get(ident_end..)?.trim_start();
    if !rest.starts_with('(') {
        return None;
    }

    let mut depth = 0u32;
    let mut close_idx = None;
    for (idx, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    close_idx = Some(idx);
                    break;
                }
            }
            _ => {}
        }
    }
    let close_idx = close_idx?;
    let suffix = rest.get(close_idx + 1..)?.trim_start();
    if suffix.starts_with('{') || suffix.starts_with(':') {
        return None;
    }

    Some((leading_ws, name))
}

fn is_reserved_word(name: &str) -> bool {
    matches!(
        name,
        "if" | "else"
            | "for"
            | "while"
            | "do"
            | "switch"
            | "case"
            | "default"
            | "break"
            | "continue"
            | "return"
            | "throw"
            | "try"
            | "catch"
            | "finally"
            | "function"
            | "class"
            | "new"
            | "this"
            | "super"
            | "typeof"
            | "void"
            | "delete"
            | "await"
            | "yield"
    )
}

pub(super) fn find_jsdoc_import_line(
    content: &str,
) -> Option<(u32, String, String, String, Vec<ImportSpecifierEntry>)> {
    for (idx, line) in content.lines().enumerate() {
        let Some(at_import) = line.find("@import") else {
            continue;
        };
        let prefix = &line[..at_import];
        let import_part = &line[at_import + "@import".len()..];
        let open = import_part.find('{')?;
        let close = import_part[open + 1..].find('}')?;
        let spec_end = open + 1 + close;
        let specs_text = import_part[open + 1..spec_end].trim();
        let after_specs = &import_part[spec_end + 1..];
        let Some(from_idx) = after_specs.find("from") else {
            continue;
        };
        let module_text = after_specs[from_idx + "from".len()..].trim();
        let Some(module_specifier) = extract_quoted_text(module_text) else {
            continue;
        };

        let specs = specs_text
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| ImportSpecifierEntry {
                raw: s.to_string(),
                local_name: s
                    .split_once(" as ")
                    .map_or_else(|| s.to_string(), |(_, local)| local.trim().to_string()),
                is_type_only: s.starts_with("type "),
            })
            .collect::<Vec<_>>();
        if specs.is_empty() {
            continue;
        }
        return Some((
            idx as u32 + 1,
            line.to_string(),
            prefix.to_string(),
            module_specifier.to_string(),
            specs,
        ));
    }
    None
}

pub(super) fn extract_jsdoc_imported_names(content: &str) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.contains("@import") {
            continue;
        }
        let Some(open) = trimmed.find('{') else {
            continue;
        };
        let Some(close_rel) = trimmed[open + 1..].find('}') else {
            continue;
        };
        let close = open + 1 + close_rel;
        for raw_spec in trimmed[open + 1..close].split(',') {
            let imported = raw_spec.split_whitespace().next().unwrap_or_default();
            if !imported.is_empty() {
                names.insert(imported.to_string());
            }
        }
    }
    names
}

pub(super) fn extract_jsdoc_type_identifier_spans(line: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let Some(open) = line.find('{') else {
        return out;
    };
    let Some(close_rel) = line[open + 1..].find('}') else {
        return out;
    };
    let close = open + 1 + close_rel;
    let type_text = &line[open + 1..close];
    let bytes = type_text.as_bytes();
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
        let Some(name) = type_text.get(start..i) else {
            continue;
        };
        if !name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) || !is_identifier(name) {
            continue;
        }
        out.push((name.to_string(), open + 1 + start));
    }
    out
}

fn is_same_import_candidate_symbol(a: &ImportCandidate, b: &ImportCandidate) -> bool {
    if a.local_name != b.local_name || a.is_type_only != b.is_type_only {
        return false;
    }
    match (&a.kind, &b.kind) {
        (
            tsz::lsp::code_actions::ImportCandidateKind::Named {
                export_name: a_export_name,
            },
            tsz::lsp::code_actions::ImportCandidateKind::Named {
                export_name: b_export_name,
            },
        ) => a_export_name == b_export_name,
        (
            tsz::lsp::code_actions::ImportCandidateKind::Default,
            tsz::lsp::code_actions::ImportCandidateKind::Default,
        )
        | (
            tsz::lsp::code_actions::ImportCandidateKind::Namespace,
            tsz::lsp::code_actions::ImportCandidateKind::Namespace,
        ) => true,
        _ => false,
    }
}

fn prefers_package_root_specifier(a: &ImportCandidate, b: &ImportCandidate) -> bool {
    if !is_same_import_candidate_symbol(a, b) {
        return false;
    }
    if a.module_specifier.starts_with('.') || b.module_specifier.starts_with('.') {
        return false;
    }
    if a.module_specifier == b.module_specifier {
        return false;
    }
    let Some(rest) = b.module_specifier.strip_prefix(&a.module_specifier) else {
        return false;
    };
    rest.starts_with('/')
}

fn relative_specifier_rank(specifier: &str) -> (usize, usize, usize) {
    let depth = specifier.matches('/').count();
    let index_penalty = usize::from(
        specifier == "."
            || specifier == ".."
            || specifier.ends_with("/index")
            || specifier.ends_with("/index.ts")
            || specifier.ends_with("/index.js"),
    );
    (depth, index_penalty, specifier.len())
}

fn prefers_shallower_relative_specifier(a: &ImportCandidate, b: &ImportCandidate) -> bool {
    if !is_same_import_candidate_symbol(a, b) {
        return false;
    }
    if !a.module_specifier.starts_with('.') || !b.module_specifier.starts_with('.') {
        return false;
    }
    if a.module_specifier == b.module_specifier {
        return false;
    }
    relative_specifier_rank(&a.module_specifier) < relative_specifier_rank(&b.module_specifier)
}

pub(super) fn reorder_import_candidates_for_package_roots(candidates: &mut [ImportCandidate]) {
    // Keep the original discovery order unless a package root/module-subpath pair
    // targets the same symbol, in which case tsserver prefers the shallower path.
    for i in 0..candidates.len() {
        for j in (i + 1)..candidates.len() {
            if prefers_package_root_specifier(&candidates[j], &candidates[i])
                || prefers_shallower_relative_specifier(&candidates[j], &candidates[i])
            {
                candidates.swap(i, j);
            }
        }
    }
}
