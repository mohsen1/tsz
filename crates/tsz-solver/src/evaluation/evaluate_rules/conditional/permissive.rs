use crate::types::TypeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PermissiveFalseBranchProbe {
    pub(super) definitive_false: bool,
    pub(super) cacheable: bool,
    pub(super) permissive_pair: Option<(TypeId, TypeId)>,
}

impl PermissiveFalseBranchProbe {
    pub(super) const fn unshared(definitive_false: bool) -> Self {
        Self {
            definitive_false,
            cacheable: true,
            permissive_pair: None,
        }
    }

    pub(super) const fn uncacheable(definitive_false: bool) -> Self {
        Self {
            definitive_false,
            cacheable: false,
            permissive_pair: None,
        }
    }

    pub(super) const fn with_permissive_pair(
        definitive_false: bool,
        permissive_check: TypeId,
        permissive_extends: TypeId,
    ) -> Self {
        Self {
            definitive_false,
            cacheable: true,
            permissive_pair: Some((permissive_check, permissive_extends)),
        }
    }
}
