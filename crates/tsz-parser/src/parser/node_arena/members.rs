//! `NodeArena` constructors for class/interface/function member nodes
//! (signatures, index signatures, property/method/constructor/accessor
//! declarations, parameters, type parameters, decorators, and heritage
//! clauses).

use crate::parser::base::NodeIndex;
use crate::parser::node::{
    AccessorData, ConstructorData, DecoratorData, ExtendedNodeInfo, HeritageData,
    IndexSignatureData, MethodDeclData, Node, NodeArena, ParameterData, PropertyDeclData,
    SignatureData, TypeParameterData,
};

impl NodeArena {
    /// Add a signature node (property/method signature)
    pub fn add_signature(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: SignatureData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;

        let data_index = self.len_u32(self.signatures.len());
        self.signatures.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_opt_list(parameters.as_ref(), parent);
        self.set_parent(type_annotation, parent);

        parent
    }

    /// Add an index signature node
    pub fn add_index_signature(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IndexSignatureData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;

        let data_index = self.len_u32(self.index_signatures.len());
        self.index_signatures.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);

        parent
    }

    /// Add a property declaration node
    pub fn add_property_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: PropertyDeclData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_annotation = data.type_annotation;
        let initializer = data.initializer;

        let data_index = self.len_u32(self.property_decls.len());
        self.property_decls.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(initializer, parent);
        parent
    }

    /// Add a method declaration node
    pub fn add_method_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: MethodDeclData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;
        let body = data.body;

        let data_index = self.len_u32(self.method_decls.len());
        self.method_decls.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(body, parent);
        parent
    }

    /// Add a constructor declaration node
    pub fn add_constructor(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ConstructorData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let body = data.body;

        let data_index = self.len_u32(self.constructors.len());
        self.constructors.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(body, parent);
        parent
    }

    /// Add an accessor declaration node (get/set)
    pub fn add_accessor(&mut self, kind: u16, pos: u32, end: u32, data: AccessorData) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let type_parameters = data.type_parameters.clone();
        let parameters = data.parameters.clone();
        let type_annotation = data.type_annotation;
        let body = data.body;

        let data_index = self.len_u32(self.accessors.len());
        self.accessors.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent_opt_list(type_parameters.as_ref(), parent);
        self.set_parent_list(&parameters, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(body, parent);

        parent
    }

    /// Add a parameter declaration node
    pub fn add_parameter(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ParameterData,
    ) -> NodeIndex {
        let name = data.name;
        let type_annotation = data.type_annotation;
        let initializer = data.initializer;
        let modifiers = data.modifiers.clone();
        let data_index = self.len_u32(self.parameters.len());
        self.parameters.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        // Set parent pointers for children
        self.set_parent(name, parent);
        self.set_parent(type_annotation, parent);
        self.set_parent(initializer, parent);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        parent
    }

    /// Add a type parameter declaration node
    pub fn add_type_parameter(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeParameterData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let constraint = data.constraint;
        let default = data.default;

        let data_index = self.len_u32(self.type_parameters.len());
        self.type_parameters.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent(constraint, parent);
        self.set_parent(default, parent);

        parent
    }

    /// Add a decorator node
    pub fn add_decorator(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: DecoratorData,
    ) -> NodeIndex {
        let expression = data.expression;
        let data_index = self.len_u32(self.decorators.len());
        self.decorators.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a heritage clause node
    pub fn add_heritage(&mut self, kind: u16, pos: u32, end: u32, data: HeritageData) -> NodeIndex {
        let types = data.types.clone();
        let data_index = self.len_u32(self.heritage_clauses.len());
        self.heritage_clauses.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&types, parent);
        parent
    }
}
