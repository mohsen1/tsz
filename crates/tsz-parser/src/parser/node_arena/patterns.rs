//! `NodeArena` constructors for destructuring binding patterns and
//! object-literal property assignments / shorthand properties.

use crate::parser::base::NodeIndex;
use crate::parser::node::{
    BindingElementData, BindingPatternData, ExtendedNodeInfo, Node, NodeArenaInner,
    PropertyAssignmentData, ShorthandPropertyData,
};

impl NodeArenaInner {
    /// Add a binding pattern node
    pub fn add_binding_pattern(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: BindingPatternData,
    ) -> NodeIndex {
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent_list(&data.elements, parent);

        let data_index = self.len_u32(self.binding_patterns.len());
        self.binding_patterns.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
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
        let name = data.name;
        let initializer = data.initializer;
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent(initializer, parent);

        let data_index = self.len_u32(self.property_assignments.len());
        self.property_assignments.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
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
        let name = data.name;
        let object_assignment_initializer = data.object_assignment_initializer;
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(name, parent);
        self.set_parent(object_assignment_initializer, parent);

        let data_index = self.len_u32(self.shorthand_properties.len());
        self.shorthand_properties.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        parent
    }
}
