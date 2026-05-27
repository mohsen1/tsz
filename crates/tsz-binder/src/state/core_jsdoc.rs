//! JSDoc `@import` tag parsing helpers for `BinderState`.

use super::BinderState;
use crate::symbol_flags;
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

impl BinderState {
    pub(super) fn parse_jsdoc_import_tag(rest: &str) -> Vec<(String, String, String)> {
        let rest = rest.trim();
        let mut results = Vec::new();
        if let Some(from_idx) = Self::find_jsdoc_import_from_keyword(rest) {
            let before_from = rest[..from_idx].trim();
            if matches!(
                before_from.split_whitespace().next(),
                Some("type" | "defer")
            ) && before_from.contains(char::is_whitespace)
            {
                return results;
            }
            let after_from = rest[from_idx + 4..].trim();
            let quote = after_from.chars().next().unwrap_or(' ');
            if quote == '"' || quote == '\'' || quote == '`' {
                let specifier = after_from[1..]
                    .split(quote)
                    .next()
                    .unwrap_or("")
                    .to_string();
                if before_from.starts_with('{') && before_from.ends_with('}') {
                    let inner = &before_from[1..before_from.len() - 1];
                    for part in Self::split_jsdoc_import_clause_items(inner) {
                        let part = part.trim();
                        if part.is_empty() {
                            continue;
                        }
                        if let Some((imported_name, local_name)) =
                            Self::split_jsdoc_import_as_keyword(part)
                        {
                            let imported_name = Self::normalize_jsdoc_import_name(imported_name);
                            results.push((
                                local_name.to_string(),
                                specifier.clone(),
                                imported_name,
                            ));
                        } else {
                            let imported_name = Self::normalize_jsdoc_import_name(part);
                            results.push((imported_name.clone(), specifier.clone(), imported_name));
                        }
                    }
                } else if let Some(("*", ns_name)) =
                    Self::split_jsdoc_import_as_keyword(before_from)
                {
                    let ns_name = ns_name.to_string();
                    if !ns_name.is_empty() {
                        results.push((ns_name, specifier, "*".to_string()));
                    }
                } else {
                    let default_name = before_from.to_string();
                    if !default_name.is_empty() {
                        results.push((default_name, specifier, "default".to_string()));
                    }
                }
            }
        }
        results
    }

