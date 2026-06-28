//! Class declaration conversion helpers for the ES5 class `AstToIr` bridge.

use super::*;

impl<'a> AstToIr<'a> {
    pub(super) fn convert_class_declaration(&self, idx: NodeIndex) -> IRNode {
        let mut transformer = ES5ClassTransformer::new(self.arena);
        transformer.set_module_kind(self.module_kind);
        transformer.set_es_module_interop(self.es_module_interop);
        transformer.set_target_es5(self.target_es5);
        transformer.set_dynamic_import_promise_counter(self.dynamic_import_promise_counter.get());
        transformer.set_indent_base(self.class_transformer_indent_base);
        transformer.set_downlevel_iteration(self.downlevel_iteration);
        if let Some(transforms) = self.transforms.clone() {
            transformer.set_transforms(transforms);
        }
        if !self.has_tc39_decorator_directive(idx) {
            let class_decorators = self.collect_class_decorators(idx);
            let has_member_decorators = self.class_has_member_decorators(idx);
            if !class_decorators.is_empty() || has_member_decorators {
                transformer.set_class_decorators(class_decorators);
                transformer.set_legacy_decorators(has_member_decorators);
            }
        }
        if let Some(source_text) = self.source_text {
            transformer.set_source_text(source_text);
        }
        if self.this_captured.get() {
            transformer.set_extends_this_captured(true);
        }
        if self.has_super && !self.is_static.get() {
            transformer.set_inherited_computed_name_super(self.super_name.clone());
        }

        if let Some(ir) = transformer.transform_class_to_ir(idx) {
            self.dynamic_import_promise_counter
                .set(transformer.dynamic_import_promise_counter());
            return ir;
        }

        IRNode::ASTRef(idx)
    }

    pub(super) fn class_expression_has_computed_name_super(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        let Some(class_data) = self.arena.get_class(node) else {
            return false;
        };
        class_data.members.nodes.iter().any(|&member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };
            let name = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .arena
                    .get_method_decl(member_node)
                    .map(|method| method.name),
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                    .arena
                    .get_property_decl(member_node)
                    .map(|property| property.name),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.arena
                        .get_accessor(member_node)
                        .map(|accessor| accessor.name)
                }
                _ => None,
            };
            let Some(name) = name else {
                return false;
            };
            let Some(name_node) = self.arena.get(name) else {
                return false;
            };
            if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                return false;
            }
            self.arena
                .get_computed_property(name_node)
                .is_some_and(|computed| contains_super_reference(self.arena, computed.expression))
        })
    }

    pub(super) fn class_expression_has_computed_name_this(&self, idx: NodeIndex) -> bool {
        !collect_class_computed_name_this_references(self.arena, idx).is_empty()
    }

    fn has_tc39_decorator_directive(&self, idx: NodeIndex) -> bool {
        self.transforms
            .as_ref()
            .and_then(|transforms| transforms.get(idx))
            .is_some_and(Self::directive_is_tc39_decorators)
    }

    fn directive_is_tc39_decorators(directive: &TransformDirective) -> bool {
        match directive {
            TransformDirective::TC39Decorators { .. } => true,
            TransformDirective::Chain(items) => {
                items.iter().any(Self::directive_is_tc39_decorators)
            }
            _ => false,
        }
    }

    fn collect_class_decorators(&self, idx: NodeIndex) -> Vec<NodeIndex> {
        let Some(node) = self.arena.get(idx) else {
            return Vec::new();
        };
        let Some(class_data) = self.arena.get_class(node) else {
            return Vec::new();
        };
        Self::collect_decorators_from_modifiers(self.arena, class_data.modifiers.as_ref())
    }

    fn class_has_member_decorators(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        let Some(class_data) = self.arena.get_class(node) else {
            return false;
        };
        class_data
            .members
            .nodes
            .iter()
            .any(|&member_idx| self.member_has_decorator(member_idx))
    }

    fn member_has_decorator(&self, member_idx: NodeIndex) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return false;
        };

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
        if !Self::collect_decorators_from_modifiers(self.arena, modifiers).is_empty() {
            return true;
        }

        let parameters = match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .arena
                .get_method_decl(member_node)
                .map(|method| &method.parameters),
            k if k == syntax_kind_ext::CONSTRUCTOR => self
                .arena
                .get_constructor(member_node)
                .map(|ctor| &ctor.parameters),
            _ => None,
        };

        parameters.is_some_and(|params| {
            params.nodes.iter().any(|&param_idx| {
                let Some(param_node) = self.arena.get(param_idx) else {
                    return false;
                };
                let Some(param) = self.arena.get_parameter(param_node) else {
                    return false;
                };
                !Self::collect_decorators_from_modifiers(self.arena, param.modifiers.as_ref())
                    .is_empty()
            })
        })
    }

    fn collect_decorators_from_modifiers(
        arena: &NodeArena,
        modifiers: Option<&NodeList>,
    ) -> Vec<NodeIndex> {
        modifiers
            .map(|m| {
                m.nodes
                    .iter()
                    .copied()
                    .filter(|&mod_idx| {
                        arena
                            .get(mod_idx)
                            .is_some_and(|node| node.kind == syntax_kind_ext::DECORATOR)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
