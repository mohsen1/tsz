//! Helpers for expanding function-local asserted type aliases during DTS emit.

use super::super::DeclarationEmitter;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn type_alias_application_substitutions(
        &self,
        type_parameters: Option<&NodeList>,
        type_node_idx: NodeIndex,
    ) -> Vec<(String, String)> {
        let Some(type_parameters) = type_parameters else {
            return Vec::new();
        };
        let Some(type_node) = self.arena.get(type_node_idx) else {
            return Vec::new();
        };
        let Some(type_ref) = self.arena.get_type_ref(type_node) else {
            return Vec::new();
        };
        let type_args = self.type_argument_list_source_text(type_ref.type_arguments.as_ref());
        if type_args.is_empty() {
            return Vec::new();
        }

        type_parameters
            .nodes
            .iter()
            .copied()
            .zip(type_args)
            .filter_map(|(param_idx, arg_text)| {
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_type_parameter(param_node)?;
                let name = self.get_identifier_text(param.name)?;
                Some((name, arg_text))
            })
            .collect()
    }

    pub(in crate::declaration_emitter) fn normalize_local_type_literal_accessor_text(
        type_text: &str,
    ) -> String {
        let trimmed = type_text.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            return type_text.to_string();
        }

        let lines: Vec<&str> = type_text.lines().collect();
        if lines.len() < 3 {
            return type_text.to_string();
        }

        let mut normalized = Vec::new();
        let mut i = 1usize;
        while i + 1 < lines.len() {
            let line = lines[i];
            let member = line.trim().trim_end_matches(';').trim();
            if let Some((name, get_type)) = Self::parse_get_accessor_member(member) {
                if i + 2 < lines.len() {
                    let next_member = lines[i + 1].trim().trim_end_matches(';').trim();
                    if let Some((set_name, set_param, set_type)) =
                        Self::parse_set_accessor_member(next_member)
                        && set_name == name
                    {
                        let get_type = Self::simplify_duplicate_union_type_text(&get_type);
                        let set_type = Self::simplify_duplicate_union_type_text(&set_type);
                        if get_type == set_type {
                            normalized.push(format!("    {name}: {get_type};"));
                        } else {
                            normalized.push(format!("    get {name}(): {get_type};"));
                            normalized.push(format!("    set {name}({set_param}: {set_type});"));
                        }
                        i += 2;
                        continue;
                    }
                }
                let get_type = Self::simplify_duplicate_union_type_text(&get_type);
                normalized.push(format!("    readonly {name}: {get_type};"));
                i += 1;
                continue;
            }

            if let Some((name, _set_param, set_type)) = Self::parse_set_accessor_member(member) {
                let set_type = Self::simplify_duplicate_union_type_text(&set_type);
                normalized.push(format!("    {name}: {set_type};"));
                i += 1;
                continue;
            }

            normalized.push(line.to_string());
            i += 1;
        }

        format!("{{\n{}\n}}", normalized.join("\n"))
    }

    fn parse_get_accessor_member(member: &str) -> Option<(String, String)> {
        let rest = member.strip_prefix("get ")?;
        let (name, ty) = rest.split_once("():")?;
        Some((name.trim().to_string(), ty.trim().to_string()))
    }

    fn parse_set_accessor_member(member: &str) -> Option<(String, String, String)> {
        let rest = member.strip_prefix("set ")?;
        let open = rest.find('(')?;
        let close = rest.rfind(')')?;
        let name = rest[..open].trim();
        let param = rest[open + 1..close].trim();
        let (param_name, param_type) = param.split_once(':')?;
        Some((
            name.to_string(),
            param_name.trim().to_string(),
            param_type.trim().to_string(),
        ))
    }

    fn simplify_duplicate_union_type_text(type_text: &str) -> String {
        let parts = Self::split_top_level_union_parts(type_text);
        if parts.len() <= 1 {
            return type_text.to_string();
        }

        let mut unique = Vec::new();
        for part in parts {
            if !unique.iter().any(|existing| existing == &part) {
                unique.push(part);
            }
        }
        unique.join(" | ")
    }

    fn split_top_level_union_parts(type_text: &str) -> Vec<String> {
        let bytes = type_text.as_bytes();
        let mut angle_depth = 0usize;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut part_start = 0usize;
        let mut parts = Vec::new();
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'|' if angle_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                    let part = type_text[part_start..i].trim();
                    if !part.is_empty() {
                        parts.push(part.to_string());
                    }
                    part_start = i + 1;
                }
                _ => {}
            }
            i += 1;
        }
        let tail = type_text[part_start..].trim();
        if !tail.is_empty() {
            parts.push(tail.to_string());
        }
        parts
    }

    pub(in crate::declaration_emitter) fn find_enclosing_block_type_alias_declaration(
        &self,
        from_idx: NodeIndex,
        name: &str,
    ) -> Option<NodeIndex> {
        let mut current_idx = from_idx;
        while let Some(ext) = self.arena.get_extended(current_idx) {
            let parent_idx = ext.parent;
            if !parent_idx.is_some() {
                return None;
            }
            let parent_node = self.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::BLOCK
                && let Some(block) = self.arena.get_block(parent_node)
                && let Some(alias_decl) =
                    block.statements.nodes.iter().copied().find_map(|stmt_idx| {
                        let stmt_node = self.arena.get(stmt_idx)?;
                        if stmt_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                            return None;
                        }
                        let alias = self.arena.get_type_alias(stmt_node)?;
                        (self.get_identifier_text(alias.name).as_deref() == Some(name))
                            .then_some(stmt_idx)
                    })
            {
                return Some(alias_decl);
            }
            current_idx = parent_idx;
        }
        None
    }
}
