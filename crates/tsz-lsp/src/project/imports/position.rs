//! Position-aware identifier and import-target utilities.
//!
//! These helpers resolve a cursor position or source-text span to an
//! identifier string or import node, enabling go-to-definition and
//! auto-import context queries.

use crate::utils::find_node_at_offset;
use tsz_common::position::{Position, Range};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::{NodeArena, NodeIndex, syntax_kind_ext};
use tsz_scanner::{SyntaxKind, is_ecmascript_identifier_part, is_ecmascript_identifier_start};

use super::super::{ImportKind, ImportTarget, Project, ProjectFile};

fn identifier_span_touches_probe(start: usize, end: usize, probe: usize) -> bool {
    (start..=end).contains(&probe)
}

impl Project {
    /// Returns `true` when `position` falls inside a `NamedImports` node —
    /// i.e., the cursor is in the `{ … }` binding list of an `import` statement.
    ///
    /// TypeScript calls this the "import statement completion" context and uses
    /// `SortText.LocationPriority` ("11") instead of `SortText.AutoImportSuggestions`
    /// ("16") for candidates offered there.
    pub(crate) fn is_in_named_import_bindings(file: &ProjectFile, position: Position) -> bool {
        let arena = file.arena();
        let source_text = file.source_text();
        let Some(offset) = file.line_map().position_to_offset(position, source_text) else {
            return false;
        };

        let mut node_idx = find_node_at_offset(arena, offset);
        if node_idx.is_none() && offset > 0 {
            node_idx = find_node_at_offset(arena, offset - 1);
        }

        // Walk up the parent chain until we hit a NAMED_IMPORTS node (found) or
        // pass the statement boundary (IMPORT_DECLARATION / SOURCE_FILE).
        // Bounded to avoid pathological cycles; import nesting is always shallow.
        let mut current = node_idx;
        for _ in 0..8 {
            let Some(node) = arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::NAMED_IMPORTS {
                return true;
            }
            if node.kind == syntax_kind_ext::IMPORT_DECLARATION
                || node.kind == syntax_kind_ext::SOURCE_FILE
            {
                break;
            }
            let Some(parent) = arena.parent_of(current) else {
                break;
            };
            current = parent;
        }

        false
    }

    pub(super) fn identifier_at_range(&self, file: &ProjectFile, range: Range) -> Option<String> {
        let start_offset = file
            .line_map()
            .position_to_offset(range.start, file.source_text())?;
        let end_offset = file
            .line_map()
            .position_to_offset(range.end, file.source_text())
            .unwrap_or(start_offset);

        self.identifier_at_offset(file, start_offset)
            .or_else(|| {
                end_offset
                    .checked_sub(1)
                    .and_then(|offset| self.identifier_at_offset(file, offset))
            })
            .or_else(|| {
                start_offset
                    .checked_sub(1)
                    .and_then(|offset| self.identifier_at_offset(file, offset))
            })
            .or_else(|| {
                Self::identifier_text_from_source_span(file.source_text(), start_offset, end_offset)
            })
    }

