use super::super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn simple_type_reference_name_text(
        &self,
        type_node_idx: NodeIndex,
    ) -> Option<String> {
        let type_node = self.arena.get(type_node_idx)?;
        if type_node.kind == SyntaxKind::Identifier as u16 {
            return self.get_identifier_text(type_node_idx);
        }
        if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let type_ref = self.arena.get_type_ref(type_node)?;
            return self.type_reference_name_text(type_ref.type_name);
        }
        None
    }
}
