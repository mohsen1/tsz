//! `NodeArena` constructors for destructuring binding patterns and
//! object-literal property assignments / shorthand properties.

use crate::parser::base::NodeIndex;
use crate::parser::node::{
    BindingElementData, BindingPatternData, ExtendedNodeInfo, Node, NodeArena,
    PropertyAssignmentData, ShorthandPropertyData,
};

impl NodeArena {
    /// Add a binding pattern node
    pub fn add_binding_pattern(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: BindingPatternData,
    ) -> NodeIndex {
        let elements = data.elements.clone();

        let data_index = self.len_u32(self.binding_patterns.len());
        self.binding_patterns.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add a binding element node
    pub fn add_binding_element(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: BindingElementData,
    ) -> NodeIndex {
        let property_name = data.property_name;
        let name = data.name;
        let initializer = data.initializer;

        let data_index = self.len_u32(self.binding_elements.len());
        self.binding_elements.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(property_name, parent);
        self.set_parent(name, parent);
        self.set_parent(initializer, parent);
        parent
    }

    /// Add a property assignment node
    pub fn add_property_assignment(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: PropertyAssignmentData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let initializer = data.initializer;

        let data_index = self.len_u32(self.property_assignments.len());
        self.property_assignments.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent(initializer, parent);
        parent
    }

    /// Add a shorthand property assignment node
    pub fn add_shorthand_property(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ShorthandPropertyData,
    ) -> NodeIndex {
        let modifiers = data.modifiers.clone();
        let name = data.name;
        let object_assignment_initializer = data.object_assignment_initializer;

        let data_index = self.len_u32(self.shorthand_properties.len());
        self.shorthand_properties.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_opt_list(modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent(object_assignment_initializer, parent);
        parent
    }
}
