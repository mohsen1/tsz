use super::*;
use tsz_parser::parser::node::ClassData;

#[derive(Default)]
struct PrivateAccessHelperNeeds {
    has: bool,
    get: bool,
    set: bool,
}

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
        let has_decorated_method_or_accessor =
            self.class_has_decorated_method_or_accessor(class_data);
        let private_helper_needs = self.decorated_static_private_member_helper_needs(class_data);
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
                || self.class_has_static_private_method(class_data)
                || self.class_has_plain_static_auto_accessor_member(class_data));
        let needs_class_auto_accessor_helpers =
            has_class_decorators && self.class_has_plain_static_auto_accessor_member(class_data);

        let helpers = self.transforms.helpers_mut();
        helpers.es_decorate = true;
        helpers.run_initializers = true;
        // tsc emits `__runInitializers` before `__esDecorate` (both priority 2,
        // so request order decides) exactly when the class has a decorated
        // method, getter, or setter: those request the method extra-initializers
        // `__runInitializers` while the class element is processed, before the
        // class-level `__esDecorate` call is built. Decorated fields,
        // auto-accessors, and bare class decorators do not, so they keep
        // `__esDecorate` first.
        if has_decorated_method_or_accessor {
            helpers.run_initializers_before_es_decorate = true;
        }
        if needs_prop_key {
            helpers.prop_key = true;
        }
        if needs_set_function_name || needs_class_set_fn_name {
            helpers.set_function_name = true;
        }
        if needs_class_auto_accessor_helpers {
            helpers.mark_class_private_field_get();
            helpers.mark_class_private_field_set();
        }
        if has_class_decorators {
            if private_helper_needs.has {
                helpers.mark_class_private_field_in();
            }
            if private_helper_needs.get {
                helpers.mark_class_private_field_get();
            }
            if private_helper_needs.set {
                helpers.mark_class_private_field_set();
            }
        }
    }

    /// True when the class has a decorated method, getter, or setter (but not
    /// an auto-accessor, which is a `PROPERTY_DECLARATION` with the `accessor`
    /// modifier). These members request the method extra-initializers
    /// `__runInitializers` helper before the class-level `__esDecorate`, which
    /// is what makes tsc emit `__runInitializers` first.
    fn class_has_decorated_method_or_accessor(&self, class_data: &ClassData) -> bool {
        class_data.members.nodes.iter().any(|&member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            let kind = member_node.kind;
            (kind == syntax_kind_ext::METHOD_DECLARATION
                || kind == syntax_kind_ext::GET_ACCESSOR
                || kind == syntax_kind_ext::SET_ACCESSOR)
                && self.member_has_decorator(member_node)
        })
    }

    fn member_has_decorator(&self, member_node: &tsz_parser::parser::node::Node) -> bool {
        let modifiers = match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .arena
                .get_method_decl(member_node)
                .and_then(|method| method.modifiers.as_ref()),
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .arena
                .get_property_decl(member_node)
                .and_then(|property| property.modifiers.as_ref()),
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .arena
                .get_accessor(member_node)
                .and_then(|accessor| accessor.modifiers.as_ref()),
            _ => None,
        };

        modifiers.is_some_and(|mods| {
            mods.nodes.iter().any(|&mod_idx| {
                self.arena
                    .get(mod_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::DECORATOR)
            })
        })
    }

    fn decorated_static_private_member_helper_needs(
        &self,
        class_data: &ClassData,
    ) -> PrivateAccessHelperNeeds {
        let mut needs = PrivateAccessHelperNeeds::default();
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if self.member_shape_is_decorated_static_private(&method.modifiers, method.name)
                    {
                        needs.has = true;
                        needs.get = true;
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR => {
                    let Some(accessor) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if self.member_shape_is_decorated_static_private(
                        &accessor.modifiers,
                        accessor.name,
                    ) {
                        needs.has = true;
                        needs.get = true;
                    }
                }
                k if k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if self.member_shape_is_decorated_static_private(
                        &accessor.modifiers,
                        accessor.name,
                    ) {
                        needs.has = true;
                        needs.set = true;
                    }
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(property) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if self.member_shape_is_decorated_static_private(
                        &property.modifiers,
                        property.name,
                    ) {
                        needs.has = true;
                        needs.get = true;
                        needs.set = true;
                    }
                }
                _ => {}
            }
        }
        needs
    }

    fn member_shape_is_decorated_static_private(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
        name: NodeIndex,
    ) -> bool {
        self.arena.is_static(modifiers)
            && self
                .arena
                .get(name)
                .is_some_and(|name_node| name_node.kind == SyntaxKind::PrivateIdentifier as u16)
            && self.modifiers_contain_decorator(modifiers)
    }

    fn modifiers_contain_decorator(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        modifiers.as_ref().is_some_and(|mods| {
            mods.nodes.iter().any(|&mod_idx| {
                self.arena
                    .get(mod_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::DECORATOR)
            })
        })
    }

    fn class_has_plain_static_auto_accessor_member(&self, class_data: &ClassData) -> bool {
        class_data.members.nodes.iter().any(|&member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                return false;
            }
            let Some(property) = self.arena.get_property_decl(member_node) else {
                return false;
            };
            self.arena.is_static(&property.modifiers)
                && self
                    .arena
                    .has_modifier(&property.modifiers, SyntaxKind::AccessorKeyword)
                && !self
                    .arena
                    .has_modifier(&property.modifiers, SyntaxKind::AbstractKeyword)
                && !self
                    .arena
                    .has_modifier(&property.modifiers, SyntaxKind::DeclareKeyword)
                && !property.modifiers.as_ref().is_some_and(|mods| {
                    mods.nodes.iter().any(|&mod_idx| {
                        self.arena
                            .get(mod_idx)
                            .is_some_and(|node| node.kind == syntax_kind_ext::DECORATOR)
                    })
                })
        })
    }

    pub(super) fn is_tc39_decorated_anonymous_class_expression(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return false;
        }
        let Some(class_data) = self.arena.get_class(node) else {
            return false;
        };
        let target_supports_native_decorators = self.ctx.options.target
            == tsz_common::ScriptTarget::ESNext
            && self.ctx.options.use_define_for_class_fields;
        class_data.name.is_none()
            && !self.ctx.options.legacy_decorators
            && !target_supports_native_decorators
            && self.class_has_decorators(class_data)
    }
}
