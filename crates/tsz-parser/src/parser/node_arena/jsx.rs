//! `NodeArena` constructors for JSX nodes (elements, fragments, opening/
//! closing tags, attributes, spread attributes, expressions, text, and
//! namespaced names).

use super::push_data_node;
use crate::parser::base::NodeIndex;
use crate::parser::node::{
    JsxAttributeData, JsxAttributesData, JsxClosingData, JsxElementData, JsxExpressionData,
    JsxFragmentData, JsxNamespacedNameData, JsxOpeningData, JsxSpreadAttributeData, JsxTextData,
    NodeArenaInner,
};

impl NodeArenaInner {
    /// Add a JSX element node
    pub fn add_jsx_element(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxElementData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.opening_element, parent);
        self.set_parent_list(&data.children, parent);
        self.set_parent(data.closing_element, parent);
        push_data_node!(self, parent, kind, pos, end, jsx_elements, data)
    }

    /// Add a JSX opening/self-closing element node
    pub fn add_jsx_opening(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxOpeningData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.tag_name, parent);
        self.set_parent_opt_list(data.type_arguments.as_ref(), parent);
        self.set_parent(data.attributes, parent);
        push_data_node!(self, parent, kind, pos, end, jsx_opening, data)
    }

    /// Add a JSX closing element node
    pub fn add_jsx_closing(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxClosingData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.tag_name, parent);
        push_data_node!(self, parent, kind, pos, end, jsx_closing, data)
    }

    /// Add a JSX fragment node
    pub fn add_jsx_fragment(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxFragmentData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.opening_fragment, parent);
        self.set_parent_list(&data.children, parent);
        self.set_parent(data.closing_fragment, parent);
        push_data_node!(self, parent, kind, pos, end, jsx_fragments, data)
    }

    /// Add a JSX attributes node
    pub fn add_jsx_attributes(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxAttributesData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_list(&data.properties, parent);
        push_data_node!(self, parent, kind, pos, end, jsx_attributes, data)
    }

    /// Add a JSX attribute node
    pub fn add_jsx_attribute(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxAttributeData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.name, parent);
        self.set_parent(data.initializer, parent);
        push_data_node!(self, parent, kind, pos, end, jsx_attribute, data)
    }

    /// Add a JSX spread attribute node
    pub fn add_jsx_spread_attribute(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxSpreadAttributeData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        push_data_node!(self, parent, kind, pos, end, jsx_spread_attributes, data)
    }

    /// Add a JSX expression node
    pub fn add_jsx_expression(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxExpressionData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        push_data_node!(self, parent, kind, pos, end, jsx_expressions, data)
    }

    /// Add a JSX text node
    pub fn add_jsx_text(&mut self, kind: u16, pos: u32, end: u32, data: JsxTextData) -> NodeIndex {
        // Leaf node: no children to parent, but still data-bearing.
        let parent = self.reserve_parent();
        push_data_node!(self, parent, kind, pos, end, jsx_text, data)
    }

    /// Add a JSX namespaced name node
    pub fn add_jsx_namespaced_name(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxNamespacedNameData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.namespace, parent);
        self.set_parent(data.name, parent);
        push_data_node!(self, parent, kind, pos, end, jsx_namespaced_names, data)
    }
}
