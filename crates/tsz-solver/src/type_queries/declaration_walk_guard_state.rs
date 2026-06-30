//! Named guard states for declaration-surface type walks.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeclarationWalkDepthState {
    Continue,
    DepthExceeded,
}

pub(crate) const fn declaration_walk_depth_state(
    depth: usize,
    limit: usize,
) -> DeclarationWalkDepthState {
    if depth > limit {
        DeclarationWalkDepthState::DepthExceeded
    } else {
        DeclarationWalkDepthState::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::{DeclarationWalkDepthState, declaration_walk_depth_state};

    #[test]
    fn declaration_walk_depth_state_names_limit_boundary() {
        assert_eq!(
            declaration_walk_depth_state(16, 16),
            DeclarationWalkDepthState::Continue
        );
        assert_eq!(
            declaration_walk_depth_state(17, 16),
            DeclarationWalkDepthState::DepthExceeded
        );
    }
}
