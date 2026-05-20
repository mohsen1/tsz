//! Class-expression static state naming helpers for the lowering pass.

use super::*;

impl<'a> LoweringPass<'a> {
    pub(super) fn class_expr_static_comma_needs_set_function_name(
        &self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        if class.name.is_some() || self.resolve_class_expr_binding_name(class_idx).is_none() {
            return false;
        }

        let needs_private_field_lowering = self.ctx.needs_es2022_lowering;
        let target_needs_field_lowering =
            self.ctx.needs_es2022_lowering || !self.ctx.options.use_define_for_class_fields;
        #[allow(clippy::nonminimal_bool)]
        let has_static_field_comma_expr = target_needs_field_lowering
            && class.members.nodes.iter().any(|&member_idx| {
                self.arena.get(member_idx).is_some_and(|member| {
                    member.kind == syntax_kind_ext::PROPERTY_DECLARATION
                        && self.arena.get_property_decl(member).is_some_and(|prop| {
                            self.arena.is_static(&prop.modifiers)
                                && !self
                                    .arena
                                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                                && !self
                                    .arena
                                    .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                                && !(needs_private_field_lowering
                                    && is_private_identifier(self.arena, prop.name))
                        })
                })
            });
        let has_static_block_comma_expr = self.ctx.needs_es2022_lowering
            && class.members.nodes.iter().any(|&member_idx| {
                self.arena.get(member_idx).is_some_and(|member| {
                    member.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
                })
            });
        let has_static_computed_method_or_accessor =
            class.members.nodes.iter().any(|&member_idx| {
                self.arena
                    .get(member_idx)
                    .is_some_and(|member| match member.kind {
                        k if k == syntax_kind_ext::METHOD_DECLARATION => {
                            self.arena.get_method_decl(member).is_some_and(|method| {
                                self.arena.is_static(&method.modifiers)
                                    && self.arena.get(method.name).is_some_and(|name| {
                                        name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                                    })
                            })
                        }
                        k if k == syntax_kind_ext::GET_ACCESSOR
                            || k == syntax_kind_ext::SET_ACCESSOR =>
                        {
                            self.arena.get_accessor(member).is_some_and(|accessor| {
                                self.arena.is_static(&accessor.modifiers)
                                    && self.arena.get(accessor.name).is_some_and(|name| {
                                        name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                                    })
                            })
                        }
                        _ => false,
                    })
            });
        let has_static_private_member = needs_private_field_lowering
            && class.members.nodes.iter().any(|&member_idx| {
                self.arena.get(member_idx).is_some_and(|member| {
                    member.kind == syntax_kind_ext::PROPERTY_DECLARATION
                        && self.arena.get_property_decl(member).is_some_and(|prop| {
                            self.arena.is_static(&prop.modifiers)
                                && is_private_identifier(self.arena, prop.name)
                        })
                })
            });

        has_static_field_comma_expr
            || has_static_block_comma_expr
            || has_static_computed_method_or_accessor
            || has_static_private_member
    }
}
