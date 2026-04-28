//! JSDoc `@import` diagnostic helpers for `CheckerState`.

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    /// TS2300: Check for duplicate `@import` names across JSDoc comments.
    ///
    /// When the same name is imported via `@import` in multiple JSDoc comments,
    /// tsc emits TS2300 "Duplicate identifier 'X'" at each occurrence.
    pub(crate) fn check_jsdoc_duplicate_imports(&mut self) {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: String = sf.text.to_string();
        let comments = sf.comments.clone();
        let mut import_names: Vec<(String, u32, u32)> = Vec::new();

        for comment in &comments {
            if !is_jsdoc_comment(comment, &source_text) {
                continue;
            }
            let comment_text =
                &source_text[comment.pos as usize..(comment.end as usize).min(source_text.len())];
            let content = get_jsdoc_content(comment, &source_text);

            for line in content.lines() {
                let trimmed = line.trim_start_matches('*').trim();
                if let Some(rest) = trimmed.strip_prefix("@import") {
                    let imports = Self::parse_jsdoc_import_tag(rest);
                    for (local_name, _specifier, _import_name) in imports {
                        if let Some(name_offset) =
                            Self::find_import_name_in_comment(comment_text, &local_name)
                        {
                            let abs_pos = comment.pos + name_offset as u32;
                            import_names.push((local_name, abs_pos, 0));
                        }
                    }
                }
            }
        }

        let mut seen: std::collections::HashMap<String, Vec<(u32, u32)>> =
            std::collections::HashMap::new();
        for (name, pos, _) in &import_names {
            seen.entry(name.clone())
                .or_default()
                .push((*pos, name.len() as u32));
        }

        for (name, positions) in &seen {
            if positions.len() > 1 {
                use crate::diagnostics::{diagnostic_codes, format_message};
                let message = format_message("Duplicate identifier '{0}'.", &[name]);
                for &(pos, len) in positions {
                    self.error_at_position(
                        pos,
                        len,
                        &message,
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                    );
                }
            }
        }
    }

    /// Find the position of an import name within a JSDoc comment text.
    /// Returns the byte offset from the start of the comment.
    fn find_import_name_in_comment(comment_text: &str, name: &str) -> Option<usize> {
        let import_idx = comment_text.find("@import")?;
        let after_import = import_idx + "@import".len();
        let rest = &comment_text[after_import..];

        if let Some(brace_pos) = rest.find('{') {
            let after_brace = &rest[brace_pos + 1..];
            if let Some(name_offset) = after_brace.find(name) {
                let before_ok = name_offset == 0
                    || !after_brace.as_bytes()[name_offset - 1].is_ascii_alphanumeric();
                let after_ok = name_offset + name.len() >= after_brace.len()
                    || !after_brace.as_bytes()[name_offset + name.len()].is_ascii_alphanumeric();
                if before_ok && after_ok {
                    return Some(after_import + brace_pos + 1 + name_offset);
                }
            }
        }

        if let Some(name_offset) = rest.find(name) {
            return Some(after_import + name_offset);
        }

        None
    }
}
