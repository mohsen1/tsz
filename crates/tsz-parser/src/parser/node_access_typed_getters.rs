//! Typed payload getters for type/signature/template/binding nodes.
//!
//! Extracted from `node_access.rs` to keep that file under the LOC ceiling.
//! Pure file-organization move; no logic changes.

use super::node::{
    ArrayTypeData, BindingElementData, BindingPatternData, CompositeTypeData, ComputedPropertyData,
    ConditionalTypeData, ExprWithTypeArgsData, FunctionTypeData, HeritageData, IndexSignatureData,
    IndexedAccessTypeData, InferTypeData, LiteralTypeData, MappedTypeData, NamedTupleMemberData,
    Node, NodeArena, ParenthesizedData, ShorthandPropertyData, SignatureData, SpreadData,
    TaggedTemplateData, TemplateExprData, TemplateLiteralTypeData, TemplateSpanData, TupleTypeData,
    TypeLiteralData, TypeOperatorData, TypeParameterData, TypePredicateData, TypeQueryData,
    WrappedTypeData,
};

impl NodeArena {
    /// Get signature data (call, construct, method, property signatures).
    #[inline]
    #[must_use]
    pub fn get_signature(&self, node: &Node) -> Option<&SignatureData> {
        use super::syntax_kind_ext::{
            CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        if node.has_data()
            && (node.kind == CALL_SIGNATURE
                || node.kind == CONSTRUCT_SIGNATURE
                || node.kind == METHOD_SIGNATURE
                || node.kind == PROPERTY_SIGNATURE)
        {
            self.signatures.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get index signature data.
    #[inline]
    #[must_use]
    pub fn get_index_signature(&self, node: &Node) -> Option<&IndexSignatureData> {
        use super::syntax_kind_ext::INDEX_SIGNATURE;
        if node.has_data() && node.kind == INDEX_SIGNATURE {
            self.index_signatures.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get heritage clause data.
    #[inline]
    #[must_use]
    pub fn get_heritage_clause(&self, node: &Node) -> Option<&HeritageData> {
        use super::syntax_kind_ext::HERITAGE_CLAUSE;
        if node.has_data() && node.kind == HERITAGE_CLAUSE {
            self.heritage_clauses.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get composite type data (union or intersection).
    #[inline]
    #[must_use]
    pub fn get_composite_type(&self, node: &Node) -> Option<&CompositeTypeData> {
        use super::syntax_kind_ext::{INTERSECTION_TYPE, UNION_TYPE};
        if node.has_data() && (node.kind == UNION_TYPE || node.kind == INTERSECTION_TYPE) {
            self.composite_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get array type data.
    #[inline]
    #[must_use]
    pub fn get_array_type(&self, node: &Node) -> Option<&ArrayTypeData> {
        use super::syntax_kind_ext::ARRAY_TYPE;
        if node.has_data() && node.kind == ARRAY_TYPE {
            self.array_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get tuple type data.
    #[inline]
    #[must_use]
    pub fn get_tuple_type(&self, node: &Node) -> Option<&TupleTypeData> {
        use super::syntax_kind_ext::TUPLE_TYPE;
        if node.has_data() && node.kind == TUPLE_TYPE {
            self.tuple_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get function type data.
    #[inline]
    #[must_use]
    pub fn get_function_type(&self, node: &Node) -> Option<&FunctionTypeData> {
        use super::syntax_kind_ext::{CONSTRUCTOR_TYPE, FUNCTION_TYPE};
        if node.has_data() && (node.kind == FUNCTION_TYPE || node.kind == CONSTRUCTOR_TYPE) {
            self.function_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type literal data.
    #[inline]
    #[must_use]
    pub fn get_type_literal(&self, node: &Node) -> Option<&TypeLiteralData> {
        use super::syntax_kind_ext::TYPE_LITERAL;
        if node.has_data() && node.kind == TYPE_LITERAL {
            self.type_literals.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get conditional type data.
    #[inline]
    #[must_use]
    pub fn get_conditional_type(&self, node: &Node) -> Option<&ConditionalTypeData> {
        use super::syntax_kind_ext::CONDITIONAL_TYPE;
        if node.has_data() && node.kind == CONDITIONAL_TYPE {
            self.conditional_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get mapped type data.
    #[inline]
    #[must_use]
    pub fn get_mapped_type(&self, node: &Node) -> Option<&MappedTypeData> {
        use super::syntax_kind_ext::MAPPED_TYPE;
        if node.has_data() && node.kind == MAPPED_TYPE {
            self.mapped_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get indexed access type data.
    #[inline]
    #[must_use]
    pub fn get_indexed_access_type(&self, node: &Node) -> Option<&IndexedAccessTypeData> {
        use super::syntax_kind_ext::INDEXED_ACCESS_TYPE;
        if node.has_data() && node.kind == INDEXED_ACCESS_TYPE {
            self.indexed_access_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get literal type data.
    #[inline]
    #[must_use]
    pub fn get_literal_type(&self, node: &Node) -> Option<&LiteralTypeData> {
        use super::syntax_kind_ext::LITERAL_TYPE;
        if node.has_data() && node.kind == LITERAL_TYPE {
            self.literal_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get wrapped type data (parenthesized, optional, rest types).
    #[inline]
    #[must_use]
    pub fn get_wrapped_type(&self, node: &Node) -> Option<&WrappedTypeData> {
        use super::syntax_kind_ext::{OPTIONAL_TYPE, PARENTHESIZED_TYPE, REST_TYPE};
        if node.has_data()
            && (node.kind == PARENTHESIZED_TYPE
                || node.kind == OPTIONAL_TYPE
                || node.kind == REST_TYPE)
        {
            self.wrapped_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get heritage clause data.
    #[inline]
    #[must_use]
    pub fn get_heritage(&self, node: &Node) -> Option<&HeritageData> {
        use super::syntax_kind_ext::HERITAGE_CLAUSE;
        if node.has_data() && node.kind == HERITAGE_CLAUSE {
            self.heritage_clauses.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get expression with type arguments data (e.g., `extends Base<T>`).
    #[inline]
    #[must_use]
    pub fn get_expr_type_args(&self, node: &Node) -> Option<&ExprWithTypeArgsData> {
        use super::syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS;
        if node.has_data() && node.kind == EXPRESSION_WITH_TYPE_ARGUMENTS {
            self.expr_with_type_args.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type query data (typeof in type position).
    #[inline]
    #[must_use]
    pub fn get_type_query(&self, node: &Node) -> Option<&TypeQueryData> {
        use super::syntax_kind_ext::TYPE_QUERY;
        if node.has_data() && node.kind == TYPE_QUERY {
            self.type_queries.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type operator data (keyof, unique, readonly).
    #[inline]
    #[must_use]
    pub fn get_type_operator(&self, node: &Node) -> Option<&TypeOperatorData> {
        use super::syntax_kind_ext::TYPE_OPERATOR;
        if node.has_data() && node.kind == TYPE_OPERATOR {
            self.type_operators.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get infer type data.
    #[inline]
    #[must_use]
    pub fn get_infer_type(&self, node: &Node) -> Option<&InferTypeData> {
        use super::syntax_kind_ext::INFER_TYPE;
        if node.has_data() && node.kind == INFER_TYPE {
            self.infer_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get template literal type data.
    #[inline]
    #[must_use]
    pub fn get_template_literal_type(&self, node: &Node) -> Option<&TemplateLiteralTypeData> {
        use super::syntax_kind_ext::TEMPLATE_LITERAL_TYPE;
        if node.has_data() && node.kind == TEMPLATE_LITERAL_TYPE {
            self.template_literal_types.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get named tuple member data.
    #[inline]
    #[must_use]
    pub fn get_named_tuple_member(&self, node: &Node) -> Option<&NamedTupleMemberData> {
        use super::syntax_kind_ext::NAMED_TUPLE_MEMBER;
        if node.has_data() && node.kind == NAMED_TUPLE_MEMBER {
            self.named_tuple_members.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type predicate data.
    #[inline]
    #[must_use]
    pub fn get_type_predicate(&self, node: &Node) -> Option<&TypePredicateData> {
        use super::syntax_kind_ext::TYPE_PREDICATE;
        if node.has_data() && node.kind == TYPE_PREDICATE {
            self.type_predicates.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get type parameter data.
    #[inline]
    #[must_use]
    pub fn get_type_parameter(&self, node: &Node) -> Option<&TypeParameterData> {
        use super::syntax_kind_ext::TYPE_PARAMETER;
        if node.has_data() && node.kind == TYPE_PARAMETER {
            self.type_parameters.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get parenthesized expression data.
    /// Returns None if node is not a parenthesized expression or has no data.
    #[inline]
    #[must_use]
    pub fn get_parenthesized(&self, node: &Node) -> Option<&ParenthesizedData> {
        use super::syntax_kind_ext::PARENTHESIZED_EXPRESSION;
        if node.has_data() && node.kind == PARENTHESIZED_EXPRESSION {
            self.parenthesized.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get template expression data.
    #[inline]
    #[must_use]
    pub fn get_template_expr(&self, node: &Node) -> Option<&TemplateExprData> {
        use super::syntax_kind_ext::TEMPLATE_EXPRESSION;
        if node.has_data() && node.kind == TEMPLATE_EXPRESSION {
            self.template_exprs.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get template span data. Accepts both `TEMPLATE_SPAN` (expression-level)
    /// and `TEMPLATE_LITERAL_TYPE_SPAN` (type-level) since both store data in
    /// the same `template_spans` array.
    #[inline]
    #[must_use]
    pub fn get_template_span(&self, node: &Node) -> Option<&TemplateSpanData> {
        use super::syntax_kind_ext::{TEMPLATE_LITERAL_TYPE_SPAN, TEMPLATE_SPAN};
        if node.has_data()
            && (node.kind == TEMPLATE_SPAN || node.kind == TEMPLATE_LITERAL_TYPE_SPAN)
        {
            self.template_spans.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get tagged template expression data.
    #[inline]
    #[must_use]
    pub fn get_tagged_template(&self, node: &Node) -> Option<&TaggedTemplateData> {
        use super::syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION;
        if node.has_data() && node.kind == TAGGED_TEMPLATE_EXPRESSION {
            self.tagged_templates.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get spread element/assignment data.
    #[inline]
    #[must_use]
    pub fn get_spread(&self, node: &Node) -> Option<&SpreadData> {
        use super::syntax_kind_ext::{SPREAD_ASSIGNMENT, SPREAD_ELEMENT};
        if node.has_data() && (node.kind == SPREAD_ELEMENT || node.kind == SPREAD_ASSIGNMENT) {
            self.spread_data.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get shorthand property assignment data.
    #[inline]
    #[must_use]
    pub fn get_shorthand_property(&self, node: &Node) -> Option<&ShorthandPropertyData> {
        use super::syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT;
        if node.has_data() && node.kind == SHORTHAND_PROPERTY_ASSIGNMENT {
            self.shorthand_properties.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get binding pattern data (`ObjectBindingPattern` or `ArrayBindingPattern`).
    #[inline]
    #[must_use]
    pub fn get_binding_pattern(&self, node: &Node) -> Option<&BindingPatternData> {
        if node.has_data() && node.is_binding_pattern() {
            self.binding_patterns.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get binding element data.
    #[inline]
    #[must_use]
    pub fn get_binding_element(&self, node: &Node) -> Option<&BindingElementData> {
        use super::syntax_kind_ext::BINDING_ELEMENT;
        if node.has_data() && node.kind == BINDING_ELEMENT {
            self.binding_elements.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get computed property name data
    #[inline]
    #[must_use]
    pub fn get_computed_property(&self, node: &Node) -> Option<&ComputedPropertyData> {
        use super::syntax_kind_ext::COMPUTED_PROPERTY_NAME;
        if node.has_data() && node.kind == COMPUTED_PROPERTY_NAME {
            self.computed_properties.get(node.data_index as usize)
        } else {
            None
        }
    }
}
