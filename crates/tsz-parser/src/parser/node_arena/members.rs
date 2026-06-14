//! `NodeArena` constructors for class/interface/function member nodes
//! (signatures, index signatures, property/method/constructor/accessor
//! declarations, parameters, type parameters, decorators, and heritage
//! clauses).

use super::push_data_node;
use crate::parser::base::NodeIndex;
use crate::parser::node::{
    AccessorData, ConstructorData, DecoratorData, HeritageData, IndexSignatureData, MethodDeclData,
    NodeArenaInner, ParameterData, PropertyDeclData, SignatureData, TypeParameterData,
};

impl NodeArenaInner {
    /// Add a signature node (property/method signature)
    pub fn add_signature(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: SignatureData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.name, parent);
        self.set_parent_opt_list(data.type_parameters.as_ref(), parent);
        self.set_parent_opt_list(data.parameters.as_ref(), parent);
        self.set_parent(data.type_annotation, parent);
        push_data_node!(self, parent, kind, pos, end, signatures, data)
    }

    /// Add an index signature node
    pub fn add_index_signature(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: IndexSignatureData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent_list(&data.parameters, parent);
        self.set_parent(data.type_annotation, parent);
        push_data_node!(self, parent, kind, pos, end, index_signatures, data)
    }

    /// Add a property declaration node
    pub fn add_property_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: PropertyDeclData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.name, parent);
        self.set_parent(data.type_annotation, parent);
        self.set_parent(data.initializer, parent);
        push_data_node!(self, parent, kind, pos, end, property_decls, data)
    }

    /// Add a method declaration node
    pub fn add_method_decl(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: MethodDeclData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.name, parent);
        self.set_parent_opt_list(data.type_parameters.as_ref(), parent);
        self.set_parent_list(&data.parameters, parent);
        self.set_parent(data.type_annotation, parent);
        self.set_parent(data.body, parent);
        push_data_node!(self, parent, kind, pos, end, method_decls, data)
    }

    /// Add a constructor declaration node
    pub fn add_constructor(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ConstructorData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent_opt_list(data.type_parameters.as_ref(), parent);
        self.set_parent_list(&data.parameters, parent);
        self.set_parent(data.body, parent);
        push_data_node!(self, parent, kind, pos, end, constructors, data)
    }

    /// Add an accessor declaration node (get/set)
    pub fn add_accessor(&mut self, kind: u16, pos: u32, end: u32, data: AccessorData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.name, parent);
        self.set_parent_opt_list(data.type_parameters.as_ref(), parent);
        self.set_parent_list(&data.parameters, parent);
        self.set_parent(data.type_annotation, parent);
        self.set_parent(data.body, parent);
        push_data_node!(self, parent, kind, pos, end, accessors, data)
    }

    /// Add a parameter declaration node
    pub fn add_parameter(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ParameterData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        // Set parent pointers for children
        self.set_parent(data.name, parent);
        self.set_parent(data.type_annotation, parent);
        self.set_parent(data.initializer, parent);
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        push_data_node!(self, parent, kind, pos, end, parameters, data)
    }

    /// Add a type parameter declaration node
    pub fn add_type_parameter(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeParameterData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.name, parent);
        self.set_parent(data.constraint, parent);
        self.set_parent(data.default, parent);
        push_data_node!(self, parent, kind, pos, end, type_parameters, data)
    }

    /// Add a decorator node
    pub fn add_decorator(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: DecoratorData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        push_data_node!(self, parent, kind, pos, end, decorators, data)
    }

    /// Add a heritage clause node
    pub fn add_heritage(&mut self, kind: u16, pos: u32, end: u32, data: HeritageData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_list(&data.types, parent);
        push_data_node!(self, parent, kind, pos, end, heritage_clauses, data)
    }
}
