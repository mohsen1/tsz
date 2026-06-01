//! Source-backed accessor-name recovery for structural declaration text.

use super::super::DeclarationEmitter;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeArena;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn source_type_setter_parameter_names(
        &self,
        source_arena: &NodeArena,
        source_type_text: &str,
    ) -> FxHashMap<String, String> {
        let source_names =
            if let Some(alias_name) = Self::leading_type_reference_name(source_type_text) {
                self.source_type_alias_setter_parameter_names(source_arena, alias_name)
            } else {
                Self::source_type_text_setter_parameter_names(source_type_text)
            };
        let mut names = FxHashMap::default();
        for (member_name, param_name) in source_names {
            if !Self::is_simple_identifier_text(&param_name) {
                continue;
            }
            names.insert(member_name, param_name);
        }
        names
    }

    fn source_type_alias_setter_parameter_names(
        &self,
        source_arena: &NodeArena,
        alias_name: &str,
    ) -> Vec<(String, String)> {
        source_arena
            .nodes
            .iter()
            .filter_map(|node| source_arena.get_type_alias(node))
            .filter(|alias| {
                self.identifier_text_from_arena(source_arena, alias.name)
                    .as_deref()
                    == Some(alias_name)
            })
            .filter_map(|alias| source_arena.get(alias.type_node))
            .filter_map(|node| source_arena.get_type_literal(node))
            .flat_map(|type_literal| type_literal.members.nodes.iter().copied())
            .filter_map(|member_idx| {
                let member_node = source_arena.get(member_idx)?;
                if member_node.kind != syntax_kind_ext::SET_ACCESSOR {
                    return None;
                }
                let accessor = source_arena.get_accessor(member_node)?;
                let member_name =
                    self.property_name_text_from_arena(source_arena, accessor.name)?;
                let param_idx = accessor.parameters.nodes.first().copied()?;
                let param_node = source_arena.get(param_idx)?;
                let param = source_arena.get_parameter(param_node)?;
                let param_name = self.identifier_text_from_arena(source_arena, param.name)?;
                Some((member_name, param_name))
            })
            .collect()
    }

    fn source_type_text_setter_parameter_names(source_type_text: &str) -> Vec<(String, String)> {
        source_type_text
            .lines()
            .filter_map(|line| {
                let member = line.trim().trim_end_matches(';').trim();
                let rest = member.strip_prefix("set ")?;
                let open_paren = rest.find('(')?;
                let member_name = rest[..open_paren].trim();
                if member_name.is_empty() {
                    return None;
                }
                let param_text = rest[open_paren + 1..].trim_start();
                let colon = param_text.find(':')?;
                let param_name = param_text[..colon].trim();
                Some((member_name.to_string(), param_name.to_string()))
            })
            .collect()
    }
}
