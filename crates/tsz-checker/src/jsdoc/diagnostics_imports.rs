//! JSDoc `@import` diagnostic helpers for `CheckerState`.

use crate::state::CheckerState;
use tsz_parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// TS2300: Check for duplicate `@import` names across JSDoc comments and
    /// against runtime ES imports in the same file.
    ///
    /// When the same name is imported via `@import` in multiple JSDoc comments
    /// (or via a JSDoc `@import` and a runtime `import` declaration), tsc emits
    /// TS2300 "Duplicate identifier 'X'" at each occurrence.
    pub(crate) fn check_jsdoc_duplicate_imports(&mut self) {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: String = sf.text.to_string();
        let comments = sf.comments.clone();
        let statements: Vec<NodeIndex> = sf.statements.nodes.clone();

        // (name, abs_pos, len, is_jsdoc)
        let mut import_positions: Vec<(String, u32, u32, bool)> = Vec::new();

        // Collect JSDoc `@import` names and positions.
        for comment in &comments {
            if !is_jsdoc_comment(comment, &source_text) {
                continue;
            }
            let comment_text =
                &source_text[comment.pos as usize..(comment.end as usize).min(source_text.len())];
            let content = get_jsdoc_content(comment, &source_text);

            for line in content.lines() {
                let trimmed = line.trim_start_matches('*').trim();
                if let Some(rest) = Self::strip_jsdoc_tag_prefix(trimmed, "import") {
                    let imports = Self::parse_jsdoc_import_tag(rest);
                    for (local_name, _specifier, _import_name) in imports {
                        if let Some(name_offset) =
                            Self::find_import_name_in_comment(comment_text, &local_name)
                        {
                            let abs_pos = comment.pos + name_offset as u32;
                            let len = local_name.len() as u32;
                            import_positions.push((local_name, abs_pos, len, true));
                        }
                    }
                }
            }
        }

        // Collect runtime ES import names and positions from top-level statements.
        for stmt_idx in &statements {
            self.collect_runtime_import_positions(*stmt_idx, &mut import_positions);
        }

        let mut seen: std::collections::HashMap<String, Vec<(u32, u32, bool)>> =
            std::collections::HashMap::new();
        for (name, pos, len, is_jsdoc) in &import_positions {
            seen.entry(name.clone())
                .or_default()
                .push((*pos, *len, *is_jsdoc));
        }

        for (name, positions) in &seen {
            if positions.len() < 2 {
                continue;
            }
            // Only emit TS2300 here if at least one occurrence is a JSDoc
            // `@import`. Pure runtime/runtime collisions are handled by the
            // symbol-based duplicate identifier pass; emitting here too would
            // double-report TS2300.
            let has_jsdoc = positions.iter().any(|&(_, _, is_jsdoc)| is_jsdoc);
            if !has_jsdoc {
                continue;
            }
            use crate::diagnostics::{diagnostic_codes, format_message};
            let message = format_message("Duplicate identifier '{0}'.", &[name]);
            for &(pos, len, _) in positions {
                self.error_at_position(pos, len, &message, diagnostic_codes::DUPLICATE_IDENTIFIER);
            }
        }
    }

    /// Walks an import declaration node and pushes the local name and span of
    /// every imported binding (default, namespace, named) onto `out`. Marks
    /// each entry as runtime (not JSDoc).
    fn collect_runtime_import_positions(
        &self,
        stmt_idx: NodeIndex,
        out: &mut Vec<(String, u32, u32, bool)>,
    ) {
        let Some(stmt) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        if stmt.kind != syntax_kind_ext::IMPORT_DECLARATION {
            return;
        }
        let Some(import) = self.ctx.arena.get_import_decl(stmt) else {
            return;
        };
        let Some(clause_node) = self.ctx.arena.get(import.import_clause) else {
            return;
        };
        let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
            return;
        };

        // Default import: `import X from "mod"`.
        if clause.name.is_some() {
            self.push_identifier_position(clause.name, out);
        }

        // Namespace + named imports.
        if clause.named_bindings.is_some()
            && let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings)
            && let Some(named) = self.ctx.arena.get_named_imports(bindings_node)
        {
            // Namespace import: `import * as ns from "mod"`.
            if named.name.is_some() {
                self.push_identifier_position(named.name, out);
            }
            // Named imports: `import { a, b as c } from "mod"`.
            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = self.ctx.arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = self.ctx.arena.get_specifier(spec_node) else {
                    continue;
                };
                self.push_identifier_position(spec.name, out);
            }
        }
    }

    fn push_identifier_position(
        &self,
        name_idx: NodeIndex,
        out: &mut Vec<(String, u32, u32, bool)>,
    ) {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };
        let Some(name) = self.get_identifier_text(name_node) else {
            return;
        };
        let pos = name_node.pos;
        let len = name.len() as u32;
        out.push((name, pos, len, false));
    }

    /// Find the position of an import name within a JSDoc comment text.
    /// Returns the byte offset from the start of the comment.
    fn find_import_name_in_comment(comment_text: &str, name: &str) -> Option<usize> {
        let import_idx = Self::jsdoc_tag_offset(comment_text, "import")?;
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
