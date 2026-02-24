use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn infer_type_from_binding_pattern(
        &mut self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
    ) -> TypeId {
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return TypeId::ANY;
        };

        let factory = self.ctx.types.factory();

        if pattern_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
            let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
                return TypeId::ANY;
            };

            let mut properties = Vec::new();

            for (i, &element_idx) in pattern_data.elements.nodes.iter().enumerate() {
                if let Some(element_node) = self.ctx.arena.get(element_idx)
                    && let Some(element_data) = self.ctx.arena.get_binding_element(element_node)
                {
                    // Compute property name
                    let name_str = if element_data.property_name.is_some() {
                        let prop_name_idx = element_data.property_name;
                        if let Some(prop_name_node) = self.ctx.arena.get(prop_name_idx) {
                            if let Some(ident) = self.ctx.arena.get_identifier(prop_name_node) {
                                ident.escaped_text.clone()
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    } else if let Some(name_node) = self.ctx.arena.get(element_data.name) {
                        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                            ident.escaped_text.clone()
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    };

                    let atom = self.ctx.types.intern_string(&name_str);

                    let mut element_type =
                        self.get_binding_element_type(pattern_idx, i, parent_type, element_data);

                    if element_data.initializer.is_some() {
                        let init_type = self.get_type_of_node(element_data.initializer);
                        if element_type == TypeId::ANY || element_type == TypeId::UNKNOWN {
                            element_type = init_type;
                        } else if !self.is_assignable_to(init_type, element_type) {
                            element_type = self
                                .ctx
                                .types
                                .factory()
                                .union(vec![element_type, init_type]);
                        }
                    } else if element_type == TypeId::ANY
                        && let Some(name_node) = self.ctx.arena.get(element_data.name)
                        && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                    {
                        element_type =
                            self.infer_type_from_binding_pattern(element_data.name, element_type);
                    }

                    let is_optional =
                        element_data.initializer.is_some() || element_data.dot_dot_dot_token;

                    let mut prop_info = tsz_solver::PropertyInfo::new(atom, element_type);
                    prop_info.optional = is_optional;
                    properties.push(prop_info);
                }
            }
            return factory.object(properties);
        } else if pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
                return TypeId::ANY;
            };

            let mut elements = Vec::new();

            for (i, &element_idx) in pattern_data.elements.nodes.iter().enumerate() {
                if element_idx.is_none() {
                    elements.push(tsz_solver::TupleElement {
                        type_id: TypeId::ANY,
                        optional: true,
                        rest: false,
                        name: None,
                    });
                    continue;
                }

                if let Some(element_node) = self.ctx.arena.get(element_idx) {
                    if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                        elements.push(tsz_solver::TupleElement {
                            type_id: TypeId::ANY,
                            optional: true,
                            rest: false,
                            name: None,
                        });
                        continue;
                    }

                    if let Some(element_data) = self.ctx.arena.get_binding_element(element_node) {
                        let mut element_type = self.get_binding_element_type(
                            pattern_idx,
                            i,
                            parent_type,
                            element_data,
                        );

                        if element_data.initializer.is_some() {
                            let init_type = self.get_type_of_node(element_data.initializer);
                            if element_type == TypeId::ANY || element_type == TypeId::UNKNOWN {
                                element_type = init_type;
                            } else if !self.is_assignable_to(init_type, element_type) {
                                element_type = self
                                    .ctx
                                    .types
                                    .factory()
                                    .union(vec![element_type, init_type]);
                            }
                        } else if element_type == TypeId::ANY
                            && let Some(name_node) = self.ctx.arena.get(element_data.name)
                            && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                        {
                            element_type = self
                                .infer_type_from_binding_pattern(element_data.name, element_type);
                        }

                        let is_optional =
                            element_data.initializer.is_some() || element_data.dot_dot_dot_token;

                        elements.push(tsz_solver::TupleElement {
                            type_id: element_type,
                            optional: is_optional,
                            rest: element_data.dot_dot_dot_token,
                            name: None,
                        });
                    }
                }
            }
            return factory.tuple(elements);
        }

        TypeId::ANY
    }
}
