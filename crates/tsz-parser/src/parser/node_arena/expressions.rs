//! `NodeArena` constructors for expression nodes (and the name-like nodes
//! that appear inside expressions: qualified names, computed property names,
//! and expression-with-type-arguments).

use super::push_data_node;
use crate::parser::base::NodeIndex;
use crate::parser::node::{
    AccessExprData, BinaryExprData, CallExprData, ComputedPropertyData, ConditionalExprData,
    ExprWithTypeArgsData, LiteralExprData, NodeArenaInner, ParenthesizedData, QualifiedNameData,
    SpreadData, TaggedTemplateData, TemplateExprData, TemplateSpanData, TypeAssertionData,
    UnaryExprData, UnaryExprDataEx,
};

impl NodeArenaInner {
    /// Add a qualified name node
    pub fn add_qualified_name(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: QualifiedNameData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.left, parent);
        self.set_parent(data.right, parent);
        push_data_node!(self, parent, kind, pos, end, qualified_names, data)
    }

    /// Add a computed property name node
    pub fn add_computed_property(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ComputedPropertyData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        push_data_node!(self, parent, kind, pos, end, computed_properties, data)
    }

    /// Add a binary expression
    pub fn add_binary_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: BinaryExprData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.left, parent);
        self.set_parent(data.right, parent);
        push_data_node!(self, parent, kind, pos, end, binary_exprs, data)
    }

    /// Add a call expression
    pub fn add_call_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: CallExprData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        self.set_parent_opt_list(data.type_arguments.as_ref(), parent);
        self.set_parent_opt_list(data.arguments.as_ref(), parent);
        push_data_node!(self, parent, kind, pos, end, call_exprs, data)
    }

    /// Add a unary expression node
    pub fn add_unary_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: UnaryExprData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.operand, parent);
        push_data_node!(self, parent, kind, pos, end, unary_exprs, data)
    }

    /// Add a property/element access expression node
    pub fn add_access_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: AccessExprData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        self.set_parent(data.name_or_argument, parent);
        push_data_node!(self, parent, kind, pos, end, access_exprs, data)
    }

    /// Add a conditional expression node (a ? b : c)
    pub fn add_conditional_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ConditionalExprData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.condition, parent);
        self.set_parent(data.when_true, parent);
        self.set_parent(data.when_false, parent);
        push_data_node!(self, parent, kind, pos, end, conditional_exprs, data)
    }

    /// Add an object/array literal expression node
    pub fn add_literal_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: LiteralExprData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent_list(&data.elements, parent);
        push_data_node!(self, parent, kind, pos, end, literal_exprs, data)
    }

    /// Add a parenthesized expression node
    pub fn add_parenthesized(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ParenthesizedData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        push_data_node!(self, parent, kind, pos, end, parenthesized, data)
    }

    /// Add a spread/await/yield expression node
    pub fn add_unary_expr_ex(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: UnaryExprDataEx,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        push_data_node!(self, parent, kind, pos, end, unary_exprs_ex, data)
    }

    /// Add a type assertion expression node
    pub fn add_type_assertion(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TypeAssertionData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        self.set_parent(data.type_node, parent);
        push_data_node!(self, parent, kind, pos, end, type_assertions, data)
    }

    /// Add a template expression node
    pub fn add_template_expr(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TemplateExprData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.head, parent);
        self.set_parent_list(&data.template_spans, parent);
        push_data_node!(self, parent, kind, pos, end, template_exprs, data)
    }

    /// Add a template span node
    pub fn add_template_span(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TemplateSpanData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        self.set_parent(data.literal, parent);
        push_data_node!(self, parent, kind, pos, end, template_spans, data)
    }

    /// Add a tagged template expression node
    pub fn add_tagged_template(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: TaggedTemplateData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.tag, parent);
        self.set_parent_opt_list(data.type_arguments.as_ref(), parent);
        self.set_parent(data.template, parent);
        push_data_node!(self, parent, kind, pos, end, tagged_templates, data)
    }

    /// Add an expression with type arguments node
    pub fn add_expr_with_type_args(
        &mut self,
        kind: u16,
        pos: u32,
        end: u32,
        data: ExprWithTypeArgsData,
    ) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        self.set_parent_opt_list(data.type_arguments.as_ref(), parent);
        push_data_node!(self, parent, kind, pos, end, expr_with_type_args, data)
    }

    /// Add a spread assignment node
    pub fn add_spread(&mut self, kind: u16, pos: u32, end: u32, data: SpreadData) -> NodeIndex {
        let parent = self.reserve_parent();
        self.set_parent(data.expression, parent);
        push_data_node!(self, parent, kind, pos, end, spread_data, data)
    }
}
