use super::super::DeclarationEmitter;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn enclosing_class_declaration_index(
        &self,
        from_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = from_idx;
        for _ in 0..64 {
            let parent_idx = self.arena.parent_of(current)?;
            let parent_node = self.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return Some(parent_idx);
            }
            current = parent_idx;
        }
        None
    }

    pub(in crate::declaration_emitter) fn collect_type_param_names_from_arena(
        &self,
        source_arena: &NodeArena,
        type_params: &NodeList,
    ) -> Vec<String> {
        type_params
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let param_node = source_arena.get(param_idx)?;
                let param = source_arena.get_type_parameter(param_node)?;
                self.identifier_text_from_arena(source_arena, param.name)
            })
            .collect()
    }

    pub(in crate::declaration_emitter) fn indexed_call_argument_type_text(
        &self,
        arg_idx: NodeIndex,
    ) -> Option<String> {
        let arg_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(arg_idx);
        let arg_node = self.arena.get(arg_idx)?;
        if arg_node.kind == SyntaxKind::ThisKeyword as u16 {
            return Some("this".to_string());
        }
        if arg_node.kind == SyntaxKind::StringLiteral as u16
            || arg_node.kind == SyntaxKind::NumericLiteral as u16
            || arg_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return self
                .get_source_slice(arg_node.pos, arg_node.end)
                .map(|text| text.trim().to_string());
        }
        if arg_node.kind == SyntaxKind::Identifier as u16 {
            return self
                .enclosing_parameter_source_type_annotation_text_for_identifier(arg_idx)
                .or_else(|| self.reference_declared_type_annotation_text(arg_idx))
                .filter(|text| !text.trim().is_empty() && text != "any");
        }
        None
    }

    pub(in crate::declaration_emitter) fn indexed_access_receiver_type_text(
        &self,
        receiver_idx: NodeIndex,
    ) -> Option<String> {
        let receiver_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(receiver_idx);
        let receiver_node = self.arena.get(receiver_idx)?;
        if receiver_node.kind == SyntaxKind::ThisKeyword as u16 {
            return Some("this".to_string());
        }
        if receiver_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return self.source_indexed_access_return_type_text(receiver_idx);
        }
        if receiver_node.kind == SyntaxKind::Identifier as u16 {
            return self
                .enclosing_parameter_source_type_annotation_text_for_identifier(receiver_idx)
                .or_else(|| self.reference_declared_type_annotation_text(receiver_idx))
                .filter(|text| !text.trim().is_empty() && text != "any");
        }
        None
    }

    pub(in crate::declaration_emitter) fn indexed_access_key_type_text(
        &self,
        key_idx: NodeIndex,
    ) -> Option<String> {
        let key_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(key_idx);
        let key_node = self.arena.get(key_idx)?;
        if key_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(key_node)?;
            let receiver_text = self.indexed_access_receiver_type_text(access.expression)?;
            return Self::array_element_type_text(&receiver_text);
        }
        if key_node.kind == SyntaxKind::Identifier as u16 {
            return self
                .enclosing_parameter_source_type_annotation_text_for_identifier(key_idx)
                .or_else(|| self.reference_declared_type_annotation_text(key_idx))
                .filter(|text| !text.trim().is_empty() && text != "any");
        }
        if key_node.kind == SyntaxKind::StringLiteral as u16
            || key_node.kind == SyntaxKind::NumericLiteral as u16
        {
            return self
                .get_source_slice(key_node.pos, key_node.end)
                .map(|text| text.trim().to_string());
        }
        None
    }

    pub(in crate::declaration_emitter) fn enclosing_parameter_source_type_annotation_text_for_identifier(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let name = self.get_identifier_text(expr_idx)?;
        let mut current = expr_idx;
        for _ in 0..32 {
            let parent_idx = self.arena.parent_of(current)?;
            let parent_node = self.arena.get(parent_idx)?;
            if let Some(func) = self.arena.get_function(parent_node) {
                for &param_idx in &func.parameters.nodes {
                    let param_node = self.arena.get(param_idx)?;
                    let param = self.arena.get_parameter(param_node)?;
                    if self.get_identifier_text(param.name).as_deref() == Some(name.as_str()) {
                        return self
                            .source_slice_from_arena(self.arena, param.type_annotation)
                            .or_else(|| {
                                self.type_annotation_text_from_arena_node(
                                    self.arena,
                                    param.type_annotation,
                                )
                            })
                            .map(|text| text.trim().to_string());
                    }
                }
                return None;
            }
            current = parent_idx;
        }
        None
    }
}
