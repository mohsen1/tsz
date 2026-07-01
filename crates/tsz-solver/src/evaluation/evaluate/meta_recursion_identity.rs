//! Recursion-identity containment for evaluator meta-operations.

use super::TypeEvaluator;
use crate::def::DefId;
use crate::evaluation::result::TerminationKind;
use crate::relations::subtype::TypeResolver;
use crate::types::{TypeData, TypeId};

impl<R: TypeResolver> TypeEvaluator<'_, R> {
    // tsc bounds productive meta-operation growth by recursion identity. For
    // evaluator-side `IndexAccess`/`KeyOf`, the stable identity is the terminal
    // definition under transparent wrappers; when evaluation grows the wrapper
    // chain but keeps that identity, preserve the deferred form.
    pub(in crate::evaluation) fn same_meta_recursion_identity(
        &self,
        before: TypeId,
        after: TypeId,
    ) -> bool {
        let Some(before_def) = self.meta_recursion_identity(before) else {
            return false;
        };
        let Some(after_def) = self.meta_recursion_identity(after) else {
            return false;
        };
        before_def == after_def || self.resolver.defs_are_equivalent(before_def, after_def)
    }

    fn meta_recursion_identity(&self, type_id: TypeId) -> Option<DefId> {
        let mut current = type_id;
        for _ in 0..32 {
            match self.interner.lookup(current)? {
                TypeData::Lazy(def_id) => return Some(def_id),
                TypeData::TypeQuery(symbol_ref) => {
                    return self.resolver.symbol_to_def_id(symbol_ref);
                }
                TypeData::Application(app_id) => {
                    current = self.interner.type_application(app_id).base;
                }
                TypeData::IndexAccess(object, _) | TypeData::KeyOf(object) => {
                    current = object;
                }
                TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                    current = inner;
                }
                _ => return None,
            }
        }
        None
    }

    pub(in crate::evaluation) fn defer_same_identity_meta_recursion(&mut self) {
        self.mark_silent_depth_bailed();
        self.note_request_termination(TerminationKind::CrossEvalCycle);
    }
}
