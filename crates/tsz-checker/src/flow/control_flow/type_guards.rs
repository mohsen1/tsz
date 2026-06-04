use tsz_common::interner::Atom;

use tsz_parser::parser::node::{CallExprData, NodeArena};

use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

use tsz_solver::narrowing::{TypeGuard, TypeofKind};

use tsz_solver::{ParamInfo, SymbolRef, TypeId, TypePredicate, TypePredicateTarget};

use crate::state::MAX_TREE_WALK_ITERATIONS;

use super::FlowAnalyzer;

use crate::query_boundaries::flow_analysis::{self as flow_query, TypeResolver};

use crate::types_domain::property_access_type::known_globals;

pub(crate) fn reference_is_in_class_property_initializer(
    arena: &NodeArena,
    reference: NodeIndex,
) -> bool {
    enclosing_class_property_initializer(arena, reference).is_some()
}

pub(crate) fn reference_uses_outer_class_property_initializer_binding(
    arena: &NodeArena,
    reference: NodeIndex,
    declaration: NodeIndex,
) -> bool {
    let Some(property) = enclosing_class_property_initializer(arena, reference) else {
        return false;
    };
    !node_is_within(arena, declaration, property)
}

fn enclosing_class_property_initializer(
    arena: &NodeArena,
    reference: NodeIndex,
) -> Option<NodeIndex> {
    let mut current = reference;
    for _ in 0..crate::state::MAX_TREE_WALK_ITERATIONS {
        let ext = arena.get_extended(current)?;
        let parent = ext.parent;
        if parent.is_none() {
            return None;
        }

        let parent_node = arena.get(parent)?;
        if parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
            return arena
                .get_property_decl(parent_node)
                .is_some_and(|property| property.initializer == current)
                .then_some(parent);
        }

        current = parent;
    }

    None
}

fn node_is_within(arena: &NodeArena, node: NodeIndex, ancestor: NodeIndex) -> bool {
    let mut current = node;
    for _ in 0..crate::state::MAX_TREE_WALK_ITERATIONS {
        if current == ancestor {
            return true;
        }
        let Some(ext) = arena.get_extended(current) else {
            return false;
        };
        let parent = ext.parent;
        if parent.is_none() {
            return false;
        }
        current = parent;
    }

    false
}

include!("type_guards_parts/part1.rs");
include!("type_guards_parts/part2.rs");
