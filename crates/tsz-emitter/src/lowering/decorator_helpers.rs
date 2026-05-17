use super::*;
use tsz_parser::parser::node::ClassData;

impl<'a> LoweringPass<'a> {
    pub(super) fn class_has_static_private_method(&self, class_data: &ClassData) -> bool {
        class_data.members.nodes.iter().any(|&member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            if member_node.kind != syntax_kind_ext::METHOD_DECLARATION {
                return false;
            }
            self.arena
                .get_method_decl(member_node)
                .is_some_and(|method| {
                    self.arena.is_static(&method.modifiers)
                        && self.arena.get(method.name).is_some_and(|name_node| {
                            name_node.kind == SyntaxKind::PrivateIdentifier as u16
                        })
                })
        })
    }

    pub(super) fn mark_tc39_decorator_helpers(&mut self, class_data: &ClassData) {
        let needs_prop_key = self.class_has_computed_decorated_member(class_data);
        let needs_set_function_name = self.class_has_private_decorated_member(class_data);
        let has_class_decorators = class_data.modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&mod_idx| {
                self.arena
                    .get(mod_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
            })
        });
        let needs_class_set_fn_name = has_class_decorators
            && ((self.ctx.target_es5 || self.ctx.needs_es2022_lowering)
                || class_data.name.is_none()
                || self.class_has_static_private_method(class_data));

        let helpers = self.transforms.helpers_mut();
        helpers.es_decorate = true;
        helpers.run_initializers = true;
        if needs_prop_key {
            helpers.prop_key = true;
        }
        if needs_set_function_name || needs_class_set_fn_name {
            helpers.set_function_name = true;
        }
    }
}
