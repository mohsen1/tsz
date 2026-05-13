use crate::types_domain::type_node::TypeNodeChecker;
use crate::types_domain::type_node_helpers::get_string_literal_from_type_index;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    pub(super) fn try_fast_alias_union_literal_index_access(
        &mut self,
        object_node_idx: NodeIndex,
        index_node_idx: NodeIndex,
    ) -> Option<TypeId> {
        let property_name = get_string_literal_from_type_index(self.ctx.arena, index_node_idx)?;
        let object_body_idx = self.non_generic_type_alias_body_from_node(object_node_idx)?;
        let mut property_types = Vec::new();
        self.collect_alias_union_property_types(
            object_body_idx,
            &property_name,
            &mut property_types,
        )?;
        (!property_types.is_empty()).then(|| self.ctx.types.factory().union(property_types))
    }

    fn collect_alias_union_property_types(
        &mut self,
        object_node_idx: NodeIndex,
        property_name: &str,
        property_types: &mut Vec<TypeId>,
    ) -> Option<()> {
        let object_node = self.ctx.arena.get(object_node_idx)?;

        if object_node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            let wrapped = self.ctx.arena.get_wrapped_type(object_node)?;
            return self.collect_alias_union_property_types(
                wrapped.type_node,
                property_name,
                property_types,
            );
        }

        if object_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let body_idx = self.non_generic_type_alias_body(object_node)?;
            return self.collect_alias_union_property_types(
                body_idx,
                property_name,
                property_types,
            );
        }

        if object_node.kind == syntax_kind_ext::UNION_TYPE {
            let composite = self.ctx.arena.get_composite_type(object_node)?;
            if composite.types.nodes.is_empty() {
                return None;
            }
            for &member_idx in &composite.types.nodes {
                self.collect_alias_union_property_types(member_idx, property_name, property_types)?;
            }
            return Some(());
        }

        if object_node.kind == syntax_kind_ext::TYPE_LITERAL {
            let property_type =
                self.type_literal_declared_property_type(object_node, property_name)?;
            property_types.push(property_type);
            return Some(());
        }

        None
    }

    fn non_generic_type_alias_body(&self, object_node: &Node) -> Option<NodeIndex> {
        let type_ref = self.ctx.arena.get_type_ref(object_node)?;
        if type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return None;
        }

        let sym_id = tsz_binder::SymbolId(self.resolve_type_symbol(type_ref.type_name)?);
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS)
            || symbol.declarations.len() != 1
        {
            return None;
        }

        let decl_node = self.ctx.arena.get(symbol.declarations[0])?;
        let type_alias = self.ctx.arena.get_type_alias(decl_node)?;
        if type_alias
            .type_parameters
            .as_ref()
            .is_some_and(|params| !params.nodes.is_empty())
        {
            return None;
        }
        Some(type_alias.type_node)
    }

    fn non_generic_type_alias_body_from_node(&self, mut node_idx: NodeIndex) -> Option<NodeIndex> {
        loop {
            let object_node = self.ctx.arena.get(node_idx)?;
            if object_node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
                node_idx = self.ctx.arena.get_wrapped_type(object_node)?.type_node;
                continue;
            }
            return self.non_generic_type_alias_body(object_node);
        }
    }

    fn type_literal_declared_property_type(
        &mut self,
        type_literal_node: &Node,
        property_name: &str,
    ) -> Option<TypeId> {
        let type_literal = self.ctx.arena.get_type_literal(type_literal_node)?;
        for &member_idx in &type_literal.members.nodes {
            let member_node = self.ctx.arena.get(member_idx)?;
            if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                continue;
            }
            let signature = self.ctx.arena.get_signature(member_node)?;
            if crate::types_domain::queries::core::get_literal_property_name(
                self.ctx.arena,
                signature.name,
            )
            .as_deref()
                == Some(property_name)
            {
                if signature.question_token || signature.type_annotation.is_none() {
                    return None;
                }
                let type_annotation = signature.type_annotation;
                return Some(self.check(type_annotation));
            }
        }
        None
    }
}