    pub(super) fn find_jsdoc_import_from_keyword(rest: &str) -> Option<usize> {
        let mut quote = None;
        let mut escaped = false;
        let mut last_from = None;

        for (idx, ch) in rest.char_indices() {
            if let Some(active_quote) = quote {
                if escaped {
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == active_quote {
                    quote = None;
                }
                continue;
            }

            if ch == '"' || ch == '\'' || ch == '`' {
                quote = Some(ch);
                continue;
            }

            if rest[idx..].starts_with("from")
                && !rest[..idx]
                    .chars()
                    .next_back()
                    .is_some_and(Self::is_jsdoc_import_keyword_part)
                && !rest[idx + 4..]
                    .chars()
                    .next()
                    .is_some_and(Self::is_jsdoc_import_keyword_part)
            {
                last_from = Some(idx);
            }
        }

        last_from
    }

    pub(super) fn split_jsdoc_import_clause_items(s: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut angle_depth = 0u32;
        let mut paren_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut square_depth = 0u32;
        let mut quote = None;
        let mut escaped = false;

        for (idx, ch) in s.char_indices() {
            if let Some(active_quote) = quote {
                if escaped {
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == active_quote {
                    quote = None;
                }
                continue;
            }

            if ch == '"' || ch == '\'' || ch == '`' {
                quote = Some(ch);
                continue;
            }

            match ch {
                '<' => angle_depth += 1,
                '>' if angle_depth > 0 => angle_depth -= 1,
                '(' => paren_depth += 1,
                ')' if paren_depth > 0 => paren_depth -= 1,
                '{' => brace_depth += 1,
                '}' if brace_depth > 0 => brace_depth -= 1,
                '[' => square_depth += 1,
                ']' if square_depth > 0 => square_depth -= 1,
                ',' if angle_depth == 0
                    && paren_depth == 0
                    && brace_depth == 0
                    && square_depth == 0 =>
                {
                    parts.push(&s[start..idx]);
                    start = idx + 1;
                }
                _ => {}
            }
        }

        if start < s.len() {
            parts.push(&s[start..]);
        }
        parts
    }

    pub(super) fn split_jsdoc_import_as_keyword(part: &str) -> Option<(&str, &str)> {
        let mut quote = None;
        let mut escaped = false;
        for (idx, ch) in part.char_indices() {
            if let Some(active_quote) = quote {
                if escaped {
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == active_quote {
                    quote = None;
                }
                continue;
            }

            if ch == '"' || ch == '\'' || ch == '`' {
                quote = Some(ch);
                continue;
            }

            if !part[idx..].starts_with("as") {
                continue;
            }

            let before_ok = part[..idx]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
            let after_idx = idx + 2;
            let after_ok = part[after_idx..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace);
            if before_ok && after_ok {
                let imported = part[..idx].trim();
                let local = part[after_idx..].trim();
                if !imported.is_empty() && !local.is_empty() {
                    return Some((imported, local));
                }
            }
        }
        None
    }

    pub(super) fn normalize_jsdoc_import_name(name: &str) -> String {
        Self::parse_jsdoc_string_literal(name).unwrap_or_else(|| name.trim().to_string())
    }

    pub(super) fn parse_jsdoc_string_literal(text: &str) -> Option<String> {
        let text = text.trim();
        let quote = text.chars().next()?;
        if quote != '"' && quote != '\'' && quote != '`' {
            return None;
        }

        let mut value = String::new();
        let mut escaped = false;
        let mut close_end = None;
        for (idx, ch) in text[quote.len_utf8()..].char_indices() {
            if escaped {
                value.push(match ch {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    _ => ch,
                });
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == quote {
                close_end = Some(quote.len_utf8() + idx + ch.len_utf8());
                break;
            }
            value.push(ch);
        }

        let close_end = close_end?;
        if !text[close_end..].trim().is_empty() {
            return None;
        }
        Some(value)
    }

    pub(super) const fn is_jsdoc_import_keyword_part(ch: char) -> bool {
        ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
    }

    /// Collapse `@import` tag continuations onto a single line so the
    /// line-by-line binder can register the alias.
    ///
    /// JSDoc allows the import clause to be split over several lines, e.g.
    ///
    /// ```text
    /// /**
    ///  * @import
    ///  * * as types
    ///  * from "./types"
    ///  */
    /// ```
    ///
    /// Without this preprocessing, the binder sees `@import` followed by an
    /// empty rest and silently fails to register the namespace alias —
    /// every later `types.A` reference then fires TS2304.
    pub(super) fn merge_jsdoc_import_continuations(jsdoc: &str) -> String {
        // Note: this preprocessor runs over JSDoc text that has already been
        // normalized by `get_jsdoc_content`, so the outer `* ` decoration is
        // gone. We MUST NOT strip any further leading `*`s, since
        // `import * as types` puts a real `*` at the start of the
        // continuation line — accidentally consuming it would turn the line
        // into `as types` and erase the namespace import.
        let mut result = String::with_capacity(jsdoc.len());
        let mut pending: Option<String> = None;
        for line in jsdoc.lines() {
            let stripped = line.trim();
            if let Some(text) = pending.as_mut() {
                let already_complete = text.contains(" from \"")
                    || text.contains(" from '")
                    || text.contains(" from `");
                if !already_complete && !stripped.is_empty() && !stripped.starts_with('@') {
                    text.push(' ');
                    text.push_str(stripped);
                    continue;
                }
                let flushed = pending
                    .take()
                    .expect("pending import text is present while flushing continuation");
                result.push_str(&flushed);
                result.push('\n');
            }
            if stripped.strip_prefix("@import").is_some_and(|rest| {
                !rest
                    .chars()
                    .next()
                    .is_some_and(Self::is_jsdoc_import_keyword_part)
            }) {
                pending = Some(line.to_string());
                continue;
            }
            result.push_str(line);
            result.push('\n');
        }
        if let Some(text) = pending {
            result.push_str(&text);
            result.push('\n');
        }
        result
    }

    pub(super) fn bind_jsdoc_import_tags(
        &mut self,
        arena: &NodeArena,
        source_file: &tsz_parser::parser::node::SourceFileData,
        root: NodeIndex,
    ) {
        if source_file.comments.is_empty()
            || source_file.is_declaration_file
            || !super::core::is_js_like_file_name(&source_file.file_name)
        {
            return;
        }

        let source_text = source_file.text.as_ref();
        for comment in &source_file.comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }
            let raw_content = get_jsdoc_content(comment, source_text);
            let merged_content = Self::merge_jsdoc_import_continuations(raw_content.as_ref());
            let content: &str = merged_content.as_str();
            for line in content.lines() {
                let trimmed = line.trim_start_matches('*').trim();
                let Some(rest) = trimmed.strip_prefix("@import") else {
                    continue;
                };
                // Tag-boundary check: `@importx { Foo } from "./x"` must not
                // be parsed as `@import`. Issue #2916.
                if rest
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    continue;
                }
                let has_attributes = rest.contains(" with ");
                for (local_name, specifier, import_name) in Self::parse_jsdoc_import_tag(rest) {
                    if local_name.is_empty() || specifier.is_empty() {
                        continue;
                    }
                    self.file_import_sources.push(specifier.clone());
                    if has_attributes {
                        continue;
                    }
                    // Do not redeclare a name that already has an alias in this
                    // scope (runtime ES import or earlier JSDoc `@import`). The
                    // type-only JSDoc binding must not override the existing
                    // alias's `is_type_only`, otherwise the runtime import is
                    // misreported as a type-only JS import (TS18042). The
                    // resulting duplicate identifier is reported as TS2300 by
                    // the JSDoc duplicate-import check in the checker.
                    let already_aliased =
                        self.current_scope.get(&local_name).is_some_and(|existing| {
                            self.symbols
                                .get(existing)
                                .is_some_and(|sym| (sym.flags & symbol_flags::ALIAS) != 0)
                        });
                    if already_aliased {
                        continue;
                    }
                    let sym_id =
                        self.declare_symbol(arena, &local_name, symbol_flags::ALIAS, root, false);
                    if let Some(sym) = self.symbols.get_mut(sym_id) {
                        // JSDoc @import bindings are type-only aliases that target a module member.
                        sym.is_type_only = true;
                        sym.import_module = Some(specifier.clone());
                        sym.import_name = Some(import_name);
                    }
                }
            }
        }
    }
}
