//! `@template` constraint/name extraction, split out of `parsing.rs` to keep
//! that file under the checker line-count boundary. Pure string-level
//! parsing (no `&self`/`&mut self`), same as its former home.

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    pub(super) fn jsdoc_template_constraints(jsdoc: &str) -> Vec<(String, Option<String>)> {
        let mut out = Vec::new();
        for raw_line in jsdoc.lines() {
            let trimmed = raw_line.trim().trim_start_matches('*').trim();
            let Some(rest) = Self::strip_jsdoc_tag_prefix(trimmed, "template") else {
                continue;
            };
            let rest = rest.trim();
            let (constraint, names_str) = if let Some(rest) = rest.strip_prefix('{') {
                let mut depth = 1usize;
                let mut close_idx = None;
                for (idx, ch) in rest.char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                close_idx = Some(idx);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(close_idx) = close_idx {
                    (
                        Some(rest[..close_idx].trim().to_string()),
                        rest[close_idx + 1..].trim(),
                    )
                } else {
                    (None, rest)
                }
            } else {
                (None, rest)
            };
            let mut cursor = 0usize;
            let bytes = names_str.as_bytes();
            let mut parsed_any = false;
            let mut applied_constraint = false;
            while cursor < bytes.len() {
                let mut saw_comma = false;
                while cursor < bytes.len() {
                    let ch = bytes[cursor] as char;
                    if ch == ',' {
                        saw_comma = true;
                        cursor += 1;
                    } else if ch.is_ascii_whitespace() {
                        cursor += 1;
                    } else {
                        break;
                    }
                }
                if cursor >= bytes.len() {
                    break;
                }

                // Bracket-default form `[T=default]`: skip the leading bracket
                // so the identifier scan sees the name, mirroring
                // `jsdoc_template_type_params`. Without this, a combined
                // `@template {C} [T=default]` drops the constraint because no
                // name is produced for the brace clause to bind to.
                let in_bracket = bytes[cursor] as char == '[';
                if in_bracket {
                    cursor += 1;
                    while cursor < bytes.len() && (bytes[cursor] as char).is_ascii_whitespace() {
                        cursor += 1;
                    }
                }

                let start = cursor;
                while cursor < bytes.len() {
                    let ch = bytes[cursor] as char;
                    if ch == '_' || ch == '$' || ch.is_ascii_alphanumeric() {
                        cursor += 1;
                    } else {
                        break;
                    }
                }
                if start == cursor {
                    break;
                }

                let name = &names_str[start..cursor];

                // Consume the remainder of a bracket-default form (`=default]`)
                // so the next iteration resumes at the following name.
                if in_bracket {
                    while cursor < bytes.len() && bytes[cursor] as char != ']' {
                        cursor += 1;
                    }
                    if cursor < bytes.len() {
                        cursor += 1;
                    }
                }

                // Skip variance and never-valid modifier keywords (`const`,
                // `in`, `out`, `private`, `static`, ...): tsc still registers
                // the real name, reporting only TS1273/TS1274 for the
                // modifier. See `NEVER_VALID_JSDOC_TEMPLATE_MODIFIERS` and
                // `jsdoc_template_type_params`'s mirrored skip.
                if name == "const"
                    || name == "in"
                    || name == "out"
                    || super::diagnostics_templates::NEVER_VALID_JSDOC_TEMPLATE_MODIFIERS
                        .contains(&name)
                {
                    continue;
                }

                if parsed_any && !saw_comma {
                    break;
                }

                let mut lookahead = cursor;
                while lookahead < bytes.len() && (bytes[lookahead] as char).is_ascii_whitespace() {
                    lookahead += 1;
                }

                if parsed_any
                    && !saw_comma
                    && lookahead < bytes.len()
                    && bytes[lookahead] as char != ','
                {
                    break;
                }

                let name_constraint = if applied_constraint {
                    None
                } else {
                    constraint.clone()
                };
                out.push((name.to_string(), name_constraint));
                parsed_any = true;
                applied_constraint = true;
                cursor = lookahead;
            }
        }
        out
    }

    pub(super) fn jsdoc_template_constraints_before_typedef_host(
        jsdoc: &str,
    ) -> Vec<(String, Option<String>)> {
        // tsc accepts @template tags AFTER a single @typedef in the same
        // JSDoc comment (templates bind to the typedef host). It does NOT
        // extend that grace to @callback or @overload — TS8039 fires for
        // misplaced templates after those tags. Match that policy: when the
        // only host in the block is a single @typedef, scan the whole block
        // for @template; otherwise restrict to the prefix before any host.
        let normalize = |raw: &str| -> String {
            raw.trim()
                .trim_start_matches("/**")
                .trim_start_matches("/*")
                .trim_start_matches('*')
                .trim()
                .trim_end_matches("*/")
                .trim()
                .to_string()
        };
        let mut typedef_count = 0usize;
        let mut other_host_count = 0usize;
        for raw_line in jsdoc.lines() {
            let trimmed = normalize(raw_line);
            if Self::jsdoc_line_starts_with_tag(&trimmed, "typedef") {
                typedef_count += 1;
            } else if Self::jsdoc_line_starts_with_tag(&trimmed, "callback")
                || Self::jsdoc_line_starts_with_tag(&trimmed, "overload")
            {
                other_host_count += 1;
            }
        }
        if typedef_count == 1 && other_host_count == 0 {
            return Self::jsdoc_template_constraints(jsdoc);
        }
        let mut prefix = String::new();
        for raw_line in jsdoc.lines() {
            let trimmed = normalize(raw_line);
            if Self::jsdoc_line_starts_with_tag(&trimmed, "typedef")
                || Self::jsdoc_line_starts_with_tag(&trimmed, "callback")
                || Self::jsdoc_line_starts_with_tag(&trimmed, "overload")
            {
                break;
            }
            prefix.push_str(raw_line);
            prefix.push('\n');
        }
        Self::jsdoc_template_constraints(&prefix)
    }
}
