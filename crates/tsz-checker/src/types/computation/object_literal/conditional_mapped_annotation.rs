//! Object-literal annotation predicates for conditional mapped types.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

type AnnotationTypeNodeSet = rustc_hash::FxHashSet<(usize, NodeIndex)>;

#[derive(Default)]
struct ObjectLiteralAnnotationWalkState {
    type_nodes: AnnotationTypeNodeSet,
    aliases: rustc_hash::FxHashSet<SymbolId>,
}

impl ObjectLiteralAnnotationWalkState {
    fn enter_type_node(&mut self, arena: &NodeArena, type_node_idx: NodeIndex) -> bool {
        !type_node_idx.is_none()
            && self
                .type_nodes
                .insert((arena as *const NodeArena as usize, type_node_idx))
    }

    fn enter_alias(&mut self, sym_id: SymbolId) -> bool {
        self.aliases.insert(sym_id)
    }
}

impl<'a> CheckerState<'a> {
    pub(crate) fn object_literal_property_has_conditional_mapped_annotation(
        &self,
        property_elem_idx: NodeIndex,
    ) -> bool {
        self.object_literal_property_annotation_satisfies(
            property_elem_idx,
            |checker, type_node| {
                let mut walk_state = ObjectLiteralAnnotationWalkState::default();
                checker.type_node_contains_conditional_mapped_value_template(
                    checker.ctx.arena,
                    type_node,
                    &mut walk_state,
                )
            },
        )
    }

    pub(crate) fn object_literal_property_has_conditional_annotation(
        &self,
        property_elem_idx: NodeIndex,
    ) -> bool {
        self.object_literal_property_annotation_satisfies(
            property_elem_idx,
            |checker, type_node| {
                let mut walk_state = ObjectLiteralAnnotationWalkState::default();
                checker.type_node_contains_conditional(
                    checker.ctx.arena,
                    type_node,
                    &mut walk_state,
                )
            },
        )
    }

    pub(crate) fn object_literal_property_has_mapped_annotation(
        &self,
        property_elem_idx: NodeIndex,
    ) -> bool {
        self.object_literal_property_annotation_satisfies(
            property_elem_idx,
            |checker, type_node| {
                let mut walk_state = ObjectLiteralAnnotationWalkState::default();
                checker.type_node_contains_mapped_value_template(
                    checker.ctx.arena,
                    type_node,
                    &mut walk_state,
                )
            },
        )
    }

    fn object_literal_property_annotation_satisfies(
        &self,
        property_elem_idx: NodeIndex,
        predicate: impl FnOnce(&Self, NodeIndex) -> bool,
    ) -> bool {
        let Some(object_idx) = self.ctx.arena.parent_of(property_elem_idx) else {
            return false;
        };
        let Some(parent_idx) = self.ctx.arena.parent_of(object_idx) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(parent_node) else {
            return false;
        };
        if var_decl.initializer != object_idx || var_decl.type_annotation.is_none() {
            return false;
        }

        predicate(self, var_decl.type_annotation)
    }

    fn type_node_contains_conditional_mapped_value_template(
        &self,
        arena: &NodeArena,
        type_node_idx: NodeIndex,
        walk_state: &mut ObjectLiteralAnnotationWalkState,
    ) -> bool {
        if !walk_state.enter_type_node(arena, type_node_idx) {
            return false;
        }
        let Some(type_node) = arena.get(type_node_idx) else {
            return false;
        };

        if let Some(mapped) = arena.get_mapped_type(type_node)
            && mapped.type_node.is_some()
            && self.type_node_contains_conditional(arena, mapped.type_node, walk_state)
        {
            return true;
        }

        if let Some(type_ref) = arena.get_type_ref(type_node) {
            if self.type_reference_alias_body_contains_conditional_mapped_value_template(
                arena,
                type_ref.type_name,
                walk_state,
            ) {
                return true;
            }
            if let Some(args) = &type_ref.type_arguments
                && args.nodes.iter().copied().any(|arg| {
                    self.type_node_contains_conditional_mapped_value_template(
                        arena, arg, walk_state,
                    )
                })
            {
                return true;
            }
            return false;
        }

        self.type_node_children_contain_conditional_mapped_value_template(
            arena, type_node, walk_state,
        )
    }

    fn type_node_contains_conditional(
        &self,
        arena: &NodeArena,
        type_node_idx: NodeIndex,
        walk_state: &mut ObjectLiteralAnnotationWalkState,
    ) -> bool {
        if !walk_state.enter_type_node(arena, type_node_idx) {
            return false;
        }
        let Some(type_node) = arena.get(type_node_idx) else {
            return false;
        };

        if type_node.kind == syntax_kind_ext::CONDITIONAL_TYPE {
            return true;
        }

        if let Some(type_ref) = arena.get_type_ref(type_node)
            && self.type_reference_alias_body_contains_conditional(
                arena,
                type_ref.type_name,
                walk_state,
            )
        {
            return true;
        }

        self.type_node_children_contain_conditional(arena, type_node, walk_state)
    }

