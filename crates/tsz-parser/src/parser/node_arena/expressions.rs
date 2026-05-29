//! `NodeArena` constructors for expression nodes (and the name-like nodes
//! that appear inside expressions: qualified names, computed property names,
//! and expression-with-type-arguments).

use crate::parser::base::NodeIndex;
use crate::parser::node::{
    AccessExprData, BinaryExprData, CallExprData, ComputedPropertyData, ConditionalExprData,
    ExprWithTypeArgsData, ExtendedNodeInfo, LiteralExprData, Node, NodeArena, ParenthesizedData,
    QualifiedNameData, SpreadData, TaggedTemplateData, TemplateExprData, TemplateSpanData,
    TypeAssertionData, UnaryExprData, UnaryExprDataEx,
};

impl NodeArena {
    /// Add a qualified name node
    pub fn add_qualified_name(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: QualifiedNameData,
    ) -> NodeIndex {
        let left = data.left;
        let right = data.right;

        let data_index = self.len_u32(self.qualified_names.len());
        self.qualified_names.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(left, parent);
        self.set_parent(right, parent);

        parent
    }

    /// Add a computed property name node
    pub fn add_computed_property(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ComputedPropertyData,
    ) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.len_u32(self.computed_properties.len());
        self.computed_properties.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a binary expression
    pub fn add_binary_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: BinaryExprData,
    ) -> NodeIndex {
        let left = data.left;
        let right = data.right;

        let data_index = self.len_u32(self.binary_exprs.len());
        self.binary_exprs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(left, parent);
        self.set_parent(right, parent);

        parent
    }

    /// Add a call expression
    pub fn add_call_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CallExprData,
    ) -> NodeIndex {
        let expression = data.expression;
        let type_arguments = data.type_arguments.clone();
        let arguments = data.arguments.clone();

        let data_index = self.len_u32(self.call_exprs.len());
        self.call_exprs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent_opt_list(type_arguments.as_ref(), parent);
        self.set_parent_opt_list(arguments.as_ref(), parent);

        parent
    }

    /// Add a unary expression node
    pub fn add_unary_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: UnaryExprData,
    ) -> NodeIndex {
        let operand = data.operand;

        let data_index = self.len_u32(self.unary_exprs.len());
        self.unary_exprs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(operand, parent);

        parent
    }

    /// Add a property/element access expression node
    pub fn add_access_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: AccessExprData,
    ) -> NodeIndex {
        let expression = data.expression;
        let name_or_argument = data.name_or_argument;

        let data_index = self.len_u32(self.access_exprs.len());
        self.access_exprs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(name_or_argument, parent);

        parent
    }

    /// Add a conditional expression node (a ? b : c)
    pub fn add_conditional_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ConditionalExprData,
    ) -> NodeIndex {
        let condition = data.condition;
        let when_true = data.when_true;
        let when_false = data.when_false;

        let data_index = self.len_u32(self.conditional_exprs.len());
        self.conditional_exprs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(condition, parent);
        self.set_parent(when_true, parent);
        self.set_parent(when_false, parent);
        parent
    }

    /// Add an object/array literal expression node
    pub fn add_literal_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: LiteralExprData,
    ) -> NodeIndex {
        let elements = data.elements.clone();

        let data_index = self.len_u32(self.literal_exprs.len());
        self.literal_exprs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent_list(&elements, parent);
        parent
    }

    /// Add a parenthesized expression node
    pub fn add_parenthesized(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ParenthesizedData,
    ) -> NodeIndex {
        let expression = data.expression;
        let data_index = self.len_u32(self.parenthesized.len());
        self.parenthesized.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a spread/await/yield expression node
    pub fn add_unary_expr_ex(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: UnaryExprDataEx,
    ) -> NodeIndex {
        let expression = data.expression;
        let data_index = self.len_u32(self.unary_exprs_ex.len());
        self.unary_exprs_ex.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }

    /// Add a type assertion expression node
    pub fn add_type_assertion(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeAssertionData,
    ) -> NodeIndex {
        let expression = data.expression;
        let type_node = data.type_node;
        let data_index = self.len_u32(self.type_assertions.len());
        self.type_assertions.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(type_node, parent);
        parent
    }

    /// Add a template expression node
    pub fn add_template_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TemplateExprData,
    ) -> NodeIndex {
        let head = data.head;
        let template_spans = data.template_spans.clone();

        let data_index = self.len_u32(self.template_exprs.len());
        self.template_exprs.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(head, parent);
        self.set_parent_list(&template_spans, parent);

        parent
    }

    /// Add a template span node
    pub fn add_template_span(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TemplateSpanData,
    ) -> NodeIndex {
        let expression = data.expression;
        let literal = data.literal;

        let data_index = self.len_u32(self.template_spans.len());
        self.template_spans.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent(literal, parent);

        parent
    }

    /// Add a tagged template expression node
    pub fn add_tagged_template(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TaggedTemplateData,
    ) -> NodeIndex {
        let tag = data.tag;
        let type_arguments = data.type_arguments.clone();
        let template = data.template;

        let data_index = self.len_u32(self.tagged_templates.len());
        self.tagged_templates.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());

        let parent = NodeIndex(index);
        self.set_parent(tag, parent);
        self.set_parent_opt_list(type_arguments.as_ref(), parent);
        self.set_parent(template, parent);

        parent
    }

    /// Add an expression with type arguments node
    pub fn add_expr_with_type_args(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExprWithTypeArgsData,
    ) -> NodeIndex {
        let expression = data.expression;
        let type_arguments = data.type_arguments.clone();
        let data_index = self.len_u32(self.expr_with_type_args.len());
        self.expr_with_type_args.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        self.set_parent_opt_list(type_arguments.as_ref(), parent);
        parent
    }

    /// Add a spread assignment node
    pub fn add_spread(&mut self, kind: u16, pos: u32, end: u32, data: SpreadData) -> NodeIndex {
        let expression = data.expression;

        let data_index = self.len_u32(self.spread_data.len());
        self.spread_data.push(data);
        let index = self.len_u32(self.nodes.len());
        self.nodes.push(Node::with_data(kind, pos, end, data_index));
        self.extended_info.push(ExtendedNodeInfo::default());
        let parent = NodeIndex(index);
        self.set_parent(expression, parent);
        parent
    }
}
