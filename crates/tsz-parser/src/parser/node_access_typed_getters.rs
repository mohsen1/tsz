//! Typed payload getters for type/signature/template/binding nodes.
//!
//! Extracted from `node_access.rs` to keep that file under the LOC ceiling.
//! Pure file-organization move; no logic changes.

use super::node::{
    ArrayTypeData, BindingElementData, BindingPatternData, CompositeTypeData, ComputedPropertyData,
    ConditionalTypeData, ExprWithTypeArgsData, FunctionTypeData, HeritageData, IndexSignatureData,
    IndexedAccessTypeData, InferTypeData, LiteralTypeData, MappedTypeData, NamedTupleMemberData,
    Node, NodeArenaInner, ParenthesizedData, ShorthandPropertyData, SignatureData, SpreadData,
    TaggedTemplateData, TemplateExprData, TemplateLiteralTypeData, TemplateSpanData, TupleTypeData,
    TypeLiteralData, TypeOperatorData, TypeParameterData, TypePredicateData, TypeQueryData,
    WrappedTypeData,
};
use super::node_access::define_kind_getters;

impl NodeArenaInner {
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

    /// Classify a property name node as `(is_string_named, single_quoted)`.
    ///
    /// `is_string_named` is true when the name was authored as a `StringLiteral`
    /// (directly or as the expression of a `COMPUTED_PROPERTY_NAME`).
    /// `single_quoted` is true when that string literal used `'…'` syntax.
    /// `single_quoted` implies `is_string_named`.
    ///
    /// Drives the `keyof` key-type policy (`{ "1": ... }` yields the string
    /// literal `"1"`; `{ 1: ... }` yields the number literal `1`) and the
    /// DTS emit quote-style preservation.
    #[must_use]
    pub fn string_property_name_flags(&self, name_idx: crate::parser::NodeIndex) -> (bool, bool) {
        use super::syntax_kind_ext::COMPUTED_PROPERTY_NAME;
        use tsz_scanner::SyntaxKind;
        let Some(name_node) = self.get(name_idx) else {
            return (false, false);
        };
        let literal_node = if name_node.kind == SyntaxKind::StringLiteral as u16 {
            Some(name_node)
        } else if name_node.kind == COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.get_computed_property(name_node)
            && let Some(expr_node) = self.get(computed.expression)
            && expr_node.kind == SyntaxKind::StringLiteral as u16
        {
            Some(expr_node)
        } else {
            None
        };
        let Some(literal_node) = literal_node else {
            return (false, false);
        };
        let single_quoted = self
            .get_literal(literal_node)
            .and_then(|lit| lit.raw_text.as_deref())
            .is_some_and(|raw| raw.starts_with('\''));
        (true, single_quoted)
    }
}

// Type/signature/binding typed getters, table-driven.
define_kind_getters! {
    /// Get signature data (call, construct, method, property signatures).
    get_signature => signatures -> SignatureData, [CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE];

    /// Get index signature data.
    get_index_signature => index_signatures -> IndexSignatureData, [INDEX_SIGNATURE];

    /// Get heritage clause data.
    get_heritage_clause => heritage_clauses -> HeritageData, [HERITAGE_CLAUSE];

    /// Get composite type data (union or intersection).
    get_composite_type => composite_types -> CompositeTypeData, [UNION_TYPE, INTERSECTION_TYPE];

    /// Get array type data.
    get_array_type => array_types -> ArrayTypeData, [ARRAY_TYPE];

    /// Get tuple type data.
    get_tuple_type => tuple_types -> TupleTypeData, [TUPLE_TYPE];

    /// Get function type data.
    get_function_type => function_types -> FunctionTypeData, [FUNCTION_TYPE, CONSTRUCTOR_TYPE];

    /// Get type literal data.
    get_type_literal => type_literals -> TypeLiteralData, [TYPE_LITERAL];

    /// Get conditional type data.
    get_conditional_type => conditional_types -> ConditionalTypeData, [CONDITIONAL_TYPE];

    /// Get mapped type data.
    get_mapped_type => mapped_types -> MappedTypeData, [MAPPED_TYPE];

    /// Get indexed access type data.
    get_indexed_access_type => indexed_access_types -> IndexedAccessTypeData, [INDEXED_ACCESS_TYPE];

    /// Get literal type data.
    get_literal_type => literal_types -> LiteralTypeData, [LITERAL_TYPE];

    /// Get wrapped type data (parenthesized, optional, rest types).
    get_wrapped_type => wrapped_types -> WrappedTypeData, [PARENTHESIZED_TYPE, OPTIONAL_TYPE, REST_TYPE];

    /// Get heritage clause data.
    get_heritage => heritage_clauses -> HeritageData, [HERITAGE_CLAUSE];

    /// Get expression with type arguments data (e.g., `extends Base<T>`).
    get_expr_type_args => expr_with_type_args -> ExprWithTypeArgsData, [EXPRESSION_WITH_TYPE_ARGUMENTS];

    /// Get type query data (typeof in type position).
    get_type_query => type_queries -> TypeQueryData, [TYPE_QUERY];

    /// Get type operator data (keyof, unique, readonly).
    get_type_operator => type_operators -> TypeOperatorData, [TYPE_OPERATOR];

    /// Get infer type data.
    get_infer_type => infer_types -> InferTypeData, [INFER_TYPE];

    /// Get template literal type data.
    get_template_literal_type => template_literal_types -> TemplateLiteralTypeData, [TEMPLATE_LITERAL_TYPE];

    /// Get named tuple member data.
    get_named_tuple_member => named_tuple_members -> NamedTupleMemberData, [NAMED_TUPLE_MEMBER];

    /// Get type predicate data.
    get_type_predicate => type_predicates -> TypePredicateData, [TYPE_PREDICATE];

    /// Get type parameter data.
    get_type_parameter => type_parameters -> TypeParameterData, [TYPE_PARAMETER];

    /// Get parenthesized expression data.
    /// Returns None if node is not a parenthesized expression or has no data.
    get_parenthesized => parenthesized -> ParenthesizedData, [PARENTHESIZED_EXPRESSION];

    /// Get template expression data.
    get_template_expr => template_exprs -> TemplateExprData, [TEMPLATE_EXPRESSION];

    /// Get template span data. Accepts both `TEMPLATE_SPAN` (expression-level)
    /// and `TEMPLATE_LITERAL_TYPE_SPAN` (type-level) since both store data in
    /// the same `template_spans` array.
    get_template_span => template_spans -> TemplateSpanData, [TEMPLATE_SPAN, TEMPLATE_LITERAL_TYPE_SPAN];

    /// Get tagged template expression data.
    get_tagged_template => tagged_templates -> TaggedTemplateData, [TAGGED_TEMPLATE_EXPRESSION];

    /// Get spread element/assignment data.
    get_spread => spread_data -> SpreadData, [SPREAD_ELEMENT, SPREAD_ASSIGNMENT];

    /// Get shorthand property assignment data.
    get_shorthand_property => shorthand_properties -> ShorthandPropertyData, [SHORTHAND_PROPERTY_ASSIGNMENT];

    /// Get binding element data.
    get_binding_element => binding_elements -> BindingElementData, [BINDING_ELEMENT];

    /// Get computed property name data
    get_computed_property => computed_properties -> ComputedPropertyData, [COMPUTED_PROPERTY_NAME];
}
