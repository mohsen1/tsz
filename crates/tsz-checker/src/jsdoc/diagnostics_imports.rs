//! JSDoc `@import` diagnostic helpers for `CheckerState`.

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    /// TS2300: Check for duplicate `@import` names within JSDoc comments,
    /// and across runtime ES `import { name }` / JSDoc `@import { name }`
    /// occurrences in the same JS file.
    ///
    /// When the same name is imported via `@import` in multiple JSDoc
    /// comments, tsc emits TS2300 "Duplicate identifier 'X'" at each
    /// occurrence. tsc additionally emits TS2300 at every position when
    /// a runtime ES `import { X }` and a JSDoc `@import { X }` declare the
    /// same local name in the same file (issue #3508).
    pub(crate) fn check_jsdoc_duplicate_imports(&mut self) {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: String = sf.text.to_string();
        let comments = sf.comments.clone();

        // Collected positions for each local name. `is_jsdoc` records whether
        // any of the positions came from a JSDoc `@import` (the trigger for
        // emitting TS2300 in the runtime-vs-JSDoc case).
        let mut positions: std::collections::HashMap<String, Vec<(u32, u32)>> =
            std::collections::HashMap::new();
        let mut has_jsdoc_origin: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        // 1) Collect JSDoc `@import` positions.
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
                            positions
                                .entry(local_name.clone())
                                .or_default()
                                .push((abs_pos, len));
                            has_jsdoc_origin.insert(local_name);
                        }
                    }
                }
            }
        }

        // 2) Collect runtime ES `import { name }` (and renamed
        //    `import { exported as local }`) local-name positions. Only the
        //    local-binding position is added; this matches tsc's TS2300
        //    span ("Duplicate identifier 'Foo'." anchors on the local name
        //    introduced by the import, not on the property_name).
        Self::collect_runtime_import_positions(&self.ctx.arena, &mut positions);

        // 3) Emit TS2300 at every position when the name has more than one
        //    occurrence AND at least one of those occurrences is a JSDoc
        //    `@import`. Pure runtime/runtime collisions are intentionally
        //    left to the symbol-based duplicate-identifier pass so we don't
        //    double-report them.
        for (name, occurrences) in &positions {
            if occurrences.len() <= 1 {
                continue;
            }
            if !has_jsdoc_origin.contains(name) {
                continue;
            }
            use crate::diagnostics::{diagnostic_codes, format_message};
            let message = format_message("Duplicate identifier '{0}'.", &[name]);
            for &(pos, len) in occurrences {
                self.error_at_position(pos, len, &message, diagnostic_codes::DUPLICATE_IDENTIFIER);
            }
        }
    }

    /// Walk the arena's `IMPORT_DECLARATION` nodes and append local-binding
    /// positions for each named/default import into `positions`.
    fn collect_runtime_import_positions(
        arena: &tsz_parser::parser::node::NodeArena,
        positions: &mut std::collections::HashMap<String, Vec<(u32, u32)>>,
    ) {
        use tsz_parser::syntax_kind_ext::{
            IMPORT_CLAUSE, IMPORT_DECLARATION, NAMED_IMPORTS, NAMESPACE_IMPORT,
        };

        for node in arena.nodes.iter() {
            if node.kind != IMPORT_DECLARATION {
                continue;
            }
            let Some(import_decl) = arena.get_import_decl(node) else {
                continue;
            };
            let Some(clause_node) = arena.get(import_decl.import_clause) else {
                continue;
            };
            if clause_node.kind != IMPORT_CLAUSE {
                continue;
            }
            let Some(clause) = arena.get_import_clause(clause_node) else {
                continue;
            };

            // Default binding: `import Foo from "./mod"` — the local name is
            // `clause.name`.
            if !clause.name.is_none()
                && let Some(name_node) = arena.get(clause.name)
                && let Some(ident) = arena.get_identifier(name_node)
            {
                let len = ident.escaped_text.len() as u32;
                positions
                    .entry(ident.escaped_text.clone())
                    .or_default()
                    .push((name_node.pos, len));
            }

            if clause.named_bindings.is_none() {
                continue;
            }
            let Some(nb_node) = arena.get(clause.named_bindings) else {
                continue;
            };
            // Namespace binding: `import * as ns from "./mod"` — the local
            // name is `ns_data.name`.
            if nb_node.kind == NAMESPACE_IMPORT
                && let Some(ns_data) = arena.get_named_imports(nb_node)
                && !ns_data.name.is_none()
                && let Some(name_node) = arena.get(ns_data.name)
                && let Some(ident) = arena.get_identifier(name_node)
            {
                let len = ident.escaped_text.len() as u32;
                positions
                    .entry(ident.escaped_text.clone())
                    .or_default()
                    .push((name_node.pos, len));
                continue;
            }
            if nb_node.kind == NAMED_IMPORTS
                && let Some(named) = arena.get_named_imports(nb_node)
            {
                for &spec_idx in &named.elements.nodes {
                    let Some(spec_node) = arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(spec) = arena.get_specifier(spec_node) else {
                        continue;
                    };
                    // Local-binding name is `spec.name` (the right side of
                    // `as`, or the same as the imported name when no alias).
                    let Some(name_node) = arena.get(spec.name) else {
                        continue;
                    };
                    let Some(ident) = arena.get_identifier(name_node) else {
                        continue;
                    };
                    let len = ident.escaped_text.len() as u32;
                    positions
                        .entry(ident.escaped_text.clone())
                        .or_default()
                        .push((name_node.pos, len));
                }
            }
        }
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
