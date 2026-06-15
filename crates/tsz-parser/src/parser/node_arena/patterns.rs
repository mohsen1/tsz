//! `NodeArena` constructors for destructuring binding patterns and
//! object-literal property assignments / shorthand properties.

use super::push_data_node;
use crate::parser::base::NodeIndex;
use crate::parser::node::{
    BindingElementData, BindingPatternData, NodeArenaInner, PropertyAssignmentData,
    ShorthandPropertyData,
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
        let parent = self.reserve_parent();
        self.set_parent_list(&data.elements, parent);
        push_data_node!(self, parent, kind, pos, end, binding_patterns, data)
    }

    /// Add a binding element node
    pub fn add_binding_element(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: BindingElementData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.property_name, parent);
        self.set_parent(data.name, parent);
        self.set_parent(data.initializer, parent);
        push_data_node!(self, parent, kind, pos, end, binding_elements, data)
    }

    /// Add a property assignment node
    pub fn add_property_assignment(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: PropertyAssignmentData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.name, parent);
        self.set_parent(data.initializer, parent);
        push_data_node!(self, parent, kind, pos, end, property_assignments, data)
    }

    /// Add a shorthand property assignment node
    pub fn add_shorthand_property(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ShorthandPropertyData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_opt_list(data.modifiers.as_ref(), parent);
        self.set_parent(data.name, parent);
        self.set_parent(data.object_assignment_initializer, parent);
        push_data_node!(self, parent, kind, pos, end, shorthand_properties, data)
    }
}
