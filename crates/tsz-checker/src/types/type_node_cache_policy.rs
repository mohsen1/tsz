use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;

pub(super) fn scoped_type_node_cache_allowed(arena: &NodeArena, idx: NodeIndex, kind: u16) -> bool {
    // Tuples, unions, and intersections can participate in recursive
    // conditional/constraint evaluation. The scoped cache key captures lexical
    // type-parameter bindings, but not recursion/fuel/relation state, so those
    // nodes must fall through and recompute under generic scope. Keep this
    // cache limited to simple array annotations, where the resolved container
    // shape is determined by the scoped element type alone.
    if kind != syntax_kind_ext::ARRAY_TYPE {
        return false;
    }

    // Even simple array nodes can be stateful when they sit under a
    // constraint/evaluation-sensitive type node. In those contexts the visible
    // lexical type-parameter bindings are not a complete cache key: recursion
    // fuel, mapped-key state, indexed-access resolution, and conditional
    // evaluation can all affect the result. Keep those paths cold/correct and
    // reserve the scoped cache for boring repeated container annotations.
    let mut current = idx;
    for _ in 0..crate::state::MAX_TREE_WALK_ITERATIONS {
        let Some(parent) = arena.parent_of(current) else {
            break;
        };
        if parent.is_none() {
            break;
        }
        if let Some(parent_node) = arena.get(parent) {
            match parent_node.kind {
                syntax_kind_ext::CONDITIONAL_TYPE
                | syntax_kind_ext::INDEXED_ACCESS_TYPE
                | syntax_kind_ext::INFER_TYPE
                | syntax_kind_ext::MAPPED_TYPE
                | syntax_kind_ext::TYPE_LITERAL
                | syntax_kind_ext::TYPE_OPERATOR
                | syntax_kind_ext::TYPE_PARAMETER
                | syntax_kind_ext::TYPE_QUERY => return false,
                _ => {}
            }
        }
        current = parent;
    }

    true
}
