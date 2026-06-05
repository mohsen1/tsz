//! `JSDoc` formatting helpers for hover Markdown and plain documentation.

use super::HoverProvider;
use crate::jsdoc::{format_inline_code, inline_links, parse_jsdoc};
use crate::resolver::ScopeWalker;
use tsz_parser::NodeIndex;

impl<'a> HoverProvider<'a> {
    /// Extract plain documentation text from `JSDoc` (without markdown formatting).
    pub(super) fn extract_plain_documentation(&self, doc: &str) -> String {
        if doc.is_empty() {
            return String::new();
        }
        let parsed = parse_jsdoc(doc);
        let mut parts = Vec::new();
        if let Some(summary) = parsed.summary.as_ref()
            && !summary.is_empty()
        {
            parts.push(inline_links::expand_to_plain_text(summary));
        }
        // Include relevant tags in plain documentation
        for tag in &parsed.tags {
            match tag.name.as_str() {
                "example" => {
                    if tag.text.is_empty() {
                        parts.push("@example".to_string());
                    } else {
                        parts.push(format!(
                            "@example {}",
                            inline_links::expand_to_plain_text(&tag.text)
                        ));
                    }
                }
                "returns" | "return" if !tag.text.is_empty() => {
                    parts.push(format!(
                        "@returns {}",
                        inline_links::expand_to_plain_text(&tag.text)
                    ));
                }
                "deprecated" => {
                    if tag.text.is_empty() {
                        parts.push("@deprecated".to_string());
                    } else {
                        parts.push(format!(
                            "@deprecated {}",
                            inline_links::expand_to_plain_text(&tag.text)
                        ));
                    }
                }
                "see" if !tag.text.is_empty() => {
                    parts.push(format!(
                        "@see {}",
                        inline_links::expand_to_plain_text(&tag.text)
                    ));
                }
                _ => {}
            }
        }
        if parts.is_empty() {
            inline_links::expand_to_plain_text(doc)
        } else {
            parts.join("\n\n")
        }
    }

    pub(super) fn format_jsdoc_for_hover(
        &self,
        doc: &str,
        root: NodeIndex,
        anchor: NodeIndex,
    ) -> Option<String> {
        if doc.is_empty() {
            return None;
        }

        let resolve = |name: &str| self.resolve_jsdoc_link_uri(root, anchor, name);
        let parsed = parse_jsdoc(doc);
        if parsed.is_empty() {
            return Some(inline_links::expand_to_markdown_with_resolver(doc, resolve));
        }

        let mut sections = Vec::new();
        if let Some(summary) = parsed.summary.as_ref()
            && !summary.is_empty()
        {
            sections.push(inline_links::expand_to_markdown_with_resolver(
                summary, resolve,
            ));
        }

        if !parsed.params.is_empty() {
            let mut names: Vec<&String> = parsed.params.keys().collect();
            names.sort();
            let mut lines = Vec::new();
            lines.push("Parameters:".to_string());
            for name in names {
                let desc = parsed.params.get(name).map_or("", |s| s.as_str());
                let name_code = format_inline_code(name);
                if desc.is_empty() {
                    lines.push(format!("- {name_code}"));
                } else {
                    lines.push(format!(
                        "- {name_code} {}",
                        inline_links::expand_to_markdown_with_resolver(desc, resolve)
                    ));
                }
            }
            sections.push(lines.join("\n"));
        }

        // Include relevant JSDoc tags
        for tag in &parsed.tags {
            match tag.name.as_str() {
                "returns" if !tag.text.is_empty() => {
                    sections.push(format!(
                        "Returns: {}",
                        inline_links::expand_to_markdown_with_resolver(&tag.text, resolve)
                    ));
                }
                "example" => {
                    // Fenced code blocks render `tag.text` verbatim, so they
                    // do not need inline escaping. Re-fence with enough
                    // backticks to outlast any fence in the example.
                    if tag.text.is_empty() {
                        sections.push("Example:".to_string());
                    } else {
                        let fence: String = "`".repeat(pick_example_fence_length(&tag.text));
                        sections.push(format!("Example:\n{fence}\n{}\n{fence}", tag.text));
                    }
                }
                "deprecated" => {
                    if tag.text.is_empty() {
                        sections.push("**@deprecated**".to_string());
                    } else {
                        sections.push(format!(
                            "**@deprecated** {}",
                            inline_links::expand_to_markdown_with_resolver(&tag.text, resolve)
                        ));
                    }
                }
                "see" if !tag.text.is_empty() => {
                    sections.push(format!(
                        "See: {}",
                        inline_links::expand_to_markdown_with_resolver(&tag.text, resolve)
                    ));
                }
                "throws" | "exception" if !tag.text.is_empty() => {
                    sections.push(format!(
                        "Throws: {}",
                        inline_links::expand_to_markdown_with_resolver(&tag.text, resolve)
                    ));
                }
                "since" if !tag.text.is_empty() => {
                    sections.push(format!(
                        "Since: {}",
                        inline_links::expand_to_markdown_with_resolver(&tag.text, resolve)
                    ));
                }
                _ => {}
            }
        }

        let formatted = sections.join("\n\n");
        if formatted.is_empty() {
            Some(inline_links::expand_to_markdown_with_resolver(doc, resolve))
        } else {
            Some(formatted)
        }
    }

    fn resolve_jsdoc_link_uri(
        &self,
        root: NodeIndex,
        anchor: NodeIndex,
        name: &str,
    ) -> Option<String> {
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = walker.resolve_name_at(root, anchor, name)?;
        let symbol = self.binder.symbols.get(symbol_id)?;
        let decl_idx = symbol.primary_declaration()?;
        if !self.declaration_belongs_to_current_arena(symbol_id, decl_idx) {
            return None;
        }
        let decl_node = self.arena.get(decl_idx)?;
        let source_len = self.source_text.len() as u32;
        if decl_node.pos > source_len
            || decl_node.end > source_len
            || decl_node.pos == decl_node.end
        {
            return None;
        }

        let pos = self
            .line_map
            .offset_to_position(decl_node.pos, self.source_text);
        Some(format!(
            "{}#L{},{}",
            self.markdown_file_uri(),
            pos.line.saturating_add(1),
            pos.character.saturating_add(1)
        ))
    }

    fn declaration_belongs_to_current_arena(
        &self,
        symbol_id: tsz_binder::SymbolId,
        decl_idx: NodeIndex,
    ) -> bool {
        self.binder
            .declaration_arenas
            .get(&(symbol_id, decl_idx))
            .is_none_or(|arenas| {
                arenas
                    .iter()
                    .any(|arena| std::ptr::eq(std::sync::Arc::as_ptr(arena), self.arena))
            })
    }

    fn markdown_file_uri(&self) -> String {
        if self.file_name.starts_with("file://") {
            self.file_name.clone()
        } else {
            format!("file://{}", self.file_name)
        }
    }
}

/// Pick a fence length for an `@example` code block. `CommonMark` §4.5 requires
/// the closing fence to be at least as long as the opening fence, so the
/// returned length must exceed every backtick-only line prefix inside `text`.
/// The minimum is three to match the conventional ` ``` ` fence.
fn pick_example_fence_length(text: &str) -> usize {
    let longest_inner_fence = text
        .lines()
        .map(|line| line.chars().take_while(|c| *c == '`').count())
        .max()
        .unwrap_or(0);
    (longest_inner_fence + 1).max(3)
}
