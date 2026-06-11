//! `NodeArena` constructors for JSX nodes (elements, fragments, opening/
//! closing tags, attributes, spread attributes, expressions, text, and
//! namespaced names).

use crate::parser::base::NodeIndex;
use crate::parser::node::{
    ExtendedNodeInfo, JsxAttributeData, JsxAttributesData, JsxClosingData, JsxElementData,
    JsxExpressionData, JsxFragmentData, JsxNamespacedNameData, JsxOpeningData,
    JsxSpreadAttributeData, JsxTextData, Node, NodeArenaInner,
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
        let opening_element = data.opening_element;
        let closing_element = data.closing_element;
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent(opening_element, parent);
        self.set_parent_list(&data.children, parent);
        self.set_parent(closing_element, parent);

        let data_index = self.len_u32(self.jsx_elements.len());
        self.jsx_elements.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        parent
    }

    /// Add a JSX opening/self-closing element node
    pub fn add_jsx_opening(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxOpeningData,
    ) -> NodeIndex {
        let tag_name = data.tag_name;
        let attributes = data.attributes;
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent(tag_name, parent);
        self.set_parent_opt_list(data.type_arguments.as_ref(), parent);
        self.set_parent(attributes, parent);

        let data_index = self.len_u32(self.jsx_opening.len());
        self.jsx_opening.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        parent
    }

    /// Add a JSX closing element node
    pub fn add_jsx_closing(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxClosingData,
    ) -> NodeIndex {
        let tag_name = data.tag_name;

        let data_index = self.len_u32(self.jsx_closing.len());
        self.jsx_closing.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(tag_name, parent);
        parent
    }

    /// Add a JSX fragment node
    pub fn add_jsx_fragment(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxFragmentData,
    ) -> NodeIndex {
        let opening_fragment = data.opening_fragment;
        let closing_fragment = data.closing_fragment;
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent(opening_fragment, parent);
        self.set_parent_list(&data.children, parent);
        self.set_parent(closing_fragment, parent);

        let data_index = self.len_u32(self.jsx_fragments.len());
        self.jsx_fragments.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        parent
    }

    /// Add a JSX attributes node
    pub fn add_jsx_attributes(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxAttributesData,
    ) -> NodeIndex {
        let parent = NodeIndex(self.len_u32(self.nodes.len()));

        self.set_parent_list(&data.properties, parent);

        let data_index = self.len_u32(self.jsx_attributes.len());
        self.jsx_attributes.push(data);
        let index = self.len_u32(self.nodes.len());
        debug_assert_eq!(parent.0, index);
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        parent
    }

    /// Add a JSX attribute node
    pub fn add_jsx_attribute(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxAttributeData,
    ) -> NodeIndex {
        let name = data.name;
        let initializer = data.initializer;

        let data_index = self.len_u32(self.jsx_attribute.len());
        self.jsx_attribute.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(name, parent);
        self.set_parent(initializer, parent);
        parent
    }

    /// Add a JSX spread attribute node
    pub fn add_jsx_spread_attribute(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxSpreadAttributeData,
    ) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.len_u32(self.jsx_spread_attributes.len());
        self.jsx_spread_attributes.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a JSX expression node
    pub fn add_jsx_expression(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxExpressionData,
    ) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.len_u32(self.jsx_expressions.len());
        self.jsx_expressions.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a JSX text node
    pub fn add_jsx_text(&mut self, kind: u16, pos: u32, end: u32, data: JsxTextData) -> NodeIndex {
        let data_index = self.len_u32(self.jsx_text.len());
        self.jsx_text.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        NodeIndex(index)
    }

    /// Add a JSX namespaced name node
    pub fn add_jsx_namespaced_name(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: JsxNamespacedNameData,
    ) -> NodeIndex {
        let namespace = data.namespace;
        let name = data.name;

        let data_index = self.len_u32(self.jsx_namespaced_names.len());
        self.jsx_namespaced_names.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(namespace, parent);
        self.set_parent(name, parent);
        parent
    }
}
