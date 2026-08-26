use std::collections::HashSet;

use crate::semantics::types::{Signature, TypeId, TypeStore};
use crate::source::DeclId;

/// Query-local proof that a generic signature's identity mapper is sufficient.
///
/// This is deliberately not cached: argument types and completion belong to
/// the active call demand, and a failed proof must stay `Deferred`.
pub(super) struct IdentityCallInstantiation {
    owner: DeclId,
    required: HashSet<u32>,
    bound: HashSet<u32>,
    exact: bool,
}

impl IdentityCallInstantiation {
    pub(super) fn new(store: &TypeStore, owner: DeclId, signature: &Signature) -> Self {
        let mut required = store.type_parameters_from(signature.return_type, owner);
        for parameter in &signature.parameters {
            required.extend(store.type_parameters_from(parameter.ty, owner));
        }
        Self {
            owner,
            required,
            bound: HashSet::new(),
            exact: true,
        }
    }

    pub(super) const fn reject(&mut self) {
        self.exact = false;
    }

    pub(super) fn observe(&mut self, store: &TypeStore, expected: TypeId, actual: TypeId) -> bool {
        let consumed = store.type_parameters_from(expected, self.owner);
        if consumed.is_empty() {
            return false;
        }
        if actual == expected {
            self.bound.extend(consumed);
            true
        } else {
            self.reject();
            false
        }
    }

    pub(super) fn is_complete(&self) -> bool {
        self.exact && self.required.is_subset(&self.bound)
    }
}
