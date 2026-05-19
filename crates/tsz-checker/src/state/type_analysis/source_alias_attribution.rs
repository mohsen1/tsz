//! Structural attribution helpers for source-file alias direct-lowering misses.

use tsz_common::perf_counters::{
    DirectSourceFileTypeAliasBodyRejectionKind,
    record_direct_source_file_type_alias_body_rejection_kind,
};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;

pub(crate) fn record_source_file_type_alias_body_rejection_kind(
    arena: &NodeArena,
    node_idx: NodeIndex,
) {
    record_direct_source_file_type_alias_body_rejection_kind(body_rejection_kind(arena, node_idx));
}

fn body_rejection_kind(
    arena: &NodeArena,
    node_idx: NodeIndex,
) -> DirectSourceFileTypeAliasBodyRejectionKind {
    use DirectSourceFileTypeAliasBodyRejectionKind as Kind;

    let Some(node) = arena.get(node_idx) else {
        return Kind::Other;
    };
    match node.kind {
        k if k == syntax_kind_ext::TYPE_REFERENCE => Kind::TypeReference,
        k if k == syntax_kind_ext::CONDITIONAL_TYPE => Kind::ConditionalType,
        k if k == syntax_kind_ext::TYPE_OPERATOR => Kind::TypeOperator,
        k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => Kind::IndexedAccessType,
        k if k == syntax_kind_ext::MAPPED_TYPE => Kind::MappedType,
        k if k == syntax_kind_ext::TYPE_LITERAL => Kind::TypeLiteral,
        k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => Kind::TemplateLiteralType,
        k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
            Kind::UnionOrIntersectionType
        }
        k if k == syntax_kind_ext::ARRAY_TYPE || k == syntax_kind_ext::TUPLE_TYPE => {
            Kind::ArrayOrTupleType
        }
        k if k == syntax_kind_ext::PARENTHESIZED_TYPE
            || k == syntax_kind_ext::OPTIONAL_TYPE
            || k == syntax_kind_ext::REST_TYPE =>
        {
            Kind::WrappedType
        }
        k if k == syntax_kind_ext::INFER_TYPE => Kind::InferType,
        _ => Kind::Other,
    }
}