    fn type_reference_alias_body_contains_conditional_mapped_value_template(
        &self,
        arena: &NodeArena,
        type_name: NodeIndex,
        walk_state: &mut ObjectLiteralAnnotationWalkState,
    ) -> bool {
        let Some(sym_id) = self.type_reference_alias_symbol(arena, type_name) else {
            return false;
        };
        if !walk_state.enter_alias(sym_id) {
            return false;
        }

        self.any_type_alias_declaration_body(sym_id, |decl_arena, alias_type_node| {
            self.type_node_contains_conditional_mapped_value_template(
                decl_arena,
                alias_type_node,
                walk_state,
            )
        })
    }

    fn type_node_children_contain_conditional_mapped_value_template(
        &self,
        arena: &NodeArena,
        type_node: &Node,
        walk_state: &mut ObjectLiteralAnnotationWalkState,
    ) -> bool {
        let mut visit = |child| {
            self.type_node_contains_conditional_mapped_value_template(arena, child, walk_state)
        };

        if let Some(composite) = arena.get_composite_type(type_node) {
            return composite.types.nodes.iter().copied().any(visit);
        }
        if let Some(array) = arena.get_array_type(type_node) {
            return visit(array.element_type);
        }
        if let Some(tuple) = arena.get_tuple_type(type_node) {
            return tuple.elements.nodes.iter().copied().any(visit);
        }
        if let Some(wrapped) = arena.get_wrapped_type(type_node) {
            return visit(wrapped.type_node);
        }
        if let Some(indexed_access) = arena.get_indexed_access_type(type_node) {
            return visit(indexed_access.object_type) || visit(indexed_access.index_type);
        }
        if let Some(type_operator) = arena.get_type_operator(type_node) {
            return visit(type_operator.type_node);
        }
        if let Some(parenthesized) = arena.get_parenthesized(type_node) {
            return visit(parenthesized.expression);
        }

        false
    }

    fn type_node_contains_mapped_value_template(
        &self,
        arena: &NodeArena,
        type_node_idx: NodeIndex,
        walk_state: &mut ObjectLiteralAnnotationWalkState,
    ) -> bool {
        if !walk_state.enter_type_node(arena, type_node_idx) {
            return false;
        }
        let Some(type_node) = arena.get(type_node_idx) else {
            return false;
        };

        if arena
            .get_mapped_type(type_node)
            .is_some_and(|mapped| mapped.type_node.is_some())
        {
            return true;
        }

        if let Some(type_ref) = arena.get_type_ref(type_node) {
            if self.type_reference_alias_body_contains_mapped_value_template(
                arena,
                type_ref.type_name,
                walk_state,
            ) {
                return true;
            }
            if let Some(args) = &type_ref.type_arguments
                && args.nodes.iter().copied().any(|arg| {
                    self.type_node_contains_mapped_value_template(arena, arg, walk_state)
                })
            {
                return true;
            }
            return false;
        }

        self.type_node_children_contain_mapped_value_template(arena, type_node, walk_state)
    }

    fn type_reference_alias_body_contains_mapped_value_template(
        &self,
        arena: &NodeArena,
        type_name: NodeIndex,
        walk_state: &mut ObjectLiteralAnnotationWalkState,
    ) -> bool {
        let Some(sym_id) = self.type_reference_alias_symbol(arena, type_name) else {
            return false;
        };
        if !walk_state.enter_alias(sym_id) {
            return false;
        }

        self.any_type_alias_declaration_body(sym_id, |decl_arena, alias_type_node| {
            self.type_node_contains_mapped_value_template(decl_arena, alias_type_node, walk_state)
        })
    }

    fn type_node_children_contain_mapped_value_template(
        &self,
        arena: &NodeArena,
        type_node: &Node,
        walk_state: &mut ObjectLiteralAnnotationWalkState,
    ) -> bool {
        let mut visit =
            |child| self.type_node_contains_mapped_value_template(arena, child, walk_state);

        if let Some(composite) = arena.get_composite_type(type_node) {
            return composite.types.nodes.iter().copied().any(visit);
        }
        if let Some(array) = arena.get_array_type(type_node) {
            return visit(array.element_type);
        }
        if let Some(tuple) = arena.get_tuple_type(type_node) {
            return tuple.elements.nodes.iter().copied().any(visit);
        }
        if let Some(wrapped) = arena.get_wrapped_type(type_node) {
            return visit(wrapped.type_node);
        }
        if let Some(indexed_access) = arena.get_indexed_access_type(type_node) {
            return visit(indexed_access.object_type) || visit(indexed_access.index_type);
        }
        if let Some(type_operator) = arena.get_type_operator(type_node) {
            return visit(type_operator.type_node);
        }
        if let Some(parenthesized) = arena.get_parenthesized(type_node) {
            return visit(parenthesized.expression);
        }

        false
    }

