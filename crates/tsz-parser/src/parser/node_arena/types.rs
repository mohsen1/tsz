//! `NodeArena` constructors for type-syntax nodes (type references, composite
//! types, function types, type queries, type literals, array/tuple/wrapped
//! types, conditional/infer/operator/indexed-access/mapped types, literal and
//! template-literal types, named tuple members, and type predicates).

use super::push_data_node;
use crate::parser::base::NodeIndex;
use crate::parser::node::{
    ArrayTypeData, CompositeTypeData, ConditionalTypeData, FunctionTypeData, IndexedAccessTypeData,
    InferTypeData, LiteralTypeData, MappedTypeData, NamedTupleMemberData, NodeArenaInner,
    TemplateLiteralTypeData, TupleTypeData, TypeLiteralData, TypeOperatorData, TypePredicateData,
    TypeQueryData, TypeRefData, WrappedTypeData,
};

impl NodeArenaInner {
    /// Add a type reference node
    pub fn add_type_ref(&mut self, kind: u16, pos: u32, end: u32, data: TypeRefData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.type_name, parent);
        self.set_parent_opt_list(data.type_arguments.as_ref(), parent);
        push_data_node!(self, parent, kind, pos, end, type_refs, data)
    }

    /// Add a union/intersection type node
    pub fn add_composite_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CompositeTypeData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_list(&data.types, parent);
        push_data_node!(self, parent, kind, pos, end, composite_types, data)
    }

    /// Add a function/constructor type node
    pub fn add_function_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: FunctionTypeData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.type_parameters.as_ref(), parent);
        self.set_parent_list(&data.parameters, parent);
        self.set_parent(data.type_annotation, parent);
        push_data_node!(self, parent, kind, pos, end, function_types, data)
    }

    /// Add a type query node (typeof)
    pub fn add_type_query(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeQueryData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expr_name, parent);
        self.set_parent_opt_list(data.type_arguments.as_ref(), parent);
        push_data_node!(self, parent, kind, pos, end, type_queries, data)
    }

    /// Add a type literal node
    pub fn add_type_literal(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeLiteralData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_list(&data.members, parent);
        push_data_node!(self, parent, kind, pos, end, type_literals, data)
    }

    /// Add an array type node
    pub fn add_array_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ArrayTypeData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.element_type, parent);
        push_data_node!(self, parent, kind, pos, end, array_types, data)
    }

    /// Add a tuple type node
    pub fn add_tuple_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TupleTypeData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_list(&data.elements, parent);
        push_data_node!(self, parent, kind, pos, end, tuple_types, data)
    }

    /// Add an optional/rest type node
    pub fn add_wrapped_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: WrappedTypeData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.type_node, parent);
        push_data_node!(self, parent, kind, pos, end, wrapped_types, data)
    }

    /// Add a conditional type node
    pub fn add_conditional_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ConditionalTypeData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.check_type, parent);
        self.set_parent(data.extends_type, parent);
        self.set_parent(data.true_type, parent);
        self.set_parent(data.false_type, parent);
        push_data_node!(self, parent, kind, pos, end, conditional_types, data)
    }

    /// Add an infer type node
    pub fn add_infer_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: InferTypeData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.type_parameter, parent);
        push_data_node!(self, parent, kind, pos, end, infer_types, data)
    }

    /// Add a type operator node (keyof, unique, readonly)
    pub fn add_type_operator(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeOperatorData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.type_node, parent);
        push_data_node!(self, parent, kind, pos, end, type_operators, data)
    }

    /// Add an indexed access type node
    pub fn add_indexed_access_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IndexedAccessTypeData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.object_type, parent);
        self.set_parent(data.index_type, parent);
        push_data_node!(self, parent, kind, pos, end, indexed_access_types, data)
    }

    /// Add a mapped type node
    pub fn add_mapped_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: MappedTypeData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.readonly_token, parent);
        self.set_parent(data.type_parameter, parent);
        self.set_parent(data.name_type, parent);
        self.set_parent(data.question_token, parent);
        self.set_parent(data.type_node, parent);
        self.set_parent_opt_list(data.members.as_ref(), parent);
        push_data_node!(self, parent, kind, pos, end, mapped_types, data)
    }

    /// Add a literal type node
    pub fn add_literal_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: LiteralTypeData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.literal, parent);
        push_data_node!(self, parent, kind, pos, end, literal_types, data)
    }

    /// Add a template literal type node
    pub fn add_template_literal_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TemplateLiteralTypeData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.head, parent);
        self.set_parent_list(&data.template_spans, parent);
        push_data_node!(self, parent, kind, pos, end, template_literal_types, data)
    }

    /// Add a named tuple member node
    pub fn add_named_tuple_member(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: NamedTupleMemberData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.name, parent);
        self.set_parent(data.type_node, parent);
        push_data_node!(self, parent, kind, pos, end, named_tuple_members, data)
    }

    /// Add a type predicate node
    pub fn add_type_predicate(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypePredicateData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.parameter_name, parent);
        self.set_parent(data.type_node, parent);
        push_data_node!(self, parent, kind, pos, end, type_predicates, data)
    }
}
