use std::collections::HashSet;

use crate::program::SemanticCompletion;
use crate::semantics::types::{DeferredType, TypeId, TypeStore};

use super::{Checker, ReferenceExpansionStack};

impl Checker<'_> {
    pub(super) fn visit_deferred_operands(
        &mut self,
        deferred: &DeferredType,
        active: &mut HashSet<TypeId>,
        references: &mut ReferenceExpansionStack,
        state: &mut SemanticCompletion,
    ) {
        let mut operands = Vec::new();
        TypeStore::push_deferred_children(deferred, &mut operands);
        self.combine_required_children(operands, active, references, state);
    }
}
