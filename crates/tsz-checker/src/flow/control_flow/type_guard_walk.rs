use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

use crate::state::MAX_TREE_WALK_ITERATIONS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TypeGuardWalkState<T> {
    Continue(T),
    Finished,
    Exhausted,
}

pub(super) struct TypeGuardWalk {
    remaining: usize,
}

impl TypeGuardWalk {
    pub(super) const fn new() -> Self {
        Self {
            remaining: MAX_TREE_WALK_ITERATIONS as usize,
        }
    }

    pub(super) fn next<T>(&mut self, next: impl FnOnce() -> Option<T>) -> TypeGuardWalkState<T> {
        if self.remaining == 0 {
            return TypeGuardWalkState::Exhausted;
        }

        self.remaining -= 1;
        next().map_or(TypeGuardWalkState::Finished, TypeGuardWalkState::Continue)
    }

    pub(super) fn next_parent(
        &mut self,
        arena: &NodeArena,
        current: NodeIndex,
    ) -> TypeGuardWalkState<NodeIndex> {
        self.next(|| {
            let ext = arena.get_extended(current)?;
            let parent = ext.parent;
            parent.is_some().then_some(parent)
        })
    }
}