    fn identifier_at_offset(&self, file: &ProjectFile, offset: u32) -> Option<String> {
        let node_idx = find_node_at_offset(file.arena(), offset);
        let node = file.arena().get(node_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        file.arena()
            .get_identifier_text(node_idx)
            .map(std::string::ToString::to_string)
    }

    fn identifier_text_from_source_span(
        source_text: &str,
        start_offset: u32,
        end_offset: u32,
    ) -> Option<String> {
        let mut probe_offsets = Vec::with_capacity(4);
        probe_offsets.push(start_offset as usize);
        if end_offset > 0 {
            probe_offsets.push((end_offset - 1) as usize);
        }
        if start_offset > 0 {
            probe_offsets.push((start_offset - 1) as usize);
        }
        if end_offset as usize > start_offset as usize {
            probe_offsets
                .push(((start_offset as usize + end_offset as usize) / 2).saturating_sub(1));
        }

        for probe in probe_offsets {
            if let Some(text) = Self::identifier_text_around_offset(source_text, probe) {
                return Some(text);
            }
        }

        None
    }

    fn identifier_text_around_offset(source_text: &str, probe_offset: usize) -> Option<String> {
        let probe = probe_offset.min(source_text.len());
        let mut current_start = None;
        let mut current_end = 0;

        for (idx, ch) in source_text.char_indices() {
            let next = idx + ch.len_utf8();
            if let Some(start) = current_start {
                if is_ecmascript_identifier_part(ch) {
                    current_end = next;
                    continue;
                }
                if identifier_span_touches_probe(start, current_end, probe) {
                    return Some(source_text[start..current_end].to_string());
                }
                current_start = None;
            }

            if is_ecmascript_identifier_start(ch) {
                current_start = Some(idx);
                current_end = next;
            }
        }

        let start = current_start?;
        identifier_span_touches_probe(start, current_end, probe)
            .then(|| source_text[start..current_end].to_string())
    }

    pub(crate) fn identifier_at_position(
        &self,
        file: &ProjectFile,
        position: Position,
    ) -> Option<(NodeIndex, String)> {
        let offset = file
            .line_map()
            .position_to_offset(position, file.source_text())?;
        let mut node_idx = find_node_at_offset(file.arena(), offset);
        if node_idx.is_none() && offset > 0 {
            node_idx = find_node_at_offset(file.arena(), offset - 1);
        }
        if node_idx.is_none() {
            return None;
        }

        let node = file.arena().get(node_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let text = file.arena().get_identifier_text(node_idx)?.to_string();
        Some((node_idx, text))
    }

    pub(crate) fn is_member_access_node(&self, arena: &NodeArena, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        while current.is_some() {
            let Some(node) = arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::QUALIFIED_NAME
            {
                return true;
            }

            let Some(ext) = arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
        }

        false
    }

    pub(super) fn import_target_at_position(
        &self,
        file: &ProjectFile,
        position: Position,
    ) -> Option<ImportTarget> {
        let offset = file
            .line_map()
            .position_to_offset(position, file.source_text())?;
        let node_idx = find_node_at_offset(file.arena(), offset);
        if node_idx.is_none() {
            return None;
        }
        self.import_target_from_node(file, node_idx)
    }

    fn import_target_from_node(
        &self,
        file: &ProjectFile,
        node_idx: NodeIndex,
    ) -> Option<ImportTarget> {
        let arena = file.arena();
        let mut current = node_idx;
        let mut import_specifier = None;
        let mut import_clause = None;
        let mut import_decl = None;

        while current.is_some() {
            let node = arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::IMPORT_SPECIFIER => {
                    import_specifier = Some(current);
                }
                k if k == syntax_kind_ext::IMPORT_CLAUSE => {
                    import_clause = Some(current);
                }
                k if k == syntax_kind_ext::IMPORT_DECLARATION
                    || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION =>
                {
                    import_decl = Some(current);
                    break;
                }
                _ => {}
            }
            current = arena.get_extended(current)?.parent;
        }

        let import_decl_idx = import_decl?;
        let import_decl = arena.get_import_decl_at(import_decl_idx)?;
        let module_specifier = arena
            .get_literal_text(import_decl.module_specifier)?
            .to_string();

        let kind = if let Some(spec_idx) = import_specifier {
            let spec = arena.get_specifier_at(spec_idx)?;
            let export_ident = if spec.property_name.is_some() {
                spec.property_name
            } else {
                spec.name
            };
            let export_name = arena.get_identifier_text(export_ident)?.to_string();
            ImportKind::Named(export_name)
        } else if let Some(clause_idx) = import_clause {
            let clause = arena.get_import_clause_at(clause_idx)?;

            if clause.name == node_idx {
                ImportKind::Default
            } else if clause.named_bindings == node_idx || import_decl.module_specifier == node_idx
            {
                ImportKind::Namespace
            } else {
                return None;
            }
        } else if import_decl.module_specifier == node_idx {
            ImportKind::Namespace
        } else {
            return None;
        };

        Some(ImportTarget {
            module_specifier,
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Project;

    #[test]
    fn identifier_text_around_offset_accepts_unicode_identifier_start_and_part() {
        let source = "const café = 日本語;";

        let cafe_start = source.find("café").expect("café");
        assert_eq!(
            Project::identifier_text_around_offset(source, cafe_start),
            Some("café".to_string())
        );
        assert_eq!(
            Project::identifier_text_around_offset(source, cafe_start + "café".len()),
            Some("café".to_string())
        );

        let japanese_mid = source.find("本").expect("日本語");
        assert_eq!(
            Project::identifier_text_around_offset(source, japanese_mid),
            Some("日本語".to_string())
        );
    }
}
