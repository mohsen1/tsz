//! `NodeArena` constructors for type-syntax nodes (type references, composite
//! types, function types, type queries, type literals, array/tuple/wrapped
//! types, conditional/infer/operator/indexed-access/mapped types, literal and
//! template-literal types, named tuple members, and type predicates).

use crate::parser::base::NodeIndex;
use crate::parser::node::{
    ArrayTypeData, CompositeTypeData, ConditionalTypeData, ExtendedNodeInfo, FunctionTypeData,
    IndexedAccessTypeData, InferTypeData, LiteralTypeData, MappedTypeData, NamedTupleMemberData,
    Node, NodeArena, TemplateLiteralTypeData, TupleTypeData, TypeLiteralData, TypeOperatorData,
    TypePredicateData, TypeQueryData, TypeRefData, WrappedTypeData,
};

impl NodeArena {
    /// Add a type reference node
    pub fn add_type_ref(&mut self, kind: u16, pos: u32, end: u32, data: TypeRefData) -> NodeIndex {
        let type_name = data.type_name;
        let type_arguments = data.type_arguments.clone();
        let data_index = self.len_u32(self.type_refs.len());
        self.type_refs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(type_name, parent);
        self.set_parent_opt_list(type_arguments.as_ref(), parent);
        parent
    }

    /// Add a union/intersection type node
    pub fn add_composite_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CompositeTypeData,
    ) -> NodeIndex {
        let types = data.types.clone();

        let data_index = self.len_u32(self.composite_types.len());
        self.composite_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_list(&types, parent);

        parent
    }

    /// Add a function/constructor type node
    pub fn add_function_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: FunctionTypeData,
    ) -> NodeIndex {
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;

        let data_index = self.len_u32(self.function_types.len());
        self.function_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);

        parent
    }

    /// Add a type query node (typeof)
    pub fn add_type_query(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeQueryData,
    ) -> NodeIndex {
        let expr_name = data.expr_name;
        let type_arguments = data.type_arguments.clone();
        let data_index = self.len_u32(self.type_queries.len());
        self.type_queries.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expr_name, parent);
        self.set_parent_opt_list(type_arguments.as_ref(), parent);
        parent
    }

    /// Add a type literal node
    pub fn add_type_literal(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeLiteralData,
    ) -> NodeIndex {
        let members = data.members.clone();
        let data_index = self.len_u32(self.type_literals.len());
        self.type_literals.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&members, parent);
        parent
    }

    /// Add an array type node
    pub fn add_array_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ArrayTypeData,
    ) -> NodeIndex {
        let element_type = data.element_type;
        let data_index = self.len_u32(self.array_types.len());
        self.array_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(element_type, parent);
        parent
    }

    /// Add a tuple type node
    pub fn add_tuple_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TupleTypeData,
    ) -> NodeIndex {
        let elements = data.elements.clone();
        let data_index = self.len_u32(self.tuple_types.len());
        self.tuple_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add an optional/rest type node
    pub fn add_wrapped_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: WrappedTypeData,
    ) -> NodeIndex {
        let type_node = data.type_node;
        let data_index = self.len_u32(self.wrapped_types.len());
        self.wrapped_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(type_node, parent);
        parent
    }

    /// Add a conditional type node
    pub fn add_conditional_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ConditionalTypeData,
    ) -> NodeIndex {
        let check_type = data.check_type;
        let extends_type = data.extends_type;
        let true_type = data.true_type;
        let false_type = data.false_type;
        let data_index = self.len_u32(self.conditional_types.len());
        self.conditional_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(check_type, parent);
        self.set_parent(extends_type, parent);
        self.set_parent(true_type, parent);
        self.set_parent(false_type, parent);
        parent
    }

    /// Add an infer type node
    pub fn add_infer_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: InferTypeData,
    ) -> NodeIndex {
        let type_parameter = data.type_parameter;
        let data_index = self.len_u32(self.infer_types.len());
        self.infer_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(type_parameter, parent);
        parent
    }

    /// Add a type operator node (keyof, unique, readonly)
    pub fn add_type_operator(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeOperatorData,
    ) -> NodeIndex {
        let type_node = data.type_node;
        let data_index = self.len_u32(self.type_operators.len());
        self.type_operators.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(type_node, parent);
        parent
    }

    /// Add an indexed access type node
    pub fn add_indexed_access_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IndexedAccessTypeData,
    ) -> NodeIndex {
        let object_type = data.object_type;
        let index_type = data.index_type;
        let data_index = self.len_u32(self.indexed_access_types.len());
        self.indexed_access_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(object_type, parent);
        self.set_parent(index_type, parent);
        parent
    }

    /// Add a mapped type node
    pub fn add_mapped_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: MappedTypeData,
    ) -> NodeIndex {
        let readonly_token = data.readonly_token;
        let type_parameter = data.type_parameter;
        let name_type = data.name_type;
        let question_token = data.question_token;
        let type_node = data.type_node;
        let members = data.members.clone();
        let data_index = self.len_u32(self.mapped_types.len());
        self.mapped_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(readonly_token, parent);
        self.set_parent(type_parameter, parent);
        self.set_parent(name_type, parent);
        self.set_parent(question_token, parent);
        self.set_parent(type_node, parent);
        self.set_parent_opt_list(members.as_ref(), parent);
        parent
    }

    /// Add a literal type node
    pub fn add_literal_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: LiteralTypeData,
    ) -> NodeIndex {
        let literal = data.literal;
        let data_index = self.len_u32(self.literal_types.len());
        self.literal_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(literal, parent);
        parent
    }

    /// Add a template literal type node
    pub fn add_template_literal_type(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TemplateLiteralTypeData,
    ) -> NodeIndex {
        let head = data.head;
        let template_spans = data.template_spans.clone();
        let data_index = self.len_u32(self.template_literal_types.len());
        self.template_literal_types.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(head, parent);
        self.set_parent_list(&template_spans, parent);
        parent
    }

    /// Add a named tuple member node
    pub fn add_named_tuple_member(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: NamedTupleMemberData,
    ) -> NodeIndex {
        let name = data.name;
        let type_node = data.type_node;

        let data_index = self.len_u32(self.named_tuple_members.len());
        self.named_tuple_members.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(type_node, parent);
        parent
    }

    /// Add a type predicate node
    pub fn add_type_predicate(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypePredicateData,
    ) -> NodeIndex {
        let parameter_name = data.parameter_name;
        let type_node = data.type_node;

        let data_index = self.len_u32(self.type_predicates.len());
        self.type_predicates.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(parameter_name, parent);
        self.set_parent(type_node, parent);
        parent
    }
}
