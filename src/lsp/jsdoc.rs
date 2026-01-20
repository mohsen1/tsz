//! JSDoc helpers for LSP features.
//!
//! Provides shared extraction and parsing for hover and signature help.

use crate::comments::{get_jsdoc_content, get_leading_comments_from_cache, is_jsdoc_comment};
use crate::parser::node::NodeArena;
use crate::parser::{NodeIndex, syntax_kind_ext};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct ParsedJsdoc {
    pub summary: Option<String>,
    pub params: HashMap<String, String>,
}

impl ParsedJsdoc {
    pub fn is_empty(&self) -> bool {
        self.summary.is_none() && self.params.is_empty()
    }
}

/// Extract the nearest JSDoc comment preceding a node.
/// Uses cached comment ranges from SourceFileData for O(log N) performance.
pub fn jsdoc_for_node(
    arena: &NodeArena,
    root: NodeIndex,
    node_idx: NodeIndex,
    source_text: &str,
) -> String {
    let Some(node) = arena.get(node_idx) else {
        return String::new();
    };
    let mut target_pos = node.pos;

    if arena.get_variable_declaration(node).is_some() {
        if let Some(ext) = arena.get_extended(node_idx) {
            let list_idx = ext.parent;
            if let Some(list_node) = arena.get(list_idx) {
                if list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
                    if let Some(list_data) = arena.get_variable(list_node) {
                        if list_data.declarations.nodes.len() == 1 {
                            if let Some(list_ext) = arena.get_extended(list_idx) {
                                let stmt_idx = list_ext.parent;
                                if let Some(stmt_node) = arena.get(stmt_idx) {
                                    if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                                        target_pos = stmt_node.pos;
                                        if let Some(stmt_ext) = arena.get_extended(stmt_idx) {
                                            let export_idx = stmt_ext.parent;
                                            if let Some(export_node) = arena.get(export_idx) {
                                                if export_node.kind
                                                    == syntax_kind_ext::EXPORT_DECLARATION
                                                {
                                                    target_pos = export_node.pos;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let comments = if let Some(root_node) = arena.get(root) {
        if let Some(sf_data) = arena.get_source_file(root_node) {
            &sf_data.comments
        } else {
            return String::new();
        }
    } else {
        return String::new();
    };

    if let Some(comment) = comments
        .iter()
        .find(|comment| comment.pos <= node.pos && node.pos < comment.end)
    {
        if is_jsdoc_comment(comment, source_text) {
            return get_jsdoc_content(comment, source_text);
        }
    }

    let leading_comments = get_leading_comments_from_cache(comments, target_pos, source_text);
    if let Some(comment) = leading_comments.last() {
        let end = comment.end as usize;
        let check = target_pos as usize;
        let gap_is_whitespace =
            end <= check && source_text[end..check].chars().all(|c| c.is_whitespace());

        if gap_is_whitespace && is_jsdoc_comment(comment, source_text) {
            return get_jsdoc_content(comment, source_text);
        }
    }

    String::new()
}

pub fn parse_jsdoc(doc: &str) -> ParsedJsdoc {
    let mut summary_lines = Vec::new();
    let mut params = HashMap::new();
    let mut current_param: Option<String> = None;
    let mut current_desc = String::new();
    let mut in_tags = false;

    for line in doc.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !in_tags {
                summary_lines.push(String::new());
            }
            continue;
        }

        if trimmed.starts_with('@') {
            in_tags = true;
            if let Some(name) = current_param.take() {
                let desc = current_desc.trim().to_string();
                if !desc.is_empty() {
                    params.insert(name, desc);
                }
                current_desc.clear();
            }

            if let Some((name, desc)) = parse_param_tag(trimmed) {
                current_param = Some(name);
                current_desc = desc;
            }
            continue;
        }

        if current_param.is_some() {
            if !current_desc.is_empty() {
                current_desc.push(' ');
            }
            current_desc.push_str(trimmed);
        } else if !in_tags {
            summary_lines.push(trimmed.to_string());
        }
    }

    if let Some(name) = current_param {
        let desc = current_desc.trim().to_string();
        if !desc.is_empty() {
            params.insert(name, desc);
        }
    }

    let summary = summary_lines.join("\n").trim().to_string();

    ParsedJsdoc {
        summary: if summary.is_empty() {
            None
        } else {
            Some(summary)
        },
        params,
    }
}

fn parse_param_tag(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("@param")?.trim();
    if rest.is_empty() {
        return None;
    }

    let rest = if rest.starts_with('{') {
        if let Some(end) = rest.find('}') {
            rest[end + 1..].trim()
        } else {
            rest
        }
    } else {
        rest
    };

    let mut parts = rest.splitn(2, char::is_whitespace);
    let name_raw = parts.next()?.trim();
    if name_raw.is_empty() {
        return None;
    }
    let desc = parts.next().unwrap_or("").trim().to_string();
    let name = normalize_param_name(name_raw);
    if name.is_empty() {
        return None;
    }
    Some((name, desc))
}

fn normalize_param_name(name: &str) -> String {
    let trimmed = name.trim();
    let mut name = if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() > 2 {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    if let Some(eq) = name.find('=') {
        name = &name[..eq];
    }
    name = name.trim();
    if let Some(stripped) = name.strip_prefix("...") {
        name = stripped;
    }
    name.trim().to_string()
}