    fn type_reference_alias_body_contains_conditional(
        &self,
        arena: &NodeArena,
        type_name: NodeIndex,
        walk_state: &mut ObjectLiteralAnnotationWalkState,
    ) -> bool {
        let Some(sym_id) = self.type_reference_alias_symbol(arena, type_name) else {
            return false;
        };
        if !walk_state.enter_alias(sym_id) {
            return false;
        }

        self.any_type_alias_declaration_body(sym_id, |decl_arena, alias_type_node| {
            self.type_node_contains_conditional(decl_arena, alias_type_node, walk_state)
        })
    }

    fn type_node_children_contain_conditional(
        &self,
        arena: &NodeArena,
        type_node: &Node,
        walk_state: &mut ObjectLiteralAnnotationWalkState,
    ) -> bool {
        let mut visit = |child| self.type_node_contains_conditional(arena, child, walk_state);

        if let Some(type_ref) = arena.get_type_ref(type_node)
            && let Some(args) = &type_ref.type_arguments
            && args.nodes.iter().copied().any(&mut visit)
        {
            return true;
        }
        if let Some(composite) = arena.get_composite_type(type_node) {
            return composite.types.nodes.iter().copied().any(&mut visit);
        }
        if let Some(array) = arena.get_array_type(type_node) {
            return visit(array.element_type);
        }
        if let Some(tuple) = arena.get_tuple_type(type_node) {
            return tuple.elements.nodes.iter().copied().any(&mut visit);
        }
        if let Some(wrapped) = arena.get_wrapped_type(type_node) {
            return visit(wrapped.type_node);
        }
        if let Some(conditional) = arena.get_conditional_type(type_node) {
            return visit(conditional.check_type)
                || visit(conditional.extends_type)
                || visit(conditional.true_type)
                || visit(conditional.false_type);
        }
        if let Some(indexed_access) = arena.get_indexed_access_type(type_node) {
            return visit(indexed_access.object_type) || visit(indexed_access.index_type);
        }
        if let Some(type_operator) = arena.get_type_operator(type_node) {
            return visit(type_operator.type_node);
        }
        if let Some(parenthesized) = arena.get_parenthesized(type_node) {
            return visit(parenthesized.expression);
        }

        false
    }

    fn type_reference_alias_symbol(
        &self,
        arena: &NodeArena,
        type_name: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let type_name_node = arena.get(type_name)?;
        if type_name_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        self.ctx.binder.resolve_identifier(arena, type_name)
    }

    fn any_type_alias_declaration_body(
        &self,
        sym_id: tsz_binder::SymbolId,
        mut predicate: impl FnMut(&NodeArena, NodeIndex) -> bool,
    ) -> bool {
        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && self.any_type_alias_body_in_binder(
                self.ctx.arena,
                self.ctx.binder,
                sym_id,
                symbol,
                &mut predicate,
            )
        {
            return true;
        }

        self.ctx.lib_contexts.iter().any(|lib_ctx| {
            lib_ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                self.any_type_alias_body_in_binder(
                    lib_ctx.arena.as_ref(),
                    lib_ctx.binder.as_ref(),
                    sym_id,
                    symbol,
                    &mut predicate,
                )
            })
        })
    }

    fn any_type_alias_body_in_binder(
        &self,
        fallback_arena: &NodeArena,
        binder: &tsz_binder::BinderState,
        sym_id: tsz_binder::SymbolId,
        symbol: &tsz_binder::Symbol,
        predicate: &mut impl FnMut(&NodeArena, NodeIndex) -> bool,
    ) -> bool {
        std::iter::once(symbol.value_declaration)
            .chain(symbol.declarations.iter().copied())
            .any(|decl_idx| {
                let arenas = binder
                    .declaration_arenas
                    .get(&(sym_id, decl_idx))
                    .map(|arenas| arenas.iter().map(std::convert::AsRef::as_ref).collect())
                    .unwrap_or_else(|| vec![fallback_arena]);

                arenas.into_iter().any(|arena| {
                    arena
                        .get(decl_idx)
                        .and_then(|decl_node| arena.get_type_alias(decl_node))
                        .is_some_and(|alias| predicate(arena, alias.type_node))
                })
            })
    }
}
