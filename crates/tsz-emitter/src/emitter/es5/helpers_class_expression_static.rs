//! ES5 class-expression static element scheduling helpers.

use crate::emitter::Printer;
use crate::emitter::core::PropertyNameEmit;
use crate::emitter::es5::helpers_async::{
    Es5StaticClassExpressionElement, Es5StaticClassExpressionField,
};
use tsz_parser::parser::node::ClassData;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn es5_class_expression_has_static_runtime_elements(
        &self,
        class_data: &ClassData,
    ) -> bool {
        class_data.members.nodes.iter().any(|&member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                return true;
            }
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                return false;
            }
            let Some(prop) = self.arena.get_property_decl(member_node) else {
                return false;
            };
            prop.initializer.is_some()
                && self.has_effective_static_modifier_js(&prop.modifiers)
                && !self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                && !self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                && !self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                && self
                    .arena
                    .get(prop.name)
                    .is_none_or(|n| n.kind != SyntaxKind::PrivateIdentifier as u16)
        })
    }

    pub(in crate::emitter) fn es5_static_class_expression_elements_with_computed_temps(
        &self,
        class_data: &ClassData,
        computed_decls: &[String],
    ) -> Vec<Es5StaticClassExpressionElement> {
        let mut inits = Vec::new();
        let mut computed_temps = computed_decls.iter();

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                inits.push(Es5StaticClassExpressionElement::StaticBlock {
                    block: member_idx,
                    saved_comment_idx: self.static_block_inner_comment_index(member_node),
                    member_pos: member_node.pos,
                });
                continue;
            }
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }
            let Some(prop) = self.arena.get_property_decl(member_node) else {
                continue;
            };
            if prop.initializer.is_none()
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                || self
                    .arena
                    .get(prop.name)
                    .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16)
            {
                continue;
            }

            let computed_temp = self
                .arena
                .get(prop.name)
                .and_then(|name| {
                    (name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                        .then(|| self.arena.get_computed_property(name))
                        .flatten()
                })
                .and_then(|computed| {
                    self.arena.get(computed.expression).and_then(|expr_node| {
                        let is_constant = expr_node.kind == SyntaxKind::StringLiteral as u16
                            || expr_node.kind == SyntaxKind::NumericLiteral as u16
                            || expr_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16;
                        (!is_constant)
                            .then(|| computed_temps.next().map(std::string::String::as_str))
                            .flatten()
                    })
                });

            if !self.has_effective_static_modifier_js(&prop.modifiers) {
                continue;
            }

            let name_emit = if let Some(temp) = computed_temp {
                Some(PropertyNameEmit::Bracket(temp.to_string()))
            } else {
                self.get_property_name_emit(prop.name)
            };
            if let Some(name_emit) = name_emit {
                inits.push(Es5StaticClassExpressionElement::Field(
                    Es5StaticClassExpressionField {
                        name_emit,
                        initializer: prop.initializer,
                        member_pos: member_node.pos,
                    },
                ));
            }
        }

        inits.sort_by_key(Es5StaticClassExpressionElement::member_pos);
        inits
    }
}
